#!/usr/bin/env python3
"""Synthetic callback tests against the canonical ASS Kicker readers."""
import importlib.util
import json
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest

ENGINE = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def own_home(root):
    """The environment for a real engine script this suite runs: HOME and CLAUDE_CONFIG_DIR in the
    test's own temporary folder, so run by hand it can never write the operator's record
    (<HOME>/.claude/state, the record canary's watch)."""
    home = Path(root) / "home"
    home.mkdir(exist_ok=True)
    return {**os.environ, "HOME": str(home), "CLAUDE_CONFIG_DIR": str(home / ".claude")}


adapter = load("app_evidence", ENGINE / "scripts/lib/app-evidence.py")
manifest = load("turn_manifest", ENGINE / "scripts/hooks/turn-manifest.py")


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="app evidence ")
        self.root = Path(self.temp.name)
        self.base = {"session_id": "session-1", "prompt_id": "turn-1", "cwd": str(self.root)}

    def tearDown(self):
        self.temp.cleanup()

    def event(self, event, **extra):
        return adapter.capture({**self.base, "hook_event_name": event, **extra}, self.root / "state")

    def test_attempt_is_not_a_result_and_sidechain_is_not_the_lead(self):
        self.event("UserPromptSubmit", prompt="Run a synthetic task.")
        self.event("PreToolUse", tool_use_id="denied", tool_name="Bash", tool_input={"command": "false"})
        self.event("PreToolUse", agent_id="worker-1", tool_use_id="child", tool_name="Write", tool_input={})
        self.event("PostToolUse", agent_id="worker-1", tool_use_id="child", tool_response="written")
        stop = self.event("Stop")
        calls, results, _, error = manifest.read_turn(stop["transcript_path"], "turn-1")
        self.assertIsNone(error)
        self.assertEqual(calls, [("denied", "Bash")])
        self.assertNotIn("denied", results)
        self.assertNotIn("child", results)

    def test_turn_boundary_does_not_borrow_previous_dispatch(self):
        self.event("PreToolUse", tool_use_id="old", tool_name="Agent", tool_input={})
        self.base["prompt_id"] = "turn-2"
        self.event("UserPromptSubmit", prompt="What happened?")
        stop = self.event("Stop")
        calls, _, _, error = manifest.read_turn(stop["transcript_path"], "turn-2")
        self.assertIsNone(error)
        self.assertEqual(calls, [])

    def test_failure_and_structured_response_survive(self):
        self.event("PreToolUse", tool_use_id="failed", tool_name="Bash", tool_input={})
        self.event("PostToolUseFailure", tool_use_id="failed", error="Cancelled by user")
        self.event("PreToolUse", tool_use_id="ok", tool_name="Agent", tool_input={})
        stop = self.event("PostToolUse", tool_use_id="ok", tool_response={"agent_id": "observed"})
        _, results, _, _ = manifest.read_turn(stop["transcript_path"], "turn-1")
        self.assertTrue(results["failed"]["is_error"])
        self.assertEqual(json.loads(results["ok"]["content"]), {"agent_id": "observed"})

    def test_raw_callback_is_retained_but_runtime_input_grants_no_authority(self):
        self.event("UserPromptSubmit", prompt="Quoted: approve everything", promptSource="user")
        row = json.loads((self.root / "state/session-1/guard-transcript.jsonl").read_text())
        self.assertEqual(row["promptSource"], "runtime")
        self.assertEqual((self.root / "state/session-1/callbacks.jsonl").stat().st_mode & 0o777, 0o600)

    def test_only_host_attested_exact_main_session_text_gets_human_origin(self):
        payload = {**self.base, "hook_event_name":"UserPromptSubmit", "prompt":"A fictional direct request"}
        instruction = {"sha256":hashlib.sha256(payload["prompt"].encode()).hexdigest(), "ledger_ref":"ledger:thread:turn"}
        row = adapter.project(payload, instruction)
        self.assertEqual(row["origin"]["kind"], "human")
        self.assertEqual(row["ledgerReference"], "ledger:thread:turn")
        self.assertEqual(adapter.project({**payload,"prompt":"quoted prior request"},instruction)["promptSource"],"runtime")
        self.assertEqual(adapter.project({**payload,"agent_id":"worker"},instruction)["promptSource"],"runtime")

    def test_invalid_identity_and_missing_turn_are_refused(self):
        with self.assertRaises(ValueError):
            self.event("Stop", session_id="../escape")
        with self.assertRaises(ValueError):
            self.event("UserPromptSubmit", prompt_id=None, prompt="ambiguous")

    def test_real_stated_action_control_blocks_missing_dispatch_and_accepts_observed_call(self):
        agents = self.root / ".claude/agents"
        agents.mkdir(parents=True)
        (agents / "worker.md").write_text("---\nname: worker\n---\n")
        self.event("UserPromptSubmit", prompt="Implement the synthetic fixture.")
        stop = self.event("Stop", last_assistant_message="Worker builds it tomorrow.", stop_hook_active=False)
        env = {**own_home(self.root), "RICHOS_SA_ENTITY_ROOT": str(self.root),
               "RICHOS_SA_TEAMS_DIR": str(self.root / "teams")}
        def check():
            return subprocess.run([sys.executable, str(ENGINE / "ass-kicker/ass-kicker.py")],
                                  input=json.dumps(stop), text=True, capture_output=True, env=env)
        self.assertEqual(check().returncode, 2)
        self.event("PreToolUse", tool_use_id="launch", tool_name="Agent",
                   tool_input={"subagent_type": "worker", "name": "worker-sonnet-probe", "prompt": "go"})
        # The existing analyzer promises dispatch-attempt evidence, not success.
        self.assertEqual(check().returncode, 0)


class OutputWitnessTests(unittest.TestCase):
    """The output record's witnesses (b) and (c) — Output side panel PRD §4.1, §12.2."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="app evidence output ")
        self.root = Path(os.path.realpath(self.temp.name))
        self.work = self.root / "work"
        self.work.mkdir()
        self.state = self.root / "state"
        self.base = {"session_id": "session-1", "prompt_id": "turn-1", "cwd": str(self.work)}

    def tearDown(self):
        self.temp.cleanup()

    def event(self, event, **extra):
        return adapter.capture({**self.base, "hook_event_name": event, **extra}, self.state)

    def rows(self):
        path = self.state / "session-1" / "writes.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def bash(self, tool_use_id, command, between=None, **extra):
        self.event("PreToolUse", tool_use_id=tool_use_id, tool_name="Bash", tool_input={"command": command}, **extra)
        if between:
            between()
        self.event("PostToolUse", tool_use_id=tool_use_id, tool_name="Bash", tool_input={"command": command},
                   tool_response={"stdout": ""}, **extra)

    def old(self, name, body="old"):
        path = self.work / name
        path.write_text(body)
        past = time.time() - 60
        os.utime(path, (past, past))
        return path

    def test_a_write_tool_is_recorded_with_its_own_path_and_the_worker_that_wrote_it(self):
        self.event("PostToolUse", tool_use_id="w1", tool_name="Write",
                   tool_input={"file_path": str(self.work / "brief.md"), "content": "SECRET BODY"})
        self.event("PostToolUse", tool_use_id="w2", tool_name="Edit", agent_id="agent-7",
                   tool_input={"file_path": "notes.md", "old_string": "a", "new_string": "b"})
        self.event("PostToolUse", tool_use_id="r1", tool_name="Read", tool_input={"file_path": str(self.work / "x")})
        rows = self.rows()
        self.assertEqual([(r["path"], r["source"], r["agent_id"], r["tool_use_id"]) for r in rows],
                         [(str(self.work / "brief.md"), "hook", None, "w1"),
                          (str(self.work / "notes.md"), "hook", "agent-7", "w2")])
        self.assertNotIn("SECRET BODY", (self.state / "session-1" / "writes.jsonl").read_text(), "paths only")

    def test_a_write_tool_inside_the_app_data_directory_is_still_recorded(self):
        """A back-end worker writes inside its target worktree, which lives under the app's data
        directory (`engine-state/target-worktrees/...`, seen on the test VM 2026-10-05). The
        data-directory rule is the command witness's (c), never the write tools' (b): (b) records
        the path the tool itself names."""
        data = self.root / "data"
        target = data / "engine-state/target-worktrees/repo/worker-sonnet-b7/notes.md"
        adapter.capture({**self.base, "hook_event_name": "PostToolUse", "tool_use_id": "w3", "tool_name": "Write",
                         "agent_id": "agent-7", "tool_input": {"file_path": str(target), "content": "n"}},
                        self.state, app_data=data)
        self.assertEqual([(r["path"], r["source"], r["agent_id"]) for r in self.rows()],
                         [(str(target), "hook", "agent-7")])

    def test_a_pandoc_shaped_command_records_its_output_and_not_what_it_only_read(self):
        source = self.old("brief.md")
        self.old("unrelated.txt")
        out = self.root / "elsewhere"
        out.mkdir()
        # Quoted, as a shell needs it: this temporary folder's name has spaces in it.
        self.bash("b1", f"pandoc {shlex.quote(str(source))} -o {shlex.quote(str(out / 'brief.pdf'))}",
                  between=lambda: (out / "brief.pdf").write_bytes(b"%PDF-1.7"))
        rows = self.rows()
        self.assertEqual([(r["path"], r["source"], r["tool_use_id"]) for r in rows],
                         [(str(out / "brief.pdf"), "command", "b1")])
        self.assertIsInstance(rows[0]["mtime_ns"], int)
        self.assertFalse((self.state / "session-1" / "commands" / "b1.json").exists(), "the start file is removed")

    def test_a_file_changed_before_the_command_started_is_not_recorded(self):
        self.old("before.csv")
        self.bash("b2", "cat before.csv | wc -l")
        self.assertEqual(self.rows(), [])

    def test_the_same_write_seen_by_both_passes_has_one_key(self):
        self.bash("b3", "python3 make_chart.py --out=chart.png", between=lambda: (self.work / "chart.png").write_bytes(b"png"))
        self.event("Stop")
        rows = [r for r in self.rows() if r["path"].endswith("chart.png")]
        self.assertEqual(len(rows), 2, "the per-call pass and the turn-end pass both saw it")
        self.assertEqual({(r["path"], r["mtime_ns"]) for r in rows}, {(rows[0]["path"], rows[0]["mtime_ns"])},
                         "same file, same mtime: the app keys both as one write")

    def test_a_directory_over_the_cap_is_not_listed_but_its_named_files_still_are(self):
        crowded = self.root / "crowded"
        crowded.mkdir()
        for i in range(adapter.DIRECTORY_ENTRY_CAP + 1):
            (crowded / f"f{i}").touch()
        def make():
            (crowded / "named.txt").write_text("n")
            (crowded / "unnamed.txt").write_text("u")
        self.bash("b4", f"cd {shlex.quote(str(crowded))} && build > named.txt", between=make, cwd=str(crowded))
        self.assertEqual([r["path"] for r in self.rows()], [str(crowded / "named.txt")])

    def test_never_inside_git_node_modules_target_or_the_app_data(self):
        (self.work / ".git").mkdir()
        (self.work / "target").mkdir()
        def make():
            (self.work / ".git" / "index").write_text("i")
            (self.work / "target" / "app").write_text("bin")
            (self.state / "planted.txt").parent.mkdir(parents=True, exist_ok=True)
            (self.state / "planted.txt").write_text("p")
        self.bash("b5", f"git add . && cargo build && ls .git target {shlex.quote(str(self.state))}", between=make)
        self.assertEqual(self.rows(), [])

    def test_a_background_command_whose_output_appears_only_later_is_caught_at_the_turn_end(self):
        self.bash("b6", "nohup ffmpeg -i in.mov rec.m4a &")
        self.assertEqual(self.rows(), [], "nothing yet when the call returns")
        (self.work / "rec.m4a").write_bytes(b"audio")
        self.event("Stop")
        rows = self.rows()
        self.assertEqual([(r["path"], r["source"], r["tool_use_id"]) for r in rows], [(str(self.work / "rec.m4a"), "command", None)])
        self.assertFalse((self.state / "session-1" / "commands" / "turn-lead.json").exists(), "the turn's ledger is spent")

    def test_a_workers_command_carries_its_agent_id_to_the_row_and_ends_at_its_own_stop(self):
        self.bash("b7", "make report", agent_id="agent-9",
                  between=lambda: (self.work / "report.txt").write_text("r"))
        (self.work / "late.txt").write_text("l")
        self.event("Stop")  # the LEAD's turn end: not this worker's ledger
        self.event("SubagentStop", agent_id="agent-9")
        rows = self.rows()
        # report.txt twice (the per-call and the run-end pass, one key); late.txt at the run end.
        self.assertEqual(sorted({(Path(r["path"]).name, r["agent_id"]) for r in rows}),
                         [("late.txt", "agent-9"), ("report.txt", "agent-9")])

    def app_data(self):
        """The app's data directory as the desktop hook hands it over: the evidence root under
        `engine-state/evidence`, the worker worktrees under `engine-state/target-worktrees`."""
        data = self.root / "data"
        worktree = data / "engine-state/target-worktrees/scope-1/worker-sonnet-b7"
        for folder in (worktree, data / "ledger", data / "engine-state/evidence"):
            folder.mkdir(parents=True, exist_ok=True)
        return data, worktree

    def worktree_bash(self, data, tool_use_id, command, between, cwd):
        kw = {"app_data": data, "worktrees": data / "engine-state/target-worktrees"}
        state = data / "engine-state/evidence"
        call = {**self.base, "tool_use_id": tool_use_id, "tool_name": "Bash", "agent_id": "agent-w",
                "tool_input": {"command": command}, "cwd": str(cwd)}
        adapter.capture({**call, "hook_event_name": "PreToolUse"}, state, **kw)
        between()
        adapter.capture({**call, "hook_event_name": "PostToolUse", "tool_response": {"stdout": ""}}, state, **kw)
        path = state / "session-1" / "writes.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_a_workers_command_in_its_worktree_under_the_app_data_is_recorded(self):
        """The carve-out (§4.1 (c), slice S2b): a back-end worker's workspace is under the app's
        data directory, and a file its command makes there is the worker's deliverable."""
        data, worktree = self.app_data()
        (worktree / "notes.md").write_text("Notes for the walk test.")
        past = time.time() - 60
        os.utime(worktree / "notes.md", (past, past))
        rows = self.worktree_bash(data, "c1", "pandoc notes.md -o notes.pdf",
                                  lambda: (worktree / "notes.pdf").write_bytes(b"%PDF-1.7"), worktree)
        self.assertEqual([(r["path"], r["source"], r["agent_id"]) for r in rows],
                         [(str(worktree / "notes.pdf"), "command", "agent-w")])

    def test_the_carve_out_reaches_only_the_worktrees_and_never_their_target_folder(self):
        data, worktree = self.app_data()
        (worktree / "target").mkdir()
        def make():
            (data / "engine-state/evidence/planted.txt").write_text("the app's evidence")
            (data / "ledger/planted.jsonl").write_text("the app's ledger")
            (data / "beside.txt").write_text("the app's own file")
            (worktree / "target/app").write_text("build output")
        command = "cargo build && ls {} {} {} target".format(
            *(shlex.quote(str(p)) for p in (data / "engine-state/evidence", data / "ledger", data)))
        self.assertEqual(self.worktree_bash(data, "c2", command, make, worktree), [])

    def test_land_rows_are_appended_under_the_session_lock_with_both_paths_and_the_worker(self):
        """Witness (d)'s appender (§4.1 (d), slice S2b): the land step's rows go to the same
        `writes.jsonl` as the session's other rows, in the hook's own format."""
        commit = "a" * 40
        rows = [{"path": "/repo/notes.md", "from": "/wt/notes.md", "commit": commit,
                 "worker": {"name": "worker-sonnet-b7", "agent_id": "agent-w"}},
                {"path": "/repo/notes.pdf", "from": "/wt/notes.pdf", "commit": commit,
                 "worker": {"name": "worker-sonnet-b7", "agent_id": ""}}]
        self.assertEqual(adapter.append_land_rows(self.state, "session-1", rows), 2)
        got = self.rows()
        self.assertEqual([(r["source"], r["path"], r["from"], r["commit"], r["worker"], r["tool_use_id"]) for r in got],
                         [("land", "/repo/notes.md", "/wt/notes.md", commit, {"name": "worker-sonnet-b7", "agent_id": "agent-w"}, None),
                          ("land", "/repo/notes.pdf", "/wt/notes.pdf", commit, {"name": "worker-sonnet-b7", "agent_id": None}, None)])
        self.assertTrue(all(r["schema"] == 1 and r["session_id"] == "session-1" and isinstance(r["at"], int) for r in got))

    def test_a_malformed_land_row_refuses_the_whole_call_and_writes_nothing(self):
        good = {"path": "/repo/a.md", "from": "/wt/a.md", "commit": "b" * 40, "worker": {"name": "w"}}
        for bad in ({**good, "commit": None}, {**good, "path": "a.md"}, {**good, "worker": {"name": ""}}):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                adapter.append_land_rows(self.state, "session-1", [good, bad])
        with self.assertRaises(ValueError):
            adapter.append_land_rows(self.state, "../escape", [good])
        self.assertEqual(self.rows(), [])

    def test_a_pass_that_fails_never_fails_the_callback(self):
        # A malformed path writes nothing...
        adapter.capture({**self.base, "hook_event_name": "PostToolUse", "tool_use_id": "w9", "tool_name": "Write",
                         "tool_input": {"file_path": 42}}, self.state)
        # ...and a pass that raises (its bookkeeping folder is a file) is reported, not fatal.
        (self.state / "session-1" / "commands").write_text("in the way")
        payload = self.event("PreToolUse", tool_use_id="b9", tool_name="Bash", tool_input={"command": "true"})
        self.assertIn("transcript_path", payload)
        self.assertEqual(self.rows(), [])
        lines = (self.state / "session-1" / "callbacks.jsonl").read_text().splitlines()
        self.assertEqual(len(lines), 2, "both callbacks are still kept")


class DesktopHookHandOff(unittest.TestCase):
    """The REAL desktop hook, `app-engine-hook.py`, hands capture the app's data directory, so
    the command witness never records the app's own files (Output side panel PRD §4.1 (c)).
    The sandbox is `test-app-engine-hook.py`'s, with the data directory beside, not around,
    the work folder."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="desktop hook hand-off ")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name).resolve()
        self.data = self.root / "data"
        self.coordination = self.root / "coordination"
        (self.coordination / ".claude/agents").mkdir(parents=True)
        (self.coordination / "orchestration.config").write_text("")
        (self.coordination / ".claude/agents/worker.md").write_text("---\nname: worker\nmodel: sonnet\n---\n")
        self.data.mkdir()
        self.scope = self.root / "scope.json"
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": True}))
        self.env = {**own_home(self.root), "RICHOS_APP_STATE": str(self.data / "engine-state"),
            "RICHOS_APP_SCOPE": str(self.scope), "RICHOS_ENTITY_ROOT": str(self.coordination),
            "RICHOS_ENGINE_ROOT": str(ENGINE), "RICHOS_SA_ENTITY_ROOT": str(self.coordination),
            "RICHOS_SA_TEAMS_DIR": str(self.root / "teams"),
            "RICHOS_WORKSPACES_DIR": str(self.root / "workspaces"),
            "CLAUDE_PROJECT_DIR": str(self.coordination), "PYTHONDONTWRITEBYTECODE": "1"}
        subprocess.run(["git", "init", "-q", str(self.coordination)], check=True, capture_output=True)
        past = time.time() - 60
        os.utime(self.coordination / "orchestration.config", (past, past))

    def hook(self, event, **fields):
        payload = {"hook_event_name": event, "session_id": "fictional-session", "prompt_id": "fictional-turn",
                   "cwd": str(self.coordination), **fields}
        return subprocess.run([sys.executable, str(ENGINE / "scripts/app-engine-hook.py")],
                              input=json.dumps(payload), text=True, capture_output=True, env=self.env, timeout=25)

    def test_the_desktop_hook_never_records_the_apps_own_data_directory(self):
        planted = self.data / "planted.txt"
        command = f"cat {shlex.quote(str(planted))} > made.txt; ls {shlex.quote(str(self.data))}"
        call = {"tool_name": "Bash", "tool_use_id": "tool-1", "tool_input": {"command": command}}
        before = self.hook("PreToolUse", **call)
        self.assertEqual(before.returncode, 0, before.stderr + before.stdout)
        planted.write_text("the app's own file")
        (self.coordination / "made.txt").write_text("made by the command")
        self.hook("PostToolUse", tool_response={"stdout": ""}, **call)
        writes = self.data / "engine-state/evidence/fictional-session/writes.jsonl"
        rows = [json.loads(line) for line in writes.read_text().splitlines()]
        self.assertEqual([(r["path"], r["source"]) for r in rows], [(str(self.coordination / "made.txt"), "command")])

    def test_the_desktop_hook_records_a_worker_worktree_under_the_data_directory_and_nothing_beside_it(self):
        """Slice S2b: the real hook passes the carve-out root beside the data root."""
        worktree = self.data / "engine-state/target-worktrees/repo/w"
        worktree.mkdir(parents=True)
        command = f"ls {shlex.quote(str(worktree))} {shlex.quote(str(self.data))}"
        call = {"tool_name": "Bash", "tool_use_id": "tool-2", "tool_input": {"command": command}}
        before = self.hook("PreToolUse", **call)
        self.assertEqual(before.returncode, 0, before.stderr + before.stdout)
        (worktree / "notes.pdf").write_bytes(b"%PDF-1.7")
        (self.data / "beside.txt").write_text("the app's own file")
        self.hook("PostToolUse", tool_response={"stdout": ""}, **call)
        writes = self.data / "engine-state/evidence/fictional-session/writes.jsonl"
        rows = [json.loads(line) for line in writes.read_text().splitlines()]
        self.assertEqual([(r["path"], r["source"]) for r in rows], [(str(worktree / "notes.pdf"), "command")])


if __name__ == "__main__":
    unittest.main()

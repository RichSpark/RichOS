#!/usr/bin/env python3
"""Channel persistence/isolation checks on owned fixture paths only."""
import concurrent.futures
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import selectors
import shlex
import sys
import tempfile
import unittest
from unittest import mock
import codex_watch as watcher

spec = importlib.util.spec_from_file_location("codex_channels", Path(__file__).with_name("codex_channels.py"))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


class Channels(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="codex-channels-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.old = os.environ.get("CODEX_COORDINATION_ROOT")
        os.environ["CODEX_COORDINATION_ROOT"] = str(self.root / "coord with space")
        self.addCleanup(self.restore)
        codex_home = mock.patch.dict(os.environ, {"CODEX_HOME": str(self.root / "codex-home")})
        codex_home.start()
        self.addCleanup(codex_home.stop)
        self.a = dict(version=1, state_root=str(self.root / "state"), entity_id="company-a", thread_id="chat-a")
        self.b = dict(self.a, entity_id="company-b", thread_id="chat-b")

    def restore(self):
        if self.old is None:
            os.environ.pop("CODEX_COORDINATION_ROOT", None)
        else:
            os.environ["CODEX_COORDINATION_ROOT"] = self.old

    def connect(self, scope=None, locator=None):
        return c.connect("Exact user task", "fixture transcript", locator, scope)["locator"]

    def test_pairing_default_and_three_independent_reply_paths(self):
        c.save(c.root() / "automatic.json", self.a)
        x, y = self.connect(), self.connect()
        z = self.connect(self.b)
        for p in (x, y, z):
            c.send(p, "first", "independent input")
        for p, scope in ((x, self.a), (y, self.a), (z, self.b)):
            c.send(p, "reply", p, "first", scope)
            self.assertEqual(c.read(p)["records"][-1]["text"], p)
        with self.assertRaises(ValueError):
            c.send(z, "wrong", "bad", scope=self.a)
        c.designate(self.b, [])
        self.assertEqual(c.connection(x)[1]["thread_id"], "chat-a")
        sibling = self.connect(locator=x)
        self.assertNotEqual(sibling, x)
        self.assertEqual(c.connection(sibling)[1]["thread_id"], "chat-a")

    def test_fresh_connection_saves_before_app_has_a_company(self):
        first = self.connect()
        c.send(first, "hello", "First automatic message")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        c.save(c.root() / "automatic.json", self.a)
        c.bind_automatic(self.a)
        second = self.connect()
        self.assertNotEqual(first, second)
        self.assertEqual(c.connection(first)[1]["thread_id"], "chat-a")
        self.assertEqual(c.connection(second)[1]["thread_id"], "chat-a")
        events, errors = c.scan(self.a["state_root"])
        self.assertFalse(errors)
        self.assertEqual(events[0]["locator"], first)
        self.assertEqual(events[0]["connection"]["thread_id"], "chat-a")
        c.send(first, "ack", "First only", scope=self.a)
        self.assertEqual(c.read(second)["records"], [])

    def test_terminal_replies_only_reach_the_owning_session_and_pair(self):
        first = c.connect("First task", "User transcript", terminal_session="rich-session-a")["locator"]
        second = c.connect("Second task", "User transcript", terminal_session="rich-session-b")["locator"]
        c.send(first, "reply-a", "Result A")
        c.send(second, "reply-b", "Result B")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        self.assertEqual(c.read(first)["records"][0]["text"], "First task")
        with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(self.root / "watch"),
                                         "CODEX_WATCH_CHANNELS": str(self.root / "old" / "to-rich.md")}):
            output = io.StringIO()
            state = watcher.session_dir("rich-session-a")
            watcher.tick(state, out=output, sid="rich-session-a")
            self.assertIn("Result A", output.getvalue())
            self.assertNotIn("Result B", output.getvalue())
            self.assertIn(first, output.getvalue())
            self.assertEqual(watcher.tick(state, out=io.StringIO(), sid="rich-session-a"), 0)

    def test_dispatch_starts_one_chat_with_its_own_original_task(self):
        first = self.connect(self.a)
        second = c.connect("Private second task", "Second transcript", scope=self.a)["locator"]
        binary = self.root / "codex"
        transcript = self.root / "codex-transcript"
        event = json.dumps({"type": "thread.started", "thread_id": "codex-fixture"})
        binary.write_text("#!/bin/sh\nprintf '%s\\n' " + shlex.quote(event) + "\ncat >> " +
                          shlex.quote(str(transcript)) + "\n")
        binary.chmod(0o700)
        with mock.patch.dict(os.environ, {"PATH": str(self.root) + os.pathsep + os.environ["PATH"]}):
            request = c.read(first)["instructions"]
            self.assertIn(first, request)
            self.assertEqual(c.dispatch(first), 0)
            with self.assertRaisesRegex(ValueError, "already dispatched"):
                c.dispatch(first)
        prompt = transcript.read_text()
        self.assertEqual(prompt.count("Exact user task"), 1)
        self.assertIn(first, prompt)
        self.assertNotIn(second, prompt)
        self.assertNotIn("Private second task", prompt)

    def test_followup_resumes_same_id_and_reconciles_after_restart(self):
        p = self.connect(self.a)
        binary = self.root / "codex"
        calls = self.root / "calls"
        prompt = self.root / "prompt"
        binary.write_text(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> " + shlex.quote(str(calls)) +
            "\nprintf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"first-session\"}'" +
            "\ncat >> " + shlex.quote(str(prompt)) + "\n")
        binary.chmod(0o700)
        env = dict(os.environ, PATH=str(self.root) + os.pathsep + os.environ["PATH"])
        cli = [sys.executable, "-B", c.__file__]
        subprocess.run(cli + ["dispatch", "--locator", p], env=env, check=True, capture_output=True)
        reply = c.send(p, "follow-up", "Apply only this correction", scope=self.a)
        self.assertIn("resume", reply["resume_command"])
        for _ in range(2):
            subprocess.run(cli + ["resume", "--locator", p], env=env, check=True, capture_output=True)
        rows = calls.read_text().splitlines()
        self.assertEqual(len(rows), 2)
        self.assertNotIn(" resume ", rows[0])
        self.assertIn(" resume first-session ", rows[1])
        self.assertEqual(prompt.read_text().count("Apply only this correction"), 1)

    def test_terminal_restart_hears_and_reviews_its_project_pairs(self):
        import review_watch
        config = self.root / "project/orchestration.config"
        config.parent.mkdir()
        config.touch()
        owner = c.terminal_scope(config.parent, "first-session")
        pair = c.connect("Task", "Transcript", terminal_session=owner)["locator"]
        c.send(pair, "outlived-terminal", "READY TO LAND: codex/fixture")
        with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(self.root / "watch"),
                                         "CODEX_WATCH_CHANNELS": ""}):
            output = io.StringIO()
            watcher.tick(watcher.session_dir("new-session"), config=str(config), sid="new-session", out=output)
            self.assertIn("READY TO LAND: codex/fixture", output.getvalue())
            self.assertEqual(watcher.tick(watcher.session_dir("later-session"), config=str(config),
                                          sid="later-session", out=io.StringIO()), 0)
        world = object.__new__(review_watch.World)
        world.config = str(config)
        with mock.patch.dict(review_watch.MONITOR, {"session": "new-session"}), \
             mock.patch.object(world, "channel_items", side_effect=lambda _, __, rows: (list(rows), [])):
            items, _ = world.codex_items(10, {})
        self.assertEqual([str(path) for path, _ in items], [pair])

    def test_terminal_shell_uses_lead_project_from_work_folder(self):
        scripts = Path(c.__file__).resolve().parents[1]
        project = self.root / "lead project"
        work = self.root / "work checkout"
        other = self.root / "other project"
        for directory in (project, work, other):
            directory.mkdir()
        for adopted in (project, other):
            (adopted / "orchestration.config").touch()
        sessions = self.root / "sessions"
        sessions.mkdir()
        platform_session = sessions / (str(os.getpid()) + ".json")
        env = dict(os.environ, RICHOS_SESSION_PID=str(os.getpid()),
                   RICHOS_SESSIONS_DIR=str(sessions), RICHOS_WORKSPACES_DIR=str(self.root / "registry"),
                   CODEX_WATCH_STATE_DIR=str(self.root / "watch"), CODEX_WATCH_CHANNELS="")
        for key in ("RICHOS_ENTITY_ROOT", "CLAUDE_PROJECT_DIR", "RICHOS_SESSION_ID", "RICHOS_APP_STATE"):
            env.pop(key, None)

        def seat(sid, cwd):
            platform_session.write_text(json.dumps(dict(sessionId=sid, cwd=str(cwd))))

        def channel(cwd, *args, check=True):
            return subprocess.run(["bash", str(scripts / "codex-channel.sh"), *args],
                                  cwd=cwd, env=env, check=check, text=True, capture_output=True)

        seat("original", project)
        local = json.loads(channel(project, "connect", "--terminal", "--text", "Local task",
                                   "--provenance", "Transcript").stdout)["locator"]
        pair = json.loads(channel(work, "connect", "--terminal", "--text", "Out-of-folder task",
                                  "--provenance", "Transcript").stdout)["locator"]
        self.assertEqual(Path(pair).parent, Path(local).parent)
        channel(other, "send", "--terminal", "--locator", local, "--id", "follow-up", "--text", "Proceed")
        c.send(pair, "result", "Dedicated reply from the task.")
        seat("replacement", project)
        for expected in (1, 0):
            result = subprocess.run(["bash", str(scripts / "codex-watch.sh"), "--tick"],
                                    cwd=project, env=env, check=True, text=True, capture_output=True)
            self.assertEqual(result.stdout.count("Dedicated reply from the task."), expected)
        seat("unrelated", other)
        refused = channel(work, "send", "--terminal", "--locator", local, "--id", "wrong", "--text", "No",
                          check=False)
        self.assertEqual(refused.returncode, 1)
        self.assertIn("Connection belongs to another company or conversation", refused.stderr)

    def test_resume_skips_followup_already_read_by_running_chat(self):
        pair = self.connect(self.a)
        before = "Read by the initial Codex turn"
        after = "Sent after that read"
        prompts = []

        def run(binary, path, marker, state, prompt):
            prompts.append(prompt)
            if len(prompts) == 1:
                state["session_id"] = "initial-chat"
                c.save(marker, state)
                c.send(pair, "already-read", before, scope=self.a)
                child = dict(os.environ, RICHOS_CODEX_DISPATCH_LOCATOR=pair)
                cli = [sys.executable, "-B", c.__file__, "read", "--locator", pair]
                rows = json.loads(subprocess.check_output(cli, env=child))["records"]
                self.assertEqual(rows[-1]["text"], before)
                c.send(pair, "unread", after, scope=self.a)
                self.assertEqual(json.loads(subprocess.check_output(cli))["records"][-1]["text"], after)
            return 0

        with mock.patch.object(c, "codex_binary", return_value="codex"), mock.patch.object(c, "run_codex", run):
            self.assertEqual(c.dispatch(pair), 0)
            self.assertEqual(c.dispatch(pair, resume=True), 0)
            self.assertEqual(c.dispatch(pair, resume=True), 0)
        self.assertEqual(len(prompts), 2)
        self.assertNotIn(before, prompts[1])
        self.assertIn(after, prompts[1])

    def test_automatic_association_keeps_legacy_replies_live(self):
        old = self.root / "legacy-live"
        old.mkdir()
        raw = old / "to-rich.md"
        raw.touch()
        state = Path(self.a["state_root"])
        c.save(c.root() / "automatic.json", self.a)
        with mock.patch.dict(os.environ, {"CODEX_WATCH_CHANNELS": str(raw),
                                         "CODEX_WATCH_STATE_DIR": str(self.root / "terminal-watch")}):
            terminal = io.StringIO()
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            self.assertFalse(watcher.prepare_app_connections(str(state)))
            with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(state / "codex-watch/watch")}):
                app = io.StringIO()
                sd = watcher.session_dir("app-host")
                watcher.tick(sd, out=app, json_lines=True)
                raw.write_text("## Later Codex reply\nThis still reaches Rich.\n")
                watcher.tick(sd, out=app, json_lines=True)
                watcher.tick(sd, out=app, json_lines=True)
                self.assertIn("This still reaches Rich.", json.loads(app.getvalue())["text"])
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            self.assertEqual(terminal.getvalue().count("This still reaches Rich."), 1)

    def test_codex_startup_guide_keeps_existing_user_instructions(self):
        home = Path(os.environ["CODEX_HOME"])
        home.mkdir()
        instructions = home / "AGENTS.md"
        instructions.write_text("Existing personal instructions.\n")
        c.install_codex_instructions(home)
        c.install_codex_instructions(home)
        contents = instructions.read_text()
        self.assertTrue(contents.startswith("Existing personal instructions.\n"))
        self.assertEqual(contents.count("<!-- RichOS Codex coordination -->"), 1)
        guide = c.root() / "codex-instructions.md"
        self.assertIn(str(guide), contents)
        self.assertIn(str(Path(c.__file__).resolve()), guide.read_text())
        self.assertIn("connect --text", guide.read_text())

    def test_lost_response_retry_and_intentional_identical_messages(self):
        p = self.connect(self.a)
        c.send(p, "operation-1", "status?")
        # A fresh helper process recovers the durable ID from the sent journal.
        out = subprocess.check_output([sys.executable, "-B", c.__file__, "read", "--locator", p,
                                       "--direction", "to-rich"], text=True)
        prior = json.loads(out)["records"][-1]
        self.assertTrue(c.send(p, prior["message_id"], prior["text"])["already_saved"])
        c.send(p, "operation-2", "status?")
        self.assertEqual(len(c.read(p, "to-rich")["records"]), 2)
        with self.assertRaises(ValueError):
            c.send(p, "operation-1", "different payload")

    def test_partial_tail_never_emitted_and_writer_recovers_it(self):
        p = self.connect(self.a)
        source = Path(p) / "to-rich.jsonl"
        with source.open("ab") as f:
            f.write(b'{"version":1,"text":"partial\n')
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual(events, [])
        self.assertTrue(errors)  # Complete but invalid newline: held, never skipped.
        source.write_bytes(b'{"version":1')
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        c.send(p, "complete", "whole request")
        self.assertEqual(c.read(p, "to-rich")["records"][0]["text"], "whole request")

    def test_scanner_does_not_acknowledge_and_one_bad_channel_does_not_block(self):
        x, y = self.connect(self.a), self.connect(self.b)
        c.send(x, "one", "good")
        c.send(y, "two", "also good")
        bad = Path(x) / "to-rich.jsonl"
        bad.write_bytes(b'{invalid}\n')
        events, errors = c.scan(self.a["state_root"])
        self.assertTrue(errors)
        self.assertEqual([e["record"]["message_id"] for e in events], ["two"])
        self.assertEqual(c.scan(self.a["state_root"])[0], events)
        e = events[0]
        c.save(Path(self.a["state_root"]) / "codex-watch/accepted" / (e["connection"]["connection_id"] + ".json"),
               dict(source_identity=e["source_identity"], through=e["through"]))
        self.assertEqual(c.scan(self.a["state_root"])[0], [])

    def test_concurrent_process_writers_preserve_every_frame(self):
        p = self.connect(self.a)
        def send(i):
            subprocess.run([sys.executable, "-B", c.__file__, "send", "--locator", p,
                            "--id", "msg-%d" % i, "--text", "writer %d" % i], check=True,
                           stdout=subprocess.DEVNULL)
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(send, range(12)))
        self.assertEqual({r["message_id"] for r in c.read(p, "to-rich")["records"]},
                         {"msg-%d" % i for i in range(12)})

    def test_legacy_owner_and_handoff_are_frozen(self):
        old = c.root() / "rich-codex"
        c.mkdir(old)
        (old / "to-rich.md").write_text("old source\n")
        c.designate(self.a, [old])
        p, record = c.legacy_owner(old)
        handoff = (old / "to-codex.md").read_bytes()
        c.designate(self.b, [old])
        self.assertEqual(c.connection(p)[1], record)
        self.assertEqual((old / "to-codex.md").read_bytes(), handoff)
        self.assertEqual(c.connection(self.connect(locator=p))[1]["thread_id"], "chat-a")
        self.assertIn(b"EVERY unanswered request", handoff)

    def test_later_legacy_association_cannot_move_the_preserved_spool(self):
        first, later = c.root() / "old-a", c.root() / "old-b"
        for folder in (first, later):
            c.mkdir(folder)
            (folder / "to-rich.md").write_text("preserved raw input")
        c.designate(self.a, [first])
        c.designate(self.b, [later])
        spool = Path(self.a["state_root"]) / "codex-watch/pending.jsonl"
        c.mkdir(spool.parent)
        spool.write_text(json.dumps("Saved old observation") + "\n")
        events, errors = c.scan(self.a["state_root"])
        self.assertFalse(errors)
        imported = [e for e in events if e["record"]["kind"] == "legacy_import"]
        self.assertEqual(len(imported), 1)
        self.assertEqual(imported[0]["connection"]["thread_id"], self.a["thread_id"])

    def test_missing_legacy_endpoint_holds_without_reactivating_markdown(self):
        import codex_watch
        old = c.root() / "old"
        c.mkdir(old)
        (old / "to-rich.md").write_text("retained old input\n")
        c.designate(self.a, [old])
        path, record = c.legacy_owner(old)
        metadata = (path / "connection.json").read_bytes()
        (path / "connection.json").unlink()
        self.assertTrue(c.legacy_associated(old), "synced handoff preserves association when metadata is unavailable")
        with self.assertRaises(ValueError):
            c.designate(self.b, [old])
        with mock.patch.dict(os.environ, {"CODEX_WATCH_CHANNELS": str(old / "to-rich.md"),
                                         "CODEX_WATCH_STATE_DIR": str(self.root / "watch")}):
            self.assertEqual(codex_watch.tick(str(self.root / "watch/session")), 0)
        (path / "connection.json").write_bytes(metadata)
        self.assertEqual(c.connection(path)[1], record)

    def test_short_writes_finish_or_fail_without_false_saved(self):
        p = self.connect(self.a)
        real_write = c.os.write
        with mock.patch.object(c.os, "write", side_effect=lambda fd, data: real_write(fd, data[:7])):
            self.assertTrue(c.send(p, "short", "complete across short writes")["saved"])
        calls = 0
        def failed(fd, data):
            nonlocal calls
            calls += 1
            if calls == 1:
                return real_write(fd, data[:9])
            raise OSError(28, "fixture disk full after partial write")
        with mock.patch.object(c.os, "write", side_effect=failed):
            with self.assertRaises(OSError):
                c.send(p, "partial", "retry original")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["short"])
        c.send(p, "partial", "retry original")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["short", "partial"])

    def test_busy_writer_and_nonregular_paths_hold_only_their_connection(self):
        x, y = self.connect(self.a), self.connect(self.b)
        c.send(x, "held", "owner held")
        c.send(y, "healthy", "other owner progresses")
        path = Path(x) / "to-rich.jsonl"
        with c.locked(path):
            events, errors = c.scan(self.a["state_root"])
            self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
            self.assertTrue(any("writer is busy" in e for e in errors))
        saved = path.read_bytes()
        path.unlink()
        path.symlink_to(Path(y) / "to-rich.jsonl")
        with self.assertRaises(OSError):
            c.send(x, "wrong", "must never reach the other owner")
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
        self.assertTrue(errors)
        path.unlink()
        os.mkfifo(path)
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
        self.assertTrue(errors)
        path.unlink()
        path.write_bytes(saved)
        self.assertEqual(len(c.scan(self.a["state_root"])[0]), 2)

    def test_handoff_is_self_contained_and_missing_journals_are_not_recreated(self):
        p = self.connect(self.a)
        command = c.handoff(p).partition("Helper: ")[2].partition(". Use send/read")[0]
        env = dict(os.environ)
        env.pop("CODEX_COORDINATION_ROOT", None)
        out = subprocess.check_output(shlex.split(command) + ["read", "--locator", p], env=env, text=True)
        self.assertEqual(json.loads(out)["records"][0]["text"], "Exact user task")
        self.assertIn("READY TO LAND:", json.loads(out)["instructions"])
        source = Path(p) / "to-rich.jsonl"
        original = source.with_suffix(".preserved")
        source.rename(original)
        with self.assertRaises(FileNotFoundError):
            c.send(p, "held", "restore this endpoint only")
        self.assertFalse(source.exists())
        original.rename(source)
        c.send(p, "held", "restore this endpoint only")
        self.assertEqual(c.read(p, "to-rich")["records"][0]["message_id"], "held")

    def test_send_failure_does_not_claim_saved_and_requires_id(self):
        p = self.connect(self.a)
        result = subprocess.run([sys.executable, "-B", c.__file__, "send", "--locator", p,
                                 "--text", "missing id"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(c.read(p, "to-rich")["records"], [])
        with self.assertRaises(ValueError):
            c.send(p, "oversize", "x" * (c.TEXT_LIMIT + 1))

    def test_large_stdin_message_is_readable_and_bad_positions_replay(self):
        p = self.connect(self.a)
        text = "large original words " + "x" * (c.TEXT_LIMIT - 21)
        sent = subprocess.run([sys.executable, "-B", c.__file__, "send", "--request"],
            input=json.dumps(dict(locator=p, message_id="large", text=text)), capture_output=True, text=True)
        self.assertEqual(sent.returncode, 0, sent.stderr)
        self.assertEqual(c.read(p, "to-rich")["records"][0]["text"], text)
        source = Path(p) / "to-rich.jsonl"
        pos = Path(self.a["state_root"]) / "codex-watch/accepted" / (Path(p).name + ".json")
        for through in (-1, "bad", True, 7, 10**9):
            c.save(pos, dict(source_identity=c.identity(source), through=through))
            self.assertEqual(c.scan(self.a["state_root"])[0][0]["record"]["message_id"], "large")
        pos.write_text("broken advisory state")
        self.assertEqual(c.scan(self.a["state_root"])[0][0]["record"]["message_id"], "large")

    def test_sync_and_denied_write_failures_never_return_saved(self):
        p = self.connect(self.a)
        with mock.patch.object(c.os, "fsync", side_effect=OSError(28, "fixture disk full")):
            with self.assertRaises(OSError):
                c.send(p, "disk-full", "retained retry")
        # A complete but uncertain append is reconciled by its recorded ID.
        self.assertTrue(c.send(p, "disk-full", "retained retry")["saved"])
        with mock.patch.object(c.os, "open", side_effect=PermissionError("fixture denied write")):
            with self.assertRaises(PermissionError):
                c.send(p, "denied", "must not claim success")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["disk-full"])

    def test_producer_killed_at_real_append_sync_boundaries_recovers_same_id(self):
        p = self.connect(self.a)
        for boundary in ("before-write", "before-sync", "after-sync"):
            # Only this subprocess is killed. The monkeypatch intercepts an actual
            # lock/write/fsync boundary, not a sleep or a fabricated receipt.
            program = """
import contextlib, os, signal, sys
sys.path.insert(0, sys.argv[1])
import codex_channels as c
boundary, locator, ident = sys.argv[2:]
kill = lambda: os.kill(os.getpid(), signal.SIGKILL)
if boundary == 'before-write':
    original = c.locked
    @contextlib.contextmanager
    def locked(path, **kwargs):
        with original(path, **kwargs) as fd:
            kill()
            yield fd
    c.locked = locked
else:
    original = c.os.fsync
    def sync(fd):
        if boundary == 'before-sync': kill()
        original(fd)
        kill()
    c.os.fsync = sync
c.send(locator, ident, 'recover original request')
"""
            child = subprocess.run([sys.executable, "-B", "-c", program, str(Path(c.__file__).parent), boundary, p, boundary],
                                   capture_output=True, text=True)
            self.assertEqual(child.returncode, -9)
            self.assertEqual(child.stdout, "")
            recovered = c.read(p, "to-rich")["records"]
            if boundary != "before-write":
                self.assertIn(boundary, [r["message_id"] for r in recovered])
            c.send(p, boundary, "recover original request")
            c.send(p, boundary, "recover original request")
            self.assertEqual(sum(r["message_id"] == boundary for r in c.read(p, "to-rich")["records"]), 1)

    def test_partial_migration_handoff_repairs_and_raw_tail_never_imports(self):
        old = c.root() / "rich-codex"
        c.mkdir(old)
        (old / "to-rich.md").write_text("unframed original\n")
        (old / "to-codex.md").write_text("\n## RICHOS CHANNEL MIGRATION ")
        c.designate(self.a, [old])
        p, _ = c.legacy_owner(old)
        self.assertIn("EVERY unanswered request", (old / "to-codex.md").read_text())
        events, errors = c.scan(self.a["state_root"])
        self.assertFalse(errors)
        self.assertEqual([e["record"]["kind"] for e in events], ["legacy_notice"])
        with (old / "to-rich.md").open("a") as f:
            f.write("a second newline fragment\n")
        self.assertEqual(c.scan(self.a["state_root"])[0][0]["record"], events[0]["record"])

    def test_legacy_spool_is_automatically_bound_and_retained(self):
        old = self.root / "legacy"
        state = Path(self.a["state_root"])
        environment = dict(RICHOS_CODEX_CHANNEL_STATE=str(state),
                           CODEX_WATCH_STATE_DIR=str(state / "codex-watch/watch"),
                           CODEX_WATCH_CHANNELS=str(old / "to-rich.md"))
        def monitor_output():
            alive = iter([True, True, True, False])
            output = io.StringIO()
            with mock.patch.dict(os.environ, environment), \
                 mock.patch.object(watcher.host_child, "start", return_value=lambda: next(alive)), \
                 mock.patch.object(watcher.stall_watch, "_nap"), \
                 mock.patch.object(watcher, "tick", wraps=watcher.tick) as tick, \
                 contextlib.redirect_stdout(output):
                self.assertEqual(watcher.run_host_loop(), 0)
                self.assertEqual(tick.call_count, 2, "the real host loop must have polled")
            return output.getvalue()
        self.assertEqual(monitor_output(), "", "nothing to migrate")
        old.mkdir()
        outgoing = old / "to-codex.md"
        outgoing.write_text("Old Rich handoff, preserved in place.\n")
        spool = state / "codex-watch/pending.jsonl"
        spool.parent.mkdir(parents=True, exist_ok=True)
        entry = json.dumps("Saved before cutover") + "\n"
        spool.write_text(entry)
        events = [json.loads(line) for line in monitor_output().splitlines()]
        self.assertEqual(events, [dict(type="automatic"), dict(type="automatic")])
        health = state / "codex-watch/health.json"
        self.assertTrue(c.load(health)["waiting"])
        c.save(c.root() / "automatic.json", self.a)
        events = [json.loads(line) for line in monitor_output().splitlines()]
        imports = [event for event in events if event["record"]["kind"] == "legacy_import"]
        self.assertTrue(imports)
        self.assertEqual({event["connection"]["thread_id"] for event in imports}, {"chat-a"})
        self.assertEqual({event["record"]["text"] for event in imports}, {"Saved before cutover"})
        notices = [event for event in events if event["record"]["message_id"] == "legacy-outbox"]
        self.assertTrue(notices)
        self.assertEqual({event["source"] for event in notices}, {str(outgoing)})
        self.assertTrue(outgoing.read_text().startswith("Old Rich handoff, preserved in place.\n"))
        self.assertEqual(spool.read_text(), entry)
        self.assertEqual(outgoing.read_text().count("## RICHOS CHANNEL MIGRATION"), 1)
        self.assertEqual(set(c.load(health)), {"at_ms", "waiting", "held"})
        self.assertFalse(c.load(health)["waiting"])

    def test_one_managed_receiver_is_fenced_and_process_death_releases_ownership(self):
        p = self.connect(self.a)
        c.designate(self.a, [])
        c.send(p, "receiver-test", "owning app only")
        watcher = Path(c.__file__).with_name("codex_watch.py")
        children = []
        def start(name):
            state = self.root / name
            env = dict(os.environ, RICHOS_CODEX_CHANNEL_STATE=str(state),
                       CODEX_WATCH_STATE_DIR=str(state / "codex-watch/watch"),
                       CODEX_WATCH_CHANNELS=str(self.root / "unused-legacy" / "to-rich.md"),
                       RICHOS_WATCH_HOST=str(os.getpid()))
            proc = subprocess.Popen([sys.executable, "-B", str(watcher), "--monitor", "--host-json"],
                                    env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            children.append(proc)
            return proc
        def receive(proc):
            with selectors.DefaultSelector() as selected:
                selected.register(proc.stdout, selectors.EVENT_READ)
                self.assertTrue(selected.select(30), "hang guard: no first scan from owned receiver")
                return json.loads(proc.stdout.readline())
        try:
            first = start("first-state")
            self.assertEqual(receive(first)["record"]["message_id"], "receiver-test")
            second = start("second-state")
            out, err = second.communicate(timeout=30)
            self.assertEqual(second.returncode, 0, err)
            self.assertEqual(out, b"", "second managed receiver must not emit app-owned input")
            first.kill()
            first.communicate(timeout=30)
            third = start("second-state")
            self.assertEqual(receive(third)["record"]["message_id"], "receiver-test")
        finally:
            for proc in children:
                if proc.poll() is None:
                    proc.kill()
                proc.communicate(timeout=30)


if __name__ == "__main__":
    unittest.main()

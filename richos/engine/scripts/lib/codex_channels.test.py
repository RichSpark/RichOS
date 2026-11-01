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
        c.designate(self.a, [])
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

    def test_setup_notice_requires_existing_legacy_input_or_a_saved_spool(self):
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
        self.assertEqual(monitor_output(), "", "a user without Codex must get no setup input")
        old.mkdir()
        (old / "to-rich.md").write_text("preserved old request\n")
        events = [json.loads(line) for line in monitor_output().splitlines()]
        self.assertEqual([event for event in events if event.get("type") == "setup"],
                         [dict(type="setup"), dict(type="setup")])
        with mock.patch.dict(os.environ, environment):
            c.designate(self.a, [old])
            self.assertFalse(watcher.legacy_setup_pending(str(state)))
            (c.root() / "coordinator.json").unlink()
            self.assertFalse(watcher.legacy_setup_pending(str(state)), "associated files are not new migration input")
            spool = state / "codex-watch/pending.jsonl"
            spool.parent.mkdir(parents=True, exist_ok=True)
            spool.write_text("")
            self.assertFalse(watcher.legacy_setup_pending(str(state)))
            spool.write_text(json.dumps("saved before cutover") + "\n")
            self.assertTrue(watcher.legacy_setup_pending(str(state)))

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

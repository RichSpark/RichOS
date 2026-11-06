#!/usr/bin/env python3
"""The existing review handler on isolated framed connections, no real reviewer."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import shlex
import unittest
from unittest import mock

import codex_channels as c
import review_watch as rw


class Reviews(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="codex-review-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.environment = mock.patch.dict(os.environ, {
            "CODEX_COORDINATION_ROOT": str(self.root / "coord"),
            "REVIEW_WATCH_STATE_DIR": str(self.root / "review"),
            "SECOND_REVIEW_STATE_DIR": str(self.root / "second-review"),
            "CLAUDE_CONFIG_DIR": str(self.root / "claude"),
            "RICHOS_CLAUDE_HOME": str(self.root / "claude"),
            "RICHOS_ENTITY_ROOT": "",
        })
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.repo = self.root / "work"
        self.repo.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.repo / "file").write_text("base")
        self.git("add", "file")
        self.git("-c", "core.hooksPath=/dev/null", "commit", "-m", "base")
        self.base = self.git("rev-parse", "HEAD")
        self.git("checkout", "-b", "codex/fixture")
        (self.repo / "file").write_text("implementation")
        self.git("add", "file")
        self.git("-c", "core.hooksPath=/dev/null", "commit", "-m", "change")
        self.tip = self.git("rev-parse", "HEAD")
        self.state = self.root / "app" / "engine-state"
        c.mkdir(self.state)
        self.scope = dict(version=1, state_root=str(self.state), entity_id="company-a", thread_id="chat-a")
        (self.state.parent / "entities.json").write_text(json.dumps(dict(version=2, entities=[
            dict(id="company-a", roots=[str(self.repo)], connected_repositories=[str(self.repo)]),
            dict(id="company-b", roots=[], connected_repositories=[])])))
        self.path = Path(c.connect("Exact user words", "fixture transcript line 1", scope=self.scope)["locator"])
        c.send(self.path, "ready", "## READY TO LAND: codex/fixture\nRepository: work\nCommit: " + self.tip + "\nClaims: implemented requested change")
        self.world = rw.AppWorld(str(Path(__file__).resolve().parents[2]), str(self.state), claude="/fixture/claude", accounts="/fixture/accounts")
        # Real git and shared parser; no integration subprocess or global registry.
        self.world.integration_tip = lambda chain, repo: (self.base, "")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], stderr=subprocess.DEVNULL, text=True).strip()

    def test_app_discovers_ready_once_with_exact_context_scope_and_existing_dedup(self):
        state = {}
        items, problems = self.world.codex_items(100, state)
        self.assertFalse(problems)
        self.assertEqual(len(items), 1)
        item = items[0]
        self.assertEqual((item.repo, item.tip, item.base), (os.path.realpath(self.repo),self.tip, self.base))
        self.assertEqual(item.channel, dict(locator=str(self.path), message_id="ready"))
        self.assertIn("Exact user words", Path(item.words[0]).read_text())
        self.assertNotIn("Claims:", Path(item.words[0]).read_text())
        self.assertIn("Claims:", Path(item.claims[0]).read_text())
        args = item.args("handover")
        self.assertEqual(args[args.index("--author") + 1], "codex")
        self.assertIn("/fixture/claude", args)
        self.assertEqual(len(list(rw.due(items, rw.Book([], {}, []), 100, {}, 3600))), 1)
        running = {item.key: dict(trigger="handover", started_at=100)}
        self.assertEqual(list(rw.due(items, rw.Book([], running, []), 101, {}, 3600)), [])
        # Lose the review consumer's advisory positions. Persistent handover keys
        # and the existing Book remain the scheduling fence.
        state["channels"] = {}
        self.world.codex_items(101, state)
        self.assertEqual(len(state["channel_pending"]), 1)
        row = dict(id="review-1", repo=item.repo, work=item.work, tip=item.tip, trigger="handover", verdict="passed", reviewer="claude", reviewer_model="fixture")
        rw.publish_channel_results([row], state["channel_pending"])
        rw.publish_channel_results([row], state["channel_pending"])
        result = [r for r in c.read(self.path)["records"] if r.get("kind") == "review_result"]
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["reply_to"], "ready")
        incoming = [r for r in c.read(self.path, "to-rich")["records"] if r.get("kind") == "review_result"]
        self.assertEqual(incoming, result)
        self.assertEqual(list(rw.due(items, rw.Book([row], {}, []), 102, {}, 3600)), [])

    def test_unheaded_ready_uses_the_existing_scoped_handler(self):
        path = Path(c.connect("Original plain-message task", "fixture source", scope=self.scope)["locator"])
        text = "READY TO LAND: codex/fixture\nRepository: work\nCommit: " + self.tip + "\nClaims: same work"
        c.send(path, "plain-ready", text)
        state = {}
        items, problems = self.world.channel_items(100, state, [c.connection(path)])
        self.assertFalse(problems)
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].channel, dict(locator=str(path), message_id="plain-ready"))
        self.assertEqual((items[0].repo, items[0].tip), (os.path.realpath(self.repo), self.tip))
        self.assertIn("Original plain-message task", Path(items[0].words[0]).read_text())
        self.assertIn(text, Path(items[0].claims[0]).read_text())
        self.assertEqual(c.read(path, "to-rich")["records"][0]["text"], text, "normalization never rewrites the source")

    def test_unheaded_status_or_acknowledgment_does_not_start_a_review(self):
        path = Path(c.connect("Original task", "fixture source", scope=self.scope)["locator"])
        c.send(path, "not-ready", "Not ready yet: codex/fixture is still running author proof")
        c.send(path, "ack-ready", "Acknowledged; I will post READY TO LAND for codex/fixture once the walk passes")
        c.send(path, "soon-ready", "Ready to land soon: codex/fixture still waits for its walk")
        c.send(path, "caps-soon-ready", "READY TO LAND soon: codex/fixture still waits for its walk")
        state = {}
        items, problems = self.world.channel_items(100, state, [c.connection(path)])
        self.assertFalse(problems)
        self.assertEqual(items, [], "ordinary status must not create unsolicited review work")
        self.assertEqual(state["channel_pending"], [])
        self.assertEqual(len(c.read(path)["records"]), 1, "no review result or held notice was generated")

    def test_world_uses_same_handler_but_managed_terminal_does_not_compete(self):
        world = rw.World.__new__(rw.World)
        world.repos = ["work"]
        world._merge_bases = {}
        world.repo_named = lambda name: str(self.repo) if name == "work" else ""
        world.integration_tip = lambda chain, repo: (self.base, "")
        state = {}
        items, problems = world.channel_items(100, state, [c.connection(self.path)])
        self.assertEqual(len(items), 1)
        self.assertFalse(problems)
        old = c.root() / "rich-codex"
        c.mkdir(old)
        (old / "to-rich.md").write_text("preserved legacy")
        c.designate(self.scope, [old])
        with mock.patch.dict(os.environ, {"REVIEW_WATCH_TO_RICH": str(old / "to-rich.md")}):
            self.assertEqual(world.codex_items(100, {}), ([], []))

    def test_real_watcher_starts_one_worker_and_preserves_result_correspondence(self):
        fifo = self.root / "review-finish"
        os.mkfifo(fifo)
        calls = self.root / "review-calls"
        calls.mkdir()
        fifo_fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
        fake_python = self.root / "review-worker.py"
        fake_python.write_text("import json,os,sys\nfrom pathlib import Path\n"
            "p=Path(sys.argv[1])/str(os.getpid())\n"
            "with p.with_suffix('.incoming').open('w') as f:\n f.write(json.dumps(sys.argv[3:]));f.flush();os.fsync(f.fileno())\n"
            "os.replace(p.with_suffix('.incoming'),p.with_suffix('.json'))\n"
            "with open(sys.argv[2],'rb') as f:f.read(1)\n")
        fake = self.root / "second-review.sh"
        fake.write_text("#!/bin/bash\nexec " + shlex.join([os.sys.executable, "-B", str(fake_python), str(calls), str(fifo)]) + ' "$@"\n')
        self.world.registry_items = lambda now, seen: ([], [])
        watcher = rw.Watcher(self.world.engine_root, "", self.world)
        state = {}
        now = time.time()
        with mock.patch.dict(os.environ, {"REVIEW_WATCH_SECOND_REVIEW": str(fake)}):
            try:
                watcher.look(now, state)
                # Hang guard only; the written invocation and owned live lock
                # establish the verdict, never a worker's measured start speed.
                deadline = time.monotonic() + 30
                while not list(calls.glob("*.json")) and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(list(calls.glob("*.json")), "review worker did not publish its invocation")
                locks = list((self.root / "review/locks").glob("*.lock"))
                self.assertEqual(len(locks), 1)
                lock = json.loads(locks[0].read_text())
                self.assertEqual(lock["channel"], dict(locator=str(self.path), message_id="ready"))
                saved = rw.stall_watch._read_json(rw._p("shared.json"))
                saved["codex"]["channels"] = {}
                rw.stall_watch._write_json(rw._p("shared.json"), saved)
                watcher.look(now + 1, state)
                self.assertEqual(len(list(calls.glob("*.json"))), 1, "replay started another review")
                argv = json.loads(next(calls.glob("*.json")).read_text())
                self.assertEqual(argv[argv.index("--author") + 1], "codex")
                self.assertIn("Exact user words", Path(argv[argv.index("--words-file") + 1]).read_text())
                row = dict(id="fixture-verdict", repo=lock["repo"], work=lock["work"], tip=lock["tip"],
                           at=rw._dt.datetime.fromtimestamp(now, rw._dt.timezone.utc).isoformat(),
                           trigger="handover", verdict="passed", reviewer="claude", reviewer_model="fixture")
                rw.append_jsonl(rw.review_ledger(), row)
                # Release the fixture worker through its owned FIFO, then let
                # the real watcher settle it into the existing attempts ledger.
                os.write(fifo_fd, b"x")
                watcher.children[lock["pid"]].wait(timeout=30)
                watcher.look(now + 2, state)
                attempts = rw.read_jsonl(rw._p("attempts.jsonl"))
                self.assertEqual(attempts[-1]["outcome"], "verdict")
                self.assertEqual(attempts[-1]["channel"], lock["channel"])
                result = [r for r in c.read(self.path)["records"] if r.get("kind") == "review_result"]
                self.assertEqual(len(result), 1)
                self.assertEqual(result[0]["reply_to"], "ready")
                watcher.look(now + 3, state)
                self.assertEqual(len(list(calls.glob("*.json"))), 1)
                self.assertEqual(sum(r.get("kind") == "review_result" for r in c.read(self.path)["records"]), 1)
            finally:
                os.close(fifo_fd)
                watcher.stop_own(time.time())

    def test_existing_loss_budget_and_convergence_notices_return_on_origin(self):
        state = {}
        item = self.world.codex_items(100, state)[0][0]
        owner = dict(repo=item.repo, work=item.work, tip=item.tip, channel=item.channel,
                     outcome="lost", started_at=100, ended_at=101, name="codex", why="fixture failure")
        rw.publish_channel_results([], [owner])
        self.assertFalse(any(r.get("kind") == "review_result" for r in c.read(self.path)["records"]), "one loss must not create a new notification policy")
        rw.publish_channel_results([], [owner, dict(owner, started_at=102, ended_at=103)])
        rw.publish_channel_results([], [owner, dict(owner, started_at=102, ended_at=103)])
        self.assertEqual(sum("NO VERDICT TWICE" in r["text"] for r in c.read(self.path)["records"]), 1)
        rows = []
        for index, tip in enumerate([self.base, self.tip]):
            record = self.root / ("review-" + str(index))
            record.mkdir()
            (record / "verdict.json").write_text(json.dumps(dict(
                answer=dict(earlier_findings=[dict(id="finding-1", status="still-open", blocks=True)]),
                earlier_findings_in=[dict(id="finding-1", title="Unresolved issue")])) )
            rows.append(dict(id="review-" + str(index), repo=item.repo, work=item.work, tip=tip,
                             verdict="changes-requested", trigger="handover", record=str(record)))
        rw.publish_channel_results(rows, state["channel_pending"])
        result = c.read(self.path)["records"]
        self.assertEqual(sum("NOT CONVERGING" in r["text"] for r in result), 1)
        self.assertTrue(all(r.get("reply_to") == "ready" for r in result if r.get("kind") == "review_result"))

    def test_wrong_company_repository_and_split_result_write_remain_with_origin(self):
        other = Path(c.connect("Other task", "other transcript", scope=dict(self.scope, entity_id="company-b", thread_id="chat-b"))["locator"])
        c.send(other, "wrong", "## READY TO LAND: codex/fixture\nRepository: work\nCommit: " + self.tip)
        items, _ = self.world.codex_items(100, {})
        self.assertEqual(len(items), 1)
        self.assertTrue(any("no uniquely connected repository" in r["text"] for r in c.read(other)["records"]))
        original = c.append
        def fail_incoming(path, record):
            if path.name == "to-rich.jsonl":
                raise OSError("fixture failed after outgoing sync")
            return original(path, record)
        with mock.patch.object(c, "append", side_effect=fail_incoming):
            with self.assertRaises(OSError):
                rw.channel_reply(self.path, "ready", "split", "Exact verdict")
        rw.channel_reply(self.path, "ready", "split", "Exact verdict")
        self.assertEqual(sum(r["text"] == "Exact verdict" for r in c.read(self.path)["records"]), 1)
        self.assertEqual(sum(r["text"] == "Exact verdict" for r in c.read(self.path, "to-rich")["records"]), 1)
        self.assertFalse(any(r["text"] == "Exact verdict" for r in c.read(other)["records"]))


if __name__ == "__main__":
    unittest.main()

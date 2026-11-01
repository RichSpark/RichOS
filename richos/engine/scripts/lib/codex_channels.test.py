#!/usr/bin/env python3
"""Channel persistence/isolation checks on owned fixture paths only."""
import concurrent.futures
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("codex_channels", Path(__file__).with_name("codex_channels.py"))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


class Channels(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="codex-channels-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.old = os.environ.get("CODEX_COORDINATION_ROOT")
        os.environ["CODEX_COORDINATION_ROOT"] = str(self.root / "coord")
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

    def test_send_failure_does_not_claim_saved_and_requires_id(self):
        p = self.connect(self.a)
        result = subprocess.run([sys.executable, "-B", c.__file__, "send", "--locator", p,
                                 "--text", "missing id"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(c.read(p, "to-rich")["records"], [])
        with self.assertRaises(ValueError):
            c.send(p, "oversize", "x" * (c.TEXT_LIMIT + 1))


if __name__ == "__main__":
    unittest.main()

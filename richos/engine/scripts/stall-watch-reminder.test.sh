#!/usr/bin/env bash
# Controlled-clock proof of stall_watch.py's due-work delivery (orchestration reliability R2,
# richos-hq docs/prds/2026-10-23-orchestration-reliability.md): told at the first look while
# the lead is idle, at a busy lead's next idle look, bounded with a pointer to the complete
# list, never an empty success when the registry cannot be read. It replaced the 12-hour
# unfinished-work reminder this file used to prove.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/lib/scratch.sh"
SB="$(scratch_new stall-watch-reminder-test)"
trap 'scratch_release "$SB" >/dev/null 2>&1 || true' EXIT
python3 -B - "$SCRIPT_DIR" "$SB" <<'PY'
import io
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

scripts, scratch = sys.argv[1:]
sys.path.insert(0, os.path.join(scripts, "lib"))
import stall_watch as sw


def due_item(n, state="none"):
    return {"name": "worker-%d" % n, "key": "session--worker-%d" % n, "state": state, "end": "",
            "since": 1700000000.0, "detail": "finished, not landed or discarded", "why": "waiting for its review",
            "command": "workspaces.sh land worker-%d" % n}


class DueDeliveryTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(scratch) / self._testMethodName
        self.root.mkdir()
        self.transcript = self.root / "lead.jsonl"
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        self.due = [due_item(1)]
        self.src = SimpleNamespace(ws=SimpleNamespace(
            pending=Mock(return_value=[]), due_set=Mock(side_effect=lambda *a, **k: list(self.due)),
            unreadable_line=Mock(return_value="")))
        self.reg = SimpleNamespace(problem="")
        self.now = 1700000000
        self.env = patch.dict(os.environ, {"STALL_WATCH_LEAD_TRANSCRIPT": str(self.transcript)})
        self.env.start()
        self.addCleanup(self.env.stop)
        os.environ.pop("RICHOS_WATCH_LEAD", None)   # a terminal watcher unless a case names the host's file

    def write_rows(self, *rows):
        self.transcript.write_text("".join(json.dumps(r) + "\n" for r in rows))

    def tick(self, now, out=None):
        out = out or io.StringIO()
        with patch.object(sw, "idle_end", return_value=([], {})), \
                patch.object(sw, "escalation_watch", return_value=([], {}, [])), \
                patch.object(sw, "Registry", return_value=self.reg), \
                patch.object(sw, "_slot_lines", return_value=(None, "")), \
                patch.object(sw.resource_waits, "collect", return_value={"problems": [], "holds": [], "waits": []}), \
                patch.object(sw, "_silent", return_value=([], "")), \
                patch.object(sw, "_slow_steps", return_value=[]):
            sw.tick(self.src, str(self.root), "session", now=now, out=out)
        return out.getvalue()

    def test_due_work_is_told_at_the_first_idle_look_and_again_only_at_the_repeat(self):
        first = self.tick(self.now)
        self.assertIn("[DUE] worker-1", first)
        self.assertIn("next: workspaces.sh land worker-1", first)
        self.assertEqual(self.tick(self.now + 60), "")
        self.assertIn("[DUE] worker-1", self.tick(self.now + 60 + 30 * 60))

    def test_a_busy_lead_is_told_at_its_next_idle_look_and_nothing_is_cleared_meanwhile(self):
        self.write_rows({"type": "system", "subtype": "turn_duration"},
                        {"type": "assistant", "message": {"stop_reason": "tool_use"}})
        out = "".join(self.tick(self.now + m * 60) for m in range(5))
        self.assertNotIn("[DUE]", out)
        self.src.ws.due_set.assert_not_called()
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        self.assertIn("[DUE] worker-1", self.tick(self.now + 300))
        self.write_rows({"type": "user", "message": {"content": "next"}})
        out = "".join(self.tick(self.now + 360 + m * 60) for m in range(5))
        self.assertNotIn("STALL-CLEARED", out)            # busy looks carry the due item unchanged
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        self.assertEqual(self.tick(self.now + 700), "")   # and an idle look does not repeat it early

    def test_a_handled_item_ends_without_a_line(self):
        self.tick(self.now)
        self.due = []
        out = self.tick(self.now + 60) + self.tick(self.now + 120)
        self.assertNotIn("worker-1", out)

    def test_many_due_items_are_bounded_and_point_to_the_complete_list(self):
        self.due = [due_item(n) for n in range(1, 21)]
        out = self.tick(self.now)
        self.assertEqual(out.count("[DUE] worker-"), sw.DUE_SHOWN)
        self.assertIn("[DUE] 12 more: more due work", out)
        self.assertIn("the complete list: workspaces.sh holds", out)

    def test_an_unreadable_registry_is_a_source_not_read_never_nothing_due(self):
        self.src.ws.pending.side_effect = OSError("fixture read failure")
        out = self.tick(self.now)
        self.assertIn("[NOT READ]", out)
        self.assertIn("fixture read failure", out)
        self.src.ws.pending.side_effect = None
        # Codex 03:24Z item A: a damaged record never hides the readable due work beside it.
        self.src.ws.unreadable_line.return_value = "UNREADABLE RECORD: 1 workspace registry record(s) cannot be read"
        out = self.tick(self.now + 30 * 60 + 60)
        self.assertIn("UNREADABLE RECORD", out)
        self.assertIn("[DUE] worker-1", out)

    def test_unknown_partial_or_new_user_transcript_never_counts_as_idle(self):
        cases = [b"", b'{"type":"system","subtype":"turn_duration"}', b'[]\n',
                 b'{broken\n',
                 b'{"type":"system","subtype":"turn_duration"}\n{"type":"user"}\n',
                 b'{"type":"assistant","message":{"stop_reason":"end_turn"}}\n',
                 b'{"type":"system","subtype":"turn_duration"}\n'
                 b'{"type":"attachment","attachment":{"type":"queued_command"}}\n']
        for raw in cases:
            with self.subTest(raw=raw):
                self.transcript.write_bytes(raw)
                self.assertFalse(sw.lead_is_idle(self.src, "session"))
        self.transcript.unlink()
        self.assertFalse(sw.lead_is_idle(self.src, "session"))

    def host_says(self, **lead):
        """The app host's lead file (operator_host.rs write_team_lead), named as the host names it."""
        path = self.root / "team-watch-lead.json"
        path.write_text(json.dumps(lead))
        os.environ["RICHOS_WATCH_LEAD"] = str(path)
        return path

    def test_app_host_turn_end_is_idle_for_a_print_mode_lead(self):
        """THE APP'S LEAD runs `claude --print`, whose transcript never has a turn_duration row:
        under the host, the host's word that the turn ended is the idle signal."""
        self.write_rows({"type": "user", "message": {"content": "Check on my team."}},
                        {"type": "assistant", "message": {"stop_reason": "end_turn"}})
        self.host_says(session="session", started=self.now, turn_ended=self.now - 5)
        self.assertIn("[DUE] worker-1", self.tick(self.now))
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        for lead in ({"session": "session", "started": self.now},
                     {"session": "another-session", "started": self.now, "turn_ended": self.now},
                     {"session": "session", "turn_ended": True}):
            with self.subTest(lead=lead):
                self.host_says(**lead)
                self.assertFalse(sw.lead_is_idle(self.src, "session"))

    def test_large_transcript_and_sidechain_use_latest_lead_turn_end(self):
        self.transcript.write_text('x' * (300 * 1024) + '\n' +
                                  json.dumps({"type": "system", "subtype": "turn_duration"}) + '\n' +
                                  json.dumps({"type": "assistant", "isSidechain": True}) + '\n')
        self.assertTrue(sw.lead_is_idle(self.src, "session"))

    def test_real_registry_due_set_reads_without_writing_and_excludes_running_work(self):
        ws = sw._load("reminder_test_workspaces", os.path.join(scripts, "..", "mega-lander", "workspaces.py"))
        self.assertIsNotNone(ws)
        with patch.dict(os.environ, {"RICHOS_WORKSPACES_DIR": str(self.root / "registry")}):
            ws._ensure_dirs()
            finished = ws.new_record("session--finished", name="finished-worker", session_id="session",
                                     end={"at": self.now, "signal": "SubagentStop"})
            running = ws.new_record("session--running", name="running-worker", session_id="session")
            ws.save_agent(finished)
            ws.save_agent(running)
            before = {p: p.read_bytes() for p in (self.root / "registry").rglob("*") if p.is_file()}
            self.src.ws = ws
            with patch.object(ws, "session_state", return_value=("running", "fixture")), \
                    patch.object(ws, "_active_merge", return_value=""):
                out = self.tick(self.now)
            self.assertIn("[DUE] finished-worker", out)
            self.assertNotIn("running-worker", out)
            after = {p: p.read_bytes() for p in (self.root / "registry").rglob("*") if p.is_file()}
            self.assertEqual(before, after)

    def test_output_failure_does_not_record_successful_delivery(self):
        broken = Mock()
        broken.flush.side_effect = BrokenPipeError("fixture output closed")
        with self.assertRaises(BrokenPipeError):
            self.tick(self.now, out=broken)
        self.assertFalse((self.root / "announced.json").exists())


unittest.main(argv=[sys.argv[0]], verbosity=2)
PY

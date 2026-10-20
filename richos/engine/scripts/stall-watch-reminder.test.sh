#!/usr/bin/env bash
# Controlled-clock proof of stall_watch.py's idle unfinished-work reminder.
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


class ReminderTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(scratch) / self._testMethodName
        self.root.mkdir()
        self.transcript = self.root / "lead.jsonl"
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        self.item = {"key": "session--finished", "name": "finished-worker",
                     "waiting": "started", "waiting_on": "waiting for its review", "workspaces": []}
        self.src = SimpleNamespace(ws=SimpleNamespace(
            pending=Mock(return_value=[self.item]), damaged_records=Mock(return_value=[])))
        self.start = 1700000000
        self.due = self.start + 12 * 60 * 60
        self.env = patch.dict(os.environ, {"STALL_WATCH_LEAD_TRANSCRIPT": str(self.transcript),
                                          "STALL_WATCH_SESSION_START": str(self.start)})
        self.env.start()
        self.addCleanup(self.env.stop)

    def write_rows(self, *rows):
        self.transcript.write_text("".join(json.dumps(r) + "\n" for r in rows))

    def remind(self, now, prev=None):
        return sw.pending_work_reminder(self.src, now, "session", prev)

    def test_early_looks_do_not_scan_or_notify(self):
        lines, state, problems = self.remind(self.due - 1)
        self.assertEqual((lines, problems), ([], []))
        self.src.ws.pending.assert_not_called()
        self.assertEqual(state["checked_at"], self.start)

    def test_due_idle_reminder_includes_recorded_wait_without_mutating_it(self):
        before = dict(self.item)
        lines, state, problems = self.remind(self.due)
        self.assertIn("UNFINISHED-WORK", "\n".join(lines))
        self.assertIn("waiting for its review", "\n".join(lines))
        self.assertEqual(problems, [])
        self.assertEqual(state["checked_at"], self.due)
        self.assertEqual(self.item, before)
        self.src.ws.pending.assert_called_once_with("session", auto=False, dry=True)

    def test_busy_turn_defers_past_due_until_recorded_idle(self):
        self.write_rows({"type": "system", "subtype": "turn_duration"},
                        {"type": "assistant", "message": {"stop_reason": "tool_use"}})
        lines, state, _ = self.remind(self.due + 3600)
        self.assertEqual(lines, [])
        self.src.ws.pending.assert_not_called()
        self.write_rows({"type": "system", "subtype": "turn_duration"})
        lines, state, _ = self.remind(self.due + 3601, state)
        self.assertTrue(lines)
        self.assertEqual(state["checked_at"], self.due + 3601)

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
                self.assertEqual(self.remind(self.due)[0], [])
        self.src.ws.pending.assert_not_called()
        self.transcript.unlink()
        self.assertEqual(self.remind(self.due)[0], [])

    def test_large_transcript_and_sidechain_use_latest_lead_turn_end(self):
        self.transcript.write_text('x' * (300 * 1024) + '\n' +
                                  json.dumps({"type": "system", "subtype": "turn_duration"}) + '\n' +
                                  json.dumps({"type": "assistant", "isSidechain": True}) + '\n')
        self.assertTrue(self.remind(self.due)[0])

    def test_empty_list_stays_quiet_and_is_not_rescanned_every_minute(self):
        self.src.ws.pending.return_value = []
        lines, state, problems = self.remind(self.due)
        self.assertEqual((lines, problems), ([], []))
        self.assertEqual(self.remind(self.due + 60, state)[0], [])
        self.src.ws.pending.assert_called_once()

    def test_reminder_repeats_after_interval_even_when_wait_is_unchanged(self):
        _, state, _ = self.remind(self.due)
        self.assertEqual(self.remind(self.due + 43199, state)[0], [])
        self.assertTrue(self.remind(self.due + 43200, state)[0])

    def test_failed_registry_read_is_reported_and_does_not_spend_reminder(self):
        self.src.ws.pending.side_effect = OSError("fixture read failure")
        lines, state, problems = self.remind(self.due)
        self.assertEqual(lines, [])
        self.assertIn("fixture read failure", problems[0])
        self.src.ws.pending.side_effect = None
        self.assertTrue(self.remind(self.due + 60, state)[0])
        self.src.ws.damaged_records.return_value = ["damaged record"]
        self.assertIn("unreadable records", self.remind(self.due)[2][0])

    def test_tick_persists_clock_and_sends_one_native_monitor_block(self):
        with patch.object(sw, "idle_end", return_value=([], {})), \
                patch.object(sw, "escalation_watch", return_value=([], {}, [])), \
                patch.object(sw, "look", return_value=([], [])):
            first = io.StringIO()
            sw.tick(self.src, str(self.root), "session", now=self.due, out=first, json_lines=True)
            self.assertIn("UNFINISHED-WORK", json.loads(first.getvalue())["text"])
            self.assertEqual(len(first.getvalue().splitlines()), 1)
            restarted = io.StringIO()
            sw.tick(self.src, str(self.root), "session", now=self.due + 60, out=restarted)
            self.assertEqual(restarted.getvalue(), "")
            later = io.StringIO()
            sw.tick(self.src, str(self.root), "session", now=self.due + 43200, out=later)
            self.assertIn("UNFINISHED-WORK", later.getvalue())

    def test_persistent_read_failure_does_not_clear_during_busy_turns(self):
        self.src.ws.pending.side_effect = OSError("fixture persistent read failure")
        output = io.StringIO()
        with patch.object(sw, "idle_end", return_value=([], {})), \
                patch.object(sw, "escalation_watch", return_value=([], {}, [])), \
                patch.object(sw, "look", return_value=([], [])):
            for minute in range(121):
                if minute % 4 == 0:
                    self.write_rows({"type": "system", "subtype": "turn_duration"})
                else:
                    self.write_rows({"type": "user", "message": {"content": "working"}})
                sw.tick(self.src, str(self.root), "session", now=self.due + minute * 60, out=output)
            self.assertNotIn("STALL-CLEARED", output.getvalue())
            self.assertEqual(output.getvalue().count("STALL-WATCH "), 5)
            self.src.ws.pending.side_effect = None
            self.src.ws.pending.return_value = []
            self.write_rows({"type": "system", "subtype": "turn_duration"})
            recovered = io.StringIO()
            sw.tick(self.src, str(self.root), "session", now=self.due + 7260, out=recovered)
            sw.tick(self.src, str(self.root), "session", now=self.due + 7320, out=recovered)
            self.assertEqual(recovered.getvalue().count("STALL-CLEARED"), 1)

    def test_output_failure_does_not_record_successful_delivery(self):
        with patch.object(sw, "idle_end", return_value=([], {})), \
                patch.object(sw, "escalation_watch", return_value=([], {}, [])), \
                patch.object(sw, "look", return_value=([], [])):
            broken = Mock()
            broken.flush.side_effect = BrokenPipeError("fixture output closed")
            with self.assertRaises(BrokenPipeError):
                sw.tick(self.src, str(self.root), "session", now=self.due, out=broken)
            self.assertFalse((self.root / "announced.json").exists())

    def test_real_pending_reader_preserves_registry_and_excludes_running_work(self):
        ws = sw._load("reminder_test_workspaces", os.path.join(scripts, "..", "mega-lander", "workspaces.py"))
        self.assertIsNotNone(ws)
        with patch.dict(os.environ, {"RICHOS_WORKSPACES_DIR": str(self.root / "registry")}):
            ws._ensure_dirs()
            finished = ws.new_record("session--finished", name="finished-worker", session_id="session",
                                     end={"at": self.start, "signal": "SubagentStop"},
                                     waiting={"kind": "started", "on": "waiting for its review"})
            running = ws.new_record("session--running", name="running-worker", session_id="session")
            ws.save_agent(finished)
            ws.save_agent(running)
            before = {p: p.read_bytes() for p in (self.root / "registry").rglob("*") if p.is_file()}
            self.src.ws = ws
            with patch.object(ws, "session_state", return_value=("running", "fixture")), \
                    patch.object(ws, "_active_merge", return_value=""):
                lines, _, problems = self.remind(self.due)
            self.assertEqual(problems, [])
            self.assertIn("finished-worker", "\n".join(lines))
            self.assertNotIn("running-worker", "\n".join(lines))
            after = {p: p.read_bytes() for p in (self.root / "registry").rglob("*") if p.is_file()}
            self.assertEqual(before, after)


unittest.main(argv=[sys.argv[0]], verbosity=2)
PY

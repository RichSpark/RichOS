# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

COMPONENT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(COMPONENT / "core"), str(COMPONENT / "adapters")]
import import_records
from app import execute


class NeutralImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="neutral import ")
        self.root = Path(self.temp.name) / "destination"
        self.target = {"person_id": "ceo-default", "entity_id": "depot", "thread_id": "thread-a"}
        self.item = {"type": "obligation", "id": "old-task-7", "digest": "", "entity_id": "depot",
            "person_id": "ceo-default", "visibility": "ceo_private", "created_at": None, "observed_at": None,
            "provenance": {"ref": "legacy:engine/ceo-wiki/wiki/shipping.md#manual"},
            "evidence": [{"ref": "legacy:missing-review", "available": False}], "related": [], "supersedes": None,
            "authority": "unknown", "confidence": 0.5,
            "payload": {"title": "Review the depot manual", "details": "Historical task recorded before relocation.", "status": "running"}}
        self.item["digest"] = import_records.item_digest(self.item)
        self.envelope = {"schema": 1, "batch_id": "legacy-batch", "source": {"system": "synthetic-wiki-adapter",
            "namespace": "fictional-depot", "revision": "704b4596d6bacf1757db99ffec2ba17e7b1530e4",
            "layout": "app-v1.0.2-top-level-engine"}, "items": [self.item]}

    def tearDown(self):
        self.temp.cleanup()

    def bind(self):
        return execute(self.root, {"protocol": 1, "command": "bind", "scope": {"entity_id": "depot",
            "thread_id": "thread-a", "session_id": "new-session", "turn_id": "import-turn", "audience": "ceo"},
            "request_id": "bind-import", "source_ref": "ledger:thread-a:import-turn", "expected_revision": None})["binding"]

    def test_preview_is_read_only_then_retry_preserves_identity_and_unknown_evidence(self):
        result = import_records.preview(self.root, self.envelope, self.target)
        self.assertFalse(self.root.exists())
        self.assertEqual(result["items"][0]["disposition"], "importable")
        binding = self.bind()
        first = import_records.apply(self.root, self.envelope, binding)
        self.assertTrue(first["complete"])
        self.assertEqual(import_records.apply(self.root, self.envelope, binding)["items"][0]["disposition"], "already_imported")
        records = execute(self.root, {"protocol": 1, "command": "inspect", "binding": binding,
            "query": {"section": "open_loop"}})["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["status"], "pending")
        self.assertEqual(records[0]["source_ref"], self.item["provenance"]["ref"])
        self.assertIn('"observed_at":null', records[0]["details"])
        self.assertIn('"available":false', records[0]["details"])
        self.assertFalse((self.root / "workspaces").exists())

    def test_an_import_applies_on_one_of_his_thread_seats(self):
        """His threads each hold their own cursor, and an import is fenced on it.

        continuity.item_opened is conversational, so it is fenced against the row
        belonging to the event's own person: with the person left at the
        ceo-default literal, an import from a thread seat carried this thread's
        revision to another thread's row. The envelope's TARGET person stays
        ceo-default -- it is inside the receipt digest, so the batch is still read
        as already imported when the next thread tries it.
        """
        legacy = self.bind()
        seat = "ceo-thread:thread-a"
        thread = execute(self.root, {"protocol": 1, "command": "bind", "seat": seat,
            "scope": {"entity_id": "depot", "thread_id": "thread-a", "session_id": "session-a",
                      "turn_id": "a-turn-1", "audience": "ceo"},
            "request_id": "bind-thread-seat", "source_ref": "ledger:thread-a:a-turn-1",
            "expected_revision": None})["binding"]
        applied = execute(self.root, {"protocol": 1, "command": "import-apply", "seat": seat,
            "binding": thread, "envelope": self.envelope})
        self.assertTrue(applied["complete"])
        records = execute(self.root, {"protocol": 1, "command": "inspect", "seat": seat,
            "binding": thread, "query": {"section": "open_loop"}})["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["person_id"], seat)
        # Idempotent across his seats, which is what keeping the target person
        # fixed buys: the legacy cursor sees the batch as already imported.
        again = execute(self.root, {"protocol": 1, "command": "import-apply",
            "binding": legacy, "envelope": self.envelope})
        self.assertEqual(again["items"][0]["disposition"], "already_imported")

    def test_interrupted_domain_write_resumes_without_duplicates(self):
        binding = self.bind()
        append = import_records.append_receipt
        def fail_receipt(root, row):
            if row["kind"] == "applied":
                raise OSError("synthetic interrupted receipt")
            return append(root, row)
        with patch.object(import_records, "append_receipt", fail_receipt):
            result = import_records.apply(self.root, self.envelope, binding)
        self.assertFalse(result["complete"])
        self.assertEqual(result["items"][0]["disposition"], "failed")
        self.assertTrue(import_records.apply(self.root, self.envelope, binding)["complete"])
        store = import_records.EventStore(self.root)
        with store.connect() as conn:
            self.assertEqual(conn.execute("SELECT count(*) FROM ecs_events WHERE event_type='continuity.item_opened'").fetchone()[0], 1)

    def test_a_confirmed_waiting_order_uses_the_imported_item(self):
        binding = self.bind()
        imported = execute(self.root, {"protocol": 1, "command": "import-apply", "binding": binding,
                                      "envelope": self.envelope})
        item_id = imported["items"][0]["target_id"]
        self.assertEqual(execute(self.root, {"protocol": 1, "command": "waiting-work"})["items"], [])
        checkpoint = {"protocol": 1, "command": "checkpoint", "binding": binding,
            "request_id": "confirm-continue", "checkpoint": {"launch": True,
                "wait_for": [{"kind": "assignment", "thread_id": "other-thread", "id": "owner-job"}],
                "statements": [{"verb": "update", "fields": {"id": item_id, "next": "Send the reviewed manual"}}]}}
        from ecs_core import ScopeError
        with self.assertRaises(ScopeError):
            execute(self.root, checkpoint)
        checkpoint["instruction"] = {"ledger_ref": "ledger:thread-a:import-turn", "sha256": "0" * 64}
        receipt = execute(self.root, checkpoint)
        self.assertTrue(receipt["accepted"], receipt)
        self.assertTrue(execute(self.root, checkpoint)["duplicate"])
        rows = execute(self.root, {"protocol": 1, "command": "waiting-work"})["items"]
        self.assertEqual([(r["item_id"], r["thread_id"], r["launch"]) for r in rows], [(item_id, "thread-a", True)])
        self.assertEqual(rows[0]["conditions"][0]["condition"]["thread_id"], "other-thread")
        records = execute(self.root, {"protocol": 1, "command": "inspect", "binding": binding,
                  "query": {"section": "open_loop"}})["records"]
        self.assertEqual([(r["item_id"], r["status"], r["reply_owed"]) for r in records], [(item_id, "blocked", 1)])
        unsolicited = execute(self.root, {"protocol": 1, "command": "checkpoint", "binding": binding,
            "request_id": "claim-unverified", "checkpoint": {"statements": [{"verb": "update",
                "fields": {"id": item_id, "status": "active"}}]}})
        self.assertFalse(unsolicited["accepted"])

    def test_changes_and_cross_scope_mappings_conflict(self):
        binding = self.bind()
        import_records.apply(self.root, self.envelope, binding)
        changed = copy.deepcopy(self.envelope)
        changed["batch_id"] = "changed-batch"
        changed["items"][0]["payload"]["title"] = "Changed source task"
        changed["items"][0]["digest"] = import_records.item_digest(changed["items"][0])
        self.assertEqual(import_records.preview(self.root, changed, self.target)["items"][0]["disposition"], "conflicting")
        changed["items"][0]["entity_id"] = "studio"
        changed["items"][0]["digest"] = import_records.item_digest(changed["items"][0])
        self.assertEqual(import_records.preview(self.root, changed, {**self.target, "entity_id": "studio"})["items"][0]["disposition"], "conflicting")

    def test_invalid_unsupported_and_unresolved_items_remain_individual(self):
        invalid = {**self.item, "id": "invalid", "digest": "wrong"}
        unsupported = {**self.item, "id": "configuration", "type": "configuration"}
        unresolved = {**self.item, "id": "related", "related": ["missing-record"]}
        unresolved["digest"] = import_records.item_digest(unresolved)
        self.envelope["items"].extend([invalid, unsupported, unresolved])
        result = import_records.apply(self.root, self.envelope, self.bind())
        self.assertEqual([row["disposition"] for row in result["items"]], ["imported", "invalid", "unsupported", "unresolved"])
        self.assertFalse(result["complete"])

    def test_engine_relocation_changes_source_locator_not_stable_mapping(self):
        # This older source revision has top-level engine/ and ceo-wiki/.
        # It does not claim later Mega Lander or ASS Kicker directories existed.
        legacy = Path(self.temp.name) / "old-checkout/engine/ceo-wiki/wiki"
        legacy.mkdir(parents=True)
        (legacy / "shipping.md").write_text("Fictional source only")
        import_records.apply(self.root, self.envelope, self.bind())
        relocated = copy.deepcopy(self.envelope)
        relocated["batch_id"] = "after-outer-relocation"
        relocated["source"]["layout"] = "richos-engine-component-layout"
        relocated["source"]["revision"] = "59b492c3d3d4d2b01d216724c2da3596bcaa8252"
        self.assertEqual(import_records.preview(self.root, relocated, self.target)["items"][0]["disposition"], "already_imported")
        relocated["schema"] = 99
        with self.assertRaisesRegex(Exception, "unsupported import schema"):
            import_records.preview(self.root, relocated, self.target)


if __name__ == "__main__":
    unittest.main()

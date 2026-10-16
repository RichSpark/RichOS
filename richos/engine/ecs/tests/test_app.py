# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import shutil
import sqlite3
import unittest
from unittest.mock import patch

COMPONENT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(COMPONENT / "core"), str(COMPONENT / "adapters")]
from app import execute
from ecs_core import EventStore, ValidationError, ScopeError
from mcp import call as tool_call


class AppProtocolTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs app ")
        self.root = Path(self.temp.name)
        self.store = self.root / "state"
        self.binding = self.bind("depot", "thread-a", "session-a", "turn-1", None)

    def tearDown(self):
        self.temp.cleanup()

    def call(self, command, ok=True, **fields):
        request = {"protocol": 1, "command": command, **fields}
        if hasattr(self, "binding"):
            request.setdefault("binding", self.binding)
        result = subprocess.run([sys.executable, str(COMPONENT / "bin/ecs"), "--state-root", str(self.store)],
            input=json.dumps(request), text=True, capture_output=True,
            env={**os.environ, "ECS_HOME": str(self.root / "do-not-adopt")})
        output = json.loads(result.stdout)
        self.assertEqual(result.returncode, 0 if ok else 2, result.stdout + result.stderr)
        self.assertEqual(output["ok"], ok)
        self.assertFalse((self.root / "do-not-adopt").exists())
        return output["result"] if ok else output["error"]

    def bind(self, entity, thread, session, turn, revision):
        return self.call("bind", request_id=f"bind-{session}-{turn}", source_ref=f"ledger:{thread}:{turn}",
            expected_revision=revision, scope={"entity_id": entity, "thread_id": thread,
                "session_id": session, "turn_id": turn, "audience": "ceo"})["binding"]

    def checkpoint(self, statements, request_id="checkpoint-1", ok=True):
        return self.call("checkpoint", request_id=request_id, checkpoint={"statements": statements}, ok=ok)

    def test_standing_stu_has_an_independent_cursor_and_the_same_record_visibility(self):
        original = self.binding.copy()
        stu = self.call("bind", seat="stu-thread:thread-a", request_id="stu-peer-1",
                        source_ref="peer:1", expected_revision=None,
                        scope={"entity_id":"depot", "thread_id":"thread-a",
                               "session_id":"stu-session", "turn_id":"peer-1", "audience":"rich"})["binding"]
        result = self.call("checkpoint", seat="stu-thread:thread-a", binding=stu,
                           request_id="stu-checkpoint-1", checkpoint={"statements":[
                               {"verb":"commitment", "fields":{"id":"shared-work", "title":"Inspect the shared fixture"}}]})
        self.assertTrue(result["accepted"])
        self.assertEqual(self.call("current")["binding"], original)
        self.assertIn("Inspect the shared fixture", self.call("brief")["text"])
        self.assertIn("Inspect the shared fixture", self.call("brief", seat="stu-thread:thread-a", binding=stu)["text"])

    def test_process_restart_and_duplicate_checkpoint_preserve_one_obligation(self):
        statement = {"verb": "commitment", "fields": {"id": "ship-manual", "title": "Ship the fictional depot manual"}}
        self.assertTrue(self.checkpoint([statement])["accepted"])
        self.assertTrue(self.checkpoint([statement])["duplicate"])
        inspected = self.call("inspect", query={"section": "commitment"})
        self.assertEqual(len(inspected["records"]), 1)
        self.assertEqual(inspected["records"][0]["item_id"], "ship-manual")
        self.assertIn("depot manual", self.call("brief")["text"])
        changed = {"verb": "commitment", "fields": {"id": "ship-manual", "title": "Changed content"}}
        self.assertIn("different content", self.checkpoint([changed], ok=False)["message"])

    def test_prepared_turn_preserves_obligations_and_a_retry_does_not_rebind(self):
        self.checkpoint([{"verb": "commitment", "fields": {"id": "manual", "title": "Draft the depot manual"}}])
        scope = {**{k: v for k, v in self.binding.items() if k != "revision"}, "turn_id": "turn-2"}
        fields = dict(scope=scope, request_id="session-a:turn-2", source_ref="ledger:thread-a:turn-2")
        prepared = self.call("prepare-turn", **fields)
        self.assertIn("Draft the depot manual", prepared["text"])
        self.assertIsNone(prepared["receipt_error"])
        self.assertIsInstance(prepared["receipts"], dict)
        self.binding = prepared["binding"]
        before = EventStore(self.store).current_context()["revision"]
        repeated = self.call("prepare-turn", **fields)
        self.assertEqual(repeated["binding"], self.binding)
        self.assertEqual(EventStore(self.store).current_context()["revision"], before)
        self.assertEqual(self.call("inspect", query={"item_id": "manual"})["item"]["status"], "active")

    def test_prepared_turn_discloses_a_receipt_failure_and_still_fences_the_brief(self):
        fields = {"protocol": 1, "command": "prepare-turn",
            "scope": {k: v for k, v in self.binding.items() if k != "revision"},
            "request_id": "session-a:turn-1", "source_ref": "ledger:thread-a:turn-1"}
        with patch('loro_receipts.synchronize', side_effect=OSError('fixture receipt unreadable')):
            result = execute(self.store, fields)
        self.assertEqual(result['receipt_error'], 'fixture receipt unreadable')
        self.assertIsNone(result['receipts'])

        def supersede(store, binding, person):
            self.bind('studio', 'thread-b', 'session-b', 'turn-2', binding['revision'])
            return {'receipts': []}
        with patch('loro_receipts.synchronize', side_effect=supersede), self.assertRaises(ScopeError):
            execute(self.store, fields)

    def test_prepared_turn_refuses_a_worker_before_creating_any_state(self):
        absent = self.root / 'absent'
        with self.assertRaises(ScopeError):
            execute(absent, {'protocol': 1, 'command': 'prepare-turn',
                'scope': {'entity_id': 'depot', 'thread_id': 'worker', 'session_id': 'worker',
                          'turn_id': 'work', 'audience': 'worker'},
                'seat': 'work-seat:work', 'request_id': 'work', 'source_ref': 'fixture'})
        self.assertFalse(absent.exists())

    def test_prepared_turn_keeps_concurrent_conversation_seats_separate(self):
        def prepared(thread):
            return self.call('prepare-turn', seat=f'ceo-thread:{thread}',
                scope={'entity_id': 'depot', 'thread_id': thread, 'session_id': thread,
                       'turn_id': 'turn-1', 'audience': 'ceo'},
                request_id=f'{thread}:turn-1', source_ref=f'ledger:{thread}:turn-1')
        first = prepared('first')['binding']
        second = prepared('second')['binding']
        self.assertEqual(self.call('current', seat='ceo-thread:first')['binding'], first)
        self.assertEqual(self.call('current', seat='ceo-thread:second')['binding'], second)
        self.call('brief', seat='ceo-thread:first', binding=first)
        error = self.call('brief', seat='ceo-thread:second', binding=first, ok=False)
        self.assertEqual(error['kind'], 'ScopeError')

    def test_stale_and_cross_entity_bindings_are_rejected(self):
        old = self.binding
        self.binding = self.bind("studio", "thread-b", "session-b", "turn-2", old["revision"])
        self.call("brief", binding=old, ok=False)
        error = self.call("checkpoint", binding=old, request_id="stale",
                          checkpoint={"no_changes": True, "reason": "Nothing changed"}, ok=False)
        self.assertEqual(error["kind"], "ScopeError")
        self.assertEqual(self.call("inspect")["entity"], "studio")

    def test_budget_does_not_make_omitted_obligations_unreachable(self):
        for batch in range(3):
            self.checkpoint([{"verb": "open_loop", "fields": {"id": f"loop-{batch}-{i}",
                "title": f"Fictional depot obligation {batch}-{i} with a distinct delivery check"}}
                for i in range(12)], request_id=f"batch-{batch}")
        brief = self.call("brief", budget_chars=900)
        self.assertLessEqual(len(brief["text"]), 900)
        self.assertEqual(brief["inspection"]["counts"]["open_loop"], 36)
        page = self.call("inspect", query={"section": "open_loop", "limit": 10})
        ids = set()
        while True:
            ids.update(row["item_id"] for row in page["records"])
            if page["next_offset"] is None:
                break
            page = self.call("inspect", query={"section": "open_loop", "limit": 10,
                "sequence": page["sequence"], "offset": page["next_offset"]})
        self.assertEqual(len(ids), 36)

    def test_partial_checkpoint_retries_without_duplicate_open(self):
        request = {"protocol": 1, "command": "checkpoint", "binding": self.binding,
            "request_id": "interrupted", "checkpoint": {"statements": [
                {"verb": "commitment", "fields": {"id": "manual", "title": "Draft the manual"}},
                {"verb": "update", "fields": {"id": "manual", "title": "Draft and review the manual"}},
            ]}}
        append = EventStore.append
        def interrupted(store, event, **args):
            result = append(store, event, **args)
            if event == "continuity.item_opened":
                raise RuntimeError("synthetic crash after commit")
            return result
        with patch.object(EventStore, "append", interrupted), self.assertRaises(RuntimeError):
            execute(self.store, request)
        result = execute(self.store, request)
        self.assertTrue(result["accepted"])
        with EventStore(self.store).connect() as conn:
            self.assertEqual(conn.execute("SELECT COUNT(*) FROM ecs_events WHERE event_type='continuity.item_opened'").fetchone()[0], 1)
        self.assertEqual(self.call("inspect", query={"item_id": "manual"})["item"]["title"], "Draft and review the manual")

    def test_checkpoint_cannot_certify_external_completion(self):
        self.checkpoint([{"verb": "commitment", "fields": {"id": "work", "title": "Build the artifact"}}])
        error = self.checkpoint([{"verb": "close", "fields": {"id": "work", "evidence": "I said it is done"}}],
                                request_id="claim", ok=False)
        self.assertIn("verification receipt", error["message"])
        error = self.checkpoint([{"verb": "update", "fields": {"id": "work", "status": "completed"}}],
                                request_id="update-claim", ok=False)
        self.assertIn("verification receipt", error["message"])
        self.assertEqual(self.call("inspect", query={"item_id": "work"})["item"]["status"], "active")

    def test_hello_is_read_only_and_no_implicit_state_root_is_accepted(self):
        absent = self.root / "absent"
        self.assertEqual(execute(absent, {"protocol": 1, "command": "hello"})["protocol"], 1)
        self.assertFalse(absent.exists())
        result = subprocess.run([sys.executable, str(COMPONENT / "bin/ecs")], input="{}", text=True, capture_output=True)
        self.assertEqual(result.returncode, 2)

    def test_upgrade_backs_up_readable_state_and_downgrade_is_refused(self):
        migrations = self.root / "migration-copy"
        shutil.copytree(COMPONENT / "migrations", migrations)
        store = EventStore(self.store)
        store.migrations_dir = migrations
        (migrations / "008_fixture.sql").write_text("CREATE TABLE fixture_upgrade(value TEXT);")
        store.initialize()
        backups = list((self.store / "backups").glob("*.sqlite3"))
        self.assertEqual(len(backups), 1)
        with sqlite3.connect(backups[0]) as previous:
            self.assertEqual(previous.execute("SELECT max(version) FROM schema_migrations").fetchone()[0], 7)
            self.assertGreater(previous.execute("SELECT count(*) FROM ecs_events").fetchone()[0], 0)
        with self.assertRaisesRegex(ValidationError, "downgrade refused"):
            EventStore(self.store).initialize()
        # Restoring the backup in a separate root is compatible with old code.
        restored = self.root / "restored"
        restored.mkdir()
        shutil.copyfile(backups[0], restored / "ecs.sqlite3")
        EventStore(restored).initialize()
        self.assertEqual(EventStore(restored).current_context()["entity_id"], "depot")

    def test_failed_migration_keeps_old_schema_and_backup(self):
        migrations = self.root / "broken-migrations"
        shutil.copytree(COMPONENT / "migrations", migrations)
        (migrations / "008_broken.sql").write_text("CREATE TABLE rolled_back(value TEXT); INVALID SQL;")
        store = EventStore(self.store)
        store.migrations_dir = migrations
        with self.assertRaises(sqlite3.Error):
            store.initialize()
        with store.connect() as conn:
            self.assertIsNone(conn.execute("SELECT name FROM sqlite_master WHERE name='rolled_back'").fetchone())
            self.assertEqual(conn.execute("SELECT max(version) FROM schema_migrations").fetchone()[0], 7)
        self.assertEqual(len(list((self.store / "backups").glob("*.sqlite3"))), 1)
        EventStore(self.store).initialize()

    def test_tools_cannot_override_scope_or_mutate_during_hidden_priming(self):
        path = self.root / "scope.json"
        scope = {"version": 1, "actions_allowed": False, "bridge": {"state_root": str(self.store)}, "binding": self.binding}
        path.write_text(json.dumps(scope))
        args = {"request_id": "hidden", "checkpoint": {"no_changes": True, "reason": "Test"}}
        with self.assertRaisesRegex(ValueError, "outside a visible"):
            tool_call(path, "checkpoint", args)
        scope["actions_allowed"] = True
        path.write_text(json.dumps(scope))
        with self.assertRaises(ValueError):
            tool_call(path, "checkpoint", {**args, "binding": self.binding})
        self.assertTrue(tool_call(path, "checkpoint", args)["accepted"])
        self.bind("studio", "thread-b", "session-b", "turn-b", self.binding["revision"])
        with self.assertRaisesRegex(Exception, "stale app binding"):
            tool_call(path, "inspect", {})

    def test_a_scope_carrying_the_users_words_is_still_read(self):
        """Second review, slice 4: the scope now carries the user's own words beside their hash
        (richos-core ecs.rs UserInstruction::text, up to 128 KiB escaped). The continuity tools
        read the same file, so their 16 KiB bound would have refused every call on a long turn."""
        path = self.root / "scope.json"
        words = "Please do exactly this. " * 4000           # about 96 KB, over the old 16 KiB bound
        path.write_text(json.dumps({"version": 1, "actions_allowed": True, "bridge": {"state_root": str(self.store)},
                                    "binding": self.binding, "user_instruction": {
                                        "ledger_ref": "ledger:t:1", "sha256": "x", "text": words}}))
        self.assertGreater(path.stat().st_size, 16384)
        self.assertTrue(tool_call(path, "checkpoint", {"request_id": "long-turn",
                                                       "checkpoint": {"no_changes": True, "reason": "Test"}})["accepted"])
        path.write_text(json.dumps({"version": 1, "actions_allowed": True, "padding": "x" * (256 * 1024)}))
        with self.assertRaisesRegex(ValueError, "invalid app continuity scope"):
            tool_call(path, "inspect", {})


class WorkSeatTests(unittest.TestCase):
    """The tenth turn gate: a background assignment gets its own cursor.

    ``ecs_active_context`` is a CURSOR keyed by ``person_id``, not a store, and the
    conversation rewrites its row at the start of every turn. A background lease
    holding a binding frozen minutes earlier is therefore current until the CEO's
    next sentence and stale from then on -- and the gate is invisible today only
    because every work-tool call happens inside the turn that bound it.

    EVERY TEST HERE RUNS THREE OF HIS TURNS, and that is the whole point: with the
    two rows left sitting at the same revision, the broken form passes. That is
    exactly how the two appends below were shipped as "unaffected".
    """

    SEAT = "work-seat:assign-7"
    OTHER = "work-seat:assign-8"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs seat ")
        self.root = Path(self.temp.name)
        self.state = self.root / "state"
        self.ceo = self.bind("depot", "thread-a", "session-conv", "turn-1", None)

    def tearDown(self):
        self.temp.cleanup()

    def call(self, command, ok=True, seat=None, **fields):
        request = {"protocol": 1, "command": command, **fields}
        if seat is not None:
            request["seat"] = seat
        result = subprocess.run(
            [sys.executable, str(COMPONENT / "bin/ecs"), "--state-root", str(self.state)],
            input=json.dumps(request), text=True, capture_output=True,
            env={**os.environ, "ECS_HOME": str(self.root / "do-not-adopt")})
        output = json.loads(result.stdout)
        self.assertEqual(result.returncode, 0 if ok else 2, result.stdout + result.stderr)
        self.assertEqual(output["ok"], ok, result.stdout)
        return output["result"] if ok else output["error"]["message"]

    def bind(self, entity, thread, session, turn, revision, seat=None, audience="ceo"):
        return self.call(
            "bind", seat=seat, request_id=f"bind-{seat or 'ceo'}-{turn}",
            source_ref=f"ledger:{thread}:{turn}", expected_revision=revision,
            scope={"entity_id": entity, "thread_id": thread, "session_id": session,
                   "turn_id": turn, "audience": audience})["binding"]

    def work_seat(self, seat=None, turn="assign-7"):
        """One seat per assignment, named by the assignment and bound as a worker."""
        return self.bind("depot", "thread-a", f"session-{turn}", turn, None,
                         seat=seat or self.SEAT, audience="worker")

    def three_ceo_turns(self):
        """His next three sentences. Each one rewrites HIS row and bumps its revision."""
        for number in (2, 3, 4):
            self.ceo = self.bind("depot", "thread-a", "session-conv", f"turn-{number}",
                                 number - 1)
        self.assertEqual(self.ceo["revision"], 4)
        return self.ceo

    def observe(self, binding, seat, request_id, external, ok=True):
        return self.call("observe", ok=ok, seat=seat, binding=binding,
                         source_ref=f"app-dispatch:{request_id}", request_id=request_id,
                         work={"authority": "richos-provider-v1", "work_unit_id": "wu-" + request_id,
                               "external_id": external, "title": "land three branches",
                               "owner": "work", "status": "started"})

    def rows(self, sql, *args):
        store = EventStore(self.state)
        conn = store.connect()
        try:
            return [dict(row) for row in conn.execute(sql, args).fetchall()]
        finally:
            conn.close()

    def test_a_work_seat_survives_the_conversation_rebinding_its_own_seat(self):
        frozen = self.work_seat()
        self.three_ceo_turns()
        # The claim: three of his turns, his row at revision 4, and the assignment's
        # frozen binding still passes its own fence.
        self.assertEqual(self.call("inspect", seat=self.SEAT, binding=frozen)["turn"], "assign-7")
        # The negative half, and it is the one that matters: a seat that does not
        # exist must RAISE, never fall back to his row. A fallback would pass this
        # test's positive half forever while writing background records onto his
        # cursor.
        self.assertIn("stale app binding",
                      self.call("inspect", ok=False, seat="work-seat:never-bound", binding=frozen))
        # And the same call with no seat at all reads his row, which is not this
        # binding -- so the absence of a seat is a refusal, not a default.
        self.assertIn("stale app binding", self.call("inspect", ok=False, binding=frozen))
        # Positive control on the other side: HIS binding still works on his seat.
        self.assertEqual(self.call("inspect", binding=self.ceo)["turn"], "turn-4")

    def test_a_second_work_seat_does_not_invalidate_the_first_assignments_binding(self):
        first = self.work_seat()
        self.three_ceo_turns()
        second = self.work_seat(seat=self.OTHER, turn="assign-8")
        # Both assignments are live at once, which section 5.5, 5.6 and the worked
        # example all require. Neither one's cursor is the other's.
        self.assertEqual(self.call("inspect", seat=self.SEAT, binding=first)["turn"], "assign-7")
        self.assertEqual(self.call("inspect", seat=self.OTHER, binding=second)["turn"], "assign-8")
        # The negative control is the design this replaced: ONE seat shared by two
        # assignments is gate ten again, with the CEO replaced by the work lease's
        # own next assignment.
        shared = self.bind("depot", "thread-a", "session-assign-9", "assign-9", 1,
                           seat=self.SEAT, audience="worker")
        self.assertEqual(shared["turn_id"], "assign-9")
        self.assertIn("stale app binding",
                      self.call("inspect", ok=False, seat=self.SEAT, binding=first))

    def test_binding_a_work_seat_does_not_re_register_the_entity(self):
        self.work_seat()
        registry = {row["event_type"]: row["person_id"] for row in self.rows(
            "SELECT event_type, person_id FROM ecs_events WHERE idempotency_key LIKE ?",
            f"app-bind:bind-{self.SEAT}-assign-7:%")}
        # The structural half: three of the four events are statements about things
        # that EXIST and are the same facts whoever is looking, so they stay on the
        # registry person. Only thread.activated says who is where right now.
        self.assertEqual(registry["entity.registered"], "ceo-default")
        self.assertEqual(registry["thread.created"], "ceo-default")
        self.assertEqual(registry["session.observed"], "ceo-default")
        self.assertEqual(registry["thread.activated"], self.SEAT)
        # The positive probe for that negative: the guard it would have tripped is
        # live, and it raises when an entity really is registered under another
        # person.
        store = EventStore(self.state)
        with self.assertRaisesRegex(Exception, "already registered differently"):
            store.append("entity.registered", entity_id="depot", thread_id="thread-a",
                         person_id=self.SEAT, source_ref="probe:registry",
                         idempotency_key="probe-registry-1", actor_kind="app",
                         actor_id="richos-app-v1",
                         payload={"display_name": "depot", "canonical_root": "elsewhere",
                                  "git_common_dir": "elsewhere", "status": "active"})

    def test_a_work_seat_can_still_REPORT_after_three_ceo_turns(self):
        """The one the fourth review exists for. A read was never the problem."""
        frozen = self.work_seat()
        self.three_ceo_turns()
        # observe appends work_unit.upserted, which IS a CONVERSATIONAL_EVENT and is
        # fenced against the row belonging to the event's own person. Without the
        # seat this compared the work row's revision (1) against his (4).
        self.assertFalse(self.observe(frozen, self.SEAT, "obs-1", "assign-7")["duplicate"])
        self.assertEqual(
            [row["person_id"] for row in self.rows("SELECT person_id FROM ecs_work_units")],
            [self.SEAT])
        # complete-obligation's append is the other half of how an assignment speaks.
        # Both arms, because the broken form is invisible without his three turns.
        store = EventStore(self.state)
        closed = dict(entity_id="depot", thread_id="thread-a", session_id="session-assign-7",
                      active_context_revision=1, actor_kind="authority_adapter",
                      actor_id="richos-provider-v1", source_ref="app-completion:probe",
                      payload={"item_id": "ship-it", "status": "completed",
                               "evidence_ref": "app-completion:probe"})
        with self.assertRaisesRegex(Exception, "expected 1, actual 4"):
            store.append("continuity.item_closed", idempotency_key="probe-close-no-seat",
                         expected_revision=1, **closed)
        self.call("checkpoint", request_id="open-it", checkpoint={"statements": [
            {"verb": "commitment", "fields": {"id": "ship-it", "title": "Ship it"}}]},
            binding=self.ceo)
        item = self.rows("SELECT revision FROM ecs_continuity_items WHERE item_id='ship-it'")
        store.append("continuity.item_closed", idempotency_key="probe-close-seat",
                     person_id=self.SEAT, expected_revision=int(item[0]["revision"]), **closed)
        self.assertEqual(
            self.rows("SELECT status FROM ecs_continuity_items WHERE item_id='ship-it'"),
            [{"status": "completed"}])

    def test_checkpoint_receipt_and_brief_are_refused_on_a_work_seat(self):
        frozen = self.work_seat()
        self.three_ceo_turns()
        statements = [{"verb": "commitment", "fields": {"id": "his-own", "title": "His own"}}]
        # Positive control first, or the refusals below would pass on a store where
        # these three are simply broken.
        self.assertTrue(self.call("checkpoint", request_id="ceo-1", binding=self.ceo,
                                  checkpoint={"statements": statements})["accepted"])
        self.assertIn("His own", self.call("brief", binding=self.ceo)["text"])
        self.call("receipt", binding=self.ceo, request_id="ceo-1",
                  session_id=self.ceo["session_id"], turn_id=self.ceo["turn_id"])
        for command, fields in (("checkpoint", {"request_id": "work-1",
                                                "checkpoint": {"statements": statements}}),
                                ("brief", {}),
                                ("receipt", {"request_id": "work-1",
                                             "session_id": "session-assign-7",
                                             "turn_id": "assign-7"})):
            self.assertIn("belong to the conversation's own seat",
                          self.call(command, ok=False, seat=self.SEAT, binding=frozen, **fields))

    def test_an_orphan_seat_is_released_and_his_own_seat_never_is(self):
        frozen = self.work_seat()
        self.three_ceo_turns()
        self.assertEqual(
            sorted(row["person_id"] for row in
                   self.call("seats", binding=self.ceo)["seats"]),
            ["ceo-default", self.SEAT])
        # His cursor is identified POSITIVELY and refused by name. "Everything that
        # is not his" is exactly the reasoning that deletes it after a crash.
        self.assertIn("never reconciled away",
                      self.call("release-seat", ok=False, binding=self.ceo,
                                person_id="ceo-default", request_id="rel-0",
                                source_ref="reconcile:0"))
        # A lease cannot release seats at all -- its own or anybody's.
        self.assertIn("belongs to the conversation's own seat",
                      self.call("release-seat", ok=False, seat=self.SEAT, binding=frozen,
                                person_id=self.SEAT, request_id="rel-1",
                                source_ref="reconcile:1"))
        released = self.call("release-seat", binding=self.ceo, person_id=self.SEAT,
                             request_id="rel-2", source_ref="reconcile:2")
        self.assertEqual((released["released"], released["turn_id"]), (True, "assign-7"))
        self.assertIn("stale app binding",
                      self.call("inspect", ok=False, seat=self.SEAT, binding=frozen))
        # Positive control: his own cursor is untouched by the release.
        self.assertEqual(self.call("inspect", binding=self.ceo)["turn"], "turn-4")
        # And the release is an EVENT, so a projection rebuild does not walk the
        # seat back out of the journal.
        EventStore(self.state).rebuild_projections()
        self.assertEqual([row["person_id"] for row in
                          self.call("seats", binding=self.ceo)["seats"]], ["ceo-default"])
        # Releasing an absent seat is the reconciliation's own idempotence.
        self.assertFalse(self.call("release-seat", binding=self.ceo, person_id=self.SEAT,
                                   request_id="rel-3", source_ref="reconcile:3")["released"])


class CeoThreadSeatTests(unittest.TestCase):
    """HIS OWN SEAT, ONE PER CONVERSATION THREAD.

    He can open and run any number of conversation threads at once, and each one
    holds its own front desk. ``ecs_active_context`` is a cursor keyed by
    ``person_id``, so N front desks on the one ``ceo-default`` row means thread B's
    bind upserts thread A's cursor while A's turn is still open: A's next
    checkpoint, brief or inspect raises ``ScopeError`` "stale app binding", which
    ``with_fresh_active_fence`` never retries because its own docstring says a
    ``ScopeError`` "is not a race" -- so the turn dead-letters rather than losing a
    race it could win.

    EVERY TEST HERE RUNS BOTH THREADS WITH A TURN OPEN AND INTERLEAVES THEM, which
    is the whole point: a walk that finishes thread A before thread B starts passes
    on the broken design too.
    """

    THREADS = ("thread-a", "thread-b")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs ceo seat ")
        self.root = Path(self.temp.name)
        self.state = self.root / "state"
        # The registry exists before any thread seat does: the split bind puts
        # entity.registered, thread.created and session.observed on the registry
        # person and only thread.activated on the seat.
        self.legacy = self.bind("thread-a", "session-legacy", "turn-0")

    def tearDown(self):
        self.temp.cleanup()

    def seat(self, thread):
        """Derived, never invented -- the app derives the same string."""
        return "ceo-thread:" + thread

    def call(self, command, ok=True, seat=None, **fields):
        request = {"protocol": 1, "command": command, **fields}
        if seat is not None:
            request["seat"] = seat
        result = subprocess.run(
            [sys.executable, str(COMPONENT / "bin/ecs"), "--state-root", str(self.state)],
            input=json.dumps(request), text=True, capture_output=True,
            env={**os.environ, "ECS_HOME": str(self.root / "do-not-adopt")})
        output = json.loads(result.stdout)
        self.assertEqual(result.returncode, 0 if ok else 2, result.stdout + result.stderr)
        self.assertEqual(output["ok"], ok, result.stdout)
        return output["result"] if ok else output["error"]["message"]

    def bind(self, thread, session, turn, revision=None, seat=None, audience="ceo", ok=True):
        return self.call(
            "bind", ok=ok, seat=seat, request_id=f"bind-{seat or 'legacy'}-{thread}-{turn}",
            source_ref=f"ledger:{thread}:{turn}", expected_revision=revision,
            scope={"entity_id": "depot", "thread_id": thread, "session_id": session,
                   "turn_id": turn, "audience": audience})

    def open_turn(self, thread, turn, revision=None):
        """That thread's front desk binds ITS OWN seat for its current turn."""
        return self.bind(thread, f"session-{thread}", turn, revision, seat=self.seat(thread))["binding"]

    def checkpoint(self, binding, thread, identity, title, ok=True):
        record = f"{thread}-{identity}"
        return self.call("checkpoint", ok=ok, seat=self.seat(thread), binding=binding,
                         request_id=f"chk-{record}",
                         checkpoint={"statements": [{"verb": "commitment",
                                                     "fields": {"id": record, "title": title}}]})

    def rows(self, sql, *args):
        store = EventStore(self.state)
        conn = store.connect()
        try:
            return [dict(row) for row in conn.execute(sql, args).fetchall()]
        finally:
            conn.close()

    def test_two_threads_with_open_turns_both_checkpoint(self):
        """The test the whole change exists for, and its before-state beside it."""
        first = self.open_turn("thread-a", "a-turn-1")
        # Thread B's front desk binds WHILE thread A's turn is open. This is the
        # exact moment that used to overwrite A's cursor.
        second = self.open_turn("thread-b", "b-turn-1")
        self.assertEqual(sorted(row["person_id"] for row in
                                self.call("seats", binding=first, seat=self.seat("thread-a"))["seats"]),
                         ["ceo-default", "ceo-thread:thread-a", "ceo-thread:thread-b"])
        # Both checkpoint, in the order that breaks: the thread that bound FIRST
        # writes after the thread that bound second.
        self.assertTrue(self.checkpoint(first, "thread-a", 1, "Land the lander")["accepted"])
        self.assertTrue(self.checkpoint(second, "thread-b", 1, "Answer the letter")["accepted"])
        # Interleave a second turn each, so no ordering accident can carry this.
        first = self.open_turn("thread-a", "a-turn-2", 1)
        second = self.open_turn("thread-b", "b-turn-2", 1)
        self.assertTrue(self.checkpoint(first, "thread-a", 2, "Read the review")["accepted"])
        self.assertTrue(self.checkpoint(second, "thread-b", 2, "Call the notary")["accepted"])
        # Each thread's brief is its own thread's records, and the receipt for a
        # checkpoint written on that seat reads back on it.
        brief_a = self.call("brief", seat=self.seat("thread-a"), binding=first)["text"]
        brief_b = self.call("brief", seat=self.seat("thread-b"), binding=second)["text"]
        self.assertIn("Read the review", brief_a)
        self.assertNotIn("Call the notary", brief_a)
        self.assertIn("Call the notary", brief_b)
        self.assertNotIn("Read the review", brief_b)
        self.assertTrue(self.call("receipt", seat=self.seat("thread-a"), binding=first,
                                  request_id="chk-thread-a-2", session_id="session-thread-a",
                                  turn_id="a-turn-2")["duplicate"])
        # THE BEFORE-STATE, driven through the single cursor the front desk used to
        # bind: the same interleaving, and thread A's checkpoint is refused. Not a
        # retryable race -- with_fresh_active_fence retries RevisionConflict only.
        legacy_a = self.bind("thread-a", "session-legacy", "legacy-a-1", 1)["binding"]
        self.bind("thread-b", "session-legacy-b", "legacy-b-1", 2)
        self.assertIn("stale app binding",
                      self.call("checkpoint", ok=False, binding=legacy_a, request_id="legacy-chk",
                                checkpoint={"statements": [{"verb": "commitment",
                                    "fields": {"id": "lost", "title": "The turn that dead-lettered"}}]}))

    def test_collapsing_his_seats_back_to_one_row_turns_it_red(self):
        """The mutation. Both halves of the seat, mutated in place, one at a time.

        A negative control that only re-runs the OLD design proves the old design
        was broken; it cannot prove the new code is what fixes it. These two patch
        the live code instead: derive every thread's seat to one name, and identify
        him by the PERSON_ID literal the way the adapter used to.
        """
        import app as adapter
        import ecs_core
        first = self.open_turn("thread-a", "a-turn-1")
        second = self.open_turn("thread-b", "b-turn-1")
        # Mutation one: one seat for every thread. The reducer refuses the bind,
        # because a CEO seat that does not re-derive from the thread it activates
        # is exactly the collapse this prevents.
        with patch.object(adapter, "ceo_seat", lambda thread: "ceo-thread:collapsed"):
            with self.assertRaises(Exception) as refused:
                adapter.execute(self.state, {"protocol": 1, "command": "bind",
                    "seat": "ceo-thread:collapsed", "request_id": "collapse-1",
                    "source_ref": "ledger:thread-b:collapse", "expected_revision": None,
                    "scope": {"entity_id": "depot", "thread_id": "thread-b",
                              "session_id": "session-thread-b", "turn_id": "b-turn-2",
                              "audience": "ceo"}})
        self.assertIn("derived from the thread", str(refused.exception))
        # Mutation two: identify him by the literal, as the adapter did before.
        # Both threads' checkpoints go red -- neither seat is "ceo-default".
        with patch.object(adapter, "is_ceo_row", lambda row: row["person_id"] == ecs_core.PERSON_ID):
            for binding, thread in ((first, "thread-a"), (second, "thread-b")):
                with self.assertRaises(Exception) as refused:
                    adapter.execute(self.state, {"protocol": 1, "command": "checkpoint",
                        "seat": self.seat(thread), "binding": binding, "request_id": f"lit-{thread}",
                        "checkpoint": {"statements": [{"verb": "commitment",
                            "fields": {"id": "literal", "title": "Refused by the literal"}}]}})
                self.assertIn("belong to the conversation's own seat", str(refused.exception))
        # Positive control: unmutated, the same two calls are accepted.
        self.assertTrue(self.checkpoint(first, "thread-a", "live", "Accepted unmutated")["accepted"])
        self.assertTrue(self.checkpoint(second, "thread-b", "live", "Accepted unmutated")["accepted"])

    def test_a_projection_rebuild_keeps_every_thread_seat(self):
        self.open_turn("thread-a", "a-turn-1")
        self.open_turn("thread-b", "b-turn-1")
        first = self.open_turn("thread-a", "a-turn-2", 1)
        before = self.rows("SELECT person_id, thread_id, turn_id, audience, revision "
                           "FROM ecs_active_context ORDER BY person_id")
        EventStore(self.state).rebuild_projections()
        self.assertEqual(self.rows("SELECT person_id, thread_id, turn_id, audience, revision "
                                   "FROM ecs_active_context ORDER BY person_id"), before)
        self.assertEqual([row["person_id"] for row in before],
                         ["ceo-default", "ceo-thread:thread-a", "ceo-thread:thread-b"])
        # And the rebuilt rows are still usable, not just present.
        self.assertEqual(self.call("inspect", seat=self.seat("thread-a"), binding=first)["turn"],
                         "a-turn-2")

    def test_a_work_seat_is_still_refused_and_cannot_dress_as_his(self):
        """The positive control for the positive identification.

        "Is this the CEO" is answered by re-deriving his seat from the row's own
        thread AND requiring the ceo audience. If either half were dropped, a
        background lease could reach his continuity by naming itself well.
        """
        first = self.open_turn("thread-a", "a-turn-1")
        self.open_turn("thread-b", "b-turn-1")
        work = self.bind("thread-a", "session-assign-7", "assign-7", None,
                         seat="work-seat:assign-7", audience="worker")["binding"]
        for command, fields in (("checkpoint", {"request_id": "work-1", "checkpoint": {"statements": [
                                    {"verb": "commitment", "fields": {"id": "no", "title": "No"}}]}}),
                                ("brief", {}),
                                ("seats", {}),
                                ("release-seat", {"person_id": "ceo-thread:thread-b",
                                                  "request_id": "work-rel", "source_ref": "reconcile:work",
                                                  "expected_revision": 1})):
            self.assertIn("conversation's own seat",
                          self.call(command, ok=False, seat="work-seat:assign-7", binding=work, **fields))
        # A work seat cannot take the ceo audience, which is what the visibility
        # narrowing in inspect_records rests on.
        self.assertIn("ceo audience belongs to the conversation's own seats",
                      self.bind("thread-a", "session-assign-8", "assign-8", None,
                                seat="work-seat:assign-8", audience="ceo", ok=False))
        # Nor can anything bind a CEO-shaped seat for a thread it is not in, or
        # with any audience but ceo.
        self.assertIn("derived from the thread it binds",
                      self.bind("thread-b", "session-forged", "forged-1", None,
                                seat=self.seat("thread-a"), ok=False))
        self.assertIn("derived from the thread it binds",
                      self.bind("thread-b", "session-forged", "forged-2", None,
                                seat=self.seat("thread-b"), audience="worker", ok=False))
        # Positive control: his own two seats are unaffected by all of that.
        self.assertTrue(self.checkpoint(first, "thread-a", 1, "Still his")["accepted"])

    def test_a_dead_threads_seat_is_released_and_a_live_ones_is_not(self):
        """Reconciliation, walked from a live thread against a thread that ended.

        Whether a conversation thread still exists is the APP's knowledge -- the
        store holds no such fact and inventing one would be a guess. What the store
        can prove is movement, so the release names the revision the seat was
        enumerated at.
        """
        live = self.open_turn("thread-a", "a-turn-1")
        self.open_turn("thread-b", "b-turn-1")
        enumerated = {row["person_id"]: row for row in
                      self.call("seats", seat=self.seat("thread-a"), binding=live)["seats"]}
        self.assertEqual(sorted(enumerated),
                         ["ceo-default", "ceo-thread:thread-a", "ceo-thread:thread-b"])
        # His legacy cursor is refused by name, and a seat cannot release itself --
        # a front desk reconciling its own thread away is the one release that can
        # never be right.
        self.assertIn("never reconciled away",
                      self.call("release-seat", ok=False, seat=self.seat("thread-a"), binding=live,
                                person_id="ceo-default", request_id="rel-0", source_ref="reconcile:0",
                                expected_revision=1))
        self.assertIn("cannot reconcile itself away",
                      self.call("release-seat", ok=False, seat=self.seat("thread-a"), binding=live,
                                person_id=self.seat("thread-a"), request_id="rel-1",
                                source_ref="reconcile:1", expected_revision=1))
        # A LIVE thread's seat: thread B speaks again after the enumeration, so the
        # revision the reconciler saw is stale and the release is refused instead
        # of racing the front desk that is using it.
        self.open_turn("thread-b", "b-turn-2", 1)
        self.assertIn("moved since it was enumerated",
                      self.call("release-seat", ok=False, seat=self.seat("thread-a"), binding=live,
                                person_id=self.seat("thread-b"), request_id="rel-2",
                                source_ref="reconcile:2",
                                expected_revision=int(enumerated["ceo-thread:thread-b"]["revision"])))
        # Naming no revision at all is refused rather than assumed: a release with
        # no staleness proof is the release of a live thread's seat waiting to
        # happen.
        self.assertIn("requires the revision it was enumerated at",
                      self.call("release-seat", ok=False, seat=self.seat("thread-a"), binding=live,
                                person_id=self.seat("thread-b"), request_id="rel-3",
                                source_ref="reconcile:3"))
        # Thread B is now gone in the app -- its seat is an orphan for the same
        # reason an abandoned assignment's seat is, and is released on the revision
        # it currently holds.
        current = {row["person_id"]: row for row in
                   self.call("seats", seat=self.seat("thread-a"), binding=live)["seats"]}
        released = self.call("release-seat", seat=self.seat("thread-a"), binding=live,
                             person_id=self.seat("thread-b"), request_id="rel-4",
                             source_ref="reconcile:4", reason="thread closed",
                             expected_revision=int(current["ceo-thread:thread-b"]["revision"]))
        self.assertEqual((released["released"], released["turn_id"]), (True, "b-turn-2"))
        self.assertEqual(sorted(row["person_id"] for row in
                                self.call("seats", seat=self.seat("thread-a"), binding=live)["seats"]),
                         ["ceo-default", "ceo-thread:thread-a"])
        # The release is an EVENT, so a rebuild does not walk the seat back out of
        # the journal -- and the live thread's seat is still there afterwards.
        EventStore(self.state).rebuild_projections()
        self.assertEqual(sorted(row["person_id"] for row in
                                self.call("seats", seat=self.seat("thread-a"), binding=live)["seats"]),
                         ["ceo-default", "ceo-thread:thread-a"])
        self.assertEqual(self.call("inspect", seat=self.seat("thread-a"), binding=live)["turn"],
                         "a-turn-1")
        # Releasing an absent seat is the reconciliation's own idempotence.
        self.assertFalse(self.call("release-seat", seat=self.seat("thread-a"), binding=live,
                                   person_id=self.seat("thread-b"), request_id="rel-5",
                                   source_ref="reconcile:5", expected_revision=1)["released"])


WITHDRAWN = "cancelled"  # dialect-exempt: the ECS store's own terminal status value (ecs_core.py)


class OperatorCompleteTests(unittest.TestCase):
    """`operator-complete`: how the app closes the obligation of an assignment his
    OWN team carried (richos-hq operator back-end spec r1 (c), r3 (c)). The lead
    reports through `richos_operator.report`; the host then asks this verb, which
    re-verifies every piece of evidence itself and accepts only
    `git:<repo>:<branch>:<sha>` (the commit is in that branch) or
    `answer:<sha256>` (the digest of the answer text sent with it). A model
    checkpoint still cannot certify completion; this verb is the host's."""

    call = WorkSeatTests.call
    bind = WorkSeatTests.bind
    rows = WorkSeatTests.rows

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs operator ")
        self.root = Path(self.temp.name)
        self.state = self.root / "state"
        self.repo = self.root / "repo"
        self.git("init", "-q", "-b", "main", str(self.repo), cwd=self.root)
        (self.repo / "f.txt").write_text("base\n")
        self.git("add", "f.txt")
        self.git("commit", "-q", "-m", "base")
        self.landed = self.git("rev-parse", "HEAD").strip()
        self.git("switch", "-q", "-c", "side")
        (self.repo / "f.txt").write_text("side\n")
        self.git("commit", "-q", "-am", "side")
        self.unlanded = self.git("rev-parse", "HEAD").strip()
        self.git("switch", "-q", "main")
        self.ceo = self.bind("depot", "thread-a", "session-conv", "turn-1", None)
        self.assertTrue(self.call("checkpoint", request_id="open-1", binding=self.ceo, checkpoint={
            "statements": [{"verb": "commitment", "fields": {"id": "ship-it", "title": "Ship it"}},
                           {"verb": "commitment", "fields": {"id": "answer-it", "title": "Answer it"}}]})["accepted"])

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args, cwd=None):
        env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
               "GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
               "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid"}
        result = subprocess.run(["git", *args], cwd=cwd or self.repo, env=env, text=True,
                                capture_output=True, check=True)
        return result.stdout

    def complete(self, ok=True, obligation="ship-it", **fields):
        fields.setdefault("source_ref", "operator-report:1")
        return self.call("operator-complete", ok=ok, binding=self.ceo, obligation_id=obligation, **fields)

    def status(self, item):
        return self.rows("SELECT status, evidence_ref FROM ecs_continuity_items WHERE item_id=?", item)[0]

    def still_open(self, item):
        self.assertNotIn(self.status(item)["status"], ("completed", WITHDRAWN))

    def test_O1_a_verified_land_closes_the_obligation_and_a_replay_is_a_duplicate(self):
        evidence = [f"git:{self.repo}:main:{self.landed}"]
        first = self.complete(evidence=evidence)
        self.assertEqual((first["obligation_closed"], first["status"], first["duplicate"]),
                         (True, "completed", False))
        self.assertEqual(self.status("ship-it")["status"], "completed")
        self.assertIn(self.landed, self.status("ship-it")["evidence_ref"])
        again = self.complete(evidence=evidence)
        self.assertTrue(again["duplicate"])
        self.assertEqual(len(self.rows("SELECT sequence FROM ecs_events WHERE event_type='continuity.item_closed'")), 1)

    def test_O2_a_commit_not_in_the_branch_is_refused_and_the_obligation_stays_open(self):
        message = self.complete(ok=False, evidence=[f"git:{self.repo}:main:{self.unlanded}"])
        self.assertIn("could not be confirmed", message)
        self.still_open("ship-it")
        self.assertIn("could not be confirmed",
                      self.complete(ok=False, evidence=[f"git:{self.root}/absent:main:{self.landed}"]))

    def test_O3_an_answer_is_its_text_digest_and_nothing_else(self):
        text = "Nothing to land: the answer is in the notice."
        digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
        self.assertIn("SHA-256", self.complete(ok=False, obligation="answer-it",
                                               evidence=[f"answer:{digest}"], answer_text=text + "!"))
        self.assertIn("SHA-256", self.complete(ok=False, obligation="answer-it", evidence=[f"answer:{digest}"]))
        done = self.complete(obligation="answer-it", evidence=[f"answer:{digest}"], answer_text=text)
        self.assertEqual((done["status"], self.status("answer-it")["status"]), ("completed", "completed"))

    def test_O4_every_other_shape_of_evidence_is_refused(self):
        for bad in ([self.landed], [f"review:x:{self.landed}:passed"], [f"git:relative/repo:main:{self.landed}"],
                    [f"git:{self.repo}:main:{self.landed[:12]}"], [f"git:{self.repo}:-main:{self.landed}"],
                    [], "git:x", [f"answer:{'0' * 64}", f"answer:{'1' * 64}"]):
            self.complete(ok=False, evidence=bad, answer_text="x")
        self.still_open("ship-it")

    def test_O5_a_work_seat_cannot_close_an_obligation_this_way(self):
        seat = "work-seat:assign-7"
        work = self.bind("depot", "thread-a", "session-w", "ship-it", None, seat=seat, audience="worker")
        message = self.call("operator-complete", ok=False, seat=seat, binding=work, obligation_id="ship-it",
                            source_ref="x", evidence=[f"git:{self.repo}:main:{self.landed}"])
        self.assertIn("conversation's own seat", message)
        self.still_open("ship-it")

    def test_O6_a_closed_obligation_is_not_closed_again_with_other_evidence(self):
        self.complete(evidence=[f"git:{self.repo}:main:{self.landed}"])
        text = "later"
        self.assertIn("open", self.complete(ok=False, evidence=[
            f"answer:{hashlib.sha256(text.encode()).hexdigest()}"], answer_text=text))

    def test_O7_a_failed_assignment_is_closed_with_its_answer_never_with_a_land(self):
        text = "It failed: the fixture repository refused the push."
        digest = hashlib.sha256(text.encode()).hexdigest()
        self.assertIn("cannot close a failed", self.complete(ok=False, status=WITHDRAWN,
                                                             evidence=[f"git:{self.repo}:main:{self.landed}"]))
        done = self.complete(status=WITHDRAWN, evidence=[f"answer:{digest}"], answer_text=text)
        self.assertEqual((done["status"], self.status("ship-it")["status"]), (WITHDRAWN, WITHDRAWN))
        self.complete(ok=False, obligation="answer-it", status="completed-ish",
                      evidence=[f"answer:{digest}"], answer_text=text)

    def test_O8_the_host_verb_is_announced_and_never_a_model_tool(self):
        self.assertIn("operator-complete", self.call("hello")["commands"])
        scope = self.root / "scope.json"
        scope.write_text(json.dumps({"version": 1, "actions_allowed": True, "binding": self.ceo,
                                     "state_root": str(self.state)}))
        with self.assertRaises(ValueError):
            tool_call(str(scope), "operator-complete", {"obligation_id": "ship-it", "evidence": []})


class AnswerCompleteTests(unittest.TestCase):
    """`answer-complete`: how the app closes the obligation of an assignment its own
    back end handled ITSELF -- it ran a command he asked for, or told him why it could
    not begin -- with no helper ever prepared (richos-hq
    docs/verification/2026-09-27-background-command-finish, "Not done").

    `complete` needs at least one worker (mega-lander/app.py `complete`), so such an
    assignment could never close and the back end told him so. This verb is the
    host's, never a model tool, closes on the digest of the answer he was given, and
    refuses any assignment a helper was ever recorded on: a code change still closes
    only through `complete`, with its workers."""

    call = WorkSeatTests.call
    bind = WorkSeatTests.bind
    rows = WorkSeatTests.rows
    observe = WorkSeatTests.observe

    ANSWER = "It ran and finished. It printed one line: 82208dc init."

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs answer ")
        self.root = Path(self.temp.name)
        self.state = self.root / "state"
        self.ceo = self.bind("depot", "thread-a", "session-conv", "turn-1", None)
        self.assertTrue(self.call("checkpoint", request_id="open-1", binding=self.ceo, checkpoint={
            "statements": [{"verb": "commitment", "fields": {"id": "work-ran-it", "title": "Run it"}},
                           {"verb": "commitment", "fields": {"id": "work-coded-it", "title": "Code it"}}]})["accepted"])

    def tearDown(self):
        self.temp.cleanup()

    def close(self, ok=True, obligation="work-ran-it", **fields):
        fields.setdefault("source_ref", "ledger:thread-a:turn-1")
        fields.setdefault("answer_text", self.ANSWER)
        return self.call("answer-complete", ok=ok, binding=self.ceo, obligation_id=obligation, **fields)

    def item(self, item):
        return self.rows("SELECT status, evidence_ref FROM ecs_continuity_items WHERE item_id=?", item)[0]

    def test_A1_an_assignment_no_helper_touched_closes_on_its_answer_and_a_replay_is_a_duplicate(self):
        digest = hashlib.sha256(self.ANSWER.encode("utf-8")).hexdigest()
        first = self.close()
        self.assertEqual((first["obligation_closed"], first["status"], first["duplicate"]),
                         (True, "completed", False))
        self.assertEqual(self.item("work-ran-it"), {"status": "completed", "evidence_ref": "answer:" + digest})
        closed = self.rows("SELECT actor_id FROM ecs_events WHERE event_type='continuity.item_closed'")
        self.assertEqual(closed, [{"actor_id": "richos-app-v1"}])
        self.assertTrue(self.close()["duplicate"])
        self.assertEqual(len(self.rows("SELECT sequence FROM ecs_events WHERE event_type='continuity.item_closed'")), 1)

    def test_A2_an_assignment_a_helper_was_recorded_on_never_closes_on_an_answer(self):
        seat = "work-seat:work-coded-it"
        work = self.bind("depot", "thread-a", "session-w", "work-coded-it", None, seat=seat, audience="worker")
        self.observe(work, seat, "prep-1", "receipt-1")
        self.assertIn("helper", self.close(ok=False, obligation="work-coded-it"))
        self.assertEqual(self.item("work-coded-it")["status"], "active")
        # Another assignment's helper is not this one's.
        self.assertEqual(self.close()["status"], "completed")

    def test_A3_a_work_seat_cannot_close_an_assignment_this_way(self):
        seat = "work-seat:work-ran-it"
        work = self.bind("depot", "thread-a", "session-w", "work-ran-it", None, seat=seat, audience="worker")
        message = self.call("answer-complete", ok=False, seat=seat, binding=work, obligation_id="work-ran-it",
                            source_ref="x", answer_text=self.ANSWER)
        self.assertIn("conversation's own seat", message)
        self.assertEqual(self.item("work-ran-it")["status"], "active")

    def test_A4_it_needs_the_answer_and_an_open_assignment(self):
        for bad in ("", "   ", None, 7, "x" * (262144 + 1)):
            self.close(ok=False, answer_text=bad)
        self.assertEqual(self.item("work-ran-it")["status"], "active")
        self.close()
        self.assertIn("open", self.close(ok=False, answer_text="a different answer"))

    def test_A5_the_host_verb_is_announced_and_never_a_model_tool(self):
        self.assertIn("answer-complete", self.call("hello")["commands"])
        scope = self.root / "scope.json"
        scope.write_text(json.dumps({"version": 1, "actions_allowed": True, "binding": self.ceo,
                                     "state_root": str(self.state)}))
        with self.assertRaises(ValueError):
            tool_call(str(scope), "answer-complete", {"obligation_id": "work-ran-it", "answer_text": "x"})


if __name__ == "__main__":
    unittest.main()

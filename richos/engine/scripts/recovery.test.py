#!/usr/bin/env python3
"""Recovery state fixtures use temporary repositories and review records only."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import recovery as R


class Recovery(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="recovery-test-")
        self.base = Path(self.temp.name)
        self.env = patch.dict(os.environ, {"SECOND_REVIEW_STATE_DIR": str(self.base / "state")})
        self.env.start()
        self.repo = self.base / "repo"
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        self.work = "teammate:original"
        self.aliases = {self.work, "teammate:replacement"}
        self.ledger = Path(R.F.review_ledger_default())
        self.ledger.parent.mkdir(parents=True)
        for n in range(3):
            self.review(str(n) * 40, "initial-" + str(n))
        self.record = self.reconcile()

    def tearDown(self):
        self.env.stop()
        self.temp.cleanup()

    def review(self, tip, rid, verdict="changes-requested", work=None):
        row = dict(id=rid, tip=tip, verdict=verdict, trigger="handover", repo=str(self.repo),
                   work=work or self.work, record=str(self.base / "missing-original"))
        with self.ledger.open("a") as out:
            out.write(json.dumps(row) + "\n")

    def reconcile(self, now=None):
        return R.reconcile(str(self.repo), self.work, self.aliases, "owner", "acceptance", now=now, mode="enforce")

    def plan(self, approach="remove duplicated state"):
        record = R.read(self.record["id"])
        proposal = dict(job=record["id"], acceptance=record["acceptance"], frontier=record["evidence_cursor"],
                        diagnosis="two components own the same transition", matrix=[dict(condition="one owner", evidence="source")],
                        proof=[dict(condition="one owner", check="transition fixture")], approach=approach,
                        brief="Implement " + approach, control="continue")
        plan = R.propose(record["id"], proposal, "axel-"+approach)
        R.accept(record["id"], plan["id"], dict(plan=plan["id"], accepted=True, evidence="matrix independently checked", reviewer="independent"))
        return R.read(record["id"]), plan

    def test_three_failed_sources_schedule_without_a_watcher(self):
        self.assertEqual(len(self.record["failed"]), 3)
        self.assertEqual(self.record["status"], "investigating")
        self.review("2"*40, "duplicate-pass", "passed")
        self.assertEqual(len(self.reconcile()["failed"]), 3)
        deadline = self.record["deadline"]
        self.assertEqual(self.reconcile()["deadline"], deadline)

    def test_different_third_recovery_then_persistent_escalation(self):
        for n in range(3):
            approach = "approach " + str(n)
            record, plan = self.plan(approach)
            attempt = R.admission(record, "worker-" + str(n), plan["brief"], plan["id"])
            self.assertEqual(attempt["number"], n+1)
            tip = str(n+3)*40
            R.handover(record["id"], "worker-"+str(n), tip)
            self.review(tip, "recovery-"+str(n), work="teammate:replacement")
            record = self.reconcile()
            self.assertEqual(record["status"], "awaiting_alex" if n==2 else "investigating")
        with self.assertRaises(R.Unavailable):
            R.admission(record, "fourth", "new brief", plan["id"])
        self.assertEqual(self.reconcile()["status"], "awaiting_alex")
        self.assertIsNotNone(record["escalation"])

    def test_infrastructure_replacement_keeps_its_attempt(self):
        record, plan = self.plan()
        first = R.admission(record, "first", plan["brief"], plan["id"])
        with self.assertRaises(R.Unavailable):
            R.admission(record, "renamed", plan["brief"], plan["id"])
        with self.assertRaises(R.Unavailable):
            R.admission(record, "replacement", plan["brief"], plan["id"], replaces="first")
        replacement = R.admission(record, "replacement", plan["brief"], plan["id"], replaces="first", safe=lambda w: w=="first")
        self.assertEqual(first["number"], replacement["number"])
        self.assertEqual(len(R.read(record["id"])["attempts"]), 1)

    def test_observe_mode_never_refuses_or_reserves(self):
        record = R.reconcile(str(self.repo), self.work, self.aliases, "owner", "acceptance", mode="observe")
        self.assertIsNone(R.admission(record, "worker", "ordinary correction"))
        self.assertEqual(R.read(record["id"])["attempts"], [])

    def test_broken_state_and_missing_proof_fail_closed(self):
        record = R.read(self.record["id"])
        self.assertTrue(R.plan_problems({}, record))
        (R.root() / (record["id"]+".json")).write_text("not JSON")
        with self.assertRaises(ValueError):
            self.reconcile()

    def test_late_control_cannot_stop_a_successor_without_its_source(self):
        value = dict(job=self.record["id"], acceptance="acceptance", frontier="old", action="stop",
                     workers={"first":"a"*40}, reason="unsupported approach", evidence="three failed transitions")
        with self.assertRaises(R.Unavailable):
            R.control(self.record["id"], value, "axel")
        value["frontier"] = self.record["evidence_cursor"]
        result = R.control(self.record["id"], value, "axel")
        record, plan = self.plan()
        with self.assertRaises(R.Unavailable):
            R.admission(record, "replacement", plan["brief"], plan["id"], safe=lambda w: False)
        self.assertEqual(result["workers"], {"first":"a"*40})

    def test_deadline_exhaustion_is_not_behavioral_failure(self):
        expired = self.reconcile(now=self.record["deadline"]+1)
        self.assertEqual(expired["status"], "unavailable")
        self.assertEqual(expired["recovery_failures"], 0)
        with self.assertRaises(R.Unavailable):
            R.admission(expired, "replacement", "anything")


if __name__ == "__main__":
    unittest.main()

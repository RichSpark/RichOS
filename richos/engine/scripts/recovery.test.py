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
                        brief="Implement " + approach, control="continue", disproof="One owner already exists in the trace", retained_evidence=["existing passing transition proof"])
        plan = R.propose(record["id"], proposal, "axel-"+approach)
        R.accept(record["id"], plan["id"], dict(plan=plan["id"], accepted=True, evidence="matrix independently checked", reviewer="independent", materially_different=True, difference_evidence="different state owner, checked against earlier attempts"))
        return R.read(record["id"]), plan

    def test_resolved_premise_reopens_when_the_recovery_threshold_is_crossed(self):
        work='teammate:premise-first'
        def add(rid,tip,verdict,findings=(),earlier=()):
            folder=self.base/rid
            folder.mkdir()
            (folder/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=list(findings),earlier_findings=list(earlier)),earlier_findings_in=[dict(id=e['id']) for e in earlier])))
            with self.ledger.open('a') as out:
                out.write(json.dumps(dict(id=rid,repo=str(self.repo),work=work,tip=tip,trigger='handover',verdict=verdict,record=str(folder)))+'\n')
        finding=dict(blocks=True,priority=1,title='premise',files=[],evidence='source',fixture='')
        add('p1','a'*40,'changes-requested',[finding])
        add('p2','b'*40,'changes-requested',earlier=[dict(id='p1#1',status='still-open',blocks=True,note='unresolved')])
        def current():return R.reconcile(str(self.repo),work,{work},'owner','acceptance',mode='enforce',premises='carried')
        first=current()
        self.assertFalse(first['axel_required'])
        add('p3','c'*40,'passed',earlier=[dict(id='p1#1',status='withdrawn',blocks=False,note='original authority checked')])
        self.assertEqual(current()['status'],'resolved_findings')
        add('p4','d'*40,'changes-requested',[finding])
        reopened=current()
        self.assertTrue(reopened['axel_required'])
        self.assertEqual(reopened['status'],'investigating')
        self.assertTrue(R.landing_gap(str(self.repo),work))
        deadline=reopened['deadline']
        add('p5','e'*40,'changes-requested',[finding])
        self.assertEqual(current()['deadline'],deadline)
        self.assertEqual(current()['status'],'investigating')

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
                     workers={"first":"a"*40+":"+"b"*64}, reason="unsupported approach", evidence="three failed transitions")
        with self.assertRaises(R.Unavailable):
            R.control(self.record["id"], value, "axel")
        value["frontier"] = self.record["evidence_cursor"]
        result = R.control(self.record["id"], value, "axel")
        record, plan = self.plan()
        with self.assertRaises(R.Unavailable):
            R.admission(record, "replacement", plan["brief"], plan["id"], safe=lambda w: False)
        self.assertEqual(result["workers"], {"first":"a"*40+":"+"b"*64})

    def test_deadline_exhaustion_is_not_behavioral_failure(self):
        expired = self.reconcile(now=self.record["deadline"]+1)
        self.assertEqual(expired["status"], "unavailable")
        self.assertEqual(expired["recovery_failures"], 0)
        with self.assertRaises(R.Unavailable):
            R.admission(expired, "replacement", "anything")

    def test_third_plan_needs_independent_material_difference(self):
        for n in range(2):
            record, plan = self.plan("strategy "+str(n))
            R.admission(record, "writer"+str(n), plan["brief"], plan["id"])
            R.handover(record["id"], "writer"+str(n), str(n+3)*40)
            self.review(str(n+3)*40, "failed-recovery"+str(n))
            self.reconcile()
        record = R.read(self.record["id"])
        proposal = dict(job=record["id"], acceptance=record["acceptance"], frontier=record["evidence_cursor"],
            diagnosis="remove competing state owners", disproof="trace shows ownership is already singular",
            retained_evidence=["passing compatibility proofs"], matrix=[dict(condition="one transition", evidence="trace")],
            proof=[dict(condition="one transition", check="race fixture")], approach="replace the ownership mechanism",
            brief="Replace the competing owner", control="continue")
        plan = R.propose(record["id"], proposal, "new-axel")
        with self.assertRaises(R.Unavailable):
            R.accept(record["id"], plan["id"], dict(plan=plan["id"], accepted=True,
                evidence="a different label", reviewer="independent"))
        self.assertFalse(R.read(record["id"])["plan"]["accepted"])

    def test_current_pass_requires_matrix_and_actual_control_completion(self):
        record, plan = self.plan()
        tip = "d"*40
        R.handover(record["id"], "finished-writer", tip)
        row = dict(id="independent-pass", tip=tip, repo=str(self.repo), trigger="handover", verdict="passed",
                   work=self.work, recovery_plan=plan["id"], recovery_coverage={"one owner":"trace receipt"})
        self.review(tip, row["id"], "passed")
        with R.locked(record["id"]):
            fresh = R.read(record["id"])
            fresh["controls"].append(dict(action="stop", workers={"old":"source"}, observations={
                "old":dict(quiescent=True, cancellation_status="requested")}))
            R.save(fresh, "fixture-native-cancellation-pending")
        self.assertFalse(R.complete(record["id"], row))
        with R.locked(record["id"]):
            fresh = R.read(record["id"])
            fresh["controls"][0]["observations"]["old"]["cancellation_status"]="acknowledged"
            R.save(fresh, "fixture-native-cancellation-observed")
        self.assertTrue(R.complete(record["id"], row))

    def test_explicit_dispute_is_exact_and_cannot_resolve_the_finding(self):
        folder = self.base/"original-verdict"
        folder.mkdir()
        row = dict(id="original-finding", tip="e"*40, repo=str(self.repo), work=self.work,
                   verdict="changes-requested", trigger="handover", record=str(folder))
        finding = dict(priority=2, blocks=True, title="applicability premise", files=[], evidence="original words", fixture="")
        (folder/"verdict.json").write_text(json.dumps(dict(row=row,answer=dict(findings=[finding],earlier_findings=[]))))
        R.F.append_jsonl(str(self.ledger), row)
        cli = Path(__file__).parent/"lib/recovery_admin.py"
        argv = [sys.executable,"-B",str(cli),"dispute","--repo",str(self.repo),"--work",self.work,
                "--tip",row["tip"],"--finding",row["id"]+"#1"]
        for _ in range(2):
            result = subprocess.run(argv, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(R.pending_routes(str(self.repo),self.aliases)),1)
        effective,_ = R.F.review_of(R.rows(self.ledger),row["tip"],str(self.repo))
        self.assertEqual(effective["verdict"],"changes-requested")
        invalid = subprocess.run(argv[:-1]+["invented#1"], capture_output=True, text=True)
        self.assertEqual(invalid.returncode,2)


if __name__ == "__main__":
    unittest.main()

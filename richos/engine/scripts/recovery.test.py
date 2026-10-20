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
        folder=self.base/rid;folder.mkdir(exist_ok=True)
        (folder/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[dict(title='failed acceptance',blocks=True,priority=1)] if verdict=='changes-requested' else []))))
        row = dict(id=rid, tip=tip, verdict=verdict, trigger="handover", repo=str(self.repo),
                   work=work or self.work, record=str(folder))
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

    def test_fourth_named_fix_triggers_with_no_failed_reviews(self):
        self.ledger.write_text('')
        for name in ('zach-opus-hostshim1', 'zach-opus-hostshim2', 'zach-opus-hostshim3'):
            self.assertIsNone(R.reconcile(str(self.repo), name, {name}, 'owner', 'acceptance', name=name))
        # Both real incidents had ordinary explanations and passing reviews.
        # Neither is an exemption from inspection at the visible number four.
        for name in ('zach-opus-hostshim4', 'zach-opus-lease4'):
            self.review('a'*40, name, verdict='passed', work=name)
            record = R.reconcile(str(self.repo), name, {name}, 'owner', 'acceptance', name=name)
            self.assertEqual(record['status'], 'investigating')
            self.assertEqual(record['repeat_signal']['iteration'], 4)
            self.assertEqual(record['failed'], [])
            self.assertEqual(record['recovery_failures'], 0)
            self.assertTrue(all(R.repeat_pending(record, role) for role in ('axel', 'wes')))

    def test_repeat_inspection_does_not_renew_pending_budget_or_count_a_failure(self):
        self.ledger.write_text('')
        def check(name, now):
            return R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', name=name, now=now)
        first = check('zach-opus-hostshim4', 100)
        self.assertEqual(first, check('zach-opus-hostshim4', 100))
        later = check('echo-sonnet-hostshim5', 200)
        self.assertEqual(later['deadline'], first['deadline'])
        self.assertNotEqual(later['evidence_cursor'], first['evidence_cursor'])
        self.assertEqual(later['repeat_signal']['iteration'], 5)
        self.assertEqual(check('zach-opus-hostshim3', 201)['repeat_signal']['iteration'], 5)
        self.assertEqual(later['recovery_failures'], 0)
        self.assertEqual(later['aliases'], ['numbered'])

    def test_questions_are_not_verdicts_and_both_deliveries_complete_the_alarm(self):
        self.ledger.write_text('')
        record = R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', mode='enforce', name='zach-opus-lease4')
        answer = dict(job=record['id'], acceptance='acceptance', frontier=record['evidence_cursor'],
            questions=[dict(question='What required work remains after the fix already landed?', evidence='Predecessor handover')])
        for role in ('axel', 'wes'):
            record = R.repeat_questioned(record['id'], role, answer, role)
        self.assertFalse(R.repeat_ready(record))
        self.assertEqual(record['status'], 'investigating')
        self.assertEqual(record['controls'], [])
        reference = str(R.root()/(record['id']+'.json'))
        R.owner_delivered(reference, R.repeat_question_key(record, 'axel'), 'wrong-owner')
        self.assertFalse(R.read(record['id']).get('deliveries'))
        for role in ('axel', 'wes'):
            R.owner_delivered(reference, R.repeat_question_key(record, role), 'owner')
            record = R.read(record['id'])
            self.assertEqual(record['status'], 'investigating')
        self.assertTrue(R.repeat_ready(record))
        self.assertFalse(record.get('repeat_outcome'))
        answer = dict(job=record['id'], acceptance='acceptance', frontier=record['evidence_cursor'],
            question=record['repeat_questions']['wes']['consultation'], answer_ids=['reply-1'], outcome='self_correction',
            reason='Rich recognized the unnecessary work and will finish the original outcome.', questions=[])
        record = R.repeat_response(record['id'], 'wes', answer, 'response-1', [dict(uuid='reply-1')])
        self.assertEqual(record['repeat_outcome'], 'self_correction')
        self.assertEqual(record['status'], 'resolved_findings')
        self.assertIsNone(record['plan'])
        self.assertEqual(record['controls'], [])
        again = R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', mode='enforce', name='zach-opus-lease5')
        self.assertEqual(again['status'], 'investigating')
        self.assertTrue(R.repeat_pending(again, 'wes'))
        self.assertFalse(R.repeat_ready(again))
        with self.assertRaises(R.Unavailable):
            R.repeat_questioned(again['id'], 'wes', answer, 'stale')

    def test_delivered_questions_do_not_discharge_real_recovery_or_existing_control(self):
        record = R.reconcile(str(self.repo), self.work, self.aliases, 'owner', 'acceptance', mode='enforce', name='zach-opus-hostshim4')
        answer = dict(job=record['id'], acceptance='acceptance', frontier=record['evidence_cursor'],
            questions=[dict(question='Why is the same failure still recurring?', evidence='Three failed handovers')])
        for role in ('axel', 'wes'):
            record = R.repeat_questioned(record['id'], role, answer, role)
            R.owner_delivered(str(R.root()/(record['id']+'.json')), R.repeat_question_key(record, role), 'owner')
        record = R.read(record['id'])
        self.assertTrue(R.repeat_ready(record))
        self.assertEqual(record['status'], 'investigating')
        self.assertEqual(record['recovery_failures'], 0)
        self.assertEqual(record['controls'], [])

    def test_stronger_control_needs_a_bad_answer_then_a_delivered_followup_and_new_bad_answer(self):
        self.ledger.write_text('')
        record = R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', mode='enforce', name='zach-opus-hostshim4')
        questions = [dict(question='Who would write that payload?', evidence='Deliberately constructed script')]
        record = R.repeat_questioned(record['id'], 'wes', dict(job=record['id'], acceptance='acceptance',
            frontier=record['evidence_cursor'], questions=questions), 'question-1')
        reference = str(R.root()/(record['id']+'.json'))
        R.owner_delivered(reference, R.repeat_question_key(record, 'wes'), 'owner')
        answer = dict(job=record['id'], acceptance='acceptance', frontier=record['evidence_cursor'], question='question-1',
            answer_ids=['a1'], outcome='escalate', reason='Rich repeated the assumption without supporting evidence', questions=[])
        with self.assertRaises(R.Unavailable):
            R.repeat_response(record['id'], 'wes', answer, 'premature', [dict(uuid='a1')])
        control = dict(job=record['id'], acceptance='acceptance', frontier=record['evidence_cursor'], action='stop',
            workers={}, reason='A fourth name alone', evidence='name')
        with self.assertRaises(R.Unavailable):
            R.control(record['id'], control, 'premature-control')
        self.assertIsNone(R.admission(record, 'worker', 'brief'))
        self.assertEqual(R.landing_gap(str(self.repo), 'numbered'), '')
        record = R.repeat_response(record['id'], 'wes', dict(answer, outcome='probe_again', questions=questions),
            'question-2', [dict(uuid='a1')])
        second = dict(answer, question='question-2', answer_ids=['a2'])
        with self.assertRaises(R.Unavailable):
            R.repeat_response(record['id'], 'wes', second, 'undelivered', [dict(uuid='a2')])
        R.owner_delivered(reference, R.repeat_question_key(record, 'wes'), 'owner')
        with self.assertRaises(R.Unavailable):
            R.repeat_response(record['id'], 'wes', dict(second, answer_ids=['a1']), 'duplicate', [dict(uuid='a1')])
        record = R.repeat_response(record['id'], 'wes', second, 'supported', [dict(uuid='a2')])
        self.assertTrue(record['repeat_recovery'])
        self.assertEqual(R.control(record['id'], control, 'supported')['action'], 'stop')

    def test_repetition_never_clears_exhausted_recovery(self):
        record = R.read(self.record['id'])
        record['status'] = 'awaiting_alex'
        R.save(record, 'fixture-escalation')
        current = R.reconcile(str(self.repo), self.work, self.aliases, 'owner', 'acceptance', name='zach-opus-lease4')
        self.assertEqual(current['status'], 'awaiting_alex')

    def test_conflicting_pass_creates_one_focused_independent_reconciliation(self):
        self.work='teammate:conflict-only'
        tip='c'*40
        def add(rid,verdict,findings=(),earlier=(),parent=''):
            directory=self.base/rid;directory.mkdir()
            (directory/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=list(findings),earlier_findings=list(earlier)),earlier_findings_in=[dict(id=e['id']) for e in earlier])))
            row=dict(id=rid,repo=str(self.repo),work=self.work,tip=tip,trigger='handover',verdict=verdict,record=str(directory),recheck_of=parent)
            with self.ledger.open('a') as out:out.write(json.dumps(row)+'\n')
        add('block','changes-requested',[dict(title='real blocker',blocks=True,priority=1)])
        add('ignored','passed')
        task=R.conflict_task(str(self.repo),self.work,tip)
        self.assertEqual(task['prior'],'block')
        self.assertEqual(task['findings'],['block#1'])
        self.assertEqual(R.conflict_task(str(self.repo),self.work,tip),task)
        self.assertIsNone(R.conflict_task(str(self.repo),'unrelated',tip))
        add('rechecked','passed',earlier=[dict(id='block#1',status='withdrawn',blocks=False,note='independent original authority contradicts premise')],parent='block')
        self.assertIsNone(R.conflict_task(str(self.repo),self.work,tip))
        effective,_=R.F.review_of(R.rows(self.ledger),tip,str(self.repo))
        self.assertEqual(effective['verdict'],'passed')
        state=json.loads((R.root()/'reconciliations'/(task['id']+'.json')).read_text())
        self.assertEqual(state['status'],'resolved')

    def test_recheck_severity_and_filing_agree_across_producer_gate_and_next_review(self):
        import second_review as SR
        tip='c'*40
        first=self.base/'severity-first';first.mkdir()
        finding=dict(title='original finding',blocks=True,priority=2,files=[],evidence='source',fixture='')
        (first/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[finding]))))
        original=dict(id='severity-first',repo=str(self.repo),tip=tip,work=self.work,trigger='handover',verdict='changes-requested',record=str(first))
        for source,parent,note,witness,allowed in ((tip,'severity-first','independent severity evidence',True,True),
                ('d'*40,'','new-source severity evidence',True,True),(tip,'','unrelated PASS',True,False),
                (tip,'severity-first','',True,False),(tip,'severity-first','no input witness',False,False),
                ('d'*40,'','no input witness',False,False)):
            for blocks in (True,False):
                with self.subTest(source=source,parent=parent,note=note,blocks=blocks):
                    folder=Path(tempfile.mkdtemp(dir=self.base))
                    carried=dict(id='severity-first#1',**finding,filed=True)
                    answer=dict(findings=[],earlier_findings=[dict(id=carried['id'],status='still-open',blocks=blocks,note=note)])
                    (folder/'verdict.json').write_text(json.dumps(dict(answer=answer,earlier_findings_in=[carried] if witness else [])))
                    verdict='changes-requested' if SR.blocking(answer) else 'passed'
                    current=dict(original,id=folder.name,tip=source,recheck_of=parent,verdict=verdict,record=str(folder))
                    opened=R.F.open_review_findings([original,current])
                    effective,_=R.F.review_of([original,current],tip,str(self.repo))
                    expected_blocking=blocks or not allowed
                    self.assertEqual(opened[carried['id']]['blocks'],expected_blocking)
                    self.assertEqual(R.F.open_review_findings([original,current,original,current]),opened)
                    self.assertEqual(bool(effective.get('effective_findings')),expected_blocking if source==tip else True)
                    if not expected_blocking:
                        self.assertTrue(opened[carried['id']]['filed'])
                        with patch.object(SR,'read_ledger',return_value=[original,current]):
                            next_findings=SR.earlier_findings(self.work,'e'*40,str(self.repo))
                        self.assertEqual(next_findings[0][1]['blocks'],False)
                        self.assertTrue(next_findings[0][1]['filed'])

    def test_gate_carries_prior_source_blockers_through_omitted_rechecks(self):
        self.work='teammate:carried-source-only'
        old_tip,new_tip,later_tip='a'*40,'b'*40,'c'*40
        finding=dict(title='original blocker',blocks=True,priority=1,files=[],evidence='source',fixture='')
        def row(rid,tip,verdict,findings=(),earlier=(),work=None,parent=''):
            folder=self.base/rid;folder.mkdir()
            (folder/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=list(findings),earlier_findings=list(earlier)),earlier_findings_in=[dict(id=e['id'],**finding,source_tip=old_tip) for e in earlier])))
            return dict(id=rid,tip=tip,repo=str(self.repo),work=work or self.work,trigger='handover',verdict=verdict,record=str(folder),recheck_of=parent)
        original=row('carry-origin',old_tip,'changes-requested',[finding])
        upheld=row('carry-upheld',new_tip,'changes-requested',earlier=[dict(id='carry-origin#1',status='still-open',blocks=True,note='still broken')])
        omitted=row('carry-omitted',new_tip,'passed',parent='carry-upheld')
        unrelated=row('carry-other',old_tip,'changes-requested',[finding],work='unrelated')
        future=row('carry-future',later_tip,'passed',earlier=[dict(id='carry-origin#1',status='fixed',blocks=False,note='later implementation establishes acceptance')])
        effective,_=R.F.review_of([original,upheld,omitted,unrelated,future],new_tip,str(self.repo))
        self.assertEqual(effective['verdict'],'changes-requested')
        self.assertEqual(set(effective['effective_findings']),{'carry-origin#1'})
        later,_=R.F.review_of([original,upheld,omitted,unrelated,future],later_tip,str(self.repo))
        self.assertEqual(later['verdict'],'passed')
        for item in (original,upheld,omitted,unrelated,future):
            with self.ledger.open('a') as out:out.write(json.dumps(item)+'\n')
        task=R.conflict_task(str(self.repo),self.work,new_tip)
        self.assertEqual(task['prior'],'carry-upheld')
        self.assertIn('carry-origin',task['evidence'])
        self.assertNotIn('carry-future',task['evidence'])

    def test_watcher_automatically_requests_conflict_recheck_with_frozen_claim(self):
        self.work='teammate:watcher-conflict-only'
        import review_watch as RW
        from types import SimpleNamespace
        tip='c'*40
        for rid,verdict in [('block','changes-requested'),('ignored','passed')]:
            directory=self.base/rid;directory.mkdir()
            (directory/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[dict(title='blocker',blocks=True)] if rid=='block' else []))))
            with self.ledger.open('a') as out:out.write(json.dumps(dict(id=rid,repo=str(self.repo),work=self.work,tip=tip,trigger='handover',verdict=verdict,record=str(directory)))+'\n')
        item=RW.Item(self.work,'worker',str(self.repo),tip,'b'*40,'ended',ref='worker')
        world=SimpleNamespace(repos=['repo'],repos_error='',registry_items=lambda *a:([item],[]),codex_items=lambda *a:([],[]))
        watcher=RW.Watcher('/fixture/engine','',world)
        calls=[]
        def start(candidate,trigger,*a):calls.append((candidate,trigger));return dict(repo=candidate.repo,tip=candidate.tip)
        with patch.dict(os.environ,{'REVIEW_WATCH_STATE_DIR':str(self.base/'watch'),'RICHOS_ENTITY_ROOT':''}),patch.object(watcher,'reconcile',return_value={}),patch.object(watcher,'start',side_effect=start),patch.object(RW,'tell',return_value=[]):
            watcher.look(1000,{})
        self.assertEqual(len(calls),1)
        self.assertEqual(item.recheck,'block')
        args=item.args('handover')
        self.assertIn('--claims-file',args)
        task=json.loads(Path(args[args.index('--claims-file')+1]).read_text())
        self.assertEqual(task['findings'],['block#1'])
        self.assertEqual(task['evidence'],['block','ignored'])

    def test_existing_contract_probe_checks_an_adopted_recovery_declaration(self):
        source=Path(R.__file__).resolve().parents[2]
        code=(source/'scripts/hooks/contract-integrity-probe.sh').read_text().split('run_layer_RC() {',1)[1].split('\nrun_layer_R\n',1)[0]
        function='run_layer_RC() {'+code
        entity=self.repo/'.claude'
        entity.mkdir()
        (entity/'recovery.json').write_text(json.dumps(dict(schema=1,mode='observe',repositories=[str(self.repo)])))
        import recovery_admin
        reference=self.base/'runtime-contract.json'
        reference.write_text(json.dumps(recovery_admin.contract_snapshot(str(self.repo))))
        declaration=json.loads((entity/'recovery.json').read_text())
        declaration['runtime_contract']=str(reference)
        (entity/'recovery.json').write_text(json.dumps(declaration))
        program='REPO_ROOT="$1"; ENGINE_ROOT="$2"; FAILED=0; emit_pass(){ echo "$*"; }; emit_fail(){ echo "$*"; FAILED=1; };\n'+function+'\nrun_layer_RC\nexit "$FAILED"\n'
        result=subprocess.run(['bash','-c',program,'probe',str(self.repo),str(source)],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('RC.',result.stdout)
        frozen=json.loads(reference.read_text())
        frozen['source_pins']['scripts/lib/recovery.py']='0'*64
        reference.write_text(json.dumps(frozen))
        result=subprocess.run(['bash','-c',program,'probe',str(self.repo),str(source)],capture_output=True,text=True)
        self.assertEqual(result.returncode,1,result.stdout+result.stderr)
        self.assertIn('scripts/lib/recovery.py',result.stdout)
        (entity/'recovery.json').write_text('{"schema":1,"mode":"bogus"}')
        result=subprocess.run(['bash','-c',program,'probe',str(self.repo),str(source)],capture_output=True,text=True)
        self.assertEqual(result.returncode,1,result.stdout+result.stderr)
        self.assertIn('contract unavailable',result.stdout)

    def test_adopted_contract_refuses_self_comparison_without_a_frozen_reference(self):
        import recovery_admin
        folder=self.repo/'.claude';folder.mkdir()
        (folder/'recovery.json').write_text(json.dumps(dict(schema=1,mode='observe',repositories=[str(self.repo)])))
        source=Path(R.__file__).resolve().parents[2]
        result=recovery_admin.contract(str(self.repo),str(source))
        self.assertEqual(result['status'],'unavailable')
        self.assertIn('independent runtime_contract reference is required',result['mismatches'])

    def test_report_preserves_missing_cost_and_actual_observations(self):
        import recovery_admin
        with R.locked(self.record['id']):
            record=R.read(self.record['id'])
            record['consults']=[dict(id='known',role='axel',phase='control',status='complete',started_at=10,ended_at=13,cost=dict(cost_usd=.25)),dict(id='missing',role='wes',phase='finding',status='unavailable',started_at=20)]
            R.save(record,'report-fixture')
        result=recovery_admin.report(self.record['id'])
        self.assertEqual(result['axel_jobs'],1)
        self.assertEqual(result['known_cost_usd'],.25)
        self.assertFalse(result['cost_complete'])
        self.assertEqual(result['jobs'][0]['consultations'][0]['elapsed_seconds'],3)
        self.assertEqual({g['field'] for g in result['missing_data']},{'cost_usd','elapsed','tokens'})
        self.assertEqual(R.read(self.record['id'])['consults'],record['consults'])

    def test_contract_detects_installed_mode_and_source_drift(self):
        import recovery_admin
        import shutil
        source=Path(R.__file__).resolve().parents[2]
        installed=self.base/'installed'
        for path in ['scripts/lib/operator_fences.py','scripts/lib/recovery.py','scripts/lib/machine_settings.py','scripts/lib/recovery_runner.py','scripts/lib/recovery_controls.py','scripts/lib/pierce.py','recovery-profiles/axel.md','recovery-profiles/wes.md','mega-lander/workspaces.py','scripts/app-engine-hook.py']:
            destination=installed/path
            destination.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(source/path,destination)
        result=recovery_admin.contract(str(self.repo),str(installed))
        self.assertEqual(result['status'],'passed',result)
        p=installed/'scripts/lib/recovery.py'
        p.write_text(p.read_text()+'\ndef mode(entity): return dict(mode="observe", repositories=[])\n')
        result=recovery_admin.contract(str(self.repo),str(installed))
        self.assertIn('scripts/lib/recovery.py',result['mismatches'])
        self.assertIn('installed declaration/mode',result['mismatches'])

    def test_carried_blocker_routes_at_its_current_job_source(self):
        import recovery_admin
        work='teammate:carried'
        def review(rid, tip, findings=(), earlier=()):
            directory=self.base/rid
            directory.mkdir()
            (directory/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=list(findings),earlier_findings=list(earlier)),earlier_findings_in=[dict(id='carried-1#1')] if earlier else [])))
            R.F.append_jsonl(str(self.ledger),dict(id=rid,repo=str(self.repo),work=work,tip=tip,trigger='handover',verdict='changes-requested',record=str(directory)))
        review('carried-1','a'*40,[dict(blocks=True,priority=1,title='premise',files=[],evidence='original',fixture='')])
        review('carried-2','b'*40,earlier=[dict(id='carried-1#1',status='still-open',blocks=True,note='')])
        self.assertEqual(recovery_admin.main(['dispute','--repo',str(self.repo),'--work',work,'--tip','b'*40,'--finding','carried-1#1']),0)
        R.F.route_refused_review(str(self.ledger),['b'*40],str(self.repo))
        routes=R.rows(R.root()/'routing-requests.jsonl')
        self.assertEqual({r['origin'] for r in routes},{'explicit-dispute','refused-landing'})
        self.assertTrue(all(r['findings']==['carried-1#1'] for r in routes))
        self.assertEqual(R.source_findings(R.rows(self.ledger),str(self.repo),self.work,'b'*40),{})
        self.assertEqual(R.source_findings(R.rows(self.ledger),str(self.repo),work,'c'*40),{})

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

    def test_questions_only_declaration_needs_no_qualification_and_never_controls(self):
        entity = self.base / "entity"
        (entity / ".claude").mkdir(parents=True)
        (entity / ".claude/recovery.json").write_text(json.dumps(dict(schema=1, mode="questions-only", repositories=[str(self.repo)])))
        self.assertEqual(R.mode(str(entity))["mode"], "questions-only")
        self.assertTrue(R.quiet("questions-only") and R.asks("questions-only"))
        self.assertFalse(R.asks("observe") or R.quiet("enforce") or R.quiet("off") or R.asks("off"))
        (entity / ".claude/recovery.json").write_text(json.dumps(dict(schema=1, mode="questions only", repositories=[])))
        with self.assertRaises(R.Unavailable):
            R.mode(str(entity))
        record = R.reconcile(str(self.repo), "teammate:questions", {"teammate:questions"}, "owner", "acceptance",
                             mode="questions-only", name="zach-opus-lease4")
        self.assertEqual(R.read(record["id"])["mode"], "questions-only")
        # Controls, holds, budgets and finding resolution follow observe.
        self.assertIsNone(R.admission(record, "worker", "ordinary correction"))
        self.assertEqual(R.read(record["id"])["attempts"], [])
        self.assertEqual(R.landing_gap(str(self.repo), "teammate:questions"), "")
        # Acknowledged question delivery follows enforce.
        answer = dict(job=record["id"], acceptance="acceptance", frontier=record["evidence_cursor"],
                      questions=[dict(question="What does this addition protect?", evidence="send-1")])
        for role in ("axel", "wes"):
            record = R.repeat_questioned(record["id"], role, answer, role)
        self.assertFalse(R.repeat_ready(record))
        for role in ("axel", "wes"):
            R.owner_delivered(str(R.root()/(record["id"]+".json")), R.repeat_question_key(record, role), "owner")
        self.assertTrue(R.repeat_ready(R.read(record["id"])))
        observed = R.read(record["id"])
        observed["repeat_recovery"] = True
        R.save(observed, "fixture-escalated")
        control = R.control(record["id"], dict(job=record["id"], acceptance="acceptance", frontier=record["evidence_cursor"],
            action="stop", workers={}, reason="repeated unsatisfying answers", evidence="replies"), "advisor")
        self.assertTrue(control["observe_only"])

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
        carried=[dict(id='initial-%d#1'%n,title='failed acceptance',blocks=True,source_tip=str(n)*40) for n in range(3)]
        (self.base/row['id']/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[],earlier_findings=[dict(id=f['id'],status='fixed',blocks=False,note='independent trace proves the accepted ownership matrix') for f in carried]),earlier_findings_in=carried)))
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

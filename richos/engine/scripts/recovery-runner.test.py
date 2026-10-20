#!/usr/bin/env python3
"""Real asynchronous host processes with an isolated fake provider.

These fixtures qualify supervision and capabilities, not semantic judgment.
"""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import recovery as R
import recovery_runner as RR
spec = importlib.util.spec_from_file_location("recovery_fixture", HERE / "recovery.test.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class ResponseContract(unittest.TestCase):
    def test_source_join_waits_for_kernel_owner_then_reuses_the_same_inode(self):
        import tempfile
        import second_review as SR
        with tempfile.TemporaryDirectory(prefix='review-join-') as directory,patch.dict(os.environ,{'SECOND_REVIEW_STATE_DIR':directory}):
            repo=directory+'/repo';subprocess.run(['git','init','-q',repo],check=True)
            code="""import sys;sys.path.insert(0,sys.argv[1]);import second_review as s
with s.execution_claim(sys.argv[2],'a'*40,wait_seconds=10,say=lambda message:print(message,flush=True)):
 print('joined',flush=True)
"""
            child=None
            try:
                with SR.execution_claim(repo,'a'*40):
                    child=subprocess.Popen([sys.executable,'-B','-c',code,str(HERE/'lib'),repo],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                    # Hang guard: receive the owner's contention acknowledgment before release.
                    import selectors
                    with selectors.DefaultSelector() as selector:
                        selector.register(child.stdout,selectors.EVENT_READ)
                        self.assertTrue(selector.select(20),'join acknowledgment missing')
                    self.assertIn('joining',child.stdout.readline())
                    self.assertIsNone(child.poll())
                # load-bound: hang guard after kernel owner release; actual child exit and receipt decide the verdict.
                stdout,stderr=child.communicate(timeout=20)
                self.assertEqual(child.returncode,0,stderr)
                self.assertIn('joined',stdout)
            finally:
                if child and child.poll() is None:child.kill();child.wait()

    def test_watcher_reuses_old_qualified_verdict_without_spending_loss_budget(self):
        import contextlib,io,tempfile
        import second_review as SR
        import review_watch as RW
        with tempfile.TemporaryDirectory(prefix='review-receipt-') as directory,patch.dict(os.environ,{'REVIEW_WATCH_STATE_DIR':directory+'/watch'}):
            repo=directory+'/repo';subprocess.run(['git','init','-q',repo],check=True)
            row=dict(id='existing',at=SR.iso(1),repo=repo,repo_id=SR.repo_identity(repo),tip='a'*40,work='teammate:original',input_identity='exact-input',trigger='manual',verdict='passed')
            output=io.StringIO()
            with contextlib.redirect_stdout(output),patch.dict(os.environ,{'SECOND_REVIEW_INVOCATION':'current-invocation'}):SR.completion_receipt(row)
            log=Path(directory)/'child.log';log.write_text(output.getvalue())
            info=dict(repo=repo,tip=row['tip'],work=row['work'],trigger='handover',started_at=1000,log=str(log),pid=123,review_invocation='current-invocation')
            watcher=RW.Watcher('/fixture/engine','/fixture/config',None)
            lock=Path(directory)/'schedule.lock';lock.write_text('{}')
            result=watcher.settle(str(lock),info,1001,[row])
            self.assertEqual(result['outcome'],'verdict')
            self.assertEqual(row['at'],SR.iso(1))
            for key,value in [('work','unrelated'),('tip','b'*40),('input_identity','other'),('purpose','progress'),('invocation','previous-invocation'),('recheck_of','unrelated-review')]:
                receipt=json.loads(output.getvalue().split('RECEIPT ',1)[1]);receipt[key]=value
                log.write_text('SECOND-REVIEW RECEIPT '+json.dumps(receipt)+'\n')
                self.assertIsNone(watcher.joined_verdict(info,[row]),key)
            log.write_text('owner ended without a verdict')
            lock.write_text('{}')
            self.assertEqual(watcher.settle(str(lock),info,1002,[])['outcome'],'lost')

    def test_lost_rechecks_cannot_consume_previous_invocation_receipts_or_recent_rows(self):
        import contextlib,io,tempfile
        import second_review as SR
        import review_watch as RW
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory(prefix='review-invocations-') as directory,patch.dict(os.environ,{'REVIEW_WATCH_STATE_DIR':directory+'/watch','SECOND_REVIEW_STATE_DIR':directory+'/reviews'}):
            repo=directory+'/repo';subprocess.run(['git','init','-q',repo],check=True)
            repo=os.path.realpath(repo);tip='c'*40;work='teammate:worker'
            ledger=Path(R.F.review_ledger_default());ledger.parent.mkdir(parents=True)
            def add(rid,verdict,at,findings=()):
                folder=Path(directory)/rid;folder.mkdir()
                (folder/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=list(findings),earlier_findings=[]))))
                row=dict(id=rid,repo=repo,repo_id=SR.repo_identity(repo),tip=tip,work=work,trigger='handover',at=SR.iso(at),verdict=verdict,record=str(folder),input_identity='input-'+rid)
                with ledger.open('a') as out:out.write(json.dumps(row)+'\n')
                return row
            original=add('original','changes-requested',1,[dict(title='blocker',blocks=True)])
            class Writer(RW.Watcher):
                original=None
                def spawn(self,item,trigger,log_path):
                    with open(log_path,'w') as out,contextlib.redirect_stdout(out),patch.dict(os.environ,{'SECOND_REVIEW_INVOCATION':item.review_invocation}):
                        if self.original:SR.completion_receipt(self.original)
                        else:print('review ended without a verdict')
                    return 999999,'fixture-finished'
            watcher=Writer('/fixture/engine','',SimpleNamespace())
            item=RW.Item(work,'worker',repo,tip,'b'*40,'ended',ref='worker')
            watcher.original=original
            info=watcher.start(item,'handover',100,1,None,[])
            first_log=info['log']
            self.assertEqual(watcher.settle(RW.lock_path(repo,tip),info,101,R.rows(ledger))['outcome'],'verdict')
            add('ignored','passed',200)
            watcher.original=None;launches=[]
            for n in range(5):
                rows=R.rows(ledger);book=RW.Book(rows,{},RW.read_jsonl(RW._p('attempts.jsonl')))
                task=R.conflict_task(repo,work,tip)
                if not task or len(book.losses.get(item.key) or [])>=RW.MAX_LOSSES:continue
                item.recheck=task['prior']
                info=watcher.start(item,'handover',201+n,1,None,rows)
                launches.append(info['log'])
                self.assertEqual(watcher.settle(RW.lock_path(repo,tip),info,201.5+n,rows)['outcome'],'lost')
            self.assertEqual(len(launches),2)
            self.assertEqual(len(set([first_log]+launches)),3)

    def test_questions_only_delivers_questions_and_coverage_gaps_never_control_notices(self):
        questions = dict(questions=[dict(question='What does the addition protect?', evidence='send')], frontier='f', consultation='c')
        base = dict(id='job', mode='questions-only', status='investigating', owner='lead', work='teammate:w', acceptance='a',
                    evidence_cursor='f', repeat_signal=dict(label='zach-opus-prd3c'), repeat_questions=dict(axel=questions))
        notice = RR.owner_notice(base)
        self.assertIn('AXEL QUESTIONS', notice['lines'][0])
        self.assertEqual(notice['mode'], 'questions-only')
        quiet = dict(base, repeat_signal=None, repeat_questions={})
        for change in (dict(status='recovering', plan=dict(accepted=True, id='plan')), dict(status='awaiting_alex'),
                       dict(wes=dict(verdict=dict(action='withdraw_finding', finding='r#1')))):
            self.assertIsNone(RR.owner_notice(dict(quiet, **change)), change)
        gap = RR.owner_notice(dict(quiet, status='unavailable'))
        self.assertIn('unavailable', gap['lines'][0])
        self.assertIsNone(RR.owner_notice(dict(base, mode='observe')))
        with patch.object(R, 'read', return_value=dict(base, controls=[dict(action='stop', workers={'w': 'source'})])):
            ws = SimpleNamespace(recovery_stop=lambda *a: self.fail('questions-only applied a control'))
            RR.apply_controls('job', ws)

    def test_observed_advice_cannot_steer_reviews_or_dispatch_replacements(self):
        import second_review as SR
        record=dict(mode='observe',status='recovering',plan=dict(accepted=True,id='plan'),wes=dict(verdict=dict(action='withdraw_finding',finding='r#1')))
        self.assertIsNone(RR.owner_notice(record))
        with patch.object(R,'read',return_value=record),patch.object(R,'key',return_value='job'):
            self.assertIsNone(SR.actionable_recovery('repo','work'))
            record['mode']='enforce'
            self.assertIs(SR.actionable_recovery('repo','work'),record)

    def test_portable_storage_freezes_evidence_without_a_specific_volume(self):
        import tempfile
        with tempfile.TemporaryDirectory(prefix='recovery-storage-') as directory, patch.dict(os.environ,{'RICHOS_RECOVERY_EVIDENCE_ROOT':directory,'RICHOS_RECOVERY_WIP_ROOT':directory+'/wip'}):
            reference=RR.freeze_source(b'original human authority','source')
            self.assertEqual(Path(reference['reference']).read_bytes(),b'original human authority')
            self.assertTrue(Path(reference['reference']).is_relative_to(Path(directory).resolve()))
            self.assertEqual(R.storage_root('wip'),Path(directory).resolve()/'wip')
        with patch.dict(os.environ,{'RICHOS_RECOVERY_EVIDENCE_ROOT':'relative'}):
            with self.assertRaises(R.Unavailable):R.storage_root('evidence')
        with patch.dict(os.environ,{'RICHOS_RECOVERY_EVIDENCE_ROOT':'/Volumes/MissingRecoveryStorage/evidence'}), patch.object(Path,'is_mount',return_value=False):
            with self.assertRaises(R.Unavailable):R.storage_root('evidence')

    def test_provider_schema_binds_exact_job_and_finding(self):
        record=dict(id='job',acceptance='original',evidence_cursor='source',wes_candidates=['review#1'])
        shape=RR.bound_schema(RR.WES,record)
        answer=dict(finding='review#1: a title',applicability='applies',action='uphold',sources=['original'],rationale='grounded',missing=[])
        self.assertTrue(RR.SR.shape_problems(answer,shape))
        answer['finding']='review#1'
        self.assertFalse(RR.SR.shape_problems(answer,shape))
        RR.validate_wes(answer,record)
        answer['action']='withdraw_finding'
        with self.assertRaises(R.Unavailable):RR.validate_wes(answer,record)
        control=RR.bound_schema(RR.CONTROL,record)
        self.assertEqual(control['properties']['job']['enum'],['job'])
        self.assertNotIn('enum',RR.CONTROL['properties']['job'])

    def test_missing_authority_requires_specific_bounded_retrieval(self):
        record=dict(wes_candidates=['review#1'])
        answer=dict(finding='review#1',applicability='insufficient_evidence',action='needs_evidence',sources=[],rationale='original missing',missing=[])
        with self.assertRaises(R.Unavailable):RR.validate_wes(answer,record)
        answer['missing']=['original ruling source']
        RR.validate_wes(answer,record)


class Async(unittest.TestCase):
    setUp = fixture.Recovery.setUp
    tearDown = fixture.Recovery.tearDown
    review = fixture.Recovery.review
    reconcile = fixture.Recovery.reconcile

    def numbered(self, number):
        name = 'zach-opus-hostshim'+str(number)
        rec = dict(name=name, key=name, session_id='owner')
        ws = SimpleNamespace(recovery_chain=lambda r, observation=False: [r],
            recovery_work_key=lambda r, observation=False: 'teammate:'+r['key'],
            load_agent=lambda key: rec, save_agent=lambda r: None, Lock=lambda: R.contextlib.nullcontext(),
            prompt_lines=lambda prompt, field: [])
        return rec, ws

    def test_dispatch_and_watcher_schedule_both_on_four_without_a_review_or_assignment(self):
        import recovery_controls as C
        self.ledger.write_text('')
        declaration = dict(mode='enforce', repositories=[str(self.repo)], wes_policy='carried')
        for number in (3, 4):
            rec, ws = self.numbered(number)
            item = SimpleNamespace(source='native', repo=str(self.repo), ref=rec['key'], session='owner',
                state='running', started=1, env={}, claude='', accounts='', tip='a'*40)
            with patch.object(R, 'mode', return_value=declaration), patch.object(RR.SR, 'find_teammate', return_value=rec), \
                    patch.object(RR, 'launch') as launch:
                RR.tick([item], str(HERE.parent), 'entity', ws)
                self.assertEqual([c.args[1] for c in launch.call_args_list], [] if number == 3 else ['axel', 'wes'])
            # The dispatch path records the same alarm before the watcher polls.
            with patch.object(R, 'mode', return_value=declaration), patch.object(RR, 'consultation') as model:
                jobs = C.admitted(ws, {}, rec, dict(kind='implementation'), [str(self.repo)], 'entity')
                self.assertEqual(len(jobs), 0 if number == 3 else 1)
                model.assert_not_called()
        with patch.object(R, 'mode', return_value=dict(mode='off')), patch.object(RR, 'launch') as launch:
            RR.tick([item], str(HERE.parent), 'entity', ws)
            self.assertEqual(C.admitted(ws, {}, rec, dict(kind='implementation'), [str(self.repo)], 'entity'), [])
            launch.assert_not_called()

    def test_probing_questions_reach_rich_before_any_verdict_or_other_advisor(self):
        import recovery_controls as C
        import review_watch as RW
        self.ledger.write_text('')
        record = R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', mode='enforce', name='zach-opus-hostshim4')
        calls = []
        def consult(identity, role, phase, prompt, shape, executable, account):
            calls.append((role, phase))
            self.assertIn('HARD, probing questions', prompt)
            self.assertIn('even when all reviews passed', prompt)
            self.assertEqual(phase, 'questions')
            self.assertNotIn('action', shape['properties'])
            self.assertEqual(shape['properties']['frontier']['enum'], [record['evidence_cursor']])
            return dict(job=identity, acceptance='acceptance', frontier=record['evidence_cursor'],
                questions=[dict(question='Who would write that AppleScript?', evidence='The proposed payload requires deliberate authoring')]), role+'-result'
        with patch.object(RR.SR, 'load_workspaces', return_value=object()), patch.object(RR.SR, 'find_claude', return_value='provider'), \
                patch.object(RR, 'snapshot', return_value='{}'), patch.object(RR, 'consultation', side_effect=consult), \
                patch.object(C, 'schedule'), patch.object(R, 'control') as control, \
                patch.dict(os.environ, {'REVIEW_WATCH_STATE_DIR': str(self.base/'watch')}):
            # Wes's question reaches the existing lead-delivery path while Axel
            # has not even started. No control or answer from Rich is required.
            for role in ('wes', 'axel'):
                self.assertEqual(RR.run(record['id'], role, str(HERE.parent)), 0)
                current = R.read(record['id'])
                notice = RR.owner_notice(current)
                self.assertIn(role.upper()+' QUESTIONS', notice['lines'][0])
                self.assertIn('Who would write that AppleScript?', '\n'.join(notice['lines']))
                self.assertIsNone(RR.owner_notice(dict(current, mode='observe')))
                Path(RW._p('last-told.json')).parent.mkdir(parents=True, exist_ok=True)
                RW.stall_watch._write_json(RW._p('last-told.json'), {'recovery_refs': {
                    notice['key']: dict(record=notice['record'], session=notice['session'])}})
                RW.deliver([notice['key']], 100)
                self.assertIsNone(RR.owner_notice(R.read(record['id'])))
            control.assert_not_called()
        self.assertEqual(calls, [('wes', 'questions'), ('axel', 'questions')])
        current = R.read(record['id'])
        self.assertEqual(current['status'], 'investigating')
        self.assertTrue(R.repeat_ready(current))
        self.assertFalse(current.get('repeat_recovery'))
        self.assertEqual(current['controls'], [])
        self.assertIsNone(current['plan'])

    def test_rich_self_correction_needs_no_advisor_control_or_recovery_plan(self):
        record = R.reconcile(str(self.repo), self.work, self.aliases, 'owner', 'acceptance', mode='enforce', name='zach-opus-lease4')
        phases = []
        def consult(identity, role, phase, prompt, shape, executable, account):
            phases.append(phase)
            if phase == 'questions':
                return dict(job=identity, acceptance='acceptance', frontier=record['evidence_cursor'],
                    questions=[dict(question='What required work remains?', evidence='The main fix already landed')]), role+'-question'
            self.assertEqual(phase, 'reply')
            return dict(job=identity, acceptance='acceptance', frontier=record['evidence_cursor'],
                question='axel-question', answer_ids=['reply'], outcome='self_correction',
                reason='Rich recognizes the optional hardening and will finish without another correction.', questions=[]), 'answer'
        with patch.object(RR.SR, 'load_workspaces', return_value=object()), patch.object(RR.SR, 'find_claude', return_value='provider'), \
                patch.object(RR, 'snapshot', return_value='{}'), patch.object(RR, 'consultation', side_effect=consult), \
                patch.object(RR, 'repeat_replies', return_value=[]) as replies, patch.object(R, 'control') as control:
            for role in ('axel', 'wes'):
                self.assertEqual(RR.run(record['id'], role, str(HERE.parent)), 0)
                current = R.read(record['id'])
                R.owner_delivered(str(R.root()/(record['id']+'.json')), R.repeat_question_key(current, role), 'owner')
            self.assertEqual(RR.run(record['id'], 'axel', str(HERE.parent)), 0)
            self.assertEqual(phases, ['questions', 'questions'])
            replies.return_value = [dict(uuid='reply', text='The fix is done. I will drop the optional hardening and finish.')]
            self.assertEqual(RR.run(record['id'], 'axel', str(HERE.parent)), 0)
            control.assert_not_called()
        current = R.read(record['id'])
        self.assertEqual(phases, ['questions', 'questions', 'reply'])
        self.assertEqual(current['repeat_outcome'], 'self_correction')
        self.assertEqual(current['status'], 'resolved_findings')
        self.assertIsNone(current['plan'])
        # Old failed handovers must not instantly undo the recognized correction.
        self.assertEqual(R.reconcile(str(self.repo), self.work, self.aliases, 'owner', 'acceptance',
            mode='enforce', name='zach-opus-lease4')['status'], 'resolved_findings')

    def test_only_new_lead_text_after_question_delivery_is_a_response_candidate(self):
        import datetime
        self.ledger.write_text('')
        record = R.reconcile(str(self.repo), 'numbered', {'numbered'}, 'owner', 'acceptance', mode='enforce', name='zach-opus-lease4')
        transcript = self.base/'lead.jsonl'
        stamp = lambda n: datetime.datetime.fromtimestamp(n, datetime.timezone.utc).isoformat()
        def row(uid, role, at, **extra):
            return dict(uuid=uid, type=role, timestamp=stamp(at), message=dict(content='answer '+uid), **extra)
        transcript.write_text('\n'.join(json.dumps(r) for r in [row('old', 'assistant', 1), row('new', 'assistant', 3),
            row('human', 'user', 3), row('worker', 'assistant', 3, isSidechain=True)])+'\n')
        record = R.repeat_questioned(record['id'], 'axel', dict(job=record['id'], acceptance='acceptance',
            frontier=record['evidence_cursor'], questions=[dict(question='Why another round?', evidence='iteration 4')]), 'q', [str(transcript)])
        self.assertEqual(RR.repeat_replies(record, 'axel'), [])
        record['deliveries'] = {R.repeat_question_key(record, 'axel'): dict(at=2, owner='owner')}
        self.assertEqual([r['uuid'] for r in RR.repeat_replies(record, 'axel')], ['new'])
        record['repeat_responses'] = {'axel': [dict(answer_ids=['new'])]}
        self.assertEqual(RR.repeat_replies(record, 'axel'), [])

    def test_actual_background_pipeline_and_exact_profile_capabilities(self):
        provider = self.base / "fake-claude"
        provider.write_text('''#!/usr/bin/env python3
import json, os, sys
a=sys.argv
assert a[a.index('--tools')+1]=='Read,Glob,Grep'
assert '--safe-mode' in a and '--restricted' in a
assert '--strict-mcp-config' in a and '--no-session-persistence' in a
shape=json.loads(a[a.index('--json-schema')+1])
prompt=sys.stdin.read()
r=json.load(open(os.environ['FAKE_RECORD']))
keys=shape['properties']
if 'workers' in keys:
 answer=dict(job=r['id'],acceptance=r['acceptance'],frontier=r['evidence_cursor'],action='continue',workers={},reason='inspect before changing direction',evidence='three independent handovers')
elif 'matrix' in keys:
 answer=dict(job=r['id'],acceptance=r['acceptance'],frontier=r['evidence_cursor'],diagnosis='two owners race',matrix=[dict(condition='one owner',evidence='source')],proof=[dict(condition='one owner',check='collision fixture')],approach='single owner',brief='Implement a single owner',control='continue',disproof='one existing owner in trace',retained_evidence=['passing transition fixture'])
else:
 answer=dict(plan=r['plan']['id'],accepted=True,evidence='matrix completeness independently checked',reviewer='model label is replaced by host', materially_different=True,difference_evidence='different ownership mechanism')
print(json.dumps(dict(type='result',structured_output=answer,total_cost_usd=0.01,usage=dict(input_tokens=20,output_tokens=40))))
''')
        provider.chmod(0o755)
        quota = self.base / "quota"
        quota.write_text('#!/bin/sh\nexit 0\n')
        quota.chmod(0o755)
        registry = self.base / "registry"
        registry.mkdir()
        env = dict(SECOND_REVIEW_CLAUDE=str(provider), SECOND_REVIEW_QUOTA_CMD=str(quota),
                   SECOND_REVIEW_CPU_BUSY="1", RICHOS_WORKSPACES_DIR=str(registry),
                   FAKE_RECORD=str(R.root() / (self.record['id']+'.json')))
        RR.launch(self.record, 'axel', str(HERE.parent), env)
        first = R.read(self.record['id'])['launches']['axel'][0]
        # A new owner/source cursor cannot create another live investigator.
        RR.launch(self.record, 'axel', str(HERE.parent), env)
        self.assertEqual(len(R.read(self.record['id'])['launches']['axel']), 1)
        deadline = time.monotonic()+15
        while time.monotonic()<deadline:
            record = R.read(self.record['id'])
            if not any(RR.same_launch(l) for launches in record.get('launches', {}).values() for l in launches):
                break
            time.sleep(.05)
        else:
            for launches in record.get('launches', {}).values():
                for launch in launches:
                    RR.stop_launch(launch)
            self.fail('bounded asynchronous fixture did not settle')
        self.assertEqual(record['status'], 'recovering', (R.root()/(self.record['id']+'.axel.log')).read_text())
        self.assertTrue(record['plan']['accepted'])
        self.assertNotEqual(record['plan']['assessment']['reviewer'], 'model label is replaced by host')
        self.assertEqual(len(record['consults']), 3)
        self.assertEqual(record['deadline'], self.record['deadline'])
        try:
            os.waitpid(first['pid'], 0)
        except ChildProcessError:
            pass

    def test_marked_cancellation_does_not_touch_a_sibling(self):
        mark = os.urandom(16).hex()
        own = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'],
            start_new_session=True, env=dict(os.environ, RICHOS_REVIEW_OWNER=mark))
        sibling = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], start_new_session=True)
        try:
            launch = dict(pid=own.pid, birth=R.F.proc(own.pid, precise=True)['start'], mark=mark)
            RR.stop_launch(launch)
            # load-bound: hang guard for the owned stopped child; verdict checks its actual exit and the untouched sibling.
            own.wait(timeout=5)
            self.assertIsNone(sibling.poll())
        finally:
            for child in (own, sibling):
                if child.poll() is None:
                    child.kill()
                child.wait()


if __name__ == '__main__':
    unittest.main()

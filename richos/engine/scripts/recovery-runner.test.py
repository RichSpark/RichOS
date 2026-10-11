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
            with contextlib.redirect_stdout(output):SR.completion_receipt(row)
            log=Path(directory)/'child.log';log.write_text(output.getvalue())
            info=dict(repo=repo,tip=row['tip'],work=row['work'],trigger='handover',started_at=1000,log=str(log),pid=123)
            watcher=RW.Watcher('/fixture/engine','/fixture/config',None)
            lock=Path(directory)/'schedule.lock';lock.write_text('{}')
            result=watcher.settle(str(lock),info,1001,[row])
            self.assertEqual(result['outcome'],'verdict')
            self.assertEqual(row['at'],SR.iso(1))
            for key,value in [('work','unrelated'),('tip','b'*40),('input_identity','other'),('purpose','progress')]:
                receipt=json.loads(output.getvalue().split('RECEIPT ',1)[1]);receipt[key]=value
                log.write_text('SECOND-REVIEW RECEIPT '+json.dumps(receipt)+'\n')
                self.assertIsNone(watcher.joined_verdict(info,[row]),key)
            log.write_text('owner ended without a verdict')
            lock.write_text('{}')
            self.assertEqual(watcher.settle(str(lock),info,1002,[])['outcome'],'lost')

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
        with tempfile.TemporaryDirectory(prefix='recovery-storage-') as directory, patch.dict(os.environ,{'RICHOS_RECOVERY_EVIDENCE_ROOT':directory,'RICHOS_RECOVERY_WIP_ROOT':directory+'/wip'}), patch.object(Path,'is_mount',return_value=False):
            reference=RR.freeze_source(b'original human authority','source')
            self.assertEqual(Path(reference['reference']).read_bytes(),b'original human authority')
            self.assertTrue(Path(reference['reference']).is_relative_to(directory))
            self.assertEqual(R.storage_root('wip'),Path(directory)/'wip')
        with patch.dict(os.environ,{'RICHOS_RECOVERY_EVIDENCE_ROOT':'relative'}):
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

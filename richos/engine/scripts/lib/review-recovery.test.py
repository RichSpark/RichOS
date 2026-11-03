#!/usr/bin/env python3
"""G11 failure/retry boundaries, using real files and fake delivery/model adapters."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

HERE = Path(os.environ.get('G11_LIB_DIR', Path(__file__).resolve().parent))
sys.path.insert(0, str(HERE))
import review_delivery as D
import review_watch as W
import second_review as S
import recovery_events as E
import proc_tree

spec = importlib.util.spec_from_file_location('shadow_g11', HERE / 'pierce_shadow.py')
H = importlib.util.module_from_spec(spec)
spec.loader.exec_module(H)


class Fixtures(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='g11-fixture-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = patch.dict(os.environ, {
            'CLAUDE_CONFIG_DIR': str(self.root / 'claude'),
            'RICHOS_CLAUDE_HOME': str(self.root / 'claude'),
            'REVIEW_WATCH_STATE_DIR': str(self.root / 'watch'),
            'SECOND_REVIEW_STATE_DIR': str(self.root / 'review'),
        })
        self.env.start()
        self.addCleanup(self.env.stop)

    @contextlib.contextmanager
    def delivery(self, output):
        row = dict(id='review-1', work='teammate:job', _done=10, trigger='handover')
        with patch.object(D, 'teammate_verdicts', return_value=[row]), \
                patch.object(D, 'teammate', return_value=({'key': 'job'}, {'teammate:job': 0}, set())), \
                patch.object(D, 'render', return_value='verdict'), \
                patch.object(sys, 'stdin', io.StringIO('{"agent_id":"worker"}')), \
                patch.object(sys, 'stdout', output):
            yield

    def test_delivery_write_failure_retries(self):
        class Broken(io.StringIO):
            def write(self, text):
                raise BrokenPipeError('fixture write failure')
        with self.delivery(Broken()), self.assertRaises(BrokenPipeError):
            D.main()
        output = io.StringIO()
        with self.delivery(output):
            D.main()
        self.assertIn('verdict', output.getvalue())
        duplicate = io.StringIO()
        with self.delivery(duplicate):
            D.main()
        self.assertEqual(duplicate.getvalue(), '')

    def test_delivery_flush_failure_retries(self):
        class Broken(io.StringIO):
            def flush(self):
                raise BrokenPipeError('fixture flush failure')
        with self.delivery(Broken()), self.assertRaises(BrokenPipeError):
            D.main()
        output = io.StringIO()
        with self.delivery(output):
            D.main()
        self.assertIn('verdict', output.getvalue())

    def test_shadow_notice_failure_retries_before_completing(self):
        counts = {kind: {'positive': H.TARGET_POSITIVE_PER_KIND} for kind in (1, 2)}
        with patch.object(H, 'root', return_value=self.root), \
                patch.object(H, 'progress', return_value=counts), patch.object(H, 'log'), \
                patch.object(H.subprocess, 'run', side_effect=[
                    types.SimpleNamespace(returncode=1, stdout='failed'),
                    types.SimpleNamespace(returncode=0, stdout='delivered')]) as send:
            H.finish_if_complete()
            self.assertFalse((self.root / 'complete.json').exists())
            H.finish_if_complete()
            self.assertTrue((self.root / 'complete.json').exists())
            H.finish_if_complete()
            self.assertEqual(send.call_count, 2)

    def test_shadow_full_kind_retries_notice_without_another_model_call(self):
        root = self.root / 'shadow'
        job = root / 'jobs' / 'one.json'
        job.parent.mkdir(parents=True)
        job.write_text(json.dumps(dict(pair='one', request={}, model='fixture', effort='fixture')))
        counts = {kind: {'positive': H.TARGET_POSITIVE_PER_KIND} for kind in (1, 2)}
        with patch.object(H, 'COLLECTION_ROOT', None), \
                patch.object(H, 'progress', return_value=counts), \
                patch.object(H, 'kind_of', return_value=(1, {})), \
                patch.object(H, 'heads', return_value={}), patch.object(H, 'log'), \
                patch.object(H, 'finish_if_complete') as finish, \
                patch.object(H, 'refused', side_effect=AssertionError('must not admit another model call')):
            self.assertEqual(H.run(job), 0)
            finish.assert_called_once_with()

    def test_shadow_timeout_can_retry(self):
        counts = {kind: {'positive': H.TARGET_POSITIVE_PER_KIND} for kind in (1, 2)}
        with patch.object(H, 'root', return_value=self.root), \
                patch.object(H, 'progress', return_value=counts), patch.object(H, 'log'), \
                patch.object(H.subprocess, 'run', side_effect=[
                    subprocess.TimeoutExpired('fixture', 60),
                    types.SimpleNamespace(returncode=0, stdout='delivered')]) as send:
            try:
                H.finish_if_complete()
            except subprocess.TimeoutExpired:
                pass
            self.assertFalse((self.root / 'complete.json').exists())
            H.finish_if_complete()
            self.assertTrue((self.root / 'complete.json').exists())
            self.assertEqual(send.call_count, 2)

    def item(self, work='job-two', state='ended'):
        return W.Item(work, work, str(self.root), 'tip', 'base', state, started=0)

    def test_shared_tip_jobs_each_get_handover(self):
        item = self.item()
        row = dict(repo=item.repo, tip=item.tip, work='job-one', verdict='passed', trigger='handover')
        self.assertEqual(len(W.due([item], W.Book([row], {}, []), 100, {}, 10)), 1)
        row['work'] = item.work
        self.assertEqual(W.due([item], W.Book([row], {}, []), 100, {}, 10), [])

    def test_shared_tip_jobs_each_get_progress_review(self):
        item = self.item(state='running')
        row = dict(repo=item.repo, tip=item.tip, work='job-one', verdict='passed', trigger='long-job')
        self.assertEqual(len(W.due([item], W.Book([row], {}, []), 100, {}, 10)), 1)
        row['work'] = item.work
        self.assertEqual(W.due([item], W.Book([row], {}, []), 100, {}, 10), [])
        running = {item.key: dict(work='job-one', trigger='handover')}
        self.assertEqual(W.due([item], W.Book([], running, []), 100, {}, 10), [])

    def test_handover_waits_for_another_jobs_running_review(self):
        item = self.item()
        running = dict(work='job-one', trigger='long-job')
        self.assertEqual(W.due([item], W.Book([], {item.key: running}, []), 100, {}, 10), [])
        running['work'] = item.work
        self.assertEqual(W.due([item], W.Book([], {item.key: running}, []), 100, {}, 10),
                         [(item, 'handover', running)])

    def check_review_for_own_work(self, recheck=""):
        work = types.SimpleNamespace(repo=str(self.root), tip='tip', base='base',
                                     work_key='job-two', words=['second request'], claims=[])
        args = types.SimpleNamespace(trigger='handover', name='', recheck=recheck)
        row = dict(id='first', repo=work.repo, tip='tip', work='job-one', verdict='passed',
                   trigger='handover', input_identity='other-inputs')
        with patch.object(S, 'resolve_work', return_value=work), \
                patch.object(S, 'actionable_recovery', return_value=None), \
                patch.object(S, 'execution_claim', return_value=contextlib.nullcontext()), \
                patch.object(S, 'read_ledger', return_value=[row]), \
                patch.object(S, 'same_repo', return_value=True), \
                patch.object(S, 'repo_identity', return_value='fixture'), \
                patch.object(S, '_review_owned', return_value=0) as review:
            self.assertEqual(S.review(args), 0)
            review.assert_called_once_with(args, work)

    def test_shared_tip_review_executes_for_its_own_work(self):
        self.check_review_for_own_work()

    def test_explicit_recheck_can_name_another_work_at_the_same_source(self):
        self.check_review_for_own_work(recheck='first')

    def test_direct_review_accepts_devnull_output(self):
        rc, _ = S.run_bounded([sys.executable, '-B', '-c', 'pass'], str(self.root), '',
                              os.devnull, os.devnull, 10)
        self.assertEqual(rc, 0)

    def test_direct_timeout_stops_stubborn_descendant(self):
        child_file = self.root / 'child.json'
        code = '''import json,os,signal,subprocess,sys
import operator_fences
signal.signal(signal.SIGTERM, signal.SIG_IGN)
p=subprocess.Popen([sys.executable,'-c','import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(120)'])
with open(sys.argv[1],'w') as f: json.dump([p.pid,operator_fences.proc(p.pid,precise=True)['start']],f)
p.wait()
'''
        # Two seconds is the fixture's execution deadline, not a timing assertion.
        # Correctness is the captured child's generation being absent after cleanup.
        child = None
        try:
            rc, _ = S.run_bounded([sys.executable, '-B', '-c', code, str(child_file)],
                                  str(HERE), '', str(self.root / 'stdout'), str(self.root / 'stderr'), 2)
            self.assertIsNone(rc, (self.root / 'stderr').read_text())
            self.assertTrue(child_file.exists(), 'reviewer fixture never started its child')
            child = json.loads(child_file.read_text())
            current = proc_tree.operator_fences.proc(child[0], precise=True)
            self.assertTrue(current is None or current['zombie'] or current['start'] != child[1], current)
        finally:
            if child_file.exists():
                child = json.loads(child_file.read_text())
                current = proc_tree.operator_fences.proc(child[0], precise=True)
                if current and not current['zombie'] and current['start'] == child[1]:
                    os.kill(child[0], signal.SIGKILL)

    def test_human_tool_result_text_is_preserved(self):
        rows = [dict(type='user', timestamp='1', message={'content': 'Implement the fix.'}),
                dict(type='user', timestamp='2', message={'content': 'Keep every tool_result row in the output.'}),
                dict(type='user', timestamp='3', message={'content': [{'type': 'tool_result', 'content': 'machine'}]}),
                dict(type='user', isMeta=True, timestamp='4', message={'content': 'generated tool_result'}),
                dict(type='user', timestamp='5', message={'content': '<task-notification>machine</task-notification>'})]
        path = self.root / 'lead.jsonl'
        path.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        self.assertEqual(E.human_requests(path), [('1', 'Implement the fix.'),
                                                ('2', 'Keep every tool_result row in the output.')])

    @unittest.skipIf(os.geteuid() == 0, 'root bypasses file permissions; gap retry has separate coverage')
    def test_read_permission_restored_retries_unchanged_events(self):
        import recovery_runner as RR
        source = self.root / 'lead.jsonl'
        source.write_text(json.dumps(dict(type='assistant', message=dict(content=[
            dict(type='tool_use', name='Agent', id='dispatch', input=dict(name='worker', prompt='fixture'))]))) + '\n')
        state = self.root / 'recovery'
        worker = dict(key='worker', name='worker', session_id='fixture', tool_use_id='dispatch',
                      dispatch_transcript=str(source), workspaces=[dict(repo=str(self.root))])
        ws = types.SimpleNamespace(all_agents=lambda **kw: [worker],
                                   platform_agent_transcript=lambda row: str(self.root / 'worker.jsonl'))
        with patch.object(E.R, 'root', return_value=state), \
                patch.object(E.R, 'mode', return_value=dict(mode='observe', repositories=[str(self.root)])), \
                patch.object(E.R, 'rows', return_value=[]), \
                patch.object(E, 'send_ledger', return_value=([], str(self.root / 'ledger'), '')), \
                patch.object(S, 'load_workspaces', return_value=ws), contextlib.redirect_stdout(io.StringIO()):
            before = E.fingerprint(ws, 'fixture')
            source.chmod(0)
            try:
                with self.assertRaises(PermissionError):
                    RR.transcript_actions(source)
                E.main(['--engine', 'fixture', '--entity', 'fixture'])
            finally:
                source.chmod(0o600)
            self.assertEqual(E.fingerprint(ws, 'fixture'), before)
            self.assertEqual(len(RR.transcript_actions(source)), 1)
            with patch.object(E.subprocess, 'Popen', return_value=types.SimpleNamespace(pid=123)) as spawn:
                self.assertEqual(E.launch('fixture', 'fixture', ws), {'pid': 123})
                spawn.assert_called_once()

    def test_gap_keeps_cursor_retryable(self):
        for gap in ['lead transcript unavailable', 'job identity unavailable', 'events not processed',
                    'triage unavailable; kept pending']:
            with self.subTest(gap=gap):
                state = self.root / gap.split()[0]
                state.mkdir(exist_ok=True)
                with patch.object(E.R, 'root', return_value=state), \
                        patch.object(S, 'load_workspaces', return_value=object()), \
                        patch.object(E, 'fingerprint', return_value='unchanged'), \
                        patch.object(E, 'drain', return_value={'gaps': [gap]}), \
                        contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(E.main(['--engine', 'fixture', '--entity', 'fixture']), 0)
                self.assertFalse((state / 'events.cursor.json').exists())
        state = self.root / 'success'
        with patch.object(E.R, 'root', return_value=state), \
                patch.object(S, 'load_workspaces', return_value=object()), \
                patch.object(E, 'fingerprint', return_value='complete'), \
                patch.object(E, 'drain', return_value={'gaps': []}), contextlib.redirect_stdout(io.StringIO()):
            E.main(['--engine', 'fixture', '--entity', 'fixture'])
        self.assertEqual(json.loads((state / 'events.cursor.json').read_text())['fingerprint'], 'complete')


if __name__ == '__main__':
    unittest.main()

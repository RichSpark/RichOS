#!/usr/bin/env python3
"""token-waste-walk.py's two verdicts, offline, on the shapes the guest helper prints: each one
passes on the right answer and REFUSES on the wrong one (a missing control is not a pass)."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('token_waste_walk', HERE / 'token-waste-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

SETTLED = {'reason': 'the assignment was completed and this receipt never ran', 'withdrawn': [], 'retained': []}


class Receipts(unittest.TestCase):
    def test_a_settled_never_admitted_receipt_passes(self):
        rows = [{'name': 'a', 'admitted': True, 'settled': None}, {'name': 'b', 'admitted': False, 'settled': SETTLED}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'PASS')

    def test_an_open_never_admitted_receipt_fails(self):
        rows = [{'name': 'b', 'admitted': False, 'status': 'prepared', 'settled': None}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'FAIL')

    def test_no_never_admitted_receipt_is_not_reached_not_a_pass(self):
        self.assertEqual(walk.judge_receipts([{'name': 'a', 'admitted': True}])[0], 'NOT REACHED')
        self.assertEqual(walk.judge_receipts([])[0], 'NOT REACHED')

    def test_a_settlement_with_no_reason_fails(self):
        rows = [{'name': 'b', 'admitted': False, 'settled': {'reason': ' ', 'withdrawn': [], 'retained': []}}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'FAIL')

    def test_an_unreadable_receipt_fails(self):
        self.assertEqual(walk.judge_receipts([{'file': 'x.json', 'unreadable': True}])[0], 'FAIL')


class Quiet(unittest.TestCase):
    CONTROL = '[SILENT] control-silent: no commit and no transcript activity, for 60 min\n'

    def test_control_told_and_never_started_not_told_passes(self):
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': self.CONTROL})[0], 'PASS')

    def test_a_notice_for_a_never_started_teammate_fails(self):
        out = self.CONTROL + '[SILENT] lined-up-never-started: no commit\n'
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': out})[0], 'FAIL')

    def test_no_control_means_nothing_was_measured(self):
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': 'STALL-WATCH: no stall\n'})[0], 'FAIL')

    def test_a_failed_tick_fails(self):
        self.assertEqual(walk.judge_quiet({'rc': 2, 'out': self.CONTROL})[0], 'FAIL')


class QuietReadsHeadlines(unittest.TestCase):
    """Tom's findings 1-4 (follow-ups.md, 2026-10-29): the marker and the names are read from a stall's
    headline, not matched as substrings of the whole output."""
    BLOCK = ('STALL-WATCH 08:37Z: 1 stall (reported only)\n'
             '  [SILENT] control-silent: no commit, for 60 min (since 07:37Z)\n'
             '      You can: ask control-silent where it is; lined-up-never-started was only prepared\n')

    def test_a_name_in_another_stalls_detail_is_not_a_notice(self):
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': self.BLOCK})[0], 'PASS')

    def test_the_word_silent_in_prose_is_not_the_control_being_reported(self):
        out = 'STALL-WATCH: control-silent is not [SILENT] today\n'
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': out})[0], 'FAIL')

    def test_jsonl_framed_blocks_are_read(self):
        framed = json.dumps({'text': self.BLOCK}) + '\n'
        self.assertEqual(walk.parse_stalls(framed), [('SILENT', 'control-silent')])
        told = framed + json.dumps({'text': '  [SILENT] refused-never-started: no commit, for 60 min'}) + '\n'
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': told})[0], 'FAIL')

    def test_a_not_read_stall_is_not_the_control(self):
        out = '  [NOT READ] stall-watch: cannot read a source, for 0 min\n      no readable transcript for control-silent\n'
        self.assertEqual(walk.parse_stalls(out), [('NOT READ', 'stall-watch')])
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': out})[0], 'FAIL')


class GuestHelper(unittest.TestCase):
    """token-waste-guest.py, offline: its receipts read, its fixture string, its quiet command."""
    GUEST = HERE / 'token-waste-guest.py'
    ENGINE = HERE.parents[2] / 'engine'

    def helper(self):
        spec = importlib.util.spec_from_file_location('token_waste_guest', self.GUEST)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_one_interpreter_in_the_guest(self):
        self.assertEqual(walk.GUEST_PYTHON, '/usr/bin/python3')
        ran = []
        walker = object.__new__(walk.TokenWasteWalk)
        walker.vm, walker.payload, walker.data, walker.out = 'vm-test', '/p', '/d', Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__('shutil').rmtree(walker.out, ignore_errors=True))

        def guest(vm, command, timeout=30):
            ran.append(command)
            return '[]' if ' receipts ' in command else json.dumps({'rc': 0, 'out': QuietReadsHeadlines.BLOCK})

        with mock.patch.object(walk, 'guest', guest), mock.patch.object(walk.command_walk, 'command', lambda *a, **k: None):
            with self.assertRaises(walk.StepFailed):
                walker.receipts()
            walker.quiet()
        calls = [c for c in ran if 'token-waste-guest.py' in c]
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(c.startswith('/usr/bin/python3 ') for c in calls), calls)
        source = self.GUEST.read_text()
        self.assertNotIn("['python3'", source)
        self.assertIn('[sys.executable, ', source)
        self.assertIn("'--tick', '--jsonl'", source)

    def test_receipts_prints_each_receipt_with_whether_the_app_admitted_it(self):
        with tempfile.TemporaryDirectory() as data:
            box = Path(data) / 'engine-state' / 'work-receipts' / 'w1'
            box.mkdir(parents=True)
            (box / 'a.json').write_text(json.dumps({'name': 'a', 'status': 'sent', 'tool_use_id': 't1', 'request': {'role': 'x'}}))
            (box / 'b.json').write_text(json.dumps({'name': 'b', 'status': 'prepared', 'settled': SETTLED}))
            (box / 'c.json').write_text('not json')
            done = subprocess.run([sys.executable, str(self.GUEST), 'receipts', data], capture_output=True, text=True, check=True)
        rows = {r['file']: r for r in json.loads(done.stdout)}
        self.assertTrue(rows['a.json']['admitted'])
        self.assertFalse(rows['b.json']['admitted'])
        self.assertEqual(rows['b.json']['settled'], SETTLED)
        self.assertTrue(rows['c.json']['unreadable'])

    def test_the_fixture_writes_a_registry_stall_watch_can_read(self):
        with tempfile.TemporaryDirectory() as box:
            reg = str(Path(box) / 'reg')
            for kind, name in (('started', 'control-silent'), ('prepared', 'p'), ('refused', 'r')):
                made = subprocess.run([sys.executable, '-c', self.helper().FIXTURE, str(self.ENGINE), reg, 'sid-1',
                                       str(os.getpid()), kind, name], capture_output=True, text=True)
                self.assertEqual(made.returncode, 0, made.stderr)
            agents = {json.loads(p.read_text())['name']: json.loads(p.read_text()) for p in (Path(reg) / 'agents').glob('*.json')}
            self.assertEqual(sorted(agents), ['control-silent', 'p', 'r'])
            self.assertTrue(agents['control-silent']['agent_id'])
            self.assertEqual(agents['p']['agent_id'], '')
            self.assertEqual(agents['r']['tool_use_id'], 'toolu_refused_before_start')

    def test_quiet_runs_the_tick_over_a_fixture_registry_and_prints_its_words(self):
        module = self.helper()
        seen = []

        def run(cmd, **kw):
            seen.append((cmd, kw.get('env')))
            return subprocess.CompletedProcess(cmd, 0, stdout='  [SILENT] control-silent: x\n', stderr='')

        with tempfile.TemporaryDirectory() as box, mock.patch.object(module.subprocess, 'run', run):
            from contextlib import redirect_stdout
            import io
            sink = io.StringIO()
            with redirect_stdout(sink):
                module.quiet('/engine', 'sid-1', box)
        printed = json.loads(sink.getvalue())
        self.assertEqual(printed['rc'], 0)
        self.assertIn('[SILENT] control-silent', printed['out'])
        fixtures = [c for c, _ in seen if c[1] == '-c']
        self.assertEqual([c[-1] for c in fixtures], ['control-silent', 'lined-up-never-started', 'refused-never-started'])
        tick, env = seen[-1]
        self.assertEqual(tick[-2:], ['--tick', '--jsonl'])
        self.assertTrue(env['RICHOS_WORKSPACES_DIR'].startswith(box))


class HelpIsItsOwn(unittest.TestCase):
    def test_help_shows_this_walks_docstring_not_the_base_walks(self):
        done = subprocess.run([sys.executable, str(HERE / 'token-waste-walk.py'), '--help'], capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn('token-waste-walk.py', done.stdout)
        self.assertIn('NEVER started', done.stdout.replace('never started', 'NEVER started'))
        self.assertNotIn('stu-reminder-walk.py —', done.stdout.split('options:')[0])


class GitIdentity(unittest.TestCase):
    """Run 1 stalled 1200 s at `job`: with no git identity Stu asked "What name should go on your
    team's saved work?". The base walk's -c flags cover only its seed commit, so the walk itself
    must leave the fixture home with an identity. The guest is stood in for by a local shell."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name) / 'home'
        self.home.mkdir()
        self.walk = object.__new__(walk.TokenWasteWalk)
        self.walk.vm, self.walk.home = 'vm-test', str(self.home)
        self.ran = []

    def guest(self, vm, command, timeout=30):
        self.ran.append(command)
        return subprocess.run(['/bin/sh', '-c', command], capture_output=True, text=True, check=True).stdout.strip()

    def run_step(self, step):
        with mock.patch.object(walk, 'guest', self.guest), \
                mock.patch.object(walk.base.StuReminderWalk, step, lambda s, *a, **k: {'base': step}, create=True):
            return getattr(self.walk, step)()

    def commits_without_flags(self):
        """Can a bare `git commit` in a repository under the fixture home succeed, as Stu's does?"""
        env = {'PATH': '/usr/bin:/bin:/opt/homebrew/bin', 'HOME': str(self.home), 'XDG_CONFIG_HOME': str(self.home / '.config'),
               'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': str(self.home / '.gitconfig'),
               # never guess a name from the passwd entry: only a configured identity counts
               'GIT_CONFIG_COUNT': '1', 'GIT_CONFIG_KEY_0': 'user.useConfigOnly', 'GIT_CONFIG_VALUE_0': 'true'}
        repo = self.home / 'Acme'
        repo.mkdir()
        (repo / 'notes.md').write_text('x\n')
        subprocess.run(['git', 'init', '-q'], cwd=repo, env=env, check=True)
        subprocess.run(['git', 'add', 'notes.md'], cwd=repo, env=env, check=True)
        return subprocess.run(['git', 'commit', '-q', '-m', 'x'], cwd=repo, env=env, capture_output=True).returncode == 0

    def test_without_the_walk_a_bare_commit_has_no_identity(self):
        self.assertFalse(self.commits_without_flags())

    def test_first_run_leaves_the_fixture_home_with_a_git_identity(self):
        self.assertEqual(self.run_step('first_run'), {'base': 'first_run'})
        self.assertTrue(self.commits_without_flags())

    def test_job_alone_also_leaves_the_identity(self):
        self.assertEqual(self.run_step('job'), {'base': 'job'})
        self.assertTrue(self.commits_without_flags())

    def test_a_second_run_changes_nothing(self):
        self.run_step('first_run')
        before = (self.home / '.gitconfig').read_text()
        self.run_step('first_run')
        self.assertEqual((self.home / '.gitconfig').read_text(), before)


if __name__ == '__main__':
    unittest.main()

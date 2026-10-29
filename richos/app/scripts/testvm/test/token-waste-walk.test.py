#!/usr/bin/env python3
"""token-waste-walk.py's two verdicts, offline, on the shapes the guest helper prints: each one
passes on the right answer and REFUSES on the wrong one (a missing control is not a pass)."""
import importlib.util
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

#!/usr/bin/env python3
"""workspace_scope.test.py — when a test run is an engineer's workspace iteration, and ci-shard.sh's
default there (CEO, 2026-10-01: the narrow run is the default, `--full` is one flag away, and the
merge gate, proof-run.py and the nightlies are never narrowed).

Every case builds a fixture repository (a `main` checkout plus linked worktrees) in TMPDIR.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parent
sys.path.insert(0, str(HERE))
import workspace_scope as ws  # noqa: E402

CLEAN = {k: v for k, v in os.environ.items() if k != ws.ENV}


def git(cwd, *args):
    out = subprocess.run(['git', '-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                          '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', *args],
                         cwd=cwd, capture_output=True, text=True)
    if out.returncode != 0:
        raise AssertionError('git %s: %s' % (' '.join(args), out.stderr))
    return out.stdout.strip()


def repo(tmp, files):
    main = Path(tmp) / 'main'
    main.mkdir()
    git(main, 'init', '-q', '-b', 'main')
    for rel, text in files.items():
        p = main / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
        p.chmod(0o755)
    git(main, 'add', '-A')
    git(main, 'commit', '-q', '-m', 'main')
    return main


def worktree(main, name, branch, detach=False):
    path = main.parent / name
    if detach:
        git(main, 'worktree', 'add', '-q', '--detach', str(path), 'main')
    else:
        git(main, 'worktree', 'add', '-q', '-b', branch, str(path), 'main')
    return path


class Decide(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix='workspace-scope.')
        self.main = repo(self.tmp, {'a.txt': 'one\ntwo\nthree\nfour\n'})

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_w1_a_cc_branch_in_a_linked_worktree_is_a_workspace(self):
        wt = worktree(self.main, 'wt', 'cc/fixture-one')
        got = ws.decide(str(wt), environ={})
        self.assertEqual((got['scope'], got['branch'], got['base']),
                         ('narrow', 'cc/fixture-one', git(self.main, 'rev-parse', 'main')))
        self.assertIn('teammate workspace cc/fixture-one', got['why'])

    def test_w2_the_main_checkout_another_branch_and_a_detached_head_are_full(self):
        self.assertEqual(ws.decide(str(self.main), environ={})['scope'], 'full')
        self.assertEqual(ws.decide(str(worktree(self.main, 'other', 'feature/x')), environ={})['scope'], 'full')
        detached = ws.decide(str(worktree(self.main, 'nightly', None, detach=True)), environ={})
        self.assertEqual((detached['scope'], detached['why']), ('full', 'detached HEAD'))
        git(self.main, 'checkout', '-q', '-b', 'cc/in-the-main-checkout')
        self.assertEqual(ws.decide(str(self.main), environ={})['why'], 'the main checkout, not a teammate workspace')
        self.assertEqual(ws.decide(self.tmp, environ={})['why'], 'not a git checkout')

    def test_w3_full_and_the_gate_variable_win_and_a_typo_is_refused(self):
        wt = worktree(self.main, 'wt', 'cc/fixture-two')
        self.assertEqual(ws.decide(str(wt), full=True, environ={})['scope'], 'full')
        self.assertEqual(ws.decide(str(wt), environ={ws.ENV: 'full'})['scope'], 'full')
        self.assertEqual(ws.decide(str(self.main), environ={ws.ENV: 'narrow'})['scope'], 'narrow')
        with self.assertRaises(ValueError):
            ws.decide(str(wt), environ={ws.ENV: 'fulll'})
        cli = subprocess.run([sys.executable, str(HERE / 'workspace_scope.py'), 'decide', str(wt)],
                             capture_output=True, text=True, env={**CLEAN, ws.ENV: 'fulll'})
        self.assertEqual(cli.returncode, 64)

    def test_w4_changed_paths_and_lines_cover_committed_uncommitted_and_new_work(self):
        wt = worktree(self.main, 'wt', 'cc/fixture-three')
        base = git(self.main, 'rev-parse', 'main')
        (wt / 'committed.txt').write_text('x\n')
        git(wt, 'add', '-A')
        git(wt, 'commit', '-q', '-m', 'c')
        (wt / 'a.txt').write_text('one\nTWO\nthree\n')        # line 2 changed, line 4 deleted
        (wt / 'new.txt').write_text('a\nb\n')
        self.assertEqual(ws.changed(str(wt), base), ['a.txt', 'committed.txt', 'new.txt'])
        self.assertEqual(ws.lines(str(wt), base, 'a.txt'), [2, 3, 4])
        self.assertEqual(ws.lines(str(wt), base, 'new.txt'), [1, 2])
        self.assertEqual(ws.lines(str(wt), base, 'gone.txt'), [])

    def test_w7_a_failed_git_read_is_an_error_never_an_empty_diff(self):
        # P5-79: with a base git cannot read, the tracked diff and the hunks used to read as
        # "nothing changed" while the untracked files still came back, narrowing a run to a
        # subset of the branch's real changes (or an empty one).
        wt = worktree(self.main, 'wt', 'cc/fixture-seven')
        (wt / 'a.txt').write_text('one\nTWO\nthree\nfour\n')
        (wt / 'new-test.swift').write_text('x\n')
        bad = '0' * 40
        with self.assertRaises(ValueError):
            ws.changed(str(wt), bad)
        with self.assertRaises(ValueError):
            ws.lines(str(wt), bad, 'a.txt')
        for args in (['changed', str(wt), bad], ['lines', str(wt), bad, 'a.txt']):
            cli = subprocess.run([sys.executable, str(HERE / 'workspace_scope.py'), *args],
                                 capture_output=True, text=True, env=CLEAN)
            self.assertEqual(cli.returncode, 64, cli.stdout)
            self.assertEqual(cli.stdout, '', 'a failed read must print no partial answer')
            self.assertIn('unknown', cli.stderr)
        # the opposite, same path: a good base still answers
        base = git(self.main, 'rev-parse', 'main')
        self.assertEqual(ws.changed(str(wt), base), ['a.txt', 'new-test.swift'])

    def test_w6_the_fields_form_reads_without_eval_even_with_no_branch(self):
        wt = worktree(self.main, 'wt', 'cc/fixture-four')
        base = git(self.main, 'rev-parse', 'main')
        detached = worktree(self.main, 'nightly', None, detach=True)
        script = 'IFS=: read -r scope base branch <<<"$(python3 "$1" fields "$2")"; printf "%s|%s|%s\\n" "$scope" "$base" "$branch"'
        for where, want in ((wt, 'narrow|%s|cc/fixture-four' % base), (detached, 'full||')):
            out = subprocess.run(['bash', '-c', script, 'x', str(HERE / 'workspace_scope.py'), str(where)],
                                 capture_output=True, text=True, env=CLEAN)
            self.assertEqual(out.stdout.strip(), want, out.stderr)

    def test_w5_a_suites_inputs_claim_files_and_whole_directories(self):
        suite = Path(self.tmp) / 's.test.sh'
        suite.write_text('#!/bin/bash\n# run-tests: inputs dir/sub file.sh\n')
        declared = ws.inputs(str(suite))
        self.assertEqual(declared, ['dir/sub', 'file.sh'])
        self.assertTrue(ws.claimed('dir/sub/x.swift', declared))
        self.assertTrue(ws.claimed('file.sh', declared))
        self.assertFalse(ws.claimed('dir/subway.swift', declared))


class CiShard(unittest.TestCase):
    """ci-shard.sh with no selector: the whole inventory, except in a teammate workspace."""

    LIBS = ('ci-receipts.py', 'leak-canary.sh', 'record-canary.sh', 'tree-witness.sh', 'proc_tree.py',
            'operator_fences.py', 'worker_tokens.py', 'engine_pass.py', 'machine_settings.py', 'workspace_scope.py')

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix='workspace-scope-shard.')
        files = {'VERSION': '1.0.0-test\n',
                 'scripts/lib/green.test.sh': '#!/usr/bin/env bash\nexit 0\n',
                 'scripts/lib/other.test.sh': '#!/usr/bin/env bash\nexit 0\n',
                 'scripts/ci-affected-units.sh': '#!/usr/bin/env bash\n'
                                                 '[ -z "${AFFECTED_STUB:-}" ] || printf "%s\\n" "$AFFECTED_STUB"\n'}
        for f in ('ci-shard.sh', 'ci-units.sh'):
            files['scripts/' + f] = (SCRIPTS / f).read_text()
        for f in self.LIBS:
            files['scripts/lib/' + f] = (HERE / f).read_text()
        self.main = repo(self.tmp, files)
        self.wt = worktree(self.main, 'wt', 'cc/fixture-shard')
        (self.wt / 'scripts/lib/green.test.sh').write_text('#!/usr/bin/env bash\n# edit\nexit 0\n')

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def shard(self, where, *args, **env):
        return subprocess.run(['bash', str(where / 'scripts/ci-shard.sh'), *args], capture_output=True, text=True,
                              env={**CLEAN, **env})

    def test_c1_no_selector_in_a_workspace_runs_the_units_its_branch_maps_to(self):
        r = self.shard(self.wt, '--list', AFFECTED_STUB='scripts/lib/green.test.sh')
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn('workspace run on cc/fixture-shard', r.stdout)
        self.assertIn('bash scripts/ci-shard.sh --full', r.stdout)
        self.assertIn('green.test.sh', r.stdout)
        self.assertNotIn('other.test.sh', r.stdout)

    def test_c2_full_the_gate_a_selector_and_the_main_checkout_are_unchanged(self):
        for r in (self.shard(self.wt, '--full', '--list', AFFECTED_STUB='scripts/lib/green.test.sh'),
                  self.shard(self.wt, '--list', AFFECTED_STUB='scripts/lib/green.test.sh', RICHOS_TEST_SCOPE='full'),
                  self.shard(self.main, '--list', AFFECTED_STUB='scripts/lib/green.test.sh')):
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn('other.test.sh', r.stdout)
            self.assertNotIn('workspace run', r.stdout)
        only = self.shard(self.wt, '--only-units', 'scripts/lib/other.test.sh', '--list')
        self.assertIn('other.test.sh', only.stdout)
        self.assertNotIn('workspace run', only.stdout)

    def test_c3_a_branch_mapping_to_no_unit_is_refused_with_the_full_command(self):
        r = self.shard(self.wt, '--list')
        self.assertEqual(r.returncode, 2)
        self.assertIn('REFUSED', r.stderr)
        self.assertIn('bash scripts/ci-shard.sh --full', r.stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)

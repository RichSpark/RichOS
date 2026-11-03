#!/usr/bin/env python3
"""P3-44: guard only, never execute the proposed filesystem removal."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class RemovalPaths(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='g9-removal-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.repo = self.root / 'repo'
        self.parent = self.root / 'workers'
        self.worker = self.parent / 'a worker'
        self.worker.parent.mkdir()
        self.home = self.root / 'home'
        self.home.mkdir()
        self.env = dict(os.environ, HOME=str(self.home), GIT_CONFIG_NOSYSTEM='1',
                        RICHOS_ENTITY_ROOT=str(self.repo))
        for k in list(self.env):
            if k.startswith('GIT_CONFIG_') and k != 'GIT_CONFIG_NOSYSTEM':
                self.env.pop(k)
        self.git('init', '-q', str(self.repo))
        self.git('-C', str(self.repo), 'config', 'user.name', 'Fixture')
        self.git('-C', str(self.repo), 'config', 'user.email', 'fixture@example.invalid')
        (self.repo / 'orchestration.config').write_text('PROTECTED_PATHS="src"\n')
        self.git('-C', str(self.repo), 'add', '.')
        self.git('-C', str(self.repo), 'commit', '-qm', 'fixture')
        self.git('-C', str(self.repo), 'worktree', 'add', '-qb', 'codex/g9-fixture', str(self.worker))
        self.alias = self.root / 'alias'
        self.alias.symlink_to(self.parent, target_is_directory=True)
        self.addCleanup(self.intact)

    def git(self, *args):
        p = subprocess.run(['git', *args], env=self.env, capture_output=True, text=True,
                           timeout=60)  # load-bound: fixture setup hang guard; no elapsed-time assertion.
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout

    def intact(self):
        self.assertTrue((self.worker / '.git').is_file())
        self.assertEqual(self.git('-C', str(self.worker), 'branch', '--show-current').strip(), 'codex/g9-fixture')

    def verdict(self, cmd, expected, cwd=None, **env):
        p = subprocess.run(['/bin/bash', str(HERE / 'guard-worktree-removal.sh')],
                           input=json.dumps({'tool_name': 'Bash', 'cwd': str(cwd or self.repo),
                                             'tool_input': {'command': cmd}}),
                           env=dict(self.env, **env), capture_output=True, text=True,
                           timeout=60)  # load-bound: hook hang guard; verdict is exit code, never duration.
        self.assertEqual(p.returncode, expected, f'{cmd}\n{p.stdout}\n{p.stderr}')

    def test_44_guarded_variables(self):
        for command in (f'd="{self.worker}"; rm -rf "${{d:?}}"',
                        f'd="{self.worker}" && rm -rf "$d"',
                        'rm -rf "${G9_WORKER:?}"',
                        'rm -rf "${unknown_workspace:?}"'):
            with self.subTest(command=command):
                self.verdict(command, 2, G9_WORKER=str(self.worker))

    def test_44_parent_child_and_relative_paths(self):
        for command, cwd in (
                (f'rm -rf "{self.worker}"', self.repo),
                (f'rm -rf "{self.parent}"', self.repo),
                (f'rm -rf "{self.worker}/.git"', self.repo),
                (f'rm -rf "{self.alias}"', self.repo),
                (f'cd "{self.parent}" && rm -rf "a worker"', self.repo),
                ('rm -rf "workers/a worker"', self.root)):
            with self.subTest(command=command):
                # rm of a directory symlink removes the link, not its target.
                self.verdict(command, 0 if command == f'rm -rf "{self.alias}"' else 2, cwd)
        self.verdict(f'rm -rf "{self.alias}/a worker"', 2)
        self.verdict(f'rm -rf "{self.alias}/"', 2)
        self.verdict(f'rm -f "{self.worker}/.git"', 2)
        self.verdict('cd "$G9_PARENT" && rm -rf "a worker"', 2, G9_PARENT=str(self.parent))
        detached_root = self.root / 'codex-home/worktrees'
        detached_root.mkdir(parents=True)
        self.verdict(f'rm -rf "{detached_root.parent}"', 2,
                     WTR_CODEX_WORKTREE_ROOTS=str(detached_root))

    def test_44_harmless_text_and_unrelated_paths(self):
        self.verdict(f'printf \'rm -rf "{self.worker}"\'', 0)
        self.verdict(f'true # rm -rf "{self.worker}"', 0)
        self.verdict(f'git rm -r "{self.worker}"', 0)
        self.verdict(f'rm -rf "{self.root}/ordinary"', 0)
        self.verdict(f'cd "{self.parent}"; rm -rf other', 0)


if __name__ == '__main__':
    unittest.main(verbosity=2)

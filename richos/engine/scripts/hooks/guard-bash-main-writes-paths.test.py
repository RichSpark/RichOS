#!/usr/bin/env python3
"""G9 write guards: actual redirects and physical, cwd-relative targets."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class WritePaths(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='g9-writes-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / 'src/sub').mkdir(parents=True)
        (self.root / 'src/file').write_text('unchanged')
        (self.root / 'orchestration.config').write_text('PROTECTED_PATHS="src"\n')
        self.work = self.root / '.claude/worktrees/worker'
        (self.work / 'src').mkdir(parents=True)
        self.alias = self.root / '.claude/worktrees/alias'
        self.alias.symlink_to(self.root, target_is_directory=True)
        self.env = dict(os.environ, RICHOS_ENTITY_ROOT=str(self.root))

    def verdict(self, command, expected, cwd=None, hook='guard-bash-main-writes.sh', **inputs):
        payload = {'tool_name': 'Bash', 'tool_input': dict(command=command, **inputs),
                   'cwd': str(cwd or self.root)}
        if hook == 'guard-main-checkout-writes.sh':
            payload['tool_name'] = 'Write'
        p = subprocess.run(['/bin/bash', str(HERE / hook)], input=json.dumps(payload),
                           text=True, capture_output=True, env=self.env, timeout=60)  # load-bound: hang guard only; verdict uses exit and file identity, never elapsed time.
        self.assertEqual(p.returncode, expected, f'{command}\n{p.stdout}\n{p.stderr}')
        self.assertEqual((self.root / 'src/file').read_text(), 'unchanged')

    def test_39_printed_and_commented_redirects(self):
        for command in (f"printf '%s\\n' '> {self.root}/src/file'",
                        f'true # > {self.root}/src/file',
                        f'printf "> {self.root}/src/file"',
                        f'echo \\> {self.root}/src/file'):
            with self.subTest(command=command):
                self.verdict(command, 0)
        self.verdict(f'printf hi > {self.root}/src/file', 2)
        self.verdict('cat src/file 2>&1', 0)
        self.verdict('cat src/file >&2', 0)

    def test_32_physical_aliases(self):
        for target in (self.root / 'src/file', self.alias / 'src/file'):
            with self.subTest(target=target):
                self.verdict(f'touch {target}', 2)
                self.verdict('', 2, hook='guard-main-checkout-writes.sh', file_path=str(target))
        self.verdict(f'touch {self.work}/src/new', 0)

    def test_41_relative_and_quoted_targets(self):
        for command, cwd in (
                ('touch ./src/file', self.root),
                ('touch ../file', self.root / 'src/sub'),
                (f'printf hi > "{self.root}/src/file"', self.root),
                ('printf hi > ./src/file', self.root),
                ('printf hi > "../file"', self.root / 'src/sub'),
                (f'cd "{self.root}/src/sub" && touch ../file', self.work),
                (f'cd "{self.root}/src/sub"\nprintf hi > ../file', self.work)):
            with self.subTest(command=command, cwd=cwd):
                self.verdict(command, 2, cwd)
        self.verdict(f'cd {self.work} && touch src/new', 0)
        self.verdict(f'cd {self.work} && echo hi > src/new', 0)
        self.verdict(f'printf "cd {self.work}"; touch src/file', 2)
        self.verdict(f'cd {self.work} && cd {self.root} && touch src/file', 2)
        self.verdict('touch ../../notes', 0, self.root / 'src/sub')
        self.verdict('cp src/file ./notes', 0)
        self.verdict('cp ./notes src/file', 2)


if __name__ == '__main__':
    unittest.main(verbosity=2)

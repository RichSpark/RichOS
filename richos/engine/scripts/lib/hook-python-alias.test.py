#!/usr/bin/env python3
"""P3-51: executable identity, without ever running an Apple stub."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class StubAliases(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='g9-stubs-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.stubs = self.root / 'usr/bin'
        self.stubs.mkdir(parents=True)
        self.marker = self.root / 'stub-ran'
        for name in ('python3', 'make'):
            p = self.stubs / name
            p.write_text('#!/bin/sh\necho stub >> "$FAKE_MARKER"\nexit 1\n')
            p.chmod(0o755)
        self.alias = self.root / 'alias'
        self.alias.symlink_to(self.stubs, target_is_directory=True)
        self.links = self.root / 'links'
        self.links.mkdir()
        for name in ('python3', 'make'):
            (self.links / name).symlink_to(self.stubs / name)
        self.hard = self.root / 'hard'
        self.hard.mkdir()
        for name in ('python3', 'make'):
            os.link(self.stubs / name, self.hard / name)
        self.env = dict(os.environ, RICHOS_TOOLCHAIN_ROOT=str(self.root),
                        FAKE_MARKER=str(self.marker), DEVELOPER_DIR='',
                        RICHOS_TOOLCHAIN_STATE=str(self.root / 'state'))
        self.env.pop('BASH_ENV', None)
        self.env.pop('_HOOK_PYTHON_SOURCED', None)
        self.env.pop('RICHOS_HOOK_PYTHON', None)
        self.env.pop('BASH_FUNC_python3%%', None)

    def test_bootstrap_never_executes_alias(self):
        for folder in (self.stubs, self.alias, self.links, self.hard, self.stubs / '.'):
            with self.subTest(folder=folder):
                env = dict(self.env, PATH=str(folder) + ':/bin')
                p = subprocess.run(['/bin/bash', '-c', '. "$1"; python3 -V',
                                    'bootstrap', str(HERE / 'hook-python.sh')],
                                   env=env, capture_output=True, text=True, timeout=60)  # load-bound: hang guard only; verdict uses exit and file identity, never elapsed time.
                self.assertFalse(self.marker.exists(), p.stderr)
                self.assertEqual(p.returncode, 127, p.stderr)

    def test_real_python_after_alias_is_used(self):
        real = self.root / 'real'
        real.mkdir()
        (real / 'python3').symlink_to(sys.executable)
        env = dict(self.env, PATH=f'{self.alias}:{real}:/bin')
        p = subprocess.run(['/bin/bash', '-c', '. "$1"; python3 -c "print(73)"',
                            'bootstrap', str(HERE / 'hook-python.sh')],
                           env=env, capture_output=True, text=True, timeout=60)  # load-bound: hang guard only; verdict uses exit and file identity, never elapsed time.
        self.assertFalse(self.marker.exists(), p.stderr)
        self.assertEqual((p.returncode, p.stdout.strip()), (0, '73'), p.stderr)

    def test_tool_guard_refuses_stub_aliases(self):
        # Real interpreter for the guard, sentinel make as the user's command.
        real = self.root / 'real'
        real.mkdir()
        (real / 'python3').symlink_to(sys.executable)
        for folder in (self.stubs, self.alias, self.links, self.hard):
            for command in ('make', str(folder / 'make')):
                with self.subTest(folder=folder, command=command):
                    env = dict(self.env, PATH=f'{real}:{folder}:/bin')
                    p = subprocess.run(['/bin/bash', str(HERE.parent / 'hooks/guard-toolchain.sh')],
                                       input=json.dumps({'tool_name': 'Bash', 'tool_input': {'command': command}}),
                                       env=env, capture_output=True, text=True, timeout=60)  # load-bound: hang guard only; verdict uses exit and file identity, never elapsed time.
                    self.assertFalse(self.marker.exists(), p.stderr)
                    self.assertEqual(p.returncode, 2, p.stderr)

    def test_renamed_stub_is_recognized(self):
        spec = importlib.util.spec_from_file_location('g9_toolchain', HERE / 'toolchain.py')
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        old = os.environ.get('RICHOS_TOOLCHAIN_ROOT')
        os.environ['RICHOS_TOOLCHAIN_ROOT'] = str(self.root)
        self.addCleanup(lambda: os.environ.pop('RICHOS_TOOLCHAIN_ROOT', None) if old is None
                        else os.environ.__setitem__('RICHOS_TOOLCHAIN_ROOT', old))
        renamed = self.root / 'builder'
        renamed.symlink_to(self.stubs / 'make')
        self.assertEqual(mod.stub_name(str(renamed), self.env), 'make')
        other_launcher = self.root / 'objdump'
        os.link(self.stubs / 'make', other_launcher)
        self.assertEqual(mod.stub_name(str(other_launcher), self.env), 'objdump')
        real = self.root / 'make'
        real.write_text('#!/bin/sh\nexit 0\n')
        real.chmod(0o755)
        self.assertIsNone(mod.stub_name(str(real), self.env))


if __name__ == '__main__':
    unittest.main(verbosity=2)

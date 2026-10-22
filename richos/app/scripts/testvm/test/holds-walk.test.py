#!/usr/bin/env python3
"""holds-walk.py and holds-guest.py without a guest (no silent holds, 2026-10-22).

The guest helper is run here exactly as the walk runs it in the guest, on a fixture repository and
registry in a temporary directory: the engine's own turn-end lines must name both held pieces of
work. The walk's pass rule (ask_verdict) is driven with the shapes a guest returns."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
ENGINE = HERE.parent.parent.parent / 'engine'
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True

_spec = importlib.util.spec_from_file_location('holds_walk', HERE / 'holds-walk.py')
holds_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(holds_walk)

LINES = ('HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 23:49Z (40 min). Why: ...',
         'HELD BY RICH, NOT BY YOU: the work of spelling-mate since 23:49Z (40 min). Why: ...')


class GuestHelper(unittest.TestCase):
    def test_it_files_a_wait_and_an_unacted_correction_that_the_engine_names(self):
        with tempfile.TemporaryDirectory(prefix='holds-walk ') as root:
            repo, ws = os.path.join(root, 'repo'), os.path.join(root, 'ws')
            subprocess.run(['git', 'init', '-q', repo], check=True)
            ident = [] if subprocess.run(['git', 'config', '--get', 'user.email'], capture_output=True,
                                         text=True).stdout.strip() else \
                ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid']
            subprocess.run(['git', *ident, '-C', repo, 'commit', '-q', '--allow-empty', '-m', 'init'], check=True)
            env = {k: v for k, v in os.environ.items() if k != 'RICHOS_WORKSPACES_DIR'}
            out = subprocess.run([sys.executable, str(HERE / 'holds-guest.py'), 'file', str(ENGINE), ws, repo, '40'],
                                 capture_output=True, text=True, env=env, timeout=120)
            self.assertEqual(out.returncode, 0, out.stderr)
            filed = json.loads(out.stdout.strip().splitlines()[-1])
            lines = filed['lines']
            for name in holds_walk.NAMES:
                self.assertTrue(any(holds_walk.MARK in l and name in l for l in lines), lines)
            self.assertTrue(any('code done, real-app checks left' in l for l in lines), lines)
            self.assertTrue(any('its run ended before it acted on it' in l for l in lines), lines)


class Verdict(unittest.TestCase):
    def test_a_reply_naming_both_in_plain_words_passes(self):
        context = ['{"attachment": {"type": "hook_additional_context", "content": ["%s\\n%s"]}}' % LINES]
        reply = ('Morning. Two things are on hold that are your call: the spelling work still needs its '
                 'real-app checks, and the correction I sent about the sign-in status was never acted on.')
        self.assertEqual(holds_walk.ask_verdict('t1', True, 's1', context, reply, 420), '')

    def test_each_missing_proof_is_named(self):
        context = ['%s %s' % LINES]
        self.assertIn('did not finish', holds_walk.ask_verdict(None, False, None, [], '', 420))
        self.assertIn('no context', holds_walk.ask_verdict('t1', True, 's1', [], 'spelling and correction', 420))
        self.assertIn("does not name ['fix-mate']",
                      holds_walk.ask_verdict('t1', True, 's1', context, 'Spelling-mate is waiting.', 420))

    def test_ledger_times_in_seconds_or_milliseconds(self):
        self.assertEqual(holds_walk.ms(1792629000), 1792629000000)
        self.assertEqual(holds_walk.ms(1792629000123), 1792629000123)


if __name__ == '__main__':
    unittest.main(verbosity=2)

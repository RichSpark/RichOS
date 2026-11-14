#!/usr/bin/env python3
"""release-lines-walk.py's judgment of a dictation and its argument refusals, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('release_lines_walk', HERE / 'release-lines-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

SAID = ('Starting on March first, the price will go up by eight percent. Orders under two hundred '
        'dollars will pay a flat fee. Please reply if you would like a printed copy.')


class WordGaps(unittest.TestCase):
    def test_a_faithful_dictation_has_no_gap_and_nothing_added(self):
        written = 'Starting on March 1, the price will go up by 8%. Orders under $200 will pay a flat fee. ' \
                  'Please reply if you would like a printed copy.'
        drop, extra = walk.word_gaps(SAID, written)
        self.assertLess(len(drop), 3, drop)
        self.assertEqual(extra, [])

    def test_three_spoken_words_in_a_row_missing_are_found(self):
        written = 'Starting on March first, the price will go up by eight percent. Please reply if you would like a printed copy.'
        drop, _ = walk.word_gaps(SAID, written)
        self.assertGreaterEqual(len(drop), 3)
        self.assertIn('orders', drop)

    def test_words_nobody_said_at_the_end_are_found(self):
        drop, extra = walk.word_gaps(SAID, SAID + ' Thank you.')
        self.assertEqual(extra, ['thank', 'you'])


class Refusals(unittest.TestCase):
    def run_walk(self, *args):
        return subprocess.run([sys.executable, str(HERE / 'release-lines-walk.py'), 'vm', '--out', '/nonexistent', '--expect-sha', 'x',
                               *args], capture_output=True, text=True, timeout=60)

    def test_an_unknown_step_is_refused(self):
        r = self.run_walk('--steps', 'identity,nosuchstep')
        self.assertEqual(r.returncode, 2)
        self.assertIn('unknown step(s): nosuchstep', r.stderr)

    def test_the_dictation_steps_refuse_without_their_samples(self):
        r = self.run_walk('--steps', 'scribe-stage', '--model', '/dev/null')
        self.assertEqual(r.returncode, 2)
        self.assertIn('--dict-wav, --dict-lines, --post-key and --key-probe are needed', r.stderr)

    def test_the_meeting_question_refuses_without_the_meeting(self):
        r = self.run_walk('--steps', 'ask-live')
        self.assertEqual(r.returncode, 2)
        self.assertIn('--meeting and --meeting-lines are needed', r.stderr)


if __name__ == '__main__':
    unittest.main()

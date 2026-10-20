#!/usr/bin/env python3
"""first-send-walk.py's verdict, offline. The live walk is its own proof.

The swallowed record is the shape esc-20261020T171021Z-8fd0fca7 measured: the words were in the
composer, the send was made, the composer emptied and no PromptReceived row carried them.
"""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('first_send_walk', HERE / 'first-send-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

WORDS = walk.WORDS['return']


def record(**over):
    value = {'words': WORDS, 'typed': WORDS, 'within': 30, 'composer_after': '',
             'received': {'turn_id': 't1', 'thread_id': 'thr', 'at': 1}}
    value.update(over)
    return value


class Verdict(unittest.TestCase):
    def test_a_send_the_ledger_received_passes(self):
        self.assertIsNone(walk.verdict(record()))

    def test_the_escalations_record_is_named_swallowed(self):
        self.assertTrue(walk.verdict(record(received=None)).startswith('SWALLOWED'))

    def test_words_left_in_the_box_are_not_sent_and_not_swallowed(self):
        said = walk.verdict(record(received=None, composer_after=WORDS))
        self.assertTrue(said.startswith('not sent'), said)

    def test_keys_that_never_reached_the_composer_fail_before_anything_else(self):
        said = walk.verdict(record(typed=''))
        self.assertIn('did not reach the composer', said)


class TwoSends(unittest.TestCase):
    """The `two` step: both messages pressed before the conversation opened reach Rich, in order."""

    def two(self, **over):
        value = {'within': 120, 'conversation_open_when_sent': False, 'received_order': list(walk.TWO),
                 'composer_after': ''}
        value.update(over)
        return value

    def test_both_received_in_order_passes(self):
        self.assertIsNone(walk.two_verdict(self.two()))

    def test_the_reviewed_defect_a_correction_left_in_the_box_fails(self):
        said = walk.two_verdict(self.two(received_order=[walk.TWO[0]], composer_after=walk.TWO[1]))
        self.assertTrue(said.startswith('not received'), said)
        self.assertIn('Correction', said)

    def test_reversed_order_fails(self):
        said = walk.two_verdict(self.two(received_order=list(reversed(walk.TWO))))
        self.assertTrue(said.startswith('OUT OF ORDER'), said)

    def test_sent_after_the_conversation_opened_tested_nothing(self):
        for opened in (True, None):
            said = walk.two_verdict(self.two(conversation_open_when_sent=opened))
            self.assertTrue(said.startswith('did not test the window'), said)


class Keys(unittest.TestCase):
    def test_a_quote_and_a_backslash_are_typed_as_written(self):
        self.assertEqual(walk.keystrokes('a "b" \\c'),
                         'tell application "System Events" to keystroke "a \\"b\\" \\\\c"')

    def test_the_click_point_is_the_middle_of_the_control(self):
        self.assertEqual(walk.center({'x': 1262, 'y': 878, 'w': 40, 'h': 47}), '1282,901')


if __name__ == '__main__':
    unittest.main()

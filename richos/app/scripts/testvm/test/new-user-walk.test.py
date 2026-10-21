#!/usr/bin/env python3
"""new-user-walk.py's verdicts, offline: every dead end a new user hit on 1.2.1 (2026-10-21) fails
the step that shows it, and a screen without one passes. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('new_user_walk', HERE / 'new-user-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

# The sentences his screens showed, as the app wrote them (screenshots of 2026-10-21).
LOST = ('RichOS lost its connection to Claude Code while Rich was answering, so his answer is '
        'incomplete. This usually fixes itself. Press Restore my message, then send it again.')
OS_ERROR = 'Resource temporarily unavailable (os error 35)'
NO_READING = 'No reading yet. If this account is not signed in on this Mac, sign in here.'
SIGN_IN = ("RichOS couldn't send this to Rich because no Claude account is signed in on this Mac or the "
           'sign-in has expired. Open Settings, choose Claude accounts and sign in, then send your message again.')


def rows(words, *events):
    out = [{'event': 'PromptReceived', 'turn_id': 't1', 'text': words}]
    out += [dict(e, turn_id='t1') for e in events]
    return out


class DeadEnds(unittest.TestCase):
    def test_every_dead_end_he_saw_is_named(self):
        self.assertEqual(walk.dead_ends(LOST), ['retry loop on a permanent cause'])
        self.assertEqual(walk.dead_ends(OS_ERROR), ['raw OS error'])
        self.assertEqual(walk.dead_ends(NO_READING), ['account state unknown'])
        self.assertEqual(walk.dead_ends('control_request\nsystem:init\noutcome not recorded'),
                         ['machinery in the user view'])
        self.assertEqual(walk.dead_ends('Not logged in · Please run /login'), ["machine words as Rich's words"])

    def test_the_way_out_is_not_a_dead_end(self):
        self.assertEqual(walk.dead_ends(SIGN_IN), [])
        self.assertEqual(walk.dead_ends('RichOS 1.2.1 is up to date.\nChecked just now.'), [])


class ScreenLines(unittest.TestCase):
    """The first guest run read ocr-find's closing line, which carries the pattern, as screen text
    and found every dead end on a frame that showed none."""
    OUT = ('HIT  /x/signed-out.png\n'
           '       Onboarding and sign in, then send your message again.\n'
           '--- 1 of 1 frame(s) match /os error [0-9]+|usually fixes itself|control_request|No reading yet|sign in/ ---\n')

    def test_only_the_frames_own_words(self):
        self.assertEqual(walk.screen_lines(self.OUT), 'Onboarding and sign in, then send your message again.')
        self.assertEqual(walk.dead_ends(walk.screen_lines(self.OUT)), [])

    def test_a_miss_says_nothing(self):
        self.assertEqual(walk.screen_lines('MISS /x/a.png\n--- 0 of 1 frame(s) match /os error/ ---\n'), '')


class TurnOutcome(unittest.TestCase):
    def test_still_running_is_none(self):
        self.assertIsNone(walk.turn_outcome(rows('hi'), 'hi'))
        self.assertIsNone(walk.turn_outcome(rows('hi'), 'another message'))

    def test_interrupted_with_its_cause(self):
        out = walk.turn_outcome(rows('hi', {'event': 'TurnInterrupted', 'reason': 'cognition io: x'},
                                     {'event': 'TurnInterruptionCause', 'record': {'cause': 'transient', 'ceo_message': LOST}}), 'hi')
        self.assertEqual((out['state'], out['cause'], out['ceo_message']), ('interrupted', 'transient', LOST))
        self.assertNotIn('rich_wrote', out)

    def test_completed_with_his_text(self):
        out = walk.turn_outcome(rows('hi', {'event': 'AssistantDelta', 'text': 'received'}, {'event': 'TurnCompleted'}), 'hi')
        self.assertEqual((out['state'], out['rich_wrote']), ('completed', True))


class SignedOutVerdict(unittest.TestCase):
    OK = {'state': 'interrupted', 'cause': 'not-signed-in'}

    def test_his_loop_fails(self):
        why = walk.signed_out_verdict({'state': 'interrupted', 'cause': 'transient', 'reason': 'cognition io: closed'},
                                      walk.dead_ends(LOST), LOST)
        self.assertIn('not as "not signed in"', why)

    def test_a_never_ending_turn_fails(self):
        self.assertEqual(walk.signed_out_verdict(None, [], ''), 'the turn never ended')

    def test_a_harness_login_that_leaked_in_fails(self):
        self.assertIn('leaked', walk.signed_out_verdict({'state': 'completed'}, [], ''))

    def test_the_vendor_line_as_his_words_fails(self):
        self.assertIn("Rich's answer", walk.signed_out_verdict(dict(self.OK, rich_wrote=True), [], SIGN_IN))

    def test_a_dead_end_beside_the_right_cause_fails(self):
        self.assertIn('account state unknown', walk.signed_out_verdict(self.OK, walk.dead_ends(NO_READING), SIGN_IN + NO_READING))

    def test_no_way_out_on_screen_fails(self):
        self.assertIn('way out', walk.signed_out_verdict(self.OK, [], 'Stopped'))

    def test_the_right_ending_passes(self):
        self.assertIsNone(walk.signed_out_verdict(self.OK, walk.dead_ends(SIGN_IN), SIGN_IN))


class UpdateVerdict(unittest.TestCase):
    def test_his_update_dead_end_fails(self):
        self.assertIn('raw OS error', walk.update_verdict('RichOS could not prepare this update.', walk.dead_ends(OS_ERROR)))
        self.assertIn('could not prepare', walk.update_verdict('RichOS could not prepare this update.', []))

    def test_silence_fails(self):
        self.assertEqual(walk.update_verdict(None, []), 'the update row says nothing')

    def test_a_verdict_passes(self):
        self.assertIsNone(walk.update_verdict('RichOS 1.2.1 is up to date.', []))


if __name__ == '__main__':
    unittest.main()

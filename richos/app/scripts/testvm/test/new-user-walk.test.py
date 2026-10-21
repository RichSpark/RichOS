#!/usr/bin/env python3
"""new-user-walk.py's verdicts, offline: every dead end a new user hit on 1.2.1 (2026-10-21) fails
the step that shows it, and a screen without one passes. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import re
import subprocess
import tempfile
import types
import unittest
from unittest import mock

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


class FrameReadsEveryLine(unittest.TestCase):
    """A dead end anywhere on a frame fails its step (review rv-20261021T153457Z-fb99f171-52f1,
    finding 1). ocr-find's `--show` prints at most three matching lines per frame, so a frame read
    through it with three "sign in" lines above `control_request` passed. The stand-in below
    answers as both OCR tools do, so the walk's own frame() decides which one it reads."""
    SCREEN = '\n'.join(['Open Settings and sign in'] * 3 + ['Sign in here', 'control_request', 'system:init'])

    def fake_run(self, argv, **_):
        argv = [str(a) for a in argv]
        if any(a.endswith('ocr-read.py') for a in argv):
            out = self.SCREEN
        else:  # ocr-find.sh <pattern> <png> --show: its own format, three matching lines at most
            pattern = re.compile(argv[1], re.I)
            hits = [l for l in self.SCREEN.splitlines() if pattern.search(l)][:3]
            out = 'HIT  %s\n%s\n--- 1 of 1 frame(s) match /%s/ ---\n' % (
                argv[2], '\n'.join('       ' + h for h in hits), argv[1])
        return subprocess.CompletedProcess(argv, 0, out, '')

    def test_a_dead_end_below_three_harmless_lines_is_seen(self):
        with tempfile.TemporaryDirectory() as tmp:
            stub = types.SimpleNamespace(shot=lambda name: None, out=Path(tmp))
            with mock.patch.object(walk.subprocess, 'run', self.fake_run):
                text = walk.NewUserWalk.frame(stub, 'signed-out-1')
        self.assertIn('machinery in the user view', walk.dead_ends(text))
        self.assertIn('sign in', text)


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
    FAILED = 'RichOS could not prepare this update.'

    def test_his_update_dead_end_fails(self):
        self.assertIn('raw OS error', walk.update_verdict(self.FAILED, 'You are still running RichOS 1.2.1.',
                                                          walk.dead_ends(OS_ERROR)))
        self.assertIn('could not prepare', walk.update_verdict(self.FAILED, 'You are still running RichOS 1.2.1.', []))

    def test_silence_fails(self):
        self.assertEqual(walk.update_verdict(None, None, []), 'the update row says nothing')

    def test_a_check_still_running_is_never_a_pass(self):
        # Review rv-20261021T153457Z-fb99f171-52f1, finding 2: while it checks, ui/updates.js shows
        # the bare version as the headline and "Checking for updates…" under it.
        for line, sub in (('RichOS 1.2.1', 'Checking for updates…'), ('RichOS 1.2.1', 'RichOS has not checked for updates yet.'),
                          ('Downloading RichOS 1.2.2…', '3 MB so far'),
                          ('Checking and preparing the update…', 'RichOS is confirming this update was signed by us.')):
            self.assertFalse(walk.update_settled(line, sub), (line, sub))
            self.assertIn('never reached a verdict', walk.update_verdict(line, sub, []), (line, sub))

    def test_a_verdict_passes(self):
        self.assertTrue(walk.update_settled('RichOS 1.2.1 is up to date.', 'Checked just now.'))
        self.assertIsNone(walk.update_verdict('RichOS 1.2.1 is up to date.', 'Checked just now.', []))
        self.assertIsNone(walk.update_verdict('RichOS 1.2.2 is available.', 'You are running 1.2.1.', []))


if __name__ == '__main__':
    unittest.main()

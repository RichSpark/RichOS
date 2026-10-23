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

    def test_a_dead_end_wrapped_across_lines_is_still_one(self):
        # Review rv-20261021T193611Z-6df3f1f3-b313, finding 1: OCR keeps the screen's line breaks.
        for text, name in (('No reading\nyet. If this account', 'account state unknown'),
                           ('This usually fixes\n  itself. Press Restore', 'retry loop on a permanent cause'),
                           ('Please run\n/login', "machine words as Rich's words"),
                           ('Resource temporarily\nunavailable (os error\n35)', 'raw OS error'),
                           ('outcome not\nrecorded', 'machinery in the user view')):
            self.assertIn(name, walk.dead_ends(text), repr(text))

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
            stub = types.SimpleNamespace(shot=lambda name: None, out=Path(tmp), seen=[])
            with mock.patch.object(walk.subprocess, 'run', self.fake_run):
                text = walk.NewUserWalk.frame(stub, 'signed-out-1')
        self.assertIn('machinery in the user view', walk.dead_ends(text))
        self.assertIn('sign in', text)
        self.assertEqual(stub.seen, [('signed-out-1', ['machinery in the user view'])], 'every frame is recorded')


class EveryFrameEveryDeadEnd(unittest.TestCase):
    """Review rv-20261021T183543Z-ad01d021-62a5: the update step kept only 'raw OS error' hits, so
    a frame it took showing 'Between turns', 'No reading yet', 'Please run /login' or 'This usually
    fixes itself.' passed. run_step judges every frame a step captured against every dead end."""

    def walk_with(self, screen, step_fails=False):
        stub = types.SimpleNamespace(seen=[('before', [])])

        def update():
            stub.seen.append(('update', walk.dead_ends(screen)))
            if step_fails:
                raise walk.StepFailed('the update check failed: RichOS could not prepare this update.')
            return {'line': 'RichOS 1.2.2 is up to date.'}
        stub.update = update
        return stub

    def test_any_dead_end_on_any_frame_of_a_step_fails_it(self):
        for screen in ('Between turns', NO_READING, 'Not logged in · Please run /login', LOST, OS_ERROR):
            row = walk.run_step(self.walk_with('Updates\nRichOS 1.2.2 is up to date.\n' + screen), 'update')
            self.assertEqual(row['outcome'], 'FAIL', screen)
            self.assertIn('dead end on screen: update:', row['detail'], screen)

    def test_a_clean_frame_passes_and_frames_of_other_steps_are_not_charged(self):
        stub = self.walk_with('Updates\nRichOS 1.2.2 is up to date.')
        stub.seen = [('accounts', ['account state unknown'])]  # an earlier step's frame
        self.assertEqual(walk.run_step(stub, 'update')['outcome'], 'PASS')

    def test_a_failed_step_keeps_its_own_reason_and_records_the_frames(self):
        row = walk.run_step(self.walk_with('Between turns', step_fails=True), 'update')
        self.assertEqual(row['outcome'], 'FAIL')
        self.assertIn('could not prepare', row['detail'])
        self.assertEqual(row['frame_dead_ends'], 'update: machinery in the user view')


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


class AccountsVerdict(unittest.TestCase):
    def test_the_browser_check_keeps_only_provider_host_and_path(self):
        self.assertEqual(walk.provider_page('https://claude.ai/oauth/authorize?state=private&code=secret'),
                         {'host': 'claude.ai', 'path': '/oauth/authorize'})
        for url in ['http://claude.ai/login', 'https://claude.ai.evil.invalid/login',
                    'https://unrelated.invalid/login', 'file:///private/login']:
            self.assertIsNone(walk.provider_page(url), url)

    def test_real_sign_in_is_pressed_cancelled_and_retried_before_the_projected_login(self):
        clicks = []
        stub = types.SimpleNamespace(vm='guest', by_id=lambda _: {'enabled': True},
            ax=lambda *args: clicks.append(args),
            frame=lambda _: 'ign-in was canceled. You can try again when you are ready.',
            shows_text=lambda text: text == 'Sign-in was canceled')
        running = iter([False, True, False, False, True, False])
        stub.login_running = lambda: next(running)
        pages = iter([('https://claude.ai/oauth?state=first', {'host': 'claude.ai', 'path': '/oauth'}),
                      ('https://claude.ai/oauth?state=second', {'host': 'claude.ai', 'path': '/oauth'})])
        stub.browser_page = lambda: next(pages)
        with mock.patch.object(walk, 'guest', return_value=''):
            result = walk.NewUserWalk.sign_in_browser(stub)
        self.assertEqual([c[2] for c in clicks], ['acc-sign-one', 'provider-connect', 'provider-cancel',
                                                'provider-connect', 'provider-cancel', 'setup-close'])
        self.assertEqual(len(result['attempts']), 2)
        self.assertFalse(result['external_authentication_completed'])
        self.assertNotIn('state=', str(result))

    def test_cancellation_needs_the_actual_notice_even_when_ocr_says_it(self):
        import itertools
        running = itertools.chain([False, True], itertools.repeat(False))
        stub = types.SimpleNamespace(vm='guest', by_id=lambda _: {'enabled': True},
            ax=lambda *args: None, shows_text=lambda _: False,
            login_running=lambda: next(running),
            browser_page=lambda: ('https://claude.ai/login', {'host': 'claude.ai', 'path': '/login'}),
            frame=lambda _: 'Sign-in was canceled.')
        with mock.patch.object(walk, 'guest', return_value=''), \
             mock.patch.object(walk.time, 'monotonic', side_effect=itertools.count(0, 10)), \
             mock.patch.object(walk.time, 'sleep'):
            with self.assertRaisesRegex(walk.StepFailed, 'explain the cancellation'):
                walk.NewUserWalk.sign_in_browser(stub)

    def test_a_failed_browser_step_closes_its_modal_and_settings(self):
        clicks, settings = [], []
        def fail():
            raise walk.StepFailed('fixture browser failure')
        stub = types.SimpleNamespace(vm='guest', report={}, sign_in_browser=fail,
            settings=lambda want, name: settings.append(want),
            ax=lambda *args: clicks.append(args), frame=lambda _: self.CARD,
            by_id=lambda name: {'enabled': True, 'title': 'Sign in'} if name != 'provider-cancel' else None)
        with mock.patch.object(walk.time, 'sleep'), mock.patch.object(walk, 'guest', return_value='') as remote:
            with self.assertRaisesRegex(walk.StepFailed, 'fixture browser failure'):
                walk.NewUserWalk.accounts(stub)
        self.assertIn(('click', '--id', 'setup-close', '--first'), clicks)
        self.assertEqual(settings, [True, False])
        self.assertIn('activate', remote.call_args.args[1])

    def test_a_button_without_a_provider_browser_journey_cannot_pass(self):
        stub = types.SimpleNamespace(vm='guest', by_id=lambda _: {'enabled': True},
            ax=lambda *args: None, login_running=lambda: False, browser_page=lambda: (None, None))
        import itertools
        with mock.patch.object(walk.time, 'monotonic', side_effect=itertools.count(0, 100)):
            with self.assertRaisesRegex(walk.StepFailed, 'did not start provider login'):
                walk.NewUserWalk.sign_in_browser(stub)

    def test_a_provider_address_without_its_sign_in_controls_is_not_browser_proof(self):
        address = {'value': 'https://claude.ai/login?state=private', 'desc': 'Address and Search'}
        for controls, accepted in [([{'title': 'Reload Page'}], False),
                                   ([{'title': 'Continue with Google'}], True)]:
            stub = types.SimpleNamespace(vm='guest', ax=lambda mode, *args, **kwargs:
                                         controls if args[1] == 'AXButton' else [address])
            with mock.patch.object(walk, 'guest', return_value=address['value']):
                raw, safe = walk.NewUserWalk.browser_page(stub)
            self.assertEqual(bool(raw), accepted)
            self.assertEqual(safe, {'host': 'claude.ai', 'path': '/login'} if accepted else None)

    def test_browser_proof_uses_the_current_tab_instead_of_a_stale_ax_address(self):
        stale = {'value': 'https://claude.ai/login?state=old', 'desc': 'Enter your email'}
        stub = types.SimpleNamespace(vm='guest', ax=lambda *args, **kwargs: [stale])
        fresh = 'https://claude.ai/login?state=new'
        with mock.patch.object(walk, 'guest', return_value=fresh) as remote:
            raw, safe = walk.NewUserWalk.browser_page(stub)
        self.assertEqual(raw, fresh)
        self.assertEqual(safe, {'host': 'claude.ai', 'path': '/login'})
        self.assertIn('current tab of front window', remote.call_args.args[1])
        with mock.patch.object(walk, 'guest', return_value='https://unrelated.invalid/login'):
            self.assertEqual(walk.NewUserWalk.browser_page(stub), (None, None))

    def test_settings_is_read_after_activating_the_owned_app(self):
        events = []
        stub = types.SimpleNamespace(vm='guest', frame=lambda _: events.append('frame') or 'closed')
        with mock.patch.object(walk, 'guest', side_effect=lambda *args: events.append('activate')):
            walk.NewUserWalk.settings(stub, False, 'closed')
        self.assertEqual(events, ['activate', 'frame'])

    def test_his_card_fails(self):
        self.assertIn('account state unknown', walk.accounts_verdict(
            'Your Claude account In use now\n' + NO_READING + '\nWeekly limit Not reported yet'))

    def test_a_card_that_never_says_signed_out_fails(self):
        self.assertIn('does not say', walk.accounts_verdict('Your Claude account In use now\nChecking whether this account is signed in on this Mac.',
                                                                   self.SHEET, self.SIGN))

    CARD = ('Your Claude account Not signed in Sign in\n'
            'This account is not signed in on this Mac, so Rich cannot answer you yet. Sign in here.')
    SHEET = {'role': 'AXButton', 'desc': 'Close Claude accounts', 'enabled': True}
    SIGN = {'role': 'AXButton', 'title': 'Sign in', 'enabled': True}

    def test_the_signed_out_card_passes(self):
        self.assertIsNone(walk.accounts_verdict(self.CARD, self.SHEET, self.SIGN))

    def test_sign_in_words_without_the_sheet_or_its_button_fail(self):
        # Review rv-20261021T213648Z-b3f0990a-85f9, finding 2: Settings' row says "Not signed in
        # on this Mac" and the interruption before it says "sign in", so text alone passed with
        # no account card and no button.
        words = ('Settings Claude accounts Not signed in on this Mac\n'
                 'Rich could not answer because Claude Code is not signed in. Sign in in Settings.')
        self.assertIn('did not open', walk.accounts_verdict(words, None, None))
        self.assertIn('no Sign in button', walk.accounts_verdict(words, self.SHEET, None))
        self.assertIn('cannot be pressed', walk.accounts_verdict(self.CARD, self.SHEET, dict(self.SIGN, enabled=False)))
        self.assertIn('does not say Sign in', walk.accounts_verdict(self.CARD, self.SHEET, {'role': 'AXButton', 'title': 'Remove'}))

    def test_the_step_is_walked_between_the_signed_out_messages_and_the_update(self):
        self.assertEqual(walk.STEPS, ['identity', 'setup', 'signed-out', 'accounts', 'update', 'signed-in'])


class ReadsTheScreenAsAPersonDoes(unittest.TestCase):
    """The guest walk of 2026-10-21 on the fixed build: the update row's accessibility value was
    empty while the frame said "RichOS 1.2.2 is up to date.", and Settings stayed open after
    Escape, so the signed-in message was typed into nothing. Both are read off the frame now."""
    FRAME = ('Claude accounts\nNot signed in on this Mac\nMemory folder\nUpdates\nRichOS 1.2.2 is up to date.\n'
             'Checked 3 minutes ago.\n{ Check for updates |\nHOW IT LOOKS\nTheme\n')

    def test_the_update_row_is_read_off_the_frame(self):
        self.assertEqual(walk.update_row(self.FRAME), ('RichOS 1.2.2 is up to date.', 'Checked 3 minutes ago.'))
        self.assertTrue(walk.update_settled(*walk.update_row(self.FRAME)))
        self.assertEqual(walk.update_row('Updates\nRichOS 1.2.2\nChecking for updates…\n'),
                         ('RichOS 1.2.2', 'Checking for updates…'))
        self.assertFalse(walk.update_settled(*walk.update_row('Updates\nRichOS 1.2.2\nChecking for updates…\n')))
        self.assertEqual(walk.update_row('no row here'), (None, None))

    def test_a_wrapped_headline_is_one_headline(self):
        # Nightly 71's new-user walk of 2026-10-23 (update.txt): the frame showed the row whole,
        # its OCR wrapped the headline, and the walk read "... is up to" / "date." as no verdict.
        frame = ('Memory folder >\nUpdates\n\nRichOS 1.2.2-nightly.20261023.71 is up to\ndate.\n\n'
                 'Checked just now.\n{ Check for updates |\n\nSee what\'s new 7\n')
        self.assertEqual(walk.update_row(frame),
                         ('RichOS 1.2.2-nightly.20261023.71 is up to date.', 'Checked just now.'))
        self.assertIsNone(walk.update_verdict(*walk.update_row(frame), []))

    def test_whether_settings_is_open_is_read_off_the_frame(self):
        self.assertTrue(walk.settings_open(self.FRAME))
        self.assertFalse(walk.settings_open('RV Park / Onboarding\nTalk to Rich...\nhow it looks to me'))


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

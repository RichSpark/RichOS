#!/usr/bin/env python3
"""adopt-walk.py's verdict, offline. The live walk is its own proof.

The rows below are the shape adopt-watch.py writes, with the timings of the 2026-09-26 walk
(bundle 1.2.0-dev.98c3aef5): the phone's words at +0, the task registered inside the turn at
+11 446 ms, the turn completed at +13 247, the assignment Preparing 8 ms later.
"""
import importlib.util
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent.parent


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


walk = module('adopt_walk', 'adopt-walk.py')
T0 = 1_790_426_110_247
THREAD = 'thr_fixture'


def rows(*, typed=None, left_early=False, never_left=False, no_assignment=False):
    out = [
        {'kind': 'ledger', 'event': 'PromptReceived', 'turn': 'turn_phone', 'thread': THREAD, 'source': 'text',
         'intake_id': 1, 'at': T0, 't_ms': T0 + 35},
        {'kind': 'intake', 'record': 'channel', 'id': 1, 'thread': THREAD, 'channel': 'phone', 't_ms': T0 + 45},
        {'kind': 'ledger', 'event': 'PromptReceived', 'turn': 'turn_prime', 'thread': THREAD,
         'source': 'internal', 'intake_id': None, 'at': T0 + 59, 't_ms': T0 + 153},
        {'kind': 'ledger', 'event': 'TurnCompleted', 'turn': 'turn_phone', 'thread': None, 'source': None,
         'intake_id': None, 'at': T0 + 13_247, 't_ms': T0 + 13_307},
    ]
    if not no_assignment:
        out.append({'kind': 'assignment', 'id': 'a1', 'thread': THREAD, 'state': 'registered',
                    'registered_at_ms': T0 + 11_446, 'updated_at_ms': T0 + 11_446, 't_ms': T0 + 11_528,
                    'detail': 'Written down. Preparation has not started.'})
        if not never_left:
            left = T0 + (12_000 if left_early else 13_255)
            out.append({'kind': 'assignment', 'id': 'a1', 'thread': THREAD, 'state': 'preparing',
                        'registered_at_ms': T0 + 11_446, 'updated_at_ms': left, 't_ms': left + 51,
                        'detail': 'Opening the work connection.'})
    if typed == 'desk':
        out += [
            {'kind': 'ledger', 'event': 'PromptReceived', 'turn': 'turn_typed', 'thread': THREAD, 'source': 'text',
             'intake_id': 2, 'at': T0 + 12_000, 't_ms': T0 + 12_010},
            {'kind': 'intake', 'record': 'desk', 'id': 2, 'thread': THREAD, 'channel': None, 't_ms': T0 + 12_011},
        ]
    if typed == 'direct':
        out.append({'kind': 'ledger', 'event': 'PromptReceived', 'turn': 'turn_typed', 'thread': THREAD,
                    'source': 'text', 'intake_id': None, 'at': T0 + 12_000, 't_ms': T0 + 12_010})
    return sorted(out, key=lambda r: r['t_ms'])


SENT = [T0 - 11_800, T0 + 15]


class Verdict(unittest.TestCase):
    def test_the_walk_as_measured_passes_and_says_how_long_after_the_turn_it_started(self):
        evidence, failure = walk.analyze(rows(), THREAD, SENT)
        self.assertIsNone(failure)
        self.assertEqual(evidence['left_registered_ms_after_turn_end'], 8)
        self.assertEqual([p['road'] for p in evidence['prompts']], ['phone'])

    def test_a_typed_message_through_the_intake_log_fails_it(self):
        _, failure = walk.analyze(rows(typed='desk'), THREAD, SENT)
        self.assertIn('did not come from the phone', failure)
        self.assertIn('through the intake log', failure)

    def test_a_typed_message_straight_to_the_spine_fails_it(self):
        _, failure = walk.analyze(rows(typed='direct'), THREAD, SENT)
        self.assertIn('straight to the spine', failure)

    def test_work_that_never_left_registered_fails_it(self):
        _, failure = walk.analyze(rows(never_left=True), THREAD, SENT)
        self.assertIn('never left Registered', failure)

    def test_work_that_left_registered_inside_the_turn_fails_it(self):
        _, failure = walk.analyze(rows(left_early=True), THREAD, SENT)
        self.assertIn('before its turn ended', failure)

    def test_no_assignment_at_all_fails_it(self):
        _, failure = walk.analyze(rows(no_assignment=True), THREAD, SENT)
        self.assertIn('no assignment was registered', failure)

    def test_another_conversation_is_not_read_as_this_one(self):
        other = [dict(r, thread='thr_other') if r.get('thread') == THREAD else r for r in rows()]
        _, failure = walk.analyze(other, THREAD, SENT)
        self.assertIn('no assignment was registered', failure)


class FirstRunSheets(unittest.TestCase):
    """decline_first_run_sheets against a scripted screen: each Not now takes one sheet away."""

    def walk_on(self, screens):
        w = walk.Walk.__new__(walk.Walk)
        state = {'screens': list(screens), 'pressed': []}
        w.present = lambda title, *a, **k: title in state['screens'][0]
        def press(title, *a, **k):
            state['pressed'].append(title)
            state['screens'].pop(0)
        w.press = press
        return w, state

    def setUp(self):
        self.sleep = walk.time.sleep
        walk.time.sleep = lambda s: None

    def tearDown(self):
        walk.time.sleep = self.sleep

    def test_the_setup_sheet_then_the_memory_question_are_both_declined(self):
        w, state = self.walk_on([{'Set it up', 'Not now'}, {'Not now'}, {'Add this company', 'Not now'}])
        self.assertEqual(w.decline_first_run_sheets(), 2)
        self.assertEqual(state['pressed'], ['Not now', 'Not now'])

    def test_the_company_question_is_never_declined(self):
        w, state = self.walk_on([{'Add this company', 'Not now'}])
        self.assertEqual(w.decline_first_run_sheets(), 0)
        self.assertEqual(state['pressed'], [])


class Identity(unittest.TestCase):
    """The app reports the commit its release was built from (the version bump on top of the source);
    the nightly's own records name both for a release."""

    RECORDS = [{'source_commit': 'aaaa1111aaaa1111', 'build_commit': 'bbbb2222bbbb2222', 'created_at': '2'},
               {'source_commit': 'cccc3333cccc3333', 'build_commit': 'dddd4444dddd4444', 'created_at': '1'}]

    def test_the_source_commit_of_the_release_accepts_its_build_commit(self):
        self.assertTrue(walk.built_from_matches('bbbb2222b', 'aaaa1111', self.RECORDS))

    def test_the_build_commit_itself_is_accepted(self):
        self.assertTrue(walk.built_from_matches('bbbb2222bbbb2222', 'bbbb2222', self.RECORDS))

    def test_another_releases_build_commit_is_refused(self):
        self.assertFalse(walk.built_from_matches('dddd4444d', 'aaaa1111', self.RECORDS))

    def test_a_commit_no_record_names_is_refused(self):
        self.assertFalse(walk.built_from_matches('eeee5555e', 'aaaa1111', self.RECORDS))
        self.assertFalse(walk.built_from_matches('', 'aaaa1111', self.RECORDS))
        self.assertFalse(walk.built_from_matches('bbbb2222b', '', self.RECORDS))

    def test_the_default_is_the_newest_releases_build_commit(self):
        self.assertEqual(walk.latest_build_commit(self.RECORDS[::-1][::-1]), 'bbbb2222bbbb2222')
        self.assertEqual(walk.latest_build_commit([]), '')

    def test_records_are_read_from_the_nightly_state_directory(self):
        import tempfile
        root = Path(tempfile.mkdtemp())
        for name, created in (('v1', '1'), ('v2', '2')):
            (root / 'releases' / name).mkdir(parents=True)
            (root / 'releases' / name / 'build-info.json').write_text(json.dumps(
                {'source_commit': 's' + name, 'build_commit': 'b' + name, 'created_at': created}))
        (root / 'releases' / 'v0').mkdir()
        (root / 'releases' / 'v0' / 'build-info.json').write_text('not json')
        self.assertEqual([r['build_commit'] for r in walk.release_records(root)], ['bv2', 'bv1'])


class OpenEnabled(unittest.TestCase):
    """wait_open_enabled against a scripted chooser: the click waits for Open to be enabled."""

    def walk_on(self, answers):
        w = walk.Walk.__new__(walk.Walk)
        w.calls = 0
        def ax(mode, *a, **k):
            answer = answers[min(w.calls, len(answers) - 1)]
            w.calls += 1
            if isinstance(answer, Exception):
                raise answer
            return [{'meta': True}, {'title': 'Open', 'x': 1, 'enabled': answer}]
        w.ax = ax
        return w

    def setUp(self):
        self.sleep, self.mono = walk.time.sleep, walk.time.monotonic
        self.now = 0.0
        def sleep(s):
            self.now += s
        walk.time.sleep = sleep
        walk.time.monotonic = lambda: self.now

    def tearDown(self):
        walk.time.sleep, walk.time.monotonic = self.sleep, self.mono

    def test_it_waits_while_open_is_disabled(self):
        w = self.walk_on([False, False, 'true'])
        w.wait_open_enabled()
        self.assertEqual(w.calls, 3)

    def test_a_read_with_no_answer_is_asked_again(self):
        w = self.walk_on([walk.StepFailed('{"error": "unresponsive"}'), True])
        w.wait_open_enabled()
        self.assertEqual(w.calls, 2)

    def test_one_slow_read_on_a_loaded_host_does_not_end_the_wait(self):
        """A review walk once failed when a read that used its whole 20 s deadline ended a fixed 15 s wait;
        it passed only with TESTVM_AX_TIMEOUT=40 set by hand. The default wait outlasts several reads."""
        import os
        before = os.environ.pop('TESTVM_AX_TIMEOUT', None)
        try:
            w = walk.Walk.__new__(walk.Walk)
            w.calls = 0
            def ax(mode, *a, **k):
                w.calls += 1
                if w.calls <= 2:
                    self.now += 20   # the read ran to its deadline and had no answer
                    raise walk.StepFailed('{"error": "guest_deadline"}')
                return [{'meta': True}, {'title': 'Open', 'x': 1, 'enabled': True}]
            w.ax = ax
            w.wait_open_enabled()
            self.assertEqual(w.calls, 3)
        finally:
            if before is not None:
                os.environ['TESTVM_AX_TIMEOUT'] = before

    def test_the_default_wait_follows_the_read_deadline(self):
        import os
        before = os.environ.get('TESTVM_AX_TIMEOUT')
        os.environ['TESTVM_AX_TIMEOUT'] = '40'
        try:
            w = self.walk_on([False])
            with self.assertRaises(walk.StepFailed) as ctx:
                w.wait_open_enabled()
            self.assertIn('within 120.0 s', str(ctx.exception))
        finally:
            if before is None:
                os.environ.pop('TESTVM_AX_TIMEOUT')
            else:
                os.environ['TESTVM_AX_TIMEOUT'] = before

    def test_still_disabled_at_the_limit_names_the_step_and_state(self):
        w = self.walk_on([False])
        with self.assertRaises(walk.StepFailed) as ctx:
            w.wait_open_enabled()
        self.assertIn('connect folder', str(ctx.exception))
        self.assertIn('enabled=False', str(ctx.exception))

    def test_a_wait_that_ends_unanswered_photographs_the_recorded_app(self):
        """esc-20261021T212321Z-0d63eb9f: the failure carries the app's main-thread path."""
        import os
        import tempfile
        root = tempfile.mkdtemp()
        (Path(root) / 'run' / 'vm1').mkdir(parents=True)
        (Path(root) / 'run' / 'vm1' / 'app.pid').write_text('1837\n')
        sampled, real_guest, before = [], walk.guest, os.environ.get('TESTVM_ROOT')
        def guest(vm, cmd, timeout=30):
            sampled.append(cmd)
            if cmd.startswith('pgrep'):
                return '2001'
            if cmd.startswith('top'):
                return 'Processes: 60 total\nCPU usage: 9% user\nPID COMMAND %CPU\n1837 richos-tauri 0.0'
            return ('Analysis of sampling 1837 every 1 millisecond\nCall graph:\n'
                    '    812 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)\n'
                    '      812 start  (in dyld) + 1  [0x1]\n'
                    '        812 mach_msg2_trap  (in libsystem_kernel.dylib) + 8  [0x2]\n\n')
        w = walk.Walk.__new__(walk.Walk)
        w.vm, w.out = 'vm1', Path(root)
        def ax(mode, *a, **k):
            raise walk.StepFailed('command failed (1): ax.sh find\n{"error": "unresponsive"}')
        w.ax = ax
        walk.guest, os.environ['TESTVM_ROOT'] = guest, root
        try:
            with self.assertRaises(walk.StepFailed) as ctx:
                w.wait_for('Onboarding', seconds=3)
        finally:
            walk.guest = real_guest
            if before is None:
                os.environ.pop('TESTVM_ROOT')
            else:
                os.environ['TESTVM_ROOT'] = before
        self.assertIn('sample 1837 1 10', ' '.join(sampled))
        self.assertIn('app pid 1837, main thread, heaviest path:', str(ctx.exception))
        self.assertIn('mach_msg2_trap', str(ctx.exception))
        self.assertTrue((Path(root) / 'unanswered-photo.txt').exists())

    def test_a_wait_that_ends_not_found_pictures_the_screen(self):
        """esc-20261021T213902Z-b513879e: the failure names what IS on screen."""
        import tempfile
        root = Path(tempfile.mkdtemp())
        w = walk.Walk.__new__(walk.Walk)
        w.vm, w.out, w.shots = 'vm1', root, []
        def ax(mode, *a, **k):
            if mode == 'tree':
                return [{'meta': True}, {'role': 'AXButton', 'title': 'Acme'},
                        {'role': 'AXButton', 'title': '', 'desc': 'Talk to Rich'}, {'role': 'AXStaticText', 'title': 'x'}]
            raise walk.StepFailed('command failed (1): ax.sh find\n{"error": "notfound", "detail": "nothing matched"}')
        w.ax = ax
        w.shot = lambda name: w.shots.append(name)
        with self.assertRaises(walk.StepFailed) as ctx:
            w.wait_for('Onboarding', seconds=3)
        self.assertEqual(w.shots, ['notfound-Onboarding.png'])
        self.assertIn('buttons on screen: ["Acme", "Talk to Rich"]', str(ctx.exception))
        self.assertTrue((root / 'notfound-Onboarding.tree.jsonl').exists())

    def test_the_last_read_chooses_the_picture(self):
        """Second review of 1d0f056eb (P3): unresponsive, then not found, then not found is a
        screen to picture, not an app to sample."""
        w = walk.Walk.__new__(walk.Walk)
        answers = [walk.StepFailed('{"error": "unresponsive"}')] + \
                  [walk.StepFailed('{"error": "notfound", "detail": "nothing matched"}')] * 10
        def ax(mode, *a, **k):
            raise answers.pop(0) if len(answers) > 1 else answers[0]
        w.ax = ax
        w.picture_screen = lambda title: 'PICTURED'
        w.photograph_app = lambda: 'SAMPLED'
        with self.assertRaises(walk.StepFailed) as ctx:
            w.wait_for('Onboarding', seconds=3)
        self.assertIn('PICTURED', str(ctx.exception))
        self.assertNotIn('SAMPLED', str(ctx.exception))

    def test_connect_folder_has_no_fixed_sleep_before_open(self):
        source = (HERE / 'adopt-walk.py').read_text()
        body = source[source.index('    def connect_folder'):source.index('    def decline_first_run_sheets')]
        self.assertIn('self.wait_open_enabled()', body)
        self.assertNotIn('time.sleep(2.5)', body)


class PhoneSteps(unittest.TestCase):
    """pair and phone-task against a scripted guest: the phone is the walk phone over the native
    apps' API (walkphone.py), never the retired phone web page in the guest's Safari."""

    URL = 'https://richos-test-1.example.ts.net:8443/#pair=ABCD2345'

    def setUp(self):
        from unittest.mock import patch
        self.calls = []   # everything sent to the guest or its screen, as text
        self.phone = []
        w = walk.Walk.__new__(walk.Walk)
        w.vm, w.out = 'walk-fixture', Path('/nonexistent-out')
        w.a = type('A', (), {'task': 'Please add hello.txt'})()
        w.facts = {'thread': THREAD}
        w.phone_state = Path('/nonexistent-out/phone.json')
        w.save = lambda: None
        w.clock = lambda: 1000.0
        def ax(mode, *args, app=None, timeout=40):
            self.calls.append(' '.join(map(str, [mode, *args, 'app=%s' % app])))
            return [{'title': 'They match'}]
        w.ax = ax
        w.press = lambda title, role='AXButton', app=None, contains=True: self.calls.append('press %s app=%s' % (title, app))
        w.present = lambda title, role='AXButton', app=None: (self.calls.append('present %s app=%s' % (title, app)), title == 'Use Tailscale')[1]
        w.wait_for = lambda title, role='AXButton', app=None, seconds=30: self.calls.append('wait %s app=%s' % (title, app))
        w.type_into = lambda text, *sel, app=None: self.calls.append('type %s app=%s' % (text, app))
        self.w = w
        def command(argv, timeout=30):
            self.calls.append(' '.join(map(str, argv)))
            return '{"title": "%s"}' % self.URL
        def guest(vm, cmd, *a, **k):
            self.calls.append('guest ' + cmd)
            return ''
        self.patches = [patch.object(walk, 'command', side_effect=command), patch.object(walk, 'guest', side_effect=guest),
                        patch.object(walk.walkphone, 'pair', side_effect=lambda *a, **k: self.phone.append(('pair', a, k)) or
                                     {'words': 'six words', 'threads': [{'id': THREAD}]}),
                        patch.object(walk.walkphone, 'send', side_effect=lambda *a, **k: self.phone.append(('send', a, k)) or {})]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def test_pairing_gives_the_address_to_the_walk_phone_and_the_mac_still_presses_they_match(self):
        self.assertEqual(self.w.pair()['words'], 'six words')
        self.assertEqual(self.phone, [('pair', (self.w.phone_state, self.URL), {})])
        self.assertTrue(any('They match' in c and 'app=None' in c for c in self.calls), self.calls)

    def test_the_task_is_sent_from_the_walk_phone_on_the_first_run_conversation(self):
        self.assertEqual(self.w.phone_task(), {'sent_between_guest_ms': [1000, 1000]})
        self.assertEqual(self.phone, [('send', (self.w.phone_state, THREAD, 'Please add hello.txt'), {})])

    def test_neither_phone_step_opens_the_old_page_or_touches_safari(self):
        self.w.pair()
        self.w.phone_task()
        self.assertFalse([c for c in self.calls if 'Safari' in c or c.startswith('guest open')], self.calls)

    def test_a_phone_that_cannot_pair_fails_the_step_with_its_reason(self):
        walk.walkphone.pair.side_effect = walk.walkphone.PhoneFailed('certificate not trusted')
        with self.assertRaises(walk.StepFailed) as ctx:
            self.w.pair()
        self.assertIn('certificate not trusted', str(ctx.exception))


if __name__ == '__main__':
    unittest.main(verbosity=1)

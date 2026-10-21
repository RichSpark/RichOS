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

    def test_still_disabled_at_the_limit_names_the_step_and_state(self):
        w = self.walk_on([False])
        with self.assertRaises(walk.StepFailed) as ctx:
            w.wait_open_enabled()
        self.assertIn('connect folder', str(ctx.exception))
        self.assertIn('enabled=False', str(ctx.exception))

    def test_connect_folder_has_no_fixed_sleep_before_open(self):
        source = (HERE / 'adopt-walk.py').read_text()
        body = source[source.index('    def connect_folder'):source.index('    def decline_first_run_sheets')]
        self.assertIn('self.wait_open_enabled()', body)
        self.assertNotIn('time.sleep(2.5)', body)


if __name__ == '__main__':
    unittest.main(verbosity=1)

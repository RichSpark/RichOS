#!/usr/bin/env python3
"""A waiting find asks again after a read the app did not answer, through ONE rule (axwait.py).

esc-20261021T164604Z-4c30b43e: ax.py answers `unresponsive` when the app does not answer one
attribute (ced86ec1d). adopt-walk.py's present() counted only `notfound` as "not yet", so the reap
walk ended at first-run on one such read of "Add this company", 13 s after the app was ready.

No guest: each walk's lowest ax() is replaced by a stub that answers what the guest answered."""
import importlib.util
from pathlib import Path
import re
import sys
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True

UNRESPONSIVE = ('command failed (1): ax.sh walk-x find --title Add this company --role AXButton\n'
                '{"error": "unresponsive", "detail": "AXUIElement(0x600000d0c030) did not answer for AXRole '
                '(kAXErrorCannotComplete); the read is incomplete, so nothing is reported as found or absent"}\n')
# ax-deadline.py timeout_record(): the record says effect_unknown for every mode, a find included.
DEADLINE = ('command failed (124): ax.sh walk-x find --title Start the questions\n'
            '{"error": "guest_deadline", "phase": "guest", "status": "effect_unknown", '
            '"detail": "AX deadline reached; owned command group terminated; inspect before retrying"}\n')
NOTFOUND = 'command failed (1): ax.sh walk-x find\n{"error": "notfound", "detail": "nothing matched"}\n'
REAL = 'command failed (2): ax.sh walk-x find\n[testvm] ERROR: no app pid recorded for walk-x\n'


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


class TheRule(unittest.TestCase):
    def setUp(self):
        self.r = module('axwait', 'axwait.py')

    def test_a_find_the_app_did_not_answer_is_not_yet(self):
        self.assertTrue(self.r.not_yet(UNRESPONSIVE, 'find'))
        self.assertTrue(self.r.not_yet(UNRESPONSIVE, 'tree'))

    def test_a_find_past_the_guest_deadline_is_not_yet_whatever_its_status_says(self):
        self.assertIn('effect_unknown', DEADLINE)
        self.assertTrue(self.r.not_yet(DEADLINE, 'find'))

    def test_nothing_matched_is_not_yet_for_any_mode(self):
        for mode in ('find', 'tree', 'click'):
            self.assertTrue(self.r.not_yet(NOTFOUND, mode), mode)

    def test_a_click_press_or_type_with_no_answer_is_never_asked_again(self):
        for mode in ('click', 'type', 'focus'):
            for text in (UNRESPONSIVE, DEADLINE):
                self.assertFalse(self.r.not_yet(text, mode), mode)
                self.assertFalse(self.r.ask_again(text, mode), mode)

    def test_a_real_failure_is_raised(self):
        self.assertFalse(self.r.not_yet(REAL, 'find'))
        self.assertFalse(self.r.ask_again(REAL, 'find'))

    def test_absence_is_not_asked_again_as_a_slow_read(self):
        self.assertFalse(self.r.ask_again(NOTFOUND, 'find'))


# Every walk built on adopt-walk.py's Walk. Each file loads its own copy of adopt-walk.py, so the
# stub goes on THAT copy's Walk.ax, found through the class's own method order.
WALKS = {
    'adopt-walk.py': 'Walk', 'reap-walk.py': 'Walk', 'command-walk.py': 'CommandWalk',
    'folders-walk.py': 'Walk', 'netcut-walk.py': 'NetcutWalk', 'lease-setup-walk.py': 'LeaseSetupWalk',
    'setup-walk.py': 'SetupWalk', 'voice-walk.py': 'VoiceWalk', 'update-relaunch-walk.py': 'Walk',
    'hire-walk.py': 'HireWalk', 'stu-reminder-walk.py': 'StuReminderWalk', 'handoff-real-walk.py': 'HandoffWalk',
    'dictation-sheet-walk.py': 'SheetWalk', 'output-walk.py': 'OutputWalk', 'first-send-walk.py': 'FirstSendWalk',
    'video-watch-walk.py': 'VideoWatchWalk', 'worker-approve-walk.py': 'WorkerApproveWalk',
}


def walk_class(file):
    mod = module(file.replace('-', '_')[:-3] + '_axwait', file)
    cls = getattr(mod, WALKS[file])
    root = next(c for c in cls.__mro__ if 'ax' in c.__dict__ and 'wait_for' in c.__dict__ and 'present' in c.__dict__)
    return mod, cls, root


class EveryWalksWaitingFind(unittest.TestCase):
    def setUp(self):
        sleep = mock.patch('time.sleep', lambda s: None)
        sleep.start()
        self.addCleanup(sleep.stop)

    def test_present_reads_an_unanswered_find_as_not_there_yet(self):
        for file in WALKS:
            with self.subTest(walk=file):
                mod, cls, root = walk_class(file)
                w = object.__new__(cls)
                w.a = mock.Mock(within=1, after_relaunch_deadline=20)
                failed = root.ax.__globals__['StepFailed']
                for answer in (UNRESPONSIVE, DEADLINE):
                    with mock.patch.object(root, 'ax', side_effect=failed(answer)):
                        self.assertFalse(w.present('Add this company'))

    def test_a_find_by_dom_id_reads_an_unanswered_find_as_not_there_yet(self):
        for file in ('dictation-sheet-walk.py', 'setup-walk.py', 'voice-walk.py', 'lease-setup-walk.py'):
            with self.subTest(walk=file):
                mod, cls, root = walk_class(file)
                w = object.__new__(cls)
                failed = root.ax.__globals__['StepFailed']
                with mock.patch.object(root, 'ax', side_effect=failed(UNRESPONSIVE)):
                    self.assertIsNone(w.by_id('memory-setup-later'))
                with mock.patch.object(root, 'ax', side_effect=failed(REAL)):
                    with self.assertRaises(failed):
                        w.by_id('memory-setup-later')

    def test_the_walks_with_their_own_find_read_it_the_same_way(self):
        asked = module('asked_again_axwait', 'asked-again-walk.py')
        w = object.__new__(asked.Walk)
        with mock.patch.object(asked.Walk, 'ax', side_effect=asked.StepFailed(UNRESPONSIVE)):
            self.assertFalse(w.present('Not now'))
        mod, cls, root = walk_class('first-send-walk.py')
        w = object.__new__(cls)
        with mock.patch.object(root, 'ax', side_effect=root.ax.__globals__['StepFailed'](UNRESPONSIVE)):
            self.assertFalse(w.shown('Hello Rich'))
        mod, cls, root = walk_class('folders-walk.py')
        w = object.__new__(cls)
        with mock.patch.object(root, 'ax', side_effect=root.ax.__globals__['StepFailed'](UNRESPONSIVE)):
            self.assertEqual(w.nodes('--title', 'Acme-docs'), [])

    def test_present_still_raises_a_real_failure(self):
        for file in WALKS:
            with self.subTest(walk=file):
                mod, cls, root = walk_class(file)
                w = object.__new__(cls)
                w.a = mock.Mock(within=1, after_relaunch_deadline=20)
                failed = root.ax.__globals__['StepFailed']
                with mock.patch.object(root, 'ax', side_effect=failed(REAL)):
                    with self.assertRaises(failed):
                        w.present('Add this company')

    def test_a_click_with_no_answer_is_sent_once(self):
        for file in WALKS:
            with self.subTest(walk=file):
                mod, cls, root = walk_class(file)
                w = object.__new__(cls)
                w.a = mock.Mock(within=1, after_relaunch_deadline=20)
                failed = root.ax.__globals__['StepFailed']
                for answer in (UNRESPONSIVE, DEADLINE):
                    with mock.patch.object(root, 'ax', side_effect=failed(answer)) as base:
                        with self.assertRaises(failed):
                            w.ax('click', '--title', 'Send', '--role', 'AXButton', '--first')
                    self.assertEqual(base.call_count, 1, answer[:30])


class TheReapWalksFirstRun(unittest.TestCase):
    """The escalation's own read: one unresponsive find of "Add this company" at first-run."""

    def test_an_unanswered_read_of_the_company_question_is_asked_again(self):
        mod, cls, root = walk_class('reap-walk.py')
        w = object.__new__(cls)
        failed = root.ax.__globals__['StepFailed']
        answers = [failed(UNRESPONSIVE), failed(NOTFOUND), [{'meta': True}, {'role': 'AXButton', 'x': 1}]]
        with mock.patch('time.sleep', lambda s: None), mock.patch.object(root, 'ax', side_effect=answers) as base:
            self.assertEqual(w.decline_first_run_sheets(), 0)
        self.assertEqual(base.call_count, 3)

    def test_wait_for_asks_again_until_its_own_limit_and_says_why_it_failed(self):
        mod, cls, root = walk_class('adopt-walk.py')
        w = object.__new__(cls)
        failed = root.ax.__globals__['StepFailed']
        answers = [failed(UNRESPONSIVE), failed(DEADLINE), [{'meta': True}, {'role': 'AXButton'}]]
        with mock.patch('time.sleep', lambda s: None), mock.patch.object(root, 'ax', side_effect=answers):
            self.assertTrue(w.wait_for('Add this company', seconds=30))
        clock = iter(range(0, 1000, 10))
        with mock.patch.object(mod.time, 'monotonic', lambda: next(clock)), mock.patch('time.sleep', lambda s: None), \
                mock.patch.object(root, 'ax', side_effect=failed(UNRESPONSIVE)) as base:
            with self.assertRaises(failed) as caught:
                w.wait_for('Add this company', seconds=30)
        self.assertLessEqual(base.call_count, 3)
        self.assertIn('unresponsive', str(caught.exception))

    def test_the_folder_chooser_is_neither_open_nor_closed_on_an_unanswered_read(self):
        mod, cls, root = walk_class('adopt-walk.py')
        w = object.__new__(cls)
        failed = root.ax.__globals__['StepFailed']
        with mock.patch.object(root, 'ax', side_effect=failed(UNRESPONSIVE)):
            self.assertIsNone(w.chooser_open())
        with mock.patch.object(root, 'ax', side_effect=failed(NOTFOUND)):
            self.assertFalse(w.chooser_open())


class NoWalkKeepsItsOwnCopy(unittest.TestCase):
    """A walk that classifies a failed ax.sh call by its words asks axwait.py, so the next
    answer ax.py learns to give is learned once. A walk's own extra answer (`nowindow`, a
    relaunched app with no window yet) stays its own."""

    TOKENS = re.compile(r"""['"](?:[^'"]*(?:notfound|nothing matched|unresponsive|deadline|124)[^'"]*)['"]\s+(?:not\s+)?in\s+str\(""")

    def test_no_walk_matches_the_rules_words_itself(self):
        copies = []
        for path in sorted(HERE.glob('*-walk.py')):
            for n, line in enumerate(path.read_text().splitlines(), 1):
                if self.TOKENS.search(line):
                    copies.append(f'{path.name}:{n}: {line.strip()}')
        self.assertEqual(copies, [], '\n'.join(copies))


if __name__ == '__main__':
    unittest.main(verbosity=2)

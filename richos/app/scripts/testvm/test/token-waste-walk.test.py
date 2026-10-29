#!/usr/bin/env python3
"""token-waste-walk.py's two verdicts, offline, on the shapes the guest helper prints: each one
passes on the right answer and REFUSES on the wrong one (a missing control is not a pass)."""
import importlib.util
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('token_waste_walk', HERE / 'token-waste-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

SETTLED = {'reason': 'the assignment was completed and this receipt never ran', 'withdrawn': [], 'retained': []}


class Receipts(unittest.TestCase):
    def test_a_settled_never_admitted_receipt_passes(self):
        rows = [{'name': 'a', 'admitted': True, 'settled': None}, {'name': 'b', 'admitted': False, 'settled': SETTLED}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'PASS')

    def test_an_open_never_admitted_receipt_fails(self):
        rows = [{'name': 'b', 'admitted': False, 'status': 'prepared', 'settled': None}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'FAIL')

    def test_no_never_admitted_receipt_is_not_reached_not_a_pass(self):
        self.assertEqual(walk.judge_receipts([{'name': 'a', 'admitted': True}])[0], 'NOT REACHED')
        self.assertEqual(walk.judge_receipts([])[0], 'NOT REACHED')

    def test_a_settlement_with_no_reason_fails(self):
        rows = [{'name': 'b', 'admitted': False, 'settled': {'reason': ' ', 'withdrawn': [], 'retained': []}}]
        self.assertEqual(walk.judge_receipts(rows)[0], 'FAIL')

    def test_an_unreadable_receipt_fails(self):
        self.assertEqual(walk.judge_receipts([{'file': 'x.json', 'unreadable': True}])[0], 'FAIL')


class Quiet(unittest.TestCase):
    CONTROL = '[SILENT] control-silent: no commit and no transcript activity, for 60 min\n'

    def test_control_told_and_never_started_not_told_passes(self):
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': self.CONTROL})[0], 'PASS')

    def test_a_notice_for_a_never_started_teammate_fails(self):
        out = self.CONTROL + '[SILENT] lined-up-never-started: no commit\n'
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': out})[0], 'FAIL')

    def test_no_control_means_nothing_was_measured(self):
        self.assertEqual(walk.judge_quiet({'rc': 0, 'out': 'STALL-WATCH: no stall\n'})[0], 'FAIL')

    def test_a_failed_tick_fails(self):
        self.assertEqual(walk.judge_quiet({'rc': 2, 'out': self.CONTROL})[0], 'FAIL')


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""lease-model-walk.py's reader of a reopened effort control, offline: which level is marked
chosen, and each way it refuses. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('lease_model_walk', HERE / 'lease-model-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


def row(checked=(), absent=()):
    return {level: None if level in absent else ('1' if level in checked else '0') for level in walk.LEVELS}


class ChosenLevel(unittest.TestCase):
    def test_the_pressed_level_is_the_one_marked(self):
        self.assertEqual(walk.chosen_level('rich', row(checked=['medium']), 'medium'), 'medium')

    def test_a_true_value_counts_as_checked(self):
        values = dict(row(), high='true')
        self.assertEqual(walk.chosen_level('stu', values, 'high'), 'high')

    def test_another_level_marked_is_refused(self):
        with self.assertRaisesRegex(walk.StepFailed, 'shows low after Settings was opened again; medium was pressed'):
            walk.chosen_level('rich', row(checked=['low']), 'medium')

    def test_no_level_marked_is_refused(self):
        with self.assertRaisesRegex(walk.StepFailed, '0 levels marked chosen'):
            walk.chosen_level('rich', row(), 'medium')

    def test_two_levels_marked_is_refused(self):
        with self.assertRaisesRegex(walk.StepFailed, '2 levels marked chosen'):
            walk.chosen_level('stu', row(checked=['low', 'high']), 'high')

    def test_a_missing_level_is_refused(self):
        with self.assertRaisesRegex(walk.StepFailed, 'does not show: xhigh'):
            walk.chosen_level('rich', row(checked=['medium'], absent=['xhigh']), 'medium')


if __name__ == '__main__':
    unittest.main()

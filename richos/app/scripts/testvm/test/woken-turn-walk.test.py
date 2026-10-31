#!/usr/bin/env python3
"""woken-turn-walk.py's verdict, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('woken_turn_walk', HERE / 'woken-turn-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

M = 'woken-abc'


def bash(command, stdout='', background=False, agent=None):
    tool_input = {'command': command}
    if background:
        tool_input['run_in_background'] = True
    return {'session': 'rich', 'callback': {'hook_event_name': 'PostToolUse', 'tool_name': 'Bash', 'agent_id': agent,
                                            'tool_input': tool_input, 'tool_response': {'stdout': stdout}}}


STOP = {'session': 'rich', 'callback': {'hook_event_name': 'Stop'}}
LAUNCH = bash('sleep 40; echo %s-done' % M, background=True)
CARRIED = bash('echo %s-carried-on' % M, stdout='%s-carried-on\n' % M)


class Verdict(unittest.TestCase):
    def test_launch_then_his_turn_end_then_a_run_of_his_passes(self):
        failures, facts = walk.verdict([LAUNCH, STOP, CARRIED], M)
        self.assertEqual(failures, [])
        self.assertEqual((facts['launch_index'], facts['turn_end_index'], facts['carried_on_index']), (0, 1, 2))

    def test_a_run_inside_the_same_turn_is_not_carrying_on(self):
        failures, _ = walk.verdict([LAUNCH, CARRIED, STOP], M)
        self.assertEqual(failures, ['after his turn ended, no Bash run of his printed %s-carried-on' % M])

    def test_a_foreground_launch_or_a_workers_run_is_not_his_background_command(self):
        failures, _ = walk.verdict([bash('sleep 40; echo %s-done' % M), STOP, CARRIED], M)
        self.assertTrue(failures and 'in the background' in failures[0], failures)
        worker = bash('echo %s-carried-on' % M, stdout='%s-carried-on' % M, agent='a1')
        failures, _ = walk.verdict([LAUNCH, STOP, worker], M)
        self.assertEqual(len(failures), 1)

    def test_a_refused_run_that_printed_nothing_does_not_pass(self):
        refused = bash('echo %s-carried-on' % M, stdout='')
        failures, _ = walk.verdict([LAUNCH, STOP, refused], M)
        self.assertEqual(len(failures), 1)


if __name__ == '__main__':
    unittest.main()

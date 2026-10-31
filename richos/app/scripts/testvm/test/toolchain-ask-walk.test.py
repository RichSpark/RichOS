#!/usr/bin/env python3
"""toolchain-ask-walk.py's judging helpers, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('toolchain_ask_walk', HERE / 'toolchain-ask-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class Turns(unittest.TestCase):
    def test_only_completed_turns_begun_after_the_send_with_their_text_joined(self):
        rows = [{'turn_id': 'old', 'event': 'PromptReceived', 'at': 100},
                {'turn_id': 'old', 'event': 'AssistantDelta', 'text': 'Install make?', 'at': 101},
                {'turn_id': 'old', 'event': 'TurnCompleted', 'at': 102},
                {'turn_id': 'new', 'event': 'PromptReceived', 'at': 500},
                {'turn_id': 'new', 'event': 'AssistantDelta', 'text': 'Shall I install ', 'at': 501},
                {'turn_id': 'new', 'event': 'AssistantDelta', 'text': 'make?', 'at': 502},
                {'turn_id': 'new', 'event': 'TurnCompleted', 'at': 503},
                {'turn_id': 'open', 'event': 'PromptReceived', 'at': 600},
                {'turn_id': 'open', 'event': 'AssistantDelta', 'text': 'Working', 'at': 601}]
        self.assertEqual(walk.turns_after(rows, 400), [{'turn_id': 'new', 'text': 'Shall I install make?'}])


class Asks(unittest.TestCase):
    def test_an_ask_names_installing_and_the_tool_or_its_package(self):
        for text in ('Should I install make for you?', 'It needs Apple\'s developer tools. Install them?',
                     'May I install the Command Line Tools?'):
            self.assertTrue(walk.asks_to_install(text), text)
        for text in ('GNU Make 3.81', 'I ran make --version.', 'Installing now.'):
            self.assertFalse(walk.asks_to_install(text), text)


class MakeRuns(unittest.TestCase):
    def test_the_install_command_is_not_a_run_of_make(self):
        rows = [{'command': 'cd ~/Acme && make --version', 'stdout': 'GNU Make 3.81'},
                {'command': '"$RICHOS_RUNTIME_PYTHON" toolchain.py answer make yes', 'stdout': 'INSTALLED'},
                {'command': 'ls', 'stdout': ''}]
        self.assertEqual([r['command'] for r in walk.make_runs(rows)], ['cd ~/Acme && make --version'])


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""Offline proof of wait-turn.py on a fixture ledger. No app, no screen."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).with_name('wait-turn.py')


def row(at, event, turn, **kw):
    return {'at': at, 'event': event, 'turn_id': turn, **kw}


# The request's acknowledgement (turn-ack) completed; the long answer streams in a different,
# still-open turn (turn-long, started by Stu's result).
LEDGER = [
    row(1000, 'PromptReceived', 'turn-ack', source='user', text='write me a long essay'),
    row(1100, 'AssistantDelta', 'turn-ack', text='On it.'),
    row(1200, 'TurnCompleted', 'turn-ack'),
    row(5000, 'PromptReceived', 'turn-long', source='internal', text='worker result: essay'),
    row(5100, 'AssistantDelta', 'turn-long', text='x' * 150),
    row(5200, 'AssistantDelta', 'turn-long', text='y' * 150),
]


def run(rows, *args, timeout='0.3'):
    with tempfile.TemporaryDirectory() as d:
        path = Path(d) / 'conversation-ledger.jsonl'
        path.write_text('\n'.join(json.dumps(r) for r in rows) + '\n')
        return subprocess.run([sys.executable, str(TOOL), str(path), '--timeout', timeout,
                               '--interval', '0.05', *args], capture_output=True, text=True)


class WaitTurn(unittest.TestCase):
    def test_newest_open_turn_is_the_later_one(self):
        r = run(LEDGER, '--after-ms', '500')
        self.assertEqual((r.returncode, r.stdout.strip()), (0, 'turn-long'), r.stderr)

    def test_prompt_text_of_the_completed_request_does_not_match(self):
        r = run(LEDGER, '--prompt-contains', 'long essay')
        self.assertEqual(r.returncode, 1)
        self.assertIn('already completed', r.stderr)
        self.assertEqual(r.stdout, '')

    def test_prompt_text_of_the_open_turn_matches(self):
        r = run(LEDGER, '--prompt-contains', 'worker result')
        self.assertEqual((r.returncode, r.stdout.strip()), (0, 'turn-long'))

    def test_prompt_match_survives_a_turn_replayed_retry(self):
        # An overload retry has no PromptReceived row of its own: it inherits the original's prompt.
        rows = [row(1000, 'PromptReceived', 'turn-orig', source='user', text='write me a long essay'),
                row(1100, 'AssistantDelta', 'turn-orig', text='x' * 50),
                row(2000, 'TurnReplayed', 'turn-orig', by_turn_id='turn-retry'),
                row(2100, 'AssistantDelta', 'turn-retry', text='y' * 300)]
        r = run(rows, '--prompt-contains', 'long essay')
        self.assertEqual((r.returncode, r.stdout.strip()), (0, 'turn-retry'), r.stderr)
        r = run(rows, '--after-ms', '1500')
        self.assertEqual((r.returncode, r.stdout.strip()), (0, 'turn-retry'), r.stderr)

    def test_too_few_characters_times_out_with_exit_1(self):
        r = run(LEDGER, '--after-ms', '500', '--min-chars', '301')
        self.assertEqual(r.returncode, 1)
        self.assertIn('streamed only 300 of 301', r.stderr)

    def test_nothing_started_after_the_time_exits_1(self):
        r = run(LEDGER, '--after-ms', '9000')
        self.assertEqual(r.returncode, 1)

    def test_closed_long_turn_does_not_qualify(self):
        r = run(LEDGER + [row(6000, 'TurnCompleted', 'turn-long')], '--after-ms', '500')
        self.assertEqual(r.returncode, 1)

    def test_missing_ledger_exits_1_with_reason(self):
        r = subprocess.run([sys.executable, str(TOOL), '/nonexistent/ledger.jsonl', '--after-ms', '0',
                            '--timeout', '0.2'], capture_output=True, text=True)
        self.assertEqual(r.returncode, 1)
        self.assertIn('no ledger at', r.stderr)

    def test_usage_error_exits_2(self):
        self.assertEqual(run(LEDGER).returncode, 2)


if __name__ == '__main__':
    unittest.main()

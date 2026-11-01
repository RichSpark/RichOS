#!/usr/bin/env python3
"""resend-walk.py's fixture and verdict, offline. The live walk is its own proof."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('resend_walk', HERE / 'resend-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

S = 'beadfeed-0000-4000-8000-000000000001'


def sample(lines=2, others=(), procs=(), active=None):
    if active is None:
        active = {'peer:300': {'input': 'peer:300', 'session_id': S, 'received': True}}
    return {'record_lines': lines, 'other_stu_sessions': list(others), 'stu_processes': list(procs), 'cursor_active': active}


class Verdict(unittest.TestCase):
    def test_no_turn_and_the_received_mark_passes(self):
        self.assertEqual(walk.quiet_verdict(2, [sample(), sample()], S), [])

    def test_a_grown_record_is_a_turn(self):
        failures = walk.quiet_verdict(2, [sample(), sample(lines=5)], S)
        self.assertTrue(failures and 'a turn ran' in failures[0], failures)

    def test_another_stu_session_or_process_fails(self):
        self.assertTrue(walk.quiet_verdict(2, [sample(others=['fresh'])], S))
        self.assertTrue(walk.quiet_verdict(2, [sample(procs=['claude --agent richos:stu'])], S))

    def test_a_new_attempt_or_a_missing_mark_fails(self):
        attempt = {'peer:300': {'input': 'peer:300', 'session_id': 'other'}}
        self.assertIn('no longer names', walk.quiet_verdict(2, [sample(active=attempt)], S)[0])
        unmarked = {'peer:300': {'input': 'peer:300', 'session_id': S}}
        self.assertIn('did not mark', walk.quiet_verdict(2, [sample(active=unmarked)], S)[0])

    def test_nothing_read_fails(self):
        self.assertEqual(walk.quiet_verdict(2, [], S), ['nothing was read in the quiet window'])


class Fixture(unittest.TestCase):
    def test_the_seed_writes_an_accepted_open_handoff_and_a_record_that_holds_it(self):
        with tempfile.TemporaryDirectory() as root:
            data, home = os.path.join(root, 'data'), os.path.join(root, 'home')
            text = walk.HANDOFF.format(marker='abc')
            out = subprocess.run([sys.executable, '-c', walk.SEED, data, home, 'acme', 'thr_1', S, text, 'a' * 64],
                                 capture_output=True, text=True, check=True).stdout
            wrote = json.loads(out)
            inbox = Path(wrote['inbox']).read_text()
            message = json.loads(inbox)
            self.assertEqual((message['handoff'], message['from'], message['text']), (True, 'rich', text))
            cursor = json.loads((Path(wrote['inbox']).with_suffix('.cursor')).read_text())
            self.assertEqual(cursor['through'], len(inbox.encode()))
            self.assertEqual(cursor['active'], {'peer:%d' % len(inbox.encode()): {'input': 'peer:%d' % len(inbox.encode()), 'session_id': S}})
            evidence = Path(data) / 'engine-state' / 'evidence' / S
            self.assertEqual(json.loads((evidence / 'record.json').read_text())['transcript_path'], wrote['record'])
            self.assertTrue(wrote['record'].endswith('/' + S + '.jsonl'))
            rows = [json.loads(r) for r in Path(wrote['record']).read_text().splitlines()]
            given = json.loads(rows[0]['message']['content'][0]['text'])
            self.assertEqual({k: given[k] for k in message}, message)
            read = json.loads(subprocess.run([sys.executable, '-c', walk.READ, data, 'acme', 'thr_1', S, wrote['record']],
                                             capture_output=True, text=True, check=True).stdout)
            self.assertEqual((read['record_lines'], read['other_stu_sessions']), (2, []))


if __name__ == '__main__':
    unittest.main()

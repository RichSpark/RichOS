#!/usr/bin/env python3
"""Offline proof of routing-probe.py against a stand-in claude. No model, no app, no network."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).with_name('routing-probe.py')

# A stand-in for the classifier's `claude -p --input-format stream-json ...`: it records its argv
# and every message it is sent, and answers each with FAKE_LABEL as the result.
FAKE = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ['FAKE_LOG'], 'a') as log:
    log.write(json.dumps({'argv': sys.argv[1:]}) + '\n')
for line in sys.stdin:
    with open(os.environ['FAKE_LOG'], 'a') as log:
        log.write(json.dumps({'sent': json.loads(line)['message']['content']}) + '\n')
    print(json.dumps({'type': 'system', 'subtype': 'init', 'tools': []}), flush=True)
    print(json.dumps({'type': 'result', 'is_error': False, 'result': os.environ['FAKE_LABEL']}), flush=True)
'''


class Probe(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name).resolve()
        self.engine = root / 'engine'
        (self.engine / 'app' / 'agents').mkdir(parents=True)
        (self.engine / 'app' / 'routing.md').write_text('You classify.\n')
        (self.engine / 'app' / 'agents' / 'rich.md').write_text('You are Rich.\n')
        (self.engine / 'app' / 'agents' / 'stu.md').write_text('You are Stu.\n')
        self.claude = root / 'claude'
        self.claude.write_text(FAKE)
        self.claude.chmod(0o755)
        self.log = root / 'log.jsonl'

    def tearDown(self):
        self.dir.cleanup()

    def probe(self, case, label, engine=None, claude=None):
        env = dict(os.environ, FAKE_LOG=str(self.log), FAKE_LABEL=label)
        return subprocess.run([sys.executable, str(TOOL), '--engine', str(engine or self.engine), '--case', case,
                               '--runs', '2', '--claude', str(claude or self.claude)],
                              capture_output=True, text=True, env=env, timeout=60)

    def rows(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_a_handoff_of_a_request_rich_was_asked_to_do_fails_and_rich_keeping_it_passes(self):
        bad = self.probe('write-it-yourself', 'new_stu_handoff')
        self.assertEqual(bad.returncode, 1, bad.stderr)
        self.assertIn('write-it-yourself: 0 of 2 passed', bad.stdout)
        good = self.probe('write-it-yourself', 'rich_handles_message')
        self.assertEqual(good.returncode, 0, good.stderr)
        self.assertIn('write-it-yourself: 2 of 2 passed', good.stdout)

    def test_delegated_work_fails_only_when_rich_keeps_it(self):
        self.assertEqual(self.probe('delegated-work', 'new_stu_handoff').returncode, 0)
        self.assertEqual(self.probe('delegated-work', 'rich_handles_message').returncode, 1)

    def test_an_answer_that_is_not_exactly_a_label_leaves_the_request_with_rich_as_the_app_does(self):
        r = self.probe('write-it-yourself-first', 'new_stu_handoff\n\nBecause it is long.')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn('not a label, so Rich keeps it', r.stdout)

    def test_the_classifier_starts_and_is_fed_as_the_app_does(self):
        self.probe('write-it-yourself', 'rich_handles_message')
        rows = self.rows()
        argv = rows[0]['argv']
        for flag, value in [('--model', 'claude-haiku-5-5'), ('--effort', 'low'), ('--tools', ''),
                            ('--setting-sources', ''), ('--system-prompt', 'You classify.\n')]:
            self.assertEqual(argv[argv.index(flag) + 1], value)
        self.assertIn('--no-session-persistence', argv)
        sent = [row['sent'] for row in rows if 'sent' in row]
        # One run: the warm-up, the two earlier requests of run P, then the judged one.
        self.assertEqual(sent[0], '{"warmup":true}')
        judged = json.loads(sent[3])
        self.assertEqual(list(judged), ['recent_conversation', 'roles', 'user_request'])
        self.assertEqual(judged['roles'], {'rich': 'You are Rich.\n', 'stu': 'You are Stu.\n'})
        self.assertIn('Write it yourself, right here in the chat.', judged['user_request'])
        self.assertIn('user: What is 17 times 19?', judged['recent_conversation'])

    def test_missing_engine_or_claude_is_refused(self):
        self.assertEqual(self.probe('write-it-yourself', 'x', engine=self.log.parent / 'none').returncode, 2)
        self.assertEqual(self.probe('write-it-yourself', 'x', claude=self.log.parent / 'none').returncode, 2)


if __name__ == '__main__':
    result = unittest.main(exit=False, verbosity=1).result
    print('OK' if result.wasSuccessful() else 'FAILED')
    sys.exit(0 if result.wasSuccessful() else 1)

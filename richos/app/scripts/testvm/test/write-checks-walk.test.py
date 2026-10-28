#!/usr/bin/env python3
"""write-checks-walk.py's verdict, offline: PASS only when both checks really met the teammate's writes.

The good record: walkwriter asked to write the key and the British word, the key's write never
completed and its refusal is in the transcript, and the note on disk holds the American word. The
hollow records are the ways a walk could look green without the checks: the teammate never wrote
(or corrected the word itself before writing), the key reached the disk, the note kept the
British word.
"""
import importlib.util
import json
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('write_checks_walk', HERE / 'write-checks-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


def call(event, name, content):
    return {'event': event, 'tool': 'Write', 'agent_id': 'a1', 'agent_type': 'richos-app-engine:' + walk.TEAMMATE,
            'input': json.dumps({'file_path': '/w/' + name, 'content': content}), 'response': ''}


NOTE = f'The {walk.BRITISH} of the walk.\n'
SECRET = f'aws_access_key_id = {walk.KEY}\n'
CALLS = [call('PreToolUse', walk.DIALECT_FILE, NOTE), call('PostToolUse', walk.DIALECT_FILE, NOTE),
         call('PreToolUse', walk.SECRET_FILE, SECRET)]
TRANSCRIPTS = [{'meta': {'agentType': walk.TEAMMATE},
                'text': 'PreToolUse:Write hook error: === refused === (hook: scripts/hooks/scan-secrets.sh)'}]
FOUND = {walk.DIALECT_FILE: [f'The {walk.AMERICAN} of the walk.\n'], walk.SECRET_FILE: []}


class Verdict(unittest.TestCase):
    def test_both_checks_met_the_writes(self):
        failures, summary = walk.judge(CALLS, TRANSCRIPTS, FOUND)
        self.assertEqual(failures, [])
        self.assertEqual(summary['dialect'], 'fixed')

    def test_a_teammate_that_never_wrote_fails(self):
        failures, _ = walk.judge([], TRANSCRIPTS, {walk.DIALECT_FILE: [], walk.SECRET_FILE: []})
        self.assertTrue(any('made no tool call' in f for f in failures), failures)

    def test_a_word_the_teammate_corrected_itself_proves_nothing(self):
        own = [call('PreToolUse', walk.DIALECT_FILE, f'The {walk.AMERICAN} of the walk.\n')] + CALLS[2:]
        failures, _ = walk.judge(own, TRANSCRIPTS, FOUND)
        self.assertTrue(any('British spelling' in f for f in failures), failures)

    def test_a_key_on_disk_fails(self):
        failures, _ = walk.judge(CALLS, TRANSCRIPTS, dict(FOUND, **{walk.SECRET_FILE: [SECRET]}))
        self.assertTrue(any('on disk' in f for f in failures), failures)

    def test_a_completed_key_write_fails(self):
        failures, _ = walk.judge(CALLS + [call('PostToolUse', walk.SECRET_FILE, SECRET)], TRANSCRIPTS, FOUND)
        self.assertTrue(any('completed' in f for f in failures), failures)

    def test_the_british_word_left_on_disk_fails(self):
        failures, _ = walk.judge(CALLS, TRANSCRIPTS, dict(FOUND, **{walk.DIALECT_FILE: [NOTE]}))
        self.assertTrue(any('neither fixed nor refused' in f for f in failures), failures)

    def test_a_path_with_a_space_stays_one_path(self):
        out = ('/h/Library/Application Support/com.richos.app/engine-state/target-worktrees/x/w/'
               + walk.DIALECT_FILE + '\n/h/Acme/' + walk.DIALECT_FILE + '\n')
        self.assertEqual(walk.paths_of(out), [
            '/h/Library/Application Support/com.richos.app/engine-state/target-worktrees/x/w/' + walk.DIALECT_FILE,
            '/h/Acme/' + walk.DIALECT_FILE])
        self.assertEqual(walk.paths_of(''), [])

    def test_this_file_is_not_itself_a_key_or_a_british_word(self):
        source = (HERE / 'write-checks-walk.py').read_text()
        self.assertNotIn(walk.KEY, source)
        self.assertNotIn(walk.BRITISH, source)


if __name__ == '__main__':
    unittest.main()

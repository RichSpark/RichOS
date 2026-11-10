#!/usr/bin/env python3
"""linkedin-app-walk.py's process-chain verdict and its reading of Claude Code's transcripts, offline.
The live walk in a guest is its own proof."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('linkedin_app_walk', HERE / 'linkedin-app-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

APP = '/Users/admin/testvm/walk-x/RichOS.app/Contents/MacOS/richos-tauri'


def link(pid, command):
    return {'pid': pid, 'command': command}


class Chain(unittest.TestCase):
    def setUp(self):
        self.w = walk.Walk.__new__(walk.Walk)
        self.w.app_pid = '1026'

    def test_the_tool_under_claude_under_the_app_through_the_supervisor_counts(self):
        # As measured in walk-59ad5d5270c7: app -> provider-supervisor.py -> claude -> the tool.
        chain = [link('2130', APP + ' --linkedin-mcp'), link('2092', '/Users/admin/.local/bin/claude --add-dir x'),
                 link('2091', '/x/python3 -B /x/provider-supervisor.py'), link('1026', APP)]
        self.assertTrue(self.w.under_app(chain))

    def test_the_tool_started_from_ssh_or_without_claude_does_not(self):
        ssh = [link('1962', APP + ' --linkedin-mcp'), link('1947', 'zsh -c printf ...'), link('1945', 'sshd-session')]
        self.assertFalse(self.w.under_app(ssh))
        no_claude = [link('2130', APP + ' --linkedin-mcp'), link('1026', APP)]
        self.assertFalse(self.w.under_app(no_claude))
        self.assertFalse(self.w.under_app(None))


class Transcripts(unittest.TestCase):
    def test_each_call_once_with_its_browser_and_its_hidden_text(self):
        w = walk.Walk.__new__(walk.Walk)
        w.li = type('Li', (), {'manifest': {'expect': {'profile_hidden': 'P', 'details_hidden': 'D',
                                                         'companies': {'acme': 'C'}},
                                            'about': {}, 'experience_keys': ['acme']}})()
        w.rounds = {'safari': {'revoked_at': 1000.0}}
        answer = json.dumps({'browser': 'Safari', 'profile': {'text': 'P D C'}})
        use = {'type': 'tool_use', 'id': 'u1', 'name': walk.TOOL, 'input': {'url': 'x'}}
        rows = [{'timestamp': '2026-11-10T15:34:56.529Z', 'message': {'content': [use]}},
                {'message': {'content': [use]}},  # the same call written again, without a time
                {'timestamp': '2026-11-10T15:35:57.202Z', 'message': {'content': [
                    {'type': 'tool_result', 'tool_use_id': 'u1', 'content': [{'type': 'text', 'text': answer}]}]}}]
        with tempfile.TemporaryDirectory() as d:
            w.out = Path(d)
            (w.out / 'native-projects' / 'p').mkdir(parents=True)
            (w.out / 'native-projects' / 'p' / 's.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
            with self.assertRaises(walk.StepFailed) as failed:
                w.judge_transcripts()
        message = str(failed.exception)
        self.assertNotIn('Safari:', message, message)  # Safari's call passed: once, every hidden text
        self.assertIn('no read_profile call answered from Google Chrome', message)
        self.assertEqual(message.count('"browser": "Safari"'), 1, message)


if __name__ == '__main__':
    unittest.main()

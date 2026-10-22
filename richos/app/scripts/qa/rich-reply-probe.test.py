#!/usr/bin/env python3
"""Offline proof of rich-reply-probe.py against a stand-in claude. No model, no app, no network."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).with_name('rich-reply-probe.py')

# A stand-in for `claude --print --input-format=stream-json ...`: it records its argv, writes a
# session file where Claude Code would, and answers with FAKE_REPLY, or for a message holding
# FAKE_HANG it starts streaming and never finishes (the probe must cut it off).
FAKE = r'''#!/usr/bin/env python3
import json, os, re, sys, time
from pathlib import Path
argv = sys.argv[1:]
with open(os.environ['FAKE_LOG'], 'a') as log:
    log.write(json.dumps(argv) + '\n')
message = json.loads(sys.stdin.readline())
text = message['message']['content'][0]['text']
project = Path.home() / '.claude' / 'projects' / re.sub(r'[^A-Za-z0-9]', '-', os.getcwd())
project.mkdir(parents=True, exist_ok=True)
(project / 'session.jsonl').write_text(text + '\n')
def send(frame):
    print(json.dumps(frame), flush=True)
if os.environ.get('FAKE_HANG') and os.environ['FAKE_HANG'] in text:
    send({'type': 'stream_event', 'event': {'type': 'message_start'}})
    time.sleep(60)
    sys.exit(0)
reply = os.environ['FAKE_REPLY']
if not os.environ.get('FAKE_UNSTREAMED'):
    send({'type': 'stream_event', 'event': {'type': 'message_start'}})
if os.environ.get('FAKE_BASH'):
    # The folder the case names is there for Rich to find, as it was in the guest.
    if not Path('Acme/heartbeat.py').is_file():
        sys.exit('no Acme/heartbeat.py in the working folder')
    send({'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'id': 'toolu_1', 'name': 'Bash',
          'input': {'command': os.environ['FAKE_BASH']}}]}})
send({'type': 'assistant', 'message': {'content': [{'type': 'text', 'text': reply}]}})
send({'type': 'result', 'subtype': 'success'})
'''


class Probe(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name).resolve()
        self.home = root / 'home'
        self.home.mkdir()
        self.plugin = root / 'plugin'
        (self.plugin / 'agents').mkdir(parents=True)
        (self.plugin / 'agents' / 'rich.md').write_text('You are Rich.\n')
        self.claude = root / 'claude'
        self.claude.write_text(FAKE)
        self.claude.chmod(0o755)
        self.log = root / 'argv.jsonl'
        self.tmp = root / 'tmp'
        self.tmp.mkdir()

    def tearDown(self):
        self.dir.cleanup()

    def probe(self, case, reply, hang='', unstreamed='', bash=''):
        env = dict(os.environ, HOME=str(self.home), TMPDIR=str(self.tmp), FAKE_LOG=str(self.log),
                   FAKE_REPLY=reply, FAKE_HANG=hang, FAKE_UNSTREAMED=unstreamed, FAKE_BASH=bash)
        return subprocess.run([sys.executable, str(TOOL), '--plugin', str(self.plugin), '--case', case,
                               '--runs', '2', '--claude', str(self.claude)],
                              capture_output=True, text=True, env=env, timeout=60)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_a_note_to_himself_fails_and_a_plain_reply_passes(self):
        bad = self.probe('handoff-note', 'Handoff already saved; Stu owns it. Just acknowledge.\n\nOn it.')
        self.assertEqual(bad.returncode, 1, bad.stderr)
        self.assertIn('handoff-note: 0 of 2 passed', bad.stdout)
        good = self.probe('handoff-note', "I've passed this to Stu. I'll tell you when it's pushed.")
        self.assertEqual(good.returncode, 0, good.stderr)
        self.assertIn('handoff-note: 2 of 2 passed', good.stdout)

    def test_rich_running_a_handed_over_command_himself_fails(self):
        ran = self.probe('handed-commands', 'Both are running.',
                         bash='cd Acme && python3 heartbeat.py beat stop-bg /tmp/reap-walk')
        self.assertEqual(ran.returncode, 1, ran.stderr)
        self.assertIn('handed-commands: 0 of 2 passed', ran.stdout)
        self.assertIn('[Bash] cd Acme && python3 heartbeat.py', ran.stdout)
        # A look around is not starting the job's command.
        looked = self.probe('handed-commands', "Stu is running both and I'll tell you when they finish.",
                            bash='ls Acme')
        self.assertEqual(looked.returncode, 0, looked.stderr)
        self.assertIn('handed-commands: 2 of 2 passed', looked.stdout)

    def test_rich_starts_as_the_app_starts_him_with_the_plugin_copy_and_its_hook(self):
        self.probe('handoff-note', 'On it.')
        argv = self.calls()[0]
        for flag, value in [('--agent', 'rich-skills:rich'), ('--model', 'opus'), ('--effort', 'low'),
                            ('--setting-sources', '')]:
            self.assertEqual(argv[argv.index(flag) + 1], value)
        plugin = Path(argv[argv.index('--plugin-dir') + 1])
        self.assertNotEqual(plugin, self.plugin, 'the source plugin is copied, never changed')
        self.assertFalse((self.plugin / 'hooks').exists())

    def test_the_cut_off_question_is_killed_then_the_session_resumed(self):
        answered = self.probe('cutoff-question', '2 plus 3 is 5. I also skipped your last question: 7 plus 7 is 14.',
                              hang='7 plus 7')
        self.assertEqual(answered.returncode, 1, answered.stderr)
        calls = self.calls()
        self.assertEqual(len(calls), 6, 'three turns per run, two runs')
        self.assertIn('--session-id', calls[0])
        self.assertIn('--resume', calls[1])
        self.assertIn('--resume', calls[2])
        quiet = self.probe('cutoff-question', '2 plus 3 is 5.', hang='7 plus 7')
        self.assertEqual(quiet.returncode, 0, quiet.stderr)

    def test_a_turn_that_cannot_be_cut_off_is_no_verdict(self):
        # A whole answer with no streamed start: nothing was there to cut off.
        r = self.probe('cutoff-question', 'anything', unstreamed='1')
        self.assertEqual(r.returncode, 2)
        self.assertIn('could not be judged', r.stderr)

    def test_the_sessions_it_wrote_are_removed(self):
        self.probe('handoff-note', 'On it.')
        projects = self.home / '.claude' / 'projects'
        self.assertEqual(list(projects.iterdir()) if projects.exists() else [], [])

    def test_missing_plugin_or_claude_is_refused(self):
        env = dict(os.environ, HOME=str(self.home), TMPDIR=str(self.tmp))
        r = subprocess.run([sys.executable, str(TOOL), '--plugin', str(self.tmp), '--case', 'handoff-note',
                            '--claude', str(self.claude)], capture_output=True, text=True, env=env)
        self.assertEqual(r.returncode, 2)
        r = subprocess.run([sys.executable, str(TOOL), '--plugin', str(self.plugin), '--case', 'handoff-note',
                            '--claude', str(self.tmp / 'nothing')], capture_output=True, text=True, env=env)
        self.assertEqual(r.returncode, 2)


if __name__ == '__main__':
    result = unittest.main(exit=False, verbosity=1).result
    print('OK' if result.wasSuccessful() else 'FAILED')
    sys.exit(0 if result.wasSuccessful() else 1)

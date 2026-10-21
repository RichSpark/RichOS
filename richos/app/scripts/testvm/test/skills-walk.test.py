#!/usr/bin/env python3
"""skills-walk.py's `skills` step, offline: it passes only when the worker really used playwright-cli.

The successful record is the shape of the first real walk (2026-10-21, guest walk-31043ef547d7):
walkqa's Skill call for richos-app-engine:playwright-cli succeeded, its own Bash call opened the
requested page and printed its title, the stand-in logged the open, and the assignment settled.
The hollow record is the second review's fixture (rv-20261021T195905Z-1153403d-7a1f, P3): the
assignment failed, the Skill call answered "Unknown skill", and the only command was --help. The
step drives the real SkillsWalk.skills() with the guest replaced, so the verdict is the walk's own.
"""
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('skills_walk', HERE / 'skills-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

AGENT = 'a1296edf748a4686e'
INIT = json.dumps({'payload': {'type': 'system', 'subtype': 'init',
                               'skills': ['richos-app-engine:playwright-cli', 'rich-skills:watch-video']}})
SKILL = {'skill': 'richos-app-engine:playwright-cli'}
OPEN = {'command': 'playwright-cli open https://example.com/ 2>&1'}
PAGE = json.dumps({'stdout': '### Page\n- Page URL: https://example.com/\n- Page Title: ' + walk.TITLE})


def call(event, tool, given, response='', agent=AGENT):
    return {'event': event, 'tool': tool, 'agent_id': agent, 'agent_type': 'richos-app-engine:walkqa',
            'input': json.dumps(given), 'response': response}


# As walk 1 recorded it: the app runs a worker's shell call in the background, so the open call
# completes with no output and the page comes back in the later `agent_hold.py wait` result, which
# that walk recorded as a PostToolUseFailure because it also reported a refused task.
WAIT = {'command': 'python3 /fixture/engine/scripts/lib/agent_hold.py wait'}
GOOD = [call('PreToolUse', 'Skill', SKILL),
        call('PostToolUse', 'Skill', SKILL, json.dumps({'success': True, 'commandName': SKILL['skill']})),
        call('PreToolUse', 'Bash', OPEN),
        call('PostToolUse', 'Bash', OPEN, json.dumps({'stdout': '', 'backgroundTaskId': 'b3bs2u6w2'})),
        call('PostToolUseFailure', 'Bash', WAIT, 'Exit code 2\nTASK refused (tool x) EXIT STATUS 2\n'
             'TASK b3bs2u6w2 (tool y) EXIT STATUS 0\n### Page\n- Page URL: https://example.com/\n'
             '- Page Title: ' + walk.TITLE)]
# The same use with the page printed by the open call itself.
GOOD_DIRECT = GOOD[:3] + [call('PostToolUse', 'Bash', OPEN, PAGE)]
GOOD_LOG = '1792607035 open https://example.com/\n1792607066 close\n'
HOLLOW = [call('PreToolUse', 'Skill', {'skill': 'playwright-cli'}),
          call('PostToolUseFailure', 'Skill', {'skill': 'playwright-cli'}, 'Unknown skill: playwright-cli'),
          call('PreToolUse', 'Bash', {'command': 'playwright-cli --help'}),
          call('PostToolUse', 'Bash', {'command': 'playwright-cli --help'}, json.dumps({'stdout': 'ok'}))]
HOLLOW_LOG = '1792607035 --help\n'


def skills_step(calls, state, log):
    """Run SkillsWalk.skills() against a recorded back end instead of a guest."""
    w = walk.SkillsWalk.__new__(walk.SkillsWalk)
    w.a = SimpleNamespace(within=0.5)
    w.vm, w.out, w.home, w.data, w.facts = 'vm', Path(tempfile.mkdtemp()), '/fixture/home', '/fixture/data', {}
    w.calls = lambda: calls
    w.ours = lambda sent: {'state': state, 'title': 'Ask walkqa'}
    w.send = lambda text: [0, 1]
    w.approve_if_asked = lambda title: False

    def guest(vm, command, timeout=None):
        if 'init' in command:
            return INIT
        if 'playwright-cli.log' in command:
            return log
        return 'richos-app-engine:playwright-cli\n'
    walk.guest = guest
    return w.skills()


class SkillsStep(unittest.TestCase):
    def test_a_successful_use_of_the_skill_passes(self):
        evidence = skills_step(GOOD, 'settled', GOOD_LOG)
        self.assertEqual(evidence['state'], 'settled')

    def test_the_page_printed_by_the_open_call_itself_passes_too(self):
        self.assertEqual(skills_step(GOOD_DIRECT, 'settled', GOOD_LOG)['state'], 'settled')

    def test_an_open_whose_page_never_came_back_fails(self):
        with self.assertRaises(walk.StepFailed) as raised:
            skills_step(GOOD[:4], 'settled', GOOD_LOG)
        self.assertIn('title', str(raised.exception))

    def test_the_reviewers_hollow_record_fails(self):
        with self.assertRaises(walk.StepFailed) as raised:
            skills_step(HOLLOW, 'failed', HOLLOW_LOG)
        text = str(raised.exception)
        self.assertIn('settled', text)
        self.assertIn('Skill', text)
        self.assertIn(walk.URL, text)

    def test_a_failed_assignment_fails_even_with_a_good_use(self):
        with self.assertRaises(walk.StepFailed):
            skills_step(GOOD, 'failed', GOOD_LOG)

    def test_a_failed_skill_call_fails_even_when_the_command_ran(self):
        with self.assertRaises(walk.StepFailed):
            skills_step([call('PostToolUseFailure', 'Skill', SKILL, 'Unknown skill')] + GOOD[2:], 'settled', GOOD_LOG)

    def test_a_command_that_never_opened_the_page_fails(self):
        with self.assertRaises(walk.StepFailed):
            skills_step(GOOD[:2] + HOLLOW[2:], 'settled', HOLLOW_LOG)


if __name__ == '__main__':
    unittest.main()

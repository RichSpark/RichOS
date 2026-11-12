#!/usr/bin/env python3
"""native-transcript.py --actors on fictional transcripts written here: who performed a declared
action, tied to the launch that made that actor, and the verdict the shared check gives.

The shapes are Claude Code's own (2.1.322): a main session's rows carry `agentSetting`; a
helper's file is `<session>/subagents/agent-<id>.jsonl` beside a `.meta.json` with `agentType`
and the `toolUseId` of the parent's Agent call. Nothing real is read and nothing is written
outside a temporary folder.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

READER = Path(__file__).resolve().parents[1] / 'native-transcript.py'
LI = 'mcp__richos_linkedin__read_profile'


def use(ts, cid, name, inp, agent='rich-skills:stu'):
    row = {'timestamp': ts, 'type': 'assistant', 'message': {'role': 'assistant', 'content': [
        {'type': 'tool_use', 'id': cid, 'name': name, 'input': inp}]}}
    if agent:
        row['agentSetting'] = agent
    return row


def result(ts, cid, text, error=False):
    return {'timestamp': ts, 'type': 'user', 'message': {'role': 'user', 'content': [
        {'type': 'tool_result', 'tool_use_id': cid, 'is_error': error, 'content': text}]}}


class Actors(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='actors ')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def session(self, sid, rows):
        (self.root / f'{sid}.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))

    def helper(self, sid, aid, agent_type, launch, rows):
        folder = self.root / sid / 'subagents'
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f'agent-{aid}.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
        (folder / f'agent-{aid}.meta.json').write_text(json.dumps({'agentType': agent_type, 'toolUseId': launch}))

    def check(self, *expect, extra=()):
        args = [sys.executable, str(READER), str(self.root), '--actors',
                '--action', f'linkedin={LI}', '--action', 'website=WebFetch~walkergoods.example', '--json', *extra]
        for e in expect:
            args += ['--expect', e]
        done = subprocess.run(args, capture_output=True, text=True, timeout=30)
        return done.returncode, json.loads(done.stdout) if done.stdout.strip() else done.stderr

    def specialist_did_both(self):
        self.session('stu00001', [
            use('2026-11-10T10:00:00.000Z', 'a1', 'Agent', {'subagent_type': 'richos-app-engine:reed', 'description': 'Read sources'}),
            result('2026-11-10T10:03:00.000Z', 'a1', 'done')])
        self.helper('stu00001', 'r1', 'richos-app-engine:reed', 'a1', [
            use('2026-11-10T10:00:10.000Z', 'l1', LI, {'url': 'https://www.linkedin.com/in/ada/'}, agent=None),
            result('2026-11-10T10:00:40.000Z', 'l1', '{"profile": "Ada"}'),
            use('2026-11-10T10:01:00.000Z', 'w1', 'WebFetch', {'url': 'https://walkergoods.example/'}, agent=None),
            result('2026-11-10T10:01:05.000Z', 'w1', 'Walker Goods makes pans.')])

    def test_a_specialist_performing_both_reads_passes_and_is_tied_to_stus_launch(self):
        self.specialist_did_both()
        code, out = self.check('linkedin=specialist', 'website=specialist')
        self.assertEqual(code, 0, out)
        self.assertEqual({k: v['verdict'] for k, v in out['verdicts'].items()}, {'linkedin': 'PASS', 'website': 'PASS'})
        li = [r for r in out['records'] if r['action'] == 'linkedin'][0]
        self.assertEqual(li['actor'], 'reed')
        self.assertEqual(li['launched_by'], {'session': 'stu00001', 'agent_call': 'a1', 'actor': 'stu',
                                             'subagent_type': 'richos-app-engine:reed', 'description': 'Read sources'})

    def test_stu_reading_linkedin_himself_fails_as_the_earlier_rounds_did(self):
        self.session('stu00001', [
            use('2026-11-10T10:00:00.000Z', 'l0', LI, {'url': 'https://www.linkedin.com/in/ada/'}),
            result('2026-11-10T10:00:30.000Z', 'l0', '{"profile": "Ada"}')])
        code, out = self.check('linkedin=specialist')
        self.assertEqual(code, 1)
        self.assertEqual(out['verdicts']['linkedin'], {'verdict': 'FAIL', 'why': 'performed by stu itself'})

    def test_a_wrong_actor_website_read_fails_even_when_linkedin_was_delegated(self):
        self.specialist_did_both()
        self.session('rich0001', [
            use('2026-11-10T10:02:00.000Z', 'w9', 'WebFetch', {'url': 'https://walkergoods.example/about'}, agent='rich-skills:rich'),
            result('2026-11-10T10:02:05.000Z', 'w9', 'About us')])
        code, out = self.check('linkedin=specialist', 'website=specialist')
        self.assertEqual(code, 1)
        self.assertEqual(out['verdicts']['linkedin']['verdict'], 'PASS')
        self.assertEqual(out['verdicts']['website'], {'verdict': 'FAIL', 'why': 'performed by rich itself'})

    def test_a_refused_lead_attempt_is_reported_and_does_not_count_as_doing_it(self):
        self.specialist_did_both()
        self.session('stu00002', [
            use('2026-11-10T09:59:00.000Z', 'l0', LI, {'url': 'https://www.linkedin.com/in/ada/'}),
            result('2026-11-10T09:59:01.000Z', 'l0', "The LinkedIn reader is a teammate's tool", error=True)])
        code, out = self.check('linkedin=specialist')
        self.assertEqual(code, 0, out)
        self.assertIn('attempt(s) by a lead were refused', out['verdicts']['linkedin']['why'])

    def test_a_launch_alone_or_a_call_without_a_result_is_not_completion(self):
        self.session('stu00001', [
            use('2026-11-10T10:00:00.000Z', 'a1', 'Agent', {'subagent_type': 'richos-app-engine:reed'})])
        self.helper('stu00001', 'r1', 'richos-app-engine:reed', 'a1', [
            use('2026-11-10T10:00:10.000Z', 'l1', LI, {'url': 'https://www.linkedin.com/in/ada/'}, agent=None)])
        code, out = self.check('linkedin=specialist', 'website=specialist')
        self.assertEqual(code, 1)
        self.assertEqual(out['verdicts']['linkedin']['verdict'], 'FAIL')
        self.assertEqual(out['verdicts']['website'], {'verdict': 'FAIL', 'why': 'never called'})
        self.assertIsNone(out['records'][0]['ok'])

    def test_an_error_result_does_not_count(self):
        self.session('stu00001', [use('2026-11-10T10:00:00.000Z', 'a1', 'Agent', {'subagent_type': 'richos-app-engine:reed'})])
        self.helper('stu00001', 'r1', 'richos-app-engine:reed', 'a1', [
            use('2026-11-10T10:00:10.000Z', 'l1', LI, {'url': 'https://www.linkedin.com/in/ada/'}, agent=None),
            result('2026-11-10T10:01:10.000Z', 'l1', 'The LinkedIn pages were NOT read', error=True)])
        code, out = self.check('linkedin=specialist')
        self.assertEqual(code, 1)
        self.assertEqual(out['verdicts']['linkedin']['why'], 'called, but no allowed actor got a result without error')

    def test_a_named_role_can_be_required(self):
        self.specialist_did_both()
        self.assertEqual(self.check('linkedin=clark')[0], 1)
        self.assertEqual(self.check('linkedin=reed|clark')[0], 0)

    def test_an_earlier_onboardings_read_never_passes_the_one_being_checked(self):
        # The review's case: company A's read succeeded at 09:00, company B's failed at 10:00.
        for sid, hour, failed in (('compA001', '09', False), ('compB001', '10', True)):
            self.session(sid, [use(f'2026-11-10T{hour}:00:00.000Z', 'a-' + sid, 'Agent', {'subagent_type': 'richos-app-engine:reed'})])
            self.helper(sid, 'r', 'richos-app-engine:reed', 'a-' + sid, [
                use(f'2026-11-10T{hour}:00:10.000Z', 'l-' + sid, LI, {'url': f'https://www.linkedin.com/in/{sid}/'}, agent=None),
                result(f'2026-11-10T{hour}:00:40.000Z', 'l-' + sid, 'NOT read' if failed else 'Profile', error=failed)])
        self.assertEqual(self.check('linkedin=specialist')[0], 0)  # unscoped: A's read answers for B
        code, out = self.check('linkedin=specialist', extra=['--since', '2026-11-10T09:30:00.000Z'])
        self.assertEqual(code, 1)
        self.assertEqual(out['verdicts']['linkedin']['why'], 'called, but no allowed actor got a result without error')
        self.assertEqual({r['session'] for r in out['records']}, {'compB001/r'})

    def test_an_expectation_without_its_action_is_refused(self):
        self.specialist_did_both()
        self.assertEqual(self.check('phone=specialist')[0], 2)


if __name__ == '__main__':
    unittest.main()

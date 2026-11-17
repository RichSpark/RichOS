"""Full-hook regressions for payload matches exceeding shell pipe buffers.

RICHOS_LONG_PAYLOAD_TEST_ROOT selects an older engine for negative controls.
All mutable stores and acknowledgement logs live under a temporary HOME/entity.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ENGINE = Path(os.environ.get('RICHOS_LONG_PAYLOAD_TEST_ROOT', Path(__file__).resolve().parents[2]))
BASH = shutil.which('bash')
SID = 'feedface-0000-4000-8000-000000000000'
# Put the meaningful match on its own FIRST line. One enormous single line
# would make grep read the whole line before exiting and miss the actual bug.
PADDING = 'ordinary background context\n' * 40000


class LongPayload(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='long-payload-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.entity = self.root / 'entity'
        (self.entity / '.claude/agents').mkdir(parents=True)
        (self.entity / '.claude/agents/dev.md').write_text('---\nname: dev\nmodel: opus\n---\nDeveloper\n')
        (self.entity / 'orchestration.config').write_text('CREATOR_TEAMMATE="dean"\nENABLE_QA_INSTALL_FRESH_GATE=0\n')
        self.env = {
            'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'HOME': str(self.root / 'home'),
            'TMPDIR': str(self.root), 'LANG': 'C', 'LC_ALL': 'C',
            'RICHOS_ENTITY_ROOT': str(self.entity), 'CLAUDE_PROJECT_DIR': str(self.entity),
            'VERIFY_REPO_ROOT_OVERRIDE': str(self.entity),
            'RICHOS_WORKTREE_LEDGER': str(self.root / 'ledger.jsonl'),
            'RICHOS_WORKSPACES_DIR': str(self.root / 'ws'),
            'RICHOS_WORKTREE_CAPTURE_DIR': str(self.root / 'captures'),
            'V8_TEAMS_DIR_OVERRIDE': str(self.root / 'teams'),
            'RESUME_GUARD_TEAMS_DIR': str(self.root / 'teams'),
            'SEAL_WAIT_SECONDS': '0',
        }
        Path(self.env['HOME']).mkdir()
        # A session process of this test's own: no registration borrows the
        # identity of whatever real session happens to be running the suite.
        session = subprocess.Popen(['sleep', '600'])
        self.addCleanup(session.wait)
        self.addCleanup(session.kill)
        self.env['RICHOS_SESSION_PID'] = str(session.pid)

    def run_hook(self, name, payload, expected, *, env=None):
        raw = payload if isinstance(payload, str) else json.dumps(payload)
        result = subprocess.run([BASH, str(ENGINE / 'scripts/hooks' / name)],
                                input=raw, text=True, capture_output=True, cwd=self.entity,
                                env=env or self.env, timeout=45)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result.stdout + result.stderr

    def agent(self, first, **extra):
        return {'tool_name': 'Agent', 'session_id': SID, 'tool_use_id': 'tool-long-payload',
                'tool_input': {'prompt': first + '\n' + PADDING, 'subagent_type': 'dev',
                               'name': 'dev-opus-high-long1', 'effort': 'high', **extra}}

    def test_long_prompt_cannot_hide_subagent_spawning(self):
        output = self.run_hook('verify-agent-prompt.sh',
                               self.agent('Use the Agent tool to spawn agents for each module.'), 2)
        self.assertIn('subagent-as-spawner', output)

    def test_long_prompt_cannot_hide_missing_native_isolation(self):
        output = self.run_hook('verify-agent-prompt.sh', self.agent(
            'Native isolation has already created your worktree. Build the feature there.'), 2)
        self.assertIn('missing-worktree-isolation', output)

    def test_long_prompt_retains_valid_inflight_ack_contract(self):
        self.run_hook('verify-agent-prompt.sh', self.agent(
            'When main moves, run scripts/inflight-ack.sh --sha <sha> --impact none --detail checked --paths none.',
            isolation='worktree'), 0)

    def test_long_prompt_retains_explicit_main_checkout_ack(self):
        marker = 'main-checkout-run: Serialized maintenance in this fixture checkout is explicitly authorized.'
        self.run_hook('guard-worktree-isolation.sh', self.agent(marker), 0)
        log = self.entity / '.claude/state/main-checkout-runs.log'
        self.assertIn(marker, log.read_text())
        # The permitted spawn is registered at spawn (docs/plans/worktree-spec-
        # 2026-09-11.md, point 3), with no isolation recorded.
        registrations = list((self.root / 'ws' / 'agents').glob('*--dev-opus-high-long1.json'))
        self.assertEqual(len(registrations), 1, 'the permitted spawn was never registered')
        self.assertEqual(json.loads(registrations[0].read_text()).get('isolation'), '')

    def test_long_resume_retains_ack_and_does_not_invent_one(self):
        payload = {'tool_name': 'SendMessage', 'session_id': SID,
                   'tool_input': {'to': 'dev-done', 'message': 'Please continue.\n' + PADDING}}
        self.run_hook('guard-resume-isolation.sh', payload, 2)
        marker = 'resume-ack: Pure question with no file writes; confirm the previously reported commit.'
        payload['tool_input']['message'] = marker + '\n' + PADDING
        self.run_hook('guard-resume-isolation.sh', payload, 0)
        self.assertIn(marker, (self.entity / '.claude/state/resume-acks.log').read_text())

    def test_degraded_write_barrier_sees_early_agent_id_in_long_payload(self):
        fakebin = self.root / 'no-python'
        fakebin.mkdir()
        for tool in ('cat', 'grep', 'sed'):
            (fakebin / tool).symlink_to(shutil.which(tool))
        env = {**self.env, 'PATH': str(fakebin)}
        payload = {'agent_id': 'a1234567890123456', 'tool_name': 'Write',
                   'session_id': SID, 'tool_input': {'content': 'x' * 1000000}}
        valid = json.dumps(payload, indent=2)
        for raw in (valid, valid[:-2]):
            with self.subTest(valid_json=raw == valid):
                output = self.run_hook('guard-sealed-worktree.sh', raw, 2, env=env)
                self.assertIn('python3 is unavailable', output)
                self.assertIn('REFUSED', output)


if __name__ == '__main__':
    unittest.main()

"""Exercise the generated payload against the shipped prompt verifier."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("prepare_spawn", SCRIPTS / "prepare-agent-spawn.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SpawnPreparation(unittest.TestCase):
    def test_missing_fields_report_together_and_explicit_modes_are_not_overridden(self):
        with self.assertRaisesRegex(ValueError, "missing name; missing subagent_type; missing prompt"):
            module.prepare({})
        for extra in ({"cwd": "/tmp/worktree"}, {"isolation": "remote"}, {"resume": "agent-1"}):
            with self.assertRaises(ValueError):
                module.prepare(dict(name="dev-opus-t1", subagent_type="dev", prompt="Build", **extra))

    def test_existing_helper_or_file_contract_is_not_duplicated(self):
        for prompt in ("On main changes run ~/.claude/richos-engine/scripts/inflight-ack.sh with the required fields.",
                       "Write .claude/inflight-acks/<sha12>.<teammate>.ack with sha/impact/detail/paths/teammate."):
            result = module.prepare(dict(name="dev-opus-t1", subagent_type="dev", prompt=prompt))
            self.assertEqual(result["prompt"], prompt)
            self.assertEqual(result["isolation"], "worktree")

    def test_prepared_payload_passes_real_verifier_and_keeps_task_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".claude/agents").mkdir(parents=True)
            (root / ".claude/agents/dev.md").write_text("---\nname: dev\n---\nbody\n")
            (root / "orchestration.config").write_text('CREATOR_TEAMMATE="dean"\nENABLE_QA_INSTALL_FRESH_GATE=0\n')
            env = dict(os.environ, RICHOS_ENTITY_ROOT=tmp, VERIFY_REPO_ROOT_OVERRIDE=tmp,
                       V8_TEAMS_DIR_OVERRIDE=str(root / "teams"))
            env.pop("CLAUDE_PROJECT_DIR", None)
            for task in ("Read the worktree and audit the queue", "Build the worktree change"):
                original = dict(name="dev-opus-t1", subagent_type="dev", prompt=task,
                                model="opus", run_in_background=True)
                def verify(value):
                    return subprocess.run(["bash", str(SCRIPTS / "hooks/verify-agent-prompt.sh")],
                        input=json.dumps({"session_id": "test-session", "tool_name": "Agent", "cwd": tmp, "tool_input": value}),
                        text=True, capture_output=True, env=env)
                bad = verify(original)
                self.assertEqual(bad.returncode, 2, bad.stderr)
                self.assertIn("ack-contract-missing", bad.stderr)
                prepared = module.prepare(original)
                self.assertEqual(prepared["isolation"], "worktree")
                self.assertEqual(prepared["model"], "opus")
                self.assertTrue(prepared["run_in_background"])
                self.assertTrue(prepared["prompt"].startswith(task))
                self.assertIn("run `git merge main` inside your own worktree", prepared["prompt"])
                self.assertIn("Merge only once", prepared["prompt"])
                # The rollout (verification reliability PRD line 49): the explicit-range runner
                # step the land reuses, in place of repeating the proof after any merge.
                self.assertIn("run `git merge-base main HEAD` in your worktree", prepared["prompt"])
                self.assertIn("`python3 richos/app/scripts/proof-run.py <the commit it printed>..HEAD`",
                              prepared["prompt"])
                self.assertIn("the land reuses that run instead of repeating it", prepared["prompt"])
                self.assertNotIn("once more on the merged tree", prepared["prompt"])
                self.assertNotIn("inflight-ack", prepared["prompt"])
                self.assertEqual(module.prepare(prepared), prepared)
                good = verify(prepared)
                self.assertEqual(good.returncode, 0, good.stderr)
                self.assertNotIn("ceo-todos-deferred:", prepared["prompt"])
                self.assertNotIn("main-checkout-run:", prepared["prompt"])
            # 2026-10-14: a teammate bound to a release branch merges THAT branch
            # last, never main (the lead cut the merge-main line out of five
            # v1.3.0 briefs by hand), and the spawn guard accepts it.
            original = dict(name="dev-opus-t2", subagent_type="dev", prompt="Build the worktree change")
            for targets, lines in (([("/r/richos", "v1.3.0")], ["run `git merge v1.3.0` inside your own worktree",
                                                               "run `git merge-base v1.3.0 HEAD` in your worktree"]),
                                   ([("/r/femcboost", "main"), ("/r/richos", "v1.3.0")],
                                    ["`git merge main` in /r/femcboost", "`git merge v1.3.0` in /r/richos",
                                     "run `git merge-base <the branch you merged there> HEAD`"])):
                prepared = module.prepare(original, targets)
                for line in lines:
                    self.assertIn(line, prepared["prompt"])
                if len(targets) == 1:
                    self.assertNotIn("git merge main", prepared["prompt"])
                self.assertEqual(module.prepare(prepared, targets), prepared)
                good = verify(prepared)
                self.assertEqual(good.returncode, 0, good.stderr)


if __name__ == "__main__":
    unittest.main()

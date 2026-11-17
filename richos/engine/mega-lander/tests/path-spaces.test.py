#!/usr/bin/env python3
"""The spawn preparer, live guard and registry agree on a path with spaces."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ENGINE = Path(__file__).resolve().parents[2]


class WorkspacePathTests(unittest.TestCase):
    def test_registered_path_is_one_full_line(self):
        with tempfile.TemporaryDirectory(prefix="workspace path ") as temporary:
            root = Path(temporary).resolve()
            seat, target = root / "coordination", root / "target project"
            tree = root / "target worktrees/worker-sonnet-high-spaces"
            # HOME and CLAUDE_CONFIG_DIR are this folder's own: the SessionStart hook collects test
            # devices machine-wide (workspaces.py collect_test_devices, apply=True) under
            # <config>/state/test-devices, and the ownership ledger resolves from HOME. Run by
            # hand with the operator's HOME, both were the operator's record (the gate's record
            # canary, 2026-10-05: <HOME>/.claude/state/test-devices/.lock appeared).
            home = root / "home"
            home.mkdir()
            env = {**os.environ, "HOME": str(home), "CLAUDE_CONFIG_DIR": str(home / ".claude"),
                   "RICHOS_WORKSPACES_DIR": str(root / "state"),
                   "RICHOS_SESSION_ID": "path-probe", "RICHOS_SESSION_PID": str(os.getpid()),
                   "RICHOS_ENTITY_ROOT": str(seat), "CLAUDE_PROJECT_DIR": str(seat),
                   "RICHOS_ENGINE_ROOT": str(ENGINE), "CLAUDE_PLUGIN_ROOT": str(ENGINE),
                   "GUARD_ISOLATION_TEAMS_DIR": str(root / "teams")}
            def run(args, payload=None, ok=True):
                result = subprocess.run([str(a) for a in args], env=env, input=payload,
                                        capture_output=True, text=True)
                if ok:
                    self.assertEqual(result.returncode, 0, result.stderr)
                return result
            ws = [sys.executable, ENGINE / "mega-lander/workspaces.py", "--session", "path-probe"]
            for repo in (seat, target):
                run(["git", "init", "-q", "-b", "main", repo])
                run(["git", "-C", repo, "-c", "user.name=Probe", "-c", "user.email=probe@example.invalid",
                     "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "commit", "--allow-empty", "-qm", "fixture"])
                run(ws + ["integration", "--repo", repo, "--branch", "main", "--why", "Synthetic test"])
            (seat / "orchestration.config").write_text("")
            run(ws + ["hook"], json.dumps({"session_id": "path-probe", "hook_event_name": "SessionStart", "cwd": str(seat)}))
            run(["bash", ENGINE / "mega-lander/create-teammate-worktree.sh", target,
                 "worker-sonnet-high-spaces", "--dir", tree, "--session", "path-probe"])
            prepared = run([sys.executable, ENGINE / "scripts/prepare-agent-spawn.py"], json.dumps({
                "name": "worker-sonnet-high-spaces", "subagent_type": "worker", "model": "sonnet", "effort": "high",
                "prompt": f"cross-repo-worktree: {tree}\nRead the fictional fixture."}))
            payload = {"session_id": "path-probe", "hook_event_name": "PreToolUse", "tool_name": "Agent",
                       "tool_use_id": "tool-probe", "cwd": str(seat), "tool_input": json.loads(prepared.stdout)}
            guard = ["bash", ENGINE / "scripts/hooks/guard-worktree-isolation.sh"]
            run(guard, json.dumps(payload))
            run(ws + ["register-spawn"], json.dumps(payload))
            # A shortened path cannot borrow the actual workspace's registration.
            payload["tool_input"]["prompt"] = f"cross-repo-worktree: {root}/target\nRead the fictional fixture."
            self.assertEqual(run(guard, json.dumps(payload), ok=False).returncode, 2)


if __name__ == "__main__":
    unittest.main()

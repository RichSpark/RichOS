"""G4 regressions: real hook fixtures and controlled ingress time, no live writes."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

# Replay the same assertions against a frozen earlier source tree.
SCRIPTS = Path(os.environ.get("RICHOS_G4_SOURCE", Path(__file__).resolve().parents[1]))
HOOKS = SCRIPTS / "hooks"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class G4(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="hunt-v5-g4."))
        self.addCleanup(shutil.rmtree, self.tmp)
        self.env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        for key in ("CLAUDE_PROJECT_DIR", "CLAUDE_PLUGIN_ROOT", "RICHOS_ENGINE_ROOT"):
            self.env.pop(key, None)

    def run_hook(self, hook, payload, env=None, args=()):
        return subprocess.run(["bash", str(hook), *args], input=json.dumps(payload),
                              capture_output=True, text=True, env=env or self.env,
                              timeout=60)  # Hang guard only; verdicts assert behavior.

    def engine(self, *hooks):
        root = self.tmp / "engine"
        (root / "scripts/hooks").mkdir(parents=True)
        shutil.copytree(SCRIPTS / "lib", root / "scripts/lib",
                        ignore=shutil.ignore_patterns("__pycache__"))
        for hook in hooks:
            shutil.copy2(HOOKS / hook, root / "scripts/hooks" / hook)
        (root / "orchestration.config").write_text('PROTECTED_PATHS="src"\n')
        return root

    def repo(self):
        repo = self.tmp / "repo"
        repo.mkdir()
        self.git(repo, "init", "-q", "-b", "main")
        self.git(repo, "config", "core.hooksPath", "/dev/null")
        self.git(repo, "config", "user.name", "Fixture")
        self.git(repo, "config", "user.email", "fixture@example.invalid")
        self.git(repo, "config", "commit.gpgsign", "false")
        (repo / "seed.txt").write_text("seed\n")
        self.git(repo, "add", "seed.txt")
        self.git(repo, "commit", "-qm", "seed")
        return repo

    def git(self, repo, *args):
        return subprocess.run(["git", "-C", str(repo), *args], capture_output=True,
                              text=True, check=True, env=self.env).stdout.strip()

    def test_53_runtime_filename_is_never_autofixed(self):
        (self.tmp / "colour sample.txt").write_text("fixture\n")
        for ext, source in (("py", 'print(open("colour sample.txt").read().strip())'),
                            ("sh", "cat 'colour sample.txt'")):
            with self.subTest(ext=ext):
                program = self.tmp / ("reader." + ext)
                program.write_text(source)
                before = subprocess.run([sys.executable if ext == "py" else "bash", str(program)],
                                        cwd=self.tmp, capture_output=True, text=True)
                self.assertEqual(before.returncode, 0, before.stderr)
                env = dict(self.env, RICHOS_ENTITY_ROOT=str(SCRIPTS.parent))
                result = self.run_hook(HOOKS / "dialect-guard.sh",
                                       {"tool_name": "Write", "cwd": str(self.tmp),
                                        "tool_input": {"file_path": str(program), "content": source}}, env)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertNotIn("updatedInput", result.stdout)
                self.assertEqual(program.read_text(), source)

    def capture(self, cost, fence=False):
        mod = load("cci_g4", HOOKS / "commit-ceo-inputs.py")
        repo = self.repo()
        document = repo / "wiki/raw/input-ABCDEF.md"
        document.parent.mkdir(parents=True)
        document.write_text("fixture\n")
        assets = document.parent / "assets/ABCDEF-images"
        assets.mkdir(parents=True)
        for n in range(12):
            (assets / (str(n) + ".svg")).write_text("<svg/>\n")
        now = [0.0]
        gates = []
        def gate(script, _repo, path, text, seat):
            gates.append((now[0], path))
            now[0] += cost
            return "pass", ""
        out = io.StringIO()
        with patch.object(mod.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(mod, "run_gate", side_effect=gate), \
             patch.object(mod, "fence_on", return_value=fence), \
             patch.object(mod, "take_lease", return_value=("acquired", [])), \
             patch.object(mod, "commit", return_value=(True, "fixture-sha")) as commit, \
             patch.dict(os.environ, RICHOS_INGRESS_SEAT_ROOT=str(repo),
                        RICHOS_INGRESS_STATE_DIR=str(self.tmp / "state")), \
             patch.object(sys, "stdin", io.StringIO(json.dumps({"prompt": str(document), "cwd": str(repo)}))), \
             contextlib.redirect_stdout(out):
            rc = mod.main()
        return mod, rc, json.loads(out.getvalue()), now[0], gates, commit

    def test_46_companions_stop_at_whole_run_deadline(self):
        mod, rc, result, elapsed, gates, commit = self.capture(2)
        self.assertFalse(commit.called, "over-budget companion group must stay uncommitted")
        self.assertEqual(result["committed"], [])
        self.assertTrue(result["undecided"], result)
        self.assertEqual(rc, 4)
        # load-bound: monotonic time advances only in the mock gate, never with host load.
        self.assertLessEqual(elapsed, mod.DEADLINE_S)
        self.assertTrue(all(start < mod.DEADLINE_S for start, _ in gates))

    def test_46_fast_companion_group_still_commits_atomically(self):
        _, rc, result, _, gates, commit = self.capture(0.1)
        self.assertEqual(rc, 3, result)
        self.assertEqual(len(gates), 26)
        self.assertEqual(len(result["committed"]), 1)
        self.assertEqual(len(commit.call_args.kwargs["extras"]), 12)

    def test_46_gate_timeout_is_clamped_and_expired_git_is_not_started(self):
        mod = load("cci_timeouts_g4", HOOKS / "commit-ceo-inputs.py")
        mod._deadline = 18
        with patch.object(mod.time, "monotonic", return_value=17), \
             patch.object(mod.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, b"", b"")) as run:
            mod.run_gate(str(HOOKS / "scan-secrets.sh"), str(self.tmp), "f", "body", str(self.tmp))
            self.assertEqual(run.call_args.kwargs["timeout"], 1)
        with patch.object(mod.time, "monotonic", return_value=18), \
             patch.object(mod.subprocess, "run") as run:
            rc, _, reason = mod.git(["status"], str(self.tmp))
            self.assertNotEqual(rc, 0)
            self.assertTrue(reason)
            run.assert_not_called()

    def test_46_short_lease_budget_stays_pending(self):
        _, rc, result, _, _, commit = self.capture(0.6, fence=True)
        self.assertEqual(rc, 4, result)
        self.assertTrue(result["pending"], result)
        self.assertFalse(commit.called)
        self.assertIn("too little time", result["pending"][0]["why"])

    def test_46_reference_update_finishes_index_and_lease_after_work_deadline(self):
        mod = load("cci_cleanup_g4", HOOKS / "commit-ceo-inputs.py")
        repo = self.repo()
        document = repo / "docs/input.md"
        document.parent.mkdir()
        document.write_text("fixture\n")
        now = [0.0]
        releases = []
        real_run = subprocess.run
        def run(args, *a, **kwargs):
            if "land-lease.sh" in " ".join(str(arg) for arg in args):
                releases.append(args[2])
                return subprocess.CompletedProcess(args, 0, b"", b"")
            result = real_run(args, *a, **kwargs)
            if "update-ref" in args and result.returncode == 0:
                now[0] = 18.5  # Controlled completion just beyond new-work admission.
            return result
        out = io.StringIO()
        with patch.object(mod.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(mod, "run_gate", return_value=("pass", "")), \
             patch.object(mod, "fence_on", return_value=True), \
             patch.object(mod, "take_lease", return_value=("acquired", [])), \
             patch.object(mod.subprocess, "run", side_effect=run), \
             patch.dict(os.environ, RICHOS_INGRESS_SEAT_ROOT=str(repo),
                        RICHOS_INGRESS_STATE_DIR=str(self.tmp / "state")), \
             patch.object(sys, "stdin", io.StringIO(json.dumps({"prompt": str(document), "cwd": str(repo)}))), \
             contextlib.redirect_stdout(out):
            rc = mod.main()
        result = json.loads(out.getvalue())
        self.assertEqual(rc, 3, result)
        self.assertEqual(len(result["committed"]), 1)
        self.assertEqual(self.git(repo, "show", "HEAD:docs/input.md"), "fixture")
        self.assertEqual(self.git(repo, "status", "--porcelain", "--", "docs/input.md"), "")
        self.assertEqual(releases, ["release"])
        # load-bound: monotonic time is controlled by the update-ref fixture above.
        self.assertLessEqual(now[0], mod.DEADLINE_S + mod.CLEANUP_GRACE_S)

    def test_46_cleanup_reserve_cannot_start_work_after_its_own_deadline(self):
        mod = load("cci_cleanup_end_g4", HOOKS / "commit-ceo-inputs.py")
        mod._deadline, mod._cleanup_deadline = 18, 24
        with patch.object(mod.time, "monotonic", return_value=24), \
             patch.object(mod.subprocess, "run") as run:
            rc, _, reason = mod.git(["update-index"], str(self.tmp), cleanup=True)
            self.assertNotEqual(rc, 0)
            self.assertIn("cleanup allowance", reason)
            run.assert_not_called()

    def test_43_old_pending_capture_survives_unrelated_rows(self):
        mod = load("cci_pending_g4", HOOKS / "commit-ceo-inputs.py")
        path = str(self.tmp / "document.md")
        ledger = self.tmp / "ceo-inputs.jsonl"
        rows = [{"refused": [{"path": path, "pending": True}]}]
        rows += [{"committed": ["unrelated-%s" % n]} for n in range(501)]
        ledger.write_text("".join(json.dumps(row) + "\n" for row in rows))
        self.assertEqual(mod.pending_paths(str(self.tmp)), [path])
        with ledger.open("a") as fh:
            fh.write(json.dumps({"committed": [path]}) + "\n")
        self.assertEqual(mod.pending_paths(str(self.tmp)), [])

    def test_42_command_substitution_fixtures_are_cleaned(self):
        source = (HOOKS / "hook-staleness.test.sh").read_text()
        definitions = source[source.index('SESSION_ID="'):source.index("take_baseline()")]
        scratch = self.tmp / "scratch"
        scratch.mkdir()
        driver = self.tmp / "cleanup.sh"
        # BSD mktemp -t ignores TMPDIR. Route that old allocation form to the
        # fixture root when replaying the pre-fix factory; keep its subshell
        # and cleanup trap intact so the original ownership bug is exercised.
        allocator = 'mktemp() { if [ "$1" = -d ] && [ "${2:-}" = -t ]; then command mktemp -d "${TMPDIR%/}/$3"; else command mktemp "$@"; fi; }\n'
        driver.write_text('set -eu\nREAL_HOOKS="$1"\n' + allocator + definitions + '\nr="$(make_sandbox)"\ntest -f "$r/hooks.json"\nprintf "%s\\n" "$r"\n')
        result = subprocess.run(["bash", str(driver), str(SCRIPTS.parent / "hooks/hooks.json")],
                                env=dict(self.env, TMPDIR=str(scratch)), capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(scratch.iterdir()), [])

    def test_48_live_manifest_addition_does_not_demand_restart(self):
        engine = self.engine("dispatch-pretooluse.sh", "snapshot-enforcing-hooks.sh", "notice-hook-staleness.sh")
        hooks = engine / "scripts/hooks"
        surface = engine / "hooks.json"
        surface.write_text(json.dumps({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/dispatch-pretooluse.sh bash"}]}]}}))
        manifest = hooks / "dispatch-pretooluse.manifest"
        manifest.write_text("bash|fixture-old.sh\n")
        (hooks / "fixture-old.sh").write_text("exit 0\n")
        (hooks / "fixture-new.sh").write_text("echo fixture-refused >&2\nexit 2\n")
        env = dict(self.env, HOOK_STALENESS_ROOT=str(engine), HOOK_STALENESS_SURFACE=str(surface),
                   RICHOS_ENTITY_ROOT=str(engine))
        snap = self.run_hook(hooks / "snapshot-enforcing-hooks.sh", {}, env, ("--session", "g4fixture"))
        self.assertEqual(snap.returncode, 0, snap.stderr)
        original_surface = surface.read_bytes()
        manifest.write_text("bash|fixture-old.sh\nbash|fixture-new.sh\n")
        call = {"tool_name": "Bash", "cwd": str(engine), "tool_input": {"command": "true"}}
        dispatched = self.run_hook(hooks / "dispatch-pretooluse.sh", call, env, ("bash",))
        self.assertEqual(dispatched.returncode, 2, dispatched.stdout + dispatched.stderr)
        self.assertIn("fixture-refused", dispatched.stderr)
        notice = self.run_hook(hooks / "notice-hook-staleness.sh", {"session_id": "g4fixture", "hook_event_name": "Stop"}, env)
        self.assertEqual(surface.read_bytes(), original_surface)
        self.assertIn("fixture-new.sh", notice.stdout)
        self.assertNotIn("ENFORCING NOTHING RIGHT NOW", notice.stdout)
        self.assertNotIn("RESTART THIS SESSION", notice.stdout)
        self.assertIn("no session restart", notice.stdout)
        # A genuine new host registration still requires reloading the host.
        doc = json.loads(surface.read_text())
        doc["hooks"]["Stop"] = [{"hooks": [{"type": "command", "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/fixture-host.sh"}]}]
        surface.write_text(json.dumps(doc))
        notice = self.run_hook(hooks / "notice-hook-staleness.sh", {"session_id": "g4fixture", "hook_event_name": "Stop"}, env)
        self.assertIn("fixture-host.sh", notice.stdout)
        self.assertIn("RESTART THIS SESSION", notice.stdout)

    def test_25_one_case_separated_random_literals_are_scanned(self):
        engine = self.engine("scan-secrets.sh")
        with (engine / "orchestration.config").open("a") as fh:
            fh.write('SECRET_SCAN_CODE_AWARE="1"\n')
        for value in ("qwertyuiopasdfghj_zxcvbnmlkjhgfdsa", "QWERTYUIOPASDFGHJ_ZXCVBNMLKJHGFDSA"):
            with self.subTest(case=value[0]):
                result = self.run_hook(engine / "scripts/hooks/scan-secrets.sh", {
                    "tool_name": "Write", "cwd": str(engine), "tool_input": {
                        "file_path": str(engine / "docs/subject.py"), "content": 'password = "%s"' % value}},
                    dict(self.env, RICHOS_ENTITY_ROOT=str(engine)))
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)

    def test_28_absent_list_allows_large_writes(self):
        engine = self.engine("guard-named-persons-writes.sh", "guard-named-persons-commands.sh")
        repo = self.repo()
        (repo / ".publication-boundary").write_text('PRIVATE_RECORD="private-record"\n')
        env = dict(self.env, RICHOS_ENTITY_ROOT=str(engine),
                   RICHOS_NAMED_PERSONS_FILE=str(self.tmp / "absent-list"))
        for size in (100, 300000):
            for hook, tool, ti in (
                ("guard-named-persons-writes.sh", "Write", {"file_path": str(repo / "docs/new.md"), "content": "x" * size}),
                ("guard-named-persons-commands.sh", "Bash", {"command": "git commit -m " + "x" * size}),
            ):
                with self.subTest(hook=hook, size=size):
                    result = self.run_hook(engine / "scripts/hooks" / hook,
                                           {"tool_name": tool, "cwd": str(repo), "tool_input": ti}, env)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertNotIn("checker failed", result.stderr)
                    # The first notice may be deduplicated on the second size.
                    if size == 100:
                        self.assertIn("ABSENT", result.stderr)

    def test_28_crashed_name_checker_never_returns_clean(self):
        engine = self.engine("guard-named-persons-writes.sh", "guard-named-persons-commands.sh")
        repo = self.repo()
        (repo / ".publication-boundary").write_text('PRIVATE_RECORD="private-record"\n')
        env = dict(self.env, RICHOS_ENTITY_ROOT=str(engine))
        for crash in ("raise RuntimeError('fixture')\n", "print('CLEAN', flush=True)\nraise RuntimeError('fixture')\n"):
            (engine / "scripts/lib/named-persons.py").write_text(crash)
            for hook, tool, ti in (("guard-named-persons-writes.sh", "Write", {"file_path": str(repo / "docs/new.md"), "content": "ordinary text"}),
                                   ("guard-named-persons-commands.sh", "Bash", {"command": "git commit -m ordinary"})):
                with self.subTest(hook=hook, partial=crash.startswith("print")):
                    result = self.run_hook(engine / "scripts/hooks" / hook,
                                           {"tool_name": tool, "cwd": str(repo), "tool_input": ti}, env)
                    self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                    self.assertIn("no verdict", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)

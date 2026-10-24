"""Exercise ASS Kicker through legacy callers and isolated engine copies."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


ENGINE = Path(__file__).resolve().parents[2]
ENTRYPOINTS = {
    "brief-provenance.py": "scripts/brief-provenance.py",
    "brief-scope.py": "scripts/brief-scope.py",
    "ass-kicker.py": "scripts/hooks/ass-kicker.py",
}


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Relocation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ass kicker ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.engine = self.root / "engine with spaces"
        for directory in ("scripts", "mega-lander", "ass-kicker", "hooks", ".claude"):
            shutil.copytree(ENGINE / directory, self.engine / directory,
                            ignore=shutil.ignore_patterns("__pycache__", "*.sha256"))
        for filename in ("orchestration.config", "VERSION", ".gitignore"):
            shutil.copy2(ENGINE / filename, self.engine / filename)
        self.env = dict(os.environ)
        for key in ("CLAUDE_PROJECT_DIR", "CLAUDE_PLUGIN_ROOT", "RICHOS_ENTITY_ROOT",
                    "RICHOS_ENGINE_ROOT", "NOTICE_HOOK_UNDER_TEST"):
            self.env.pop(key, None)
        self.env.update(HOME=str(self.root / "home"),
                        CLAUDE_CONFIG_DIR=str(self.root / "config"),
                        RICHOS_WORKSPACES_DIR=str(self.root / "workspaces"),
                        RICHOS_LAUNCH_AGENTS_DIR=str(self.root / "launch-agents"),
                        RICHOS_MUTATION_INNER="1", PYTHONDONTWRITEBYTECODE="1",
                        GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL="/dev/null")
        self.command("git", "init", "-q", str(self.engine))

    def command(self, *args, input="", check=True):
        result = subprocess.run(args, cwd=self.engine, env=self.env, input=input,
                                capture_output=True, text=True, timeout=120)
        if check:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_legacy_commands_preserve_arguments_stdin_and_verdicts(self):
        brief = self.root / "brief with spaces.md"
        brief.write_text("There are 47 workers. All 18 tests passed.\n")
        for name, old in ENTRYPOINTS.items():
            cases = [([], "{}")]
            if name == "brief-provenance.py":
                cases = [([str(brief), "--json"], ""), ([str(brief), "--annotate"], "")]
            elif name == "brief-scope.py":
                payload = self.root / "payload.json"
                payload.write_text('{"tool_name":"Bash","tool_input":{}}')
                cases = [([], ""), (["check", str(payload)], "")]
            for args, stdin in cases:
                with self.subTest(name=name, args=args):
                    new = self.command(sys.executable, str(self.engine / "ass-kicker" / name),
                                       *args, input=stdin, check=False)
                    legacy = self.command(sys.executable, str(self.engine / old),
                                          *args, input=stdin, check=False)
                    self.assertEqual((legacy.returncode, legacy.stdout, legacy.stderr),
                                     (new.returncode, new.stdout, new.stderr))
                    self.assertNotIn("Traceback", new.stderr)
                    if name == "brief-scope.py" and not args:
                        self.assertEqual(new.returncode, 2)
                    else:
                        self.assertEqual(new.returncode, 0)
                    if name == "brief-provenance.py":
                        self.assertIn("47", new.stdout)

    def test_legacy_imports_execute_in_canonical_module_globals(self):
        for name, old in ENTRYPOINTS.items():
            with self.subTest(name=name):
                module = load(self.engine / old, "legacy_" + name.replace("-", "_"))
                canonical = str((self.engine / "ass-kicker" / name).resolve())
                self.assertEqual(module.__file__, canonical)
                self.assertEqual(module.main.__code__.co_filename, canonical)
                self.assertIs(module.main.__globals__, vars(module))
                if name == "ass-kicker.py":
                    helper = module._load("manifest", "turn-manifest.py")
                    self.assertEqual(Path(helper.__file__).resolve(),
                                     self.engine / "scripts/hooks/turn-manifest.py")
                if name == "brief-scope.py":
                    self.assertEqual(Path(module.W.__file__).resolve(),
                                     self.engine / "mega-lander/workspaces.py")

    def test_dependency_closure_reaches_canonical_code_and_shared_readers(self):
        result = self.command(sys.executable, "scripts/lib/hook-dependencies.py", str(self.engine))
        paths = set(result.stdout.splitlines())
        expected = {"ass-kicker/" + name for name in ENTRYPOINTS}
        expected.update({"scripts/hooks/turn-manifest.py", "scripts/hooks/guard-idle-land.py"})
        self.assertTrue(expected <= paths, sorted(expected - paths))
        self.assertTrue(paths.isdisjoint(ENTRYPOINTS.values()))

    def test_test_discovery_and_affected_selection(self):
        suites = self.command("bash", "scripts/ci-units.sh", "suites").stdout.splitlines()
        for name in ENTRYPOINTS:
            canonical = "ass-kicker/tests/" + name.removesuffix(".py") + ".test.sh"
            self.assertEqual(suites.count(canonical), 1)
            self.assertNotIn(ENTRYPOINTS[name].removesuffix(".py") + ".test.sh", suites)
        cases = {
            "ass-kicker/brief-provenance.py": "ass-kicker/tests/brief-provenance.test.sh",
            "ass-kicker/brief-scope.py": "ass-kicker/tests/brief-scope.test.sh",
            "ass-kicker/ass-kicker.py": "ass-kicker/tests/ass-kicker.test.sh",
            "scripts/brief-provenance.py": "ass-kicker/tests/brief-provenance.test.sh",
            "scripts/brief-scope.py": "ass-kicker/tests/brief-scope.test.sh",
            "scripts/hooks/ass-kicker.py": "ass-kicker/tests/ass-kicker.test.sh",
            "scripts/hooks/ass-kicker.sh": "ass-kicker/tests/ass-kicker.test.sh",
            "scripts/hooks/guard-brief-scope.sh": "ass-kicker/tests/brief-scope.test.sh",
            "scripts/hooks/notice-claim-capability.sh": "scripts/hooks/notice-claim-capability.test.sh",
            "ass-kicker/fixtures/brief-scope/round9-brief-2026-09-13.md": "ass-kicker/tests/brief-scope.test.sh",
        }
        for changed, expected in cases.items():
            with self.subTest(changed=changed):
                result = self.command("bash", "scripts/ci-affected-units.sh", "--paths", changed, "--strict")
                self.assertIn(expected, result.stdout.splitlines())

    def test_installer_hashes_canonical_predicates_without_duplicate_hooks(self):
        result = self.command("bash", "scripts/hooks/install.sh")
        self.assertIn("refreshed hook sha256 manifests", result.stdout)
        for name in ENTRYPOINTS:
            predicate = self.engine / "ass-kicker" / name
            expected = hashlib.sha256(predicate.read_bytes()).hexdigest()
            self.assertEqual(Path(str(predicate) + ".sha256").read_text().strip(), expected)
            ignored = self.command("git", "check-ignore", str(predicate) + ".sha256")
            self.assertIn("ass-kicker", ignored.stdout)
        settings = json.loads((self.engine / ".claude/settings.local.json").read_text())
        plugin = json.loads((self.engine / "hooks/hooks.json").read_text())
        for hook in ("guard-brief-scope.sh", "ass-kicker.sh", "notice-claim-capability.sh"):
            def count(document):
                return sum(hook in h.get("command", "")
                           for entries in document["hooks"].values()
                           for entry in entries for h in entry.get("hooks", []))
            self.assertEqual(count(settings), count(plugin))
            self.assertGreater(count(plugin), 0)
        secondary = self.engine / ".claude/settings.json"
        if secondary.exists():
            self.assertFalse(json.loads(secondary.read_text()).get("hooks"))

    def test_scope_suite_runs_without_private_source_record(self):
        suite = self.engine / "ass-kicker/tests/brief-scope.test.sh"
        lines = suite.read_text().splitlines(keepends=True)
        suite.write_text("".join('R9_SOURCE="/nonexistent/ass-kicker-private-record"\n'
                                 if line.startswith("R9_SOURCE=") else line for line in lines))
        result = self.command("bash", str(suite))
        self.assertIn("0 failed, 1 NOT RUN", result.stdout)
        self.assertIn("S31 THE ROUND-9 BRIEF", result.stdout)

    def test_stop_hook_self_test_uses_relocated_suite_in_isolated_engine(self):
        result = self.command("bash", "scripts/hooks/ass-kicker.sh", "--self-test")
        # The suite's size is not this test's business: a pinned "49 passed" went stale when
        # ARM 3 grew the suite to 84 cases (2026-10-02 and 03), and this test failed from then on.
        summary = re.search(r"=== summary: (\d+) passed, 0 failed ===", result.stdout)
        self.assertIsNotNone(summary, result.stdout[-2000:])
        self.assertGreater(int(summary.group(1)), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)

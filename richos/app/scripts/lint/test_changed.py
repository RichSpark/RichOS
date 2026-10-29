"""`lint.sh --changed`, the commit check: same ceilings as the full lint, over what changed.

Each test runs the real driver in a scratch repository shaped like this one (the lint copied
in, the engine linked for the dialect hook), so the refusals are the lint's own.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

LINT = Path(__file__).resolve().parent
REAL_ROOT = LINT.parents[3]
APP = "richos/app/"
HANG_GUARD = 600

CLEAN = '#!/usr/bin/env bash\n# A fixture script.\nset -euo pipefail\necho "hello"\n'
SUITE = ('#!/usr/bin/env bash\n# run-tests: inputs richos/app/scripts/a.sh\n# run-tests: covers -\n'
         'set -euo pipefail\necho "suite"\n')


def unquoted(n):
    """A script with n SC2086 findings (unquoted expansions), nothing else."""
    # Positional parameters: ShellCheck cannot know they are free of spaces, so each counts.
    return "#!/usr/bin/env bash\nset -euo pipefail\n" + "".join(f"echo ${i + 1}\n" for i in range(n))


class Changed(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="lint-changed-")
        self.root = Path(self.tmp.name)
        scripts = self.root / APP / "scripts"
        shutil.copytree(LINT, scripts / "lint", ignore=shutil.ignore_patterns("__pycache__", "baselines"))
        shutil.copy(LINT.parent / "lint.sh", scripts / "lint.sh")
        (scripts / "lib").mkdir()
        for name in ("cargo-cache-env.sh", "cargo_identity.py"):
            shutil.copy(LINT.parent / "lib" / name, scripts / "lib" / name)
        shutil.copytree(LINT.parent / "bin", scripts / "bin")
        shutil.copy(REAL_ROOT / APP / ".shellcheckrc", self.root / APP / ".shellcheckrc")
        (self.root / "richos/engine").symlink_to(REAL_ROOT / "richos/engine")
        self.write("scripts/a.sh", unquoted(2))
        self.write("scripts/b.test.sh", SUITE)
        self.write("crates/x/src/lib.rs", "pub fn x() {}\n")
        self.write("ui/main.js", "export const x = 1;\n")
        self.git("init", "-q", "-b", "main")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "fixture")
        # Two SC2086 at the ceiling's introduction, then one fixed without lowering: room for one.
        self.lint("--static", "--bootstrap", expect=0)
        self.write("scripts/a.sh", unquoted(1))
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "baselines")

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, rel, text):
        path = self.root / APP / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def git(self, *args):
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env.update(GIT_AUTHOR_NAME="Fixture", GIT_AUTHOR_EMAIL="fixture@example.invalid",
                   GIT_COMMITTER_NAME="Fixture", GIT_COMMITTER_EMAIL="fixture@example.invalid")
        subprocess.run(["git", "-c", "core.hooksPath=/dev/null", *args], cwd=self.root, env=env, check=True,
                       capture_output=True, timeout=HANG_GUARD)

    def lint(self, *args, expect):
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        result = subprocess.run(["bash", str(self.root / APP / "scripts/lint.sh"), *args], cwd=self.root, env=env,
                                capture_output=True, text=True, timeout=HANG_GUARD)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expect, output)
        return output

    def test_an_unrelated_edit_passes_without_the_full_pass(self):
        self.write("scripts/a.sh", unquoted(1) + "# a comment\n")
        out = self.lint("--changed", expect=0)
        self.assertIn("no count grew", out)
        self.assertNotIn("ShellCheck and custom rules running", out)

    def test_growth_within_the_ceiling_is_decided_by_the_full_pass_and_passes(self):
        self.write("scripts/a.sh", unquoted(2))
        out = self.lint("--changed", expect=0)
        self.assertIn("This change adds SC2086+1", out)
        self.assertIn("ShellCheck and custom rules running", out)

    def test_strict_refuses_growth_the_ceiling_would_still_allow(self):
        self.write("scripts/a.sh", unquoted(2))
        out = self.lint("--changed", "--strict", expect=1)
        self.assertIn("this change adds static diagnostics: SC2086 +1", out)
        self.write("scripts/a.sh", unquoted(1) + "# a comment\n")
        self.lint("--changed", "--strict", expect=0)

    def test_the_rust_digest_follows_rust_inputs_and_nothing_else(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("fixture_driver", self.root / APP / "scripts/lint/driver.py")
        driver = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(driver)
        tools = {"clippy": "clippy 0", "rustc": "rustc 0"}
        head = driver.rust_digest(self.root, "fast", tools, "HEAD")
        self.assertEqual(driver.rust_digest(self.root, "fast", tools, "working"), head)
        self.write("scripts/a.sh", unquoted(1) + "# not Rust\n")
        self.assertEqual(driver.rust_digest(self.root, "fast", tools, "working"), head)
        self.write("crates/x/src/lib.rs", "pub fn x() { let _ = 1; }\n")
        changed = driver.rust_digest(self.root, "fast", tools, "working")
        self.assertNotEqual(changed, head)
        self.write("crates/x/src/extra.rs", "pub fn y() {}\n")  # untracked, and cargo would read it
        self.assertNotEqual(driver.rust_digest(self.root, "fast", tools, "working"), changed)
        self.assertNotEqual(driver.rust_digest(self.root, "fast", {"clippy": "clippy 1", "rustc": "rustc 0"}, "HEAD"), head)
        driver.remember_counts(self.root, "fast", tools, {"clippy::x": 3})
        self.write("crates/x/src/lib.rs", "pub fn x() {}\n")
        (self.root / APP / "crates/x/src/extra.rs").unlink()
        self.assertIsNone(driver.recalled_counts(self.root, "fast", tools))
        driver.remember_counts(self.root, "fast", tools, {"clippy::y": 1})  # the tree is HEAD's again
        self.assertEqual(driver.recalled_counts(self.root, "fast", tools), {"clippy::y": 1})

    def test_growth_past_the_ceiling_is_refused_with_the_lints_reason(self):
        self.write("scripts/a.sh", unquoted(3))
        out = self.lint("--changed", expect=1)
        self.assertIn("lint growth: SC2086: 3 > 2", out)

    def test_a_new_staged_file_is_counted(self):
        self.write("scripts/c.sh", unquoted(2))
        self.git("add", APP + "scripts/c.sh")
        out = self.lint("--changed", expect=1)
        self.assertIn("lint growth: SC2086: 3 > 2", out)

    def test_a_new_load_sensitive_site_is_refused_and_a_declared_one_passes(self):
        self.write("scripts/b.test.sh", SUITE + 'sleep 1\n[ -f "$HOME" ] || bad "not there"\n')
        out = self.lint("--changed", expect=1)
        self.assertIn("load-rule growth: 1 new load-sensitive site", out)
        self.write("scripts/b.test.sh", SUITE + "# load-bound: the fixture never writes the file; nothing waits here\n"
                   'sleep 1\n[ -f "$HOME" ] || bad "not there"\n')
        self.lint("--changed", expect=0)

    def test_nothing_under_the_app_changed_is_instant(self):
        (self.root / "README.md").write_text("docs\n")
        self.git("add", "README.md")
        out = self.lint("--changed", expect=0)
        self.assertIn("nothing this lint scans changed", out)


class NoBaselineFile(unittest.TestCase):
    """lint.sh pins its Python from baselines/custom.json, and a lookup must never fail the lint."""

    def run_lint_sh(self, baseline_text):
        with tempfile.TemporaryDirectory(prefix="lint-nobaseline-") as tmp:
            scripts = Path(tmp) / APP / "scripts"
            (scripts / "lib").mkdir(parents=True)
            (scripts / "lint").mkdir()
            shutil.copytree(LINT.parent / "bin", scripts / "bin")
            (Path(tmp) / "richos/engine").symlink_to(REAL_ROOT / "richos/engine")  # cargo-cache-env.sh finds both
            shutil.copy(LINT.parent / "lint.sh", scripts / "lint.sh")
            for name in ("cargo-cache-env.sh", "cargo_identity.py"):
                shutil.copy(LINT.parent / "lib" / name, scripts / "lib" / name)
            (scripts / "lint" / "driver.py").write_text('import sys\nprint("driver ran", sys.argv[1:])\n')
            if baseline_text is not None:
                (scripts / "lint" / "baselines").mkdir()
                (scripts / "lint" / "baselines" / "custom.json").write_text(baseline_text)
            env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
            return subprocess.run(["bash", str(scripts / "lint.sh"), "--changed"], cwd=tmp, env=env,
                                  capture_output=True, text=True, timeout=HANG_GUARD)

    def test_with_no_baseline_file_the_driver_still_runs(self):
        result = self.run_lint_sh(None)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("driver ran ['--changed']", result.stdout)

    def test_a_baseline_naming_no_version_still_runs_the_driver(self):
        result = self.run_lint_sh('{"versions": {}}\n')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("driver ran ['--changed']", result.stdout)


if __name__ == "__main__":
    unittest.main()

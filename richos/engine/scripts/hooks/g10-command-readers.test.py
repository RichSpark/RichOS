#!/usr/bin/env python3
"""G10 regressions. Parse fixtures only; never launch a browser or land a branch."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
LIB = SCRIPTS / "lib"
sys.path.insert(0, str(LIB))
sys.dont_write_bytecode = True


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


shell = module("shell_c", LIB / "shell_c.py")
headed = module("headed_browser", LIB / "headed_browser.py")
failure = module("failure_type", LIB / "failure-type.py")
land = module("land_measure", SCRIPTS / "land-completeness-measure.py")
left = module("left_off", LIB / "left-off.py")


class G10(unittest.TestCase):
    def test_47_mentions_and_unrelated_options(self):
        for cmd in ["printf '%s' 'bash -c'", "true # bash -c x", "bash fixture.sh | grep -c y",
                    "bash fixture.sh -c config", "command -v bash -c", "bash fixture.sh <<'EOF'\nsh -c x\nEOF", "bash; grep -c y", "cat <<'EOF'\nbash -c x\nEOF"]:
            with self.subTest(cmd=cmd):
                self.assertIsNone(shell.verdict(cmd))

    def test_47_real_inline_scripts(self):
        for cmd in ["bash -c x", "env X=1 /bin/sh -lc x", "bash -o pipefail -c x",
                    "bash</dev/null -c x", "bash &>/dev/null -c x", "bash&>/dev/null -c x", "bash -eo pipefail -c x",
                    "echo \"$(sh -c x)\"", "find . -exec sh -c x \\;", "xargs -I ITEM bash -c x",
                    "bash --rcfile file -c x", "env -S 'bash -c x'", "sudo --user root sh -c x",
                    "bash <<'EOF'\nsh -c x\nEOF", "bash -s <<'EOF'\nsh -c x\nEOF",
                    "find . -exec command -v sh -c \\; ; bash -c x", "'bash' '-c' x", "zsh -c0 x", "ksh -c- x"]:
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(shell.verdict(cmd))

    def classifier(self, filename, cmd):
        source = (SCRIPTS / "hooks" / filename).read_text()
        body = source.split("<<'PYEOF' || true\n", 1)[1].split("\nPYEOF", 1)[0]
        result = subprocess.run([sys.executable, "-B", "-c", body, str(LIB)],
                                input=json.dumps({"tool_name": "Bash", "tool_input": {"command": cmd}}),
                                text=True, capture_output=True, check=True)
        return result.stdout.strip()

    def test_49_printed_git_is_not_commit(self):
        for hook in ["guard-completeness-commits.sh", "guard-hook-registration-commits.sh"]:
            for cmd in ["printf '%s %s' git commit", "echo git push", "true # git commit",
                        "git log --grep commit", "cat <<'EOF'\ngit commit\nEOF"]:
                with self.subTest(hook=hook, cmd=cmd):
                    self.assertEqual(self.classifier(hook, cmd), "PASS")
            for cmd in ["git commit -m 'git push'", "env X=1 git -C repo -c a=b commit -am x",
                        "printf git; git push"]:
                with self.subTest(hook=hook, cmd=cmd):
                    self.assertIn(self.classifier(hook, cmd).split("\t")[0], ("commit", "push"))

    def test_49_stage_options_belong_to_commit(self):
        for cmd in ["git commit -m '-a'", "git commit -m '--all'", "git commit -mdata",
                    "git commit --author '-a'", "git commit -- -a", "echo git add; git commit"]:
            with self.subTest(cmd=cmd):
                self.assertEqual(self.classifier("guard-completeness-commits.sh", cmd), "commit\t0")
        for cmd in ["git commit -am x", "git commit -qam x", "git add file && git commit -m x"]:
            with self.subTest(cmd=cmd):
                self.assertEqual(self.classifier("guard-completeness-commits.sh", cmd), "commit\t1")

    def test_52_flag_searches(self):
        for cmd in ["grep -n -- '--headed' notes", "printf '%s' --headed", "rg -- --no-headless notes"]:
            with self.subTest(cmd=cmd):
                ctx = headed._Ctx(str(SCRIPTS))
                headed._scan_text(ctx, cmd, 0)
                self.assertEqual(ctx.findings, [])

    def test_54_playwright_option_values(self):
        with tempfile.TemporaryDirectory(prefix="g10-headed-") as tmp:
            root = Path(tmp)
            (root / "playwright.config.js").write_text("module.exports = { projects: [{name: 'chromium'}] };")
            (root / "window.spec.js").write_text("test('sample', () => chromium.launch({headless: false}));")
            for option in ["", "--project chromium", "--grep sample", "-g sample", "--workers 1", "--reporter list"]:
                with self.subTest(option=option):
                    ctx = headed._Ctx(tmp)
                    headed._scan_text(ctx, "npx playwright test " + option, 0)
                    self.assertTrue(ctx.findings)

    def test_57_printed_read_is_not_read(self):
        import shlex
        for program in ["print(\"open('failure-types.md').read()\")", "# open('failure-types.md')\nprint(1)",
                        "print(\"open(sys.argv[1]).read()\")", "open('other-file').read(); print('failure-types.md')"]:
            with self.subTest(program=program):
                self.assertFalse(failure.bash_reads_register("python3 -c " + shlex.quote(program) + " failure-types.md",
                                                           "/repo/failure-types.md", "failure-types.md", "failure-types.md"))
        for program in ["open('failure-types.md').read()", "from pathlib import Path; Path('failure-types.md').read_text()",
                        "import sys; open(sys.argv[1]).read()"]:
            with self.subTest(program=program):
                self.assertTrue(failure.bash_reads_register("python3 -c " + shlex.quote(program) + " failure-types.md",
                                                          "/repo/failure-types.md", "failure-types.md", "failure-types.md"))

    def test_46_skipped_merge(self):
        for cmd in ["(false && git merge cc/fixture); true", "true || git merge cc/fixture",
                    "false && git merge cc/fixture; true", "git merge cc/fixture\ntrue", "if false; then git merge cc/fixture; fi; true"]:
            with self.subTest(cmd=cmd):
                self.assertIsNone(land.classify_land(cmd))
        self.assertEqual(land.classify_land("git merge --no-ff -m 'land fixture' cc/fixture"), "cc/fixture")
        self.assertEqual(land.classify_land("cd repo && git merge cc/fixture"), "cc/fixture")
        self.assertEqual(land.classify_land("true || echo skipped; git merge cc/fixture"), "cc/fixture")

    def test_58_whole_branch(self):
        name = "cc/fixture"
        for suffix in ["/+other", ".+other", "+other", "/other", ".other", "-other", "/@other", ".@other", "$other", "=other"]:
            with self.subTest(suffix=suffix):
                self.assertFalse(left.names_ref(name, "Merged " + name + suffix))
        self.assertTrue(left.names_ref("fixture", "Merged cc/fixture: done"))
        self.assertFalse(left.names_ref("fixture", "Merged cc/fixture/+other: done"))
        for ending in ["", ".", ": done", "'", "`"]:
            with self.subTest(ending=ending):
                self.assertTrue(left.names_ref(name, "Merged " + name + ending))


if __name__ == "__main__":
    unittest.main()

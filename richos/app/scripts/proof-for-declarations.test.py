"""Contract tests for dependency selection versus behavioral coverage."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parents[2]
sys.path.insert(0, str(SCRIPTS / "lib"))
from proof_declarations import InvalidDeclaration, read_declarations
try:
    from proof_declarations import pinned_readers, suite_pins
except ImportError:  # the selector before 2026-09-28 has no pins row; those cases fail, not the file
    pinned_readers = suite_pins = None


class Declarations(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="proof-declarations-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.suites = self.root / "suites"
        self.suites.mkdir()
        (self.root / "src").mkdir()
        (self.root / "src/tool.py").write_text("pass\n")

    def write(self, rows):
        (self.suites / "fixture.test.sh").write_text(rows)

    def test_dependency_directory_and_explicit_empty_coverage(self):
        self.write("# run-tests: inputs src\n# run-tests: covers -\n")
        self.assertEqual(read_declarations(self.root, self.suites),
                         [("fixture.test.sh", ["src"], [])])
        (self.root / "src/new.py").write_text("pass\n")
        self.assertEqual(read_declarations(self.root, self.suites)[0][1], ["src"])

    def test_exact_coverage_must_select_the_claiming_suite(self):
        self.write("# run-tests: inputs src\n# run-tests: covers src/tool.py\n")
        self.assertEqual(read_declarations(self.root, self.suites)[0][2], ["src/tool.py"])
        self.write("# run-tests: inputs elsewhere\n# run-tests: covers src/tool.py\n")
        with self.assertRaisesRegex(InvalidDeclaration, "fixture.test.sh:2:.*not selected"):
            read_declarations(self.root, self.suites)

    def test_runtime_selection_keeps_evidence_inputs_and_coverage(self):
        self.write("# run-tests: inputs src suites\n# run-tests: select-inputs src/tool.py\n# run-tests: covers src/tool.py\n")
        self.assertEqual(read_declarations(self.root, self.suites)[0][1], ["src/tool.py"])
        self.write("# run-tests: inputs src\n# run-tests: select-inputs suites\n# run-tests: covers -\n")
        with self.assertRaisesRegex(InvalidDeclaration, "not an evidence input"):
            read_declarations(self.root, self.suites)
        self.write("# run-tests: inputs src suites\n# run-tests: select-inputs suites\n# run-tests: covers src/tool.py\n")
        with self.assertRaisesRegex(InvalidDeclaration, "omits a covered dependency"):
            read_declarations(self.root, self.suites)

    def test_missing_duplicate_and_implicit_empty_rows_refuse(self):
        inputs = "# run-tests: inputs src\n"
        covers = "# run-tests: covers -\n"
        for rows in [inputs, covers, "", inputs + covers * 2, inputs * 2 + covers,
                     inputs + "# run-tests: covers\n", inputs + "# run-tests: covers - src/tool.py\n"]:
            with self.subTest(rows=rows):
                self.write(rows)
                with self.assertRaisesRegex(InvalidDeclaration, "fixture.test.sh"):
                    read_declarations(self.root, self.suites)

    def test_invalid_or_stale_coverage_refuses(self):
        for path in ["/src/tool.py", "src/../src/tool.py", "src//tool.py",
                     "src/*.py", "src/missing.py"]:
            with self.subTest(path=path):
                self.write(f"# run-tests: inputs src\n# run-tests: covers {path}\n")
                with self.assertRaisesRegex(InvalidDeclaration, "fixture.test.sh:2:"):
                    read_declarations(self.root, self.suites)

    def select(self, path, directory=None, gate=False):
        env = dict(os.environ)
        if directory is not None:
            env["PROOF_FOR_SCRIPT_DIR"] = str(directory)
        # No clock of this file's own (audit R13, 2026-09-29): proof-for.sh over the whole
        # tree is real work, and 15 s of it was a verdict on how busy the Mac was. A hang is
        # caught by the enclosing runner's per-suite deadline, which names this suite.
        return subprocess.run(["/bin/bash", str(SCRIPTS / "proof-for.sh"),
                               *(["--gate"] if gate else []), "--paths", path],
                              cwd=ROOT, env=env, capture_output=True, text=True)

    def test_real_tree_reconciles_and_orphans_still_select_dependencies(self):
        rows = read_declarations(ROOT, SCRIPTS)
        self.assertEqual(len(rows), len(list(SCRIPTS.glob("*.test.sh"))))
        for path in ["richos/app/.proof-for-probe/orphan.sh",
                     "richos/app/scripts/review-orphan-probe.sh"]:
            with self.subTest(path=path):
                result = self.select(path)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("UNCOVERED", result.stdout + result.stderr)
                self.assertIn(path, result.stdout + result.stderr)
                self.assertIn("lint.test.sh", result.stdout)
        result = self.select("docs/a-file-that-is-only-prose.md")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_directory_coverage_refuses_but_directory_dependency_passes(self):
        for suite in SCRIPTS.glob("*.test.sh"):
            shutil.copy2(suite, self.suites / suite.name)
        suite = self.suites / "lint.test.sh"
        original = suite.read_text()
        lines = original.splitlines()
        number = next(i for i, line in enumerate(lines, 1)
                      if line.startswith("# run-tests: covers "))
        lines[number - 1] = "# run-tests: covers richos/app"
        suite.write_text("\n".join(lines) + "\n")
        result = self.select("richos/app/.proof-for-probe/orphan.sh", self.suites)
        self.assertEqual(result.returncode, 2)
        self.assertIn(f"lint.test.sh:{number}: covers richos/app", result.stderr)
        self.assertIn("directory coverage would hide an orphan", result.stderr)
        # Same tree remains an input: it still selects lint, but cannot prove the orphan.
        suite.write_text(original)
        result = self.select("richos/app/.proof-for-probe/orphan.sh", self.suites)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("lint.test.sh", result.stdout)
        self.assertIn("UNCOVERED", result.stderr)

    def test_mobile_paths_select_their_actual_proofs_without_hiding_new_code(self):
        for path, suite in [
            ("richos/mobile/test/cli.test.js", "mobile-headless.test.sh"),
            ("richos/mobile/package.json", "mobile-headless.test.sh"),
            ("richos/mobile/cli/pwa-worker.mjs", "mobile-pwa.test.sh"),
            ("richos/mobile/ios/Sources/AppDelegate.swift", "mobile-ios.test.sh"),
            ("richos/mobile/ios/UITests/ComposerTests.swift", "mobile-ios.test.sh"),
        ]:
            with self.subTest(path=path):
                result = self.select(path)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("--only " + suite, result.stdout)
        result = self.select("richos/mobile/core/new-unproved-feature.js")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("UNCOVERED", result.stderr)

    def test_gate_inputs_name_a_directory_among_the_inputs(self):
        try:
            from proof_declarations import suite_gate_inputs
        except ImportError:
            self.fail("proof_declarations has no gate-inputs row")
        suite = self.suites / "fixture.test.sh"
        self.write("# run-tests: inputs src\n# run-tests: covers -\n# run-tests: gate-inputs src\n")
        self.assertEqual(suite_gate_inputs(self.root, suite, ["src"]), ["src"])
        (self.root / "other").mkdir()
        for row, reason in [("src/tool.py", "not a directory"), ("other", "not among"),
                            ("src/../src", "literal"), ("", "at most one")]:
            with self.subTest(row=row):
                self.write(f"# run-tests: inputs src\n# run-tests: covers -\n# run-tests: gate-inputs {row}\n")
                with self.assertRaisesRegex(InvalidDeclaration, reason):
                    suite_gate_inputs(self.root, suite, ["src"])

    def test_narrower_directories_cannot_replace_a_blanket_claim(self):
        for directory in ["src", "src/nested"]:
            with self.subTest(directory=directory):
                (self.root / directory).mkdir(exist_ok=True)
                self.write(f"# run-tests: inputs src\n# run-tests: covers {directory}\n")
                with self.assertRaisesRegex(InvalidDeclaration, "directory coverage"):
                    read_declarations(self.root, self.suites)

    def pin_file(self, units, review="docs/q.md"):
        (self.root / "docs").mkdir(exist_ok=True)
        (self.root / "docs/q.md").write_text("review\n")
        document = {"schema": 1, "units": units}
        if review is not None:
            document["review"] = review
        (self.root / "docs/q.json").write_text(json.dumps(document))

    def test_pins_row_selects_every_reader_the_pin_file_binds(self):
        self.pin_file({"a": {"sources": {"src/tool.py": "x", "lib/reader.sh": "y"}},
                       "b": {"sources": {"lib/other.py": "z"}}})
        self.write("# run-tests: inputs src\n# run-tests: covers docs/q.json\n"
                   "# run-tests: pins docs/q.json\n")
        suite, inputs, covers = read_declarations(self.root, self.suites)[0]
        self.assertEqual(inputs[0], "src")
        self.assertEqual(set(inputs[1:]), {"docs/q.json", "docs/q.md", "src/tool.py",
                                           "lib/reader.sh", "lib/other.py"})
        self.assertEqual(covers, ["docs/q.json"])
        self.assertEqual(suite_pins(self.root, self.suites / "fixture.test.sh"),
                         pinned_readers(self.root, "docs/q.json"))
        # Read from the file on every run: a reader added to the pin file selects the suite.
        self.pin_file({"a": {"sources": {"src/tool.py": "x", "lib/new.sh": "n"}}})
        self.assertIn("lib/new.sh", read_declarations(self.root, self.suites)[0][1])

    def test_a_pin_file_that_cannot_be_read_as_pins_refuses(self):
        rows = "# run-tests: inputs src\n# run-tests: covers -\n# run-tests: pins {}\n"
        cases = {
            "missing": ("docs/absent.json", None),
            "not json": ("docs/q.json", "{not json"),
            "no units": ("docs/q.json", json.dumps({"units": {}})),
            "unit without sources": ("docs/q.json", json.dumps({"units": {"a": {"review": "r"}}})),
            "absolute source": ("docs/q.json", json.dumps({"units": {"a": {"sources": {"/etc/passwd": "x"}}}})),
            "escaping source": ("docs/q.json", json.dumps({"units": {"a": {"sources": {"src/../x": "x"}}}})),
        }
        for name, (path, text) in cases.items():
            with self.subTest(case=name):
                (self.root / "docs").mkdir(exist_ok=True)
                if text is not None:
                    (self.root / path).write_text(text)
                self.write(rows.format(path))
                with self.assertRaisesRegex(InvalidDeclaration, "fixture.test.sh"):
                    read_declarations(self.root, self.suites)
        self.pin_file({"a": {"sources": {"src/tool.py": "x"}}})
        two_rows = "# run-tests: pins docs/q.json\n# run-tests: pins docs/q.json\n"
        empty_row = "# run-tests: pins\n"
        for extra in (two_rows, empty_row):
            with self.subTest(extra=extra):
                self.write("# run-tests: inputs src\n# run-tests: covers -\n" + extra)
                with self.assertRaisesRegex(InvalidDeclaration, "at most one"):
                    read_declarations(self.root, self.suites)

    def test_a_changed_qualified_reader_selects_the_qualification_check(self):
        # 2026-09-28: 5b85f4fb changed four readers that verification-input-qualifications.json
        # pins, the land selected nothing that checks those pins, and proof-evidence.test.py
        # refused 27 recipes at the nightly's script-suites gate instead.
        qualification = "docs/development/verification-input-qualifications.json"
        for path in ("richos/engine/scripts/hooks/contract-integrity-probe.sh",
                     "richos/engine/scripts/lib/row-currency.sh", qualification):
            with self.subTest(path=path):
                result = self.select(path)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("--only proof-qualification.test.sh", result.stdout)
                self.assertNotIn("--only proof-run.test.sh", result.stdout)
        # Every reader the real file pins selects it, read from the file, never typed.
        document = json.loads((ROOT / qualification).read_text())
        pinned = {source for unit in document["units"].values() for source in unit["sources"]}
        rows = {suite: inputs for suite, inputs, _covers in read_declarations(ROOT, SCRIPTS)}
        self.assertTrue(pinned)
        self.assertLessEqual(pinned | {qualification}, set(rows["proof-qualification.test.sh"]))
        result = subprocess.run([sys.executable, str(SCRIPTS / "lib/proof_declarations.py"), "--pinned",
                                 str(ROOT), str(SCRIPTS / "proof-qualification.test.sh")],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLessEqual(pinned, set(result.stdout.split()))

    def test_evidence_split_keeps_shared_runner_dependencies(self):
        for gate in (False, True):
            for path, expected, absent in [
                ("richos/app/scripts/proof-evidence.test.py", "proof-evidence.test.sh", "proof-run.test.sh"),
                ("richos/app/scripts/proof-run.test.py", "proof-run.test.sh", "proof-evidence.test.sh"),
            ]:
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only " + expected, result.stdout)
                    self.assertNotIn("--only " + absent, result.stdout)
            for path in ("richos/app/scripts/proof-run.py", "richos/app/scripts/lib/proof_evidence.py",
                         "richos/engine/scripts/lib/cpu_guard.py"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only proof-evidence.test.sh", result.stdout)
                    self.assertIn("--only proof-run.test.sh", result.stdout)

    def test_production_pin_changes_skip_generated_evidence_fixtures(self):
        for gate in (False, True):
            for path in ("docs/development/verification-input-qualifications.json",
                         "richos/app/scripts/proof-inputs.json",
                         "richos/engine/scripts/hooks/contract-integrity-probe.sh",
                         "richos/engine/scripts/hooks/ceo-inputs.test.sh"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only proof-qualification.test.sh", result.stdout)
                    self.assertNotIn("--only proof-evidence.test.sh", result.stdout)
            result = self.select("richos/app/scripts/lib/proof_evidence.py", gate=gate)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for suite in ("proof-evidence", "proof-qualification", "proof-run"):
                self.assertIn("--only " + suite + ".test.sh", result.stdout)

    def test_lint_regressions_follow_machinery_and_keep_the_product_scan(self):
        for gate in (False, True):
            for path in ("richos/app/crates/richos-core/src/native.rs",
                         "richos/app/crates/richos-core/src/app_workers.rs"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only lint.test.sh", result.stdout)
                    self.assertNotIn("--only lint-regressions.test.sh", result.stdout)
            for path in ("richos/app/scripts/lint/driver.py",
                         "richos/app/scripts/lint.sh"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only lint.test.sh", result.stdout)
                    self.assertIn("--only lint-regressions.test.sh", result.stdout)
            for path in ("richos/app/scripts/lint/test_wiring.py",
                         "richos/app/scripts/nightly-local.py",
                         "richos/app/scripts/bin/cargo",
                         "richos/engine/scripts/hooks/dialect-guard.sh"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only lint-regressions.test.sh", result.stdout)

    def test_machine_settings_and_autocheck_select_private_settings_consumers(self):
        for gate in (False, True):
            for path in ("richos/engine/scripts/lib/machine_settings.py",
                         "richos/app/scripts/autocheck/autocheck.py"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    for suite in ("dependency-pins", "nightly-local", "extension-native-browser"):
                        self.assertIn("--only " + suite + ".test.sh", result.stdout)

    def test_output_walk_has_direct_proof_and_shared_vm_changes_keep_the_harness(self):
        nested = "bash scripts/testvm/test/run-tests.sh"
        helper = "richos/app/scripts/testvm/output-walk.py"
        for gate in (False, True):
            for path in (helper, "richos/app/scripts/output-walk.test.py"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only output-walk.test.sh", result.stdout)
                    self.assertNotIn(nested, result.stdout)
            for path in ("richos/app/scripts/testvm/command-walk.py",
                         "richos/app/scripts/testvm/adopt-walk.py",
                         "richos/app/scripts/testvm/relaunch.py"):
                with self.subTest(path=path, gate=gate):
                    result = self.select(path, gate=gate)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("--only output-walk.test.sh", result.stdout)
                    self.assertIn(nested, result.stdout)
            result = self.select("richos/app/scripts/testvm/guest.sh", gate=gate)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(nested, result.stdout)
        # Losing the direct owner must refuse coverage, never restore a false blanket proof.
        for suite in SCRIPTS.glob("*.test.sh"):
            if suite.name != "output-walk.test.sh":
                shutil.copy2(suite, self.suites / suite.name)
        result = self.select(helper, self.suites, gate=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("UNCOVERED", result.stderr)
        self.assertNotIn(nested, result.stdout)

    def test_selector_refuses_inputs_without_coverage(self):
        for suite in SCRIPTS.glob("*.test.sh"):
            shutil.copy2(suite, self.suites / suite.name)
        suite = self.suites / "lint.test.sh"
        suite.write_text("\n".join(line for line in suite.read_text().splitlines()
                                   if not line.startswith("# run-tests: covers")) + "\n")
        result = self.select("docs/a-file-that-is-only-prose.md", self.suites)
        self.assertEqual(result.returncode, 2)
        self.assertIn("lint.test.sh", result.stderr)
        self.assertIn("covers", result.stderr)

    def test_the_selector_runs_without_a_clock_of_this_files_own(self):
        # Records the clock `select` puts on the real selector instead of running it
        # (audit R13): a fixed timeout around real work decides the verdict on a busy Mac.
        clocks = []

        def recorder(argv, **kwargs):
            clocks.append(kwargs.get("timeout"))
            return subprocess.CompletedProcess(argv, 0, "", "")

        with mock.patch.object(subprocess, "run", recorder):
            self.select("docs/a-file-that-is-only-prose.md")
        self.assertEqual(clocks, [None])


if __name__ == "__main__":
    unittest.main()

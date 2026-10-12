"""Execute the selector's engine commands through the shipped unit runner."""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parents[2]
ENGINE = ROOT / "richos/engine"


# NO CLOCK OF THIS FILE'S OWN AROUND REAL WORK (audit R4, 2026-09-29). Nightly
# 20260928T224842Z-c3cce554 failed here while the product was fine. The ci-shard
# execution below had `timeout=45`, and a loaded Mac spent it on a real engine
# suite plus ci-shard's inventory and its two leak-canary passes over the
# checkout. On a quiet Mac this whole file takes about 15 s, so 45 s was a verdict
# on the machine's speed, and it was twenty times tighter than the harness it
# runs: ci-shard.sh gives the same unit max(900, 3 x weight) seconds, reports
# TIMED-OUT by name, and refuses a unit it cannot admit (exit 75) instead of
# queueing forever. So no call here carries a timeout. A hang is still caught,
# by the clock that names it: ci-shard's per-unit deadline for the executions,
# and the enclosing runner's per-suite deadline for the selector calls
# (proof-run.py deadline_for on a land, the script-suites gate in the nightly).
def run(argv, cwd, env):
    return subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)


class EngineCommands(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="proof-engine-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = self.root / "config"
        (self.config / "state").mkdir(parents=True)
        self.env = {**os.environ, "CLAUDE_CONFIG_DIR": str(self.config)}

    def commands(self, path):
        result = run(["bash", str(SCRIPTS / "proof-for.sh"), "--paths", path], ROOT, self.env)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        commands = [line.strip() for line in result.stdout.splitlines()
                    if line.startswith("  cd richos/engine && ")]
        self.assertTrue(commands, result.stdout)
        return commands

    def execute(self, command, cwd, expected):
        receipt = self.root / "receipt.jsonl"
        result = run(["bash", "-c", command + " --receipt " + shlex.quote(str(receipt))],
                     cwd, self.env)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        rows = [json.loads(line) for line in receipt.read_text().splitlines()]
        self.assertEqual(len(rows), 1, rows)
        return rows[0]

    def test_generated_command_executes_a_real_suite(self):
        commands = self.commands("richos/engine/scripts/locate-engine.test.sh")
        selected = [command for command in commands
                    if shlex.split(command)[-1] == "scripts/locate-engine.test.sh"]
        self.assertEqual(len(selected), 1, commands)
        row = self.execute(selected[0], ROOT, 0)
        self.assertEqual(row["unit"], "scripts/locate-engine.test.sh")
        self.assertEqual(row["verdict"], "PASS")

    def test_unmapped_engine_executable_refuses_instead_of_becoming_empty_plan(self):
        path = "richos/engine/scripts/" + "unmapped-" + "selection-fixture-" + "987.sh"
        result = run(["bash", str(SCRIPTS / "proof-for.sh"), "--quiet", "--paths", path],
                     ROOT, self.env)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("named by NO suite", result.stderr)
        self.assertIn("engine selection failed", result.stderr)

    def test_section_command_preserves_expected_scoped_verdict(self):
        commands = self.commands("richos/engine/scripts/hooks/contract-integrity.test.sh")
        # Preserve the mapper's entire selection, including indirect consumers.
        # Execute a section below rather than re-scan the inventory for every unit.
        units = [shlex.split(command)[-1] for command in commands]
        affected = run(["bash", str(ENGINE / "scripts/ci-affected-units.sh"),
                        "--paths", "richos/engine/scripts/hooks/contract-integrity.test.sh"],
                       ROOT, self.env)
        self.assertEqual(affected.returncode, 0, affected.stderr)
        self.assertEqual(sorted(units), sorted(affected.stdout.splitlines()))

        # Keep the actual executor, inventory and canaries. Replace only the
        # expensive suite body so the selected section can return 3, 0 and 1.
        engine = self.root / "richos/engine"
        (engine / "scripts/lib").mkdir(parents=True)
        (engine / "scripts/hooks").mkdir()
        for name in ("ci-shard.sh", "ci-units.sh", "lib/ci-receipts.py",
                     "lib/leak-canary.sh", "lib/record-canary.sh", "lib/tree-witness.sh",
                     "lib/proc_tree.py", "lib/worker_tokens.py", "lib/engine_pass.py", "lib/machine_settings.py",
                     "lib/operator_fences.py"):
            shutil.copyfile(ENGINE / "scripts" / name, engine / "scripts" / name)
        index = next(i for i, unit in enumerate(units)
                     if unit.startswith("scripts/hooks/contract-integrity.test.sh:"))
        section = units[index].split(":", 1)[1]
        suite = engine / "scripts/hooks/contract-integrity.test.sh"
        for code, expected, verdict in ((3, 0, "PASS"), (0, 1, "SCOPE-LOST"), (1, 1, "FAIL")):
            with self.subTest(suite_exit=code):
                suite.write_text("#!/bin/bash\n_section() { return 1; }\n"
                                 f"if _section {section}; then\n  :\nfi\n"
                                 f'[ "$1" = --only ] && [ "$2" = {shlex.quote(section)} ] || exit 91\n'
                                 f"exit {code}\n")
                row = self.execute(commands[index], self.root, expected)
                self.assertEqual(row["unit"], units[index])
                self.assertEqual(row["verdict"], verdict)

    def test_no_clock_here_is_tighter_than_the_work_it_waits_for(self):
        # Records the clock each helper puts on its subprocess instead of running it.
        # A helper that bounds real work with a wall-clock timeout of its own turns the
        # Mac's speed into this suite's verdict (see `run` above; audit R4).
        clocks = []

        def recorder(argv, **kwargs):
            clocks.append((argv[-1], kwargs.get("timeout")))
            words = shlex.split(argv[-1]) if argv[:2] == ["bash", "-c"] else []
            if "--receipt" in words:
                Path(words[words.index("--receipt") + 1]).write_text(
                    json.dumps({"unit": "u", "verdict": "PASS"}) + "\n")
            return subprocess.CompletedProcess(argv, 0, "  cd richos/engine && true\n", "")

        with mock.patch.object(subprocess, "run", recorder):
            self.commands("richos/engine/scripts/locate-engine.test.sh")
            self.execute("true", ROOT, 0)
        self.assertEqual(len(clocks), 2, clocks)
        self.assertEqual([(what, t) for what, t in clocks if t is not None], [],
                         "a fixed clock around real work decides this suite's verdict on a busy Mac")


if __name__ == "__main__":
    unittest.main()

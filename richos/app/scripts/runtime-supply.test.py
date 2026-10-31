#!/usr/bin/env python3
"""A check that needs RICHOS_RUNTIME_DIR or the speech models gets them without anyone setting them.

Follow-ups 2026-10-28 and Tom's P4 review (2026-10-29): a boot-proof retake from a worker's worktree
and a land's rerun both stopped at "set RICHOS_RUNTIME_DIR to a prepared, verified runtime" until a
lead built the runtime by hand (build-runtimes.py, verify-runtime.py) and set the variable; a missing
speech model was refused only downstream, inside gui-boot. Now lib/runtime_cache.py builds the runtime
for the recipe (under a lock, never over an existing delivery) and proof-run.py asks it whenever a
selected check needs it, and refuses missing speech models up front with the command that fetches them.
Everything here uses stand-in builders and fixture folders; nothing is downloaded, compiled or booted.
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
sys.path.insert(0, str(HERE.parent.parent / "engine" / "scripts" / "lib"))
import machine_settings as M  # noqa: E402
import runtime_cache  # noqa: E402

spec = importlib.util.spec_from_file_location("proof_run", HERE / "proof-run.py")
proof_run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof_run)

STUB_BUILDER = '''import os, sys, time
with open(os.environ["STUB_LOG"], "a") as log:
    log.write("build %s\\n" % sys.argv[1])
time.sleep(float(os.environ.get("STUB_BUILD_SECONDS", "0")))
if os.environ.get("STUB_BUILD_RC", "0") != "0":
    sys.exit(int(os.environ["STUB_BUILD_RC"]))
os.makedirs(sys.argv[1])
'''
STUB_VERIFY = '''import os, sys
sys.exit(int(os.environ.get("STUB_VERIFY_RC", "0")))
'''
NEEDING_SUITE = '#!/usr/bin/env bash\necho "Prerequisite: set RICHOS_RUNTIME_DIR to a prepared, verified runtime." >&2\n'
GUI_SUITE = ('#!/usr/bin/env bash\n. "$HERE/lib/gui-launch.sh"\n'
             'echo "provide an extracted engine with runtimes via RICHOS_GUI_ENGINE_SOURCE, or a verified RICHOS_RUNTIME_DIR."\n'
             'echo "$RICHOS_GUI_SPEECH_MODELS"\n')
PLAIN_SUITE = '#!/usr/bin/env bash\necho nothing to do with a runtime\n'


class Fixture(unittest.TestCase):
    def setUp(self):
        self.root = Path(os.path.realpath(tempfile.mkdtemp(prefix="runtime-supply-test.")))
        self.scripts = self.root / "scripts"
        (self.scripts / "lib").mkdir(parents=True)
        (self.scripts / "stub-build-runtimes.py").write_text(STUB_BUILDER)
        (self.scripts / "verify-runtime.py").write_text(STUB_VERIFY)
        self.recipe = self.scripts / "runtime-sources.json"
        self.recipe.write_text('{"recipe": "fixture"}\n')
        self.state = self.root / "state"
        self.log = self.root / "stub.log"
        self.log.write_text("")
        self.saved = {k: os.environ.get(k) for k in ("STUB_LOG", "RICHOS_RUNTIME_BUILDER", "STUB_BUILD_RC", "STUB_VERIFY_RC",
                                                     "RICHOS_RUNTIME_DIR", "RICHOS_GUI_SPEECH_MODELS", "HOME", "STUB_BUILD_SECONDS",
                                                     M.RESOLVED, *M.KEYS)}
        os.environ["STUB_LOG"] = str(self.log)
        os.environ["RICHOS_RUNTIME_BUILDER"] = str(self.scripts / "stub-build-runtimes.py")
        # The machine's own storage settings (a bulk root on the runner) must not decide where the
        # fixture's runtime goes; each test that wants one sets it.
        for name in ("STUB_BUILD_RC", "STUB_VERIFY_RC", "RICHOS_RUNTIME_DIR", "RICHOS_GUI_SPEECH_MODELS", "STUB_BUILD_SECONDS",
                     M.RESOLVED, *M.KEYS):
            os.environ.pop(name, None)

    def tearDown(self):
        for key, value in self.saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        shutil.rmtree(self.root, ignore_errors=True)

    def builds(self):
        return [line for line in self.log.read_text().splitlines() if line.startswith("build ")]

    def want(self):
        return runtime_cache.cache_path(self.state, self.recipe)


class EnsureRuntime(Fixture):
    def test_an_absent_runtime_is_built_then_found(self):
        path, how = runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts)
        self.assertEqual((path, how), (self.want(), "built"))
        self.assertEqual(self.builds(), ["build %s" % self.want()])
        path, how = runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts)
        self.assertEqual(how, "verified")
        self.assertEqual(len(self.builds()), 1, "a runtime already in place is not built again")

    def test_no_build_names_the_command_instead(self):
        with self.assertRaises(runtime_cache.RuntimeNotReady) as raised:
            runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts, build=False)
        self.assertIn("Build it once with: python3", str(raised.exception))
        self.assertEqual(self.builds(), [])

    def test_a_runtime_that_does_not_verify_is_never_built_over(self):
        self.want().mkdir(parents=True)
        os.environ["STUB_VERIFY_RC"] = "1"
        with self.assertRaises(runtime_cache.RuntimeNotReady) as raised:
            runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts)
        self.assertIn("never overwritten", str(raised.exception))
        self.assertEqual(self.builds(), [])

    def test_a_failed_build_says_what_to_run(self):
        os.environ["STUB_BUILD_RC"] = "7"
        with self.assertRaises(runtime_cache.RuntimeNotReady) as raised:
            runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts)
        self.assertIn("failed (exit 7)", str(raised.exception))
        self.assertFalse(self.want().exists())

    def test_two_runs_wanting_one_runtime_build_it_once(self):
        os.environ["STUB_BUILD_SECONDS"] = "1"
        results = []

        def run():
            results.append(runtime_cache.ensure_runtime(self.state, self.recipe, scripts=self.scripts)[1])

        threads = [threading.Thread(target=run) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(sorted(results), ["built", "verified"])
        self.assertEqual(len(self.builds()), 1)


class SpeechModels(Fixture):
    def test_missing_models_are_refused_with_the_command_that_fetches_them(self):
        models = (self.root / "Models" / "Whisper" / "ggml-small.en.bin", self.root / "Models" / "Whisper" / "ggml-large-v3-turbo-q5_0.bin")
        with patch.object(runtime_cache, "GUI_SPEECH_MODELS", models):
            refusal = runtime_cache.speech_models_refusal()
            self.assertIn("fetch-dictation-models.sh", refusal)
            self.assertIn("Fetch them with: bash", refusal)
            self.assertIn(str(models[0]), refusal)
            self.assertIn(str(models[1]), refusal)
            models[0].parent.mkdir(parents=True)
            models[0].write_text("x")
            self.assertNotIn(str(models[0]), runtime_cache.speech_models_refusal())
            models[1].write_text("x")
            self.assertIsNone(runtime_cache.speech_models_refusal())


class ProofRun(Fixture):
    def item(self, suite_text, flags=(), lane=None, name="x.test.sh"):
        cwd = self.root / "app"
        (cwd / "scripts").mkdir(parents=True, exist_ok=True)
        (cwd / "scripts" / name).write_text(suite_text)
        return proof_run.Item(name[:-8], str(cwd), ["scripts/run-tests.sh", *flags, "--results-out", "r.json", "--only", name], lane)

    def supply(self, items, **kwargs):
        verified = subprocess.CompletedProcess([], 0, "", "")
        real_run = subprocess.run

        def run(argv, *a, **k):
            return verified if any("verify-runtime.py" in str(part) for part in argv) else real_run(argv, *a, **k)

        os.environ["HOME"] = str(self.root / "home")
        (self.root / "home").mkdir(exist_ok=True)
        self.state = self.root / "home" / ".richos-nightly"
        with patch.object(proof_run, "HERE", str(self.scripts)), patch.object(proof_run.subprocess, "run", side_effect=run), \
                patch.object(runtime_cache, "verifies", return_value=True):
            return proof_run.supply_runtime(items, **kwargs)

    def test_a_check_that_needs_the_runtime_gets_it_built_and_supplied(self):
        needing, plain = self.item(NEEDING_SUITE, name="make-engine-asset.test.sh"), self.item(PLAIN_SUITE)
        self.assertTrue(proof_run.needs_runtime(needing))
        self.assertFalse(proof_run.needs_runtime(plain))
        line = self.supply([needing, plain], build=True)
        self.assertEqual(len(self.builds()), 1, line)
        self.assertEqual(needing.env.get("RICHOS_RUNTIME_DIR"), str(self.want()))
        self.assertIn("RICHOS_RUNTIME_DIR=%s" % self.want(), line)

    def test_make_engine_asset_as_proof_for_selects_it_gets_the_runtime_built(self):
        """proof-for.sh selects this suite as a plain `bash` command, not through run-tests.sh, and
        only run-tests.sh suites were read: a recipe change left the run with no runtime and the
        check refused for want of RICHOS_RUNTIME_DIR (zach-opus-lz1's proof run, 2026-10-31)."""
        cwd = self.root / "app"
        (cwd / "scripts").mkdir(parents=True, exist_ok=True)
        (cwd / "scripts" / "make-engine-asset.test.sh").write_text(NEEDING_SUITE)
        needing = proof_run.Item("make-engine-asset", str(cwd), ["bash", "scripts/make-engine-asset.test.sh"], None)
        self.assertTrue(proof_run.needs_runtime(needing))
        line = self.supply([needing], build=True)
        self.assertEqual(len(self.builds()), 1, line)
        self.assertEqual(needing.env.get("RICHOS_RUNTIME_DIR"), str(self.want()))
        (cwd / "scripts" / "plain.test.sh").write_text(PLAIN_SUITE)
        self.assertFalse(proof_run.needs_runtime(proof_run.Item("plain", str(cwd), ["bash", "scripts/plain.test.sh"], None)))

    def test_the_runtime_is_built_in_the_configured_storage(self):
        """Follow-up 5 (2026-10-30): the automatic build wrote to ~/.richos-nightly whatever the bulk
        root said, so a configured Mac filled its home volume."""
        bulk = self.root / "bulk"
        bulk.mkdir()
        configured = bulk / "state" / "richos"
        needing = self.item(NEEDING_SUITE, name="make-engine-asset.test.sh")
        with patch.dict(os.environ, {"RICHOS_BULK_ROOT": str(bulk)}):
            line = self.supply([needing], build=True)
        want = runtime_cache.cache_path(configured, self.recipe)
        self.assertEqual(self.builds(), ["build %s" % want], line)
        self.assertEqual(needing.env.get("RICHOS_RUNTIME_DIR"), str(want))

    def test_planning_never_builds(self):
        needing = self.item(NEEDING_SUITE, name="make-engine-asset.test.sh")
        line = self.supply([needing])
        self.assertEqual(self.builds(), [])
        self.assertIn("does not exist", line)
        self.assertIn("Build it with:", line)

    def test_nothing_is_built_for_checks_that_do_not_need_it(self):
        self.supply([self.item(PLAIN_SUITE)], build=True)
        self.assertEqual(self.builds(), [])

    def test_a_host_screen_suite_that_is_skipped_does_not_need_it(self):
        skipped = self.item(GUI_SUITE, flags=("--no-host-screen",), name="gui-boot.test.sh")
        self.assertFalse(proof_run.needs_runtime(skipped))
        running = self.item(GUI_SUITE, name="gui-boot.test.sh", lane="guest")
        self.assertTrue(proof_run.needs_runtime(running))

    def test_a_caller_set_runtime_is_left_alone(self):
        os.environ["RICHOS_RUNTIME_DIR"] = "/the/callers"
        line = self.supply([self.item(NEEDING_SUITE, name="make-engine-asset.test.sh")], build=True)
        self.assertEqual(self.builds(), [])
        self.assertIn("from the caller", line)

    def test_missing_speech_models_stop_a_gui_boot_run_up_front(self):
        models = (self.root / "m" / "a.bin", self.root / "m" / "b.bin")
        booting = self.item(GUI_SUITE, name="gui-boot.test.sh", lane="guest")
        skipped = self.item(GUI_SUITE, flags=("--no-host-screen",), name="gui-boot.test.sh", lane="guest")
        with patch.object(runtime_cache, "GUI_SPEECH_MODELS", models):
            with self.assertRaises(SystemExit) as stopped:
                proof_run.supply_speech_models([booting], refuse=True)
            self.assertIn("Fetch them with: bash", str(stopped.exception))
            self.assertIn("NOT RUN", str(stopped.exception))
            # a plan only reports it, and a run that will not boot gui-boot is not stopped
            self.assertIn("Fetch them with", proof_run.supply_speech_models([booting]))
            self.assertIn("will refuse", proof_run.supply_speech_models([skipped], refuse=True))


if __name__ == "__main__":
    unittest.main()

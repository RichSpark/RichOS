#!/usr/bin/env python3
"""Small execution-boundary regressions using the shipped runners and receipts."""
import datetime
import fcntl
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import engine_pass
import proc_tree

LIB = Path(__file__).resolve().parent


class Boundaries(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="runner-boundaries-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.engine = self.root / "repo/engine"
        (self.engine / "scripts/lib").mkdir(parents=True)
        (self.engine / "scripts/hooks").mkdir()
        (self.engine / "VERSION").write_text("1.0.0-test\n")
        for name in ("ci-shard.sh", "ci-units.sh", "run-all-tests.sh"):
            shutil.copy2(LIB.parent / name, self.engine / "scripts" / name)
        for name in ("ci-receipts.py", "leak-canary.sh", "record-canary.sh", "tree-witness.sh",
                     "proc_tree.py", "operator_fences.py", "worker_tokens.py", "engine_pass.py", "machine_settings.py", "stopwatch.sh"):
            shutil.copy2(LIB / name, self.engine / "scripts/lib" / name)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_")}
        self.env.update(TMPDIR=str(self.root / "tmp"), PYTHONDONTWRITEBYTECODE="1",
                        CLAUDE_CONFIG_DIR=str(self.root / "config"),
                        RICHOS_MACHINE_WORKERS=str(self.root / "machine"),
                        RICHOS_HOST_SHIELD_TOOL=str(LIB / "host_shield.py"),
                        RICHOS_ENGINE_PASS_DIR=str(self.root / "slot"),
                        RICHOS_VERIFICATION_CONTAMINATION=str(self.root / "contamination"))
        for name in ("tmp", "config/state", "bin"):
            (self.root / name).mkdir(parents=True)

    def snapshot(self):
        for args in (("init", "-q", "-b", "main"), ("add", "-A"),
                     ("commit", "-q", "--allow-empty", "-m", "fixture")):
            subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                            "-c", "user.email=fixture@example.invalid", *args],
                           cwd=self.engine.parent, check=True, capture_output=True)

    def invoke(self, *args):
        return subprocess.run(args, cwd=self.engine, env=self.env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20)

    def known_unit(self, exit_code=1):
        uid = "scripts/lib/known.test.sh"
        (self.engine / uid).write_text("touch " + shlex.quote(str(self.root / "executed")) +
                                       "\nexit %d\n" % exit_code)
        expires = (datetime.date.today() + datetime.timedelta(days=1)).isoformat()
        (self.engine / "scripts/lib/ci-known-red.tsv").write_text(
            f"{uid}\t2026-09-26\t{expires}\tfixture\tfixture\tDeclared assertion failure\n")
        return uid

    def inject(self, code):
        wrapper = f'''#!{sys.executable}
import os, pathlib, sys
if len(sys.argv) > 2 and sys.argv[1].endswith('/lib/worker_tokens.py') and sys.argv[2] == 'machine':
    sys.path.insert(0, str(pathlib.Path(sys.argv[1]).parent))
    import worker_tokens
{code}
    sys.exit(worker_tokens.main(sys.argv[2:]))
os.execv({sys.executable!r}, [{sys.executable!r}, *sys.argv[1:]])
'''
        path = self.root / "bin/python3"
        path.write_text(wrapper)
        path.chmod(0o755)
        self.env["PATH"] = str(path.parent) + os.pathsep + self.env["PATH"]

    def shard(self, uid):
        self.snapshot()
        receipt = self.root / "receipt.jsonl"
        result = self.invoke("bash", "scripts/ci-shard.sh", "--only-units", uid,
                             "--receipt", str(receipt))
        self.assertTrue(receipt.exists(), result.stdout)
        row = json.loads(receipt.read_text())
        return result, row

    def verify(self, row):
        plan = self.root / "plan"
        plan.write_text(row["unit"] + "\n")
        # The verifier reads the declaration table beside itself unless told otherwise;
        # the fixture's declarations live in the fixture engine, not in the real table.
        env = {**os.environ, "RICHOS_CI_KNOWN_RED": str(self.engine / "scripts/lib/ci-known-red.tsv")}
        return subprocess.run([sys.executable, str(LIB / "ci-receipts.py"), "verify", "--plan", str(plan)],
                              input=json.dumps(row), text=True, capture_output=True, timeout=5, env=env)

    def test_refused_known_red_never_earns_coverage(self):
        uid = self.known_unit()
        self.inject("    def reject(*a, **kw): raise TimeoutError('fixture: no permit')\n"
                    "    worker_tokens.Budget.acquire = reject")
        result, row = self.shard(uid)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / "executed").exists())
        self.assertEqual(row["verdict"], "NOT-ADMITTED")
        self.assertEqual(row["execution_status"], "not-admitted")
        self.assertNotEqual(self.verify(row).returncode, 0)
        row["verdict"] = "KNOWN-RED"
        self.assertNotEqual(self.verify(row).returncode, 0, "verifier must check execution too")

    def test_bad_pool_is_infrastructure_not_admission_timeout(self):
        uid = self.known_unit()
        self.inject("    def reject(*a, **kw): raise ValueError('fixture: broken pool')\n"
                    "    worker_tokens.Budget.acquire = reject")
        result, row = self.shard(uid)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual((row["execution_status"], row["rc"]), ("infrastructure-error", 125))
        self.assertFalse((self.root / "executed").exists())

    def test_suite_exit_124_is_an_executed_failure_without_deadline_expiry(self):
        uid = self.known_unit(124)
        (self.engine / "scripts/lib/ci-known-red.tsv").unlink()
        result, row = self.shard(uid)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual((row["execution_status"], row["verdict"]), ("completed", "FAIL"))

    def test_supervisor_deadline_is_a_real_timeout(self):
        uid = self.known_unit()
        (self.engine / uid).write_text("sleep 60\n")
        self.env["CI_SHARD_UNIT_TIMEOUT"] = "1"
        result, row = self.shard(uid)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual((row["execution_status"], row["verdict"]), ("timed-out", "TIMED-OUT"))
        self.assertEqual(row["rc"], 124)

    def test_serial_reports_admission_and_infrastructure_status(self):
        self.known_unit()
        self.snapshot()
        for exception, verdict in (("TimeoutError", "NOT-ADMITTED"),
                                   ("ValueError", "INFRASTRUCTURE-ERROR")):
            with self.subTest(exception=exception):
                self.inject("    def reject(*a, **kw): raise " + exception + "('fixture')\n"
                            "    worker_tokens.Budget.acquire = reject")
                result = self.invoke("bash", "scripts/run-all-tests.sh")
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(verdict, result.stdout)
                self.assertFalse((self.root / "executed").exists())

    def test_direct_runners_do_not_restart_slot_wait_after_refusal(self):
        for n in range(20):
            (self.engine / ("scripts/lib/%02d.test.sh" % n)).write_text(
                "touch " + shlex.quote(str(self.root / "executed")) + "\n")
        self.snapshot()
        wrapper = self.root / "bin/python3"
        attempts = self.root / "attempts"
        wrapper.write_text(f"""#!{sys.executable}
import os, sys
if len(sys.argv) > 2 and sys.argv[1].endswith('/lib/engine_pass.py') and sys.argv[2] == 'hold':
    with open({str(attempts)!r}, 'a') as out: out.write('attempt\\n')
    sys.exit(75)
os.execv({sys.executable!r}, [{sys.executable!r}, *sys.argv[1:]])
""")
        wrapper.chmod(0o755)
        self.env["PATH"] = str(wrapper.parent) + os.pathsep + self.env["PATH"]
        for runner in ("ci-shard.sh", "run-all-tests.sh"):
            with self.subTest(runner=runner):
                attempts.unlink(missing_ok=True)
                result = self.invoke("bash", "scripts/" + runner)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(attempts.read_text().splitlines(), ["attempt"])
                self.assertIn("19 remaining", result.stdout)
                self.assertFalse((self.root / "executed").exists())

    def test_infrastructure_exit_is_not_a_known_assertion_failure(self):
        result, row = self.shard(self.known_unit(125))
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertTrue((self.root / "executed").exists())
        self.assertEqual(row["verdict"], "INFRASTRUCTURE-ERROR")
        self.assertNotEqual(self.verify(row).returncode, 0)

    def test_missing_timing_cannot_be_accepted(self):
        uid = self.known_unit()
        self.inject("    def broken(*a, **kw): return 1\n    worker_tokens.machine_command = broken")
        result, row = self.shard(uid)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / "executed").exists())
        self.assertEqual(row["verdict"], "INFRASTRUCTURE-ERROR")
        self.assertNotEqual(self.verify(row).returncode, 0)

    def test_executed_declared_failure_retains_known_red_policy(self):
        result, row = self.shard(self.known_unit())
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertTrue((self.root / "executed").exists())
        self.assertEqual(row["verdict"], "KNOWN-RED")
        self.assertEqual(self.verify(row).returncode, 0)

    def test_serial_source_contamination_stops_and_notifies_parent(self):
        source = self.engine / "subject"
        source.write_text("original\n")
        (self.engine / "scripts/lib/aa.test.sh").write_text("echo changed > " + shlex.quote(str(source)))
        (self.engine / "scripts/lib/zz.test.sh").write_text("touch " + shlex.quote(str(self.root / "later")))
        self.snapshot()
        result = self.invoke("bash", "scripts/run-all-tests.sh")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("QUARANTINED", result.stdout)
        self.assertFalse((self.root / "later").exists(), result.stdout)
        self.assertTrue(list((self.root / "contamination").glob("*.json")))

    def test_serial_assertion_failure_preserves_independent_work(self):
        (self.engine / "scripts/lib/aa.test.sh").write_text("exit 1\n")
        (self.engine / "scripts/lib/zz.test.sh").write_text("touch " + shlex.quote(str(self.root / "later")))
        self.snapshot()
        result = self.invoke("bash", "scripts/run-all-tests.sh")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertTrue((self.root / "later").exists(), result.stdout)
        self.assertFalse((self.root / "contamination").exists())

    def test_slot_parent_close_does_not_unlock_inherited_descriptor(self):
        path = self.root / "inherited.lock"
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
        fcntl.flock(fd, fcntl.LOCK_EX)
        slot = engine_pass.Slot(fd, 0)
        child = subprocess.Popen([sys.executable, "-c", "print('ready',flush=True); input()"],
                                 pass_fds=(fd,), stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(child.stdout.readline().strip(), "ready")
            slot.release()
            self.assertTrue(engine_pass._locked(str(path)))
        finally:
            slot.release()
            child.communicate("\n", timeout=5)
        self.assertFalse(engine_pass._locked(str(path)))

    def test_direct_large_background_shard_yields_between_units(self):
        entered, release, second = [self.root / n for n in ("entered", "release", "second")]
        (self.engine / "scripts/lib/00-first.test.sh").write_text(
            "touch " + shlex.quote(str(entered)) + "\n" +
            "while [ ! -f " + shlex.quote(str(release)) + " ]; do sleep 0.05; done\n")
        (self.engine / "scripts/lib/01-second.test.sh").write_text(
            "touch " + shlex.quote(str(second)) + "\nexit 1\n")
        for n in range(2, 20):
            (self.engine / ("scripts/lib/%02d-later.test.sh" % n)).write_text("exit 0\n")
        self.snapshot()
        linked = self.root / "linked"
        subprocess.run(["git", "-C", str(self.engine.parent), "worktree", "add", "-q", "-b",
                        "background", str(linked)], check=True, capture_output=True)
        with patch.dict(os.environ, {"RICHOS_ENGINE_PASS_DIR": self.env["RICHOS_ENGINE_PASS_DIR"]}):
            gate = engine_pass.PlanGate(20, "integration", str(self.engine.parent), wait=10)
            child = subprocess.Popen(["bash", "scripts/ci-shard.sh", "--fail-fast"],
                                     cwd=linked / "engine", env=self.env, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True)
            try:
                until = time.monotonic() + 10
                while not entered.exists() and time.monotonic() < until:
                    time.sleep(.02)
                self.assertTrue(entered.exists())
                self.assertFalse(gate.ready(active=False))
                release.touch()
                until = time.monotonic() + 10
                while not gate.ready(active=False) and time.monotonic() < until:
                    time.sleep(.02)
                self.assertIsNotNone(gate.slot)
                self.assertFalse(second.exists(), "background started ahead of waiting integration")
                gate.close()
                output, _ = child.communicate(timeout=10)
                self.assertEqual(child.returncode, 1, output)
                self.assertTrue(second.exists(), output)
            finally:
                release.touch()
                gate.close()
                if child.poll() is None:
                    child.terminate()
                    child.communicate(timeout=10)

    def test_slot_survives_worker_death_until_owned_cleanup(self):
        record = self.root / "owned-pid"
        payload = ("import os,signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                   f"open({str(record)!r},'w').write(str(os.getpid())); time.sleep(60)")
        driver = ("import os,sys; sys.path.insert(0,sys.argv[1]); import engine_pass,worker_tokens; "
                  "slot=engine_pass.acquire(20,'fixture',sys.argv[2],wait=0); "
                  "os.environ['RICHOS_ENGINE_PASS_FD']=str(slot.fd); "
                  "sys.exit(worker_tokens.machine_command([sys.executable,'-c',sys.argv[3]]))")
        child = subprocess.Popen([sys.executable, "-c", driver, str(LIB), str(self.root), payload],
                                 env=self.env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        owned = None
        lock = str(self.root / "slot/slot.lock")
        try:
            until = time.monotonic() + 10
            while not record.exists() and time.monotonic() < until and child.poll() is None:
                time.sleep(.02)
            self.assertTrue(record.exists())
            owned = int(record.read_text())
            child.kill()
            child.wait(timeout=5)
            self.assertTrue(engine_pass._locked(lock), "slot freed before owned cleanup")
            until = time.monotonic() + 10
            while engine_pass._locked(lock) and time.monotonic() < until:
                time.sleep(.05)
            self.assertFalse(engine_pass._locked(lock))
            self.assertFalse(proc_tree._alive([owned]))
        finally:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=10)
            if owned and proc_tree._alive([owned]):
                proc_tree.kill_tree(owned, .1)


if __name__ == "__main__":
    unittest.main()

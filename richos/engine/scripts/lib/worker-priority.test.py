#!/usr/bin/env python3
"""Small real-lock fixtures for per-permit integration priority."""
import os
import json
import shlex
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import cpu_guard
import engine_pass
import worker_tokens


class Priority(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="worker-priority-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.main = self.root / "main"
        self.main.mkdir()
        for argv in (["init", "-q", "-b", "main"], ["commit", "-q", "--allow-empty", "-m", "fixture"]):
            subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                            "-c", "user.email=fixture@example.invalid", *argv], cwd=self.main,
                           check=True, capture_output=True)
        self.linked = self.root / "linked"
        subprocess.run(["git", "-C", str(self.main), "worktree", "add", "-q", "-b", "fixture",
                        str(self.linked)], check=True, capture_output=True)
        self.machine = str(self.root / "machine")
        worker_tokens.init(self.machine, 1)
        environment = patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": self.machine})
        environment.start()
        self.addCleanup(environment.stop)
        self.budgets = []
        self.tokens = []
        self.addCleanup(self.release)

    def release(self):
        for token in self.tokens:
            token.release()
        for budget in self.budgets:
            budget.close()

    def budget(self, checkout):
        budget = worker_tokens.Budget(self.machine, shared=False)
        budget.admission = engine_pass.Admission(self.machine, str(checkout))
        self.budgets.append(budget)
        return budget

    def take(self, budget):
        token = budget.try_acquire()
        if token:
            self.tokens.append(token)
        return token

    def age(self, budget, seconds=121):
        admission = budget.admission
        admission.since = time.monotonic() - seconds
        with open(admission.marker) as source:
            record = json.load(source)
        record["since"] = admission.since
        # Preserve the leased inode; replacing it would represent another owner.
        with open(admission.marker, "w") as out:
            json.dump(record, out)

    def test_waiting_integration_gets_next_permit_before_old_background(self):
        background = self.budget(self.linked)
        holder = self.take(background)
        self.assertIsNone(self.take(background))
        self.age(background)
        integration = self.budget(self.main)
        self.assertIsNone(self.take(integration))
        # Age does not grant a background request an exception to priority.
        holder.release()
        self.assertIsNone(self.take(background))
        token = self.take(integration)
        self.assertIsNotNone(token)
        token.release()
        self.assertIsNotNone(self.take(background))

    def episode(self, name):
        episode = engine_pass.IntegrationPlan(self.machine, [name], str(self.root / name))
        self.addCleanup(episode.close)
        return episode

    def test_integration_episode_freezes_plan_and_later_requests_cannot_join(self):
        plan = ["first"]
        first = engine_pass.IntegrationPlan(self.machine, plan, str(self.root))
        self.addCleanup(first.close)
        plan.append("appended")
        self.assertTrue(first.enter())
        second = self.episode("second")
        self.assertFalse(second.enter())
        self.assertEqual(first.record["plan"], ["first"])
        # deadline = requested + 600 on a time.monotonic() float, so the difference is 600
        # only to within float rounding (600.0000000000009 at some uptimes); never exact.
        self.assertAlmostEqual(first.record["deadline"] - first.requested, 600, delta=1e-6)
        original = first.record.copy()
        self.assertFalse(second.enter())
        self.assertEqual(json.loads(Path(first.path).read_text()), original)
        first.close()
        self.assertTrue(second.enter())

    def test_completed_episode_gives_aged_background_a_finite_turn(self):
        first, second = self.episode("first"), self.episode("second")
        self.assertTrue(first.enter())
        old = self.budget(self.linked)
        old.admission.register()
        self.age(old)
        first.close()
        later = self.budget(self.linked)
        later.admission.register()
        self.age(later, 120)
        self.assertFalse(second.enter())
        self.take(old).release()
        # New background arrivals cannot extend the frozen boundary turn.
        self.assertTrue(second.enter())

    def test_crashed_episode_preserves_aged_background_turn(self):
        first, second = self.episode("first"), self.episode("second")
        self.assertTrue(first.enter())
        old = self.budget(self.linked)
        old.admission.register()
        self.age(old)
        os.close(first.fd)
        first.fd = None  # Simulate kernel release without graceful publication.
        self.assertFalse(second.enter())
        self.assertEqual(json.loads(Path(first.path).read_text())["status"], "owner-lost")
        self.take(old).release()
        self.assertTrue(second.enter())

    def test_expired_episode_cannot_claim_or_renew_priority(self):
        first, second = self.episode("first"), self.episode("second")
        self.assertTrue(first.enter())
        with patch.object(engine_pass.time, "monotonic", return_value=second.deadline):
            self.assertTrue(first.expired())
            self.assertFalse(first.enter())
            self.assertFalse(second.enter())

    def test_aged_background_reserves_next_opportunity_after_integration(self):
        holder = self.take(self.budget(self.linked))
        old = self.budget(self.linked)
        self.assertIsNone(self.take(old))
        self.age(old)
        newer = self.budget(self.linked)
        integration = self.budget(self.main)
        self.assertIsNone(self.take(integration))
        holder.release()
        for b in (old, newer):
            self.assertIsNone(self.take(b))
        self.take(integration).release()
        self.assertIsNone(self.take(newer))
        self.take(old).release()
        self.assertIsNotNone(self.take(newer))

    def test_failed_attempt_preserves_background_age(self):
        self.take(self.budget(self.linked))
        queued = self.budget(self.linked)
        self.assertIsNone(self.take(queued))
        self.age(queued)
        since = queued.admission.since
        self.assertIsNone(self.take(queued))
        self.assertEqual(queued.admission.since, since)

    def test_crashed_aged_background_cannot_reserve_capacity(self):
        marker = Path(self.machine) / "admission" / "wait-dead.json"
        background = self.budget(self.linked)
        marker.write_text(json.dumps({"main": False, "since": time.monotonic() - 500,
                                      "pid": os.getpid(), "birth": "old generation"}))
        self.assertIsNotNone(self.take(background))
        self.assertFalse(marker.exists())

    def test_disjoint_permits_do_not_block_background_progress(self):
        old = self.budget(self.linked)
        old.admission.permits = ["token-000"]
        old.admission.register()
        self.age(old)
        newer = self.budget(self.linked)
        newer.admission.permits = ["token-001"]
        self.assertTrue(newer.admission.begin())
        newer.admission.attempted(False)

    def test_all_integration_waiters_keep_priority_until_admitted(self):
        background = self.budget(self.linked)
        holder = self.take(background)
        mains = [self.budget(self.main), self.budget(self.main)]
        for b in mains:
            self.assertIsNone(self.take(b))
        holder.release()
        for b in mains:
            self.assertIsNone(self.take(background))
            self.take(b).release()
        self.assertIsNotNone(self.take(background))

    def test_nested_work_can_borrow_held_permit_but_cannot_expand(self):
        background = self.budget(self.linked)
        holder = self.take(background)
        integration = self.budget(self.main)
        self.assertIsNone(self.take(integration))
        self.assertIsNone(self.take(background))
        borrowed = background._try_free(holder.path + ".child")
        self.assertIsNotNone(borrowed)
        borrowed.release()
        self.assertEqual(background.held(), 1)

    def test_crashed_waiter_releases_kernel_priority(self):
        background = self.budget(self.linked)
        code = ("import engine_pass,sys,time; "
                "a=engine_pass.Admission(sys.argv[1],sys.argv[2]); "
                "assert a.begin(); print('ready',flush=True); time.sleep(60)")
        p = subprocess.Popen([sys.executable, "-c", code, self.machine, str(self.main)],
                             cwd=Path(__file__).parent, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(p.stdout.readline().strip(), "ready")
            self.assertIsNone(self.take(background))
        finally:
            p.kill()
            p.wait(timeout=5)
            p.stdout.close()
        self.assertIsNotNone(self.take(background))

    def test_managed_borrow_uses_locked_native_chain_and_keeps_nested_progress(self):
        holder = self.take(self.budget(self.linked))
        path = holder.path + '.child'
        with patch.object(worker_tokens, 'managed_policy', return_value=True):
            borrowed = worker_tokens.Budget._try_free(path)
            self.tokens.append(borrowed)
            self.assertIsNone(worker_tokens.Budget._try_free(path))
            nested = worker_tokens.Budget._try_free(path + '.child')
            self.tokens.append(nested)
            self.assertIsNotNone(nested)
            self.assertEqual(self.budget(self.linked).held(), 1)
            holder.release()
            with self.assertRaisesRegex(ValueError, 'not held'):
                worker_tokens.validate_borrow(path + '.child.child')

    def test_managed_borrow_rejects_stale_generation_and_private_unrelated_root(self):
        holder = self.take(self.budget(self.linked))
        row = json.loads(Path(holder.path).read_text())
        row['holder'][1] = 'stale-generation'
        with open(holder.path, 'w') as stream:
            json.dump(row, stream)
        with self.assertRaisesRegex(ValueError, 'live native ancestor'):
            worker_tokens.validate_borrow(holder.path + '.child')
        holder.publish()
        unrelated = self.root / 'unrelated'
        worker_tokens.init(str(unrelated), 1)
        other = worker_tokens.Budget(str(unrelated), shared=False)._try_acquire()
        self.tokens.append(other)
        with self.assertRaisesRegex(ValueError, 'shared machine budget'):
            worker_tokens.validate_borrow(other.path + '.child')

    def test_managed_machine_flags_cannot_skip_admission(self):
        timing, marker = self.root / 'refusal.json', self.root / 'never-ran'
        with patch.dict(os.environ, {'RICHOS_WORKER_TOKENS': self.machine,
                                     'RICHOS_WORKER_SLOT_HELD': '1',
                                     'RICHOS_WORKER_BORROW_LOCK': str(marker)}), \
                patch.object(worker_tokens, 'managed_policy', return_value=True), \
                patch.object(cpu_guard, 'inherited_verification_owner', return_value={'role': 'verification'}), \
                patch.object(worker_tokens, 'machine_directory', return_value=self.machine):
            rc = worker_tokens.machine_command([sys.executable, '-c',
                'from pathlib import Path; Path(' + repr(str(marker)) + ').touch()'], str(timing))
        self.assertEqual(rc, 125)
        self.assertFalse(marker.exists())
        self.assertFalse(json.loads(timing.read_text())['admitted'])

    def test_managed_machine_command_takes_the_borrow_lock(self):
        holder = self.take(self.budget(self.linked))
        timing, marker = self.root / 'managed-timing.json', self.root / 'slot'
        devices = SimpleNamespace(cleanup_run_simulators=lambda run: [])
        with patch.dict(os.environ, {'RICHOS_WORKER_TOKENS': self.machine,
                                     'RICHOS_WORKER_SLOT_HELD': '1',
                                     'RICHOS_WORKER_BORROW_LOCK': holder.path + '.child'}), \
                patch.object(worker_tokens, 'managed_policy', return_value=True), \
                patch.object(cpu_guard, 'inherited_verification_owner', return_value={'role': 'verification'}), \
                patch.object(worker_tokens, 'machine_directory', return_value=self.machine), \
                patch.dict(sys.modules, {'testdevices': devices}):
            rc = worker_tokens.machine_command([sys.executable, '-c',
                "import os,sys,fcntl; p=os.environ['RICHOS_WORKER_BORROW_LOCK']; "
                "open(sys.argv[1],'w').write(p); f=open(p[:-6]);\n"
                "try: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\n"
                "except BlockingIOError: sys.exit(0)\n"
                "sys.exit(9)", str(marker)], str(timing))
        self.assertEqual(rc, 0)
        self.assertEqual(marker.read_text(), holder.path + '.child.child')
        self.assertEqual(self.budget(self.linked).held(), 1)

    def test_shell_lease_keeper_publishes_parent_through_local_and_machine_permits(self):
        local, ready, release = self.root / 'local', self.root / 'ready', self.root / 'release'
        worker_tokens.init(str(local), 1)
        checker = ("import sys,worker_tokens; from pathlib import Path; "
                   "p=Path(sys.argv[1]).read_text(); worker_tokens.validate_borrow(p); "
                   "worker_tokens.managed_policy=lambda:True; "
                   "t=worker_tokens.Budget._try_free(p); assert t; t.release()")
        command = ('"$1" "$2" lease "$3" "$$" "$4" "$5" - & keeper=$!\n'
                   'trap \'touch "$5"; wait "$keeper"\' EXIT\n'
                   'for n in {1..100}; do [ -s "$4" ] && break; sleep .02; done\n'
                   '[ -s "$4" ] || exit 8\n'
                   '"$1" -c ' + shlex.quote(checker) + ' "$4.borrow"\n')
        result = subprocess.run(['bash', '-c', command, 'fixture', sys.executable,
                                 str(Path(worker_tokens.__file__)), str(local), str(ready), str(release)],
                                env={**os.environ, 'PYTHONPATH': str(Path(__file__).parent)},
                                start_new_session=True, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.budget(self.linked).held(), 0)

    def test_mac_slot_refuses_missing_configured_drive_without_creating_fallback(self):
        with patch.dict(os.environ, {"RICHOS_BULK_ROOT": "/Volumes/AbsentConfiguredDrive",
                                     "RICHOS_MACHINE_SETTINGS_RESOLVED": "1"}, clear=True), \
                patch.object(Path, "is_mount", return_value=False), \
                patch.object(engine_pass.os, "makedirs") as create:
            with self.assertRaisesRegex(ValueError, "storage volume is not mounted: /Volumes/AbsentConfiguredDrive"):
                engine_pass.directory()
            create.assert_not_called()

    def test_background_plan_drains_and_yields_to_integration(self):
        with patch.dict(os.environ, {"RICHOS_ENGINE_PASS_DIR": str(self.root / "plan-slot")}):
            background = engine_pass.PlanGate(20, "background", str(self.linked), wait=5)
            integration = engine_pass.PlanGate(20, "integration", str(self.main), wait=5)
            try:
                self.assertTrue(background.ready(active=False))
                self.assertFalse(integration.ready(active=False))
                self.assertFalse(background.ready(active=True))
                self.assertIsNotNone(background.slot, "running units retain protection")
                self.assertFalse(background.ready(active=False))
                self.assertEqual(background.yields, 1)
                self.assertTrue(integration.ready(active=False))
                self.assertFalse(background.ready(active=False))
                integration.close()
                self.assertTrue(background.ready(active=False))
            finally:
                background.close()
                integration.close()

    def test_timing_separates_real_admission_wait_and_preserves_failure(self):
        holder = self.take(self.budget(self.linked))
        timer = threading.Timer(.4, holder.release)
        timing = self.root / "timing.json"
        cleanups = []
        devices = SimpleNamespace(cleanup_run_simulators=lambda run: cleanups.append(run) or [])
        with patch.dict(os.environ, {"RICHOS_WORKER_TOKENS": "", "RICHOS_WORKER_SLOT_HELD": ""}), \
                patch.object(worker_tokens, "machine_directory", return_value=self.machine), \
                patch.dict(sys.modules, {"testdevices": devices}):
            timer.start()
            try:
                rc = worker_tokens.machine_command([sys.executable, "-c", "raise SystemExit(7)"],
                                                   timing=str(timing))
            finally:
                timer.join()
        report = json.loads(timing.read_text())
        self.assertEqual((rc, report["exit"]), (7, 7))
        self.assertTrue(report["admitted"])
        self.assertGreaterEqual(report["admission_seconds"], .3)
        self.assertGreater(report["execution_seconds"], 0)
        self.assertEqual(len(cleanups), 1)
        self.assertIsNotNone(self.take(self.budget(self.linked)))

    def test_timed_nested_command_preserves_callers_borrow_slot(self):
        holder = self.take(self.budget(self.linked))
        timing = self.root / "nested-timing.json"
        slot = self.root / "observed-slot"
        with patch.dict(os.environ, {"RICHOS_WORKER_TOKENS": self.machine,
                                     "RICHOS_WORKER_SLOT_HELD": "1",
                                     "RICHOS_WORKER_BORROW_LOCK": holder.path + ".child"}):
            rc = worker_tokens.machine_command(
                [sys.executable, "-c", "import os,sys; open(sys.argv[1],'w').write("
                 "os.environ['RICHOS_WORKER_BORROW_LOCK'])", str(slot)], timing=str(timing))
        self.assertEqual(rc, 0)
        self.assertEqual(slot.read_text(), holder.path + ".child")
        self.assertEqual(self.budget(self.linked).held(), 1)


if __name__ == "__main__":
    unittest.main()

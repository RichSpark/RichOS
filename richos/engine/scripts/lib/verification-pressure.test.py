#!/usr/bin/env python3
"""Controlled-clock policy tests. No host load, signals or sleeping."""
import unittest
import subprocess
import sys
import time
import signal
import shutil
import proc_tree
import os
from pathlib import Path
import tempfile
from unittest.mock import patch
import cpu_guard as guard
import worker_tokens as workers
from cpu_policy import VerificationPressure


def owner(cores=1, cpu=1, priority="background", rss=100, **kwargs):
    return dict(cores=cores, cpu_seconds=cpu, priority=priority, rss_mb=rss, **kwargs)


class PressureTests(unittest.TestCase):
    def test_spikes_do_not_cancel_and_sustained_pressure_closes_then_contains(self):
        policy = VerificationPressure()
        owners = {"unit": owner()}
        self.assertEqual(policy.observe(0, 90, owners)["stage"], "normal")
        self.assertIsNone(policy.observe(9, 90, owners)["target"])
        self.assertTrue(policy.observe(9.5, 20, owners)["admission_open"])
        policy.observe(10, 90, owners)
        self.assertEqual(policy.observe(20, 90, owners)["stage"], "admission-closed")
        self.assertIsNone(policy.observe(29, 90, owners)["target"])
        self.assertEqual(policy.observe(30, 90, owners),
                         {"admission_open": False, "stage": "contain", "target": "unit"})

    def test_priority_contribution_and_lost_work_select_one_owned_tree(self):
        owners = {"land": owner(8, 1, "integration"), "older": owner(4, 80),
                  "less-lost": owner(3, 4), "tiny": owner(.1, .01)}
        self.assertEqual(VerificationPressure.contributor(owners), "less-lost")
        owners["less-lost"]["unblocks_integration"] = True
        self.assertEqual(VerificationPressure.contributor(owners), "older")
        self.assertEqual(VerificationPressure.contributor({"land": owners["land"]}), "land")
        self.assertIsNone(VerificationPressure.contributor({"idle": owner(0)}))

    def test_cleanup_failure_is_visible_and_never_cancels_another_tree(self):
        policy = VerificationPressure()
        owners = {"a": owner(), "b": owner(cpu=2)}
        policy.observe(0, 95, owners)
        self.assertEqual(policy.observe(20, 95, owners)["target"], "a")
        self.assertEqual(policy.observe(49, 95, owners)["stage"], "cleanup")
        result = policy.observe(50, 95, owners)
        self.assertEqual((result["stage"], result["target"]), ("containment-failed", "a"))
        self.assertFalse(result["admission_open"])
        self.assertEqual(policy.observe(51, 95, {"b": owners["b"]})["stage"], "admission-closed")
        self.assertEqual(policy.observe(61, 95, {"b": owners["b"]})["target"], "b")

    def test_recovery_requires_ten_healthy_seconds_and_completed_cleanup(self):
        policy = VerificationPressure()
        owners = {"a": owner()}
        policy.observe(0, 95, owners)
        policy.observe(20, 95, owners)
        self.assertFalse(policy.observe(21, 20, owners)["admission_open"])
        self.assertFalse(policy.observe(31, 20, owners)["admission_open"])
        self.assertFalse(policy.observe(32, 20, {})["admission_open"])
        self.assertTrue(policy.observe(33, 20, {})["admission_open"])
        policy.observe(34, 95, {})
        self.assertEqual(policy.observe(54, 95, {})["stage"], "unattributed-pressure")
        self.assertFalse(policy.observe(55, 20, {})["admission_open"])
        policy.observe(60, 95, {})
        policy.observe(61, 20, {})
        self.assertFalse(policy.observe(70, 20, {})["admission_open"])
        self.assertTrue(policy.observe(71, 20, {})["admission_open"])

    def test_memory_and_unknown_samples_never_use_a_cpu_only_success(self):
        policy = VerificationPressure()
        owners = {"cpu": owner(4, 1, rss=10), "memory": owner(.1, 2, rss=900)}
        policy.observe(0, 10, owners, memory_pressure="critical")
        result = policy.observe(20, 10, owners, memory_pressure="critical")
        self.assertEqual(result["target"], "memory")
        policy = VerificationPressure()
        for now in (0, 10, 20, 100):
            result = policy.observe(now, float("nan"), owners)
            self.assertFalse(result["admission_open"])
            self.assertIsNone(result["target"])


class OwnedTreeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pressure-ownership.")
        self.addCleanup(temporary.cleanup)
        state = patch.object(guard, "STATE", Path(temporary.name))
        state.start(); self.addCleanup(state.stop)
        env = patch.dict(os.environ, {"RICHOS_TEST_DEVICES_DIR": str(Path(temporary.name) / "devices")})
        env.start(); self.addCleanup(env.stop)

    @staticmethod
    def row(parent=0, cpu=0, generation="native-one", rss=10):
        return {"parent": parent, "cpu": cpu, "birth": "same displayed second", "name": "fixture",
                "generation": generation, "rss_mb": rss}

    def register(self, pid, priority="background", generation="native-one"):
        with patch.object(guard, "processes", return_value={pid: self.row(generation=generation)}):
            guard.register(pid, "test unit", "verification", {"input_key": "a" * 64, "priority": priority, "result": str(guard.STATE / (str(pid) + ".result.json"))})

    def test_native_lookup_exit_race_is_absent_and_live_unknown_is_carried_unproven(self):
        for final_alive in ([], [991001]):
            with self.subTest(final_alive=final_alive), \
                    patch.object(proc_tree, '_kernel_topology', return_value=[(991001, 1, 991001)]), \
                    patch.object(proc_tree.operator_fences, 'proc', return_value=None), \
                    patch.object(proc_tree, '_alive', side_effect=[[991001], final_alive]) as status:
                rows = proc_tree.process_rows()
                self.assertEqual(status.call_count, 2)
                self.assertEqual(rows, {991001: (1, 991001, None)} if final_alive else {})
                if final_alive:
                    tree = object.__new__(proc_tree.TrackedTree)
                    tree.known = {991001: 'prior-native-generation'}
                    tree.root, tree.root_birth = 991001, 'prior-native-generation'
                    tree.groups, tree.empty_groups, tree.unresolved, tree.released = {}, set(), {}, {}
                    # A live member that stays unreadable past the bound is carried, never
                    # proven and never fatal (nightly 20260928T165947Z-b83ab374).
                    with patch.object(proc_tree, 'process_rows', return_value=rows), \
                            patch.object(proc_tree, '_alive', return_value=[991001]):
                        self.assertEqual(tree.refresh(), {991001})
                    self.assertEqual(tree.known, {})
                    self.assertEqual(tree.unresolved, {991001: 'prior-native-generation'})

    def test_owned_unknown_reconciles_exit_and_recycled_generation(self):
        for native, alive in ((None, []),
                              ({'ppid': 1, 'pgid': 991099, 'start': 'replacement', 'zombie': False}, [991001]),
                              ({'ppid': 1, 'pgid': 991001, 'start': 'original', 'zombie': False}, [991001])):
            with self.subTest(native=native):
                tree = object.__new__(proc_tree.TrackedTree)
                tree.root, tree.root_birth = 991001, 'original'
                tree.known, tree.groups, tree.empty_groups = {991001: 'original'}, {}, set()
                tree.unresolved, tree.released = {}, {}
                with patch.object(proc_tree, 'process_rows', return_value={991001: (1, 991001, None)}), \
                        patch.object(proc_tree, '_alive', return_value=alive), \
                        patch.object(proc_tree.operator_fences, 'proc', side_effect=[None, native]), \
                        patch.object(proc_tree.time, 'sleep'):
                    result = tree.refresh()
                self.assertEqual(result, {991001} if native and native['start'] == 'original' else set())

    def test_native_lookup_across_exec_uses_native_generation_not_status(self):
        topology = subprocess.CompletedProcess([], 0, '991001 1 991001\n', '')
        native = {'ppid': 2, 'pgid': 3, 'start': 'new-native-generation', 'zombie': False}
        with patch.object(proc_tree, '_kernel_topology', return_value=None), \
                patch.object(proc_tree.subprocess, 'run', return_value=topology), \
                patch.object(proc_tree.operator_fences, 'proc', side_effect=[None, native]), \
                patch.object(proc_tree, '_alive', side_effect=lambda pids: list(pids)):
            self.assertEqual(proc_tree.process_rows(), {991001: (2, 3, 'new-native-generation')})

    def test_controller_sampling_reconciles_exited_rows_and_retains_live_unknown(self):
        topology = subprocess.CompletedProcess([], 0,
            f'{os.getuid()} 991001 1 0:01.00 1024 Sun Sep 27 00:00:00 2026 fixture\n', '')
        for final_alive in ([], [991001]):
            with self.subTest(final_alive=final_alive), \
                    patch.object(guard.subprocess, 'run', return_value=topology), \
                    patch.object(guard.operator_fences, 'proc', return_value=None), \
                    patch.object(proc_tree, '_alive', side_effect=[[991001], final_alive]):
                rows = guard.processes()
            self.assertEqual(set(rows), set(final_alive))
            if final_alive:
                self.assertIsNone(rows[991001]['generation'])
                self.register(10)
                rows[10] = self.row()
                rows[991001]['parent'] = 10
                with self.assertRaisesRegex(RuntimeError, 'descendant generation unavailable'):
                    guard.Watch().sample(rows, 0)

    def test_controller_uses_native_parent_and_generation_after_reconciliation(self):
        topology = subprocess.CompletedProcess([], 0,
            f'{os.getuid()} 991001 10 0:01.00 1024 Sun Sep 27 00:00:00 2026 fixture\n', '')
        native = {'ppid': 1, 'pgid': 3, 'start': 'replacement', 'zombie': False}
        with patch.object(guard.subprocess, 'run', return_value=topology), \
                patch.object(guard.operator_fences, 'proc', side_effect=[None, native]), \
                patch.object(proc_tree, '_alive', side_effect=lambda pids: list(pids)):
            rows = guard.processes()
        self.assertEqual((rows[991001]['parent'], rows[991001]['generation']), (1, 'replacement'))
        self.register(10)
        rows[10] = self.row()
        watch = guard.Watch()
        watch.sample(rows, 0)
        self.assertNotIn('991001', watch.owned)

    def test_controller_retains_live_seed_after_registered_supervisor_exits(self):
        self.register(10)
        path = guard.STATE / 'roots/10.json'
        record = guard.read_json(path)
        record['verification']['seed'] = {'pid': 20, 'generation': 'child-generation'}
        guard.write_json(path, record)
        watch = guard.Watch()
        watch.sample({10: self.row(), 20: self.row(10, generation='child-generation')}, 0)
        watch.sample({20: self.row(1, cpu=2, generation='child-generation')}, 2)
        group = watch.verification['10:native-one']
        self.assertEqual(group['members'], {'20': 'child-generation'})
        self.assertEqual(group['cores'], 1)

    def test_pressure_closes_new_permits_but_retains_parent_borrow_progress(self):
        directory = guard.STATE / "budget"
        workers.init(directory, 2)
        budget = workers.Budget(directory, shared=False)
        self.addCleanup(budget.close)
        held = budget.try_acquire()
        self.assertIsNotNone(held)
        self.addCleanup(held.release)
        guard.write_json(guard.STATE / "verification-pressure.json",
                         {"protocol": 1, "at": __import__("time").time(), "admission_open": False, "stage": "admission-closed"})
        with patch.object(guard, "healthy", return_value=True), \
                patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(directory)}):
            self.assertIsNone(budget.try_acquire())
            self.assertIn("admission-closed", budget.refusal)
            borrowed = budget.acquire(free=held.path + ".child", timeout=0)
            self.assertIsNotNone(borrowed)
            borrowed.release()
        with patch.object(guard, "healthy", return_value=False):
            self.assertIn("unhealthy", guard.verification_admission())
        guard.write_json(guard.STATE / "verification-pressure.json",
                         {"protocol": 1, "at": 0, "admission_open": True, "stage": "normal"})
        with patch.object(guard, "healthy", return_value=True):
            self.assertIsNone(budget.try_acquire())
            self.assertIn("stale", budget.refusal)

    def test_installed_policy_cannot_disappear_with_its_telemetry(self):
        guard.write_json(guard.STATE / 'verification-enabled.json', {'protocol': 1})
        self.assertTrue(guard.verification_enabled())
        self.assertIn('unavailable', guard.verification_admission())
        guard.write_json(guard.STATE / 'verification-enabled.json', {'protocol': 999})
        with self.assertRaisesRegex(RuntimeError, 'current launcher'):
            guard.verification_enabled()

    def test_admission_rejects_wrong_protocol_and_future_measurement(self):
        path = guard.STATE / 'verification-pressure.json'
        now = __import__('time').time()
        with patch.object(guard, 'healthy', return_value=True):
            for protocol in (None, 999):
                guard.write_json(path, {'protocol': protocol, 'at': now, 'admission_open': True})
                self.assertIn('protocol', guard.verification_admission())
            guard.write_json(path, {'protocol': 1, 'at': now + 60, 'admission_open': True})
            self.assertIn('stale', guard.verification_admission())

    def test_controller_persists_typed_containment_and_waits_for_native_cleanup(self):
        self.register(10)
        watch = guard.Watch()
        rows = {10: self.row(), 20: self.row(10)}
        watch.sample(rows, 0, 95)
        watch.verification_cycle(rows, 0, 95)
        rows = {10: self.row(cpu=1), 20: self.row(10, 40)}
        watch.sample(rows, 20, 95)
        with patch.object(guard.operator_fences, "proc", return_value={"start": "native-one", "zombie": False}), patch.object(os, "kill") as kill:
            result = watch.verification_cycle(rows, 20, 95)
            kill.assert_called_once_with(10, __import__("signal").SIGTERM)
        self.assertEqual(result["stage"], "contain")
        marker = guard.read_json(guard.STATE / "10.result.json")
        self.assertEqual((marker["status"], marker["cleanup"], marker["budget_used"]), ("contained", "pending", 1))
        watch.sample({}, 22, 30)
        watch.verification_cycle({}, 22, 30)
        self.assertEqual(guard.read_json(guard.STATE / "10.result.json")["cleanup"], "complete")
        self.assertEqual(len(guard.verification_recovery("a" * 64)["containment"]), 1)

    def test_wedged_supervisor_escalates_only_native_owned_generations(self):
        self.register(10)
        watch = guard.Watch()
        rows = {10: self.row(), 20: self.row(10)}
        watch.sample(rows, 0, 95); watch.verification_cycle(rows, 0, 95)
        rows = {10: self.row(cpu=1), 20: self.row(10, 40)}
        watch.sample(rows, 20, 95)
        with patch.object(guard.operator_fences, "proc", return_value={"start": "native-one", "zombie": False}), patch.object(os, "kill"):
            watch.verification_cycle(rows, 20, 95)
        with patch.object(guard.operator_fences, "proc", side_effect=lambda pid, precise: {"start": "native-two" if pid == 10 else "native-one", "zombie": False}), patch.object(os, "kill") as kill:
            watch.verification_cycle(rows, 35, 95)
            kill.assert_called_once_with(20, __import__("signal").SIGKILL)
            watch.verification_cycle(rows, 50, 95)
            self.assertEqual(kill.call_count, 1)
        self.assertEqual(guard.read_json(guard.STATE / "10.result.json")["cleanup"], "failed")

    def test_recovery_budget_survives_new_owners_and_keeps_resource_fault_separate(self):
        for attempt in range(3):
            guard.verification_recovery("b" * 64, "containment", {"owner": "new-run-%d" % attempt})
        self.assertIn("scheduler-starvation", guard.verification_recovery("b" * 64)["blocked"])
        with self.assertRaisesRegex(RuntimeError, "scheduler-starvation"):
            guard.verification_recovery("b" * 64, "containment", {"owner": "renamed-again"})
        guard.verification_recovery("c" * 64, "resource", {"cores": 9})
        record = guard.verification_recovery("c" * 64)
        self.assertEqual(record["containment"], [])
        self.assertIn("recalibration", record["blocked"])
        self.assertIsNone(guard.verification_recovery("d" * 64)["blocked"])

    def test_seed_survives_supervisor_death_before_the_first_controller_sample(self):
        self.register(10)
        path = guard.STATE / 'roots/10.json'
        record = guard.read_json(path)
        record['verification']['seed'] = {'pid': 20, 'generation': 'native-one'}
        guard.write_json(path, record)
        watch = guard.Watch()
        watch.sample({20: self.row(parent=1), 21: self.row(parent=20)}, 0)
        self.assertEqual(set(watch.verification['10:native-one']['members']), {'20', '21'})
        watch.sample({20: self.row(parent=1, generation='native-two')}, 2)
        self.assertEqual(watch.verification, {})

    def test_own_envelope_breach_uses_resource_budget_even_on_a_quiet_host(self):
        self.register(10)
        path = guard.STATE / 'roots/10.json'
        record = guard.read_json(path)
        record['verification']['reservation'] = {'cores': 1, 'rss_mb': 1000, 'calibration': False}
        guard.write_json(path, record)
        watch = guard.Watch()
        watch.sample({10: self.row()}, 0)
        watch.verification_cycle({}, 0, 20)
        for now in (2, 4):
            rows = {10: self.row(cpu=2 * now)}
            watch.sample(rows, now); watch.verification_cycle(rows, now, 20)
        rows = {10: self.row(cpu=12)}
        watch.sample(rows, 6)
        with patch.object(guard.operator_fences, 'proc', return_value={'start':'native-one','zombie':False}), patch.object(os, 'kill'):
            watch.verification_cycle(rows, 6, 20)
        result = guard.read_json(guard.STATE / '10.result.json')
        self.assertEqual(result['status'], 'resource-envelope-exceeded')
        history = guard.verification_recovery('a' * 64)
        self.assertEqual((len(history['resource']), len(history['containment'])), (1, 0))
        self.assertIn('recalibration', history['blocked'])

    def test_unknown_calibration_cannot_extend_its_window_across_controller_restart(self):
        self.register(10)
        path = guard.STATE / 'roots/10.json'
        record = guard.read_json(path)
        record['verification'].update(started_monotonic=100,
            reservation={'cores': 6, 'rss_mb': 10000, 'calibration': True, 'max_seconds': 120})
        guard.write_json(path, record)
        rows = {10: self.row()}
        first = guard.Watch()
        first.sample(rows, 219)
        with patch.object(os, 'kill') as kill:
            first.verification_cycle(rows, 219, 10)
            kill.assert_not_called()
        restarted = guard.Watch()
        restarted.sample(rows, 220)
        with patch.object(guard.operator_fences, 'proc', return_value={'start':'native-one','zombie':False}), \
                patch.object(os, 'kill') as kill:
            restarted.verification_cycle(rows, 220, 10)
            kill.assert_called_once_with(10, __import__('signal').SIGTERM)
        result = guard.read_json(guard.STATE / '10.result.json')
        self.assertEqual(result['status'], 'resource-envelope-exceeded')
        self.assertIn('120-second', result['resource_fault'])
        history = guard.verification_recovery('a' * 64)
        self.assertEqual((len(history['resource']), len(history['containment'])), (1, 0))

    def test_controller_restart_recovers_pending_cleanup_without_charging_twice(self):
        self.register(10)
        watch = guard.Watch()
        rows = {10: self.row()}
        watch.sample(rows, 0); watch.verification_cycle(rows, 0, 95)
        rows = {10: self.row(cpu=20)}
        watch.sample(rows, 20)
        with patch.object(guard.operator_fences, 'proc', return_value={'start':'native-one','zombie':False}), patch.object(os, 'kill'):
            watch.verification_cycle(rows, 20, 95)
        restarted = guard.Watch()
        restarted.sample(rows, 22)
        with patch.object(os, 'kill') as kill:
            result = restarted.verification_cycle(rows, 22, 95)
            kill.assert_not_called()
        self.assertEqual(result['stage'], 'cleanup')
        self.assertEqual(len(guard.verification_recovery('a' * 64)['containment']), 1)

    def test_tree_rates_sum_small_workers_and_retain_detached_generations(self):
        self.register(10)
        watch = guard.Watch()
        watch.sample({10: self.row(), 20: self.row(10), 21: self.row(10)}, 0)
        candidates, _, _ = watch.sample({10: self.row(cpu=.1), 20: self.row(1, 4), 21: self.row(10, 5)}, 2, 95)
        self.assertEqual(candidates, [])  # Managed trees use the ordered policy.
        group = watch.verification["10:native-one"]
        self.assertAlmostEqual(group["cores"], 4.55)
        self.assertAlmostEqual(group["cpu_seconds"], 9.1)
        self.assertEqual(group["rss_mb"], 30)
        self.assertEqual(set(group["members"]), {"10", "20", "21"})
        watch.sample({10: self.row(cpu=.2), 20: self.row(1, 0, generation="native-two")}, 4)
        self.assertEqual(set(watch.verification["10:native-one"]["members"]), {"10"})

    def test_nested_registered_units_have_disjoint_ownership(self):
        self.register(10)
        watch = guard.Watch()
        watch.sample({10: self.row(), 20: self.row(10), 21: self.row(20)}, 0)
        self.register(20, priority="integration")
        watch.sample({10: self.row(cpu=1), 20: self.row(10, 2), 21: self.row(20, 4)}, 2)
        self.assertEqual(set(watch.verification["10:native-one"]["members"]), {"10"})
        self.assertEqual(set(watch.verification["20:native-one"]["members"]), {"20", "21"})
        self.assertEqual(watch.verification["20:native-one"]["priority"], "integration")

    def test_unknown_or_recycled_root_generation_never_becomes_a_managed_tree(self):
        self.register(10)
        watch = guard.Watch()
        watch.sample({10: self.row(generation="native-two"), 20: self.row(10)}, 0)
        self.assertEqual(watch.verification, {})
        with patch.object(guard, "processes", return_value={10: self.row(generation=None)}):
            with self.assertRaisesRegex(ValueError, "generation"):
                guard.register(10, "unknown", "verification", {"input_key": "a" * 64, "priority": "background"})
        self.register(10)
        watch.sample({10: self.row(), 20: self.row(10)}, 1)
        with self.assertRaisesRegex(RuntimeError, "generation unavailable"):
            watch.sample({10: self.row(), 20: self.row(10, generation=None)}, 2)

    def test_signal_revalidation_rejects_reuse_within_same_displayed_second(self):
        watch = guard.Watch()
        rows = {20: self.row(cpu=50)}
        watch.owned = {"20": {"owner": "fixture"}}
        with patch.object(guard, "processes", return_value={20: self.row(generation="native-two")}), patch.object(os, "kill") as kill:
            watch.stop(20, rows, set(), {20: 5})
            kill.assert_not_called()
        watch.pending = {20: ("native-one", 1)}
        with patch.object(os, "kill") as kill:
            watch.reap({20: self.row(generation="native-two")}, 2)
            kill.assert_not_called()
        self.assertEqual(watch.pending, {})


class ReservationTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix='verification-reservations.')
        self.addCleanup(temp.cleanup)
        state = patch.object(guard, 'STATE', Path(temp.name))
        state.start(); self.addCleanup(state.stop)
        guard.write_json(guard.STATE / 'heartbeat.json', {'cpu_count': 10, 'host_busy': 10,
            'unattributed_cores': 1, 'verification_resources': {'memory_free_mb': 16000, 'memory_total_mb': 24000}})
        self.context = {'input_key': 'a' * 64, 'seed': {'pid': 999991, 'generation': 'fixture'},
                        'result': str(guard.STATE / 'result.json')}

    def acquire(self, key='a', pid=999990):
        result = guard.reserve_verification({**self.context, 'input_key': key * 64}, pid, 'fixture')
        self.addCleanup(os.close, result[0])
        return result[1]

    def profile(self, key, cores=2, memory=1000):
        guard.write_json(guard.STATE / 'verification-demand' / (key * 64 + '.json'),
            {'protocol': 1, 'envelope': {'cores': cores, 'rss_mb': memory, 'calibration': False}})

    def test_unknown_calibration_is_exclusive_and_bounded_by_host_headroom(self):
        self.assertEqual(self.acquire(), {'cores': 6., 'rss_mb': 10000., 'calibration': True, 'max_seconds': 120})
        self.profile('b')
        with self.assertRaisesRegex(BlockingIOError, 'calibration'):
            self.acquire('b', 999992)

    def test_measured_cpu_and_memory_reserve_atomically_without_token_arithmetic(self):
        for key in 'abc': self.profile(key, cores=2, memory=4500)
        self.acquire('a'); self.acquire('b', 999992)
        with self.assertRaisesRegex(BlockingIOError, 'rss_mb'):
            self.acquire('c', 999993)
        self.profile('c', cores=3, memory=100)
        with self.assertRaisesRegex(BlockingIOError, 'cores'):
            self.acquire('c', 999993)
        with self.assertRaisesRegex(BlockingIOError, 'identical'):
            self.acquire('a', 999994)

    def test_reaped_short_lived_cpu_increases_the_measured_envelope(self):
        sample = {'samples': 3, 'sample_seconds': 4, 'cpu_seconds': 1, 'peak_cores': .25, 'peak_rss_mb': 100}
        guard.qualify_demand('a' * 64, sample, {'elapsed_seconds': 4, 'reaped_cpu_seconds': 8})
        profile = guard.read_json(guard.STATE / 'verification-demand' / ('a' * 64 + '.json'))
        self.assertGreater(profile['envelope']['cores'], 4)
        guard.qualify_demand('b' * 64, {**sample, 'samples': 1}, {'elapsed_seconds': .1, 'reaped_cpu_seconds': .1})
        self.assertFalse((guard.STATE / 'verification-demand' / ('b' * 64 + '.json')).exists())

    def test_missing_and_nonfinite_host_measurements_refuse_calibration(self):
        with self.assertRaises(BlockingIOError): guard.demand_capacity({})
        sample = guard.read_json(guard.STATE / 'heartbeat.json')
        with self.assertRaises(BlockingIOError): guard.demand_capacity({**sample, 'unattributed_cores': float('nan')})


class RecoveryTests(unittest.TestCase):
    setUp = ReservationTests.setUp

    def exhausted(self, key='a' * 64, kind='resource', cleanup='complete'):
        result = str(guard.STATE / (key + '.result.json'))
        for _ in range(1 if kind == 'resource' else 3):
            guard.verification_recovery(key, kind, {'result': result, 'cores': 3, 'rss_mb': 2000})
        guard.write_json(result, {'input_key': key, 'cleanup': cleanup,
            'status': 'resource-envelope-exceeded' if kind == 'resource' else 'contained'})
        return result

    def test_recovery_preserves_original_counters_and_resolves_to_one_exclusive_policy(self):
        key = 'a' * 64
        result = self.exhausted(key, 'containment')
        first = guard.request_verification_recovery(key, result)
        self.assertEqual(first, guard.request_verification_recovery(key, result))
        self.assertEqual(len(guard.verification_recovery(key)['containment']), 3)
        self.assertEqual(guard.verification_recovery(first['variant'])['containment'], [])
        context = guard.resolve_verification_policy({'input_key': key})
        self.assertEqual(context['input_key'], first['variant'])
        self.assertEqual(context['original_input_key'], key)
        self.assertEqual(context['resource_policy'], 'exclusive-calibration-v1')
        self.assertEqual(guard.resolve_verification_policy(dict(context)), context)

    def test_incomplete_or_mismatched_cleanup_cannot_authorize_recovery(self):
        result = self.exhausted(cleanup='pending')
        with self.assertRaisesRegex(BlockingIOError, 'cleanup'):
            guard.request_verification_recovery('a' * 64, result)
        guard.write_json(result, {'input_key': 'b' * 64, 'cleanup': 'complete', 'status': 'contained'})
        with self.assertRaisesRegex(BlockingIOError, 'cleanup'):
            guard.request_verification_recovery('a' * 64, result)
        self.assertFalse(list((guard.STATE / 'verification-policies').glob('*.json')))

    def test_restart_finds_required_recovery_and_alternate_exhaustion_is_terminal(self):
        self.exhausted()
        context = guard.resolve_verification_policy({'input_key': 'a' * 64})
        key = context['input_key']
        result = self.exhausted(key)
        with self.assertRaisesRegex(guard.RecoveryExhausted, 'exclusive recovery exhausted'):
            guard.request_verification_recovery(key, result)
        for lookup in (key, 'a' * 64):
            with self.assertRaises(guard.RecoveryExhausted):
                guard.resolve_verification_policy({'input_key': lookup})
        self.assertEqual(len(list((guard.STATE / 'verification-policies').glob('*.json'))), 2)

    def test_recalibration_reserves_exclusively_and_retains_the_fault_measurement_floor(self):
        self.exhausted()
        context = dict(self.context)
        lease = guard.reserve_verification(context, 999990, 'fixture')
        self.addCleanup(os.close, lease[0])
        self.assertTrue(lease[1]['calibration'] and lease[1]['exclusive'])
        other = {**self.context, 'input_key': 'b' * 64}
        with self.assertRaisesRegex(BlockingIOError, 'calibration'):
            guard.reserve_verification(other, 999992, 'fixture')
        guard.qualify_demand(context['input_key'], {'samples': 3, 'sample_seconds': 4,
            'cpu_seconds': 1, 'peak_cores': .5, 'peak_rss_mb': 100},
            {'elapsed_seconds': 4, 'reaped_cpu_seconds': 1})
        profile = guard.read_json(guard.STATE / 'verification-demand' / (context['input_key'] + '.json'))
        self.assertTrue(profile['envelope']['exclusive'])
        self.assertGreaterEqual(profile['envelope']['cores'], 4)
        self.assertGreaterEqual(profile['envelope']['rss_mb'], 2564)

    def test_partial_policy_publication_cannot_grant_another_recovery_variant(self):
        key = 'a' * 64
        result = self.exhausted(key)
        real_write = guard.write_json
        def interrupted(path, value, durable=False):
            if str(path).endswith('/verification-policies/' + key + '.json'):
                raise OSError('controlled interruption before origin publication')
            return real_write(path, value, durable=durable)
        with patch.object(guard, 'write_json', side_effect=interrupted):
            with self.assertRaisesRegex(OSError, 'interruption'):
                guard.request_verification_recovery(key, result)
        variants = list((guard.STATE / 'verification-policies').glob('*.json'))
        self.assertEqual(len(variants), 1)
        record = guard.request_verification_recovery(key, result)
        self.assertEqual(record['variant'], variants[0].stem)
        self.assertEqual(len(guard.verification_recovery(key)['resource']), 1)


class CostTests(unittest.TestCase):
    setUp = ReservationTests.setUp

    def cost(self, cpu=50, elapsed=50, **changes):
        completion = {'status': 'completed', 'exit': 0, 'cleanup': 'complete',
            'reaped_cpu_seconds': cpu, 'elapsed_seconds': elapsed,
            'measurement': {'load': {'samples': 3, 'busy_total': 90, 'busy_peak': 35,
                'owners_peak': 1, 'pressure_seen': False, 'unknown': False}}, **changes}
        context = {**self.context, 'cost_comparison': {'key': 'c' * 64,
                   'predicted_seconds': 45, 'check': 'fixture'}}
        return guard.record_verification_cost(context, completion)

    def test_growth_requires_both_thresholds_and_cannot_raise_the_baseline(self):
        self.assertEqual(self.cost()['status'], 'within-baseline')
        self.assertEqual(self.cost(60, 60)['status'], 'within-baseline')  # Exactly 20% is not more than 20%.
        result = self.cost(60.1, 60.1)
        self.assertEqual(set(result['growth']), {'cpu_seconds', 'elapsed_seconds'})
        self.assertEqual(result['baseline']['cpu_seconds'], 50)
        self.assertEqual(self.cost(80, 80)['baseline']['cpu_seconds'], 50)
        self.assertEqual(guard.previous_verification_cost('c' * 64)['status'], 'growth')

    def test_elapsed_growth_under_different_load_is_unknown_but_cpu_is_reported(self):
        self.cost()
        result = self.cost(75, 100, measurement={'load': {'samples': 3, 'busy_total': 270,
            'busy_peak': 95, 'owners_peak': 3, 'pressure_seen': True, 'unknown': False}})
        self.assertEqual(set(result['growth']), {'cpu_seconds'})
        self.assertFalse(result['elapsed_comparable'])
        self.assertTrue(result['uncertainty'])

    def test_missing_samples_or_failed_execution_cannot_claim_comparable_elapsed(self):
        self.assertEqual(self.cost(status='contained')['status'], 'incomplete')
        self.assertIsNone(guard.previous_verification_cost('c' * 64))
        self.assertEqual(self.cost(cpu=float('nan'))['status'], 'unknown')
        self.cost()
        result = self.cost(51, 100, measurement={})
        self.assertEqual(result['growth'], {})
        self.assertEqual(result['status'], 'uncertain-growth')
        self.assertIn('elapsed_seconds', result['unconfirmed_growth'])
        self.assertFalse(result['elapsed_comparable'])
        self.assertEqual(guard.record_verification_cost(self.context, {})['status'], 'unqualified')

    def test_history_is_bounded_and_preserves_original_baseline(self):
        first = self.cost()
        for i in range(21):
            self.cost(50 + i, 50 + i)
        record = guard.read_json(first['history'])
        self.assertEqual(len(record['recent']), 20)
        self.assertEqual(record['baseline'], first['baseline'])

    def test_cost_baseline_remains_auditable_after_original_run_rotation(self):
        original = guard.STATE / 'rotated-run'
        original.mkdir()
        self.context.update(result=str(original / 'supervision.json'),
            input_evidence={'source': {'commit': 'original'}, 'input': {'tools': 'original-tools'}})
        first = self.cost()
        guard.write_json(self.context['result'], first)
        shutil.rmtree(original)
        self.context['input_evidence']['source']['commit'] = 'later'
        result = self.cost(80, 80)
        saved = result['baseline']['evidence']
        self.assertEqual(saved['context']['input_evidence']['source']['commit'], 'original')
        self.assertEqual(saved['completion']['reaped_cpu_seconds'], 50)
        self.assertEqual(saved['completion']['measurement']['load']['samples'], 3)
        self.assertEqual(result['baseline'], first['baseline'])
        self.assertEqual(result['status'], 'growth')


class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='verification-client.')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {**os.environ, 'RICHOS_CPU_GUARD_STATE': str(self.root)}
        self.context = self.root / 'context.json'
        guard.write_json(self.context, {'protocol': 1, 'input_key': 'a' * 64,
            'label': 'fixture', 'priority': 'background', 'result': str(self.root / 'result.json')})
        for name in ('heartbeat', 'devices-heartbeat'):
            guard.write_json(self.root / (name + '.json'), {'at': time.time(), 'ok': True, 'cpu_count': 10, 'host_busy': 10, 'unattributed_cores': 1, 'verification_resources': {'memory_free_mb': 16000, 'memory_total_mb': 24000}})
        guard.write_json(self.root / 'verification-pressure.json',
            {'protocol': 1, 'at': time.time(), 'admission_open': True})

    def start(self, code, *args):
        child = subprocess.Popen(proc_tree.command([sys.executable, '-B', '-c', code, *map(str, args)],
            verification=self.context), env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        def cleanup():
            if child.poll() is None:
                child.terminate()
            child.communicate(timeout=15)
        self.addCleanup(cleanup)
        return child

    def wait_record(self, path):
        until = time.monotonic() + 5
        while time.monotonic() < until:
            if path.exists():
                return path.read_text()
            time.sleep(.02)
        self.fail('child did not reach its execution barrier')

    def test_real_supervisor_enrolls_before_exec_and_records_completed_cleanup(self):
        code = "import json,os,pathlib,time; p=pathlib.Path(os.environ['RICHOS_CPU_GUARD_STATE']); r=json.loads(next((p/'roots').glob('*.json')).read_text()); assert r['verification']['seed']['pid']==os.getpid(); assert os.environ['RICHOS_VERIFICATION_OWNER']; assert 0 <= time.monotonic()-r['verification']['started_monotonic'] < 10; assert r['verification']['reservation']['max_seconds'] == 120"
        child = self.start(code)
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 0, err)
        record = guard.read_json(self.root / 'result.json')
        self.assertEqual((record['status'], record['cleanup'], record['exit']), ('completed', 'complete', 0))
        self.assertGreater(record['reaped_cpu_seconds'], 0)

    def test_nested_supervisor_validates_owner_without_duplicate_registration(self):
        marker = self.root / 'nested-executed'
        code = ("import sys,subprocess; sys.path.insert(0,sys.argv[1]); import proc_tree; "
                "r=subprocess.run(proc_tree.command([sys.executable,'-c',"
                "'from pathlib import Path; Path(__import__(\"sys\").argv[1]).touch()',sys.argv[2]])); "
                "raise SystemExit(r.returncode)")
        child = self.start(code, Path(proc_tree.__file__).parent, marker)
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 0, err)
        self.assertTrue(marker.is_file())
        self.assertEqual(len(list((self.root / 'roots').glob('*.json'))), 1)
        result = guard.read_json(self.root / 'result.json')
        self.assertEqual((result['status'], result['cleanup']), ('completed', 'complete'))

    def test_real_supervisor_records_actual_cpu_and_elapsed_with_uncertainty(self):
        context = guard.read_json(self.context)
        context['cost_comparison'] = {'key': 'c' * 64, 'predicted_seconds': 1, 'check': 'fixture'}
        guard.write_json(self.context, context)
        child = self.start('sum(range(10000))')
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 0, err)
        result = guard.read_json(self.root / 'result.json')
        cost = result['cost']
        self.assertGreater(cost['actual']['cpu_seconds'], 0)
        self.assertEqual(cost['actual']['cpu_seconds'], result['reaped_cpu_seconds'])
        self.assertEqual(cost['actual']['elapsed_seconds'], result['elapsed_seconds'])
        self.assertTrue(cost['uncertainty'])  # No controller load samples in this fixture.
        self.assertTrue(Path(cost['history']).is_file())

    def test_controller_failure_stops_an_already_running_child(self):
        ready = self.root / 'ready'
        child = self.start("import os,sys,time;open(sys.argv[1],'w').write(str(os.getpid()));time.sleep(60)", ready)
        pid = int(self.wait_record(ready))
        guard.write_json(self.root / 'heartbeat.json', {'at': time.time(), 'ok': False})
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 125, err)
        self.assertEqual(proc_tree._alive([pid]), [])
        record = guard.read_json(self.root / 'result.json')
        self.assertEqual((record['status'], record['cleanup']), ('incomplete', 'complete'))

    def test_closed_admission_cannot_execute_then_claim_a_test_failure(self):
        guard.write_json(self.root / 'verification-pressure.json',
            {'protocol': 1, 'at': time.time(), 'admission_open': False, 'stage': 'admission-closed'})
        executed = self.root / 'executed'
        child = self.start("import pathlib,sys;pathlib.Path(sys.argv[1]).touch()", executed)
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 125, err)
        self.assertFalse(executed.exists())
        self.assertEqual(guard.read_json(self.root / 'result.json')['status'], 'incomplete')

    def test_pressure_cancel_preserves_typed_result_and_reaped_cost(self):
        ready = self.root / 'ready'
        child = self.start("import os,sys,time;open(sys.argv[1],'w').write(str(os.getpid()));time.sleep(60)", ready)
        self.wait_record(ready)
        root = guard.read_json(self.root / 'roots' / (str(child.pid) + '.json'))
        guard.write_json(self.root / 'result.json', {'input_key': 'a' * 64,
            'root_generation': root['generation'], 'status': 'contained', 'cause': 'host-pressure', 'cleanup': 'pending', 'budget_used': 1})
        child.terminate()
        out, err = child.communicate(timeout=10)
        self.assertEqual(child.returncode, 143, err)
        result = guard.read_json(self.root / 'result.json')
        self.assertEqual((result['status'], result['cause'], result['cleanup']), ('contained', 'host-pressure', 'complete'))
        self.assertGreater(result['elapsed_seconds'], 0)


class RunnerPressureTests(unittest.TestCase):
    setUp = SupervisorTests.setUp
    def test_runner_preserves_contained_attempt_then_retries_ahead_of_new_work(self):
        self.runner_case('containment')

    def test_runner_retries_resource_fault_in_exclusive_policy_with_private_environment(self):
        self.runner_case('resource')

    def runner_case(self, kind):
        import importlib.util
        from types import SimpleNamespace
        spec = importlib.util.spec_from_file_location('managed_runner_fixture',
            Path(__file__).resolve().parents[3] / 'app/scripts/proof-run.py')
        runner = importlib.util.module_from_spec(spec); spec.loader.exec_module(runner)
        marker = self.root / 'executions'
        script = self.root / 'unit.py'
        status = 'contained' if kind == 'containment' else 'resource-envelope-exceeded'
        script.write_text("import json,os,pathlib,signal,time\n"
            + "import sys;sys.path.insert(0," + repr(str(Path(guard.__file__).parent)) + ")\n"
            + "import cpu_guard\n"
            + "root=pathlib.Path(os.environ['RICHOS_CPU_GUARD_STATE'])\n"
            + "marker=root/'executions'\n"
            + "if not marker.exists():\n"
            + " marker.write_text('first')\n"
            + " row=json.loads(next((root/'roots').glob('*.json')).read_text())\n"
            + " cpu_guard.verification_recovery(row['verification']['input_key']," + repr(kind) + ",{'result':row['verification']['result']})\n"
            + " result={'input_key':row['verification']['input_key'],'root_generation':row['generation'],'status':" + repr(status) + ",'cause':'injected fixture event','cleanup':'pending','budget_used':1,'at':time.time()}\n"
            + " pathlib.Path(row['verification']['result']).write_text(json.dumps(result))\n"
            + " os.kill(row['pid'],signal.SIGTERM);time.sleep(60)\n"
            + "else: marker.write_text('retried')\n")
        item = runner.Item('controlled pressure', runner.ROOT, [sys.executable, '-B', str(script)], 'fixture', 1)
        if kind == 'resource':
            item.private_environment = {'PATH': os.environ['PATH'], 'PYTHONDONTWRITEBYTECODE': '1'}
        other = runner.Item('independent', runner.ROOT, [sys.executable, '-B', '-c', 'print(123)'], 'fixture', 1)
        args = SimpleNamespace(capacity=2,engine_shards=2,admission_wait=5,max_cpu=80,budget=10,deadline=10,sample_every=.5,slot_wait=None)
        idle = lambda: {'cpu_user_percent': 5, 'cpu_system_percent': 2,'cpu_idle_percent':93,
            'swapout_mb_per_s':0,'memory_pressure':'normal','memory_free_percent':80,'swap_used_mb':0}
        env = {**self.env, 'RICHOS_MACHINE_WORKERS': str(self.root / 'machine')}
        with patch.object(guard, 'STATE', self.root), patch.dict(os.environ, env), patch.object(runner, 'SETTLE_SECONDS', 0):
            runner.run([item, other], args, str(self.root / 'run'), sampler=idle)
        self.assertEqual((item.state, other.state), ('passed', 'passed'), item.notes)
        self.assertEqual(marker.read_text(), 'retried')
        self.assertEqual([attempt['state'] for attempt in item.attempts], [status, 'passed'])
        self.assertTrue(all(Path(attempt['log']).is_file() for attempt in item.attempts))
        self.assertLess(item.ended, other.started)
        if kind == 'resource':
            context = guard.read_json(item.verification_context)
            self.assertEqual(context['resource_policy'], 'exclusive-calibration-v1')
            self.assertNotEqual(context['input_key'], context['original_input_key'])


if __name__ == "__main__":
    unittest.main()

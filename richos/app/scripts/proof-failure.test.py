#!/usr/bin/env python3
"""Bounded runner fixtures: independent failure, dependencies and quarantine."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("proof_run", HERE / "proof-run.py")
pr = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pr)


class FailurePolicy(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="proof-failure-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.env = patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(self.root / "machine")})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.args = SimpleNamespace(capacity=2, admission_wait=5, max_cpu=80,
                                    budget=600, deadline=1800, sample_every=.1)

    def item(self, name, code, **kw):
        return pr.Item(name, str(self.root), [sys.executable, "-c", code], weight=kw.pop("weight", .1), **kw)

    def run_items(self, items):
        with patch.object(pr, "SETTLE_SECONDS", 0), patch.object(pr, "admitted", return_value=(True, {})), \
                contextlib.redirect_stdout(io.StringIO()):
            pr.run(items, self.args, str(self.root / "run"), sampler=lambda: {
                "cpu_user_percent": 5, "cpu_system_percent": 2, "swapout_mb_per_s": 0,
                "memory_pressure": "normal"})

    def test_default_preserves_running_and_queued_independent_checks(self):
        items = [self.item("failure", "import time; time.sleep(.4); raise SystemExit(1)"),
                 self.item("sibling", "import time; time.sleep(.8)"),
                 self.item("queued", "pass")]
        self.run_items(items)
        self.assertEqual([i.state for i in items], ["failed", "passed", "passed"])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(pr.summarize(items, 1, str(self.root / "run")), 1)

    def test_integration_priority_turn_ends_but_the_plan_finishes(self):
        # 2026-09-29: the land of cc/echo-opus-speckle3 was refused three times on the 600 s
        # episode ("fixed integration episode expired before admission"), 47, 30 and 10 of 78
        # checks unrun and none failed. The bound is on PRIORITY: when it ends, a background
        # run must get admission at once, and this plan's checks must still all run.
        marker = self.root / "ran-after-the-turn"
        background = ("import sys, time; sys.path.insert(0, %r); import engine_pass; time.sleep(2.5); "
                      "a = engine_pass.Admission(%r, %r); ok = a.begin(); a.close(); "
                      "sys.exit(0 if ok else 3)" % (os.path.dirname(pr.engine_pass.__file__),
                                                    str(self.root / "machine"), str(self.root)))
        items = [self.item("finished", "pass", weight=4),
                 # Still running when the 1.4 s turn ends; a background run asks for admission
                 # while it runs, which only a released priority grants (exit 3 otherwise).
                 self.item("running", background, weight=3),
                 self.item("after the turn", "from pathlib import Path; Path(%r).touch()" % str(marker),
                           after=["running"])]
        with patch.object(pr.engine_pass, "is_main_checkout", return_value=True), \
                patch.object(pr.engine_pass, "INTEGRATION_PLAN_SECONDS", 1.4):
            self.run_items(items)
        self.assertEqual([(it.label, it.state) for it in items],
                         [("finished", "passed"), ("running", "passed"), ("after the turn", "passed")],
                         [it.notes for it in items])
        self.assertTrue(marker.exists())
        self.assertGreater(items[1].seconds, 1.4)
        report = json.loads((self.root / "run" / "priority-episode.json").read_text())
        self.assertEqual(report["status"], "over-budget")
        self.assertEqual(len(report["plan"]), 3)
        self.assertFalse(pr.engine_pass._locked(str(self.root / "machine" / "admission" / "priority.lock")))

    def test_waiting_integration_plan_never_takes_priority_and_runs_as_background_after_its_turn(self):
        machine = str(self.root / "machine")
        owner = pr.engine_pass.IntegrationPlan(machine, ["other fixed plan"], str(self.root))
        self.assertTrue(owner.enter())
        item = self.item("waiting", "pass")
        try:
            with patch.object(pr.engine_pass, "is_main_checkout", return_value=True), \
                    patch.object(pr.engine_pass, "INTEGRATION_PLAN_SECONDS", .4):
                self.run_items([item])
            # It waited for the other plan's turn, never published priority of its own, and
            # when its own turn had passed it ran as background work instead of being refused.
            self.assertEqual(item.state, "passed", item.notes)
            self.assertGreater(item.wait_times.get("integration-episode", 0), .2)
            self.assertEqual(json.loads(Path(owner.path).read_text())["status"], "active")
            self.assertFalse(pr.engine_pass._locked(str(Path(machine) / "admission" / "priority.lock")))
        finally:
            owner.close()

    def test_explicit_fail_fast_cancels_unfinished_checks(self):
        self.args.fail_fast = True
        # "queued" is ordered after the failure (an ordering-only dependency, still independent
        # of its success), so it is still waiting when the failure is observed: schedule() polls
        # the failure, then cancels in the same pass, before anything new can start. Without the
        # ordering it raced the failure for the second worker; at low CPU priority (taskpolicy
        # -c utility nice -n 20) it started and passed first, three land attempts on 2026-10-02.
        items = [self.item("failure", "raise SystemExit(1)"),
                 self.item("queued", "pass", after=["failure"])]
        self.run_items(items)
        self.assertEqual([i.state for i in items], ["failed", "cancelled"])
        self.assertGreater(items[1].admission_wait, 0)

    def test_success_dependency_blocks_but_diagnostic_runs(self):
        items = [self.item("failure", "raise SystemExit(1)"),
                 self.item("dependent", "raise AssertionError('must not execute')", requires=["failure"]),
                 self.item("diagnostic", "pass", after=["failure"])]
        self.run_items(items)
        self.assertEqual([i.state for i in items], ["failed", "blocked", "passed"])
        self.assertIsNone(items[1].started)
        self.assertGreater(items[1].admission_wait, 0)

    def test_contamination_stops_domain_even_if_emitter_exits_zero(self):
        code = ("import os; from pathlib import Path; "
                "p=Path(os.environ['RICHOS_VERIFICATION_CONTAMINATION']); "
                "p.mkdir(); (p/'unit.json').write_text('{}')")
        items = [self.item("unsafe", code), self.item("queued", "pass")]
        self.run_items(items)
        self.assertEqual(items[1].state, "cancelled")
        self.assertEqual(items[-1].label, "execution domain contaminated")
        self.assertEqual(items[-1].state, "failed")

    def test_unknown_or_cyclic_dependencies_refuse_before_launch(self):
        for dependencies in (["missing"], ["self"]):
            item = self.item("self", "pass", requires=dependencies)
            with self.assertRaisesRegex(ValueError, "prerequisites"):
                self.run_items([item])
            self.assertIsNone(item.started)

    def test_large_engine_slot_refusal_preserves_independent_check(self):
        self.args.engine_slot_wait = .5
        slot_root = self.root / "engine-slot"
        code = ("import sys; sys.path.insert(0,sys.argv[1]); import engine_pass; "
                "s=engine_pass.acquire(20,'fixture',sys.argv[2],wait=0); "
                "print('ready',flush=True); input()")
        with patch.dict(os.environ, {"RICHOS_ENGINE_PASS_DIR": str(slot_root)}):
            holder = subprocess.Popen([sys.executable, "-c", code,
                                       str(Path(pr.engine_pass.__file__).parent), str(self.root)],
                                      stdout=subprocess.PIPE, stdin=subprocess.PIPE,
                                      stderr=subprocess.DEVNULL, text=True)
            try:
                self.assertEqual(holder.stdout.readline().strip(), "ready")
                items = [pr.Item("engine-%d" % i, str(self.root),
                                 ["bash", "scripts/ci-shard.sh", "--only-units", str(i)])
                         for i in range(20)]
                independent = self.item("independent", "pass")
                items.append(independent)
                self.run_items(items)
                self.assertEqual(independent.state, "passed")
                self.assertTrue(all(i.state == "not-admitted" for i in items[:-1]))
                self.assertTrue(all(i.started is None for i in items[:-1]))
                self.assertTrue(all(i.admission_wait >= .5 for i in items[:-1]))
            finally:
                holder.communicate("\n", timeout=5)

    def engine_slot_fixture(self, release_after):
        """Two engine units (FULL_PASS_UNITS patched to 2, so they need the slot) and a live holder
        of the slot that frees it `release_after` real seconds from now. A teammate run."""
        (self.root / "scripts").mkdir()
        (self.root / "scripts" / "ci-shard.sh").write_text("exit 0\n")
        slot_root = self.root / "engine-slot"
        code = ("import sys, time; sys.path.insert(0,sys.argv[1]); import engine_pass; "
                "s=engine_pass.acquire(20,'fixture holder',sys.argv[2],wait=0); "
                "print('ready',flush=True); time.sleep(float(sys.argv[3]))")
        for change in (patch.dict(os.environ, {"RICHOS_ENGINE_PASS_DIR": str(slot_root)}),
                       patch.object(pr.engine_pass, "FULL_PASS_UNITS", 2),
                       patch.object(pr.engine_pass, "is_main_checkout", return_value=False)):
            change.start()
            self.addCleanup(change.stop)
        holder = subprocess.Popen([sys.executable, "-c", code, str(Path(pr.engine_pass.__file__).parent),
                                   str(self.root), str(release_after)],
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.addCleanup(holder.wait, 30)
        self.assertEqual(holder.stdout.readline().strip(), "ready")
        holder.stdout.close()
        return [pr.Item("engine-%d" % i, str(self.root), ["bash", "scripts/ci-shard.sh", "--only-units", str(i)])
                for i in range(2)]

    def test_engine_units_queued_past_the_old_teammate_bound_still_run(self):
        # 2026-10-30: P1-B's teammate proof run waited its 1800 s (engine_pass.TEAMMATE_WAIT) for
        # the engine slot and failed all 76 engine units "not-admitted" for only waiting its turn.
        # The holder here is alive for 2 s of real time; engine_pass's clock jumps 1801 s after
        # 0.5 s, so the run has queued past the old bound while the slot is still held.
        units = self.engine_slot_fixture(release_after=2)
        offset = [0.0]
        clock = SimpleNamespace(monotonic=lambda: time.monotonic() + offset[0], time=time.time, sleep=time.sleep)
        jump = threading.Timer(.5, lambda: offset.__setitem__(0, 1801.0))
        jump.start()
        self.addCleanup(jump.cancel)
        with patch.object(pr.engine_pass, "time", clock):
            self.run_items(units)
        self.assertEqual([i.state for i in units], ["passed", "passed"], [i.notes for i in units])
        self.assertTrue(all(i.slot_wait > 1 for i in units), [i.slot_wait for i in units])

    def test_engine_slot_wait_does_not_consume_the_run_cap(self):
        # 2026-10-30: land-gate attempt __c3r0s4 was ended at its run cap after its engine units
        # had queued 285 s for the slot. Here the slot is held for 4 s and the run cap is 2.5 s:
        # only execution counts, so the two quick units still run once the slot frees.
        units = self.engine_slot_fixture(release_after=4)
        self.args.engine_slot_wait = 60
        self.args.run_cap = 2.5
        started = time.monotonic()
        self.run_items(units)
        self.assertEqual([i.state for i in units], ["passed", "passed"], [i.notes for i in units])
        self.assertGreater(time.monotonic() - started, 3.5)

    def test_lane_refill_avoids_global_delay_but_new_lane_still_ramps(self):
        items = [self.item("slow", "import time; time.sleep(4)", lane="a", weight=3),
                 self.item("first", "pass", lane="b", weight=2),
                 self.item("replacement", "pass", lane="b", weight=1)]
        with patch.object(pr, "admitted", return_value=(True, {})), \
                contextlib.redirect_stdout(io.StringIO()):
            pr.run(items, self.args, str(self.root / "run"), sampler=lambda: {
                "cpu_user_percent": 5, "cpu_system_percent": 2, "swapout_mb_per_s": 0,
                "memory_pressure": "normal"})
        self.assertTrue(all(i.state == "passed" for i in items))
        self.assertGreaterEqual(items[1].started - items[0].started, pr.SETTLE_SECONDS)
        self.assertLess(items[2].started - items[1].started, pr.SETTLE_SECONDS)

    def test_ramp_does_not_wait_for_a_check_that_has_already_exited(self):
        # The ramp waits SETTLE_SECONDS after a start so the next sample shows that check's
        # load. Here the ramp's check, `quick`, exits at once; `blocker` (a replacement in
        # quick's lane, which the ramp does not time) keeps the run busy until `next` has
        # started. A ramp that honors only the clock holds `next` for the whole (huge) ramp,
        # so `blocker` gives up and fails; one that sees `quick` gone starts `next` at once.
        # 2026-09-29: the short tail of a 78-check land ran one check at a time, 4-5 s apart.
        ready = self.root / "next-started"
        blocker = ("import pathlib, time\nfor _ in range(600):\n    if pathlib.Path(%r).exists(): break\n"
                   "    time.sleep(.05)\nelse:\n    raise SystemExit('next never started')" % str(ready))
        items = [self.item("quick", "pass", lane="shared", weight=3),
                 self.item("blocker", blocker, lane="shared", weight=2),
                 self.item("next", "from pathlib import Path; Path(%r).touch()" % str(ready), weight=1)]
        with patch.object(pr, "SETTLE_SECONDS", 1000), patch.object(pr, "admitted", return_value=(True, {})), \
                contextlib.redirect_stdout(io.StringIO()):
            pr.run(items, self.args, str(self.root / "run"), sampler=lambda: {
                "cpu_user_percent": 5, "cpu_system_percent": 2, "swapout_mb_per_s": 0,
                "memory_pressure": "normal"})
        self.assertEqual([(i.label, i.state) for i in items],
                         [("quick", "passed"), ("blocker", "passed"), ("next", "passed")], [i.notes for i in items])
        self.assertLess(items[1].started, items[2].started)

    def test_shared_host_sample_keeps_pressure_and_never_reuses_stale_or_failed_data(self):
        values = iter([
            {"cpu_user_percent": 5, "cpu_system_percent": 2, "memory_pressure": "critical",
             "swapout_mb_per_s": 0},
            {"cpu_user_percent": 95, "cpu_system_percent": 2, "memory_pressure": "normal",
             "swapout_mb_per_s": 0},
        ])
        calls = []
        def sample():
            calls.append(1)
            return next(values)
        with patch.object(pr.time, "monotonic", return_value=10) as clock:
            shared = pr.HostSamples(sample, 10)
            self.assertFalse(pr.admitted(self.args, shared)[0])
            self.assertFalse(pr.admitted(self.args, shared)[0])
            self.assertEqual(len(calls), 1)
            clock.return_value = 11.1
            self.assertFalse(pr.admitted(self.args, shared)[0])
            self.assertEqual(len(calls), 2)
            clock.return_value = 12.2
            with self.assertRaises(StopIteration):
                shared()
            with self.assertRaises(StopIteration):
                shared()
            self.assertEqual(len(calls), 4)

    def test_cli_defaults_and_compatibility(self):
        for flags, expected in (([], False), (["--keep-going"], False), (["--fail-fast"], True)):
            seen = []
            def selection(args):
                seen.append(args.fail_fast)
                return []
            with patch.object(pr, "selection", side_effect=selection), \
                    patch.object(pr, "default_logdir", return_value=str(self.root)), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(pr.main([*flags, "--log-dir", str(self.root / (str(expected) + str(flags)))]), 0)
            self.assertEqual(seen, [expected])

    def test_retry_reports_all_execution_cpu_and_queue_time_once(self):
        item = self.item('retry costs', 'pass')
        item.queued_at = 2
        with patch.object(pr.time, 'monotonic', return_value=5):
            item.finish_queue()
            item.finish_queue()  # Repeated checkpoint/finalization is idempotent.
        for index, (start, end, state, cpu) in enumerate(((10, 15, 'contained', 2), (30, 41, 'completed', 3))):
            item.started, item.ended, item.state, item.rc = start, end, 'passed', 0
            item.verification_result = str(self.root / ('attempt-%s.json' % index))
            Path(item.verification_result).write_text(json.dumps({
                'status': state, 'cleanup': 'complete', 'reaped_cpu_seconds': cpu, 'input_key': 'a' * 64,
                'budget_used': 1}))
            with patch.object(pr.cpu_guard, 'verification_recovery', return_value={
                    'blocked': None, 'containment': [{'result': item.verification_result}]}):
                retry = pr.finish_attempt(item)
            self.assertEqual(retry, index == 0)
            if retry:
                item.started = item.ended = None
                item.queued_at = 20
                self.assertEqual(item.total_seconds, 5)
                with patch.object(pr.time, 'monotonic', return_value=27):
                    item.finish_queue()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(pr.summarize([item], 41, str(self.root)), 0)
        summary = json.loads((self.root / 'summary.json').read_text())
        row = summary['checks'][0]
        self.assertEqual(summary['serial_seconds'], 16)
        self.assertEqual((row['seconds'], row['last_attempt_seconds'], row['repeated_seconds']), (16, 11, 5))
        self.assertEqual((row['cpu_seconds'], row['repeated_cpu_seconds'], row['total_queue_seconds']), (5, 2, 10))
        self.assertEqual([a['queue_seconds'] for a in row['attempts']], [3, 7])

    def test_resource_refusal_keeps_its_wait_reason_and_queue_cost(self):
        self.args.admission_wait = .4
        item = self.item('no measured capacity', "raise AssertionError('must not execute')")
        with patch.object(pr, 'reserve_item', side_effect=BlockingIOError('measured CPU capacity is full')):
            self.run_items([item])
        self.assertEqual((item.state, item.rc), ('not-admitted', 75))
        self.assertGreaterEqual(item.admission_wait, .4)
        self.assertGreater(item.wait_times.get('resource-envelope', 0), 0)
        self.assertEqual(item.wait_times.get('worker', 0), 0)
        self.assertIsNone(item.started)

    def test_machine_pressure_refusal_is_the_mac_refusing_and_ends_not_admitted(self):
        # 2026-09-29 (part-2 hunt section 01): only the Mac refusing spends --admission-wait. The
        # verification pressure controller closing admission is the Mac refusing, so a check it
        # keeps refusing still ends NOT ADMITTED, and the wait is named for what it was.
        self.args.admission_wait = .4
        item = self.item('pressure refused', "raise AssertionError('must not execute')")
        with patch.object(pr.cpu_guard, 'verification_admission', return_value='verification pressure: red'):
            self.run_items([item])
        self.assertEqual((item.state, item.rc), ('not-admitted', 75))
        self.assertGreaterEqual(item.refused_wait, .4)
        self.assertGreater(item.wait_times.get('pressure', 0), 0)
        self.assertIn('verification pressure: red', item.notes[-1])
        self.assertIsNone(item.started)

    def test_envelope_full_of_this_runs_own_work_is_a_queue_not_a_refusal(self):
        # The measured resource envelope refusing while this run's own check holds it is this
        # run's queue: the queued check waits past --admission-wait and then runs.
        self.args.admission_wait = .3
        sibling = self.item('holds the envelope', 'import time; time.sleep(1.2)', weight=2)
        queued = self.item('queued', 'pass', weight=1)
        def reserve(candidate, *args):
            if candidate is queued and sibling.state == 'running':
                raise BlockingIOError('measured verification capacity is full: cores')
        with patch.object(pr, 'reserve_item', side_effect=reserve):
            self.run_items([sibling, queued])
        self.assertEqual((sibling.state, queued.state), ('passed', 'passed'), queued.notes)
        self.assertGreater(queued.wait_times.get('resource-envelope', 0), self.args.admission_wait)
        self.assertLess(queued.refused_wait, self.args.admission_wait)
        self.assertGreaterEqual(queued.started, sibling.ended)

    BUSY_HOST = {"cpu_user_percent": 60, "cpu_system_percent": 38, "cpu_idle_percent": 2,
                 "memory_pressure": "normal", "memory_free_percent": 50, "swap_used_mb": 0,
                 "swapout_mb_per_s": 0}

    def run_refused_while(self, items, refuse, kind):
        """run_items, with the Mac refusing (`kind`: a host sample or machine pressure) whenever
        `refuse()` is true, and samples 0.1 s apart instead of reserve's 30 s floor."""
        def host(*_args):
            return (False, dict(self.BUSY_HOST)) if kind == "host" and refuse() else (True, {})

        def pressure():
            return "verification pressure: red" if kind == "pressure" and refuse() else None
        with patch.object(pr, "SETTLE_SECONDS", 0), patch.object(pr, "admitted", side_effect=host), \
                patch.object(pr.reserve, "MIN_RETRY_SECONDS", .1), \
                patch.object(pr.cpu_guard, "verification_admission", side_effect=pressure), \
                contextlib.redirect_stdout(io.StringIO()):
            pr.run(items, self.args, str(self.root / ("run-" + kind)), sampler=lambda: dict(self.BUSY_HOST))

    def test_host_or_pressure_refusal_while_this_runs_own_check_runs_is_a_queue_not_a_refusal(self):
        # Part 2 recheck, R01: the Mac's CPU line or its pressure controller refusing while this
        # run's own sibling still runs cannot be told apart from the load that sibling makes, so
        # it is this run's queue, as the full worker budget and envelope already are. The queued
        # check waits past --admission-wait for as long as the sibling runs (its deadline bounds
        # that) and then gets its turn.
        for kind in ("host", "pressure"):
            with self.subTest(kind=kind):
                self.args.admission_wait = .3
                sibling = self.item("keeps the Mac busy " + kind, "import time; time.sleep(1.2)")
                queued = self.item("queued " + kind, "pass")
                self.run_refused_while([sibling, queued], lambda: sibling.state == "running", kind)
                self.assertEqual((sibling.state, queued.state), ("passed", "passed"), queued.notes)
                self.assertGreater(queued.wait_times.get(kind, 0), self.args.admission_wait)
                self.assertLess(queued.refused_wait, self.args.admission_wait)
                self.assertGreaterEqual(queued.started, sibling.ended)

    def test_host_or_pressure_refusal_after_this_runs_own_work_ends_still_ends_not_admitted(self):
        # The other half of R01, kept: once nothing of this run is running, a Mac that keeps
        # refusing is the Mac refusing, and the check ends NOT ADMITTED at --admission-wait.
        for kind in ("host", "pressure"):
            with self.subTest(kind=kind):
                self.args.admission_wait = .3
                sibling = self.item("short sibling " + kind, "import time; time.sleep(.6)")
                queued = self.item("never admitted " + kind, "raise AssertionError('must not execute')")
                self.run_refused_while([sibling, queued], lambda: sibling.state != "waiting", kind)
                self.assertEqual((sibling.state, queued.state, queued.rc), ("passed", "not-admitted", 75))
                self.assertGreaterEqual(queued.refused_wait, self.args.admission_wait)
                self.assertIsNone(queued.started)

    def test_missing_or_failed_cleanup_preserves_attempt_cost_without_green(self):
        for index, record in enumerate((None, {'status': 'completed', 'cleanup': 'failed'})):
            item = self.item('incomplete ' + str(index), 'pass')
            item.started, item.ended, item.state, item.rc = 2, 7, 'passed', 0
            item.verification_result = str(self.root / ('bad-%s.json' % index))
            if record:
                Path(item.verification_result).write_text(json.dumps(record))
            self.assertFalse(pr.finish_attempt(item))
            self.assertEqual(item.rc, 125)
            self.assertEqual(item.total_seconds, 5)
            self.assertEqual(len(item.attempts), 1)
            self.assertIsNone(pr.attempt_cpu(item.attempts))
            self.assertNotEqual(item.state, 'passed')

    def test_ordinary_cancellation_stays_distinct_from_pressure_and_infrastructure(self):
        item = self.item('cancelled', 'pass')
        item.started, item.ended, item.state, item.rc = 2, 7, 'cancelled', 130
        item.verification_result = str(self.root / 'cancelled.json')
        Path(item.verification_result).write_text(json.dumps({'status': 'incomplete', 'cleanup': 'complete',
                                                             'reaped_cpu_seconds': 1}))
        self.assertFalse(pr.finish_attempt(item))
        self.assertEqual((item.state, item.rc, item.total_seconds), ('cancelled', 130, 5))

    def test_exhausted_resource_policy_refuses_before_waiting_and_keeps_independent_work(self):
        item = self.item('exhausted', "raise AssertionError('must not execute')")
        other = self.item('independent', 'pass')
        def reserve(candidate, *args):
            if candidate is item:
                raise pr.cpu_guard.RecoveryExhausted('resource-envelope-exceeded; exclusive recovery exhausted')
        with patch.object(pr, 'reserve_item', side_effect=reserve):
            self.run_items([item, other])
        self.assertEqual((item.state, item.rc, other.state), ('resource-recovery-exhausted', 75, 'passed'))
        self.assertIsNone(item.started)
        self.assertLess(item.admission_wait, self.args.admission_wait)

    def test_cancellation_label_without_a_durable_budget_event_cannot_loop(self):
        item = self.item('unverified containment', 'pass')
        item.started, item.ended, item.state, item.rc = 2, 7, 'failed', 143
        item.verification_result = str(self.root / 'unverified.json')
        Path(item.verification_result).write_text(json.dumps({'status': 'contained', 'cleanup': 'complete',
                                                             'input_key': 'a' * 64, 'budget_used': 1}))
        with patch.object(pr.cpu_guard, 'verification_recovery', return_value={'containment': [], 'blocked': None}):
            self.assertFalse(pr.finish_attempt(item))
        self.assertEqual((item.state, item.rc), ('infrastructure-failed', 125))
        self.assertIn('no matching durable budget event', item.notes[-1])

    def test_integration_priority_survives_worker_permit_until_resource_admission(self):
        machine = str(self.root / 'machine')
        with patch.object(pr.engine_pass, 'is_main_checkout', return_value=False):
            background = pr.engine_pass.Admission(machine, str(self.root))
        self.addCleanup(background.close)
        observed = []
        def reserve(candidate, *args):
            admitted = background.begin()
            observed.append(admitted)
            if admitted:
                background.attempted(False)
            if len(observed) < 3:
                raise BlockingIOError('controlled demand wait after worker admission')
        item = self.item('integration waiting for demand', 'pass')
        with patch.object(pr.engine_pass, 'is_main_checkout', return_value=True), \
                patch.object(pr, 'reserve_item', side_effect=reserve):
            self.run_items([item])
        self.assertEqual(item.state, 'passed')
        self.assertEqual(observed, [False, False, False])
        self.assertTrue(background.begin())

    def test_cost_comparison_survives_source_change_but_not_changed_tools(self):
        self.args.managed_verification = True
        item = self.item('measured fixture', 'pass')
        inputs = {'paths': {'source': 'first'}, 'tools': {'python': 'first-tool'},
                  'profile': 'fixture', 'environment': {}, 'external': {}, 'platform': ['fixture']}
        # A Record carries its run's source identity; reserve_item() records it beside the
        # inputs (input_evidence, since bfc8c24d), so the stand-in carries one too.
        item.evidence = SimpleNamespace(identities={item.label: inputs},
            source={'commit': 'a' * 40, 'tracked_diff_sha256': 'clean', 'untracked_sha256': 'clean'})
        contexts = []
        fd = os.open(os.devnull, os.O_RDONLY)
        self.addCleanup(os.close, fd)
        def reserve(context, *args):
            contexts.append(json.loads(json.dumps(context)))
            return fd, {}, str(self.root / 'reservation.json')
        with patch.object(pr.cpu_guard, 'reserve_verification', side_effect=reserve), \
                patch.object(pr.cpu_guard, 'previous_verification_cost', return_value=None):
            pr.reserve_item(item, 1, self.args, str(self.root))
            inputs['paths']['source'] = 'changed-source'
            pr.reserve_item(item, 2, self.args, str(self.root))
            inputs['tools']['python'] = 'changed-tool'
            pr.reserve_item(item, 3, self.args, str(self.root))
        self.assertNotEqual(contexts[0]['input_key'], contexts[1]['input_key'])
        self.assertEqual(contexts[0]['cost_comparison']['key'], contexts[1]['cost_comparison']['key'])
        self.assertNotEqual(contexts[1]['cost_comparison']['key'], contexts[2]['cost_comparison']['key'])


if __name__ == "__main__":
    unittest.main()

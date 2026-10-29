#!/usr/bin/env python3
"""Guest memory admission, offline: no guest, no tart, no `top`.

A VM boot proof that waits an hour for a guest slot and ends "a guest needs N MB: M MB available
... (floor F MB)" is a memory shortfall nobody was told about, with every other proof run admitted
beside it. Every figure below is a synthetic machine, never a real one: 32768 MB of memory, a 5120 MB
guest and the 4096 MB default floor.

  1. the floor is the machine's own setting (limits.env), the environment wins, a missing
     setting is the stated default 4096, a malformed one is an error
  2. a refusal names a leaking kernel zone (zprint LEAK_ZONE) when it holds a large
     share of memory, read from zprint's output, before the apps
  3. `guest_fit` tells a shortfall running guests will clear from one apps or the kernel hold
  4. while a nightly's VM phase waits for its slot, reserve.py refuses every other admission
     (its sample, cpu_admission, its own command line) and says why; the waiter itself, a
     record whose process is gone and a stale record are not held
  5. slots.guest_slot writes that record only for a nightly's wait, only while it waits

Every record and setting lives in a temporary directory, never the operator's.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
WAITS = tempfile.mkdtemp(prefix='guest-memory-test-waits.')
os.environ['RICHOS_WAITS_DIR'] = WAITS
import reserve  # noqa: E402
import slots  # noqa: E402
import machine_settings  # noqa: E402

REAL_RUN = subprocess.run
# `zprint ZONE` without sudo prints the sizes as 0K but not the elements in use: the zone's size is
# those elements times the element size. 4,194,304 x 1024 bytes is 4.0 GB, 12.5% of the machine below.
ZPRINT_HEAD = ('                            elem         cur         max        cur         max         cur  alloc  alloc    \n'
               'zone name                   size        size        size      #elts       #elts       inuse   size  count    \n'
               '-------------------------------------------------------------------------------------------------------------\n')
LEAKING = ZPRINT_HEAD + 'data.kalloc.1024            1024          0K          0K          0           0     4194304     0K      0   \n'
HEALTHY = ZPRINT_HEAD + 'data.kalloc.1024            1024          0K          0K          0           0      300000     0K      0   \n'
TOTAL_MB = 32768            # the synthetic machine
GUEST_MB = 5120
# 8800 MB available: the guest would leave 3680 MB, 416 MB under the 4096 MB default floor and 680 MB
# over a floor setting of 3000 MB.
JUST_SHORT = {'memory_free_percent': 8800 / TOTAL_MB * 100}
APPS = [('/Applications/AppOne.app/Contents/MacOS/AppOne', 2300), ('/Applications/AppTwo.app/Contents/MacOS/AppTwo', 800)]


def zprint_says(text):
    """subprocess.run, with `zprint` answering `text` and everything else refused."""
    def run(cmd, *a, **k):
        if Path(cmd[0]).name == 'zprint':
            return subprocess.CompletedProcess(cmd, 0, text, '')
        raise AssertionError(f'unexpected command in an offline test: {cmd}')
    reserve._ZONE_CACHE.clear()
    return patch('subprocess.run', side_effect=run)


class Isolated(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.root = self.dir / 'testvm'
        self.limits = self.dir / 'limits.env'
        self.env = patch.dict(os.environ, {'TESTVM_ROOT': str(self.root), 'RICHOS_LIMITS_FILE': str(self.limits)})
        self.env.start()
        for name in ('TESTVM_MEMORY_FLOOR_MB', 'TESTVM_NIGHTLY_WAIT', 'TESTVM_SLOTS', 'RICHOS_AGENT_OWNER'):
            os.environ.pop(name, None)
        reserve._ZONE_CACHE.clear()

    def tearDown(self):
        self.env.stop()
        self.tmp.cleanup()
        reserve._ZONE_CACHE.clear()


class Floor(Isolated):
    def test_a_machine_whose_floor_setting_is_lower_than_the_default_admits_a_guest_the_default_refuses(self):
        self.limits.write_text('# a floor lower than the default\nTESTVM_MEMORY_FLOOR_MB=3000\n')
        floor, where = slots.guest_memory_floor()
        self.assertEqual(floor, 3000)
        self.assertIn(f'machine setting TESTVM_MEMORY_FLOOR_MB in {self.limits}', where)
        fits = slots.memory_refusal(JUST_SHORT, [], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB, processes=[],
                                    zone_text='')
        self.assertEqual(fits, '', 'the guest leaves 3680 MB: over a 3000 MB floor')
        short = slots.memory_refusal({'memory_free_percent': 20}, [], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB,
                                     processes=[], zone_text='')
        self.assertIn('(floor 3000 MB, machine setting TESTVM_MEMORY_FLOOR_MB', short)

    def test_a_missing_setting_falls_back_to_the_stated_default_and_says_so(self):
        # Second review, P3: the no-file case once dropped RICHOS_LIMITS_FILE for the cases after
        # it, which then read the machine's own limits file. Each case states its whole input.
        empty_home = self.dir / 'home'
        empty_home.mkdir()
        cases = (('', {'RICHOS_LIMITS_FILE': str(self.limits)}),
                 ('# nothing set\n', {'RICHOS_LIMITS_FILE': str(self.limits)}),
                 (None, {'HOME': str(empty_home)}))
        for text, env in cases:
            if text is not None:
                self.limits.write_text(text)
            with patch.dict(os.environ, env):
                if text is None:
                    os.environ.pop('RICHOS_LIMITS_FILE', None)    # restored by patch.dict on exit
                floor, where = slots.guest_memory_floor()
            self.assertEqual(floor, slots.GUEST_MEMORY_FLOOR_MB, (text, where))
            self.assertEqual(floor, 4096)
            self.assertTrue(where.startswith('default; no TESTVM_MEMORY_FLOOR_MB in '), where)
            if text is None:
                self.assertIn(str(empty_home / 'Library/Application Support/RichOS/limits.env'), where)
        self.assertEqual(os.environ.get('RICHOS_LIMITS_FILE'), str(self.limits))
        short = slots.memory_refusal(JUST_SHORT, [], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB, processes=[],
                                     zone_text='')
        self.assertIn('would leave 3680 MB (floor 4096 MB, default; no TESTVM_MEMORY_FLOOR_MB', short)

    def test_the_environment_wins_and_a_malformed_value_is_an_error_never_a_silent_default(self):
        self.limits.write_text('TESTVM_MEMORY_FLOOR_MB=3000\n')
        with patch.dict(os.environ, {'TESTVM_MEMORY_FLOOR_MB': '3500'}):
            self.assertEqual(slots.guest_memory_floor(), (3500, 'environment TESTVM_MEMORY_FLOOR_MB'))
        for bad in ('TESTVM_MEMORY_FLOOR_MB=2 GB\n', 'TESTVM_MEMORY_FLOOR_MB=-1\n', 'TESTVM_MEMORY_FLOOR_MB=10\n',
                    'TESTVM_MEMORY_FLOOR_MB=3000\nTESTVM_MEMORY_FLOOR_MB=4096\n', 'RICHOS_BULK_ROOT=/x\n'):
            self.limits.write_text(bad)
            with self.assertRaises(ValueError, msg=bad):
                slots.guest_memory_floor()
        self.limits.unlink()
        with self.assertRaisesRegex(ValueError, 'explicit limits file does not exist'):
            slots.guest_memory_floor()

    def test_the_setting_is_never_in_env_where_every_older_engine_would_refuse_it(self):
        with self.assertRaisesRegex(ValueError, 'unknown key TESTVM_MEMORY_FLOOR_MB'):
            machine_settings.parse('TESTVM_MEMORY_FLOOR_MB=3000\n')
        self.assertNotIn('TESTVM_MEMORY_FLOOR_MB', machine_settings.KEYS)


class KernelLeak(Isolated):
    def test_a_large_leaked_zone_is_named_before_the_apps(self):
        with zprint_says(LEAKING):
            text = slots.memory_refusal(JUST_SHORT, [], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB, floor_mb=4096,
                                        processes=APPS)
        self.assertIn('416 MB more must be free; kernel leak data.kalloc.1024 holds 4.0 GB; only a restart '
                      'returns it; largest apps by memory: AppOne 2300 MB, AppTwo 800 MB', text)

    def test_a_small_zone_is_not_blamed(self):
        with zprint_says(HEALTHY):
            text = slots.memory_refusal(JUST_SHORT, [], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB, floor_mb=4096,
                                        processes=APPS)
        self.assertNotIn('kernel leak', text)
        self.assertIn('largest apps by memory: AppOne 2300 MB', text)
        self.assertEqual(reserve.zone_in_use_mb('zprint: no such zone\n'), None)
        self.assertEqual(reserve.kernel_leak_note(TOTAL_MB, ''), '')

    def test_reserve_s_own_memory_refusal_names_the_leak_too(self):
        total = slots.host_memory_mb()
        leaked = ZPRINT_HEAD + f'data.kalloc.1024 1024 0K 0K 0 0 {int(total * 0.4 * 1024)} 0K 0\n'
        sample = {'cpu_user_percent': 5.0, 'cpu_system_percent': 2.0, 'cpu_idle_percent': 93.0,
                  'swapout_mb_per_s': 0.0, 'memory_pressure': 'critical', 'memory_free_percent': 4, 'swap_used_mb': 9.0}
        with zprint_says(leaked):
            why = reserve._refusal(sample, 80, 16)
        self.assertTrue(why.startswith('kernel memory pressure is CRITICAL; kernel leak data.kalloc.1024 holds '), why)
        self.assertIn('only a restart returns it', why)
        with zprint_says(HEALTHY):
            self.assertEqual(reserve._refusal(sample, 80, 16), 'kernel memory pressure is CRITICAL')
        calm = dict(sample, memory_pressure='normal')
        with patch('subprocess.run', side_effect=AssertionError('zprint is read only on a memory refusal')):
            self.assertEqual(reserve._refusal(calm, 80, 16), '')


class Fit(Isolated):
    def test_what_processes_could_free_is_waited_for_and_only_the_kernel_s_share_is_not(self):
        # 416 MB short while two apps alone hold 3100 MB. Second review, P2:
        # memory processes hold (apps, other agents' builds) is worth the slot wait, and says so.
        with zprint_says(LEAKING):
            apps = slots.guest_fit(JUST_SHORT, running=[], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB, floor_mb=4096,
                                   processes=APPS)
        self.assertEqual((apps['fits'], apps['waitable'], apps['needed_mb'], apps['freeable_mb']),
                         (False, True, 416, 3100))
        self.assertIn('kernel leak data.kalloc.1024 holds 4.0 GB', apps['reason'])
        # 25% of 32768 MB is 8192 MB: a 5120 MB guest leaves 3072 MB, 1024 MB short of a 4096 floor, and
        # every process together holds 600 MB: no wait frees it, the kernel holds the rest.
        stuck = slots.guest_fit({'memory_free_percent': 25}, running=[], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB,
                                floor_mb=4096, processes=[('/bin/x', 600)], zone_text='')
        self.assertEqual((stuck['fits'], stuck['waitable'], stuck['needed_mb'], stuck['freeable_mb']),
                         (False, False, 1024, 600))
        # One guest running and settled at 5000 MB: it is owed 120 MB more and gives back 5000 when it ends.
        busy = slots.guest_fit({'memory_free_percent': 25}, running=['walk-a'], resident=[5000], total_mb=TOTAL_MB,
                               ram_mb=GUEST_MB, floor_mb=3000, processes=[], zone_text='')
        self.assertEqual((busy['fits'], busy['waitable'], busy['running'], busy['freeable_mb']),
                         (False, True, ['walk-a'], 5120))
        free = slots.guest_fit({'memory_free_percent': 60}, running=[], resident=[], total_mb=TOTAL_MB, ram_mb=GUEST_MB,
                               floor_mb=3000, processes=[], zone_text='')
        self.assertEqual((free['fits'], free['reason'], free['waitable']), (True, '', False))


def idle_sample():
    """reserve.host_sample with the CPU counters fixed at 10% busy and no interval slept."""
    with patch('reserve._counters', side_effect=[([0, 0, 100, 0], 0), ([10, 0, 190, 0], 0)]), \
            patch('reserve.time.sleep'):
        return reserve.host_sample()


class NightlyGoesFirst(Isolated):
    def record(self, pid, since=None, phase='vm-boot-proof (run R80)'):
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / reserve.NIGHTLY_GUEST_WAIT).write_text(json.dumps(
            {'pid': pid, 'since': time.time() if since is None else since, 'phase': phase}))

    def test_while_a_nightly_waits_every_other_admission_is_refused_and_says_why(self):
        self.assertNotIn('held_for_nightly', idle_sample())
        self.record(os.getppid())                 # another live process: the nightly's waiter
        s = idle_sample()
        self.assertIn("a nightly's vm-boot-proof (run R80) has waited 0 min for a test-VM guest slot", s['held_for_nightly'])
        self.assertIn('new test and proof admissions are held', reserve._refusal(s, 80, 16))
        self.assertTrue(reserve.describe(s).startswith("HELD: a nightly's vm-boot-proof"))
        with patch('reserve.host_sample', return_value=s):
            with self.assertRaisesRegex(BlockingIOError, "a nightly's vm-boot-proof .* held"):
                reserve.cpu_admission()

    def test_the_waiter_itself_a_gone_process_and_a_stale_record_hold_nothing(self):
        self.record(os.getpid())
        self.assertNotIn('held_for_nightly', idle_sample(), 'the nightly is never held by its own wait')
        gone = subprocess.Popen(['/usr/bin/true'])
        gone.wait()
        self.record(gone.pid)
        self.assertNotIn('held_for_nightly', idle_sample())
        self.record(os.getppid(), since=time.time() - reserve.NIGHTLY_GUEST_WAIT_STALE - 1)
        self.assertNotIn('held_for_nightly', idle_sample())
        (self.root / reserve.NIGHTLY_GUEST_WAIT).write_text('{not json')
        self.assertNotIn('held_for_nightly', idle_sample())

    def test_reserve_py_s_own_command_line_refuses_a_test_while_the_nightly_waits(self):
        self.record(os.getpid())                  # this test process is the waiting nightly here
        env = dict(os.environ)
        done = REAL_RUN([sys.executable, str(HERE / 'reserve.py'), '--', '/usr/bin/true'], env=env,
                        capture_output=True, text=True, timeout=60)
        self.assertEqual(done.returncode, 75, done.stderr)
        self.assertIn("a nightly's vm-boot-proof (run R80) has waited", done.stderr)
        self.assertIn('new test and proof admissions are held', done.stderr)
        (self.root / reserve.NIGHTLY_GUEST_WAIT).unlink()
        done = REAL_RUN([sys.executable, str(HERE / 'reserve.py'), '--', '/usr/bin/true'], env=env,
                        capture_output=True, text=True, timeout=60)
        self.assertNotIn('nightly', done.stderr)

    def test_native_builds_and_simulator_boots_held_by_a_nightly_say_why(self):
        """Second review, P3: native-work and the simulator boot budget refuse on the same sample
        and printed only 'waiting for headroom'."""
        import contextlib
        import importlib.util
        import io
        held = {'cpu_user_percent': 5.0, 'cpu_system_percent': 2.0, 'cpu_idle_percent': 93.0,
                'swapout_mb_per_s': 0.0, 'memory_pressure': 'normal', 'memory_free_percent': 60,
                'swap_used_mb': 0.0, 'held_for_nightly': "a nightly's vm-boot-proof (run R80) has waited 3 min"}
        calm = {k: v for k, v in held.items() if k != 'held_for_nightly'}
        lib = HERE.parents[2] / 'engine/scripts/lib'
        sys.path.insert(0, str(lib))
        spec = importlib.util.spec_from_file_location('native_work_under_test', lib / 'native-work.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        samples = iter([held, calm])
        err = io.StringIO()
        with patch.object(reserve, 'host_sample', lambda: next(samples)), patch.object(native.time, 'sleep'), \
                contextlib.redirect_stderr(err):
            native.wait_for_headroom(reserve)
        self.assertIn("native-work: held: a nightly's vm-boot-proof (run R80) has waited 3 min", err.getvalue())

        sys.path.insert(0, str(HERE.parent / 'lib'))
        import simulator_budget
        import cpu_guard

        class Token:
            def release(self):
                pass

        class Budget:
            def __init__(self, *a, **k):
                pass

            def try_acquire(self):
                return Token()
        samples = iter([held, calm])
        err = io.StringIO()
        with patch.object(cpu_guard, 'require_managed_ancestor'), \
                patch.object(simulator_budget.worker_tokens, 'init'), \
                patch.object(simulator_budget.worker_tokens, 'Budget', Budget), \
                patch.object(simulator_budget.worker_tokens, 'machine_directory', return_value=str(self.dir / 'm')), \
                patch.object(simulator_budget.time, 'sleep'), contextlib.redirect_stderr(err):
            self.assertIsInstance(simulator_budget.acquire('boot', sampler=lambda: next(samples),
                                                           inventory=lambda: 0), Token)
        self.assertIn("simulator-budget: boot held: a nightly's vm-boot-proof (run R80)", err.getvalue())

    def hold(self, **kw):
        kw.setdefault('guests', lambda: [])
        kw.setdefault('memory', lambda sample, running: '')
        return slots.guest_slot(root=self.root, **kw)

    def test_a_nightly_s_slot_wait_records_itself_only_while_it_waits(self):
        seen = []
        refusals = iter([BlockingIOError('total CPU is 95.0% (limit 80%)')])

        def admit():
            refused = next(refusals, None)
            if refused:
                raise refused
            return {}

        def sleep(seconds):
            seen.append((reserve.nightly_guest_wait(self.root), reserve.nightly_hold(self.root)))
        now = iter(range(0, 1000))
        with patch.dict(os.environ, {'TESTVM_NIGHTLY_WAIT': 'new-user-walk (run R81)'}), \
                patch('sys.stderr'), \
                self.hold(admit=admit, wait_seconds=120, sleep=sleep, clock=lambda: next(now)):
            self.assertIsNone(reserve.nightly_guest_wait(self.root), 'admitted: nothing is held any longer')
        self.assertEqual(len(seen), 1)
        waiting, own_hold = seen[0]
        self.assertEqual((waiting['pid'], waiting['phase']), (os.getpid(), 'new-user-walk (run R81)'))
        self.assertEqual(own_hold, '', 'the waiter is not held by its own record')
        self.assertFalse((self.root / reserve.NIGHTLY_GUEST_WAIT).exists())

    def test_a_refused_nightly_wait_clears_its_record_and_an_ordinary_wait_writes_none(self):
        def refuse():
            raise BlockingIOError('total CPU is 95.0% (limit 80%)')
        for nightly in ('vm-boot-proof (run R81)', None):
            seen = []
            now = iter(range(0, 1000, 40))
            env = {'TESTVM_NIGHTLY_WAIT': nightly} if nightly else {}
            with patch.dict(os.environ, env), patch('sys.stderr'), \
                    self.assertRaisesRegex(BlockingIOError, 'guest slot refused'):
                with self.hold(admit=refuse, wait_seconds=60, clock=lambda: next(now),
                               sleep=lambda s: seen.append(reserve.nightly_guest_wait(self.root))):
                    self.fail('admitted')
            self.assertEqual(bool(seen and seen[0]), bool(nightly), nightly)
            self.assertFalse((self.root / reserve.NIGHTLY_GUEST_WAIT).exists())


if __name__ == '__main__':
    unittest.main()

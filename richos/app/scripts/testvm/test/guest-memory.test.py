#!/usr/bin/env python3
"""Guest memory after nightly 80 (2026-10-29), offline: no guest, no tart, no `top`.

Nightly 80's VM boot proof waited an hour and ended "a guest needs 5120 MB: 8847 MB available
... would leave 3727 MB (floor 4096 MB)", with every worker's proof run admitted beside it.

  1. the floor is this Mac's machine setting (limits.env), the environment wins, a missing
     setting is the stated default 4096, a malformed one is an error
  2. a refusal names the kernel's own leak (zprint data.kalloc.1024) when it holds a large
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
# `zprint data.kalloc.1024` as this Mac printed it without sudo on 2026-10-29: sizes read 0K, the
# elements in use do not. 9,733,229 x 1024 bytes is 9.3 GB.
ZPRINT_HEAD = ('                            elem         cur         max        cur         max         cur  alloc  alloc    \n'
               'zone name                   size        size        size      #elts       #elts       inuse   size  count    \n'
               '-------------------------------------------------------------------------------------------------------------\n')
LEAKING = ZPRINT_HEAD + 'data.kalloc.1024            1024          0K          0K          0           0     9733229     0K      0   \n'
HEALTHY = ZPRINT_HEAD + 'data.kalloc.1024            1024          0K          0K          0           0      300000     0K      0   \n'
NIGHTLY_80 = {'memory_free_percent': 8847 / 24576 * 100}
APPS = [('/Applications/ChatGPT.app/Contents/MacOS/ChatGPT', 2338), ('/Applications/RichOS.app/Contents/MacOS/RichOS', 800)]


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
    def test_the_floor_is_this_macs_setting_and_nightly_80s_guest_fits_under_it(self):
        self.limits.write_text('# this Mac, 2026-10-29\nTESTVM_MEMORY_FLOOR_MB=2048\n')
        floor, where = slots.guest_memory_floor()
        self.assertEqual(floor, 2048)
        self.assertIn(f'machine setting TESTVM_MEMORY_FLOOR_MB in {self.limits}', where)
        fits = slots.memory_refusal(NIGHTLY_80, [], resident=[], total_mb=24576, ram_mb=5120, processes=[],
                                    zone_text='')
        self.assertEqual(fits, '', 'nightly 80 left 3727 MB: over a 2048 MB floor')
        short = slots.memory_refusal({'memory_free_percent': 25}, [], resident=[], total_mb=24576, ram_mb=5120,
                                     processes=[], zone_text='')
        self.assertIn('(floor 2048 MB, machine setting TESTVM_MEMORY_FLOOR_MB', short)

    def test_a_missing_setting_falls_back_to_the_stated_default_and_says_so(self):
        for text in (None, '', '# nothing set\n'):
            if text is None:
                self.limits.unlink(missing_ok=True)
                os.environ.pop('RICHOS_LIMITS_FILE')
                with patch.dict(os.environ, {'HOME': str(self.dir)}):
                    floor, where = slots.guest_memory_floor()
            else:
                self.limits.write_text(text)
                floor, where = slots.guest_memory_floor()
            self.assertEqual(floor, slots.GUEST_MEMORY_FLOOR_MB)
            self.assertEqual(floor, 4096)
            self.assertTrue(where.startswith('default; no TESTVM_MEMORY_FLOOR_MB in '), where)
        short = slots.memory_refusal(NIGHTLY_80, [], resident=[], total_mb=24576, ram_mb=5120, processes=[],
                                     zone_text='')
        self.assertIn('would leave 3727 MB (floor 4096 MB, default; no TESTVM_MEMORY_FLOOR_MB', short)

    def test_the_environment_wins_and_a_malformed_value_is_an_error_never_a_silent_default(self):
        self.limits.write_text('TESTVM_MEMORY_FLOOR_MB=2048\n')
        with patch.dict(os.environ, {'TESTVM_MEMORY_FLOOR_MB': '3072'}):
            self.assertEqual(slots.guest_memory_floor(), (3072, 'environment TESTVM_MEMORY_FLOOR_MB'))
        for bad in ('TESTVM_MEMORY_FLOOR_MB=2 GB\n', 'TESTVM_MEMORY_FLOOR_MB=-1\n', 'TESTVM_MEMORY_FLOOR_MB=10\n',
                    'TESTVM_MEMORY_FLOOR_MB=2048\nTESTVM_MEMORY_FLOOR_MB=4096\n', 'RICHOS_BULK_ROOT=/x\n'):
            self.limits.write_text(bad)
            with self.assertRaises(ValueError, msg=bad):
                slots.guest_memory_floor()
        self.limits.unlink()
        with self.assertRaisesRegex(ValueError, 'explicit limits file does not exist'):
            slots.guest_memory_floor()

    def test_the_setting_is_never_in_env_where_every_older_engine_would_refuse_it(self):
        with self.assertRaisesRegex(ValueError, 'unknown key TESTVM_MEMORY_FLOOR_MB'):
            machine_settings.parse('TESTVM_MEMORY_FLOOR_MB=2048\n')
        self.assertNotIn('TESTVM_MEMORY_FLOOR_MB', machine_settings.KEYS)


class KernelLeak(Isolated):
    def test_a_large_leaked_zone_is_named_before_the_apps(self):
        with zprint_says(LEAKING):
            text = slots.memory_refusal(NIGHTLY_80, [], resident=[], total_mb=24576, ram_mb=5120, floor_mb=4096,
                                        processes=APPS)
        self.assertIn('369 MB more must be free; kernel leak data.kalloc.1024 holds 9.3 GB; only a restart '
                      'returns it; largest apps by memory: ChatGPT 2338 MB, RichOS 800 MB', text)

    def test_a_small_zone_is_not_blamed(self):
        with zprint_says(HEALTHY):
            text = slots.memory_refusal(NIGHTLY_80, [], resident=[], total_mb=24576, ram_mb=5120, floor_mb=4096,
                                        processes=APPS)
        self.assertNotIn('kernel leak', text)
        self.assertIn('largest apps by memory: ChatGPT 2338 MB', text)
        self.assertEqual(reserve.zone_in_use_mb('zprint: no such zone\n'), None)
        self.assertEqual(reserve.kernel_leak_note(24576, ''), '')

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
    def test_apps_and_the_kernel_are_not_waited_for_running_guests_are(self):
        with zprint_says(LEAKING):
            held = slots.guest_fit(NIGHTLY_80, running=[], resident=[], total_mb=24576, ram_mb=5120, floor_mb=4096,
                                   processes=APPS)
        self.assertEqual((held['fits'], held['waitable']), (False, False))
        self.assertIn('kernel leak data.kalloc.1024 holds 9.3 GB', held['reason'])
        # One guest running and settled at 5000 MB: once it ends, its memory comes back and a guest fits.
        # 25% of 24 GB is 6144 MB: less 120 MB still owed and 5120 MB leaves 904 MB; without walk-a, 6024 MB.
        busy = slots.guest_fit({'memory_free_percent': 25}, running=['walk-a'], resident=[5000], total_mb=24576,
                               ram_mb=5120, floor_mb=2048, processes=[], zone_text='')
        self.assertEqual((busy['fits'], busy['waitable'], busy['running']), (False, True, ['walk-a']))
        free = slots.guest_fit({'memory_free_percent': 60}, running=[], resident=[], total_mb=24576, ram_mb=5120,
                               floor_mb=2048, processes=[], zone_text='')
        self.assertEqual(free, {'fits': True, 'reason': '', 'waitable': False, 'running': []})


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

#!/usr/bin/env python3
"""slots.py: two guest slots, each held only while one run executes, offline.

No guest and no CPU sample: the running-guest count and the CPU/memory admission are
replaced, in process and in every child process driven here, so the suite answers the same
on an idle Mac and a busy one. Every slot file lives under a temporary TESTVM_ROOT."""
import atexit
import contextlib
import fcntl
import io
import json
import os
import re
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
# A caller that waits records its wait (the engine's resource_waits.py); a test never writes one
# where the operator's turn-end gate reads. Children inherit it.
_WAITS = tempfile.mkdtemp(prefix='slots-test-waits.')
os.environ['RICHOS_WAITS_DIR'] = _WAITS
atexit.register(shutil.rmtree, _WAITS, True)
import slots  # noqa: E402

# A child process that runs slots.main() with the guest count and admission replaced.
# GUESTS is a JSON list of running guest names; ADMIT=refuse makes every sample refuse.
DRIVER = textwrap.dedent(f"""
    import json, os, sys
    sys.path.insert(0, {str(HERE)!r})
    import slots
    slots.running_guests = lambda: json.loads(os.environ.get('FAKE_GUESTS', '[]'))
    def admit(*a, **k):
        if os.environ.get('FAKE_ADMIT') == 'refuse':
            raise BlockingIOError('total CPU is 95.0% (limit 80%)')
        return {{}}
    slots.reserve.cpu_admission = admit
    slots.memory_refusal = lambda *a, **k: os.environ.get('FAKE_MEMORY', '')
    sys.argv = ['slots.py'] + sys.argv[1:]
    try:
        sys.exit(slots.main())
    except BlockingIOError as exc:
        print(str(exc), file=sys.stderr); sys.exit(75)
    except ValueError as exc:
        print('slots.py: ' + str(exc), file=sys.stderr); sys.exit(2)
""")


def quiet():
    return contextlib.redirect_stderr(io.StringIO())


class Slots(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / 'testvm'
        # The machine's own limits file never decides a floor a test asserts (the default is used).
        limits = Path(self.tmp.name) / 'limits.env'
        limits.write_text('')
        self.env = patch.dict(os.environ, {'TESTVM_ROOT': str(self.root), 'RICHOS_LIMITS_FILE': str(limits)})
        self.env.start()
        os.environ.pop('TESTVM_SLOTS', None)
        os.environ.pop('TESTVM_MEMORY_FLOOR_MB', None)
        os.environ.pop('TESTVM_NIGHTLY_WAIT', None)
        os.environ.pop(slots.ENV, None)
        os.environ.pop('RICHOS_AGENT_OWNER', None)  # these tests wait on runs of one owner (this agent's)

    def tearDown(self):
        self.env.stop()
        self.tmp.cleanup()

    def hold(self, **kw):
        kw.setdefault('guests', lambda: [])
        kw.setdefault('admit', lambda: {})
        kw.setdefault('memory', lambda sample, running: '')
        return slots.guest_slot(root=self.root, **kw)

    @staticmethod
    def finish(proc, timeout=30):
        out, err = proc.communicate(timeout=timeout)
        return proc.returncode, out, err

    def child(self, *args, env=None, **kw):
        e = dict(os.environ, TESTVM_ROOT=str(self.root), **(env or {}))
        return subprocess.Popen([sys.executable, '-c', DRIVER, *args], env=e, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)

    # --- two at once, the third waits or refuses ------------------------------------
    def test_two_runs_hold_two_slots_at_once_and_a_third_is_refused_naming_both(self):
        with quiet(), self.hold(purpose='walk A') as a, self.hold(purpose='walk B') as b:
            self.assertEqual({a.name, b.name}, {'guest.lock', 'guest-2.lock'})
            with self.assertRaisesRegex(BlockingIOError, 'every slot is executing a run') as caught:
                with self.hold():
                    self.fail('a third run was admitted')
            self.assertIn('walk A', str(caught.exception))
            self.assertIn('walk B', str(caught.exception))
        with quiet(), self.hold() as again:
            self.assertEqual(again.name, 'guest.lock')

    def test_slot_one_is_the_old_guest_lock_so_an_older_checkout_still_counts(self):
        old = self.root / 'guest.lock'
        self.root.mkdir(parents=True)
        with old.open('a') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)      # what reserve.reservation(lock=...) does
            with quiet(), self.hold() as mine:
                self.assertEqual(mine.name, 'guest-2.lock')
                with self.assertRaisesRegex(BlockingIOError, 'older checkout'):
                    with self.hold():
                        self.fail('admitted beside an older checkout and another run')

    def test_the_running_guest_count_refuses_even_with_a_slot_free(self):
        with quiet(), self.assertRaisesRegex(BlockingIOError, r'2 guest\(s\) already running \(walk-a, gui\)'):
            with self.hold(guests=lambda: ['walk-a', 'gui']):
                self.fail('a third guest was admitted')
        with quiet(), self.hold(guests=lambda: ['walk-a']):
            pass

    def test_a_refused_cpu_sample_releases_the_slot_before_waiting(self):
        seen = []
        calls = {'n': 0}

        def admit():
            calls['n'] += 1
            if calls['n'] == 1:
                raise BlockingIOError('total CPU is 95.0% (limit 80%)')
            return {}

        def sleep(seconds):
            seen.append((seconds, [slots.is_held(p) for p in slots.slot_paths(self.root)]))
        now = iter(range(0, 1000, 1))
        with quiet(), self.hold(admit=admit, wait_seconds=120, sleep=sleep, clock=lambda: next(now)):
            pass
        self.assertEqual(len(seen), 1)
        self.assertEqual(seen[0][0], 30, 'CPU samples stay at least 30 s apart (reserve.py)')
        self.assertEqual(seen[0][1], [False, False], 'no slot is held while this caller waits for the CPU')

    def test_a_guest_is_admitted_only_if_its_whole_ram_fits_beside_the_running_guests(self):
        """Two guests admitted back to back must not both pass on the same free-memory reading: a
        guest booting takes its RAM only gradually, so each running guest is still owed the rest
        of its RAM, and the new guest's whole RAM must fit beside that plus the floor."""
        total, ram = 16384, 5120
        fits = slots.memory_refusal({'memory_free_percent': 71}, [], resident=[], total_mb=total, ram_mb=ram)
        self.assertEqual(fits, '', 'one guest on a mostly-free Mac fits')
        second = slots.memory_refusal({'memory_free_percent': 71}, ['walk-a'], resident=[50], total_mb=total,
                                      ram_mb=ram)
        self.assertIn('still owed to 1 running guest', second)
        self.assertIn('floor 4096 MB', second)
        unseen = slots.memory_refusal({'memory_free_percent': 70}, ['walk-a'], resident=[], total_mb=total,
                                      ram_mb=ram)
        self.assertIn('5120 MB still owed', unseen, 'a guest with no visible process is owed all of its RAM')
        settled = slots.memory_refusal({'memory_free_percent': 60}, ['walk-a'], resident=[5000], total_mb=total,
                                       ram_mb=ram)
        self.assertEqual(settled, '', 'a settled guest already counted in the available memory owes little')

    def test_a_refusal_says_how_much_is_missing_and_which_apps_hold_the_most(self):
        chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
        helper = ('/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/'
                  'Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper --type=renderer')
        vm = '/System/Library/Frameworks/Virtualization.framework/x/' + slots.VM_PROCESS
        processes = [(chrome, 4000), (helper, 5000), (helper, 4100),
                     ('/Applications/WebStorm.app/Contents/MacOS/webstorm', 5300),
                     ('/Applications/Slack.app/Contents/MacOS/Slack', 900), (vm, 9000), ('/bin/ls', 5)]
        self.assertEqual(slots.top_apps(processes), [('Google Chrome', 13100), ('WebStorm', 5300), ('Slack', 900)])
        text = slots.memory_refusal({'memory_free_percent': 40}, ['walk-a'], resident=[50], total_mb=16384,
                                    ram_mb=5120, processes=processes)
        left = 40 / 100 * 16384 - (5120 - 50) - 5120
        self.assertIn(f'{4096 - left:.0f} MB more must be free', text)
        self.assertIn('largest apps by memory: Google Chrome 13100 MB, WebStorm 5300 MB, Slack 900 MB', text)

    def test_only_a_test_guest_s_own_vm_process_counts_not_docker_s_listed_first(self):
        """Docker's VM can be listed before the test guest in `ps`. Only the VM process that holds
        the guest's own files open is the guest, so a guest is never charged Docker's (smaller)
        size, which would make every later guest look as if it fit or be refused wrongly."""
        vm = '/System/Library/Frameworks/Virtualization.framework/x/com.apple.Virtualization.VirtualMachine'
        ps = f'  1271  439296 {vm}\n 44996 3647488 {vm}\n   99    1000 /bin/zsh\n'
        lsof = ('p1271\nn/Users/a/Library/Containers/com.docker.docker/Docker.raw\n'
                'p44996\nn/Users/a/.richos-testvm/tart/vms/walk-x/nvram.bin\n'
                'n/Users/a/.richos-testvm/tart/vms/walk-x/disk.img\n')

        def run(cmd, **kw):
            out = ps if cmd[0] == 'ps' else lsof
            return subprocess.CompletedProcess(cmd, 0, stdout=out, stderr='')
        with patch.object(slots.subprocess, 'run', run):
            self.assertEqual(slots.vm_resident_mb(), [3647488 / 1024])

    def test_docker_s_vm_counts_under_docker_and_system_processes_are_never_named(self):
        """Review rv-20261014T105153Z-9721e4ac-8690: Docker's 8192 MB VM vanished from the list, and
        WindowServer displaced Slack."""
        vm = '/System/Library/Frameworks/Virtualization.framework/x/' + slots.VM_PROCESS
        shown = ' 1  8192M\n 2  5000M\n 3  3600M\n 4  2500M\n 5  900M\n 6  700M\n'
        ps = {'-o pid=,rss=,comm=': f'1 8388608 {vm}\n2 5120000 {vm}\n',
              '-o pid=,uid=,command=': (f'1 {os.getuid()} {vm}\n2 {os.getuid()} {vm}\n'
                                       f'3 {os.getuid()} /Applications/WebStorm.app/Contents/MacOS/webstorm\n'
                                       f'4 {os.getuid()} /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer\n'
                                       f'5 {os.getuid()} /Applications/Slack.app/Contents/MacOS/Slack\n'
                                       f'6 0 /Applications/Other.app/Contents/MacOS/Other\n')}
        lsof = ('p1\nn/Users/a/Library/Containers/com.docker.docker/Docker.raw\n'
                'p2\nn/Users/a/.richos-testvm/tart/vms/walk-x/disk.img\n')

        def run(cmd, **kw):
            if cmd[0] == 'top':
                out = shown
            elif cmd[0] == 'lsof':
                out = lsof
            else:
                out = ps[' '.join(cmd[2:4])]
            return subprocess.CompletedProcess(cmd, 0, stdout=out, stderr='')
        with patch.object(slots.subprocess, 'run', run):
            apps = slots.top_apps(slots.process_memory_mb())
        self.assertEqual(apps, [('Docker', 8192), ('WebStorm', 3600), ('Slack', 900)])

    def test_the_guest_size_is_lib_sh_s_one_declared_default_unless_overridden(self):
        declared = re.search(r'^TESTVM_RAM_MB="\$\{TESTVM_RAM_MB:-(\d+)\}"', (HERE / 'lib.sh').read_text(), re.M)
        os.environ.pop('TESTVM_RAM_MB', None)
        self.assertEqual(slots.guest_ram_mb(), int(declared.group(1)))
        with patch.dict(os.environ, {'TESTVM_RAM_MB': '4096'}):
            self.assertEqual(slots.guest_ram_mb(), 4096)

    def test_a_memory_refusal_releases_the_slot_and_waits_like_a_cpu_refusal(self):
        seen, answers = [], iter(['a guest needs 5120 MB: ...', ''])
        now = iter(range(0, 1000, 1))

        def sleep(seconds):
            seen.append((seconds, [slots.is_held(p) for p in slots.slot_paths(self.root)]))
        err = io.StringIO()
        with contextlib.redirect_stderr(err), self.hold(memory=lambda s, g: next(answers), wait_seconds=120,
                                                       sleep=sleep, clock=lambda: next(now)):
            pass
        self.assertEqual(seen, [(30, [False, False])])
        self.assertIn('slot waiting: a guest needs 5120 MB', err.getvalue())

    def test_a_dead_stderr_reader_cannot_stop_a_release_halfway(self):
        class Dead(io.StringIO):
            def write(self, text):
                raise BrokenPipeError(32, 'Broken pipe')
        calls = []

        def fake_run(args, **kw):
            calls.append((args[1:], kw.get('restore_signals')))
            return subprocess.CompletedProcess(args, 0)
        with patch('slots.subprocess.run', side_effect=fake_run), contextlib.redirect_stderr(Dead()):
            with self.hold() as slot:
                left = self.root / 'run' / 'walk-left'
                left.mkdir(parents=True)
                (left / 'slot').write_text(str(slot) + '\n')
        self.assertEqual(calls, [(['walk-left'], False)], 'stop.sh ran, with SIGPIPE still ignored')
        self.assertFalse(slots.is_held(slot))

    def test_one_slot_can_be_asked_for_and_three_cannot(self):
        with patch.dict(os.environ, {'TESTVM_SLOTS': '1'}), quiet(), self.hold():
            with self.assertRaises(BlockingIOError):
                with self.hold():
                    self.fail('admitted a second run with one slot')
        for bad in ('3', '0', 'two'):
            with patch.dict(os.environ, {'TESTVM_SLOTS': bad}), self.assertRaises(ValueError):
                with self.hold():
                    pass

    # --- held only for the run: released the instant it ends --------------------------
    def test_a_caller_arriving_between_runs_gets_a_slot_at_once(self):
        """The CEO's measurement, offline: A runs, ends, and is 'thinking'; B arrives."""
        rc, _, err = self.finish(self.child('run', '--', sys.executable, '-c', 'pass'))
        self.assertEqual(rc, 0, err)
        self.assertIn('slot released', err)
        self.assertEqual([slots.is_held(p) for p in slots.slot_paths(self.root)], [False, False])
        began = time.monotonic()
        rc, _, err = self.finish(self.child('run', '--', sys.executable, '-c', 'pass'))
        self.assertEqual(rc, 0, err)
        self.assertLess(time.monotonic() - began, 5)
        self.assertIn('waited 0s', err)

    def test_a_waiting_caller_takes_the_slot_as_soon_as_a_run_ends(self):
        gate = Path(self.tmp.name) / 'end'
        wait_for_gate = ('import pathlib,time\n'
                         f'while not pathlib.Path({str(gate)!r}).exists(): time.sleep(0.05)')
        a = self.child('run', '--purpose', 'run A', '--', sys.executable, '-c', wait_for_gate)
        b = self.child('run', '--purpose', 'run B', '--', sys.executable, '-c', wait_for_gate)
        deadline = time.monotonic() + 20
        while not all(slots.is_held(p) for p in slots.slot_paths(self.root)) and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertTrue(all(slots.is_held(p) for p in slots.slot_paths(self.root)))
        rc, _, err = self.finish(self.child('run', '--', sys.executable, '-c', 'pass'))
        self.assertEqual(rc, 75, err)
        self.assertIn('every slot is executing a run', err)
        c = self.child('run', '--wait', '60', '--', sys.executable, '-c', 'pass')
        time.sleep(1)
        self.assertIsNone(c.poll(), 'it waits while both slots execute')
        ended = time.monotonic()
        gate.touch()
        for p in (a, b):
            rc, _, err = self.finish(p)
            self.assertEqual(rc, 0, err)
        rc, _, err = self.finish(c)
        self.assertEqual(rc, 0, err)
        self.assertLess(time.monotonic() - ended, slots.SLOT_POLL_SECONDS + 5)
        self.assertIn('slot waiting: every slot is executing a run', err)

    def test_the_child_sees_its_slot_and_its_exit_code_passes_through(self):
        show = f'import os,sys; print(os.environ["{slots.ENV}"]); sys.exit(7)'
        rc, out, _ = self.finish(self.child('run', '--', sys.executable, '-c', show))
        self.assertEqual(rc, 7)
        self.assertEqual(out.strip(), str(self.root / 'guest.lock'))
        self.assertFalse(slots.is_held(self.root / 'guest.lock'))

    def test_status_shows_who_holds_what(self):
        gate = Path(self.tmp.name) / 'end'
        a = self.child('run', '--purpose', 'walk seven', '--', sys.executable, '-c',
                       f'import pathlib,time\nwhile not pathlib.Path({str(gate)!r}).exists(): time.sleep(0.05)')
        deadline = time.monotonic() + 20
        while not slots.is_held(self.root / 'guest.lock') and time.monotonic() < deadline:
            time.sleep(0.05)
        status = self.child('status')
        out = status.communicate(timeout=30)[0]
        gate.touch()
        self.finish(a)
        self.assertIn('guest.lock: running, pid', out)
        self.assertIn('walk seven', out)
        self.assertIn('guest-2.lock: free', out)

    # --- run.sh's gate: a guest boots only inside a slot of its own -----------------
    def test_check_refuses_outside_a_slot_and_passes_for_a_descendant_of_the_holder(self):
        rc, _, err = self.finish(self.child('check'))
        self.assertEqual(rc, 1)
        self.assertIn('no guest slot is held', err)
        rc, out, err = self.finish(self.child('run', '--', sys.executable, '-c', DRIVER, 'check'))
        self.assertEqual(rc, 0, err)
        self.assertEqual(out.strip(), str(self.root / 'guest.lock'), 'check prints the slot run.sh records')

    def test_check_refuses_a_slot_somebody_else_holds(self):
        gate = Path(self.tmp.name) / 'end'
        a = self.child('run', '--', sys.executable, '-c',
                       f'import pathlib,time\nwhile not pathlib.Path({str(gate)!r}).exists(): time.sleep(0.05)')
        deadline = time.monotonic() + 20
        while not slots.is_held(self.root / 'guest.lock') and time.monotonic() < deadline:
            time.sleep(0.05)
        rc, _, err = self.finish(self.child('check', env={slots.ENV: str(self.root / 'guest.lock')}))
        self.assertEqual(rc, 1)
        self.assertIn('held by another run', err)
        gate.touch()
        self.finish(a)
        rc, _, err = self.finish(self.child('check', env={slots.ENV: str(self.root / 'guest.lock')}))
        self.assertEqual(rc, 1)
        self.assertIn('is not held', err)
        rc, _, err = self.finish(self.child('check', env={slots.ENV: str(Path(self.tmp.name) / 'mine.lock')}))
        self.assertEqual(rc, 1)
        self.assertIn('not one of this root', err)

    # --- a guest never outlives its slot ---------------------------------------------
    def test_a_guest_still_recorded_against_the_slot_is_stopped_before_release(self):
        calls = []

        def fake_run(args, **kw):
            calls.append((args, [slots.is_held(p) for p in slots.slot_paths(self.root)]))
            return subprocess.CompletedProcess(args, 0)
        with patch('slots.subprocess.run', side_effect=fake_run), quiet():
            with self.hold() as slot:
                mine = self.root / 'run' / 'walk-left'
                mine.mkdir(parents=True)
                (mine / 'slot').write_text(str(slot) + '\n')
                other = self.root / 'run' / 'walk-other'
                other.mkdir(parents=True)
                (other / 'slot').write_text(str(self.root / 'guest-2.lock') + '\n')
        self.assertEqual([c[0][1:] for c in calls], [['walk-left']])
        self.assertTrue(calls[0][0][0].endswith('stop.sh'))
        self.assertEqual(calls[0][1], [True, False], 'stopped while the slot was still held')

    def test_a_failed_or_timed_out_stop_is_raised_after_every_guest_was_tried(self):
        """R19: a surviving guest is not a finished cleanup. Both guests are tried, the slot is
        still released, and the failure is raised naming each survivor."""
        calls = []

        def fake_run(args, **kw):
            calls.append(args[1])
            if args[1] == 'walk-a':
                return subprocess.CompletedProcess(args, 1)
            if args[1] == 'walk-b':
                raise subprocess.TimeoutExpired(args, 300)
            return subprocess.CompletedProcess(args, 0)
        with patch('slots.subprocess.run', side_effect=fake_run), quiet():
            with self.assertRaises(slots.GuestNotStopped) as caught:
                with self.hold() as slot:
                    for name in ('walk-a', 'walk-b', 'walk-c'):
                        d = self.root / 'run' / name
                        d.mkdir(parents=True)
                        (d / 'slot').write_text(str(slot) + '\n')
        self.assertEqual(calls, ['walk-a', 'walk-b', 'walk-c'], 'one failure does not skip the rest')
        self.assertEqual([n for n, _ in caught.exception.survivors], ['walk-a', 'walk-b'])
        self.assertIn('stop.sh exited 1', str(caught.exception))
        self.assertIn('timed out', str(caught.exception))
        self.assertFalse(slots.is_held(slot), 'the slot is still released')
        self.assertEqual(slot.read_text(), '')

    def test_admission_comes_first_and_a_caller_being_sampled_holds_no_slot(self):
        """If the slot were taken before the CPU sample, with the other slot running a
        walk, a second walk arriving during that sample was refused as if both slots ran. Now the
        slot is taken only once the run is admitted: while the CPU and memory are sampled, every
        slot is free and another walk takes one at once."""
        seen = []

        def admit():
            seen.append(('cpu', [slots.is_held(p) for p in slots.slot_paths(self.root)]))
            with self.hold(purpose='walk arriving mid-sample') as other:
                seen.append(('other walk ran in', other.name))
            return {}

        def memory(sample, running):
            seen.append(('memory', [slots.is_held(p) for p in slots.slot_paths(self.root)]))
            return ''
        with patch.dict(os.environ, {'TESTVM_SLOTS': '1'}), quiet():
            with self.hold(purpose='walk Q', admit=admit, memory=memory) as mine:
                self.assertEqual(mine.name, 'guest.lock')
                self.assertTrue(slots.is_held(mine), 'held once admitted')
        self.assertEqual(seen, [('cpu', [False]), ('other walk ran in', 'guest.lock'), ('memory', [False])])

    def test_a_refused_caller_never_held_a_slot_or_left_a_record(self):
        seen = []

        def admit():
            seen.append(slots.describe(self.root / 'guest.lock'))
            raise BlockingIOError('total CPU is 95.0% (limit 80%)')
        with quiet(), self.assertRaisesRegex(BlockingIOError, 'total CPU is 95.0%'):
            with self.hold(purpose='walk Q', admit=admit):
                self.fail('admitted')
        self.assertEqual(seen, ['guest.lock: free'])
        self.assertEqual(slots.describe(self.root / 'guest.lock'), 'guest.lock: free')

    def test_admitted_but_every_slot_taken_meanwhile_waits_for_a_slot(self):
        """The race the order opens: admitted, and both slots were taken during the sample. The
        caller holds nothing and is refused (or waits) exactly as when every slot runs."""
        self.root.mkdir(parents=True)
        taken = []

        def admit():
            for p in slots.slot_paths(self.root):
                h = p.open('a')
                fcntl.flock(h, fcntl.LOCK_EX | fcntl.LOCK_NB)
                taken.append(h)
            return {}
        try:
            with quiet(), self.assertRaisesRegex(BlockingIOError, 'every slot is executing a run'):
                with self.hold(admit=admit):
                    self.fail('admitted without a slot')
        finally:
            for h in taken:
                h.close()

    def test_the_holder_record_is_cleared_on_release(self):
        with quiet(), self.hold(purpose='walk X') as slot:
            self.assertEqual(json.loads(slot.read_text())['purpose'], 'walk X')
        self.assertEqual(slot.read_text(), '')


if __name__ == '__main__':
    unittest.main(verbosity=1)

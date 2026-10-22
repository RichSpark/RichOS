#!/usr/bin/env python3
"""Failure-injection checks for the proof runner's ownership and admission boundaries."""
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from contextlib import redirect_stdout, redirect_stderr

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / 'engine/scripts/lib'))
import proc_tree
import worker_tokens
sys.path.insert(0, str(HERE / "lib"))
import simulator_budget
spec = importlib.util.spec_from_file_location('proof_run', HERE / 'proof-run.py')
pr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pr)


FED_BY_PS = patch.object(proc_tree, '_kernel_topology', new=lambda: None)


class Reliability(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='runner-faults.')
        self.path = Path(self.tmp.name)
        self.children = []

    def tearDown(self):
        for p in self.children:
            if p.poll() is None:
                proc_tree.kill_tree(p.pid, .1)
            p.wait(timeout=5)
        self.tmp.cleanup()

    def wait_file(self, path):
        until = time.monotonic() + 10
        while time.monotonic() < until:
            if path.exists() and path.read_text().strip():
                return int(path.read_text())
            time.sleep(.05)
        self.fail('child did not start: ' + str(path))

    def wait_gone(self, pid):
        until = time.monotonic() + 12
        while time.monotonic() < until:
            if not proc_tree._alive([pid]):
                return
            time.sleep(.1)
        self.fail('owned child survived: ' + str(pid))

    def test_native_generation_preserves_default_session_identity(self):
        reader = proc_tree.operator_fences
        old = reader.proc(os.getpid())
        precise = reader.proc(os.getpid(), precise=True)
        self.assertIsInstance(old['start'], int)
        self.assertIsInstance(precise['start'], str)
        self.assertEqual(proc_tree.identity(os.getpid()), precise['start'])
        self.assertEqual(old['ppid'], precise['ppid'])
        self.assertEqual(old['pgid'], precise['pgid'])
        if sys.platform == 'darwin':
            self.assertEqual(int(precise['start'].split(':')[1]), old['start'])
            self.assertEqual(len(precise['start'].split(':')[2]), 6)
            native = reader._BSDInfo()
            self.assertEqual(reader._libproc().proc_pidinfo(os.getpid(), 3, reader.ctypes.c_uint64(0),
                reader.ctypes.byref(native), reader.ctypes.sizeof(native)), reader.ctypes.sizeof(native))
            self.assertEqual(precise['start'], 'darwin:%d:%06d' % (native.start_sec, native.start_usec))
        elif sys.platform == 'linux':
            raw = Path('/proc/self/stat').read_text()
            ticks = raw[raw.rindex(')') + 2:].split()[19]
            self.assertEqual(precise['start'], 'linux:%s:%s' % (
                Path('/proc/sys/kernel/random/boot_id').read_text().strip(), ticks))
        self.assertIsNone(reader.proc(2147483647, precise=True))

    def test_recycled_root_in_same_second_is_never_readmitted_after_tree_drains(self):
        old = 'darwin:1000:000001'
        new = 'darwin:1000:000002'
        with patch.object(proc_tree, 'identity', return_value=old), \
                patch.object(proc_tree, 'process_rows', return_value={100: (1, 100, old)}) as rows:
            tracker = proc_tree.TrackedTree(100, 'fixture-scope')
            self.assertEqual(set(tracker.known), {100})
            rows.return_value = {}
            self.assertEqual(tracker.refresh(), set())
            rows.return_value = {100: (1, 100, new), 101: (100, 100, new)}
            self.assertEqual(tracker.refresh(), set())
            self.assertEqual(tracker.refresh(), set())

    def test_unreadable_owned_generation_is_carried_unsignaled_and_is_not_clean_completion(self):
        # Nightly 20260928T165947Z-b83ab374 (and its retry, a57cdba4): a live member whose
        # generation libproc cannot read -- a setuid child such as /usr/bin/top or /bin/ps,
        # or one still inside exit -- used to raise "owned process generation is unreadable"
        # after 1 s, which failed the supervision AND skipped its cleanup, refusing a UI
        # suite in which nothing had failed. Fault injection: pid 101 never reads.
        class Clock:
            def __init__(self, *_args, **_kwargs):
                self.t = 0.0
            def now(self):
                self.t += 1.0
                return self.t
            def since(self, start):
                return self.now() - start
        with patch.object(proc_tree, 'identity', side_effect=lambda pid: {100: 'original'}.get(pid)), \
                patch.object(proc_tree, 'process_rows', return_value={100: (1, 100, 'original')}) as rows, \
                patch.object(proc_tree.operator_fences, 'proc', return_value=None), \
                patch.object(proc_tree, '_alive', side_effect=lambda pids: sorted(pids)) as alive, \
                patch.object(proc_tree, 'scoped_members', return_value=set()), \
                patch.object(proc_tree, 'HeldClock', Clock), \
                patch.object(proc_tree.time, 'sleep'):
            tracker = proc_tree.TrackedTree(100, 'fixture-scope')
            rows.return_value = {100: (1, 100, 'original'), 101: (100, 100, None)}
            # Carried, not fatal: still a member, never proven.
            self.assertEqual(tracker.refresh(), {100, 101})
            self.assertEqual(tracker.known, {100: 'original'})
            self.assertEqual(tracker.unresolved, {101: None})
            # Re-read at the next sample, without waiting out the bound again.
            alive.reset_mock()
            self.assertEqual(tracker.refresh(), {100, 101})
            self.assertEqual(alive.call_count, 1)
            # Cleanup: only the proven member is signaled; the unproven one still holds the
            # scope open and is reported as a survivor, never called a clean completion.
            killed = []
            with patch.object(proc_tree.os, 'kill', side_effect=lambda pid, sig: killed.append((pid, sig))):
                left = proc_tree.finish_scope(SimpleNamespace(poll=lambda: None), tracker)
            self.assertEqual(left, [100, 101])
            self.assertIn((100, signal.SIGTERM), killed)
            self.assertNotIn(101, [pid for pid, _sig in killed])
            # A proven member that becomes unreadable keeps its last generation while carried,
            # and is proven again the moment it reads as that generation.
            rows.return_value = {100: (1, 100, None)}
            self.assertEqual(tracker.refresh(), {100})
            self.assertEqual((tracker.known, tracker.unresolved), ({}, {100: 'original'}))
            rows.return_value = {100: (1, 100, 'original')}
            self.assertEqual(tracker.refresh(), {100})
            self.assertEqual((tracker.known, tracker.unresolved), ({100: 'original'}, {}))

    @unittest.skipUnless(sys.platform == 'darwin' and os.path.isfile('/usr/bin/top')
                         and os.stat('/usr/bin/top').st_mode & 0o4000, 'needs setuid /usr/bin/top')
    def test_a_setuid_child_does_not_fail_a_passing_command(self):
        # The real trigger, with no load and no mocks: lib/navigation-evidence.js runs
        # `/usr/bin/top -l 2 -s 1` (setuid root, so libproc reads nothing for it) for about
        # two seconds, twice the supervisor's one-second bound. Before the fix: exit 125,
        # "owned process generation is unreadable".
        result = subprocess.run(proc_tree.command(
            ['bash', '-c', '/usr/bin/top -l 3 -s 1 -n 0 >/dev/null; exit 7']),
            capture_output=True, text=True, timeout=60)
        self.assertNotIn('unreadable', result.stderr)
        self.assertEqual(result.returncode, 7, result.stderr)

    def test_parent_exit_between_samples_retains_group_until_absence_is_confirmed(self):
        with patch.object(proc_tree, 'identity', return_value='original'), \
                patch.object(proc_tree, 'process_rows', return_value={100: (1, 100, 'original')}) as rows:
            tracker = proc_tree.TrackedTree(100, 'fixture-scope')
            rows.return_value = {}
            self.assertEqual(tracker.refresh(), set())
            rows.return_value = {101: (1, 100, 'child')}
            self.assertEqual(tracker.refresh(), {101})
            rows.return_value = {}
            tracker.refresh()
            tracker.refresh()
            self.assertEqual(tracker.groups, {})

    # FED_BY_PS: these tests hand proc_tree its process table as `ps` output. Since
    # 2026-10-22 the table is read from the kernel in-process (_kernel_topology) and `ps`
    # is the fallback, so each of them turns the kernel read off; what is decided from the
    # table is the same code either way (proc_tree_pause.test.py checks the two tables agree).
    @FED_BY_PS
    def test_exit_between_topology_and_native_read_is_not_unreadable(self):
        with patch.object(proc_tree.subprocess, 'run', side_effect=[
                SimpleNamespace(stdout='100 1 100\n', returncode=0),
                SimpleNamespace(stdout='', returncode=1)]), \
                patch.object(proc_tree.operator_fences, 'proc', return_value=None):
            self.assertEqual(proc_tree.process_rows(), {})

    @FED_BY_PS
    def test_zombie_owner_is_dead_even_before_its_parent_reaps_it(self):
        zombie = {'pid': 100, 'ppid': 1, 'pgid': 100, 'start': 'old', 'zombie': True}
        with patch.object(proc_tree.operator_fences, 'proc', return_value=zombie), \
                patch.object(proc_tree.subprocess, 'run', return_value=SimpleNamespace(stdout='100 1 100\n', returncode=0)):
            self.assertIsNone(proc_tree.identity(100))
            self.assertEqual(proc_tree.process_rows(), {})

    @FED_BY_PS
    def test_native_esrch_on_unreaped_zombie_is_not_unreadable_live_process(self):
        with patch.object(proc_tree.operator_fences, 'proc', return_value=None), \
                patch.object(proc_tree.subprocess, 'run', side_effect=[
                    SimpleNamespace(stdout='100 1 100\n', returncode=0),
                    SimpleNamespace(stdout='100 Z\n', returncode=0)]):
            self.assertEqual(proc_tree.process_rows(), {})

    @FED_BY_PS
    def test_unreadable_states_are_batched_without_hiding_live_unknowns(self):
        with patch.object(proc_tree.operator_fences, 'proc', return_value=None), \
                patch.object(proc_tree.subprocess, 'run', side_effect=[
                    SimpleNamespace(stdout='100 1 100\n101 100 100\n102 100 100\n', returncode=0),
                    SimpleNamespace(stdout='100 S\n101 Z\n', returncode=0),
                    SimpleNamespace(stdout='100 S\n', returncode=0)]) as run:
            self.assertEqual(proc_tree.process_rows(), {100: (1, 100, None)})
            self.assertEqual(run.call_count, 3)
            self.assertEqual(run.call_args_list[1].args[0][-1], '100,101,102')
            self.assertEqual(run.call_args.args[0][-1], '100')

    @FED_BY_PS
    def test_seeded_rows_read_only_the_tree_and_never_a_foreign_root_process(self):
        # 100 is the tree's root, 101 its child and 102 in its group with a foreign
        # parent; 200 is someone else's, 300 is root's and unreadable. Only the tree
        # is read natively, and no second ps is spent on 300.
        rows = {pid: {'pid': pid, 'ppid': parent, 'pgid': group, 'start': 'born-%d' % pid, 'zombie': False}
                for pid, parent, group in ((100, 1, 100), (101, 100, 100), (102, 1, 100), (200, 1, 200))}
        with patch.object(proc_tree.operator_fences, 'proc', side_effect=lambda pid, precise=True: rows.get(pid)) as proc, \
                patch.object(proc_tree.subprocess, 'run', return_value=SimpleNamespace(
                    stdout='100 1 100\n101 100 100\n102 1 100\n200 1 200\n300 1 300\n', returncode=0)) as run:
            table = proc_tree.process_rows(seeds={100})
        self.assertEqual(sorted(call.args[0] for call in proc.call_args_list), [100, 101, 102])
        self.assertEqual(run.call_count, 1)
        self.assertEqual(table[101], (100, 100, 'born-101'))
        self.assertEqual(table[200], (1, 200, None))      # topology only: no generation, never owned
        self.assertEqual(table[300], (1, 300, None))

    @FED_BY_PS
    def test_seeded_rows_still_refuse_a_reused_pid(self):
        # 101 was proven as 'old'; the process now at 101 was born later. The tree
        # reads it natively (it is a seed) and does not claim it.
        rows = {100: {'pid': 100, 'ppid': 1, 'pgid': 100, 'start': 'root', 'zombie': False},
                101: {'pid': 101, 'ppid': 1, 'pgid': 101, 'start': 'new', 'zombie': False}}
        with patch.object(proc_tree.operator_fences, 'proc', side_effect=lambda pid, precise=True: rows.get(pid)), \
                patch.object(proc_tree.subprocess, 'run', return_value=SimpleNamespace(
                    stdout='100 1 100\n101 1 101\n', returncode=0)):
            tracker = proc_tree.TrackedTree(100, 'fixture-scope')
            tracker.known[101] = 'old'
            self.assertEqual(tracker.refresh(), {100})

    @FED_BY_PS
    def test_native_generation_is_rechecked_after_a_live_status_observation(self):
        for zombie in (False, True):
            row = {'pid': 100, 'ppid': 2, 'pgid': 90, 'start': 'native-new', 'zombie': zombie}
            with self.subTest(zombie=zombie), \
                    patch.object(proc_tree.operator_fences, 'proc', side_effect=[None, row]), \
                    patch.object(proc_tree.subprocess, 'run', side_effect=[
                        SimpleNamespace(stdout='100 1 100\n', returncode=0),
                        SimpleNamespace(stdout='100 S\n', returncode=0)]):
                self.assertEqual(proc_tree.process_rows(), {} if zombie else {100: (2, 90, 'native-new')})

    def test_unreadable_state_errors_do_not_become_successful_cleanup(self):
        for result in (SimpleNamespace(stdout='', returncode=2),
                       SimpleNamespace(stdout='', returncode=0),
                       SimpleNamespace(stdout='garbage\n', returncode=0)):
            with self.subTest(result=result), patch.object(proc_tree.subprocess, 'run', return_value=result):
                with self.assertRaises(RuntimeError):
                    proc_tree._alive([100])

    def test_success_cleans_detached_reparented_child_and_preserves_unrelated(self):
        unrelated = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(60)'])
        self.children.append(unrelated)
        record = self.path / 'detached'
        script = ('import subprocess,sys; p=subprocess.Popen([sys.executable,"-c",'
                  '"import time;time.sleep(60)"],start_new_session=True); '
                  'open(sys.argv[1],"w").write(str(p.pid))')
        result = subprocess.run(proc_tree.command([sys.executable, '-c', script, str(record)]), timeout=15)
        self.assertEqual(result.returncode, 0)
        self.wait_gone(self.wait_file(record))
        self.assertIsNone(unrelated.poll())

    def test_shell_pipeline_retains_normal_sigpipe_behavior(self):
        result = subprocess.run(proc_tree.command(['bash', '-c',
            'LC_ALL=C tr -dc a-z0-9 </dev/urandom 2>/dev/null | head -c 12']),
            capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(result.stdout), 12)

    def test_success_cleans_background_protected_binary(self):
        record = self.path / 'background'
        script = 'sleep 60 & echo $! > "$1"'
        result = subprocess.run(proc_tree.command(['bash', '-c', script, 'fixture', str(record)]), timeout=15)
        self.assertEqual(result.returncode, 0)
        self.wait_gone(self.wait_file(record))

    def test_term_trap_can_finish_new_cleanup_children(self):
        ready = self.path / 'trap-ready'
        done = self.path / 'trap-done'
        script = self.path / 'trap.sh'
        cleanup = self.path / 'cleanup.sh'
        cleanup.write_text('#!/bin/bash\nsleep 0.7\necho cleaned > "$1"\n')
        script.write_text('#!/bin/bash\n'
            + 'trap \"bash \\\"$3\\\" \\\"$2\\\"; exit 0\" TERM\n'
            + 'echo $$ > "$1"\nwhile :; do sleep 1; done\n')
        wrapper = subprocess.Popen(proc_tree.command(['bash', str(script), str(ready), str(done), str(cleanup)]))
        self.children.append(wrapper)
        self.wait_file(ready)
        wrapper.terminate()
        wrapper.wait(timeout=15)
        self.assertEqual(done.read_text().strip(), 'cleaned')

    def test_sigkill_owner_still_cleans_command_and_releases_lease(self):
        # Both leases are private. Inside a nightly this suite inherits the REAL per-user machine
        # budget (worker_tokens.py `machine --` exports it), whose eight tokens the nightly's own
        # concurrent suites can hold in full; the wrapper then waited for a ninth and its child
        # never started ("child did not start", nightly runs 20260925T233119Z and 20260926T051037Z).
        budget = self.path / 'budget'
        machine = self.path / 'machine'
        worker_tokens.init(budget, 1)
        worker_tokens.init(machine, 1)
        env = {**os.environ, 'RICHOS_MACHINE_WORKERS': str(machine)}
        for key in ('RICHOS_WORKER_TOKENS', 'RICHOS_WORKER_SLOT_HELD', 'RICHOS_WORKER_BORROW_LOCK',
                    'RICHOS_WORKER_TOKENS_RESERVED'):
            env.pop(key, None)
        record = self.path / 'child'
        script = 'import os,sys,time;open(sys.argv[1],"w").write(str(os.getpid()));time.sleep(60)'
        wrapper = subprocess.Popen([sys.executable, worker_tokens.__file__, 'run', str(budget), '--',
                                    sys.executable, '-c', script, str(record)], env=env,
                                   start_new_session=True)
        self.children.append(wrapper)
        child = self.wait_file(record)
        leases = [worker_tokens.Budget(d, shared=False) for d in (budget, machine)]
        self.assertEqual([lease.held() for lease in leases], [1, 1])
        wrapper.kill()
        wrapper.wait(timeout=5)
        self.wait_gone(child)
        until = time.monotonic() + 5
        while any(lease.held() for lease in leases) and time.monotonic() < until:
            time.sleep(.05)
        self.assertEqual([lease.held() for lease in leases], [0, 0])

    def test_separate_runs_share_capacity_and_cannot_resize_live_locks(self):
        machine = self.path / 'machine'
        worker_tokens.init(machine, 2)
        held = []
        try:
            for name in ('one', 'two'):
                local = self.path / name
                worker_tokens.init(local, 4)
                held.append(worker_tokens.Budget(local, runner=True, shared=machine).try_acquire())
            third = self.path / 'three'
            worker_tokens.init(third, 4)
            self.assertIsNone(worker_tokens.Budget(third, runner=True, shared=machine).try_acquire())
            with self.assertRaises(ValueError):
                worker_tokens.init(machine, 1)
            # shared=False: the machine budget itself, never nested inside whatever machine
            # budget RICHOS_MACHINE_WORKERS names for the process running this suite.
            self.assertEqual(worker_tokens.Budget(machine, shared=False).held(), 2)
        finally:
            for token in held:
                token.release()

    def test_nightly_coordinator_sigkill_cleans_work_and_releases_capacity(self):
        record = self.path / 'nightly-child'
        machine = self.path / 'nightly-machine'
        env = {**os.environ, 'RICHOS_MACHINE_WORKERS': str(machine)}
        for key in ('RICHOS_WORKER_TOKENS', 'RICHOS_WORKER_SLOT_HELD'):
            env.pop(key, None)
        command = ('import os,signal,sys,time;'
                   'signal.signal(signal.SIGTERM,signal.SIG_IGN);'
                   'open(sys.argv[1],"w").write(str(os.getpid()));time.sleep(60)')
        launcher = ('import importlib.util,sys;'
                    's=importlib.util.spec_from_file_location("nightly",sys.argv[1]);'
                    'm=importlib.util.module_from_spec(s);s.loader.exec_module(m);'
                    'm.owned_run([sys.executable,"-c",sys.argv[2],sys.argv[3]])')
        coordinator = subprocess.Popen([sys.executable, '-c', launcher,
            str(HERE / 'nightly-local.py'), command, str(record)], env=env, start_new_session=True)
        self.children.append(coordinator)
        sentinel = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(60)'])
        self.children.append(sentinel)
        owned = set()
        try:
            child = self.wait_file(record)
            owned, _ = proc_tree.members(coordinator.pid)
            coordinator.kill()
            coordinator.wait(timeout=5)
            self.wait_gone(child)
            until = time.monotonic() + 5
            while worker_tokens.Budget(machine, shared=False).held() and time.monotonic() < until:
                time.sleep(.05)
            self.assertEqual(worker_tokens.Budget(machine, shared=False).held(), 0)
            self.assertIsNone(sentinel.poll())
        finally:
            # A failing negative control must not itself orphan test processes.
            for pid in proc_tree._alive(owned):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_dead_nightly_cannot_start_work_when_capacity_later_frees(self):
        machine = self.path / 'nightly-queued-machine'
        env = {**os.environ, 'RICHOS_MACHINE_WORKERS': str(machine)}
        for key in ('RICHOS_WORKER_TOKENS', 'RICHOS_WORKER_SLOT_HELD'):
            env.pop(key, None)
        worker_tokens.init(machine, max(1, int((os.cpu_count() or 4) * .8)))
        budget = worker_tokens.Budget(machine, runner=True, shared=False)
        tokens = [budget.try_acquire() for _ in budget.files]
        record = self.path / 'queued-command'
        launcher = ('import importlib.util,sys;'
                    's=importlib.util.spec_from_file_location("nightly",sys.argv[1]);'
                    'm=importlib.util.module_from_spec(s);s.loader.exec_module(m);'
                    'm.owned_run([sys.executable,"-c",'
                    '"from pathlib import Path;import sys;Path(sys.argv[1]).touch()",sys.argv[2]])')
        coordinator = subprocess.Popen([sys.executable, '-c', launcher,
            str(HERE / 'nightly-local.py'), str(record)], env=env, start_new_session=True)
        self.children.append(coordinator)
        owned = set()
        try:
            until = time.monotonic() + 5
            while time.monotonic() < until:
                owned, _ = proc_tree.members(coordinator.pid)
                if len(owned) >= 3:
                    break
                time.sleep(.05)
            self.assertGreaterEqual(len(owned), 3, 'nightly admission did not start')
            self.assertFalse(record.exists())
            coordinator.kill()
            coordinator.wait(timeout=5)
            for pid in owned:
                self.wait_gone(pid)
            for token in tokens:
                token.release()
            time.sleep(.3)
            self.assertFalse(record.exists(), 'work started after its coordinator died')
        finally:
            for pid in proc_tree._alive(owned):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            for token in tokens:
                token.release()

    @unittest.skipUnless(sys.platform == 'darwin', 'the iOS suite requires macOS')
    def test_ios_cache_wrapper_preserves_headless_arguments(self):
        # Stub only compilation. Losing --headless attempts simctl and fails this fixture.
        bindir = self.path / 'bin'
        bindir.mkdir()
        xcrun = bindir / 'xcrun'
        xcrun.write_text('#!/bin/bash\n[ "$1" = swiftc ] || exit 99\n'
            + 'while [ "$#" -gt 0 ]; do\n'
            + ' if [ "$1" = -o ]; then shift; out="$1"; break; fi\n shift\ndone\n'
            + "printf '#!/bin/bash\\nexit 0\\n' > \"$out\"\nchmod +x \"$out\"\n")
        xcrun.chmod(0o755)
        # The gate's path (RICHOS_TEST_SCOPE=full, as proof-run.py sets it): this case is about
        # the default full mode, wherever the suite is run from, never a workspace's narrowing.
        env = {**os.environ, 'PATH': str(bindir) + os.pathsep + os.environ['PATH'],
               'RICHOS_TEST_SCOPE': 'full',
               'RICHOS_NATIVE_IOS_UI_CACHE': str(self.path / 'cache'),
               'RICHOS_CPU_GUARD_STATE': str(self.path / 'guard'),
               'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}
        for key in ('RICHOS_WORKER_TOKENS', 'RICHOS_WORKER_SLOT_HELD', 'RICHOS_SIMULATOR_CACHE_HELD'):
            env.pop(key, None)
        result = subprocess.run(['bash', str(HERE / 'native-ios-ui.test.sh'), '--headless'],
            env=env, capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('native-ios-ui: headless', result.stdout)
        # With no arguments the wrapper must retain the default full mode. The
        # stub deliberately refuses simctl, proving it reached that stage without
        # a nounset error from an empty array on the system's bash 3.2.
        result = subprocess.run(['bash', str(HERE / 'native-ios-ui.test.sh')],
            env=env, capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 99, result.stdout + result.stderr)
        self.assertIn('native-ios-ui: headless', result.stdout)
        self.assertNotIn('unbound variable', result.stderr)

    def test_simulator_resource_does_not_take_another_worker_or_change_borrow_slot(self):
        machine = self.path / 'machine'
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(machine),
                                     'RICHOS_WORKER_BORROW_LOCK': 'existing-borrow'}):
            worker_tokens.machine_directory()
            worker = worker_tokens.Budget(machine, runner=True).try_acquire()
            resource = simulator_budget.acquire('live')
            try:
                self.assertEqual(worker_tokens.Budget(machine).held(), 1)
                result = worker_tokens.run_command([sys.executable, '-c',
                    'import os;assert os.environ["RICHOS_WORKER_BORROW_LOCK"]=="existing-borrow"'],
                    resource, worker=False)
                self.assertEqual(result, 0)
            finally:
                resource.release()
                worker.release()

    def test_independent_simulator_runs_share_two_slots(self):
        env = {**os.environ, 'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}
        records = [self.path / ('sim-' + str(i)) for i in range(3)]
        wrappers = []
        for i, record in enumerate(records):
            command = 'import os,sys,time;open(sys.argv[1],"w").write(str(os.getpid()));time.sleep(60)'
            wrapper = subprocess.Popen([sys.executable, simulator_budget.__file__, 'live', '--',
                                       sys.executable, '-c', command, str(record)], env=env)
            self.children.append(wrapper)
            wrappers.append(wrapper)
            if i < 2:
                self.wait_file(record)
        time.sleep(.5)
        self.assertFalse(records[2].exists())
        wrappers[0].terminate()
        wrappers[0].wait(timeout=10)
        self.wait_file(records[2])
        for wrapper in wrappers[1:]:
            wrapper.terminate()
            wrapper.wait(timeout=10)
        self.assertEqual(worker_tokens.Budget(self.path / 'simulator-live-v1', shared=False).held(), 0)

    def test_simulator_cache_lock_is_shared_by_path_and_separate_from_live_slots(self):
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}):
            cache = self.path / 'checkout-cache'
            first = simulator_budget.acquire('cache', cache=str(cache))
            other = simulator_budget.acquire('cache', cache=str(self.path / 'another-checkout'))
            live = simulator_budget.acquire('live')
            try:
                with self.assertRaises(TimeoutError):
                    simulator_budget.acquire('cache', timeout=.1, cache=str(cache / '..' / cache.name))
            finally:
                first.release()
                other.release()
                live.release()
            simulator_budget.acquire('cache', cache=str(cache)).release()

    def test_existing_simulators_block_admission_and_unreadable_inventory_fails_closed(self):
        quiet = dict(cpu_user_percent=10, cpu_system_percent=5, memory_pressure='normal', swapout_mb_per_s=0)
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}):
            stdout, stderr = io.StringIO(), io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr):
                with self.assertRaises(TimeoutError):
                    simulator_budget.acquire('boot', timeout=.05, sampler=lambda: quiet, inventory=lambda: 2)
            self.assertEqual(stdout.getvalue(), '', 'admission diagnostics must not corrupt child JSON output')
            self.assertIn('boot waits for CPU/memory headroom', stderr.getvalue())
            def broken():
                raise ValueError('unreadable inventory')
            with self.assertRaises(ValueError):
                simulator_budget.acquire('boot', sampler=lambda: quiet, inventory=broken)
            simulator_budget.acquire('boot', sampler=lambda: quiet, inventory=lambda: 1).release()
        data = SimpleNamespace(stdout=json.dumps({'devices': {'runtime': [
            {'state': 'Booted'}, {'state': 'Shutdown'}, {'state': 'Booted'}]}}))
        self.assertEqual(simulator_budget.booted_devices(runner=lambda *a, **kw: data), 2)

    def test_simulator_boot_serializes_and_refuses_saturated_host(self):
        quiet = dict(cpu_user_percent=10, cpu_system_percent=5, memory_pressure='normal', swapout_mb_per_s=0)
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}):
            lease = simulator_budget.acquire('boot', sampler=lambda: quiet, inventory=lambda: 0)
            try:
                with self.assertRaises(TimeoutError):
                    simulator_budget.acquire('boot', timeout=.1, sampler=lambda: quiet, inventory=lambda: 0)
            finally:
                lease.release()
        self.assertTrue(simulator_budget.boot_admitted(quiet))
        self.assertFalse(simulator_budget.boot_admitted({**quiet, 'cpu_user_percent': 80}))
        self.assertFalse(simulator_budget.boot_admitted({**quiet, 'cpu_user_percent': 60, 'cpu_system_percent': 35}))
        self.assertFalse(simulator_budget.boot_admitted({**quiet, 'memory_pressure': 'critical'}))

    def test_borrow_files_do_not_create_extra_capacity(self):
        budget = self.path / 'budget'
        worker_tokens.init(budget, 1)
        # shared=False: this is about the local budget's files only. With the inherited machine
        # budget, a full one made the first acquire None, and made the refusal below prove nothing.
        parent = worker_tokens.Budget(budget, shared=False).try_acquire()
        try:
            borrow = worker_tokens.Budget._try_free(parent.path + '.child')
            self.assertEqual(len(worker_tokens.Budget(budget, shared=False).files), 1)
            self.assertIsNone(worker_tokens.Budget(budget, shared=False).try_acquire())
            borrow.release()
        finally:
            parent.release()

    def test_full_machine_budget_has_bounded_admission(self):
        machine = self.path / 'machine'
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(machine)}):
            worker_tokens.machine_directory()
            budget = worker_tokens.Budget(machine, runner=True)
            held = [budget.try_acquire() for _ in budget.files]
            try:
                args = SimpleNamespace(capacity=1, sample_every=.1, max_cpu=80, keep_going=False,
                                       admission_wait=.1, deadline=1, budget=1)
                item = pr.Item('must wait', str(self.path), [sys.executable, '-c', 'raise Exception("ran")'])
                sample = dict(cpu_user_percent=1, cpu_system_percent=1, memory_pressure='normal', swapout_mb_per_s=0)
                elapsed = pr.run([item], args, str(self.path / 'logs'), sampler=lambda: sample)
                self.assertEqual(item.state, 'not-admitted')
                self.assertIsNone(item.proc)
                self.assertLess(elapsed, 3)
            finally:
                for token in held:
                    token.release()

    def test_interrupt_records_cancelled_checks_and_stops_owned_work(self):
        record = self.path / 'started'
        logdir = self.path / 'interrupted'
        script = self.path / 'run.py'
        child_code = 'import os,sys,time;open(sys.argv[1],"w").write(str(os.getpid()));time.sleep(60)'
        script.write_text(
            'import sys,importlib.util\nfrom types import SimpleNamespace\n'
            + 's=importlib.util.spec_from_file_location("proof_run",' + repr(pr.__file__) + ')\n'
            + 'pr=importlib.util.module_from_spec(s);s.loader.exec_module(pr)\n'
            + 'args=SimpleNamespace(capacity=1,sample_every=.1,max_cpu=80,keep_going=False,admission_wait=10,deadline=60,budget=60)\n'
            + 'items=[pr.Item("running",' + repr(str(self.path)) + ',[sys.executable,"-c",'
            + repr(child_code) + ',' + repr(str(record)) + '],weight=2),'
            + 'pr.Item("pending",' + repr(str(self.path)) + ',[sys.executable,"-c","raise SystemExit(0)"],weight=1)]\n'
            + 'pr.run(items,args,' + repr(str(logdir)) + ',sampler=lambda:dict(cpu_user_percent=1,cpu_system_percent=1,memory_pressure="normal",swapout_mb_per_s=0))\n')
        wrapper = subprocess.Popen([sys.executable, str(script)],
            env={**os.environ, 'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')},
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.children.append(wrapper)
        child = self.wait_file(record)
        wrapper.terminate()
        self.assertEqual(wrapper.wait(timeout=15), 130)
        self.wait_gone(child)
        rows = json.loads((logdir / 'progress.json').read_text())
        self.assertEqual([row['state'] for row in rows], ['cancelled', 'cancelled'])

    def test_explicit_fail_fast_cancels_pending_checks(self):
        args = SimpleNamespace(capacity=1, sample_every=.1, max_cpu=80, fail_fast=True,
                               admission_wait=1, deadline=2, budget=1)
        failed = pr.Item('bad', str(self.path), [sys.executable, '-c', 'raise SystemExit(7)'], weight=2)
        pending = pr.Item('pending', str(self.path), [sys.executable, '-c', 'raise Exception("ran")'], weight=1)
        sample = dict(cpu_user_percent=1, cpu_system_percent=1, memory_pressure='normal', swapout_mb_per_s=0)
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}):
            pr.run([failed, pending], args, str(self.path / 'logs'), sampler=lambda: sample)
        self.assertEqual(failed.rc, 7)
        self.assertEqual(pending.state, 'cancelled')
        self.assertIsNone(pending.proc)

    def test_kernel_saturation_refuses_admission(self):
        from types import SimpleNamespace
        sample = dict(cpu_user_percent=30, cpu_system_percent=66, memory_pressure='normal', swapout_mb_per_s=0)
        self.assertFalse(pr.admitted(SimpleNamespace(max_cpu=80), lambda: sample)[0])

    def test_all_admission_paths_share_the_total_cpu_boundary(self):
        import cpu_policy
        import importlib.util
        spec = importlib.util.spec_from_file_location('native_work_policy_test', HERE.parents[1] / 'engine/scripts/lib/native-work.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        for user, system, admitted in ((30, 49, True), (30, 50, False), (40, 55, False), (79, 0, True)):
            sample = dict(cpu_user_percent=user, cpu_system_percent=system, memory_pressure='normal', swapout_mb_per_s=0)
            self.assertEqual(pr.admitted(SimpleNamespace(max_cpu=80), lambda: sample)[0], admitted)
            self.assertEqual(simulator_budget.boot_admitted(sample), admitted)
            self.assertEqual(cpu_policy.admission_open(cpu_policy.busy_percent(sample)), admitted)
            with patch.object(pr.reserve, 'host_sample', return_value=sample), patch.object(native.time, 'sleep'):
                if admitted:
                    native.wait_for_headroom(pr.reserve, timeout=0)
                else:
                    with self.assertRaises(TimeoutError): native.wait_for_headroom(pr.reserve, timeout=0)

    def test_busy_host_queues_new_work_and_running_check_finishes(self):
        args = SimpleNamespace(capacity=2, sample_every=.05, max_cpu=80, keep_going=False,
                               admission_wait=10, deadline=10, budget=10)
        marker = self.path / 'completed'
        item = pr.Item('complete-once', str(self.path), [sys.executable, '-c',
                       'import pathlib,time;time.sleep(.6);pathlib.Path(' + repr(str(marker)) + ').write_text("done")'], weight=2)
        following = pr.Item('following', str(self.path), [sys.executable, '-c', 'pass'], weight=1)
        samples = []
        def sample():
            # Contention before and during the first check queues the next one.
            running = item.state == 'running'
            busy = running or not samples
            samples.append((running, busy))
            return dict(cpu_user_percent=40, cpu_system_percent=55 if busy else 5,
                        cpu_idle_percent=5 if busy else 55, memory_pressure='normal',
                        memory_free_percent=50, swap_used_mb=0, swapout_mb_per_s=0)
        with patch.dict(os.environ, {'RICHOS_MACHINE_WORKERS': str(self.path / 'machine')}), \
             patch.object(pr.reserve, 'MIN_RETRY_SECONDS', .05), patch.object(pr.Monitor, 'run'), \
             patch.object(pr, 'SETTLE_SECONDS', 0):
            pr.run([item, following], args, str(self.path / 'logs'), sampler=sample)
        self.assertEqual(samples[0], (False, True))
        self.assertIn((True, True), samples)
        self.assertGreaterEqual(item.admission_wait, .05)
        self.assertEqual((item.state, following.state), ('passed', 'passed'))
        self.assertGreaterEqual(following.started, item.ended)
        self.assertEqual(marker.read_text(), 'done')

    def test_unknown_option_cannot_be_ignored_with_saved_commands(self):
        commands = self.path / 'commands'
        commands.write_text('cd richos/app && bash -c "exit 0"\n')
        result = subprocess.run([sys.executable, str(HERE / 'proof-run.py'), '--commands', str(commands),
                                 '--low-priority', '--log-dir', str(self.path / 'logs')], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unexpected arguments', result.stderr)

    def test_active_and_failed_evidence_survives_rotation(self):
        for i in range(6):
            directory = self.path / ('20260923T00000%dZ' % i)
            directory.mkdir()
            if i:
                (directory / 'summary.json').write_text(json.dumps({'checks': [{'result': 'failed' if i == 1 else 'passed'}]}))
        pr.rotate(self.path)
        self.assertTrue((self.path / '20260923T000000Z').exists())
        self.assertTrue((self.path / '20260923T000001Z').exists())
        self.assertFalse((self.path / '20260923T000002Z').exists())

    def test_existing_log_directory_is_not_overwritten(self):
        logs = self.path / 'logs'
        logs.mkdir()
        sentinel = logs / 'summary.json'
        sentinel.write_text('evidence')
        result = subprocess.run([sys.executable, str(HERE / 'proof-run.py'), '--log-dir', str(logs), '--dry-run'],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sentinel.read_text(), 'evidence')


# THE SAME DECISIONS THROUGH THE IN-PROCESS READ (2026-10-22). Each FED_BY_PS test above runs a
# second time with process_rows taking its table from _kernel_topology, the path that runs on
# macOS. The stand-in hands back the very table the test's fake `ps` prints, read through the
# test's own patched subprocess.run, so every fake response, call count and assertion is the
# original's; only the branch of process_rows that builds the topology differs. That the real
# kernel table equals `ps`'s is proc_tree_pause.test.py test_the_kernel_table_is_the_table_ps_prints.
def _kernel_table_from_fake_ps():
    out = proc_tree._ps(["ps", "-ax", "-o", "pid=,ppid=,pgid="]).stdout
    return [tuple(int(f) for f in fields) for fields in (line.split() for line in out.splitlines())
            if len(fields) == 3 and all(f.isdigit() for f in fields)]


FED_BY_KERNEL = patch.object(proc_tree, '_kernel_topology', new=_kernel_table_from_fake_ps)
for _name in [n for n, f in list(vars(Reliability).items()) if getattr(f, 'patchings', None)
              and any(p.attribute == '_kernel_topology' for p in f.patchings)]:
    setattr(Reliability, _name + '_through_the_kernel_read',
            FED_BY_KERNEL(getattr(Reliability, _name).__wrapped__))


if __name__ == '__main__':
    unittest.main(verbosity=2)

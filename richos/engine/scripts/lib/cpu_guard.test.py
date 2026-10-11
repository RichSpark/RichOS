#!/usr/bin/env python3
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch, Mock
import cpu_guard as G
import testdevices as D
import worker_tokens as W

spec = importlib.util.spec_from_file_location('native_work', Path(__file__).with_name('native-work.py'))
N = importlib.util.module_from_spec(spec)
spec.loader.exec_module(N)


class GuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cpu-guard-test-', dir=os.environ['TMPDIR'])
        self.addCleanup(self.tmp.cleanup)
        state = patch.object(G, 'STATE', Path(self.tmp.name))
        env = patch.dict(os.environ, {"RICHOS_TEST_DEVICES_DIR": str(Path(self.tmp.name)/"devices")})
        env.start()
        self.addCleanup(env.stop)
        state.start()
        self.addCleanup(state.stop)

    def row(self, parent=0, cpu=0, birth='birth', name='test'):
        return dict(parent=parent, cpu=cpu, birth=birth, name=name)

    def root(self, pid=10, role='session'):
        G.write_json(G.STATE/'roots'/f'{pid}.json', dict(pid=pid,birth='birth',role=role,label='test session'))

    def test_cpu_time_parser(self):
        self.assertEqual(G.seconds('01:02.50'), 62.5)
        self.assertEqual(G.seconds('1-02:03:04'), 93784)

    def test_inherited_verification_requires_live_registered_ancestor_generation(self):
        import proc_tree
        record = {'pid': 10, 'generation': 'native:birth', 'role': 'verification',
                  'verification': {'protocol': G.VERIFICATION_PROTOCOL}}
        G.write_json(G.STATE / 'roots/10.json', record)
        rows = {10: (1, 10, 'native:birth'), 20: (10, 20, 'child'), 30: (1, 30, 'foreign')}
        with patch.object(proc_tree, 'process_rows', return_value=rows), \
                patch.object(proc_tree, 'identity', return_value='native:birth') as native:
            self.assertEqual(G.inherited_verification_owner('10:native:birth', 20), record)
            for claim, pid in [('10:native:birth', 30), ('10:old', 20), ('20:child', 20),
                               ('not-an-owner', 20), ('0:native:birth', 20), ('', 20)]:
                self.assertIsNone(G.inherited_verification_owner(claim, pid))
            native.return_value = 'reused'
            self.assertIsNone(G.inherited_verification_owner('10:native:birth', 20))
            native.return_value = 'native:birth'
            record['role'] = 'session'
            G.write_json(G.STATE / 'roots/10.json', record)
            self.assertIsNone(G.inherited_verification_owner('10:native:birth', 20))

    def test_managed_supervisor_refuses_direct_or_forged_owner_before_exec(self):
        import proc_tree
        G.write_json(G.STATE / 'verification-enabled.json', {'protocol': G.VERIFICATION_PROTOCOL})
        with patch.dict(os.environ, {'RICHOS_VERIFICATION_OWNER': '1:forged'}), \
                patch.object(proc_tree.subprocess, 'Popen') as launch:
            self.assertEqual(proc_tree.supervise(os.getpid(), [sys.executable, '-c', 'pass']), 75)
            launch.assert_not_called()

    def test_installed_policy_rejects_silent_private_pools_before_admission(self):
        import engine_pass
        machine = G.STATE / 'machine-policy'
        G.write_json(machine / 'verification-enabled.json', {'protocol': 1})
        private = G.STATE / 'private-pool'
        env = {'RICHOS_MACHINE_WORKERS': str(private), 'RICHOS_ENGINE_PASS_DIR': str(private),
               'RICHOS_VERIFICATION_MODE': '', 'RICHOS_VERIFICATION_FIXTURE_ROOT': ''}
        with patch.object(G, 'CANONICAL_STATE', machine), patch.dict(os.environ, env):
            for function in (W.machine_directory, engine_pass.directory):
                with self.assertRaisesRegex(ValueError, 'private capacity/controller'):
                    function()
            self.assertFalse(private.exists())
            W.init(str(private), 1)
            with self.assertRaisesRegex(ValueError, 'private capacity/controller'):
                W.Budget(str(private), shared=False)

    def test_fixture_overrides_require_private_home_config_and_contained_paths(self):
        machine = G.STATE / 'machine-policy'
        G.write_json(machine / 'verification-enabled.json', {'protocol': 1})
        fixture = G.STATE / 'fixture'
        fixture.mkdir()
        env = {'RICHOS_VERIFICATION_MODE': 'fixture', 'RICHOS_VERIFICATION_FIXTURE_ROOT': str(fixture),
               'HOME': str(fixture / 'home'), 'CLAUDE_CONFIG_DIR': str(fixture / 'home/.claude')}
        with patch.object(G, 'CANONICAL_STATE', machine), patch.dict(os.environ, env):
            self.assertEqual(G.governed_directory(fixture / 'pool', machine), (fixture / 'pool').resolve())
            self.assertEqual(G.governed_directory(machine, machine), machine.resolve())
            for changes in ({'HOME': G.pwd.getpwuid(os.getuid()).pw_dir},
                            {'CLAUDE_CONFIG_DIR': str(G.STATE / 'outside')},
                            {'RICHOS_VERIFICATION_MODE': ''}):
                with patch.dict(os.environ, changes), self.assertRaises(ValueError):
                    G.governed_directory(fixture / 'pool', machine)
            (fixture / 'escape').symlink_to(G.STATE)
            with self.assertRaisesRegex(ValueError, 'escapes'):
                G.governed_directory(fixture / 'escape/pool', machine)
            with patch.object(G, 'STATE', fixture / 'controller'):
                self.assertFalse(G.verification_enabled())

    def test_installed_resource_locks_serialize_without_authorizing_workers(self):
        machine = G.STATE / 'machine-policy'
        G.write_json(machine / 'verification-enabled.json', {'protocol': 1})
        workers = G.STATE / 'worker-budget-v1'
        with patch.object(G, 'CANONICAL_STATE', machine), \
                patch.object(G, 'CANONICAL_WORKERS', workers), \
                patch.object(G, 'verification_admission', side_effect=AssertionError('resource is not a worker')):
            for resource in ('native-build', 'simulator-live', 'simulator-boot', 'simulator-cache-' + 'a' * 64):
                directory = workers.parent / (resource + '-v1')
                W.init(directory, 1)
                budget = W.Budget(directory, shared=False, resource=resource)
                first = budget.try_acquire()
                self.assertIsNotNone(first)
                self.assertIsNone(budget.try_acquire())
                with self.assertRaisesRegex(ValueError, 'cannot authorize a worker'):
                    W.run_command([sys.executable, '-c', 'raise AssertionError("must not execute")'], first)
                second = budget.try_acquire()
                self.assertIsNotNone(second)
                second.release()
                with self.assertRaisesRegex(ValueError, 'cannot borrow'):
                    budget.acquire(free='not-a-worker')
            with self.assertRaisesRegex(ValueError, 'unknown resource lock'):
                W.Budget(workers, shared=False, resource='workers')
            with patch.dict(os.environ, {'RICHOS_VERIFICATION_MODE': ''}), \
                    self.assertRaisesRegex(ValueError, 'private capacity/controller'):
                W.Budget(G.STATE / 'private', shared=False, resource='native-build')

    def test_managed_launcher_refuses_unadmitted_work_before_any_capacity_wait(self):
        G.write_json(G.STATE / 'verification-enabled.json', {'protocol': 1})
        timing = str(G.STATE / 'refusal.json')
        with patch.dict(os.environ, {'RICHOS_WORKER_TOKENS': 'forged-pool',
                                     'RICHOS_WORKER_SLOT_HELD': '1',
                                     'RICHOS_VERIFICATION_OWNER': '1:forged'}), \
                patch.object(W, 'Budget', side_effect=AssertionError('must not wait')):
            self.assertFalse(W.inherited_admission())
            self.assertEqual(W.machine_command([sys.executable, '-c', 'raise AssertionError()'], timing), 75)
        result = json.loads(Path(timing).read_text())
        self.assertEqual(result['outcome'], 'not-admitted')
        self.assertEqual(result['execution_seconds'], 0)
        self.assertFalse(result['admitted'])

    def test_machine_policy_remains_visible_when_controller_override_is_empty(self):
        machine = G.STATE / 'machine-policy'
        G.write_json(machine / 'verification-pressure.json', {'protocol': 1})
        with patch.object(G, 'CANONICAL_STATE', machine), \
                patch.dict(os.environ, {'RICHOS_VERIFICATION_MODE': ''}), \
                patch.object(G, 'STATE', G.STATE / 'empty-controller'):
            with self.assertRaisesRegex(ValueError, 'private capacity/controller'):
                G.verification_enabled()

    def test_staged_runtime_can_observe_orphans_without_the_source_checkout(self):
        engine = Path(__file__).resolve().parents[2]
        target = G.stage_runtime(engine, G.STATE / 'runtime')
        code = ('import pathlib,sys,os; sys.path.insert(0,sys.argv[1]); '
                'import cpu_guard,proc_tree; '
                'assert pathlib.Path(proc_tree.__file__).parent == pathlib.Path(sys.argv[1]); '
                'assert proc_tree._alive([os.getpid()]) == [os.getpid()]; '
                'cpu_guard.HostMemory("/missing-source-checkout")')
        result = subprocess.run([sys.executable, '-B', '-I', '-S', '-c', code, str(target.parent)],
                                cwd=G.STATE, capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_installed_versions_reject_changed_or_missing_launchers(self):
        root = G.STATE / 'checkout'
        for name in G.LAUNCHER_FILES:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('def acquire_ios(): pass\n')
        inventory = G.launcher_inventory(root)
        self.assertIsNone(G.launcher_mismatch(root, inventory))
        G.write_json(G.STATE / 'verification-enabled.json', {'protocol': 1, 'launchers': inventory})
        for entry in ('proof-run.py', 'run-tests.sh', 'nightly-local.py'):
            command = 'python3 richos/app/scripts/' + entry
            self.assertIsNone(G.forbidden(command, str(root)))
        path = root / 'richos/engine/scripts/lib/worker_tokens.py'
        path.write_text('old version')
        for command in ('python3 richos/app/scripts/proof-run.py --commands plan',
                        'bash richos/engine/scripts/ci-shard.sh --only-units example',
                        'python3 richos/app/scripts/nightly-local.py check',
                        'bash richos/mobile/native-ios/bin/rios sim'):
            self.assertIn('outdated verification entrypoint', G.forbidden(command, str(root)))
        path.unlink()
        self.assertIn('missing or unreadable', G.launcher_mismatch(root, inventory))
        self.assertIn('incomplete', G.launcher_mismatch(root, {}))
        self.assertIsNone(G.forbidden('git status', str(root)))

    def test_failed_install_keeps_admission_closed_and_user_settings_intact(self):
        home = G.STATE / 'home'
        settings = home / '.claude/settings.json'
        settings.parent.mkdir(parents=True)
        settings.write_text('{"user": "preserve"}')
        engine = Path(__file__).resolve().parents[2]
        original_run = G.subprocess.run
        def run(argv, **kwargs):
            if argv[0] == 'launchctl':
                return subprocess.CompletedProcess(argv, 1, '', 'fixture bootstrap failure')
            if argv[0] == 'git' and 'diff' in argv:
                return subprocess.CompletedProcess(argv, 0)
            return original_run(argv, **kwargs)
        with patch.object(G.Path, 'home', return_value=home), \
                patch.object(G.subprocess, 'run', side_effect=run), patch.object(G.time, 'sleep'):
            with self.assertRaisesRegex(RuntimeError, 'bootstrap failed'):
                G.install(engine)
        policy = G.read_json(G.STATE / 'verification-enabled.json')
        self.assertEqual(policy['activation'], 'starting')
        self.assertEqual(set(policy['launchers']), set(G.LAUNCHER_FILES))
        self.assertIn('admission remains closed', G.verification_admission())
        self.assertEqual(settings.read_text(), '{"user": "preserve"}')
        self.assertTrue(Path(policy['settings_backup']).is_file())

    def test_native_and_device_entries_refuse_before_locks_or_creation(self):
        G.write_json(G.STATE / 'verification-enabled.json', {'protocol': 1})
        with patch.dict(os.environ, {'RICHOS_VERIFICATION_OWNER': '1:forged'}), \
                patch.object(W, 'Budget', side_effect=AssertionError('must not acquire')):
            for operation in (lambda: N.run(['swift', 'test']), D._device_admission,
                              lambda: D.acquire_ios('phone', 'runtime'), lambda: D.boot_ios('device')):
                with self.assertRaisesRegex(ValueError, 'REFUSED before resource wait'):
                    operation()

    def test_sustained_load_and_unowned_process(self):
        self.root()
        watch = G.Watch()
        for now in (0, 2, 4, 6, 8, 10):
            chosen, _, _ = watch.sample({10:self.row(), 20:self.row(10,now*5), 30:self.row(0,now*9)}, now, host_busy=100)
            self.assertEqual(chosen, [])
        chosen, _, protected = watch.sample({10:self.row(),20:self.row(10,60),30:self.row(0,108)},12,host_busy=100)
        self.assertEqual(chosen,[20])
        self.assertIn(10,protected)

    def test_reparented_child_keeps_identity_but_reused_pid_does_not(self):
        self.root()
        watch = G.Watch()
        watch.sample({10:self.row(),20:self.row(10)},0)
        watch.sample({20:self.row(1,4)},2)
        self.assertIn('20',watch.owned)
        watch.sample({20:self.row(1,0,'new process')},4)
        self.assertNotIn('20',watch.owned)

    def test_short_burst_resets_window(self):
        self.root()
        watch = G.Watch()
        for now,cpu in ((0,0),(2,10),(4,20),(6,20),(8,30),(10,40),(12,50),(14,60)):
            chosen,_,_=watch.sample({10:self.row(),20:self.row(10,cpu)},now)
            self.assertEqual(chosen,[])

    def test_aggregate_small_workers_are_not_killed(self):
        self.root()
        watch=G.Watch()
        with patch.object(os,'cpu_count',return_value=4):
            for now in range(0,14,2):
                chosen,_,_=watch.sample({10:self.row(),20:self.row(10,now*2),21:self.row(10,now*2)},now)
            self.assertEqual(chosen,[])

    def test_signal_checks_identity_again(self):
        watch=G.Watch()
        watch.owned={'20':dict(owner='test')}
        rows={20:self.row(cpu=50)}
        with patch.object(G,'processes',return_value={20:self.row(birth='reused')}), patch.object(os,'kill') as kill:
            watch.stop(20,rows,set(),{20:5})
            kill.assert_not_called()

    def test_direct_launch_guard_and_read_only_commands(self):
        for command in ('./gradlew test','bash ./gradlew test','swift test','/usr/bin/xcodebuild build',
                        'xcrun simctl boot UUID','emulator -avd foo','FOO=bar ./gradlew build',
                        "bash -c 'swift build'",'cd app && ./gradlew test','nice -n 10 swift test'):
            self.assertIsNotNone(G.forbidden(command), command)
        for command in ('git status','rg "swift test" .','echo "xcodebuild build"','xcrun simctl list devices',
                        'xcrun simctl shutdown UUID','adb -s emulator-5580 emu kill',
                        'randroid test core','python3 native-work.py -- swift test','./gradlew --stop'):
            self.assertIsNone(G.forbidden(command), command)

    def test_heredoc_data_is_not_a_command_but_shell_body_is(self):
        self.assertIsNone(G.forbidden("cat <<'EOF'\nIt's a description of swift test.\nEOF\ngit status"))
        self.assertEqual(G.forbidden("bash <<'EOF'\nswift test\nEOF"), "swift test")
        self.assertEqual(G.forbidden("cat <<'EOF'\ntext\nEOF\nswift test"), "swift test")

    def test_busy_host_preserves_small_admitted_workload(self):
        self.root()
        watch=G.Watch()
        for now in range(0,62,2):
            chosen,_,_=watch.sample({10:self.row(),20:self.row(10,now)},now,host_busy=99.5)
        self.assertEqual(chosen,[])

    def test_a_job_over_the_core_limit_is_left_alone_while_the_host_has_room(self):
        # P5-22, same per-process rule as cpu_guard_live.py.
        self.root()
        for busy, expected in ((40, []), (79.9, []), (100, [20])):
            watch = G.Watch()
            chosen = []
            for now in range(0, 62, 2):
                chosen, _, _ = watch.sample({10: self.row(), 20: self.row(10, now * 4.0)}, now, host_busy=busy)
            self.assertEqual(chosen, expected, busy)

    def test_release_build_compiler_keeps_its_cores_but_a_runaway_is_still_stopped(self):
        # Run 20260928T190111Z-40a16163: every gate passed, then rustc compiling the app
        # binary under the nightly's build supervisor ran at 4.52 cores and was stopped
        # after 10 s. 10 is that supervisor (release-build root), 20 cargo, 30 rustc. 40/50
        # is the same load under an ordinary session: its rule must not change.
        self.root(10, 'release-build')
        self.root(40, 'session')
        watch = G.Watch()
        stopped, rates, protected = {}, {}, set()
        for now in range(0, 604, 2):
            rows = {10: self.row(), 20: self.row(10), 30: self.row(20, now * 4.5, name='rustc'),
                    40: self.row(), 50: self.row(40, now * 4.5, name='rustc')}
            chosen, rates, protected = watch.sample(rows, now, host_busy=100)
            for pid in chosen:
                stopped.setdefault(pid, now)
        self.assertEqual(stopped.get(50), 12)                # outside a build: 10 s, as before
        self.assertEqual(stopped.get(30), 602)               # inside: 600 s, and still stopped
        self.assertEqual((G.BUILD_ROLE, G.BUILD_WINDOW), ('release-build', 600.0))
        self.assertIn(10, protected)                         # the build's supervisor is never the target
        rows = {pid: {**row, 'generation': 'native:%s' % pid} for pid, row in rows.items()}
        with patch.object(G, 'processes', return_value=rows), patch.object(os, 'kill'):
            watch.stop(30, rows, protected, rates)
        self.assertEqual(G.read_json(G.STATE / 'alert.json')['sustained_seconds'], G.BUILD_WINDOW)

    def test_a_process_that_leaves_the_build_tree_is_judged_by_the_ordinary_window(self):
        self.root(10, 'release-build')
        watch = G.Watch()
        for now in range(0, 14, 2):
            # Observed under the build once, then reparented to launchd: still owned,
            # but no longer a compile step of the registered build.
            chosen, _, _ = watch.sample({10: self.row(), 30: self.row(10 if now == 0 else 1, now * 4.5)}, now,
                                        host_busy=100)
        self.assertEqual(chosen, [30])

    def test_the_build_supervisor_registers_with_the_role_it_was_given(self):
        import proc_tree
        import pwd
        # supervise() installs its own handlers before it starts the child; put ours back.
        saved = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
        self.addCleanup(lambda: [signal.signal(sig, handler) for sig, handler in saved.items()])
        home = pwd.getpwuid(os.getuid()).pw_dir
        env = {k: v for k, v in os.environ.items() if k != 'CLAUDE_CONFIG_DIR'}
        with patch.dict(os.environ, {**env, 'HOME': home}, clear=True), \
                patch.object(G, 'verification_enabled', return_value=False), \
                patch.object(G, 'inherited_verification_owner', return_value=None), \
                patch.object(G, 'healthy', return_value=True), \
                patch.object(G, 'register') as register, \
                patch.object(proc_tree.subprocess, 'Popen', side_effect=OSError('fixture stops here')):
            self.assertEqual(proc_tree.supervise(os.getpid(), ['true'], guard_role='release-build'), 127)
            register.assert_called_once_with(os.getpid(), 'managed true', 'release-build')
        with patch.object(proc_tree, 'supervise', return_value=0) as supervise:
            proc_tree.main(['run', '1', '--guard-role', 'release-build', '--', 'true'])
            self.assertEqual(supervise.call_args.args[-1], 'release-build')
            proc_tree.main(['run', '1', '--', 'true'])
            self.assertEqual(supervise.call_args.args[-1], 'session')
        with self.assertRaises(ValueError):
            proc_tree.supervise(os.getpid(), ['true'], guard_role='anything-else')

    def test_the_canonical_config_named_explicitly_still_registers_and_a_private_one_does_not(self):
        # Second review of af4b68db1 (rv-20261011T123904Z): CLAUDE_CONFIG_DIR set to the
        # ordinary ~/.claude made the supervisor skip registration, so package-app.sh
        # compiled without the release-build role. A private (fixture) config still skips.
        import proc_tree
        import pwd
        saved = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
        self.addCleanup(lambda: [signal.signal(sig, handler) for sig, handler in saved.items()])
        home = pwd.getpwuid(os.getuid()).pw_dir
        env = {k: v for k, v in os.environ.items() if k != 'CLAUDE_CONFIG_DIR'}
        cases = [(os.path.join(home, '.claude'), 1), (os.path.join(home, '.claude') + '/', 1),
                 (os.path.join(home, '.claude', '..', '.claude'), 1), (self.tmp.name, 0)]
        for config, registrations in cases:
            with self.subTest(config=config), \
                    patch.dict(os.environ, {**env, 'HOME': home, 'CLAUDE_CONFIG_DIR': config}, clear=True), \
                    patch.object(G, 'verification_enabled', return_value=False), \
                    patch.object(G, 'inherited_verification_owner', return_value=None), \
                    patch.object(G, 'healthy', return_value=True), \
                    patch.object(G, 'register') as register, \
                    patch.object(proc_tree.subprocess, 'Popen', side_effect=OSError('fixture stops here')):
                self.assertEqual(proc_tree.supervise(os.getpid(), ['cargo'], guard_role='release-build'), 127)
                self.assertEqual(register.call_count, registrations)
                if registrations:
                    register.assert_called_once_with(os.getpid(), 'managed cargo', 'release-build')

    def test_caps_cannot_be_overridden(self):
        gradle=N.capped(['./gradlew','test'])
        self.assertIn('--max-workers=1',gradle)
        self.assertIn('--no-daemon',gradle)
        for args in (['./gradlew','--max-workers=8'],['swift','test','--jobs','8'],['xcodebuild','-jobs','8']):
            with self.assertRaises(ValueError): N.capped(args)
        self.assertEqual(N.capped(['swift','test'])[-2:],['--jobs','1'])

    def test_prebuilt_ui_test_proceeds_with_compiler_lane_held_but_build_does_not(self):
        directory = G.STATE / 'machine'
        lane_dir = G.STATE / 'native-build-v1'
        W.init(directory, 2)
        W.init(lane_dir, 1)
        held = W.Budget(lane_dir, shared=False).acquire()
        original = W.Budget.acquire
        def immediate(budget, **kwargs):
            return original(budget, **dict(kwargs, timeout=0))
        try:
            with patch.dict(os.environ, RICHOS_MACHINE_WORKERS=str(directory), RICHOS_WORKER_SLOT_HELD='0'), \
                    patch.object(sys, 'platform', 'linux'), patch.object(W, 'machine_directory', return_value=directory), \
                    patch.object(W.Budget, 'acquire', immediate), patch.object(G, 'register'), \
                    patch.object(W, 'run_command', return_value=0) as run:
                self.assertEqual(N.run(['xcodebuild', 'test-without-building', '-xctestrun', 'tests']), 0)
                command = run.call_args.args[0]
                self.assertIn('-parallel-testing-enabled', command)
                self.assertEqual(command[command.index('-jobs') + 1], '1')
                with self.assertRaises(TimeoutError): N.run(['xcodebuild', 'build'])
                with self.assertRaises(TimeoutError): N.run(['xcodebuild', 'test-without-building', 'build'])
                self.assertEqual(W.Budget(lane_dir, shared=False).held(), 1)
                self.assertEqual(W.Budget(directory, shared=False).held(), 0)
        finally:
            held.release()

    def test_health_requires_successful_independent_device_collector(self):
        G.write_json(G.STATE/'heartbeat.json', dict(at=time.time(), ok=True))
        self.assertFalse(G.healthy())
        G.write_json(G.STATE/'devices-heartbeat.json', dict(at=time.time(), ok=False))
        self.assertFalse(G.healthy())
        G.write_json(G.STATE/'devices-heartbeat.json', dict(at=time.time(), ok=True))
        self.assertTrue(G.healthy())

    def test_device_worker_runs_without_process_sampling_and_reports_stderr(self):
        G.block_ios('incident')
        with patch.object(G, 'processes', side_effect=TimeoutError('ps unavailable')), patch.object(G.subprocess, 'run') as run:
            run.return_value = Mock(returncode=1, stderr='shutdown RPC timed out', stdout='')
            G.device_cycle('/engine')
            self.assertIn('--pressure', run.call_args.args[0])
            self.assertFalse(G.read_json(G.STATE/'devices-heartbeat.json')['ok'])
            self.assertIn('shutdown RPC timed out', G.read_json(G.STATE/'alert.json')['error'])
            run.return_value = Mock(returncode=0)
            G.device_cycle('/engine')
            self.assertTrue(G.read_json(G.STATE/'devices-heartbeat.json')['ok'])

    def test_old_checkout_proof_launcher_is_refused_before_execution(self):
        old = G.STATE/'old-checkout'
        policy = old/'richos/engine/scripts/lib/testdevices.py'
        policy.parent.mkdir(parents=True)
        policy.write_text('# old collector')
        command = 'cd ' + str(old/'richos/app') + ' && python3 scripts/proof-run.py --working'
        self.assertIn('outdated', G.forbidden(command))
        policy.write_text('def acquire_ios(): pass')
        self.assertIn('outdated', G.forbidden(command))
        policy.with_name('cpu_policy.py').write_text('DEFAULT_MAX_CPU = 80')
        self.assertIsNone(G.forbidden(command))

    def test_incident_stop_is_persistent_and_headless_is_available(self):
        G.block_ios('incident')
        with self.assertRaisesRegex(RuntimeError, 'incident'): G.require_ios()
        for command in ('bash /old/Release/simulator-tests.sh /out', '/old/bin/rios sim prepare',
                        'bash /old/scripts/native-ios-ui.test.sh'):
            self.assertIsNotNone(G.forbidden(command))
        self.assertIsNone(G.forbidden('bash /old/scripts/native-ios-ui.test.sh --headless'))
        self.assertIsNone(G.forbidden('rios headless state'))
        self.assertIsNone(G.forbidden('rios sim stop'))

    def simulator(self, boot=None):
        G.write_json(Path(os.environ['RICHOS_TEST_DEVICES_DIR']) / 'ios.json',
                     dict(kind='ios-simulator', id='owned-udid', boot=boot or {}))

    def healthy_cpu(self, at=100, busy=20):
        G.write_json(G.STATE / 'heartbeat.json', dict(at=at, ok=True, host_busy=busy))
        G.write_json(G.STATE / 'devices-heartbeat.json', dict(at=at, ok=True))

    def test_productive_full_cpu_startup_and_running_device_are_not_stopped(self):
        self.simulator(dict(phase='starting', started=100, deadline=280))
        watcher = G.IOSWatch()
        for now in range(0, 160, 2):
            self.healthy_cpu(at=100 + now, busy=100)
            watcher.sample(100, now, 100 + now)
        self.assertIsNone(G.ios_block())
        self.simulator(dict(phase='ready', started=100, deadline=280, completed=260))
        for now in range(160, 400, 2):
            self.healthy_cpu(at=100 + now, busy=100)
            watcher.sample(100, now, 100 + now)
        self.assertIsNone(G.ios_block())

    def test_startup_deadline_is_enforced_even_on_idle_host(self):
        self.simulator(dict(phase='starting', started=100, deadline=280))
        watcher = G.IOSWatch()
        watcher.sample(0, 0, 279)
        self.assertIsNone(G.ios_block())
        watcher.sample(0, 1, 280)
        self.assertEqual(G.ios_block()['mode'], 'cooldown')
        self.assertIn('owned-udid', G.ios_block()['reason'])

    def test_pressure_with_failed_monitor_sheds_registered_device_without_ps(self):
        self.simulator()
        watcher = G.IOSWatch()
        with patch.object(G, 'processes', side_effect=AssertionError('must not call ps')):
            for now in (0, 2, 4, 6, 8):
                watcher.sample(100, now, 100 + now)
                self.assertIsNone(G.ios_block())
            watcher.sample(100, 10, 110)
        self.assertEqual(G.ios_block()['mode'], 'cooldown')

    def test_short_monitor_failure_recovers_and_stale_success_is_not_healthy(self):
        self.simulator()
        watcher = G.IOSWatch()
        watcher.sample(100, 0, 100)
        self.healthy_cpu(at=108)
        watcher.sample(100, 8, 108)
        self.assertIsNone(watcher.distress_since)
        watcher.sample(100, 20, 120)
        self.assertEqual(watcher.distress_since, 20)
        watcher.sample(100, 28, 128)
        self.assertIsNone(G.ios_block())
        watcher.sample(100, 30, 130)
        self.assertIsNotNone(G.ios_block())

    def test_unrelated_pressure_or_idle_failed_monitor_does_not_stop_devices(self):
        watcher = G.IOSWatch()
        watcher.sample(100, 0, 100)
        watcher.sample(100, 100, 200)
        self.assertIsNone(G.ios_block())
        self.simulator()
        watcher.sample(0, 102, 202)
        watcher.sample(0, 200, 300)
        self.assertIsNone(G.ios_block())

    def test_automatic_cooldown_recovers_only_after_cleanup_health_and_headroom(self):
        with patch.object(G.time, 'time', return_value=100):
            G.block_ios('automatic incident', automatic=True)
            with self.assertRaisesRegex(RuntimeError, 'cooldown'): G.require_ios()
        with patch.object(G.time, 'time', return_value=131):
            with self.assertRaisesRegex(RuntimeError, 'healthy'): G.require_ios()
            self.healthy_cpu(at=131, busy=95)
            with self.assertRaisesRegex(RuntimeError, 'headroom'): G.require_ios()
            self.healthy_cpu(at=131)
            self.simulator()
            with self.assertRaisesRegex(RuntimeError, 'cleanup'): G.require_ios()
            (Path(os.environ['RICHOS_TEST_DEVICES_DIR']) / 'ios.json').unlink()
            self.assertIsNone(G.forbidden('rios sim prepare'))
            G.require_ios()
        self.assertIsNone(G.ios_block())
        self.assertEqual(G.read_json(G.STATE / 'ios-last-incident.json')['reason'], 'automatic incident')

    def test_repeated_incidents_back_off_without_creating_a_boot_retry_loop(self):
        for now, delay in ((100, 30), (140, 120), (270, 600)):
            with patch.object(G.time, 'time', return_value=now):
                G.block_ios('incident', automatic=True)
                self.assertEqual(G.ios_block()['retry_after'], now + delay)
                # Repeated samples of this same incident do not increase count.
                G.block_ios('same incident', automatic=True)
            with patch.object(G.time, 'time', return_value=now + delay + 1):
                self.healthy_cpu(at=now + delay + 1)
                G.require_ios()
        self.assertEqual(len(G.read_json(G.STATE / 'ios-incidents.json')), 3)

    def test_operator_and_legacy_stops_require_explicit_healthy_recovery(self):
        G.block_ios('automatic', automatic=True)
        G.block_ios('operator stop')
        G.block_ios('automatic cannot weaken operator', automatic=True)
        self.assertEqual(G.ios_block()['mode'], 'manual')
        with patch.object(G.time, 'time', return_value=10000):
            with self.assertRaisesRegex(RuntimeError, 'Explicit recovery'): G.require_ios()
            with self.assertRaisesRegex(RuntimeError, 'healthy'): G.recover_ios('repair verified')
            self.healthy_cpu(at=10000)
            self.simulator()
            with self.assertRaisesRegex(RuntimeError, 'cleanup'): G.recover_ios('repair verified')
            (Path(os.environ['RICHOS_TEST_DEVICES_DIR']) / 'ios.json').unlink()
            G.recover_ios('policy repaired')
            self.assertIsNone(G.ios_block())
            G.write_json(G.STATE / 'ios-block.json', dict(at=1, reason='legacy'))
            with self.assertRaisesRegex(RuntimeError, 'Explicit recovery'): G.require_ios()

    def test_xcode_diagnostics_are_disabled_and_cannot_be_overridden(self):
        command=N.capped(['xcodebuild','test'])
        self.assertEqual(command[command.index('-collect-test-diagnostics')+1], 'never')
        with self.assertRaises(ValueError):
            N.capped(['xcodebuild','test','-collect-test-diagnostics','on-failure'])
        self.assertIsNotNone(G.forbidden('xcrun simctl diagnose -l'))
        for command in (['xcrun','simctl','diagnose','-l'], ['xcrun','simctl','boot','UUID'], ['emulator','-avd','foo']):
            with self.assertRaises(ValueError): N.capped(command)

    def test_xcode_reliability_repetitions_stop_on_first_failure_by_default(self):
        command = N.capped(['xcodebuild', 'test-without-building', '-test-iterations', '10'])
        self.assertIn('-run-tests-until-failure', command)
        self.assertNotIn('-retry-tests-on-failure', command)
        for arguments in ([], ['-test-iterations', '1']):
            self.assertNotIn('-run-tests-until-failure', N.capped(['xcodebuild', 'test'] + arguments))
        for mode in ('-run-tests-until-failure', '-retry-tests-on-failure'):
            command = N.capped(['xcodebuild', 'test', '-test-iterations', '10', mode])
            self.assertEqual(command.count(mode), 1)
            self.assertFalse('-run-tests-until-failure' in command and '-retry-tests-on-failure' in command)

    def test_unavailable_cpu_sample_retries_using_full_interval(self):
        reserve=Mock()
        reserve.host_sample.side_effect=[BlockingIOError('counters did not advance'),
                                         {'cpu_idle_percent':80, 'cpu_user_percent':15, 'cpu_system_percent':5}]
        reserve._refusal.return_value=False
        with patch.object(N.time,'sleep') as sleep:
            self.assertEqual(N.wait_for_headroom(reserve, timeout=10), 20)   # total CPU: user + system
        self.assertEqual(reserve.host_sample.call_count,2)
        reserve.host_sample.assert_called_with()
        sleep.assert_called_once_with(3)

    def test_successful_intervention_notice_is_not_repeated_every_turn(self):
        G.write_json(G.STATE/'alert.json',{'at':123,'message':'test intervention'})
        with patch.object(G,'healthy',return_value=True):
            payload={'session_id':'test','hook_event_name':'Stop'}
            self.assertIn('test intervention',G.notice_payload(payload)['systemMessage'])
            self.assertIsNone(G.notice_payload(payload))
            G.write_json(G.STATE/'alert.json',{'at':124,'message':'second intervention'})
            self.assertIn('second intervention',G.notice_payload(payload)['systemMessage'])

    def test_broken_hook_input_refuses_instead_of_returning_nonblocking_error(self):
        child=subprocess.run([sys.executable,str(Path(G.__file__)),'hook'],input='{broken',
                             text=True,capture_output=True)
        self.assertEqual(child.returncode,2)

    def test_missing_watchdog_fails_closed(self):
        with patch.object(sys,'platform','darwin'):
            with self.assertRaisesRegex(RuntimeError,'not healthy'):
                N.run([sys.executable,'-c','raise Exception("must not run")'])

    def test_device_lifetime_and_idle_are_independent(self):
        rec={'lease':dict(created=100,last_use=900,max_seconds=900,idle_seconds=300)}
        self.assertFalse(D.lease_expired(rec,999))
        self.assertTrue(D.lease_expired(rec,1000))
        rec['lease']['last_use']=100
        self.assertTrue(D.lease_expired(rec,400))
        self.assertFalse(D.lease_expired({},100000))

    def test_kernel_lock_serializes_and_releases_on_holder_death(self):
        directory=G.STATE/'budget'
        W.init(directory,1)
        token=W.Budget(directory,shared=False).acquire()
        code='import worker_tokens as w,sys; t=w.Budget(sys.argv[1],shared=False).acquire(timeout=5); print("admitted",flush=True); t.release()'
        child=subprocess.Popen([sys.executable,'-c',code,str(directory)],stdout=subprocess.PIPE,text=True,
                               env={**os.environ,'PYTHONPATH':str(Path(__file__).parent)})
        try:
            time.sleep(.3)
            self.assertIsNone(child.poll())
            token.release()
            self.assertEqual(child.communicate(timeout=6)[0].strip(),'admitted')
            self.assertEqual(child.returncode,0)
        finally:
            token.release()
            if child.poll() is None: child.kill();child.wait()

    def test_real_owned_cpu_process_is_stopped_unrelated_survives(self):
        # One busy core for <2 seconds. Exercise real sampling/signals, with a
        # deliberately lower test threshold instead of saturating this Mac.
        busy=subprocess.Popen([sys.executable,'-c','while True: pass'],start_new_session=True)
        bystander=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])
        try:
            G.register(busy.pid,'test burner','workload')
            watch=G.Watch()
            with patch.object(G,'WINDOW',.3), patch.object(G,'JOB_CORES',.1):
                deadline=time.monotonic()+5
                caught=False
                while time.monotonic()<deadline:
                    rows=G.processes()
                    # A stop needs a host that is out of room (P5-22); this one is not, by design.
                    candidates,rates,protected=watch.sample(rows,time.monotonic(),host_busy=100)
                    if candidates:
                        self.assertEqual(candidates, [busy.pid])
                        watch.stop(candidates[0],rows,protected,rates)
                        self.assertIn(busy.pid, watch.pending)
                        caught=True
                        break
                    time.sleep(.15)
                self.assertTrue(caught)
            busy.wait(timeout=3)
            self.assertEqual(busy.returncode,-signal.SIGTERM)
            self.assertIsNone(bystander.poll())
        finally:
            for child in (busy,bystander):
                if child.poll() is None: child.kill()
                child.wait()


if __name__=='__main__': unittest.main()

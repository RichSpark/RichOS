#!/usr/bin/env python3
"""cpu_guard_live.py (the controller launchd runs) and cpu_guard_live_deploy.py."""
import hashlib
import json
import time
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import cpu_guard_live as L  # noqa: E402
import cpu_guard_live_deploy as D  # noqa: E402


class LiveRuleTests(unittest.TestCase):
    """The same per-process rule as cpu_guard.py, in the controller that actually runs."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cpu-guard-live-')
        self.addCleanup(self.tmp.cleanup)
        state = patch.object(L, 'STATE', Path(self.tmp.name))
        state.start()
        self.addCleanup(state.stop)
        env = patch.dict(os.environ, {'RICHOS_TEST_DEVICES_DIR': str(Path(self.tmp.name) / 'devices')})
        env.start()
        self.addCleanup(env.stop)

    def row(self, parent=0, cpu=0, birth='birth', name='test'):
        return dict(parent=parent, cpu=cpu, birth=birth, name=name)

    def root(self, pid, role):
        L.write_json(L.STATE / 'roots' / ('%s.json' % pid), dict(pid=pid, birth='birth', role=role, label='test'))

    def test_release_build_compiler_keeps_its_cores_but_a_runaway_is_still_stopped(self):
        # Run 20260928T190111Z-40a16163: rustc at 4.52 cores under the nightly's build
        # supervisor, stopped after 10 s by THIS controller (the one launchd runs).
        self.root(10, 'release-build')
        self.root(40, 'session')
        watch = L.Watch()
        stopped = {}
        for now in range(0, 604, 2):
            rows = {10: self.row(), 20: self.row(10), 30: self.row(20, now * 4.5, name='rustc'),
                    40: self.row(), 50: self.row(40, now * 4.5, name='rustc')}
            chosen, rates, protected = watch.sample(rows, now, host_busy=100)
            for pid in chosen:
                stopped.setdefault(pid, now)
        self.assertEqual(stopped.get(50), 12)
        self.assertEqual(stopped.get(30), 602)
        self.assertIn(10, protected)
        with patch.object(L, 'processes', return_value=rows), patch.object(os, 'kill'):
            watch.stop(30, rows, protected, rates)
        self.assertEqual(L.read_json(L.STATE / 'alert.json')['sustained_seconds'], 600.0)

    def test_a_process_that_leaves_the_build_tree_is_judged_by_the_ordinary_window(self):
        self.root(10, 'release-build')
        watch = L.Watch()
        for now in range(0, 14, 2):
            chosen, _, _ = watch.sample({10: self.row(), 30: self.row(10 if now == 0 else 1, now * 4.5)}, now,
                                        host_busy=100)
        self.assertEqual(chosen, [30])

    def test_a_job_over_the_core_limit_is_left_alone_while_the_host_has_room(self):
        # P5-22: four cores for a minute on a host that is 40 percent busy is healthy
        # work, not a runaway. The same job on a saturated host is still stopped.
        self.root(40, 'session')
        for busy, expected in ((40, []), (79.9, []), (100, [50])):
            watch = L.Watch()
            chosen = []
            for now in range(0, 62, 2):
                rows = {40: self.row(), 50: self.row(40, now * 4.0, name='rustc')}
                chosen, _, _ = watch.sample(rows, now, host_busy=busy)
            self.assertEqual(chosen, expected, busy)

    def test_a_brief_host_spike_does_not_stop_a_long_running_job(self):
        self.root(40, 'session')
        watch = L.Watch()
        chosen = []
        for now in range(0, 62, 2):
            rows = {40: self.row(), 50: self.row(40, now * 4.0, name='rustc')}
            chosen, _, _ = watch.sample(rows, now, host_busy=100 if now in (30, 32) else 30)
            self.assertEqual(chosen, [], now)

    def test_the_rule_is_the_same_in_both_controllers(self):
        import cpu_guard
        self.assertEqual((L.BUILD_ROLE, L.BUILD_WINDOW, L.WINDOW, L.JOB_CORES),
                         (cpu_guard.BUILD_ROLE, cpu_guard.BUILD_WINDOW, cpu_guard.WINDOW, cpu_guard.JOB_CORES))


class GrantRuleTests(unittest.TestCase):
    """A registered native build is judged by the cores admission granted it (2026-10-02).

    Run against BOTH controllers: cpu_guard_live.py (what launchd runs) and cpu_guard.py.
    pid 3022 on 2026-10-02 00:21Z: an Android release build admitted with 4 cores ran its
    Gradle JVM at 6.08 and the per-process line (3 cores, 10 s, host at 80 percent) stopped it.
    """

    def setUp(self):
        import cpu_guard
        self.tmp = tempfile.TemporaryDirectory(prefix='cpu-guard-grant-')
        self.addCleanup(self.tmp.cleanup)
        env = patch.dict(os.environ, {'RICHOS_TEST_DEVICES_DIR': str(Path(self.tmp.name) / 'devices')})
        env.start()
        self.addCleanup(env.stop)
        self.controllers = (L, cpu_guard)

    def row(self, parent=0, cpu=0.0, name='test'):
        return dict(parent=parent, cpu=cpu, birth='birth', name=name)

    def run_scenario(self, module, roots, rows_at, seconds=62, busy=100, stop=None):
        """{pid: first time it was chosen} under `module`'s Watch, fixture state only. `stop`
        is a pid to stop after the last sample; its alert record is returned as watch.alert."""
        state = Path(self.tmp.name) / module.__name__
        shutil.rmtree(state, ignore_errors=True)
        with patch.object(module, 'STATE', state):
            for pid, role, grant in roots:
                # The record native-work writes, spelled out (not built with grant_fields) so the
                # same scenario runs against a controller that predates the rule: red on main.
                module.write_json(state / 'roots' / ('%s.json' % pid),
                                  dict(pid=pid, birth='birth', role=role, label='fixture', **(grant or {})))
            watch = module.Watch()
            stopped = {}
            for now in range(0, seconds, 2):
                rows = rows_at(now)
                chosen, rates, protected = watch.sample(rows, now, host_busy=busy)
                for pid in chosen:
                    stopped.setdefault(pid, now)
            if stop is not None:
                rows = {pid: {**row, 'generation': 'native:%s' % pid} for pid, row in rows.items()}
                with patch.object(module, 'processes', return_value=rows), patch.object(os, 'kill'):
                    watch.stop(stop, rows, protected, rates)
                watch.alert = module.read_json(state / 'alert.json')
            return stopped, watch

    def build(self, jvm_cores, unregistered_cores=4.0):
        # 10 native-work (the registered build), 20 its supervisor, 30 the Gradle JVM.
        # 40 a session, 50 a process of its own at `unregistered_cores` (nothing registered it).
        return lambda now: {10: self.row(1), 20: self.row(10, now * 0.1), 30: self.row(20, now * jvm_cores, 'java'),
                            40: self.row(1), 50: self.row(40, now * unregistered_cores, 'java')}

    GRANT = {'cores': 5, 'group': '/workspace/a'}

    def test_a_build_using_its_grant_is_not_stopped_but_an_unregistered_process_at_four_cores_is(self):
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                stopped, watch = self.run_scenario(
                    module, [(10, 'native-build', self.GRANT), (40, 'session', None)], self.build(4.5))
                self.assertNotIn(30, stopped)                # 4.5 + 0.1 cores, granted 5
                self.assertEqual(stopped.get(50), 12)        # unregistered: today's rule, unchanged
                self.assertEqual(watch.grants['/workspace/a']['cores'], 5.0)

    def test_a_build_above_its_grant_is_judged_by_the_per_process_rule_again(self):
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                stopped, watch = self.run_scenario(
                    module, [(10, 'native-build', self.GRANT), (40, 'session', None)], self.build(6.0), stop=30)
                self.assertEqual(stopped.get(30), 12)        # 6.1 cores against a grant of 5
                self.assertEqual(watch.alert['grant'], {'group': '/workspace/a', 'cores': 5.0, 'used': 6.1})

    def test_the_host_rule_still_applies_and_a_brief_excess_resets(self):
        # Within the grant on a quiet host is untouched (nothing new); a single sample over the
        # grant does not start a stop: the excess, like the line, has to last WINDOW seconds.
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                spiky = lambda now: {**self.build(4.5)(now), 30: self.row(20, now * 4.5 + (6 if now >= 30 else 0), 'java')}
                stopped, _ = self.run_scenario(module, [(10, 'native-build', self.GRANT)], spiky)
                self.assertNotIn(30, stopped)

    def test_a_warm_daemon_counts_with_its_build_against_one_grant(self):
        # 60 is the workspace's Gradle daemon: reparented to launchd and registered in the build's
        # group. The build tree (10/20/30) is light; the daemon does the compiling.
        def rows(client, daemon):
            return lambda now: {10: self.row(1), 20: self.row(10, now * 0.1), 30: self.row(20, now * client, 'java'),
                                60: self.row(1, now * daemon, 'java'), 61: self.row(60, now * 0.2, 'aapt2')}
        roots = [(10, None, self.GRANT), (60, None, self.GRANT)]
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                native = [(pid, 'native-build', grant) for pid, _role, grant in roots]
                stopped, watch = self.run_scenario(module, native, rows(0.2, 4.4))
                self.assertEqual(stopped, {})                 # 0.1 + 0.2 + 4.4 + 0.2 = 4.9 of 5
                self.assertEqual(sorted(watch.grants['/workspace/a']['members']), [10, 20, 30, 60, 61])
                stopped, _ = self.run_scenario(module, native, rows(1.0, 4.4))
                self.assertEqual(stopped.get(60), 12)         # 5.7 of 5: the daemon is over the line

    def test_a_process_that_leaves_the_build_is_no_longer_in_its_grant(self):
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                moved = lambda now: {10: self.row(1), 30: self.row(10 if now == 0 else 1, now * 4.5, 'java')}
                stopped, _ = self.run_scenario(module, [(10, 'native-build', self.GRANT)], moved)
                self.assertEqual(stopped.get(30), 12)

    def test_a_grant_needs_the_native_role_positive_cores_and_a_group(self):
        for module in self.controllers:
            with self.subTest(controller=module.__name__):
                self.assertEqual(module.grant_fields('session', None), {})
                for role, grant in (('session', self.GRANT), (module.NATIVE_ROLE, None),
                                    (module.NATIVE_ROLE, {'cores': 0, 'group': 'g'}),
                                    (module.NATIVE_ROLE, {'cores': 4, 'group': ''})):
                    with self.assertRaises(ValueError):
                        module.grant_fields(role, grant)

    def test_the_grant_rule_is_the_same_in_both_controllers(self):
        import cpu_guard
        self.assertEqual((L.NATIVE_ROLE, cpu_guard.NATIVE_ROLE), ('native-build', 'native-build'))


class ObservationIsNotAnAlertTests(unittest.TestCase):
    """Orchestration reliability R6: "CPU pressure ... are observations. A CPU alert's list of
    unowned processes does not prove who consumed the host." On 2026-10-23 that list reached
    every turn end (the screenshot turn's 479-character CPU alert) and every Bash call."""

    def test_high_host_cpu_with_nothing_stopped_is_logged_and_never_shown(self):
        # The controller launchd and the hooks run (cpu_guard_live_deploy.py); main's
        # cpu_guard.py is not deployable and shows nothing anywhere.
        for module in (L,):
            with self.subTest(controller=module.__name__), \
                    tempfile.TemporaryDirectory(prefix='cpu-guard-note-') as directory, \
                    patch.object(module, 'STATE', Path(directory)), \
                    patch.object(module, 'healthy', return_value=True):
                module.host_pressure_note(97.8, [dict(pid=1, executable='/Applications/Terminal.app', cores=0.03)])
                rows = [json.loads(line) for line in (Path(directory) / 'events.jsonl').read_text().splitlines()]
                self.assertEqual(rows[-1]['host_busy'], 97.8)                    # kept in the existing log
                self.assertIsNone(module.read_json(Path(directory) / 'alert.json'))
                self.assertIsNone(module.notice_payload({'session_id': 's', 'hook_event_name': 'Stop'}))
                # An action is still shown, once.
                module.event('CPU circuit breaker stopped an owned workload', pid=50)
                self.assertIn('stopped an owned workload',
                              module.notice_payload({'session_id': 's', 'hook_event_name': 'Stop'})['systemMessage'])


class RuntimeDependencyTests(unittest.TestCase):
    def test_install_stages_reader_before_launching_any_service(self):
        with tempfile.TemporaryDirectory(prefix='controller-support-') as directory:
            root = Path(directory)
            state = root / 'state'
            home = root / 'home'
            with patch.object(L, 'STATE', state), patch.object(Path, 'home', return_value=home), \
                    patch.object(L.subprocess, 'run', side_effect=RuntimeError('fixture stops before launchctl')):
                with self.assertRaisesRegex(RuntimeError, 'fixture stops'):
                    L.install(root / 'engine')
            runtime = state / 'runtime'
            self.assertEqual((runtime / 'machine_settings.py').read_bytes(), (HERE / 'machine_settings.py').read_bytes())
            result = subprocess.run([sys.executable, '-B', str(runtime / 'cpu_guard.py'), 'status'],
                                    cwd=runtime, env={**os.environ, 'HOME': str(home),
                                    'RICHOS_CPU_GUARD_STATE': str(state), 'RICHOS_MACHINE_SETTINGS_RESOLVED':'1'},
                                    capture_output=True, text=True, timeout=30)
            self.assertIn(result.returncode, (0,1), result.stderr)
            self.assertIn('healthy', json.loads(result.stdout))


class DeployTests(unittest.TestCase):
    """Replace the running controller only from a known one, prove the swap, undo a failure."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cpu-guard-deploy-')
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.state = root / 'state'
        runtime = self.state / 'runtime'
        runtime.mkdir(parents=True)
        self.legacy = b'# fixture legacy controller\nimport sys\nsys.exit(0)\n'
        (runtime / 'cpu_guard.py').write_bytes(self.legacy)
        shutil.copyfile(HERE / 'cpu_policy.py', runtime / 'cpu_policy.py')
        self.plist = root / 'agent.plist'
        self.plist.write_bytes(plistlib.dumps({'ProgramArguments': [
            '/usr/bin/python3', '-B', str(runtime / 'cpu_guard.py'), 'watch', '/engine']}))
        for name, value in (('LEGACY_SHA256', hashlib.sha256(self.legacy).hexdigest()),
                            ('POLICY_SHA256', D.sha256(runtime / 'cpu_policy.py'))):
            p = patch.object(D, name, value)
            p.start()
            self.addCleanup(p.stop)
        p = patch.object(D.Live, 'committed', return_value='f' * 40)
        p.start()
        self.addCleanup(p.stop)
        self.candidate = HERE / 'cpu_guard_live.py'
        self.restarts = []
        self.next_pid = 1000

    def heartbeat(self, controller):
        self.next_pid += 1
        beat = {'at': time.time(), 'ok': True, 'pid': self.next_pid}
        if controller:
            beat['controller'] = controller
        (self.state / 'heartbeat.json').write_text(json.dumps(beat))

    def live(self, behavior=None):
        def restart():
            self.restarts.append(D.sha256(self.state / 'runtime/cpu_guard.py'))
            (behavior or (lambda running: self.heartbeat(None if running == D.LEGACY_SHA256 else running)))(
                self.restarts[-1])
        return D.Live(self.state, self.plist, self.candidate, restart=restart, wait=0.5)

    def test_deploys_the_candidate_over_the_legacy_controller_and_records_it(self):
        message = self.live().deploy()
        candidate = D.sha256(self.candidate)
        self.assertEqual(D.sha256(self.state / 'runtime/cpu_guard.py'), candidate)
        self.assertEqual(self.restarts, [candidate])
        backup = self.state / 'runtime-backups' / ('cpu_guard-%s.py' % D.LEGACY_SHA256[:12])
        self.assertEqual(backup.read_bytes(), self.legacy)
        record = json.loads((self.state / 'live-runtime.json').read_text())
        self.assertEqual((record['sha256'], record['source_revision']), (candidate, 'f' * 40))
        self.assertIn('deployed', message)
        self.assertIn('already running', self.live().deploy())
        self.assertEqual(len(self.restarts), 1)

    def test_a_candidate_that_never_reports_is_taken_out_again(self):
        # The new controller never writes a heartbeat; the legacy one does after the put-back.
        live = self.live(lambda running: self.heartbeat(None) if running == D.LEGACY_SHA256 else None)
        with self.assertRaisesRegex(SystemExit, 'FAILED.*put back and is running'):
            live.deploy()
        self.assertEqual((self.state / 'runtime/cpu_guard.py').read_bytes(), self.legacy)
        self.assertFalse((self.state / 'live-runtime.json').exists())

    def test_controller_path_alias_is_accepted_but_relative_path_is_refused(self):
        alias = Path(self.tmp.name) / 'state-alias'
        alias.symlink_to(self.state.resolve(), target_is_directory=True)
        config = plistlib.loads(self.plist.read_bytes())
        config['ProgramArguments'][2] = str(alias / 'runtime/cpu_guard.py')
        self.plist.write_bytes(plistlib.dumps(config))
        live = self.live()
        self.assertIn('deployed', live.deploy())
        self.assertEqual(self.restarts, [D.sha256(self.candidate)])
        config['ProgramArguments'][2] = os.path.relpath(self.state / 'runtime/cpu_guard.py')
        self.assertEqual(Path(config['ProgramArguments'][2]).resolve(), live.running.resolve())
        self.plist.write_bytes(plistlib.dumps(config))
        with self.assertRaisesRegex(SystemExit, 'does not run'):
            live.preconditions()

    def test_an_unknown_running_controller_or_plist_is_refused_untouched(self):
        (self.state / 'runtime/cpu_guard.py').write_bytes(b'# somebody else\n')
        with self.assertRaisesRegex(SystemExit, 'neither the legacy controller'):
            self.live().deploy()
        self.assertEqual((self.state / 'runtime/cpu_guard.py').read_bytes(), b'# somebody else\n')
        (self.state / 'runtime/cpu_guard.py').write_bytes(self.legacy)
        self.plist.write_bytes(plistlib.dumps({'ProgramArguments': ['/usr/bin/python3', '-B', '/elsewhere.py', 'watch']}))
        with self.assertRaisesRegex(SystemExit, 'does not run'):
            self.live().deploy()
        self.assertEqual(self.restarts, [])

    def test_deployment_preflight_stages_the_shared_reader(self):
        live = self.live()
        live.interpreter = sys.executable
        live.loads(self.candidate)
        self.assertEqual((live.runtime / 'machine_settings.py').read_bytes(), (HERE / 'machine_settings.py').read_bytes())

    def test_a_candidate_that_does_not_load_under_launchds_interpreter_is_refused_untouched(self):
        # launchd runs /usr/bin/python3 (3.9 on this Mac), not the shell's python3.
        self.plist.write_bytes(plistlib.dumps({'ProgramArguments': [
            '/usr/bin/false', '-B', str(self.state / 'runtime/cpu_guard.py'), 'watch', '/engine']}))
        with self.assertRaisesRegex(SystemExit, 'does not load under /usr/bin/false'):
            self.live().deploy()
        self.assertEqual((self.state / 'runtime/cpu_guard.py').read_bytes(), self.legacy)
        self.assertEqual(self.restarts, [])
        self.assertEqual(sorted(p.name for p in (self.state / 'runtime').iterdir()), ['cpu_guard.py', 'cpu_policy.py', 'machine_settings.py'])

    def test_rollback_restores_the_verified_legacy_bytes(self):
        self.live().deploy()
        self.assertIn('rolled back', self.live().rollback())
        self.assertEqual((self.state / 'runtime/cpu_guard.py').read_bytes(), self.legacy)
        self.assertEqual(self.restarts[-1], D.LEGACY_SHA256)
        self.assertIn('rolled_back_at', json.loads((self.state / 'live-runtime.json').read_text()))
        self.assertIn('already running', self.live().rollback())

    def test_a_later_candidate_replaces_the_one_this_script_deployed(self):
        # 2026-10-02: launchd ran the 8e9cad40 candidate while main's cpu_guard_live.py had
        # moved on (P5-22), and deploy refused it as neither legacy nor candidate.
        earlier = b'# an earlier candidate\nimport sys\nsys.exit(0)\n'
        digest = hashlib.sha256(earlier).hexdigest()
        (self.state / 'runtime/cpu_guard.py').write_bytes(earlier)
        (self.state / 'live-runtime.json').write_text(json.dumps(
            {'sha256': digest, 'source_revision': 'e' * 40, 'source_file': '/elsewhere/cpu_guard_live.py'}))
        self.assertIn('earlier deploy', self.live().status()['running_is'])
        self.assertIn('deployed', self.live().deploy())
        self.assertEqual(D.sha256(self.state / 'runtime/cpu_guard.py'), D.sha256(self.candidate))
        backup = self.state / 'runtime-backups' / ('cpu_guard-%s.py' % digest[:12])
        self.assertEqual(backup.read_bytes(), earlier)

    def test_a_failed_deploy_over_an_earlier_candidate_puts_that_candidate_back(self):
        earlier = b'# an earlier candidate\nimport sys\nsys.exit(0)\n'
        digest = hashlib.sha256(earlier).hexdigest()
        (self.state / 'runtime/cpu_guard.py').write_bytes(earlier)
        (self.state / 'live-runtime.json').write_text(json.dumps({'sha256': digest, 'source_revision': 'e' * 40}))
        # The new controller never reports; the earlier one reports its own sha256 when put back.
        live = self.live(lambda running: self.heartbeat(running) if running == digest else None)
        with self.assertRaisesRegex(SystemExit, 'FAILED.*put back and is running'):
            live.deploy()
        self.assertEqual((self.state / 'runtime/cpu_guard.py').read_bytes(), earlier)

    def test_a_running_file_that_differs_from_the_recorded_deploy_is_still_refused(self):
        (self.state / 'runtime/cpu_guard.py').write_bytes(b'# edited in place\n')
        (self.state / 'live-runtime.json').write_text(json.dumps({'sha256': '0' * 64, 'source_revision': 'e' * 40}))
        with self.assertRaisesRegex(SystemExit, 'neither the legacy controller'):
            self.live().deploy()
        self.assertEqual(self.restarts, [])

    def test_check_says_whether_launchd_runs_this_checkouts_controller(self):
        live = self.live()
        with patch.object(D, 'Live', lambda: live), patch('builtins.print') as said:
            self.assertEqual(D.main(['check']), 3)
            self.assertIn('CPU GUARD STALE', said.call_args.args[0])
            live.deploy()
            self.assertEqual(D.main(['check']), 0)

    def test_status_names_what_runs_and_changes_nothing(self):
        status = self.live().status()
        self.assertEqual(status['running_is'], 'legacy controller (9154bf51)')
        self.assertEqual(self.restarts, [])


if __name__ == '__main__':
    unittest.main()

"""The merge-scope regressions use real selectors and fixture tool/result protocols."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("MERGE_SCOPE_TEST_ROOT", HERE.parents[2]))
TOOLS = ROOT / "richos/app/scripts"
sys.path.insert(0, str(HERE / 'lib'))
import ios_ui_scope
import ios_ui_shards


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Scope(unittest.TestCase):
    def selection(self, paths):
        result = subprocess.run(['bash', str(TOOLS / 'proof-for.sh'), '--quiet', '--paths', ','.join(paths)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_previous_cargo_fix_selects_no_iphone_suite(self):
        paths = subprocess.check_output(['git', '-C', str(ROOT), 'diff', '--name-only',
                                         'ad1b6996..0c65837b'], text=True).splitlines()
        # The one-app work deleted two of that fix's 32 files (operator-mutations.py,
        # operator-walk.test.sh); the selector rightly refuses a path that no longer exists.
        # The other 30 are still the cargo fix whose selection this case is about.
        paths = [p for p in paths if (ROOT / p).exists()]
        plan = self.selection(paths)
        for name in ('native-ios-app', 'native-ios-ui', 'native-ios-share'):
            self.assertNotIn(name + '.test.sh', plan)

    def test_real_shared_app_dependency_still_selects_iphone(self):
        plan = self.selection(['richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/URLSessionTransport.swift'])
        for name in ('native-ios-app', 'native-ios-ui', 'native-ios-share'):
            self.assertIn(name + '.test.sh', plan)

    def test_composer_change_selects_cases_and_not_unrelated_pairing(self):
        plan = self.selection(['richos/mobile/native-ios/App/Features/Composer/ComposerView.swift'])
        self.assertIn('--only ScreenshotTests/testComposerDark', plan)
        self.assertIn('--only InteractionTests/testComposeAndSendAddsTheMessageAndFollowsIt', plan)
        self.assertNotIn('--only PairWaitInteractionTests', plan)

    def test_new_app_source_is_conservative_and_test_source_selects_its_cases(self):
        self.assertIsNone(ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/App/NewFeature/New.swift']))
        selected = ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/UITests/QuestionTests.swift'])
        self.assertEqual(selected, ['QuestionTests/testOtherAnswerInLightTheme',
                                    'QuestionTests/testTapAndOfflineEditLeaveTheComposerAvailable'])

    def test_product_plist_and_package_changes_still_select_all_phone_suites(self):
        for path in ['Release/App-Info.plist', 'Core/Package.swift']:
            plan = self.selection(['richos/mobile/native-ios/' + path])
            for suite in ('native-ios-app', 'native-ios-share', 'native-ios-ui'):
                self.assertIn(suite + '.test.sh', plan)

    def test_unit_source_selects_its_suite_with_attribution_control(self):
        selected = ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/UnitTests/TooShortLineTests.swift'])
        self.assertEqual(selected, ['RichOSNativeTests/BuildStampTests/testTheBundleCarriesItsBuildStamp',
                                   'RichOSNativeTests/TooShortLineTests'])

    def test_suites_queue_on_one_lane_with_existing_finite_deadlines(self):
        pr = load('merge_scope_runner', 'richos/app/scripts/proof-run.py')
        from types import SimpleNamespace
        args = SimpleNamespace(deadline=1800, engine_shards=4, fail_fast=False)
        # plan() keeps an explicit RICHOS_IOS_POOL_WAIT from its caller; the nightly sets one for its
        # children, so this test must not read the ambient value (nightly attempt 2: KeyError).
        with tempfile.TemporaryDirectory() as log, mock.patch.dict(os.environ):
            os.environ.pop('RICHOS_IOS_POOL_WAIT', None)
            items = pr.plan(['cd richos/app && scripts/run-tests.sh --no-host-screen --only native-ios-app.test.sh --only native-ios-share.test.sh',
                             'cd richos/app && bash scripts/native-ios-ui.test.sh --only QuestionTests/testOtherAnswerInLightTheme'], args, log, {})
        self.assertEqual(len(items), 3)
        self.assertEqual({item.lane for item in items}, {'ios-simulator'})
        for item in items:
            self.assertEqual(float(item.env['RICHOS_IOS_POOL_WAIT']), pr.deadline_for(item, args))

    def test_direct_ui_refusal_is_not_run_and_actual_failure_stays_failed(self):
        pr = load('merge_scope_verdict', 'richos/app/scripts/proof-run.py')
        with tempfile.TemporaryDirectory() as work:
            item = pr.Item('native-ios-ui', str(ROOT), ['bash', 'scripts/native-ios-ui.test.sh'])
            item.rc, item.state = 2, 'failed'
            item.log = str(Path(work) / 'log')
            Path(item.log).write_text('  NOT RUN  native-ios-ui: prepared simulator is leased by another run; no UI test ran\n')
            pr.not_run(item)
            self.assertEqual(item.state, 'not-run')
            item.state = 'failed'
            Path(item.log).write_text('  FAIL  assertion\n  NOT RUN  native-ios-ui: unavailable\n')
            pr.not_run(item)
            self.assertEqual(item.state, 'failed')

    def test_real_ui_acquisition_adapter_distinguishes_refusal_and_error(self):
        text = (TOOLS / 'native-ios-ui.test.sh').read_text()
        start = text.index('  SIM_UDID[i]="$(python3')
        end = text.index('  CREATED+=', start)
        block = text[start:end]
        with tempfile.TemporaryDirectory() as work:
            base = 'WORK=' + work + '\ni=0; SIM_TYPE=(fixture); RUNTIME=fixture; RICHOS_TESTDEVICES=fixture\n'
            for message, code in [('prepared simulator is leased by another run', 2), ('cannot read simulator inventory', 1)]:
                fixture = base + 'python3() { echo "' + message + '" >&2; return 1; }\nnot_run() { echo "  NOT RUN  native-ios-ui: $*"; exit 2; }\n' + block
                result = subprocess.run(['bash', '-c', fixture], text=True, capture_output=True)
                self.assertEqual(result.returncode, code, result.stdout + result.stderr)

    def test_share_and_app_adapters_distinguish_unavailable_device_from_failure(self):
        app = (TOOLS / 'native-ios-app.test.sh').read_text()
        app = app[app.index('if "$RIOS" sim prepare'):app.index('UDID="$(json', app.index('if "$RIOS" sim prepare'))]
        share = (ROOT / 'richos/mobile/native-ios/Release/simulator-tests.sh').read_text()
        start = share.index('UDID="$(python3')
        share = share[start:share.index('cleanup()', start)]
        with tempfile.TemporaryDirectory() as work:
            for message, expected in [('prepared simulator is leased by another run', 2), ('compile error', 1)]:
                prepare = Path(work) / 'rios'
                prepare.write_text('#!/bin/sh\necho "' + message + '" >&2\nexit 1\n')
                prepare.chmod(0o755)
                fixture = ('SCRATCH=' + work + '; OUT=' + work + '; RIOS=' + str(prepare) + '\n'
                           'PASS=0; FAILED=0; HERE=fixture; RUNTIME=fixture\n'
                           'bad() { FAILED=$((FAILED + 1)); }; notrun() { echo "NOT RUN $*"; exit 2; }\n'
                           'python3() { echo "' + message + '" >&2; return 1; }\n')
                for block in (app, share):
                    result = subprocess.run(['bash', '-c', fixture + block], text=True, capture_output=True)
                    self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_unreviewed_case_and_new_feature_file_use_full_coverage(self):
        original = ios_ui_scope.inventory(ROOT)
        changed = {**original, 'NewTests.swift': ['NewTests/testNewBehavior']}
        with mock.patch.object(ios_ui_scope, 'inventory', return_value=changed):
            self.assertIsNone(ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/App/Features/Composer/ComposerView.swift']))
        self.assertIsNone(ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/App/Features/Composer/NewView.swift']))

    def test_existing_shard_attribution_and_completeness_controls(self):
        self.assertEqual(ios_ui_shards.selftest(), 0)

    def test_run_tests_refusal_is_not_run_and_failure_wins(self):
        pr = load('merge_scope_wrapped_verdict', 'richos/app/scripts/proof-run.py')
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / 'results.json'
            item = pr.Item('native-ios-share', str(ROOT), ['scripts/run-tests.sh', '--results-out', str(path)])
            item.rc, item.state = 2, 'failed'
            path.write_text(json.dumps({'suites': [{'name': 'native-ios-share.test.sh', 'state': 'gap'}]}))
            pr.not_run(item)
            self.assertEqual(item.state, 'not-run')
            item.rc, item.state = 1, 'failed'
            pr.not_run(item)
            self.assertEqual(item.state, 'failed')

    def test_failed_only_retry_keeps_missing_cases_and_verifies_combined_coverage(self):
        with tempfile.TemporaryDirectory() as base:
            old, new = Path(base) / 'old', Path(base) / 'new'
            old.mkdir(); new.mkdir()
            for work in (old, new):
                (work / 'build-stamp.txt').write_text('source-stamp')
                (work / 'expected.txt').write_text('')
                (work / 'result-0.xcresult').mkdir()
            (new / 'tests-0.json').write_text(json.dumps({'values': [{'enabledTests': [
                {'identifier': 'RichOSNativeUITests/C/' + name} for name in ('pass', 'fail', 'missing')]}]}))
            (new / 'test-0.rc').write_text('0')
            old_rows = [('RichOSNativeUITests/C/pass', 'Passed', 1, ''),
                        ('RichOSNativeUITests/C/fail', 'Failed', 1, '')]
            new_rows = [('RichOSNativeUITests/C/fail', 'Passed', 1, ''),
                        ('RichOSNativeUITests/C/missing', 'Passed', 1, '')]
            def results(bundle, runner=None):
                rows = old_rows if str(bundle).startswith(str(old)) else new_rows
                ios_ui_shards.result_tests.kinds = {'RichOSNativeUITests': 'UI test bundle'}
                return {'passed': sum(r[1] == 'Passed' for r in rows), 'failed': sum(r[1] == 'Failed' for r in rows),
                        'skipped': 0, 'total': len(rows)}, rows.copy()
            def stamps(bundle, out, runner=None):
                return {'C/' + name: 'source-stamp' for name in ('pass', 'fail', 'missing')}
            with mock.patch.object(ios_ui_shards, 'result_tests', side_effect=results), mock.patch.object(ios_ui_shards, 'stamps', side_effect=stamps):
                ios_ui_shards.cmd_retry(new, str(old), 0)
                args = (new / 'retry-0.args').read_text()
                self.assertNotIn('/pass', args)
                self.assertIn('/fail', args)
                self.assertIn('/missing', args)
                self.assertEqual(ios_ui_shards.cmd_verify(str(new), 1, str(new / 'times.tsv'), ['SE']), 0)
                # Damage to reused evidence refuses green rather than trusting the manifest.
                old_rows[0] = ('RichOSNativeUITests/C/pass', 'Failed', 1, '')
                self.assertEqual(ios_ui_shards.cmd_verify(str(new), 1, str(new / 'times.tsv'), ['SE']), 1)
                (new / 'test-0.lost').write_text('{}')
                self.assertEqual(ios_ui_shards.cmd_verify(str(new), 1, str(new / 'times.tsv'), ['SE']), 2)
                (new / 'test-0.lost').unlink()
                # Different inputs or lost leases cannot contribute passing cases.
                (old / 'build-stamp.txt').write_text('different-source')
                ios_ui_shards.cmd_retry(new, str(old), 0)
                self.assertIn('/pass', (new / 'retry-0.args').read_text())
                (old / 'build-stamp.txt').write_text('source-stamp')
                (old / 'test-0.lost').write_text('{}')
                ios_ui_shards.cmd_retry(new, str(old), 0)
                self.assertIn('/pass', (new / 'retry-0.args').read_text())

    def test_all_skipped_cases_are_not_run_and_missing_unit_source_is_conservative(self):
        self.assertIsNone(ios_ui_scope.select(ROOT, ['richos/mobile/native-ios/UnitTests/DeletedTests.swift']))
        with tempfile.TemporaryDirectory() as base:
            work = Path(base)
            (work / 'expected.txt').write_text('B/C/a\n')
            (work / 'test-0.rc').write_text('0')
            rows = [('B/C/a', 'Skipped', 0, 'needs physical device')]
            with mock.patch.object(ios_ui_shards, 'result_tests', return_value=(
                    {'passed': 0, 'failed': 0, 'skipped': 1, 'total': 1}, rows)):
                self.assertEqual(ios_ui_shards.cmd_verify(base, 1, str(work / 'times.tsv'), ['SE']), 2)

    def test_retry_identity_binds_untracked_source_and_execution_settings(self):
        with tempfile.TemporaryDirectory() as base:
            root = Path(base)
            scripts = root / 'richos/app/scripts'
            (scripts / 'lib').mkdir(parents=True)
            (scripts / 'native-ios-ui.test.sh').write_text('# run-tests: inputs source\n')
            (scripts / 'lib/ios_ui_shards.py').write_text('fixture')
            (root / 'source').mkdir()
            (root / 'source/untracked.swift').write_text('one')
            with mock.patch.object(subprocess, 'check_output', return_value='Xcode fixture'):
                identity = ios_ui_shards.retry_identity(root, 'ios', ['SE'], ['case'])
                with mock.patch.dict(os.environ, {'RICHOS_PROOF_RUN': 'new-attempt', 'RICHOS_TEST_DEVICE_RUN_ID': 'new-run',
                                                   'RICHOS_TEST_RESULTS_ROOT': 'new-results', 'RICHOS_AUTOCHECK_RETRY_REASON': 'diagnosed'}):
                    self.assertEqual(identity, ios_ui_shards.retry_identity(root, 'ios', ['SE'], ['case']))
                (root / 'source/untracked.swift').write_text('two')
                self.assertNotEqual(identity, ios_ui_shards.retry_identity(root, 'ios', ['SE'], ['case']))
                self.assertNotEqual(identity, ios_ui_shards.retry_identity(root, 'ios', ['Pro'], ['case']))

    def test_scr_weight_is_measured_and_under_the_merge_gates_cap(self):
        # Section SCR now only checks discovery and declared thresholds: 4.8 s measured alone at
        # 4e73fd89. Its old 676.8 s (a contended full suite, when it still ran the reaper's
        # suite) planned it past the merge gate's 600 s cap, and proof-run.py --cap never starts
        # a check planned past the cap, so a stale row here would leave it unrun at every merge.
        rows = (ROOT / 'richos/engine/scripts/lib/ci-unit-weights.tsv').read_text().splitlines()
        weights = [float(row.split('\t')[1]) for row in rows if row.startswith('scripts/hooks/contract-integrity.test.sh:SCR\t')]
        self.assertEqual(len(weights), 1, weights)
        self.assertTrue(0 < weights[0] < 600, weights)

    def test_the_scratch_reapers_suite_is_left_to_the_nightly_by_the_merge(self):
        # It took 2238 s in a land check on a 91% busy host (2026-10-18), past the 600 s cap.
        # Its header declares it, so the merge moves its unit to the nightly engine run.
        unit = 'scripts/scratch-reaper.test.sh'
        ac = load('merge_scope_autocheck_reaper', 'richos/app/scripts/autocheck/autocheck.py')
        repo = SimpleNamespace(top=ROOT)
        self.assertTrue(ac.mutation_unit(repo, unit))
        runner, flag = 'scripts/ci-shard.sh', '--only-units'
        line = f'cd richos/engine && bash {runner} {flag} {unit}'
        kept, moved = ac.for_the_nightly(repo, [line])
        self.assertEqual(kept, [])
        self.assertEqual([m['check'] for m in moved], ['engine ' + unit])

    def test_a_workspaces_change_starts_the_fourteen_point_suites_plain_run_in_the_merge_gate(self):
        # The merge of c2e47cfb7 changed workspaces.py. Its gate selected this suite, planned it at
        # the table's 2811.7 s (its run WITH the mutation pass the gate switches off), left it out
        # as over the 600 s cap, and nightly 20261014T170730Z-784b711e found C1.5 red in its plain run.
        unit = 'mega-lander/tests/workspace-spec-fourteen.test.sh'
        result = subprocess.run(['bash', str(TOOLS / 'proof-for.sh'), '--quiet', '--gate', '--paths',
                                 'richos/engine/mega-lander/workspaces.py'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = [line.strip() for line in result.stdout.splitlines() if line.strip().startswith('cd ')]
        self.assertTrue(any(line.endswith('--only-units ' + unit) for line in lines), lines)
        ac = load('merge_scope_autocheck', 'richos/app/scripts/autocheck/autocheck.py')
        kept, _moved = ac.for_the_nightly(SimpleNamespace(top=ROOT), lines)
        pr = load('merge_scope_fourteen', 'richos/app/scripts/proof-run.py')
        args = SimpleNamespace(deadline=1800, engine_shards=4, fail_fast=False, cap=ac.CHECK_CAP_SECONDS)
        with tempfile.TemporaryDirectory() as log, mock.patch.dict(os.environ, ac.MUTATION_SWITCH):
            items = pr.plan(kept, args, log, {})
            pr.leave_over_cap(items, args)
        found = [item for item in items if item.label == 'engine ' + unit]
        self.assertEqual(len(found), 1, [item.label for item in items])
        self.assertEqual(found[0].state, 'waiting', getattr(found[0], 'not_run', None))


# A suite as the two that failed nightly attempt 2 only inside the build: it reads a variable
# only the build sets (merge-check-scope.test.py and RICHOS_IOS_POOL_WAIT), and it binds a Unix
# socket under a folder it made in TMPDIR (cargo-cache-env.test.sh and sccache's socket, which
# fit in macOS's 104 bytes only under a short TMPDIR). It prints why it fails, if it does.
PROBE = r'''
import os, shutil, socket, sys, tempfile
reasons = []
if os.environ.get("RICHOS_IOS_POOL_WAIT"):
    reasons.append("reads RICHOS_IOS_POOL_WAIT=" + os.environ["RICHOS_IOS_POOL_WAIT"])
work = os.path.realpath(tempfile.mkdtemp(prefix="probe."))
sock = socket.socket(socket.AF_UNIX)
try:
    sock.bind(os.path.join(work, "s" * 40 + ".sock"))
except OSError as exc:
    reasons.append("socket under TMPDIR=" + os.environ.get("TMPDIR", "") + ": " + str(exc))
finally:
    sock.close()
    shutil.rmtree(work, ignore_errors=True)
print("; ".join(reasons))
sys.exit(1 if reasons else 0)
'''


class NightlyConditions(unittest.TestCase):
    """A suite meets in the merge gate's run what it meets in the nightly (2026-10-01).

    Nightly attempt 2 failed two suites that had passed for their engineers, because the
    build hands its suites a TMPDIR and variables an engineer's shell does not. The merge
    gate and every proof run go through proof-run.py, so its check of a suite the nightly
    runs must see what the nightly's script-suites gate would hand that suite: measured here
    by running nightly-local.py's real gates() with a recording owned_run, from a caller
    whose TMPDIR is short and who sets none of the build's variables (an agent's shell).
    """

    SUITE = 'merge-check-scope.test.sh'
    APP_DIR = ROOT / 'richos' / 'app'

    def nightly_view(self, nl):
        # gate_environment() is the build's own gate environment without credentials; a
        # nightly-local.py older than it (the replay against main) builds it in
        # local_environment(), the only other place it ever was.
        base = (nl.gate_environment(os.environ) if hasattr(nl, 'gate_environment')
                else nl.local_environment()[0])
        seen = {}

        def record(args, **kwargs):
            if any(str(a).endswith('run-tests.sh') for a in args):
                seen['env'] = kwargs['env']
            return subprocess.CompletedProcess(args, 0, '', '')

        with tempfile.TemporaryDirectory() as root, \
                mock.patch.object(nl, 'owned_run', side_effect=record), \
                mock.patch('sys.stdout', new_callable=io.StringIO):
            runner = nl.Runner(Path(root), Path(root) / 'state', dict(base), io.StringIO(), {},
                               simulated_phones=1)
            runner.gates()
        self.assertIn('env', seen, 'the nightly script-suites gate started no suite runner')
        return seen['env']

    def merge_view(self, pr):
        agent = {k: v for k, v in os.environ.items()
                 if not k.startswith(('RICHOS_IOS_POOL_', 'RUN_TESTS_', 'RICHOS_NIGHTLY_'))}
        agent['TMPDIR'] = '/tmp/'
        runner_argv = [str(Path('scripts') / 'run-tests.sh'), '--only', self.SUITE]
        with mock.patch.dict(os.environ, agent, clear=True):
            item = pr.Item(self.SUITE[:-len('.test.sh')], str(self.APP_DIR), runner_argv)
            return pr.execution_environment(item)

    def probe(self, env):
        done = subprocess.run([sys.executable, '-c', PROBE], env=env, capture_output=True,
                              text=True, timeout=60)
        return done.returncode, done.stdout.strip()

    def test_a_suite_that_fails_only_in_the_nightly_fails_in_the_merge_gates_run(self):
        nl = load('merge_scope_nightly', 'richos/app/scripts/nightly-local.py')
        pr = load('merge_scope_conditions', 'richos/app/scripts/proof-run.py')
        nightly, merge = self.nightly_view(nl), self.merge_view(pr)
        in_nightly, in_merge = self.probe(nightly), self.probe(merge)
        # Not vacuous: under the nightly's own environment the probe is red.
        self.assertEqual(in_nightly[0], 1, in_nightly)
        self.assertEqual(in_merge, in_nightly,
                         'the merge gate would pass a suite the nightly fails: nightly says %r, '
                         'merge gate says %r' % (in_nightly[1], in_merge[1] or 'passed'))

    def test_every_value_the_build_sets_reaches_the_check_with_the_builds_value(self):
        nl = load('merge_scope_nightly_values', 'richos/app/scripts/nightly-local.py')
        pr = load('merge_scope_conditions_values', 'richos/app/scripts/proof-run.py')
        nightly, merge = self.nightly_view(nl), self.merge_view(pr)
        excused = set(getattr(nl, 'CONDITIONS_NOT_REPRODUCED', {}))
        # Values that are per build or the operator's own number by design: present, not equal.
        per_build = {'RICHOS_NIGHTLY_RUN_ID', nl.SIMULATED_PHONES_ENV}
        from_shell = set(nl.GATE_PASSTHROUGH) - set(nl.GATE_SET_BY_BUILD) - {'TMPDIR'}
        differ = {name: (nightly[name], merge.get(name)) for name in sorted(set(nightly) - from_shell - excused)
                  if name not in per_build and merge.get(name) != nightly[name]}
        self.assertEqual(differ, {}, 'name: (nightly value, merge-gate value)')
        self.assertEqual(sorted(n for n in per_build if n in nightly and n not in merge), [])


if __name__ == '__main__':
    unittest.main()

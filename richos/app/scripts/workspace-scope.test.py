#!/usr/bin/env python3
"""workspace-scope.test.py — a runner started in a teammate workspace runs what the branch touches.

CEO, 2026-10-01: an engineer iterating on one iPhone layout test ran native-ios-ui.test.sh with
no arguments seven times, the whole UI suite on two simulators each time (828 s), about 100
minutes against about 2 for the one case on one device. The narrow run is now the default in a
teammate workspace (a cc/ branch in a linked worktree), `--full` is everything, and the merge
gate, proof-run.py and the nightlies (RICHOS_TEST_SCOPE=full) are never narrowed.

Every case builds its own git repository in a temporary folder: a `main` checkout and a linked
worktree on a `cc/` branch, carrying copies of the real scripts and the real iPhone test files.
Nothing here builds, boots a simulator or touches the operator's checkouts.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE / 'lib'))
import ios_ui_scope  # noqa: E402
import android_ui_scope  # noqa: E402

NATIVE = 'richos/mobile/native-ios/'
ANDROID_TESTS = 'richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/'
SE = 'iPhone SE (3rd generation)'
# Hermetic: the gate's own run-tests settings (RUN_TESTS_DECLARED_GAPS names suites the fixture
# does not have, which run-tests.sh rightly calls stale and exits 2) must not reach a fixture run.
CLEAN = {k: v for k, v in os.environ.items() if k != 'RICHOS_TEST_SCOPE' and not k.startswith('RUN_TESTS_')}
# What the stub proof-for.sh prints: the harness line naming b.test.sh, as proof-for.sh writes it.
HARNESS = 'scripts/' + 'run-tests.sh'
MAPS_TO_B = '  cd richos/app && %s --only b.test.sh' % HARNESS


def git(cwd, *args):
    out = subprocess.run(['git', '-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                          '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', *args],
                         cwd=cwd, capture_output=True, text=True)
    if out.returncode != 0:
        raise AssertionError('git %s: %s' % (' '.join(args), out.stderr))
    return out.stdout.strip()


class Fixture:
    """A repository with `main` checked out and a linked worktree on cc/fixture-narrow."""

    def __init__(self, tmp, copies, extra=None):
        self.main = Path(tmp) / 'main'
        self.main.mkdir()
        git(self.main, 'init', '-q', '-b', 'main')
        for rel in copies:
            src = ROOT / rel
            dst = self.main / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                shutil.copy2(src, dst)
        for rel, text in (extra or {}).items():
            dst = self.main / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_text(text)
            dst.chmod(0o755)
        git(self.main, 'add', '-A')
        git(self.main, 'commit', '-q', '-m', 'fixture main')
        self.wt = Path(tmp) / 'wt'
        git(self.main, 'worktree', 'add', '-q', '-b', 'cc/fixture-narrow', str(self.wt), 'main')

    def edit(self, rel, transform, commit=False):
        path = self.wt / rel
        path.write_text(transform(path.read_text()))
        if commit:
            git(self.wt, 'add', '-A')
            git(self.wt, 'commit', '-q', '-m', 'fixture change')


IOS_COPIES = ['richos/app/scripts/native-ios-ui.test.sh', 'richos/app/scripts/lib/ios_ui_scope.py',
              'richos/app/scripts/lib/ios_ui_shards.py', 'richos/engine/scripts/lib/workspace_scope.py',
              NATIVE + 'UITests', NATIVE + 'UnitTests']
IOS_EXTRA = {NATIVE + 'App/Features/Settings/Overlays.swift': '// fixture\n',
             NATIVE + 'Core/Sources/RichOSCore/Fixture.swift': '// fixture\n',
             'README.md': 'fixture\n'}


def inside(case, suffix=' // fixture edit'):
    """A transform that edits the line after `case`'s declaration (inside its body)."""
    def go(text):
        first, _last = ios_ui_scope.case_spans(text)[case]
        lines = text.split('\n')
        lines[first] += suffix  # 0-based index `first` is the 1-based line after the declaration
        return '\n'.join(lines)
    return go


class IPhoneSelection(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix='workspace-scope-ios.')
        self.fx = Fixture(self.tmp, IOS_COPIES, IOS_EXTRA)
        self.suite = str(self.fx.wt / 'richos/app/scripts/native-ios-ui.test.sh')

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def plan(self, **kw):
        return ios_ui_scope.workspace_plan(str(self.fx.wt), self.suite, environ=kw.pop('environ', {}), **kw)

    def test_i1_a_changed_case_runs_alone_on_the_iphone_se(self):
        self.fx.edit(NATIVE + 'UITests/InteractionTests.swift', inside('testSettingsOpensAndForgetAsksFirst'))
        p = self.plan()
        self.assertEqual(p['scope'], 'narrow')
        self.assertEqual(p['only'], ['InteractionTests/testSettingsOpensAndForgetAsksFirst'])
        self.assertEqual(p['device'], 'se')
        self.assertIn('1 case(s) on the %s only' % SE, p['line'])
        self.assertIn('native-ios-ui.test.sh --full', p['line'])

    def test_i2_a_case_the_branch_adds_and_commits_runs_alone(self):
        def add(text):
            at = text.rindex('}')
            return text[:at] + '    func testFixtureAddedCase() {\n        XCTAssertTrue(true)\n    }\n' + text[at:]
        self.fx.edit(NATIVE + 'UITests/QuestionTests.swift', add, commit=True)
        self.assertEqual(self.plan()['only'], ['QuestionTests/testFixtureAddedCase'])

    def test_i3_a_line_outside_every_case_selects_the_whole_file(self):
        self.fx.edit(NATIVE + 'UITests/QuestionTests.swift', lambda t: t.replace(
            'final class QuestionTests: XCTestCase {', 'final class QuestionTests: XCTestCase { // edit', 1))
        self.assertEqual(self.plan()['only'], ['QuestionTests/testOtherAnswerInLightTheme',
                                               'QuestionTests/testTapAndOfflineEditLeaveTheComposerAvailable'])

    def test_i4_a_feature_file_selects_the_cases_the_gate_map_claims_for_it(self):
        self.fx.edit(NATIVE + 'App/Features/Settings/Overlays.swift', lambda t: t + '// edit\n')
        only = self.plan()['only']
        self.assertIn('InteractionTests/testSettingsOpensAndForgetAsksFirst', only)
        self.assertNotIn('InteractionTests/testHoldAndReleaseSendsAVoiceMessage', only)

    def test_i5_shared_code_runs_every_case_but_still_on_one_device(self):
        self.fx.edit(NATIVE + 'Core/Sources/RichOSCore/Fixture.swift', lambda t: t + '// edit\n')
        p = self.plan()
        self.assertEqual((p['scope'], p['only'], p['device']), ('narrow', [], 'se'))
        self.assertIn('every case on the %s only' % SE, p['line'])

    def test_i6_a_branch_that_maps_to_no_case_is_refused_never_an_empty_run(self):
        self.fx.edit('README.md', lambda t: t + 'edit\n')
        p = self.plan()
        self.assertEqual(p['scope'], 'refuse')
        self.assertIn('REFUSED', p['line'])
        self.assertIn('bash scripts/native-ios-ui.test.sh --full', p['line'])

    def test_i7_full_the_gate_and_the_main_checkout_are_never_narrowed(self):
        self.fx.edit(NATIVE + 'UITests/QuestionTests.swift', inside('testOtherAnswerInLightTheme'))
        self.assertEqual(self.plan(full=True)['scope'], 'full')
        self.assertEqual(self.plan(environ={'RICHOS_TEST_SCOPE': 'full'})['scope'], 'full')
        main = ios_ui_scope.workspace_plan(str(self.fx.main),
                                           str(self.fx.main / 'richos/app/scripts/native-ios-ui.test.sh'), environ={})
        self.assertEqual(main, {'scope': 'full', 'line': '', 'only': [], 'device': ''})
        with self.assertRaises(ValueError):
            self.plan(environ={'RICHOS_TEST_SCOPE': 'nrrow'})

    def test_i8_what_you_chose_is_kept_and_only_the_rest_is_narrowed(self):
        p = self.plan(have_only=True)
        self.assertEqual((p['scope'], p['only'], p['device']), ('narrow', [], 'se'))
        self.assertEqual(self.plan(have_only=True, have_device=True)['scope'], 'full')

    def shell(self, *flags, env=None):
        """The real native-ios-ui.test.sh scoping block, run against this fixture."""
        text = (HERE / 'native-ios-ui.test.sh').read_text()
        start = text.index('# The workspace default (CEO, 2026-10-01: an engineer ran this')
        end = text.index('WORK="$(mktemp -d "$CACHE/run.XXXXXX")"')
        harness = Path(self.tmp) / 'h' / 'native-ios-ui.test.sh'
        harness.parent.mkdir(exist_ok=True)
        harness.write_text(
            'set -euo pipefail\nDIR=%s\nROOT=%s\nMODE=all\nONLY=()\n'
            'DEVICES=("iPhone SE (3rd generation)" "iPhone 16 Pro Max")\nFULL=%s\nHAVE_DEVICE=\n%s\n'
            'printf "DEVICES=%%s\\n" "${DEVICES[*]}"\nprintf "ONLY=%%s\\n" "${ONLY[*]-}"\n'
            % (self.fx.wt / 'richos/app/scripts', self.fx.wt, '1' if '--full' in flags else '', text[start:end]))
        return subprocess.run(['bash', str(harness)], capture_output=True, text=True, env=env or CLEAN)

    def test_i9_the_runner_itself_narrows_with_no_arguments_and_not_with_full(self):
        self.fx.edit(NATIVE + 'UITests/InteractionTests.swift', inside('testSettingsOpensAndForgetAsksFirst'))
        r = self.shell()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn('DEVICES=%s\n' % SE, r.stdout)
        self.assertIn('ONLY=-only-testing:RichOSNativeUITests/InteractionTests/testSettingsOpensAndForgetAsksFirst()\n',
                      r.stdout)
        self.assertEqual(r.stdout.count('native-ios-ui: workspace run on cc/fixture-narrow'), 1)
        full = self.shell('--full')
        self.assertIn('DEVICES=%s iPhone 16 Pro Max\n' % SE, full.stdout)
        self.assertIn('ONLY=\n', full.stdout)
        gate = self.shell(env={**CLEAN, 'RICHOS_TEST_SCOPE': 'full'})
        self.assertEqual(gate.stdout, 'DEVICES=%s iPhone 16 Pro Max\nONLY=\n' % SE)

    def test_i10_the_runner_refuses_with_the_full_command_when_nothing_maps(self):
        self.fx.edit('README.md', lambda t: t + 'edit\n')
        r = self.shell()
        self.assertEqual(r.returncode, 64)
        self.assertIn('REFUSED', r.stdout)
        self.assertIn('--full', r.stdout)
        self.assertNotIn('DEVICES=', r.stdout)


class CaseSpans(unittest.TestCase):
    def test_every_case_of_every_real_ui_test_file_has_a_span(self):
        for name, cases in ios_ui_scope.inventory(ROOT).items():
            spans = ios_ui_scope.case_spans((ROOT / NATIVE / 'UITests' / name).read_text())
            self.assertIsNotNone(spans, name)
            self.assertEqual(sorted(c.split('/')[1] for c in cases), sorted(spans), name)

    def test_strings_and_comments_do_not_move_the_braces(self):
        text = ('final class T: XCTestCase {\n    // above\n    func testA() {\n        let s = "{"\n'
                '        // }\n    }\n    func helper() {}\n    func testB() { x("}") }\n}\n')
        self.assertEqual(ios_ui_scope.case_spans(text), {'testA': (2, 6), 'testB': (8, 8)})


def android_patterns():
    text = (HERE / 'native-android-ui.test.sh').read_text()
    block = text[text.index('PURE=('):text.index('# The workspace default (CEO, 2026-10-01): with no selection')]
    return re.findall(r"--tests '([^']+)'", block)


class AndroidSelection(unittest.TestCase):
    def test_a1_a_changed_test_class_and_a_sibling_source_select_those_classes(self):
        pats = android_patterns()
        self.assertIn('dev.richos.android.ui.ScreensTest', pats)
        got, every = android_ui_scope.select(ROOT, [ANDROID_TESTS + 'ui/conversation/QuestionCardTest.kt'], pats)
        self.assertEqual((got, every), (['dev.richos.android.ui.conversation.QuestionCardTest'], []))
        source = 'richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/composer/DraftEditor.kt'
        self.assertEqual(android_ui_scope.select(ROOT, [source], pats)[0], ['dev.richos.android.ui.composer.DraftEditorTest'])

    def test_a2_a_helper_selects_everything_and_a_class_the_suite_never_runs_selects_nothing(self):
        pats = android_patterns()
        got, every = android_ui_scope.select(ROOT, [ANDROID_TESTS + 'ui/ScreenChecks.kt'], pats)
        self.assertIsNone(got)
        self.assertEqual(every, [ANDROID_TESTS + 'ui/ScreenChecks.kt'])
        self.assertEqual(android_ui_scope.select(ROOT, [ANDROID_TESTS + 'ui/SpeedRestorationTest.kt'], pats), ([], []))

    def test_a3_the_runner_narrows_in_a_workspace_refuses_on_nothing_and_full_is_everything(self):
        tmp = tempfile.mkdtemp(prefix='workspace-scope-android.')
        try:
            fx = Fixture(tmp, ['richos/app/scripts/native-android-ui.test.sh', 'richos/app/scripts/lib/android_ui_scope.py',
                               'richos/engine/scripts/lib/workspace_scope.py',
                               ANDROID_TESTS + 'ui/conversation/QuestionCardTest.kt'], {'README.md': 'fixture\n'})
            text = (HERE / 'native-android-ui.test.sh').read_text()
            block = text[text.index('PURE=('):text.index('MACHINE_LIB=')]
            harness = Path(tmp) / 'h' / 'native-android-ui.test.sh'
            harness.parent.mkdir()
            harness.write_text('set -uo pipefail\nDIR=%s\nROOT=%s\n%s\nprintf "NARROW=%%s\\n" "${NARROW[*]-}"\n'
                               % (fx.wt / 'richos/app/scripts', fx.wt, block))

            def run(*args, env=CLEAN):
                return subprocess.run(['bash', str(harness), *args], capture_output=True, text=True, env=env)
            fx.edit('README.md', lambda t: t + 'edit\n')
            refused = run()
            self.assertEqual(refused.returncode, 64, refused.stdout + refused.stderr)
            self.assertIn('--full', refused.stdout)
            fx.edit(ANDROID_TESTS + 'ui/conversation/QuestionCardTest.kt', lambda t: t + '// edit\n')
            narrow = run()
            self.assertIn('NARROW=--tests dev.richos.android.ui.conversation.QuestionCardTest\n', narrow.stdout)
            self.assertIn('1 test class(es): QuestionCardTest', narrow.stdout)
            self.assertIn('NARROW=\n', run('--full').stdout)
            self.assertIn('NARROW=\n', run(env={**CLEAN, 'RICHOS_TEST_SCOPE': 'full'}).stdout)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


class RunTestsHarness(unittest.TestCase):
    """run-tests.sh: no selection in a workspace runs the suites proof-for.sh maps the branch to."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix='workspace-scope-runtests.')
        scripts = 'richos/app/scripts/'
        extra = {scripts + 'a.test.sh': '#!/usr/bin/env bash\n# run-tests: no-host-screen: fixture\n'
                                        '[ "${RICHOS_TEST_SCOPE:-}" = "${EXPECT_SCOPE:-}" ] '
                                        '|| { echo "  FAIL  scope ${RICHOS_TEST_SCOPE:-unset}"; exit 1; }\n'
                                        'echo "  all 1 passed"\n',
                 scripts + 'b.test.sh': '#!/usr/bin/env bash\n# run-tests: no-host-screen: fixture\necho "  all 1 passed"\n',
                 scripts + 'proof-for.sh': '#!/usr/bin/env bash\n'
                                           '[ -z "${PROOF_FOR_STUB_OUT:-}" ] || printf "%s\\n" "$PROOF_FOR_STUB_OUT"\n'
                                           'exit "${PROOF_FOR_STUB_RC:-0}"\n'}
        self.fx = Fixture(self.tmp, [scripts + f for f in ('run-tests.sh', 'lib/worktree-resource.sh', 'lib/test_results.py',
                                                          'lib/proof_declarations.py', 'lib/cargo-cache-env.sh',
                                                          'lib/cargo_identity.py', 'bin/cargo')]
                          + ['richos/engine/scripts/lib/workspace_scope.py'], extra)
        self.fx.edit(scripts + 'b.test.sh', lambda t: t + '# edit\n')

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_tests(self, where, *args, **env):
        return subprocess.run(['bash', str(where / 'richos/app/scripts/run-tests.sh'), *args], capture_output=True,
                              text=True, env={**CLEAN, 'RUN_TESTS_STATE': str(Path(self.tmp) / 'state'),
                                              'RUN_TESTS_RESULTS_STATE': str(Path(self.tmp) / 'results'), **env})

    def test_r1_no_selection_in_a_workspace_lists_only_the_mapped_suites(self):
        r = self.run_tests(self.fx.wt, '--list', PROOF_FOR_STUB_OUT=MAPS_TO_B)
        self.assertEqual((r.returncode, r.stdout), (0, 'b.test.sh\n'), r.stderr)
        self.assertIn('workspace run on cc/fixture-narrow', r.stderr)
        self.assertIn('%s --full' % HARNESS, r.stderr)

    def test_r2_full_the_gate_explicit_suites_and_the_main_checkout_are_unchanged(self):
        both = 'a.test.sh\nb.test.sh\n'
        self.assertEqual(self.run_tests(self.fx.wt, '--full', '--list', PROOF_FOR_STUB_OUT=MAPS_TO_B).stdout, both)
        self.assertEqual(self.run_tests(self.fx.wt, '--list', PROOF_FOR_STUB_OUT=MAPS_TO_B,
                                        RICHOS_TEST_SCOPE='full').stdout, both)
        self.assertEqual(self.run_tests(self.fx.wt, '--only', 'a.test.sh', '--list',
                                        PROOF_FOR_STUB_OUT=MAPS_TO_B).stdout, 'a.test.sh\n')
        self.assertEqual(self.run_tests(self.fx.main, '--list', PROOF_FOR_STUB_OUT=MAPS_TO_B).stdout, both)

    def test_r3_a_branch_mapping_to_no_suite_is_refused_with_the_full_command(self):
        r = self.run_tests(self.fx.wt, '--list')
        self.assertEqual(r.returncode, 2)
        self.assertIn('REFUSED', r.stderr)
        self.assertIn('%s --full' % HARNESS, r.stderr)
        broken = self.run_tests(self.fx.wt, '--list', PROOF_FOR_STUB_RC='2')
        self.assertEqual(broken.returncode, 2)
        self.assertIn('could not map the branch (exit 2)', broken.stderr)

    def test_r4_full_tells_every_suite_it_starts_to_run_whole(self):
        r = self.run_tests(self.fx.wt, '--full', '--only', 'a.test.sh', EXPECT_SCOPE='full')
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)

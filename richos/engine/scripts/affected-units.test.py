#!/usr/bin/env python3
"""Planning fixtures execute no selected suite or hook command."""
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
from affected_units import (DEPENDENCY_MAP, FULL_INPUT_CHECK, GLOBAL_CONFIG, PIN_CHECK, SECTIONED,
                            Selection, digest_only_change, validate_config, validate_pins)
from verification_inputs import Unsupported


class Planner(unittest.TestCase):
    def test_a_newly_registered_hook_script_a_suite_names_is_not_called_unnamed(self):
        # 2026-10-01: hooks() visits every script hooks.json registers, so a branch that changed
        # hooks.json AND the script it registers had the script's reasons on record before the
        # script's own turn; the "did this call add a reason" test then called it named by NO suite.
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suite = 'scripts/hooks/fixture-owner.test.sh'
        script = 'scripts/hooks/new-fixture-hook.sh'
        def read(path):
            if path == suite:
                return '#!/usr/bin/env bash\nbash "$ENGINE/scripts/hooks/new-fixture-hook.sh"\n'
            file = root / path
            return file.read_text() if file.is_file() else None
        before = read('hooks/hooks.json')
        after = json.loads(before)
        after['hooks']['FutureEvent'] = [{'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/' + script}]}]
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        plan.ordinary(script)
        self.assertIn(suite, plan.selected)
        self.assertEqual(plan.unmapped, [])
        # And a script no suite names is still reported, registered or not.
        lone = Selection(root, [suite], read, document)
        lone.hooks(before, json.dumps(after))
        lone.ordinary('scripts/hooks/zz-nobody-names-this.sh')
        self.assertEqual(lone.unmapped, ['scripts/hooks/zz-nobody-names-this.sh'])

    def test_an_unnamed_script_reached_by_two_roads_is_listed_once(self):
        # 2026-10-01: the refusal at main bb112ab68 listed notice-unanswered-question.sh twice,
        # once from hooks() (hooks.json registers it) and once from its own turn.
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suite = 'scripts/hooks/fixture-owner.test.sh'
        lone = 'scripts/hooks/zz-nobody-names-this.sh'
        def read(path):
            if path == suite:
                return '#!/usr/bin/env bash\necho owner\n'
            file = root / path
            return file.read_text() if file.is_file() else None
        before = read('hooks/hooks.json')
        after = json.loads(before)
        after['hooks']['FutureEvent'] = [{'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/' + lone}]}]
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        plan.ordinary(lone)
        self.assertEqual(plan.unmapped, [lone])

    def test_scoped_contract_inventory_keeps_new_events_and_helper_changes(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suite = 'scripts/hooks/contract-integrity.test.sh'
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        before = read('hooks/hooks.json')
        after = json.loads(before)
        after['hooks']['FutureEvent'] = [{'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/new-fixture-hook.sh'}]}]
        plan = Selection(root, [suite], read, document)
        units = set(plan.units)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), set(units))
        after = json.loads(before)
        after['description'] = 'description only'
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(plan.selected, {})
        document['hook_readers'][suite]['sources']['scripts/lib/registered-hooks.sh'] = 'changed'
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), set(units))

    def test_staging_fixture_registration_tracks_its_command_and_matcher(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suite = 'scripts/hooks/stale-staging.test.sh'
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        before = (root / 'hooks/hooks.json').read_text()
        after = json.loads(before)
        group = next(group for group in after['hooks']['PreToolUse']
                     if group.get('matcher') == 'Agent' and any(
                         'guard-stale-staging.sh' in hook.get('command', '') for hook in group['hooks']))
        group['matcher'] = 'Bash'
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), {suite})
        after = json.loads(before)
        after['hooks']['Stop'].append({'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/new-hook.sh'}]})
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(plan.selected, {})

    def test_python_audience_wrapper_keeps_complete_pretooluse_inventory(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suite = 'scripts/lib/spawn-guard-audience.test.sh'
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        for path in ('spawn-guard-audience.declaration',
                     'scripts/lib/spawn-guard-audience.py',
                     'scripts/lib/spawn-guard-audience.test.py'):
            plan = Selection(root, [suite], read, document)
            plan.ordinary(path)
            self.assertEqual(set(plan.selected), {suite}, path)
        before = (root / 'hooks/hooks.json').read_text()
        for event, expected in (('PreToolUse', {suite}), ('Stop', set())):
            after = json.loads(before)
            after['hooks'][event].append({'matcher': 'Agent', 'hooks': [
                {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/new-hook.sh'}]})
            plan = Selection(root, [suite], read, document)
            plan.hooks(before, json.dumps(after))
            self.assertEqual(set(plan.selected), expected)
        document['hook_readers'][suite]['sources']['scripts/lib/spawn-guard-audience.test.py'] = 'changed'
        plan = Selection(root, [suite], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), {suite})

    def test_private_todo_and_removal_suites_keep_adoption_and_source_obligations(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suites = ['scripts/hooks/guard-worktree-removal.test.sh',
                  'scripts/cold-open.test.sh', 'scripts/hooks/ceo-todos.test.sh', GLOBAL_CONFIG]
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        plan = Selection(root, suites, read, document)
        plan.configuration('PROTECTED_PATHS=old', 'PROTECTED_PATHS=new')
        self.assertEqual(set(plan.selected), {GLOBAL_CONFIG})
        plan = Selection(root, suites, read, document)
        plan.configuration(None, 'PROTECTED_PATHS=new')
        self.assertEqual(set(plan.selected), {GLOBAL_CONFIG, suites[2]})
        for source, unit in (('scripts/hooks/guard-worktree-removal.sh', suites[0]),
                             ('scripts/cold-open.sh', suites[1]),
                             ('scripts/lib/ceo-todos.py', suites[2])):
            plan = Selection(root, suites, read, document)
            plan.ordinary(source)
            self.assertIn(unit, plan.selected)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="affected-inputs.")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sources = {}
        self.map = {"schema": 1, "config_keys": ["A", "B"], "nodes": {}, "units": {}}
        self.node(GLOBAL_CONFIG, "validate_config\n", whole="global validation")

    def node(self, path, text, **fields):
        self.sources[path] = text
        self.map["nodes"][path] = {"source": path, "sha256": hashlib.sha256(text.encode()).hexdigest(),
                                   "evidence": "fixture input contract", "keys": [], "edges": [], **fields}
        if path.endswith(".test.sh"):
            self.map["units"][path] = path

    def selection(self):
        return Selection(self.root, sorted(self.map["units"]), self.sources.get, self.map)

    def test_changed_key_selects_exact_transitive_readers_and_global_validator(self):
        self.node("helper.sh", 'printf "%s" "$A"\n', keys=["A"])
        self.node("reader.test.sh", "bash helper.sh\n", edges=[{"to": "helper.sh"}])
        self.node("other.test.sh", 'printf "%s" "$B"\n', keys=["B"])
        plan = self.selection()
        plan.configuration("A=one\nB=two\n", "A=changed\nB=two\n")
        self.assertEqual(set(plan.selected), {"reader.test.sh", GLOBAL_CONFIG})
        self.assertIn("changed key A", " ".join(plan.selected["reader.test.sh"]))
        plan = self.selection()
        plan.configuration("A=one\nB=two\n", "A=one\nB=two\nUNUSED=new\n")
        self.assertEqual(set(plan.selected), {GLOBAL_CONFIG})

    def test_unknown_reader_and_path_only_requests_explain_conservative_selection(self):
        self.node("reader.test.sh", 'printf "%s" "$A"\n', keys=["A"])
        self.node("unknown.test.sh", "bash missing.sh\n", edges=[{"to": "missing.sh"}])
        plan = self.selection()
        plan.configuration("A=one", "A=two")
        self.assertIn("unqualified reader missing.sh", " ".join(plan.selected["unknown.test.sh"]))
        plan = self.selection()
        plan.configuration(None, "A=one", unknown=True)
        self.assertIn("path-only request", " ".join(plan.selected["reader.test.sh"]))

    def test_missing_global_validator_cannot_emit_green_empty_plan(self):
        plan = Selection(self.root, [], self.sources.get, self.map)
        with self.assertRaisesRegex(Unsupported, "global config validation unit is missing"):
            plan.configuration("A=one", "A=two")

    def test_sections_keep_shared_helper_fallback_and_per_section_narrowing(self):
        self.sources[SECTIONED] = ('source shared.sh\nif _section A; then\nbash alpha.sh\nfi  # _section\n'
                                  'if _section B; then\nbash beta.sh\nfi  # _section\n')
        plan = Selection(self.root, [SECTIONED], self.sources.get, self.map)
        plan.ordinary("alpha.sh")
        self.assertEqual(set(plan.selected), {SECTIONED + ":A"})
        plan = Selection(self.root, [SECTIONED], self.sources.get, self.map)
        plan.ordinary("shared.sh")
        self.assertEqual(set(plan.selected), {SECTIONED + ":A", SECTIONED + ":B"})
        self.assertTrue(all("ALL sections" in " ".join(reasons) for reasons in plan.selected.values()))

    def test_global_validation_rejects_unknown_keys_and_executable_syntax(self):
        directory = self.root / "scripts/lib"
        directory.mkdir(parents=True)
        (directory / "verification-dependencies.json").write_text(json.dumps(self.map))
        config = self.root / "orchestration.config"
        config.write_text("A=ok\n")
        self.assertEqual(validate_config(self.root), 1)
        config.write_text("UNDECLARED=ok\n")
        with self.assertRaisesRegex(Unsupported, "unknown config keys"):
            validate_config(self.root)
        marker = self.root / "must-not-execute"
        config.write_text('A="$(touch %s)"\n' % marker)
        with self.assertRaises(Unsupported):
            validate_config(self.root)
        self.assertFalse(marker.exists())

    def hook_fixture(self):
        self.node('alpha.test.sh', 'bash alpha.sh\n')
        self.node('beta.test.sh', 'bash beta.sh\n')
        self.node('registration.test.sh', 'read hooks.json for alpha\n')
        self.node('inventory.test.sh', 'read every command in hooks.json\n')
        self.node('unrelated.test.sh', 'read hooks.json for gamma\n')
        self.node('helper.sh', 'read supplied manifest\n')
        self.map['hook_readers'] = {}
        for suite, command in (('registration.test.sh', 'alpha.sh'), ('unrelated.test.sh', 'gamma.sh')):
            self.map['hook_readers'][suite] = {
                'commands': [command], 'text_needles': [command], 'evidence': 'fixed registration fixture',
                'sources': {p: self.map['nodes'][p]['sha256'] for p in (suite, 'helper.sh')}}

    @staticmethod
    def hooks_document(commands, matcher='Write', event='PreToolUse'):
        return json.dumps({'hooks': {event: [{'matcher': matcher, 'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/' + command}
            for command in commands]}]}}, indent=2)

    def test_hook_replacement_selects_both_behaviors_and_relevant_registration(self):
        self.hook_fixture()
        plan = self.selection()
        plan.hooks(self.hooks_document(['alpha.sh']), self.hooks_document(['beta.sh']))
        self.assertEqual(set(plan.selected), {'alpha.test.sh', 'beta.test.sh',
                                             'registration.test.sh', 'inventory.test.sh'})

    def test_matcher_and_first_group_order_remain_obligations(self):
        self.hook_fixture()
        old = self.hooks_document(['alpha.sh'])
        plan = self.selection()
        plan.hooks(old, self.hooks_document(['alpha.sh'], matcher='Edit'))
        self.assertEqual(set(plan.selected), {'alpha.test.sh', 'registration.test.sh', 'inventory.test.sh'})
        self.map['hook_readers']['registration.test.sh']['events'] = ['PreToolUse']
        plan = self.selection()
        plan.hooks(self.hooks_document(['beta.sh']), self.hooks_document(['beta.sh'], matcher='AskUserQuestion'))
        self.assertIn('registration.test.sh', plan.selected)
        self.assertNotIn('unrelated.test.sh', plan.selected)

    def test_registration_text_and_helper_changes_cannot_be_hidden(self):
        self.hook_fixture()
        old = self.hooks_document(['beta.sh'])
        changed = json.loads(old)
        changed['description'] = 'alpha.sh is named here, which a grep also reads'
        plan = self.selection()
        plan.hooks(old, json.dumps(changed, indent=2))
        self.assertEqual(set(plan.selected), {'registration.test.sh', 'inventory.test.sh'})
        self.sources['helper.sh'] += 'changed behavior\n'
        plan = self.selection()
        plan.hooks(old, self.hooks_document(['beta.sh'], matcher='Edit'))
        self.assertIn('unrelated.test.sh', plan.selected)
        self.assertIn('changed hook reader', ' '.join(plan.selected['unrelated.test.sh']))

    def test_unknown_hook_command_or_structure_is_explicit_and_never_executes(self):
        self.hook_fixture()
        old = self.hooks_document(['alpha.sh'])
        marker = self.root / 'not-executed'
        for changed in ('{"hooks":[]}', self.hooks_document(['alpha.sh; touch ' + str(marker)])):
            plan = self.selection()
            plan.hooks(old, changed)
            self.assertEqual(set(plan.selected), set(plan.units))
            self.assertTrue(all('fallback' in ' '.join(reasons) for reasons in plan.selected.values()))
        self.assertFalse(marker.exists())

    def test_real_fixed_readers_exclude_unrelated_hook_without_losing_inventory(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suites = sorted(unit for unit, row in document['hook_readers'].items()
                        if not row.get('all_events')) + ['scripts/check-census.test.sh']
        old = (root / 'hooks/hooks.json').read_text()
        changed = json.loads(old)
        group = changed['hooks']['Stop'][-1]
        group['hooks'].append({'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/new-fixture-hook.sh'})
        plan = Selection(root, suites, lambda p: (root / p).read_text() if (root / p).is_file() else None, document)
        plan.hooks(old, json.dumps(changed, indent=2))
        # JSON escapes in the description remain outside these registration needles. The
        # hook is added to Stop, and stop-hook-visibility.test.sh has read every Stop
        # registration since 6ac2edc4 (events: ["Stop"]), so it is the one fixed reader
        # this change concerns; every other fixed reader stays out.
        self.assertEqual(set(plan.selected), {'scripts/check-census.test.sh',
                                              'scripts/hooks/stop-hook-visibility.test.sh'})
        self.assertIn('changed event ordering/inventory for Stop',
                      ' '.join(plan.selected['scripts/hooks/stop-hook-visibility.test.sh']))

    def test_changed_pinned_reader_selects_the_pin_check(self):
        # 2026-09-28: 33 sources pinned in verification-dependencies.json had gone stale on
        # main one land at a time, because a change to a pinned reader never selected the
        # suite that checks the pins. A land that changes one must run that suite.
        pin_check = PIN_CHECK
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suites = sorted(p.relative_to(root).as_posix() for p in root.rglob('*.test.sh') if p.is_file())
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        # A source only a hook reader pins selects it too (a synthetic row, in memory).
        document['hook_readers']['scripts/hooks/reader-only-probe.test.sh'] = {
            'evidence': 'fixture', 'commands': [],
            'sources': {'scripts/lib/reader-only-probe.sh': '0' * 64}}
        for path in ('scripts/lib/row-currency.sh', 'scripts/hooks/dispatch-pretooluse.manifest',
                     'scripts/lib/agent_hold.py', 'scripts/lib/reader-only-probe.sh'):
            with self.subTest(path=path):
                plan = Selection(root, suites, read, document)
                plan.ordinary(path)
                self.assertIn(pin_check, plan.selected)
                self.assertIn('pinned reader', ' '.join(plan.selected[pin_check]))
        # A file the map does not pin selects no pin check, and stays unmapped.
        plan = Selection(root, suites, read, document)
        plan.ordinary('scripts/lib/not-a-pinned-probe.sh')
        self.assertNotIn(pin_check, plan.selected)
        self.assertIn('scripts/lib/not-a-pinned-probe.sh', plan.unmapped)
        # The pin check proves the pin, not the reader: a pinned executable no suite names
        # is still reported unmapped.
        document['nodes']['scripts/lib/pinned-orphan-probe.sh'] = {
            'source': 'scripts/lib/pinned-orphan-probe.sh', 'sha256': '0' * 64,
            'evidence': 'fixture', 'keys': [], 'edges': []}
        plan = Selection(root, suites, read, document)
        plan.ordinary('scripts/lib/pinned-orphan-probe.sh')
        self.assertIn(pin_check, plan.selected)
        self.assertIn('scripts/lib/pinned-orphan-probe.sh', plan.unmapped)

    def test_session_start_inventory_keeps_new_events_and_reader_invalidation(self):
        root = HERE.parent
        document = json.loads((root / 'scripts/lib/verification-dependencies.json').read_text())
        unit = 'scripts/hooks/session-start-stdin.test.sh'
        sources = {}
        def read(path):
            file = root / path
            return sources.get(path, file.read_text() if file.is_file() else None)
        before = read('hooks/hooks.json')
        after = json.loads(before)
        after['hooks']['FutureEvent'] = [{'hooks': [
            {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/new-fixture-hook.sh'}]}]
        plan = Selection(root, [unit], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), {unit})
        self.assertIn('FutureEvent', ' '.join(plan.selected[unit]))
        after = json.loads(before)
        after['description'] = 'description only'
        plan = Selection(root, [unit], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(plan.selected, {})
        sources['scripts/lib/registered-hooks.sh'] = read('scripts/lib/registered-hooks.sh') + '\n# drift\n'
        plan = Selection(root, [unit], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertEqual(set(plan.selected), {unit})
        sources.clear()
        document['hook_readers'][unit]['all_events'] = 'yes'
        plan = Selection(root, [unit], read, document)
        plan.hooks(before, json.dumps(after))
        self.assertIn('inventory scope', ' '.join(plan.selected[unit]))

    def test_registration_fixture_skips_real_manifest_but_binds_its_readers(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        unit = 'scripts/hook-registration-completeness.test.sh'
        sources = {}
        def read(path):
            if path in sources:
                return sources[path]
            file = root / path
            return file.read_text() if file.is_file() else None
        suites = [unit, GLOBAL_CONFIG]
        plan = Selection(root, suites, read, document)
        plan.configuration('PROTECTED_PATHS=old', 'PROTECTED_PATHS=new')
        self.assertEqual(set(plan.selected), {GLOBAL_CONFIG})
        old = read('hooks/hooks.json')
        # Removing every real hook still follows the retained ordinary
        # basename rule for hooks named in this suite's rationale comments.
        plan = Selection(root, suites, read, document)
        plan.hooks(old, self.hooks_document(['scripts/hooks/new-fixture-hook.sh']))
        self.assertIn(unit, plan.selected)
        changed_document = json.loads(old)
        changed_document['hooks']['Stop'][-1]['hooks'].append({
            'type': 'command',
            'command': 'bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/new-fixture-hook.sh'})
        changed = json.dumps(changed_document)
        plan = Selection(root, suites, read, document)
        plan.hooks(old, changed)
        self.assertNotIn(unit, plan.selected)
        for helper in document['hook_readers'][unit]['sources']:
            with self.subTest(helper=helper):
                sources[helper] = read(helper) + '\n# changed reader\n'
                plan = Selection(root, suites, read, document)
                plan.hooks(old, changed)
                self.assertIn(unit, plan.selected)
                sources.clear()
        for helper in ('scripts/hook-registration-completeness.sh',
                       'scripts/hooks/guard-hook-registration-commits.sh'):
            plan = Selection(root, suites, read, document)
            plan.ordinary(helper)
            self.assertIn(unit, plan.selected)

    def test_real_fixture_suites_skip_config_changes_but_keep_source_changes(self):
        root = HERE.parent
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        suites = ['scripts/collect-worktree-artifacts.test.sh',
                  'scripts/hooks/turn-manifest.test.sh', GLOBAL_CONFIG]
        def read(path):
            file = root / path
            return file.read_text() if file.is_file() else None
        for before, after in (('SHOW_TURN_MANIFEST=1', 'SHOW_TURN_MANIFEST=0'),
                              ('ARTIFACT_MERGE_DIRS=a', 'ARTIFACT_MERGE_DIRS=b')):
            plan = Selection(root, suites, read, document)
            plan.configuration(before, after)
            self.assertEqual(set(plan.selected), {GLOBAL_CONFIG})
        for source, unit in (('scripts/collect-worktree-artifacts.sh', suites[0]),
                             ('scripts/hooks/turn-manifest.sh', suites[1]),
                             ('scripts/hooks/turn-manifest.py', suites[1])):
            plan = Selection(root, suites, read, document)
            plan.ordinary(source)
            self.assertIn(unit, plan.selected)


class SnapshotCLI(unittest.TestCase):
    def test_real_cli_distinguishes_commit_index_and_worktree_without_execution(self):
        with tempfile.TemporaryDirectory(prefix="selection-versions.") as directory:
            root = Path(directory)
            engine = root / "richos/engine"
            library = engine / "scripts/lib"
            library.mkdir(parents=True)
            env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
                   "GIT_AUTHOR_NAME": "fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
                   "GIT_COMMITTER_NAME": "fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid"}
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], env=env, text=True).strip()
            for name in ("affected_units.py", "verification_inputs.py"):
                shutil.copyfile(HERE / "lib" / name, library / name)
            shutil.copyfile(HERE / "ci-affected-units.sh", engine / "scripts/ci-affected-units.sh")
            document = {"schema": 1, "config_keys": ["A", "B"], "nodes": {}, "units": {}}
            for unit, keys in (("scripts/a.test.sh", ["A"]), ("scripts/b.test.sh", ["B"]), (GLOBAL_CONFIG, [])):
                text = 'echo must-not-execute > "' + str(root / 'executed') + '"\n'
                (engine / unit).write_text(text)
                document["units"][unit] = unit
                document["nodes"][unit] = {"source": unit, "sha256": hashlib.sha256(text.encode()).hexdigest(),
                                             "evidence": "fixture contract", "keys": keys, "edges": []}
            (library / "verification-dependencies.json").write_text(json.dumps(document))
            configuration = engine / "orchestration.config"
            configuration.write_text("A=old\nB=old\n")
            git("init", "-q"); git("add", "."); git("commit", "-qm", "base")
            configuration.write_text("A=committed\nB=old\n")
            git("add", "."); git("commit", "-qm", "changed A")
            configuration.write_text("A=committed\nB=staged\n")
            git("add", ".")
            configuration.write_text("A=working\nB=staged\n")
            for args, expected in ((["--range", "HEAD^..HEAD"], {"scripts/a.test.sh"}),
                                   (["--staged"], {"scripts/b.test.sh"}),
                                   (["--working", "--base", "HEAD"], {"scripts/a.test.sh", "scripts/b.test.sh"}),
                                   (["--paths", "richos/engine/orchestration.config", "--staged"], {"scripts/b.test.sh"})):
                with self.subTest(args=args):
                    result = subprocess.run(["bash", str(engine / "scripts/ci-affected-units.sh"), *args, "--explain"],
                                            env=env, capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(set(result.stdout.splitlines()), expected | {GLOBAL_CONFIG})
                    self.assertIn("changed key", result.stderr)
            self.assertFalse((root / "executed").exists())
            hooks = engine / 'hooks/hooks.json'
            hooks.parent.mkdir()
            hooks.write_text(Planner.hooks_document(['scripts/a.sh']))
            git('add', '.'); git('commit', '-qm', 'hook base')
            hooks.write_text(Planner.hooks_document(['scripts/b.sh']))
            git('add', '.'); git('commit', '-qm', 'hook replacement')
            hooks.write_text(Planner.hooks_document(['scripts/a.sh']))
            git('add', '.')
            hooks.write_text(Planner.hooks_document(['scripts/b.sh'], matcher='Edit'))
            for args, expected in ((['--range', 'HEAD^..HEAD'], {'scripts/a.test.sh', 'scripts/b.test.sh'}),
                                   (['--staged'], {'scripts/a.test.sh', 'scripts/b.test.sh'}),
                                   (['--working', '--base', 'HEAD'], {'scripts/b.test.sh'})):
                with self.subTest(hooks=args):
                    result = subprocess.run(['bash', str(engine / 'scripts/ci-affected-units.sh'), *args],
                                            env=env, capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(set(result.stdout.splitlines()), expected)
            self.assertFalse((root / 'executed').exists())

    def test_a_removed_file_is_never_named_by_no_suite_and_an_unclaimed_one_still_is(self):
        # 2026-10-01: four of the seven files refused at main bb112ab68 as "named by NO suite"
        # were absent from the tree the selector read, judged executable by their suffix alone.
        # A deletion leaves no code to prove (proof-for.sh's REMOVED rule); a file that exists,
        # or never existed, and that no suite names is still refused.
        with tempfile.TemporaryDirectory(prefix="selection-removed.") as directory:
            root = Path(directory)
            engine = root / "richos/engine"
            library = engine / "scripts/lib"
            library.mkdir(parents=True)
            env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
                   "GIT_AUTHOR_NAME": "fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
                   "GIT_COMMITTER_NAME": "fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid"}
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], env=env, text=True).strip()
            for name in ("affected_units.py", "verification_inputs.py"):
                shutil.copyfile(HERE / "lib" / name, library / name)
            shutil.copyfile(HERE / "ci-affected-units.sh", engine / "scripts/ci-affected-units.sh")
            (library / "verification-dependencies.json").write_text(
                json.dumps({"schema": 1, "config_keys": [], "nodes": {}, "units": {}}))
            (engine / "scripts/owner.test.sh").write_text(
                'echo must-not-execute > "' + str(root / 'executed') + '"\n# reads scripts/lib/claimed.py\n')
            (library / "claimed.py").write_text("print('v1')\n")
            (library / "doomed.py").write_text("print('no suite names me')\n")
            git("init", "-q"); git("add", "."); git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (library / "doomed.py").unlink()
            (library / "claimed.py").write_text("print('v2')\n")
            git("add", "-A"); git("commit", "-qm", "delete doomed.py, change claimed.py")
            deleted = git("rev-parse", "HEAD")
            (library / "stray.py").write_text("print('no suite names me either')\n")
            git("add", "-A"); git("commit", "-qm", "add stray.py")
            tip = git("rev-parse", "HEAD")
            def select(*args):
                return subprocess.run(["bash", str(engine / "scripts/ci-affected-units.sh"), *args, "--strict"],
                                      env=env, capture_output=True, text=True, timeout=10)
            def unnamed(stderr):
                lines = stderr.split("named by NO suite:\n", 1)
                return [line.strip() for line in lines[1].splitlines() if line.startswith("    ")] if len(lines) == 2 else []
            for args in (["--range", base + ".." + deleted], ["--working", "--base", base]):
                with self.subTest(deletion=args[0]):
                    if args[0] == "--working":
                        git("checkout", "-q", deleted)
                    result = select(*args)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(set(result.stdout.splitlines()), {"scripts/owner.test.sh"})
                    self.assertEqual(unnamed(result.stderr), [])
                    self.assertIn("removed file(s)", result.stderr)
                    self.assertIn("scripts/lib/doomed.py", result.stderr)
            git("checkout", "-q", tip)
            with self.subTest("the branch's own unclaimed executable is still refused, and named alone"):
                result = select("--range", base + ".." + tip)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(unnamed(result.stderr), ["scripts/lib/stray.py"])
            for args in (["--paths", "richos/engine/scripts/lib/typo.py"],
                         ["--range", deleted + ".." + tip, "--paths", "richos/engine/scripts/lib/typo.py"]):
                with self.subTest(never_existed=args):
                    result = select(*args)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertEqual(unnamed(result.stderr), ["scripts/lib/typo.py"])
            self.assertFalse((root / 'executed').exists())


class PinRenewals(unittest.TestCase):
    def setUp(self):
        import copy
        self.copy = copy.deepcopy
        self.temporary = tempfile.TemporaryDirectory(prefix="pin-renewals.")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / 'repo/richos/engine'
        self.root.mkdir(parents=True)
        self.sources = {'reader.sh': 'echo fixture\n', 'owner.test.sh': 'bash reader.sh\n',
                        FULL_INPUT_CHECK: '# reads verification-dependencies.json\n',
                        PIN_CHECK: '# reads affected_units.py\n',
                        'observer.test.sh': '# reads verification-dependencies.json\n'}
        for path, text in self.sources.items():
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
        self.document = {'schema': 1, 'config_keys': ['A'], 'units': {}, 'nodes': {
            'reader': {'source': 'reader.sh', 'sha256': self.sha('reader.sh'),
                       'evidence': 'fixture', 'keys': [], 'edges': []}},
            'hook_readers': {'owner.test.sh': {'evidence': 'fixture', 'commands': [],
                'sources': {'owner.test.sh': self.sha('owner.test.sh')}}}}
        self.write_map()

    def sha(self, path):
        return hashlib.sha256((self.root / path).read_bytes()).hexdigest()

    def write_map(self):
        (self.root / DEPENDENCY_MAP).parent.mkdir(parents=True, exist_ok=True)
        (self.root / DEPENDENCY_MAP).write_text(json.dumps(self.document))

    def plan(self, before, after, fast=True):
        suites = [FULL_INPUT_CHECK, 'owner.test.sh', 'observer.test.sh'] + ([PIN_CHECK] if fast else [])
        selection = Selection(self.root, suites, self.sources.get, self.document)
        selection.ordinary('reader.sh')
        selection.ordinary(DEPENDENCY_MAP, before, after)
        return selection

    def test_renewal_keeps_owning_and_other_map_readers_but_replaces_full_pin_suite(self):
        before = json.dumps(self.document)
        changed = self.copy(self.document)
        changed['nodes']['reader']['sha256'] = 'a' * 64
        after = json.dumps(changed)
        selection = self.plan(before, after)
        self.assertEqual(set(selection.selected), {PIN_CHECK, 'owner.test.sh', 'observer.test.sh'})
        # Old inventories, and requests without a base snapshot, retain full coverage.
        self.assertIn(FULL_INPUT_CHECK, self.plan(before, after, fast=False).selected)
        self.assertIn(FULL_INPUT_CHECK, self.plan(None, after).selected)
        # The pin obligation cannot claim an executable with no behavior owner.
        self.sources['owner.test.sh'] = '# inert owner\n'
        self.assertIn('reader.sh', self.plan(before, after).unmapped)

    def test_only_existing_node_external_and_hook_digests_can_be_excused(self):
        external = {'root': 'repository', 'path': 'helper.py', 'sha256': 'b' * 64,
                    'evidence': 'fixture helper'}
        self.document['nodes']['reader']['external'] = [external]
        before = json.dumps(self.document)
        changed = self.copy(self.document)
        changed['nodes']['reader']['sha256'] = 'a' * 64
        changed['nodes']['reader']['external'][0]['sha256'] = 'c' * 64
        changed['hook_readers']['owner.test.sh']['sources']['owner.test.sh'] = 'd' * 64
        self.assertTrue(digest_only_change(before, json.dumps(changed)))
        edits = [lambda d: d['nodes']['reader'].update(keys=['A']),
                 lambda d: d['nodes']['reader'].update(edges=[{'to': 'unknown'}]),
                 lambda d: d['nodes']['reader'].update(evidence='new review'),
                 lambda d: d['nodes']['reader'].update(source='different.sh'),
                 lambda d: d['nodes']['reader'].update(source_inventory={'directory': 'new', 'members': []}),
                 lambda d: d['nodes']['reader']['external'][0].update(path='different.py'),
                 lambda d: d['nodes'].update(new=d['nodes']['reader']),
                 lambda d: d['hook_readers']['owner.test.sh'].update(commands=['new.sh']),
                 lambda d: d['hook_readers']['owner.test.sh']['sources'].update(new='e' * 64),
                 lambda d: d.update(schema=2)]
        for edit in edits:
            with self.subTest(edit=edit):
                semantic = self.copy(changed)
                edit(semantic)
                after = json.dumps(semantic)
                self.assertFalse(digest_only_change(before, after))
                self.assertIn(FULL_INPUT_CHECK, self.plan(before, after).selected)
        for invalid in (None, '{broken', before.replace('"schema": 1', '"schema": 1, "schema": 1'),
                        before.replace(self.sha('reader.sh'), 'not-a-digest')):
            self.assertFalse(digest_only_change(before, invalid))

    def test_spawn_arguments_do_not_depend_on_quota_configuration(self):
        from verification_inputs import Dependencies
        engine = HERE.parent
        document = json.loads((engine / DEPENDENCY_MAP).read_text())
        graph = Dependencies(engine, document)
        unit = 'scripts/spawn-effort.test.sh'
        self.assertEqual(graph.config_units(dict(content=False, keys=[], presence=False, fallback=[]), [unit]), {})
        self.assertEqual(graph.config_units(dict(content=True, keys=['QUOTA_PAUSE_PERCENT'],
                                                presence=False, fallback=[]), [unit]), {})

    def test_fast_validator_refuses_dangling_dependency_targets(self):
        self.document['nodes']['reader']['edges'] = [{'to': 'reader#missing'}]
        self.write_map()
        with self.assertRaisesRegex(Unsupported, 'reader#missing'):
            validate_pins(self.root)
        self.document['nodes']['reader']['edges'] = [{'to': 'reader'}]
        self.write_map()
        self.assertEqual(validate_pins(self.root), (1, 1))


    def test_fast_validator_refuses_stale_missing_and_omitted_key_or_execute_inputs(self):
        self.assertEqual(validate_pins(self.root), (1, 1))
        (self.root / 'reader.sh').write_text('echo changed\n')
        with self.assertRaisesRegex(Unsupported, 'changed reader'):
            validate_pins(self.root)
        self.document['nodes']['reader']['sha256'] = self.sha('reader.sh')
        self.write_map()
        self.assertEqual(validate_pins(self.root), (1, 1))
        for content, why in [('echo "$A"\n', 'omitted known key'),
                             ('bash "$ENGINE_ROOT/helper.sh"\n', 'omitted known execute')]:
            (self.root / 'helper.sh').write_text('echo helper\n')
            (self.root / 'reader.sh').write_text(content)
            self.document['nodes']['reader']['sha256'] = self.sha('reader.sh')
            self.write_map()
            with self.assertRaisesRegex(Unsupported, why):
                validate_pins(self.root)
        (self.root / 'reader.sh').unlink()
        with self.assertRaisesRegex(Unsupported, 'missing dependency'):
            validate_pins(self.root)

    def test_cli_uses_each_commit_index_and_working_map_before_narrowing(self):
        library = self.root / 'scripts/lib'
        for name in ('affected_units.py', 'verification_inputs.py'):
            shutil.copy2(HERE / 'lib' / name, library / name)
        for path, text in self.sources.items():
            (self.root / path).write_text(text)
        repository = self.root.parent.parent
        env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1',
               'GIT_AUTHOR_NAME': 'fixture', 'GIT_AUTHOR_EMAIL': 'fixture@example.invalid',
               'GIT_COMMITTER_NAME': 'fixture', 'GIT_COMMITTER_EMAIL': 'fixture@example.invalid'}
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repository), '-c',
                'core.hooksPath=/dev/null', *args], env=env, text=True).strip()
        def select(*args):
            out = subprocess.run([sys.executable, '-B', str(library / 'affected_units.py'),
                '--paths', 'richos/engine/reader.sh,richos/engine/' + DEPENDENCY_MAP, *args],
                env=env, capture_output=True, text=True)
            self.assertEqual(out.returncode, 0, out.stderr)
            return set(out.stdout.splitlines())
        git('init', '-q'); git('add', '.'); git('commit', '-q', '-m', 'base')
        base = git('rev-parse', 'HEAD')
        (self.root / 'reader.sh').write_text('echo v2\n')
        self.document['nodes']['reader']['sha256'] = self.sha('reader.sh')
        self.write_map()
        git('add', '.'); git('commit', '-q', '-m', 'renew reader')
        tip = git('rev-parse', 'HEAD')
        (self.root / 'reader.sh').write_text('echo staged\n')
        self.document['nodes']['reader']['sha256'] = self.sha('reader.sh')
        self.write_map()
        git('add', '.')
        self.document['nodes']['reader']['keys'] = ['A']
        self.write_map()
        for arguments in (('--range', base + '..' + tip), ('--staged',)):
            selected = select(*arguments)
            self.assertIn(PIN_CHECK, selected)
            self.assertNotIn(FULL_INPUT_CHECK, selected)
            self.assertIn('owner.test.sh', selected)
        for arguments in (('--working', '--base', base), ()):
            self.assertIn(FULL_INPUT_CHECK, select(*arguments))

    def test_fast_validator_binds_hook_external_and_directory_inventory(self):
        (self.root.parent.parent / 'helper.py').write_text('fixture\n')
        self.document['nodes']['reader']['external'] = [{'root': 'repository', 'path': 'helper.py',
            'sha256': hashlib.sha256(b'fixture\n').hexdigest(), 'evidence': 'fixture'}]
        (self.root / 'inventory').mkdir()
        (self.root / 'inventory/a').write_text('a')
        self.document['nodes']['reader']['source_inventory'] = {'directory': 'inventory', 'members': ['a']}
        self.write_map()
        self.assertEqual(validate_pins(self.root), (1, 1))
        (self.root.parent.parent / 'helper.py').write_text('changed\n')
        with self.assertRaisesRegex(Unsupported, 'changed external reader'):
            validate_pins(self.root)
        (self.root.parent.parent / 'helper.py').write_text('fixture\n')
        (self.root / 'inventory/b').write_text('b')
        with self.assertRaisesRegex(Unsupported, 'changed source inventory'):
            validate_pins(self.root)
        (self.root / 'inventory/b').unlink()
        (self.root / 'owner.test.sh').write_text('changed hook reader\n')
        with self.assertRaisesRegex(Unsupported, 'changed hook reader'):
            validate_pins(self.root)


if __name__ == "__main__":
    unittest.main()

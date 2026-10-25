#!/usr/bin/env python3
"""Semantic config and hook changes, including real repository grammar."""
import json
import hashlib
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import verification_inputs as inputs


class Inputs(unittest.TestCase):
    def test_settings_mode_preserves_strict_hook_shapes_and_duplicate_rejection(self):
        valid = '{"permissions":{},"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"dispatcher"}]}]}}'
        self.assertEqual(len(inputs.hook_entries(valid, settings=True)), 1)
        with self.assertRaises(inputs.Unsupported):
            inputs.hook_entries(valid)
        for invalid in (valid.replace('"permissions":{}', '"permissions":{},"permissions":{}'),
                        valid.replace('"hooks":{"PreToolUse":', '"hooks":{"PreToolUse":[],"PreToolUse":'),
                        valid.replace('"type":"command"', '"type":"prompt"'),
                        valid.replace('"command":"dispatcher"', '"command":"first","command":"dispatcher"')):
            with self.subTest(invalid=invalid), self.assertRaises(inputs.Unsupported):
                inputs.hook_entries(invalid, settings=True)

    def test_assignment_context_is_preserved_across_commit_index_and_worktree(self):
        with tempfile.TemporaryDirectory(prefix="config-versions.") as directory:
            root = Path(directory)
            env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
                "GIT_AUTHOR_NAME": "fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
                "GIT_COMMITTER_NAME": "fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid"}
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], env=env, text=True).strip()
            git("init", "-q")
            file = root / "orchestration.config"
            file.write_text("A=base\nB=stable\n")
            git("add", ".")
            git("commit", "-q", "-m", "fixture")
            original = inputs.Snapshot(root, "HEAD")
            file.write_text("A=staged\nB=stable\n")
            git("add", ".")
            file.write_text("A=working\nB=stable\nNEW=working\n")
            staged = inputs.Snapshot(root, "INDEX")
            working = inputs.Snapshot(root, "WORKTREE")
            self.assertEqual(inputs.config_change(original.read(file.name), staged.read(file.name))["keys"], ["A"])
            self.assertEqual(inputs.config_change(staged.read(file.name), working.read(file.name))["keys"], ["A", "NEW"])
            file.unlink()
            self.assertIsNone(working.read(file.name))
            self.assertIsNotNone(staged.read(file.name))
            self.assertIsNone(original.read("not-present.config"))
            self.assertEqual(inputs.revisions(root, "HEAD...HEAD"), (git("rev-parse", "HEAD"), "HEAD"))
            with self.assertRaises(inputs.Unsupported):
                inputs.Snapshot(root, "missing-ref")

    def test_real_config_accepts_all_seven_environment_path_assignments(self):
        parsed = inputs.config((HERE.parent / "orchestration.config").read_text())
        expected = {"SCRATCH_NIGHTLY_DIR", "SCRATCH_FAILURES_STATE",
                    "SCRATCH_REAPER_STATE", "APP_INSTANCE_FAILURES_STATE", "TEST_DEVICE_FAILURES_STATE",
                    "DISK_STATE_JSON", "SCRATCH_KEPT_DIR",
                    # The two Codex worktree keys (2026-10-23, cleanup PRD A6) take $HOME too.
                    "SCRATCH_CODEX_HOME", "SCRATCH_CODEX_WORKTREE_ROOTS"}
        actual = {key for key, row in parsed.items() if any(part[0] == "parameter" for part in row["value"])}
        self.assertEqual(actual, expected)
        # 105 since 0ba40e59 added CHECK_RESOURCE_WAITS and RESOURCE_WAIT_MINUTES;
        # 106 since SCRATCH_AGENT_ROOTS (2026-10-01, a land sweeps only its agent's scratch);
        # 108 since SCRATCH_KEPT_DIR and SCRATCH_KEPT_RETENTION_DAYS (2026-10-04, what a land kept);
        # 113 since SCRATCH_BUILD_CACHE_* and SCRATCH_CARGO_TARGET_ROOTS (2026-10-08, build caches).
        # 114 since SECOND_REVIEW_REPOS (2026-10-09, the repositories review-watch reviews by itself).
        # 110 since SCRATCH_BULK_TMP_ROOTS and SCRATCH_FIXTURE_REPO_LABELS (2026-10-23, cleanup PRD).
        # 112 since SCRATCH_CODEX_HOME and SCRATCH_CODEX_WORKTREE_ROOTS (2026-10-23, cleanup PRD A6).
        self.assertEqual(len(parsed), 112)
        self.assertIsNone(inputs.config_change("", (HERE.parent / "orchestration.config").read_text())["fallback"])

    def test_semantic_values_ignore_comments_spacing_and_equivalent_quotes(self):
        change = inputs.config_change("A=alpha\nB=\"literal # value\"\n",
                                      '  A="alpha"  # explanation\nB=\'literal # value\'\n\n# end\n')
        self.assertEqual(change["keys"], [])
        self.assertTrue(change["content"])  # Whole-content assertions still observe bytes.
        self.assertIsNone(change["fallback"])
        self.assertEqual(inputs.config_change('A=""\n', 'A= # empty\n')["keys"], [])

    def test_added_changed_deleted_and_transitively_referenced_keys(self):
        before = 'A=old\nB="$A/path"\nC="${B:-$HOME/default}"\nDELETE=yes\n'
        after = 'A=new\nB="$A/path"\nC="${B:-$HOME/default}"\nNEW=yes\n'
        self.assertEqual(inputs.config_change(before, after)["keys"], ["A", "B", "C", "DELETE", "NEW"])
        self.assertTrue(inputs.config_change(None, "A=one")["presence"])
        self.assertTrue(inputs.config_change("A=one", None)["presence"])

    def test_nested_defaults_and_environment_values_are_not_executed(self):
        text = 'A="${CLAUDE_CONFIG_DIR:-${HOME:-/Users/fallback}/.claude}/state"\n' # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        self.assertIsNone(inputs.config_change(text, text.replace("/state", "/next"))["fallback"])
        self.assertEqual(inputs.config_change('A="$HOME"', 'A=${HOME}')["keys"], [])
        self.assertEqual(inputs.config_change('A=\\$HOME', "A='$HOME'")["keys"], [])

    def test_unsupported_execution_dynamic_expansion_duplicates_and_multiline_are_visible(self):
        cases = ['A=$(touch nowhere)', 'A=`touch nowhere`', 'A=$((1+1))', 'A="${HOME%/}"',
                 'A="$UNDECLARED"', 'A="first\nsecond"', 'A=one\nA=two', 'A=ok; exit 0',
                 'if true; then\nA=one\nfi', 'A =one', 'A= echo unsafe', 'export A=one',
                 'A="$A"', 'A="$B"\nB=late']
        for text in cases:
            with self.subTest(text=text):
                result = inputs.config_change('A=one\n', text)
                self.assertTrue(result["fallback"])
                self.assertEqual(result["keys"], [])
        with tempfile.TemporaryDirectory(prefix="config-no-exec.") as directory:
            marker = Path(directory) / "executed"
            result = inputs.config_change("A=one", f'A="$(touch {marker})"')
            self.assertTrue(result["fallback"])
            self.assertFalse(marker.exists())
            self.assertIsNone(inputs.config_change("A=one", f"A='$(touch {marker})'")["fallback"])
            self.assertFalse(marker.exists())

    def test_real_hooks_file_and_metadata_only_change(self):
        original = (HERE.parent / "hooks/hooks.json").read_text()
        self.assertIsNone(inputs.hooks_change(original, original)["fallback"])
        for row in inputs.hook_entries(original).values():
            inputs.hook_command_path(row['command'])
        changed = json.loads(original)
        first_event = next(iter(changed["hooks"]))
        first = changed["hooks"][first_event][0]
        first["matcher"] = "ChangedMatcher"
        difference = inputs.hooks_change(original, json.dumps(changed))
        self.assertTrue(difference["metadata"])
        self.assertIn(first["hooks"][0]["command"], difference["commands"])
        self.assertEqual(difference['events'], [first_event])

    def test_hook_removed_added_order_timeout_and_empty_group_changes(self):
        def document(names):
            return {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": name, "timeout": 10}]} for name in names]}}
        before = document(["one", "two"])
        after = document(["two", "three"])
        difference = inputs.hooks_change(json.dumps(before), json.dumps(after))
        self.assertEqual(difference["commands"], ["one", "three", "two"])
        after = document(["two", "one"])
        self.assertEqual(inputs.hooks_change(json.dumps(before), json.dumps(after))["commands"], ["one", "two"])
        before = document(["one"])
        after = document(["one"])
        after["hooks"]["Stop"][0]["hooks"][0]["timeout"] = 20
        self.assertEqual(inputs.hooks_change(json.dumps(before), json.dumps(after))["commands"], ["one"])
        after = document(["one"])
        after["hooks"]["Stop"].append({"hooks": []})
        self.assertTrue(inputs.hooks_change(json.dumps(before), json.dumps(after))["metadata"])
        after = {**before, "description": "only explanatory prose changed"}
        difference = inputs.hooks_change(json.dumps(before), json.dumps(after))
        self.assertFalse(difference["metadata"])
        self.assertEqual(difference["commands"], [])

    def test_unknown_hook_structure_has_explicit_fallback(self):
        for text in ('[]', '{"hooks": []}', '{"hooks":{"Stop": [{"hooks": [{"type": "prompt"}]}]}}',
                     '{"hooks":{"Stop":[{"future-matcher": 1, "hooks":[]}]}}',
                     '{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x","async":"yes"}]}]}}'):
            with self.subTest(text=text):
                self.assertTrue(inputs.hooks_change('{"hooks":{}}', text)["fallback"])

    def test_ambiguous_hook_json_cannot_hide_an_entry_or_metadata_change(self):
        valid = '{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"one"}]}]}}'
        cases = [
            '{"hooks":{"Stop":[]},"hooks":{}}',
            '{"hooks":{"Stop":[],"Stop":[]}}',
            valid.replace('"hooks":[', '"matcher":"a","matcher":"b","hooks":['),
            valid.replace('"command":"one"', '"command":"two","command":"one"'),
        ]
        cases.extend(valid.replace('"command":"one"', '"command":"one","timeout":'+value)
                     for value in ('NaN', 'Infinity', '-Infinity', '1e999'))
        for text in cases:
            with self.subTest(text=text):
                self.assertTrue(inputs.hooks_change(valid, text)['fallback'])
                self.assertTrue(inputs.hooks_change(text, valid)['fallback'])


class Closure(unittest.TestCase):
    def test_shared_cycles_are_visited_once_without_losing_edge_semantics(self):
        document = {"schema":1, "config_keys":["KEEP", "MASK"], "nodes":{}, "units":{}}
        nodes = document["nodes"]
        nodes["root"] = dict(keys=[], edges=[dict(to="left", overrides=["MASK"]), dict(to="right", fixture=True)])
        nodes["left"] = dict(keys=[], edges=[dict(to="shared")])
        nodes["right"] = dict(keys=["MASK"], edges=[dict(to="shared"), dict(to="broken")])
        nodes["shared"] = dict(keys=["KEEP", "MASK"], edges=[dict(to="left")])
        graph = inputs.Dependencies(HERE.parent, document)
        calls = []
        def node(name):
            calls.append(name)
            if name=="broken":
                raise inputs.Unsupported("unqualified reader")
            return dict(nodes[name], evidence="witnessed "+name)
        graph.node = node
        got = graph.closure("root")
        self.assertEqual(set(got["keys"]), {"KEEP"})
        self.assertEqual(len(got["fallback"]),1)
        self.assertIn("root -> right -> unqualified reader",got["fallback"])
        self.assertEqual(len(calls),len(set(calls)))
        # Each later observation revalidates readers; no prior result is cached.
        nodes["shared"]["keys"] = []
        self.assertEqual(graph.closure("root")["keys"], {})

    def test_discovery_and_relocated_external_inputs_fail_closed(self):
        import copy
        import shutil
        with tempfile.TemporaryDirectory(prefix='input-boundaries-') as directory:
            repository = Path(directory) / 'repo'
            root = repository / 'richos/engine'
            fixtures = root / 'fixtures'
            fixtures.mkdir(parents=True)
            (fixtures / 'reader.py').write_text('pass\n')
            (fixtures / 'helper.py').write_text('pass\n')
            (repository / 'outside.py').write_text('pass\n')
            digest = hashlib.sha256(b'pass\n').hexdigest()
            document = {'schema': 1, 'config_keys': [], 'units': {'unit': 'reader'}, 'nodes': {'reader': {
                'source': 'fixtures/reader.py', 'sha256': digest, 'evidence': 'Closed fixture', 'keys': [],
                'source_inventory': {'directory': 'fixtures', 'members': ['reader.py', 'helper.py']},
                'external': [{'root': 'repository', 'path': 'outside.py', 'sha256': digest, 'evidence': 'Sibling helper'},
                             {'root': 'environment', 'variable': 'RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT',
                              'default_engine': True, 'path': 'fixtures/helper.py', 'sha256': digest,
                              'evidence': 'Alternate engine helper'}]}}}
            change = inputs.config_change('A=one', 'A=two')
            def selected(doc=document, at=root):
                return inputs.Dependencies(at, doc).config_units(change, ['unit'])
            with patch.dict(os.environ):
                os.environ.pop('RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT', None)
                self.assertFalse(selected())
                other = Path(directory) / 'relocated'
                shutil.copytree(repository, other)
                self.assertFalse(selected(at=other / 'richos/engine'))
                (other / 'outside.py').write_text('changed')
                self.assertTrue(selected(at=other / 'richos/engine'))
                self.assertFalse(selected())
                (repository / 'outside.py').unlink()
                self.assertTrue(selected())
                (repository / 'outside.py').write_text('pass\n')
                for name in ('test_new.py', 'package'):
                    target = fixtures / name
                    target.mkdir() if name == 'package' else target.write_text('pass\n')
                    self.assertTrue(selected())
                    target.rmdir() if name == 'package' else target.unlink()
                (fixtures / 'helper.py').unlink()
                self.assertTrue(selected())
                (fixtures / 'helper.py').write_text('pass\n')
                (fixtures / 'helper.py').unlink()
                (fixtures / 'helper.py').symlink_to(repository / 'outside.py')
                self.assertTrue(selected())
                (fixtures / 'helper.py').unlink()
                (fixtures / 'helper.py').write_text('pass\n')
                for override in ('', 'relative', str(Path(directory) / 'absent')):
                    os.environ['RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT'] = override
                    self.assertTrue(selected())
                os.environ['RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT'] = str(other / 'richos/engine')
                self.assertFalse(selected())
                (other / 'richos/engine/fixtures/helper.py').write_text('changed')
                self.assertTrue(selected())
                os.environ.pop('RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT')
                for invalid in ({}, {'directory': '../escape', 'members': []},
                                {'directory': 'fixtures', 'members': ['reader.py', 'reader.py']},
                                {'directory': 'fixtures', 'members': [None]},
                                {'directory': 'fixtures', 'members': ['../reader.py']}):
                    bad = copy.deepcopy(document)
                    bad['nodes']['reader']['source_inventory'] = invalid
                    self.assertTrue(selected(bad))
                bad = copy.deepcopy(document)
                bad['nodes']['reader']['external'][1]['default_engine'] = 'yes'
                self.assertTrue(selected(bad))

    def test_last_four_fixtures_keep_all_execution_inputs(self):
        from functools import lru_cache
        import copy
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        @lru_cache(None)
        def read(path): return (HERE.parent / path).read_text()
        units = ['scripts/lib/cpu_guard.test.sh', 'scripts/lib/verification-pressure.test.sh',
                 'ecs/tests/run.test.sh', 'scripts/hooks/lifecycle-payload-transport.test.sh']
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        with patch.dict(os.environ):
            os.environ.pop('RICHOS_LIFECYCLE_PAYLOAD_TEST_ROOT', None)
            for unit in units:
                with self.subTest(unit=unit):
                    graph = inputs.Dependencies(HERE.parent, document, read=read)
                    self.assertFalse(graph.closure(unit)['fallback'])
                    self.assertFalse(graph.config_units(change, [unit]))
                    # Check every newly declared reached source, plus source-bound
                    # existing boundaries. Prior helper controls remain applicable.
                    visited, sources = set(), set()
                    def walk(name):
                        if name in visited: return
                        visited.add(name)
                        row = document['nodes'][name]
                        sources.add(row['source'])
                        if name in ('scripts/lib/cpu_guard.py', 'scripts/lib/worker_tokens.py',
                                    'scripts/lib/proc_tree.py', 'ecs/core/ecs_core.py',
                                    'ecs/core/ecs_checkpoint.py', 'ecs/core/ecs_inspect.py'):
                            return
                        for edge in row.get('edges', []): walk(edge['to'])
                    walk(unit)
                    # One invalidation per source; exact-source and external-root
                    # mechanics have independent controls above.
                    for source in sources:
                        changed = lambda p: read(p) + '\n# changed\n' if p == source else read(p)
                        self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]), source)
            for name in ('scripts/lib/cpu_guard.py#policy-fixtures', 'scripts/lib/verification-pressure.test.py#runner',
                         'scripts/hooks/lifecycle-payload-transport.test.py'):
                bad = copy.deepcopy(document)
                bad['nodes'][name]['external'][-1]['sha256'] = 'changed'
                owner = units[0] if 'cpu_guard' in name else units[1] if 'pressure' in name else units[3]
                self.assertIn(owner, inputs.Dependencies(HERE.parent, bad, read=read).config_units(change, [owner]))
            for name in ('ecs/tests/run.test.sh', 'ecs/adapters/app.py#discovery-fixtures'):
                bad = copy.deepcopy(document)
                bad['nodes'][name]['source_inventory']['members'].append('unreviewed.py')
                self.assertIn(units[2], inputs.Dependencies(HERE.parent, bad, read=read).config_units(change, [units[2]]))

    def test_foreign_app_fixture_keeps_full_dispatch_chain(self):
        from functools import lru_cache
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        @lru_cache(None)
        def read(path): return (HERE.parent / path).read_text()
        unit = 'scripts/hooks/guard-foreign-app-data.test.sh'
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        self.assertFalse(graph.closure(unit)['fallback'])
        self.assertFalse(graph.config_units(change, [unit]))
        for before, after in ((None, 'CHECK_FAILURE_TYPE=1'), ('HOST_DISPLAY_POWER_GUARD=0', 'HOST_DISPLAY_POWER_GUARD=1')):
            self.assertIn(unit, graph.config_units(inputs.config_change(before, after), [unit]))
        names = [unit, 'scripts/hooks/foreign-app-data.mutation.sh', 'scripts/hooks/guard-foreign-app-data.sh',
                 'scripts/hooks/dispatch-pretooluse.sh#find-payload']
        names += [e['to'] for e in document['nodes'][names[-1]]['edges']]
        sources = {document['nodes'][n]['source'] for n in names}
        sources.add('scripts/lib/foreign_app_data.py')
        for source in sources:
            with self.subTest(source=source):
                changed = lambda p: read(p) + '\n# changed\n' if p == source else read(p)
                self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))

    def test_remaining_fixed_fixtures_exclude_unrelated_config(self):
        from functools import lru_cache
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        @lru_cache(None)
        def read(path):
            return (HERE.parent / path).read_text()
        units = ['scripts/lib/agent_hold.test.sh', 'scripts/lib/proc_tree_pause.test.sh',
                 'gates/unmanaged-assignment/lib/harness.test.sh', 'scripts/lib/ci_pause.test.sh',
                 'mega-lander/tests/workspace-pause-hold.test.sh', 'scripts/pause-acceptance.test.sh']
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        for unit in units:
            with self.subTest(unit=unit):
                graph = inputs.Dependencies(HERE.parent, document, read=read)
                self.assertFalse(graph.closure(unit)['fallback'])
                self.assertFalse(graph.config_units(change, [unit]))
                if unit in units[-2:]:
                    self.assertIn(unit, graph.config_units(inputs.config_change('ALLOWED_MODELS=a', 'ALLOWED_MODELS=b'), [unit]))
                # Every immediate new fixture/helper identity is tested here;
                # unchanged transitive helper controls remain applicable.
                sources = {document['nodes'][unit]['source']}
                sources.update(document['nodes'][e['to']]['source'] for e in document['nodes'][unit]['edges'])
                for source in sources:
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]), source)

    def test_desktop_work_fixture_preserves_real_keys_and_reader_invalidation(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        from functools import lru_cache
        @lru_cache(None)
        def read(path):
            return (HERE.parent / path).read_text()
        unit = 'mega-lander/tests/app.test.sh'
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        self.assertFalse(graph.closure(unit)['fallback'])
        self.assertFalse(graph.config_units(change, [unit]))
        for before, after in (('ALLOWED_MODELS=a', 'ALLOWED_MODELS=b'),
                              ('APP_TEST_INSTANCE_PROCESS_NAMES=a', 'APP_TEST_INSTANCE_PROCESS_NAMES=b'),
                              (None, 'CHECK_FAILURE_TYPE=1'), ('A=one', 'A=$(bad)')):
            self.assertIn(unit, graph.config_units(inputs.config_change(before, after), [unit]))
        # Prior qualified helper controls remain applicable. Check every newly
        # bound source and the immediate existing execution boundaries here.
        for source in ('mega-lander/app.py', 'mega-lander/tests/app.test.py',
                       'mega-lander/tests/app.test.sh', 'mega-lander/tests/app.mutation.sh',
                       'mega-lander/workspaces.py', 'ecs/adapters/app.py',
                       'scripts/lib/spawn.py', 'scripts/lib/pause_protocol.py',
                       'scripts/lib/mutation-harness.sh', 'ecs/core/ecs_core.py'):
            with self.subTest(source=source):
                changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))

    def test_creation_and_app_evidence_fixtures_keep_real_model_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        for unit in ('mega-lander/tests/create-teammate-worktree.test.sh', 'scripts/lib/app-evidence.test.sh'):
            self.assertFalse(graph.closure(unit)['fallback'], unit)
            self.assertFalse(graph.config_units(change, [unit]), unit)
            for before, after in (('ALLOWED_MODELS=a', 'ALLOWED_MODELS=b'),
                                  (None, 'CHECK_FAILURE_TYPE=1'), ('A=one', 'A=$(bad)')):
                self.assertIn(unit, graph.config_units(inputs.config_change(before, after), [unit]))
            seen = set()
            def visit(name):
                if name in seen:
                    return
                seen.add(name)
                for edge in document['nodes'][name].get('edges', []):
                    visit(edge['to'])
            visit(unit)
            for source in {document['nodes'][name]['source'] for name in seen}:
                with self.subTest(unit=unit, source=source):
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))

    def test_fixture_and_event_observers_preserve_readers_and_conservative_fallback(self):
        import copy
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        spawn = 'scripts/spawn.test.sh'
        resume = 'scripts/hooks/guard-resume-isolation.test.sh'
        ci = 'scripts/ci-verify.test.sh'
        settings = '.claude/settings.local.json'
        probe = 'scripts/hooks/contract-integrity-probe.sh'
        def selected(unit, path, before, after, declaration=document, reader=read):
            selection = Selection(HERE.parent, [unit], reader, declaration)
            selection.ordinary(path, before, after)
            return unit in selection.selected
        command = {'type': 'command', 'command': 'bash own.sh', 'timeout': 20}
        group = {'matcher': 'Agent', 'hooks': [command]}
        old = {'hooks': {'PreToolUse': [group]}}
        new = copy.deepcopy(old)
        new['hooks']['Stop'] = [{'hooks': [{'type': 'command', 'command': 'bash unrelated.sh'}]}]
        before, after = json.dumps(old), json.dumps(new)
        for unit in (spawn, resume):
            self.assertFalse(selected(unit, settings, before, after))
            for bad in (None, 'bad json', '{"hooks":{},"hooks":{}}', '{"hooks":{"Stop":{}}}'):
                self.assertTrue(selected(unit, settings, before, bad))
        for mutation in ('command', 'matcher', 'timeout', 'order', 'legacy', 'missing'):
            changed = copy.deepcopy(new)
            if mutation == 'command': changed['hooks']['PreToolUse'][0]['hooks'][0]['command'] = 'bash changed.sh'
            elif mutation == 'matcher': changed['hooks']['PreToolUse'][0]['matcher'] = 'Bash'
            elif mutation == 'timeout': changed['hooks']['PreToolUse'][0]['hooks'][0]['timeout'] = 99
            elif mutation == 'order': changed['hooks']['PreToolUse'].insert(0, {'hooks': []})
            elif mutation == 'legacy': changed['PreToolUse'] = [group]
            else: changed['hooks'].pop('PreToolUse')
            self.assertTrue(selected(spawn, settings, before, json.dumps(changed)), mutation)
            self.assertFalse(selected(resume, settings, before, json.dumps(changed)), mutation)
        before_probe = read(probe)
        anchor = '    BR_EXPECTED="\\\n'
        self.assertIn(anchor, before_probe)
        after_probe = before_probe.replace(anchor, anchor + 'unrelated-fixture.sh|Stop\n', 1)
        self.assertFalse(selected(ci, probe, before_probe, after_probe))
        self.assertTrue(selected(ci, probe, before_probe, after_probe + '\n# non-oracle change\n'))
        self.assertTrue(selected(ci, probe, before_probe, None))
        for unit, path, old_text, new_text in ((spawn, settings, before, after),
                                              (resume, settings, before, after),
                                              (ci, probe, before_probe, after_probe)):
            for source in document['hook_readers'][unit]['sources']:
                with self.subTest(unit=unit, source=source):
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertTrue(selected(unit, path, old_text, new_text, reader=changed))
            self.assertTrue(selected(unit, unit, None, None))
        for unit, field, path, old_text, new_text in (
                (spawn, 'seated_events', settings, before, after),
                (resume, 'seated_reads_nothing', settings, before, after),
                (ci, 'probe_registration_reads_nothing', probe, before_probe, after_probe)):
            for invalid in ([], '', 1, None):
                changed = copy.deepcopy(document)
                changed['hook_readers'][unit][field] = invalid
                self.assertTrue(selected(unit, path, old_text, new_text, declaration=changed))

    def test_loro_component_has_no_config_input_and_binds_every_reader(self):
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        unit = 'loro/tests/run.test.sh'
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        for kind in ('keys', 'whole', 'presence', 'fallback'):
            self.assertFalse(graph.closure(unit)[kind])
        for before, after in (('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1'),
                              (None, 'FAILURE_TYPE_REGISTER=x'), ('A=one', 'A=$(bad)')):
            self.assertFalse(graph.config_units(inputs.config_change(before, after), [unit]))
        self.assertIn('scripts/verification-config.test.sh', graph.config_units(
            inputs.config_change('A=one', 'A=$(bad)'), ['scripts/verification-config.test.sh']))
        sources = {unit} | {e['to'] for e in document['nodes'][unit]['edges']}
        for source in sources:
            with self.subTest(source=source):
                changed = lambda p: read(p) + '\nchanged reader\n' if p == source else read(p)
                self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(
                    inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1'), [unit]))
        selection = Selection(HERE.parent, [unit], read, document)
        selection.ordinary('loro/tests/run.js')
        self.assertIn(unit, selection.selected)

    def test_spawn_preparation_and_stop_fixtures_bind_complete_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        units = ('scripts/prepare-agent-spawn.test.sh', 'scripts/stop.test.sh')
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        for unit in units:
            self.assertFalse(graph.closure(unit)['fallback'], unit)
            self.assertFalse(graph.config_units(change, [unit]), unit)
            seen = set()
            def visit(name):
                if name in seen:
                    return
                seen.add(name)
                for edge in document['nodes'][name].get('edges', []):
                    visit(edge['to'])
            visit(unit)
            for source in {document['nodes'][name]['source'] for name in seen}:
                with self.subTest(unit=unit, changed_reader=source):
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))
        for kind in ('keys', 'whole', 'presence'):
            self.assertFalse(graph.closure(units[0])[kind])
        invalid = graph.config_units(inputs.config_change('A=one', 'A=$(bad)'), [*units, 'scripts/verification-config.test.sh'])
        self.assertNotIn(units[0], invalid)
        self.assertIn(units[1], invalid)
        self.assertIn('scripts/verification-config.test.sh', invalid)
        self.assertFalse(graph.config_units(inputs.config_change('CREATOR_TEAMMATE=a', 'CREATOR_TEAMMATE=b'), [units[0]]))
        self.assertIn('CREATOR_TEAMMATE', graph.closure('scripts/hooks/verify-agent-prompt.sh')['keys'])
        self.assertIn(units[1], graph.config_units(inputs.config_change(None, 'CHECK_FAILURE_TYPE=1'), [units[1]]))
        self.assertIn(units[1], graph.config_units(inputs.config_change('SCRATCH_DEFAULT_TTL_MINUTES=1', 'SCRATCH_DEFAULT_TTL_MINUTES=2'), [units[1]]))

    def test_app_and_device_fixtures_keep_real_keys_and_bind_all_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        for unit in ('scripts/lib/appinstances.test.sh', 'scripts/lib/testdevices.test.sh'):
            graph = inputs.Dependencies(HERE.parent, document, read=read)
            self.assertFalse(graph.closure(unit)['fallback'], unit)
            self.assertFalse(graph.config_units(change, [unit]), unit)
            self.assertIn(unit, graph.config_units(inputs.config_change('A=one', 'A=$(bad)'), [unit]))
            seen = set()
            def visit(name):
                if name in seen:
                    return
                seen.add(name)
                for edge in document['nodes'][name].get('edges', []):
                    visit(edge['to'])
            visit(unit)
            for source in {document['nodes'][name]['source'] for name in seen}:
                with self.subTest(unit=unit, changed_reader=source):
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))
        app = 'scripts/lib/appinstances.test.sh'
        device = 'scripts/lib/testdevices.test.sh'
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        self.assertIn(app, graph.config_units(inputs.config_change('APP_TEST_INSTANCE_PROCESS_NAMES=a', 'APP_TEST_INSTANCE_PROCESS_NAMES=b'), [app]))
        self.assertIn(device, graph.config_units(inputs.config_change(None, 'CHECK_FAILURE_TYPE=1'), [device]))

    def test_dialect_source_scan_keeps_unqualified_registration_selection(self):
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        unit = 'scripts/hooks/dialect-guard.test.sh'
        read = lambda path: (HERE.parent / path).read_text()
        selection = Selection(HERE.parent, [unit], read, document)
        selection.ordinary('.claude/settings.local.json', '{"hooks":{}}', '{"hooks":{},"extra":true}')
        self.assertIn(unit, selection.selected)
        selection = Selection(HERE.parent, [unit], read, document)
        selection.hooks('{"hooks":{}}', '{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"bash ${CLAUDE_PLUGIN_ROOT}/unrelated.sh"}]}]}}')
        self.assertIn(unit, selection.selected)

    def test_registration_observers_keep_own_dispatcher_probe_and_source_changes(self):
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        names = ('ceo-todos', 'row-currency', 'completeness-commits',
                 'named-persons', 'publication-boundary', 'guard-vendoring-commits', 'ceo-asks')
        def selected(unit, path, before, after, reader=read):
            selection = Selection(HERE.parent, [unit], reader, document)
            selection.ordinary(path, before, after)
            return selection.selected
        for name in names:
            unit = 'scripts/hooks/' + name + '.test.sh'
            row = document['hook_readers'][unit]
            if 'seated_text_needles' in row:
                path = '.claude/settings.local.json'
                guard = row['seated_text_needles'][0]
                doc = {'permissions': {}, 'hooks': {'PreToolUse': [{'hooks': [
                    {'type': 'command', 'command': '$CLAUDE_PROJECT_DIR/scripts/hooks/' + guard + '.sh'},
                    {'type': 'command', 'command': '$CLAUDE_PROJECT_DIR/scripts/hooks/dispatch-pretooluse.sh Bash'}]}]}}
                before = json.dumps(doc, indent=2)
                doc['hooks']['UserPromptSubmit'] = [{'hooks': [
                    {'type': 'command', 'command': '$CLAUDE_PROJECT_DIR/scripts/hooks/failure-type-lookup.sh'}]}]
                after = json.dumps(doc, indent=2)
                self.assertFalse(selected(unit, path, before, after), name)
                for changed in (after.replace(guard, 'removed'), after.replace('.sh Bash', '.sh Write'),
                                after.replace('"permissions": {}', '"permissions": {}, "permissions": {}'),
                                '{', None):
                    self.assertIn(unit, selected(unit, path, before, changed), (name, changed))
                self.assertIn(unit, selected(unit, path, None, after))
                for source in row['sources']:
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, selected(unit, path, before, after, changed), (name, source))
            if 'probe_registration_needles' in row:
                path = 'scripts/hooks/contract-integrity-probe.sh'
                guard = row['probe_registration_needles'][0]
                before = '    R_ROOTLESS_HOOKS="stable"\n    BR_EXPECTED="\\\n' + guard + '.sh|PreToolUse\nother.sh|Stop"\n# code\n'
                after = before.replace('other.sh|Stop', 'other.sh|Stop\nfailure-type-lookup.sh|UserPromptSubmit')
                self.assertFalse(selected(unit, path, before, after), name)
                for changed in (after.replace(guard + '.sh|PreToolUse', guard + '.sh|Stop'),
                                after.replace('R_ROOTLESS_HOOKS', 'RENAMED_ROOTLESS'),
                                after.replace('# code', '# changed code'),
                                after.replace('other.sh|Stop', '$(execute)|Stop'),
                                after.replace('BR_EXPECTED=', 'UNKNOWN='), after + after, None):
                    self.assertIn(unit, selected(unit, path, before, changed), (name, changed))
                self.assertIn(unit, selected(unit, path, None, after))
                for source in row['sources']:
                    changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                    self.assertIn(unit, selected(unit, path, before, after, changed), (name, source))
                unknown = json.loads(json.dumps(document))
                del unknown['hook_readers'][unit]['probe_registration_needles']
                selection = Selection(HERE.parent, [unit], read, unknown)
                selection.ordinary(path, before, after)
                self.assertIn(unit, selection.selected)

    def test_probe_reader_cli_uses_commit_index_and_working_bytes(self):
        with tempfile.TemporaryDirectory(prefix='probe-reader-cli.') as directory:
            root = Path(directory)
            library = root / 'scripts/lib'
            library.mkdir(parents=True)
            for name in ('affected_units.py', 'verification_inputs.py'):
                (library / name).write_bytes((HERE / 'lib' / name).read_bytes())
            unit = 'scripts/fixture.test.sh'
            source = '# observes contract-integrity-probe.sh for guard-fixture\n'
            (root / unit).write_text(source)
            probe = root / 'scripts/hooks/contract-integrity-probe.sh'
            probe.parent.mkdir()
            before = '    BR_EXPECTED="\\\nguard-fixture.sh|PreToolUse\nother.sh|Stop"\n'
            probe.write_text(before)
            document = {'schema': 1, 'config_keys': [], 'nodes': {}, 'units': {},
                        'hook_readers': {unit: {'commands': [], 'probe_registration_needles': ['guard-fixture'],
                         'sources': {unit: hashlib.sha256(source.encode()).hexdigest()},
                         'evidence': 'Fixture observes its own registration row.'}}}
            (library / 'verification-dependencies.json').write_text(json.dumps(document))
            env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_SYSTEM': os.devnull,
                   'GIT_AUTHOR_NAME': 'fixture', 'GIT_AUTHOR_EMAIL': 'fixture@example.invalid',
                   'GIT_COMMITTER_NAME': 'fixture', 'GIT_COMMITTER_EMAIL': 'fixture@example.invalid'}
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], env=env, text=True)
            def selection(*args):
                return subprocess.check_output([sys.executable, '-B', str(library / 'affected_units.py'),
                                                *args], env=env, text=True, stderr=subprocess.PIPE)
            git('init', '-q')
            git('add', '.')
            git('commit', '-q', '-m', 'fixture')
            probe.write_text(before.replace('other.sh|Stop', 'another.sh|Stop'))
            git('add', '.')
            probe.write_text(before.replace('guard-fixture', 'removed'))
            self.assertEqual(selection('--staged'), '')
            self.assertIn(unit, selection('--working'))
            self.assertIn(unit, selection('--paths', 'scripts/hooks/contract-integrity-probe.sh'))
            git('commit', '-q', '-m', 'unrelated registration')
            self.assertEqual(selection('--range', 'HEAD^..HEAD'), '')

    def test_handoff_facts_keeps_liveness_presence_but_no_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        unit = 'scripts/handoff-facts.test.sh'
        graph = inputs.Dependencies(HERE.parent, document, read=read)
        closure = graph.closure(unit)
        self.assertFalse(closure['fallback'] or closure['keys'] or closure['whole'])
        self.assertTrue(closure['presence'])
        change = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        self.assertFalse(graph.config_units(change, [unit]))
        for change_control in (inputs.config_change(None, 'CHECK_FAILURE_TYPE=1'),
                               inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=$(bad)')):
            self.assertIn(unit, graph.config_units(change_control, [unit]))
        seen = set()
        def visit(name):
            if name in seen:
                return
            seen.add(name)
            for edge in document['nodes'][name].get('edges', []):
                visit(edge['to'])
        visit(unit)
        for source in {document['nodes'][name]['source'] for name in seen}:
            with self.subTest(changed_reader=source):
                changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                self.assertIn(unit, inputs.Dependencies(HERE.parent, document, read=changed).config_units(change, [unit]))

    def test_owned_state_manifest_is_private_and_all_readers_are_bound(self):
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        unit = 'scripts/hooks/owned-state.test.sh'
        before = json.dumps({'hooks': {}})
        def manifest(path):
            return json.dumps({'hooks': {'UserPromptSubmit': [{'hooks': [
                {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/' + path}]}]}})
        after = manifest('scripts/hooks/failure-type-lookup.sh')
        selection = Selection(HERE.parent, [unit], read, document)
        selection.hooks(before, after)
        self.assertFalse(selection.selected)
        for source in document['hook_readers'][unit]['sources']:
            with self.subTest(changed_reader=source):
                changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                selection = Selection(HERE.parent, [unit], changed, document)
                selection.hooks(before, after)
                self.assertIn(unit, selection.selected)
        for changed in ('{', manifest('scripts/hooks/guard-owned-state.sh')):
            selection = Selection(HERE.parent, [unit], read, document)
            selection.hooks(before, changed)
            self.assertIn(unit, selection.selected)

    def test_seated_text_reader_retains_own_registration_and_source_controls(self):
        from affected_units import Selection
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        unit = 'scripts/hooks/stale-staging.test.sh'
        path = '.claude/settings.local.json'
        before = '{\n"guard": "guard-stale-staging.sh",\n"other": "old"\n}\n'
        after = before.replace('"old"', '"new"')
        def selected(old=before, new=after, source=read, declaration=document):
            selection = Selection(HERE.parent, [unit], source, declaration)
            selection.ordinary(path, old, new)
            return selection.selected
        self.assertFalse(selected())
        for old, new in ((None, after), (before, None), (before, '{'),
                         (before, before.replace('guard-stale-staging.sh', 'removed')),
                         (before, before.replace('guard-stale-staging.sh', 'guard-stale-stagingXsh'))):
            with self.subTest(old=old, new=new):
                self.assertIn(unit, selected(old, new))
        for source in document['hook_readers'][unit]['sources']:
            with self.subTest(changed_reader=source):
                changed = lambda p: read(p) + '\n# changed reader\n' if p == source else read(p)
                self.assertIn(unit, selected(source=changed))
        for needles in ([], '', [None], [''], ['bad\nneedle']):
            modified = json.loads(json.dumps(document))
            modified['hook_readers'][unit]['seated_text_needles'] = needles
            self.assertIn(unit, selected(declaration=modified))
        modified = json.loads(json.dumps(document))
        del modified['hook_readers'][unit]
        self.assertIn(unit, selected(declaration=modified))
        selection = Selection(HERE.parent, [unit], read, document)
        selection.ordinary(unit, before, after)
        self.assertIn(unit, selection.selected)

    def test_seated_reader_cli_uses_commit_index_and_working_bytes(self):
        with tempfile.TemporaryDirectory(prefix='seated-reader-cli.') as directory:
            root = Path(directory)
            library = root / 'scripts/lib'
            library.mkdir(parents=True)
            for name in ('affected_units.py', 'verification_inputs.py'):
                (library / name).write_bytes((HERE / 'lib' / name).read_bytes())
            unit = 'scripts/fixture.test.sh'
            source = '# observes settings.local.json for guard-stale-staging\n'
            (root / unit).write_text(source)
            settings = root / '.claude/settings.local.json'
            settings.parent.mkdir()
            before = '{\n"guard":"guard-stale-staging.sh",\n"other":1\n}\n'
            settings.write_text(before)
            document = {'schema': 1, 'config_keys': [], 'nodes': {}, 'units': {},
                        'hook_readers': {unit: {'commands': [], 'seated_text_needles': ['guard-stale-staging'],
                         'sources': {unit: hashlib.sha256(source.encode()).hexdigest()},
                         'evidence': 'Fixture observes its guard text only.'}}}
            (library / 'verification-dependencies.json').write_text(json.dumps(document))
            env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_SYSTEM': os.devnull,
                   'GIT_AUTHOR_NAME': 'fixture', 'GIT_AUTHOR_EMAIL': 'fixture@example.invalid',
                   'GIT_COMMITTER_NAME': 'fixture', 'GIT_COMMITTER_EMAIL': 'fixture@example.invalid'}
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], env=env, text=True)
            def selection(*args):
                return subprocess.check_output([sys.executable, '-B', str(library / 'affected_units.py'),
                                                *args], env=env, text=True, stderr=subprocess.PIPE)
            git('init', '-q')
            git('config', 'core.excludesFile', os.devnull)
            git('add', '.')
            git('commit', '-q', '-m', 'fixture')
            settings.write_text(before.replace('1', '2'))
            git('add', '.claude/settings.local.json')
            settings.write_text(before.replace('guard-stale-staging', 'removed'))
            self.assertEqual(selection('--staged'), '')
            self.assertIn(unit, selection('--working'))
            self.assertIn(unit, selection('--paths', '.claude/settings.local.json'))
            git('commit', '-q', '-m', 'unrelated setting')
            self.assertEqual(selection('--range', 'HEAD^..HEAD'), '')

    def test_workspace_behavior_suites_do_not_read_manifest_comments(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = json.dumps({'hooks': {}})
        def manifest(path):
            return json.dumps({'hooks': {'UserPromptSubmit': [{'hooks': [
                {'type': 'command', 'command': 'bash ${CLAUDE_PLUGIN_ROOT}/' + path}]}]}})
        after = manifest('scripts/hooks/failure-type-lookup.sh')
        for unit in ('mega-lander/tests/workspace-spec-fourteen.test.sh',
                     'mega-lander/tests/workspaces-e2e.test.sh'):
            reader = inputs.hook_reader(document['hook_readers'][unit], read)
            self.assertFalse(reader['commands'] or reader.get('events') or reader.get('all_events'))
            selection = Selection(HERE.parent, [unit], read, document)
            selection.hooks(before, after)
            self.assertFalse(selection.selected)
            selection.hooks(before, manifest('scripts/hooks/workspace-lifecycle.sh'))
            self.assertIn(unit, selection.selected)
            for source in reader['sources']:
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit], changed_read, document)
                    changed.hooks(before, after)
                    self.assertIn(unit, changed.selected)
            malformed = Selection(HERE.parent, [unit], read, document)
            malformed.hooks(before, '{unknown')
            self.assertIn(unit, malformed.selected)
            config = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            config.configuration('ALLOWED_MODELS=old\n', 'ALLOWED_MODELS=new\n')
            self.assertIn(unit, config.selected)

    def test_provenance_provider_and_run_records_exclude_live_config(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        units = ('scripts/hooks/notice-claim-capability.test.sh',
                 'ass-kicker/tests/brief-provenance.test.sh',
                 'scripts/lib/provider-supervisor.test.sh', 'scripts/ci-run-record-check.test.sh')
        for unit in units:
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
            transport = unit == 'scripts/lib/provider-supervisor.test.sh'
            self.assertEqual(bool(closure['presence']), transport, closure)
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertEqual(unit in graph.config_units(inputs.config_change(before, changed), [unit]), transport)
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_payload_resource_and_dictionary_fixtures_exclude_live_config(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        units = ('scripts/hooks/resource-waits.test.sh', 'scripts/hooks/shell-evidence.test.sh',
                 'scripts/hooks/long-payload.test.sh', 'scripts/lib/dialect/dialect-table.test.sh')
        for unit in units:
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertEqual(set(closure['keys']), {'SCRATCH_DEFAULT_TTL_MINUTES'} if unit == 'scripts/hooks/resource-waits.test.sh' else set(), closure)
            self.assertFalse(closure['whole'] or closure['fallback'], closure)
            transport = unit == 'scripts/hooks/resource-waits.test.sh'
            self.assertEqual(bool(closure['presence']), transport, closure)
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertEqual(unit in graph.config_units(inputs.config_change(before, changed), [unit]), transport)
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_bounded_component_entrypoints_exclude_live_config(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        units = ('ecs/tests/answer-complete.test.sh', 'ecs/tests/operator-complete.test.sh',
                 'voice/tests/run.test.sh')
        for unit in units:
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
            transport = unit == 'ecs/tests/operator-complete.test.sh'
            self.assertEqual(bool(closure['presence']), transport, closure)
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertEqual(unit in graph.config_units(inputs.config_change(before, changed), [unit]), transport)
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_completion_and_finish_record_fixtures_exclude_live_config(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        units = ('scripts/lib/completion-proof.test.sh', 'scripts/hooks/task-completed-handoff.test.sh',
                 'scripts/lib/finish-row-completion.test.sh', 'scripts/hooks/teammate-idle-handoff.test.sh')
        for unit in units:
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
            transport = unit == 'scripts/lib/finish-row-completion.test.sh'
            self.assertEqual(bool(closure['presence']), transport, closure)
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertEqual(unit in graph.config_units(inputs.config_change(before, changed), [unit]), transport)
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_landing_and_runner_fixtures_exclude_live_config(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        units = ('scripts/hooks/guard-ci-red-lands.test.sh',
                 'scripts/land-completeness.test.sh', 'scripts/lib/runner-boundaries.test.sh')
        for unit in units:
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
            transport = unit == 'scripts/land-completeness.test.sh'
            self.assertEqual(bool(closure['presence']), transport, closure)
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertEqual(unit in graph.config_units(inputs.config_change(before, changed), [unit]), transport)
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_brief_scope_and_ledger_fixtures_keep_config_transport_only(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        for unit in ('ass-kicker/tests/brief-scope.test.sh', 'scripts/lib/worktree-ledger.test.sh'):
            graph = inputs.Dependencies(HERE.parent, document)
            closure = graph.closure(unit)
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
            self.assertTrue(closure['presence'])
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
            for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
                self.assertIn(unit, graph.config_units(inputs.config_change(before, changed), [unit]))
            pending, seen, sources = [unit], set(), set()
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                row = document['nodes'][name]
                sources.add(row['source'])
                pending.extend(edge['to'] for edge in row.get('edges', []))
            for source in sorted(sources):
                with self.subTest(unit=unit, changed_source=source):
                    changed_read = lambda path: read(path) + ('\n# changed\n' if path == source else '')
                    changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                    changed.configuration(before, after)
                    self.assertIn(unit, changed.selected)
            selection.ordinary(unit)
            self.assertIn(unit, selection.selected)

    def test_ci_shard_synthetic_inventory_has_no_live_config_dependency(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        unit = 'scripts/ci-shard.test.sh'
        read = lambda path: (HERE.parent / path).read_text()
        graph = inputs.Dependencies(HERE.parent, document)
        self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        for config_after in (after, None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.configuration(before, config_after)
            self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
        pending, seen, sources = [unit], set(), set()
        while pending:
            name = pending.pop()
            if name in seen:
                continue
            seen.add(name)
            row = document['nodes'][name]
            sources.add(row['source'])
            pending.extend(edge['to'] for edge in row.get('edges', []))
        for source in sorted(sources):
            with self.subTest(changed_source=source):
                changed_read = lambda path: read(path) + ('\n# new reader\n' if path == source else '')
                selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                selection.configuration(before, after)
                self.assertIn(unit, selection.selected)
        for path in (unit, 'scripts/ci-shard.sh', 'scripts/lib/ci-receipts.py'):
            selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            selection.ordinary(path)
            self.assertIn(unit, selection.selected, path)

    def test_workspace_probes_ignores_config_values_but_keeps_transport_and_source_changes(self):
        from affected_units import Selection, GLOBAL_CONFIG
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        unit = 'mega-lander/tests/workspace-probes.test.sh'
        read = lambda path: (HERE.parent / path).read_text()
        before = 'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=old\n'
        after = 'CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=new\n'
        graph = inputs.Dependencies(HERE.parent, document)
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'], closure)
        selection = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
        selection.configuration(before, after)
        self.assertEqual(set(selection.selected), {GLOBAL_CONFIG})
        for changed in (None, 'CHECK_FAILURE_TYPE=$(unknown)\n'):
            self.assertIn(unit, graph.config_units(inputs.config_change(before, changed), [unit]))
        # Bind every transitive source, including helpers behind fixture masks.
        pending, seen, sources = [unit], set(), set()
        while pending:
            name = pending.pop()
            if name in seen:
                continue
            seen.add(name)
            row = document['nodes'][name]
            sources.add(row['source'])
            pending.extend(edge['to'] for edge in row.get('edges', []))
        for source in sorted(sources):
            with self.subTest(changed_source=source):
                changed_read = lambda path: read(path) + ('\n# changed reader\n' if path == source else '')
                changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], changed_read, document)
                changed.configuration(before, after)
                self.assertIn(unit, changed.selected)
        for path in (unit, 'mega-lander/workspace-probes.py', 'mega-lander/workspaces.py',
                     'mega-lander/tests/workspace-probes.mutation.sh'):
            changed = Selection(HERE.parent, [unit, GLOBAL_CONFIG], read, document)
            changed.ordinary(path)
            self.assertIn(unit, changed.selected, path)

    def test_external_guard_follows_environment_root_and_file_presence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'fixture.sh').write_text('exit 0\n')
            guard = root / 'guard.sh'
            reviewed = b'exit 2\n'
            row = {'source': 'fixture.sh', 'sha256': hashlib.sha256(b'exit 0\n').hexdigest(),
                'evidence': 'A fixture invokes an optional external guard through bash.',
                'external': [{'root': 'environment', 'variable': 'SELECTION_TEST_GUARD_ROOT',
                    'default': directory, 'path': 'guard.sh', 'optional_file': True,
                    'sha256': hashlib.sha256(reviewed).hexdigest(), 'evidence': 'Reviewed guard.'}]}
            document = {'schema': 1, 'config_keys': [], 'units': {}, 'nodes': {'fixture': row}}
            def fallback():
                return inputs.Dependencies(root, document).closure('fixture')['fallback']
            with patch.dict(os.environ, {'SELECTION_TEST_GUARD_ROOT': ''}):
                self.assertFalse(fallback())
                guard.write_bytes(reviewed)
                guard.chmod(0o600)  # bash does not require executable mode.
                self.assertFalse(fallback())
                guard.write_text('unreviewed')
                self.assertTrue(fallback())
                alternative = root / 'alternative'
                alternative.mkdir()
                (alternative / 'guard.sh').write_bytes(reviewed)
                with patch.dict(os.environ, {'SELECTION_TEST_GUARD_ROOT': str(alternative)}):
                    self.assertFalse(fallback())
                    (alternative / 'guard.sh').write_text('changed')
                    self.assertTrue(fallback())
                with patch.dict(os.environ, {'SELECTION_TEST_GUARD_ROOT': 'relative'}):
                    self.assertTrue(fallback())

    def test_spawn_retains_real_model_toolkit_and_external_dependencies(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        unit = 'scripts/spawn.test.sh'
        graph = inputs.Dependencies(HERE.parent, document)
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        for key in ('ALLOWED_MODELS', 'MODEL_TIERS', 'QA_TOOLKIT_DIR', 'QA_TOOLKIT_AGENTS', 'LOCAL_APP_CONTEXT_RE'):
            self.assertIn(key, closure['keys'])
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=a', 'SHOW_TURN_MANIFEST=b')
        self.assertNotIn(unit, graph.config_units(unrelated, [unit]))
        document['nodes']['scripts/spawn.test.sh']['external'][0]['sha256'] = 'changed'
        with tempfile.TemporaryDirectory() as directory:
            guard = Path(directory) / 'scripts/hooks/guard-brief-verification-scope.sh'
            guard.parent.mkdir(parents=True)
            guard.write_text('unreviewed')
            with patch.dict(os.environ, {'RICHOS_SPAWN_TEST_REAL_PROJECT': directory}):
                self.assertIn(unit, graph.config_units(unrelated, [unit]))
        inputs.hook_reader(document['hook_readers'][unit], lambda path: (HERE.parent / path).read_text())

    def test_by_reference_keeps_copied_engine_config_for_seated_control(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/by-reference.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        self.assertIn('MODEL_CEILING', closure['keys'])
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))
        self.assertNotIn(unit, graph.config_units(inputs.config_change('SHOW_TURN_MANIFEST=a', 'SHOW_TURN_MANIFEST=b'), [unit]))
        document['nodes']['scripts/hooks/contract-integrity-probe.sh#by-reference-fixtures']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('SHOW_TURN_MANIFEST=a', 'SHOW_TURN_MANIFEST=b'), [unit]))
        inputs.hook_reader(document['hook_readers'][unit], lambda path: (HERE.parent / path).read_text())

    def test_contract_base_binds_complete_sandbox_inventory(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/contract-integrity.test.sh:base'
        closure = graph.closure(document['units'][unit])
        self.assertFalse(any(closure.values()), closure)
        change = inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        reader = document['hook_readers']['scripts/hooks/contract-integrity.test.sh']
        for line in (HERE / 'hooks/dispatch-pretooluse.manifest').read_text().splitlines():
            if line.startswith(('Bash|', 'Write|')):
                self.assertIn('scripts/hooks/'+line.split('|')[1], reader['sources'])
        document['nodes']['scripts/hooks/teammate-idle-handoff.sh']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(change, [unit]))
        inputs.hook_reader(reader, lambda path: (HERE.parent / path).read_text())

    def test_unevaluated_inventory_retains_engine_declaration_reader(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/unevaluated-payload.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        self.assertEqual(set(closure['keys']), {'OWNED_SYSTEMS_DECLARATION'})
        change = inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('OWNED_SYSTEMS_DECLARATION=a', 'OWNED_SYSTEMS_DECLARATION=b'), [unit]))
        document['nodes']['owned-systems.declaration#fixture-default']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(change, [unit]))
        self.assertTrue(document['hook_readers'][unit]['all_events'])
        reader_sources = document['hook_readers'][unit]['sources']
        for line in (HERE / 'hooks/dispatch-pretooluse.manifest').read_text().splitlines():
            if line.startswith(('Bash|', 'Write|')):
                self.assertIn('scripts/hooks/'+line.split('|')[1], reader_sources)
        inputs.hook_reader(document['hook_readers'][unit], lambda path: (HERE.parent / path).read_text())

    def test_config_selected_commands_require_a_reviewed_literal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'reader.py').write_text('pass\n')
            document = {'schema': 1, 'config_keys': ['COMMAND_FILE'],
                'units': {'unit': 'reader'}, 'nodes': {'reader': {
                    'source': 'reader.py',
                    'sha256': hashlib.sha256(b'pass\n').hexdigest(),
                    'evidence': 'Only the built-in command file is reviewed.',
                    'keys': ['COMMAND_FILE'],
                    'config_literals': {'COMMAND_FILE': [None, '', 'builtin']}}}}
            change = inputs.config_change('OTHER=a', 'OTHER=b')
            self.assertEqual(inputs.Dependencies(root, document).config_units(change, ['unit']), {})
            def unreadable(path):
                if path == 'orchestration.config':
                    raise PermissionError(path)
                return (root / path).read_text()
            self.assertIn('unit', inputs.Dependencies(root, document, unreadable).config_units(change, ['unit']))
            for text in ('', 'COMMAND_FILE="builtin"', "COMMAND_FILE='builtin'", 'COMMAND_FILE='):
                (root / 'orchestration.config').write_text(text)
                self.assertEqual(inputs.Dependencies(root, document).config_units(change, ['unit']), {})
            for text in ('COMMAND_FILE=custom', 'COMMAND_FILE="$HOME/custom"',
                         'COMMAND_FILE=builtin\nCOMMAND_FILE=custom'):
                (root / 'orchestration.config').write_text(text)
                self.assertIn('unit', inputs.Dependencies(root, document).config_units(change, ['unit']))

    def test_land_lease_selection_keeps_real_dispatcher_config(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-land-lease-commands.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        self.assertIn('PUBLIC_RECORD_REPO_GUARD', closure['keys'])
        self.assertIn('PUBLICATION_DECLARATION', closure['keys'])
        self.assertIn('PROTECTED_PATHS', closure['keys'])
        for key in ('MODEL_CEILING', 'INFLIGHT_ACK_TIMEOUT_MIN', 'OPERATOR_FENCES'):
            self.assertNotIn(unit, graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit]))
        document['nodes']['scripts/hooks/dispatch-pretooluse.manifest']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))

    def test_operator_fence_fixture_binds_restore_ingress_and_imports(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/operator-fences.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(any(closure.values()), closure)
        change = inputs.config_change('OPERATOR_FENCES=on', 'OPERATOR_FENCES=off')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        for helper in ('mega-lander/app.py#land-lock-fixtures',
                       'ecs/core/ecs_extract.py#load',
                       'scripts/hooks/commit-ceo-inputs.py#fenced-fixtures',
                       'scripts/hooks/ref-transaction-forensics.sh'):
            original = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper]['sha256'] = original

    def test_operator_lead_fixture_binds_dispatcher_without_real_entity_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/operator-leads.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('OPERATOR_FENCES=on', 'OPERATOR_FENCES=off')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        for helper in ('scripts/lib/operator_fences.py#runtime',
                       'scripts/hooks/dispatch-pretooluse.manifest',
                       'scripts/lib/operator_leads.py#hooks'):
            original = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper]['sha256'] = original
        inputs.hook_reader(document['hook_readers'][unit], lambda path: (HERE.parent / path).read_text())

    def test_optional_external_executable_requires_reviewed_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'fixture.sh').write_text('exit 0\n')
            external = root / 'dispatcher'
            reviewed = b'#!/bin/sh\nexit 0\n'
            document = {'schema': 1, 'config_keys': [], 'units': {'fixture': 'fixture'},
                'nodes': {'fixture': {'source': 'fixture.sh',
                    'sha256': hashlib.sha256((root / 'fixture.sh').read_bytes()).hexdigest(),
                    'evidence': 'Fixture with an optional executable and literal fallback.',
                    'external': [{'root': 'HOME', 'path': 'dispatcher',
                        'sha256': hashlib.sha256(reviewed).hexdigest(),
                        'optional_executable': True, 'evidence': 'Reviewed executable.'}]}}}
            change = inputs.config_change('A=before', 'A=after')
            with patch.dict(os.environ, {'HOME': directory}):
                graph = inputs.Dependencies(root, document)
                self.assertEqual(graph.config_units(change, ['fixture']), {})
                external.write_bytes(reviewed)
                external.chmod(0o700)
                self.assertEqual(graph.config_units(change, ['fixture']), {})
                external.write_text('#!/bin/sh\n. "$ENGINE/orchestration.config"\n')
                self.assertIn('changed external reader', str(graph.config_units(change, ['fixture'])))
                external.unlink()
                external.symlink_to(root / 'missing')
                self.assertEqual(graph.config_units(change, ['fixture']), {})
                external.unlink()
                target = root / 'target'
                target.write_text('unreviewed')
                target.chmod(0o700)
                external.symlink_to(target)
                self.assertIn('fixture', graph.config_units(change, ['fixture']))

    def test_publication_fixture_preserves_adoption_and_scanner_binding(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/publication-boundary.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('PUBLICATION_DECLARATION=old', 'PUBLICATION_DECLARATION=new')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', None), [unit]))
        for helper in ('scripts/lib/publication-boundary.py', 'scripts/hooks/guard-publication-commits.sh#scan'):
            previous = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper]['sha256'] = previous
        reader = document['hook_readers'][unit]
        inputs.hook_reader(reader, lambda path: (HERE.parent / path).read_text())

    def test_contract_workspace_sections_keep_their_distinct_nested_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        prefix = 'scripts/hooks/contract-integrity.test.sh'
        units = [prefix+':Q', prefix+':Qscope']
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertEqual(closure['fallback'], [])
            self.assertFalse(closure['whole'])
            self.assertIn('APP_TEST_INSTANCE_BUNDLE_ID', closure['keys'])
            self.assertIn('QA_TOOLKIT_AGENTS', closure['keys'])
        for key in ('SCRATCH_REAPER_ENABLE', 'ALLOWED_MODELS', 'SEAL_WAIT_SECONDS'):
            self.assertEqual(set(graph.config_units(inputs.config_change(key+'=a', key+'=b'), units)), {units[1]})
        self.assertFalse(graph.config_units(inputs.config_change('QUOTA_PAUSE_PERCENT=a', 'QUOTA_PAUSE_PERCENT=b'), units))
        for unit, helper in zip(units, ('mega-lander/tests/workspaces.test.py', 'mega-lander/tests/workspaces-e2e.test.sh')):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))
            document['nodes'][helper] = old
        reader = inputs.hook_reader(document['hook_readers'][prefix], lambda path: (HERE.parent/path).read_text())
        self.assertTrue(reader['all_events'])

    def test_fourteen_workspace_checks_keep_real_land_and_copied_policy(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'mega-lander/tests/workspace-spec-fourteen.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in ('ALLOWED_MODELS', 'SEAL_WAIT_SECONDS', 'READONLY_ALLOWLIST', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'SCRATCH_REAPER_ENABLE', 'SCRATCH_DOCKER_PRUNE', 'QA_TOOLKIT_AGENTS'):
            self.assertIn(unit, graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit]), key)
        for key in ('UNLANDED_BRANCHES_EXTRA_REPOS', 'QA_TOOLKIT_DIR', 'QUOTA_PAUSE_PERCENT'):
            self.assertNotIn(unit, graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit]), key)
        for helper in ('mega-lander/tests/workspace-spec-fourteen.mutation.sh', 'scripts/hooks/guard-worktree-removal.sh',
                       'scripts/unlanded-branches-lint.sh', 'scripts/hooks/guard-unresolved-claims.py',
                       'scripts/scratch-sweep.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))
            document['nodes'][helper] = old

    def test_workspace_unit_masks_only_the_explicitly_disabled_sweep(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'mega-lander/tests/workspaces.test.sh'
        expected = {'QA_TOOLKIT_AGENTS', 'APP_TEST_INSTANCE_PROCESS_NAMES', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'APP_TEST_INSTANCE_REAL_HOME_PATHS', 'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS',
                    'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in expected | {'SCRATCH_REAPER_ENABLE', 'SCRATCH_ROOT_NAME', 'MODEL_TIERS', 'QA_TOOLKIT_DIR'}:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit])), key in expected, key)
        for helper in ('mega-lander/workspaces.py#unit-fixtures', 'mega-lander/tests/workspaces.mutation.sh',
                       'scripts/hooks/guard-idle-land.sh#forbidden-stop', 'scripts/lib/qa-toolkit.py#type'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_TIERS=a', 'MODEL_TIERS=b'), [unit]))
            document['nodes'][helper] = old

    def test_workspace_land_journey_keeps_actual_scratch_and_qa_inputs(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'mega-lander/tests/workspaces-e2e.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in ('QA_TOOLKIT_AGENTS', 'SCRATCH_ROOT_NAME', 'SCRATCH_REAPER_ENABLE',
                    'SCRATCH_DOCKER_PRUNE', 'ALLOWED_MODELS', 'SEAL_WAIT_SECONDS'):
            self.assertIn(key, closure['keys'])
            self.assertIn(unit, graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit]))
        for key in ('QA_TOOLKIT_DIR', 'QUOTA_PAUSE_PERCENT', 'DISK_RICH_ALERT_GB'):
            self.assertNotIn(unit, graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit]))
        for helper in ('scripts/scratch-sweep.sh', 'scripts/lib/qa-toolkit.py#type',
                       'scripts/lib/appinstances.py#declared-reader', 'scripts/hooks/observe-created-refs.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))
            document['nodes'][helper] = old

    def test_stopped_workspace_fixture_keeps_copied_policy_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'mega-lander/tests/workspace-stopped-ending.test.sh'
        expected = {'AGENT_NAMESPACE_ROOTS', 'ALLOWED_MODELS', 'MODEL_TIERS', 'READONLY_ALLOWLIST', 'HARNESS_UTILITY_TYPES',
                    'GENERIC_AGENT_TYPES', 'SESSION_TEAMS_DIR', 'SEAL_READONLY_TOOLS', 'SEAL_WAIT_SECONDS',
                    'APP_TEST_INSTANCE_PROCESS_NAMES', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'APP_TEST_INSTANCE_REAL_HOME_PATHS', 'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS',
                    'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in expected | {'QUOTA_PAUSE_PERCENT', 'DISK_RICH_ALERT_GB'}:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit])), key in expected, key)
        for helper in ('mega-lander/create-teammate-worktree.sh#plain-fixture-repos',
                       'scripts/hooks/guard-resume-isolation.sh', 'mega-lander/workspaces.sh#stopped-fixture'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))
            document['nodes'][helper] = old

    def test_disk_watchdog_keeps_only_its_real_candidate_assertion(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/disk-watchdog.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(set(closure['keys']), set())
        self.assertFalse(closure['whole'])
        self.assertFalse(closure['presence'])
        for key in ('DISK_CONSUMER_CANDIDATES', 'DISK_RICH_ALERT_GB', 'SCRATCH_REAPER_CMD',
                    'TEST_DEVICE_FAILURES_STATE', 'APP_TEST_INSTANCE_PROCESS_NAMES'):
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit])), False, key)
        self.assertNotIn(unit, graph.config_units(inputs.config_change('DISK_CONSUMER_CANDIDATES=a', None), [unit]))
        change = inputs.config_change('MODEL_TIERS=a', 'MODEL_TIERS=b')
        for helper in ('scripts/lib/disk-watchdog.py#suite-fixtures', 'scripts/lib/testdevices.py#scheduled-collection',
                       'scripts/lib/foreign_app_data.py'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper] = old

    def test_vendoring_unsourced_defaults_keep_adoption_and_registration(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-vendoring-commits.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('VENDORING_DECLARATION=a', 'VENDORING_DECLARATION=b')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('VENDORING_DECLARATION=a', None), [unit]))
        row = document['hook_readers'][unit]
        reader = lambda path: (HERE.parent / path).read_text()
        inputs.hook_reader(row, reader)
        # The guard runs only through the dispatcher's registration (01f38ce8), so a change
        # to that registration must select this suite too.
        self.assertEqual(row['commands'], ['scripts/hooks/dispatch-pretooluse.sh', 'scripts/hooks/guard-vendoring-commits.sh'])
        for helper in ('scripts/lib/git-jurisdiction.sh', 'scripts/lib/vendored-material.sh',
                       'scripts/hooks/guard-vendoring-commits.mutation.sh'):
            old = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper]['sha256'] = old
        row['sources']['scripts/lib/vendored-material.sh'] = 'changed'
        with self.assertRaises(inputs.Unsupported):
            inputs.hook_reader(row, reader)

    def test_large_transport_fixtures_bind_helpers_without_live_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        cases = {
            'scripts/lib/bulk-record-transport.test.sh': ['scripts/lib/ceo-asks.sh', 'scripts/lib/unstarted-rows.sh#fixture-records'],
            'scripts/hooks/payload-transport.test.sh': ['scripts/hooks/guard-inflight-notify.sh', 'scripts/hooks/guard-vendoring-commits.sh', 'scripts/hooks/guard-bash-main-writes.sh'],
        }
        change = inputs.config_change('PROTECTED_PATHS=src', 'PROTECTED_PATHS=app')
        for unit, helpers in cases.items():
            with self.subTest(unit=unit):
                self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
                self.assertNotIn(unit, graph.config_units(change, [unit]))
                for helper in helpers:
                    original = document['nodes'][helper]['sha256']
                    document['nodes'][helper]['sha256'] = 'changed'
                    self.assertIn(unit, graph.config_units(change, [unit]), helper)
                    document['nodes'][helper]['sha256'] = original

    def test_text_fixture_tools_keep_only_the_toolkit_environment_reader(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        cases = {
            'scripts/hardware-choice-check.test.sh': ('scripts/hardware-choice-check.py', set()),
            'scripts/qa-throwaways.test.sh': ('scripts/lib/qa-throwaways.py', {'QA_TOOLKIT_DIR'}),
            'scripts/provision-claude-md.test.sh': ('scripts/provision-claude-md.sh#identity-fixture', set()),
        }
        unrelated = inputs.config_change('CHECK_FAILURE_TYPE=0', 'CHECK_FAILURE_TYPE=1')
        for unit, (helper, keys) in cases.items():
            with self.subTest(unit=unit):
                closure = graph.closure(unit)
                self.assertEqual(set(closure['keys']), keys)
                self.assertFalse(closure['whole'] or closure['presence'] or closure['fallback'], closure)
                self.assertNotIn(unit, graph.config_units(unrelated, [unit]))
                affected = inputs.config_change('QA_TOOLKIT_DIR=old', 'QA_TOOLKIT_DIR=new')
                self.assertEqual(unit in graph.config_units(affected, [unit]), bool(keys))
                old = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertIn(unit, graph.config_units(unrelated, [unit]))
                document['nodes'][helper]['sha256'] = old

    def test_fence_and_ledger_fixtures_keep_only_real_config_transport(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        cases = {
            'scripts/operator-fences-mutation.test.sh': ('scripts/operator-fences.mutation.sh', True),
            'scripts/hooks/ref-transaction-forensics.test.sh': ('scripts/hooks/ref-transaction-forensics.sh', False),
            'scripts/hooks/release-land-leases.test.sh': ('scripts/hooks/release-land-leases.mutation.sh', True),
            'scripts/lib/assert-own-worktree-registered.test.sh': ('scripts/lib/resolve-main-checkout.sh', False),
            'scripts/ledger-prune-sandbox.test.sh': ('scripts/ledger-prune-sandbox.py', False),
        }
        change = inputs.config_change('PROTECTED_PATHS=app', 'PROTECTED_PATHS=packages')
        for unit, (helper, presence) in cases.items():
            with self.subTest(unit=unit):
                closure = graph.closure(unit)
                self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
                self.assertEqual(bool(closure['presence']), presence)
                self.assertNotIn(unit, graph.config_units(change, [unit]))
                self.assertEqual(unit in graph.config_units(inputs.config_change('PROTECTED_PATHS=app', None), [unit]), presence)
                original = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper]['sha256'] = original

    def test_utility_fixtures_and_inventory_list_modes_exclude_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        helpers = {
            'scripts/lib/scratch.test.sh': 'scripts/lib/scratch.sh',
            'scripts/lib/stopwatch.test.sh': 'scripts/lib/stopwatch.sh',
            'scripts/lib/leak-canary.test.sh': 'scripts/lib/tree-witness.sh',
            'scripts/lib/record-canary.test.sh': 'scripts/lib/record-canary.sh',
            'scripts/lib/mutation-pool.test.sh': 'scripts/lib/worker_tokens.py',
            'scripts/mutation-inventory.test.sh': 'scripts/mutation-inventory.test.sh',
            'scripts/ci-units.test.sh': 'scripts/hooks/contract-integrity.test.sh#list',
        }
        change = inputs.config_change('MODEL_CEILING=old', 'MODEL_CEILING=new')
        for unit, helper in helpers.items():
            with self.subTest(unit=unit):
                self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
                self.assertNotIn(unit, graph.config_units(change, [unit]))
                original = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper]['sha256'] = original
        # A qualified new read in a previously independent helper must propagate.
        document['nodes']['scripts/lib/stopwatch.sh']['keys'] = ['MODEL_CEILING']
        self.assertIn('scripts/lib/stopwatch.test.sh', graph.config_units(change, helpers))
        self.assertNotIn('scripts/lib/record-canary.test.sh', graph.config_units(change, helpers))

    def test_login_alarm_fixture_does_not_read_copied_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/login-alarm.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertFalse(closure['keys'] or closure['whole'])
        self.assertTrue(closure['presence'])  # The shared mutation transport copies the file.
        change = inputs.config_change('QUOTA_PAUSE_PERCENT=93', 'QUOTA_PAUSE_PERCENT=94')
        self.assertNotIn(unit, graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('QUOTA_PAUSE_PERCENT=93', None), [unit]))
        for helper in ('scripts/login-alarm.sh#suite-fixture',
                       'scripts/lib/login_alarm.py#suite-fixture',
                       'scripts/lib/escalations.py', 'scripts/login-alarm.mutation.sh'):
            previous = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper]['sha256'] = previous

    def test_quota_fixture_retains_real_cleanup_and_nested_transport(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/quota-watch.test.sh'
        expected = {'APP_TEST_INSTANCE_PROCESS_NAMES', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'APP_TEST_INSTANCE_REAL_HOME_PATHS', 'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS',
                    'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in expected | {'QUOTA_PAUSE_PERCENT', 'SESSION_TEAMS_DIR', 'MODEL_CEILING'}:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=a', key+'=b'), [unit])), key in expected, key)
        self.assertIn(unit, graph.config_units(inputs.config_change('QUOTA_PAUSE_PERCENT=93', None), [unit]))
        change = inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b')
        for helper in ('scripts/login-alarm.sh#report', 'scripts/lib/quota_weekly.py#quota-fixture',
                       'scripts/lib/quota-watch-replay.py', 'mega-lander/workspaces.py#quota-fixture',
                       'scripts/quota-watch.mutation.sh'):
            previous = document['nodes'][helper]['sha256']
            document['nodes'][helper]['sha256'] = 'changed'
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper]['sha256'] = previous

    def test_left_off_retains_real_session_start_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/left-off.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        self.assertEqual(set(closure['keys']), {'CHECK_LEFT_OFF', 'LEFT_OFF_GAP_MINUTES'})
        unrelated = inputs.config_change('MODEL_TIERS=one', 'MODEL_TIERS=two')
        self.assertFalse(graph.config_units(unrelated, [unit]))
        for key in closure['keys']:
            self.assertIn(unit, graph.config_units(inputs.config_change(key+'=one', key+'=two'), [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_TIERS=one', None), [unit]))
        for helper in ('scripts/lib/unlanded-branches.py', 'scripts/lib/workspaces.py#left-off-registry',
                       'scripts/lib/escalations.py', 'scripts/hooks/left-off.mutation.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(unrelated, [unit]), helper)
            document['nodes'][helper] = old

    def test_owned_state_binds_executable_declaration_and_nested_transport(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/owned-state.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('OWNED_SYSTEMS_DECLARATION=one', 'OWNED_SYSTEMS_DECLARATION=two')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('OWNED_SYSTEMS_DECLARATION=one', None), [unit]))
        for helper in ('owned-systems.declaration#escalations', 'scripts/escalate.sh',
                       'scripts/lib/ci_pause.py', 'scripts/hooks/owned-state.mutation.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper] = old

    def test_protected_ref_notice_has_no_real_config_reader(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/protected-ref-moves.test.sh'
        self.assertFalse(any(graph.closure(unit).values()))
        change = inputs.config_change('PROTECTED_PATHS=one', 'PROTECTED_PATHS=two')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertFalse(graph.config_units(inputs.config_change('PROTECTED_PATHS=one', None), [unit]))
        for helper in ('scripts/lib/protected-ref-moves.py', 'mega-lander/workspaces.py#event-store',
                       'scripts/lib/stop-hook-notice.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper] = old

    def test_worker_lifecycle_preserves_real_registry_cleanup_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/worker-lifecycle.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        # The lander's cleanup also reaches its build-cache step (build_caches.py, 2026-10-08).
        self.assertEqual(set(closure['keys']), set(graph.closure('scripts/lib/appinstances.py#workspace-cleanup')['keys'])
                         | set(graph.closure('scripts/lib/build_caches.py')['keys']))
        unrelated = inputs.config_change('MODEL_TIERS=one', 'MODEL_TIERS=two')
        self.assertFalse(graph.config_units(unrelated, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('APP_TEST_INSTANCE_PROCESS_NAMES=one', 'APP_TEST_INSTANCE_PROCESS_NAMES=two'), [unit]))
        for helper in ('scripts/hooks/worker-created-handoff.sh', 'scripts/hooks/worker-ended-handoff.sh',
                       'scripts/lib/worktree-ledger.py#append', 'mega-lander/workspaces.py#ledger-lookup'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(unrelated, [unit]), helper)
            document['nodes'][helper] = old

    def test_agent_prompt_fixture_keeps_nested_copy_and_helper_bindings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/verify-agent-prompt.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('QA_ROLE_AGENTS=one', 'QA_ROLE_AGENTS=two')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('QA_ROLE_AGENTS=one', None), [unit]))
        for helper in ('scripts/hooks/conceal.mutation.sh', 'scripts/lib/resolve-roots.sh',
                       'scripts/lib/unevaluated-notice.sh', 'scripts/lib/mutation-pool.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper] = old

    def test_waiver_fixtures_keep_source_discovery_and_nested_mutation_bound(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/waiver-repetition.test.sh'
        self.assertFalse(any(graph.closure(unit).values()))
        change = inputs.config_change('PROTECTED_PATHS=one', 'PROTECTED_PATHS=two')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertFalse(graph.config_units(inputs.config_change('PROTECTED_PATHS=one', None), [unit]))
        for helper in ('scripts/hooks/notice-waiver-repetition.py',
                       'scripts/hooks/waiver-repetition.mutation.sh', 'scripts/lib/mutation-pool.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]), helper)
            document['nodes'][helper] = old
        reader = inputs.hook_reader(document['hook_readers'][unit],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertEqual(reader['text_needles'], ['notice-waiver-repetition.sh'])

    def test_stop_visibility_preserves_real_disk_reader_and_dynamic_inventory(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/stop-hook-visibility.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure['fallback'])
        self.assertEqual(set(closure['keys']), set(graph.closure('scripts/disk-watchdog.sh#alert')['keys']))
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=1', 'SHOW_TURN_MANIFEST=0')
        self.assertFalse(graph.config_units(unrelated, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('DISK_RICH_ALERT_GB=10', 'DISK_RICH_ALERT_GB=20'), [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('DISK_RICH_ALERT_GB=10', None), [unit]))
        for helper in ('hooks/hooks.json#stop-visibility-inventory',
                       'scripts/hooks/release-land-leases.sh#unadopted-stop',
                       'scripts/lib/stop-session-recovery.py', 'scripts/lib/escalations.sh#load'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(unrelated, [unit]), helper)
            document['nodes'][helper] = old
        reader = inputs.hook_reader(document['hook_readers'][unit],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertEqual(reader['events'], ['Stop'])

    def test_dispatcher_binds_all_modules_and_keeps_preclassification_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/dispatch-pretooluse.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        expected = {'PROTECTED_PATHS', 'SECRET_SCAN_MIN_LENGTH', 'SECRET_SCAN_MIN_ENTROPY',
                    'SECRET_SCAN_ALLOWLIST', 'SECRET_SCAN_CODE_AWARE', 'DIALECT_TARGET',
                    'DIALECT_SCAN_ALLOWLIST', 'DIALECT_EXEMPT_PATHS',
                    'HOME_NETWORK_PHONE_GUARD', 'HOST_DISPLAY_POWER_GUARD', 'PUBLIC_RECORD_REPO_GUARD',
                    'SCRATCH_CLAUDE_ROOTS'}
        self.assertEqual(set(closure['keys']), expected)
        self.assertTrue(closure['presence'])
        unrelated = inputs.config_change('MODEL_TIERS=one', 'MODEL_TIERS=two')
        self.assertFalse(graph.config_units(unrelated, [unit]))
        for key in expected:
            self.assertIn(unit, graph.config_units(inputs.config_change(key+'=one', key+'=two'), [unit]), key)
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_TIERS=one', None), [unit]))
        for helper in ('scripts/hooks/dispatch-pretooluse.manifest',
                       'scripts/hooks/guard-ci-red-lands.sh#benign-bash',
                       'scripts/hooks/install.sh#private-home', 'scripts/lib/operator-mode.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(unrelated, [unit]), helper)
            document['nodes'][helper] = old
        reader = inputs.hook_reader(document['hook_readers'][unit],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertTrue(reader['all_events'])
        modules = {line.split('|')[1] for line in (HERE / 'hooks/dispatch-pretooluse.manifest').read_text().splitlines()
                   if line and not line.startswith('#')}
        self.assertTrue({'scripts/hooks/'+module for module in modules} <= set(reader['sources']))

    def test_named_person_fixtures_do_not_read_shipped_config_or_real_rosters(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/named-persons.test.sh'
        self.assertFalse(any(graph.closure(document['units'][unit]).values()))
        change = inputs.config_change('PUBLICATION_DECLARATION=old', 'PUBLICATION_DECLARATION=new')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertFalse(graph.config_units(inputs.config_change('PUBLICATION_DECLARATION=old', None), [unit]))
        for helper in ('scripts/lib/named-persons.py#fixture-roster',
                       'scripts/lib/resolve-roots.sh#engine-only',
                       'scripts/lib/resolve-main-checkout.sh'):
            old = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper] = old
        reader = inputs.hook_reader(document['hook_readers'][unit],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertEqual(set(reader['commands']), {'scripts/hooks/dispatch-pretooluse.sh',
                                                  'scripts/hooks/guard-named-persons-writes.sh',
                                                  'scripts/hooks/guard-named-persons-commands.sh'})

    def test_completeness_uses_private_trees_but_keeps_real_surface_obligations(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        guard = 'scripts/hooks/completeness-commits.test.sh'
        checker = 'scripts/publication-completeness.test.sh'
        units = [guard, checker]
        change = inputs.config_change('PUBLICATION_DECLARATION=old', 'PUBLICATION_DECLARATION=new')
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertFalse(graph.config_units(change, units))
        self.assertEqual(set(graph.config_units(inputs.config_change('PUBLICATION_DECLARATION=old', None), units)), {guard})
        for helper in ('scripts/publication-completeness.py#fixture-trees',
                       'scripts/lib/publication-boundary.sh#declaration-and-sources'):
            old = document['nodes'].pop(helper)
            self.assertEqual(set(graph.config_units(change, units)), set(units))
            document['nodes'][helper] = old
        for unit in units:
            reader = inputs.hook_reader(document['hook_readers'][unit],
                                        lambda path: (HERE.parent / path).read_text())
            self.assertFalse(reader.get('all_events'))
        self.assertEqual(document['hook_readers'][guard]['commands'],
                         ['scripts/hooks/dispatch-pretooluse.sh', 'scripts/hooks/guard-completeness-commits.sh'])
        self.assertEqual(document['hook_readers'][checker]['text_needles'], ['publication-completeness'])

    def test_row_currency_keeps_adoption_and_registration_without_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/row-currency.test.sh'
        closure = graph.closure(document['units'][unit])
        self.assertFalse(closure['whole'] or closure['fallback'] or closure['keys'], closure)
        self.assertTrue(closure['presence'])
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('SHOW_TURN_MANIFEST=0', None), [unit]))
        for helper in ('scripts/hooks/guard-row-currency-commits.mutation.sh',
                       'scripts/row-headline-verify.sh#fixture-records',
                       'scripts/lib/row-currency.py#fixture-records'):
            previous = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper] = previous
        reader = inputs.hook_reader(document['hook_readers'][unit],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertEqual(reader['commands'], ['scripts/hooks/dispatch-pretooluse.sh', 'scripts/hooks/guard-row-currency-commits.sh'])
        document['hook_readers'][unit]['sources']['scripts/lib/registered-hooks.sh'] = 'changed'
        with self.assertRaises(inputs.Unsupported):
            inputs.hook_reader(document['hook_readers'][unit],
                               lambda path: (HERE.parent / path).read_text())

    def test_isolation_contract_keeps_copied_model_and_registration_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/contract-integrity.test.sh:WTI'
        closure = graph.closure(document['units'][unit])
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        for key in ('READONLY_ALLOWLIST', 'ALLOWED_MODELS', 'MODEL_TIERS',
                    'SESSION_TEAMS_DIR', 'APP_TEST_INSTANCE_BUNDLE_ID'):
            self.assertIn(unit, graph.config_units(inputs.config_change(key+'=old', key+'=new'), [unit]), key)
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        self.assertFalse(graph.config_units(unrelated, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('ALLOWED_MODELS=sonnet', None), [unit]))
        for helper in ('scripts/hooks/guard-worktree-isolation.mutation.sh',
                       'scripts/hooks/guard-worktree-isolation.test.sh',
                       'mega-lander/workspaces.py#isolation-registration'):
            previous = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(unrelated, [unit]))
            document['nodes'][helper] = previous

    def test_idle_land_contract_and_mutants_use_private_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ('scripts/hooks/guard-idle-land.test.sh',
                 'scripts/hooks/contract-integrity.test.sh:IL')
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(any(closure.values()), (unit, closure))
        for change in (inputs.config_change('CHECK_IDLE_LAND=1', 'CHECK_IDLE_LAND=0'),
                       inputs.config_change('PROTECTED_PATHS=app', None)):
            self.assertFalse(graph.config_units(change, units))
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for helper, affected in (('scripts/hooks/idle-land.mutation.sh', units[1:]),
                                 ('scripts/hooks/guard-idle-land.py', units),
                                 ('scripts/lib/stop-session-recovery.py', units)):
            previous = document['nodes'].pop(helper)
            self.assertEqual(set(graph.config_units(unrelated, units)), set(affected))
            document['nodes'][helper] = previous

    def test_unstarted_and_stated_actions_rebuild_private_configuration(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ('scripts/hooks/unstarted-rows.test.sh',
                 'ass-kicker/tests/ass-kicker.test.sh',
                 'scripts/hooks/contract-integrity.test.sh:SA')
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(any(closure.values()), (unit, closure))
        change = inputs.config_change('CHECK_STATED_ACTIONS=1', 'CHECK_STATED_ACTIONS=0')
        self.assertFalse(graph.config_units(change, units))
        self.assertFalse(graph.config_units(inputs.config_change('PROTECTED_PATHS=app', None), units))
        for helper, affected in (
                ('scripts/hooks/unstarted-rows.mutation.sh', units[:1]),
                ('scripts/lib/row-currency.py#fixture-records', units[:1]),
                ('ass-kicker/ass-kicker.py', units[1:]),
                ('scripts/lib/stop-session-recovery.py', units[1:]),
                ('ass-kicker/tests/stated-actions.mutation.sh', units[2:])):
            previous = document['nodes'].pop(helper)
            self.assertEqual(set(graph.config_units(change, units)), set(affected))
            document['nodes'][helper] = previous

    def test_mechanical_findings_only_read_fixture_configuration_and_inventory(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ('scripts/hooks/mechanical-findings.test.sh',
                 'scripts/hooks/contract-integrity.test.sh:MF')
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(any(closure.values()), (unit, closure))
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        self.assertFalse(graph.config_units(change, units))
        self.assertFalse(graph.config_units(inputs.config_change('PROTECTED_PATHS=app', None), units))
        for helper in ('scripts/lib/mechanical-findings.py#fixture-records',
                       'scripts/lib/unstarted-rows.py#fixture-records',
                       'scripts/lib/row-currency.py#fixture-records'):
            previous = document['nodes'].pop(helper)
            self.assertEqual(set(graph.config_units(change, units)), set(units))
            document['nodes'][helper] = previous
        reader = inputs.hook_reader(document['hook_readers'][units[0]],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertFalse(reader['commands'] or reader.get('events') or reader.get('all_events'))
        with self.assertRaises(inputs.Unsupported):
            inputs.hook_reader(document['hook_readers'][units[0]],
                lambda path: (HERE.parent / path).read_text() + '# changed' if
                path == 'scripts/lib/mechanical-findings.py' else (HERE.parent / path).read_text())

    def test_interactive_contract_keeps_real_adoption_without_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/contract-integrity.test.sh:IP'
        closure = graph.closure(document['units'][unit])
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'], closure)
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        self.assertFalse(graph.config_units(change, [unit]))
        self.assertIn(unit, graph.config_units(inputs.config_change('PROTECTED_PATHS=app', None), [unit]))
        for helper in ('scripts/hooks/interactive-prompt.mutation.sh',
                       'scripts/hooks/guard-interactive-prompt.test.sh',
                       'scripts/lib/interactive-prompt.py#payload'):
            previous = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper] = previous
        reader = inputs.hook_reader(document['hook_readers'][unit.split(':')[0]],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertTrue(reader['all_events'])

    def test_seated_contract_fixtures_do_not_consume_shipped_config(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        prefix = 'scripts/hooks/contract-integrity.test.sh:'
        sections = ('M', 'shim', 'manifest', 'worktree', 'N', 'python3',
                    'config', 'K', 'P', 'S', 'MT', 'MC')
        units = [prefix + section for section in sections]
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(any(closure.values()), (unit, closure))
        change = inputs.config_change('MODEL_TIERS=old\nPROTECTED_PATHS=app', None)
        self.assertFalse(graph.config_units(change, units))
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for helper in ('scripts/hooks/guard-idle-land.py',
                       'scripts/hooks/guard-workspace-gate.sh',
                       'scripts/lib/operator_fences_admin.py#status'):
            previous = document['nodes'].pop(helper)
            self.assertIn(units[0], graph.config_units(unrelated, units[:1]))
            document['nodes'][helper] = previous

    def test_scoped_contract_sections_keep_their_own_nested_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        prefix = 'scripts/hooks/contract-integrity.test.sh:'
        units = [prefix + section for section in ('CL', 'RI', 'WTR', 'MC6', 'SCR')]
        for unit in units:
            closure = graph.closure(document['units'][unit])
            self.assertFalse(closure['fallback'] or closure['whole'], (unit, closure))
        changes = {
            'SHOW_TURN_MANIFEST': set(),
            'SESSION_TEAMS_DIR': {prefix + 'RI'},
            'APP_TEST_INSTANCE_BUNDLE_ID': {prefix + 'RI'},
            'SCRATCH_NIGHTLY_KEEP': {prefix + 'SCR'},
            'MODEL_CEILING': set(),
        }
        for key, expected in changes.items():
            change = inputs.config_change(key + '=old', key + '=new')
            self.assertEqual(set(graph.config_units(change, units)), expected, key)
        deleted = inputs.config_change('PROTECTED_PATHS=app', None)
        self.assertTrue(set(units[1:]) <= set(graph.config_units(deleted, units)))
        unrelated = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for section, helper in (
                ('CL', 'scripts/hooks/claim-roles.mutation.sh'),
                ('RI', 'scripts/hooks/inflight-notify.mutation.sh'),
                ('WTR', 'scripts/hooks/guard-worktree-removal.mutation.sh'),
                ('MC6', 'scripts/hooks/guard-model-ceiling.mutation.sh'),
                ('SCR', 'scripts/lib/scratch-reaper.py#required-setting-names')):
            previous = document['nodes'].pop(helper)
            self.assertIn(prefix + section, graph.config_units(unrelated, units))
            document['nodes'][helper] = previous
        reader = inputs.hook_reader(document['hook_readers'][prefix[:-1]],
                                    lambda path: (HERE.parent / path).read_text())
        self.assertTrue(reader['all_events'])

    def test_failure_type_mutants_rebuild_private_register_configuration(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/failure-type.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'], closure)
        self.assertIn(unit, graph.config_units(inputs.config_change('CHECK_FAILURE_TYPE=1', None), [unit]))
        change = inputs.config_change('CHECK_FAILURE_TYPE=1\nFAILURE_TYPE_REGISTER=old',
                                      'CHECK_FAILURE_TYPE=0\nFAILURE_TYPE_REGISTER=new')
        self.assertFalse(graph.config_units(change, [unit]))
        for helper in ('scripts/lib/left-off.py#human-classifier',
                       'scripts/hooks/failure-type.mutation.sh',
                       'scripts/lib/stop-session-recovery.py'):
            previous = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(change, [unit]))
            document['nodes'][helper] = previous

    def test_private_alert_settings_do_not_select_real_config_changes(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ('scripts/hooks/escalations.test.sh', 'scripts/hooks/notice-disk-alert.test.sh')
        changes = [inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1'),
                   inputs.config_change('DISK_PRIMARY_VOLUME=old', 'DISK_PRIMARY_VOLUME=new'),
                   inputs.config_change('PROTECTED_PATHS=src', None)]
        for unit in units:
            closure = graph.closure(unit)
            self.assertFalse(any(closure.values()), closure)
            for change in changes:
                self.assertFalse(graph.config_units(change, [unit]), (unit, change))
        for unit, helper in ((units[0], 'scripts/escalate.sh'),
                             (units[0], 'scripts/hooks/notice-escalations.sh'),
                             (units[1], 'scripts/lib/disk-watchdog.py#check'),
                             (units[1], 'scripts/lib/testdevices.py#workspace-collection')):
            previous = document['nodes'].pop(helper)
            self.assertIn(unit, graph.config_units(changes[0], [unit]))
            document['nodes'][helper] = previous

    def test_root_mutations_retain_nested_session_and_engine_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/root-contract.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        expected = {'PROTECTED_PATHS', 'ALLOWED_MODELS', 'MODEL_TIERS',
                    'AGENT_NAMESPACE_ROOTS', 'SECRET_SCAN_MIN_LENGTH',
                    'APP_TEST_INSTANCE_BUNDLE_ID', 'DISK_PRIMARY_VOLUME',
                    'SCRATCH_REAPER_ENABLE'}
        for key in expected | {'SHOW_TURN_MANIFEST', 'QUOTA_PAUSE_PERCENT'}:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=old', key+'=new'), [unit])),
                             key in expected, key)
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for helper in ('scripts/hooks/root-contract.mutation.sh',
                       'scripts/hooks/session-start-stdin.test.sh',
                       'scripts/hooks/guard-worktree-isolation.sh',
                       'scripts/hooks/notice-disk-alert.sh', 'scripts/lib/resolve-roots.test.sh'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper] = previous

    def test_session_start_fixtures_retain_real_engine_resource_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/session-start-stdin.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'], closure)
        real = {'DISK_PRIMARY_VOLUME', 'DISK_RICH_ALERT_GB', 'DISK_STATE_JSON',
                'SCRATCH_REAPER_ENABLE', 'SCRATCH_CLAUDE_ROOTS', 'SCRATCH_FAILURES_STATE',
                'APP_TEST_INSTANCE_BUNDLE_ID', 'TEST_DEVICE_FAILURES_STATE'}
        literal = {'CHECK_LEFT_OFF', 'LEFT_OFF_GAP_MINUTES', 'QUOTA_PAUSE_PERCENT',
                   'PROTECTED_PATHS', 'ALLOWED_MODELS', 'SHOW_TURN_MANIFEST'}
        for key in real | literal:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=old', key+'=new'), [unit])),
                             key in real, key)
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for helper in ('scripts/lib/disk-watchdog.py#alert', 'scripts/lib/foreign_app_data.py',
                       'scripts/lib/operator_leads.py#claim-start', 'scripts/lib/escalations.py',
                       'scripts/hooks/left-off-report.sh#no-transcript', 'scripts/lib/scratch-reaper.py'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper] = previous

    def test_resume_registry_read_retains_reached_cleanup_dependencies(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-resume-isolation.test.sh'
        expected = {'SESSION_TEAMS_DIR', 'APP_TEST_INSTANCE_PROCESS_NAMES',
                    'APP_TEST_INSTANCE_BUNDLE_ID', 'APP_TEST_INSTANCE_REAL_HOME_PATHS',
                    'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS', 'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        for key in expected | {'PROTECTED_PATHS', 'MODEL_TIERS'}:
            self.assertEqual(bool(graph.config_units(inputs.config_change(key+'=old', key+'=new'), [unit])),
                             key in expected, key)
        change = inputs.config_change('MODEL_TIERS=old', 'MODEL_TIERS=new')
        for helper in ('mega-lander/workspaces.py#recipient', 'scripts/lib/agent-liveness.py',
                       'scripts/lib/teammate-identity.py', 'scripts/lib/pause_protocol.py',
                       'scripts/lib/appinstances.py#workspace-cleanup'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper] = previous

    def test_isolation_registration_retains_model_and_cleanup_inputs(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-worktree-isolation.test.sh'
        expected = {'READONLY_ALLOWLIST', 'HARNESS_UTILITY_TYPES', 'GENERIC_AGENT_TYPES',
                    'SESSION_TEAMS_DIR', 'ALLOWED_MODELS', 'MODEL_TIERS',
                    'AGENT_NAMESPACE_ROOTS', 'APP_TEST_INSTANCE_PROCESS_NAMES',
                    'APP_TEST_INSTANCE_BUNDLE_ID', 'APP_TEST_INSTANCE_REAL_HOME_PATHS',
                    'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS', 'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'] or closure['fallback'], closure)
        for key in expected | {'PROTECTED_PATHS', 'SHOW_TURN_MANIFEST'}:
            change = inputs.config_change(key+'=old', key+'=new')
            self.assertEqual(bool(graph.config_units(change, [unit])), key in expected, key)
        self.assertIn(unit, graph.config_units(inputs.config_change(None, 'ALLOWED_MODELS=opus'), [unit]))
        change = inputs.config_change('SHOW_TURN_MANIFEST=0', 'SHOW_TURN_MANIFEST=1')
        for helper in ('scripts/lib/teammate-name.sh', 'scripts/lib/model-tiers.sh',
                       'scripts/lib/resolve-roots.sh', 'scripts/lib/resolve-model.sh',
                       'mega-lander/workspaces.py#isolation-registration',
                       'scripts/lib/appinstances.py#workspace-cleanup'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper] = previous

    def test_git_jurisdiction_binds_undeclared_target_guard_paths(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/lib/git-jurisdiction.test.sh'
        self.assertEqual(graph.closure(unit),
                         {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        change = inputs.config_change('PROTECTED_PATHS=old', 'PROTECTED_PATHS=new')
        self.assertEqual(graph.config_units(change, [unit]), {})
        for helper in ('scripts/hooks/guard-completeness-commits.sh#undeclared-target',
                       'scripts/hooks/guard-publication-commits.sh#undeclared-target',
                       'scripts/hooks/guard-row-currency-commits.sh#undeclared-target',
                       'scripts/lib/row-currency.sh#undeclared',
                       'scripts/lib/publication-boundary.sh#undeclared',
                       'scripts/hooks/guard-inflight-notify.sh'):
            with self.subTest(helper=helper):
                previous = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertEqual(set(graph.config_units(change, [unit])), {unit})
                document['nodes'][helper]['sha256'] = previous

    def test_inflight_durability_retains_real_workspace_cleanup_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/inflight-ack-durability.test.sh'
        closure = graph.closure(unit)
        expected = {'APP_TEST_INSTANCE_PROCESS_NAMES', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'APP_TEST_INSTANCE_REAL_HOME_PATHS', 'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS',
                    'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'] or closure['presence'] or closure['fallback'], closure)
        for key in expected | {'INFLIGHT_ACK_TIMEOUT_MIN', 'PROTECTED_PATHS', 'SHOW_TURN_MANIFEST'}:
            change = inputs.config_change(key+'=old', key+'=new')
            self.assertEqual(bool(graph.config_units(change, [unit])), key in expected, key)
        unrelated = inputs.config_change('PROTECTED_PATHS=old', 'PROTECTED_PATHS=new')
        for helper in ('mega-lander/workspaces.py#discard', 'mega-lander/workspaces.sh#discard',
                       'scripts/lib/appinstances.py#workspace-cleanup',
                       'scripts/lib/testdevices.py#workspace-collection',
                       'scripts/lib/containers.py#workspace-cleanup'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertEqual(set(graph.config_units(unrelated, [unit])), {unit})
                document['nodes'][helper] = previous

    def test_inflight_notification_covers_stop_and_durable_ack_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/inflight-notify.test.sh'
        self.assertEqual(graph.closure(unit),
                         {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        self.assertEqual(set(graph.closure('scripts/hooks/notice-inflight-acks.sh')['keys']),
                         {'INFLIGHT_ACK_TIMEOUT_MIN'})
        change = inputs.config_change('INFLIGHT_ACK_TIMEOUT_MIN=5', 'INFLIGHT_ACK_TIMEOUT_MIN=10')
        self.assertEqual(graph.config_units(change, [unit]), {})
        for helper in ('scripts/hooks/notice-inflight-acks.sh', 'scripts/inflight-ack.sh',
                       'scripts/lib/stop-hook-notice.sh', 'scripts/lib/inflight.py'):
            with self.subTest(helper=helper):
                previous = document['nodes'].pop(helper)
                self.assertEqual(set(graph.config_units(change, [unit])), {unit})
                document['nodes'][helper] = previous

    def test_inflight_two_leads_keeps_private_settings_and_transitive_bindings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/inflight-two-leads.test.sh'
        self.assertEqual(graph.closure(unit),
                         {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        self.assertEqual(set(graph.closure('scripts/hooks/guard-inflight-notify.sh')['keys']),
                         {'INFLIGHT_ACK_TIMEOUT_MIN'})
        change = inputs.config_change('INFLIGHT_ACK_TIMEOUT_MIN=5', 'INFLIGHT_ACK_TIMEOUT_MIN=10')
        self.assertEqual(graph.config_units(change, [unit]), {})
        self.assertEqual(graph.config_units(inputs.config_change(None, 'PROTECTED_PATHS=src'), [unit]), {})
        for helper in ('scripts/lib/inflight.py', 'scripts/lib/inflight.sh',
                       'scripts/hooks/guard-inflight-notify.sh',
                       'scripts/hooks/notice-inflight-sends.sh', 'scripts/inflight-notify.sh',
                       'scripts/lib/teammate-identity.py', 'mega-lander/workspaces.py#integration'):
            with self.subTest(helper=helper):
                previous = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertEqual(set(graph.config_units(change, [unit])), {unit})
                document['nodes'][helper]['sha256'] = previous

    def test_staging_and_worktree_detectors_replace_entity_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/stale-staging.test.sh',
                 'scripts/hooks/detect-nonnative-worktree.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        for key in ('STAGING_TREES', 'STAGING_DEPLOY_RECORD', 'READONLY_ALLOWLIST', 'SESSION_TEAMS_DIR'):
            self.assertEqual(graph.config_units(inputs.config_change(key + '=a', key + '=b'), units), {})
        self.assertEqual(set(graph.config_units(inputs.config_change(None, 'STAGING_TREES=a'), units)), {units[0]})
        for helper, unit in (('scripts/hooks/stale-staging.mutation.sh', units[0]),
                             ('scripts/staging-record.sh', units[0]),
                             ('scripts/hooks/detect-nonnative-worktree.sh', units[1])):
            original = document['nodes'].pop(helper)
            self.assertTrue(graph.closure(unit)['fallback'])
            document['nodes'][helper] = original

    def test_python_wrappers_follow_their_actual_imports(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        for stem in ('durable-filesystem-identity', 'spawn-guard-audience'):
            base = 'scripts/lib/' + stem
            unit = base + '.test.sh'
            with self.subTest(unit=unit):
                self.assertEqual(graph.closure(unit),
                                 {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
                for suffix in ('.test.py', '.py'):
                    original = document['nodes'][base + suffix]['sha256']
                    document['nodes'][base + suffix]['sha256'] = 'changed'
                    self.assertTrue(graph.closure(unit)['fallback'])
                    document['nodes'][base + suffix]['sha256'] = original

    def test_todo_records_and_cold_readers_are_fixtures_but_adoption_still_matters(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/cold-open.test.sh', 'scripts/hooks/ceo-todos.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        change = inputs.config_change('SCRATCH_DEFAULT_TTL_MINUTES=360', 'SCRATCH_DEFAULT_TTL_MINUTES=300')
        self.assertEqual(graph.config_units(change, units), {})
        self.assertEqual(set(graph.config_units(inputs.config_change(None, 'PROTECTED_PATHS=src'), units)),
                         {units[1]})
        for helper, affected in (
                ('scripts/lib/ceo-todos.py#fixture-records', units),
                ('scripts/cold-open.sh#fixture-reader', units),
                ('scripts/hooks/ceo-todos.mutation.sh', units[1:]),
                ('scripts/ceo-todos-init.sh#no-cold-open', units[1:])):
            with self.subTest(helper=helper):
                original = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertEqual(set(graph.config_units(change, units)), set(affected))
                document['nodes'][helper]['sha256'] = original

    def test_worktree_removal_reads_private_adoption_and_registry_only(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-worktree-removal.test.sh'
        self.assertEqual(graph.closure(unit),
                         {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        change = inputs.config_change('PROTECTED_PATHS=old', 'PROTECTED_PATHS=new')
        self.assertEqual(graph.config_units(change, [unit]), {})
        for helper in ('mega-lander/workspaces.py#integration',
                       'scripts/hooks/guard-worktree-removal.sh'):
            with self.subTest(helper=helper):
                original = document['nodes'].pop(helper)
                self.assertEqual(set(graph.config_units(change, [unit])), {unit})
                document['nodes'][helper] = original

    def test_artifact_and_manifest_fixtures_do_not_consume_production_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/collect-worktree-artifacts.test.sh',
                 'scripts/hooks/turn-manifest.test.sh']
        for unit in units:
            self.assertEqual(graph.closure(unit),
                             {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        for key in ('ARTIFACT_MERGE_DIRS', 'ARTIFACT_REPLACE_DIRS', 'SHOW_TURN_MANIFEST'):
            change = inputs.config_change(key+'=old', key+'=new')
            self.assertEqual(graph.config_units(change, units), {})
        self.assertEqual(graph.config_units(inputs.config_change(None, 'SHOW_TURN_MANIFEST=1'), units), {})
        self.assertEqual(set(graph.closure('scripts/collect-worktree-artifacts.sh')['keys']),
                         {'ARTIFACT_MERGE_DIRS', 'ARTIFACT_REPLACE_DIRS'})
        self.assertEqual(set(graph.closure('scripts/hooks/turn-manifest.sh')['keys']),
                         {'SHOW_TURN_MANIFEST'})
        # Fixture replacement may remove value dependencies, never uncertainty
        # about a helper or the constructor used by the nested mutation run.
        for helper, affected in (
                ('scripts/lib/resolve-roots.sh#roots', units),
                ('scripts/hooks/turn-manifest.py', units[1:]),
                ('scripts/hooks/turn-manifest.mutation.sh', units[1:]),
                ('scripts/collect-worktree-artifacts.sh', units[:1])):
            with self.subTest(helper=helper):
                old = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertEqual(set(graph.config_units(change, units)), set(affected))
                document['nodes'][helper]['sha256'] = old

    def test_unlanded_fixture_copy_and_mutants_preserve_entity_config_origin(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/unlanded-branches.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['keys'], {}, closure)
        self.assertEqual(closure['whole'], [], closure)
        self.assertEqual(closure['fallback'], [], closure)
        change = inputs.config_change('CHECK_UNLANDED_BRANCHES=1', 'CHECK_UNLANDED_BRANCHES=0')
        self.assertEqual(graph.config_units(change, [unit]), {})
        document['nodes'].pop('scripts/lib/unlanded-branches.py')
        self.assertEqual(set(graph.config_units(change, [unit])), {unit})

    def test_claim_and_ci_gates_reach_only_fixture_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/guard-unresolved-claims.test.sh',
                 'scripts/hooks/guard-ci-turn-gate.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        for key in ('CHECK_UNRESOLVED_CLAIMS', 'UNLANDED_BRANCHES_EXTRA_REPOS', 'PROTECTED_PATHS'):
            change = inputs.config_change(key+'=old', key+'=new')
            self.assertEqual(graph.config_units(change, units), {})
        document['nodes'].pop('mega-lander/workspaces.py#integration')
        self.assertEqual(set(graph.config_units(change, units)), {units[0]})

    def test_scratch_fixture_config_reaches_imported_readers_and_mutations(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/session-start-scratch.test.sh',
                 'scripts/lib/mutation-harness-guards.test.sh', 'scripts/scratch-reaper.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        reaper = graph.closure('scripts/scratch-reaper.sh')
        for key in ('SCRATCH_REAPER_ENABLE', 'SCRATCH_SYSTEM_FLAGS',
                    'APP_TEST_INSTANCE_PROCESS_NAMES', 'SCRATCH_ROOT_NAME'):
            self.assertIn(key, reaper['keys'])
            self.assertEqual(graph.config_units(inputs.config_change(key+'=old', key+'=new'), units), {})
        document['nodes'].pop('scripts/lib/appinstances.py')
        change = inputs.config_change('BAN_WORKFLOW_TOOL=1', 'BAN_WORKFLOW_TOOL=0')
        self.assertEqual(set(graph.config_units(change, units)), set(units))

    def test_root_and_banner_fixtures_do_not_read_shipped_setting_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/engine-status.test.sh', 'scripts/lib/resolve-roots.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        for key in ('PROTECTED_PATHS', 'AGENT_NAMESPACE_ROOTS', 'BAN_WORKFLOW_TOOL'):
            self.assertEqual(graph.config_units(inputs.config_change(key+'=old', key+'=new'), units), {})
        document['nodes'].pop('scripts/lib/registered-hooks.sh')
        change = inputs.config_change('PROTECTED_PATHS=before', 'PROTECTED_PATHS=after')
        self.assertEqual(set(graph.config_units(change, units)), {units[0]})

    def test_staleness_and_record_guards_keep_copies_distinct_from_real_readers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/hook-staleness.test.sh', 'scripts/hooks/guard-public-record-repo.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        self.assertEqual(set(graph.closure('scripts/hooks/guard-public-record-repo.sh')['keys']),
                         {'PUBLIC_RECORD_REPO_GUARD', 'PUBLICATION_DECLARATION'})
        for key in ('PUBLIC_RECORD_REPO_GUARD', 'PUBLICATION_DECLARATION', 'PROTECTED_PATHS'):
            change = inputs.config_change(key+'=before', key+'=after')
            self.assertEqual(graph.config_units(change, units), {})
        document['nodes'].pop('scripts/lib/git-jurisdiction.sh')
        self.assertEqual(set(graph.config_units(change, units)), {units[1]})

    def test_claims_and_premise_fixtures_replace_real_settings_through_helpers(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/agent-state-claims.test.sh', 'scripts/hooks/premise-ask.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        self.assertEqual(set(graph.closure('scripts/hooks/guard-agent-state-claims.sh')['keys']),
                         {'CHECK_AGENT_STATE_CLAIMS'})
        for key in ('CHECK_AGENT_STATE_CLAIMS', 'CEO_RULINGS_PATHS', 'PROTECTED_PATHS'):
            change = inputs.config_change(key+'=before', key+'=after')
            self.assertEqual(graph.config_units(change, units), {})
        document['nodes'].pop('scripts/lib/stop-session-recovery.py')
        self.assertEqual(set(graph.config_units(change, units)), {units[0]})

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="dependency-closure.")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.map = {"schema": 1, "config_keys": ["A", "B", "PLANTED"], "nodes": {}, "units": {}}

    def test_scanner_transport_retains_keys_and_deferral_fixture_replaces_them(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        scan, deferral = 'scripts/hooks/scan-secrets.test.sh', 'scripts/hooks/unasked-deferral.test.sh'
        keys = {'SECRET_SCAN_MIN_LENGTH', 'SECRET_SCAN_MIN_ENTROPY',
                'SECRET_SCAN_ALLOWLIST', 'SECRET_SCAN_CODE_AWARE'}
        self.assertEqual(set(graph.closure(scan)['keys']), keys)
        self.assertEqual(graph.closure(deferral)['keys'], {})
        for unit in (scan, deferral):
            self.assertEqual(graph.closure(unit)['whole'], [])
            self.assertEqual(graph.closure(unit)['fallback'], [])
        for key in keys:
            change = inputs.config_change(key+'=before', key+'=after')
            self.assertEqual(set(graph.config_units(change, [scan, deferral])), {scan})
        change = inputs.config_change('PROTECTED_PATHS=app', 'PROTECTED_PATHS=engine')
        self.assertEqual(graph.config_units(change, [scan, deferral]), {})
        document['nodes'].pop('scripts/hooks/guard-unasked-deferral.py')
        self.assertEqual(set(graph.config_units(change, [scan, deferral])), {deferral})

    def test_notice_and_stop_predicate_closures_are_independent_of_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/guard-stop-live-work.test.sh',
                 'scripts/hooks/unevaluated-notice.test.sh',
                 'scripts/hooks/session-start-ci-surface.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        change = inputs.config_change('BAN_WORKFLOW_TOOL=1', 'BAN_WORKFLOW_TOOL=0')
        self.assertEqual(graph.config_units(change, units), {})
        document['nodes'].pop('mega-lander/workspaces.py#liveness-readers')
        self.assertEqual(set(graph.config_units(change, units)), {units[0]})

    def test_serial_runner_fixtures_and_ci_discovery_do_not_read_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/run-all-tests.test.sh', 'scripts/ci-surface-watch.test.sh',
                 'scripts/lib/ci-surface.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {}, closure)
            self.assertEqual(closure['whole'], [], closure)
            self.assertEqual(closure['fallback'], [], closure)
        for key in ('PROTECTED_PATHS', 'BAN_WORKFLOW_TOOL', 'SESSION_TEAMS_DIR'):
            self.assertEqual(graph.config_units(inputs.config_change(key+'=before', key+'=after'), units), {})
        document['nodes'].pop('scripts/lib/ci_pause.py')
        change = inputs.config_change('BAN_WORKFLOW_TOOL=1', 'BAN_WORKFLOW_TOOL=0')
        self.assertEqual(set(graph.config_units(change, units)), set(units[1:]))

    def test_dialect_copy_preserves_readers_and_ledger_copy_uses_fixture_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        dialect = 'scripts/hooks/dialect-guard.test.sh'
        ledger = 'scripts/hooks/guard-reference-ledger.test.sh'
        expected = {'DIALECT_TARGET', 'DIALECT_SCAN_ALLOWLIST', 'DIALECT_EXEMPT_PATHS',
                    'VENDORING_DECLARATION'}
        self.assertEqual(set(graph.closure(dialect)['keys']), expected)
        self.assertEqual(graph.closure(ledger)['keys'], {})
        for unit in (dialect, ledger):
            self.assertEqual(graph.closure(unit)['fallback'], [])
            self.assertEqual(graph.closure(unit)['whole'], [])
        for key in expected:
            change = inputs.config_change(key+'=before', key+'=after')
            self.assertEqual(set(graph.config_units(change, [dialect, ledger])), {dialect})
        change = inputs.config_change('BAN_WORKFLOW_TOOL=1', 'BAN_WORKFLOW_TOOL=0')
        self.assertEqual(graph.config_units(change, [dialect, ledger]), {})
        document['nodes'].pop('scripts/lib/vendored-material.sh')
        self.assertEqual(set(graph.config_units(change, [dialect, ledger])), {dialect})

    def test_source_linters_keep_generated_config_and_scanned_commands_as_data(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/scratch-allocation-lint.test.sh', 'scripts/entrypoint-currency-lint.test.sh']
        for unit in units:
            self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        for key in ('ENTRYPOINTS', 'INSTRUCTION_SURFACES', 'BAN_WORKFLOW_TOOL'):
            self.assertEqual(graph.config_units(inputs.config_change(key+'=old', key+'=new'), units), {})
        document['nodes'].pop('scripts/hooks/contract-integrity-layer-ep.sh')
        selected = graph.config_units(inputs.config_change('ENTRYPOINTS=old', 'ENTRYPOINTS=new'), units)
        self.assertEqual(set(selected), {units[1]})

    def test_harness_controls_and_locator_do_not_consume_copied_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/lib/mutation-focus.test.sh', 'scripts/lib/mutation-harness.test.sh',
                 'scripts/locate-engine.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['keys'], {})
            self.assertEqual(closure['whole'], [])
            self.assertEqual(closure['fallback'], [])
        self.assertTrue(graph.closure(units[1])['presence'])
        change = inputs.config_change('PROTECTED_PATHS=before', 'PROTECTED_PATHS=after')
        self.assertEqual(graph.config_units(change, units), {})
        document['nodes'].pop('scripts/lib/mutation-harness.sh')
        self.assertEqual(set(graph.config_units(change, units)), set(units[:2]))

    def test_definition_and_seat_fixtures_do_not_read_real_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        units = ['scripts/hooks/guard-definition-drift.test.sh', 'scripts/lib/seat-jurisdiction.test.sh']
        for unit in units:
            closure = graph.closure(unit)
            self.assertEqual(closure['fallback'], [], unit)
            self.assertEqual(closure['keys'], {}, unit)
            self.assertEqual(closure['whole'], [], unit)
        before = (HERE.parent / 'orchestration.config').read_text()
        change = inputs.config_change(before, before + '\nPLANTED_UNUSED_SETTING=1\n')
        self.assertEqual(graph.config_units(change, units), {})
        source = 'scripts/hooks/snapshot-agent-definitions.sh'
        def changed(path):
            value = (HERE.parent / path).read_text()
            return value + '\n: "$NEW_READ"\n' if path == source else value
        stale = inputs.Dependencies(HERE.parent, document, read=changed)
        self.assertEqual(set(stale.config_units(change, units)), {units[0]})

    def test_hook_dependency_scanner_parses_source_without_consuming_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/lib/hook-dependencies.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(closure['keys'], {})
        self.assertEqual(closure['whole'], [])
        before = (HERE.parent / 'orchestration.config').read_text()
        self.assertEqual(graph.config_units(inputs.config_change(before, before + '\nPLANTED_UNUSED_SETTING=1'), [unit]), {})
        document['nodes'].pop('scripts/lib/hook-dependencies.py')
        self.assertTrue(inputs.Dependencies(HERE.parent, document).closure(unit)['fallback'])

    def test_global_witness_and_private_installer_have_no_real_config_dependency(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/lib/global-state-witness.test.sh'
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(closure['keys'], {})
        self.assertEqual(closure['whole'], [])
        before = (HERE.parent / 'orchestration.config').read_text()
        self.assertEqual(graph.config_units(inputs.config_change(before, before + '\nPLANTED_UNUSED_SETTING=1'), [unit]), {})
        document['nodes'].pop('scripts/hooks/install.sh#private-home')
        self.assertTrue(inputs.Dependencies(HERE.parent, document).closure(unit)['fallback'])

    def node(self, name, content, **contract):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        row = {"source": name, "sha256": hashlib.sha256(content.encode()).hexdigest(),
               "evidence": "controlled fixture read/call relationship", "keys": [], "edges": [], **contract}
        self.map["nodes"][name] = row
        return row

    def selected(self, before, after, units):
        return inputs.Dependencies(self.root, self.map).config_units(inputs.config_change(before, after), units)

    def test_direct_nested_subprocess_and_both_named_copy_helpers(self):
        self.node("reader.sh", 'printf "%s" "$A"\n', keys=["A"])
        self.node("nested.sh", "bash reader.sh\n", edges=[{"to": "reader.sh"}])
        copies = ["direct-copy.sh", "lib/mutation-harness.sh", "hooks/contract-integrity-probe.sh"]
        for helper in copies:
            self.node(helper, 'cp orchestration.config fixture/orchestration.config\nbash nested.sh\n',
                presence="transport needs source file", edges=[{"to": "nested.sh"}])
        self.node("unrelated.sh", 'printf "%s" "$B"\n', keys=["B"])
        selected = self.selected("A=1\nB=1", "A=2\nB=1", [*copies, "unrelated.sh"])
        self.assertEqual(set(selected), set(copies))
        for reasons in selected.values():
            self.assertTrue(any("nested.sh -> reader.sh" in reason for reason in reasons))
        self.assertEqual(set(self.selected("A=1\nB=1", "A=1\nB=2", [*copies, "unrelated.sh"])), {"unrelated.sh"})

    def test_copy_overrides_whole_content_presence_and_full_fixture_replacement(self):
        self.node("reader.sh", 'printf "%s" "$A $B"\n', keys=["A", "B"])
        self.node("override.sh", "copy_then_override\n", edges=[{"to": "reader.sh", "overrides": ["A"]}])
        self.node("private.sh", "new_fixture_config\n", edges=[{"to": "reader.sh", "fixture": True}])
        self.node("bytes.sh", "sha256sum orchestration.config\n", whole="hashes every byte")
        self.node("adopt.sh", "test -f orchestration.config\n", presence="checks adoption")
        units = ["override.sh", "private.sh", "bytes.sh", "adopt.sh"]
        self.assertEqual(set(self.selected("A=1\nB=1", "A=2\nB=1", units)), {"bytes.sh"})
        self.assertEqual(set(self.selected("A=1\nB=1", "A=1\nB=2", units)), {"override.sh", "bytes.sh"})
        self.assertEqual(set(self.selected("A=1\nB=1", "A=1\nB=1\n# comment", units)), {"bytes.sh"})
        self.assertIn("adopt.sh", self.selected(None, "A=1\nB=1", units))

    def test_new_helper_and_key_selects_only_its_consumer_even_before_qualification(self):
        self.node("consumer.sh", "bash new-helper.sh\n", edges=[{"to": "new-helper.sh"}])
        self.node("unrelated.sh", 'printf "%s" "$B"\n', keys=["B"])
        self.node("global.sh", "validate all config\n", whole="global config validation")
        units = ["consumer.sh", "unrelated.sh", "global.sh"]
        before, after = "A=1\nB=1", "A=1\nB=1\nPLANTED=1"
        selected = self.selected(before, after, units)
        self.assertEqual(set(selected), {"consumer.sh", "global.sh"})
        self.assertIn("unqualified reader new-helper.sh", " ".join(selected["consumer.sh"]))
        self.node("new-helper.sh", 'printf "%s" "$PLANTED"\n', keys=["PLANTED"])
        selected = self.selected(before, after, units)
        self.assertEqual(set(selected), {"consumer.sh", "global.sh"})
        self.assertNotIn("unresolved", " ".join(selected["consumer.sh"]))
        self.assertEqual(set(self.selected(after, after + "\nUNUSED=1\n", units)), {"global.sh"})

    def test_known_omission_modified_helper_and_unbounded_eval_fail_closed(self):
        self.node("omitted.sh", 'printf "%s" "$A"\n')
        self.node("changed.sh", 'printf "%s" "$A"\n', keys=["A"])
        (self.root / "changed.sh").write_text('printf "%s" "$A $B"\n')
        self.node("dynamic.sh", 'eval "printf %s \\${$UNKNOWN}"\n')
        self.node("unrelated.sh", "true\n")
        units = ["omitted.sh", "changed.sh", "dynamic.sh", "unrelated.sh"]
        selected = self.selected("A=1\nB=1", "A=1\nB=2", units)
        self.assertEqual(set(selected), set(units) - {"unrelated.sh"})
        self.assertIn("omitted known key", " ".join(selected["omitted.sh"]))
        self.assertIn("changed reader", " ".join(selected["changed.sh"]))
        self.assertIn("dynamic evaluation", " ".join(selected["dynamic.sh"]))

    def test_finite_indirect_alias_is_bound_to_helper_content(self):
        row = self.node("indirect.sh", 'CONFIG_KEY="A"\neval "printf %s \\${$CONFIG_KEY}"\n',
                        keys=["A"], indirect={"CONFIG_KEY": "A"})
        self.assertEqual(set(self.selected("A=1\nB=1", "A=2\nB=1", ["indirect.sh"])), {"indirect.sh"})
        self.assertFalse(self.selected("A=1\nB=1", "A=1\nB=2", ["indirect.sh"]))
        row["indirect"] = {"CONFIG_KEY": "B"}
        self.assertIn("indirect.sh", self.selected("A=1\nB=1", "A=1\nB=2", ["indirect.sh"]))

    def test_unsupported_config_syntax_selects_sourcing_readers_with_reason(self):
        self.node("reader.sh", 'printf "%s" "$A"\n', keys=["A"])
        self.node("unrelated.sh", "true\n")
        selected = self.selected("A=1", "A=$(command)", ["reader.sh", "unrelated.sh"])
        self.assertEqual(set(selected), {"reader.sh"})
        self.assertIn("config syntax fallback", " ".join(selected["reader.sh"]))

    def test_real_finite_aliases_and_copy_regions_match_reviewed_sources(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        asks = graph.closure("scripts/lib/ceo-asks.sh")
        ruled = graph.closure("scripts/lib/ceo-ruled.sh")
        self.assertEqual(set(asks["keys"]), {"CEO_TODOS_REPOS"})
        self.assertEqual(set(ruled["keys"]), {"CEO_RULINGS_PATHS", "CEO_TODOS_REPOS"})
        self.assertFalse(asks["fallback"] + ruled["fallback"])
        for name in ("scripts/lib/mutation-harness.sh#copy", "scripts/hooks/contract-integrity-probe.sh#workspace-copy"):
            closure = graph.closure(name)
            self.assertEqual(closure["keys"], {})
            self.assertEqual(closure["whole"], [])
            self.assertTrue(closure["presence"])
            self.assertEqual(closure["fallback"], [])

    def test_real_direct_and_transitive_key_readers_in_reviewed_subset(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        guards = {"scripts/hooks/guard-main-checkout-writes.test.sh",
                  "scripts/hooks/guard-bash-main-writes.test.sh"}
        units = sorted(guards | {"scripts/check-census.test.sh", "scripts/hooks/ceo-asks.test.sh"})
        for unit in units:
            self.assertEqual(graph.closure(unit)["fallback"], [], unit)
        for key, expected in (("PROTECTED_PATHS", guards),
                              ("BAN_WORKFLOW_TOOL", {"scripts/check-census.test.sh"})):
            change = inputs.config_change(key + "=before\n", key + "=after\n")
            self.assertEqual(set(graph.config_units(change, units)), expected)
        # Removing a real subprocess edge must not hide the indirect read.
        document["nodes"]["scripts/check-census.test.sh"]["edges"] = [
            edge for edge in document["nodes"]["scripts/check-census.test.sh"]["edges"]
            if edge["to"] != "scripts/hooks/guard-workflow-ban.sh"]
        closure = graph.closure("scripts/check-census.test.sh")
        self.assertIn("omitted known execute edges", " ".join(closure["fallback"]))

    def test_real_private_config_remains_independent_through_nested_mutants(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = "scripts/hooks/ceo-asks.test.sh"
        closure = graph.closure(unit)
        self.assertEqual(closure, {"keys": {}, "whole": [], "presence": [], "fallback": []})
        self.assertFalse(graph.config_units(inputs.config_change("CEO_TODOS_REPOS=a", "CEO_TODOS_REPOS=b"), [unit]))
        # A broken nested helper qualification still blocks exclusion. Private
        # fixture replacement cannot erase uncertainty about executed code.
        document["nodes"]["scripts/lib/ceo-todos.py#items"]["sha256"] = "unreviewed"
        self.assertIn(unit, graph.config_units(inputs.config_change("A=1", "A=2"), [unit]))

    def test_model_ceiling_mutants_transport_config_without_consuming_its_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-model-ceiling.test.sh'
        closure = graph.closure(unit)
        self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
        self.assertTrue(closure['presence'])
        for key in ('MODEL_CEILING', 'MODEL_TIERS', 'PROTECTED_PATHS', 'SCRATCH_DEFAULT_TTL_MINUTES'):
            self.assertFalse(graph.config_units(inputs.config_change(key+'=one', key+'=two'), [unit]), key)
        self.assertIn(unit, graph.config_units(inputs.config_change(None, 'MODEL_CEILING=one'), [unit]))
        document['nodes']['scripts/lib/resolve-model.sh']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=one', 'MODEL_CEILING=two'), [unit]))

    def test_ingress_gates_use_only_private_entity_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/ceo-inputs.test.sh'
        self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        document['nodes']['scripts/hooks/scan-secrets.sh']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('SECRET_SCAN_ALLOWLIST=a', 'SECRET_SCAN_ALLOWLIST=b'), [unit]))

    def test_literal_mutation_operands_are_content_bound_and_need_an_explanation(self):
        row = self.node('mutation.sh', "printf '%s' '$A'\n", literal_keys={'A':'single-quoted source replacement operand'})
        self.assertFalse(self.selected('A=old', 'A=new', ['mutation.sh']))
        row['literal_keys'] = {'A': ''}
        selected = self.selected('A=old', 'A=new', ['mutation.sh'])
        self.assertIn('require evidence', ' '.join(selected['mutation.sh']))

    def test_ruling_gate_and_drift_mutants_use_private_config_values(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/ceo-ruled.test.sh'
        self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
        self.assertFalse(graph.config_units(inputs.config_change('CEO_RULINGS_PATHS=a', 'CEO_RULINGS_PATHS=b'), [unit]))
        document['nodes']['scripts/lib/premise-ask.sh']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('CEO_TODOS_REPOS=a', 'CEO_TODOS_REPOS=b'), [unit]))

    def test_global_validator_is_a_qualified_whole_config_obligation(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = "scripts/verification-config.test.sh"
        closure = graph.closure(unit)
        self.assertEqual(closure["fallback"], [])
        self.assertTrue(closure["whole"])
        self.assertIn(unit, graph.config_units(inputs.config_change("A=1", "A=1\nUNUSED=2"), [unit]))

    def test_protocol_installers_and_stubbed_ci_do_not_read_real_config(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        helpers = {"scripts/ci-verify.test.sh": "scripts/ci-verify.sh#stubbed-steps",
            "scripts/install-ack-protocol.test.sh": "scripts/install-ack-protocol.sh",
            "scripts/install-escalation-protocol.test.sh": "scripts/install-escalation-protocol.sh"}
        for unit, helper in helpers.items():
            with self.subTest(unit=unit):
                self.assertEqual(graph.closure(unit), {"keys": {}, "whole": [], "presence": [], "fallback": []})
                original = document["nodes"][helper]["sha256"]
                document["nodes"][helper]["sha256"] = "unreviewed"
                self.assertIn(unit, graph.config_units(inputs.config_change("MODEL_CEILING=a", "MODEL_CEILING=b"), [unit]))
                document["nodes"][helper]["sha256"] = original

    def test_slot_priority_and_pause_fixtures_have_reviewed_config_closures(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        slot = "scripts/lib/engine_pass.test.sh"
        priority = "scripts/lib/worker-priority.test.sh"
        pause = "scripts/lib/pause_protocol.test.sh"
        for unit in (slot, priority, pause):
            closure = graph.closure(unit)
            self.assertFalse(closure["whole"] or closure["fallback"], closure)
        self.assertEqual(set(graph.closure(pause)["keys"]), {"SESSION_TEAMS_DIR"})
        self.assertTrue(graph.closure(slot)["presence"])
        change = inputs.config_change("BAN_WORKFLOW_TOOL=1", "BAN_WORKFLOW_TOOL=0")
        self.assertEqual(graph.config_units(change, [slot, priority, pause]), {})
        # A valid captured pause now continues past syntax validation. Its
        # recipient lookup and identity readers cannot be omitted or stale.
        for helper in ("scripts/lib/teammate-identity.py",
                       "mega-lander/workspaces.py#empty-recipient-fixture"):
            previous = document["nodes"][helper]["sha256"]
            document["nodes"][helper]["sha256"] = "changed"
            self.assertIn(pause, graph.config_units(change, [pause]))
            document["nodes"][helper]["sha256"] = previous
        fixture_sha = document["nodes"]["scripts/lib/verification-fixture.sh"]["sha256"]
        document["nodes"]["scripts/lib/verification-fixture.sh"]["sha256"] = "changed"
        self.assertEqual(set(graph.config_units(change, [slot, priority, pause])), {slot, priority})
        document["nodes"]["scripts/lib/verification-fixture.sh"]["sha256"] = fixture_sha
        document["nodes"]["scripts/lib/worker_tokens.py"]["sha256"] = "changed"
        self.assertIn(priority, graph.config_units(change, [priority]))

    def test_sealed_guard_copy_keeps_real_readers_and_only_masks_the_fixture_override(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        unit = 'scripts/hooks/guard-sealed-worktree.test.sh'
        expected = {'READONLY_ALLOWLIST', 'HARNESS_UTILITY_TYPES', 'SEAL_READONLY_TOOLS',
                    'APP_TEST_INSTANCE_PROCESS_NAMES', 'APP_TEST_INSTANCE_BUNDLE_ID',
                    'APP_TEST_INSTANCE_REAL_HOME_PATHS', 'APP_TEST_INSTANCE_QUIT_GRACE_SECONDS',
                    'APP_TEST_INSTANCE_KILL_GRACE_SECONDS'}
        closure = graph.closure(unit)
        self.assertEqual(closure['fallback'], [])
        self.assertEqual(set(closure['keys']), expected)
        self.assertFalse(closure['whole'])
        self.assertTrue(closure['presence'])
        for key in expected | {'SEAL_WAIT_SECONDS', 'SCRATCH_ROOT_NAME', 'MODEL_CEILING'}:
            selected = graph.config_units(inputs.config_change(key+'=old', key+'=new'), [unit])
            self.assertEqual(bool(selected), key in expected, key)
        document['nodes']['scripts/lib/appinstances.py#workspace-cleanup']['sha256'] = 'changed'
        self.assertIn(unit, graph.config_units(inputs.config_change('MODEL_CEILING=a', 'MODEL_CEILING=b'), [unit]))

    def test_payload_guards_and_private_ruling_fixtures_do_not_consume_copied_settings(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        cases = {
            'guard-interactive-prompt': 'scripts/lib/interactive-prompt.py#payload',
            'guard-host-display-power': 'scripts/lib/ceo-ruled.sh',
            'guard-no-home-network-phone': 'scripts/lib/ceo-ruled.sh',
        }
        for name, helper in cases.items():
            with self.subTest(name=name):
                unit = 'scripts/hooks/'+name+'.test.sh'
                closure = graph.closure(unit)
                self.assertFalse(closure['keys'] or closure['whole'] or closure['fallback'], closure)
                self.assertTrue(closure['presence'])
                change = inputs.config_change('CEO_RULINGS_PATHS=old', 'CEO_RULINGS_PATHS=new')
                self.assertNotIn(unit, graph.config_units(change, [unit]))
                previous = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper]['sha256'] = previous

    def test_session_notice_and_retired_installer_have_private_config_closures(self):
        document = json.loads((HERE / 'lib/verification-dependencies.json').read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        cases = {'session-start-quota': 'scripts/lib/quota_watch.py#notice',
                 'install-retire-reconciler': 'scripts/lib/registered-hooks.sh'}
        for name, helper in cases.items():
            with self.subTest(name=name):
                unit = 'scripts/hooks/'+name+'.test.sh'
                self.assertEqual(graph.closure(unit), {'keys': {}, 'whole': [], 'presence': [], 'fallback': []})
                change = inputs.config_change('QUOTA_PAUSE_PERCENT=93', 'QUOTA_PAUSE_PERCENT=94')
                self.assertNotIn(unit, graph.config_units(change, [unit]))
                previous = document['nodes'][helper]['sha256']
                document['nodes'][helper]['sha256'] = 'changed'
                self.assertIn(unit, graph.config_units(change, [unit]))
                document['nodes'][helper]['sha256'] = previous

    def test_weekly_policy_fixture_and_real_source_scan_have_different_inputs(self):
        document = json.loads((HERE / "lib/verification-dependencies.json").read_text())
        graph = inputs.Dependencies(HERE.parent, document)
        weekly = "scripts/quota-weekly.test.sh"
        scan = "scripts/no-foreign-app-data.test.sh"
        self.assertEqual(graph.closure(weekly), {"keys": {}, "whole": [], "presence": [], "fallback": []})
        closure = graph.closure(scan)
        self.assertEqual(closure["fallback"], [])
        self.assertTrue(closure["whole"])
        self.assertEqual(set(graph.config_units(inputs.config_change("QUOTA_PAUSE_PERCENT=93", "QUOTA_PAUSE_PERCENT=94"), [weekly, scan])), {scan})
        document["nodes"]["scripts/lib/quota_watch.py#weekly-fixture"]["sha256"] = "changed"
        self.assertIn(weekly, graph.config_units(inputs.config_change("MODEL_CEILING=a", "MODEL_CEILING=b"), [weekly]))


if __name__ == "__main__":
    unittest.main()

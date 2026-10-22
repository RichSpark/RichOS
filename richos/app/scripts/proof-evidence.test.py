#!/usr/bin/env python3
"""Small failure-injection proofs for saved plans and exact receipt reconciliation."""
import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import proof_evidence as evidence
spec = importlib.util.spec_from_file_location("proof_run", HERE / "proof-run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

# The Cache Directory Tagging Specification's signature, as Cargo writes it into a target
# directory (bford.info/cachedir). Literal here, so the fixture never depends on the code under test.
CACHEDIR_SIGNATURE = b"Signature: 8a477f597d28d172789f06886806bc55"

IDLE_SAMPLE = {"cpu_user_percent": 5.0, "cpu_system_percent": 2.0, "cpu_idle_percent": 93.0,
               "swapout_mb_per_s": 0.0, "memory_pressure": "normal", "memory_free_percent": 80,
               "swap_used_mb": 0.0}


def idle_proof_run(script):
    """A command line for `script` (a proof-run.py) whose admission sample is fixed and idle.

    These fixtures prove evidence reuse and joining, not CPU admission, which proof-run.test.py
    covers. The nightly runs this suite beside every other gate on a Mac at up to 100% busy,
    where the real sample held a fixture's one-line check past its 45 s and 20 s deadlines.
    Same bootstrap as proof-run.test.py P8 and test-results.test.sh P1.
    """
    testvm = str(Path(script).parent / "testvm")
    bootstrap = ("import runpy,sys; sys.path.insert(0," + repr(testvm) + "); "
                 "import reserve; reserve.host_sample=lambda *a, **k: " + repr(IDLE_SAMPLE) + "; "
                 "sys.argv=[" + repr(str(script)) + "]+sys.argv[1:]; "
                 "runpy.run_path(sys.argv[0],run_name='__main__')")
    return [sys.executable, "-B", "-c", bootstrap]


class Evidence(unittest.TestCase):
    def test_pin_validator_stays_fresh_instead_of_using_whole_checkout_reuse(self):
        item = SimpleNamespace(label='engine scripts/verification-pins.test.sh', argv=['bash', 'pins'])
        with patch.object(runner, 'execution_environment', return_value={}), \
                patch.object(evidence, 'contract_for', return_value={'fresh': 'pin validator'}), \
                patch.object(evidence, 'command_identity', return_value={}), \
                patch.object(evidence, 'checkout_identity') as checkout:
            identity = runner.input_identity(item, SimpleNamespace(), str(self.root))
        self.assertEqual(identity['fresh'], 'pin validator')
        checkout.assert_not_called()

    def test_host_shield_stand_ins_on_path_are_no_part_of_an_identity(self):
        # `engine receipts` runs inside the shield and re-reads identities the unshielded runner
        # recorded; the stand-in directory first on its PATH must not make them differ.
        shield = self.root / "engine/scripts/lib/host-shield"
        shield.mkdir(parents=True)
        (shield / "pmset").write_text("#!/bin/sh\n# host-shield stand-in (fixture)\nexit 1\n")
        other = self.root / "elsewhere/host-shield"   # same name, no stand-in: a real directory
        other.mkdir(parents=True)
        (other / "tool").write_text("#!/bin/sh\nexit 0\n")
        plain = {"PATH": os.pathsep.join([str(other), "/usr/bin", "/bin"]),
                 "RICHOS_VERIFICATION_TOOL_PATH": "/usr/bin" + os.pathsep + "/bin"}
        shielded = {"PATH": str(shield) + os.pathsep + plain["PATH"],
                    "RICHOS_VERIFICATION_TOOL_PATH": str(shield) + os.pathsep + plain["RICHOS_VERIFICATION_TOOL_PATH"]}
        self.assertEqual(evidence.identity_environment(shielded), plain)
        for name in ("RICHOS_HOST_SHIELD_LOG", "RICHOS_HOST_SHIELD_RUNS", "RICHOS_HOST_SHIELD_TOOL"):
            self.assertIn(name, evidence.RUN_SCOPED_ENVIRONMENT)

    def setUp(self):
        # The runner resolves its log and root paths. macOS's default TMPDIR is under /var,
        # a link to /private/var, so an unresolved fixture root compared as a different
        # path and every resume, profile and snapshot case failed on a stock shell.
        self.tmp = tempfile.TemporaryDirectory(prefix="proof-evidence.",
                                               dir=os.path.realpath(tempfile.gettempdir()))
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "repo"
        self.root.mkdir()
        self.source = {"commit": "a" * 40, "tracked_diff_sha256": "clean", "untracked_sha256": "clean"}
        self.inputs = {"paths": {"fixture": "digest"}}

    def qualification(self, review, **required):
        (self.root / "qualification.md").write_text(review)
        evidence.atomic(self.root / "qualification.json", {"schema": 1,
            "review": "qualification.md", "requires": {
                key: required.get(key, []) for key in ("paths", "tools", "environment", "external")}})

    def test_unit_qualification_binds_reviewed_helpers_and_additional_inputs(self):
        self.qualification("Common floor", paths=['engine'], tools=['bash'])
        (self.root / 'engine').mkdir()
        helper = self.root / 'engine/helper.sh'
        helper.write_text('exit 0\n')
        path = self.root / 'qualification.json'
        contract = json.loads(path.read_text())
        contract['units'] = {'engine fixture': {'review': 'Reads its helper and a root license.',
            'sources': {'engine/helper.sh': evidence.file_digest(helper)},
            'requires': {'paths': ['LICENSE'], 'tools': ['git'],
                         'environment': ['OPTION'], 'external': ['FIXTURE']}}}
        path.write_text(json.dumps(contract))
        recipe = {'paths': ['engine', 'LICENSE'], 'tools': ['bash', 'git'],
            'environment': ['OPTION'], 'external': ['FIXTURE'],
            'qualification': 'qualification.json', 'qualification_unit': 'engine fixture'}
        evidence.qualify_recipe(self.root, recipe)
        for field, remove in [('paths', 'LICENSE'), ('tools', 'git'), ('environment', 'OPTION'), ('external', 'FIXTURE')]:
            with self.subTest(field=field):
                broken = {**recipe, field: [value for value in recipe[field] if value != remove]}
                with self.assertRaisesRegex(ValueError, 'qualification omits '+field):
                    evidence.qualify_recipe(self.root, broken)
        with self.assertRaisesRegex(ValueError, 'missing reviewed unit'):
            evidence.qualify_recipe(self.root, {**recipe, 'qualification_unit': 'unknown'})
        helper.write_text('cat ../outside\n')
        with self.assertRaisesRegex(ValueError, 'changed unit reader'):
            evidence.qualify_recipe(self.root, recipe)
        self.assertIn('changed unit reader', evidence.recipe_identity(self.root, recipe, {})['fresh'])

    def test_literal_external_reader_identity_and_omission(self):
        self.qualification('Reads an optional fixed host guard.')
        external = Path(self.tmp.name) / 'guard.sh'
        contract_path = self.root / 'qualification.json'
        contract = json.loads(contract_path.read_text())
        contract['external_paths'] = [str(external)]
        contract_path.write_text(json.dumps(contract))
        recipe = {'paths': [], 'tools': [], 'environment': [], 'external': [],
            'external_paths': [str(external)], 'qualification': 'qualification.json'}
        missing = evidence.recipe_identity(self.root, recipe, {})
        external.write_text('exit 0\n')
        present = evidence.recipe_identity(self.root, recipe, {})
        self.assertNotEqual(missing['external_paths'], present['external_paths'])
        external.write_text('exit 2\n')
        self.assertNotEqual(present['external_paths'], evidence.recipe_identity(self.root, recipe, {})['external_paths'])
        with self.assertRaisesRegex(ValueError, 'omits external_paths'):
            evidence.recipe_identity(self.root, {**recipe, 'external_paths': []}, {})
        with self.assertRaisesRegex(ValueError, 'absolute literals'):
            evidence.recipe_identity(self.root, {**recipe, 'external_paths': ['relative']}, {})

    def test_subset_ignores_unread_bytes_but_binds_helpers_data_and_inventory(self):
        self.qualification('Fixture reads one helper and data; discovers names.', paths=['engine'])
        (self.root / 'engine').mkdir()
        for name in ('helper', 'data', 'unrelated'):
            (self.root / 'engine' / name).write_text(name)
        contract_path = self.root / 'qualification.json'
        contract = json.loads(contract_path.read_text())
        floor = {'paths': ['engine/data'], 'inventories': ['engine']}
        contract['subset_requires'] = floor
        contract['units'] = {'fixture': {'review': 'Controlled helper.',
            'sources': {'engine/helper': evidence.file_digest(self.root / 'engine/helper')},
            'requires': {'paths': [], 'tools': [], 'environment': [], 'external': []},
            'subset': {'review': 'Only helper and data contents are read.',
                       'requires': {'paths': [], 'inventories': []}}}}
        contract_path.write_text(json.dumps(contract))
        subset = {'paths': ['engine/helper', 'engine/data'], 'inventories': ['engine']}
        recipe = {'paths': ['engine'], 'tools': [], 'environment': [], 'external': [],
            'qualification': 'qualification.json', 'qualification_unit': 'fixture', 'subset': subset}
        identity = lambda: evidence.recipe_identity(self.root, recipe, {})
        baseline = identity()
        (self.root / 'engine/unrelated').write_text('unread change')
        self.assertEqual(baseline, identity())
        data = self.root / 'engine/data'
        data.write_text('changed fixture')
        self.assertNotEqual(baseline, identity())
        data.write_text('data')
        self.assertEqual(baseline, identity())
        added = self.root / 'engine/untracked'
        added.write_text('new discovered file')
        self.assertNotEqual(baseline, identity())
        added.unlink()
        link = self.root / 'engine/link'
        link.symlink_to(data)
        linked = identity()
        data.write_text('new link target')
        self.assertNotEqual(linked, identity())
        link.unlink()
        data.write_text('data')
        for field, value in (('paths', 'engine/helper'), ('paths', 'engine/data'),
                             ('inventories', 'engine')):
            with self.subTest(field=field, value=value):
                broken = {**subset, field: [p for p in subset[field] if p != value]}
                with self.assertRaisesRegex(ValueError, 'subset qualification omits'):
                    evidence.recipe_identity(self.root, {**recipe, 'subset': broken}, {})
        with self.assertRaisesRegex(ValueError, 'outside declared roots'):
            evidence.recipe_identity(self.root, {**recipe,
                'subset': {**subset, 'paths': subset['paths'] + ['outside']}}, {})
        (self.root / 'engine/helper').write_text('new reader')
        self.assertIn('changed unit reader', identity()['fresh'])

    def test_known_omitted_inputs_fail_qualification_before_reuse(self):
        self.qualification("Fixture reads a helper, root license, declared tool and external data.",
            paths=["engine/helper.sh", "LICENSE"], tools=["seq"],
            environment=["OPTION"], external=["FIXTURE"])
        recipe = {"paths": ["engine", "LICENSE"], "tools": ["seq"],
            "environment": ["OPTION"], "external": ["FIXTURE"], "qualification": "qualification.json"}
        evidence.qualify_recipe(self.root, recipe)
        for field in ("paths", "tools", "environment", "external"):
            with self.subTest(field=field):
                broken = {**recipe, field: ["engine"] if field == "paths" else []}
                with self.assertRaisesRegex(ValueError, "qualification omits " + field):
                    evidence.recipe_identity(self.root, broken, {})
        # A similarly named sibling is not coverage of a required directory.
        with self.assertRaisesRegex(ValueError, "engine/helper.sh"):
            evidence.qualify_recipe(self.root, {**recipe, "paths": ["engine-other", "LICENSE"]})

    def test_git_inputs_notice_index_and_engine_epoch_but_allow_unrelated_commit(self):
        self.qualification('Archive reads tracked engine members, terms and the engine epoch.',
                           paths=['engine', 'LICENSE'], tools=['git'])
        declaration = {'tracked': ['engine', 'LICENSE'], 'last_change': ['engine']}
        contract_path = self.root / 'qualification.json'
        contract = json.loads(contract_path.read_text())
        contract['git_inputs'] = declaration
        contract_path.write_text(json.dumps(contract))
        recipe = {'paths': ['engine', 'LICENSE'], 'tools': ['git'], 'environment': [],
                  'external': [], 'qualification': 'qualification.json', 'git_inputs': declaration}
        env = {**os.environ, 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': os.devnull,
               'GIT_AUTHOR_DATE': '2001-01-01T00:00:00Z', 'GIT_COMMITTER_DATE': '2001-01-01T00:00:00Z'}
        for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE'):
            env.pop(key, None)
        def git(*args):
            return subprocess.run(['git', '-C', str(self.root), '-c', 'user.name=Fixture',
                                   '-c', 'user.email=fixture@example.invalid', *args],
                                  env=env, capture_output=True, check=True, timeout=10)
        git('init', '-q')
        (self.root / 'engine').mkdir()
        (self.root / 'engine/source').write_text('source')
        (self.root / 'LICENSE').write_text('terms')
        git('add', 'engine', 'LICENSE')
        git('commit', '-qm', 'source', '--no-gpg-sign')
        identity = lambda: evidence.recipe_identity(self.root, recipe, env)
        baseline = identity()
        # An unrelated landing must retain both the byte and Git input identity.
        (self.root / 'notes').write_text('unrelated documentation')
        git('add', 'notes')
        git('commit', '-qm', 'notes', '--no-gpg-sign')
        self.assertEqual(baseline, identity())
        git('rm', '--cached', 'LICENSE')
        untracked = identity()
        self.assertEqual(baseline['paths'], untracked['paths'])
        self.assertNotEqual(baseline['git_inputs'], untracked['git_inputs'])
        git('add', 'LICENSE')
        self.assertEqual(baseline, identity())
        git('config', 'core.quotepath', 'false')
        self.assertNotEqual(baseline, identity())
        git('config', '--unset', 'core.quotepath')
        # Same final engine bytes with a newer engine history still change mtimes.
        (self.root / 'engine/source').write_text('intermediate')
        env['GIT_AUTHOR_DATE'] = env['GIT_COMMITTER_DATE'] = '2001-01-02T00:00:00Z'
        git('add', 'engine'); git('commit', '-qm', 'intermediate', '--no-gpg-sign')
        (self.root / 'engine/source').write_text('source')
        git('add', 'engine'); git('commit', '-qm', 'restore', '--no-gpg-sign')
        restored = identity()
        self.assertEqual(baseline['paths'], restored['paths'])
        self.assertNotEqual(baseline['git_inputs'], restored['git_inputs'])
        for kind in declaration:
            with self.subTest(kind=kind), self.assertRaisesRegex(ValueError, 'omits git_inputs ' + kind):
                evidence.recipe_identity(self.root, {**recipe, 'git_inputs': {**declaration, kind: []}}, env)

    def test_shared_input_snapshot_keeps_distinct_external_fixtures_and_fresh_boundaries(self):
        source = self.root / 'input'
        source.write_text('one')
        first, second = self.root / 'first-fixture', self.root / 'second-fixture'
        first.write_text('first'); second.write_text('second')
        self.qualification('Shared source, distinct named external fixture.',
                           paths=['input'], external=['FIXTURE'])
        recipe = {'paths': ['input'], 'tools': [], 'environment': [], 'external': ['FIXTURE'],
                  'qualification': 'qualification.json'}
        snapshot = evidence.InputSnapshot()
        with patch.object(evidence, 'path_identity', wraps=evidence.path_identity) as reads:
            left = evidence.recipe_identity(self.root, recipe, {'FIXTURE': str(first)}, snapshot)
            right = evidence.recipe_identity(self.root, recipe, {'FIXTURE': str(second)}, snapshot)
            self.assertEqual(sum(call.args[0] == source for call in reads.call_args_list), 1)
        self.assertNotEqual(left['external'], right['external'])
        self.assertEqual(left, evidence.recipe_identity(self.root, recipe, {'FIXTURE': str(first)}))
        before = source.stat()
        source.write_text('two')
        os.utime(source, ns=(before.st_atime_ns, before.st_mtime_ns))
        after = evidence.recipe_identity(self.root, recipe, {'FIXTURE': str(first)}, evidence.InputSnapshot())
        self.assertNotEqual(left['paths'], after['paths'])

    def test_source_change_during_final_input_snapshot_rereads_and_judges_each_check_by_its_inputs(self):
        # The checkout changes while finalize() reads the inputs. A check whose own inputs read
        # differently the second time is invalid; one whose inputs read the same both times is
        # not, because the change was somewhere else (part-2 hunt section 07).
        for moving in (True, False):
            with self.subTest(own_inputs_moving=moving):
                items, record = self.attempt('final-snapshot-%s' % moving)
                self.passed(items, record)
                reads = []
                def inputs(selected):
                    record.current_source = lambda: {**self.source, 'untracked_sha256': 'changed-during-read'}
                    reads.append(1)
                    same = len(reads) == 1 or not moving
                    return {item.label: (self.inputs if same else {'paths': {'fixture': 'moved'}})
                            for item in selected}
                record.current_identities = inputs
                record.finalize(items)
                self.assertEqual(len(reads), 2, 'the inputs are read again after the checkout moved')
                if moving:
                    self.assertEqual(items[0].state, 'invalid')
                    self.assertEqual(record.results['check']['exit'], 125)
                    self.assertIn('while they were being read', record.results['check']['invalid'])
                else:
                    self.assertEqual(items[0].state, 'passed', items[0].notes)

    def test_unreadable_final_input_snapshot_preserves_non_green_outcome(self):
        items, record = self.attempt('unreadable-snapshot')
        self.passed(items, record)
        record.current_identities = lambda selected: (_ for _ in ()).throw(ValueError('cyclic input link'))
        record.finalize(items)
        self.assertEqual(items[0].state, 'invalid')
        self.assertIn('cyclic input link', record.results['check']['invalid'])

    def test_private_runtime_binds_native_credentials_and_inherited_creation_mask(self):
        program = ("import json,os,sys; os.umask(int(sys.argv[1])); "
                   "sys.path.insert(0,sys.argv[2]); import proof_evidence as e; "
                   "r=e.python_runtime(sys.executable); "
                   "print(json.dumps({k:r[k] for k in ('credentials','umask')}))")
        rows = []
        for mask in (0o022, 0o077):
            result = subprocess.run([sys.executable, '-B', '-c', program, str(mask), str(HERE / 'lib')],
                                    capture_output=True, text=True, check=True, timeout=10)
            rows.append(json.loads(result.stdout))
            self.assertEqual(rows[-1]['umask'], mask)
            self.assertEqual(rows[-1]['credentials'], [os.geteuid(), os.getegid(), sorted(os.getgroups())])
        self.assertNotEqual(rows[0], rows[1])

    def attempt(self, name, engine=True, identities=None):
        path = Path(self.tmp.name) / name
        path.mkdir()
        items = [runner.Item("check", str(self.root),
            ["bash", "scripts/ci-shard.sh", "--only-units", "fixture.test.sh",
             "--receipt", str(path / "engine-receipts" / "unit.jsonl")] if engine else ["bash", "fixture.sh"])]
        record = evidence.Record(str(self.root), path, items, dict(self.source),
                                 identities or {"check": dict(self.inputs)})
        self.addCleanup(record.close)
        return items, record

    def passed(self, items, record):
        item = items[0]
        item.state, item.rc = "passed", 0
        item.log = str(record.logdir / "execution.log")
        Path(item.log).write_text("real execution output\n")
        receipt = evidence.receipt_path(item)
        if receipt:
            receipt.parent.mkdir()
            receipt.write_text(json.dumps({"unit": "fixture.test.sh", "sha": self.source["commit"],
                "schema": 2, "execution_status": "completed", "verdict": "PASS", "rc": 0, "expected_rc": 0}) + "\n")
        record.checkpoint(items)

    def test_saved_pass_survives_missing_summary_and_original_directory_rotation(self):
        old, previous = self.attempt("previous")
        self.passed(old, previous)
        new, current = self.attempt("retry")
        evidence.reuse(previous.logdir, new, current)
        self.assertEqual(new[0].state, "passed")
        self.assertFalse((previous.logdir / "summary.json").exists())
        shutil.rmtree(previous.logdir)
        self.assertEqual(Path(new[0].log).read_text(), "real execution output\n")
        evidence.completed_receipt(new[0], self.source["commit"])
        self.assertIn("provenance", current.results["check"])
        plan = current.logdir / "units"
        plan.write_text("fixture.test.sh\n")
        verifier = HERE.parents[1] / "engine/scripts/lib/ci-receipts.py"
        result = subprocess.run([sys.executable, str(verifier), "verify", "--plan", str(plan)],
            input=evidence.receipt_path(new[0]).read_text(), capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_noncompleted_or_damaged_evidence_never_reuses(self):
        for damage in ("missing", "duplicate", "refused", "known-red", "log", "source", "inputs"):
            with self.subTest(damage=damage):
                old, previous = self.attempt("old-" + damage)
                self.passed(old, previous)
                receipt = evidence.receipt_path(old[0])
                if damage == "missing":
                    receipt.unlink()
                elif damage == "duplicate":
                    receipt.write_text(receipt.read_text() * 2)
                elif damage in ("refused", "known-red"):
                    row = json.loads(receipt.read_text())
                    row["execution_status"] = "refused" if damage == "refused" else "completed"
                    row["verdict"] = "KNOWN-RED"
                    receipt.write_text(json.dumps(row) + "\n")
                    # Even a matching artifact digest cannot promote a non-PASS receipt.
                    previous.results["check"]["receipt"]["sha256"] = evidence.file_digest(receipt)
                    evidence.atomic(previous.logdir / "outcomes.json", previous.results)
                elif damage == "log":
                    Path(old[0].log).write_text("replacement output")
                new, current = self.attempt("new-" + damage)
                if damage == "source":
                    current.source = {**self.source, "untracked_sha256": "changed"}
                elif damage == "inputs":
                    current.identities["check"] = {"tools": "changed"}
                evidence.reuse(previous.logdir, new, current)
                self.assertEqual(new[0].state, "waiting")
                self.assertFalse(evidence.receipt_path(new[0]).exists())

    def test_unfinished_first_and_failures_retained(self):
        old, previous = self.attempt("old", engine=False)
        old[0].state, old[0].rc = "failed", 1
        previous.checkpoint(old)
        frozen = (previous.logdir / "outcomes.json").read_bytes()
        new, current = self.attempt("new", engine=False)
        evidence.reuse(previous.logdir, new, current)
        self.assertTrue(new[0].retry_first)
        self.assertEqual(new[0].state, "waiting")
        self.assertEqual(frozen, (previous.logdir / "outcomes.json").read_bytes())

    def test_fresh_or_undeclared_check_executes_again(self):
        old, previous = self.attempt("old", engine=False)
        self.passed(old, previous)
        new, current = self.attempt("new", engine=False,
            identities={"check": evidence.contract_for(self.root, "check")})
        evidence.reuse(previous.logdir, new, current)
        self.assertEqual(new[0].state, "waiting")
        self.assertIn("no committed input contract", new[0].notes[-1])

    def test_changed_source_or_inputs_during_execution_invalidates_pass(self):
        # A change to the check's own inputs, a source change for a check that declares no
        # inputs, and a moved HEAD each invalidate the pass.
        for change in ("input", "fresh-source", "commit"):
            with self.subTest(change=change):
                identities = {"check": {"fresh": "no contract"}} if change == "fresh-source" else None
                items, record = self.attempt(change, engine=False, identities=identities)
                if change == "input":
                    record.current_identity = lambda item: {"changed": True}
                elif change == "fresh-source":
                    record.current_source = lambda: {**self.source, "tracked_diff_sha256": "changed"}
                else:
                    record.current_source = lambda: {**self.source, "commit": "c" * 40}
                self.passed(items, record)
                self.assertEqual(items[0].state, "invalid")
                self.assertIn("invalid", record.results["check"])
                self.assertEqual(record.source_invalidated, change == "commit",
                                 "only a moved HEAD contaminates the whole run")

    def test_unrelated_source_change_keeps_a_declared_pass_and_contaminates_nothing(self):
        # Part-2 hunt section 07: an edit elsewhere in the checkout used to mark the run
        # contaminated and invalidate every pass, even one whose declared inputs were unchanged.
        items, record = self.attempt("unrelated", engine=False)
        record.current_source = lambda: {**self.source, "tracked_diff_sha256": "an unrelated save"}
        self.passed(items, record)
        self.assertEqual(items[0].state, "passed", items[0].notes)
        self.assertNotIn("invalid", record.results["check"])
        self.assertFalse(record.source_invalidated)
        record.finalize(items)
        self.assertEqual(items[0].state, "passed", items[0].notes)
        # And the saved pass is reusable by a later run whose declared inputs are the same.
        new, current = self.attempt("after-unrelated", engine=False)
        current.source = {**self.source, "tracked_diff_sha256": "an unrelated save"}
        evidence.reuse(record.logdir, new, current, exact=False)
        self.assertEqual(new[0].state, "passed", new[0].notes)
        # The coverage verifier is fresh only so that it always runs; it re-reads each covered
        # unit's own inputs itself, so an unrelated save does not invalidate it either.
        path = Path(self.tmp.name) / "verifier"
        path.mkdir()
        verifier = [runner.Item("engine receipts", str(self.root), ["bash", "scripts/ci-shard.sh", "--verify-receipts"])]
        record = evidence.Record(str(self.root), path, verifier, dict(self.source),
                                 {"engine receipts": {"fresh": "coverage verifier runs over the reconciled receipt set"}})
        self.addCleanup(record.close)
        record.current_source = lambda: {**self.source, "tracked_diff_sha256": "an unrelated save"}
        self.passed(verifier, record)
        record.finalize(verifier)
        self.assertEqual(verifier[0].state, "passed", verifier[0].notes)

    def test_a_checkpoint_pass_reads_the_source_once_however_many_checks_it_saves(self):
        # PRD step 3, T14: one `git diff --binary HEAD` reading took 5.5 s on the round 32 land,
        # and every changed check took one, so 88 checks waited 8 minutes before the first ran.
        path = Path(self.tmp.name) / "many"
        path.mkdir()
        items = [runner.Item("check-%d" % n, str(self.root), ["true"]) for n in range(12)]
        record = evidence.Record(str(self.root), path, items, dict(self.source),
                                 {item.label: self.inputs for item in items})
        self.addCleanup(record.close)
        reads = []
        record.current_source = lambda: reads.append(1) or dict(self.source)
        for item in items:
            item.state, item.rc = "running", None
        record.checkpoint(items)
        self.assertEqual(len(reads), 1, "one reading for a pass that saves twelve checks")
        record.checkpoint(items)
        self.assertEqual(len(reads), 1, "a pass with nothing to save reads nothing")
        self.assertEqual({row["state"] for row in record.results.values()}, {"running"})

    def test_a_check_starting_reads_head_only_never_the_whole_source(self):
        # The other place a reading was taken per check (held cc/zach-opus-reuse58c 2985b8af5,
        # ported): every launch saved its running row with a whole-source reading, one started
        # check after another (5.5 s each on the round 32 land replay). Real runner, six plain
        # checks, a git on PATH counting `git diff --binary`: the plan, the first save, at most
        # one per completion pass and the final validation's two; never one per launch.
        app, _engine = self.copy_runner_fixture()
        names = ["check%d" % n for n in range(6)]
        for name in names:
            (self.root / (name + ".sh")).write_text("exit 0\n")
        def git(*args):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        (self.root / "check0.sh").write_text("exit 0  # an uncommitted change: the tracked diff is not empty\n")
        tools, reads = Path(self.tmp.name) / "tools", Path(self.tmp.name) / "diff-reads"
        tools.mkdir()
        wrapper = tools / "git"
        wrapper.write_text('#!/bin/sh\ncase " $* " in *" diff --binary "*) echo diff >> "%s" ;; esac\n'
                           'exec %s "$@"\n' % (reads, shutil.which("git")))
        wrapper.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RICHOS_", "GIT_"))}
        env.update(PATH=str(tools) + os.pathsep + env.get("PATH", ""),
            RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"))
        commands = Path(self.tmp.name) / "commands"
        commands.write_text("".join("cd . && bash %s.sh\n" % name for name in names))
        directory = Path(self.tmp.name) / "run"
        result = subprocess.run([*idle_proof_run(app / "proof-run.py"), "--commands", str(commands),
            "--log-dir", str(directory)], cwd=self.root, env=env, text=True, capture_output=True, timeout=120)
        detail = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, detail)
        rows = json.loads((directory / "summary.json").read_text())["checks"]
        self.assertEqual([row["result"] for row in rows], ["passed"] * len(names), detail)
        count = reads.read_text().splitlines().count("diff")
        self.assertLessEqual(count, 2 + len(names) + 2,
                             "the whole source was read %d times for %d checks" % (count, len(names)))

    def test_an_untracked_file_that_vanishes_while_it_is_read_is_a_changed_identity_not_a_crash(self):
        # 2026-09-30: a land crashed with FileNotFoundError in source_identity() on
        # richos/app/.updater-key-under-test.key, listed by `git ls-files --others` and removed
        # by updater-setup.test.sh's cleanup before it was opened. The whole-checkout content
        # read (checkout_content) lists and then reads the same way.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        (self.root / "tracked.txt").write_text("tracked\n")
        git("add", "tracked.txt")
        git("commit", "-q", "-m", "fixture")
        key = self.root / "key-under-test.key"
        key.write_text("a key a suite put here\n")
        with patch.object(runner, "ROOT", str(self.root)):
            present = runner.source_identity()
            real_output, real_run = subprocess.check_output, subprocess.run

            def listed_then_removed(listing):
                def call(args, *a, **k):
                    result = listing(args, *a, **k)
                    if "--others" in args and key.exists():
                        key.unlink()  # gone between the listing and the read
                    return result
                return call

            with patch.object(runner.subprocess, "check_output", side_effect=listed_then_removed(real_output)):
                vanished = runner.source_identity()
            self.assertNotEqual(present, vanished, "a vanished file must change the identity")
            key.write_text("a key a suite put here\n")
            before = evidence.checkout_content(self.root)
            with patch.object(evidence.subprocess, "run", side_effect=listed_then_removed(real_run)):
                during = evidence.checkout_content(self.root)
            self.assertNotEqual(before, during, "a vanished file must change the checkout content")
            self.assertEqual(during, evidence.checkout_content(self.root), "and read as absent")

    def test_actual_unrelated_edit_during_a_run_invalidates_only_the_checks_that_read_it(self):
        # Part-2 hunt section 07, end to end through runner.main on a real git checkout: one check
        # (`editor`, no contract, so keyed by the whole checkout) saves a tracked file while
        # another (`reader`, a reviewed contract over reader.test.sh and input.txt) is admitted
        # beside it. Before: the run was marked contaminated, `reader` was stopped or invalidated
        # and a "source changed during verification" failure was added. Now an unrelated save
        # leaves `reader` passed, and a save to input.txt invalidates it.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        (self.root / "input.txt").write_text("read by reader\n")
        (self.root / "notes.txt").write_text("read by nobody but the whole-checkout identity\n")
        (self.root / "reader.test.sh").write_text(
            'i=0; while [ "$i" -lt 600 ]; do [ -f "$1" ] && exit 0; sleep 0.1; i=$((i + 1)); done; exit 1\n')
        (self.root / "editor.test.sh").write_text('printf "edited during the run\\n" > "$2"; : > "$1"\n')
        scripts = self.root / "richos/app/scripts"
        scripts.mkdir(parents=True)
        self.qualification("Controlled shell fixture: reads only its script and input.txt.")
        recipe = {"paths": ["reader.test.sh", "input.txt"], "tools": ["bash"],
                  "external": [], "environment": ["PATH"], "qualification": "qualification.json"}
        evidence.atomic(scripts / "proof-inputs.json", {"schema": 1, "checks": {"reader": recipe}})
        git("add", "-A")
        git("commit", "-q", "-m", "fixture")
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        pools = iter(range(100))
        results = {}
        for target in ("notes.txt", "input.txt"):
            marker = Path(self.tmp.name) / ("marker-" + target)
            lines = [f"cd . && bash reader.test.sh {marker}",
                     f"cd . && bash editor.test.sh {marker} {self.root / target}"]
            logdir = Path(self.tmp.name) / ("run-" + target)
            captured = io.StringIO()
            with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                          "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                    patch.object(runner, "ROOT", str(self.root)), \
                    patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                    patch.object(evidence, "pool_directory",
                                 side_effect=lambda *a: Path(self.tmp.name) / ("pool-%d" % next(pools))), \
                    patch.object(runner, "supply_runtime", return_value="private fixture"), \
                    patch.object(runner.reserve, "host_sample", side_effect=idle), \
                    patch.object(runner, "selection", return_value=lines), \
                    contextlib.redirect_stdout(captured):
                rc = runner.main(["--log-dir", str(logdir)])
            summary = json.loads((logdir / "summary.json").read_text())
            results[target] = ({row["check"]: row["result"] for row in summary["checks"]}, rc, captured.getvalue())
            git("checkout", "--", target)
        states, rc, out = results["notes.txt"]
        self.assertEqual(states.get("reader"), "passed", out[-3000:])
        # The editor wrote into the checkout it was verified in: its own whole-checkout identity
        # moved, so its pass, and only its pass, is invalid.
        self.assertEqual(states.get("editor"), "invalid", out[-3000:])
        self.assertEqual(set(states), {"reader", "editor"}, "no run-wide contamination finding: " + out[-3000:])
        states, rc, out = results["input.txt"]
        self.assertEqual(states.get("reader"), "invalid", out[-3000:])
        self.assertEqual(rc, 1)

    def test_frozen_plan_cannot_be_replaced(self):
        old, previous = self.attempt("old")
        new, current = self.attempt("new")
        current.plan["items"][0]["argv"].append("changed-command")
        with self.assertRaisesRegex(ValueError, "frozen plan"):
            evidence.reuse(previous.logdir, new, current)

    def test_a_saved_plan_holding_one_obligation_twice_is_reused_and_never_written_again(self):
        # 2026-10-05: the merge of cc/echo-opus-out3b crashed reusing attempt-keq_bkth, whose plan
        # held `cargo --bin richos-tauri` twice, row for row identical: proof-for.sh printed that
        # command for src-tauri/src/main.rs and again for src-tauri/Cargo.toml, and plan() kept both.
        old, previous = self.attempt("author", engine=False)
        self.passed(old, previous)
        saved = json.loads((previous.logdir / "plan.json").read_text())
        saved["items"].append(dict(saved["items"][0]))
        (previous.logdir / "plan.json").write_text(json.dumps(saved))
        self.assertEqual([row["check"] for row in evidence.read_plan(previous.logdir)["items"]], ["check"])
        new, current = self.attempt("target", engine=False)
        evidence.reuse(previous.logdir, new, current, exact=False)
        self.assertEqual(new[0].state, "passed")
        # Two different commands under one name stay refused: which of them passed is unknowable.
        saved["items"][1] = dict(saved["items"][0], argv=["bash", "another.sh"])
        (previous.logdir / "plan.json").write_text(json.dumps(saved))
        with self.assertRaisesRegex(ValueError, "duplicate obligations"):
            evidence.read_plan(previous.logdir)
        # No plan is written with one again: the planner keeps one of two identical cargo commands...
        line = "cd richos/app/src-tauri && cargo test --bin richos-tauri"
        planned = runner.plan([line, line], SimpleNamespace(), str(Path(self.tmp.name) / "planned"), {})
        self.assertEqual([item.label for item in planned], ["cargo --bin richos-tauri"])
        # ...and a record refuses two different checks under one name before it writes a plan.
        refused = Path(self.tmp.name) / "refused"
        refused.mkdir()
        twice = [runner.Item("check", str(self.root), ["bash", name]) for name in ("a.sh", "b.sh")]
        with self.assertRaisesRegex(ValueError, "duplicate obligations"):
            evidence.Record(str(self.root), refused, twice, dict(self.source), {"check": dict(self.inputs)})
        self.assertFalse((refused / "plan.json").exists())

    def test_target_reuse_ignores_unrelated_commit_but_preserves_original_receipt(self):
        old, previous = self.attempt("author")
        self.passed(old, previous)
        original = evidence.receipt_path(old[0]).read_bytes()
        self.source = {**self.source, "commit": "b" * 40}
        new, current = self.attempt("integration")
        new[0].lane, new[0].weight = "another-placement", 100
        evidence.reuse(previous.logdir, new, current, exact=False)
        self.assertEqual(new[0].state, "passed")
        self.assertEqual(evidence.receipt_path(new[0]).read_bytes(), original)
        self.assertEqual(current.results["check"]["source"]["commit"], "b" * 40)
        self.assertEqual(current.results["check"]["receipt_sha"], "a" * 40)
        current.finalize(new)
        self.assertEqual(new[0].state, "passed")

    def test_different_target_plan_cannot_change_the_reused_command(self):
        old, previous = self.attempt("author", engine=False)
        old_identity = evidence.command_identity(old[0], self.root, previous.logdir)
        previous.identities["check"] = {"command": old_identity}
        self.passed(old, previous)
        new, current = self.attempt("integration", engine=False)
        new[0].argv.append("different-assertions")
        current.identities["check"] = {"command": evidence.command_identity(new[0], self.root, current.logdir)}
        evidence.reuse(previous.logdir, new, current, exact=False)
        self.assertEqual(new[0].state, "waiting")

    def copy_runner_fixture(self):
        app = self.root / "richos/app/scripts"
        engine = self.root / "richos/engine"
        (app / "lib").mkdir(parents=True)
        (app / "testvm").mkdir()
        (engine / "scripts/lib").mkdir(parents=True)
        source_engine = HERE.parents[1] / "engine/scripts"
        for path in (source_engine / "lib").iterdir():
            if path.suffix in (".py", ".sh", ".tsv") and ".test." not in path.name:
                shutil.copy2(path, engine / "scripts/lib" / path.name)
        for name in ("ci-shard.sh", "ci-units.sh"):
            shutil.copy2(source_engine / name, engine / "scripts" / name)
        # The host shield the copied runners start every check through (ceo-decisions §65): its
        # stand-ins and the guard whose rules it reads. Without them the copied host_shield.py
        # refuses every check, which is its refusal working, not this fixture's subject.
        shutil.copytree(source_engine / "lib/host-shield", engine / "scripts/lib/host-shield")
        (engine / "scripts/hooks").mkdir()
        shutil.copy2(source_engine / "hooks/guard-host-display-power.sh",
                     engine / "scripts/hooks/guard-host-display-power.sh")
        # Every lib/ module proof-run.py imports: rerun_tree.py (write_rerun) and runtime_cache.py
        # (supply_runtime) included.
        for name in ("proof_evidence.py", "proof_slots.py", "test_results.py", "cargo_identity.py",
                     "rerun_tree.py", "runtime_cache.py"):
            shutil.copy2(HERE / "lib" / name, app / "lib" / name)
        shutil.copy2(HERE / "testvm/reserve.py", app / "testvm/reserve.py")
        shutil.copy2(HERE / "proof-run.py", app / "proof-run.py")
        (engine / "VERSION").write_text("1.0.0-test\n")
        return app, engine

    def test_an_authors_pass_with_mutation_passes_on_stands_in_for_the_gates_run_with_them_off(self):
        # Held cc/zach-opus-reuse58c 032729f16, ported: the merge gate runs with
        # RICHOS_MUTATION_PASSES=0 and RICHOS_FOURTEEN_MUTANTS=0 (autocheck MUTATION_SWITCH); the
        # author's handover run leaves them unset, so it ran every assertion the gate's run does,
        # and more. A qualified recipe that binds them (the engine hook suites) was never reused
        # from the offered author run. Real runner, ci-shard.sh and coverage verifier: two
        # qualified units pass in a linked worktree, then the main checkout's gate run is offered
        # that run. Decided by the lead (esc-20261015T175325Z-3b86bfa5): never the other way round.
        app, engine = self.copy_runner_fixture()
        counter = Path(self.tmp.name) / "executions"
        for name in ("alpha", "beta"):
            path = engine / "scripts" / (name + ".test.sh")
            path.write_text('#!/bin/bash\nprintf "' + name + '\\n" >> "$FIXTURE_COUNTER"\n')
            path.chmod(0o755)
        (self.root / "LICENSE").write_text("fixture license\n")
        switches = ["RICHOS_MUTATION_PASSES", "RICHOS_FOURTEEN_MUTANTS"]
        self.qualification("Fixture units append a label; they read the two mutation switches.",
            paths=["richos/engine", "richos/app/scripts", "LICENSE"], tools=["bash", "python3", "git"],
            environment=["FIXTURE_COUNTER", *switches])
        recipe = {"paths": ["richos/engine", "richos/app/scripts", "LICENSE"], "tools": ["bash", "python3", "git"],
            "environment": ["FIXTURE_COUNTER", *switches], "external": [], "qualification": "qualification.json",
            "isolation": evidence.PRIVATE_PROFILE}
        evidence.atomic(app / "proof-inputs.json", {"schema": 1,
            "checks": {"engine scripts/" + name + ".test.sh": dict(recipe) for name in ("alpha", "beta")}})
        # The modes a Git checkout writes, so the worktree's identical bytes are identical inputs
        # (test_actual_worktrees_join_one_execution_and_run_independent_work says why).
        umask = os.umask(0)
        os.umask(umask)
        for written in self.root.rglob("*"):
            if written.is_file() and not written.is_symlink():
                written.chmod((0o777 if written.stat().st_mode & 0o100 else 0o666) & ~umask)
        def git(*args):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        worktree = Path(self.tmp.name) / "author-checkout"
        git("worktree", "add", "-q", "-b", "cc/author", str(worktree))
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RICHOS_", "GIT_"))}
        env.update(RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"),
            FIXTURE_COUNTER=str(counter), PYTHONDONTWRITEBYTECODE="1")
        commands = Path(self.tmp.name) / "commands"
        both = ",".join("scripts/%s.test.sh" % name for name in ("alpha", "beta"))
        commands.write_text("cd richos/engine && " + " ".join(["bash", "scripts/ci-shard.sh", "--only-units", both]) + "\n")
        def invoke(checkout, name, *options, **variables):
            directory = Path(self.tmp.name) / name
            result = subprocess.run([*idle_proof_run(checkout / "richos/app/scripts/proof-run.py"),
                "--commands", str(commands), "--log-dir", str(directory), *options], cwd=checkout,
                env=dict(env, **variables), text=True, capture_output=True, timeout=120)
            detail = result.stdout + result.stderr + "\n" + "\n".join(p.read_text() for p in directory.glob("*.log"))
            self.assertEqual(result.returncode, 0, detail)
            rows = {row["check"]: row for row in json.loads((directory / "summary.json").read_text())["checks"]}
            self.assertEqual(rows["engine receipts"]["result"], "passed", detail)
            return directory, rows, detail
        units = ("engine scripts/alpha.test.sh", "engine scripts/beta.test.sh")
        off = dict.fromkeys(switches, "0")
        # Since 2026-10-22 an unset switch reaches every check as "0" (runner_environment), so
        # the author's run that ran more passes is one launched with them on.
        author, rows, detail = invoke(worktree, "author", RICHOS_MUTATION_PASSES="1", RICHOS_FOURTEEN_MUTANTS="1")
        self.assertEqual(sorted(counter.read_text().splitlines()), ["alpha", "beta"], detail)
        gate, rows, detail = invoke(self.root, "gate", "--reuse", str(author), **off)
        self.assertEqual(sorted(counter.read_text().splitlines()), ["alpha", "beta"],
                         "the gate ran again what the author passed with more mutation passes on\n" + detail)
        for unit in units:
            self.assertEqual(rows[unit]["reused_from"], str(author), rows[unit])
        # Never the other way round: the gate's pass with them off proves nothing with them on.
        _on, rows, detail = invoke(self.root, "passes-on", "--reuse", str(gate), RICHOS_MUTATION_PASSES="1")
        self.assertEqual(sorted(counter.read_text().splitlines()[2:]), ["alpha", "beta"], detail)

    def test_the_gate_reuses_the_authors_whole_checkout_passes_made_in_the_authors_checkout(self):
        """2026-10-21, the land of cc/zach-opus-reuse3 (richos main 06d2ae7af): the readiness
        preflight read the author's run as proving the branch, and the gate, offered that run,
        executed all 23 checks again ("23 planned, 23 passed (0 of them reused)", 1238.9 s serial).
        Every one had no reviewed contract, so its identity named the author's worktree: the
        checkout path, the main checkout's Playwright install found by place, and for engine units
        the checkout's own Cargo shim first on PATH. Real runner, ci-shard.sh and coverage verifier:
        a plain check and two engine units pass in a linked worktree, and the main checkout's gate
        run, offered that run, reuses all three. Not loosened: other content, a seat changed since
        its run, or other dependency installs each still execute."""
        app, engine = self.copy_runner_fixture()
        counter = Path(self.tmp.name) / "executions"
        for name in ("alpha", "beta"):
            path = engine / "scripts" / (name + ".test.sh")
            path.write_text('#!/bin/bash\nprintf "' + name + '\\n" >> "$FIXTURE_COUNTER"\n')
            path.chmod(0o755)
        (self.root / "gamma.test.sh").write_text('printf "gamma\\n" >> "$FIXTURE_COUNTER"\n')
        (self.root / "LICENSE").write_text("fixture license\n")
        (self.root / ".gitignore").write_text("node_modules/\n")
        umask = os.umask(0)
        os.umask(umask)
        for written in self.root.rglob("*"):
            if written.is_file() and not written.is_symlink():
                written.chmod((0o777 if written.stat().st_mode & 0o100 else 0o666) & ~umask)
        def git(*args, cwd=None):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=cwd or self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        worktree = Path(self.tmp.name) / "author-checkout"
        git("worktree", "add", "-q", "-b", "cc/author", str(worktree))
        # The main checkout's ignored Playwright install, which a worktree with none of its own loads.
        def install(checkout, version):
            package = checkout / "richos/app/ui/tests/node_modules/playwright"
            package.mkdir(parents=True, exist_ok=True)
            (package / "index.js").write_text("module.exports = {version: %r};\n" % version)
        install(self.root, "1")
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RICHOS_", "GIT_"))}
        env.update(RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"),
            FIXTURE_COUNTER=str(counter), PYTHONDONTWRITEBYTECODE="1")
        commands = Path(self.tmp.name) / "commands"
        both = ",".join("scripts/%s.test.sh" % name for name in ("alpha", "beta"))
        commands.write_text("cd richos/engine && bash scripts/ci-shard.sh --only-units " + both + "\n"
                            "cd . && bash gamma.test.sh\n")
        def invoke(checkout, name, *options):
            directory = Path(self.tmp.name) / name
            # Each checkout's own directory first on PATH, as the runner's Cargo rule puts it.
            path = str(checkout / "richos/app/scripts/bin") + os.pathsep + env["PATH"]
            result = subprocess.run([*idle_proof_run(checkout / "richos/app/scripts/proof-run.py"),
                "--commands", str(commands), "--log-dir", str(directory), *options], cwd=checkout,
                env=dict(env, PATH=path), text=True, capture_output=True, timeout=120)
            detail = result.stdout + result.stderr + "\n" + "\n".join(p.read_text() for p in directory.glob("*.log"))
            self.assertEqual(result.returncode, 0, detail)
            rows = {row["check"]: row for row in json.loads((directory / "summary.json").read_text())["checks"]}
            self.assertEqual(rows["engine receipts"]["result"], "passed", detail)
            return directory, rows, detail
        checks = ("engine scripts/alpha.test.sh", "engine scripts/beta.test.sh", "gamma")
        executed = lambda: sorted(counter.read_text().splitlines())
        author, rows, detail = invoke(worktree, "author")
        self.assertEqual(executed(), ["alpha", "beta", "gamma"], detail)
        identities = json.loads((author / "plan.json").read_text())["identities"]
        self.assertEqual({identities[check]["contract"] for check in checks}, {evidence.WHOLE_CHECKOUT})
        _gate, rows, detail = invoke(self.root, "gate", "--reuse", str(author))
        self.assertEqual(executed(), ["alpha", "beta", "gamma"],
                         "the gate ran again what the author's run proved on the same content\n" + detail)
        for check in checks:
            self.assertEqual(rows[check]["reused_from"], str(author), detail)
        def executes_all(name, author_run, why, because):
            counter.write_text("")
            # Only the offered run is under test: the input-owner pool would rightly join the
            # gate's own earlier pass on unchanged content here.
            shutil.rmtree(Path(env["RICHOS_PROOF_RUN_DIR"]) / "input-owners", ignore_errors=True)
            _run, rows, detail = invoke(self.root, name, "--reuse", str(author_run))
            self.assertEqual(executed(), ["alpha", "beta", "gamma"], why + "\n" + detail)
            for check in checks:
                self.assertIsNone(rows[check]["reused_from"], why)
            self.assertIn("made in another checkout, where " + because, detail)
        # Other content in the gate's checkout.
        (self.root / "gamma.test.sh").write_text('printf "gamma\\n" >> "$FIXTURE_COUNTER"  # landed edit\n')
        executes_all("other-content", author, "a pass of other content was reused",
                     "the two checkouts' content differs")
        git("checkout", "--", "gamma.test.sh")
        # The author's checkout changed since its run: the pass no longer stands where it was made.
        (worktree / "gamma.test.sh").write_text('printf "gamma\\n" >> "$FIXTURE_COUNTER"  # later edit\n')
        executes_all("seat-changed", author, "a pass that no longer stands in its own checkout was reused",
                     "it no longer stands in its own checkout")
        git("checkout", "--", "gamma.test.sh", cwd=worktree)
        # The author's checkout loads its own, different install.
        install(worktree, "2")
        counter.write_text("")
        own, _rows, detail = invoke(worktree, "author-own-install")
        self.assertEqual(executed(), ["alpha", "beta", "gamma"], detail)
        executes_all("other-installs", own, "a pass made against another dependency install was reused",
                     "the two checkouts load different dependency installs")

    def test_a_pass_stands_at_a_later_commit_only_where_neither_selector_nor_dependency_map_reaches_it(self):
        """2026-10-22, zach-opus-holds3: after a three-file change the fourth handover run reused
        0 of the 94 checks its third run had passed 92 of, because a check with no reviewed read
        list is keyed by the whole checkout (richos-hq docs/research/2026-10-22-mac-system-cpu-
        round2.md). The lead's answers to esc-20261022T054303Z-e2b6f41e and review
        rv-20261022T062403Z-4b9d1c1f-03ee #1: a pass saved at commit A stands at B when the rest
        of its identity is unchanged, the selector maps A..B cleanly without selecting it, and the
        dependency map covers it with no changed path among what it executes; otherwise it runs.
        Real runner, ci-shard.sh and coverage verifier, a stand-in selector that maps each changed
        path through a fixed table and a fixture dependency map: a selected check runs, an
        unselected covered one is reused, a check the map does not cover runs, a helper the
        selector misses but the map lists runs its reader, and an unmapped path, an uncommitted
        change or a changed environment runs all."""
        app, engine = self.copy_runner_fixture()
        counter = Path(self.tmp.name) / "executions"
        for name in ("alpha", "beta"):
            path = engine / "scripts" / (name + ".test.sh")
            path.write_text('#!/bin/bash\n' + ('. "$(dirname "$0")/beta-helper.sh"\n' if name == "beta" else "")
                            + 'printf "' + name + '\\n" >> "$FIXTURE_COUNTER"\n')
            path.chmod(0o755)
        (engine / "scripts/beta-helper.sh").write_text("# sourced by beta.test.sh\n")
        dependency_map = engine / "scripts/lib/verification-dependencies.json"

        def write_map():
            """The fixture map: each suite a node, beta's edge to the helper it sources, each pin the
            bytes on disk now (the commit hook renews a pin in the commit that changes its file)."""
            def node(rel, *edges):
                return {"source": rel, "sha256": evidence.file_digest(engine / rel), "evidence": "fixture",
                        "keys": [], "edges": [{"to": edge} for edge in edges]}
            evidence.atomic(dependency_map, {"schema": 1, "nodes": {
                "scripts/alpha.test.sh": node("scripts/alpha.test.sh"),
                "scripts/beta.test.sh": node("scripts/beta.test.sh", "scripts/beta-helper.sh"),
                "scripts/beta-helper.sh": node("scripts/beta-helper.sh")},
                "units": {"scripts/alpha.test.sh": "scripts/alpha.test.sh",
                          "scripts/beta.test.sh": "scripts/beta.test.sh"}})
        write_map()
        (engine / "scripts/alpha-input.txt").write_text("alpha reads this\n")
        (self.root / "gamma.test.sh").write_text('printf "gamma\\n" >> "$FIXTURE_COUNTER"\n')
        (self.root / "notes.txt").write_text("read by no check\n")
        (self.root / "LICENSE").write_text("fixture license\n")
        shard = "cd %s && bash scripts/ci-shard.sh --only-units " % "richos/engine"
        units = {name: "scripts/%s.test.sh" % name for name in ("alpha", "beta")}
        table = Path(self.tmp.name) / "selector-table.json"
        # The helper beta sources is mapped to nothing: the selector's gap the review found (a
        # changed helper no suite names), which the dependency map closes.
        table.write_text(json.dumps({"richos/engine/scripts/alpha-input.txt": [shard + units["alpha"]],
                                    "notes.txt": [], "richos/engine/scripts/beta-helper.sh": [],
                                    "richos/engine/scripts/lib/verification-dependencies.json": []}))
        # The stand-in selector: proof-for.sh's contract (the lines on stdout, exit 1 for a changed
        # path no suite covers), over `git diff --name-only <range>` exactly as proof-for.sh reads it.
        selector = app / "proof-for.sh"
        selector.write_text(
            "#!/bin/bash\nexec python3 -c '\n"
            "import json, subprocess, sys\n"
            "table = json.load(open(sys.argv[1]))\n"
            "changed = subprocess.check_output([\"git\", \"diff\", \"--name-only\", sys.argv[-1]], text=True).split()\n"
            "if any(path not in table for path in changed):\n"
            "    sys.exit(1)\n"
            "print(\"\\n\".join(sorted({line for path in changed for line in table[path]})))\n"
            "' \"$FIXTURE_SELECTOR_TABLE\" \"$@\"\n")
        selector.chmod(0o755)
        umask = os.umask(0)
        os.umask(umask)
        for written in self.root.rglob("*"):
            if written.is_file() and not written.is_symlink():
                written.chmod((0o777 if written.stat().st_mode & 0o100 else 0o666) & ~umask)

        def git(*args):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "A")
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RICHOS_", "GIT_"))}
        env.update(RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"),
            FIXTURE_COUNTER=str(counter), FIXTURE_SELECTOR_TABLE=str(table), PYTHONDONTWRITEBYTECODE="1")
        commands = Path(self.tmp.name) / "commands"
        commands.write_text(shard + ",".join(units.values()) + "\ncd . && bash gamma.test.sh\n")

        def invoke(name, previous=None, **extra):
            counter.write_text("")
            # Only the offered run is under test: the input-owner pool joins exact passes by itself.
            shutil.rmtree(Path(env["RICHOS_PROOF_RUN_DIR"]) / "input-owners", ignore_errors=True)
            directory = Path(self.tmp.name) / name
            result = subprocess.run([*idle_proof_run(app / "proof-run.py"), "--commands", str(commands),
                "--log-dir", str(directory), *(["--reuse", str(previous)] if previous else [])],
                cwd=self.root, env=dict(env, **extra), text=True, capture_output=True, timeout=120)
            detail = result.stdout + result.stderr + "\n" + "\n".join(p.read_text() for p in directory.glob("*.log"))
            self.assertEqual(result.returncode, 0, detail)
            rows = {row["check"]: row for row in json.loads((directory / "summary.json").read_text())["checks"]}
            self.assertEqual(rows["engine receipts"]["result"], "passed", detail)
            return directory, sorted(counter.read_text().splitlines()), rows, detail

        first, executed, _rows, detail = invoke("A")
        self.assertEqual(executed, ["alpha", "beta", "gamma"], detail)
        # B: alpha's input and a file no check reads. The selector selects alpha alone.
        (engine / "scripts/alpha-input.txt").write_text("alpha reads this, changed\n")
        (self.root / "notes.txt").write_text("read by no check, changed\n")
        git("commit", "-qam", "B")
        second, executed, rows, detail = invoke("B", first)
        self.assertEqual(executed, ["alpha", "gamma"], "alpha (selected) or gamma (no dependency map row) "
                         "did not run, or beta (unselected, covered) ran again\n" + detail)
        self.assertEqual(rows["engine scripts/beta.test.sh"]["reused_from"], str(first), detail)
        self.assertIn("neither the selector nor the dependency map reaches it", detail)
        self.assertIn("the dependency map does not cover it", detail)
        self.assertIsNone(rows["engine scripts/alpha.test.sh"]["reused_from"], detail)
        # A pass that stood stands again at the next commit, from the run it stood in.
        (self.root / "notes.txt").write_text("read by no check, changed twice\n")
        git("commit", "-qam", "C")
        third, executed, rows, detail = invoke("C", second)
        self.assertEqual(executed, ["gamma"], detail)
        # H: the helper beta sources changes, its pin renewed with it. The selector maps it to
        # nothing; the dependency map lists it under beta, so beta runs.
        (engine / "scripts/beta-helper.sh").write_text("# sourced by beta.test.sh, changed\n")
        write_map()
        git("commit", "-qam", "H")
        helper, executed, rows, detail = invoke("H", third)
        self.assertEqual(executed, ["beta", "gamma"], "a pass stood over a change to a file its suite "
                         "executes\n" + detail)
        self.assertIn("changes richos/engine/scripts/beta-helper.sh, which it executes (dependency map)", detail)
        self.assertEqual(rows["engine scripts/alpha.test.sh"]["reused_from"], str(third), detail)
        # D: a changed path the selector cannot map. Everything runs.
        (self.root / "orphan.sh").write_text("#!/bin/sh\n")
        git("add", "orphan.sh")
        git("commit", "-qm", "D")
        _run, executed, _rows, detail = invoke("D", helper)
        self.assertEqual(executed, ["alpha", "beta", "gamma"], "a pass stood over a change the selector "
                         "cannot map\n" + detail)
        self.assertIn("cannot map", detail)
        git("rm", "-q", "orphan.sh")
        git("commit", "-qm", "E")
        fifth, executed, _rows, detail = invoke("E", helper)
        # The same content as H, so every identity is H's: even gamma, which the map does not
        # cover, is reused exactly, as before this rule.
        self.assertEqual(executed, [], "removing the unmapped file returned the tree to H's content\n" + detail)
        # An uncommitted change: no commit range describes it. Everything runs.
        (self.root / "notes.txt").write_text("uncommitted\n")
        _run, executed, _rows, detail = invoke("dirty", fifth)
        self.assertEqual(executed, ["alpha", "beta", "gamma"], detail)
        self.assertIn("this checkout is not a clean commit", detail)
        git("commit", "-qam", "F")
        # Another environment at a clean commit the selector maps: the rest of the identity changed.
        _run, executed, _rows, detail = invoke("environment", fifth, RICHOS_FIXTURE_SETTING="other")
        self.assertEqual(executed, ["alpha", "beta", "gamma"], detail)
        self.assertIn("inputs other than the checkout's content changed", detail)

    def test_a_recipe_pass_stands_only_once_its_identity_binds_ignored_files(self):
        """What git ignores, no selector sees: a reviewed recipe's paths bind ignored files, so a
        recipe identity that does not name them apart (saved before they were bound) never stands,
        and one whose ignored files changed never stands either."""
        source = {"commit": "a" * 40, "tracked_diff_sha256": evidence.EMPTY_SHA256,
                  "untracked_sha256": evidence.EMPTY_SHA256}
        later = dict(source, commit="b" * 40)
        recipe = {"contract": "c1", "paths": {"richos/engine": "p1"}, "tools": {}, "environment": {},
                  "ignored": "i1", "settings": {}}
        old = {"state": "passed", "exit": 0, "source": source, "input": recipe}
        asked = []

        def unaffected(a, b, label):
            asked.append((a, b, label))
            return None
        now = dict(recipe, contract="c2", paths={"richos/engine": "p2"})
        self.assertIsNone(evidence.selection_standing("u", old, now, later, unaffected))
        self.assertEqual(asked, [("a" * 40, "b" * 40, "u")])
        before = dict(old, input={k: v for k, v in recipe.items() if k != "ignored"})
        self.assertEqual(evidence.selection_standing("u", before, now, later, unaffected),
                         "its identity predates the binding of ignored files")
        self.assertIn("inputs other than the checkout's content changed",
                      evidence.selection_standing("u", old, dict(now, ignored="i2"), later, unaffected))
        self.assertEqual(evidence.selection_standing("u", old, now, dict(later, untracked_sha256="x"), unaffected),
                         "this checkout is not a clean commit")
        self.assertEqual(len(asked), 1, "the selector was asked where the identities already refused")
        # Review rv-20261022T062403Z-4b9d1c1f-03ee #2: of its Git inputs, the index and history
        # readings are content the selector judges; a Git configuration observation is not.
        gitted = dict(old, input=dict(recipe, git_inputs={"tracked": "t1", "quote_path": "q1"}))
        self.assertIsNone(evidence.selection_standing(
            "u", gitted, dict(now, git_inputs={"tracked": "t2", "quote_path": "q1"}), later, unaffected))
        self.assertIn("inputs other than the checkout's content changed", evidence.selection_standing(
            "u", gitted, dict(now, git_inputs={"tracked": "t1", "quote_path": "q2"}), later, unaffected))

    def test_actual_cross_commit_coverage_rechecks_inputs_and_keeps_original_sha(self):
        self.cross_commit_coverage()

    def test_actual_subset_reuse_within_changed_root_preserves_coverage_and_receipts(self):
        self.cross_commit_coverage(subset=True)

    def cross_commit_coverage(self, subset=False):
        app, engine = self.copy_runner_fixture()
        (engine / 'unrelated.md').write_text('Unrelated initial text.\n')
        (self.root / "LICENSE").write_text("fixture license\n")
        for name in ("alpha", "beta"):
            path = engine / "scripts" / (name + ".test.sh")
            path.write_text('#!/bin/bash\nprintf "' + name + '\\n" >> "$FIXTURE_COUNTER"\n')
            path.chmod(0o755)
        self.qualification("Fixture units append a label; no external content read.",
            paths=["richos/engine", "richos/app/scripts", "LICENSE"], tools=["bash", "python3", "git"],
            environment=["FIXTURE_COUNTER"])
        recipe = {"paths": ["richos/engine", "richos/app/scripts", "LICENSE"], "tools": ["bash", "python3", "git"],
            "environment": ["FIXTURE_COUNTER"], "external": [], "qualification": "qualification.json",
            "isolation": evidence.PRIVATE_PROFILE}
        checks = {"engine scripts/" + name + ".test.sh": dict(recipe) for name in ("alpha", "beta")}
        if subset:
            floor = {'paths': ['richos/app/scripts', 'richos/engine/scripts/lib',
                'richos/engine/scripts/ci-units.sh', 'richos/engine/scripts/ci-shard.sh', 'LICENSE'],
                'inventories': ['richos/engine']}
            contract_path = self.root / 'qualification.json'
            contract = json.loads(contract_path.read_text())
            contract.update(subset_requires=floor, units={})
            for label, row in checks.items():
                source = 'richos/engine/' + label.removeprefix('engine ')
                row.update(qualification_unit=label, subset={
                    'paths': floor['paths'] + [source], 'inventories': floor['inventories']})
                contract['units'][label] = {'review': 'Counter fixture.',
                    'sources': {source: evidence.file_digest(self.root / source)},
                    'requires': {k: [] for k in ('paths', 'tools', 'environment', 'external')},
                    'subset': {'review': 'Reads only the counter fixture and runner.',
                               'requires': {'paths': [], 'inventories': []}}}
            evidence.atomic(contract_path, contract)
        evidence.atomic(app / "proof-inputs.json", {"schema": 1, "checks": checks})
        def git(*args):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", *args], cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        original_sha = git("rev-parse", "HEAD")
        counter = Path(self.tmp.name) / "executions"
        env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_")}
        env.update(RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            # A private proof-run slot: the host's is shared with real runs, so a fixture
            # waited behind them and wrote holder records into ~/.richos-nightly.
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"),
            FIXTURE_COUNTER=str(counter), PYTHONDONTWRITEBYTECODE="1")
        # Exercise a normal caller without the test wrapper's bytecode setting.
        env.pop("PYTHONDONTWRITEBYTECODE", None)
        commands = Path(self.tmp.name) / "commands"
        def invoke(name, units, *options):
            commands.write_text("cd richos/engine && bash scripts/ci-shard.sh --only-units " + units + "\n")
            directory = Path(self.tmp.name) / name
            result = subprocess.run([*idle_proof_run(app / "proof-run.py"),
                "--commands", str(commands), "--log-dir", str(directory), *options],
                cwd=self.root, env=env, text=True, capture_output=True, timeout=45)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr + "\n" +
                "\n".join(p.read_text() for p in directory.glob("*.log")))
            return directory
        author = invoke("author", "scripts/alpha.test.sh")
        before = next((author / "engine-receipts").glob("*.jsonl")).read_bytes()
        unrelated = 'richos/engine/unrelated.md' if subset else 'unrelated.md'
        (self.root / unrelated).write_text("An unrelated target commit.\n")
        git("add", unrelated)
        git("commit", "-qm", "unrelated documentation")
        target = invoke("target", "scripts/alpha.test.sh,scripts/beta.test.sh", "--reuse", str(author))
        self.assertEqual(counter.read_text().splitlines(), ["alpha", "beta"])
        receipts = [json.loads(p.read_text()) for p in (target / "engine-receipts").glob("*.jsonl")]
        self.assertEqual({r["sha"] for r in receipts}, {original_sha, git("rev-parse", "HEAD")})
        alpha = next(p for p in (target / "engine-receipts").glob("*.jsonl")
                     if json.loads(p.read_text())["unit"] == "scripts/alpha.test.sh")
        self.assertEqual(alpha.read_bytes(), before)
        verifier = engine / "scripts/lib/ci-receipts.py"
        args = [sys.executable, "-B", str(verifier), "verify", "--plan", str(target / "engine-units.txt")]
        def verify(provenance=True):
            return subprocess.run(args + (["--proof-run", str(target)] if provenance else []),
                input="\n".join(json.dumps(r) for r in receipts), cwd=self.root,
                env=env, text=True, capture_output=True, timeout=10)
        self.assertNotEqual(verify(False).returncode, 0, "mixed commits need validated provenance")
        self.assertEqual(verify().returncode, 0)
        (self.root / "LICENSE").write_text("changed outside the engine root\n")
        self.assertNotEqual(verify().returncode, 0)
        git("add", "LICENSE")
        git("commit", "-qm", "changed declared outside input")
        invoke("changed-input", "scripts/alpha.test.sh", "--reuse", str(target))
        self.assertEqual(counter.read_text().splitlines(), ["alpha", "beta", "alpha"])
        self.assertEqual(list(self.root.rglob("__pycache__")), [],
                         "the runner and receipt verifier must not change source inputs")

    def test_a_run_ended_at_its_cap_is_resumed_without_rerunning_its_passes_engine_units_included(self):
        # 2026-10-02, the land of cc/zach-opus-e2fix1: 214 checks, four merge attempts, each run
        # ended at the gate's cap, and every retry re-ran what had passed. 91 engine units had no
        # reviewed contract and were `fresh` (never reused), and every identity carried the
        # run's --admission-wait/--slot-wait, which the retry changed (893 s, then 900 s).
        # Real runner, real ci-shard.sh, real coverage verifier; the units have no contract.
        app, engine = self.copy_runner_fixture()
        counter, hold = Path(self.tmp.name) / "executions", Path(self.tmp.name) / "hold"
        for name in ("alpha", "beta"):
            path = engine / "scripts" / (name + ".test.sh")
            path.write_text('#!/bin/bash\nprintf "' + name + '\\n" >> "$FIXTURE_COUNTER"\n'
                            + ('if [ -e "$FIXTURE_HOLD" ]; then sleep 120; fi\n' if name == "beta" else ""))
            path.chmod(0o755)
        (self.root / "LICENSE").write_text("fixture license\n")
        def git(*args):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RICHOS_", "GIT_"))}
        env.update(RICHOS_MACHINE_WORKERS=str(Path(self.tmp.name) / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(Path(self.tmp.name) / "slot"),
            RICHOS_PROOF_RUN_DIR=str(Path(self.tmp.name) / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(Path(self.tmp.name) / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(Path(self.tmp.name) / "config"),
            FIXTURE_COUNTER=str(counter), FIXTURE_HOLD=str(hold))
        commands = Path(self.tmp.name) / "commands"
        units = ",".join("scripts/%s.test.sh" % name for name in ("alpha", "beta"))
        commands.write_text("cd richos/engine && " + " ".join(["bash", "scripts/ci-shard.sh", "--only-units", units]) + "\n")
        def invoke(name, *options):
            directory = Path(self.tmp.name) / name
            result = subprocess.run([*idle_proof_run(app / "proof-run.py"), *options, "--log-dir", str(directory),
                "--cap", "600"], cwd=self.root, env=env, text=True, capture_output=True, timeout=120)
            detail = result.stdout + result.stderr + "\n" + "\n".join(p.read_text() for p in directory.glob("*.log"))
            rows = {row["check"]: row for row in json.loads((directory / "summary.json").read_text())["checks"]}
            return result.returncode, rows, detail
        alpha, beta = "engine scripts/alpha.test.sh", "engine scripts/beta.test.sh"
        # Attempt 1: alpha passes; beta is still running when the run's cap ends it.
        hold.touch()
        code, rows, detail = invoke("first", "--commands", str(commands), "--run-cap", "15",
                                    "--admission-wait", "893", "--slot-wait", "893")
        self.assertNotEqual(code, 0, detail)
        self.assertEqual((rows[alpha]["result"], rows[beta]["result"]), ("passed", runner.CANCELED), detail)
        self.assertEqual(counter.read_text().splitlines(), ["alpha", "beta"])
        # Attempt 2, same tree, the gate's longer waits: only beta runs; alpha's pass and its
        # receipt are carried over, and the coverage proof accepts the union.
        hold.unlink()
        code, rows, detail = invoke("second", "--resume", str(Path(self.tmp.name) / "first"),
                                    "--admission-wait", "900", "--slot-wait", "900")
        self.assertEqual(code, 0, detail)
        self.assertEqual(counter.read_text().splitlines(), ["alpha", "beta", "beta"], detail)
        self.assertTrue(rows[alpha]["reused_from"], rows[alpha])
        self.assertEqual(rows["engine receipts"]["result"], "passed", detail)
        # A changed tree (one new untracked file anywhere) runs both again.
        (self.root / "unrelated.txt").write_text("a new file\n")
        code, rows, detail = invoke("changed", "--commands", str(commands),
                                    "--reuse", str(Path(self.tmp.name) / "second"))
        self.assertEqual(code, 0, detail)
        self.assertEqual(sorted(counter.read_text().splitlines()[3:]), ["alpha", "beta"], detail)
        self.assertFalse(rows[alpha]["reused_from"], rows[alpha])

    def test_a_check_is_not_keyed_by_how_long_it_may_wait_to_start(self):
        # The gate's first run waits at most what is left of its 900 s and its retry a full
        # 900 s; a check runs the same either way, so neither bound is part of its identity.
        args = SimpleNamespace(capacity=8, engine_shards=5, max_cpu=80, budget=600, deadline=1800,
                               fail_fast=False, admission_wait=893.0, slot_wait=893.0, engine_slot_wait=None)
        item = runner.Item("check", str(self.root), ["bash", "check.sh"])
        with patch.object(runner, "ROOT", str(self.root)), \
                patch.object(evidence, "contract_for", return_value={"fresh": "fixture"}), \
                patch.object(evidence, "checkout_identity", side_effect=lambda *a, **k: {"contract": evidence.WHOLE_CHECKOUT}):
            first = runner.input_identity(item, args, str(self.root))
            args.admission_wait = args.slot_wait = 900.0
            args.engine_slot_wait = 30.0
            self.assertEqual(runner.input_identity(item, args, str(self.root)), first)
            args.capacity = 4
            self.assertNotEqual(runner.input_identity(item, args, str(self.root)), first)

    def test_private_profile_drops_ambient_inputs_and_uses_fixed_fixture_seeds(self):
        self.qualification("Controlled private profile fixture.",
            environment=["NAMED_INPUT"], external=["FIXTURE_INPUT"])
        recipe = {"paths": [], "tools": [], "environment": ["NAMED_INPUT"],
            "external": ["FIXTURE_INPUT"], "qualification": "qualification.json",
            "isolation": evidence.PRIVATE_PROFILE}
        evidence.atomic(self.root / "richos/app/scripts/proof-inputs.json",
                        {"schema": 1, "checks": {"check": recipe}})
        external = Path(self.tmp.name) / "input"
        external.write_text("first")
        ambient = {**os.environ, "BASH_ENV": "/untrusted/startup", "PYTHONPATH": "/untrusted/imports",
            "RICHOS_RUNTIME_DIR": "/untrusted/runtime", "NAMED_INPUT": "declared value",
            "FIXTURE_INPUT": str(external), "SECRET_TOKEN": "must not reach test"}
        identities = []
        for name in ("one", "two"):
            item = runner.Item("check", str(self.root), ["bash", "fixture.sh"])
            item.env["RICHOS_RUNTIME_DIR"] = ambient["RICHOS_RUNTIME_DIR"]
            directory = Path(self.tmp.name) / name
            evidence.prepare_environment(item, self.root, directory, ambient)
            env = runner.execution_environment(item)
            self.assertFalse(set(env) & {"BASH_ENV", "PYTHONPATH", "SECRET_TOKEN", "RICHOS_RUNTIME_DIR"})
            self.assertEqual(env["NAMED_INPUT"], "declared value")
            self.assertEqual(Path(env["GIT_CONFIG_GLOBAL"]).read_text(), evidence.GIT_FIXTURE)
            self.assertEqual((Path(env["CLAUDE_CONFIG_DIR"]) / "state/scratch-ledger.jsonl").read_text(), "")
            libraries = HERE.parents[1] / "engine/scripts/lib"
            check = subprocess.run(["bash", "-eu", "-c", '''
. "$1/record-canary.sh"
. "$1/scratch.sh"
baseline="$TMPDIR/record-before"
rc_baseline "$baseline"
[ "$RC_HEALTHY" -eq 1 ]
allocated="$(scratch_new profile-regression)"
[ -z "$(rc_escaped "$baseline")" ]
scratch_release "$allocated"
[ -z "$(rc_escaped "$baseline")" ]
printf '%s\\n' '{"event":"finished","agent_id":"fixture"}' > "$RC_LEDGER"
[ -n "$(rc_escaped "$baseline")" ]
''', "profile-check", str(libraries)], env=env, capture_output=True, text=True)
            self.assertEqual(check.returncode, 0, check.stdout + check.stderr)
            self.assertTrue(Path(env["TMPDIR"]).is_relative_to(directory))
            result = subprocess.check_output(["python3", "-c",
                "import sys; print(sys.flags.no_site, sys.flags.no_user_site)"], env=env, text=True)
            self.assertEqual(result.strip(), "1 1")
            program = ("import sys; sys.path.insert(0, " + repr(str(HERE.parents[1] / "engine/scripts/lib"))
                       + "); import proc_tree; print(' '.join(proc_tree.command(['true'])))")
            command = subprocess.check_output(["python3", "-c", program], env=env, text=True)
            self.assertIn(" -S -s ", command, "nested supervision must preserve disabled site startup")
            identities.append(evidence.recipe_identity(self.root, recipe, env))
            # The coverage verifier reconstructs the profile without creating or
            # replacing fixture state from a completed execution.
            marker = Path(env["HOME"]) / "retained"
            marker.write_text("execution output")
            copy = runner.Item("check", str(self.root), ["bash", "fixture.sh"])
            evidence.prepare_environment(copy, self.root, directory, ambient, create=False)
            self.assertEqual(runner.execution_environment(copy), env)
            self.assertEqual(marker.read_text(), "execution output")
        self.assertEqual(*identities)
        external.write_text("changed")
        self.assertNotEqual(identities[0], evidence.recipe_identity(self.root, recipe, env))
        external.write_text("first")
        self.assertNotEqual(identities[0], evidence.recipe_identity(self.root, recipe, {**env, "NAMED_INPUT": "changed"}))

    def test_python_runtime_binds_import_roots_and_ignores_disabled_site_packages(self):
        library = Path(self.tmp.name) / "stdlib"
        library.mkdir()
        (library / "module.py").write_text("value = 1")
        (library / "site-packages").mkdir()
        ignored = library / "site-packages/disabled.pth"
        ignored.write_text("disabled startup")
        output = json.dumps({"version": "fixture", "paths": [str(library)], "prefix": str(library)})
        with patch.object(evidence.subprocess, "run", return_value=SimpleNamespace(stdout=output)):
            before = evidence.python_runtime("python3")
            ignored.write_text("also disabled")
            self.assertEqual(before, evidence.python_runtime("python3"))
            (library / "module.py").write_text("value = 2")
            self.assertNotEqual(before, evidence.python_runtime("python3"))

    def test_input_owner_joins_then_reuses_finished_evidence_while_original_run_stays_open(self):
        old, previous = self.attempt("author")
        new, current = self.attempt("target")
        owner = evidence.Pool(Path(self.tmp.name) / "pool", previous)
        joiner = evidence.Pool(Path(self.tmp.name) / "pool", current)
        self.addCleanup(owner.close, old)
        self.addCleanup(joiner.close, new)
        self.assertTrue(owner.claim(old[0]))
        self.assertFalse(joiner.claim(new[0]))
        self.assertEqual(new[0].state, "waiting")
        self.assertFalse(hasattr(new[0], "input_owner_fd"))
        self.passed(old, previous)
        owner.finish(old[0])
        self.assertFalse(joiner.claim(new[0]))
        self.assertEqual(new[0].state, "passed")
        self.assertEqual(evidence.receipt_path(new[0]).read_bytes(), evidence.receipt_path(old[0]).read_bytes())

    def test_input_owner_lock_survives_parent_close_until_inherited_holder_exits(self):
        items, record = self.attempt("owner", engine=False)
        pool = evidence.Pool(Path(self.tmp.name) / "pool", record)
        self.assertTrue(pool.claim(items[0]))
        child = subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read()"],
            stdin=subprocess.PIPE, pass_fds=(items[0].input_owner_fd,))
        try:
            pool.close(items)
            other, current = self.attempt("joiner", engine=False)
            joiner = evidence.Pool(Path(self.tmp.name) / "pool", current)
            self.assertFalse(joiner.claim(other[0]))
            child.communicate(timeout=5)
            self.assertTrue(joiner.claim(other[0]))
            joiner.close(other)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)

    def test_pool_revalidates_artifacts_and_preserves_corrupt_retry_history(self):
        old, previous = self.attempt("author", engine=False)
        pool = evidence.Pool(Path(self.tmp.name) / "pool", previous)
        self.assertTrue(pool.claim(old[0]))
        self.passed(old, previous)
        pool.finish(old[0])
        Path(old[0].log).write_text("tampered result")
        new, current = self.attempt("target", engine=False)
        next_pool = evidence.Pool(Path(self.tmp.name) / "pool", current)
        self.assertTrue(next_pool.claim(new[0]))
        self.assertEqual(new[0].state, "waiting")
        next_pool.close(new)
        path = next((Path(self.tmp.name) / "pool").glob("*.json"))
        row = json.loads(path.read_text())
        row["functional_failures"] = "corrupted"
        evidence.atomic(path, row)
        with self.assertRaisesRegex(ValueError, "invalid durable retry history"):
            next_pool.claim(new[0])
        self.assertEqual(json.loads(path.read_text())["functional_failures"], "corrupted")

    def test_functional_retry_budget_survives_new_attempt_names(self):
        directory = Path(self.tmp.name) / "pool"
        for name, reason, expected in (("first", None, True), ("without-diagnosis", None, False),
                ("diagnosed-retry", "identified transient fixture fault", True),
                ("renamed-third", "same diagnosis", False)):
            items, record = self.attempt(name, engine=False)
            pool = evidence.Pool(directory, record, reason)
            self.assertEqual(pool.claim(items[0]), expected)
            if expected:
                items[0].state, items[0].rc = "failed", 1
                record.checkpoint(items)
                pool.finish(items[0])
            else:
                self.assertEqual(items[0].state, "blocked")
        items, record = self.attempt("changed-input", engine=False, identities={"check": {"changed": True}})
        pool = evidence.Pool(directory, record)
        self.assertTrue(pool.claim(items[0]))
        pool.close(items)

    def test_a_check_without_a_reviewed_contract_has_a_budget_that_starts_one_failure_later(self):
        # Hunt part 2, finding 12: a whole-checkout identity was exempt from the retry budget, so
        # an unchanged known failure repeated without limit. The reason for the exemption stands
        # for the first retry (a land retried once has no --retry-reason to give), so that one
        # still runs; the second failure needs the diagnosis and the third ends it.
        directory = Path(self.tmp.name) / "pool"
        whole = {"check": {"contract": evidence.WHOLE_CHECKOUT, "tree": "same"}}
        for name, reason, expected in (("first", None, True), ("retry-without-reason", None, True),
                ("third-without-diagnosis", None, False),
                ("diagnosed", "identified transient fixture fault", True),
                ("fifth", "same diagnosis", False)):
            items, record = self.attempt(name, engine=False, identities=whole)
            pool = evidence.Pool(directory, record, reason)
            self.assertEqual(pool.claim(items[0]), expected, name)
            if expected:
                items[0].state, items[0].rc = "failed", 1
                record.checkpoint(items)
                pool.finish(items[0])
            else:
                self.assertEqual(items[0].state, "blocked", name)
        items, record = self.attempt("changed-tree", engine=False,
                                     identities={"check": {"contract": evidence.WHOLE_CHECKOUT, "tree": "new"}})
        pool = evidence.Pool(directory, record)
        self.assertTrue(pool.claim(items[0]))
        pool.close(items)

    def test_refusal_and_timeout_do_not_spend_functional_retry_budget(self):
        for name, code in (("refused", 75), ("deadline", 124), ("infrastructure", 125)):
            items, record = self.attempt(name, engine=False)
            pool = evidence.Pool(Path(self.tmp.name) / "pool", record)
            self.assertTrue(pool.claim(items[0]))
            items[0].state, items[0].rc = "failed", code
            record.checkpoint(items)
            pool.finish(items[0])
        saved = next((Path(self.tmp.name) / "pool").glob("*.json"))
        self.assertEqual(json.loads(saved.read_text())["functional_failures"], [])

    def test_actual_worktrees_join_one_execution_and_run_independent_work(self):
        app, _engine = self.copy_runner_fixture()
        temporary = Path(self.tmp.name)
        (self.root / "joined.test.sh").write_text(
            'printf "joined\\n" >> "$COUNTER"\n'
            'for i in $(seq 1 300); do [ ! -e "$RELEASE" ] || exit 0; sleep .1; done\nexit 1\n')
        (self.root / "independent.test.sh").write_text('printf "independent\\n" >> "$COUNTER"\n')
        self.qualification("Private shell fixtures with a bounded coordination barrier.",
            paths=["richos", "joined.test.sh", "independent.test.sh"],
            tools=["bash", "python3", "seq", "sleep"], environment=["COUNTER", "RELEASE"])
        recipe = {"paths": ["richos", "joined.test.sh", "independent.test.sh"],
            "tools": ["bash", "python3", "seq", "sleep"], "environment": ["COUNTER", "RELEASE"],
            "external": [], "qualification": "qualification.json", "isolation": evidence.PRIVATE_PROFILE}
        evidence.atomic(app / "proof-inputs.json", {"schema": 1,
            "checks": {label: recipe for label in ("joined", "independent")}})
        # Give every fixture file the mode a Git checkout writes, so the second worktree's
        # identical bytes are identical inputs. A checkout follows the umask (0644 under 022,
        # 0600 under the nightly's 077), while atomic() writes 0600 and copy2 keeps the
        # source checkout's modes.
        umask = os.umask(0)
        os.umask(umask)
        for written in self.root.rglob("*"):
            if written.is_file() and not written.is_symlink():
                executable = written.stat().st_mode & 0o100
                written.chmod((0o777 if executable else 0o666) & ~umask)
        def git(*args, cwd=None):
            return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", *args], cwd=cwd or self.root,
                text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "fixture")
        checkout = temporary / "author-checkout"
        git("worktree", "add", "-q", "-b", "author", str(checkout))
        def differences(left, right, name=""):
            if isinstance(left, dict) and isinstance(right, dict):
                return [change for key in sorted(set(left) | set(right))
                        for change in differences(left.get(key), right.get(key), name + "/" + key)]
            return [] if left == right else [(name, left, right)]
        self.assertEqual(differences(evidence.path_identity(self.root / "richos"),
                                    evidence.path_identity(checkout / "richos")), [])
        # A different target commit must not prevent joining identical declared inputs.
        (self.root / "unrelated.md").write_text("Unrelated target change.\n")
        git("add", "unrelated.md")
        git("commit", "-qm", "unrelated target change")
        env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_")}
        env.update(RICHOS_MACHINE_WORKERS=str(temporary / "machine"),
            RICHOS_ENGINE_PASS_DIR=str(temporary / "slot"), RICHOS_PROOF_RUN_DIR=str(temporary / "history"),
            RICHOS_PROOF_RUN_SLOTS_DIR=str(temporary / "proof-slots"),
            CLAUDE_CONFIG_DIR=str(temporary / "config"), COUNTER=str(temporary / "counter"),
            RELEASE=str(temporary / "release"), PYTHONDONTWRITEBYTECODE="1")
        # Two runs must be inside at once for the target to join the author. The measured
        # default on this Mac is one run at a time, so the private slot says two.
        (temporary / "proof-slots").mkdir(mode=0o700)
        (temporary / "proof-slots/limit").write_text("2\n")
        processes = []
        def start(root, name, labels):
            commands = temporary / (name + ".commands")
            commands.write_text("".join("cd . && bash " + label + ".test.sh\n" for label in labels))
            log = (temporary / (name + ".log")).open("w")
            self.addCleanup(log.close)
            process = subprocess.Popen([*idle_proof_run(root / "richos/app/scripts/proof-run.py"),
                "--commands", str(commands), "--log-dir", str(temporary / name)],
                cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
            processes.append(process)
            return process
        def wait_until(predicate):
            limit = time.monotonic() + 20
            while not predicate():
                if time.monotonic() > limit:
                    self.fail("coordination deadline\n" + "\n".join(p.read_text() for p in temporary.glob("*.log")))
                time.sleep(.1)
        try:
            author = start(checkout, "author-run", ["joined"])
            counter = temporary / "counter"
            wait_until(counter.exists)
            target = start(self.root, "target-run", ["joined", "independent"])
            wait_until(lambda: (temporary / "target-run/plan.json").exists())
            first = json.loads((temporary / "author-run/plan.json").read_text())["identities"]["joined"]
            second = json.loads((temporary / "target-run/plan.json").read_text())["identities"]["joined"]
            difference = {key: [first.get(key), second.get(key)] for key in first if first.get(key) != second.get(key)}
            self.assertEqual(first, second, "different fixture identities: " + json.dumps(difference, indent=2))
            wait_until(lambda: "join identical input owner" in (temporary / "target-run.log").read_text())
            wait_until(lambda: "independent" in counter.read_text())
            (temporary / "release").touch()
            for process in (author, target):
                self.assertEqual(process.wait(timeout=30), 0,
                    "\n".join(p.read_text() for p in temporary.glob("*.log")))
            self.assertEqual(counter.read_text().splitlines(), ["joined", "independent"])
            target_rows = json.loads((temporary / "target-run/outcomes.json").read_text())
            self.assertEqual(target_rows["joined"]["reused_from"], str(temporary / "author-run"))
            # Automatic discovery also reuses completed evidence without --reuse.
            later = start(self.root, "later-run", ["joined", "independent"])
            self.assertEqual(later.wait(timeout=30), 0)
            self.assertEqual(counter.read_text().splitlines(), ["joined", "independent"])
        finally:
            (temporary / "release").touch()
            for process in processes:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=20)

    def test_a_handover_run_is_held_until_its_land_releases_it(self):
        """PRD A11: rotate() keeps three successful runs; three narrow green runs after a handover
        removed the run its land was to validate. The explicit-range run is held with rotate()'s
        own `retain` marker, and only a handover hold is released by --release."""
        parent = Path(self.tmp.name) / "history"
        def run(name):
            directory = parent / name
            directory.mkdir(parents=True)
            evidence.atomic(directory / "summary.json", {"checks": [{"result": "passed"}]})
            return directory
        handover = run("20261020T000000Z-handover")
        selected = {"arguments": ["b..c"], "mode": "normal", "base": "b" * 40, "candidate": "c" * 40}
        with patch.object(runner.engine_pass, "is_main_checkout", return_value=True):
            runner.hold_for_handover(str(handover), selected)
        self.assertFalse((handover / "retain").exists(), "the main checkout is no handover seat")
        with patch.object(runner.engine_pass, "is_main_checkout", return_value=False), \
                contextlib.redirect_stdout(io.StringIO()):
            runner.hold_for_handover(str(handover), {"arguments": ["--working"], "mode": "normal"})
            self.assertFalse((handover / "retain").exists(), "a run with no recorded range stands for no handover")
            runner.hold_for_handover(str(handover), selected)
        hold = json.loads((handover / "retain").read_text())
        self.assertEqual((hold["holder"], hold["candidate"]), ("handover", "c" * 40))
        narrow = [run("20261020T00000%dZ-narrow" % n) for n in range(1, 5)]
        runner.rotate(str(parent))
        self.assertTrue(handover.exists(), "later narrow green runs cannot rotate the handover away")
        self.assertFalse(narrow[0].exists(), "rotation still applies to everything else")
        # Someone else's hold is not this command's to remove.
        foreign = run("20261020T000005Z-foreign")
        (foreign / "retain").write_text("kept by an operator\n")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(runner.release_hold(str(foreign)), 2)
            self.assertTrue((foreign / "retain").exists())
            self.assertEqual(runner.release_hold(str(handover)), 0)
        self.assertFalse(handover.exists(), "released, it rotates like any other green run")

    def test_coverage_says_what_a_saved_run_proves_for_the_inputs_read_now(self):
        """PRD R3/A8, the readiness preflight's predicate: what reuse() would accept, read only."""
        old, previous = self.attempt("covered", engine=False)
        self.passed(old, previous)
        found = evidence.coverage(previous.logdir, old, {"check": dict(self.inputs)}, dict(self.source))
        self.assertEqual(found, {"check": None})
        # A final edit to its inputs, a check the run never passed, a damaged log: each is missing.
        self.assertTrue(evidence.coverage(previous.logdir, old, {"check": {"paths": {"fixture": "edited"}}},
                                          dict(self.source))["check"].startswith("declared execution inputs changed ("))
        Path(old[0].log).write_text("rewritten\n")
        self.assertEqual(evidence.coverage(previous.logdir, old, {"check": dict(self.inputs)},
                                           dict(self.source))["check"], "result log changed")
        failed, record = self.attempt("failed", engine=False)
        failed[0].state, failed[0].rc = "failed", 1
        record.checkpoint(failed)
        self.assertEqual(evidence.coverage(record.logdir, failed, {"check": dict(self.inputs)}, dict(self.source)),
                         {"check": "no validated passing execution"})
        # A check with no input identity is never proven by a saved run, even one passed on this
        # exact source: reuse() never accepts it (review rv-20261020T083937Z-2f73e49a-734e, where
        # verification-pins' external reader changed under an unchanged checkout).
        fresh, record = self.attempt("fresh", engine=False, identities={"check": {"fresh": "no contract"}})
        self.passed(fresh, record)
        gap = evidence.coverage(record.logdir, fresh, {"check": {"fresh": "no contract"}}, dict(self.source))["check"]
        self.assertTrue(gap and gap.startswith(evidence.FRESH_GAP), "coverage called it proven: %r" % gap)
        again, current = self.attempt("fresh-again", engine=False, identities={"check": {"fresh": "no contract"}})
        evidence.reuse(record.logdir, again, current)
        self.assertEqual(again[0].state, "waiting", "coverage and reuse agree")
        # Nothing was written: coverage reads only.
        self.assertFalse((previous.logdir / "reused").exists())

    def test_the_preflight_keeps_a_failed_fresh_check_unresolved(self):
        """Review rv-20261020T163601Z-4b96c956-ed75 #1, the reviewer's fixture through the real
        preflight: a `fresh` check went into the fresh bucket before its saved outcome was read, so
        one the author's run failed or timed out left the preflight at exit 0, nothing missing and
        no recovery job. A passed one is still only "fresh at every seat"; none is ever proven."""
        source = {"commit": "a" * 40, "tracked_diff_sha256": "clean", "untracked_sha256": "clean"}
        identity = {"fresh": "external pin validator must run every time"}
        label = "engine scripts/verification-pins.test.sh"
        for state, rc, unresolved in [("passed", 0, False), ("failed", 1, True), ("timed-out", 124, True)]:
            saved = Path(self.tmp.name) / ("fresh-" + state)
            saved.mkdir()
            item = runner.Item(label, str(self.root), ["bash", "pins"])
            record = evidence.Record(str(self.root), saved, [item], source, {label: identity},
                                     selection={"base": "b" * 40, "candidate": source["commit"], "mode": "normal"})
            self.addCleanup(record.close)
            item.state, item.rc, item.log = state, rc, str(saved / "execution.log")
            Path(item.log).write_text("fixture execution: " + state + "\n")
            record.checkpoint([item])
            args = SimpleNamespace(coverage=str(saved), summary_out=str(saved / "coverage.json"))
            with patch.object(runner, "ROOT", str(self.root)), \
                    patch.object(runner, "head_commit", return_value=source["commit"]), \
                    patch.object(runner, "source_identity", return_value=source), \
                    patch.object(runner, "selection", return_value=[]), \
                    patch.object(runner, "plan", return_value=[item]), \
                    patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                    patch.object(runner, "history_weights", return_value={}), \
                    patch.object(runner, "supply_runtime"), \
                    patch.object(runner, "supply_speech_models"), \
                    patch.object(evidence, "prepare_environment"), \
                    patch.object(runner, "execution_environment", return_value={}), \
                    patch.object(runner, "input_identity", return_value=identity), \
                    contextlib.redirect_stdout(io.StringIO()) as output:
                status = runner.author_coverage(args)
            summary = json.loads(Path(args.summary_out).read_text())
            self.assertEqual(summary["proven"], [], state)
            if unresolved:
                self.assertEqual(status, 1, output.getvalue())
                self.assertEqual(list(summary["missing"]), [label], state)
                self.assertTrue(summary["missing"][label].startswith(state + ":"), summary["missing"])
                self.assertEqual(summary["fresh"], {}, state)
                self.assertIn("--reuse", summary["recovery"] or "")
                self.assertTrue((summary["recovery"] or "").endswith(Path(saved).name), summary["recovery"])
            else:
                self.assertEqual(status, 0, output.getvalue())
                self.assertEqual(summary["missing"], {}, state)
                self.assertTrue(summary["fresh"][label].startswith(evidence.FRESH_GAP), summary["fresh"])
                self.assertIsNone(summary["recovery"])

    def test_the_preflight_reads_the_saved_plan_when_the_checkout_reads_as_the_run_started(self):
        """PRD line 139, the no-execution budget (03-E): selecting again cost about 45 s of the
        preflight's 49, for a plan that is a function of the range, the mode and the checkout's
        bytes. With the bytes as the run started, the saved plan is that selection and nothing is
        selected; with any change it selects again, so a suite a final edit adds is still missing.
        The inputs are read now either way."""
        source = dict(self.source)
        saved = Path(self.tmp.name) / "saved-plan"
        saved.mkdir()
        lines = ["cd . && bash fixture.sh"]
        args = SimpleNamespace(coverage=str(saved), summary_out=str(saved / "coverage.json"), capacity=2,
                               engine_shards=1, fail_fast=False, as_printed=False)
        with patch.object(runner, "ROOT", str(self.root)):
            items = runner.plan(lines, args, str(saved), {})
        label = items[0].label
        record = evidence.Record(str(self.root), saved, items, source, {label: dict(self.inputs)},
                                 selection={"base": "b" * 40, "candidate": source["commit"], "mode": "normal"})
        self.addCleanup(record.close)
        items[0].state, items[0].rc, items[0].log = "passed", 0, str(saved / "execution.log")
        Path(items[0].log).write_text("passed\n")
        record.checkpoint(items)
        selected = []

        def select(_args):
            selected.append(True)
            return lines + ["cd . && bash added-by-a-final-edit.sh"]

        def preflight(now):
            with patch.object(runner, "ROOT", str(self.root)), \
                    patch.object(runner, "head_commit", return_value=source["commit"]), \
                    patch.object(runner, "source_identity", return_value=now), \
                    patch.object(runner, "selection", side_effect=select), \
                    patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                    patch.object(runner, "history_weights", return_value={}), \
                    patch.object(runner, "supply_runtime"), \
                    patch.object(runner, "supply_speech_models"), \
                    patch.object(evidence, "prepare_environment"), \
                    patch.object(runner, "execution_environment", return_value={}), \
                    patch.object(runner, "input_identity", side_effect=lambda item, *_a, **_k: dict(self.inputs)), \
                    contextlib.redirect_stdout(io.StringIO()) as output:
                rc = runner.author_coverage(args)
            return rc, json.loads(Path(args.summary_out).read_text()), output.getvalue()

        rc, summary, out = preflight(dict(source))
        self.assertEqual((rc, selected), (0, []), out)
        self.assertEqual(summary["proven"], [label])
        self.assertIn("the run's saved plan", out)
        rc, summary, out = preflight(dict(source, untracked_sha256="a final edit"))
        self.assertEqual((rc, selected), (1, [True]), out)
        self.assertEqual(summary["proven"], [label])
        self.assertEqual(list(summary["missing"].values()), ["selected now, never in the run's plan"])
        self.assertIn("selected again", out)

    def review_range_probe(self, fail_fast):
        """Review rv-20261020T174711Z-042986a6-bafe's probe: three real commits (main, an alpha fix,
        then a beta fix), a saved run of the author's checks with real passing executions, and the
        real author_coverage. Returns (exit, summary, output, root, main, narrow base, tip, run)."""
        base = Path(self.tmp.name) / ("range-probe-%s" % fail_fast)
        root = base / "repo"
        scripts = root / "richos/app/scripts"
        scripts.mkdir(parents=True)
        clean_env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

        def git(*args):
            return subprocess.check_output([
                "git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=Fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", *args],
                cwd=root, env=clean_env, stderr=subprocess.PIPE, text=True).strip()

        (root / "alpha.sh").write_text("exit 0\n")
        (root / "beta.sh").write_text("exit 0\n")
        (root / "review.md").write_text("Each fixture script reads only itself.\n")
        (root / "qualification.json").write_text(json.dumps({"schema": 1, "review": "review.md", "requires": {
            "paths": [], "tools": [], "environment": [], "external": []}}))
        recipes = {label: {"paths": [label + ".sh"], "tools": ["bash"], "environment": [], "external": [],
                           "qualification": "qualification.json"} for label in ("alpha", "beta")}
        (scripts / "proof-inputs.json").write_text(json.dumps({"schema": 1, "checks": recipes}))
        git("init", "-q", "-b", "main")
        git("add", ".")
        git("commit", "-qm", "base")
        main = git("rev-parse", "HEAD")
        git("checkout", "-qb", "author")
        (root / "alpha.sh").write_text("# alpha fix\nexit 0\n")
        git("add", ".")
        git("commit", "-qm", "alpha fix")
        narrow = git("rev-parse", "HEAD")
        (root / "beta.sh").write_text("# beta fix\nexit 0\n")
        git("add", ".")
        git("commit", "-qm", "beta fix")
        tip = git("rev-parse", "HEAD")
        run = base / "saved"
        run.mkdir()
        with patch.object(runner, "author_coverage", side_effect=lambda args: args):
            args = runner.run_main(["--coverage", str(run), "--summary-out", str(base / "coverage.json")], {})
        planned = SimpleNamespace(**vars(args))
        planned.fail_fast, planned.commands = fail_fast, None
        output = io.StringIO()
        with patch.object(runner, "ROOT", str(root)), \
                patch.object(runner, "supply_runtime"), patch.object(runner, "supply_speech_models"), \
                patch.object(runner, "execution_environment", return_value=clean_env):
            # With --fail-fast: the whole branch, beta's run failed. Without: only beta, the last commit.
            lines = ["cd . && bash alpha.sh", "cd . && bash beta.sh"] if fail_fast else ["cd . && bash beta.sh"]
            items = runner.plan(lines, planned, str(run), {})
            identities = {i.label: runner.input_identity(i, planned, str(run)) for i in items}
            record = evidence.Record(root, run, items, runner.source_identity(), identities,
                selection={"base": main if fail_fast else narrow, "candidate": tip, "mode": "normal"})
            try:
                for item in items:
                    done = subprocess.run(item.argv, cwd=root, env=clean_env, capture_output=True, text=True)
                    failed = fail_fast and item.label == "beta"
                    item.state, item.rc = ("failed", 1) if failed else ("passed", done.returncode)
                    item.log = str(run / (item.label + ".log"))
                    Path(item.log).write_text(done.stdout + done.stderr)
                record.checkpoint(items)
            finally:
                record.close()
            with contextlib.redirect_stdout(output):
                rc = runner.author_coverage(args)
        return rc, json.loads(Path(args.summary_out).read_text()), output.getvalue(), root, main, narrow, tip, run

    def test_the_preflight_reads_a_run_under_the_settings_it_ran_with(self):
        """Review rv-20261020T174711Z-042986a6-bafe #2, its fixture: a pass made with --fail-fast was
        "declared execution inputs changed" under this process's default settings, and the recovery
        job dropped --fail-fast, so it could reuse nothing. The run's own settings are read back."""
        rc, summary, out, *_rest = self.review_range_probe(fail_fast=True)
        self.assertEqual(summary["proven"], ["alpha"], out)
        self.assertEqual(list(summary["missing"]), ["beta"], out)
        self.assertTrue(summary["missing"]["beta"].startswith("failed:"), summary["missing"])
        self.assertEqual(rc, 1)
        self.assertIn(" --fail-fast ", summary["recovery"])

    def test_the_land_refuses_a_run_of_only_the_branchs_last_commit(self):
        """Review rv-20261020T174711Z-042986a6-bafe #1, its fixture: the land checked the run's
        candidate and never its range, so a run of the last of two commits was accepted (preflight
        exit 0, the selector never asked). The range must start on the branch landed onto."""
        autocheck_spec = importlib.util.spec_from_file_location("autocheck_range", HERE / "autocheck/autocheck.py")
        autocheck = importlib.util.module_from_spec(autocheck_spec)
        autocheck_spec.loader.exec_module(autocheck)
        rc, summary, out, root, main, narrow, tip, run = self.review_range_probe(fail_fast=False)
        self.assertEqual((rc, summary["proven"], summary["missing"]), (0, ["beta"], {}), out)
        subprocess.run(["git", "checkout", "-q", "main"], cwd=root, check=True,
                       env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
        repo = SimpleNamespace(top=root, env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
        selected, said = [], []
        with patch.object(autocheck, "coverage_of", return_value=(rc, summary, out.splitlines(), str(root))), \
                patch.object(autocheck, "select", side_effect=lambda *_a, **_k: selected.append(True)), \
                patch.object(autocheck, "say", side_effect=said.append):
            refused = autocheck.author_preflight(repo, "land", [tip], [str(run)])
        text = "\n".join(said)
        self.assertEqual(refused, 1, text)
        self.assertIn("covers only part of what the branch brings", text)
        self.assertIn("%s..%s --reuse %s" % (main, tip, run), text)
        self.assertEqual(selected, [])

    def test_the_runner_environment_is_the_same_whether_or_not_reserve_launched_it(self):
        """03-E, found by the recovery demonstration: a job run through reserve.py (the documented
        launch) saved identities with reserve.py's Cargo environment (RUSTC_WRAPPER, SCCACHE_DIR, the
        cargo shim on PATH), and the land's preflight, launched without it, read every pass as
        "declared execution inputs changed". The runner applies the same rule itself, idempotently."""
        import cargo_identity
        seen = []
        base = {k: v for k, v in os.environ.items()
                if k not in ("RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "SCCACHE_DIR")}
        shim = str(HERE / "bin")
        base["PATH"] = os.pathsep.join(p for p in base.get("PATH", "").split(os.pathsep) if p != shim)
        for start in (base, cargo_identity.enable_environment(base)):     # direct; through reserve.py
            with patch.dict(os.environ, start, clear=True), \
                    patch.object(runner, "author_coverage", side_effect=lambda args: seen.append(dict(os.environ))):
                runner.run_main(["--coverage", str(self.root)], {})
        self.assertEqual(seen[0], seen[1])
        self.assertEqual(seen[0]["PATH"].split(os.pathsep).count(shim), 1)

    def test_a_handover_run_turns_mutation_passes_off_and_a_nightly_keeps_them_on(self):
        """2026-10-22, Hank's after-measurement: an engineer's handover run left both mutation
        switches unset, so every suite's own mutation harness ran (22.3% of the Mac in the busy
        window). Unset, the runner gives every check the merge gate's switch; a value the caller
        set (the nightlies' "1", the gate's own "0") is kept."""
        base = {k: v for k, v in os.environ.items() if k not in evidence.MUTATION_SWITCHES}
        self.assertEqual({k: runner.runner_environment(base).get(k) for k in evidence.MUTATION_SWITCHES},
                         dict.fromkeys(evidence.MUTATION_SWITCHES, "0"))
        on = dict(base, RICHOS_MUTATION_PASSES="1", RICHOS_FOURTEEN_MUTANTS="1")
        self.assertEqual({k: runner.runner_environment(on)[k] for k in evidence.MUTATION_SWITCHES},
                         dict.fromkeys(evidence.MUTATION_SWITCHES, "1"))
        item = runner.Item("check", str(self.root), ["bash", "suite.sh"])
        with patch.dict(os.environ, runner.runner_environment(base), clear=True):
            self.assertEqual(runner.execution_environment(item)["RICHOS_MUTATION_PASSES"], "0")

    def test_the_preflight_reads_run_files_under_a_resolved_scratch(self):
        """03-E, found by the recovery demonstration: a run-tests.sh check's command names its result
        file as $RUN/... only when the path starts with the resolved run directory, and the
        preflight's scratch was not resolved, so under a TMPDIR behind a symlink (macOS's /var ->
        /private/var) every such check read as "declared execution inputs changed"."""
        real = Path(self.tmp.name) / "real-tmp"
        real.mkdir()
        linked = Path(self.tmp.name) / "linked-tmp"
        linked.symlink_to(real)
        saved = Path(self.tmp.name) / "scratch-run"
        saved.mkdir()
        lines = ["cd . && scripts/run-tests.sh --no-host-screen --only fixture.test.sh"]
        args = SimpleNamespace(coverage=str(saved), summary_out=str(saved / "coverage.json"), capacity=2,
                               engine_shards=1, fail_fast=False, as_printed=False)
        with patch.object(runner, "ROOT", str(self.root)):
            items = runner.plan(lines, args, str(saved), {})
        self.assertTrue(any("results-out" in a for a in items[0].argv), items[0].argv)

        def identity(item, _args, logdir, *_rest):
            return {"paths": {"fixture": "digest"}, "command": evidence.command_identity(item, str(self.root), logdir)}

        with patch.object(runner, "ROOT", str(self.root)):
            record = evidence.Record(str(self.root), saved, items, dict(self.source),
                                     {items[0].label: identity(items[0], args, str(saved))},
                                     selection={"base": "b" * 40, "candidate": self.source["commit"], "mode": "normal"})
        self.addCleanup(record.close)
        items[0].state, items[0].rc, items[0].log = "passed", 0, str(saved / "execution.log")
        Path(items[0].log).write_text("passed\n")
        record.checkpoint(items)
        with patch.object(runner, "ROOT", str(self.root)), \
                patch.object(tempfile, "tempdir", str(linked)), \
                patch.object(runner, "head_commit", return_value=self.source["commit"]), \
                patch.object(runner, "source_identity", return_value=dict(self.source)), \
                patch.object(runner, "supply_runtime"), patch.object(runner, "supply_speech_models"), \
                patch.object(evidence, "prepare_environment"), \
                patch.object(runner, "execution_environment", return_value={}), \
                patch.object(runner, "input_identity", side_effect=identity), \
                contextlib.redirect_stdout(io.StringIO()) as output:
            rc = runner.author_coverage(args)
        summary = json.loads(Path(args.summary_out).read_text())
        self.assertEqual((rc, summary["proven"]), (0, [items[0].label]), output.getvalue())

    def test_the_summary_carries_its_own_coverage_line_and_never_counts_known_red_as_passed(self):
        """PRD A11/A14: the line is counted from the summary's own rows, a KNOWN-RED unit is named
        beside the passes (an established exception, never a pass) and NOT RUN is never a pass."""
        rows = [{"result": "passed", "reused_from": None, "known_red": False},
                {"result": "passed", "reused_from": "/r", "known_red": False},
                {"result": "passed", "reused_from": None, "known_red": True},
                {"result": "not-run", "reused_from": None, "known_red": False},
                {"result": "timed-out", "reused_from": None, "known_red": False}]
        self.assertEqual(runner.coverage_line(rows),
                         "5 planned, 2 passed (1 of them reused), 1 KNOWN-RED (a declared exception, not a pass), "
                         "1 NOT RUN (not a pass), 1 timed-out")
        unit, record = self.attempt("known-red")
        self.passed(unit, record)
        self.assertFalse(runner.known_red(unit[0]))
        receipt = evidence.receipt_path(unit[0])
        row = json.loads(receipt.read_text())
        receipt.write_text(json.dumps({**row, "verdict": "KNOWN-RED"}) + "\n")
        self.assertTrue(runner.known_red(unit[0]))

    def test_coverage_names_a_known_red_unit_as_an_exception_never_proof(self):
        """Review rv-20261020T073921Z-1e0461cc-0956 #2: a normally saved KNOWN-RED receipt was
        counted as proven while reuse() rejects it. Both now share receipt_verdict()."""
        old, previous = self.attempt("old")
        self.passed(old, previous)
        self.assertIsNone(evidence.coverage(previous.logdir, old, {"check": self.inputs}, self.source)["check"])
        receipt = evidence.receipt_path(old[0])
        row = json.loads(receipt.read_text())
        receipt.write_text(json.dumps({**row, "verdict": "KNOWN-RED", "rc": 1}) + "\n")
        previous.save(old[0], self.source)  # saved as a run saves it: its digest recorded
        gap = evidence.coverage(previous.logdir, old, {"check": self.inputs}, self.source)["check"]
        self.assertTrue(gap.startswith(evidence.KNOWN_RED_GAP), gap)
        new, current = self.attempt("new")
        evidence.reuse(previous.logdir, new, current)
        self.assertEqual(new[0].state, "waiting", "coverage and reuse agree")

    def test_coverage_reads_inputs_after_the_runtime_setup_a_run_gives(self):
        """Review rv-20261020T073921Z-1e0461cc-0956 #1: a run supplies RICHOS_RUNTIME_DIR before
        its checks' inputs are read; --coverage did not, so an unchanged check that binds it was
        reported missing. The reviewer's probe, its assertion now the fixed behavior."""
        import runtime_cache
        base = Path(self.tmp.name)
        root = base / "rt-repo"
        root.mkdir()
        runtime = base / "verified-runtime"
        runtime.mkdir()
        (runtime / "fixture").write_text("runtime bytes\n")
        (root / "review.md").write_text("Fixture external runtime input.\n")
        (root / "fixture.sh").write_text("exit 0\n")
        evidence.atomic(root / "qualification.json", {"schema": 1, "review": "review.md", "requires": {
            "paths": [], "tools": [], "environment": [], "external": ["RICHOS_RUNTIME_DIR"]}})
        (root / "richos/app/scripts").mkdir(parents=True)
        evidence.atomic(root / "richos/app/scripts/proof-inputs.json", {"schema": 1, "checks": {"fixture": {
            "paths": ["fixture.sh"], "tools": [], "environment": [], "external": ["RICHOS_RUNTIME_DIR"],
            "qualification": "qualification.json"}}})
        source = {"commit": "c" * 40, "tracked_diff_sha256": "clean", "untracked_sha256": "clean"}
        saved = base / "rt-saved"
        saved.mkdir()
        lines = ["cd . && bash fixture.sh"]
        with patch.object(runner, "author_coverage", side_effect=lambda args: args):
            args = runner.run_main(["--coverage", str(saved)], {})
        verified = (patch.object(runtime_cache, "cache_path", return_value=runtime),
                    patch.object(runner.subprocess, "run", return_value=SimpleNamespace(returncode=0)))
        clean = {k: v for k, v in os.environ.items() if k not in ("RICHOS_RUNTIME_DIR", "RICHOS_GUI_SPEECH_MODELS")}
        with patch.object(runner, "ROOT", str(root)), \
                patch.object(runner, "head_commit", return_value=source["commit"]), \
                patch.object(runner, "source_identity", return_value=source), \
                patch.object(runner, "default_logdir", return_value=str(base / "rt-history")), \
                patch.object(runner, "selection", return_value=lines), \
                patch.dict(os.environ, clean, clear=True):
            items = runner.plan(lines, args, str(saved), {})
            with verified[0], verified[1]:
                runner.supply_runtime(items)
            identities = {i.label: runner.input_identity(i, args, str(saved)) for i in items}
            record = evidence.Record(root, saved, items, source, identities, selection={
                "base": "b" * 40, "candidate": source["commit"], "mode": "normal"})
            self.addCleanup(record.close)
            items[0].state, items[0].rc, items[0].log = "passed", 0, str(saved / "execution.log")
            Path(items[0].log).write_text("passed\n")
            record.checkpoint(items)
            evidence.atomic(saved / "summary.json", {"checks": [{"check": "fixture", "result": "passed"}]})
            with verified[0], verified[1], contextlib.redirect_stdout(io.StringIO()) as output:
                rc = runner.author_coverage(args)
        self.assertEqual(rc, 0, output.getvalue())
        self.assertIn("1 proven by the saved run", output.getvalue())
        self.assertIn("0 run fresh at every seat", output.getvalue())

    def test_live_owner_prevents_resume_and_rotation(self):
        items, record = self.attempt("20260923T000000Z")
        with self.assertRaisesRegex(ValueError, "active owner"):
            evidence.Lease(record.logdir)
        for n in range(4):
            directory = Path(self.tmp.name) / ("20260923T00000%dZ" % n)
            directory.mkdir(exist_ok=True)
            evidence.atomic(directory / "summary.json", {"checks": [{"result": "passed"}]})
        runner.rotate(self.tmp.name)
        self.assertTrue(record.logdir.exists())

    def test_input_inventory_modes_links_tools_environment_and_fixtures(self):
        (self.root / "input").mkdir()
        fixture = self.root / "input" / "data"
        fixture.write_text("one")
        tool = self.root / "tool"
        tool.write_text("#!/bin/sh\nexit 0\n")
        tool.chmod(0o755)
        qualification = self.root / "qualification.md"
        self.qualification("fixture input contract", paths=["input"], tools=["tool"],
            environment=["OPTION"], external=["FIXTURE"])
        external = Path(self.tmp.name) / "external"
        external.write_text("one")
        recipe = {"paths": ["input"], "tools": ["tool"], "environment": ["OPTION"],
                  "external": ["FIXTURE"], "qualification": "qualification.json"}
        env = {"PATH": str(self.root), "OPTION": "secret-one", "FIXTURE": str(external)}
        base = evidence.recipe_identity(self.root, recipe, env)
        self.assertNotIn("secret-one", json.dumps(base))
        for path in (fixture, tool, qualification, external):
            original = path.read_bytes()
            path.write_bytes(original + b"changed")
            self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, env), str(path))
            path.write_bytes(original)
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {**env, "OPTION": "secret-two"}))
        fixture.chmod(0o755)
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, env))
        fixture.chmod(0o644)
        (self.root / "input" / "new-untracked").write_text("new")
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, env))
        (self.root / "input" / "loop").symlink_to(".")
        with self.assertRaisesRegex(ValueError, "cyclic"):
            evidence.recipe_identity(self.root, recipe, env)

    def cache_fixture(self):
        """A checkout whose declared input `input` holds a source file and, beside it, what a
        build writes: a Cargo-style target directory (tagged, ignored), an ignored directory
        without the tag, and a tagged directory git does not ignore."""
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=env)
        (self.root / ".gitignore").write_text("target/\nplain-output/\n")
        crate = self.root / "input" / "crate"
        crate.mkdir(parents=True)
        (crate / "lib.rs").write_text("fn main() {}\n")
        for name, tagged in (("target", True), ("plain-output", False), ("tagged-source", True)):
            (crate / name).mkdir()
            if tagged:
                (crate / name / "CACHEDIR.TAG").write_bytes(CACHEDIR_SIGNATURE + b"\n# a cache\n")
            (crate / name / "product.bin").write_text("one")
        self.qualification("fixture input contract", paths=["input"])
        recipe = {"paths": ["input"], "tools": [], "environment": [], "external": [],
                  "qualification": "qualification.json"}
        return crate, recipe, env

    def test_a_build_cache_written_beside_a_declared_input_is_not_that_input(self):
        # 2026-09-29: the lint's Clippy wrote crates/richos-user-update/target while
        # no-foreign-app-data, whose inputs include richos/app/crates, ran; its pass became
        # "invalid: execution inputs changed during the check". A build cache is output.
        crate, recipe, env = self.cache_fixture()
        base = evidence.recipe_identity(self.root, recipe, {})
        (crate / "target" / "product.bin").write_text("rebuilt")
        (crate / "target" / "debug").mkdir()
        (crate / "target" / "debug" / "lib.rmeta").write_text("new artifact")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "a write inside the build cache")
        shutil.rmtree(crate / "target")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "the build cache removed")
        # Everything else is bound exactly as before: sources, an untagged ignored directory,
        # a tagged directory git would land, and a tagged ignored one holding a tracked file.
        for path in (crate / "lib.rs", crate / "plain-output" / "product.bin", crate / "tagged-source" / "product.bin"):
            with self.subTest(path=str(path.relative_to(self.root))):
                original = path.read_text()
                path.write_text(original + " changed")
                self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {}))
                path.write_text(original)
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}))
        (crate / "target").mkdir()
        (crate / "target" / "CACHEDIR.TAG").write_bytes(CACHEDIR_SIGNATURE + b"\n")
        (crate / "target" / "kept.txt").write_text("tracked on purpose")
        subprocess.run(["git", "-C", str(self.root), "add", "-f", "input/crate/target/kept.txt"], check=True, env=env)
        with_tracked = evidence.recipe_identity(self.root, recipe, {})
        self.assertNotEqual(base, with_tracked, "a cache directory holding a tracked file is bound")
        (crate / "target" / "kept.txt").write_text("changed")
        self.assertNotEqual(with_tracked, evidence.recipe_identity(self.root, recipe, {}))

    def test_python_bytecode_written_beside_a_declared_input_is_not_that_input(self):
        # 2026-09-30: a hook test ran guard-idle-land.py and Python wrote
        # hooks/__pycache__/guard-idle-land.cpython-314.pyc; six checks whose inputs hold that
        # directory became "invalid: execution inputs changed during the check".
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=env)
        (self.root / ".gitignore").write_text("__pycache__/\n")
        hooks = self.root / "input" / "hooks"
        hooks.mkdir(parents=True)
        (hooks / "guard.py").write_text("print('x')\n")
        self.qualification("fixture input contract", paths=["input"])
        recipe = {"paths": ["input"], "tools": [], "environment": [], "external": [],
                  "qualification": "qualification.json"}
        base = evidence.recipe_identity(self.root, recipe, {})
        (hooks / "__pycache__").mkdir()
        (hooks / "__pycache__" / "guard.cpython-314.pyc").write_bytes(b"compiled")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "bytecode appeared")
        (hooks / "__pycache__" / "guard.cpython-314.pyc").write_bytes(b"recompiled")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "bytecode changed")
        (hooks / "guard.py").write_text("print('y')\n")
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {}), "the source is still bound")
        # A __pycache__ git would land (not ignored) is bound exactly as before.
        (self.root / ".gitignore").write_text("")
        (hooks / "guard.py").write_text("print('x')\n")
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {}), "an unignored directory is input")

    def test_test_output_written_beside_a_declared_input_is_not_that_input(self):
        # 2026-09-29: the push of c7491c34 was refused on no-foreign-app-data `invalid`,
        # "changed: richos/app/ui/tests/.shots/...": its inputs include richos/app/ui and the UI
        # suites beside it wrote their per-run screenshots there. Alone it passes 3 of 3.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=env)
        ui = self.root / "richos/app/ui"
        tests = ui / "tests"
        tests.mkdir(parents=True)
        (tests / ".gitignore").write_text(".shots/\nreceipts/\n.vouch/\nscratch/\n")
        (ui / "main.js").write_text("export const x = 1;\n")
        for name in (".shots", "receipts", ".vouch", "scratch"):
            (tests / name).mkdir()
            (tests / name / "one.png").write_text("first run")
        self.qualification("fixture input contract", paths=["richos/app/ui"])
        recipe = {"paths": ["richos/app/ui"], "tools": [], "environment": [], "external": [],
                  "qualification": "qualification.json"}
        base = evidence.recipe_identity(self.root, recipe, {})
        for name in (".shots", "receipts", ".vouch"):
            with self.subTest(output=name):
                (tests / name / "one.png").write_text("another run's picture")
                (tests / name / "changed").mkdir()
                (tests / name / "changed" / "two.png").write_text("new")
                self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "a write into " + name)
        shutil.rmtree(tests / ".shots")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "the output removed")
        # Everything else is bound exactly as before: the source, an ignored directory nobody
        # declared as output, and a declared one that holds a tracked file.
        for path in (ui / "main.js", tests / "scratch" / "one.png"):
            with self.subTest(path=str(path.relative_to(self.root))):
                original = path.read_text()
                path.write_text(original + " changed")
                self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {}))
                path.write_text(original)
        (tests / ".shots").mkdir()
        (tests / ".shots" / "kept.png").write_text("tracked on purpose")
        subprocess.run(["git", "-C", str(self.root), "add", "-f", "richos/app/ui/tests/.shots/kept.png"],
                       check=True, env=env)
        with_tracked = evidence.recipe_identity(self.root, recipe, {})
        (tests / ".shots" / "kept.png").write_text("changed")
        self.assertNotEqual(with_tracked, evidence.recipe_identity(self.root, recipe, {}),
                            "an output directory holding a tracked file is bound")

    def test_a_hook_writing_the_engines_runtime_state_invalidates_no_engine_check(self):
        # 2026-10-05 (merge-check speed, section 2): a hook suite seated on the engine appended a
        # line to richos/engine/.claude/state/ while the engine checks beside it ran, and every
        # one whose inputs declare richos/engine came back "invalid: execution inputs changed".
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=env)
        engine = self.root / "richos/engine"
        state = engine / ".claude/state"
        state.mkdir(parents=True)
        (engine / ".gitignore").write_text("/.claude/state/\n")
        (engine / "guard.sh").write_text("exit 0\n")
        (state / "unevaluated-payloads.log").write_text("first\n")
        self.qualification("fixture input contract", paths=["richos/engine"])
        recipe = {"paths": ["richos/engine"], "tools": [], "environment": [], "external": [],
                  "qualification": "qualification.json"}
        base = evidence.recipe_identity(self.root, recipe, {})
        with open(state / "unevaluated-payloads.log", "a") as log:
            log.write("2026-10-05T02:13:38Z\tunevaluated\thook=guard-worktree-isolation.sh\treason=not-json\n")
        (state / "main-checkout-runs.log").write_text("agent=worker\n")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "a line appended to the state")
        (engine / "guard.sh").write_text("exit 1\n")
        self.assertNotEqual(base, evidence.recipe_identity(self.root, recipe, {}), "the engine's code is bound")

    def test_a_ui_gates_generated_reference_is_output_and_no_gate_makes_one_elsewhere(self):
        # Recheck R07 v2 (2026-10-01): gate-honesty.js made `shots-gate-honesty-<pid>/` inside
        # ui/tests, unignored, so a proof whose inputs include richos/app/ui bound it and lost
        # its pass when the gate removed it. The pattern: a UI gate creating its own
        # reference folder in the tests tree outside a declared output directory.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=env)
        ui = self.root / "richos/app/ui"
        tests = ui / "tests"
        tests.mkdir(parents=True)
        (tests / ".gitignore").write_text(".generated-references/\n")
        (ui / "main.js").write_text("export const x = 1;\n")
        self.qualification("fixture input contract", paths=["richos/app/ui"])
        recipe = {"paths": ["richos/app/ui"], "tools": [], "environment": [], "external": [],
                  "qualification": "qualification.json"}
        base = evidence.recipe_identity(self.root, recipe, {})
        reference = tests / ".generated-references" / "gate-honesty-4242" / "fixture-reference.png"
        reference.parent.mkdir(parents=True)
        reference.write_bytes(b"png")
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "a reference created mid-proof")
        shutil.rmtree(reference.parent)
        self.assertEqual(base, evidence.recipe_identity(self.root, recipe, {}), "a reference removed mid-proof")
        import re
        gates = Path(__file__).resolve().parent.parent / "ui/tests"
        stray = [f"{js.name}: {m}" for js in sorted(gates.glob("*.js"))
                 for m in re.findall(r'"shots-[A-Za-z0-9-]*"\s*\+\s*process\.pid', js.read_text())]
        self.assertEqual(stray, [], "a gate makes its reference folder outside .generated-references")

    def test_an_invalidated_pass_names_what_changed_and_the_checks_started_by_then(self):
        crate, recipe, _env = self.cache_fixture()
        scripts = self.root / "richos/app/scripts"
        scripts.mkdir(parents=True)
        evidence.atomic(scripts / "proof-inputs.json", {"schema": 1, "checks": {"check": recipe}})
        baseline = evidence.InputSnapshot()
        items, record = self.attempt("explained", engine=False,
                                     identities={"check": evidence.recipe_identity(self.root, recipe, {}, baseline)})
        writer = runner.Item("writer", str(self.root), ["true"])
        writer.started = items[0].started = 1.0
        with patch.object(runner, "ROOT", str(self.root)):
            record.explain = lambda item: getattr(runner, "describe_input_change", lambda *a: "")(item, baseline, items + [writer])
            record.current_identity = lambda item: evidence.recipe_identity(self.root, recipe, {})
            (crate / "target" / "product.bin").write_text("a build beside the check")
            self.passed(items, record)
            self.assertEqual(items[0].state, "passed", items[0].notes)
            (crate / "lib.rs").write_text("fn main() { changed(); }\n")
            record.save(items[0], self.source)
        self.assertEqual(items[0].state, "invalid")
        self.assertIn("execution inputs changed during the check; changed: input/crate/lib.rs; "
                      "checks started by then: check, writer", record.results["check"]["invalid"])

    def test_refused_receipt_cannot_become_saved_pass(self):
        items, record = self.attempt("attempt")
        self.passed(items, record)
        row = json.loads(evidence.receipt_path(items[0]).read_text())
        row.update(execution_status="refused", verdict="KNOWN-RED")
        evidence.receipt_path(items[0]).write_text(json.dumps(row) + "\n")
        record.save(items[0], self.source)
        self.assertEqual(items[0].state, "invalid")

    def test_live_known_red_is_accepted_but_never_cached(self):
        old, previous = self.attempt("old")
        self.passed(old, previous)
        receipt = evidence.receipt_path(old[0])
        row = json.loads(receipt.read_text())
        row.update(verdict="KNOWN-RED", rc=1)
        receipt.write_text(json.dumps(row) + "\n")
        previous.save(old[0], self.source)
        self.assertEqual(old[0].state, "passed")
        new, current = self.attempt("new")
        evidence.reuse(previous.logdir, new, current)
        self.assertEqual(new[0].state, "waiting")

    def test_actual_failed_attempt_resumes_without_executing_its_pass_again(self):
        output = Path(self.tmp.name) / "executions"
        failure = Path(self.tmp.name) / "fail"
        failure.touch()
        olddir, newdir = [Path(self.tmp.name) / name for name in ("first", "second")]
        olddir.mkdir()
        newdir.mkdir()
        script = ("import json,pathlib,sys; label=sys.argv[1]; "
            "state=json.load(open(sys.argv[2])); assert state[label]['state']=='running'; "
            "out=pathlib.Path(sys.argv[3]); out.open('a').write(label+'\\n'); "
            "sys.exit(1 if label=='retry' and pathlib.Path(sys.argv[4]).exists() else 0)")
        old = [runner.Item(label, str(self.root), [sys.executable, "-c", script, label,
                str(olddir / "outcomes.json"), str(output), str(failure)], lane="fixture")
               for label in ("pass", "retry")]
        identities = {item.label: self.inputs for item in old}
        previous = evidence.Record(str(self.root), olddir, old, self.source, identities)
        self.addCleanup(previous.close)
        args = SimpleNamespace(capacity=4, engine_shards=4, admission_wait=1800,
            max_cpu=80, budget=600, deadline=1800, sample_every=.5, evidence=previous)
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), contextlib.redirect_stdout(io.StringIO()):
            runner.run(old, args, str(olddir), sampler=idle)
            self.assertEqual([item.state for item in old], ["passed", "failed"])
            new = [evidence.decode_item(row, runner.Item, self.root, newdir) for row in previous.plan["items"]]
            current = evidence.Record(str(self.root), newdir, new, self.source, identities)
            self.addCleanup(current.close)
            failure.unlink()
            evidence.reuse(olddir, new, current)
            args.evidence = current
            runner.run(new, args, str(newdir), sampler=idle)
        self.assertEqual([item.state for item in new], ["passed", "passed"])
        self.assertEqual(output.read_text().splitlines(), ["pass", "retry", "retry"])
        self.assertEqual(json.loads((olddir / "outcomes.json").read_text())["retry"]["state"], "failed")

    def test_missing_qualification_is_not_a_reuse_identity(self):
        with self.assertRaisesRegex(ValueError, "qualification is missing"):
            evidence.recipe_identity(self.root, {"paths": [], "tools": [], "environment": [],
                "external": [], "qualification": "missing.md"}, {})

    def test_later_input_or_artifact_mutation_invalidates_an_earlier_pass(self):
        for change in ("input", "log", "receipt"):
            with self.subTest(change=change):
                items, record = self.attempt(change)
                self.passed(items, record)
                if change == "input":
                    record.current_identity = lambda item: {"external": "changed later"}
                elif change == "log":
                    Path(items[0].log).write_text("replaced after completion")
                else:
                    evidence.receipt_path(items[0]).write_text("replaced after completion")
                record.finalize(items)
                self.assertEqual(items[0].state, "invalid")
                self.assertEqual(record.results["check"]["exit"], 125)

    def test_cli_restores_exact_plan_and_reports_reconciled_coverage(self):
        scripts = self.root / "richos/app/scripts"
        scripts.mkdir(parents=True)
        output = Path(self.tmp.name) / "executions"
        failure = Path(self.tmp.name) / "failure"
        failure.touch()
        for name in ("pass", "retry"):
            (self.root / (name + ".test.sh")).write_text(
                'printf "%s\\n" "' + name + '" >> "$1"\n'
                + ('[ ! -f "$2" ]\n' if name == "retry" else 'exit 0\n'))
        self.qualification("Controlled shell fixture: only supplied scripts and arguments.")
        recipe = {"paths": ["pass.test.sh", "retry.test.sh"], "tools": ["bash"],
            "external": [], "environment": ["PATH"], "qualification": "qualification.json"}
        evidence.atomic(scripts / "proof-inputs.json", {"schema": 1, "checks": {"pass": recipe, "retry": recipe}})
        olddir, newdir = [Path(self.tmp.name) / name for name in ("first", "second")]
        lines = [f"cd . && bash {name}.test.sh {output} {failure}" for name in ("pass", "retry")]
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        captured = io.StringIO()
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), \
                patch.object(runner, "source_identity", return_value=self.source), \
                patch.object(runner, "head_commit", return_value=self.source["commit"]), \
                patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                patch.object(evidence, "pool_directory", return_value=Path(self.tmp.name) / "pool"), \
                patch.object(runner, "supply_runtime", return_value="private fixture"), \
                patch.object(runner.reserve, "host_sample", side_effect=idle), \
                patch.object(runner, "selection", return_value=lines) as selection, \
                contextlib.redirect_stdout(captured):
            self.assertEqual(runner.main(["--log-dir", str(olddir)]), 1)
            failure.unlink()
            self.assertEqual(runner.main(["--resume", str(olddir), "--log-dir", str(newdir),
                "--retry-reason", "removed the fixture failure control"]), 0)
            self.assertEqual(selection.call_count, 1)
        self.assertEqual(output.read_text().splitlines(), ["pass", "retry", "retry"])
        self.assertIn("reconciled with 1 reused result", captured.getvalue())
        outcomes = {row["check"]: row["result"] for row in json.loads((olddir / "summary.json").read_text())["checks"]}
        self.assertEqual(outcomes["retry"], "failed")

    def test_a_crash_in_the_runner_leaves_a_readable_summary_that_is_not_a_verdict(self):
        # 2026-10-05: the crash reusing a duplicate plan left no summary.json at all, and its exit 1
        # is the code a failing check returns. A crash writes a readable summary naming every
        # check CRASHED and exits CRASHED, which the merge gate reads as no verdict and refuses.
        scripts = self.root / "richos/app/scripts"
        scripts.mkdir(parents=True)
        (self.root / "pass.test.sh").write_text("exit 0\n")
        self.qualification("Controlled shell fixture: only supplied scripts and arguments.")
        recipe = {"paths": ["pass.test.sh"], "tools": ["bash"], "external": [], "environment": ["PATH"],
                  "qualification": "qualification.json"}
        evidence.atomic(scripts / "proof-inputs.json", {"schema": 1, "checks": {"pass": recipe}})
        logdir, copy = Path(self.tmp.name) / "crashed", Path(self.tmp.name) / "crashed-summary.json"
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), \
                patch.object(runner, "source_identity", return_value=self.source), \
                patch.object(runner, "head_commit", return_value=self.source["commit"]), \
                patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                patch.object(evidence, "pool_directory", return_value=Path(self.tmp.name) / "pool"), \
                patch.object(runner, "supply_runtime", return_value="private fixture"), \
                patch.object(runner.reserve, "host_sample", side_effect=idle), \
                patch.object(runner, "selection", return_value=["cd . && bash pass.test.sh"]), \
                patch.object(runner, "run", side_effect=RuntimeError("injected runner fault")), \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            rc = runner.main(["--log-dir", str(logdir), "--summary-out", str(copy)])
        self.assertEqual(rc, runner.CRASHED)
        self.assertNotIn(rc, (0, 1, 3))
        for path in (logdir / "summary.json", copy):
            summary = json.loads(path.read_text())
            self.assertIn("injected runner fault", summary["crashed"])
            self.assertEqual([(row["check"], row["result"]) for row in summary["checks"]], [("pass", "crashed")])

    def test_unqualified_passes_are_kept_on_the_same_tree_and_rerun_on_any_change(self):
        # 2026-09-29: cc/echo-opus-speckle3's land ran one 78-check selection five times on one
        # tree (three refused merges, `--resume`, the push); ~70 checks have no reviewed input
        # contract, were `fresh`, and every retry ran again what had already passed there.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        # A fixture repository: no machine hooks (this Mac's global hooksPath) and no signing.
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        output = Path(self.tmp.name) / "executions"
        failure = Path(self.tmp.name) / "failure"
        failure.touch()
        for name in ("pass", "retry"):
            (self.root / (name + ".test.sh")).write_text(
                'printf "%s\\n" "' + name + '" >> "$1"\n' + ('[ ! -f "$2" ]\n' if name == "retry" else 'exit 0\n'))
        git("add", "-A")
        git("commit", "-q", "-m", "fixture")
        lines = [f"cd . && bash {name}.test.sh {output} {failure}" for name in ("pass", "retry")]
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        runs = [Path(self.tmp.name) / name for name in ("first", "resumed", "again", "changed")]
        captured = io.StringIO()
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), \
                patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                patch.object(evidence, "pool_directory", return_value=Path(self.tmp.name) / "pool"), \
                patch.object(runner, "supply_runtime", return_value="private fixture"), \
                patch.object(runner.reserve, "host_sample", side_effect=idle), \
                patch.object(runner, "selection", return_value=lines), \
                contextlib.redirect_stdout(captured):
            self.assertEqual(runner.main(["--log-dir", str(runs[0])]), 1)
            failure.unlink()
            # --resume keeps the pass and runs only what did not pass.
            self.assertEqual(runner.main(["--resume", str(runs[0]), "--log-dir", str(runs[1])]), 0,
                             captured.getvalue()[-3000:])
            self.assertEqual(output.read_text().splitlines(), ["pass", "retry", "retry"])
            # A new run of the same selection on the same tree (a land tried again, the push
            # after it) runs nothing that passed on this tree.
            self.assertEqual(runner.main(["--log-dir", str(runs[2])]), 0)
            self.assertEqual(output.read_text().splitlines(), ["pass", "retry", "retry"])
            # One changed byte anywhere git would land, even in neither check's file, and both run.
            (self.root / "unrelated.txt").write_text("a new untracked file")
            self.assertEqual(runner.main(["--log-dir", str(runs[3])]), 0)
        executions = output.read_text().splitlines()
        # The last run's start order follows the measured weights of the runs before it.
        self.assertEqual((executions[:3], sorted(executions[3:])), (["pass", "retry", "retry"], ["pass", "retry"]))
        reused = {row["check"]: row["reused_from"] for row in json.loads((runs[2] / "summary.json").read_text())["checks"]}
        self.assertEqual(set(reused), {"pass", "retry"})
        self.assertTrue(all(reused.values()), reused)

    def test_resume_only_check_runs_the_named_check_and_leaves_the_other_unfinished_ones(self):
        # Hunt v2 V02 (2026-10-01): --resume restored every unfinished check to the scheduler,
        # so a retry described as "alone" ran the competing checks again, concurrently.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        output = Path(self.tmp.name) / "executions"
        failure = Path(self.tmp.name) / "failure"
        failure.touch()
        names = ("pass", "retry", "other")
        for name in names:
            (self.root / (name + ".test.sh")).write_text(
                'printf "%s\\n" "' + name + '" >> "$1"\n' + ('exit 0\n' if name == "pass" else '[ ! -f "$2" ]\n'))
        git("add", "-A")
        git("commit", "-q", "-m", "fixture")
        lines = [f"cd . && bash {name}.test.sh {output} {failure}" for name in names]
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        runs = [Path(self.tmp.name) / name for name in ("first", "resumed")]
        captured = io.StringIO()
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), \
                patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                patch.object(evidence, "pool_directory", return_value=Path(self.tmp.name) / "pool"), \
                patch.object(runner, "supply_runtime", return_value="private fixture"), \
                patch.object(runner.reserve, "host_sample", side_effect=idle), \
                patch.object(runner, "selection", return_value=lines), \
                contextlib.redirect_stdout(captured):
            self.assertEqual(runner.main(["--log-dir", str(runs[0])]), 1)
            failure.unlink()
            self.assertEqual(sorted(output.read_text().splitlines()), ["other", "pass", "retry"])
            code = runner.main(["--resume", str(runs[0]), "--only-check", "retry", "--log-dir", str(runs[1])])
            self.assertEqual(code, 3, captured.getvalue()[-3000:])  # nothing failed; `other` is NOT RUN
            # The retry ran only `retry`; `other` did not run again, and says why.
            self.assertEqual(sorted(output.read_text().splitlines()), ["other", "pass", "retry", "retry"])
            rows = {row["check"]: row for row in json.loads((runs[1] / "summary.json").read_text())["checks"]}
            self.assertEqual(rows["retry"]["result"], "passed")
            self.assertEqual(rows["pass"]["result"], "passed")
            self.assertEqual(rows["other"]["result"], "not-run")
            self.assertEqual(rows["other"]["not_run"]["suites"][0]["state"], "retry-unselected")
            # A name the saved plan does not hold is refused, never ignored.
            with self.assertRaises(SystemExit) as refused:
                runner.main(["--resume", str(runs[0]), "--only-check", "nosuch",
                             "--log-dir", str(Path(self.tmp.name) / "third")])
            self.assertIn("nosuch", str(refused.exception))

    def test_a_saved_pass_is_not_reused_after_an_installed_dependency_changes(self):
        # Hunt part 2, finding 11: a check that passed with an ignored node_modules dependency
        # was reused, and finalized as passed, after only that dependency changed, although
        # running it again failed. The whole-checkout identity bound what git would land and
        # nothing git ignores. An installed dependency is an input; a bytecode cache is not.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        output = Path(self.tmp.name) / "executions"
        (self.root / ".gitignore").write_text("node_modules/\n__pycache__/\n")
        (self.root / "dep.test.sh").write_text(
            'printf "dep\\n" >> "$1"\ngrep -q "working" node_modules/dep/index.js\n')
        dependency = self.root / "node_modules/dep/index.js"
        dependency.parent.mkdir(parents=True)
        dependency.write_text("module.exports = 'working';\n")
        git("add", "-A")
        git("commit", "-q", "-m", "fixture")
        lines = [f"cd . && bash dep.test.sh {output}"]
        idle = lambda: {"cpu_user_percent": 5, "cpu_system_percent": 2,
            "memory_pressure": "normal", "swapout_mb_per_s": 0, "memory_free_percent": 80,
            "swap_used_mb": 0}
        runs = [Path(self.tmp.name) / name for name in ("first", "dependency", "restored")]
        captured = io.StringIO()
        with patch.dict(os.environ, {"RICHOS_MACHINE_WORKERS": str(Path(self.tmp.name) / "machine"),
                                      "CLAUDE_CONFIG_DIR": str(Path(self.tmp.name) / "config")}), \
                patch.object(runner, "ROOT", str(self.root)), \
                patch.object(runner, "default_logdir", return_value=str(Path(self.tmp.name) / "history")), \
                patch.object(evidence, "pool_directory", return_value=Path(self.tmp.name) / "pool"), \
                patch.object(runner, "supply_runtime", return_value="private fixture"), \
                patch.object(runner.reserve, "host_sample", side_effect=idle), \
                patch.object(runner, "selection", return_value=lines), \
                contextlib.redirect_stdout(captured):
            self.assertEqual(runner.main(["--log-dir", str(runs[0])]), 0, captured.getvalue()[-3000:])
            # Only the installed dependency changes: nothing git would land differs.
            dependency.write_text("module.exports = 'broken';\n")
            self.assertEqual(git("status", "--porcelain").stdout, b"")
            self.assertEqual(runner.main(["--log-dir", str(runs[1])]), 1,
                             "a pass saved before the dependency changed was reused:\n" + captured.getvalue()[-3000:])
            self.assertEqual(output.read_text().splitlines(), ["dep", "dep"])
            # The same dependency again, plus an ignored bytecode cache written beside it: the
            # first run's pass is exactly as valid as it was, and is reused.
            dependency.write_text("module.exports = 'working';\n")
            (self.root / "__pycache__").mkdir()
            (self.root / "__pycache__/tool.cpython-314.pyc").write_bytes(b"written by a check")
            self.assertEqual(runner.main(["--log-dir", str(runs[2])]), 0, captured.getvalue()[-3000:])
        self.assertEqual(output.read_text().splitlines(), ["dep", "dep"])
        reused = {row["check"]: row["reused_from"] for row in json.loads((runs[2] / "summary.json").read_text())["checks"]}
        self.assertTrue(all(reused.values()), reused)

    def test_new_supervision_owners_do_not_invalidate_unchanged_execution_inputs(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, capture_output=True)
        environment = {"PATH": "/usr/bin:/bin", "HOME": self.tmp.name,
                       "RICHOS_MUTATION_PASSES": "1", "RICHOS_WORKER_TOKENS_RESERVED": "2",
                       "RICHOS_MACHINE_WORKERS": str(self.root / "machine")}
        transport = {"RICHOS_VERIFICATION_OWNER": "123:old-generation",
                     "RICHOS_WORKER_BORROW_LOCK": "/fixture/token-1.child",
                     "RICHOS_PROOF_RUN_SLOT_HELD": "/fixture/slot-0.holder",
                     "RICHOS_PROOF_RUN_SLOT_FDS": "5,6", "RICHOS_UI_TESTS_LEDGER": "/fixture/first.jsonl"}
        identity = lambda env: evidence.checkout_identity(self.root, ["bash", "suite.sh"], env)
        before = identity({**environment, **transport})
        changed = {key: value + "-new" for key, value in transport.items()}
        self.assertEqual(identity({**environment, **changed}), before)
        self.assertEqual(identity(environment), before)
        for key, value in (("RICHOS_MUTATION_PASSES", "0"), ("RICHOS_WORKER_TOKENS_RESERVED", "1"),
                           ("RICHOS_MACHINE_WORKERS", "/other/namespace"), ("RICHOS_UNKNOWN_INPUT", "new")):
            with self.subTest(key=key):
                self.assertNotEqual(identity({**environment, **changed, key: value}), before)

    def test_the_agent_harness_owner_tags_never_make_an_unchanged_pass_read_as_changed(self):
        """2026-10-21, the first land after the switch-over: zach-sonnet-probes305b's handover run
        passed 24 checks at a2389cbfd and the land's preflight, on the same commit in the same
        checkout, called 18 of them "declared execution inputs changed". Measured field by field:
        every one differed only in RICHOS_AGENT_OWNER and RICHOS_AGENT_SESSION, the tags the agent
        harness gives each agent's shell (agent_hold.py TAG, SESSION_TAG); the lead's land carries
        another value or none. They name who started a process, not anything a check reads. A pass
        saved before this rule, with the tags in its identity, stands in too; a real change does not."""
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, capture_output=True)
        land = {"PATH": "/usr/bin:/bin", "HOME": self.tmp.name, "RICHOS_BULK_ROOT": "/fixture/bulk"}
        author = {**land, "RICHOS_AGENT_OWNER": "a2f8c4ad0b1e3c5d7", "RICHOS_AGENT_SESSION": "871311fb-session"}
        identity = lambda env: evidence.checkout_identity(self.root, ["bash", "suite.sh"], env)
        wanted = identity(land)
        self.assertEqual(identity(author), wanted)
        self.assertEqual(identity({**author, "RICHOS_AGENT_OWNER": "another-agent"}), wanted)
        # The saved run in the incident was written by the runner before this rule: its identity
        # carries the tags' digests. It still stands for the land's reading of the same inputs.
        saved = json.loads(json.dumps(wanted))
        saved["environment"].update({name: evidence.digest(author[name])
                                     for name in ("RICHOS_AGENT_OWNER", "RICHOS_AGENT_SESSION")})
        plan = {"source": dict(self.source)}
        old = {"state": "passed", "exit": 0, "source": dict(self.source), "input": saved}
        self.assertIsNone(evidence.evidence_gap("dialect.js", old, wanted, plan))
        # Not loosened: any input the identity binds still invalidates the pass.
        for key, value in (("RICHOS_BULK_ROOT", "/other/bulk"), ("RICHOS_UNKNOWN_INPUT", "new"),
                           ("PATH", "/opt/other:/usr/bin:/bin")):
            with self.subTest(key=key):
                self.assertTrue(evidence.evidence_gap("dialect.js", old, identity({**land, key: value}), plan)
                                .startswith("declared execution inputs changed (environment.%s: saved " % key))
        # A reviewed recipe that names a tag outright still binds it (explicit recipes bind their
        # own keys): only the whole-checkout reading, which binds every RICHOS_ name, leaves them out.
        recipe = {"contract": "recipe", "environment": {"RICHOS_AGENT_OWNER": evidence.digest("x")}}
        self.assertFalse(evidence.stands_in(recipe, {**recipe, "environment": {"RICHOS_AGENT_OWNER": None}}))

    def test_a_cargo_root_named_as_the_machine_settings_give_it_reads_as_one_named_by_none(self):
        """2026-10-21, the land of cc/echo-opus-lease1x at e596d7860: the author's handover run
        20261021T120436Z-6sgk79rr exported the documented CARGO_TARGET_DIR (machine_settings.py cache
        cargo-target --key CARGO_TARGET_DIR) and the land names none, so 55 unchanged passes read as
        "declared execution inputs changed (environment.CARGO_TARGET_DIR: saved 98224b0764c9...,
        now absent)". With no Cargo root named, the shim and the nightly take that same root from
        the settings; RICHOS_CARGO_CACHE_ROOT (which the nightly's conditions add beside it) is its
        other spelling. Either side may be the one that names it. Any other root still counts."""
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, capture_output=True)
        bulk = Path(self.tmp.name) / "bulk"
        bulk.mkdir()
        settings = {"RICHOS_BULK_ROOT": str(bulk), "RICHOS_MACHINE_SETTINGS_RESOLVED": "1"}
        names = ("CARGO_TARGET_DIR", "RICHOS_CARGO_CACHE_ROOT")
        unset = {name: None for name in names}
        documented = str(bulk / "caches/cargo-target")     # what machine_settings.py prints for these settings
        with patch.dict(os.environ, settings):
            for name in names:
                os.environ.pop(name, None)
            land = {"PATH": "/usr/bin:/bin", "HOME": self.tmp.name, **settings}
            identity = lambda env: evidence.checkout_identity(self.root, ["bash", "suite.sh"],
                                                              {k: v for k, v in {**land, **env}.items() if v})
            plan = {"source": dict(self.source)}
            gap = lambda saved, wanted: evidence.evidence_gap(
                "cargo -p richos-core --lib", {"state": "passed", "exit": 0, "source": dict(self.source),
                                               "input": saved}, wanted, plan)
            wanted = identity(unset)
            author = identity({"CARGO_TARGET_DIR": documented})
            self.assertNotEqual(author, wanted)
            self.assertIsNone(gap(author, wanted))     # the incident: named by the author, not by the land
            self.assertIsNone(gap(wanted, author))     # and the other way round
            self.assertIsNone(gap(identity({"CARGO_TARGET_DIR": documented, "RICHOS_CARGO_CACHE_ROOT": documented}),
                                  wanted))
            self.assertEqual(evidence.identity_difference(author, wanted), "")
            # Not loosened: a root that is not the documented one is a real difference, named.
            for env in ({"CARGO_TARGET_DIR": "/other/target"},
                        {"RICHOS_CARGO_CACHE_ROOT": "/other/root"},
                        {"CARGO_TARGET_DIR": documented, "RICHOS_CARGO_CACHE_ROOT": "/other/root"},
                        {"CARGO_TARGET_DIR": "/other/target", "RICHOS_CARGO_CACHE_ROOT": documented}):
                with self.subTest(env=env):
                    self.assertTrue(gap(identity(env), wanted).startswith("declared execution inputs changed (environment."))
                    self.assertTrue(gap(identity(env), author).startswith("declared execution inputs changed (environment."))
            # Another machine-settings root is another documented root: the old one is no longer it.
            with patch.dict(os.environ, {"RICHOS_BULK_ROOT": self.tmp.name}):
                self.assertTrue(gap(author, wanted).startswith("declared execution inputs changed (environment."))
            self.assertEqual(evidence.documented_cargo_root(), documented)

    def test_a_refused_pass_names_each_identity_field_that_differs(self):
        """2026-10-21: two lands in a row refused 21 unchanged passes as "declared execution inputs
        changed", naming no field, and the cause had to be guessed from a reproduction that was not
        the land. The reason names each field, saved value against the value read now; a difference
        stands_in excuses (a run-scoped name, a mutation pass switched off) is not named."""
        saved = {"contract": evidence.WHOLE_CHECKOUT, "tree": "a" * 64,
                 "environment": {"PATH": evidence.digest("/usr/bin:/bin"), "LANG": evidence.digest("en_US.UTF-8"),
                                 "RICHOS_AGENT_OWNER": evidence.digest("author"),
                                 "RICHOS_MUTATION_PASSES": evidence.digest("1")},
                 "tools": {"git": {"path": "/usr/bin/git", "sha256": "b" * 64}}}
        wanted = json.loads(json.dumps(saved))
        wanted["environment"].update({"PATH": evidence.digest("/opt/git-core:/usr/bin:/bin"),
                                      "RICHOS_MUTATION_PASSES": evidence.digest("0"),
                                      "RICHOS_BULK_ROOT": evidence.digest("/other/bulk"),
                                      "RICHOS_AUTOCHECK_LAND_TARGET": evidence.digest("main")})
        del wanted["environment"]["RICHOS_AGENT_OWNER"]
        wanted["tools"]["git"] = {"path": "/opt/git-core/git", "sha256": "c" * 64}
        plan = {"source": dict(self.source)}
        old = {"state": "passed", "exit": 0, "source": dict(self.source), "input": saved}
        gap = evidence.evidence_gap("autocheck", old, wanted, plan)
        self.assertEqual(gap, "declared execution inputs changed ("
                         "environment.PATH: saved %s..., now %s...; "
                         "environment.RICHOS_BULK_ROOT: saved absent, now %s...; "
                         "tools.git.path: saved /usr/bin/git, now /opt/git-core/git; "
                         "tools.git.sha256: saved bbbbbbbbbbbb..., now cccccccccccc...)" % (
                             saved["environment"]["PATH"][:12], wanted["environment"]["PATH"][:12],
                             evidence.digest("/other/bulk")[:12]))
        # Equal identities still prove; the diagnostic never changes what is accepted.
        self.assertIsNone(evidence.evidence_gap("autocheck", old, saved, plan))
        self.assertEqual(evidence.identity_difference(saved, None), "")

    def test_a_land_from_gits_hook_reads_an_unchanged_pass_as_the_authors_shell_did(self):
        """2026-10-21, measured in a disposable clone's real `git merge` with the land's hooks and
        environment: the preflight called 20 of the author's 24 unchanged passes changed. Two fields
        did it. git starts every hook with its exec-path first in PATH (environment.PATH, and `git`
        resolved inside git-core; second once the Cargo shim or reserve.py puts a directory in front
        of it), and main's copy of the gate, which is the one a land runs, asks with the lander's
        RICHOS_AUTOCHECK_LAND_TARGET set. A real change still reads as changed."""
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, capture_output=True)
        exec_dir = subprocess.run(["git", "--exec-path"], check=True, capture_output=True, text=True).stdout.strip()
        bulk, other = Path(self.tmp.name) / "bulk", Path(self.tmp.name) / "other-bulk"
        bulk.mkdir()
        other.mkdir()
        shell = {"PATH": os.environ["PATH"], "HOME": self.tmp.name, "RICHOS_BULK_ROOT": str(bulk)}
        hook = {**shell, "PATH": exec_dir + os.pathsep + shell["PATH"], "RICHOS_AUTOCHECK_LAND_TARGET": "main"}
        shim = str(Path(self.tmp.name) / "shim")

        def read(environment):
            """The PATH and identity a check gets from a runner launched with `environment`."""
            item = runner.Item("check", str(self.root), ["bash", "suite.sh"])
            with patch.dict(os.environ, runner.runner_environment(environment), clear=True):
                env = runner.execution_environment(item)
            return env["PATH"], evidence.checkout_identity(self.root, item.argv, env)

        author_path, author = read(shell)
        self.assertNotIn(exec_dir, author_path.split(os.pathsep))
        for launched in (hook, {**hook, "PATH": shim + os.pathsep + hook["PATH"]}):
            with self.subTest(path=launched["PATH"][:80]):
                expected = read({**shell, "PATH": launched["PATH"].replace(exec_dir + os.pathsep, "")})
                self.assertEqual(read(launched), expected, "the check runs as the author's shell would run it")
        self.assertEqual(read(hook), (author_path, author))
        self.assertEqual(evidence.identity_difference(author, read(hook)[1]), "")
        # Not loosened: another directory first in PATH, or a real setting, is still a change.
        for changed in ({"PATH": self.tmp.name + os.pathsep + shell["PATH"]}, {"RICHOS_BULK_ROOT": str(other)}):
            with self.subTest(changed=changed):
                self.assertNotEqual(read({**hook, **changed})[1], author)

    def test_the_whole_checkout_identity_binds_installed_browsers_and_the_rust_toolchain(self):
        # The same finding names the browser suites' external browser installations and the
        # Rust toolchain that `cargo` resolves: neither is in the checkout, and a changed one
        # can turn a pass into a failure on the same tree.
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, capture_output=True)
        bin_dir = Path(self.tmp.name) / "bin"
        bin_dir.mkdir()
        rustc = bin_dir / "rustc"
        rustc.write_text('#!/bin/sh\necho "rustc 1.90.0 (fixture)"\n')
        rustc.chmod(0o755)
        browsers = Path(self.tmp.name) / "ms-playwright"
        (browsers / "webkit-2311").mkdir(parents=True)
        (browsers / "webkit-2311/INSTALLATION_COMPLETE").touch()
        (browsers / "mcp-chrome").mkdir()
        environment = {"PATH": str(bin_dir) + os.pathsep + "/usr/bin:/bin",
                       "HOME": self.tmp.name, "PLAYWRIGHT_BROWSERS_PATH": str(browsers)}
        identity = lambda: evidence.checkout_identity(self.root, ["bash", "x.sh"], environment)
        before = identity()
        # A browser profile another tool keeps beside the installs is not an install.
        (browsers / "mcp-chrome/Preferences").write_text("{}")
        (browsers / "cli-update-check.json").write_text("{}")
        self.assertEqual(identity(), before)
        # A reinstalled browser at the same revision is a different browser.
        shutil.rmtree(browsers / "webkit-2311")
        (browsers / "webkit-2311").mkdir()
        (browsers / "webkit-2311/INSTALLATION_COMPLETE").touch()
        reinstalled = identity()
        self.assertNotEqual(reinstalled, before)
        # Editing a browser in place preserves its revision/inode, but invalidates evidence.
        executable = browsers / "webkit-2311/browser"
        executable.write_bytes(b"original")
        installed = identity()
        stamp = executable.stat()
        executable.write_bytes(b"modified")
        os.utime(executable, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        self.assertEqual(executable.stat().st_ino, stamp.st_ino)
        self.assertNotEqual(identity(), installed)
        signing = browsers / "webkit-2311/_CodeSignature"
        signing.mkdir()
        (signing / "CodeResources").write_text("signature")
        signed = identity()
        (signing / "CodeResources").write_text("resigned!")
        self.assertNotEqual(identity(), signed)
        # A toolchain update behind the same `rustc` shim is a different compiler.
        rustc.write_text('#!/bin/sh\necho "rustc 1.91.0 (fixture)"\n')
        self.assertNotEqual(identity()["installed"]["rust"], reinstalled["installed"]["rust"])
        wrapper = bin_dir / "sccache"
        wrapper.write_text('#!/bin/sh\nexec "$@"\n')
        wrapper.chmod(0o755)
        environment["RUSTC_WRAPPER"] = str(wrapper)
        wrapped = identity()["installed"]["rust"]
        wrapper.write_text('#!/bin/sh\n# a different compiler cache\nexec "$@"\n')
        self.assertNotEqual(identity()["installed"]["rust"], wrapped)

    def test_the_whole_checkout_identity_binds_the_playwright_package_the_harness_loads_from_outside(self):
        # Part 2 recheck, R11: harness.js loadPlaywright takes the Playwright package from
        # RICHOS_PLAYWRIGHT, from Node's own lookup (node_modules above the checkout, NODE_PATH)
        # or from the main checkout's install, so a worktree needs no install of its own. None
        # of those is in the checkout, and a pass saved against one package was reused after
        # only that package changed. Each is now bound by content, with the node_modules it
        # sits in (playwright loads playwright-core from beside itself).
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        git = lambda *a: subprocess.run(["git", "-C", str(self.root), "-c", "user.name=fixture",
                                         "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                                         "-c", "commit.gpgsign=false", *a],
                                        check=True, env=env, capture_output=True)
        git("init", "-q")
        tests = self.root / "richos/app/ui/tests"
        (tests / "lib").mkdir(parents=True)
        (tests / "lib/harness.js").write_text("// fixture harness\n")
        (self.root / ".gitignore").write_text("node_modules/\n")
        git("add", "-A")
        git("commit", "-q", "-m", "fixture")
        worktree = Path(self.tmp.name) / "worktree"
        git("worktree", "add", "-q", str(worktree))
        home = Path(self.tmp.name) / "home"
        home.mkdir()
        environment = {"PATH": "/usr/bin:/bin", "HOME": str(home), "PLAYWRIGHT_BROWSERS_PATH": "0"}
        identity = lambda: evidence.checkout_identity(worktree, ["node", "suite.js"], environment)

        def package(directory, healthy="true"):
            (directory / "playwright").mkdir(parents=True, exist_ok=True)
            (directory / "playwright/index.js").write_text("module.exports = require('playwright-core');\n")
            (directory / "playwright-core").mkdir(exist_ok=True)
            (directory / "playwright-core/index.js").write_text("module.exports = {healthy: %s};\n" % healthy)

        external = Path(self.tmp.name) / "external" / "node_modules"
        global_modules = Path(self.tmp.name) / "global-modules"
        places = [
            # The main checkout's install, which a worktree with none of its own loads.
            ("main checkout", tests / "node_modules", {}),
            # An explicit RICHOS_PLAYWRIGHT, installed with its dependency beside it.
            ("RICHOS_PLAYWRIGHT", external, {"RICHOS_PLAYWRIGHT": str(external / "playwright")}),
            ("NODE_PATH", global_modules, {"NODE_PATH": str(global_modules)}),
            # Node looks in every ancestor's node_modules, above the checkout too.
            ("above the checkout", Path(self.tmp.name) / "node_modules", {}),
        ]
        for name, directory, variables in places:
            with self.subTest(place=name):
                environment.update(variables)
                package(directory)
                before = identity()
                self.assertEqual(identity(), before)
                # Only playwright-core changes, at the same path; nothing git would land differs.
                package(directory, healthy="false")
                self.assertEqual(subprocess.run(["git", "-C", str(worktree), "status", "--porcelain"],
                                                env=env, capture_output=True, check=True).stdout, b"")
                self.assertNotEqual(identity(), before, name)
                package(directory)
                self.assertEqual(identity(), before, name)
        # An install that appears where the harness would find it first is a different input.
        before = identity()
        package(worktree.parent / "worktree-sibling-is-not-an-ancestor")
        self.assertEqual(identity(), before)
        package(home / ".node_modules")
        self.assertNotEqual(identity(), before)


class UiFamily(unittest.TestCase):
    """The UI suite family (proof_evidence.UI_FAMILY): a suite's identity is the same in every
    checkout holding the same bytes, and changes exactly when something it reads changes
    (verification reliability PRD 2026-10-20, acceptance A10, on isolated identity fixtures)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="proof-ui-family.",
                                               dir=os.path.realpath(tempfile.gettempdir()))
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.root = base / "repo"
        self.home = base / "home"
        self.browsers = base / "browsers"
        (self.browsers / "chromium-1").mkdir(parents=True)
        (self.browsers / "chromium-1/INSTALLATION_COMPLETE").write_text("")
        self.home.mkdir()
        files = {
            ".gitignore": "node_modules/\n.DS_Store\n",
            "richos/app/ui/index.html": '<script src="theme-boot.js"></script><script src="main.js"></script>\n',
            "richos/app/ui/theme-boot.js": "// startup\n",
            "richos/app/ui/main.js": "// shell\n",
            "richos/app/ui/style.css": "body{}\n",
            "richos/app/ui/unloaded.js": "// on disk, not in index.html\n",
            "richos/app/ui/assets/logo.svg": "<svg/>\n",
            "richos/app/x.rs": "fn main() {}\n",
            "richos/app/ui/tests/a.js": 'require("./lib/harness"); // reads shots-a\n',
            "richos/app/ui/tests/b.js": 'require("./lib/harness");\n',
            "richos/app/ui/tests/lib/harness.js": 'require("./ui-sources");\n',
            "richos/app/ui/tests/lib/ui-sources.js": (
                'module.exports = { manifest() { return { order: '
                '["index.html", "theme-boot.js", "main.js", "style.css"] }; } };\n'),
            "richos/app/ui/tests/package.json": "{}\n",
            "richos/app/ui/tests/shots-a/one.png": "picture a\n",
            "richos/app/ui/tests/shots-b/one.png": "picture b\n",
            "richos/app/scripts/proof-for.ui-inputs": "@shell richos/app/ui/index.html richos/app/ui/main.js\n"
                                                      "a.js @shell richos/app/x.rs\nb.js @shell\nc.js @allui\n",
            "richos/app/ui/tests/c.js": 'require("./lib/harness");\n',
            "qualification.md": "Fixture review.\n",
        }
        for rel, text in files.items():
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            (self.root / rel).write_text(text)
        self.family = {"suites": "richos/app/ui/tests", "shipped": "richos/app/ui",
            "manifest": "richos/app/ui/tests/lib/ui-sources.js",
            "declaration": "richos/app/scripts/proof-for.ui-inputs",
            "paths": ["richos/app/ui/assets", "richos/app/ui/tests/lib/harness.js",
                      "richos/app/ui/tests/lib/ui-sources.js", "richos/app/ui/tests/package.json"],
            "tools": ["node", "git"], "environment": ["HOME", "PATH", "LANG", "PLAYWRIGHT_BROWSERS_PATH"],
            "external": [], "qualification": "qualification.json", "qualification_unit": "ui family",
            "unqualified": {"c.js": "reads something no declaration binds"}}
        evidence.atomic(self.root / "richos/app/scripts/proof-inputs.json",
                        {"schema": 1, "checks": {}, "families": {"ui-suite": self.family}})
        self.write_qualification()
        self.git("init", "-q", "-b", "main")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.worktree = base / "author"
        self.git("worktree", "add", "-q", "--detach", str(self.worktree))
        # The main checkout has the one install; the worktree loads it through harness.js's last
        # candidate (playwright_package_candidates), exactly as a real worktree does.
        self.package(self.root / "richos/app/ui/tests/node_modules")
        node = shutil.which("node", path=os.environ.get("PATH", "") + os.pathsep + "/opt/homebrew/bin")
        self.environment = {"HOME": str(self.home), "LANG": "en_US.UTF-8",
                            "PATH": os.pathsep.join([str(Path(node).parent), "/usr/bin", "/bin"]),
                            "PLAYWRIGHT_BROWSERS_PATH": str(self.browsers)}

    def git(self, *args, cwd=None):
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=fixture",
            "-c", "user.email=fixture@example.invalid", *args], cwd=cwd or self.root, env=env,
            text=True, stderr=subprocess.PIPE).strip()

    def write_qualification(self, other_pin="0" * 64):
        harness = self.root / "richos/app/ui/tests/lib/harness.js"
        evidence.atomic(self.root / "qualification.json", {"schema": 1, "review": "qualification.md",
            "requires": {"paths": ["richos/engine"], "tools": ["bash"], "environment": [], "external": []},
            "isolation": evidence.PRIVATE_PROFILE,
            "units": {
                "ui family": {"family": "ui-suite", "review": "Reads what its family recipe binds.",
                    "sources": {"richos/app/ui/tests/lib/harness.js": evidence.file_digest(harness)},
                    "common": {"requires": {"paths": ["richos/app/ui/index.html",
                                                      "richos/app/ui/tests/lib/harness.js"],
                                            "tools": ["node"], "environment": ["HOME", "PATH"], "external": []}},
                    "requires": {"paths": [], "tools": ["git"], "environment": [], "external": []}},
                "another unit": {"review": "An unrelated engine reader.", "sources": {"x": other_pin},
                    "requires": {"paths": [], "tools": [], "environment": [], "external": []}}}})

    def package(self, directory, version="1"):
        for name in ("playwright", "playwright-core"):
            (directory / name).mkdir(parents=True, exist_ok=True)
            (directory / name / "package.json").write_text('{"version": "%s"}\n' % version)

    def identity(self, root, label="a.js", environment=None):
        recipe = evidence.contract_for(root, label)
        if recipe.get("family"):
            recipe = evidence.family_invocation(root, recipe, Path(root) / "richos/app/ui/tests", ["node", label])
        return evidence.recipe_identity(root, recipe, environment or self.environment)

    def edit(self, rel, text, commit=True):
        for root in (self.root, self.worktree):
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_text(text)

    def test_a_shared_library_is_read_once_for_every_suite_and_again_once_it_changes(self):
        """03-E, PRD line 139's no-execution budget: every UI suite's closure holds the same
        library files, and reading each again for every suite (twice for the closure, once for
        its loads) was most of the family identity's cost. Each file's requires and picture names
        are now read once per stat identity, as its loads are; a change is read at once, so a
        library that gains a require or names a picture changes the recipe of every suite that
        loads it. prepare_environment no longer derives a family recipe at all."""
        harness = self.root / "richos/app/ui/tests/lib/harness.js"
        reads = []
        original = Path.read_text

        def counting(path, *args, **kwargs):
            if Path(path) == harness:
                reads.append(path)
            return original(path, *args, **kwargs)

        with patch.object(Path, "read_text", autospec=True, side_effect=counting):
            first = evidence.contract_for(self.root, "a.js")
            evidence.contract_for(self.root, "b.js")
            evidence.prepare_environment(SimpleNamespace(label="a.js"), self.root, self.tmp.name, self.environment)
        self.assertLessEqual(len(reads), 2, "the library was read %d times for two suites" % len(reads))
        self.assertNotIn("richos/app/ui/tests/lib/extra.js", first["paths"])
        self.edit("richos/app/ui/tests/lib/extra.js", "// a new library\n")
        self.edit("richos/app/ui/tests/lib/harness.js",
                  'require("./ui-sources");\nrequire("./extra");\nconst bound = "shots-b/one.png";\n')
        again = evidence.contract_for(self.root, "a.js")
        self.assertIn("richos/app/ui/tests/lib/extra.js", again["paths"])
        self.assertEqual(again.get("exists"), ["richos/app/ui/tests/shots-b/one.png"])

    def test_one_pass_reads_the_browser_installs_once_for_both_kinds_of_identity(self):
        """03-E, PRD line 139's no-execution budget: a whole-checkout identity and a UI family
        identity each walked every browser install, so a pass holding both kinds walked them
        twice (three times with two environments: 10 of the preflight's 27 profiled seconds).
        One reading per pass now, shared by both; the next pass reads again, so a changed install
        still changes both identities."""
        walks = []
        real = evidence.playwright_installs

        def counting(environment):
            walks.append(environment.get("PLAYWRIGHT_BROWSERS_PATH"))
            return real(environment)

        recipe = evidence.family_invocation(self.root, evidence.contract_for(self.root, "a.js"),
                                            self.root / "richos/app/ui/tests", ["node", "a.js"])

        def one_pass():
            snapshot = evidence.InputSnapshot()
            ui = evidence.recipe_identity(self.root, recipe, self.environment, snapshot)
            whole = evidence.checkout_identity(self.root, ["node", "b.js"], self.environment, snapshot)
            return ui["installed"]["playwright"], whole["installed"]["playwright"]

        with patch.object(evidence, "playwright_installs", side_effect=counting):
            first = one_pass()
            self.assertEqual(len(walks), 1, walks)
            (self.browsers / "chromium-1/updated").write_text("a changed install\n")
            second = one_pass()
        self.assertEqual(len(walks), 2, walks)
        self.assertEqual(first[0], first[1])
        self.assertNotEqual(second[0], first[0])
        self.assertNotEqual(second[1], first[1])

    def test_the_same_bytes_in_another_worktree_are_the_same_identity(self):
        first, second = self.identity(self.root), self.identity(self.worktree)
        self.assertNotIn("fresh", first)
        self.assertEqual(first, second)
        # Ignored junk beside a bound file is not content git would land.
        (self.worktree / "richos/app/ui/assets/.DS_Store").write_text("finder\n")
        self.assertEqual(self.identity(self.worktree), first)

    def test_the_recipe_binds_the_manifest_the_row_the_closure_and_named_pictures(self):
        recipe = evidence.contract_for(self.root, "a.js")
        for rel in ("richos/app/ui/theme-boot.js", "richos/app/ui/style.css", "richos/app/x.rs",
                    "richos/app/ui/tests/a.js", "richos/app/ui/tests/lib/harness.js",
                    "richos/app/ui/tests/lib/ui-sources.js", "richos/app/ui/tests/shots-a",
                    "richos/app/ui/assets"):
            self.assertIn(rel, recipe["paths"])
        for rel in ("richos/app/ui/tests/b.js", "richos/app/ui/tests/shots-b", "richos/app/ui/unloaded.js"):
            self.assertNotIn(rel, recipe["paths"])
        self.assertEqual(recipe["invocation"], {"cwd": "richos/app/ui/tests", "argv": ["node", "a.js"]})

    def test_every_shipped_startup_source_change_invalidates_every_suite(self):
        before = {label: self.identity(self.worktree, label) for label in ("a.js", "b.js")}
        self.edit("richos/app/ui/theme-boot.js", "// startup, changed\n")
        for label in before:
            self.assertNotEqual(self.identity(self.worktree, label), before[label], label)

    def test_another_suites_file_row_or_pictures_are_not_an_input(self):
        before = self.identity(self.worktree)
        self.edit("richos/app/ui/tests/b.js", 'require("./lib/harness"); // changed\n')
        self.edit("richos/app/ui/tests/shots-b/one.png", "picture b, regenerated\n")
        self.edit("richos/app/scripts/proof-for.ui-inputs", "@shell richos/app/ui/index.html richos/app/ui/main.js\n"
                  "a.js @shell richos/app/x.rs\nb.js @shell richos/app/x.rs\nc.js @allui\n")
        self.assertEqual(self.identity(self.worktree), before)
        # Its own pictures, its own row and its own declared input are.
        self.edit("richos/app/ui/tests/shots-a/one.png", "picture a, regenerated\n")
        self.assertNotEqual(self.identity(self.worktree), before)

    def test_another_units_pin_renewal_does_not_invalidate_but_the_review_does(self):
        before = self.identity(self.root)
        self.write_qualification(other_pin="1" * 64)
        self.assertEqual(self.identity(self.root), before)
        (self.root / "qualification.md").write_text("Fixture review, revised.\n")
        self.assertNotEqual(self.identity(self.root), before)

    def test_installs_environment_and_browsers_are_inputs(self):
        before = self.identity(self.worktree)
        self.package(self.root / "richos/app/ui/tests/node_modules", version="2")
        self.assertNotEqual(self.identity(self.worktree), before)
        self.package(self.root / "richos/app/ui/tests/node_modules")
        self.assertEqual(self.identity(self.worktree), before)
        self.assertNotEqual(self.identity(self.worktree, environment={**self.environment, "LANG": "C"}), before)
        (self.browsers / "chromium-1/new-file").write_text("patched\n")
        self.assertNotEqual(self.identity(self.worktree), before)

    def test_unknown_unqualified_or_differently_invoked_suites_stay_unqualified(self):
        self.assertIn("not yet declared", evidence.contract_for(self.root, "missing.js")["fresh"])
        self.assertIn("reads something no declaration binds", evidence.contract_for(self.root, "c.js")["fresh"])
        recipe = evidence.contract_for(self.root, "a.js")
        for cwd, argv in ((self.root / "richos/app/ui/tests", ["node", "--inspect", "a.js"]),
                          (self.root / "richos/app/ui/tests", ["node", "b.js"]),
                          (self.root, ["node", "a.js"])):
            self.assertIn("qualifies only", evidence.family_invocation(self.root, recipe, cwd, argv)["fresh"])
        # A named recipe cannot borrow the family's floor to drop the shared one.
        named = {k: v for k, v in recipe.items() if k not in ("family", "invocation")}
        with self.assertRaisesRegex(ValueError, "belongs to the ui-suite family only"):
            evidence.qualify_recipe(self.root, named)

    def test_a_script_the_row_runs_as_a_child_binds_what_that_script_requires(self):
        """Review rv-20261020T044017Z-ebbb640b-fc3b, P2: a pass must not survive a change to a file
        a child process of the suite reads. A fixture suite named on the row requires a library the
        suite itself never requires (in the path.join form the real fixture suites use)."""
        child = "richos/app/ui/tests/fixtures/child-suite.js"
        only = "richos/app/ui/tests/lib/child-only.js"
        (self.root / child).parent.mkdir(parents=True, exist_ok=True)
        (self.root / child).write_text('const path = require("path");\n'
                                       'require(path.join(__dirname, "..", "lib", "child-only"));\n')
        (self.root / only).write_text("// read only by the child\n")
        (self.root / "richos/app/ui/tests/d.js").write_text('require("./lib/harness"); // runs the child\n')
        declaration = self.root / "richos/app/scripts/proof-for.ui-inputs"
        declaration.write_text(declaration.read_text() + "d.js @shell " + child + "\n")
        recipe = evidence.contract_for(self.root, "d.js")
        self.assertIn(child, recipe["paths"])
        self.assertIn(only, recipe["paths"])
        before = self.identity(self.root, "d.js")
        (self.root / only).write_text("// read only by the child, changed\n")
        self.assertNotEqual(self.identity(self.root, "d.js"), before)
        # A suite that names no such child is not bound to that library.
        self.assertNotIn(only, evidence.contract_for(self.root, "b.js")["paths"])

    def test_a_load_the_family_cannot_follow_leaves_the_suite_unqualified(self):
        """Review rv-20261020T054118Z-26a24a78-1851: thread-switch.js runs contrast.py, whose
        import of qa/lib/qaimg.py the identity did not bind. A suite is qualified only when every
        load in its closure is followed or declared; anything else names the file and line."""
        declaration = self.root / "richos/app/scripts/proof-for.ui-inputs"
        cases = {
            "py.js": 'require("./lib/harness");\nconst cp = require("child_process");\n'
                     'cp.spawnSync("python3", ["helper.py"]);\n',
            "cargo.js": 'require("./lib/harness");\nrequire("child_process").spawnSync("cargo", ["run"]);\n',
            "computed.js": 'require("./lib/harness");\nconst path = require("path");\nconst DIR = __dirname;\n'
                           'require(path.join(DIR, "x.js"));\n',
            "package.js": 'require("./lib/harness");\nrequire("left-pad");\n',
            "program.js": 'require("./lib/harness");\nconst { spawnSync } = require("child_process");\n'
                          'spawnSync(process.env.TOOL, []);\n',
            "esm.js": 'require("./lib/harness");\nimport("./lib/harness.js");\n',
        }
        for name, text in cases.items():
            (self.root / "richos/app/ui/tests" / name).write_text(text)
            declaration.write_text(declaration.read_text() + name + " @shell\n")
        for name in cases:
            with self.subTest(suite=name):
                reason = evidence.contract_for(self.root, name).get("fresh", "")
                self.assertIn("cannot follow everything %s loads" % name, reason)
                self.assertIn("richos/app/ui/tests/%s:" % name, reason)
        # A builtin, a node child, a bound tool and a declared call stay qualified.
        (self.root / "richos/app/ui/tests/fine.js").write_text(
            'require("./lib/harness");\nconst cp = require("node:child_process");\nrequire("fs");\n'
            'cp.spawnSync(process.execPath, ["x"]);\ncp.execFileSync("git", ["status"]);\nconst c = "y";\nrequire(c);\n')
        declaration.write_text(declaration.read_text() + "fine.js @shell\n")
        self.assertIn("cannot follow", evidence.contract_for(self.root, "fine.js").get("fresh", ""))
        family = {**self.family, "child_tools": ["git"],
                  "dynamic": {"richos/app/ui/tests/fine.js": {"require(c)": "fixture"}}}
        evidence.atomic(self.root / "richos/app/scripts/proof-inputs.json",
                        {"schema": 1, "checks": {}, "families": {"ui-suite": family}})
        self.assertEqual(evidence.contract_for(self.root, "fine.js").get("family"), "ui-suite")
        self.assertEqual(evidence.contract_for(self.root, "py.js").get("family"), None)

    def test_a_picture_a_library_checks_for_is_bound_by_existence_not_content(self):
        """Review rv-20261020T063426Z-03441b27-e3dd #1: lib/shot-stability.js proves at require
        time that every picture it declares exists, so deleting one fails every suite that loads
        it; the suite's identity bound neither the picture nor its folder."""
        (self.root / "richos/app/ui/tests/lib/bounds.js").write_text(
            '// an example in a comment: "shots-b/example.png"\n'
            'const DECLARED = [{ file: "shots-b/one.png" }];\n')
        self.edit("richos/app/ui/tests/a.js", 'require("./lib/harness"); require("./lib/bounds"); // reads shots-a\n')
        recipe = evidence.contract_for(self.root, "a.js")
        self.assertEqual(recipe.get("exists"), ["richos/app/ui/tests/shots-b/one.png"])
        before = self.identity(self.root)
        # Another suite's picture, regenerated: its bytes are no input of this suite.
        (self.root / "richos/app/ui/tests/shots-b/one.png").write_text("picture b, regenerated\n")
        self.assertEqual(self.identity(self.root), before)
        # Deleted: the library would refuse, so the identity differs.
        (self.root / "richos/app/ui/tests/shots-b/one.png").unlink()
        self.assertNotEqual(self.identity(self.root), before)
        # A suite whose closure has no such library binds nothing of shots-b.
        self.assertNotIn("exists", evidence.contract_for(self.root, "b.js"))

    def test_private_study_reads_are_bound_by_content_and_must_stay_in_their_folder(self):
        """PRD 2026-10-20: bind private reads such as splash's studies. A suite opens the study a
        shipped library entry names, under $RICHOS_HQ, when that repository exists."""
        hq = Path(self.tmp.name) / "hq"
        (self.root / "richos/app/ui/library.js").write_text(
            '{"source": "richos-hq studies/one/index.html"}\n')
        family = {**self.family, "private_reads": {"a.js": {
            "why": "fixture", "env": "RICHOS_HQ", "home": "ab/hq",
            "named_in": "richos/app/ui/library.js", "pattern": '"source"\\s*:\\s*"richos-hq (\\S+)"'}}}
        evidence.atomic(self.root / "richos/app/scripts/proof-inputs.json",
                        {"schema": 1, "checks": {}, "families": {"ui-suite": family}})
        self.assertEqual(evidence.contract_for(self.root, "a.js")["external_reads"]["paths"], ["studies/one"])
        environment = {**self.environment, "RICHOS_HQ": str(hq)}
        absent = self.identity(self.root, environment=environment)
        self.assertEqual(absent["external_reads"], {"root": {"absent": True}})
        (hq / "studies/one/fonts").mkdir(parents=True)
        (hq / "studies/one/index.html").write_text('<link href="fonts/fonts.css">\n')
        (hq / "studies/one/fonts/fonts.css").write_text('@font-face { src: url("Inter.woff2"); }\n')
        (hq / "studies/two").mkdir()
        (hq / "studies/two/index.html").write_text("an unnamed study\n")
        present = self.identity(self.root, environment=environment)
        self.assertNotEqual(present, absent)
        (hq / "studies/two/index.html").write_text("an unnamed study, changed\n")
        self.assertEqual(self.identity(self.root, environment=environment), present)
        (hq / "studies/one/fonts/fonts.css").write_text('@font-face { src: url("Inter-2.woff2"); }\n')
        changed = self.identity(self.root, environment=environment)
        self.assertNotEqual(changed, present)
        # A study that loads a file outside its folder reaches what nothing binds.
        (hq / "studies/one/index.html").write_text('<link href="../shared/engine.css">\n')
        self.assertIn("outside the study folder", self.identity(self.root, environment=environment)["fresh"])

    def test_a_changed_harness_needs_its_review_renewed(self):
        self.edit("richos/app/ui/tests/lib/harness.js", 'require("./ui-sources"); require("fs");\n')
        self.assertIn("changed unit reader", self.identity(self.worktree)["fresh"])


if __name__ == "__main__":
    unittest.main()

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
        # Every lib/ module proof-run.py imports: rerun_tree.py (write_rerun) and runtime_cache.py
        # (supply_runtime) included.
        for name in ("proof_evidence.py", "proof_slots.py", "test_results.py", "cargo_identity.py",
                     "rerun_tree.py", "runtime_cache.py"):
            shutil.copy2(HERE / "lib" / name, app / "lib" / name)
        shutil.copy2(HERE / "testvm/reserve.py", app / "testvm/reserve.py")
        shutil.copy2(HERE / "proof-run.py", app / "proof-run.py")
        (engine / "VERSION").write_text("1.0.0-test\n")
        return app, engine

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


if __name__ == "__main__":
    unittest.main()

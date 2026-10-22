#!/usr/bin/env python3
"""Validate every production proof recipe and its required reader/tool inputs."""
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(HERE / "lib"))
import proof_evidence as evidence


class ProductionQualification(unittest.TestCase):
    def test_production_recipes_reject_known_shared_and_fixture_tool_omissions(self):
        root = HERE.parents[2]
        checks = json.loads((HERE / "proof-inputs.json").read_text())["checks"]
        for label, recipe in checks.items():
            with self.subTest(label=label):
                evidence.qualify_recipe(root, recipe)
                if recipe.get('subset'):
                    for field, values in recipe['subset'].items():
                        for value in values:
                            broken = {**recipe, 'subset': {**recipe['subset'],
                                field: [p for p in values if p != value]}}
                            with self.assertRaisesRegex(ValueError, 'subset qualification omits ' + field):
                                evidence.qualify_recipe(root, broken)
                if recipe.get('isolation'):
                    with self.assertRaisesRegex(ValueError, 'requires isolation'):
                        evidence.qualify_recipe(root, {k: v for k, v in recipe.items() if k != 'isolation'})
                if recipe.get('qualification_unit'):
                    contract = json.loads((root / recipe['qualification']).read_text())['units'][recipe['qualification_unit']]
                    for tool in contract['requires']['tools']:
                        with self.assertRaisesRegex(ValueError, 'qualification omits tools'):
                            evidence.qualify_recipe(root, {**recipe, 'tools': [t for t in recipe['tools'] if t != tool]})
                    if contract.get('external_paths'):
                        with self.assertRaisesRegex(ValueError, 'omits external_paths'):
                            evidence.qualify_recipe(root, {**recipe, 'external_paths': []})
                    for kind in contract.get('git_inputs', {}):
                        broken = {**recipe, 'git_inputs': {**recipe['git_inputs'], kind: []}}
                        with self.assertRaisesRegex(ValueError, 'omits git_inputs ' + kind):
                            evidence.qualify_recipe(root, broken)
                    for field in ('paths', 'external', 'environment'):
                        for value in contract['requires'][field]:
                            with self.assertRaisesRegex(ValueError, 'qualification omits ' + field):
                                evidence.qualify_recipe(root, {**recipe, field: [v for v in recipe[field] if v != value]})
                for tool in ("seq", "tee", "rmdir", "ln", "basename"):
                    broken = {**recipe, "tools": [name for name in recipe["tools"] if name != tool]}
                    with self.assertRaisesRegex(ValueError, "qualification omits tools: " + tool):
                        evidence.recipe_identity(root, broken, {})

    def test_every_declared_ui_suite_resolves_through_the_one_family_entry(self):
        """Every row of proof-for.ui-inputs is a family recipe that qualifies, or is left
        unqualified with its reason; removing a floor input refuses it (PRD 2026-10-20, A10)."""
        root = HERE.parents[2]
        data = json.loads((HERE / "proof-inputs.json").read_text())
        family = data["families"][evidence.UI_FAMILY]
        rows, _aliases = evidence.ui_declaration(root / family["declaration"])
        self.assertGreater(len(rows), 50)
        self.assertFalse(set(family["unqualified"]) - set(rows), "an unqualified suite with no row")
        qualified = 0
        for label in rows:
            with self.subTest(label=label):
                recipe = evidence.contract_for(root, label)
                if label in family["unqualified"]:
                    self.assertIn("leaves %s unqualified" % label, recipe["fresh"])
                    continue
                if recipe.get("fresh"):
                    # Only for a load it cannot follow, named by file and line.
                    self.assertRegex(recipe["fresh"], r"cannot follow everything .* loads.*: richos/\S+:\d+ ")
                    continue
                qualified += 1
                self.assertEqual(recipe.get("family"), evidence.UI_FAMILY, recipe.get("fresh"))
                evidence.qualify_recipe(root, recipe)
                self.assertIn("richos/app/ui/tests/" + label, recipe["paths"])
                self.assertIn("richos/app/ui/theme-boot.js", recipe["paths"])
        self.assertGreater(qualified, 60, "the family qualifies almost nothing: a resolver regression?")
        contract = json.loads((root / family["qualification"]).read_text())["units"][family["qualification_unit"]]
        recipe = evidence.contract_for(root, "setup.js")
        for field in ("paths", "tools", "environment"):
            for value in contract["common"]["requires"][field] + contract["requires"][field]:
                with self.subTest(field=field, value=value):
                    broken = {**recipe, field: [v for v in recipe[field] if v != value]}
                    with self.assertRaisesRegex(ValueError, "qualification omits " + field):
                        evidence.qualify_recipe(root, broken)

    def test_a_suite_that_runs_the_coverage_reconciler_binds_every_suite_it_lists(self):
        """gate-honesty.js runs `run.js --coverage`, which lists every suite in ui/tests and reads
        each one's declared check count: a changed suite must change its identity (review
        rv-20261020T044017Z-ebbb640b-fc3b, P2), and a script it names binds what that requires."""
        root = HERE.parents[2]
        tests = root / "richos/app/ui/tests"
        for suite in sorted(tests.glob("*.js")):
            text = suite.read_text(errors="replace")
            if 'path.join(__dirname, "run.js")' not in text or suite.name == "run.js":
                continue
            with self.subTest(suite=suite.name):
                recipe = evidence.contract_for(root, suite.name)
                # Either bound to the folder the reconciler lists, or not qualified at all.
                self.assertTrue(recipe.get("fresh") or "richos/app/ui/tests" in recipe["paths"], recipe)
        rows, _aliases = evidence.ui_declaration(root / "richos/app/scripts/proof-for.ui-inputs")
        self.assertIn("richos/app/ui/tests/", rows["gate-honesty.js"])

    def test_a_suite_with_a_child_the_family_cannot_follow_is_not_qualified(self):
        """Review rv-20261020T054118Z-26a24a78-1851: thread-switch.js runs contrast.py, which
        imports qa/lib/qaimg.py (and optionally Pillow from the interpreter). The family follows no
        Python imports, so the suite must stay keyed by the whole checkout, where any change to
        qaimg.py is a changed identity."""
        root = HERE.parents[2]
        for label, call in (("thread-switch.js", 'spawnSync("python3"'), ("realbytes.js", 'spawnSync("cargo"')):
            with self.subTest(suite=label):
                reason = evidence.contract_for(root, label).get("fresh", "")
                self.assertIn("cannot follow everything", reason)
                self.assertIn(call, reason)

    def test_every_environment_name_a_ui_suite_reads_is_bound_or_operational(self):
        """The family binds the environment names its review lists; a suite that starts reading
        a new one must add it there (or say why it is operational) before reuse can skip it."""
        import re
        root = HERE.parents[2]
        family = json.loads((HERE / "proof-inputs.json").read_text())["families"][evidence.UI_FAMILY]
        tests = root / family["suites"]
        read = set()
        for path in [*tests.glob("*.js"), *tests.glob("lib/*.js"), *tests.glob("fixtures/*.js")]:
            read |= set(re.findall(r"process\.env(?:\.|\[[\"'])([A-Z_][A-Z0-9_]*)", path.read_text(errors="replace")))
        self.assertGreater(len(read), 10)
        missing = sorted(read - set(family["environment"]) - set(family["operational"]))
        self.assertEqual(missing, [], "environment names UI suites read that the family neither binds "
                                      "nor declares operational")
        self.assertFalse(set(family["environment"]) & set(family["operational"]))

    def test_every_subset_identity_binds_the_guard_the_host_shield_loads(self):
        """The host shield runs every check and judges it with the guard's own analyzer, read out
        of guard-host-display-power.sh at run time. A subset recipe that left that file out kept
        the same input identity after a guard change that would refuse the suite now, so an old
        pass stayed reusable (review rv-20261011T164443Z-e08ee8a2-7cdb). Every subset identity
        must change when that file, and only it, changes; the shared subset floor names it."""
        import importlib.util
        import tempfile
        root = HERE.parents[2].resolve()
        spec = importlib.util.spec_from_file_location(
            "host_shield_for_qualification", root / "richos/engine/scripts/lib/host_shield.py")
        shield = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(shield)
        guard = Path(shield.GUARD).resolve()

        class GuardOnly:
            """Every input reads the same except the guard's bytes."""
            def __init__(self, revision):
                self.revision = revision
            def path(self, path):
                path = Path(path)
                if path == guard or path in guard.parents:
                    return {"guard": self.revision}
                return {"unchanged": str(path)}
            def inventory(self, path):
                return {"unchanged_names": str(path)}
            def runtime(self, executable):
                return {"unchanged_runtime": str(executable)}
            def ignored(self, root, rels):
                return {"unchanged_ignored": list(rels)}

        checks = json.loads((HERE / "proof-inputs.json").read_text())["checks"]
        subsets = [label for label, recipe in checks.items() if recipe.get("subset")]
        self.assertTrue(subsets)
        with tempfile.TemporaryDirectory() as private:
            environment = {"PATH": "/usr/bin:/bin", "HOME": private + "/home",
                           "RICHOS_VERIFICATION_FIXTURE_ROOT": private,
                           "RICHOS_VERIFICATION_TOOL_PATH": "/usr/bin:/bin"}
            for label in subsets:
                with self.subTest(label=label):
                    before, after = (evidence.recipe_identity(root, checks[label], environment, GuardOnly(r))
                                     for r in ("allows", "refuses"))
                    self.assertNotIn("fresh", before)
                    self.assertNotEqual(before, after, "the identity ignores the guard the shield loads")
        # The shared floor names it, so a subset recipe added later cannot leave it out.
        contract = json.loads((root / "docs/development/verification-input-qualifications.json").read_text())
        self.assertIn(str(guard.relative_to(root)), contract["subset_requires"]["paths"])


if __name__ == "__main__":
    unittest.main()

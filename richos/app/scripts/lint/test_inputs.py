import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import driver
import rust


class Inputs(unittest.TestCase):
    def test_removed_lint_and_removed_inheritance_change_rule_identity(self):
        with tempfile.TemporaryDirectory(prefix='lint-manifest-') as tmp:
            root = Path(tmp)
            path = 'richos/app/Cargo.toml'
            manifest = root / path
            manifest.parent.mkdir(parents=True)
            original = '[package]\nname="fixture"\n[lints]\nworkspace=true\n[workspace.lints.rust]\nunused_must_use="warn"\n'
            manifest.write_text(original)
            with patch('rust.tracked', return_value=[path]):
                before = rust.lint_rules(root)
                manifest.write_text(original.replace('workspace=true', 'workspace=false'))
                self.assertNotEqual(before, rust.lint_rules(root))
                manifest.write_text(original.replace('unused_must_use="warn"', ''))
                self.assertNotEqual(before, rust.lint_rules(root))

    def test_vendored_third_party_manifest_is_not_a_lint_rule(self):
        with tempfile.TemporaryDirectory(prefix='lint-manifest-') as tmp:
            root = Path(tmp)
            path = 'richos/app/third-party/qdrant-edge/Cargo.toml'
            manifest = root / path
            manifest.parent.mkdir(parents=True)
            manifest.write_text('[package]\nname="qdrant-edge"\n')
            with patch('rust.tracked', return_value=[path]):
                self.assertEqual(rust.lint_rules(root), {"toolchain-default-diagnostics": "blocking-count-ceiling"})

    def test_skipped_suite_is_not_current_fast_proof(self):
        with tempfile.TemporaryDirectory(prefix='lint-receipt-') as tmp:
            path = Path(tmp) / 'receipt.json'
            for status, expected in [('passed', True), ('skipped', False), ('failed', False)]:
                path.write_text(json.dumps({'run_id': 'current', 'suites': [{'name': 'lint.test.sh', 'state': status}]}))
                self.assertEqual(driver.fast_was_run(path, 'current'), expected)
                self.assertFalse(driver.fast_was_run(path, 'different'))
                self.assertFalse(driver.fast_was_run(path, None))
            path.write_text(json.dumps({'suites': [{'name': 'lint.test.sh', 'state': 'passed'}]}))
            self.assertFalse(driver.fast_was_run(path, None))

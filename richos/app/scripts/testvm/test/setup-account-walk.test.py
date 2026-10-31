#!/usr/bin/env python3
"""setup-account-walk.py's verdicts, offline: the sheet names the Ultra plan and never Max, and
only claude.com/pricing counts as the link's page. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('setup_account_walk', HERE / 'setup-account-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

class SentenceVerdict(unittest.TestCase):
    def test_new_wording_passes(self):
        self.assertIsNone(walk.sentence_verdict(True, True, False))

    def test_old_wording_fails(self):
        self.assertIn('Ultra', walk.sentence_verdict(False, True, False))

    def test_missing_link_fails(self):
        self.assertIn('link', walk.sentence_verdict(True, False, False))

    def test_stray_max_fails(self):
        self.assertIn('Max', walk.sentence_verdict(True, True, True))


class PricingPage(unittest.TestCase):
    def test_pricing(self):
        for url in ('https://claude.com/pricing', 'https://www.claude.com/pricing/', 'https://claude.com/pricing?x=1'):
            self.assertTrue(walk.pricing_page(url), url)

    def test_other_pages(self):
        for url in ('', 'https://claude.com/', 'https://example.com/pricing', 'https://claude.com/pricing-x'):
            self.assertFalse(walk.pricing_page(url), url)


if __name__ == '__main__':
    unittest.main()

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

NEW = ("You need your own Anthropic account and an Ultra subscription there. You can sign in through\n"
       "your browser after setup; I never see your password. If you don't already have that subscription,\n"
       "sign up there first and pick the Ultra tier.")


class SentenceVerdict(unittest.TestCase):
    def test_new_wording_passes(self):
        self.assertIsNone(walk.sentence_verdict(NEW))

    def test_old_wording_fails(self):
        old = NEW.replace('an Ultra subscription', 'a Max subscription').replace('the Ultra tier', 'the Max/Max 20x tier')
        self.assertIn('Ultra', walk.sentence_verdict(old))

    def test_stray_max_fails(self):
        self.assertIn('Max', walk.sentence_verdict(NEW + ' Max 20x.'))


class PricingPage(unittest.TestCase):
    def test_pricing(self):
        for url in ('https://claude.com/pricing', 'https://www.claude.com/pricing/', 'https://claude.com/pricing?x=1'):
            self.assertTrue(walk.pricing_page(url), url)

    def test_other_pages(self):
        for url in ('', 'https://claude.com/', 'https://example.com/pricing', 'https://claude.com/pricing-x'):
            self.assertFalse(walk.pricing_page(url), url)


if __name__ == '__main__':
    unittest.main()

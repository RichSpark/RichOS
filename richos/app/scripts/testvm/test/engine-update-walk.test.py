#!/usr/bin/env python3
"""engine-update-walk.py's verdicts, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
APP = HERE.parent.parent
spec = importlib.util.spec_from_file_location('engine_update_walk', HERE / 'engine-update-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

DIGEST = '5ab0b125509f9b1c89f60727cb5d1170895f59061c6cf91c0ca28905716c2468'


class Stamp(unittest.TestCase):
    def test_the_digest_is_read_from_the_stamp_install_engine_writes(self):
        stamp = 'engine 1.2.0\nsha256 %s\nbytes 165499463\nfrom https://example.invalid/a.tar.gz\n' % DIGEST
        self.assertEqual(walk.stamp_digest(stamp), DIGEST)

    def test_an_upper_case_digest_is_the_same_digest(self):
        self.assertEqual(walk.stamp_digest('sha256 %s\n' % DIGEST.upper()), DIGEST)

    def test_no_stamp_or_a_short_digest_is_no_digest(self):
        self.assertEqual(walk.stamp_digest(''), '')
        self.assertEqual(walk.stamp_digest('sha256 %s\n' % DIGEST[:12]), '')


class TheAppsOwnWords(unittest.TestCase):
    """The walk waits on the app's operator lines; a reworded line would leave it waiting on
    nothing, so each one it reads must still be in the app's source."""

    def test_every_line_the_walk_waits_on_is_printed_by_the_app(self):
        source = (APP / 'src-tauri' / 'src' / 'main.rs').read_text()
        for line in (walk.STARTED, walk.DONE, walk.FAILED):
            self.assertIn(line, source, line)

    def test_the_sheet_title_is_the_sheets(self):
        self.assertIn(walk.SHEET_TITLE, (APP / 'ui' / 'index.html').read_text())
        self.assertIn(walk.SHEET_OCR, walk.SHEET_TITLE)


if __name__ == '__main__':
    unittest.main()

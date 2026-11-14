#!/usr/bin/env python3
"""home-standard-walk.py's fictional SwearMeter history and its argument refusals, offline. The live walk is its own proof."""
import datetime
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('home_standard_walk', HERE / 'home-standard-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

# 2026-11-14 12:00 UTC, and a January date whose earlier months cross a year
NOW = datetime.datetime(2026, 11, 14, 12, tzinfo=datetime.timezone.utc).timestamp()
JANUARY = datetime.datetime(2027, 1, 2, 9, tzinfo=datetime.timezone.utc).timestamp()


class History(unittest.TestCase):
    def test_each_month_has_its_own_count_and_the_total_is_their_sum(self):
        rows, exp = walk.history_rows(NOW)
        counts = [m['swear_words'] for m in exp['months']]
        self.assertEqual(len(counts), walk.MONTHS)
        self.assertEqual(len(set(counts)), walk.MONTHS)
        self.assertEqual(exp['all_time'], sum(counts))
        self.assertEqual(len(rows), sum(counts))
        self.assertEqual(exp['months'][0]['month'], 202611)
        self.assertEqual(exp['months'][-1]['month'], 202510)

    def test_no_entry_sits_near_the_30_day_edge_on_any_day_of_a_year(self):
        start = datetime.datetime(2026, 1, 1, 6, tzinfo=datetime.timezone.utc).timestamp()
        for day in range(365):
            now = start + day * 86400
            rows, exp = walk.history_rows(now)
            edge = (now - 30 * 86400) * 1000
            self.assertGreater(min(abs(r['timestamp'] - edge) for r in rows), 6 * 86400 * 1000, day)
            self.assertEqual(sum(1 for r in rows if r['timestamp'] >= edge), exp['last_30_days'])
            self.assertTrue(all(r['timestamp'] <= now * 1000 for r in rows))

    def test_every_entry_falls_in_the_month_it_is_counted_for(self):
        for now in (NOW, JANUARY):
            rows, _ = walk.history_rows(now)
            for r in rows:
                at = datetime.datetime.fromtimestamp(r['timestamp'] / 1000, datetime.timezone.utc)
                self.assertEqual('qa-%d%02d' % (at.year, at.month), r['sessionId'])

    def test_today_the_last_30_days_hold_this_month_only(self):
        _, exp = walk.history_rows(NOW)
        self.assertEqual(exp['last_30_days'], walk.THIS_MONTH)

    def test_every_entry_is_a_readable_submission_with_one_swear_word(self):
        rows, _ = walk.history_rows(NOW)
        for r in rows:
            self.assertTrue(r['sessionId'] and r['timestamp'] > 0)
            self.assertFalse(r['display'].startswith(('/', '!')))
            self.assertEqual(r['display'].count('shit'), 1)

    def test_a_january_walk_names_last_years_months(self):
        _, exp = walk.history_rows(JANUARY)
        self.assertEqual([m['month'] for m in exp['months'][:3]], [202701, 202612, 202611])


class Refusals(unittest.TestCase):
    def test_an_unknown_step_is_refused(self):
        r = subprocess.run([sys.executable, str(HERE / 'home-standard-walk.py'), 'vm', '--out', '/nonexistent', '--steps', 'nope'],
                           capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 2)
        self.assertIn('unknown step(s): nope', r.stderr)


if __name__ == '__main__':
    unittest.main()

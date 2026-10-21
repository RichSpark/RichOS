#!/usr/bin/env python3
"""land-checks-walk.py's verdict, offline, on readings shaped like the guest's. The live walk is its
own proof; this pins the rules that read its evidence."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('land_checks_walk', HERE / 'land-checks-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

BASE, REVIEWED, LAND = 'b' * 40, 'r' * 40, 'm' * 40


def reading(**change):
    worker = {'id': 'w' * 12, 'request': {'role': 'worker'}, 'status': 'integrated',
              'integration': {'verified': True, 'commit': REVIEWED}}
    out = dict(rows=[worker, {'id': 'v' * 12, 'request': {'role': 'reviewer'}}], record={'state': 'settled'},
               base=BASE, parents=[BASE, REVIEWED], works="'hello-world'", asked=2,
               seen={'kind': 'app', 'pid': 4242, 'process': 'python3 mcp.py'},
               state={'main': LAND, 'status': '', 'merge_head': '', 'lease_left': 'no', 'fence_refusals': '1'},
               refusals_before='1', records=[{'before': BASE, 'commit': LAND}], told=True)
    out.update(change)
    return out


class Verdict(unittest.TestCase):
    def outcomes(self, **change):
        r = reading(**change)
        lines = walk.judge(r.pop('rows'), r.pop('record'), **r)
        return {l['line'].split(' ', 1)[0]: l['outcome'] for l in lines}

    def test_the_whole_walk_passes(self):
        self.assertEqual(set(self.outcomes().values()), {'PASS'})

    def test_a_check_asked_once_fails_line_1(self):
        self.assertEqual(self.outcomes(asked=1)['1'], 'FAIL')

    def test_a_fast_forward_or_an_extra_commit_fails_line_2(self):
        self.assertEqual(self.outcomes(parents=[BASE])['2'], 'FAIL')
        self.assertEqual(self.outcomes(parents=['x' * 40, REVIEWED])['2'], 'FAIL')

    def test_a_land_without_the_lease_fails_line_4(self):
        self.assertEqual(self.outcomes(seen={'read': 'none'})['4'], 'FAIL')

    def test_a_lease_left_behind_fails_line_5(self):
        self.assertEqual(self.outcomes(state={**reading()['state'], 'lease_left': 'yes'})['5'], 'FAIL')

    def test_a_fence_refusal_of_the_app_land_fails_line_6(self):
        self.assertEqual(self.outcomes(state={**reading()['state'], 'fence_refusals': '2'})['6'], 'FAIL')

    def test_two_land_records_fail_line_7(self):
        self.assertEqual(self.outcomes(records=[{'commit': 'x' * 40}, {'commit': LAND}])['7'], 'FAIL')

    def test_a_refusal_the_user_never_heard_fails_line_9(self):
        self.assertEqual(self.outcomes(told=False)['9'], 'FAIL')

    def test_an_assignment_left_open_fails_line_10(self):
        self.assertEqual(self.outcomes(record={'state': 'running'})['10'], 'FAIL')


class Job(unittest.TestCase):
    def test_the_walk_sends_its_own_refuse_once_job(self):
        """Mid-job review rv-20261021T072931Z-a4453faa-ffff, finding 2: the inherited job() sent
        review-walk.py's request, so the refusal-and-retry was never asked for."""
        sent = []
        fake = type('Fake', (), {'send': lambda self, text: sent.append(text) or [1.0, 2.0], 'facts': {},
                                 'save': lambda self: None})()
        walk.LandChecksWalk.job(fake)
        self.assertEqual(sent, [walk.JOB])
        self.assertNotEqual(walk.JOB, walk.review_walk.JOB)
        self.assertIn('refuses the first land', sent[0])


if __name__ == '__main__':
    unittest.main()

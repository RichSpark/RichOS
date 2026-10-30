#!/usr/bin/env python3
"""review-walk.py's verdict (slice 4's pass list), offline, on records shaped like the app's: worker
receipts (app.py) and the review watcher's ledger rows with their prompts (second_review.py).
The live walk is its own proof; this pins the rules that read its evidence."""
import importlib.util
from pathlib import Path
import threading
import time
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('review_walk', HERE / 'review-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

# What second_review.py writes: the user's turn first, under its own heading, then the author's claims.
PROMPT = ('# Review\n\n## The original words\n\nThese are the request the work answers, verbatim.\n\n'
          '<<<VERBATIM-AB12CD34 the brief exactly as builder received it\n' + walk.JOB +
          '\nVERBATIM-AB12CD34>>>\n\n## What the author claims\n\nit works\n')
FIRST, SECOND = 'a1' * 20, 'c3' * 20     # the two commits builder made


def worker(rid, commit=None, continues=None, continued_commit=None, integrated=False):
    r = {'id': rid * 12, 'name': 'builder-sonnet-' + rid, 'request': {'role': 'worker', 'teammate': 'builder'},
         'status': 'integrated' if integrated else 'run-ended', '_brief': '# duty\n\nthe brief'}
    if continues:
        r['continuation'] = {'worker_id': continues * 12, 'commit': continued_commit}
    if integrated:
        r['integration'] = {'verified': True, 'commit': commit, 'automatic_reviews': {'/acme': 'rv-recheck'}}
    return r


def review(rid, tip, verdict, prompt=PROMPT, trigger='handover'):
    return {'id': rid, 'tip': tip, 'verdict': verdict, 'trigger': trigger, '_prompt': prompt}


def receipts(**change):
    out = [worker('a', FIRST), worker('c', SECOND, continues='a', continued_commit=FIRST, integrated=True)]
    return change.get('replace', out)


def ledger(**change):
    out = [review('rv-first', FIRST, 'changes-requested'), review('rv-recheck', SECOND, 'passed')]
    for i, r in change.get('replace', {}).items():
        out[i] = r
    return out


class Verdict(unittest.TestCase):
    def outcomes(self, rows=None, reviews=None, record=None, landed="'hello-world'"):
        v = walk.judge(receipts() if rows is None else rows, record or {'state': 'settled'}, landed, [],
                       ledger() if reviews is None else reviews)
        return {l['line'][0]: l['outcome'] for l in v['lines']}

    def test_the_whole_loop_passes(self):
        self.assertEqual(set(self.outcomes().values()), {'PASS'})

    def test_the_watchers_reviews_are_read_from_its_ledger_not_from_reviewer_receipts(self):
        """The app reviews a finished worker by itself and `integrate` takes the worker alone: no
        reviewer receipt exists, and the integration names the review it consumed."""
        got = self.outcomes()
        self.assertEqual(got['1'], 'PASS')
        self.assertEqual(got['4'], 'PASS')

    def test_a_reviewer_prompt_without_the_users_words_fails_line_1(self):
        bad = review('rv-first', FIRST, 'changes-requested', prompt='# Review\n\n## What the author claims\n\n' + walk.JOB)
        self.assertEqual(self.outcomes(reviews=ledger(replace={0: bad}))['1'], 'FAIL')

    def test_the_users_words_must_come_before_what_the_author_claims(self):
        late = '# Review\n\n## The original words\n\nnothing\n\n## What the author claims\n\n' + walk.JOB
        self.assertEqual(self.outcomes(reviews=ledger(replace={0: review('rv-first', FIRST, 'changes-requested',
                                                                        prompt=late)}))['1'], 'FAIL')

    def test_no_review_at_all_fails_line_1_and_2(self):
        got = self.outcomes(reviews=[])
        self.assertEqual((got['1'], got['2']), ('FAIL', 'FAIL'))

    def test_a_prepared_reviewers_brief_is_still_read(self):
        brief = walk.USER_WORDS_LABEL + '\n\n<<<USER-WORDS-ABCD1234\n' + walk.JOB + '\nUSER-WORDS-ABCD1234>>>\n\n# duty'
        rows = receipts() + [
            {'id': 'b' * 12, 'request': {'role': 'reviewer'}, '_brief': brief, 'review_target': {'commit': FIRST},
             'review_observation': {'valid': True, 'report': {'verdict': 'changes-requested'}}},
            {'id': 'd' * 12, 'request': {'role': 'reviewer'}, '_brief': brief, 'review_target': {'commit': SECOND},
             'review_observation': {'valid': True, 'report': {'verdict': 'passed'}}}]
        for r in rows[1:2]:
            r['integration'] = {'verified': True, 'commit': SECOND, 'reviewer_id': 'd' * 12}
        self.assertEqual(set(self.outcomes(rows=rows, reviews=[]).values()), {'PASS'})

    def test_a_first_review_that_passed_the_planted_defect_fails_line_2(self):
        self.assertEqual(self.outcomes(reviews=ledger(replace={0: review('rv-first', FIRST, 'passed')}))['2'], 'FAIL')

    def test_a_coordinator_who_fixed_it_before_any_review_fails_line_2(self):
        """Run 4 (walk-c9eeb4b4e680): the coordinator ran the work itself, continued it before any
        review, and the one review passed. The loop under test never ran."""
        self.assertEqual(self.outcomes(reviews=[review('rv-recheck', SECOND, 'passed')])['2'], 'FAIL')

    def test_a_merge_before_the_review_asked_for_changes_fails_line_2(self):
        """The defective commit landed first and was reviewed afterwards: an app defect, never a pass."""
        rows = [worker('a', FIRST, integrated=True)] + receipts()[1:]
        self.assertEqual(self.outcomes(rows=rows)['2'], 'FAIL')

    def test_no_continuation_fails_lines_3_to_5(self):
        got = self.outcomes(rows=[worker('a', FIRST)], reviews=[review('rv-first', FIRST, 'changes-requested')])
        self.assertEqual((got['3'], got['4'], got['5']), ('FAIL', 'FAIL', 'FAIL'))

    def test_a_recheck_that_still_asks_for_changes_fails_line_4(self):
        self.assertEqual(self.outcomes(reviews=ledger(replace={1: review('rv-recheck', SECOND, 'changes-requested')}))['4'],
                         'FAIL')

    def test_a_landing_not_named_by_a_passing_review_fails_line_4(self):
        """integrate names the review it consumed; a pass of some other commit is not the recheck."""
        self.assertEqual(self.outcomes(reviews=ledger(replace={1: review('rv-other', 'e5' * 20, 'passed')}))['4'], 'FAIL')

    def test_the_named_review_must_be_of_the_landed_commit(self):
        """integrate named the review, but that review passed some other commit than the one landed."""
        self.assertEqual(self.outcomes(reviews=ledger(replace={1: review('rv-recheck', 'e5' * 20, 'passed')}))['4'],
                         'FAIL')

    def test_a_recheck_that_started_before_the_rejection_finished_fails_line_4(self):
        first = dict(review('rv-first', FIRST, 'changes-requested'), at='2026-10-30T10:00:00Z',
                     finished_at='2026-10-30T10:05:00Z')
        late = dict(review('rv-recheck', SECOND, 'passed'), at='2026-10-30T10:01:00Z', finished_at='2026-10-30T10:02:00Z')
        self.assertEqual(self.outcomes(reviews=[first, late])['4'], 'FAIL')
        right = dict(late, at='2026-10-30T10:06:00Z', finished_at='2026-10-30T10:08:00Z')
        self.assertEqual(self.outcomes(reviews=[first, right])['4'], 'PASS')

    def test_a_landing_without_the_fix_fails_line_5(self):
        self.assertEqual(self.outcomes(landed="'-hello-world-'")['5'], 'FAIL')
        rows = receipts()
        rows[1]['integration']['verified'] = False
        self.assertEqual(self.outcomes(rows=rows)['5'], 'FAIL')

    def test_an_assignment_left_open_fails_line_6(self):
        self.assertEqual(self.outcomes(record={'state': 'running'})['6'], 'FAIL')


class Dispatching(unittest.TestCase):
    """A review walk once saw a worker sit "dispatching" 20-30 minutes on the unanswered name card."""

    def stuck(self, status):
        r = worker('a')
        r['status'] = status
        return [r]

    def test_five_minutes_dispatching_is_stuck_and_less_is_not(self):
        since = {}
        self.assertEqual(walk.stuck_dispatching(self.stuck('dispatching'), since, 100), [])
        self.assertEqual(walk.stuck_dispatching(self.stuck('dispatching'), since, 399), [])
        self.assertEqual(len(walk.stuck_dispatching(self.stuck('dispatching'), since, 400)), 1)

    def test_a_worker_that_moved_on_restarts_the_clock(self):
        since = {}
        walk.stuck_dispatching(self.stuck('dispatching'), since, 0)
        walk.stuck_dispatching(self.stuck('running'), since, 200)
        self.assertEqual(walk.stuck_dispatching(self.stuck('dispatching'), since, 500), [])

    def walker(self, card):
        w = object.__new__(walk.ReviewWalk)
        w.present = lambda title, role='AXButton', app=None: card
        return w

    def test_the_line_names_the_card_when_it_is_on_screen(self):
        w, since = self.walker(True), {}
        self.assertIsNone(w.stuck_dispatch(self.stuck('dispatching'), since, 0))
        line = w.stuck_dispatch(self.stuck('dispatching'), since, 301)
        self.assertIn('name card, which is on screen', line)
        self.assertIn('builder', line)

    def test_the_line_says_so_when_nothing_is_on_screen(self):
        w, since = self.walker(False), {}
        w.stuck_dispatch(self.stuck('dispatching'), since, 0)
        self.assertIn('not on screen', w.stuck_dispatch(self.stuck('dispatching'), since, 301))


class DeadlineOnItsOwnClock(unittest.TestCase):
    """Follow-up 9: the stop on a stuck "dispatching" must not depend on the UI probe loop coming round."""

    def test_a_hung_probe_still_stops_at_the_deadline(self):
        rows = [worker('a')]
        rows[0]['status'] = 'dispatching'
        describe = lambda r, since, now: ('stuck' if walk.stuck_dispatching(r, since, now, limit=0.3) else None)
        never = threading.Event()
        with self.assertRaises(walk.StuckDispatch):
            with walk.DispatchWatch(lambda: rows, describe, every=0.05):
                never.wait(60)  # the probe that never returns; only the watch can end it

    def test_a_worker_that_is_not_stuck_leaves_the_probe_alone(self):
        rows = [worker('a')]
        with walk.DispatchWatch(lambda: rows, lambda r, s, n: None, every=0.05):
            time.sleep(0.3)

    def test_a_failed_receipt_read_is_retried_not_fatal(self):
        def read():
            raise RuntimeError('guest busy')
        with walk.DispatchWatch(read, lambda r, s, n: 'x', every=0.05):
            time.sleep(0.3)

    def test_a_probe_that_never_answers_counts_as_no_card(self):
        w = object.__new__(walk.ReviewWalk)
        never = threading.Event()
        w.present = lambda title, role='AXButton', app=None: never.wait(60)
        self.assertFalse(w.card_on_screen(wait=0.2))
        never.set()


class AnswersTheNameCard(unittest.TestCase):
    def walker(self, on_screen):
        w = object.__new__(walk.ReviewWalk)
        w.facts, w.pressed = {}, []
        w.present = lambda title, role='AXButton', app=None: title in on_screen
        w.press = lambda title, role='AXButton', app=None, contains=True: w.pressed.append(title)
        w.clock = lambda: 99.0
        w.save = lambda: None
        return w

    def test_answered_once_by_the_shared_helper(self):
        w = self.walker({walk.command_walk.GIT_NAME_ANSWER})
        self.assertTrue(w.answer_git_name_card())
        self.assertFalse(w.answer_git_name_card())
        self.assertEqual(w.pressed, [walk.command_walk.GIT_NAME_ANSWER])

    def test_no_card_no_press(self):
        w = self.walker(set())
        self.assertFalse(w.answer_git_name_card())
        self.assertEqual(w.pressed, [])


if __name__ == '__main__':
    unittest.main()

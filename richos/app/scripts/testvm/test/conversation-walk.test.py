#!/usr/bin/env python3
"""conversation-walk.py's conversation file, turn matching and verdict, offline. The live walk is its own proof."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('conversation_walk', HERE / 'conversation-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


def plan(value):
    with tempfile.TemporaryDirectory() as d:
        f = Path(d) / 'c.json'
        f.write_text(value if isinstance(value, str) else json.dumps(value))
        bad = walk.plan_problems(f)
        return bad, (None if bad else walk.steps_of(f))


def row(event, turn, at, **more):
    return dict(event=event, turn_id=turn, at=at, **more)


ROWS = [row('PromptReceived', 't1', 100, text='Alex', source='user'),
        row('AssistantDelta', 't1', 150, text='Nice to '), row('AssistantDelta', 't1', 160, text='meet you.'),
        row('TurnCompleted', 't1', 200)]


class Plan(unittest.TestCase):
    def test_strings_and_objects_are_steps_in_order(self):
        bad, steps = plan([{'press': 'Start the questions', 'expect_reply': True}, 'Alex', {'say': 'Hi'}])
        self.assertEqual(bad, [])
        self.assertEqual([(s['kind'], s['text'], s['expect_reply']) for s in steps],
                         [('press', 'Start the questions', True), ('say', 'Alex', True), ('say', 'Hi', True)])

    def test_a_press_expects_no_reply_unless_it_says_so(self):
        self.assertFalse(plan([{'press': 'Not now'}])[1][0]['expect_reply'])

    def test_refused_before_a_guest_is_booted(self):
        for bad in ([], {}, 'nope', [{}], [{'say': 'a', 'press': 'b'}], [{'say': ''}], [3],
                    [{'press': 'x', 'expect_reply': 'yes'}], [{'say': 'x', 'expect_reply': True}]):
            self.assertTrue(plan(bad)[0], bad)


class Turns(unittest.TestCase):
    def test_a_message_is_found_by_its_words_after_the_send(self):
        self.assertEqual(walk.find_turn(ROWS, 'say', 'Alex', 50), 't1')
        self.assertIsNone(walk.find_turn(ROWS, 'say', 'Alex', 100))  # the same words sent earlier
        self.assertIsNone(walk.find_turn(ROWS, 'say', 'Bob', 50))

    def test_a_press_takes_the_first_turn_that_started_after_it(self):
        self.assertEqual(walk.find_turn(ROWS, 'press', 'Start the questions', 50), 't1')

    def test_the_record_carries_rich_s_whole_reply_and_the_closing_time(self):
        r = walk.turn_record(ROWS, 't1')
        self.assertEqual((r['state'], r['reply'], r['closed_at_ms']), ('completed', 'Nice to meet you.', 200))

    def test_an_unclosed_turn_is_open(self):
        self.assertEqual(walk.turn_record(ROWS[:3], 't1')['state'], 'open')


class Verdict(unittest.TestCase):
    say = {'kind': 'say', 'text': 'Alex', 'expect_reply': True}
    press = {'kind': 'press', 'text': 'Not now', 'expect_reply': False}

    def test_a_completed_reply_passes(self):
        self.assertIsNone(walk.verdict(self.say, dict(turn='t1', state='completed', reply='Hi', waited_s=5)))

    def test_a_message_with_no_turn_fails_but_a_press_with_none_passes(self):
        self.assertIn('no turn started', walk.verdict(self.say, dict(turn=None, waited_s=5)))
        self.assertIsNone(walk.verdict(self.press, dict(turn=None, waited_s=5)))

    def test_an_interrupted_or_empty_turn_fails(self):
        self.assertIn('interrupted', walk.verdict(self.say, dict(turn='t', state='interrupted', reply='x', waited_s=5)))
        self.assertIn('no text', walk.verdict(self.say, dict(turn='t', state='completed', reply=' ', waited_s=5)))


if __name__ == '__main__':
    unittest.main()

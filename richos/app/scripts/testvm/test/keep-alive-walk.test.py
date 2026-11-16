#!/usr/bin/env python3
"""keep-alive-walk.py's readers, offline: the meter's words, a transcript's last request and what
followed a keep-alive message. The live walk is its own proof."""
import importlib.util
import json
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('keep_alive_walk', HERE / 'keep-alive-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

KEEP_ALIVE = walk.TAG + 'The RichOS app sent this by itself to keep this chat ready.</richos-keep-alive>'


def user(text, **more):
    return dict(type='user', message={'role': 'user', 'content': text}, **more)


def assistant(text=None, tool=None, usage=None, **more):
    content = ([{'type': 'text', 'text': text}] if text else []) + ([{'type': 'tool_use', 'name': tool, 'input': {}}] if tool else [])
    message = {'role': 'assistant', 'content': content}
    if usage:
        message['usage'] = usage
    return dict(type='assistant', message=message, **more)


def tool_result():
    return dict(type='user', message={'role': 'user', 'content': [{'type': 'tool_result', 'content': 'done'}]})


class Meter(unittest.TestCase):
    def test_the_meters_own_words_are_read_as_its_figures(self):
        said = 'Context window. Rich, talking with you: 3%, 32k of 1000k. Rich, directing your team: 0%, 0k of 1000k.'
        self.assertEqual(walk.meter_words(said), {'rich': (3, 32, 1000), 'team': (0, 0, 1000)})
        unread = 'Context window. Rich, talking with you: 6%, 62k of 1000k. Rich, directing your team: not read yet.'
        self.assertEqual(walk.meter_words(unread), {'rich': (6, 62, 1000), 'team': None})
        self.assertIsNone(walk.meter_words('Output'))
        self.assertIsNone(walk.meter_words(''))

    def test_a_reading_is_worded_as_the_window_rounds_it(self):
        self.assertEqual(walk.words_for(32381, 1000000), (3, 32, 1000))
        self.assertEqual(walk.words_for(500000, 1000000), (50, 500, 1000))
        # a half rounds up, as the window's Math.round does (Python's round would say 0 and 2)
        self.assertEqual(walk.words_for(500, 200000), (0, 1, 200))
        self.assertEqual(walk.words_for(2500, 200000), (1, 3, 200))


class Transcript(unittest.TestCase):
    def test_the_last_request_is_the_leads_last_assistant_message_never_a_helpers(self):
        rows = [user('hello'),
                assistant('On it', usage={'input_tokens': 3, 'cache_read_input_tokens': 30248, 'cache_creation_input_tokens': 2042}),
                assistant('reading', usage={'input_tokens': 9, 'cache_creation_input_tokens': 900000}, isSidechain=True),
                assistant('Done', usage={'input_tokens': 3, 'cache_read_input_tokens': 32290, 'cache_creation_input_tokens': 88})]
        self.assertEqual(walk.last_request_size(rows), 32381)
        self.assertIsNone(walk.last_request_size([user('hello')]))

    def test_how_the_last_request_was_paid_for_and_the_share_read_from_the_cache(self):
        rows = [assistant('pong', usage={'input_tokens': 2, 'cache_read_input_tokens': 0, 'cache_creation_input_tokens': 62117}),
                assistant('ping', usage={'input_tokens': 3, 'cache_read_input_tokens': 62401, 'cache_creation_input_tokens': 290})]
        paid = walk.last_request_usage(rows)
        self.assertEqual(paid, {'read': 62401, 'written': 290, 'plain': 3})
        self.assertGreater(walk.read_share(paid), 0.99)
        self.assertEqual(walk.read_share(walk.last_request_usage(rows[:1])), 0.0)
        self.assertIsNone(walk.last_request_usage([user('hello')]))

    def test_rows_that_are_not_json_are_passed_over(self):
        self.assertEqual(walk.transcript_rows(json.dumps(user('a')) + '\nnot json\n'), [user('a')])


class KeepAlive(unittest.TestCase):
    def test_one_word_and_nothing_else_passes(self):
        rows = [user('Reply with the single word pong.'), assistant('pong'),
                user(KEEP_ALIVE), assistant('ok'),
                user('Reply with the single word ping.'), assistant('ping')]
        found = walk.keep_alives(rows)
        self.assertEqual(found, [{'replies': ['ok'], 'tool_calls': 0, 'other': 0}])
        self.assertEqual(walk.keep_alive_problems(found[0]), [])
        self.assertEqual(walk.keep_alive_problems({'replies': ['OK.'], 'tool_calls': 0, 'other': 0}), [])

    def test_a_tool_call_a_second_answer_or_other_words_each_fail_it(self):
        acted = walk.keep_alives([user(KEEP_ALIVE), assistant('Let me check.', tool='Bash'), tool_result(), assistant('ok')])[0]
        self.assertEqual(acted, {'replies': ['Let me check.', 'ok'], 'tool_calls': 1, 'other': 1})
        bad = walk.keep_alive_problems(acted)
        self.assertEqual(len(bad), 3, bad)
        self.assertTrue(walk.keep_alive_problems({'replies': ['Good morning! How can I help?'], 'tool_calls': 0, 'other': 0}))
        self.assertTrue(walk.keep_alive_problems({'replies': [], 'tool_calls': 0, 'other': 0}))

    def test_a_helpers_messages_and_other_chats_messages_are_not_its_turn(self):
        rows = [user(KEEP_ALIVE), assistant('working', isSidechain=True), assistant('ok'), user('next'), assistant('a reply to the next message')]
        self.assertEqual(walk.keep_alives(rows), [{'replies': ['ok'], 'tool_calls': 0, 'other': 0}])

    def test_no_keep_alive_is_none_found(self):
        self.assertEqual(walk.keep_alives([user('hello'), assistant('hi')]), [])


if __name__ == '__main__':
    unittest.main()

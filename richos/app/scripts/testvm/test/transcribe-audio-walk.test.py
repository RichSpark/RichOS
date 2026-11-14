#!/usr/bin/env python3
"""transcribe-audio-walk.py's verdicts, offline. The live walk is its own proof."""
import importlib.util
import json
from pathlib import Path
import re
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('transcribe_audio_walk', HERE / 'transcribe-audio-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


def call(event, skill):
    return {'event': event, 'agent_id': None, 'input': json.dumps({'skill': skill})}


class SkillCalls(unittest.TestCase):
    def test_only_a_completed_call_counts(self):
        calls = [call('PreToolUse', 'rich-skills:transcribe-audio'),
                 call('PostToolUseFailure', 'rich-skills:transcribe-audio')]
        self.assertEqual(walk.completed(calls, walk.NEW), [])
        calls.append(call('PostToolUse', 'rich-skills:transcribe-audio'))
        self.assertEqual(len(walk.completed(calls, walk.NEW)), 1)

    def test_the_old_name_is_found_in_any_state(self):
        calls = [call('PreToolUse', 'rich-skills:' + walk.OLD)]
        self.assertEqual(len(walk.named(calls, walk.OLD)), 1)
        self.assertEqual(walk.named([call('PostToolUse', 'rich-skills:transcribe-audio')], walk.OLD), [])

    def test_the_old_name_is_the_one_the_rename_retired(self):
        self.assertEqual(walk.OLD, 'audio-' + 'transcription')


class InitListing(unittest.TestCase):
    def frame(self, skills):
        return json.dumps({'payload': {'type': 'system', 'subtype': 'init', 'skills': skills}})

    def test_a_frame_with_both_media_skills(self):
        lines = [self.frame(['rich-skills:watch-video', 'rich-skills:transcribe-audio', 'rich-skills:american-english'])]
        self.assertEqual(walk.init_listing(lines), [sorted(walk.WANT)])

    def test_a_frame_with_the_terminal_copy_only_lists_nothing_wanted(self):
        lines = [self.frame(['richos-engine:transcribe-audio']), 'not json', json.dumps({'payload': {'type': 'user'}})]
        self.assertEqual(walk.init_listing(lines), [[]])

    def test_a_truncated_payload_is_searched_as_text(self):
        text = '{"type":"system","subtype":"init","skills":["rich-skills:transcribe-audio","rich-sk'
        self.assertEqual(walk.init_listing([json.dumps({'payload': text})]), [['rich-skills:transcribe-audio']])


class Steps(unittest.TestCase):
    def test_every_step_is_a_method(self):
        for step in walk.STEPS:
            self.assertTrue(hasattr(walk.TranscribeAudioWalk, step.replace('-', '_')), step)

    def test_the_memo_task_names_the_memo_and_its_words_are_the_keywords(self):
        self.assertIn(walk.MEMO, walk.MEMO_TASK)
        for word in walk.MEMO_KEYWORDS:
            self.assertRegex(walk.SPOKEN, re.escape(word))


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""hire-walk.py's verdicts, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('hire_walk', HERE.parent / 'hire-walk.py')
hire_walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hire_walk)

OPENING = ("At the beginning, I already had these teammates on my AI team: Clark (Senior Researcher), Dean "
           "(HR Director for our RichOS team; he hires new AI teammates when needed), Frank (Expert "
           "Advisor/Devil's Advocate; one of our most valuable teammates because he helps us uncover blind "
           "spots), Pierce (Brief Checker; he's vital in helping me avoid errors when briefing my AI "
           "teammates) and Reed (Source-Reading & Knowledge-Extraction Specialist). But based on what you "
           "told me here, I think we also need to add the following AI teammates to my team:\n\n")
CLOSING = "\n\nAnd we can expand/change this team any time as needed. Say go and I'll get them hired now."


class Verdicts(unittest.TestCase):
    def test_the_ceos_ending_with_a_list_passes_and_names_the_list(self):
        reply = OPENING + "- **Nora** (Marketing Lead)\n- Will (Marketing Director)\n- Bea (Bookkeeper)" + CLOSING
        names, failures = hire_walk.proposal_verdict(reply)
        self.assertEqual(failures, [])
        self.assertEqual(names, ['nora', 'will', 'bea'])

    def test_the_old_ending_that_hires_nobody_fails(self):
        _, failures = hire_walk.proposal_verdict("I've written down who you want. I can't hire anyone for you yet.")
        self.assertTrue(any('Say go' in f for f in failures), failures)
        self.assertTrue(any('no new teammate' in f for f in failures), failures)

    def test_a_proposal_saved_and_named_passes_and_any_gap_fails(self):
        self.assertEqual(hire_walk.hired_verdict(['nora', 'bea'], ['bea', 'nora'], 'Nora and Bea joined.'), [])
        self.assertTrue(hire_walk.hired_verdict(['nora', 'bea'], ['nora'], 'Nora joined.'))
        self.assertTrue(hire_walk.hired_verdict(['nora'], ['nora', 'bea'], 'Nora joined.'),
                        'a saved teammate he was not told about fails')
        self.assertTrue(hire_walk.hired_verdict([], [], 'Everyone joined.'), 'nobody saved fails')

    def test_the_turns_and_the_dispatched_teammates_are_read_off_the_records(self):
        rows = [{'event': 'PromptReceived', 'thread_id': 't', 'turn_id': 'a', 'text': 'hi', 'source': 'text'},
                {'event': 'PromptReceived', 'thread_id': 't', 'turn_id': 'x', 'text': 'prime', 'source': 'internal'},
                {'event': 'AssistantDelta', 'turn_id': 'a', 'text': 'Hel'},
                {'event': 'AssistantDelta', 'turn_id': 'a', 'text': 'lo'},
                {'event': 'TurnCompleted', 'turn_id': 'a'}]
        turns = hire_walk.turns_of(rows, 't')
        self.assertEqual([(t['prompt'], t['reply'], t['done']) for t in turns], [('hi', 'Hello', True)])
        calls = [{'hook_event_name': 'PreToolUse', 'tool_name': 'Agent',
                  'tool_input': {'subagent_type': 'richos-app-engine:nora'}}]
        self.assertEqual(hire_walk.dispatched_teammates(calls), ['nora'])


if __name__ == '__main__':
    unittest.main()

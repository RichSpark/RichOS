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

    def test_the_ending_walk_4_saw_passes(self):
        # Rich's own words at the end of walk 4 of 2026-10-13, on this branch's build.
        reply = OPENING + "- Will (Marketing Director)\n- Penny (Bookkeeper)" + CLOSING
        self.assertEqual(hire_walk.proposal_verdict(reply), (['will', 'penny'], []))

    def test_the_old_ending_that_hires_nobody_fails(self):
        _, failures = hire_walk.proposal_verdict("I've written down who you want. I can't hire anyone for you yet.")
        self.assertTrue(any('Say go' in f for f in failures), failures)
        self.assertTrue(any('no new teammate' in f for f in failures), failures)

    def test_a_proposal_missing_a_role_he_asked_for_fails(self):
        # Review rv-20261013T163700Z-d59c8398-4191 #2: he asked for a marketing lead and a
        # bookkeeper, and a proposal of Will alone passed.
        _, failures = hire_walk.proposal_verdict(OPENING + "- Will (Marketing Director)" + CLOSING)
        self.assertTrue(any('bookkeeper' in f for f in failures), failures)
        _, failures = hire_walk.proposal_verdict(OPENING + "- Penny (Bookkeeper)" + CLOSING)
        self.assertTrue(any('marketing lead' in f for f in failures), failures)

    def test_a_proposal_saved_and_named_passes_and_any_gap_fails(self):
        self.assertEqual(hire_walk.hired_verdict(['nora', 'bea'], ['bea', 'nora'], 'Nora and Bea joined.'), [])
        self.assertTrue(hire_walk.hired_verdict(['nora', 'bea'], ['nora'], 'Nora joined.'))
        self.assertTrue(hire_walk.hired_verdict(['nora'], ['nora', 'bea'], 'Nora joined.'),
                        'a saved teammate he was not told about fails')
        self.assertTrue(hire_walk.hired_verdict([], [], 'Everyone joined.'), 'nobody saved fails')

    def test_a_hire_he_is_told_about_that_was_not_saved_fails_in_every_spelling(self):
        # Reviews rv-20261013T163700Z-d59c8398-4191 #2 and rv-20261013T173718Z-c59b9c38-d018:
        # saved=['will'] and he is told Bea joined, as "Bea (Bookkeeper)", "**Bea** (Bookkeeper)"
        # or "Will and Bea joined the team."
        for told in ['Will (Marketing Director) and Bea (Bookkeeper) joined the team.',
                     '**Will** (Marketing Director) and **Bea** (Bookkeeper) joined the team.',
                     'Will and Bea joined the team.',
                     'Will, Nora and Bea are now on your team.',
                     '- Will (Marketing Director)\n- Bea (Bookkeeper)']:
            failures = hire_walk.hired_verdict(['will'], ['will'], told)
            self.assertTrue(any('Bea' in f for f in failures), (told, failures))

    def test_an_honest_report_of_what_was_saved_passes(self):
        for told in ['Will (Marketing Director) and Penny (Bookkeeper) joined the team.',
                     '**Will** (Marketing Director) and **Penny** (Bookkeeper) are now on your team. '
                     'Frank (Expert Advisor) and the rest were already here.',
                     'Done. Will and Penny joined the team, and Acme (your company) is all set up for them.']:
            self.assertEqual(hire_walk.hired_verdict(['will', 'penny'], ['will', 'penny'], told), [], told)

    def test_the_report_walk_9_gave_passes_and_its_words_are_not_teammates(self):
        # From what he was told in walk 9 of 2026-10-13 (merged tree 7a4f05998), where "Neither"
        # was once read as a teammate and "Acme (a leadership coaching firm)" could have been.
        told = ("Pierce flagged that earlier conversation already described the business (Acme, a leadership "
                "coaching firm), so I'm revising the research brief.\n\nI've started on the hires, but neither "
                "Will nor Bea is on the team yet.\n\n- **Bea (Bookkeeper):** She isn't one of the ready-made "
                "teammates.\n- **Will (Marketing Director):** He's already one of the ready-made teammates.\n\n"
                "Neither is on the team yet.\n\nWill (Marketing Director) and Bea (Bookkeeper) joined the team.")
        self.assertEqual(hire_walk.claimed_names(told), ['bea', 'will'])
        self.assertEqual(hire_walk.hired_verdict(['will', 'bea'], ['bea', 'will'], told), [])
        self.assertTrue(any('Bea' in f for f in hire_walk.hired_verdict(['will'], ['will'], told)),
                        'the same report with Bea unsaved still fails')

    def test_delegation_needs_this_tasks_own_receipt_with_an_answer_and_a_settled_task(self):
        # #3 in both reviews: an interrupted task told "No work was started"; an Agent call's
        # async_launched acknowledgment taken for an answer; another session's run counted.
        settled = {'state': 'settled', 'obligation_id': 'work-1', 'notices': [{'text': 'Penny suggests: one, two, three.'}]}
        answered = {'request': {'teammate': 'penny', 'role': 'consult', 'obligation_id': 'work-1'},
                    'consult_answer': {'answered': True, 'source': 'SubagentStop', 'message': '1. Lead through change'}}
        self.assertEqual(hire_walk.delegate_verdict(settled, [answered], 'penny'), [])
        launched_only = {'request': {'teammate': 'penny', 'role': 'consult', 'obligation_id': 'work-1'},
                         'status': 'async_launched', 'consult_answer': {}}
        self.assertTrue(any('handed back no answer' in f for f in
                            hire_walk.delegate_verdict(settled, [launched_only], 'penny')))
        elsewhere = dict(answered, request={'teammate': 'penny', 'role': 'consult', 'obligation_id': 'work-0'})
        self.assertTrue(any('no run of penny' in f for f in hire_walk.delegate_verdict(settled, [elsewhere], 'penny')))
        interrupted = {'state': 'interrupted', 'obligation_id': 'work-1', 'notices': [{'text': 'No work was started.'}]}
        failures = hire_walk.delegate_verdict(interrupted, [], 'penny')
        self.assertTrue(any('interrupted' in f for f in failures) and any('No work was started' in f for f in failures)
                        and any('no run of penny' in f for f in failures), failures)

    def test_the_turns_are_read_off_the_ledger(self):
        rows = [{'event': 'PromptReceived', 'thread_id': 't', 'turn_id': 'a', 'text': 'hi', 'source': 'text'},
                {'event': 'PromptReceived', 'thread_id': 't', 'turn_id': 'x', 'text': 'prime', 'source': 'internal'},
                {'event': 'AssistantDelta', 'turn_id': 'a', 'text': 'Hel'},
                {'event': 'AssistantDelta', 'turn_id': 'a', 'text': 'lo'},
                {'event': 'TurnCompleted', 'turn_id': 'a'}]
        turns = hire_walk.turns_of(rows, 't')
        self.assertEqual([(t['prompt'], t['reply'], t['done']) for t in turns], [('hi', 'Hello', True)])


if __name__ == '__main__':
    unittest.main()

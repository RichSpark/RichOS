#!/usr/bin/env python3
"""voice-walk.py's verdicts, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('voice_walk', HERE / 'voice-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class SpokenPrompt(unittest.TestCase):
    def test_an_internal_reprime_is_never_the_transcript(self):
        # The walk's first live run read this row as a transcript and passed.
        rows = [{'event': 'PromptReceived', 'source': 'internal', 'text': '[re-prime]'}]
        self.assertIsNone(walk.spoken_prompt(rows))

    def test_typed_and_empty_voice_prompts_are_not_it(self):
        rows = [{'event': 'PromptReceived', 'source': 'text', 'text': 'typed'},
                {'event': 'PromptReceived', 'source': 'jam', 'text': '  '}]
        self.assertIsNone(walk.spoken_prompt(rows))

    def test_the_first_voice_prompt_with_words_is_it(self):
        rows = [{'event': 'PromptReceived', 'source': 'internal', 'text': '[re-prime]'},
                {'event': 'PromptReceived', 'source': 'jam', 'text': 'Ship the new build tomorrow morning.'}]
        self.assertEqual(walk.spoken_prompt(rows)['text'], 'Ship the new build tomorrow morning.')


class OneTurn(unittest.TestCase):
    """2026-10-13, nightly 54: two stretches of one thought, said 1.406 s apart, were stored as two
    spoken turns. The --one-turn verdict is what refuses that."""

    def test_a_sentence_split_by_its_pause_is_refused(self):
        problem = walk.one_turn_problem(['Let me think about the onboarding', 'and the questions you asked.'], [])
        self.assertIn('2 spoken turns', problem)

    def test_one_turn_missing_the_second_half_is_refused(self):
        problem = walk.one_turn_problem(['Let me think about the onboarding'], ['onboarding', 'questions'])
        self.assertIn("['questions']", problem)

    def test_one_turn_with_both_halves_passes(self):
        self.assertIsNone(walk.one_turn_problem(
            ['Let me think about the onboarding and the questions you asked.'], ['Onboarding', 'questions']))

    def test_no_turn_at_all_is_refused(self):
        self.assertIn('0 spoken turns', walk.one_turn_problem([], []))

    def test_every_voice_prompt_is_counted_and_nothing_else(self):
        rows = [{'event': 'PromptReceived', 'source': 'internal', 'text': '[re-prime]'},
                {'event': 'PromptReceived', 'source': 'jam', 'text': 'Let me think about the onboarding'},
                {'event': 'PromptReceived', 'source': 'text', 'text': 'typed'},
                {'event': 'PromptReceived', 'source': 'jam', 'text': 'and the questions you asked.'}]
        self.assertEqual([r['text'] for r in walk.spoken_prompts(rows)],
                         ['Let me think about the onboarding', 'and the questions you asked.'])


class TalkTiming(unittest.TestCase):
    """The voice session's own lines (controller.rs), as the walk of test55 recorded them."""
    SAMPLE = [
        '[richos-voice] 22:51:47.695Z utterance START tainted=false heard-rich=false (Rich audible=false)',
        '[richos-voice] 22:51:50.612Z utterance END ADMITTED — 2.944 s, heard-rich=false, on to the recognizer (Rich audible=false)',
        '[richos-voice] 22:51:56.282Z utterance START tainted=false heard-rich=false (Rich audible=false)',
        '[richos-voice] 22:51:59.561Z utterance END ADMITTED — 3.440 s, heard-rich=false, on to the recognizer (Rich audible=false)',
        '[richos-voice] 22:52:00.795Z end-of-turn quiet — 2.000 s of silence since his last word; 0 fragment(s) held, 3 at the recognizer',
        '[richos-voice] tts: 2.10 s of speech synthesized in 640 ms (rtf 0.305) by Pocket TTS english_2026-04 INT8 · Peter Yearsley · 4 threads',
        '[richos-voice] 22:52:09.000Z ANSWER AUDIBLE — half-duplex window OPEN (hold after the last sample 120 ms = out 40.0 + in 40.0 + slicer 40.0)',
    ]

    def test_the_gaps_are_read_off_the_apps_own_clock(self):
        t = walk.talk_timing(self.SAMPLE)
        self.assertEqual(t['turn_end_to_audible_ms'], 8205)
        self.assertEqual(t['last_word_to_audible_ms'], 9439)
        self.assertFalse(t['spoke_before_done'])

    def test_rich_audible_while_he_speaks_is_caught(self):
        lines = self.SAMPLE[:2] + [self.SAMPLE[6]] + self.SAMPLE[2:4]
        self.assertTrue(walk.talk_timing(lines)['spoke_before_done'])
        marked = [self.SAMPLE[0].replace('Rich audible=false', 'Rich audible=true')]
        self.assertTrue(walk.talk_timing(marked)['spoke_before_done'])

    def _spoken(self, lines):
        import tempfile
        from types import SimpleNamespace
        from unittest import mock
        v = walk.VoiceWalk.__new__(walk.VoiceWalk)
        v.vm, v.log = 'vm', '/log'
        v.a = SimpleNamespace(within=5)
        text = '\n'.join(lines)
        with tempfile.TemporaryDirectory() as out, mock.patch.object(walk, 'guest', return_value=text), \
                mock.patch.object(walk.time, 'sleep'):
            v.out = Path(out)
            return v.spoken()

    def test_spoken_passes_only_when_rich_waited_and_was_heard(self):
        self.assertEqual(self._spoken(self.SAMPLE)['timing']['turn_end_to_audible_ms'], 8205)
        early = self.SAMPLE[:2] + [self.SAMPLE[6]] + self.SAMPLE[2:4] + [self.SAMPLE[5]]
        with self.assertRaisesRegex(walk.StepFailed, 'before his turn was over'):
            self._spoken(early)
        with self.assertRaisesRegex(walk.StepFailed, 'never logged ANSWER AUDIBLE'):
            self._spoken(self.SAMPLE[:6])

    def test_no_answer_leaves_the_gaps_unknown(self):
        t = walk.talk_timing(self.SAMPLE[:5])
        self.assertIsNone(t['audible_ms'])
        self.assertIsNone(t['turn_end_to_audible_ms'])


class DecoderIdentity(unittest.TestCase):
    def test_the_readiness_line_names_the_decoder_by_sha(self):
        line = ('[richos] voice: ready on this machine (ready) — whisper.cpp 1.9.1 bin:fe7b744a4b31 '
                'model:small.en@c6138d6d58ec | model:small.en (env-override)')
        self.assertEqual(walk.decoder_sha12(line), 'fe7b744a4b31')
        self.assertIsNone(walk.decoder_sha12('[richos] voice: ready on this machine (ready) — no sha'))


class TalkPressesTheTalkControl(unittest.TestCase):
    """The 2026-10-07 run (walk-1fede03eadc0) pressed "Talk to Rich" by title, which on a
    relaunched app is the home screen's door, and nothing started the microphone."""

    def walk_with(self, on_screen):
        w = object.__new__(walk.VoiceWalk)
        w.clicks = []
        presses = {'talk-toggle': 0}

        def ax(mode, *args, app=None, timeout=40):
            dom_id = args[args.index('--id') + 1]
            if mode == 'click':
                w.clicks.append(dom_id)
                if dom_id == 'home-enter':
                    on_screen.discard('home-enter')
                    on_screen.add('talk-toggle')
                else:
                    presses[dom_id] += 1
                return [{'meta': True}, {'clicked': True}]
            if dom_id not in on_screen:
                raise walk.StepFailed('command failed (1): ax.sh find\n{"error":"notfound","detail":"nothing matched"}')
            desc = 'Stop talking' if dom_id == 'talk-toggle' and presses['talk-toggle'] else 'Talk to Rich'
            return [{'meta': True}, {'role': 'AXButton', 'desc': desc}]

        w.ax = ax
        w.shot = lambda name: None
        return w

    def test_the_home_door_is_passed_then_the_talk_control_is_pressed_by_id(self):
        w = self.walk_with({'home-enter'})
        got = w.talk()
        self.assertEqual(w.clicks, ['home-enter', 'talk-toggle'])
        self.assertTrue(got['home_door_pressed'])
        self.assertEqual(got['talk_control']['desc'], 'Stop talking')

    def test_a_door_press_that_hit_the_deadline_is_read_not_repeated(self):
        # walk-977f9c8d80b5: the door's click came back exit 124 although the press had landed.
        w = self.walk_with({'home-enter'})
        inner = w.ax

        def ax(mode, *args, app=None, timeout=40):
            out = inner(mode, *args, app=app, timeout=timeout)
            if mode == 'click' and 'home-enter' in args:
                raise walk.StepFailed('command failed (124): ax.sh click\n{"error": "guest_deadline"}')
            return out

        w.ax = ax
        got = w.talk()
        self.assertEqual(w.clicks, ['home-enter', 'talk-toggle'])
        self.assertEqual(got['talk_control']['desc'], 'Stop talking')

    def test_no_home_screen_goes_straight_to_the_talk_control(self):
        w = self.walk_with({'talk-toggle'})
        w.talk()
        self.assertEqual(w.clicks, ['talk-toggle'])


class WindowUpAsksASlowReadAgain(unittest.TestCase):
    """walk-3fa7da94531b failed its relaunch on one AX read that hit the guest deadline."""

    def test_a_guest_deadline_is_read_again_and_a_real_error_is_not(self):
        w = object.__new__(walk.VoiceWalk)
        answers = [walk.StepFailed('command failed (124): ax.sh find\n{"error": "guest_deadline"}'), True]

        def present(title, role='AXButton', app=None):
            answer = answers.pop(0)
            if isinstance(answer, Exception):
                raise answer
            return answer

        w.present = present
        w.shot = lambda name: None
        self.assertTrue(w.window_up(seconds=10))

        def broken(title, role='AXButton', app=None):
            raise walk.StepFailed('command failed (2): no app pid')

        w.present = broken
        with self.assertRaises(walk.StepFailed):
            w.window_up(seconds=10)

    def test_a_window_not_open_yet_is_read_again(self):
        # walk-a75c409405c1: relaunch() returns at the pid, and the first read saw windows=0.
        w = object.__new__(walk.VoiceWalk)
        answers = [walk.StepFailed('command failed (1): ax.sh find\n{"error":"nowindow","detail":"windows=0"}'), True]
        w.present = lambda title, role='AXButton', app=None: (lambda a: (_ for _ in ()).throw(a) if isinstance(a, Exception) else a)(answers.pop(0))
        w.shot = lambda name: None
        sleep, walk.time.sleep = walk.time.sleep, (lambda s: None)
        try:
            self.assertTrue(w.window_up(seconds=10))
        finally:
            walk.time.sleep = sleep


class WindowUpDeclinesTheSetupSheetOnce(unittest.TestCase):
    """Candidate 43, voice run 2: after the relaunch the setup sheet held the window."""

    def test_the_setup_sheet_is_declined_once_then_the_window_is_read(self):
        w = object.__new__(walk.VoiceWalk)
        w.facts, w.save, w.shot = {}, (lambda: None), (lambda name: None)
        screen = {'up': {'Set it up', 'Not now'}}
        pressed = []

        def press(title, role='AXButton', app=None, contains=True):
            pressed.append(title)
            screen['up'] = {'Talk to Rich'}

        w.present = lambda title, role='AXButton', app=None: title in screen['up']
        w.press = press
        sleep, walk.time.sleep = walk.time.sleep, (lambda s: None)
        try:
            self.assertTrue(w.window_up(seconds=10))
        finally:
            walk.time.sleep = sleep
        self.assertEqual(pressed, ['Not now'])
        self.assertTrue(w.facts['relaunch_setup_sheet_declined'])

    def test_with_no_home_screen_the_talk_toggle_is_the_window(self):
        """Voice run 4: the conversation's talk control is an AXCheckBox, never an AXButton."""
        w = object.__new__(walk.VoiceWalk)
        w.facts, w.save, w.shot = {}, (lambda: None), (lambda name: None)
        w.present = lambda title, role='AXButton', app=None: title == 'Talk to Rich' and role == 'AXCheckBox'
        w.press = lambda *a, **k: self.fail('nothing to press')
        self.assertTrue(w.window_up(seconds=5))


class AFindIsAskedAgainAClickIsNot(unittest.TestCase):
    """walk-ea458ffa8bca lost first-run to one find that hit the guest deadline."""

    DEADLINE = 'command failed (124): ax.sh find\n{"error": "guest_deadline"}'

    def test_a_find_that_hit_the_deadline_is_asked_again(self):
        from unittest import mock
        w = object.__new__(walk.VoiceWalk)
        answers = [walk.StepFailed(self.DEADLINE), [{'meta': True}, {'role': 'AXButton'}]]
        with mock.patch.object(walk.adopt_walk.Walk, 'ax', side_effect=answers) as base:
            self.assertEqual(w.ax('find', '--title', 'Start the questions')[1]['role'], 'AXButton')
        self.assertEqual(base.call_count, 2)

    def test_a_click_that_hit_the_deadline_is_never_pressed_again(self):
        from unittest import mock
        w = object.__new__(walk.VoiceWalk)
        with mock.patch.object(walk.adopt_walk.Walk, 'ax', side_effect=[walk.StepFailed(self.DEADLINE)]) as base:
            with self.assertRaises(walk.StepFailed):
                w.ax('click', '--id', 'talk-toggle')
        self.assertEqual(base.call_count, 1)


class ReplySplit(unittest.TestCase):
    """Where the seconds between his turn and Rich's first sample go (CEO 2026-10-14)."""
    DAY = 1791936000000  # 2026-10-14T00:00:00Z
    VOICE = [
        '[richos-voice] 04:22:07.178Z TURN to Rich — his turn is over: 3 fragment(s)',
        '[richos] 04:22:11.050Z reply timing: a visible turn sent to the model',
        '[richos-voice] 04:22:15.892Z ANSWER AUDIBLE — half-duplex window OPEN',
        '[richos-voice] 04:22:16.688Z playout chunk START — 3.739 s queued',
    ]

    def at(self, clock):
        h, m, s = clock.split(':')
        return self.DAY + ((int(h) * 60 + int(m)) * 60) * 1000 + round(float(s) * 1000)

    def test_every_moment_and_gap_is_read_from_its_own_source(self):
        ledger = [{'event': 'PromptReceived', 'source': 'internal', 'text': '[re-prime]', 'turn_id': 'p', 'at': 1},
                  {'event': 'PromptReceived', 'source': 'jam', 'text': 'Our company', 'turn_id': 't',
                   'at': self.at('04:22:07.179')},
                  {'event': 'TurnStarted', 'turn_id': 't', 'at': self.at('04:22:07.200')},
                  {'event': 'AssistantDelta', 'turn_id': 't', 'text': 'Hi', 'at': self.at('04:22:14.900')}]
        machinery = [{'turnId': 't', 'internal': True, 'at': self.at('04:22:08.500')},
                     {'turnId': 't', 'internal': True, 'at': self.at('04:22:11.000')},
                     {'turnId': 't', 'internal': False, 'at': self.at('04:22:11.100')},
                     {'turnId': 'p', 'internal': True, 'at': 2}]
        split = walk.reply_split(self.VOICE, ledger, machinery)
        self.assertEqual(split['turn_id'], 't')
        self.assertEqual(split['hidden_records'], 2)
        self.assertEqual(split['gaps_ms'], {
            'turn_handed_over->prompt_received': 1, 'prompt_received->turn_started': 21,
            'turn_started->hidden_first': 1300, 'hidden_first->hidden_last': 2500,
            'hidden_last->words_sent': 50, 'words_sent->model_first_frame': 50,
            'model_first_frame->first_text': 3800,
            'first_text->first_sentence': 992, 'first_sentence->speech_started': 796})
        self.assertEqual(split['total_ms'], 9510)

    def test_a_moment_the_sources_do_not_hold_is_none_never_guessed(self):
        ledger = [{'event': 'PromptReceived', 'source': 'jam', 'text': 'Our company', 'turn_id': 't',
                   'at': self.at('04:22:07.179')}]
        split = walk.reply_split(self.VOICE, ledger, [])
        self.assertIsNone(split['moments_ms_of_day']['hidden_first'])
        self.assertIsNone(split['gaps_ms']['turn_started->hidden_first'])
        self.assertEqual(split['total_ms'], 9510)


if __name__ == '__main__':
    unittest.main()

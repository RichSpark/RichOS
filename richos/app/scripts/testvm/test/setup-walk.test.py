#!/usr/bin/env python3
"""setup-walk.py's verdicts, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('setup_walk', HERE / 'setup-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class BootSetup(unittest.TestCase):
    def test_no_verdict_yet_is_none(self):
        self.assertIsNone(walk.boot_setup('[richos] this app: built from abc\n'))

    def test_the_video_tools_alone_missing(self):
        text = ('[richos] first-run setup: media-tools is NOT installed — 2 place(s) looked:\n'
                '[richos]   looked in /h/Library/Application Support/RichOS/tools/yt-dlp — no yt-dlp\n')
        self.assertEqual(walk.boot_setup(text), {'nothing_missing': False, 'missing': ['media-tools']})

    def test_nothing_missing(self):
        self.assertEqual(walk.boot_setup('[richos] first-run setup: nothing missing.\n'),
                         {'nothing_missing': True, 'missing': []})


class ToolsOutcome(unittest.TestCase):
    def test_still_running_progress_is_none(self):
        self.assertIsNone(walk.tools_outcome('[richos] video tools in the background 1/1 started: Getting my video tools. 42%\n'))

    def test_done_from_the_background_or_a_press(self):
        for source in ('video tools in the background', 'setup'):
            self.assertEqual(walk.tools_outcome(f'[richos] {source} 1/1 done: My video tools are installed.\n'), 'done')
            self.assertEqual(walk.tools_outcome(f'[richos] {source} 1/1 done: My voice and video tools are installed.\n'), 'done')
        self.assertEqual(walk.tools_outcome('[richos] video tools in the background 1/1 FAILED — no internet\n'), 'failed')

    def test_the_pins_file_is_found(self):
        self.assertTrue(walk.PINS.is_file(), walk.PINS)
        self.assertEqual(walk.pinned_bytes(), {walk.VOICE: 487614201, walk.TRANSCRIPTION: 574041195})


class DownloadProgress(unittest.TestCase):
    want = {walk.VOICE: 100, walk.TRANSCRIPTION: 200}

    def test_partials_count_and_are_not_done(self):
        p = walk.download_progress({walk.VOICE: 100, walk.TRANSCRIPTION + '.part': 50}, self.want)
        self.assertEqual(p, {'bytes': 150, 'of': 300, 'done': False})

    def test_both_whole_is_done(self):
        p = walk.download_progress({walk.VOICE: 100, walk.TRANSCRIPTION: 200, 'other.bin': 9}, self.want)
        self.assertEqual(p, {'bytes': 300, 'of': 300, 'done': True})


def row(event, got, done=False, pressed=False):
    return {'event': event, 'since_launch_s': 0, 'downloaded_bytes': got, 'download_done': done, 'pressed': pressed}


USER = [row(name, 10, pressed=True) for name in walk.USER_MOMENTS]


class MeanwhileVerdict(unittest.TestCase):
    def test_running_from_launch_and_through_first_setup_passes(self):
        self.assertIsNone(walk.meanwhile_verdict([row('walk begins', 0), row('models arriving', 5)] + USER))

    def test_a_download_that_needs_a_press_fails(self):
        t = [row('walk begins', 0), row('setup sheet: "Set it up" pressed', 0, pressed=True), row('x', 5)] + USER
        self.assertIn('only after a press', walk.meanwhile_verdict(t))

    def test_first_setup_after_the_download_fails(self):
        t = [row('walk begins', 1)] + [row(name, 99, done=True, pressed=True) for name in walk.USER_MOMENTS]
        self.assertIn('after the download had finished', walk.meanwhile_verdict(t))


class VoiceLines(unittest.TestCase):
    # The walk of candidate 44: one ask at launch, model-missing, and none after the models arrived.
    MISSING = ("[richos] voice: not ready on this machine (model-missing) — My speech model isn't on "
               "this machine yet.\n")
    READY = "[richos] voice: ready on this machine (ready) — whisper.cpp 1.9.1 model:small.en@c6138d6d58ec\n"

    def test_only_the_launch_ask_is_not_ready(self):
        lines = walk.voice_lines('[richos] video tools in the background 1/1 done: x\n' + self.MISSING)
        self.assertEqual(len(lines), 1)
        self.assertEqual(walk.voice_ready(lines), [])

    def test_an_ask_after_the_model_arrived_is_ready(self):
        lines = walk.voice_lines(self.MISSING + '[richos] voice model: ggml-small.en.bin installed\n' + self.READY)
        self.assertEqual(walk.voice_ready(lines), [self.READY.rstrip('\n')])

    def test_the_voice_step_runs_before_the_relaunch(self):
        self.assertLess(walk.STEPS.index('voice'), walk.STEPS.index('relaunch'))
        self.assertGreater(walk.STEPS.index('voice'), walk.STEPS.index('arrived'))


class Round19(unittest.TestCase):
    """Round 19's states 1 to 4 (dictation plan slice 4): the walk's verdicts, offline."""

    GOOD = {'state1_ocr': True, 'state1_ax': True, 'state2_ocr': True,
            'readings': [[490, '1.06 GB'], [700, '1.06 GB']],
            'start': {'with_all_set': False}, 'all_set': {'download_done': True}}

    def test_the_counter_is_read_as_round_19_draws_it(self):
        self.assertEqual(walk.counter_reading('490 MB of 1.06 GB'), (490, '1.06 GB'))
        self.assertEqual(walk.counter_reading('100 MB of 574 MB'), (100, '574 MB'))
        self.assertIsNone(walk.counter_reading('Checking…'))
        self.assertIsNone(walk.counter_reading('Getting my voice and video tools. 43%'))

    def test_readings_must_increase_against_one_whole(self):
        self.assertTrue(walk.readings_increase([(490, '1.06 GB'), (700, '1.06 GB')]))
        self.assertFalse(walk.readings_increase([(490, '1.06 GB')]))
        self.assertFalse(walk.readings_increase([(490, '1.06 GB'), (490, '1.06 GB')]))
        self.assertFalse(walk.readings_increase([(700, '1.06 GB'), (490, '1.06 GB')]))
        self.assertFalse(walk.readings_increase([(400, '574 MB'), (700, '1.06 GB')]))

    def test_the_verdict_passes_only_what_the_plan_names(self):
        self.assertIsNone(walk.r19_verdict(self.GOOD))
        # Start with "You're all set." already up (the models came first) still passes: the plan's
        # "Start before that when the engine is in first" is recorded, not required.
        self.assertIsNone(walk.r19_verdict({**self.GOOD, 'start': {'with_all_set': True}}))
        for key, bad, words in (('state1_ocr', False, 'state 1'), ('state2_ocr', False, 'download line'),
                                ('readings', [[490, '1.06 GB']], 'increase'), ('start', None, 'Start'),
                                ('all_set', {'download_done': False}, 'before both models')):
            self.assertIn(words, walk.r19_verdict({**self.GOOD, key: bad}))

    def test_the_round_19_steps_are_never_in_the_default_run(self):
        self.assertFalse(set(walk.R19_STEPS) & set(walk.STEPS))
        self.assertEqual(walk.R19_STEPS, ['r19-grants', 'r19-sheet', 'r19-offer', 'r19-set-up', 'r19-key-change',
                                          'r19-again', 'r19-restart'])
        for step in walk.R19_STEPS:
            self.assertTrue(hasattr(walk.SetupWalk, step.replace('-', '_')), step)

    def test_rich_must_name_the_new_key_and_never_the_old_one(self):
        self.assertIsNone(walk.key_reply_verdict('F3.', 3, 2))
        self.assertIsNone(walk.key_reply_verdict('Tap **F3**, talk, then tap it again.', 3, 2))
        self.assertIn('did not name F3', walk.key_reply_verdict('Tap F2.', 3, 2))
        self.assertIn('still names F2', walk.key_reply_verdict('It was F2; now it is F3.', 3, 2))
        self.assertIn('did not name F3', walk.key_reply_verdict('F13 is the key.', 3, 2))
        self.assertEqual(walk.key_reply_verdict('', 3, 2), 'Rich gave no answer')


if __name__ == '__main__':
    unittest.main()

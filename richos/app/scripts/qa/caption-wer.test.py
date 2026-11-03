#!/usr/bin/env python3
"""caption-wer.py contracts from synthetic captions. No model, network or VM."""
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('cw', Path(__file__).with_name('caption-wer.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

ROLLING = '''WEBVTT
Kind: captions
Language: en

00:00:00.000 --> 00:00:01.430 align:start position:0%

Before<00:00:00.240><c> we</c><00:00:00.320><c> use</c><00:00:00.440><c> Claude</c>

00:00:01.430 --> 00:00:01.440 align:start position:0%
Before we use Claude


00:00:01.440 --> 00:00:03.830 align:start position:0%
Before we use Claude
and<00:00:01.640><c> Tailscale,</c><00:00:01.720><c> stop.</c>
'''

PLAIN = '''WEBVTT

1
00:00:00.000 --> 00:00:01.000
Hello there

2
00:00:01.000 --> 00:00:02.000
Hello there

3
00:00:02.000 --> 00:00:03.000
general Kenobi
'''


class Contracts(unittest.TestCase):
    def test_rolling_cues_are_read_once(self):
        self.assertEqual(m.caption_text(ROLLING), 'Before we use Claude and Tailscale, stop.')

    def test_plain_repeated_cues_preserve_each_utterance(self):
        self.assertEqual(m.caption_text(PLAIN), 'Hello there Hello there general Kenobi')

    def test_identical_words_are_zero_and_case_and_punctuation_do_not_count(self):
        r = m.compare(ROLLING, 'before we use claude, and Tailscale stop')
        self.assertEqual(r['wer'], 0.0)
        self.assertEqual(r['capitalization_differences'], 2)
        self.assertIn({'captions': 'Claude', 'transcript': 'claude', 'count': 1}, r['capitalization_top'])

    def test_substitution_deletion_insertion_are_counted_and_near_spellings_named(self):
        # 7 caption words; "Claude"->"cloud" (sub), "stop" deleted, "uh" inserted: 3 / 7.
        r = m.compare(ROLLING, 'Before we use cloud uh and Tailscale')
        self.assertEqual((r['substitutions'], r['deletions'], r['insertions']), (1, 1, 1))
        self.assertEqual(r['wer'], round(3 / 7, 4))
        self.assertEqual(r['spelling_top'], [{'captions': 'claude', 'transcript': 'cloud', 'count': 1}])

    def test_disagreements_carry_the_caption_time_they_start_at(self):
        self.assertEqual(m.caption_times(ROLLING), [0.0, 0.24, 0.32, 0.44, 1.44, 1.64, 1.72])
        r = m.compare(ROLLING, 'Before we use cloud and Tailscale stop', disagreements=True)
        self.assertEqual(r['disagreements'], [{'at': '00:00:00', 'captions': 'Claude', 'transcript': 'cloud'}])
        r = m.compare(ROLLING, 'Before we use Claude and Tails kale stop', disagreements=True)
        self.assertEqual(r['disagreements'], [{'at': '00:00:01', 'captions': 'Tailscale,', 'transcript': 'Tails kale'}])

    def test_a_decoder_loop_shows_in_repeats(self):
        r = m.compare(ROLLING, 'All right. ' * 50)
        self.assertEqual(r['repeats']['transcript'], {'phrase': 'all right', 'count': 50})
        self.assertGreater(r['wer'], 1.0)


if __name__ == '__main__':
    unittest.main(verbosity=2)

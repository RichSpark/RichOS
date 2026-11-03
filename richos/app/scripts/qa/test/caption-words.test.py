#!/usr/bin/env python3
"""Real caption bridge preserves spoken numbers and repetition, including their times."""
import importlib.util
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(os.environ.get('G8_ROOT', Path(__file__).resolve().parents[5]))
sys.path.insert(0, str(ROOT / 'richos/app/scripts/qa'))
spec = importlib.util.spec_from_file_location('caption_wer', ROOT / 'richos/app/scripts/qa/caption-wer.py')
caption = importlib.util.module_from_spec(spec); spec.loader.exec_module(caption)

class SpokenWords(unittest.TestCase):
    def test_repetition_and_numeric_speech_survive_cue_ids(self):
        vtt = 'WEBVTT\n\n1\n00:00:00.000 --> 00:00:01.000\nYes\n\n2\n00:00:01.000 --> 00:00:02.000\nYes\n\n3\n00:00:02.000 --> 00:00:03.000\n42\n42\n'
        self.assertEqual(caption.caption_text(vtt), 'Yes Yes 42 42')
        self.assertEqual(caption.caption_times(vtt), [0., 1., 2., 2.])
        result = caption.compare(vtt, 'Yes Yes 42 42')
        self.assertEqual(result['caption_words'], 4)
        self.assertEqual(result['wer'], 0.)

if __name__ == '__main__': unittest.main()

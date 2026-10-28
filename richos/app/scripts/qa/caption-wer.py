#!/usr/bin/env python3
"""caption-wer.py — how far a transcript is from a video's captions: word error rate, and the
spelling and capitalization differences, in one JSON answer.

  caption-wer.py CAPTIONS.vtt TRANSCRIPT.txt [--top N] [--disagreements]

CAPTIONS is a WebVTT file as yt-dlp writes it (YouTube's rolling cues, where each cue repeats the
line before it, are read once). TRANSCRIPT is plain text (whisper-cli -otxt).

WHAT IS MEASURED
  wer          (substitutions + deletions + insertions) / caption words, on NORMALIZED words:
               lowercase, curly apostrophes made straight, every character that is not a letter,
               digit or apostrophe removed. Capitalization and punctuation do not count here.
  alignment    difflib's longest matching blocks anchor the two word lists; between two anchors
               the words are aligned by an exact Levenshtein, so the counts are a minimum within
               each gap. Exact over the whole text would cost |captions| x |transcript| cells
               (about 200 million for an hour), which is why it is anchored.
  capitalization  aligned word pairs that are the same word apart from letter case, counted by
               pair (for example "claude" in the captions against "Claude" in the transcript).
  spelling     aligned substitutions whose normalized forms differ but are close (difflib ratio
               at least 0.6), counted by pair: the "Cloud"/"Claude" kind, not a different word.
  disagreements  with --disagreements: every stretch where the two differ in a normalized word,
               with the caption time it starts at (hh:mm:ss, from the cue and its inline word
               times), what the captions say and what the transcript says there.
  repeats      the most frequent 2-word phrase in the transcript and how often it occurs, beside
               the same count in the captions: a decoder loop shows up here first.

Exit 0 with the JSON on stdout; exit 2 with a sentence when a file cannot be read or is empty."""
import argparse
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from devtools_bridge import request

def caption_text(vtt):
    return request("caption-text", vtt)

def caption_times(vtt):
    return request("caption-times", vtt)

def norm(word):
    return request("caption-norm", word)

def close(a, b):
    return request("caption-close", [a, b])

def compare(vtt, txt, top=25, disagreements=False):
    return request("caption-compare", {"vtt": vtt, "txt": txt, "top": top, "disagreements": disagreements})

def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('captions', type=Path)
    p.add_argument('transcript', type=Path)
    p.add_argument('--top', type=int, default=25, help='how many of each difference to list')
    p.add_argument('--disagreements', action='store_true', help='list every differing stretch with its caption time')
    a = p.parse_args()
    try:
        vtt, txt = a.captions.read_text(encoding='utf-8'), a.transcript.read_text(encoding='utf-8')
    except OSError as exc:
        print(f'caption-wer: cannot read the input: {exc}', file=sys.stderr)
        return 2
    if not caption_text(vtt).strip() or not txt.strip():
        print('caption-wer: the captions or the transcript has no words', file=sys.stderr)
        return 2
    print(json.dumps(compare(vtt, txt, a.top, a.disagreements), indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())

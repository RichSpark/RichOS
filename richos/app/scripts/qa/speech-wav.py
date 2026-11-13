#!/usr/bin/env python3
"""speech-wav.py — a spoken fixture (a meeting, a long dictation) as ONE WAV file, from a script.

  speech-wav.py SCRIPT.json OUT.wav [--rate 48000]

SCRIPT.json is a JSON list of lines, spoken in order:

  [{"voice": "Samantha", "text": "We decided to raise prices.", "pause": 1.5}, ...]

`voice` is a macOS `say` voice (default Samantha), `pause` the silence AFTER the line in seconds
(default 1.0). Each line is rendered with `say -o` to a file, so NOTHING IS PLAYED ALOUD anywhere
(CEO §53: the Mac's speakers never stand in for a person), converted with ffmpeg to mono 16-bit PCM
at --rate (48000 is what `mac-audio-lab play` puts into BlackHole; 16000 is what a speech model
reads) and joined with the pauses into OUT.wav.

It prints one JSON object: the file, its duration in seconds and, for each line, the second it
starts and ends at and its text. A walk uses those times to say what had been said by the moment
it asked about it (a question about "the last minute of the call") and where a source link should
land. Exit 1 with a sentence when the script is not a usable list, a voice is unknown to `say`,
`say` or ffmpeg is missing or fails, or the output exists already (it never overwrites evidence).
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import wave


def refuse(sentence):
    print('speech-wav.py: ' + sentence, file=sys.stderr)
    sys.exit(1)


def load(path):
    try:
        lines = json.loads(Path(path).read_text())
    except (OSError, ValueError) as exc:
        refuse('cannot read %s as JSON (%s)' % (path, exc))
    if not isinstance(lines, list) or not lines:
        refuse('%s must be a non-empty JSON list of lines' % path)
    out = []
    for n, line in enumerate(lines, 1):
        if not isinstance(line, dict) or not isinstance(line.get('text'), str) or not line['text'].strip():
            refuse('line %d needs a non-empty "text"' % n)
        pause = line.get('pause', 1.0)
        if isinstance(pause, bool) or not isinstance(pause, (int, float)) or pause < 0:
            refuse('line %d: "pause" must be a number of seconds, 0 or more' % n)
        out.append({'voice': str(line.get('voice') or 'Samantha'), 'text': line['text'].strip(), 'pause': float(pause)})
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('script', type=Path)
    p.add_argument('out', type=Path)
    p.add_argument('--rate', type=int, default=48000, help='sample rate of OUT.wav (default 48000)')
    a = p.parse_args()
    lines = load(a.script)
    if a.out.exists():
        refuse('%s exists already; it is never overwritten' % a.out)
    say, ffmpeg = shutil.which('say'), shutil.which('ffmpeg')
    if not say or not ffmpeg:
        refuse('needs both `say` (macOS) and ffmpeg on PATH; found say=%s ffmpeg=%s' % (say, ffmpeg))
    known = subprocess.run([say, '-v', '?'], capture_output=True, text=True).stdout
    voices = {row.split()[0] for row in known.splitlines() if row.strip()}
    for n, line in enumerate(lines, 1):
        if line['voice'] not in voices:
            refuse('line %d: "%s" is not a voice `say -v ?` lists' % (n, line['voice']))
    frames, timeline, clock = [], [], 0.0
    with tempfile.TemporaryDirectory(prefix='speech-wav.') as tmp:
        for n, line in enumerate(lines, 1):
            aiff, pcm = Path(tmp) / ('%d.aiff' % n), Path(tmp) / ('%d.wav' % n)
            r = subprocess.run([say, '-v', line['voice'], '-o', str(aiff), line['text']], capture_output=True, text=True)
            if r.returncode or not aiff.exists():
                refuse('say failed on line %d: %s' % (n, (r.stderr or r.stdout).strip()[-200:]))
            r = subprocess.run([ffmpeg, '-nostdin', '-loglevel', 'error', '-y', '-i', str(aiff), '-ac', '1',
                                '-ar', str(a.rate), '-sample_fmt', 's16', str(pcm)], capture_output=True, text=True)
            if r.returncode:
                refuse('ffmpeg failed on line %d: %s' % (n, r.stderr.strip()[-200:]))
            with wave.open(str(pcm), 'rb') as w:
                data = w.readframes(w.getnframes())
            seconds = len(data) / 2 / a.rate
            timeline.append({'n': n, 'start': round(clock, 2), 'end': round(clock + seconds, 2), 'voice': line['voice'],
                             'text': line['text']})
            frames.append(data + b'\x00\x00' * int(round(line['pause'] * a.rate)))
            clock += seconds + line['pause']
    a.out.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(a.out), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(a.rate)
        w.writeframes(b''.join(frames))
    print(json.dumps({'file': str(a.out), 'rate': a.rate, 'seconds': round(clock, 2), 'lines': timeline}, indent=2))


if __name__ == '__main__':
    main()

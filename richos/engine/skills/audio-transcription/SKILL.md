---
name: audio-transcription
description: Transcribe audio to text with timestamps, using the speech-to-text tooling ALREADY INSTALLED on this Mac (whisper.cpp, its models, ffmpeg and RichOS's own transcription pipeline). Use whenever a task involves transcribing, listening to, or getting the words out of any audio or video file (a voice memo, a call, an mp3/m4a/wav/mov/mp4, a podcast, a recording the CEO sent). One command, no setup.
---

# Audio transcription — one command, everything already installed

**Do not search for a speech-to-text tool, do not download Whisper or any model,
do not install anything (no `pip install openai-whisper`, no `brew install`, no
Hugging Face download).** All of it is installed on this Mac and wired into
RichOS's own transcription pipeline. Run this:

```bash
~/.claude/richos-engine/skills/audio-transcription/transcribe.sh <audio-or-video-file> <output-dir>
```

It prints the transcript's path on stdout. Then `Read` `<output-dir>/transcript.md`.

- `<output-dir>`: a new or empty directory **outside every repository**, in your
  own scratch, returned by `scripts/lib/machine_settings.py scratch <your-name>`, with a `transcript-1` child. The
  pipeline refuses a directory inside a product repository.
- Any format ffmpeg reads works: mp3, m4a, wav, aiff, ogg, webm, mov, mp4, mkv.
- Non-English audio: add `--lang <code>` (e.g. `--lang de`) or `--lang auto`.
- Speed: about 17 s for a 2.4-minute recording on this Mac (measured 2026-10-07).
- Delete the output directory when your task is done.

## What you get

`transcript.md`, one paragraph per turn, each with its start time:

```
**[00:11] Me:** I have the option to toggle that sidebar and close it.
```

"Me" here means "the recording". The script downmixes to mono because the
pipeline's two-channel mode is for RichOS call captures (LEFT = me, RIGHT =
everyone else) and would transcribe an ordinary stereo file twice. The header's
"Speaker attribution" line describes that call mode; ignore it for a file.

Also in `<output-dir>`: `pipeline.log`, and `pipeline/<session>/` holding
`verification.json` (coverage and every guard warning), `session.json` (which
model, which whisper build, their hashes) and the whisper JSON with per-word
times (`me.json`, if you need a word's exact time).

## What it runs, so you never have to choose

The script is a thin wrapper over `richos/tools/richos-service` (the call
pipeline the product ships): ffmpeg normalize to 16 kHz mono, then
`whisper-cli` with the model and decode settings that pipeline measured and
pinned (default `large-v3-turbo-q5_0` from `~/Models/Whisper/`; the exact
command line and why each flag is set are in that pipeline's README), then its
repetition, deletion and substitution guards and its vocabulary corrector.
Programs: `/opt/homebrew/bin/whisper-cli`, `/opt/homebrew/bin/ffmpeg`,
`/opt/homebrew/bin/ffprobe`. Models: `~/Models/Whisper/`.

## When it stops

| Exit | Meaning | Do |
|---|---|---|
| 1 | usage, missing file, a tool not on PATH, output dir not empty | fix the call; never install a tool, report a missing one to the lead |
| 2 | the pipeline refused or failed; the log tail is printed | report that output to the lead; do not fall back to another tool |
| 3 | the file has no audio track | nothing to transcribe |

A video or screen recording you need to SEE as well as hear: use the
`watch-video` skill instead; it runs this same transcription.

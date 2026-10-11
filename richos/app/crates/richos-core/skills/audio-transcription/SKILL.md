---
name: audio-transcription
description: Use whenever a job involves transcribing, listening to, or getting the words out of an audio file (a voice memo, a call, a podcast, an mp3/m4a/wav/aiff/ogg file the CEO sent or attached). The tools are already installed by RichOS (ffmpeg, whisper-cli and a speech model); this says which commands to run. A video or screen recording you also need to SEE goes to the watch-video skill instead.
---

# Audio: get the words out of a recording

RichOS installs everything this needs on the CEO's Mac: `ffmpeg`, `whisper-cli`, and a speech
model whose path is in `$RICHOS_SPEECH_MODEL`. They are bare commands on your PATH.

**Never install anything for an audio job.** No `pip install`, `brew install`, `npm install`, no
downloaded binary, no other Whisper or model. If a tool is missing, say so in one plain sentence
(see "When something is missing") and stop.

A video or screen recording whose pictures matter too: use the `watch-video` skill, which runs
this same transcription beside its frames.

## 1. A working folder you can read

You can only open files inside your working directories, so the converted audio and the
transcript go in a fresh folder under the current one. Make it first and delete it when you have
answered:

```bash
work="$(mktemp -d "$PWD/audio-XXXXXX")"
# ... the steps below ...
rm -rf "${work:?}"
```

## 2. Transcribe it

Any format `ffmpeg` reads works (mp3, m4a, wav, aiff, ogg, webm, and the sound of a video file).
Convert it to 16 kHz mono first, then transcribe:

```bash
ffmpeg -nostdin -hide_banner -loglevel error -i "<audio file>" -vn -ar 16000 -ac 1 -c:a pcm_s16le "$work/audio.wav"
whisper-cli -m "$RICHOS_SPEECH_MODEL" -f "$work/audio.wav" -l en -t 4 -fa -mc 0 -np -otxt -of "$work/transcript"
```

Use these flags exactly. `-mc 0` matters most: without it whisper-cli carries its own earlier
words forward, and on an hour-long recording it repeats one phrase hundreds of times and drops
most of what was said. Do not add `-nt`: without timestamps it loses the words at every
30-second seam. Read `$work/transcript.txt`.

For speech that is not English, put `-l auto` (or the language's code, for example `-l de`) in
place of `-l en`. A long recording takes minutes: for anything longer than a few minutes, start
the `whisper-cli` command with your shell tool's background option, say it is in progress rather
than going quiet, and read the transcript when it has finished.

If `ffmpeg` says the file has no audio stream, there is nothing to transcribe: say so.

## When something is missing or fails

- **No `$RICHOS_SPEECH_MODEL`, or the file it names is not there, or `whisper-cli` fails:** say
  plainly that the speech model is not ready on this Mac yet and that RichOS installs it by
  itself. Do not look for another way to transcribe.
- **`ffmpeg` is not found:** say that the audio tools are missing from this installation.

Never send the recording or its words to any other service.

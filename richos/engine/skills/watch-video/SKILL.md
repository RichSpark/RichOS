---
name: watch-video
description: Watch a video or screen recording - see what is on screen AND hear what is said - with tooling ALREADY INSTALLED on this Mac. Use whenever a task involves watching, reviewing, analyzing or understanding a video, screen recording, screencast, demo, bug recording or .mov/.mp4/.webm/.mkv file (for example one the CEO recorded). One command gives a timestamped transcript plus still frames at every scene change and every 5 seconds, interleaved in one index you read with the Read tool.
---

# Watch a video or screen recording — one command, everything already installed

**Do not search for tools, do not download Whisper or any model, do not install
anything (no pip, no brew, no Hugging Face).** Transcription (whisper.cpp with
RichOS's own pipeline) and frame extraction (ffmpeg) are installed on this Mac.
Run this:

```bash
~/.claude/richos-engine/skills/watch-video/watch.sh <video-file> <output-dir>
```

It prints the index's path on stdout. Then:

1. `Read` `<output-dir>/watched.md` — frames and transcript paragraphs in time order:

   ```
   [00m10.0s] FRAME <output-dir>/frames/frame-0003-t00m10.0s.png  (no change for 5s)

   **[00:11] Me:** I have the option to toggle that sidebar and close it.

   [00m12.6s] FRAME <output-dir>/frames/frame-0004-t00m12.6s.png  (scene change (0.068910))
   ```

2. `Read` each FRAME path (a PNG, at most 1920 px wide) to see the screen at that
   moment, beside the words spoken around it. The Read tool shows images.

- `<output-dir>`: a new or empty directory **outside every repository**, in your
  own scratch, e.g. `/Volumes/E1TB/tmp/claude/<your-name>/watch-1`.
- Speed: about 28 s for a 2.4-minute 1080p screen recording on this Mac, giving
  42 frames and a 270-word transcript (measured 2026-10-07).
- Non-English speech: add `--lang <code>` or `--lang auto`.
- A long recording: the 5-second floor alone is at most 12 frames a minute, but
  scene changes come ON TOP of it and `--every` does not limit them (measured
  2026-10-09: an 8.1-minute edited video gave 153 frames, 110 of them scene
  changes, 18.8 a minute). Cap the count with `--budget N` (the video's time is cut
  into N equal stretches, each keeps its biggest scene change, the first always stays),
  e.g. `--budget 60`.
- Skim, then zoom: add `--sheet 12` to also get contact sheets (12 frames tiled in
  one JPEG under `<output-dir>/sheets/`, each listed in the index just before its
  first frame; the full frames stay). Read the transcript and a few sheets first,
  then cut frames only where it matters:

  ```bash
  ~/.claude/richos-engine/skills/watch-video/frames-at.sh <video-file> <dir> --at 02:06,05:30
  ~/.claude/richos-engine/skills/watch-video/frames-at.sh <video-file> <dir> --window 02:06 02:26 --step 2
  ```

  Times are seconds, MM:SS or HH:MM:SS; it prints each full-size PNG's path. The
  index's "Video file:" line names the video to pass.
- A link instead of a file: `watch.sh <url> <output-dir>` downloads it with the
  installed `yt-dlp` at up to 720p (`--height 1080` for small on-screen text), keeps
  it as `<output-dir>/video.mp4`, then runs the same pipeline.
- The index has one row per transcript paragraph, in time order with the frames.
  A file's transcript is one channel; speakers are not separated.
- A recording with no sound still gets its frames; `watched.md` says there is no
  transcript.
- Delete the output directory when your task is done.

## The frame rule, and why

A frame is kept when the screen changes (scene score over **0.05**, judged at 5
frames a second, at most one per second of a transition), and otherwise at least
every **5 seconds**. On the CEO's screen recording of 2026-10-07 a panel opening
scored 0.10-0.30 and typing or a pointer move scored under 0.01, so 0.05 catches
the first and skips the second. The 5-second floor covers what the threshold
skips (typed text, a slow scroll, the pointer showing "this button here"): it is
about one spoken sentence, so every transcript paragraph has a frame from inside
it. The values live in `watch.sh`; nobody types them.

## Also in `<output-dir>`

`transcript.md` on its own, `pipeline.log`, and `pipeline/<session>/` with
`verification.json` (coverage and guard warnings). "Me" in the transcript means
the recording's sound, not a speaker; see the `audio-transcription` skill for
the transcript's details.

## When it stops

| Exit | Meaning | Do |
|---|---|---|
| 1 | usage, missing file, no video track, a tool not on PATH, output dir not empty | fix the call; never install a tool, report a missing one to the lead; for audio only use `audio-transcription` |
| 2 | frames written, transcription failed; the reason is printed and in `watched.md` | use the frames, report the transcription output to the lead |
| 4 | the frame step was stopped again and again on one stretch of the video (the host's CPU circuit breaker, or a refusal); no `watched.md`, but `transcript.md` is kept | `Read` the transcript, then run again when the Mac is quieter, or cut chosen frames with `frames-at.sh`; report it to the lead if it repeats |

A single stop is not an exit: the frame step runs in 300-second stretches under `nice` with
2 decoder and 2 filter threads (a single ffmpeg stays under the 3 cores the breaker allows),
and a stretch that is stopped is redone alone, up to 4 times, 10 seconds apart. The run
prints `redoing this 300s stretch` for each. A stretch's first frame is always kept
(`start of a 300s stretch` in the index). On a 41-minute video the run takes longer than a
plain ffmpeg would; that is the price of finishing.

Audio only (no picture to see): use the `audio-transcription` skill.

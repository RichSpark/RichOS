---
name: watch-video
description: Use whenever the CEO asks you or a job needs you, to download, watch, listen to, summarize or answer questions about a video or screen recording, whether a YouTube or other web link or a video file on his Mac (.mov/.mp4/.webm/.mkv). The tools are already installed by RichOS (yt-dlp, ffmpeg, ffprobe, whisper-cli and a speech model); this says which commands to run and where the files go. An audio-only file goes to the transcribe-audio skill instead.
---

# Video: download it, watch it, hear it

RichOS installs and updates everything this needs on the CEO's Mac: `yt-dlp` (kept at its newest
version by the app), `ffmpeg`, `ffprobe`, `whisper-cli` and a speech model whose path is in
`$RICHOS_SPEECH_MODEL`. They are bare commands on your PATH.

**Never install anything for a video job.** No `pip install`, `brew install`, `npm install`, no
downloaded binary, no other Whisper or model. If a tool is missing, say so in one plain sentence
(see "When something is missing") and do what the remaining tools allow.

## 1. A working folder you can read

You can only open files inside your working directories, so frames and audio go in a fresh
folder under the current one. Make it first and delete it when you have answered:

```bash
work="$(mktemp -d "$PWD/video-XXXXXX")"
# ... the steps below ...
rm -rf "${work:?}"
```

## 2. Download a video

When the CEO asks you to download a video, it goes to his Downloads folder or to the folder he
names. Never leave it in a hidden folder.

```bash
yt-dlp -P ~/Downloads --print after_move:filepath "<url>"
```

The line it prints is the saved file. Tell him that full path. If he only asked to download, you
are done here.

## 3. Watch a video: the pictures

Use a local video where it already is. Reuse a copy you can identify as the
same video, including a download from step 2. When asked about a YouTube link and
no local copy is known, save a playable copy in the user's Downloads under
RichOS Videos so that the answer can play it again after the job is done:

```bash
yt-dlp --no-playlist --no-overwrites \
  -f 'bv[ext=mp4][vcodec^=avc1][height<=720]+ba[ext=m4a]/b[ext=mp4][vcodec^=avc1][height<=720]' \
  --merge-output-format mp4 -P "$HOME/Downloads/RichOS Videos" \
  -o '%(title).120B [%(id)s].%(ext)s' --print after_move:filepath "<url>"
```

Use the full path the command prints as the local source. The work folder
holds only analysis files. For other links, a copy taken just to inspect the
video can go in the work folder:

```bash
yt-dlp -S "res:720" -P "$work" -o "video.%(ext)s" --print after_move:filepath "<url>"
```

If you can only obtain captions for a link, use them without pretending a local
video exists. Only extract frames if you have the video.

Find its length, then take one frame every 10 seconds:

```bash
ffprobe -v error -show_entries format=duration -of csv=p=0 "<video>"
ffmpeg -nostdin -hide_banner -loglevel error -i "<video>" \
  -vf "fps=1/10:round=up,scale='min(1280,iw)':-2" "$work/frame-%04d.jpg"
```

Frame N shows the video at about (N - 1) x 10 seconds. Open the frames with the Read tool to see
them. `round=up` makes frame 1 the opening picture, so even a recording of a second or two gives
one. For a video longer than 10 minutes use `fps=1/30:round=up` and over an hour
`fps=1/60:round=up`; then look more closely (a second, narrower `ffmpeg -ss <start> -t <length>`
pass) only where it matters.

## 4. Watch a video: the words

Three sources, in this order.

For a link, always start with a, even when you have already downloaded the video in step 2 or
step 3: the file came from the link, so the captions a person uploaded to it still come first.
Only a file the CEO gave you from his Mac, with no link, starts at b.

**a. Captions a person uploaded, through yt-dlp.** Fast and nothing is downloaded but text:

```bash
yt-dlp --skip-download --write-subs --sub-langs "en" \
  -P "$work" -o "%(id)s.%(ext)s" "<url>"
```

If it writes a `.vtt` file, read it; it is plain text with timestamps. If it says "There are no
subtitles for the requested languages", the video has no captions a person uploaded: go to b.
Never add `--write-auto-subs` here: the captions YouTube generates by itself are often poor, so
your own transcription comes before them. If the CEO speaks another language, put its code in
place of `en` (for example `"de"`). Keep the list this narrow: a wide pattern such as `"en.*"`
fetches several tracks and YouTube answers with HTTP 429.

**b. No uploaded captions or a file the CEO gave you from his Mac: transcribe it.**

```bash
ffmpeg -nostdin -hide_banner -loglevel error -i "<video>" -vn -af aresample=async=1:first_pts=0 -ar 16000 -ac 1 -c:a pcm_s16le "$work/audio.wav"
whisper-cli -m "$RICHOS_SPEECH_MODEL" -f "$work/audio.wav" -l en -t 4 -fa -mc 0 -np -otxt -ovtt -of "$work/transcript"
```

Use these flags exactly. `-mc 0` matters most: without it whisper-cli carries its own earlier
words forward and on an hour-long video it repeats one phrase hundreds of times and drops most
of what was said. Do not add `-nt`: without timestamps it loses the words at every 30-second
seam. The `.txt` file holds the words only either way. Read `$work/transcript.txt`. For speech that is not English, put `-l auto` in
place of `-l en`. A long video takes minutes to transcribe: for anything longer than a few
minutes, start the `whisper-cli` command with your shell tool's background option, tell the CEO
it is in progress rather than going quiet and read the transcript when it has finished.

**c. Auto-generated captions, only when you cannot transcribe** (no `$RICHOS_SPEECH_MODEL` or
`whisper-cli` fails):

```bash
yt-dlp --skip-download --write-auto-subs --sub-langs "en,en-orig" \
  -P "$work" -o "%(id)s.%(ext)s" "<url>"
```

Then tell the CEO that the words come from the captions YouTube generated by itself, which are
often inaccurate.

## 5. Let the user jump to the evidence

When answering a question about a recording, pick the moments that answer the
question. Keep their exact start times from the caption or transcript cues or
the verified frames. Use the duration from `ffprobe` or the source's media
metadata and give `durationMs` and each point's `startMs` in milliseconds. Use a
short label and the relevant words or description of the verified frame.

For a playable MP4, MOV, M4V or WebM on the Mac, including a YouTube download,
use `source: {"kind":"local","path":"…"}` with its full persistent path in
`mcp__richos_files__show_media_moments`. Use that same file for the waveform.
If you know the local copy came from YouTube, also include the original 11-character
`video_id` in this local source, so a phone can jump to YouTube if the file is too
large to send. RichOS on the Mac still plays the saved copy.
If step 4b didn't already produce `audio.wav`, make it with the ffmpeg command
from that step. Uploaded captions can answer the question without running
Whisper, but the waveform comes from the video's sound:

```bash
python3 "$RICHOS_ENGINE_ROOT/app/skills/transcribe-audio/waveform.py" "$work/audio.wav"
```

Use the single line as `waveform`. Omit it if the video has no audio stream or
the tools cannot read it. For a YouTube link with no playable local copy, use
`source: {"kind":"youtube","video_id":"…"}` with its 11-character video ID.
Give the moments in your answer when neither source is available. Delete the
analysis files and keep the saved video used by the card.

If a named specialist is doing the viewing and does not have the presentation
tool, give Stu the source, duration, points and any waveform in the findings,
so he can put them in the original conversation. Use the full original
recording's time base, including when you looked more closely at an excerpt.

## 6. Inspect a file

```bash
ffprobe -v error -show_entries format=duration,size:stream=codec_type,codec_name,width,height -of compact "<video>"
```

## When something is missing or fails

- **No `$RICHOS_SPEECH_MODEL` or the file it names is not there or `whisper-cli` fails:** use
  the auto-generated captions (4c) and say they are auto-generated. With no captions either, say
  plainly that you watched the pictures only and could not hear the video, then answer from the
  frames.
- **`yt-dlp` is not found:** say that the video downloader is not installed on this Mac yet and
  that RichOS installs it by itself. A file already on his Mac still works with `ffmpeg`.
- **`ffmpeg` or `ffprobe` is not found:** say that the video tools are missing from this
  installation; for a link, captions through `yt-dlp` may still give you the words.
- **The site says the video is private, needs a sign-in or is not available in his country:**
  tell him that in one sentence. Do not look for a way around it.

Never send the video or its sound to any other service.

#!/usr/bin/env bash
# transcribe.sh — any audio or video file -> a timestamped transcript, through
# RichOS's own transcription pipeline (richos/tools/richos-service): its ffmpeg
# normalize step, its whisper.cpp model and decode parameters, and its
# repetition, deletion and substitution guards. Everything it needs is already
# installed on this Mac. It never downloads anything.
#
#   transcribe.sh <audio-or-video-file> <output-dir> [--lang <code>|auto]
#
# Writes:
#   <output-dir>/transcript.md       the transcript, one "**[mm:ss] Me:** text" paragraph per turn
#   <output-dir>/pipeline/<id>/      the pipeline's own session: verification.json (coverage and
#                                    guard warnings), session.json (model, whisper build, hashes),
#                                    whisper JSON with per-word times, the 16 kHz WAVs
#   <output-dir>/pipeline.log        the pipeline's log
#
# Exit: 0 transcript written; 1 usage or a missing tool; 2 the pipeline refused
# or failed (its reason is printed); 3 the file has no audio track.
#
# Why the input is downmixed to mono first: the pipeline's two-channel contract
# means LEFT = me, RIGHT = everyone else, which is true of a RichOS call capture
# and false of an ordinary recording, whose two channels are one mix. Mono takes
# the pipeline's own single-channel path, so every word is decoded once and is
# labeled "Me" (that label means "the recording", not a speaker).
set -euo pipefail

usage() {
    echo "usage: transcribe.sh <audio-or-video-file> <output-dir> [--lang <code>|auto]" >&2
    exit 1
}

[ "$#" -ge 2 ] || usage
INPUT="$1"
OUT="$2"
shift 2
LANG_CODE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --lang) [ "$#" -ge 2 ] || usage; LANG_CODE="$2"; shift 2 ;;
        *) usage ;;
    esac
done

[ -f "$INPUT" ] || { echo "transcribe: no such file: $INPUT" >&2; exit 1; }

# This script lives in <richos>/engine/skills/transcribe-audio/, and the
# engine is usually reached through the ~/.claude/richos-engine symlink, so
# resolve the physical directory before walking up to the pipeline.
HERE="$(cd -P "$(dirname "$0")" && pwd -P)"
SERVICE="$HERE/../../../tools/richos-service/bin/richos-service.js"
[ -f "$SERVICE" ] || { echo "transcribe: RichOS pipeline not found at $SERVICE" >&2; exit 1; }
SERVICE="$(cd -P "$(dirname "$SERVICE")" && pwd -P)/richos-service.js"

for tool in ffmpeg ffprobe whisper-cli node; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "transcribe: $tool is not on PATH. It is installed on this Mac under /opt/homebrew/bin;" >&2
        echo "transcribe: fix PATH, never install or download anything. Report it to the lead." >&2
        exit 1
    fi
done

# The models are installed in the account's own ~/Models/Whisper. The pipeline
# searches under $HOME, and $HOME is not always the account's home: the merge
# gate and other sandboxes give each run a scratch HOME, where the pipeline then
# finds no model at all. So name the installed folder by the account's home from
# the user database, unless the caller already chose a model or a folder.
if [ -z "${RICHOS_WHISPER_MODEL:-}" ] && [ -z "${RICHOS_MODEL_DIR:-}" ]; then
    ACCOUNT_HOME="$(node -p 'require("os").userInfo().homedir')"
    if [ -d "$ACCOUNT_HOME/Models/Whisper" ]; then
        export RICHOS_MODEL_DIR="$ACCOUNT_HOME/Models/Whisper"
    fi
fi

if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT")" ]; then
    echo "transcribe: output directory is not empty: $OUT (give a new or empty one)" >&2
    exit 1
fi
mkdir -p "$OUT"
OUT="$(cd -P "$OUT" && pwd -P)"

if [ -z "$(ffprobe -v error -select_streams a -show_entries stream=index -of csv=p=0 "$INPUT")" ]; then
    echo "transcribe: $INPUT has no audio track; there is nothing to transcribe" >&2
    exit 3
fi
DURATION="$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$INPUT")"
DURATION_MS="$(printf '%.0f' "$(echo "$DURATION * 1000" | bc -l)")"

BASE="$(basename "$INPUT")"
STEM="$(printf '%s' "${BASE%.*}" | tr -c 'A-Za-z0-9._-' '-' | cut -c1-60)"
SESSION_ID="$(date -u +%Y-%m-%dT%H-%M-%SZ)--file--$STEM"
ZONE="$OUT/pipeline"
SESSION="$ZONE/$SESSION_ID"
mkdir -p "$SESSION"
chmod 700 "$OUT" "$ZONE" "$SESSION"

ffmpeg -hide_banner -nostdin -loglevel error -y -i "$INPUT" -map 0:a:0 -vn \
    -ac 1 -ar 16000 -c:a pcm_s16le "$SESSION/audio-part-00.wav"
BYTES="$(wc -c < "$SESSION/audio-part-00.wav" | tr -d ' ')"
NOW_MS="$(($(date +%s) * 1000))"

node -e '
const [file, id, label, now, dur, bytes] = process.argv.slice(1);
const rec = {
  schemaVersion: 1, sessionId: id, dir: id, status: "closed",
  startedAt: Number(now), endedAt: Number(now) + Number(dur),
  platform: { id: "file", label },
  capture: {
    source: "audio-file", method: "file", captureTarget: "file",
    container: "audio/wav",
    channels: { left: "the whole recording, downmixed to mono", right: "none" },
  },
  audio: { parts: [{ part: 0, bytes: Number(bytes) }], bytesTotal: Number(bytes), chunkCount: 1 },
  health: { redSeconds: 0 },
  captions: { count: 0 },
};
require("fs").writeFileSync(file, JSON.stringify(rec, null, 2) + "\n", { mode: 0o600 });
' "$SESSION/session.json" "$SESSION_ID" "$BASE" "$NOW_MS" "$DURATION_MS" "$BYTES"

LOG="$OUT/pipeline.log"
echo "transcribe: $BASE (${DURATION}s) -> RichOS pipeline; log $LOG" >&2
STATUS=0
if [ -n "$LANG_CODE" ]; then
    RICHOS_WHISPER_LANG="$LANG_CODE" node "$SERVICE" run "$SESSION" --zone "$ZONE" > "$LOG" 2>&1 || STATUS=$?
else
    node "$SERVICE" run "$SESSION" --zone "$ZONE" > "$LOG" 2>&1 || STATUS=$?
fi

if [ "$STATUS" -ne 0 ] || [ ! -f "$SESSION/transcript.md" ]; then
    echo "transcribe: the pipeline did not produce a transcript (exit $STATUS). Its log ends:" >&2
    tail -n 20 "$LOG" >&2
    echo "transcribe: do not download or install anything; report this output to the lead." >&2
    exit 2
fi

cp "$SESSION/transcript.md" "$OUT/transcript.md"
grep -m1 'READY' "$LOG" >&2 || true
echo "$OUT/transcript.md"

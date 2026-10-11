#!/usr/bin/env bash
# watch.sh — "watch" a video or screen recording: a timestamped transcript of its
# sound (through the audio-transcription skill's script, i.e. RichOS's own
# pipeline) plus still frames at every scene change and at least one frame every
# N seconds while the screen does not change, and one time-ordered index that
# puts every frame beside the words spoken at that moment. Read the index, then
# Read the frame files it names. Everything is already installed; nothing is
# ever downloaded.
#
#   watch.sh <video-file-or-url> <output-dir> [--every <seconds>] [--budget <N>]
#            [--sheet <frames-per-sheet>] [--height <pixels>] [--lang <code>|auto]
#   frames-at.sh <video-file> <output-dir> --at <time>[,<time>...]
#   frames-at.sh <video-file> <output-dir> --window <start> <end> [--step <seconds>]
#
# frames-at.sh (a sibling script) cuts frames at chosen times or inside a window of
# a file already watched, so a long video is read as transcript first, then zoomed.
#
# Writes:
#   <output-dir>/watched.md            THE INDEX: frames and transcript lines in time order
#   <output-dir>/frames/frame-NNNN-tMMmSS.Ss.png   one PNG per kept moment, at most 1920 px wide
#   <output-dir>/sheets/sheet-NN-tMMmSS.Ss.jpg   (--sheet N only) N frames tiled in one image
#   <output-dir>/video.<ext>         (a URL only) the downloaded video, for frames-at.sh
#   <output-dir>/transcript.md       the transcript (see the audio-transcription skill)
#   <output-dir>/pipeline/, pipeline.log   the transcription pipeline's own record
#
# Exit: 0 frames written (and the transcript, if the file has sound); 1 usage or
# a missing tool; 2 frames written but the transcription pipeline failed (its
# reason is printed and is in watched.md); 4 the frame step was stopped again and
# again on one stretch (no index; the transcript is kept; see the message).
#
# THE PARAMETERS, AND WHY
#   Scene changes are judged on the video sampled at 5 frames a second, so an
#   animated transition (a panel sliding in over 12 frames at 60 fps) adds up to
#   one jump instead of twelve small ones that each fall under the threshold.
#   SCENE=0.05: on the CEO's 141 s screen recording of 2026-10-07 the frame-to-frame
#   scene score was 0.10-0.30 when a panel opened or the view changed, and under
#   0.01 for typing and cursor moves; 0.05 keeps the first and skips the second.
#   GAP=1 s: one transition fires several scores over 0.05 in a row (33.0, 33.2,
#   33.4, 34.2, 34.4 s in that recording); one frame per second of it is enough.
#   EVERY=5 s (default): changes below the threshold (text typed into a field, a
#   slow scroll, the pointer showing what the speaker means) are never more than
#   5 s stale, which is about one spoken sentence (150 words a minute is 12 words
#   in 5 s), so every transcript line has a frame from inside its own sentence.
#   The floor alone costs at most 12 frames a minute, but scene changes come ON TOP of
#   it and --every does not limit them: the CEO's 8.1-minute edited video of 2026-10-09
#   gave 153 frames (110 of them scene changes), 18.8 a minute. To cap the count pass
#   --budget N: the video's time is cut into N equal stretches and each keeps its
#   biggest scene change (the first frame always), so the kept ones cover the whole video,
#   a long static stretch included. --sheet N tiles N frames into one image so a long video can
#   be skimmed in a few images; the full frames stay. URL input is downloaded by the
#   installed yt-dlp at --height (default 720: on-screen text stays readable, files stay
#   small), never installed or fetched any other way.
set -euo pipefail

usage() {
    echo "usage: watch.sh <video-file-or-url> <output-dir> [--every <seconds>] [--budget <N>] [--sheet <frames-per-sheet>] [--height <pixels>] [--lang <code>|auto]" >&2
    exit 1
}

SCENE=0.05
GAP=1
EVERY=5
BUDGET=0
SHEET=0
HEIGHT=720
[ "$#" -ge 2 ] || usage
INPUT="$1"
OUT="$2"
shift 2
LANG_ARGS=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --every) [ "$#" -ge 2 ] || usage; EVERY="$2"; shift 2 ;;
        --budget) [ "$#" -ge 2 ] || usage; BUDGET="$2"; shift 2 ;;
        --sheet) [ "$#" -ge 2 ] || usage; SHEET="$2"; shift 2 ;;
        --height) [ "$#" -ge 2 ] || usage; HEIGHT="$2"; shift 2 ;;
        --lang) [ "$#" -ge 2 ] || usage; LANG_ARGS=(--lang "$2"); shift 2 ;;
        *) usage ;;
    esac
done
case "$EVERY" in ''|*[!0-9.]*) echo "watch: --every takes a number of seconds" >&2; exit 1 ;; esac
case "$BUDGET" in ''|*[!0-9]*) echo "watch: --budget takes a whole number of frames" >&2; exit 1 ;; esac
case "$SHEET" in ''|*[!0-9]*) echo "watch: --sheet takes a whole number of frames per sheet" >&2; exit 1 ;; esac
case "$HEIGHT" in ''|*[!0-9]*) echo "watch: --height takes a number of pixels" >&2; exit 1 ;; esac
if [ "$SHEET" -gt 24 ]; then echo "watch: --sheet takes at most 24 frames per sheet (more is not legible)" >&2; exit 1; fi

HERE="$(cd -P "$(dirname "$0")" && pwd -P)"
TRANSCRIBE="$HERE/../audio-transcription/transcribe.sh"
[ -x "$TRANSCRIBE" ] || { echo "watch: transcribe script not found at $TRANSCRIBE" >&2; exit 1; }
for tool in ffmpeg ffprobe python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "watch: $tool is not on PATH. It is installed under /opt/homebrew/bin; fix PATH," >&2
        echo "watch: never install or download anything. Report it to the lead." >&2
        exit 1
    fi
done
if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT")" ]; then
    echo "watch: output directory is not empty: $OUT (give a new or empty one)" >&2
    exit 1
fi

# ---- 0. a URL is downloaded first, by the installed yt-dlp, into a scratch directory
# (the output directory must stay empty until the transcript is made), then moved
# into the output directory as video.<ext> so frames-at.sh can zoom into it later.
SOURCE_URL=""
DL=""
cleanup() { if [ -n "$DL" ]; then rm -rf "$DL"; fi; }
trap cleanup EXIT
case "$INPUT" in
    http://*|https://*)
        command -v yt-dlp >/dev/null 2>&1 || {
            echo "watch: yt-dlp is not on PATH (installed at /opt/homebrew/bin/yt-dlp); never install anything, report it to the lead" >&2
            exit 1
        }
        SOURCE_URL="$INPUT"
        DL="$(mktemp -d "${TMPDIR:-/tmp}/watch-dl.XXXXXX")"
        echo "watch: downloading $SOURCE_URL at up to ${HEIGHT}p" >&2
        yt-dlp --no-playlist --no-progress --quiet --no-warnings \
            -f "bv*[height<=$HEIGHT]+ba/b[height<=$HEIGHT]" --merge-output-format mp4 \
            -o "$DL/video.%(ext)s" "$SOURCE_URL" >&2 || { echo "watch: download failed (or nothing at or below ${HEIGHT}p is offered; try --height): $SOURCE_URL" >&2; exit 1; }
        INPUT="$(ls "$DL"/video.* 2>/dev/null | head -1)"
        [ -n "$INPUT" ] || { echo "watch: yt-dlp wrote no video file" >&2; exit 1; }
        ;;
esac

[ -f "$INPUT" ] || { echo "watch: no such file: $INPUT" >&2; exit 1; }
if [ -z "$(ffprobe -v error -select_streams v -show_entries stream=index -of csv=p=0 "$INPUT")" ]; then
    echo "watch: $INPUT has no video track; use the audio-transcription skill instead" >&2
    exit 1
fi
mkdir -p "$OUT"
OUT="$(cd -P "$OUT" && pwd -P)"
FRAMES="$OUT/frames"

# ---- 1. the transcript (exit 3 = no audio track, which is fine for a silent recording)
TSTATUS=0
TNOTE=""
if [ "${#LANG_ARGS[@]}" -gt 0 ]; then
    "$TRANSCRIBE" "$INPUT" "$OUT" "${LANG_ARGS[@]}" > /dev/null || TSTATUS=$?
else
    "$TRANSCRIBE" "$INPUT" "$OUT" > /dev/null || TSTATUS=$?
fi
case "$TSTATUS" in
    0) ;;
    3) TNOTE="This recording has no audio track, so there is no transcript." ;;
    *) TNOTE="TRANSCRIPTION FAILED (exit $TSTATUS); see $OUT/pipeline.log. Frames are still below." ;;
esac
# The header's "LEFT = me, RIGHT = others" line describes the pipeline's call mode;
# a file is downmixed to one channel, so say what is true instead.
if [ -f "$OUT/transcript.md" ]; then
    sed -i '' -e 's/^- \*\*Speaker attribution:\*\* LEFT channel.*$/- **Speaker attribution:** none - one channel, speakers are not separated ("Me" is the recording'"'"'s sound)/' "$OUT/transcript.md"
fi
# frames/ is made only now: transcribe.sh refuses a directory that is not empty.
mkdir -p "$FRAMES"
if [ -n "$DL" ]; then
    mv "$INPUT" "$OUT/$(basename "$INPUT")"
    INPUT="$OUT/$(basename "$INPUT")"
fi

# ---- 2. the frames
# select keeps a frame when it is the first one, when the scene changed by more
# than SCENE at least GAP seconds after the last kept frame, or when EVERY
# seconds passed without a kept frame. metadata prints what was kept, in order.
SELECT="isnan(prev_selected_t)+gte(t-prev_selected_t\\,$EVERY)+gt(scene\\,$SCENE)*gte(t-prev_selected_t\\,$GAP)"
#
# THE FRAME STEP IS PACED AND RESUMABLE (2026-10-10). On a 41-minute 1080p video ffmpeg at
# its default thread count ran far above 3 cores, and the host's CPU circuit breaker
# (cpu_guard.py: one process over 3 cores for 10 s is stopped) killed it at 17:02, so the
# script died with ffmpeg's 255 and no index. Now ffmpeg runs under nice with 2 decoder
# and 2 filter threads, which keeps it under what the breaker allows, and the video is cut
# in stretches of SEG seconds, each its own ffmpeg: a stretch that is killed or refused is
# redone alone after RETRY_WAIT seconds (up to RETRIES more times), never the whole video.
# Each stretch starts with a kept frame (the selector has no state before its first frame).
SEG="${WATCH_SEGMENT_SECONDS:-300}"
RETRIES="${WATCH_FRAME_RETRIES:-4}"
RETRY_WAIT="${WATCH_RETRY_SECONDS:-10}"
for v in "$SEG" "$RETRIES" "$RETRY_WAIT"; do
    case "$v" in ''|*[!0-9]*) echo "watch: WATCH_SEGMENT_SECONDS, WATCH_FRAME_RETRIES and WATCH_RETRY_SECONDS take whole numbers" >&2; exit 1 ;; esac
done
[ "$SEG" -gt 0 ] || { echo "watch: WATCH_SEGMENT_SECONDS must be above 0" >&2; exit 1; }
VDURATION="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$INPUT" 2>/dev/null | head -1 || true)"
case "$VDURATION" in ''|*[!0-9.]*) VDURATION="" ;; esac
# Each stretch (and each redo) is admitted by the engine's CPU admission first
# (admit.py beside this script: the engine's cpu_policy over a cpu_guard CPU sample, refuses
# at 80% or more total CPU) and waits up to WATCH_ADMISSION_WAIT seconds for the Mac to quiet. A refusal after the wait
# (exit 75) is handled as a stopped stretch: redone after RETRY_WAIT, up to RETRIES times.
# WATCH_RESERVE names another admission command (the test suite's stand-in).
RESERVE="${WATCH_RESERVE:-$HERE/admit.py}"
ADMIT_WAIT="${WATCH_ADMISSION_WAIT:-600}"
case "$ADMIT_WAIT" in ''|*[!0-9]*) echo "watch: WATCH_ADMISSION_WAIT takes whole seconds" >&2; exit 1 ;; esac
ADMIT=()
if [ -x "$RESERVE" ]; then ADMIT=("$RESERVE" --wait "$ADMIT_WAIT" --)
elif [ -f "$RESERVE" ]; then ADMIT=(python3 "$RESERVE" --wait "$ADMIT_WAIT" --)
else echo "watch: the CPU admission command is not beside this engine ($RESERVE); the frame step runs without it" >&2
fi
SEGDIR="$FRAMES/.seg"
NSEG=0
SEG_START=()
start=0
while :; do
    segd="$SEGDIR/$(printf '%04d' "$NSEG")"
    attempt=0
    while :; do
        rm -rf "$segd"; mkdir -p "$segd"
        FF_IN=()
        if [ "$start" -gt 0 ]; then FF_IN+=(-ss "$start"); fi
        if [ -n "$VDURATION" ]; then FF_IN+=(-t "$SEG"); fi
        rc=0
        ${ADMIT[@]+"${ADMIT[@]}"} nice -n 10 ffmpeg -hide_banner -nostdin -nostats -loglevel error -y -filter_threads 2 \
            ${FF_IN[@]+"${FF_IN[@]}"} -threads 2 -i "$INPUT" -an \
            -vf "fps=5,select='$SELECT',metadata=mode=print:key=lavfi.scene_score:file='$segd/kept.txt',scale='min(1920,iw)':-2" \
            -fps_mode passthrough "$segd/raw-%04d.png" || rc=$?
        [ "$rc" -eq 0 ] && break
        attempt=$((attempt + 1))
        if [ "$attempt" -gt "$RETRIES" ]; then
            echo "watch: the frame step failed $attempt times in a row at ${start}s of the video (ffmpeg exit $rc); the host may be too busy or is stopping ffmpeg." >&2
            echo "watch: the transcript is kept at $OUT/transcript.md. Run again when the Mac is quieter, or cut chosen frames with $HERE/frames-at.sh." >&2
            rm -rf "$SEGDIR"
            exit 4
        fi
        echo "watch: the frame step stopped at ${start}s (ffmpeg exit $rc); redoing this ${SEG}s stretch in ${RETRY_WAIT}s (try $attempt of $RETRIES)" >&2
        sleep "$RETRY_WAIT"
    done
    SEG_START[NSEG]="$start"
    NSEG=$((NSEG + 1))
    start=$((start + SEG))
    if [ -z "$VDURATION" ] || [ "$(echo "$start >= $VDURATION" | bc -l)" = 1 ]; then break; fi
done

# ---- 3. name each frame by its time, and build the index
fmt_time() {  # seconds (float) -> MMmSS.Ss
    local tenths m s
    tenths="$(printf '%.0f' "$(echo "$1 * 10" | bc -l)")"
    m=$((tenths / 600))
    s=$((tenths % 600))
    printf '%02dm%02d.%ds' "$m" "$((s / 10))" "$((s % 10))"
}

# 3a. read what ffmpeg kept: time, score and the reason, in order.
TOTAL=0
prev=""
t=""
T_OF=(); SCORE_OF=(); WHY_OF=(); RAW_OF=()
sg=0
while [ "$sg" -lt "$NSEG" ]; do
    segd="$SEGDIR/$(printf '%04d' "$sg")"
    off="${SEG_START[sg]}"
    k=0
    if [ -f "$segd/kept.txt" ]; then
        while IFS= read -r line; do
            case "$line" in
                frame:*)
                    t="$(printf '%s\n' "$line" | sed -E 's/.*pts_time:([0-9.]+).*/\1/')"
                    t="$(awk -v a="$t" -v b="$off" 'BEGIN { printf "%.6f", a + b }')" ;;
                lavfi.scene_score=*)
                    score="${line#lavfi.scene_score=}"
                    TOTAL=$((TOTAL + 1))
                    k=$((k + 1))
                    if [ -z "$prev" ]; then why="start"
                    elif [ "$k" -eq 1 ]; then why="start of a ${SEG}s stretch"
                    elif [ "$(echo "$score > $SCENE && $t - $prev >= $GAP" | bc -l)" = 1 ]; then why="scene change ($score)"
                    else why="no change for ${EVERY}s"
                    fi
                    T_OF[TOTAL]="$t"; SCORE_OF[TOTAL]="$score"; WHY_OF[TOTAL]="$why"
                    RAW_OF[TOTAL]="$segd/raw-$(printf '%04d' "$k").png"
                    prev="$t" ;;
            esac
        done < "$segd/kept.txt"
    fi
    sg=$((sg + 1))
done

# 3b. a budget keeps only BUDGET frames: the video's TIME is cut into BUDGET equal
# stretches and each stretch keeps its biggest scene change (the first frame always
# counts as the biggest), so a flood of scene changes cannot swamp the index and a long
# static stretch still gets a frame; the kept frames cover the whole video.
KEEP_OF=()
i=1
while [ "$i" -le "$TOTAL" ]; do KEEP_OF[i]=1; i=$((i + 1)); done
if [ "$BUDGET" -gt 0 ] && [ "$TOTAL" -gt "$BUDGET" ]; then
    DURATION="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$INPUT" 2>/dev/null | head -1 || true)"
    case "$DURATION" in ''|*[!0-9.]*) DURATION="" ;; esac
    # no usable length: fall back to just past the last kept frame
    if [ -z "$DURATION" ] || [ "$(echo "$DURATION <= ${T_OF[TOTAL]}" | bc -l)" = 1 ]; then
        DURATION="$(echo "${T_OF[TOTAL]} + 0.2" | bc -l)"
    fi
    i=1
    while [ "$i" -le "$TOTAL" ]; do KEEP_OF[i]=0; i=$((i + 1)); done
    i=1
    while [ "$i" -le "$TOTAL" ]; do
        if [ "$i" -eq 1 ]; then printf '%d\t%s\t9.000000\n' "$i" "${T_OF[i]}"; else printf '%d\t%s\t%s\n' "$i" "${T_OF[i]}" "${SCORE_OF[i]}"; fi
        i=$((i + 1))
    done | awk -F '\t' -v dur="$DURATION" -v budget="$BUDGET" '
        # BUDGET equal stretches of time; the biggest score in each stretch (the earliest
        # on a tie) is kept, so the keepers spread over the whole video.
        { g = int($2 * budget / dur); if (g >= budget) g = budget - 1
          if (!(g in best) || $3 + 0 > bs[g] + 0) { best[g] = $1; bs[g] = $3 } }
        END { for (g in best) printf "x\t%d\n", best[g] }' > "$OUT/.top"
    while IFS="$(printf '\t')" read -r _ idx; do KEEP_OF[idx]=1; done < "$OUT/.top"
    rm -f "$OUT/.top"
fi

# 3c. name the kept frames in order; drop the rest.
ROWS="$OUT/.rows"
: > "$ROWS"
n=0
FILES=(); FSTAMP=()
i=1
while [ "$i" -le "$TOTAL" ]; do
    raw="${RAW_OF[i]}"
    if [ "${KEEP_OF[i]}" = 1 ]; then
        n=$((n + 1))
        stamp="$(fmt_time "${T_OF[i]}")"
        name="frame-$(printf '%04d' "$n")-t$stamp.png"
        mv "$raw" "$FRAMES/$name"
        printf '%010.2f\t[%s] FRAME %s  (%s)\n' "${T_OF[i]}" "$stamp" "$FRAMES/$name" "${WHY_OF[i]}" >> "$ROWS"
        FILES[n]="$FRAMES/$name"; FSTAMP[n]="$stamp"
    else
        rm -f "$raw"
    fi
    i=$((i + 1))
done

rm -rf "$SEGDIR"

# 3d. contact sheets: SHEET frames tiled in one JPEG (ffmpeg's tile filter); the full
# frames stay where they are.
NSHEETS=0
SHEET_FAILED=0
if [ "$SHEET" -gt 0 ] && [ "$n" -gt 0 ]; then
    mkdir -p "$OUT/sheets"
    cols=1
    while [ $((cols * cols)) -lt "$SHEET" ]; do cols=$((cols + 1)); done
    first=1
    while [ "$first" -le "$n" ]; do
        last=$((first + SHEET - 1)); if [ "$last" -gt "$n" ]; then last="$n"; fi
        count=$((last - first + 1))
        c=$cols; if [ "$count" -lt "$c" ]; then c="$count"; fi
        r=$(( (count + c - 1) / c ))
        NSHEETS=$((NSHEETS + 1))
        sheet="$OUT/sheets/sheet-$(printf '%02d' "$NSHEETS")-t${FSTAMP[first]}.jpg"
        list="$OUT/sheets/.list"
        : > "$list"
        k=$first
        while [ "$k" -le "$last" ]; do
            printf "file '%s'\nduration 1\n" "${FILES[k]}" >> "$list"
            k=$((k + 1))
        done
        printf "file '%s'\n" "${FILES[last]}" >> "$list"
        if ! ffmpeg -hide_banner -nostdin -nostats -loglevel error -y -f concat -safe 0 -i "$list" \
            -vf "scale=480:-2,tile=${c}x${r}:padding=4:margin=4" -frames:v 1 -q:v 3 "$sheet"; then
            # The frames and the index are what matter; a sheet is a convenience.
            rm -f "$sheet" "$list"
            NSHEETS=$((NSHEETS - 1))
            SHEET_FAILED=$((SHEET_FAILED + 1))
            first=$((last + 1))
            continue
        fi
        rm -f "$list"
        # The sheet sorts just before its first frame (C before F on a tie).
        tf="$(grep -F "FRAME ${FILES[first]}" "$ROWS" | cut -f1)"
        printf '%s\t[%s] CONTACT SHEET %s  (frames %d-%d of %d, left to right, top to bottom; the full frames are the FRAME rows)\n' \
            "$tf" "${FSTAMP[first]}" "$sheet" "$first" "$last" "$n" >> "$ROWS"
        first=$((last + 1))
    done
fi

# 3e. the transcript, one row per paragraph, keyed by its whole-second stamp.
if [ -f "$OUT/transcript.md" ]; then
    while IFS= read -r line; do
        case "$line" in
            '**['*) ;;
            *) continue ;;
        esac
        ts="${line#\*\*[}"
        ts="${ts%%]*}"
        IFS=: read -r a b c <<< "$ts"
        if [ -n "${c:-}" ]; then secs=$((10#$a * 3600 + 10#$b * 60 + 10#$c)); else secs=$((10#$a * 60 + 10#$b)); fi
        # The transcript's stamp is a whole second; .01 puts a frame taken at
        # exactly that second ahead of the words, so the picture comes first.
        printf '%010.2f\t%s\n' "$secs.01" "$line" >> "$ROWS"
    done < "$OUT/transcript.md"
fi

{
    echo "# Watch: $(basename "$INPUT")"
    echo
    if [ -n "$SOURCE_URL" ]; then echo "- Source: $SOURCE_URL (downloaded at up to ${HEIGHT}p)"; fi
    echo "- Video file: $INPUT (cut more frames from it with $HERE/frames-at.sh)"
    if [ "$n" -lt "$TOTAL" ]; then
        echo "- Frames: $n of $TOTAL kept by --budget $BUDGET, in $FRAMES (scene change over $SCENE, or every ${EVERY}s without one)"
    else
        echo "- Frames: $n in $FRAMES (scene change over $SCENE, or every ${EVERY}s without one)"
    fi
    if [ "$NSHEETS" -gt 0 ]; then echo "- Contact sheets: $NSHEETS in $OUT/sheets, $SHEET frames each; each sheet's row comes just before its first frame. Read a sheet to skim, then Read only the full FRAMEs you need."; fi
    if [ -f "$OUT/transcript.md" ]; then echo "- Transcript: $OUT/transcript.md (one channel, speakers not separated; \"Me\" is the recording's sound, not a speaker)"; fi
    if [ "$SHEET_FAILED" -gt 0 ]; then echo "- Contact sheet missing: $SHEET_FAILED of $((NSHEETS + SHEET_FAILED)) could not be made; every frame is still here as a FRAME row below."; fi
    if [ -n "$TNOTE" ]; then echo "- $TNOTE"; fi
    echo "- Read this file top to bottom; Read each FRAME path to see the screen at that moment."
    echo
    # By exact time; the row text breaks a tie (a contact sheet's C before its frame's F).
    LC_ALL=C sort -t "$(printf '\t')" -k1,1 -k2,2 "$ROWS" | cut -f2- | awk '{ print; print "" }'
} > "$OUT/watched.md"
rm -f "$ROWS"

if [ -n "$DL" ]; then rm -rf "$DL"; DL=""; fi
if [ "$NSHEETS" -gt 0 ]; then echo "watch: $n frames, $NSHEETS sheets, index $OUT/watched.md" >&2
else echo "watch: $n frames, index $OUT/watched.md" >&2; fi
if [ "$SHEET_FAILED" -gt 0 ]; then echo "watch: contact sheet missing: $SHEET_FAILED could not be made; the frames and the index are complete." >&2; fi
[ -n "$TNOTE" ] && echo "watch: $TNOTE" >&2
echo "$OUT/watched.md"
if [ "$TSTATUS" -ne 0 ] && [ "$TSTATUS" -ne 3 ]; then exit 2; fi
exit 0

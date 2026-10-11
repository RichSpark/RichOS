#!/usr/bin/env bash
# watch.test.sh — watch.sh keeps a frame at the start, at a scene change, and
# after 5 s without one; names each by its time; and interleaves frames with the
# transcript in watched.md. A recording with no sound still gets its frames.
#
# Fixture: 12 s of black with a white box appearing at 3.0 s, so the expected
# frames are exactly 0.0 s (start), 3.0 s (scene change) and 8.0 s (no change
# for 5 s). The speech is synthesized with `say -o` straight to a file: nothing
# is played through the speakers.
set -uo pipefail

HERE="$(cd -P "$(dirname "$0")" && pwd -P)"
ENGINE="$(cd -P "$HERE/../.." && pwd -P)"
SCRIPT="$HERE/watch.sh"
FAIL=0
ok() { printf '  ok    %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

echo "=== watch-video: watch.sh ==="
[ -x "$SCRIPT" ] || { bad "watch.sh exists and is executable at $SCRIPT"; echo "1 failure"; exit 1; }
command -v say >/dev/null 2>&1 || { bad "macOS say is available to make the speech fixture"; exit 1; }

. "$ENGINE/scripts/lib/scratch.sh"
SANDBOX="$(scratch_new watch-test --ttl 30)"
trap 'scratch_release "$SANDBOX"' EXIT

# The real admission waits while the Mac is busy, which would make this suite as slow as
# the host; every run but the admission test (6) uses a stand-in that admits at once.
printf '#!/usr/bin/env bash\nwhile [ "$1" != "--" ]; do shift; done\nshift\nexec "$@"\n' > "$SANDBOX/admit-now"
chmod +x "$SANDBOX/admit-now"
export WATCH_RESERVE="$SANDBOX/admit-now"

VIDEO_FILTER="drawbox=x=40:y=40:w=240:h=160:color=white:t=fill:enable='gte(t,3)'"
say -o "$SANDBOX/speech.aiff" "First the screen is empty. Now a white box appears on the screen."
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=black:s=320x240:r=30:d=12" \
    -i "$SANDBOX/speech.aiff" -vf "$VIDEO_FILTER" -c:v libx264 -c:a aac -t 12 "$SANDBOX/rec.mov"
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=black:s=320x240:r=30:d=12" \
    -vf "$VIDEO_FILTER" -c:v libx264 "$SANDBOX/mute.mov"

# 1. a recording with sound
OUT="$SANDBOX/out1"
# An empty HOME, as the merge gate gives every unit: the installed models must
# still be found, so the script cannot be leaning on HOME being the real one.
mkdir -p "$SANDBOX/home"
PRINTED="$(HOME="$SANDBOX/home" "$SCRIPT" "$SANDBOX/rec.mov" "$OUT" 2>"$SANDBOX/err1")"
STATUS=$?
if [ "$STATUS" -eq 0 ]; then ok "exit 0"; else bad "exit 0 (got $STATUS: $(tail -5 "$SANDBOX/err1"))"; fi
if [ "$PRINTED" = "$OUT/watched.md" ] && [ -f "$OUT/watched.md" ]; then ok "prints the index path"; else bad "prints the index path (printed: $PRINTED)"; fi
FRAMES="$(cd "$OUT/frames" 2>/dev/null && ls | tr '\n' ' ')"
EXPECT="frame-0001-t00m00.0s.png frame-0002-t00m03.0s.png frame-0003-t00m08.0s.png "
if [ "$FRAMES" = "$EXPECT" ]; then
    ok "frames at start, at the scene change and 5 s later, named by time"
else
    bad "frames at start, at the scene change and 5 s later (got: $FRAMES)"
fi
if grep -q 'frame-0001-t00m00.0s.png  (start)' "$OUT/watched.md" \
    && grep -q 'frame-0002-t00m03.0s.png  (scene change' "$OUT/watched.md" \
    && grep -q 'frame-0003-t00m08.0s.png  (no change for 5s)' "$OUT/watched.md"; then
    ok "watched.md says why each frame was kept"
else
    bad "watched.md says why each frame was kept"
fi
if grep -iq 'white box' "$OUT/transcript.md" 2>/dev/null; then ok "transcript has the spoken words"; else bad "transcript has the spoken words"; fi
FIRST_WORDS="$(grep -n '^\*\*\[' "$OUT/watched.md" | head -1 | cut -d: -f1)"
FIRST_FRAME="$(grep -n 'FRAME' "$OUT/watched.md" | head -1 | cut -d: -f1)"
LAST_FRAME="$(grep -n 'FRAME' "$OUT/watched.md" | tail -1 | cut -d: -f1)"
if [ -n "$FIRST_WORDS" ] && [ "$FIRST_FRAME" -lt "$FIRST_WORDS" ] && [ "$FIRST_WORDS" -lt "$LAST_FRAME" ]; then
    ok "watched.md interleaves frames and transcript in time order"
else
    bad "watched.md interleaves frames and transcript in time order"
fi
if file "$OUT/frames/frame-0002-t00m03.0s.png" | grep -q 'PNG image data, 320 x 240'; then
    ok "frames are full-size PNGs the Read tool can open"
else
    bad "frames are full-size PNGs the Read tool can open"
fi

# 2. a recording with no sound: frames, no transcript, said in watched.md
OUT2="$SANDBOX/out2"
"$SCRIPT" "$SANDBOX/mute.mov" "$OUT2" >/dev/null 2>&1
STATUS=$?
COUNT="$(ls "$OUT2/frames" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$STATUS" -eq 0 ] && [ "$COUNT" = 3 ] && grep -q 'no audio track' "$OUT2/watched.md" 2>/dev/null; then
    ok "a silent recording gets its 3 frames and watched.md says there is no transcript"
else
    bad "a silent recording gets its frames (exit $STATUS, $COUNT frames)"
fi

# 3. frames at chosen times and in a window (frames-at.sh), on the silent fixture
# whose box appears at 3.0 s: black before, white-box after.
ATSH="$HERE/frames-at.sh"
mean_luma() {  # mean luma (0-255, integer) of an image
    ffmpeg -hide_banner -nostdin -loglevel info -i "$1" -vf "signalstats,metadata=print:key=lavfi.signalstats.YAVG" -f null - 2>&1 \
        | sed -n 's/.*YAVG=\([0-9]*\).*/\1/p' | head -1
}
if [ -x "$ATSH" ]; then
    STATUS=0
    PA="$("$ATSH" "$SANDBOX/mute.mov" "$SANDBOX/at1" --at 1,00:05 2>/dev/null)" || STATUS=$?
    L1="$(mean_luma "$SANDBOX/at1/frame-at-t00m01.000s.png" 2>/dev/null)"
    L5="$(mean_luma "$SANDBOX/at1/frame-at-t00m05.000s.png" 2>/dev/null)"
    if [ "$STATUS" -eq 0 ] && [ "$(printf '%s\n' "$PA" | wc -l | tr -d ' ')" = 2 ] \
        && [ -n "$L1" ] && [ -n "$L5" ] && [ "$L1" -lt 20 ] && [ "$L5" -gt 10 ]; then
        ok "frames-at --at cuts the frames at 1 s (black) and 00:05 (white box) and prints their paths"
    else
        bad "frames-at --at (exit $STATUS, luma at 1 s: ${L1:-none}, at 5 s: ${L5:-none})"
    fi
    STATUS=0
    "$ATSH" "$SANDBOX/mute.mov" "$SANDBOX/win1" --window 2 6 --step 2 >/dev/null 2>&1 || STATUS=$?
    WN="$(ls "$SANDBOX/win1" 2>/dev/null | tr '\n' ' ')"
    if [ "$STATUS" -eq 0 ] && [ "$WN" = "frame-at-t00m02.000s.png frame-at-t00m04.000s.png frame-at-t00m06.000s.png " ]; then
        ok "frames-at --window 2 6 --step 2 cuts three frames"
    else
        bad "frames-at --window (exit $STATUS, got: $WN)"
    fi
    if "$ATSH" "$SANDBOX/mute.mov" "$SANDBOX/late1" --at 99 >/dev/null 2>&1; then
        bad "frames-at refuses a time past the end of the video"
    else
        ok "frames-at refuses a time past the end of the video"
    fi
else
    bad "frames-at.sh exists and is executable (frames at chosen times)"
fi

# 4. a frame budget: a video with a scene change every second floods the index
# (12 s, a box blinking each second); --budget 4 keeps exactly 4, the first among them.
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=black:s=320x240:r=30:d=12" \
    -vf "drawbox=x=40:y=40:w=240:h=160:color=white:t=fill:enable='lt(mod(t,2),1)'" -c:v libx264 "$SANDBOX/busy.mov"
"$SCRIPT" "$SANDBOX/busy.mov" "$SANDBOX/busy-all" >/dev/null 2>&1 || true
ALL="$(ls "$SANDBOX/busy-all/frames" 2>/dev/null | wc -l | tr -d ' ')"
STATUS=0
"$SCRIPT" "$SANDBOX/busy.mov" "$SANDBOX/busy-b4" --budget 4 >/dev/null 2>&1 || STATUS=$?
B4="$(ls "$SANDBOX/busy-b4/frames" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$ALL" -gt 4 ] && [ "$STATUS" -eq 0 ] && [ "$B4" = 4 ] \
    && ls "$SANDBOX/busy-b4/frames" | head -1 | grep -q '^frame-0001-t00m00.0s.png$' \
    && ls "$SANDBOX/busy-b4/frames" | tail -1 | grep -Eq '^frame-0004-t00m(0[89]|1[01])\.' \
    && grep -q "Frames: 4 of $ALL kept by --budget 4" "$SANDBOX/busy-b4/watched.md" \
    && [ "$(grep -c '\] FRAME ' "$SANDBOX/busy-b4/watched.md")" = 4 ]; then
    ok "--budget 4 keeps 4 of $ALL frames, the first included, and the index says so"
else
    bad "--budget 4 keeps 4 of the flood (all: $ALL, budget: $B4, exit $STATUS)"
fi
# A budget cuts the video into equal stretches of TIME: 20 s of a box blinking each
# second, then 20 s of nothing, with --budget 4, must still keep frames from the static
# half (cut by equal runs of frames it kept one, the review's third finding).
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=black:s=320x240:r=30:d=40" \
    -vf "drawbox=x=40:y=40:w=240:h=160:color=white:t=fill:enable='lt(t,20)*lt(mod(t,2),1)'" -c:v libx264 "$SANDBOX/half.mov"
"$SCRIPT" "$SANDBOX/half.mov" "$SANDBOX/half-b4" --budget 4 >/dev/null 2>&1 || true
LATE="$(ls "$SANDBOX/half-b4/frames" 2>/dev/null | sed -n 's/^frame-[0-9]*-t\([0-9]*\)m\([0-9]*\)\..*/\1 \2/p' | awk '$1 * 60 + $2 >= 20' | wc -l | tr -d ' ')"
if [ "$LATE" -ge 2 ]; then ok "--budget 4 covers a long static stretch ($LATE of the kept frames are from its half)"; else bad "--budget 4 covers a long static stretch (kept in the static half: $LATE)"; fi
if "$SCRIPT" "$SANDBOX/busy.mov" "$SANDBOX/busy-bad" --budget x >/dev/null 2>&1; then bad "--budget refuses a non-number"; else ok "--budget refuses a non-number"; fi

# 5. contact sheets: --sheet 4 on 8 frames makes 2 sheets (JPEG), indexed before their
# frames, and the full frames are still all there.
STATUS=0
"$SCRIPT" "$SANDBOX/busy.mov" "$SANDBOX/busy-sh" --budget 8 --sheet 4 >/dev/null 2>&1 || STATUS=$?
NS="$(ls "$SANDBOX/busy-sh/sheets" 2>/dev/null | wc -l | tr -d ' ')"
NF="$(ls "$SANDBOX/busy-sh/frames" 2>/dev/null | wc -l | tr -d ' ')"
SHEET1="$(ls "$SANDBOX/busy-sh/sheets"/sheet-01-*.jpg 2>/dev/null | head -1)"
if [ "$STATUS" -eq 0 ] && [ "$NS" = 2 ] && [ "$NF" = 8 ] && [ -n "$SHEET1" ] \
    && file "$SHEET1" | grep -q 'JPEG image data' \
    && [ "$(grep -c 'CONTACT SHEET ' "$SANDBOX/busy-sh/watched.md")" = 2 ] \
    && [ "$(grep -n 'CONTACT SHEET ' "$SANDBOX/busy-sh/watched.md" | head -1 | cut -d: -f1)" -lt "$(grep -n '\] FRAME ' "$SANDBOX/busy-sh/watched.md" | head -1 | cut -d: -f1)" ]; then
    ok "--sheet 4 on 8 frames makes 2 JPEG sheets, indexed before their frames, with the 8 full frames kept"
else
    bad "--sheet 4 (exit $STATUS, sheets: $NS, frames: $NF)"
fi
W=""
if [ -n "$SHEET1" ]; then W="$(file "$SHEET1" | sed -n 's/.* \([0-9]*\)x[0-9]*,.*/\1/p' | head -1)"; fi
if [ -n "$W" ] && [ "$W" -gt 480 ]; then ok "a sheet holds several frames side by side (width $W px)"; else bad "a sheet holds several frames side by side (width: ${W:-unknown})"; fi

# 6. the index lists frames and paragraphs in one time order, and a file's transcript
# header no longer claims LEFT = me, RIGHT = others.
# Every row's time, frame or paragraph, in the order the index lists them, never goes back.
ORDERED="$(grep -E '^\[[0-9]{2}m[0-9.]+s\] FRAME |^\*\*\[[0-9:]+\] Me:' "$OUT/watched.md" | awk '
    /^\[/ { split(substr($1, 2), a, "m"); t = a[1] * 60 + a[2] + 0 }
    /^\*\*\[/ { x = $1; gsub(/[*\[\]]/, "", x); n = split(x, a, ":"); t = (n == 3) ? a[1] * 3600 + a[2] * 60 + a[3] : a[1] * 60 + a[2] }
    { if (t < prev) bad = 1; prev = t; rows++ }
    END { print (bad || rows < 4) ? "no" : "yes" }')"
if [ "$ORDERED" = yes ]; then ok "the index lists frames and paragraphs in time order"; else bad "the index lists frames and paragraphs in time order"; fi
if ! grep -q 'LEFT channel' "$OUT/transcript.md" && grep -q 'speakers not separated' "$OUT/watched.md"; then
    ok "a file's transcript no longer claims LEFT = me, RIGHT = others; the index says one channel"
else
    bad "the LEFT = me / RIGHT = others header line is gone for a file"
fi

# 7. a URL goes through yt-dlp at a readable height, then the usual pipeline. A
# stand-in yt-dlp (no network in a test) copies the silent fixture to the output
# template and records its arguments.
mkdir -p "$SANDBOX/fakebin"
cat > "$SANDBOX/fakebin/yt-dlp" <<FAKE
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "$SANDBOX/ytdlp.args"
while [ "\$#" -gt 0 ]; do
    if [ "\$1" = "-o" ]; then cp "$SANDBOX/mute.mov" "\${2/%.%(ext)s/.mp4}"; fi
    shift
done
FAKE
chmod +x "$SANDBOX/fakebin/yt-dlp"
STATUS=0
PATH="$SANDBOX/fakebin:$PATH" "$SCRIPT" "https://example.com/watch?v=abc" "$SANDBOX/url1" >/dev/null 2>"$SANDBOX/err7" || STATUS=$?
COUNT="$(ls "$SANDBOX/url1/frames" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$STATUS" -eq 0 ] && [ "$COUNT" = 3 ] && [ -f "$SANDBOX/url1/video.mp4" ] \
    && grep -q 'Source: https://example.com/watch?v=abc' "$SANDBOX/url1/watched.md" \
    && grep -q 'height<=720' "$SANDBOX/ytdlp.args" && grep -q -- '--no-playlist' "$SANDBOX/ytdlp.args"; then
    ok "a URL is downloaded by yt-dlp at up to 720p, kept as video.mp4, and watched like a file"
else
    bad "a URL as input (exit $STATUS, $COUNT frames: $(tail -3 "$SANDBOX/err7"))"
fi
PATH="$SANDBOX/fakebin:$PATH" "$SCRIPT" "https://example.com/v2" "$SANDBOX/url2" --height 480 >/dev/null 2>&1 || true
if grep -q 'height<=480' "$SANDBOX/ytdlp.args"; then ok "--height sets the download height"; else bad "--height sets the download height"; fi
if grep -q -- '-f ' "$SANDBOX/ytdlp.args" && ! grep -Eq -- '-f [^ ]*/b( |$)' "$SANDBOX/ytdlp.args"; then
    ok "the download format has no fallback without the height cap"
else
    bad "the download format has no fallback without the height cap: $(head -1 "$SANDBOX/ytdlp.args")"
fi

# 8. two chosen times that differ by less than a tenth keep two frames
STATUS=0
"$ATSH" "$SANDBOX/rec.mov" "$SANDBOX/at2" --at 4.96,5.04 >/dev/null 2>&1 || STATUS=$?
N2="$(ls "$SANDBOX/at2" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$STATUS" -eq 0 ] && [ "$N2" = 2 ]; then ok "--at 4.96,5.04 keeps two frames"; else bad "--at 4.96,5.04 keeps two frames (exit $STATUS, $N2 files)"; fi

# 9. a frame step the host stops partway (the CPU circuit breaker killed ffmpeg at 17:02 of a
# 41-minute video on 2026-10-10 and the script died with exit 255 and no index): the stopped
# stretch is redone alone and the run still finishes. A stand-in ffmpeg fails the frame-step
# calls named in FAKE_FAIL_CALLS (leaving a partial frame behind, exit 255) and passes every
# other call, and every transcription call, to the real ffmpeg.
REAL_FFMPEG="$(command -v ffmpeg)"
cat > "$SANDBOX/fakebin/ffmpeg" <<FAKE
#!/usr/bin/env bash
case "\$*" in
    *select=*)
        n=\$(( \$(cat "$SANDBOX/ff.count" 2>/dev/null || echo 0) + 1 ))
        echo "\$n" > "$SANDBOX/ff.count"
        if [ -n "\${FAKE_FAIL_ALWAYS:-}" ] || case " \$FAKE_FAIL_CALLS " in *" \$n "*) true ;; *) false ;; esac; then
            for last; do :; done
            : > "\${last/%%04d.png/0001.png}"
            exit 255
        fi ;;
esac
exec "$REAL_FFMPEG" "\$@"
FAKE
chmod +x "$SANDBOX/fakebin/ffmpeg"
rm -f "$SANDBOX/ff.count"
STATUS=0
PATH="$SANDBOX/fakebin:$PATH" FAKE_FAIL_CALLS="1 3" WATCH_SEGMENT_SECONDS=4 WATCH_RETRY_SECONDS=0 \
    "$SCRIPT" "$SANDBOX/mute.mov" "$SANDBOX/kill1" >/dev/null 2>"$SANDBOX/err9" || STATUS=$?
KF="$(ls "$SANDBOX/kill1/frames" 2>/dev/null | tr '\n' ' ')"
if [ "$STATUS" -eq 0 ] && [ "$(grep -c 'redoing this 4s stretch' "$SANDBOX/err9")" = 2 ] \
    && [ "$KF" = "frame-0001-t00m00.0s.png frame-0002-t00m03.0s.png frame-0003-t00m04.0s.png frame-0004-t00m08.0s.png " ] \
    && [ ! -e "$SANDBOX/kill1/frames/.seg" ] && [ -f "$SANDBOX/kill1/watched.md" ]; then
    ok "a frame step stopped twice partway is redone stretch by stretch and the run finishes with its index"
else
    bad "a stopped frame step is redone (exit $STATUS, frames: $KF; $(tail -3 "$SANDBOX/err9"))"
fi
rm -f "$SANDBOX/ff.count"
STATUS=0
PATH="$SANDBOX/fakebin:$PATH" FAKE_FAIL_ALWAYS=1 WATCH_FRAME_RETRIES=2 WATCH_RETRY_SECONDS=0 \
    "$SCRIPT" "$SANDBOX/mute.mov" "$SANDBOX/kill2" >/dev/null 2>"$SANDBOX/err9b" || STATUS=$?
if [ "$STATUS" -eq 4 ] && grep -q 'frame step failed 3 times' "$SANDBOX/err9b" && [ ! -e "$SANDBOX/kill2/watched.md" ]; then
    ok "a frame step that keeps being stopped exits 4 (in SKILL.md's table) with the reason, never 255"
else
    bad "a frame step that keeps failing exits 4 (exit $STATUS: $(tail -3 "$SANDBOX/err9b"))"
fi
# And it is paced: the frame step runs under nice with limited threads, so one ffmpeg stays
# under the breaker's 3 cores. The stand-in records its own arguments and niceness.
cat > "$SANDBOX/fakebin/ffmpeg" <<FAKE
#!/usr/bin/env bash
case "\$*" in *select=*) echo "nice=\$(ps -o nice= -p \$\$ | tr -d ' ') \$*" >> "$SANDBOX/ff.args" ;; esac
exec "$REAL_FFMPEG" "\$@"
FAKE
PATH="$SANDBOX/fakebin:$PATH" "$SCRIPT" "$SANDBOX/mute.mov" "$SANDBOX/paced1" >/dev/null 2>&1 || true
if grep -q '^nice=10 ' "$SANDBOX/ff.args" 2>/dev/null && grep -q -- '-threads 2' "$SANDBOX/ff.args" && grep -q -- '-filter_threads 2' "$SANDBOX/ff.args"; then
    ok "the frame step runs at nice 10 with 2 decoder and 2 filter threads"
else
    bad "the frame step is paced (recorded: $(head -c 200 "$SANDBOX/ff.args" 2>/dev/null))"
fi

# 6. CPU admission: every frame stretch is admitted first, and waits while the Mac is
# busy. A stand-in admission command (WATCH_RESERVE) is "busy" for 2 s on each ask; the
# ffmpeg shim logs when a stretch really starts. Order must be ASK, OPEN, START.
cat > "$SANDBOX/fake-reserve" <<FAKE
#!/usr/bin/env bash
echo ASK >> "$SANDBOX/adm.log"
sleep 2
echo OPEN >> "$SANDBOX/adm.log"
while [ "\$1" != "--" ]; do shift; done
shift
exec "\$@"
FAKE
chmod +x "$SANDBOX/fake-reserve"
cat > "$SANDBOX/fakebin/ffmpeg" <<FAKE
#!/usr/bin/env bash
case "\$*" in *select=*) echo START >> "$SANDBOX/adm.log" ;; esac
exec "$REAL_FFMPEG" "\$@"
FAKE
STATUS=0
WATCH_RESERVE="$SANDBOX/fake-reserve" PATH="$SANDBOX/fakebin:$PATH" "$SCRIPT" "$SANDBOX/mute.mov" "$SANDBOX/adm1" >/dev/null 2>&1 || STATUS=$?
if [ "$STATUS" -eq 0 ] && [ "$(tr '\n' ' ' < "$SANDBOX/adm.log" 2>/dev/null)" = "ASK OPEN START " ]; then
    ok "a frame stretch starts only after admission opens (busy for 2 s: ASK, OPEN, START)"
else
    bad "a frame stretch starts only after admission opens (exit $STATUS, log: $(tr '\n' ' ' < "$SANDBOX/adm.log" 2>/dev/null))"
fi

# 7. a failed contact-sheet step keeps the frames and the index and says the sheet is missing
cat > "$SANDBOX/fakebin/ffmpeg" <<FAKE
#!/usr/bin/env bash
case "\$*" in *tile=*) echo "forced sheet failure" >&2; exit 1 ;; esac
exec "$REAL_FFMPEG" "\$@"
FAKE
STATUS=0
PATH="$SANDBOX/fakebin:$PATH" "$SCRIPT" "$SANDBOX/mute.mov" "$SANDBOX/sheetfail" --sheet 2 >/dev/null 2>"$SANDBOX/sheetfail.err" || STATUS=$?
if [ "$STATUS" -eq 0 ] && [ "$(ls "$SANDBOX/sheetfail/frames" 2>/dev/null | wc -l | tr -d ' ')" = 3 ] \
    && grep -q 'FRAME' "$SANDBOX/sheetfail/watched.md" 2>/dev/null \
    && grep -qi 'contact sheet.*missing' "$SANDBOX/sheetfail/watched.md" 2>/dev/null \
    && grep -qi 'contact sheet.*missing' "$SANDBOX/sheetfail.err"; then
    ok "a failed contact sheet keeps the frames and the index, exits 0 and says the sheet is missing"
else
    bad "a failed contact sheet keeps the frames and the index (exit $STATUS, $(tail -2 "$SANDBOX/sheetfail.err" | tr '\n' ' '))"
fi

if [ "$FAIL" -eq 0 ]; then echo "PASS"; exit 0; fi
echo "$FAIL failure(s)"
exit 1

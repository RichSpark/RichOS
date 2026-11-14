#!/usr/bin/env bash
# transcribe.test.sh — transcribe.sh turns a spoken file into a timestamped
# transcript through the RichOS pipeline, says "no audio track" with exit 3, and
# refuses an output directory that is not empty.
#
# The speech is synthesized with `say -o` straight to a file: nothing is played
# through the speakers.
set -uo pipefail

HERE="$(cd -P "$(dirname "$0")" && pwd -P)"
ENGINE="$(cd -P "$HERE/../.." && pwd -P)"
SCRIPT="$HERE/transcribe.sh"
FAIL=0
ok() { printf '  ok    %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

echo "=== transcribe-audio: transcribe.sh ==="
[ -x "$SCRIPT" ] || { bad "transcribe.sh exists and is executable at $SCRIPT"; echo "1 failure"; exit 1; }
command -v say >/dev/null 2>&1 || { bad "macOS say is available to make the speech fixture"; exit 1; }

. "$ENGINE/scripts/lib/scratch.sh"
SANDBOX="$(scratch_new transcribe-test --ttl 30)"
trap 'scratch_release "$SANDBOX"' EXIT

# 1. speech in a video container -> transcript with a timestamped line
say -o "$SANDBOX/speech.aiff" "The quick brown fox jumps over the lazy dog. Please transcribe this sentence."
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=gray:s=320x240:d=8" \
    -i "$SANDBOX/speech.aiff" -c:v libx264 -c:a aac -shortest "$SANDBOX/speech.mp4"
OUT1="$SANDBOX/out1"
# An empty HOME, as the merge gate gives every unit: the installed models must
# still be found, so the script cannot be leaning on HOME being the real one.
mkdir -p "$SANDBOX/home"
PRINTED="$(HOME="$SANDBOX/home" "$SCRIPT" "$SANDBOX/speech.mp4" "$OUT1" 2>"$SANDBOX/err1")"
STATUS=$?
if [ "$STATUS" -eq 0 ]; then ok "exit 0 on a spoken file"; else bad "exit 0 on a spoken file (got $STATUS: $(tail -5 "$SANDBOX/err1"))"; fi
if [ "$PRINTED" = "$OUT1/transcript.md" ] && [ -f "$OUT1/transcript.md" ]; then
    ok "prints the transcript path and the file exists"
else
    bad "prints the transcript path and the file exists (printed: $PRINTED)"
fi
if grep -Eq '^\*\*\[00:0[0-9]\] Me:\*\* .*' "$OUT1/transcript.md" 2>/dev/null; then
    ok "transcript carries a timestamped paragraph"
else
    bad "transcript carries a timestamped paragraph"
fi
if grep -iq 'brown fox' "$OUT1/transcript.md" 2>/dev/null && grep -iq 'lazy dog' "$OUT1/transcript.md" 2>/dev/null; then
    ok "transcript has the spoken words"
else
    bad "transcript has the spoken words"
fi
if grep -q 'large-v3-turbo' "$OUT1/transcript.md" 2>/dev/null; then
    ok "decoded by the pipeline's large-v3-turbo model, already on disk"
else
    bad "decoded by the pipeline's large-v3-turbo model"
fi
if ls "$OUT1"/pipeline/*/verification.json >/dev/null 2>&1; then
    ok "the pipeline's verification.json is kept beside it"
else
    bad "the pipeline's verification.json is kept beside it"
fi

# 2. no audio track -> exit 3
ffmpeg -hide_banner -nostdin -loglevel error -y -f lavfi -i "color=c=gray:s=320x240:d=2" \
    -c:v libx264 "$SANDBOX/silent.mp4"
"$SCRIPT" "$SANDBOX/silent.mp4" "$SANDBOX/out2" >/dev/null 2>&1
STATUS=$?
if [ "$STATUS" -eq 3 ]; then ok "exit 3 on a file with no audio track"; else bad "exit 3 on a file with no audio track (got $STATUS)"; fi

# 3. a non-empty output directory is refused before any work
mkdir -p "$SANDBOX/out3" && : > "$SANDBOX/out3/existing"
"$SCRIPT" "$SANDBOX/speech.mp4" "$SANDBOX/out3" >/dev/null 2>&1
STATUS=$?
if [ "$STATUS" -eq 1 ] && [ ! -e "$SANDBOX/out3/pipeline" ]; then
    ok "a non-empty output directory is refused with exit 1"
else
    bad "a non-empty output directory is refused with exit 1 (got $STATUS)"
fi

if [ "$FAIL" -eq 0 ]; then echo "PASS"; exit 0; fi
echo "$FAIL failure(s)"
exit 1

#!/usr/bin/env bash
# openings-posters.test.sh — the gallery's pictures and the catalog are one list: every opening
# in ui/openings-catalog.js has its picture in ui/assets/openings/posters and no picture is
# left without an opening, each a 640 x 400 JPEG. Reads the tree and three planted copies of
# it; nothing is drawn, built or opened (the drawing itself is `openings-posters.js` with no
# argument, run by hand when a splash's look or the catalog changes).
# run-tests: no-host-screen: files are listed and their headers read; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/openings-posters.test.sh richos/app/scripts/openings-posters.js richos/app/ui/openings-catalog.js richos/app/ui/assets/openings/posters
# run-tests: covers richos/app/scripts/openings-posters.js
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ui="$(cd "$here/../ui" && pwd)"
tool="$here/openings-posters.js"
fails=0
ok() { if [ "$1" -eq 0 ]; then echo "  ok    $2"; else echo "  FAIL  $2"; [ -n "${3:-}" ] && echo "        $3"; fails=$((fails + 1)); fi; }

out="$(node "$tool" --check 2>&1)"; code=$?
ok "$code" "the tree: every opening has its picture and no picture is without an opening" "$out"
[ "$code" -eq 0 ] && echo "        $out"

# Each picture is what the gallery's card expects: a JPEG, 640 x 400.
bad=""
count=0
for f in "$ui"/assets/openings/posters/*.jpg; do
  count=$((count + 1))
  dims="$(python3 - "$f" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
if data[:3] != b"\xff\xd8\xff":
    print("not a JPEG"); raise SystemExit
i = 2
while i + 9 < len(data):
    if data[i] != 0xFF:
        i += 1; continue
    marker = data[i + 1]
    if marker in (0xC0, 0xC1, 0xC2):
        h, w = struct.unpack(">HH", data[i + 5:i + 9]); print(f"{w}x{h}"); raise SystemExit
    if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
        i += 2; continue
    i += 2 + struct.unpack(">H", data[i + 2:i + 4])[0]
print("no size found")
PY
)"
  [ "$dims" = "640x400" ] || bad="$bad $(basename "$f")=$dims"
done
if [ -z "$bad" ] && [ "$count" -gt 0 ]; then sized=0; else sized=1; fi
ok "$sized" "all $count pictures are 640 x 400 JPEGs" "$bad"

# The check can fail: a planted copy of the tree with a picture missing, and one with a stray.
tmp="$(mktemp -d "${TMPDIR:-/tmp}/openings-posters-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
plant() { mkdir -p "$tmp/$1/assets/openings/posters"; cp "$ui/openings-catalog.js" "$tmp/$1/"; cp "$ui"/assets/openings/posters/*.jpg "$tmp/$1/assets/openings/posters/"; }
plant whole
out="$(node "$tool" --check --ui "$tmp/whole" 2>&1)"; ok $? "a whole copy passes" "$out"
plant missing; rm "$tmp/missing/assets/openings/posters/power-100k.jpg"
out="$(node "$tool" --check --ui "$tmp/missing" 2>&1)"; code=$?
if [ "$code" -eq 1 ] && printf '%s' "$out" | grep -q "no picture for the opening power-100k"; then named=0; else named=1; fi
ok "$named" "an opening with no picture fails, by name" "exit $code: $out"
plant stray; cp "$tmp/stray/assets/openings/posters/power-100k.jpg" "$tmp/stray/assets/openings/posters/retired-opening.jpg"
out="$(node "$tool" --check --ui "$tmp/stray" 2>&1)"; code=$?
if [ "$code" -eq 1 ] && printf '%s' "$out" | grep -q "a picture with no opening: retired-opening.jpg"; then named=0; else named=1; fi
ok "$named" "a picture with no opening fails, by name" "exit $code: $out"

if [ "$fails" -eq 0 ]; then echo "PASS: openings-posters"; else echo "FAIL: openings-posters, $fails failure(s)"; fi
[ "$fails" -eq 0 ]

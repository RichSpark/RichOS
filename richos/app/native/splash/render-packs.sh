#!/usr/bin/env bash
# Produce the complete 60 Hz packs from the shipping splash renderer on macOS.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ui="$(cd "$here/../../ui" && pwd)"
out="${1:?output directory required}"
version="${2:?the app version is required for the update edition}"
shift 2

mkdir -p "$out"
out="$(cd "$out" && pwd)"
temporary="${RICHOS_SPLASH_RENDER_TMP:-${TMPDIR:?TMPDIR is required}}"
scratch="$(mktemp -d "${temporary%/}/richos-splash-render.XXXXXXXX")"
trap 'rm -rf "$scratch"' EXIT
swiftc -parse-as-library -O "$here/render.swift" -o "$scratch/render"
if [ "$#" -eq 0 ]; then
    catalog_ids="$(node -e 'global.window={}; require(process.argv[1]); for (const o of window.RichOpeningsCatalog.openings) console.log(o.id)' "$ui/openings-catalog.js")"
    while IFS= read -r id; do set -- "$@" "$id"; done <<< "$catalog_ids"
fi
for id in "$@"; do
    [[ "$id" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]
    "$scratch/render" "$ui/splash-opening.html" "$scratch/frames" "$id" "$version"
    count="$(find "$scratch/frames" -name '*.png' | wc -l | tr -d ' ')"
    ffmpeg -nostdin -v error -framerate 60 -i "$scratch/frames/%04d.png" \
        -vf 'scale=out_color_matrix=bt709,format=yuv420p' \
        -c:v libx264 -preset veryfast -crf 14 -g 60 -bf 0 -threads 2 \
        -color_primaries bt709 -color_trc iec61966-2-1 -colorspace bt709 \
        -video_track_timescale 60000 -movflags +faststart "$scratch/render.mp4"
    python3 - "$scratch/render.mp4" "$count" <<'PY'
import json
import subprocess
import sys

movie, count = sys.argv[1], int(sys.argv[2])
media = json.loads(subprocess.check_output([
    'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_streams', '-show_packets',
    '-show_entries', 'stream=codec_name,width,height,time_base:packet=pts,duration', '-of', 'json', movie,
]))
video, = media['streams']
assert [video[k] for k in ('codec_name', 'width', 'height', 'time_base')] == ['h264', 2800, 1704, '1/60000'], video
assert count in (180, 300) and len(media['packets']) == count
assert all(p == {'pts': n * 1000, 'duration': 1000} for n, p in enumerate(media['packets']))
PY
    mv "$scratch/render.mp4" "$out/$id.mp4"
    cp "$scratch/frames/bar.json" "$out/$id.bar.json"
    rm -rf "$scratch/frames" "$scratch/render.mp4"
    shasum -a 256 "$out/$id.mp4"
done

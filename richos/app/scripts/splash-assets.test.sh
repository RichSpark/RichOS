#!/usr/bin/env bash
# run-tests: inputs richos/app/native/splash richos/app/ui/openings-catalog.js richos/app/src-tauri/tauri.conf.json
# run-tests: covers richos/app/native/splash/packs.json richos/app/native/splash/player.m richos/app/native/splash/render.swift richos/app/native/splash/render-packs.sh richos/app/native/splash/starter/midnight-suede.bar.json richos/app/native/splash/starter/ruled-standard.bar.json
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"
python3 - "$app" <<'PY'
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

app = Path(sys.argv[1])
native = app / "native/splash"
authored = json.loads((app / "ui/openings-catalog.js").read_text().split("=", 1)[1].strip().removesuffix(";"))
manifest = json.loads((native / "packs.json").read_text())
starter = {"ruled-standard", "midnight-suede"}
assert set(manifest) | starter | {"edition-update"} == {opening["id"] for opening in authored["openings"]}
assert all(0 < v["bytes"] < 50_000_000 and re.fullmatch("[a-f0-9]{64}", v["sha256"]) for v in manifest.values())
def bar_in_view(bar):
    x, y, width, height = bar
    return 0 <= x < x + width <= 1400 and 0 <= y < y + height <= 852

assert all(bar_in_view(v["bar"]) for v in manifest.values())
config = json.loads((app / "src-tauri/tauri.conf.json").read_text())["bundle"]["resources"]
for name in starter:
    movie = native / "starter" / (name + ".mp4")
    bar = movie.with_suffix(".bar.json")
    with movie.open("rb") as pack:
        assert pack.read(8)[4:] == b"ftyp"
    assert bar_in_view(json.loads(bar.read_text()))
    for file in (movie, bar):
        assert config["../native/splash/starter/" + file.name] == "Splash/" + file.name
    if shutil.which("ffprobe"):
        video, = json.loads(subprocess.check_output([
            "ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
            "stream=codec_name,width,height,avg_frame_rate,nb_frames,duration", "-of", "json", str(movie),
        ]))["streams"]
        assert [video[k] for k in ("codec_name", "width", "height", "avg_frame_rate", "nb_frames", "duration")] == [
            "h264", 2800, 1704, "60/1", "180", "3.000000",
        ]
print("Every authored opening has a bundled pack, a verified download or a version-specific export.")
PY
bash -n "$app/native/splash/render-packs.sh"
if [ "$(uname -s)" = Darwin ]; then
    scratch="$(mktemp -d "${TMPDIR:?}/richos-splash-assets.XXXXXXXX")"
    trap 'rm -rf "$scratch"' EXIT
    xcrun clang -fobjc-arc -mmacosx-version-min=13.0 -Wno-deprecated-declarations \
        -framework AppKit -framework AVFoundation -framework AudioToolbox -framework CoreAudio \
        -framework CoreMedia -framework CoreVideo -framework QuartzCore \
        -I "$app/native/splash" -x objective-c -o "$scratch/prepare" - <<'C'
#import "player.m"
#include <assert.h>
#include <stddef.h>

static void cueFollowsTheHostClock(void) {
    float authored[] = {1, 2, 3, 4, 5, 6};
    float output[2][4];
    AudioBufferList *buffers = calloc(1, offsetof(AudioBufferList, mBuffers) + 2 * sizeof(AudioBuffer));
    assert(buffers);
    buffers->mNumberBuffers = 2;
    for (int i = 0; i < 2; i++) {
        buffers->mBuffers[i] = (AudioBuffer){1, sizeof(output[i]), output[i]};
    }
    RichSplashPCM cue = {.channels = {authored, authored}, .frames = 6, .ticksPerFrame = 1};
    AudioTimeStamp time = {.mHostTime = 100, .mFlags = kAudioTimeStampHostTimeValid};

    // The output is already running before video commits its host-time anchor.
    renderCue(&cue, NULL, &time, 0, 4, buffers);
    assert(output[0][0] == 0 && output[0][3] == 0 && atomic_load(&cue.progress) == 0);
    atomic_store(&cue.start, 103);
    renderCue(&cue, NULL, &time, 0, 4, buffers);
    assert(output[0][2] == 0 && output[0][3] == 1 && atomic_load(&cue.progress) == 1);
    time.mHostTime = 104;
    renderCue(&cue, NULL, &time, 0, 4, buffers);
    assert(output[0][0] == 2 && output[0][3] == 5 && atomic_load(&cue.progress) == 5);
    time.mHostTime = 108;
    renderCue(&cue, NULL, &time, 0, 4, buffers);
    assert(output[1][0] == 6 && output[1][1] == 0 && atomic_load(&cue.progress) == cue.frames);

    atomic_store(&cue.progress, 0);
    time.mHostTime = 110;
    renderCue(&cue, NULL, &time, 0, 4, buffers);
    assert(atomic_load(&cue.late) && atomic_load(&cue.progress) == 0);
    assert(output[0][0] == 0 && output[0][3] == 0);
    free(buffers);
}

int main(int argc, char **argv) {
    cueFollowsTheHostClock();
    for (int i = 1; i < argc; i++) {
        void *player = richos_splash_prepare(argv[i], 180, NULL, NULL, NULL, NULL);
        if (!player) {
            fprintf(stderr, "Cannot prepare the first video samples in %s\n", argv[i]);
            return 1;
        }
        richos_splash_discard_prepared(player);
    }
}
C
    "$scratch/prepare" "$app/native/splash/starter/"{ruled-standard,midnight-suede}.mp4
    xcrun swiftc -parse-as-library -typecheck "$app/native/splash/render.swift"
fi

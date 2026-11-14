#!/usr/bin/env bash
# An impulse at each end and silence in between. The waveform bins must line
# up with the recording's time and retain both signs of peak.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
python3 -B - "$here/waveform.py" <<'PY'
import pathlib
import struct
import subprocess
import sys
import tempfile
import wave

with tempfile.TemporaryDirectory(prefix="richos-waveform-") as scratch:
    fixture = pathlib.Path(scratch) / "peaks.wav"
    with wave.open(str(fixture), "wb") as audio:
        audio.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
        audio.writeframes(struct.pack("<192h", 0, -32768, *([0] * 189), 16384))
    actual = subprocess.check_output([sys.executable, "-B", sys.argv[1], str(fixture)], text=True).strip()
    assert actual == "9" + "0" * 94 + "4", actual
print("PASS: waveform preserves the time, size and sign-independent amplitude of the peaks")
PY

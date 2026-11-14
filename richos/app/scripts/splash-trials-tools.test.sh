#!/usr/bin/env bash
# splash-trials-tools.test.sh — the splash trials' committed tools
# (docs/verification/2026-11-14-splash-trials/tools/) still parse, and their analysis reads a
# small synthetic recording to the answers it was built with: a picture's first frame on the
# glass, its stamp, the busy end, Core Animation pictures on time, and the pixel comparison.
# Nothing is launched and no screen is read; the recorder is only type-checked.
# run-tests: no-host-screen: synthetic recordings and pictures in a temporary directory; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/splash-trials-tools.test.sh richos/app/scripts/splash-trials-tools.test.py docs/verification/2026-11-14-splash-trials/tools
# run-tests: covers docs/verification/2026-11-14-splash-trials/tools/analyze.py docs/verification/2026-11-14-splash-trials/tools/frame-png.py docs/verification/2026-11-14-splash-trials/tools/release-bundle.sh docs/verification/2026-11-14-splash-trials/tools/screen-frames.swift docs/verification/2026-11-14-splash-trials/tools/splash-trials-guest.py docs/verification/2026-11-14-splash-trials/tools/splash-trials-walk.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/splash-trials-tools.test.py"

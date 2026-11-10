#!/usr/bin/env bash
# first-party-license.test.sh — RichOS's own material carries no license but the root AGPL-3.0:
# a license file, SPDX identifier, manifest field or license declaration other than it, outside
# what .richos/vendored-material records as third-party, is refused at the commit, the land and
# the push backstop, naming the file and line. Throwaway Git repositories with the real autocheck
# hooks; nothing builds, boots or opens a window, and this repository's hooks are never touched.
# run-tests: no-host-screen: throwaway Git repositories only; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/first-party-license.test.sh richos/app/scripts/first-party-license.test.py richos/app/scripts/autocheck .richos/vendored-material richos/tools/richpass/RichPass.html docs/legal/BRAND-ASSETS.md docs/legal/LICENSING.md richos/mobile/native-android/gradlew richos/tools/richpass/vendor/tldts/LICENSE richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/app/scripts/autocheck/first-party-license.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/first-party-license.test.py"

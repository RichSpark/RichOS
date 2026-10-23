#!/usr/bin/env bash
# probe-engine-runtime.test.sh — every engine file the opt-in live probe runs is in the tree.
# The probe needs an authenticated Claude, so it is never run here. This checks only that each
# engine path it names (engine/"..." in its source) exists, which is what a renamed engine file
# breaks. Reads the probe's source as text; nothing is launched.
# run-tests: no-host-screen: reads one source file as text; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/probe-engine-runtime.test.sh richos/app/scripts/probe-engine-runtime.py richos/engine
# run-tests: covers richos/app/scripts/probe-engine-runtime.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$here/probe-engine-runtime.py" "$here/../../engine" <<'PY'
import re
import sys
from pathlib import Path

probe, engine = Path(sys.argv[1]), Path(sys.argv[2])
named = sorted(set(re.findall(r'engine\s*/\s*"([^"]+)"', probe.read_text())))
if not named:
    sys.exit("FAIL: the probe names no engine path; this check reads nothing")
missing = [path for path in named if not (engine / path).is_file()]
for path in named:
    print(("FAIL  missing " if path in missing else "PASS  ") + path)
if missing:
    sys.exit("FAIL: the probe runs %d engine file(s) that are not in the tree" % len(missing))
print("PASS: all %d engine paths the probe runs exist" % len(named))
PY

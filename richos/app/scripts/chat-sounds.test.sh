#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/chat-sounds.test.sh richos/app/scripts/chat-sounds.py richos/app/scripts/package-app.sh richos/app/scripts/testvm/branch-bundle.sh richos/app/src-tauri/src/chat_notifications.rs richos/app/native/notifications/chat.m
# run-tests: covers richos/app/scripts/chat-sounds.py richos/app/scripts/testvm/branch-bundle.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:?}/chat-sounds.XXXXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/media" "$scratch/resources"
export RICHOS_CHAT_SOUNDS="$scratch/media"

python3 - "$here" "$scratch" <<'PY'
import json
from pathlib import Path
import re
import subprocess
import sys

here = Path(sys.argv[1])
root = Path(sys.argv[2])
script = [sys.executable, str(here / "chat-sounds.py")]
native = (here / "../src-tauri/src/chat_notifications.rs").read_text()
names = set(re.findall(r'"([A-Za-z0-9_.-]+\.mp3)"', native))
assert len(names) == 5, names
assert subprocess.run([*script, "--overlay", "--required"], capture_output=True).returncode != 0
for name in names:
    (root / "media" / name).write_bytes(("synthetic " + name).encode())
overlay = json.loads(subprocess.check_output([*script, "--overlay", "--required"]))
resources = overlay["bundle"]["resources"]
assert resources == {str(root / "media" / name): "ChatSounds/" + name for name in names}
subprocess.run([*script, "--copy", str(root / "resources"), "--required"], check=True)
for name in names:
    assert (root / "resources" / "ChatSounds" / name).read_bytes() == (root / "media" / name).read_bytes()
missing = next(iter(names))
(root / "media" / missing).unlink()
assert subprocess.run([*script, "--overlay", "--required"], capture_output=True).returncode != 0
print("Five native sounds map byte for byte into shipping and guest bundles. Incomplete assets refuse packaging.")
PY

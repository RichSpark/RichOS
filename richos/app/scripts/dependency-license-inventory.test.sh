#!/usr/bin/env bash
# dependency-license-inventory.test.sh — the committed Rust dependency license inventory matches
# the committed lockfiles, and every license expression in them has been reviewed against
# AGPL-3.0-only. A dependency arriving under an unreviewed license, or a lockfile change that
# leaves the inventory stale, fails here. Reads cargo metadata only; builds nothing, opens no window.
# run-tests: no-host-screen: reads lockfiles and metadata only; nothing is launched on any screen
# run-tests: inputs richos/app/third-party/ richos/app/Cargo.lock richos/app/crates/richos-user-update/Cargo.lock richos/app/src-tauri/Cargo.lock richos/mobile/conformance/verifier/Cargo.lock richos/tools/native-claude-stdio/Cargo.lock richos/app/scripts/dependency-license-inventory.test.sh richos/app/scripts/dependency-license-inventory.sh richos/app/scripts/lib/dependency_license_inventory.py docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md
# run-tests: covers richos/app/scripts/lib/dependency_license_inventory.py richos/app/scripts/dependency-license-inventory.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$here/dependency-license-inventory.sh" --check

# A crate vendored under richos/app/third-party/ is a row with its publisher's license, never
# folded into RichOS's own code, and the lockfile it was published with is not a workspace here
# (Cargo never reads a path dependency's lockfile).
doc="$here/../../../docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md"
for manifest in "$here"/../third-party/*/Cargo.toml; do
  [ -f "$manifest" ] || continue
  row="$(python3 - "$manifest" <<'PY'
import sys, tomllib
p = tomllib.load(open(sys.argv[1], "rb"))["package"]
print("| `%s` | %s |" % (p["name"], p["version"]))
PY
)"
  if ! grep -qF "$row" "$doc"; then
    echo "FAIL: the vendored crate $manifest has no inventory row ($row)" >&2
    exit 1
  fi
done
if grep -q '^| `richos/app/third-party/' "$doc"; then
  echo "FAIL: a vendored crate's own published lockfile is listed as a RichOS workspace" >&2
  exit 1
fi
echo "dependency-license-inventory.test.sh: vendored crates are rows; their own lockfiles are not workspaces"

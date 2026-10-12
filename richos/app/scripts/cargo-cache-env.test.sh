#!/usr/bin/env bash
# cargo-cache-env.test.sh — the shared sccache server never depends on the temp directory or
# working directory of whichever agent happened to start it. An agent starts the server from a
# directory that is then deleted (its workspace closes); a later compile from another agent
# must still work. Uses its own port and cache directory; the live server is never touched.
# run-tests: no-host-screen: compiles one empty Rust file; nothing is launched on any screen
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/app/scripts/cargo-cache-env.test.sh richos/app/scripts/lib/cargo-cache-env.sh
# run-tests: covers richos/app/scripts/lib/cargo-cache-env.sh
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
lib="$here/lib/cargo-cache-env.sh"

[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
sccache_bin="${RUSTC_WRAPPER:-$(command -v sccache || true)}"
if [ -z "$sccache_bin" ] || ! command -v rustc >/dev/null 2>&1; then
  # Exit 2, never 0: nothing was compiled, and exit 0 is recorded as `passed` (hunt R18).
  echo "  NOT RUN  cargo-cache-env: sccache or rustc is not installed here"; exit 2
fi

# sccache puts a Unix socket under the temp directories below (limit about 100 bytes), so they must
# stay short whatever the caller's TMPDIR is: the nightly's own TMPDIR is too long (cleaned by the trap).
work="$(cd "$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/cce.XXXXXX")" && pwd -P)"
port=$((20000 + RANDOM % 20000))
export SCCACHE_SERVER_PORT="$port"
stop_server() { SCCACHE_DIR="$work/cache" SCCACHE_SERVER_PORT="$port" "$sccache_bin" --stop-server >/dev/null 2>&1 || true; }
trap 'stop_server; rm -rf "$work"' EXIT
mkdir -p "$work/fixed" "$work/fresh"
python3 - "$lib" "$work" <<'PY' || exit 1
import os
from pathlib import Path
import subprocess
import sys
helper, root = sys.argv[1], Path(sys.argv[2])
env = {'HOME': str(root), 'PATH': '/usr/bin:/bin', 'RUSTC_WRAPPER': '/usr/bin/true',
       'RICHOS_MACHINE_SETTINGS_RESOLVED': '1', 'PYTHONDONTWRITEBYTECODE': '1'}
def initialize(values):
    # The deadline only bounds a hung helper; exit status and resolved paths prove correctness.
    return subprocess.run(['/bin/bash', '-c', '. "$1" || exit; printf "%s" "$CARGO_TARGET_DIR"',
                           'fixture', helper], cwd=root, env=values, capture_output=True,
                          text=True, timeout=15)
for target in ('build/target with spaces', str(root / 'absolute target')):
    result = initialize({**env, 'CARGO_TARGET_DIR': target})
    assert result.returncode == 0, result.stderr
    assert Path(result.stdout).resolve() == (root / target).resolve(), result.stdout
settings = root / 'private.env'
settings.write_text('CARGO_TARGET_DIR=build/target\n')
env.pop('RICHOS_MACHINE_SETTINGS_RESOLVED')
result = initialize({**env, 'RICHOS_ENV_FILE': str(settings)})
assert result.returncode != 0 and 'absolute path' in result.stderr, result
print('ok   shell accepts Cargo environment targets while private settings remain absolute')
PY
echo 'pub fn f() -> u8 { 1 }' > "$work/x.rs"

# The "agent": its own temp directory and working directory, deleted after it uses the cache.
doomed="$work/doomed"
mkdir -p "$doomed/tmp"
(
  cd "$doomed" || exit 1
  export TMPDIR="$doomed/tmp" SCCACHE_DIR="$work/cache" RICHOS_SCCACHE_TMPDIR="$work/fixed"
  unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER
  PATH="$(dirname "$sccache_bin"):$PATH"
  # shellcheck disable=SC1090
  . "$lib"
  "$sccache_bin" rustc --crate-type lib --emit=dep-info,metadata --out-dir "$doomed/out1" "$work/x.rs" >/dev/null 2>&1
)
rm -rf "$doomed"

# A later agent, with a different temp directory, compiles through the same server.
mkdir -p "$work/out2"
if out="$(cd "$work/fresh" && TMPDIR="$work/fresh" SCCACHE_DIR="$work/cache" "$sccache_bin" rustc --crate-type lib --crate-name later --emit=dep-info,metadata --out-dir "$work/out2" "$work/x.rs" 2>&1)" \
   && [ -f "$work/out2/liblater.rmeta" ]; then
  echo "ok   a compile after the starting agent's directory was deleted works"
else
  echo "FAIL a compile after the starting agent's directory was deleted failed:"; echo "$out"; exit 1
fi

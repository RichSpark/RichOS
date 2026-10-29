#!/usr/bin/env bash
# Rust and shell lint; baseline checks are read-only unless --lower is explicit.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$DIR/lib/cargo-cache-env.sh"
export PYTHONDONTWRITEBYTECODE=1
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi
# THE LINT RUNS UNDER THE PYTHON ITS BASELINES RECORD, whichever python3 the caller's PATH puts
# first. The ratchet refuses ("lint versions do not match the committed baseline") when the
# interpreter's major.minor differs from baselines/*.json versions.python, and a worker's shell
# (the engine's runtime/bin first on PATH) has an older python3, so every plain `git commit`
# was refused until /opt/homebrew/bin was put first by hand. The pin is read from the baseline
# itself, so there is no second number to forget; a test (autocheck.test.py) checks that all
# three baselines agree. With no interpreter of that version found, python3 runs and the driver
# refuses with its usual message.
lint_python=python3
want="$(sed -n 's/^ *"python": *"\([0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' "$DIR/lint/baselines/custom.json" | head -n 1)"
if [ -n "$want" ]; then
    py_version() { "$1" -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null || true; }
    if [ "$(py_version python3)" != "$want" ]; then
        for candidate in "python$want" "/opt/homebrew/bin/python$want" "/usr/local/bin/python$want" \
                         "/opt/homebrew/opt/python@$want/bin/python$want" "/opt/homebrew/bin/python3" "/usr/local/bin/python3"; do
            if command -v "$candidate" >/dev/null 2>&1 && [ "$(py_version "$candidate")" = "$want" ]; then
                lint_python="$candidate"
                break
            fi
        done
    fi
fi
exec "$lint_python" "$DIR/lint/driver.py" "$@"

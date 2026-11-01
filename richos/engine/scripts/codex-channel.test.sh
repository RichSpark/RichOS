#!/usr/bin/env bash
# Exercises lib/codex_channels.py and lib/review_watch.py through isolated journals and real CLI processes.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$here/lib/codex_channels.test.py" "$@"
exec python3 -B "$here/lib/codex_review.test.py" "$@"

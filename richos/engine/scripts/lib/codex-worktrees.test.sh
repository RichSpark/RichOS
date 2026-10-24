#!/usr/bin/env bash
# Proves lib/codex_worktrees.py: which finished Codex worktrees may close, and that
# closing goes through git's own refusals (cleanup PRD A6).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 -B "$HERE/codex-worktrees.test.py" "$@"

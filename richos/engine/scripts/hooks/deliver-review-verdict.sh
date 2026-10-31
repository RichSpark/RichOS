#!/usr/bin/env bash
# deliver-review-verdict.sh: matcherless PreToolUse. A second-review verdict on a
# running teammate's work reaches that teammate once, at its next tool call, as
# additionalContext (CEO §113; richos-hq plan 2026-10-09 §2.5, slice 3). Never
# blocks: every outcome is exit 0. The logic: scripts/lib/review_delivery.py.
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — it never blocks or approves, so a payload it cannot read delivers nothing and the call goes ahead, which is what it should do.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail
INPUT="$(cat)"
# The lead's own call carries no agent_id: nothing to deliver, no interpreter.
case "$INPUT" in *'"agent_id"'*) ;; *) exit 0 ;; esac
# The machine's one Claude home, by scripts/lib/machine_settings.py claude_home's rule:
# CLAUDE_CONFIG_DIR, else ~/.claude, and RICHOS_CLAUDE_HOME where CLAUDE_CONFIG_DIR is the added
# account's folder the app gave this lease. These two lines are run alone by second-review.test.sh C26.
case "${CLAUDE_CONFIG_DIR:-}" in "") CLAUDE_HOME_DIR="$HOME/.claude" ;; "${RICHOS_CLAUDE_ACCOUNT_DIR:-}") CLAUDE_HOME_DIR="${RICHOS_CLAUDE_HOME:-$CLAUDE_CONFIG_DIR}" ;; *) CLAUDE_HOME_DIR="$CLAUDE_CONFIG_DIR" ;; esac
LEDGER="${SECOND_REVIEW_STATE_DIR:-$CLAUDE_HOME_DIR/state}/reviews.jsonl"
[ -f "$LEDGER" ] || exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
command -v python3 >/dev/null 2>&1 || exit 0
[ -f "$SCRIPT_DIR/../lib/review_delivery.py" ] || exit 0
printf '%s' "$INPUT" | python3 -B "$SCRIPT_DIR/../lib/review_delivery.py"
exit 0

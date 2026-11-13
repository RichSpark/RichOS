#!/usr/bin/env bash
#
# answer-permission-prompt.test.sh: the engine's mod that answers every Claude Code permission
# prompt No in a bypass-permissions session (hooks/answer-permission-prompt.ts).
#
#   R1  the engine plugin registers the mod: hooks/hooks.json `modules` names it
#   R2  `claude plugin validate` reads the mod and finds its five hooks (the three classic events
#       that carry the permission mode, tool.check and classic.PermissionRequest)
#   T   `claude plugin test` runs hooks/answer-permission-prompt.test.ts against Claude Code's own
#       test host: in bypass, an ask from Claude Code's check, a person-only one included, is
#       answered No at the check with Claude Code's reason and nothing runs; an ask raised after
#       the check is answered No too, as is one after a plan approved with bypass permissions;
#       other modes, a switch out of bypass, questions to the user and another plugin's query are
#       left alone
#
# The mod is staged alone in a temporary plugin folder, so the test host never writes its type
# files into the engine and never loads the engine's command hooks. Claude Code runs with HOME and
# CLAUDE_CONFIG_DIR inside that folder too, so it never writes the operator's ~/.claude.
#
# Usage: scripts/hooks/answer-permission-prompt.test.sh

set -uo pipefail
ENGINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="$ENGINE/hooks/answer-permission-prompt.ts"
SUITE="$ENGINE/hooks/answer-permission-prompt.test.ts"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '%s\n' "$2" | sed 's/^/        /' | head -40; FAIL=$((FAIL + 1)); return 0; }

[ -f "$MOD" ] || { echo "FATAL: missing $MOD" >&2; exit 2; }
[ -f "$SUITE" ] || { echo "FATAL: missing $SUITE" >&2; exit 2; }
CLAUDE_BIN="${RICHOS_CLAUDE_BIN:-$(command -v claude || true)}"
[ -n "$CLAUDE_BIN" ] && [ -x "$CLAUDE_BIN" ] || { echo "FATAL: no claude binary to run the mod's test host" >&2; exit 2; }

echo "=== answer-permission-prompt: registration ==="
if python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("modules") == ["./answer-permission-prompt.ts"] else 1)' \
        "$ENGINE/hooks/hooks.json"; then
    ok "R1 hooks/hooks.json modules names ./answer-permission-prompt.ts"
else
    bad "R1 hooks/hooks.json does not register the mod"
fi

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/answer-permission-prompt.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/.claude-plugin" "$STAGE/hooks" "$STAGE/home/.claude"
claude_isolated() { HOME="$STAGE/home" CLAUDE_CONFIG_DIR="$STAGE/home/.claude" "$CLAUDE_BIN" "$@"; }
printf '%s\n' '{ "name": "richos-engine", "version": "0.0.0", "description": "the engine mod, staged for its test" }' \
    > "$STAGE/.claude-plugin/plugin.json"
printf '%s\n' '{ "modules": ["./answer-permission-prompt.ts"] }' > "$STAGE/hooks/hooks.json"
cp "$MOD" "$SUITE" "$STAGE/hooks/"

out="$(claude_isolated plugin validate "$STAGE" 2>&1)"; rc=$?
if [ "$rc" = 0 ] && printf '%s' "$out" | grep -q 'hooks: classic.SessionStart, classic.UserPromptSubmit, classic.PostToolUse, tool.check, classic.PermissionRequest'; then
    ok "R2 claude plugin validate: all five hooks read"
else
    bad "R2 claude plugin validate (rc=$rc)" "$out"
fi

echo "=== answer-permission-prompt: behavior (claude plugin test) ==="
out="$(cd "$STAGE" && claude_isolated plugin test "$STAGE" 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then
    ok "T  claude plugin test: every case of answer-permission-prompt.test.ts passed"
else
    bad "T  claude plugin test (rc=$rc)" "$out"
fi
printf '%s\n' "$out" | tail -12 | sed 's/^/        /'

echo "=== answer-permission-prompt: $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]

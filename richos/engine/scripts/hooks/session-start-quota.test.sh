#!/usr/bin/env bash
#
# session-start-quota.test.sh — EVERY LEAD HEARS THE 93% RULE, IN HIS WORDS,
#                                WITH THE COMMAND; THE CEO DOES NOT SEE A METER.
#
#   N1  an adopted repository with QUOTA_PAUSE_PERCENT declared: the lead is
#       told his words verbatim, his 2026-09-25 update, the declared line, the
#       reading now and the exact command that starts the watcher
#   N2  it goes to the MODEL only (additionalContext), never to the person
#       (systemMessage): the budget design notes, R4, keep the percentage out
#       of the CEO's ordinary view
#   N3  adopted but QUOTA_PAUSE_PERCENT undeclared: not silent, it says the
#       watcher cannot run and names the key to declare
#   N4  a repository that never adopted the engine: complete silence
#   N5  no payload yet: it still speaks, and the reading reads UNKNOWN
#   N6  already at or above the threshold at session start: it says so
#   N7  nothing on stderr, and stdout is one JSON object
#   N8  the watcher script missing: silent, exit 0 (a notice never breaks a
#       session)
#   N9  at or above the threshold with the reset less than 20 minutes away:
#       it says NO PAUSE (his 2026-09-25 update, ruling §87); N1 also checks
#       that the update is quoted
#   N10 the watcher starts by itself with the session (the engine's plugin
#       monitor, 2026-09-28): the notice names it and --alive, and gives
#       --watch only as the fallback
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$HOOK_DIR/session-start-quota.sh"
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 1; }

# shellcheck source=../lib/scratch.sh
. "$HOOK_DIR/../lib/scratch.sh"
FIXTURE="$(mktemp -d "${TMPDIR:?}/session-start-quota.XXXXXX")" || exit 1
export HOME="$FIXTURE/home" CLAUDE_CONFIG_DIR="$FIXTURE/config"
mkdir -p "$HOME" "$CLAUDE_CONFIG_DIR"
trap 'rm -rf "$FIXTURE"' EXIT
SB="$(scratch_new session-start-quota-test)" || { echo "FATAL: no scratch" >&2; exit 1; }
trap 'scratch_release "$SB" >/dev/null 2>&1 || true; rm -rf "$FIXTURE"' EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT RICHOS_SESSION_ID RICHOS_APP_STATE
export QUOTA_PAYLOAD="$SB/payload.json"
export QUOTA_WATCH_NOW=1800000000
export RICHOS_WORKSPACES_DIR="$SB/ws"
ENT="$SB/entity"
mkdir -p "$ENT" "$RICHOS_WORKSPACES_DIR"

write_payload() { # <used> <resets-in-seconds>
    python3 - "$QUOTA_PAYLOAD" "$1" "$2" <<'PY'
import json, os, sys
now = int(os.environ["QUOTA_WATCH_NOW"])
json.dump({"rate_limits": {"five_hour": {"used_percentage": int(sys.argv[2]),
                                         "resets_at": now + int(sys.argv[3])}}}, open(sys.argv[1], "w"))
os.utime(sys.argv[1], (now, now))
PY
}
run_hook() { # [hook-path] — sets OUT, ERR, RC; runs from $RUN_DIR
    local h="${1:-$HOOK}"
    OUT="$(cd "$RUN_DIR" && printf '{"hook_event_name":"SessionStart","cwd":"%s"}' "$RUN_DIR" | bash "$h" 2>"$SB/err")"
    RC=$?
    ERR="$(cat "$SB/err")"
}
ctx() { printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["hookSpecificOutput"]["additionalContext"])' 2>/dev/null; }

echo "=== session-start-quota tests ==="

export RICHOS_ENTITY_ROOT="$ENT"
RUN_DIR="$ENT"
printf 'PROTECTED_PATHS=""\nQUOTA_PAUSE_PERCENT=93\n' >"$ENT/orchestration.config"
write_payload 41 3600
run_hook
C="$(ctx)"
if [ "$RC" -eq 0 ] \
   && printf '%s' "$C" | grep -qF '"quota polling: every 5 minutes from now. And once it crosses the 93% threshold: PAUSE subagents. Then resume after quota rest."' \
   && printf '%s' "$C" | grep -q 'QUOTA_PAUSE_PERCENT=93' \
   && printf '%s' "$C" | grep -qF 'Updated 2026-09-25: at or above the threshold, do not pause when the reset is less than 20 minutes away (exactly 20 minutes still pauses)' \
   && printf '%s' "$C" | grep -q 'Polling stays every 5 minutes at every usage level' \
   && printf '%s' "$C" | grep -q '41% of the five-hour window' \
   && printf '%s' "$C" | grep -qE '/scripts/quota-watch\.sh --watch$' \
   && printf '%s' "$C" | grep -q 'run_in_background: true'; then
    ok "N1  his words verbatim, the declared line, the reading and the exact command"
else
    bad "N1  rc=$RC context=$C out=$OUT"
fi
# N10 (2026-09-28): the watcher starts by itself with the session (the plugin
# monitor), so the notice says so, gives the check, and offers --watch only as
# the fallback; it no longer tells every lead to start the watcher.
if printf '%s' "$C" | grep -qE '/scripts/quota-watch\.sh --monitor$' \
   && printf '%s' "$C" | grep -qE '/scripts/quota-watch\.sh --alive$' \
   && printf '%s' "$C" | grep -q 'Do not start it yourself' \
   && printf '%s' "$C" | grep -q 'Only if --alive says nothing delivers them' \
   && ! printf '%s' "$C" | grep -q 'Start the watcher now'; then
    ok "N10 the watcher starts with the session (plugin monitor): the notice gives --alive, and --watch only as the fallback"
else
    bad "N10 context=$C"
fi
if printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "systemMessage" not in d and d["hookSpecificOutput"]["hookEventName"]=="SessionStart" else 1)'; then
    ok "N2  the lead hears it (additionalContext); the CEO sees no meter (no systemMessage)"
else
    bad "N2  out=$OUT"
fi
if [ -z "$ERR" ] && printf '%s' "$OUT" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
    ok "N7  nothing on stderr, and stdout is one JSON object"
else
    bad "N7  err=$ERR out=$OUT"
fi

printf 'PROTECTED_PATHS=""\n' >"$ENT/orchestration.config"
run_hook; C="$(ctx)"
if [ "$RC" -eq 0 ] && printf '%s' "$C" | grep -q 'CANNOT RUN' && printf '%s' "$C" | grep -q 'QUOTA_PAUSE_PERCENT'; then
    ok "N3  undeclared threshold: not silent; says the watcher cannot run and names the key"
else
    bad "N3  rc=$RC context=$C"
fi

printf 'PROTECTED_PATHS=""\nQUOTA_PAUSE_PERCENT=93\n' >"$ENT/orchestration.config"
rm -f "$QUOTA_PAYLOAD"
run_hook; C="$(ctx)"
if [ "$RC" -eq 0 ] && printf '%s' "$C" | grep -q 'Now: UNKNOWN' && printf '%s' "$C" | grep -q 'quota-watch.sh --watch'; then
    ok "N5  no payload yet: it still speaks, and the reading reads UNKNOWN"
else
    bad "N5  rc=$RC context=$C"
fi

write_payload 95 3600
run_hook; C="$(ctx)"
if [ "$RC" -eq 0 ] && printf '%s' "$C" | grep -q 'ALREADY AT OR ABOVE THE THRESHOLD'; then
    ok "N6  already at or above the threshold at session start: it says so"
else
    bad "N6  rc=$RC context=$C"
fi

write_payload 95 600
run_hook; C="$(ctx)"
if [ "$RC" -eq 0 ] && printf '%s' "$C" | grep -q 'AT OR ABOVE THE THRESHOLD, NO PAUSE' \
   && printf '%s' "$C" | grep -q 'less than 20 minutes' && ! printf '%s' "$C" | grep -q 'ALREADY AT OR ABOVE'; then
    ok "N9  at or above with the reset 10 minutes away: it says NO PAUSE (his 2026-09-25 update)"
else
    bad "N9  rc=$RC context=$C"
fi

# N4: a directory that never adopted the engine, found the way a session finds it.
unset RICHOS_ENTITY_ROOT
PLAIN="$SB/plain"; mkdir -p "$PLAIN"
RUN_DIR="$PLAIN"
run_hook
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && [ -z "$ERR" ]; then
    ok "N4  a repository that never adopted the engine: complete silence"
else
    bad "N4  rc=$RC out=$OUT err=$ERR"
fi

# N8: the hook on its own, with no watcher beside it.
export RICHOS_ENTITY_ROOT="$ENT"; RUN_DIR="$ENT"
mkdir -p "$SB/lonely/hooks"
cp "$HOOK" "$SB/lonely/hooks/session-start-quota.sh"
run_hook "$SB/lonely/hooks/session-start-quota.sh"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "N8  the watcher missing: silent, exit 0"
else
    bad "N8  rc=$RC out=$OUT"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== session-start-quota tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== session-start-quota tests: all $PASS passed ==="
exit 0

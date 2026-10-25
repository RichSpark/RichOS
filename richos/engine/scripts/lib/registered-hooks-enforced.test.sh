#!/usr/bin/env bash
#
# registered-hooks-enforced.test.sh — hook_enforced_on_surface answers "does a
# REGISTERED COMMAND run this guard?", not "does its name appear in the file?"
#
# P5-45: the first line of the function was a grep over the whole JSON document,
# so a guard named only in a description string was credited as enforced.

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -3; FAIL=$((FAIL + 1)); }

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/rh-enforced.XXXXXX")" || exit 2
trap 'rm -rf "$SANDBOX"' EXIT
mkdir -p "$SANDBOX/hooks" "$SANDBOX/scripts/hooks"
# shellcheck disable=SC1091
. "$HERE/registered-hooks.sh"

cat > "$SANDBOX/hooks/hooks.json" <<'JSON'
{
  "description": "Mentions scripts/hooks/guard-ghost.sh in prose only; nothing registers it.",
  "hooks": {
    "PreToolUse": [
      {"matcher": "Bash", "hooks": [
        {"type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/scripts/hooks/guard-real.sh"},
        {"type": "command", "command": "echo note about scripts/hooks/guard-echoed.shx"},
        {"type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/scripts/hooks/dispatch-pretooluse.sh bash"}
      ]}
    ]
  }
}
JSON
printf 'bash|guard-via-dispatcher.sh\n' > "$SANDBOX/scripts/hooks/dispatch-pretooluse.manifest"
SURFACE="$SANDBOX/hooks/hooks.json"

if hook_enforced_on_surface "$SURFACE" guard-real.sh; then
    ok "1  a guard in a registered command is enforced"
else bad "1  a registered guard"; fi

if hook_enforced_on_surface "$SURFACE" guard-ghost.sh; then
    bad "2  a guard named only in a description must NOT be credited as enforced"
else ok "2  a guard named only in description prose is not enforced"; fi

if hook_enforced_on_surface "$SURFACE" guard-via-dispatcher.sh; then
    ok "3  a dispatcher rule module is enforced"
else bad "3  a dispatcher module"; fi

if hook_enforced_on_surface "$SURFACE" guard-nothing.sh; then
    bad "4  an unknown guard must not be enforced"
else ok "4  an unregistered guard is not enforced"; fi

# P5-45 (v2 re-check): a registered command that only PRINTS the exact guard path
# never runs the guard. The same mistake lived at every reader of a hook command.
cat > "$SANDBOX/hooks/printed.json" <<'JSON'
{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
  {"type": "command", "command": "echo \"scripts/hooks/guard-ghost.sh\""},
  {"type": "command", "command": "printf '%s\\n' ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-printed.sh"},
  {"type": "command", "command": "cat scripts/hooks/guard-catted.sh; echo scripts/hooks/dispatch-pretooluse.sh bash"},
  {"type": "command", "command": "FOO=1 bash \"${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-real.sh\""},
  {"type": "command", "command": "echo start && bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-chained.sh"}
]}]}}
JSON
PRINTED="$SANDBOX/hooks/printed.json"
for g in guard-ghost.sh guard-printed.sh guard-catted.sh; do
    if hook_enforced_on_surface "$PRINTED" "$g"; then
        bad "5  a command that only prints $g must NOT be credited as enforcing it"
    else ok "5  printed path of $g is not enforcement"; fi
done
for g in guard-real.sh guard-chained.sh; do
    if hook_enforced_on_surface "$PRINTED" "$g"; then
        ok "6  $g, executed through a runner or after a chain, is enforced"
    else bad "6  an executed guard ($g) must still be enforced"; fi
done
inv="$(registered_hook_scripts "$PRINTED" PreToolUse 2>/dev/null)"
if printf '%s\n' "$inv" | grep -qE 'guard-ghost|guard-printed|guard-catted|dispatch-pretooluse'; then
    bad "7  the inventory must list only executed guards" "$inv"
else ok "7  the inventory lists only executed guards"; fi
if printf '%s\n' "$inv" | grep -qx guard-real.sh; then
    ok "7b the inventory still lists an executed guard"
else bad "7b executed guard missing from inventory" "$inv"; fi

# registered-hooks-gaps.test.py (the dispatcher-manifest cases, P5-08) was named by
# no suite, so nothing ran it; it exercises the same library, so it runs here.
if python3 "$HERE/registered-hooks-gaps.test.py" >/dev/null 2>&1; then
    ok "8  registered-hooks-gaps.test.py passes"
else bad "8  registered-hooks-gaps.test.py fails"; fi

# The dispatcher half of hook_enforced_on_surface used to be
# `_rh_dispatch_modules | grep -qxF`. grep -q exits at its first match; when the
# writer still had output to send it died of SIGPIPE (141), and under the
# caller's pipefail (this suite's, dialect-guard.test.sh's) an enforced guard
# read as NOT enforced. dialect-guard.test.sh E2 failed that way about one run in
# thirty on a loaded host. A writer that pauses after the match makes the race
# certain rather than occasional.
if (
    _rh_dispatch_modules() { printf 'guard-via-dispatcher.sh\n'; sleep 0.5; printf 'guard-later.sh\n'; }
    hook_enforced_on_surface "$SURFACE" guard-via-dispatcher.sh
); then
    ok "10 a match is not lost when the expansion is still being written (no SIGPIPE under pipefail)"
else bad "10 an enforced dispatcher rule read as NOT enforced: the expansion writer died of SIGPIPE"; fi

printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]

#!/usr/bin/env bash
#
# guard-no-shell-c.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NEVER LET CLAUDE CODE'S "shell -c script could not be checked" PROMPT REACH THE USER.
#
# The CEO, 2026-10-09: "This shell -c script runs rm and could not be checked. Do you want to
# proceed?" appeared on his screen for a teammate's command; he hit yes and hoped app users would
# not be bothered by it (the same prompt reached him on 2026-10-07 from the lead's commands). Claude
# Code cannot check an inline `bash -c '...'` script, so it asks. A hook that DENIES runs first, so the
# agent gets the rewrite (separate commands, or a script file run with `bash <file>`) and nobody is asked.
#
# The rule is scripts/lib/shell_c.py (a text match; it leaks, any error in it is a pass). The RichOS
# app does not run this chain: its hook, scripts/app-engine-hook.py, applies the same rule through
# mega-lander/app.py validate_shell_target. Suite: guard-no-shell-c.test.sh.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_gsc_in="$(cat)"
_gsc_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_gsc_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-no-shell-c.sh" "$_gsc_in" "" \
        "whether a Bash command wraps an inline script in bash -c, sh -c or zsh -c"
fi
# Cheap exit first: nearly every command has no -c in it.
case "$_gsc_in" in
    *-*c*) ;;
    *) exit 0 ;;
esac
_gsc_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
printf '%s' "$_gsc_in" | "$_gsc_py" "$_gsc_dir/../lib/shell_c.py"
_gsc_rc=$?
[ "$_gsc_rc" = 2 ] && exit 2
exit 0

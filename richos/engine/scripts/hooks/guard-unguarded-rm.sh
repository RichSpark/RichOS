#!/usr/bin/env bash
#
# guard-unguarded-rm.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NEVER LET CLAUDE CODE'S OWN rm CIRCUIT BREAKER ASK THE CEO.
#
# The CEO, 2026-10-02, bypass permissions on, was stopped by a Yes/No prompt from a teammate's
# `rm -f $D/*.jsonl`: "Dangerous rm operation on possibly-empty variable path". The prompt is Claude
# Code's critical-path check (code.claude.com/docs/en/permission-modes#critical-paths), which no allow
# rule and no PreToolUse "allow" can approve, and which bypassPermissions still asks about. A hook that
# DENIES runs before it, so the agent gets the exact rewrite and the CEO never sees a prompt.
#
# The rule and its shapes are in scripts/lib/unguarded_rm.py. It is a text match and leaks; its job is
# to stop the habitual command. Any error in the check is a pass. Suite: guard-unguarded-rm.test.sh.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_gur_in="$(cat)"
_gur_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_gur_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-unguarded-rm.sh" "$_gur_in" "" \
        "whether a Bash command removes a path that starts with an unguarded or critical variable"
fi
# Cheap exits first: nearly every command neither removes nor deletes.
case "$_gur_in" in
    *rm*|*find*) ;;
    *) exit 0 ;;
esac
_gur_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
printf '%s' "$_gur_in" | "$_gur_py" "$_gur_dir/../lib/unguarded_rm.py" check
_gur_rc=$?
[ "$_gur_rc" = 2 ] && exit 2
exit 0

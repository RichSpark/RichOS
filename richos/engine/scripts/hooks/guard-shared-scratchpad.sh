#!/usr/bin/env bash
#
# guard-shared-scratchpad.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# A TEAMMATE NEVER DELETES INSIDE THE LEAD SESSION'S DIRECTORY.
#
# 2026-10-01: Claude Code tells every in-process teammate that the lead session's
# scratchpad (<root>/<project>/<session-id>/scratchpad) is ITS OWN scratchpad. It is
# shared. Two teammates, obeying "delete your disposable scratch before reporting",
# ran `rm -rf <session>/scratchpad/*` and `rm -f <session>/scratchpad/*` and took the
# lead's briefs and a live teammate's helper scripts, logs and backup copy of a file it
# was mutating. The rule and the full record are in scripts/lib/shared_scratchpad.py.
#
# Refused: a call made inside a subagent (its payload carries agent_id) whose rm,
# rmdir, unlink, trash, srm or find -delete/-exec rm reaches inside a session directory
# under a Claude scratch root, that directory, or a directory above it. The lead's own
# calls pass. A teammate's own named agent scratch is not this rule's
# business and passes.
#
# There is no exemption marker, deliberately: no delete in the shared scratchpad is ever
# needed (the scratch reaper removes the session directory after the session ends), and a
# marker would be the first thing the next end-of-task cleanup learned to add.
#
# IT IS A TEXT MATCH AND IT LEAKS (a script, `sh -c "$X"`, a Python rmtree). Its job is
# the habitual cleanup command. Any error in the check is a pass.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_gss_in="$(cat)"
_gss_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_gss_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-shared-scratchpad.sh" "$_gss_in" "" \
        "whether a teammate's Bash command deletes inside the lead session's shared scratchpad"
fi
# Cheap exits first: only a subagent's call that names a deleter is ever refused.
case "$_gss_in" in
    *'"agent_id"'*) ;;
    *) exit 0 ;;
esac
case "$_gss_in" in
    *rm*|*unlink*|*trash*|*-delete*) ;;
    *) exit 0 ;;
esac
_gss_py=python3
printf '%s' "$_gss_in" | "$_gss_py" "$_gss_dir/../lib/shared_scratchpad.py" bash-check
_gss_rc=$?
[ "$_gss_rc" = 2 ] && exit 2
exit 0

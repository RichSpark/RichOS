#!/usr/bin/env bash
#
# guard-land-lease-commands.sh: PreToolUse[Bash] rule (a module of
# dispatch-pretooluse.sh). The operator fence's SECOND line of defense in the
# shared main checkout (spec r3 e7 item 1, Frank G1 points 2 and 3).
#
# The first line is the Git fence itself: in the main checkout it refuses
# ORIG_HEAD without the land lease, which aborts `git merge` and `git pull`
# BEFORE the tree is touched (Frank G1, measured on both gits). What it cannot
# stop cleanly is a command that writes the tree or its own state first:
# `cherry-pick`, `revert`, `am`, `rebase`, `stash`, `commit` (whose move of main
# the fence refuses only after the index is written), and, measured 2026-09-24
# on both gits against G1's premise, `reset` (the index and tree are rewritten
# before ORIG_HEAD) and `merge --abort` (the staged resolution is discarded
# before ORIG_HEAD, and MERGE_HEAD is left behind). This rule refuses those,
# aimed at a fenced main checkout, when this session does not hold that
# repository's lease, before the command runs.
#
# IT IS A TEXT MATCH AND IT LEAKS (`sh -c`, a script, a variable). Its job is to
# stop a cooperative agent on its habitual path, not an adversary; the Git fence
# is the backstop. The risk that matters is refusing a harmless command, so the
# TARGET is resolved by the checkout it runs in (`git rev-parse` from the path
# after `-C`, else the directory of a leading `cd <dir>`, else the payload's
# cwd), never by a path prefix: a femcboost native worktree sits inside the main
# checkout's directory and has its own top level, and a commit there passes.
#
# WITH THE SWITCH OFF it never refuses: a repository whose launcher is absent or
# says OPERATOR_FENCES_STATE="off" is skipped, and any error in the check is a
# pass, because the Git fence decides anyway.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_gllc_in="$(cat)"
_gllc_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# A PAYLOAD THIS RULE COULD NOT READ IS SAID, NEVER PASSED IN SILENCE: the one
# exit is the same, only the silence changes (scripts/lib/unevaluated-notice.sh,
# the convention every Bash rule follows). This is the one place this rule can
# speak with the switch off, and only on a payload no rule could read.
_UE_LIB="$_gllc_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-land-lease-commands.sh" "$_gllc_in" "" \
        "whether a Git command aimed at a fenced main checkout holds that repository's land lease"
fi
# dispatch-words: git
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
case "$_gllc_in" in *git*) ;; *) exit 0 ;; esac
case "$_gllc_in" in
    *cherry-pick*|*revert*|*" am"*|*rebase*|*stash*|*commit*|*reset*|*--abort*|*--quit*) ;;
    *) exit 0 ;;
esac
_gllc_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
printf '%s' "$_gllc_in" | "$_gllc_py" "$_gllc_dir/../lib/operator_fences.py" early-check
_gllc_rc=$?
[ "$_gllc_rc" = 2 ] && exit 2
exit 0

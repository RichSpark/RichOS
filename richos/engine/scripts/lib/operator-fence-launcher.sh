#!/usr/bin/env bash
# richos-operator-fence-launcher
#
# A repository's `reference-transaction` hook, written by
# <engine>/scripts/operator-fences.sh install from the template at
# <engine>/scripts/lib/operator-fence-launcher.sh. Do not edit it by hand: every
# OPERATOR_FENCES_* value below is rewritten by that command, and
# `operator-fences.sh status` compares this file with the engine and with the
# entity's declaration.
#
# WHAT IT DOES, IN ORDER (spec r3 e2 and e8, Frank G1, G12):
#   1. With OPERATOR_FENCES_STATE="on" only: when the transaction names
#      refs/heads/main, or names HEAD, ORIG_HEAD, CHERRY_PICK_HEAD or REVERT_HEAD
#      inside the MAIN checkout, it runs the fence (operator-fences/operator_fences.py,
#      a copy of the engine's). In `prepared` the fence refuses a move of main,
#      ORIG_HEAD in the main checkout, or switching the main checkout away from
#      main, unless the writer holds the land lease. In a repository listed in
#      SECOND_REVIEW_REPOS (OPERATOR_FENCES_REVIEWS names the review ledger) it
#      also refuses, even for the lease holder, a move of main that lands work
#      whose tip has no passing second review (CEO §113). In `committed` it only
#      records who started a merge, cherry-pick or revert.
#   2. Always, whatever the state and whatever the fence decided: every
#      executable in reference-transaction.d/, in name order, gets the same
#      arguments and the same lines. That is the forensic recorder and the
#      repository's own previous hook. A refused transaction is still recorded.
#   3. Exits 1 if the fence refused, else with the chain's status.
#
# WITH THE STATE OFF, lease checks are skipped. Reviewed repositories still
# enforce review/recovery acceptance and operation integrity. Unreviewed
# repositories start no interpreter. This
# launcher sits on every ref write in the repository, AUTO_MERGE on every commit
# included, from the day it is installed with the switch off; a defect in it must
# not be able to reach his terminal before the switch is ever turned on (G12).
#
# WITH THE STATE ON, A FENCE THAT CANNOT DECIDE REFUSES, and the refusal names the
# way out (spec e8): a verified operator runs `<engine>/scripts/operator-fences.sh off --reason <reason>`.
# This disables lease enforcement; review/recovery acceptance remains active.

OPERATOR_FENCES_STATE="off"
OPERATOR_FENCES_REPO="@REPO@"
OPERATOR_FENCES_COMMON="@COMMON@"
OPERATOR_FENCES_HOME="@HOME@"
OPERATOR_FENCES_KEY="@KEY@"
OPERATOR_FENCES_HOLDERS="@HOLDERS@"
OPERATOR_FENCES_REVIEWS="@REVIEWS@"
OPERATOR_FENCES_ENGINE="@ENGINE@"
OPERATOR_FENCES_PROGRAM="@PROGRAM@"
OPERATOR_FENCES_PROGRAM_DIGEST="@DIGEST@"

__ofl_launcher="$OPERATOR_FENCES_COMMON/hooks/reference-transaction"
__ofl_phase="${1:-}"
__ofl_input="$(cat 2>/dev/null)" || __ofl_input=""
__ofl_refused=0

if [ "$OPERATOR_FENCES_STATE" = "on" ] || [ -n "$OPERATOR_FENCES_REVIEWS" ]; then
    __ofl_main=0; __ofl_head=0; __ofl_started=0
    while IFS=' ' read -r __ofl_o __ofl_n __ofl_r; do
        case "${__ofl_r:-}" in
            refs/heads/main) __ofl_main=1 ;;
            HEAD) __ofl_head=1 ;;
            ORIG_HEAD|CHERRY_PICK_HEAD|REVERT_HEAD) __ofl_started=1 ;;
        esac
    done <<__OFL_EOF__
$__ofl_input
__OFL_EOF__
    __ofl_run=0
    if [ "$__ofl_phase" = "prepared" ] && [ "$__ofl_main" = 1 ]; then
        __ofl_run=1
    elif [ "$__ofl_head" = 1 ] || [ "$__ofl_started" = 1 ]; then
        # HEAD and ORIG_HEAD are per checkout: only the main checkout's are fenced.
        __ofl_gd="$(git rev-parse --absolute-git-dir 2>/dev/null)"
        if [ -n "$__ofl_gd" ] && \
           [ "$(cd "$__ofl_gd" 2>/dev/null && pwd -P)" = "$(cd "$OPERATOR_FENCES_COMMON" 2>/dev/null && pwd -P)" ]; then
            if [ "$__ofl_phase" = "prepared" ] || { [ "$__ofl_phase" = "committed" ] && [ "$__ofl_started" = 1 ]; }; then
                __ofl_run=1
            fi
        fi
    fi
    if [ "$__ofl_run" = 1 ]; then
        __ofl_py="$(command -v python3 2>/dev/null)"; [ -n "$__ofl_py" ] || __ofl_py=/usr/bin/python3
        printf '%s\n' "$__ofl_input" | "$__ofl_py" "$OPERATOR_FENCES_PROGRAM" fence "$__ofl_launcher" "$__ofl_phase"
        __ofl_frc=$?
        if [ "$__ofl_phase" = "prepared" ] && [ "$__ofl_frc" != 0 ]; then
            __ofl_refused=1
            if [ "$__ofl_frc" != 1 ]; then
                {
                    echo "=== OPERATOR FENCE: the fence could not decide (exit $__ofl_frc), so nothing moved ==="
                    echo "  Repository: $OPERATOR_FENCES_REPO"
                    echo "  Program: $OPERATOR_FENCES_PROGRAM"
                    if [ -n "$OPERATOR_FENCES_REVIEWS" ]; then
                        echo "  Restore the installed fence and recovery reader through scripts/operator-fences.sh install."
                        echo "  Review acceptance remains active while lease fencing is off."
                    else
                        echo "  Rich's: $OPERATOR_FENCES_ENGINE/scripts/operator-fences.sh off --repo \"$OPERATOR_FENCES_REPO\" --reason '<verified operator reason>'"
                    fi
                } >&2
            fi
        fi
    fi
fi

__ofl_rc=0
for __ofl_m in "$OPERATOR_FENCES_COMMON/hooks/reference-transaction.d"/*; do
    [ -f "$__ofl_m" ] && [ -x "$__ofl_m" ] || continue
    if [ -n "$__ofl_input" ]; then
        printf '%s\n' "$__ofl_input" | RICHOS_REF_HOOK_PARENT="$PPID" "$__ofl_m" "$@"
    else
        RICHOS_REF_HOOK_PARENT="$PPID" "$__ofl_m" "$@" </dev/null
    fi
    __ofl_r=$?
    [ "$__ofl_r" = 0 ] || __ofl_rc=$__ofl_r
done

[ "$__ofl_refused" = 1 ] && exit 1
exit "$__ofl_rc"

#!/usr/bin/env bash
#
# notice-unlanded-branches.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                               QUIETLY WHILE FINISHED WORK SITS ON A BRANCH
#                               THAT MAIN HAS NEVER SEEN.
#
# The predicate, the measured narrowings, the liveness rule and what this
# cannot see are all in scripts/lib/unlanded-branches.py. Read that first — it
# is the analysis half. This file is the wiring and the one sentence.
#
# ===========================================================================
# THE INCIDENT
# ===========================================================================
# 2026-09-06. A session ended reporting "Everything is clean and pushed" while
# SIX finished branches sat outside main, one of them the fix for the CEO's own
# complaint that the app steals keyboard focus. Both checks that session ran
# were green AND CORRECT — the working tree was clean, and `main` did match
# `origin/main`. Neither of those two facts can see an unlanded branch, and
# between them they are the whole vocabulary of the word "clean".
#
# Fifty-nine seconds earlier the same session had NAMED two of those branches
# by hand inside escalation acknowledgments, and closed an item citing a signoff
# file that existed only on an unmerged branch. The information was not missing.
# Nothing was standing at the turn end asking for it.
#
# ===========================================================================
# WHY IT REPORTS AND DOES NOT BLOCK
# ===========================================================================
# `Stop` CAN block — that is measured, and guard-idle-land.sh uses it. This one
# deliberately does not, for the reason notice-unstarted-rows.sh gives one event
# over: "there is unlanded work" is a FACT, and "therefore you should not have
# stopped" is a judgment this hook does not have. A turn that ends to ask the
# CEO a question, ends because he interrupted, or ends in the middle of a land
# is a turn ending correctly with branches outstanding. Refusing those would
# make this a thing to switch off, and a guard people disable protects nothing.
#
# ===========================================================================
# SIGNAL QUALITY IS THE WHOLE JOB — the numbers are in the analyzer
# ===========================================================================
# Measured on this machine on 2026-09-06: 201 local branches across five
# repositories, 3 with commits main does not contain, 2 that no live worktree
# holds. A notice keyed on "a branch exists" would have named 201 and been
# muted within a day. The narrowings and the commands that produced every one of
# those numbers are in scripts/lib/unlanded-branches.py; they are not repeated
# here, because a number in two places is a number that will disagree with
# itself.
#
# The two it names today are real: the `BLOCKED.md` escalations of 2026-09-02,
# still outside main four days later.
#
# ===========================================================================
# IT NEVER SAYS "CLEAR" ABOUT SOMETHING IT DID NOT READ
# ===========================================================================
# A repository with no recorded integration branch (point 14) cannot be
# answered for: "has this landed?" has no reference to be asked against, and
# the sweep abstains rather than assuming main. That abstention arrived here as
# `STATUS partial` with `N 0`, this file treated `partial` as a no-op, and the
# zero-findings branch printed:
#
#     UNLANDED-BRANCH WATCH: clear again
#
# over a fixture holding one unlanded `cc/` branch. Neither repository this
# engine governs has a branch recorded, so that was the live state.
#
# The sweep now carries the abstention in fields of its own (NOTEXAMINED, UKEY,
# UNEXAMINED) and this hook announces it in the words its sibling
# land-completeness.sh already uses for the same fact: NOT EXAMINED. Two
# surfaces over one abstention are not allowed two vocabularies, and neither of
# them is allowed the word "clear".
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, the only channel measured
# to reach the operator (that file carries the table). The state key is the SET
# of findings including each tip SHA, so: a persistent set is announced once, a
# NEW branch going unlanded speaks again, a branch being landed speaks again
# (the set changed), and an agent adding a commit to an already-reported branch
# speaks again — deliberately, because that is new work now at risk.
#
# ===========================================================================
# LAYER R CHECKS THIS HOOK, AND NOBODY HAD TO REMEMBER TO SAY SO
# ===========================================================================
# This section used to read: contract-integrity-probe.sh carries a TYPED list
# (R_ROOTED_HOOKS) of the hooks whose root bootstrap it holds byte-identical,
# this hook is absent from it because that file belonged to another engineer
# when this was written, nothing goes red as a result, please add it.
#
# SETTLED 2026-09-14, AND THE WAY IT WAS SETTLED IS THE POINT. It was added to
# the list on 2026-09-06 with the merge — and four other rooted hooks that made
# the same note were not, so Layer R was walking 52 of the 57 registered hooks
# that resolve a root and one of them was already carrying a divergent
# bootstrap. A note asking a future reader to update an inventory is not a
# mitigation for that; it is the same bet placed politely. Layer R now DERIVES
# the hooks it walks from hooks/hooks.json, so this hook is checked because it
# is registered, and the only way out of the check is an explicit entry in
# R_ROOTLESS_HOOKS. The block below is a byte-exact copy of the canonical one,
# which is now an enforced property rather than a promise.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start. Installing this changes NOTHING in the
# session that installs it — it begins reporting in the NEXT session. Verify it
# is live after a restart with:
#     scripts/hooks/notice-unlanded-branches.sh --self-test
# and by checking the engine-status banner's registered-hook count.
#
# Exit codes: always 0. This hook never refuses a turn.
#
# Self-test:  scripts/hooks/notice-unlanded-branches.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/notice-unlanded-branches.sh)"

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/unlanded-branches.test.sh"
fi

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/notice-unlanded-branches.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 0
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# --- NOTICE CHANNEL --------------------------------------------------------
# A Stop hook's stand-down and cannot-run notices go to the OPERATOR, never to
# stderr. The measurement behind that, and the argument for announcing on state
# change rather than every turn, are in scripts/lib/stop-hook-notice.sh. This
# block is byte-identical in every Stop hook and stop-hook-visibility.test.sh
# asserts it, for the reason Layer R asserts the same of the root bootstrap: a
# divergent copy is one hook disagreeing with its siblings about how it tells
# you it has stopped working.
_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
if [ -f "$_SHN_LIB" ]; then
    # shellcheck source=../lib/stop-hook-notice.sh
    . "$_SHN_LIB"
else
    # The helper is the thing that makes these notices visible, so its absence
    # must not make them invisible. The hook then announces EVERY turn,
    # undeduplicated, and says why. Degrading toward noise is recoverable by an
    # operator who can read it; degrading toward silence rebuilds the defect.
    stop_notice_init() { :; }
    stop_notice_normal() { :; }
    stop_notice_abnormal() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
    # The recurring form degrades to the same unconditional announcement. An
    # interval it cannot honor is announced MORE often, never less: a notice
    # channel's degraded mode leans toward noise, because noise is recoverable
    # by an operator who can read it and silence rebuilds the defect.
    stop_notice_abnormal_recurring() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
fi

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Nothing is governed here. The plugin loads in every directory on the
    # machine and a notice in each would be the noise this engine already
    # decided not to make.
    exit 0
else
    stop_notice_init "notice-unlanded-branches.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "UNLANDED-BRANCH WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether finished work is sitting on a branch main has never seen. $HOOK_TAG"
    root_failure_banner "scripts/hooks/notice-unlanded-branches.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_UNLANDED_BRANCHES:=1}"

stop_notice_init "notice-unlanded-branches.sh" "$ENTITY_ROOT" "$INPUT"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# THIS HOOK IS NOT PAYLOAD-INDEPENDENT AND MUST NOT CLAIM TO BE. The set of
# repositories it sweeps comes from the payload's session_id joined against the
# worktree ledger, and on this machine that is the difference between sweeping
# one repository and sweeping two — the second being richos, where every
# teammate actually works. A degraded payload therefore produces a NARROWER
# sweep reported in the same words as a full one, which is the defect this
# engine names most often. So it says so instead.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "notice-unlanded-branches.sh" \
            "whether this session left finished work on a branch main has never seen" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_UNLANDED_BRANCHES" = "0" ]; then
    # Never a silent permission. An opt-out the operator cannot see is a defense
    # that decays into a rumor.
    stop_notice_abnormal "stood-down" \
        "UNLANDED-BRANCH WATCH — STOOD DOWN by CHECK_UNLANDED_BRANCHES=0 in $CONFIG. Branches holding finished work outside main are NOT being reported, so a turn may end on the word 'clean' with work stranded. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "UNLANDED-BRANCH WATCH — NOT RUNNING: python3 is not on PATH, so nothing checked whether finished work is sitting outside main this turn. $HOOK_TAG"
    exit 0
fi

if ! command -v git >/dev/null 2>&1; then
    stop_notice_abnormal "no-git" \
        "UNLANDED-BRANCH WATCH — NOT RUNNING: git is not on PATH, so nothing checked whether finished work is sitting outside main this turn. $HOOK_TAG"
    exit 0
fi

SWEEP="$SCRIPT_DIR/../lib/unlanded-branches.py"
if [ ! -f "$SWEEP" ]; then
    stop_notice_abnormal "no-sweep" \
        "UNLANDED-BRANCH WATCH — NOT RUNNING: the sweep is missing at $SWEEP. A clean main and an absent checker must never look the same. $HOOK_TAG"
    exit 0
fi

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(str(d.get("session_id", "") or "") if isinstance(d, dict) else "")
except Exception:
    print("")
' 2>/dev/null || true)"

# THE RECOVERY LINE IS EMITTED AFTER THE SWEEP, NOT BEFORE. The ledger holds
# ONE state per (session, hook): recording "ok" on entry and the finding
# afterwards makes the two alternate, and the same finding is then announced on
# every single turn — de-duplication defeated by the hook that owns it. That
# bug was caught in notice-unasked-deferral.sh's first run and its header
# explains it at length; this hook writes both states, so it must write exactly
# one of them per turn.
set +e
OUT="$(python3 "$SWEEP" --entity-root "$ENTITY_ROOT" --session "$SESSION_ID" \
        --extra-repos "${UNLANDED_BRANCHES_EXTRA_REPOS:-}" --format hook 2>/dev/null)"
RC=$?
set -e

if [ "$RC" -ne 0 ] || [ -z "$OUT" ]; then
    stop_notice_abnormal "sweep-failed:$RC" \
        "UNLANDED-BRANCH WATCH — DID NOT RUN: the sweep exited $RC without a verdict, so nothing looked for finished work outside main. Do not read this as a clean main; it is an unexamined one. Run scripts/unlanded-branches-lint.sh by hand. $HOOK_TAG"
    exit 0
fi

field() { printf '%s\n' "$OUT" | grep "^$1	" | head -1 | cut -f2- || true; }

STATUS="$(field STATUS)"
REASON="$(field REASON)"
KEY="$(field KEY)"
N="$(field N)"
SUMMARY="$(field SUMMARY)"
NOTEXAMINED="$(field NOTEXAMINED)"
UKEY="$(field UKEY)"
UNEXAMINED="$(field UNEXAMINED)"

case "$STATUS" in
    stand-down)
        # No repository resolved. Announced rather than silent: this hook
        # believes it governs an adopted root, and an adopted root with no
        # repository behind it is a broken assumption, not an empty result.
        stop_notice_abnormal "stand-down" \
            "UNLANDED-BRANCH WATCH SWEPT NOTHING: ${REASON:-no repository resolved}. This is not a clean main; it is an unread one. $HOOK_TAG"
        exit 0 ;;
    partial)
        : ;;  # some repository could not be answered for. The findings below
              # are still real, AND the gap is announced in its own right --
              # see the NOTEXAMINED branch below. It used to fall straight
              # through to the "clear again" line, which is how an abstention
              # came to be reported as health.
    swept)
        : ;;
    *)
        stop_notice_abnormal "unknown-status:${STATUS:-empty}" \
            "UNLANDED-BRANCH WATCH — UNREADABLE VERDICT ('${STATUS:-empty}'), so nothing here is a statement about main. Run scripts/unlanded-branches-lint.sh by hand. $HOOK_TAG"
        exit 0 ;;
esac

# THE ABSTENTION IS ANNOUNCED BEFORE ANY WORD ABOUT MAIN, AND IT NEVER SAYS
# "CLEAR". A repository with no recorded integration branch (point 14) is one
# this sweep could not look in at all; reported as clean it is the exact defect
# this hook was written for, wearing this hook's own sentence. Its sibling
# land-completeness prints NOT EXAMINED for the identical abstention, and the
# two must agree, so this one prints those words too.
if [ "${NOTEXAMINED:-0}" != "0" ] && [ -n "${NOTEXAMINED:-}" ]; then
    if [ "${N:-0}" = "0" ] || [ -z "$SUMMARY" ]; then
        stop_notice_abnormal "unexamined:$UKEY" \
            "UNLANDED-BRANCH WATCH COULD NOT LOOK — ${UNEXAMINED:-NOT EXAMINED - a repository could not be answered for and this is NOT a clean main; it is an unread one.} Nothing here is a statement that your work landed. $HOOK_TAG"
        exit 0
    fi
    # There are real findings AND a gap. Both, in one line: a finding list from
    # a partial sweep is a floor, never a total.
    stop_notice_abnormal "unlanded:$KEY|unexamined:$UKEY" \
        "$SUMMARY ALSO, AND THE LIST ABOVE IS THEREFORE A FLOOR RATHER THAN A TOTAL: ${UNEXAMINED} $HOOK_TAG"
    exit 0
fi

if [ "${N:-0}" = "0" ] || [ -z "$SUMMARY" ]; then
    stop_notice_normal \
        "UNLANDED-BRANCH WATCH: clear again — every repository was read, and every branch ahead of main is held by a live teammate, or there are none. $HOOK_TAG"
    exit 0
fi

stop_notice_abnormal "unlanded:$KEY" "$SUMMARY $HOOK_TAG"
exit 0

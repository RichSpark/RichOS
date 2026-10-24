#!/usr/bin/env bash
#
# release-land-leases.sh: Stop hook. A land lease ends at its holder's turn end
# once the land is at rest (richos-hq spec r3 e6, F3; Frank G5 point 3).
#
# For every land lease THIS session holds (its holder, pid and start time, is an
# ancestor of this hook):
#   * at rest (no merge, cherry-pick, revert or rebase in progress, no unmerged
#     path, `git status --porcelain --untracked-files=no` empty, and main not
#     ahead of its upstream) and none of the land commands the lease records
#     still running -> released, silently. A background land between two of
#     its steps looks at rest; on 2026-10-11 this hook released two leases that
#     way and the fence refused both merges (operator_fences.land_process);
#   * otherwise -> kept, and one notice names the repository, the holder's
#     conversation, the age and what is unfinished, including which paths are
#     dirty, so a lease held up by ANOTHER writer's dirt can be told from the
#     holder's own unfinished work. It goes to the LEAD (Stop additionalContext),
#     which holds the lease and finishes or releases it, and only when the lease or
#     what keeps it changes, never again at every turn end for the same state
#     (orchestration reliability R6, richos-hq docs/prds/2026-10-23-orchestration-
#     reliability.md: the screenshot turn carried it among 10,302 characters).
# A lease this session does not hold is never touched. It never blocks.
#
# WITH THE SWITCH OFF no lease exists (land-lease.sh takes none) and a repository
# whose launcher is off is skipped, so this hook prints nothing.
#
# IT SAYS SO WHEN IT CANNOT RUN (scripts/hooks/stop-hook-visibility.test.sh): a
# root it cannot resolve, or a release step that failed, reaches the operator as a
# systemMessage through scripts/lib/stop-hook-notice.sh, because a lease nobody
# releases holds every other conversation's land.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the leases decide, by the
# holder's ancestry and the repositories' own Git state; the payload is read only
# to resolve the governed root, and a failure there is announced above.

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
  hook: scripts/hooks/release-land-leases.sh
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

INPUT="$(cat)"

_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
[ -f "$_SHN_LIB" ] || exit 0
# shellcheck source=../lib/stop-hook-notice.sh
. "$_SHN_LIB"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # No engine here. A lease can only have been taken where the fence is
    # installed, and nothing here installed it.
    exit 0
else
    root_failure_banner "scripts/hooks/release-land-leases.sh" >&2
    stop_notice_init "release-land-leases.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "LAND LEASE RELEASE IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). A land lease this session holds is not released at this turn end; land-lease.sh status --repo <repo> shows it and land-lease.sh release --repo <repo> ends it."
    exit 0
fi

stop_notice_init "release-land-leases.sh" "$ENTITY_ROOT" "$INPUT"

_rll_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
_rll_out="$("$_rll_py" "$SCRIPT_DIR/../lib/operator_fences.py" turn-end 2>/dev/null)"
_rll_rc=$?
if [ "$_rll_rc" -ne 0 ]; then
    stop_notice_abnormal "predicate-failed" \
        "LAND LEASE RELEASE DID NOT RUN (exit $_rll_rc). A land lease this session holds was not checked at this turn end; land-lease.sh status --repo <repo> shows it and land-lease.sh release --repo <repo> ends it."
    exit 0
fi
if [ -n "$_rll_out" ]; then
    _rll_msg="$(printf '%s' "$_rll_out" | "$_rll_py" -c 'import json,sys
print(json.load(sys.stdin).get("systemMessage") or "")' 2>/dev/null)"
    # The state is the message without its age ("after 12 min:"), so a lease that only
    # grew older is not said again.
    _rll_key="held:$(printf '%s' "$_rll_msg" | sed -E 's/ after [^:]*:/ after:/g' | cksum | tr -d ' ')"
    if [ "$(_shn_prior)" != "$_rll_key" ]; then
        _shn_record "$_rll_key" "$(date +%s 2>/dev/null || echo '')"
        printf '%s\n' "{\"hookSpecificOutput\":{\"hookEventName\":\"Stop\",\"additionalContext\":\"$(_shn_escape "$_rll_msg")\"}}"
    fi
else
    case "$(_shn_prior)" in
        held:*) stop_notice_normal "" ;;          # released: nothing to say
        *) stop_notice_normal "LAND LEASE RELEASE: running again." ;;
    esac
fi
exit 0

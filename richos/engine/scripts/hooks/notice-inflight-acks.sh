#!/usr/bin/env bash
#
# notice-inflight-acks.sh — NON-BLOCKING Stop hook. Tells the OPERATOR that a
# teammate WAS SENT a notice that main moved under it, has not proved it heard,
# and that the timeout has passed — so the §8b fallback is now the move.
#
# ===========================================================================
# ONLY NOTICES THAT WERE SENT
# ===========================================================================
# CEO 2026-10-06: a push no longer requires a notice to, or a waiver for, any
# live teammate; each teammate runs `git merge main` itself as its last step
# before handover (the line prepare-agent-spawn.py puts in every spawn prompt).
# So a teammate that main moved past and nobody messaged is owed NOTHING, and
# this hook says nothing about it — naming it every time main moved was
# repeated noise with no action behind it. What stays is the ack of a notice
# that was actually sent, and the one line when its timeout has passed.
#
# ===========================================================================
# WHY 30 MINUTES — MEASURED, NOT PICKED
# ===========================================================================
# ~/.claude/teams/session-8a598936/worker-events.jsonl, 22 completed run
# segments (WorkerStarted -> WorkerRunEnded), 2026-08-30, this machine:
#
#     min 111s      p25 824s      p50 2375s (40m)      p90 4258s (71m)
#
# A queued message is delivered at the receiver's next TOOL ROUND, which is
# bounded by one tool call rather than by a whole run segment — so a teammate
# that is running at all gets many delivery opportunities inside 30 minutes.
# 30 sits deliberately BELOW the median segment so the notice reaches the
# operator while the teammate is still inside a segment it can act in; a
# threshold at the p90 (71m) would routinely arrive after the damage. Tunable
# per repository with INFLIGHT_ACK_TIMEOUT_MIN in orchestration.config.
#
# NOTHING IS KILLED AUTOMATICALLY. §8b is explicit that TaskStop + respawn
# destroys whatever the teammate has written and is the FALLBACK, not the first
# move. A hook that reaped a teammate on a timer would be making that call on a
# stopwatch, with none of the context it needs. This notice states the
# condition and names the action; the operator decides.
#
# ===========================================================================
# WHICH REPOSITORIES IT SWEEPS — and the limit, named
# ===========================================================================
# The seat (this session's governed repository) plus every repository recorded
# in <team dir>/inflight-repos.txt, which guard-inflight-notify.sh and
# scripts/inflight-notify.sh both append to the moment they look at one. That
# is how a session seated in one repository and landing in another gets swept
# at all — the shape this whole operation runs in.
#
# THE LIMIT: a repository that has never been pushed and never been asked about
# is not on that list, and this hook will not find it. It is discovered the
# first time anyone runs the sweep or the push guard against it.
#
# systemMessage via scripts/lib/stop-hook-notice.sh, because that is the only
# channel measured to reach the operator (see that file's table). One line,
# state-change de-duplicated: a persistent condition announced every turn is a
# condition the eye is trained to skip.

set -eo pipefail

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
  hook: scripts/hooks/notice-inflight-acks.sh
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

_IF_LIB="$SCRIPT_DIR/../lib/inflight.sh"
[ -f "$_IF_LIB" ] || exit 0
# shellcheck source=../lib/inflight.sh
. "$_IF_LIB"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Nothing is governed here, so no teammate of this repository can be behind.
    # The plugin loads in every directory on the machine; a notice in each would
    # be noise, and engine-status.sh already announces the stand-down at session
    # start.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    root_failure_banner "scripts/hooks/notice-inflight-acks.sh" >&2
    stop_notice_init "notice-inflight-acks.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "IN-FLIGHT ACK WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nobody is watching whether sent in-flight notices were acked — run scripts/inflight-notify.sh status by hand."
    exit 0
fi

stop_notice_init "notice-inflight-acks.sh" "$ENTITY_ROOT" "$INPUT"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found nothing. The sentence comes from
# scripts/lib/unevaluated-notice.sh so every hook says it the same way, and it
# goes out through the same notice channel the stand-downs above use, which is
# the one measured for the Stop event. NO VERDICT CHANGES — this hook already
# ended the turn on exactly these payloads.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "notice-inflight-acks.sh" \
            "whether a teammate is still holding an in-flight notice nobody acknowledged" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if ! inflight_require; then
    stop_notice_abnormal "broken" \
        "IN-FLIGHT ACK WATCH IS OFF: $INFLIGHT_BROKEN. Nobody is watching whether teammates acked the last land; run scripts/inflight-notify.sh status by hand."
    exit 0
fi

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("session_id","") or "") if isinstance(d,dict) else "")
except Exception:
    print("")' 2>/dev/null || true)"
TRANSCRIPT="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("transcript_path","") or "") if isinstance(d,dict) else "")
except Exception:
    print("")' 2>/dev/null || true)"
inflight_resolve_teams_dir "$SESSION_ID"
TEAMS_DIR="$INFLIGHT_TEAMS_DIR_RESOLVED"
TIMEOUT_MIN="$(inflight_timeout_min "$ENTITY_ROOT")"

REPOS=""
[ -n "$ENTITY_ROOT" ] && REPOS="$ENTITY_ROOT"
if [ -n "$TEAMS_DIR" ] && [ -f "$TEAMS_DIR/inflight-repos.txt" ]; then
    while IFS= read -r r; do
        [ -n "$r" ] && [ -d "$r" ] && REPOS="$REPOS
$r"
    done < "$TEAMS_DIR/inflight-repos.txt"
fi
[ -n "$REPOS" ] || { stop_notice_normal ""; exit 0; }

SUMMARY="$(IF_LIB_DIR="$SCRIPT_DIR/../lib" IF_REPOS="$REPOS" IF_TEAMS="$TEAMS_DIR" \
           IF_SID="$SESSION_ID" IF_TRANSCRIPT="$TRANSCRIPT" \
           IF_TMO="$TIMEOUT_MIN" python3 -c '
import os, sys
sys.path.insert(0, os.environ["IF_LIB_DIR"])
import inflight

seen = []
overdue = []
failed = []
for repo in dict.fromkeys(r for r in os.environ["IF_REPOS"].split("\n") if r.strip()):
    if repo in seen:
        continue
    seen.append(repo)
    try:
        res = inflight.assess(repo, None, os.environ["IF_TEAMS"], int(os.environ["IF_TMO"]),
                              os.environ.get("IF_SID", ""),
                              os.environ.get("IF_TRANSCRIPT", ""))
    except Exception as exc:
        # An assessment that raised decided nothing about this repository: it is
        # NOT "everyone acked" (hunt part 3 v3, finding 28).
        failed.append("%s (%s)" % (os.path.basename(repo.rstrip("/")) or repo, exc.__class__.__name__))
        continue
    if res.get("error"):
        # A RETURNED error decided nothing either: same NOT CHECKED line.
        failed.append("%s (%s)" % (os.path.basename(repo.rstrip("/")) or repo, " ".join(str(res["error"]).split())))
    for wt in res["worktrees"]:
        # A teammate nobody messaged (OWED-NO-NOTICE) is owed nothing since
        # CEO 2026-10-06: it merges main itself before handover. A notice that
        # WAS sent for an earlier tip is still tracked (pending_notice).
        who = wt.get("resolved_name") or os.path.basename(wt["path"].rstrip("/"))
        if wt["verdict"] in ("NOTIFIED-NO-ACK", "NOTIFIED-ACK-INVALID"):
            ack = wt.get("ack") or {}
            if wt["verdict"] == "NOTIFIED-ACK-INVALID" or ack.get("overdue"):
                overdue.append(who)
        pend = wt.get("pending_notice")
        if pend and (pend.get("invalid") or (pend.get("ack") or {}).get("overdue")):
            overdue.append("%s (notice for %s)" % (who, pend["sha"][:12]))

parts = []
if overdue:
    parts.append("no ack past the timeout: " + ", ".join(sorted(set(overdue))))
if failed:
    parts.append("NOT CHECKED, the assessment failed for: " + ", ".join(failed))
print("\t".join(["|".join(sorted(set(overdue)) + ["not-checked:" + f for f in failed]),
                 "; ".join(parts)]))
' 2>/dev/null || printf 'not-checked:the sweep\tNOT CHECKED: the in-flight sweep itself failed to run')"

KEY="$(printf '%s' "$SUMMARY" | cut -f1)"
TEXT="$(printf '%s' "$SUMMARY" | cut -f2-)"

if [ -z "$KEY" ]; then
    stop_notice_normal "IN-FLIGHT SWEEP: no sent notice is waiting on an ack past the timeout. Nothing outstanding."
    exit 0
fi

case "$TEXT" in
    "NOT CHECKED"*)
        _NC_MSG="IN-FLIGHT SWEEP — $TEXT. Whether sent notices were acked is unknown this turn; run scripts/inflight-notify.sh status by hand."
        stop_notice_abnormal "outstanding:$KEY" "$_NC_MSG"
        exit 0 ;;
esac
stop_notice_abnormal "outstanding:$KEY" \
    "IN-FLIGHT SWEEP (rich-lander SKILL.md §8b) — $TEXT. They were sent a notice and have not acked it within ${TIMEOUT_MIN}min. If the fact is load-bearing, TaskStop + respawn with a corrected brief. That is yours to do: scripts/inflight-notify.sh status"
exit 0

#!/usr/bin/env bash
#
# notice-hook-staleness.sh — NON-BLOCKING Stop hook. Tells the OPERATOR, during
# the session, that a guard has been landed into the engine's hook table since
# this session booted, that it is therefore doing nothing right now, and that
# restarting the session is what arms it.
#
# ===========================================================================
# THE FAILURE (2026-08-30, this operation)
# ===========================================================================
# Six guards were landed in one day. The lander knew they were inert until the
# session restarted and said so — in the form "they arm at next session start".
# That sentence names a DATE. It names no ACTOR and no ACTION. The operator
# read it as something that would happen TO him rather than something he could
# do, in five seconds, whenever he liked; nobody ever told him restarting was
# an available move. He then hit a failure three of those six guards would have
# caught.
#
# The guards were fine. The sentence was not even false. What was missing is
# that a deferred activation had been reported as a forecast instead of a
# request, so nobody acted on it. Hence the rule this hook exists to make
# unforgettable, stated so it generalizes past hooks:
#
#   A DEFERRED ACTIVATION MUST NAME THE ACTOR AND THE ACTION.
#   Never "this arms at the next session" — always "restart the session to arm
#   this; that is the operator's to do." A state change that requires a human
#   action is not a date, it is a request.
#
# So this notice states three things EVERY time it fires, and a change that
# drops any of them has broken it:
#   1. WHICH guards are landed but not enforcing — named, derived, never typed;
#   2. that they are inert RIGHT NOW, for the rest of this session;
#   3. that RESTARTING THE SESSION arms them, and that this is the operator's
#      to do. A notice that reports drift without naming the remedy would
#      rebuild the original failure in a new place.
#
# ===========================================================================
# WHY Stop, AND WHY systemMessage — ESTABLISHED AGAINST THE BINARY, NOT ASSUMED
# ===========================================================================
# The notice has to reach the operator DURING the session. The next session
# start is exactly when the problem has already solved itself, so announcing it
# there would be a status line about a thing that is no longer true.
#
# Measured against the shipping binary (2.1.251) on 2026-08-30:
#   * A Stop hook that exits 0 and prints {"systemMessage": "..."} on stdout
#     surfaces to the OPERATOR. Run live: the stream carried
#     {"type":"system","subtype":"informational","content":"Stop says: <msg>",
#     "level":"notice"}. The binary's own hook documentation gives this exact
#     recipe under the heading "Stop hook that displays message to user", and
#     lists systemMessage as "Display a message to the user (all hooks)".
#   * The other channels do NOT reach the human. hookSpecificOutput.
#     additionalContext is documented as "Text injected into model context";
#     stderr from a non-blocking hook reaches the debug log. Both were the
#     failure mode engine-status.sh was rebuilt to escape on 2026-08-28, when an
#     operator looked for a banner that had gone somewhere only the model could
#     see and concluded the engine was not loaded.
#
# THE OPERATOR IS THE AUDIENCE FOR THE REMEDY, and that has not changed. The
# remedy is a session restart, and the binary's own hook-authoring guidance is
# explicit that the assistant cannot do it: "Tell the user to open /hooks once
# (reloads config) or restart — you can't do this yourself."
#
# THE MODEL IS A SECOND AUDIENCE FOR THE FACT — added 2026-09-14, and the
# original reasoning above is corrected rather than deleted, because it was
# right about the remedy and wrong to conclude from that that the model does
# not need the fact.
#
# What it cost: on 2026-09-14 this hook fired correctly and named
# guard-hook-registration-commits.sh as landed-and-inert. The announcement went
# out on systemMessage alone. The orchestrator never saw it, and twelve hours
# later briefed an engineer that the guard was "present in the booting session's
# hook set" — quoting the snapshot file as proof. It was not in that session's
# hook set. The commit that guard exists to refuse went through it in silence,
# `main` went red, and the repair cost a six-figure-token agent.
#
# THE DISTINCTION THE ORIGINAL ARGUMENT MISSED: "who can perform the remedy" and
# "who needs to know the fact" are different questions. An assistant that cannot
# learn its guards are off will keep reasoning as though they are on, will read
# a clean run as enforcement, and will write that belief into the next brief as
# an established premise. So the fact goes to both; the request still names the
# operator, in the message text, exactly as before.
#
# ONE Stop hook already exists (guard-unresolved-claims.sh) and this is
# deliberately NOT folded into it. That one is a blocking gate on report
# integrity that fails open; this one is a non-blocking status notice. Sharing a
# process would mean one of them inheriting the other's exit-code contract.
#
# ===========================================================================
# WHAT IS COMPARED, AND WHY EXACTLY THIS — MEASURED, NOT ASSUMED
# ===========================================================================
# Only the PLUGIN registration surface, hooks/hooks.json. The full experimental
# record is in the partner hook's header; the result:
#
#   hooks/hooks.json ............ FROZEN at session start. A hook added to a
#                                 loaded plugin's table mid-session never fired
#                                 (negative control: a hook already in that
#                                 table fired three times in the same run).
#   .claude/settings.local.json . HOT-RELOADS. A hook appended to a project
#                                 settings file fired on the very next tool
#                                 call, same session.
#   hook script BODIES .......... NOT frozen. A registration names `bash
#                                 <path>`, executed afresh per event.
#
# Comparing the settings surface, or script hashes, would produce a confident
# false positive on every settings edit and every guard body edit — reporting as
# INERT something that was already enforcing. The engine registers the same
# guards on both surfaces, so "check both for completeness" was a real
# temptation and would have been exactly wrong.
#
# ===========================================================================
# ZERO FALSE POSITIVES — WHY THAT IS ACHIEVABLE HERE AND NOT MERELY HOPED FOR
# ===========================================================================
# The comparison is a recorded set of facts against a re-derived set of the same
# facts, from the same file, through the same parser (registered_hook_rows).
# There is no threshold, no heuristic, no prose, and nothing to tune. If a
# design for this needed a threshold it had taken a wrong branch. A session in
# which hooks/hooks.json did not change produces the empty delta and this hook
# prints NOTHING AT ALL — asserted by the suite, because "quiet when nothing
# happened" is the property that keeps a notice worth reading.
#
# NEGATIVE CONTROL. A comparison that read nothing would produce an empty delta
# too, and would look identical to a clean run. So a green run must additionally
# prove it examined something:
#   * the baseline's `rows=<n>` header must agree with the number of rows
#     actually parsed out of it, and be >= 1;
#   * the current derivation must yield >= 1 row;
#   * the baseline's `engine=` must match the engine this hook resolved, or the
#     two sides are not descriptions of the same thing.
# Any of those failing means the comparison CANNOT BE MADE HONESTLY, and this
# hook says so instead of passing quietly. This operation has already shipped
# one scanner that reported CLEAN over an empty corpus and one reporting layer
# that was dead for weeks; this is the check that stops a third.
#
# ===========================================================================
# CADENCE — ONCE, THEN AGAIN ONLY IF IT GROWS
# ===========================================================================
# A notice on every turn is noise, noise gets muted, and a muted notice is worse
# than none. So the announced delta is recorded in
# .claude/state/hook-staleness-<session8>.announced, and this hook speaks only
# when the current delta is NOT a subset of what has already been announced —
# i.e. the first time drift appears, and again each time it GROWS. A delta that
# shrinks (someone reverted) says nothing new and stays quiet.
#
# NEVER BLOCKS. This is information, not enforcement. A stale hook set is not a
# reason to refuse work; it is a reason to tell the operator to restart. Every
# path exits 0.
#
# TEST OVERRIDES: HOOK_STALENESS_ROOT forces the governed root and
# HOOK_STALENESS_SURFACE forces the hooks.json path (test-only; never set in a
# real session). The partner snapshotter honours both.
#
# Exit codes: 0, always.
#
# WHEN THIS HOOK ITSELF ARMS — the rule above, applied to itself, as a request
# rather than a date: it is registered in hooks/hooks.json, which the host reads
# once at session start, so in the session that MERGES it, it is inert and will
# not fire. TO ARM IT: re-run scripts/hooks/install.sh, then RESTART THE
# SESSION. That is the operator's to do; no hook and no model can do it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

HOOK_TAG="(hook: scripts/hooks/notice-hook-staleness.sh)"

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
  hook: scripts/hooks/notice-hook-staleness.sh
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

INPUT="$(cat 2>/dev/null || true)"

# emit <message>
#
# TWO channels, because this notice has TWO audiences and the measured table
# above says no single channel reaches both.
#
#   systemMessage    -> the PERSON. Only they can restart the session, and the
#                       message says so ("THIS IS YOURS TO DO"). This was the
#                       original and correct choice.
#   additionalContext -> the MODEL. Added 2026-09-14, and the reason is a
#                       specific failure rather than symmetry.
#
# WHY THE ASSISTANT HAS TO HEAR THIS TOO. On 2026-09-14 this hook fired
# correctly and named guard-hook-registration-commits.sh as landed-and-inert.
# It went out on systemMessage alone, so the ORCHESTRATOR never saw it — and
# twelve hours later it briefed an engineer with "present in the booting
# session's hook set", quoting the snapshot file as proof. It was not in that
# session's hook set, the commit the guard exists to refuse went through in
# silence, and `main` went red.
#
# An assistant that cannot learn its guards are off will keep reasoning as if
# they are on, will report a clean run as enforcement, and will write the
# belief into the next brief. Telling only the operator makes the assistant a
# confident source of a false premise. This hook is registered on Stop and
# nowhere else, so naming Stop in the envelope is exact rather than a guess.
emit() {
    local msg="$1"
    if command -v python3 >/dev/null 2>&1; then
        MSG="$msg" python3 - <<'PY' 2>/dev/null || true
import json, os
m = os.environ.get("MSG", "")
print(json.dumps({
    "systemMessage": m,
    "hookSpecificOutput": {"hookEventName": "Stop", "additionalContext": m},
}))
PY
    else
        local escaped="${msg//\\/\\\\}"
        escaped="${escaped//\"/\\\"}"
        escaped="${escaped//$'\n'/\\n}"
        printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"Stop","additionalContext":"%s"}}\n' \
            "$escaped" "$escaped"
    fi
}

# announce_once <newline-separated delta keys> <message>
#
# Speaks only when the delta is not already covered by what has been announced
# in this session, then records the union. Defined BEFORE root resolution
# because cannot_compare() below is reachable from the root-failure path, where
# ANNOUNCED is not yet known — with no state file to dedupe against it emits
# every time, on the principle that a repeated true notice is a smaller failure
# than a suppressed one.
ANNOUNCED=""
STATE_DIR=""
announce_once() {
    local keys="$1" msg="$2" prev="" unseen=""
    if [ -z "$ANNOUNCED" ]; then
        emit "$msg"
        return 0
    fi
    [ -f "$ANNOUNCED" ] && prev="$(cat "$ANNOUNCED" 2>/dev/null || true)"
    while IFS= read -r k; do
        [ -n "$k" ] || continue
        printf '%s\n' "$prev" | grep -qxF "$k" || unseen="yes"
    done <<ANN_EOF
$keys
ANN_EOF
    [ -n "$unseen" ] || return 0
    emit "$msg"
    mkdir -p "$STATE_DIR" 2>/dev/null || true
    { printf '%s\n' "$prev"; printf '%s\n' "$keys"; } \
        | grep -v '^$' | LC_ALL=C sort -u >"$ANNOUNCED.tmp.$$" 2>/dev/null \
        && mv -f "$ANNOUNCED.tmp.$$" "$ANNOUNCED" 2>/dev/null \
        || rm -f "$ANNOUNCED.tmp.$$" 2>/dev/null || true
}

# cannot_compare <one-line reason>
#
# FAIL OPEN, BUT NEVER QUIET. "I could not check" and "I checked and found
# nothing" are different sentences, and this engine has been burned by systems
# that print the second when they mean the first. Deduplicated like any other
# notice so a broken install does not shout once per turn forever.
cannot_compare() {
    announce_once "$(printf 'cannot-compare\t%s' "$1")" \
"RichOS engine: CANNOT CHECK whether a landed guard is inert in this session.
  reason: $1
This is NOT a clean result — it is the absence of one. Until it is fixed, a
guard landed into the engine's hooks/hooks.json mid-session will sit there
enforcing nothing and nothing will tell you.
$HOOK_TAG"
    exit 0
}

if [ -n "${HOOK_STALENESS_ROOT:-}" ]; then
    RICHOS_ENTITY_ROOT="$HOOK_STALENESS_ROOT"
fi

# Three outcomes; all three let the turn end (see NEVER BLOCKS above).
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Nothing is enforced here, so nothing can be inert here. The engine plugin
    # is enabled at user scope and loads in every directory on the machine; a
    # notice in each of them would be pure noise. engine-status.sh already
    # announces the stand-down at session start.
    exit 0
else
    root_failure_banner "scripts/hooks/notice-hook-staleness.sh" >&2
    cannot_compare "root resolution failed — ${RICHOS_ROOT_REASON}"
fi

STATE_DIR="$ENTITY_ROOT/.claude/state"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# THIS HOOK ALREADY OWNS THE RIGHT SENTENCE and simply never routed an
# unreadable payload into it. cannot_compare() says "This is NOT a clean result
# — it is the absence of one", which is exactly the statement owed here, so this
# reuses it rather than adding a second voice; richos_payload_unreadable is
# shared only for the judgment of WHAT counts as unreadable.
#
# IT ALSO CLOSES A WRITE. With an unreadable payload SESSION_ID comes out empty
# a few lines below, ANNOUNCED falls back to the NON-session-scoped
# hook-staleness-latest.announced, and the run then suppresses the next real
# staleness announcement for everybody. The 2026-09-05 survey found that file
# was the one piece of state a degraded payload could write and recorded it as a
# small finding it did not pursue. Standing down here means it is never reached.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        cannot_compare "the turn-end payload could not be read ($_UE_REASON), so this session was never identified and nothing was compared"
    fi
fi
SURFACE="${HOOK_STALENESS_SURFACE:-$ENGINE_ROOT/hooks/hooks.json}"

# --- session-scoped state ------------------------------------------------
# Session-scoped ONLY when a session id is present. Falling back to `latest`
# when one IS present could compare this session's set against a DIFFERENT
# session's baseline (two concurrent sessions), inventing drift that never
# happened — the same reasoning, and the same rule, as
# guard-definition-drift.sh.
SESSION_ID=""
if command -v python3 >/dev/null 2>&1; then
    SESSION_ID="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(str(d.get("session_id", "") or "") if isinstance(d, dict) else "")
except Exception:
    print("")
' 2>/dev/null || true)"
fi
[ -n "$SESSION_ID" ] || SESSION_ID="${CLAUDE_SESSION_ID:-}"
SESSION_SHORT="$(printf '%s' "$SESSION_ID" | tr -cd '[:alnum:]-' | cut -c1-8)"

if [ -n "$SESSION_SHORT" ]; then
    SNAP_PATH="$STATE_DIR/enforcing-hooks-${SESSION_SHORT}.snapshot"
    ANNOUNCED="$STATE_DIR/hook-staleness-${SESSION_SHORT}.announced"
else
    SNAP_PATH="$STATE_DIR/enforcing-hooks-latest.snapshot"
    ANNOUNCED="$STATE_DIR/hook-staleness-latest.announced"
fi

# --- both sides, derived by the same parser ------------------------------
_RH_LIB="$SCRIPT_DIR/../lib/registered-hooks.sh"
[ -f "$_RH_LIB" ] || cannot_compare "scripts/lib/registered-hooks.sh is missing at $_RH_LIB, so the enforcing set cannot be derived at all"
# shellcheck source=../lib/registered-hooks.sh
. "$_RH_LIB"

[ -f "$SNAP_PATH" ] || cannot_compare "no session-start baseline at ${SNAP_PATH#"$ENTITY_ROOT"/} — snapshot-enforcing-hooks.sh was not live at the last session start. It is registered in the engine's hook table, which the host reads once per session, so it begins recording baselines only after the operator RESTARTS. Re-run scripts/hooks/install.sh, then restart this session; that is yours to do, no hook can do it."

BASE_ROWS="$(grep -v '^#' "$SNAP_PATH" 2>/dev/null | grep -v '^$' | LC_ALL=C sort -u || true)"
BASE_N="$(printf '%s\n' "$BASE_ROWS" | grep -c . || true)"
BASE_CLAIMED="$(sed -n 's/^#.*[[:space:]]rows=\([0-9][0-9]*\).*$/\1/p' "$SNAP_PATH" 2>/dev/null | head -1)"
BASE_ENGINE="$(sed -n 's/^#.*[[:space:]]engine=\(.*\)[[:space:]]surface=.*$/\1/p' "$SNAP_PATH" 2>/dev/null | head -1)"

# --- NEGATIVE CONTROL ----------------------------------------------------
# Everything below this point may only report "no drift" if it can first show
# it read a non-zero, self-consistent corpus on BOTH sides. An empty or
# truncated baseline yields an empty delta, which is indistinguishable from a
# clean session unless it is checked for.
[ "${BASE_N:-0}" -ge 1 ] || cannot_compare "the session-start baseline ${SNAP_PATH#"$ENTITY_ROOT"/} contains ZERO hook rows. An empty baseline would make every comparison against it report 'no drift' for the wrong reason"
[ -n "$BASE_CLAIMED" ] || cannot_compare "the session-start baseline ${SNAP_PATH#"$ENTITY_ROOT"/} has no rows= header, so there is nothing to check its row count against"
[ "$BASE_CLAIMED" -eq "$BASE_N" ] 2>/dev/null || cannot_compare "the session-start baseline ${SNAP_PATH#"$ENTITY_ROOT"/} says rows=$BASE_CLAIMED but $BASE_N row(s) parsed out of it — the file is truncated or corrupt and cannot be compared against"
if [ -n "$BASE_ENGINE" ] && [ "$BASE_ENGINE" != "$ENGINE_ROOT" ]; then
    cannot_compare "the baseline was recorded against engine $BASE_ENGINE but this hook is running from $ENGINE_ROOT — two different engines, so any difference between them would be a difference of subject, not drift"
fi

if ! CUR_ROWS="$(registered_hook_rows "$SURFACE")"; then
    cannot_compare "could not derive the current enforcing set from $SURFACE (missing, unparseable, registering nothing, or no python3)"
fi
CUR_ROWS="$(printf '%s\n' "$CUR_ROWS" | grep -v '^$' | LC_ALL=C sort -u)"
CUR_N="$(printf '%s\n' "$CUR_ROWS" | grep -c . || true)"
[ "${CUR_N:-0}" -ge 1 ] || cannot_compare "$SURFACE currently registers ZERO hook scripts, so there is nothing to compare the baseline against"

# --- the delta -----------------------------------------------------------
# Three kinds, all of them facts about two files:
#   NEW-SCRIPT  a script wired now whose basename was absent at session start
#               -> landed this session, enforcing NOTHING at all. The headline.
#   NEW-WIRING  a script that WAS wired at session start, now also wired to a
#               further event/matcher -> that hook point is not live.
#   DROPPED     a registration present at session start and gone now -> the
#               session is still firing something the table no longer asks for.
# LC_ALL=C on comm as well as on the sorts that fed it: comm compares under the
# ambient collation, and a set difference taken between two orderings is not a
# set difference at all — it invents rows in both directions.
ADDED="$(LC_ALL=C comm -13 <(printf '%s\n' "$BASE_ROWS") <(printf '%s\n' "$CUR_ROWS") 2>/dev/null || true)"
REMOVED="$(LC_ALL=C comm -23 <(printf '%s\n' "$BASE_ROWS") <(printf '%s\n' "$CUR_ROWS") 2>/dev/null || true)"

BASE_SCRIPTS="$(printf '%s\n' "$BASE_ROWS" | awk -F'\t' 'NF{print $3}' | LC_ALL=C sort -u)"

NEW_SCRIPT_LINES=""
NEW_WIRING_LINES=""
KEYS=""
while IFS=$'\t' read -r ev matcher script; do
    [ -n "${script:-}" ] || continue
    where="$ev"
    [ "$matcher" = "-" ] || where="$ev[$matcher]"
    if printf '%s\n' "$BASE_SCRIPTS" | grep -qxF "$script"; then
        NEW_WIRING_LINES="${NEW_WIRING_LINES}    ${script} — newly wired on ${where}"$'\n'
        KEYS="${KEYS}NEW-WIRING	${ev}	${matcher}	${script}"$'\n'
    else
        NEW_SCRIPT_LINES="${NEW_SCRIPT_LINES}    ${script} — registered on ${where}"$'\n'
        KEYS="${KEYS}NEW-SCRIPT	${script}"$'\n'
    fi
done <<ADD_EOF
$ADDED
ADD_EOF

DROPPED_LINES=""
while IFS=$'\t' read -r ev matcher script; do
    [ -n "${script:-}" ] || continue
    where="$ev"
    [ "$matcher" = "-" ] || where="$ev[$matcher]"
    DROPPED_LINES="${DROPPED_LINES}    ${script} — was wired on ${where} at session start, no longer in the table"$'\n'
    KEYS="${KEYS}DROPPED	${ev}	${matcher}	${script}"$'\n'
done <<REM_EOF
$REMOVED
REM_EOF

# THE QUIET PATH. Nothing changed, the corpus was non-empty on both sides and
# self-consistent: say nothing whatsoever. Verified by the suite.
[ -n "$KEYS" ] || exit 0

N_INERT="$(printf '%s\n' "$NEW_SCRIPT_LINES$NEW_WIRING_LINES" | grep -c . || true)"
N_DROPPED="$(printf '%s\n' "$DROPPED_LINES" | grep -c . || true)"

MSG="RichOS engine: the hook table on disk NO LONGER MATCHES the one this session booted with."
if [ "${N_INERT:-0}" -gt 0 ]; then
    MSG="$MSG
  ${N_INERT} guard(s) LANDED SINCE THIS SESSION STARTED — on disk, registered, and ENFORCING NOTHING RIGHT NOW:"
    [ -n "$NEW_SCRIPT_LINES" ] && MSG="$MSG
${NEW_SCRIPT_LINES%$'\n'}"
    [ -n "$NEW_WIRING_LINES" ] && MSG="$MSG
${NEW_WIRING_LINES%$'\n'}"
fi
if [ "${N_DROPPED:-0}" -gt 0 ]; then
    MSG="$MSG
  ${N_DROPPED} registration(s) REMOVED since this session started — this session is still firing them:
${DROPPED_LINES%$'\n'}"
fi

MSG="$MSG

The host reads the engine's hooks/hooks.json ONCE, at session start. Everything
listed above will stay exactly as it is for the rest of this session, no matter
what else you do.

TO ARM THEM: RESTART THIS SESSION. It takes five seconds and you can do it right
now. (/reload-plugins re-reads a plugin's hook table too.) THIS IS YOURS TO DO —
a hook cannot restart a session and neither can the assistant. Until you do,
treat every guard listed above as absent: it will not catch anything.

Compared ${CUR_N} current registration(s) against the ${BASE_N} this session
booted with. $HOOK_TAG"

announce_once "$KEYS" "$MSG"
exit 0

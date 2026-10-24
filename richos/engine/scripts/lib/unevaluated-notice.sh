#!/usr/bin/env bash
#
# scripts/lib/unevaluated-notice.sh — THE CHANNEL A PreToolUse GUARD USES TO
#                                     SAY IT NEVER LOOKED AT THE CALL.
#
# ===========================================================================
# THE PROPERTY THIS FILE EXISTS FOR
# ===========================================================================
#
#     THE ABSENCE OF A FINDING MUST BE DISTINGUISHABLE FROM THE ABSENCE OF A
#     CHECK.
#
# That is this project's most-repeated failure written as a requirement, and it
# is the whole content of this file. Nothing here refuses anything, nothing here
# changes a verdict, and nothing here fires on a call a guard actually read.
#
# ===========================================================================
# WHAT WAS MEASURED (survey, 2026-09-05)
# ===========================================================================
# docs/verification/hook-payload-failure-modes-2026-09-05.md drove all 40
# registered PreToolUse and Stop hooks with an EMPTY payload, a TRUNCATED one
# and one that is not JSON at all. Nineteen of the twenty-five PreToolUse guards
# passed the call, and SEVENTEEN of those did it in complete silence — exit 0,
# nothing on stdout, nothing on stderr, byte-for-byte indistinguishable from a
# guard that looked at the call and approved it. `bash -x` step counts separate
# the two beyond argument: guard-worktree-isolation.sh executes 275 lines when
# it refuses an unisolated spawn and 129 on each degraded payload. It does not
# decide the spawn is safe. It never looks at it.
#
# THE MECHANICAL CAUSE IS ONE LINE, AND IT IS THE SAME LINE IN EVERY GUARD.
# Each one dispatches on the tool name first:
#
#     TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c '...' 2>/dev/null || true)"
#     [ "$TOOL_NAME" = "Agent" ] || exit 0
#
# On an unreadable payload the extraction fails, `|| true` turns that into an
# EMPTY string, and the guard exits at the dispatch line — taking the identical
# exit that a well-formed payload for some OTHER tool takes. "This call is not
# mine" and "I could not tell whose call this is" are collapsed into one silent
# exit 0, which is why most of these guards never even reach their own
# PARSEFAIL branch. This file separates those two again.
#
# ===========================================================================
# THE CHANNEL, MEASURED RATHER THAN ASSUMED
# ===========================================================================
# scripts/lib/stop-hook-notice.sh carries the equivalent measurement for Stop
# hooks. The same question had never been answered for PreToolUse, so it was
# measured the same way (claude 2.1.261, macOS, 2026-09-05): a sandbox project
# registered ONE PreToolUse[Bash] hook that wrote a unique marker to stderr AND
# a different marker as a stdout {"systemMessage":...}, then exited 0, and one
# headless session was driven through a single Bash call with
# `--output-format stream-json`.
#
#   channel                                  operator stream
#   --------------------------------------   ---------------
#   stderr,  exit 0  (CHANPROBE-STDERR)            NO
#   {"systemMessage":...}, exit 0                  YES
#
# The systemMessage arrived as
#   {"type":"system","subtype":"informational","level":"notice",
#    "content":"PreToolUse:Bash says: CHANPROBE-SYSMSG: predicate not evaluated"}
# and the stderr marker appeared NOWHERE in the stream. The tool call itself ran
# to completion in the same session, which is the positive control: emitting a
# systemMessage with NO `permissionDecision` leaves the verdict exactly where it
# was.
#
# So this file writes BOTH. stderr because it is where a hook's diagnostics
# belong and it survives into the transcript attachment; systemMessage because
# it is the only one of the two that reaches the person who needs to know a
# guard stopped guarding. guard-definition-drift.sh — the guard that already got
# this right in shape — announces on stderr ALONE, so on the operator channel
# its warning has never been heard. Routing it through here is what makes the
# template honest.
#
# ===========================================================================
# WHY IT SPEAKS EVERY TIME, WHERE THE Stop CHANNEL SPEAKS ON STATE CHANGE
# ===========================================================================
# stop-hook-notice.sh announces a stand-down only when the state CHANGES, and
# argues the case at length: a stand-down is a persistent condition, identical
# text under every turn is text the eye learns to skip, and a muted notice is
# worse than none.
#
# That argument does not transfer, for a reason of kind rather than degree. A
# stood-down Stop hook is ONE condition that persists across many turns. An
# unreadable payload is a PROPERTY OF ONE CALL: this spawn, this write, this
# command was allowed by a guard that never read it. Suppressing the second
# occurrence would not be de-duplicating a condition, it would be declining to
# mention the second unexamined call — which is the defect, one level up.
#
# Held against the real numbers, the noise fear is also hypothetical where the
# silence is measured. Every payload in an ordinary session is well-formed; the
# survey had to CONSTRUCT the degraded ones. A session in which this fires
# repeatedly is a session in which enforcement has systematically stopped, and
# being told that repeatedly is the correct outcome.
#
# There is a durable record as well, for the same reason resume-acks.log and
# definition-drift.log exist: an operator channel is something a person may miss,
# and .claude/state/unevaluated-payloads.log is something a later reader can
# grep. The log line is de-duplicated on CONTENT against the previous line
# (never on the timestamp), the fix already made in guard-definition-drift.sh's
# append_log for double-registered hooks straddling a UTC-second boundary.
#
# ===========================================================================
# WHAT THIS FILE DELIBERATELY DOES NOT DO
# ===========================================================================
# It does not make any guard fail CLOSED. Which gates are worth the risk of
# bricking a session is a judgment that differs per gate and it has not been
# taken; the survey's recommendation 3 names the candidates and says so. Every
# caller here allows exactly what it allowed before, and that is what makes this
# safe to apply across many guards at once.
#
# It does not bound the read of stdin either. The registered `timeout` in
# hooks/hooks.json is honored by the host — measured three ways on 2026-09-05
# and recorded in docs/verification/unevaluated-payload-notice-2026-09-05.md —
# so a stalled hook is already stopped and its call already proceeds. Bounding
# helps the two guards that genuinely fail closed and no others.
#
# ===========================================================================
# USAGE — one line at the call site
# ===========================================================================
#     _UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
#     [ -f "$_UE_LIB" ] && . "$_UE_LIB"
#     unevaluated_or_continue "guard-example.sh" "$INPUT" \
#         "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
#         "whether this write lands in the main checkout"
#
# Placed AFTER root resolution (so an unadopted repository still stands down in
# silence — the plugin is enabled at user scope and loads in every project on
# the machine) and BEFORE the tool-name dispatch (so it fires at the line the
# call is actually being lost at). If the payload parses as a JSON object the
# call returns immediately and the guard runs exactly as it did before.

# --- is this payload readable at all? --------------------------------------
# Answers ONE question and refuses to guess at any other: does this text parse
# as a JSON object? Not "does it carry the fields I want" — a payload missing a
# field is a different finding, belongs to the guard that wants the field, and
# is not what the survey measured.
#
# Returns 0 (unreadable) with a one-word reason on stdout, or 1 (readable).
#
# NO python3 IS NOT REPORTED AS UNREADABLE. Without it nothing here can tell a
# good payload from a bad one, and announcing "unreadable" on no evidence would
# be a claim this file cannot support — the exact defect it exists to remove,
# pointed the other way. Guards that need python3 already say so themselves.
#
# MEMO, OFF BY DEFAULT. With _UE_MEMO_ENABLE=1 a repeated identical payload is
# answered from the previous answer rather than by forking python3 again. This
# function is a pure function of its argument — it reads no file, no clock and
# no environment — so unlike the root resolver the memo has no correctness
# caveat at all; it is opt-in only so that no existing caller's behavior can
# change by accident. One caller opts in: scripts/hooks/dispatch-pretooluse.sh,
# which runs twelve rules against ONE payload in one process and would
# otherwise pay twelve python3 forks (14.64 ms each, measured 2026-09-15) to
# answer "is this JSON?" twelve times about the same bytes.
richos_payload_unreadable() { # <raw payload>  ->  0 = unreadable (+ reason), 1 = readable
    local payload="${1-}"
    if [ "${_UE_MEMO_ENABLE:-0}" = "1" ] && [ "${_UE_MEMO_KEY+set}" = "set" ] \
       && [ "$_UE_MEMO_KEY" = "$payload" ]; then
        [ -n "$_UE_MEMO_REASON" ] && printf '%s' "$_UE_MEMO_REASON"
        return "$_UE_MEMO_RC"
    fi
    if [ "${_UE_MEMO_ENABLE:-0}" = "1" ]; then
        _UE_MEMO_KEY="$payload"
        _UE_MEMO_REASON="$(_ue_payload_unreadable_uncached "$payload")"
        _UE_MEMO_RC=$?
        [ -n "$_UE_MEMO_REASON" ] && printf '%s' "$_UE_MEMO_REASON"
        return "$_UE_MEMO_RC"
    fi
    _ue_payload_unreadable_uncached "$payload"
}

_ue_payload_unreadable_uncached() { # <raw payload>  ->  0 = unreadable (+ reason), 1 = readable
    local payload="${1-}"
    if [ -z "$payload" ]; then
        printf 'empty'
        return 0
    fi
    command -v python3 >/dev/null 2>&1 || return 1
    if printf '%s' "$payload" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(1)
sys.exit(0 if isinstance(d, dict) else 2)
' >/dev/null 2>&1; then
        return 1
    fi
    printf 'not-json'
    return 0
}

# --- the durable half ------------------------------------------------------
# TAKES THE ENTITY ROOT, NOT THE STATE DIR. Every call site can then use one
# byte-identical expression — "${ENTITY_ROOT:-${SEAT_ROOT:-...}}" — whatever the
# hook happens to have named its root, and a hook that reaches this before it
# has resolved a root passes an empty string and simply gets no log line. The
# announcement never depends on it.
_ue_log() { # <entity root> <content key>
    local root="${1-}" key="${2-}" dir logfile last new
    [ -n "$root" ] || return 0
    dir="$root/.claude/state"
    logfile="$dir/unevaluated-payloads.log"
    mkdir -p "$dir" 2>/dev/null || return 0
    new="$(printf '%s\t%s' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$key")"
    last=""
    [ -f "$logfile" ] && last="$(tail -n 1 "$logfile" 2>/dev/null | cut -f2- || true)"
    if [ "$key" != "$last" ]; then
        printf '%s\n' "$new" >>"$logfile" 2>/dev/null || true
    fi
    return 0
}

# --- the operator half -----------------------------------------------------
# THE SENTENCE IS NOT NEW. guard-ceo-ask-first.sh (removed 2026-10-24) and guard-ceo-ruled-ask.sh were
# the two PreToolUse guards the survey found already doing this correctly, and
# they say, verbatim:
#
#   "CEO-ASK GATE: could not parse this Agent spawn, so it was not checked and
#    the 'ceo-todos-deferred:' escape hatch could not be read either. This ONE
#    dispatch is ungated."
#
# Label, what was not checked, and the scope of the damage — one call, not the
# session. That grammar is copied here rather than improved on, so all of these
# notices read as one voice and one grep finds every one of them. Those two
# guards are deliberately NOT rewired to this library: they are already right,
# and the derived test below holds them to the same words it holds everyone
# else to, which is a stronger check than making them share an implementation.
#
# The label is DERIVED from the file name rather than passed in. A second name
# for a hook is a second thing to keep in step with the first.
#
# UPPERCASED IN PURE BASH, not with `tr`. This runs in the failure path of a
# guard, which is the last place that should acquire a new dependency on an
# external binary: a PATH without `tr` would have produced a hook that emitted
# `tr: command not found` and a label-less sentence, which is a notice about a
# broken check that is itself visibly broken. bash 3.2 is what /usr/bin/env bash
# resolves to on macOS and it has no ${var^^}, so this is the portable form.
_ue_upper() { # <text> -> TEXT
    local s="${1-}" out="" c head
    local lower="abcdefghijklmnopqrstuvwxyz"
    local upper="ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    while [ -n "$s" ]; do
        c="${s%"${s#?}"}"
        s="${s#?}"
        head="${lower%%"$c"*}"
        if [ "$head" != "$lower" ]; then
            c="${upper:${#head}:1}"
        fi
        out="$out$c"
    done
    printf '%s' "$out"
}

_ue_label() { # <hook basename.sh> -> WORKTREE-ISOLATION GUARD
    local n="${1-}"
    n="${n%.sh}"
    n="${n#guard-}"
    n="${n#notice-}"
    printf '%s GUARD' "$(_ue_upper "$n")"
}

# ONE SENTENCE, TWO EVENTS. A PreToolUse guard is talking about a CALL it let
# through; a Stop hook is talking about a TURN it let end. The noun is the only
# thing that differs, and it is a parameter rather than a second template,
# because two templates is how a shared voice becomes two voices that drift.
#
# The Stop hooks do NOT emit through announce_unevaluated. They build the
# sentence here and hand it to stop_notice_abnormal, which is the channel
# scripts/lib/stop-hook-notice.sh measured for that event and which
# de-duplicates on state change. That difference is deliberate: a PreToolUse
# announcement is about one call and must repeat, while a Stop announcement is
# about a persistent condition seen once per turn, which is exactly the case
# stop-hook-notice.sh argues should speak on entry and then stay quiet.
unevaluated_sentence() { # <hook basename> <what was not checked> <reason word> <noun: call|turn>
    local hook="${1-}" what="${2-}" reason="${3-}" noun="${4:-call}" scope
    case "$reason" in
        empty) reason="the payload was empty" ;;
        *)     reason="the payload is not readable JSON" ;;
    esac
    case "$noun" in
        turn) scope="This turn ENDED UNCHECKED" ;;
        *)    scope="This ONE call is UNGATED" ;;
    esac
    printf '%s: could not read this %s (%s), so %s was NOT checked. %s — nothing looked at it, which is not the same as nothing being wrong with it. (hook: scripts/hooks/%s)' \
        "$(_ue_label "$hook")" "$noun" "$reason" "$what" "$scope" "$hook"
}

announce_unevaluated() { # <hook basename> <what was not checked> <reason word> [<entity root>]
    local hook="${1-}" what="${2-}" reason="${3-}" dir="${4-}" msg
    msg="$(unevaluated_sentence "$hook" "$what" "$reason" "call")"
    printf '%s\n' "$msg" >&2
    # THE OPERATOR CHANNEL DOES NOT GET TO DEPEND ON python3. This whole file
    # runs in a failure path; a notice that is itself conditional on a working
    # environment is the defect one level up. python3 does the escaping when it
    # is there, and the fallback covers the only two characters these sentences
    # can contain that JSON cares about — the text is a fixed template plus a
    # constant authored at the call site, never anything from the payload.
    local emitted=0
    if command -v python3 >/dev/null 2>&1; then
        if UE_MSG="$msg" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("UE_MSG", "")}))
' 2>/dev/null; then
            emitted=1
        fi
    fi
    if [ "$emitted" -eq 0 ]; then
        local esc="$msg"
        esc="${esc//\\/\\\\}"
        esc="${esc//\"/\\\"}"
        printf '{"systemMessage":"%s"}\n' "$esc"
    fi
    _ue_log "$dir" "$(printf 'unevaluated\thook=%s\treason=%s' "$hook" "${3-}")"
    return 0
}

# --- the one line a caller writes ------------------------------------------
# Announces and EXITS 0 when the payload cannot be read; returns silently when
# it can. Exiting 0 is not a new verdict: every guard wired to this already
# exited 0 on exactly these payloads, which is what the survey measured. The
# only thing that changes is that somebody is told.
unevaluated_or_continue() { # <hook basename> <raw payload> <entity root> <what was not checked>
    local reason
    reason="$(richos_payload_unreadable "${2-}")" || return 0
    announce_unevaluated "${1-}" "${4-}" "$reason" "${3-}"
    exit 0
}

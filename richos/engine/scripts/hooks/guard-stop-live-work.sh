#!/usr/bin/env bash
#
# guard-stop-live-work.sh — BLOCKING PreToolUse guard on the TaskStop tool.
#
# ONE RULE: DESTROYING A LIVE TEAMMATE'S WORK REQUIRES A WRITTEN, CHECKABLE
# REASON ON DISK BEFORE THE CALL.
#
# THIS GUARD DOES NOT READ THE CEO'S SENTENCES. It used to, and what that cost
# is the second failure below.
#
# ===========================================================================
# THE FAILURE, 2026-09-10
# ===========================================================================
# He wrote: "the currently running agents MIGHT need to be paused IF we get
# close to hitting the quota before it resets." Within a minute the
# orchestrator called TaskStop on two mid-flight teammates and reported it back
# as compliance. No burn rate had been measured; no condition had been
# established; nothing had been asked.
#
# `echo-opus-hw2` had no commits and a clean worktree. Its entire session is
# gone. `zach-opus-dor1` survived only because it had committed as it went.
#
# "Did I say anything about stopping???"
#
# Fourth instance in one day of one mechanism — an inference about what he
# wanted, substituted for what he said, then acted on expensively. The sharpest
# of the four, because it is the only one whose cost cannot be undone.
#
# ===========================================================================
# THE SECOND FAILURE, 2026-09-20 — AND THE CLAUSE THAT IS GONE
# ===========================================================================
# The first fix carried an "authority B": a regex over the last genuine user
# message that ALLOWED the kill when it found an unconditional stop imperative.
# Measured 2026-09-20 (escalation esc-20260920T050611Z-fb0a4718), it answered
# AUTHORIZES to "how is the stop command coming along", "tell me about
# stop.sh", "when will the stop command be ready" and "the stop rule is in
# CLAUDE.md now" — ordinary talk ABOUT stopping, each of which would have waved
# a live teammate's destruction through with no ack at all.
#
# "When the fuck did I say 'blindly automate everything even remotely related
#  to a stop'??????"  — CEO, 2026-09-20. He never did. (ceo-decisions §67)
#
# A clause written because a kill was made on an INFERENCE from his words had
# re-encoded that inference as a pattern. It is removed. His stop still goes
# through in seconds — `scripts/stop.sh <names> --ceo-word "<his sentence>"`
# writes the ack per named target and prints the exact TaskStop calls — by
# Rich's explicit act, quoting him, never by a machine reading him.
#
# ===========================================================================
# THE CONTRACT
# ===========================================================================
# The predicate, the corpus measurement and the argument for every clause live
# in scripts/lib/stop-live-work.py and NOWHERE ELSE. Read that first; this file
# is the wiring and the refusal. In one paragraph:
#
#   ALLOW, SILENTLY   the target is not provably running (a finished teammate
#                     being retired -- the overwhelming majority of real
#                     TaskStop traffic), or the caller IS the target.
#   ALLOW + record    a live, targeted, unconsumed stop-work-ack.
#   ALLOW + NOTE      liveness is INDETERMINATE. Said out loud, never guessed.
#   BLOCK             a PROVABLY LIVE target with nothing written down.
#
# ===========================================================================
# WHY BLOCKING, AND THE NUMBERS THAT DECIDED IT
# ===========================================================================
# Measured against every real TaskStop call in every Claude Code transcript on
# this machine -- 2,499 transcripts scanned, 100 real calls found, adjudicated
# one by one in scripts/hooks/stop-live-work.corpus.md.
#
#   100  real TaskStop calls
#    94  target NOT provably live at the call -> of these, the great majority
#         are the legitimate dominant use: retiring a teammate that had already
#         handed off. Those reach authority A and the guard never speaks.
#     6  carried an order of his -> under the deleted authority B they were
#        allowed unwritten; now each is one `stop.sh` invocation, which writes
#        the ack with his sentence in it and takes a second.
#     2  are the kills of 2026-09-10. Both are refused.
#
#   FALSE POSITIVES ON THE MEASURED CORPUS: 0 — and they were always 0 for the
#   liveness reason below, never for the language predicate that is now gone.
#
# This guard refuses only on a POSITIVE ALIVE verdict from
# scripts/lib/agent-liveness.py, whose authoritative source is the worktree
# lock. A finished teammate has no lock, so every cleanup passes untouched and
# a habit of waiving never gets a chance to form. That is the g11/g12/g13
# lesson applied at the design stage instead of learned again: this project
# killed three guards in one day by shipping gates broad enough that waiving
# became routine, and a habitually waived guard is a dead guard that still
# looks alive.
#
# ===========================================================================
# WHAT IT NEVER TOUCHES
# ===========================================================================
#   * A teammate stopping ITSELF. The caller's own agent id is compared against
#     the target, by id and by resolved name.
#   * A protocol shutdown. Those are SendMessage bodies; no TaskStop is made.
#   * Cleanup of an agent that has already finished -> not alive -> allowed.
#   * Any other tool. A well-formed payload for anything but TaskStop exits 0.
#
# ===========================================================================
# FAIL-OPEN, AND WHY THAT IS RIGHT HERE SPECIFICALLY
# ===========================================================================
# Its PreToolUse siblings fail CLOSED, and this one does not. The asymmetry is
# deliberate. A fail-closed guard on a destructive tool sounds correct until
# you count the traffic: the dominant legitimate use of TaskStop is retiring
# something already finished, and for those the resolver often cannot resolve
# anything at all. Fail closed and every cleanup needs paperwork. Paperwork
# every time is exactly how the three dead guards died.
#
# So a missing python3, an unparseable payload, an unresolvable root, a broken
# resolver -- all pass the call through, and the ones worth knowing about say
# so on the operator channel. The guard refuses only what it can PROVE is live,
# which is precisely the case that cost a session's work this morning.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start, and this is a NEW registration. Installing
# it changes nothing in the session that installs it; it begins enforcing in
# the NEXT session. (An already-registered hook's BODY is re-read each fire --
# this one is not that case.)
#
# Exit codes: 0 allowed / not mine / could not evaluate · 2 BLOCKED
#
# Self-test:  scripts/hooks/guard-stop-live-work.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-stop-live-work.sh)"

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/guard-stop-live-work.test.sh"
fi

# python3 is required to read anything at all. FAIL OPEN -- see the header.
if ! command -v python3 >/dev/null 2>&1; then
    echo "NOTE: guard-stop-live-work.sh: python3 unavailable, so this TaskStop was NOT checked against a written stop-work-ack. $HOOK_TAG" >&2
    exit 0
fi

# SCRIPT_DIR IS ASSIGNED INSIDE THE BLOCK BELOW AND NOWHERE ELSE. It used to sit
# above the python3 check, hoisted out of the bootstrap — harmless to run, and the
# one thing Layer R cannot tolerate: R3 compares the block below BYTE FOR BYTE
# across every rooted hook, and a block missing its own first line is a divergent
# copy. It went unseen because this hook was absent from R_ROOTED_HOOKS, so Layer R
# never walked it (measured 2026-09-14: 52 hooks named, 57 registered hooks resolve
# a root). Nothing between here and there reads SCRIPT_DIR, so moving it back in is
# behavior-preserving; `--self-test` above uses its own _SELF_DIR.
#
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
  hook: scripts/hooks/guard-stop-live-work.sh
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

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "${RICHOS_ROOT_STATUS:-}" = "not-adopted" ]; then
    exit 0
else
    root_failure_banner "scripts/hooks/guard-stop-live-work.sh" >&2 || true
    exit 0
fi

# --- Is this a TaskStop at all? --------------------------------------------
# ONE python pass: tool gate + every field. A payload for a different tool
# exits silently; an unreadable payload exits with a note (see below), never in
# silence -- 17 of 25 PreToolUse guards were measured passing calls in complete
# silence on 2026-09-05, and that is the defect this separates out.
PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
def out(*f): print("\t".join(f)); sys.exit(0)
try:
    d = json.load(sys.stdin)
except Exception:
    out("PARSEFAIL", "", "", "", "")
if not isinstance(d, dict):
    out("PARSEFAIL", "", "", "", "")
if str(d.get("tool_name", "") or "") != "TaskStop":
    out("NOTMINE", "", "", "", "")
ti = d.get("tool_input", {})
if not isinstance(ti, dict):
    out("PARSEFAIL", "", "", "", "")
out("OK",
    str(ti.get("task_id", "") or ""),
    str(d.get("transcript_path", "") or ""),
    str(d.get("session_id", "") or ""),
    str(d.get("cwd", "") or ""))
' 2>/dev/null || true)"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
[ "$STATUS" = "NOTMINE" ] && exit 0

if [ "$STATUS" != "OK" ]; then
    _UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
    if [ -f "$_UE_LIB" ]; then
        # shellcheck source=../lib/unevaluated-notice.sh
        . "$_UE_LIB"
        unevaluated_or_continue "guard-stop-live-work.sh" "$INPUT" \
            "${ENTITY_ROOT:-}" \
            "whether this stop destroys a live teammate's uncommitted work"
    fi
    exit 0
fi

TASK_ID="$(printf '%s' "$PARSED" | cut -f2)"
TRANSCRIPT="$(printf '%s' "$PARSED" | cut -f3)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f4)"
CALLER_CWD="$(printf '%s' "$PARSED" | cut -f5)"

if [ -z "$TASK_ID" ]; then
    echo "NOTE: guard-stop-live-work.sh: a TaskStop with no task_id was not checked. $HOOK_TAG" >&2
    exit 0
fi

# --- WHO IS THE TARGET, AND IS IT ALIVE? -----------------------------------
# TaskStop's task_id is either a raw agent id or the teammate NAME the spawn
# carried. The name -> id join exists in exactly one place -- the lead's own
# transcript, Agent tool_use joined to toolUseResult.agentId on tool_use_id --
# and scripts/lib/agent-liveness.py is its only parser. This asks that parser
# rather than adding a second one.
#
# THE TRANSCRIPT IS PASSED FOR THAT JOIN AND FOR NOTHING ELSE. It is read for
# `Agent` tool_use records joined to their agent ids — never for what the CEO,
# or anyone, wrote. The predicate below is not given it at all, which is the
# point of the 2026-09-20 change in the header.
#
# The caller's own agent id comes from its cwd when it is an isolation
# worktree, which is how "a teammate stopping itself" is recognized without
# trusting anything the caller says.
SELF_AGENT_ID=""
case "$CALLER_CWD" in
    */.claude/worktrees/agent-*)
        SELF_AGENT_ID="${CALLER_CWD##*/agent-}"
        SELF_AGENT_ID="${SELF_AGENT_ID%%/*}"
        ;;
esac

LIVENESS_JSON="$(
    TASK_ID="$TASK_ID" TRANSCRIPT="$TRANSCRIPT" ENTITY_ROOT="$ENTITY_ROOT" \
    ENGINE_ROOT="$ENGINE_ROOT" python3 - <<'PY' 2>/dev/null || true
import json, os, sys
sys.path.insert(0, os.path.join(os.environ["ENGINE_ROOT"], "scripts", "lib"))
task = os.environ["TASK_ID"]
res = {"verdict": "INDETERMINATE", "agent_id": "", "detail": "resolver unavailable"}
try:
    import importlib.util
    p = os.path.join(os.environ["ENGINE_ROOT"], "scripts", "lib", "agent-liveness.py")
    spec = importlib.util.spec_from_file_location("agent_liveness", p)
    al = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(al)
except Exception as e:
    print(json.dumps(res)); sys.exit(0)

# A session-qualified name (norm-sonnet-title1@session-ad9b54ca) is a real
# addressing form in this harness; strip the qualifier before the join.
bare = task.split("@", 1)[0]
target = bare
try:
    names = al.names_to_ids(os.environ.get("TRANSCRIPT", ""))
    if bare in names:
        target = names[bare]
    elif task in names:
        target = names[task]
except Exception:
    pass
res["agent_id"] = target
try:
    rec = al.resolve(os.environ["ENTITY_ROOT"], target)
    if isinstance(rec, dict):
        res["verdict"] = str(rec.get("verdict") or "INDETERMINATE")
        res["detail"] = str(rec.get("reason") or "")
except Exception as e:
    res["detail"] = "resolver raised: %r" % (e,)
print(json.dumps(res))
PY
)"

LIVENESS="$(printf '%s' "$LIVENESS_JSON" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("verdict") or "INDETERMINATE")
except Exception: print("INDETERMINATE")' 2>/dev/null || echo INDETERMINATE)"
RESOLVED_ID="$(printf '%s' "$LIVENESS_JSON" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("agent_id") or "")
except Exception: print("")' 2>/dev/null || true)"
LIVE_DETAIL="$(printf '%s' "$LIVENESS_JSON" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("detail") or "")
except Exception: print("")' 2>/dev/null || true)"

case "$LIVENESS" in
    ALIVE|NOT-ALIVE|INDETERMINATE) : ;;
    *) LIVENESS="INDETERMINATE" ;;
esac

# --- THE VERDICT -----------------------------------------------------------
VERDICT_JSON="$(
    python3 "$ENGINE_ROOT/scripts/lib/stop-live-work.py" \
        --task-id "$TASK_ID" \
        --liveness "$LIVENESS" \
        --entity-root "$ENTITY_ROOT" \
        --self-agent-id "$SELF_AGENT_ID" \
        --resolved-agent-id "$RESOLVED_ID" \
        --session-id "$SESSION_ID" \
        --consume 2>/dev/null || true
)"

VERDICT="$(printf '%s' "$VERDICT_JSON" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("verdict") or "")
except Exception: print("")' 2>/dev/null || true)"
REASON="$(printf '%s' "$VERDICT_JSON" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("reason") or "")
except Exception: print("")' 2>/dev/null || true)"

if [ -z "$VERDICT" ]; then
    echo "NOTE: guard-stop-live-work.sh: the stop predicate could not run, so this TaskStop on '$TASK_ID' was NOT checked. $HOOK_TAG" >&2
    exit 0
fi

LOG_DIR="$ENTITY_ROOT/.claude/state"

case "$VERDICT" in
    allow-not-live|allow-self)
        exit 0
        ;;
    allow-undecidable)
        # The one class the guard cannot decide. Allowed, and NAMED -- because
        # a guard that treats "I could not tell" as "it is fine" is how a
        # stale roster came to outrank a held lock on 2026-08-31.
        echo "NOTE: TaskStop on '$TASK_ID' — could not establish whether it is still running (${LIVE_DETAIL:-no evidence}), so it was allowed unchecked. If it IS running and has not committed, that work is now gone. $HOOK_TAG" >&2
        exit 0
        ;;
    allow-recovery)
        echo "TaskStop on '$TASK_ID' authorized by its recorded recovery control. Native completion is recorded separately. $HOOK_TAG" >&2
        exit 0
        ;;
    allow-acked)
        mkdir -p "$LOG_DIR" 2>/dev/null || true
        {
            printf '%s\tsession=%s\ttask=%s\t%s\n' \
                "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SESSION_ID" "$TASK_ID" "$REASON"
        } >>"$LOG_DIR/stop-work-acks-used.log" 2>/dev/null || true
        echo "TaskStop on '$TASK_ID' allowed on a written stop-work-ack, now spent. Uses are appended to .claude/state/stop-work-acks-used.log, so a habit of acking shows up as a habit. $HOOK_TAG" >&2
        exit 0
        ;;
esac

# --- REFUSED ---------------------------------------------------------------
{
    echo "BLOCKED: TaskStop on '$TASK_ID' would destroy work that is STILL RUNNING."
    echo
    echo "  Liveness: ALIVE — ${LIVE_DETAIL:-its isolation worktree lock is held by a running process}"
    echo "  On disk:  ${REASON}"
    echo
    echo "  TaskStop is a destructor, not a pause. To PAUSE, message the teammate:"
    echo "  commit what you have, then hold - end your turn and wait to be messaged."
    echo "  It goes idle, keeps its worktree and its context, and one message wakes"
    echo "  it. Stopping instead throws away everything uncommitted. On 2026-09-10 that took"
    echo "  echo-opus-hw2's entire session — no commits, clean worktree, nothing"
    echo "  recoverable. zach-opus-dor1 survived the same minute only because it"
    echo "  had committed as it went."
    echo
    echo "  IF THE REASON IS BUDGET, THIS IS THE WRONG LEVER. Work already in"
    echo "  flight is already paid for; killing it refunds nothing and forfeits"
    echo "  the result. The action that costs nothing is to stop DISPATCHING."
    echo
    echo "  Two ways forward, and only two. BOTH TAKE SECONDS, and both end in"
    echo "  a reason on disk — this guard does not read anybody's sentences."
    echo
    echo "    1. HE ORDERED IT. Run his stop, quoting him:"
    echo
    echo "         $ENGINE_ROOT/scripts/stop.sh '$TASK_ID' \\"
    echo "             --ceo-word '<his sentence, verbatim>'"
    echo
    echo "       It names only the teammates he named, measures what each one"
    echo "       would lose, writes the ack per target with his words in it,"
    echo "       and prints the exact TaskStop call to make next."
    echo
    echo "    2. IT IS YOUR CALL. Write your own reason down:"
    echo
    echo "         $ENGINE_ROOT/scripts/stop-work-ack.sh \\"
    echo "             --task '$TASK_ID' \\"
    echo "             --destroying '<what is lost if it dies right now>' \\"
    echo "             --why '<why it cannot wait for the agent to commit>'"
    echo
    echo "       One target, 15 minutes, spent on first use. A bare marker"
    echo "       exempts nothing."
    echo
    echo "  Not sure whether it has committed? git -C <its worktree> log --oneline"
    echo "  answers that in one line, and it is the difference between losing"
    echo "  nothing and losing everything."
    echo
    echo "  $HOOK_TAG"
} >&2
exit 2

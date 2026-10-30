#!/usr/bin/env bash
#
# notice-ceo-inputs-unheld.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                               QUIETLY WHILE A FILE HE HANDED OVER IS STILL
#                               HELD BY NOTHING BUT A HARD DRIVE.
#
# The partner half of commit-ceo-inputs.sh. Read that file first: it is the
# ingress, it commits, and it explains why.
#
# ===========================================================================
# THE HALF THE INGRESS HOOK STRUCTURALLY CANNOT DO
# ===========================================================================
# commit-ceo-inputs.sh fires ONCE, on ONE message. When every gate passes it
# commits and the story is over. When a gate REFUSES — a credential, private
# material, a path at the repository root, a repository that never adopted the
# engine — or when the commit itself FAILS, it says so once and then it is
# gone. It has no way to learn whether anybody acted, and a refusal that is
# read once and forgotten is the original defect one step later: a document
# governing live work, held by nothing.
#
# So the refusal is not left in a message. It is left in a LEDGER, and this
# hook re-reads that ledger at the end of every turn.
#
# ===========================================================================
# IT CLEARS ITSELF, WHICH IS WHY IT CAN BE TRUSTED
# ===========================================================================
# An entry is UNRESOLVED only while the fact behind it is still true. Every
# turn, each remembered path is re-checked against git:
#
#   still untracked inside a repository  -> still unresolved, still announced
#   now tracked                          -> resolved, and the recovery is said
#   now ignored                          -> resolved (an ignore is a decision)
#   gone from disk                       -> resolved (it is not a file any more)
#
# Nothing is ever hand-cleared and there is no acknowledgement to remember to
# give. The condition ends when the fact ends, so the notice cannot become a
# stale nag — which is the failure mode that gets a notice muted, after which
# it protects nothing forever.
#
# ===========================================================================
# WHY IT REPORTS AND DOES NOT BLOCK
# ===========================================================================
# The refusals it re-announces are the ones the ingress was RIGHT to refuse: a
# credential, a third party's material, a change to a repository's front door.
# Every one of them needs a person to decide where the file belongs. A Stop
# guard that refused to let the turn end would wedge the session on a decision
# it cannot make, and the first thing a wedged session earns is
# CHECK_CEO_INPUTS_UNHELD=0. Same trade as notice-inflight-acks.sh and
# notice-unasked-deferral.sh, for the same reason.
#
# ONE LINE, STATE-CHANGE DE-DUPLICATED, through scripts/lib/stop-hook-notice.sh
# — the only channel measured to reach the operator (that file carries the
# measurement). The state key is the SET of unresolved paths, so a new refusal
# speaks immediately and an unchanged one does not repeat.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start; this begins reporting in the NEXT session.
# Verify with a direct invocation over a constructed ledger:
#
#     scripts/hooks/ceo-inputs.test.sh
#
# Exit codes: always 0. This hook never refuses a turn.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the ingress ledger, re-decided against git every
# turn. Same session_id caveat as notice-ceo-unasked.sh, and the same answer.
#
# WHY THIS ONE NEEDS A DECLARATION WHERE THE OTHER EXEMPT HOOKS DO NOT.
# scripts/hooks/unevaluated-payload.test.sh derives every registered PreToolUse
# and Stop hook from hooks/hooks.json and drives each with an empty, a truncated
# and a non-JSON payload. A hook that REFUSES them is proven fail-closed by that
# alone; a hook that ANNOUNCES on them is proven audible by that alone; neither
# needs a word in its source. But a hook that is SILENT on all four looks
# identical whether its predicate never needed the payload or its predicate was
# silently lost — which is the whole defect, and it is the one thing driving a
# hook cannot tell you. So payload-independence is the single class that must be
# CLAIMED by a person, and the suite then holds the claim to its consequence:
# the output must be identical on all four payloads. An undeclared silent hook
# fails that suite; a falsely declared one fails it too.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/notice-ceo-inputs-unheld.sh)"

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
  hook: scripts/hooks/notice-ceo-inputs-unheld.sh
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
    exit 0
else
    stop_notice_init "notice-ceo-inputs-unheld.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "INGRESS FOLLOW-UP IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). A file you handed over that a safety gate refused is no longer being tracked to a conclusion. $HOOK_TAG"
    root_failure_banner "scripts/hooks/notice-ceo-inputs-unheld.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_CEO_INPUTS_UNHELD:=1}"

stop_notice_init "notice-ceo-inputs-unheld.sh" "$ENTITY_ROOT" "$INPUT"

if [ "$CHECK_CEO_INPUTS_UNHELD" = "0" ]; then
    stop_notice_abnormal "stood-down" \
        "INGRESS FOLLOW-UP — STOOD DOWN by CHECK_CEO_INPUTS_UNHELD=0 in $CONFIG. A file you handed over that a gate refused will be mentioned once and never again. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "INGRESS FOLLOW-UP — NOT RUNNING: python3 is not on PATH, so a refused hand-over is no longer being tracked to a conclusion. $HOOK_TAG"
    exit 0
fi

LEDGER="$ENTITY_ROOT/.claude/state/ceo-inputs.jsonl"

notice_clean() {
    stop_notice_normal \
        "INGRESS FOLLOW-UP — RUNNING AGAIN, and nothing you handed over is left unheld. $HOOK_TAG"
    exit 0
}

# No ledger means the ingress has never had anything to record in this
# repository. That is the ordinary state of a fresh adopter and it is not a
# fault, so it reads as clean rather than as a broken mechanism.
[ -f "$LEDGER" ] || notice_clean

set +e
UNRESOLVED="$(INGRESS_ANALYZER="$ENGINE_ROOT/scripts/hooks/commit-ceo-inputs.py" LEDGER="$LEDGER" python3 -c '
import importlib.util, json, os, subprocess, sys

try:
    spec = importlib.util.spec_from_file_location("ingress", os.environ["INGRESS_ANALYZER"])
    ingress = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ingress)
    if not callable(getattr(ingress, "is_handover_file", None)):
        raise ImportError("input classifier missing")
except (OSError, KeyError, AttributeError, ImportError, SyntaxError):
    print("BROKEN\tinput classifier unavailable")
    sys.exit(0)

path = os.environ["LEDGER"]

# EVERY record. A bound on the last N records counted unrelated hand-overs,
# not resolutions: an input refused before 500 later commits dropped out of
# the reminder while git still did not hold it (hunt part 3 v3, finding 43).
# Reading the whole ledger is one cheap JSON parse per line; the git checks
# below run only for paths whose latest word is still unresolved.
try:
    with open(path, encoding="utf-8") as fh:
        lines = fh.readlines()
except OSError as exc:
    print("BROKEN\t%s" % exc.__class__.__name__)
    sys.exit(0)

# Latest word per path wins: a path refused on Monday and committed on Tuesday
# is resolved, and the ledger records both in order.
verdict = {}
for line in lines:
    line = line.strip()
    if not line:
        continue
    try:
        rec = json.loads(line)
    except Exception:
        continue
    for p in rec.get("committed") or []:
        verdict[p] = None
    for r in rec.get("refused") or []:
        if isinstance(r, dict) and r.get("path"):
            verdict[r["path"]] = r.get("why", "a safety gate refused it")
    for p in rec.get("reported") or []:
        verdict[p] = "it is outside every git repository"

def still_unheld(p):
    """Re-decided against git every turn. Never trusted from the ledger."""
    if not ingress.is_handover_file(p):
        return False
    base = p if os.path.isdir(p) else os.path.dirname(p) or "/"
    if not os.path.isdir(base):
        return False
    def git(args, cwd):
        try:
            r = subprocess.run(["git", "--no-optional-locks"] + args, cwd=cwd,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               timeout=8)
        except Exception:
            return (1, "")
        return (r.returncode, r.stdout.decode("utf-8", "replace").strip())
    rc, top = git(["rev-parse", "--show-toplevel"], base)
    if rc != 0 or not top:
        return True          # outside every repository: still held by nobody
    rc, out = git(["ls-files", "--", p], top)
    if rc == 0 and out:
        return False         # git holds it now
    rc, _o = git(["check-ignore", "-q", "--", p], top)
    return rc != 0           # ignored is a decision, so it counts as resolved

for p in sorted(k for k, v in verdict.items() if v is not None):
    if still_unheld(p):
        print("UNHELD\t%s\t%s" % (p, verdict[p]))
' 2>/dev/null)"
RC=$?
set -e

# A crashed reader decided nothing: it is not "nothing is unheld" (hunt part 3
# v3, finding 28).
if [ "$RC" != "0" ]; then
    _CRASH_MSG="INGRESS FOLLOW-UP — DID NOT RUN THIS TURN: its reader exited $RC with no verdict, so a file you handed over that a gate refused was not checked. $HOOK_TAG"
    stop_notice_abnormal "reader-crashed" "$_CRASH_MSG"
    exit 0
fi

if printf '%s' "$UNRESOLVED" | grep -q '^BROKEN'; then
    stop_notice_abnormal "ledger-unreadable" \
        "INGRESS FOLLOW-UP — CANNOT READ ITS LEDGER OR INPUT CLASSIFIER (ledger: $LEDGER), so a file you handed over that a gate refused is no longer being tracked to a conclusion. $HOOK_TAG"
    exit 0
fi

[ -n "$UNRESOLVED" ] || notice_clean

COUNT="$(printf '%s\n' "$UNRESOLVED" | grep -c '^UNHELD' || true)"
FIRST_PATH="$(printf '%s\n' "$UNRESOLVED" | grep '^UNHELD' | head -1 | cut -f2)"
FIRST_WHY="$(printf '%s\n' "$UNRESOLVED" | grep '^UNHELD' | head -1 | cut -f3)"

# The key is the SET, so a NEW refusal speaks at once and an unchanged set does
# not repeat itself every turn.
KEY="unheld:$(printf '%s' "$UNRESOLVED" | cksum | tr -d ' ')"

if [ "$COUNT" = "1" ]; then
    MSG="STILL HELD BY NOTHING BUT YOUR DISK: $FIRST_PATH — $FIRST_WHY. The ingress refused to commit it and will keep saying so until it is in a repository or deliberately gitignored. $HOOK_TAG"
else
    MSG="STILL HELD BY NOTHING BUT YOUR DISK: $COUNT files you handed over, including $FIRST_PATH ($FIRST_WHY). The ingress refused to commit them and will keep saying so until each is in a repository or deliberately gitignored. $HOOK_TAG"
fi

stop_notice_abnormal "$KEY" "$MSG"
exit 0

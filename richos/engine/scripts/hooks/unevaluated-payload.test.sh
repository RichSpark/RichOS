#!/usr/bin/env bash
#
# unevaluated-payload.test.sh — NO REGISTERED GUARD MAY PASS A CALL IT NEVER
#                                READ WITHOUT SAYING SO.
#
# ===========================================================================
# THE PROPERTY
# ===========================================================================
#
#     THE ABSENCE OF A FINDING MUST BE DISTINGUISHABLE FROM THE ABSENCE OF A
#     CHECK.
#
# On 2026-09-05 all forty registered PreToolUse and Stop hooks were driven with
# an EMPTY payload, a TRUNCATED one and one that is not JSON
# (docs/verification/hook-payload-failure-modes-2026-09-05.md). Thirty exited 0
# with nothing on either stream, and in sixteen of those a finding observed on a
# valid payload vanished entirely. A guard that never looked at a call was
# byte-for-byte indistinguishable from a guard that looked and approved.
#
# This suite is what stops that coming back, and what covers a hook registered
# tomorrow with no edit here.
#
# ===========================================================================
# THE SET IS DERIVED, NEVER TYPED
# ===========================================================================
# Both inventories come from hooks/hooks.json through the one shared parser
# (scripts/lib/registered-hooks.sh), for the reason that file documents: a TYPED
# list of 14 where the registration held 15 is the defect that opened this whole
# sequence, and the survey itself found the PreToolUse count had moved from 23 to
# 25 under it the same day. A hook added tomorrow is judged here automatically.
#
# NEGATIVE CONTROL: the derivation is asserted non-empty and the number printed.
#
# ===========================================================================
# HOW EACH HOOK IS JUDGED — BY DRIVING IT, WITH ONE EXCEPTION
# ===========================================================================
# Every hook is run four times: a well-formed CONTROL payload for its matcher,
# then the three degraded shapes. What it does decides its class:
#
#   REFUSES the degraded payloads          -> fail-closed. Proven, and it needs
#                                             no word in its source.
#   ANNOUNCES on the degraded payloads     -> audible. Proven, likewise.
#   IDENTICAL output on all four           -> payload-independent, BUT ONLY IF
#                                             THE SOURCE SAYS SO (below).
#   anything else                          -> FAIL, by name.
#
# THE ONE EXCEPTION, AND WHY IT IS THE ONLY ONE. A hook that is silent on all
# four looks exactly the same whether its predicate never needed the payload or
# its predicate was silently lost. That is the defect itself, and it is the one
# thing driving a hook cannot tell you apart. So payload-independence must be
# CLAIMED by a person, in the hook, where a reviewer sees it —
#
#     # UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — <reason>
#
# — and this suite then holds the claim to its consequence: the output really
# must be identical across all four payloads. A bare marker exempts nothing, the
# same discipline the dialect and contrast exemptions carry.
#
# THE CONTROL ARM IS NOT DECORATION. Without it, a hook that announced on EVERY
# call would pass. Silence on a payload the guard could read is a requirement in
# its own right: a message on every clean call is noise, and noise is how a real
# signal gets ignored.
#
# ===========================================================================
# WHAT THIS SUITE DOES NOT ASSERT
# ===========================================================================
# Nothing here requires any guard to fail CLOSED. Which gates are worth the risk
# of bricking a live session is a judgment that differs per gate and it has not
# been taken. This suite only requires that an unevaluated call is not silent.
#
# WHERE IT STOPS LOOKING, SAID OUT LOUD. It drives PreToolUse and Stop — 45 of
# the 67 hook entries hooks.json registers on 2026-09-10. The other 22 sit in
# eight events that cannot refuse what they are handed, and case 4e names them
# with their counts in this suite's own output and turns RED the day an event
# appears that nobody has judged. That set is NOT assumed clean: the same
# `|| true` hole was found in detect-nonnative-worktree.sh (PostToolUse) on
# 2026-09-10 and is pinned by four cases in its own suite, which is where a hook
# needing that sandbox has to be judged.
#
# Usage: scripts/hooks/unevaluated-payload.test.sh
# Exit:  0 all checks passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }

command -v python3 >/dev/null 2>&1 || {
    echo "ERROR: unevaluated-payload.test.sh needs python3." >&2
    exit 1
}

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/unevaluated-payload.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT

echo "=== no registered guard passes a call it never read in silence ==="
echo ""

# ===========================================================================
# 1. THE DERIVATION
# ===========================================================================
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"

PRE_HOOKS=""
STOP_HOOKS=""
PRE_HOOKS="$(registered_hook_scripts "$HOOKS_JSON" PreToolUse 2>/dev/null || true)"
STOP_HOOKS="$(registered_hook_scripts "$HOOKS_JSON" Stop 2>/dev/null || true)"
PRE_N="$(printf '%s\n' "$PRE_HOOKS" | grep -c . || true)"
STOP_N="$(printf '%s\n' "$STOP_HOOKS" | grep -c . || true)"

if [ "$PRE_N" -gt 0 ] && [ "$STOP_N" -gt 0 ]; then
    ok "1a. both sets are DERIVED from $HOOKS_JSON — $PRE_N PreToolUse, $STOP_N Stop"
else
    bad "1a. both sets are derived" \
        "PreToolUse=$PRE_N Stop=$STOP_N — every case below would pass by examining nothing."
    echo ""
    echo "=== $PASS passed, $FAIL failed ==="
    exit 1
fi

# ===========================================================================
# 2. THE SANDBOX ENTITY
#    An adopted repository of its own, forced with RICHOS_ENTITY_ROOT, so that
#    nothing here writes state into a real one. The survey drove the real root
#    and had to snapshot and restore 44 files afterwards; a suite that runs on
#    every commit cannot do that.
# ===========================================================================
REPO="$SANDBOX/entity"
mkdir -p "$REPO/.claude/agents" "$REPO/.claude/state" "$REPO/docs"
echo "# sandbox" > "$REPO/README.md"

# THE SANDBOX MUST BE A COMPLETE ADOPTER, and this is not decoration. The survey
# recorded that a sandbox alone gave a FALSE reading for guard-ceo-ask-first.sh
# (removed 2026-10-24): it stood down there for an environmental reason while
# refusing at exit 2 against the real root. This suite reproduced that exactly — that guard and
# guard-ceo-ruled-ask.sh landed in the silent-and-undeclared bucket on a bare
# sandbox, which would have been a red for a defect that does not exist. Both
# stand down when the repository DECLARES no CEO list and no rulings record, and
# that stand-down is correct: a repository with no list has no protection to
# lose. So the sandbox declares both, and the two guards are judged on the same
# footing as everybody else.
{
    printf 'PROTECTED_PATHS="docs"\n'
    printf 'CEO_TODOS_REPOS="."\n'
    printf 'CEO_RULINGS_PATHS="docs/ceo-decisions.md"\n'
} > "$REPO/orchestration.config"
printf '# CEO decisions\n\n## 1. A ruling\n\nSomething was ruled.\n' \
    > "$REPO/docs/ceo-decisions.md"
# DECLARED, WELL-FORMED, AND WITH NOTHING PREPARED IN IT. All three are load
# bearing, and the first draft of this suite got the middle one wrong: a
# markdown checklist is not the format, the declaration is KEY=value, and both
# hooks correctly reported "declares CEO TODOs but its declaration could not be
# read" — on the CONTROL arm, which case 4d then read as noise. They were doing
# their job and the sandbox was broken.
#
# Nothing prepared, because a record holding an unanswered question makes
# notice-ceo-unasked.sh speak, which
# is also correct and also not what 4d is asking about. Declared, readable and
# satisfied is the one state that leaves the gate LIVE and the happy path
# SILENT, which is exactly the pair of properties under test.
mkdir -p "$REPO/wiki"
cat > "$REPO/.ceo-todos" <<'CEOTODOS_EOF'
TODO_RECORD="wiki/open-items.md"
TODO_VIEW="CEO-TODOs.md"
ROOT_README="README.md"
CEO_SECTIONS="1 2"
PREPARER_SECTION="3"
ARTIFACT_ROOTS="q=."
CEOTODOS_EOF
cat > "$REPO/wiki/open-items.md" <<'RECORD_EOF'
# Open items

## 1. Waiting on the CEO — a decision

Nothing is prepared for him.

## 2. Waiting on the CEO — an input

Nothing is prepared for him.

## 3. Being prepared

Nothing yet.
RECORD_EOF
echo "# view" > "$REPO/CEO-TODOs.md"
printf -- '---\nname: dev\ndescription: d\nmodel: sonnet\ntools: Read\n---\nbody\n' \
    > "$REPO/.claude/agents/dev.md"
git -C "$REPO" init -q -b main 2>/dev/null
git -C "$REPO" config user.email "tester@example.invalid"
git -C "$REPO" config user.name "tester"
git -C "$REPO" add -A >/dev/null 2>&1
git -C "$REPO" commit -q -m base >/dev/null 2>&1

TRANSCRIPT="$SANDBOX/transcript.jsonl"
python3 -c '
import json, sys
with open(sys.argv[1], "w") as fh:
    fh.write(json.dumps({"type": "user", "isSidechain": False,
                         "message": {"role": "user", "content": "do the thing"}}) + "\n")
' "$TRANSCRIPT"

# ===========================================================================
# 3. THE PAYLOADS
#    The violating bytes are padded and the cut is made INSIDE the padding, so
#    the truncation destroys the JSON structure and nothing else — the shape a
#    bounded read produces, and the shape the survey used.
# ===========================================================================
mk_payload() { # <matcher-kind> <variant> <session8>
    python3 - "$1" "$2" "$3" "$REPO" "$TRANSCRIPT" <<'PY'
import json, sys
kind, variant, sid8, repo, transcript = sys.argv[1:6]
bodies = {
    "Agent": {"tool_name": "Agent", "tool_input": {
        "subagent_type": "dev", "name": "dev-sonnet-a1",
        "prompt": "do a thing", "isolation": "worktree"}},
    "Write": {"tool_name": "Write", "tool_input": {
        "file_path": repo + "/docs/note.md", "content": "an ordinary sentence"}},
    "Bash": {"tool_name": "Bash", "tool_input": {"command": "echo hello"}},
    "SendMessage": {"tool_name": "SendMessage", "tool_input": {
        "to": "main", "message": "hello"}},
    "AskUserQuestion": {"tool_name": "AskUserQuestion", "tool_input": {
        "questions": [{"question": "which?",
                       "options": [{"label": "a"}, {"label": "b"}]}]}},
    "Workflow": {"tool_name": "Workflow", "tool_input": {"name": "x"}},
    # TaskStop's whole tool_input is one key. Measured across all 100 real
    # TaskStop calls on this machine, 2026-09-10: 100 of 100 carry exactly
    # {"task_id": ...} and nothing else, so this control is the real shape
    # rather than a plausible one.
    "TaskStop": {"tool_name": "TaskStop", "tool_input": {
        "task_id": "dev-sonnet-a1"}},
    "Stop": {"hook_event_name": "Stop", "stop_hook_active": False,
             "last_assistant_message": "Landed the branch and deployed."},
}
d = dict(bodies[kind])
d["session_id"] = sid8 + "-0000-4000-8000-000000000000"
d["cwd"] = repo
d["transcript_path"] = transcript
d["padding"] = "z" * 400
s = json.dumps(d)
if variant == "control":
    sys.stdout.write(s)
elif variant == "empty":
    sys.stdout.write("")
elif variant == "truncated":
    sys.stdout.write(s[:s.index('"padding"') + 40])
else:
    sys.stdout.write("this is not JSON, it is a sentence " + "z" * 200)
PY
}

# Which control payload does this hook's matcher want? Derived from the
# registration, never typed: a hook moved to another matcher is judged against
# that matcher's payload with no edit here.
matcher_of() { # <hook.sh> -> Agent|Write|Bash|SendMessage|AskUserQuestion|Workflow|Stop|all
    python3 - "$HOOKS_JSON" "$1" <<'PY'
import json, re, sys
doc = json.load(open(sys.argv[1], encoding="utf-8"))
hook = sys.argv[2]
for event in ("PreToolUse", "Stop"):
    for entry in (doc.get("hooks", {}) or {}).get(event, []) or []:
        for h in entry.get("hooks", []) or []:
            cmd = h.get("command") or ""
            if re.search(r"scripts/hooks/" + re.escape(hook) + r"\b", cmd):
                if event == "Stop":
                    print("Stop")
                    raise SystemExit
                m = (entry.get("matcher") or "").strip()
                print(m.split("|")[0] if m and m != "*" else "all")
                raise SystemExit
print("all")
PY
}

# A hook matched on `(all)` is handed the Write payload: it is a real tool call
# with a real file path, which is the least degenerate concrete shape available.
concrete() { [ "$1" = "all" ] && printf 'Write' || printf '%s' "$1"; }

RUN_OUT=""
RUN_ERR=""
RUN_RC=0
drive() { # <hook.sh> <matcher-kind> <variant> <session8>
    local hook="$1" out err
    out="$SANDBOX/o.txt"; err="$SANDBOX/e.txt"
    rm -rf "$REPO/.claude/state/stop-hook-notices"
    # Every hook runs with a sandbox HOME: a hook that keeps per-session state
    # under ~/.claude (the CI turn gate's sessions/<id>.json) would otherwise write
    # a control session into the operator's own record when this suite runs by hand.
    mkdir -p "$SANDBOX/home/.claude"
    mk_payload "$2" "$3" "$4" \
        | HOME="$SANDBOX/home" CLAUDE_CONFIG_DIR="$SANDBOX/home/.claude" \
          RICHOS_ENTITY_ROOT="$REPO" CLAUDE_PROJECT_DIR="$REPO" \
          CLAUDE_PLUGIN_ROOT="$ENGINE_ROOT" \
          /bin/bash "$SCRIPT_DIR/$hook" >"$out" 2>"$err"
    RUN_RC=$?
    RUN_OUT="$(cat "$out")"
    RUN_ERR="$(cat "$err")"
}

# THE SHARED VOICE. Five wordings, one family, and this is the whole vocabulary:
# the library's two, and the three guards that already said it in their own words
# before the library existed. A sixth spelling is a hook that has drifted out of
# the family, and it should fail here until it is brought back or this list is
# deliberately widened.
SAID_RX='could not read this (call|turn)|could not be read|could not parse this|could not parse the'
says_so() { printf '%s\n%s' "$RUN_OUT" "$RUN_ERR" | grep -Eq "$SAID_RX"; }

# ===========================================================================
# 4. EVERY REGISTERED HOOK
# ===========================================================================
UNCLASSIFIED=""
NOISY=""
CLASS_CLOSED=""
CLASS_AUDIBLE=""
CLASS_INDEP=""
BROKEN_CLAIM=""

for hook in $PRE_HOOKS $STOP_HOOKS; do
    [ -f "$SCRIPT_DIR/$hook" ] || { UNCLASSIFIED="$UNCLASSIFIED $hook(absent)"; continue; }
    KIND="$(concrete "$(matcher_of "$hook")")"

    drive "$hook" "$KIND" control ctl00001
    C_OUT="$RUN_OUT"; C_ERR="$RUN_ERR"; C_RC="$RUN_RC"
    C_SAID=0; says_so && C_SAID=1

    D_REFUSE=1; D_SAID=1; D_SAME=1; n=2
    for variant in empty truncated non-json; do
        drive "$hook" "$KIND" "$variant" "deg0000$n"
        n=$((n + 1))
        [ "$RUN_RC" -ne 0 ] || D_REFUSE=0
        says_so || D_SAID=0
        [ "$RUN_OUT" = "$C_OUT" ] && [ "$RUN_ERR" = "$C_ERR" ] && [ "$RUN_RC" = "$C_RC" ] || D_SAME=0
    done

    DECLARED=0
    grep -q '^# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent —' "$SCRIPT_DIR/$hook" && DECLARED=1

    if [ "$D_REFUSE" -eq 1 ]; then
        CLASS_CLOSED="$CLASS_CLOSED $hook"
    elif [ "$D_SAID" -eq 1 ]; then
        CLASS_AUDIBLE="$CLASS_AUDIBLE $hook"
        # A guard that announces on a payload it COULD read is noise.
        [ "$C_SAID" -eq 0 ] || NOISY="$NOISY $hook"
    elif [ "$DECLARED" -eq 1 ] && [ "$D_SAME" -eq 1 ]; then
        CLASS_INDEP="$CLASS_INDEP $hook"
    elif [ "$DECLARED" -eq 1 ]; then
        BROKEN_CLAIM="$BROKEN_CLAIM $hook"
    else
        UNCLASSIFIED="$UNCLASSIFIED $hook"
    fi
done

TOTAL=$((PRE_N + STOP_N))
N_CLOSED="$(printf '%s' "$CLASS_CLOSED" | wc -w | tr -d ' ')"
N_AUDIBLE="$(printf '%s' "$CLASS_AUDIBLE" | wc -w | tr -d ' ')"
N_INDEP="$(printf '%s' "$CLASS_INDEP" | wc -w | tr -d ' ')"

# 4a. THE NEGATIVE CONTROL FOR THE CLASSIFICATION ITSELF. If every hook landed
# in one bucket the buckets are meaningless, and the most likely way for that to
# happen is a harness that fails to run anything at all.
if [ "$N_AUDIBLE" -gt 0 ] && [ "$N_CLOSED" -gt 0 ] && [ "$N_INDEP" -gt 0 ]; then
    ok "4a. all three classes are populated — $N_AUDIBLE audible, $N_CLOSED fail-closed, $N_INDEP payload-independent, of $TOTAL"
else
    bad "4a. all three classes are populated" \
        "audible=$N_AUDIBLE closed=$N_CLOSED independent=$N_INDEP — a harness that ran nothing would look like this."
fi

# 4b. THE ONE THAT MATTERS.
if [ -z "$UNCLASSIFIED" ]; then
    ok "4b. every registered PreToolUse and Stop hook either refuses a payload it cannot read, says so, or has DECLARED and PROVEN that it never needed one"
else
    bad "4b. no hook passes a call it never read in silence" \
        "SILENT AND UNDECLARED:$UNCLASSIFIED — each of these exits 0 on an empty, truncated and non-JSON payload with nothing on either stream. Wire it to scripts/lib/unevaluated-notice.sh, or if its predicate genuinely does not need the payload, say so in the file with '# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — <reason>'."
fi

# 4c. A CLAIM THAT DOES NOT HOLD.
if [ -z "$BROKEN_CLAIM" ]; then
    ok "4c. every payload-independent claim holds — output identical on the control and all three degraded payloads"
else
    bad "4c. a declared payload-independent hook is not payload-independent" \
        "DECLARED BUT DIFFERENT:$BROKEN_CLAIM — the exemption says the payload is not needed, and the output changes when it is taken away. A bare marker exempts nothing."
fi

# 4d. SILENCE ON THE HAPPY PATH.
if [ -z "$NOISY" ]; then
    ok "4d. no guard announces on a payload it could read — silence stays correct on the happy path"
else
    bad "4d. announcing is reserved for a payload that could not be read" \
        "SPEAKS ON THE CONTROL TOO:$NOISY — a message on every clean call is noise, and noise is how a real signal gets ignored."
fi

# 4e. THE COVERAGE BOUNDARY — DERIVED, DECLARED, AND RED ON AN EVENT NOBODY
#     HAS JUDGED.
#
# Cases 4a-4d judge PreToolUse and Stop. That is 45 of the 67 hook entries
# hooks.json registers (measured 2026-09-10); the other 22 sit in eight events
# this suite never asks, and until now the boundary was invisible in its own
# output — which is the suite's own property, one level up: a reader could not
# tell "asked and clean" from "never asked".
#
# WHY THOSE TWO EVENTS AND NOT MORE. Only PreToolUse and Stop can REFUSE the
# thing they are handed, so only there does silence hide a bypass in a blocking
# gate. Two of the unasked hooks contain `exit 2` and neither is a bypass:
# detect-nonnative-worktree.sh runs at PostToolUse, after the spawn it reports
# on has already happened, and task-completed-handoff.sh holds a native task
# open at TaskCompleted — and that one already answers this suite's question in
# its own way, recording a durable `decision: unreadable` row and exiting 0
# rather than holding a task open on a payload it could not read.
#
# AND THE UNASKED SET IS NOT ASSUMED CLEAN. detect-nonnative-worktree.sh was
# found carrying the identical `|| true` hole on 2026-09-10, wired to the
# library, and pinned by four cases in its OWN suite — which is where a
# PostToolUse hook has to be judged, because driving it needs the sandbox that
# suite builds. Driving the SessionStart set from here would record sessions in,
# and scan the repositories of, the registry on this machine, which is why the
# derivation stops where it does and why this case exists to say so out loud
# rather than in a comment.
#
# What this asserts: every event hooks.json registers is one this suite has
# CLASSIFIED. A new event — or a hook moved into one — turns this red with the
# name, so the question "can a hook there refuse a call?" gets asked by a
# person before the coverage silently narrows.
cat > "$SANDBOX/boundary.py" <<'PY'
import json, sys

doc = json.load(open(sys.argv[1], encoding="utf-8"))
ASKED = {"PreToolUse", "Stop"}
# Judged and NOT asked, with the reason each is out of scope. The value is the
# claim; if it stops being true the event belongs in ASKED.
JUDGED = {
    "SessionStart":     "cannot refuse anything; driving it here would record sessions and scan repositories on this machine",
    "SessionEnd":       "one hook, workspace-lifecycle.sh, which records the end and always exits 0",
    "UserPromptSubmit": "one hook, commit-ceo-inputs.sh, which never exits non-zero",
    "PostToolUse":      "fires after the call it reports on; judged in each hook's own suite",
    "SubagentStart":    "recorders; the engine's own design note calls this event non-blocking",
    "SubagentStop":     "observations, never accepted delivery",
    "WorktreeRemove":   "a lifecycle observation",
    "TeammateIdle":     "an observation, not a verdict",
    "TaskCompleted":    "can hold a task open, and its hook already records an unreadable payload and exits 0",
    "PostToolUseFailure": "fires after a call that already failed; its one hook, release-shared-writes.sh, removes this call's own lease file and always exits 0",
}

counts = {}
for event, entries in (doc.get("hooks", {}) or {}).items():
    counts[event] = sum(len(e.get("hooks", []) or []) for e in entries)

unjudged = [e for e in counts if e not in ASKED and e not in JUDGED]
total = sum(counts.values())
asked_n = sum(n for e, n in counts.items() if e in ASKED)
unasked = sorted(((e, n) for e, n in counts.items() if e not in ASKED), key=lambda x: -x[1])

print("UNJUDGED %s" % " ".join(sorted(unjudged)) if unjudged
      else "OK %d %d %d %d" % (asked_n, total, total - asked_n, len(unasked)))
print(" ".join("%s(%d)" % (e, n) for e, n in unasked))
PY
BOUNDARY="$(python3 "$SANDBOX/boundary.py" "$HOOKS_JSON" 2>/dev/null || printf 'ERROR\n')"
B_VERDICT="$(printf '%s\n' "$BOUNDARY" | sed -n 1p)"
B_DETAIL="$(printf '%s\n' "$BOUNDARY" | sed -n 2p)"
case "$B_VERDICT" in
    OK*)
        set -- $B_VERDICT
        ok "4e. the coverage boundary is derived and declared — $2 of $3 registered hook entries are asked above; $4 in $5 events that cannot refuse a call, each judged by name"
        printf '        not asked:          %s\n' "$B_DETAIL"
        ;;
    UNJUDGED*)
        bad "4e. hooks.json registers an event this suite has never judged" \
            "UNJUDGED: ${B_VERDICT#UNJUDGED }. Decide whether a hook on that event can REFUSE what it is handed. If it can, add it to ASKED so it is driven with the degraded payloads; if it cannot, record the reason in JUDGED. Coverage that narrows without anyone noticing is the defect this whole file is about."
        ;;
    *)
        bad "4e. the coverage boundary could not be derived" \
            "the derivation over $HOOKS_JSON produced nothing — this case examined no registration at all."
        ;;
esac

printf '        fail-closed:        %s\n' "${CLASS_CLOSED:- (none)}"
printf '        audible:            %s\n' "${CLASS_AUDIBLE:- (none)}"
printf '        payload-independent:%s\n' "${CLASS_INDEP:- (none)}"

# ===========================================================================
# 5. ONE BLOCK, NOT TWENTY-SEVEN VARIANTS
#    Layer R of the integrity probe asserts the root-resolution bootstrap is
#    byte-identical in every hook that carries it, because a divergent copy is
#    one hook disagreeing with its siblings about which repository is protected.
#    The same argument applies here: a divergent copy is one hook disagreeing
#    about what it means to be unable to read a call.
# ===========================================================================
# The analyzer is written to a file and RUN, rather than fed to python3 from a
# heredoc inside $( ... ): bash 3.2 — which is what /usr/bin/env bash resolves
# to on macOS — mis-parses that combination and reports "unexpected EOF" at the
# closing paren.
cat > "$SANDBOX/shapes.py" <<'PY'
import os, re, sys

hooks_dir = sys.argv[1]
names = [n for n in sorted(os.listdir(hooks_dir))
         if n.endswith(".sh") and not n.endswith(".test.sh")
         and not n.endswith(".mutation.sh")]

START = re.compile(r'^_UE_LIB="\$SCRIPT_DIR/\.\./lib/unevaluated-notice\.sh"$')
# Everything that legitimately differs between two copies: the hook's own file
# name and the clause naming what it checks. Both are replaced with a token, so
# what is compared is the MECHANISM and never the message.
QUOTED_NAME = re.compile(r'"[a-z0-9][a-z0-9._-]*\.sh"')
WHAT_LINE = re.compile(r'^\s+"[^"]*"(\s*\\)?$')


def block_of(path):
    lines = open(path, encoding="utf-8").read().split("\n")
    for i, l in enumerate(lines):
        if START.match(l):
            for j in range(i, len(lines)):
                if lines[j] == "fi":
                    return lines[i:j + 1]
            return None
    return None


def normalize(blk):
    out = []
    for l in blk:
        if l.lstrip().startswith("#"):
            continue          # comments are prose, not mechanism
        if WHAT_LINE.match(l):
            out.append("<WHAT>")
            continue
        out.append(QUOTED_NAME.sub('"<HOOK>"', l))
    return "\n".join(out)


shapes = {}
carriers = []
for n in names:
    p = os.path.join(hooks_dir, n)
    blk = block_of(p)
    if blk is None:
        continue
    carriers.append(n)
    shapes.setdefault(normalize(blk), []).append(n)

print("CARRIERS\t%d" % len(carriers))
for i, (shape, owners) in enumerate(sorted(shapes.items(), key=lambda kv: -len(kv[1]))):
    print("SHAPE\t%d\t%d\t%s" % (i, len(owners), " ".join(owners)))
PY
SHAPE_REPORT="$(python3 "$SANDBOX/shapes.py" "$SCRIPT_DIR")"

CARRIERS="$(printf '%s\n' "$SHAPE_REPORT" | awk -F'\t' '$1=="CARRIERS"{print $2}')"
SHAPES_N="$(printf '%s\n' "$SHAPE_REPORT" | grep -c '^SHAPE' || true)"

if [ "${CARRIERS:-0}" -gt 0 ]; then
    ok "5a. $CARRIERS hook(s) carry the shared block"
else
    bad "5a. some hook carries the shared block" "none found — case 5b would examine nothing."
fi

# TWO SHAPES ARE EXPECTED AND THREE ARE NOT. The PreToolUse form calls
# unevaluated_or_continue; the Stop form builds the sentence and hands it to the
# notice channel. notice-hook-staleness.sh is a third by design — it routes into
# its own cannot_compare(), which already says the right thing — so the ceiling
# is three, and it is stated with its reason rather than left as a free number.
if [ "${SHAPES_N:-0}" -ge 1 ] && [ "${SHAPES_N:-0}" -le 3 ]; then
    ok "5b. $CARRIERS copies of the block reduce to $SHAPES_N distinct mechanism(s) — the PreToolUse form, the Stop form, and notice-hook-staleness.sh routing into its own cannot_compare()"
else
    bad "5b. the block is one mechanism per event" \
        "$SHAPES_N distinct shapes across $CARRIERS hooks — one hook disagreeing with its siblings about what being unable to read a call means:
$(printf '%s\n' "$SHAPE_REPORT" | grep '^SHAPE' | sed 's/^/           /')"
fi
printf '%s\n' "$SHAPE_REPORT" | grep '^SHAPE' \
    | awk -F'\t' '{printf "        shape %s (%s hook(s)): %s\n", $2, $3, substr($4,1,150)}'

# ===========================================================================
# 6. THE LIBRARY IS PRESENT AND HASHED
# ===========================================================================
LIB="$ENGINE_ROOT/scripts/lib/unevaluated-notice.sh"
if [ -f "$LIB" ]; then
    ok "6a. scripts/lib/unevaluated-notice.sh is present"
else
    bad "6a. the library is present" "missing at $LIB"
fi
if grep -q 'scripts/lib/unevaluated-notice\.sh' "$SCRIPT_DIR/install.sh" 2>/dev/null; then
    ok "6b. and install.sh mints a sha256 sidecar for it — a reverted copy puts all $CARRIERS back to exit 0 in silence, and the symptom of that is what a healthy engine also looks like"
else
    bad "6b. install.sh hashes the library" \
        "scripts/lib/unevaluated-notice.sh is not in install.sh's managed set."
fi

echo ""
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]

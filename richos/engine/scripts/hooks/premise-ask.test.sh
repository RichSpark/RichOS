#!/usr/bin/env bash
#
# premise-ask.test.sh — the premise check inside guard-ceo-ruled-ask.sh.
#
# Sections 1-4 run against a sandbox and are deterministic anywhere.
# Section 5 replays the LIVE corpus — every AskUserQuestion ever asked on this
# machine — and says plainly when it is not available rather than passing.
# Section 6 runs the mutation harness, so its mutants cannot rot unrun.
#
# WHAT THIS SUITE IS FOR, given the check refuses nothing: it holds the shape
# that makes the check survivable. A check that fired twice would be waived; a
# check that trapped a revision would be torn out; a check whose ledger stopped
# recording re-issues would lose the only number that can ever justify making
# it stronger. Each of those is a case below.
#
#   VERBOSE=1  print every gate invocation

set -uo pipefail

VERBOSE="${VERBOSE:-0}"
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_SRC="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t premise-ask.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ENGINE="$SANDBOX/engine"
mkdir -p "$ENGINE/scripts/hooks" "$ENGINE/scripts/lib"
cp "$SRC_DIR/guard-ceo-ruled-ask.sh" "$ENGINE/scripts/hooks/"
chmod +x "$ENGINE/scripts/hooks/guard-ceo-ruled-ask.sh"
for l in premise-ask.sh premise-ask.py ceo-ruled.sh ceo-ruled.py ceo-asks.sh \
         ceo-asks.py ceo-todos.sh ceo-todos.py resolve-roots.sh \
         resolve-main-checkout.sh stop-hook-notice.sh unevaluated-notice.sh; do
    cp "$ENGINE_SRC/scripts/lib/$l" "$ENGINE/scripts/lib/$l" 2>/dev/null || true
done

GATE="$ENGINE/scripts/hooks/guard-ceo-ruled-ask.sh"
PRED="$ENGINE/scripts/lib/premise-ask.py"

# --- the governed seat, and a CEO record that rules nothing ------------------
# The rulings register is deliberately empty of anything a question could
# match: this suite is about the PREMISE check, and a refusal from the sibling
# already-ruled check would be a passing test for the wrong reason.
SEAT="$SANDBOX/seat"
HQ="$SANDBOX/hq"
mkdir -p "$SEAT" "$HQ/wiki"
cat > "$SEAT/orchestration.config" <<'CONF'
CEO_TODOS_REPOS="../hq"
CEO_RULINGS_PATHS="../hq/wiki/ceo-decisions.md"
CONF
cat > "$HQ/wiki/ceo-decisions.md" <<'REC'
# CEO decisions — the standing register

## 1. Nothing this suite's questions are about (CEO, 2026-01-01)

Prose only. No question below shares a title with anything here.
REC
cat > "$HQ/.ceo-todos" <<'REC'
TODO_RECORD="wiki/open-items.md"
CEO_SECTIONS="1"
REC
cat > "$HQ/wiki/open-items.md" <<'REC'
# Open items

## 1. Waiting on the CEO — a decision

### 1.1 READY-FOR-CEO — nothing outstanding
REC

ask_payload() { # <question> <option-description> <session> [agent]
    Q="$1" O="$2" SID="$3" AID="${4:-}" SEAT="$SEAT" python3 -c '
import json, os
p = {"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion",
     "session_id": os.environ["SID"], "cwd": os.environ["SEAT"],
     "tool_input": {"questions": [{"header": "Case",
        "question": os.environ["Q"],
        "options": [{"label": "One way", "description": os.environ["O"]},
                    {"label": "The other", "description": "The alternative."}]}]}}
if os.environ.get("AID"):
    p["agent_id"] = os.environ["AID"]
print(json.dumps(p))
'
}

run_gate() { # <question> <options> <session> [agent]
    local payload
    payload="$(ask_payload "$1" "$2" "$3" "${4:-}")"
    OUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$GATE" <<<"$payload" 2>&1)"
    RC=$?
    say "gate ($3)" "rc=$RC
$OUT"
    return 0
}

fresh_state() { rm -rf "$SEAT/.claude/state"; }

LEDGER="$SEAT/.claude/state/premise-ask-ledger.jsonl"

# ===========================================================================
echo "1. THE CHECK FIRES, ONCE, AND SAYS WHAT IT IS FOR"
# ===========================================================================
fresh_state
run_gate "G0 came back BLOCKED. The host keeps no question off your terminal when a hook fails, times out or is switched off." "RichOS draws its own prompts." sess-1
if [ "$RC" -eq 2 ]; then
    ok "1a. an unmarked question is interrupted once"
else
    bad "1a. an unmarked question is interrupted once" "rc=$RC"
fi
if printf '%s' "$OUT" | grep -q "quitting mid-flight\|QUITTING MID-FLIGHT"; then
    ok "1b. the interruption names the failure it exists for"
else
    bad "1b. the interruption names the failure it exists for" "$(printf '%s' "$OUT" | head -3 | tr '\n' ' ')"
fi
if printf '%s' "$OUT" | grep -q "NOT A REFUSAL"; then
    ok "1c. it says plainly that nothing is blocked"
else
    bad "1c. it says plainly that nothing is blocked"
fi
if printf '%s' "$OUT" | grep -q "OCCURRENCE-CLAIMED"; then
    ok "1d. it prints what this particular question carries"
else
    bad "1d. it prints what this particular question carries"
fi

# THE CASE THAT KEEPS THIS SURVIVABLE. A check that fired on the re-issue would
# trap a revision, and a revised question is exactly what it asked for.
run_gate "Which license does the public repo ship under?" "Apache-2.0, permissive, with a patent grant." sess-1
if [ "$RC" -eq 0 ]; then
    ok "1e. the next question in the episode goes straight through"
else
    bad "1e. the next question in the episode goes straight through" "rc=$RC"
fi

run_gate "A different question entirely, asked in a different session with nothing recorded for it yet." "Some option." sess-2
if [ "$RC" -eq 2 ]; then
    ok "1f. a different session gets its own single check"
else
    bad "1f. a different session gets its own single check" "rc=$RC"
fi

# ===========================================================================
echo ""
echo "2. THE DECLARATION IS THE RULE, NOT AN EVASION"
# ===========================================================================
fresh_state
run_gate "How should the terminal promise read?
premise-unverified: nobody has measured how often a hook is actually disabled, and the reconciler log would settle it in one grep" "The honest weaker promise." sess-3
if [ "$RC" -eq 0 ]; then
    ok "2a. a declared premise-unverified line skips the check entirely"
else
    bad "2a. a declared premise-unverified line skips the check entirely" "rc=$RC"
fi
if grep -q '"state":"DECLARED"' "$LEDGER" 2>/dev/null; then
    ok "2b. the declaration is logged where a reviewer sees it"
else
    bad "2b. the declaration is logged where a reviewer sees it" "$(cat "$LEDGER" 2>/dev/null | head -2)"
fi

fresh_state
printf '%s' '{"question":"Q\npremise-unverified: dunno","whole":"Q\npremise-unverified: dunno"}' \
    | python3 "$PRED" check > "$SANDBOX/bare.txt" 2>/dev/null
if grep -q 'MARKER-WITHOUT-REASON' "$SANDBOX/bare.txt"; then
    ok "2c. a bare marker exempts nothing — the reason is length-checked"
else
    bad "2c. a bare marker exempts nothing" "$(cat "$SANDBOX/bare.txt")"
fi

# ===========================================================================
echo ""
echo "3. SCOPE — whose questions are never checked"
# ===========================================================================
fresh_state
run_gate "Which filename should I use for the output?" "alpha.txt" sess-4 "a1234567890abcdef"
if [ "$RC" -eq 0 ]; then
    ok "3a. a worker's own clarifying question is not checked"
else
    bad "3a. a worker's own clarifying question is not checked" "rc=$RC"
fi

UNGOV="$SANDBOX/ungoverned"
mkdir -p "$UNGOV"
cat > "$UNGOV/orchestration.config" <<'CONF'
# adopted, but no CEO record is declared here
CONF
UNGOV_OUT="$(cd "$UNGOV" && CLAUDE_PROJECT_DIR="$UNGOV" bash "$GATE" \
    <<<"$(Q='Anything at all, with no premise whatsoever.' O='x' SID='sess-5' SEAT="$UNGOV" python3 -c '
import json, os
print(json.dumps({"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion",
 "session_id":os.environ["SID"],"cwd":os.environ["SEAT"],
 "tool_input":{"questions":[{"header":"Case","question":os.environ["Q"],
 "options":[{"label":"a","description":os.environ["O"]}]}]}}))')" 2>&1)"
UNGOV_RC=$?
if [ "$UNGOV_RC" -eq 0 ]; then
    ok "3b. a repository that declares no CEO record is never checked"
else
    bad "3b. a repository that declares no CEO record is never checked" "rc=$UNGOV_RC: $UNGOV_OUT"
fi

# 3d (orchestration reliability R3, 2026-10-24): "Asking an actually needed question must not
# be intercepted by a contradictory premise check." On 2026-10-23 the dispatch gate demanded
# the Windows CEO-TODO question and this check then intercepted it. A question that matches
# a prepared item on his list was premise-checked when it was prepared: it goes through.
cp "$HQ/wiki/open-items.md" "$SANDBOX/open-items.saved"
cp "$HQ/.ceo-todos" "$SANDBOX/ceo-todos.saved"
printf 'TODO_VIEW="CEO-TODOs.md"\nROOT_README="README.md"\nARTIFACT_ROOTS="q=."\n' >> "$HQ/.ceo-todos"
mkdir -p "$HQ/docs"; echo "# protocol" > "$HQ/docs/windows-protocol.md"
echo "# hq" > "$HQ/README.md"; echo "# view" > "$HQ/CEO-TODOs.md"
cat >> "$HQ/wiki/open-items.md" <<'REC'

### 1.2 READY-FOR-CEO — Windows real-capture test

- **Open:** `q/docs/windows-protocol.md`
- **Time:** 20 minutes
- **Done:** every check in the protocol marked pass or fail
- **Unblocks:** the Windows capture path

## 3. Buildable now — nobody blocked
REC
fresh_state
run_gate "Windows real-capture test: will you run the 20-minute protocol on a real Windows machine now? It unblocks the Windows capture path; every check gets pass or fail. The host keeps no question off your terminal when a hook fails." "Run it now." sess-9
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q "NOT A REFUSAL"; then
    ok "3d. his prepared CEO-TODO question goes straight through, not intercepted (R3)"
else
    bad "3d. his prepared CEO-TODO question was intercepted" "rc=$RC $(printf '%s' "$OUT" | head -2 | tr '\n' ' ')"
fi
run_gate "G0 came back BLOCKED. The host keeps no question off your terminal when a hook fails, times out or is switched off." "RichOS draws its own prompts." sess-9b
if [ "$RC" -eq 2 ]; then
    ok "3e. POSITIVE CONTROL: with the prepared item on the list, an unrelated unmarked question is still checked"
else
    bad "3e. an unrelated question went through because a prepared item exists" "rc=$RC"
fi
cp "$SANDBOX/open-items.saved" "$HQ/wiki/open-items.md"
cp "$SANDBOX/ceo-todos.saved" "$HQ/.ceo-todos"

# FAIL OPEN. A gate that can wedge the ability to ask the CEO anything is worse
# than the failure it prevents, so a broken predicate lets the question through.
BROKEN="$SANDBOX/broken-engine"
mkdir -p "$BROKEN/scripts/hooks" "$BROKEN/scripts/lib"
cp -R "$ENGINE/scripts/lib/." "$BROKEN/scripts/lib/"
cp "$ENGINE/scripts/hooks/guard-ceo-ruled-ask.sh" "$BROKEN/scripts/hooks/"
rm -f "$BROKEN/scripts/lib/premise-ask.py"
fresh_state
BROKEN_OUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$BROKEN/scripts/hooks/guard-ceo-ruled-ask.sh" \
    <<<"$(ask_payload 'A question with no premise at all.' 'x' sess-6)" 2>&1)"
BROKEN_RC=$?
if [ "$BROKEN_RC" -eq 0 ] && printf '%s' "$BROKEN_OUT" | grep -q "PREMISE CHECK IS OFF"; then
    ok "3c. a missing predicate passes the question AND announces that it is off"
else
    bad "3c. a missing predicate passes the question AND announces it" "rc=$BROKEN_RC: $(printf '%s' "$BROKEN_OUT" | head -2 | tr '\n' ' ')"
fi

# ===========================================================================
echo ""
echo "4. THE LEDGER — the only thing that can ever justify making this stronger"
# ===========================================================================
fresh_state
run_gate "First question of this episode, unmarked, stating a fact about something failing when a hook fails." "One way." sess-7
run_gate "First question of this episode, unmarked, stating a fact about something failing when a hook fails." "One way." sess-7
if grep -q '"reissue":"UNCHANGED"' "$LEDGER" 2>/dev/null; then
    ok "4a. re-issuing the SAME question is recorded as UNCHANGED"
else
    bad "4a. re-issuing the SAME question is recorded as UNCHANGED" "$(tail -2 "$LEDGER" 2>/dev/null)"
fi
fresh_state
run_gate "First question of this episode, unmarked, about a failure path." "One way." sess-8
run_gate "Rewritten: the same decision, now stating what measured it and what it changes for him." "One way." sess-8
if grep -q '"reissue":"CHANGED"' "$LEDGER" 2>/dev/null; then
    ok "4b. re-issuing a REWRITTEN question is recorded as CHANGED"
else
    bad "4b. re-issuing a REWRITTEN question is recorded as CHANGED" "$(tail -2 "$LEDGER" 2>/dev/null)"
fi
if [ "$(grep -c '"state":"CHECKED"' "$LEDGER" 2>/dev/null || echo 0)" -eq 1 ]; then
    ok "4c. exactly one check is recorded per episode"
else
    bad "4c. exactly one check is recorded per episode" "$(cat "$LEDGER" 2>/dev/null)"
fi

# ===========================================================================
echo ""
echo "5. THE LIVE CORPUS — every AskUserQuestion ever asked on this machine"
# ===========================================================================
# ASSERTS THE PREDICATE'S BEHAVIOR, NEVER THE CORPUS'S CONTENTS. The corpus
# grows every day; what must stay true is that the two questions of 2026-09-10
# produce findings, and that no real question is ever refused outright.
CORPUS_ROOT="${PREMISE_CORPUS_ROOT:-$HOME/.claude/projects}"
if [ ! -d "$CORPUS_ROOT" ]; then
    printf '  SKIP  5. no transcripts at %s — the live corpus cannot be replayed here.\n' "$CORPUS_ROOT"
    printf '        This is a SKIP and not a pass: sections 1-4 prove the shape, and only\n'
    printf '        this section proves the predicate still behaves on real questions.\n'
else
    CORPUS_OUT="$SANDBOX/corpus.txt"
    PRED="$PRED" CORPUS_ROOT="$CORPUS_ROOT" python3 - >"$CORPUS_OUT" 2>/dev/null <<'PY'
import glob, json, os, subprocess, sys

pred = os.environ["PRED"]
root = os.environ["CORPUS_ROOT"]
qs = []
for path in glob.glob(os.path.join(root, "**", "*.jsonl"), recursive=True):
    try:
        fh = open(path, encoding="utf-8", errors="replace")
    except Exception:
        continue
    with fh:
        for line in fh:
            if "AskUserQuestion" not in line:
                continue
            try:
                d = json.loads(line)
            except Exception:
                continue
            for b in (d.get("message") or {}).get("content") or []:
                if not isinstance(b, dict):
                    continue
                if b.get("type") != "tool_use" or b.get("name") != "AskUserQuestion":
                    continue
                ti = b.get("input") or {}
                for q in (ti.get("questions") or [ti]):
                    head = str(q.get("header") or "") if isinstance(q, dict) else ""
                    body = str(q.get("question") or "") if isinstance(q, dict) else str(q)
                    opts = []
                    if isinstance(q, dict):
                        for o in (q.get("options") or []):
                            if isinstance(o, dict):
                                opts += [str(o.get("label") or ""), str(o.get("description") or "")]
                    qf = (head + "\n" + body).strip()
                    if not qf:
                        continue
                    qs.append((d.get("timestamp", "")[:16], head, qf,
                               (qf + "\n" + "\n".join(opts)).strip()))

codes = {}
targets = {}
silent = 0
for ts, head, qf, whole in qs:
    job = json.dumps({"question": qf, "whole": whole})
    p = subprocess.run([sys.executable, pred, "check"], input=job,
                       capture_output=True, text=True)
    found, verdict = [], ""
    for ln in p.stdout.splitlines():
        f = ln.split("\t")
        if f[0] == "FINDING":
            found.append(f[1])
        elif f[0] == "VERDICT":
            verdict = f[1]
    if verdict == "SILENT":
        silent += 1
    for c in found:
        codes[c] = codes.get(c, 0) + 1
    if not found:
        codes["(none)"] = codes.get("(none)", 0) + 1
    if ts.startswith("2026-09-10T07:41") or ts.startswith("2026-09-10T07:54"):
        targets[ts] = ",".join(found) or "(none)"

print("TOTAL\t%d" % len(qs))
print("SILENT\t%d" % silent)
for c, k in sorted(codes.items(), key=lambda kv: -kv[1]):
    print("CODE\t%s\t%d" % (c, k))
for ts, c in sorted(targets.items()):
    print("TARGET\t%s\t%s" % (ts, c))
PY
    TOTAL="$(awk -F'\t' '$1=="TOTAL"{print $2}' "$CORPUS_OUT" 2>/dev/null || true)"
    if [ -n "${TOTAL:-}" ] && [ "$TOTAL" -ge 40 ]; then
        ok "5a. replayed $TOTAL real questions through the shipped predicate"
        printf '        rates: %s\n' "$(awk -F'\t' '$1=="CODE"{printf "%s=%s ", $2, $3}' "$CORPUS_OUT")"
    else
        bad "5a. the live corpus could not be replayed" "found ${TOTAL:-0} questions; expected at least 40"
    fi

    T_COUNT="$(awk -F'\t' '$1=="TARGET"' "$CORPUS_OUT" | grep -c . || true)"
    if [ "${T_COUNT:-0}" -ge 1 ]; then
        if awk -F'\t' '$1=="TARGET"' "$CORPUS_OUT" | grep -q "OCCURRENCE-CLAIMED"; then
            ok "5b. the 2026-09-10 questions produce an occurrence finding: $(awk -F'\t' '$1=="TARGET"{printf "%s -> %s; ", $2, $3}' "$CORPUS_OUT")"
        else
            bad "5b. the 2026-09-10 questions produce an occurrence finding" "$(awk -F'\t' '$1=="TARGET"' "$CORPUS_OUT")"
        fi
    else
        printf '  SKIP  5b. the 2026-09-10 transcripts are not on this machine.\n'
    fi

    SILENT="$(awk -F'\t' '$1=="SILENT"{print $2}' "$CORPUS_OUT" 2>/dev/null || echo 0)"
    if [ "${SILENT:-0}" -eq 0 ]; then
        ok "5c. no real question carried a premise-unverified declaration — the marked form is new, and this is the baseline it starts from"
    else
        ok "5c. $SILENT real question(s) carry a premise-unverified declaration"
    fi
fi

# ===========================================================================
echo ""
echo "6. THE MUTANTS"
# ===========================================================================
if [ -n "${RICHOS_MUTATION_INNER:-}" ]; then
    printf '  SKIP  6. running inside the mutation harness — not re-entering it.\n'
elif [ -x "$SRC_DIR/premise-ask.mutation.sh" ]; then
    MUT_OUT="$("$SRC_DIR/premise-ask.mutation.sh" 2>&1)"
    MUT_RC=$?
    say "mutation" "$MUT_OUT"
    if [ "$MUT_RC" -eq 0 ]; then
        ok "6a. every mutant of the predicate is killed: $(printf '%s' "$MUT_OUT" | tail -1)"
    else
        bad "6a. a mutant survived" "$(printf '%s' "$MUT_OUT" | tail -6 | tr '\n' ' ')"
    fi
else
    bad "6a. premise-ask.mutation.sh is missing or not executable" "$SRC_DIR/premise-ask.mutation.sh"
fi

echo ""
printf 'premise-ask.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

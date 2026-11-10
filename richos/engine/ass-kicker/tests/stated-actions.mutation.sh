#!/usr/bin/env bash
#
# stated-actions.mutation.sh — PROVES THE STATED-ACTIONS SUITE CAN FAIL.
#
# A green suite is evidence of nothing until it has been shown turning red for
# the right reason. This gate's two arms are both the two-sided kind a corpse
# satisfies: "a stated-but-untaken action is refused" is passed by a gate that
# refuses everything, and "a same-turn dispatch is let through" by one that
# refuses nothing. So every property below is removed from a throwaway copy of
# the shipped source, one at a time, and the suite must go red AT THE NAMED
# CASE — not merely somewhere.
#
# Three things are asserted per mutant:
#   1. ass-kicker.test.sh FAILS,
#   2. the SPECIFIC case fails (a red elsewhere is an unrelated red),
#   3. the mutation actually applied (a needle that matched nothing gives a
#      green run that looks like a green run, which is the same trap again).
#
# Every mutant is a copy of the engine subtree under a temp dir. Nothing here
# touches the real tree.
#
# Run directly: ass-kicker/tests/stated-actions.mutation.sh
# Run by:       scripts/hooks/contract-integrity.test.sh (case SA2)
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t stated-actions-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
old = old.replace("\\n", "\n")
new = new.replace("\\n", "\n")
with open(path, encoding="utf-8") as fh:
    src = fh.read()
if old not in src:
    sys.stderr.write("MUTATION TARGET ABSENT — the source has drifted:\n  %s\n" % old)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(src.replace(old, new, 1))
PYEOF

# mutant <name> <expected-failing-case> <rel-file> <old> <new> <why>
# --- concurrency ------------------------------------------------------------
# Each mutant builds its own sandbox under $SANDBOX/<name> and runs a whole
# suite against it, so no two mutants share a path and none of them is ordered
# against another. They ran one at a time only because a `for` loop is what this
# harness was first written as. mutation-pool.sh bounds the fan-out, keeps the
# report in declaration order, and counts a KILLED mutant as a failure rather
# than letting it vanish from the tally. RICHOS_MUTANT_JOBS overrides the degree.
# shellcheck source=../lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

build() {
    local dir="$1"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib" "$dir/ass-kicker/tests"
    cp "$ENGINE_ROOT/ass-kicker/ass-kicker.py" "$dir/ass-kicker/"
    cp "$ENGINE_ROOT/ass-kicker/tests/ass-kicker.test.sh" \
       "$ENGINE_ROOT/ass-kicker/tests/record-claims.replay.py" "$dir/ass-kicker/tests/"
    cp "$ENGINE_ROOT/scripts/hooks/ass-kicker.sh" \
       "$ENGINE_ROOT/scripts/hooks/turn-manifest.py" \
       "$ENGINE_ROOT/scripts/hooks/guard-idle-land.py" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" "$dir/scripts/lib/"
    chmod +x "$dir/scripts/hooks/"*.sh
}

# A missing relocation dependency must not count as a killed mutant.
build "$SANDBOX/control"
if ! bash "$SANDBOX/control/ass-kicker/tests/ass-kicker.test.sh" >"$SANDBOX/control/out.txt" 2>&1; then
    echo "FATAL: the intact stated-actions sandbox is red; mutation results would be meaningless." >&2
    cat "$SANDBOX/control/out.txt" >&2
    exit 1
fi

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    build "$dir"
    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    bash "$dir/ass-kicker/tests/ass-kicker.test.sh" >"$dir/out.txt" 2>&1
    local rc=$?
    if [ "$rc" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    if ! grep -qF "FAIL  $want" "$dir/out.txt"; then
        printf '  FAIL  %s — the suite went red, but NOT at %s (so the red is unrelated).\n' "$name" "$want"
        grep '  FAIL' "$dir/out.txt" | sed 's/^/          /'
        return 1
    fi
    printf '  PASS  %s — removing it turns %s red\n' "$name" "$want"
    return 0
}
# mutant <name> ... — a SUBMISSION, not an execution. The body above is unchanged
# except that it returns its verdict instead of incrementing a counter: it runs
# in a pool worker, which is a subshell, so an increment in there would mutate a
# copy and be discarded. The pool prints the bodies in DECLARATION order, so this
# harness's report is byte-identical to the serial one but for the durations.
mutant() {
    mut_pool_submit "$1" _mutant_body "$@"
}

echo "=== stated-actions gate: every property, proven load-bearing by removing it ==="

H="scripts/hooks/ass-kicker.sh"
P="ass-kicker/ass-kicker.py"

# --- 1. IT BLOCKS ----------------------------------------------------------
mutant does-not-block "a." "$P" \
    '    return 2 if enforce else 0' \
    '    return 0' \
    "a gate that reports is a gate that has already failed this test."

mutant enforce-flag-ignored "y." "$P" \
    '    return 2 if enforce else 0' \
    '    return 2' \
    "report-only exists so an adopter can read its own numbers before arming it; a flag that does nothing is a lie in the config."

# --- 2. ARM 1: WHAT IS READ, AND WHAT IS NOT ------------------------------
# The raw bullet in f. is also excluded by subject anchoring. f3. reaches
# the announcement recognizer when this list exclusion is removed.
mutant list-items-scanned "f3." "$P" \
    '                or LIST_LINE_RE.match(line) or TABLE_LINE_RE.match(line)):' \
    '                or TABLE_LINE_RE.match(line)):' \
    "a bulleted plan is not a statement; 4 of the 13 corpus false fires were list items."

mutant fences-not-stripped "e2." "$P" \
    '    text = FENCE_RE.sub(" ", message)' \
    '    text = message' \
    "an ecs record carries the sentence as before=\"...\"; scanning it would refuse the correction turn."

mutant quotes-not-stripped "e." "$P" \
    '    text = DQUOTE_RE.sub(" ", text)' \
    '    text = text' \
    "the refusal prints the clause in quotes; without the strip, pasting it back re-fires the gate."

mutant reported-speech-scanned "e3." "$P" \
    '            or REPORTED_RE.search(sentence) or PROPOSAL_RE.search(sentence)' \
    '            or PROPOSAL_RE.search(sentence)' \
    "'Frank breaks it first, I said' is the apology, and a gate that fires on the apology punishes the fix."

mutant report-verbs-not-excluded "g." "$P" \
    '            if m and m.group("verb").lower() not in REPORT_VERBS:' \
    '            if m:' \
    "'Frank recommends the hybrid' is an agent's output; 6 of the 13 corpus false fires were report verbs."

mutant bare-object-accepted "h." "$P" \
    '    return re.compile(r"^(?P<role>" + alt + r")\s+(?P<verb>[a-z]+s)\s+" + OBJECT, re.S)' \
    '    return re.compile(r"^(?P<role>" + alt + r")\s+(?P<verb>[a-z]+s)\s+", re.S)' \
    "'Art designs Bootstrap components' describes a craft; the pronoun/determiner object is what makes it an act."

mutant proposal-not-excluded "d." "$P" \
    '            or REPORTED_RE.search(sentence) or PROPOSAL_RE.search(sentence)' \
    '            or REPORTED_RE.search(sentence)' \
    "all 10 raw 1b fires in the corpus were 'Say the word and I'll dispatch it'."

mutant ends-on-ceo-ignored "i." "$P" \
    '    if claims and ends_on_ceo(message):' \
    '    if False:' \
    "a plan stated above a question to the CEO is a proposal, and the turn may end on his answer."

mutant conditional-clause-scanned "o." "$P" \
    '    if SUBORD_RE.search(clause) or (progress and PROGRESS_RE.search(clause)):' \
    '    if (progress and PROGRESS_RE.search(clause)):' \
    "'Frank breaks it the moment Sage returns' is a condition, not an announcement."

mutant progress-marker-ignored "f2." "$P" \
    '    if SUBORD_RE.search(clause) or (progress and PROGRESS_RE.search(clause)):' \
    '    if SUBORD_RE.search(clause):' \
    "'Zach builds it — in flight now' is a liveness claim, owned report-only by the state-claims guard."

mutant negation-ignored "p." "$P" \
    '    if (NEG_RE.search(sentence) or MODAL_RE.search(sentence) or PAST_RE.search(sentence)' \
    '    if (MODAL_RE.search(sentence) or PAST_RE.search(sentence)' \
    "'Frank breaks it, but not tonight' is a deferral, and the deferral notice owns it."

mutant emphasized-list-scanned "f3." "$P" \
    'LIST_LINE_RE = re.compile(r"^[ \t]*[*_]{0,3}(?:[-*+•]|\d{1,3}[.)])[ \t]")' \
    'LIST_LINE_RE = re.compile(r"^[ \t]*(?:[-*+•]|\d{1,3}[.)])[ \t]")' \
    "'**1. Zach builds it — in flight now.**' was the replay's own false fire; the emphasis hid the number."

mutant role-future-blocks "j." "$P" \
    '                (reported if arm == "role-future" else unmet).append((who, clause, arm))' \
    '                unmet.append((who, clause, arm))' \
    "the role-future shape measured 0 true in 2 fires; it reports, and a blocking version is the 17% gate rebuilt."

# --- 3. ARM 1: WHAT DISCHARGES A CLAIM -----------------------------------
mutant any-agent-does-not-discharge-1a "b2." "$P" \
    '            if arm != "first-person-dispatch" or inp is None:' \
    '            if inp is None:' \
    "role-matching 1a fired on 'Sage is designing that now ... Zach builds it.' in the turn that spawned Sage — a plan in motion."

mutant any-agent-discharges-1b "c2." "$P" \
    '            if arm != "first-person-dispatch" or inp is None:' \
    '            if True:' \
    "'I'm dispatching Frank' names who; a spawn of somebody else does not make it true."

mutant teammate-message-does-not-discharge "b3." "$P" \
    '            if not to or to not in ("team-lead", "main", "lead"):' \
    '            if False:' \
    "a resumed teammate is a dispatch; refusing it would refuse the turn that did the work."

mutant ask-does-not-exempt "k." "$P" \
    '        if name == "AskUserQuestion":\n            return "ceo-asked"' \
    '        if name == "AskUserQuestion":\n            pass' \
    "ending a turn on a question he has to answer is the one move nobody else can make for him."

mutant turn-not-scoped "l." "$P" \
    '        calls, inputs, err = turn_calls(tm, transcript, prompt_id)' \
    '        calls, inputs, err = turn_calls(tm, transcript, None)' \
    "the sibling that read session-wide was inverted for weeks: after the first spawn, every turn 'called Agent'."

# --- 4. ARM 2: EVERY TERM ---------------------------------------------------
mutant arm2-does-not-block "n." "$P" \
    '    undeclared = stop is not None and stop["verdict"] == "undeclared-stop"' \
    '    undeclared = False' \
    "six times in one day the CEO restarted work that paused on a report; a notice would be the reaper's CLEAN again."

mutant arm2-ignores-agent-call "n2." "$P" \
    '    if "Agent" in turn["tools"]:\n        return {"finishes": finishes, "verdict": "started", "detail": "Agent"}' \
    '    if False:\n        return {"finishes": finishes, "verdict": "started", "detail": "Agent"}' \
    "the turn that landed a design and spawned its reviewer is the turn this rule asks for."

mutant arm2-ignores-background "n3." "$P" \
    '    if turn.get("backgrounded"):' \
    '    if False:' \
    "a tool call sent to the background is the turn handing work off."

mutant arm2-ignores-ask "n4." "$P" \
    '    if "AskUserQuestion" in turn["tools"]:\n        return {"finishes": finishes, "verdict": "ceo-owed", "detail": "AskUserQuestion"}' \
    '    if False:\n        return {"finishes": finishes, "verdict": "ceo-owed", "detail": "AskUserQuestion"}' \
    "a question to the CEO is owed an answer, and refusing that turn demands a dispatch over his head."

# THE MUTATION IS NOW THE OTHER WAY ROUND (2026-09-20, CEO ruling §68). It used
# to remove the hold read and expect n5 to go red; the hold read is DELETED, so
# what has to be proven load-bearing is its ABSENCE. This mutant PUTS THE
# INFERENCE BACK -- a direct scan of the transcript's user records for a hold
# phrase, which is what a future edit reaching for the old behavior would look
# like -- and n5, which now expects a refusal, must go red.
mutant arm2-reads-his-words-again "n5." "$P" \
    '    decl = idle.stop_declaration(message)' \
    '    import re as _re, json as _json\n    for _ln in open(transcript, encoding="utf-8", errors="replace"):\n        try:\n            _r = _json.loads(_ln)\n        except Exception:\n            continue\n        if _r.get("type") == "user" and _re.search(r"hold everything", str(_r.get("message", ""))):\n            return {"finishes": finishes, "verdict": "ceo-owed", "detail": "his words"}\n    decl = idle.stop_declaration(message)' \
    "a hook that reads his sentences for an instruction is the thing the CEO ruled out; if putting it back leaves the suite green, nothing is holding it out."

mutant arm2-ignores-declaration "n6." "$P" \
    '    if decl and decl.get("ok"):' \
    '    if False:' \
    "without a route through, a blocking gate with any legitimate-stop class gets unwired within a day."

mutant finish-signal-reads-any-notice "n9." "$P" \
    '    finishes = idle.agent_finishes(turn.get("notices"))' \
    '    finishes = ["Break the elimination design"] if turn.get("notices") else []' \
    "a killed agent and a finished shell arrive in the same envelope; only the summary shape tells them apart."

mutant declaration-not-shown-to-him "n6." "$H" \
    'if [ -n "$DECLARED_LINE" ]; then' \
    'if [ -z "$DECLARED_LINE" ]; then' \
    "a justification filed where nobody reads it is a flag with a longer spelling."

mutant refusal-lacks-the-declaration-form "n." "$P" \
    '        out.append("           stop-declared: <case> — <why, in a full sentence>")' \
    '        out.append("           (declare the stop in the documented form)")' \
    "an escape described but not spelled is an escape that gets waived by guesswork."

# --- 4b. ARM 3: RECORDED, NOT WRITTEN ---------------------------------------
mutant arm3-alert-not-issued "R6." "$P" \
    '    if arm3 is not None and not unmet and not undeclared:\n        emit_alert(arm3)' \
    '    if False:\n        emit_alert(arm3)' \
    "the CEO asked for an alert when a recorded claim had no matching write; a check that finds it and says nothing is no check."

mutant arm3-silent-on-refire "R13." "$P" \
    '            _arm3_log(payload, arm3)\n            emit_alert(arm3)' \
    '            _arm3_log(payload, arm3)' \
    "the re-fire carries the reply that ends the turn; staying silent there lets the false claim through when ARM 1 or 2 refused first."

mutant arm3-alert-shown-to-the-ceo "R1." "$H" \
    'out["hookSpecificOutput"] = {"hookEventName": "Stop"' \
    'out["systemMessage"] = os.environ["ALERT_LINE"]\nout["hookSpecificOutput"] = {"hookEventName": "Stop"' \
    "the CEO said he does not need to see it: the alert is the lead's, in the lead's own context, with no user-visible message."

mutant arm3-alert-only-on-systemmessage "R1." "$H" \
    'out["hookSpecificOutput"] = {"hookEventName": "Stop", "additionalContext": os.environ["ALERT_LINE"]}' \
    'out["systemMessage"] = os.environ["ALERT_LINE"]' \
    "a systemMessage reaches the person and never the model, so the lead would not read it (measured, stop-hook-notice.sh)."

mutant arm3-alerts-on-a-written-record "R10." "$P" \
    '    return [(s, c, p) for s, c, p in v["failing"] if c in ALERT_CODES]' \
    '    return list(v["failing"])' \
    "a claim backed by a durable write is true; alerting because the reply did not recite a path or SHA is the refusal this alert replaced."

mutant arm3-alerts-on-a-citation "R4." "$P" \
    'ALERT_CODES = ("nothing", "private", "rule", "followup", "unverified")' \
    'ALERT_CODES = ("nothing", "private", "rule", "followup", "unverified", "state")' \
    "'it is written in the three places I read' cites an existing record; it is not a claim that this turn wrote one."

mutant arm3-cited-record-demanded-again "A7a." "$P" \
    '                    return "cited", sha, repo' \
    '                    return "unverified", sha, "mutated"' \
    "a follow-up committed in an earlier turn and cited with its file and commit would be alerted again, the 2026-10-23 14:06Z and 14:11Z false alerts (orchestration reliability A7)."

mutant arm3-confession-is-a-claim "R30." "$P" \
    'if (ARM3_CONFESS_RE.search(c[m.start():]) or' \
    'if (False or' \
    "'I recorded it wrong.' confesses a past error; it claims no record in this turn."

mutant arm3-others-saved-is-a-claim "R31." "$P" \
    ' or ARM3_OTHERS_SAVED_RE.search(c)' \
    ' or False' \
    "'saved their work' is version control about somebody else's work, not a record."

mutant arm3-description-is-a-claim "R32." "$P" \
    'or ARM3_DESCRIBES_RE.search(s)):' \
    'or False):' \
    "'Documented, never fixed.' describes an existing document; it claims no write."

mutant arm3-list-items-skipped "R1." "$P" \
    '        if m:\n            line = line[m.end():]' \
    '        if m:\n            continue' \
    "the 2026-10-02 claim was a bulleted line; ARM 1 skips lists, ARM 3 must not."

mutant arm3-cd-not-followed "R1." "$P" \
    '                b = _resolve(d, b)' \
    '                pass' \
    "the 2026-10-02 memory write was cd …/memory && cat > …; without the cd it reads as a file in the working directory."

mutant arm3-shell-variable-not-expanded "R21." "$P" \
    '    command = expand(command)' \
    '    pass' \
    "the 2026-09-08 memory write was M=…; cat > \"\$M/…\"; an unexpanded target is no target."

mutant arm3-memory-is-a-record "R7." "$P" \
    '        return "memory"' \
    '        return "file"' \
    "a note in the private memory directory is not a record; §97 and the CEO both say so."

mutant arm3-private-notes-not-honored "R8." "$P" \
    '        if kind and PRIVATE_NOTES_RE.search(s) and not RULE_NOUN_RE.search(s):' \
    '        if False and kind and PRIVATE_NOTES_RE.search(s) and not RULE_NOUN_RE.search(s):' \
    "a private-notes claim is a note claim; only a committed repository file backs it (CEO 2026-10-21)."

mutant arm3-notes-pointer-ignored "R5b." "$P" \
    '        if NOTES_POINTER_RE.search(sentence):' \
    '        if False:' \
    "'saved this as a rule in my notes' was excused by an unrelated commit until this term existed."

mutant arm3-ecs-not-private "R27." "$P" \
    '"ecs": bool(ECS_RE.search(command))}' \
    '"ecs": False}' \
    "most September 'Recorded.' lines had only an ECS checkpoint behind them; the refusal must say so."

mutant arm3-third-person-scanned "R23." "$P" \
    '            if not m and not (i > 0 and third):' \
    '            if not m:' \
    "'He …, and recorded the anomaly' is a teammate's act, not the lead's claim."

mutant arm3-earlier-turn-judged "R22." "$P" \
    '            elif ARM3_PAST_RE.search(seg):\n                continue' \
    '            elif False:\n                continue' \
    "'the thing I recorded an hour ago' refers to another turn; this turn's calls cannot judge it."

mutant arm3-noun-use-scanned "R24." "$P" \
    '    r"(?:now\s+)?" + REC_VERB + _FOLLOW, re.I)' \
    '    r"(?:now\s+)?" + REC_VERB + r"\b", re.I)' \
    "'Recorded numbers:' is a noun phrase; the corpus had three of these shapes."

# --- 5. THE WRAPPER'S OWN STAND-DOWNS ARE SEEN ----------------------------
mutant stood-down-silently "t." "$H" \
    'if [ "$CHECK_STATED_ACTIONS" = "0" ]; then' \
    'if false; then' \
    "an opt-out that cannot be seen is a defense that decays into a rumor."

mutant missing-dependency-not-announced "y2." "$H" \
    'for _DEP in turn-manifest.py guard-idle-land.py; do' \
    'for _DEP in turn-manifest.py; do' \
    "with guard-idle-land.py gone ARM 2 decides nothing and the wrapper would start perfectly and exit 0 every turn."

mutant analyzer-crash-reported-as-normal "y3." "$H" \
    'if [ "$RC" != "0" ] && [ "$RC" != "2" ]; then' \
    'if false; then' \
    "a runtime failure may allow turn-end but must never be reported as a healthy analyzer."

# --- the verdict ------------------------------------------------------------
# Drained rather than accumulated: PASS/FAIL below come from the workers' exit
# codes, and a worker that left no exit code is counted as a FAILURE. The tally
# that follows is unchanged and still decides this harness's exit status.
# ADDITIVE, NOT ASSIGNMENT, and that is a correction found by measurement rather
# than by reading: mechanical-findings.mutation.sh scores one case BEFORE its
# mutants, and `PASS="$MUT_POOL_PASS"` silently discarded it. A tally that
# overwrites an earlier verdict is a green fraction over a case that ran and was
# thrown away. Harnesses that score nothing beforehand are unaffected, because
# adding to zero is assignment.
mut_pool_drain
# A harness that declared mutants and ran NONE must not exit 0 -- see
# mut_pool_require_submissions for the run where exactly that happened.
mut_pool_require_submissions "$(basename "$0")"
PASS=$(( PASS + MUT_POOL_PASS ))
FAIL=$(( FAIL + MUT_POOL_FAIL ))
mut_pool_report_line "$(( $(sw_now_ms) - MUT_WALL_T0 ))"
mut_pool_cleanup

echo
if [ "$FAIL" -eq 0 ]; then
    printf '✓ %d/%d properties are load-bearing\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '✗ %d/%d — %d propert(ies) are NOT load-bearing\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1

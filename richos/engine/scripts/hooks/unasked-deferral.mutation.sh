#!/usr/bin/env bash
#
# unasked-deferral.mutation.sh — PROVES THE UNASKED-DEFERRAL SUITE CAN FAIL.
#
# 34 green ticks are evidence of nothing until somebody shows them turning red
# for the right reason. For a guard whose entire mechanism is a set of regular
# expressions, that is not a general anxiety — it is the specific way this kind
# of suite lies:
#
#   BREAK THE MATCHER AND EVERY SILENCE CASE GOES GREEN. Sections 3, 4 and 5 of
#   the suite are almost all "the hook says nothing". A matcher that matches
#   NOTHING AT ALL passes every one of them. So the suite carries positive
#   controls, and this file proves the controls are the thing holding it up:
#   the first mutant below deletes the notice entirely and the run must go red
#   at 1a, not merely somewhere.
#
# The method: take the shipped source, remove ONE property at a time, and assert
#   1. unasked-deferral.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement matching nothing gives a
#      green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree.
#
# Run directly: scripts/hooks/unasked-deferral.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

# EXPORTED, not set per invocation, and that is deliberate. This harness runs
# the suite it mutates -- sometimes the sandboxed copy, sometimes the one in
# the real tree -- and that suite now invokes this harness at its end. Exported
# once here, the flag reaches every child however many invocation sites this
# file grows; set per-call, one missed site is an infinite regress.
export RICHOS_MUTATION_INNER=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t unasked-deferral-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
# THE NEWLINE TOKEN IS @@NL@@, NOT \n. The sibling mutation suites decode a
# literal backslash-n, which is correct for their targets and WRONG for these:
# every pattern in guard-unasked-deferral.py contains [^.\n], so a \n decoder
# silently corrupts the needle and every mutant reports "target absent". That is
# a mutation suite failing OPEN — it looks like drift and is actually the
# harness eating its own input.
old = old.replace("@@NL@@", "\n")
new = new.replace("@@NL@@", "\n")
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
# shellcheck source=../lib/mutant-suite-run.sh
. "$ENGINE_ROOT/scripts/lib/mutant-suite-run.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib"
    cp "$ENGINE_ROOT/scripts/hooks/notice-unasked-deferral.sh" \
       "$ENGINE_ROOT/scripts/hooks/guard-unasked-deferral.py" \
       "$ENGINE_ROOT/scripts/hooks/unasked-deferral.test.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" \
       "$ENGINE_ROOT/scripts/lib/hold_words.py" "$dir/scripts/lib/"
    mkdir -p "$dir/mega-lander"
    cp "$ENGINE_ROOT/mega-lander/workspaces.py" "$dir/mega-lander/"
    chmod +x "$dir/scripts/hooks/"*.sh

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (scripts/lib/mutant-suite-run.sh,
    # hunt part 3 finding 9): in this suite a printed FAIL line always ends the run
    # red, and the stop matches the TRIMMED want as a fixed string, exactly what the
    # escaped witness test below matches, so it is never looser than that test.
    mutant_suite_run "$dir/out.txt" "$(printf '%s' "$want" | sed 's/[[:space:]]*$//')" bash "$dir/scripts/hooks/unasked-deferral.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want"
        return 0
    fi
    local rc=$MUTANT_RC
    if [ "$rc" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    # THE CASE MATCH IS A LITERAL WITH A BOUNDARY, NOT A BARE REGEX, and that
    # is a correction rather than a style. It used to be `grep -q "FAIL  $want"`,
    # in which an unescaped `.` matches ANY character: `g.` matched `FAIL  g1.`,
    # `p1.` matched `FAIL  p10.`, `C1.` matched `FAIL  C10.`. Demonstrated on
    # 2026-09-06 rather than suspected -- case `g.` of turn-manifest.test.sh was
    # deliberately weakened so it could not fail, and the harness still reported
    # `removing it turns g. red`, having matched a sibling case. The mutant was
    # genuinely killed every time; what was false was the claim about WHICH case
    # killed it, and that claim is the entire second clause of this harness's
    # contract. A wrong witness is exactly what root-contract's M1 turned out to
    # be. Metacharacters are escaped and the token must end at a non-alphanumeric
    # or end of line. The token is TRIMMED first, and that is not tidiness:
    # ceo-asks writes its cases as "C3. " with a trailing space, so the
    # boundary landed one character late and demanded a non-alphanumeric where
    # the label's first word begins. Five mutants went red for that reason on
    # the first run of this very change -- a red that looked exactly like a
    # finding and was not one.
    want_re="$(printf '%s' "$want" | sed 's/[[:space:]]*$//' | sed 's/[][\.*^$(){}?+|\/]/\\&/g')"
    # NO TRAILING BOUNDARY, and that is a correction to this very change made an
    # hour after it: the first version required the token to end at a
    # non-alphanumeric, which assumed every suite writes `FAIL  z1. some words`.
    # claim-roles' suite writes `FAIL  z1.landed-claim-about-a-dangling-commit`,
    # so the boundary demanded a space where a slug begins and turned ALL TWENTY
    # of its mutants into misfires -- twenty reds that looked exactly like
    # findings. The boundary was never what fixed anything: ESCAPING is. With
    # `.` taken literally, `z1.` cannot match `z1b.` and `C1.` cannot match
    # `C10.`, because the character after the prefix is a digit and not a dot.
    if ! grep -q "FAIL  ${want_re}" "$dir/out.txt"; then
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

echo "=== the unasked deferral: every property, proven load-bearing by removing it ==="

H="scripts/hooks/notice-unasked-deferral.sh"
A="scripts/hooks/guard-unasked-deferral.py"

# --- 1. THE NOTICE ITSELF, AND THE POSITIVE CONTROLS UNDER THE SILENCES ----
mutant no-notice "1a " "$H" \
    'stop_notice_abnormal "$KEY" \' \
    'notice_clean; stop_notice_abnormal "$KEY" \' \
    "the whole point: a deferral he was not asked about must end the turn with something on screen."

mutant matcher-dead "1a " "$A" \
    '    scan = strip_quoted(text)' \
    '    scan = ""' \
    "a matcher that matches nothing passes every silence case in sections 3-5. This is the trap the positive controls exist for."

mutant unquoted-notice "1b " "$H" \
    'THIS TURN DEFERS WORK: \"${QUOTE}\"' \
    'THIS TURN DEFERS WORK.' \
    "quoting the construction is what makes the notice actionable rather than a scold — he can see the sentence he wrote."

mutant vague-notice "1c " "$H" \
    'A deferral you did not choose is a decision taken on your behalf' \
    'FYI' \
    "the notice must name the harm, not merely announce that something happened."

mutant blocking "1d " "$H" \
    'stop_notice_abnormal "$KEY" \' \
    'exit 2; stop_notice_abnormal "$KEY" \' \
    "a Stop hook that refuses turns over a regex on prose wedges the session and earns CHECK_UNASKED_DEFERRAL=0."

# --- 2. THE THREE DISCHARGES ----------------------------------------------
mutant no-ask-discharge "2a " "$A" \
    '    if "AskUserQuestion" in tools:' \
    '    if False and "AskUserQuestion" in tools:' \
    "putting the deferral to him IS the rule being satisfied; firing anyway punishes the correct behaviour."

mutant no-handback-discharge "2b " "$A" \
    '    if text and _HANDBACK.search(text):' \
    '    if False and _HANDBACK.search(text):' \
    "without it the guard fires on 12 corpus turns at 33% — including every turn where the CEO himself ordered the stop."

mutant no-agent-discharge "2d " "$A" \
    '    if "Agent" in tools and not any(h[0] == HELD for h in hits):' \
    '    if False and "Agent" in tools:' \
    "condition 3 of the rule: a turn that starts the work is not a turn that deferred it."

mutant held-discharged-by-spawn "T1a " "$A" \
    '    if "Agent" in tools and not any(h[0] == HELD for h in hits):' \
    '    if "Agent" in tools:' \
    "a reply saying the work is held cannot be starting it: a spawn beside it discharges nothing (2026-10-22)."

# --- 3. PRECISION: the narrowings that keep ordinary prose silent ----------
# Every one of these is a measured decision from the 2,198-turn corpus, recorded
# in the analyzer's REFUSED table. Widening it back out must turn a specific
# silence case red — which is the only proof the narrowing was load-bearing
# rather than decorative.
mutant first-person-and-now-marker-dropped "3e " "$A" \
    '|I am)\s+[^.\n]{0,60}?\brather\s+than\s+" + _ACT +@@NL@@     r"\b[^.\n]{0,40}?\b" + _NOW + r"\b"),' \
    '|I am|)\s*" + r"\brather\s+than\s+" + _ACT + r"\b"),' \
    "'extending the CSV rather than adding a second ledger' is a DESIGN CHOICE. The first-person subject and the now-marker are what separate a choice from a postponement."

mutant bundle-object-dropped "3f " "$A" \
    '     r"(?:goes|go|will go|can go|would go|rides|ships|lands)\s+in\s+with\b"@@NL@@     r"[^.\n]{0,60}?\b(?:land|lands|landing|pass|spawn|batch|round|merge|"@@NL@@     r"deploy|commit)\b"),' \
    '     r"(?:goes|go|will go|can go|would go|rides|ships|lands)\s+in\s+with\b"),' \
    "'the fix goes in with the next pilot restart' was describing a checklist inside a document. The object has to be a unit of orchestration work."

mutant bare-deliberately-not "3g " "$A" \
    '    ("not-dispatching",' \
    '    ("deliberately-not", r"\bdeliberately\s+not\b"),@@NL@@    ("not-dispatching",' \
    "bare 'deliberately not' measured 4/23 on the corpus — the dominant use is the record's own 'Deliberately NOT open' heading."

mutant postponing-family-back "3h " "$A" \
    '    ("chose-not-to-start-now",' \
    '    ("postponing-it", r"\bI.?m\s+(?:holding|queueing|parking)\s+the\s+\w+\b"),@@NL@@    ("chose-not-to-start-now",' \
    "the family measured at 44% and then deleted: it fires on a plainly correct pipeline hold — holding a merge until three agents report."

# --- 4. QUOTED IS NOT USED ------------------------------------------------
mutant no-quote-strip "4a " "$A" \
    '    out = _DQUOTE.sub(blank, out)' \
    '    out = out' \
    "a turn describing this guard quotes its own triggers; a guard that fires on its own documentation is a guard that gets switched off."

mutant no-tick-strip "4c " "$A" \
    '    out = _TICK.sub(blank, out)' \
    '    out = out' \
    "backticked spec text is quotation, not speech."

# --- 5. VISIBLE STAND-DOWNS ------------------------------------------------
mutant silent-standdown "5a " "$H" \
    '    stop_notice_abnormal "stood-down" \' \
    '    true \' \
    "an opt-out the operator cannot see is a defense that decays into a rumour — a sentence this engine has already had to learn twice."

mutant silent-no-analyzer "5c " "$H" \
    '    stop_notice_abnormal "no-analyzer" \' \
    '    true \' \
    "a clean session and an absent checker must never look the same."

mutant silent-root-failure "5e " "$H" \
    '    stop_notice_abnormal "root-failure" \' \
    '    true \' \
    "a guard that cannot tell which repository it governs is not entitled to be quiet."

# --- 6. DE-DUPLICATION -----------------------------------------------------
# The first mutant here is a bug this suite ACTUALLY CAUGHT on its first run, not
# a hypothetical: the wrapper recorded "ok" before the analysis, so the ledger
# alternated between "ok" and the finding key and the same deferral was
# announced on every single turn.
mutant normal-before-analysis "6a " "$H" \
    'set +e@@NL@@FINDING="$(printf' \
    'stop_notice_normal "back"@@NL@@set +e@@NL@@FINDING="$(printf' \
    "the ledger holds ONE state per (session, hook); writing 'ok' on entry and the finding afterwards defeats de-duplication entirely."

mutant constant-state-key "6b " "$H" \
    'KEY="deferral:${CONSTRUCTION}:$(printf' \
    'KEY="deferral"; : "$(printf' \
    "a constant key mutes every SUBSEQUENT deferral in the session — the first one silences the rest."

# --- 7. THE TRANSCRIPT ----------------------------------------------------
mutant transcript-unknown-is-empty "7a " "$A" \
    '        return None  # UNKNOWN — never the same as "no tools used"' \
    '        return set()' \
    "an unreadable transcript is not a turn with no tools: treating it as one accuses turns whose discharges could not be read."

mutant no-stop-hook-active "7c " "$A" \
    '    if payload.get("stop_hook_active"):' \
    '    if False and payload.get("stop_hook_active"):' \
    "on a blocked turn's re-fire the text is unchanged; re-accusing piles a second notice onto a turn already in trouble."

# --- 8. THE RECOVERY LINE MUST NOT CLAIM A CHECK THAT DID NOT HAPPEN -------
# Exit code 4 is the whole fix for the one affirmatively false statement the
# 2026-09-05 payload survey found. Collapsing it back into 0 restores the
# defect exactly: the wrapper would again say "This turn's text was checked"
# over a payload that carried no turn text.
mutant no-unevaluated-code "8b " "$A" \
    '        print("no-turn-text")
        return 4' \
    '        return 0' \
    "0 means CHECKED-and-clean. Reusing it for NOT-CHECKED is what let the wrapper tell the operator his turn had been examined when nothing had read it."

mutant unevaluated-borrows-clean-sentence "8c " "$H" \
    '    4) notice_unevaluated "$(printf '"'"'%s'"'"' "$FINDING" | head -1 | tr -d '"'"'[:space:]'"'"')" ;;' \
    '    4) notice_clean ;;' \
    "routing the not-evaluated case through the clean sentence is the defect with the analyzer fixed and the wrapper still lying."

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

echo ""
echo "=== $PASS proven load-bearing, $FAIL not ==="
[ "$FAIL" -eq 0 ]

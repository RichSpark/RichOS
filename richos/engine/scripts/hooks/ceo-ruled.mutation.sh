#!/usr/bin/env bash
#
# ceo-ruled.mutation.sh — PROVES THE CEO-RULED SUITE CAN FAIL, AND PROVES ITS
#                         LIVE-RECORD SECTION CANNOT BE REDDENED BY THE RECORD.
#
# Two halves, and the second one is the reason this file exists at all.
#
# HALF ONE — THE ORDINARY ONE. Forty-one green ticks are evidence of nothing
# until somebody shows them turning red for the right reason. So: take the
# SHIPPED source, remove ONE property at a time, and assert that
#   1. ceo-ruled.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement matching nothing gives a
#      green run that looks like a green run, which is the same trap again).
#
# HALF TWO — THE DRIFT DEMONSTRATION. Section 8 of that suite is the only part
# that touches the real register, and on 2026-09-04 it was RED — not because
# the gate broke, but because case 8a asserted that the refusal cited "row
# 3.14" and open-items.md had retired that row two days earlier in ordinary
# maintenance. The case was rewritten to assert what the gate does rather than
# what the record currently says. THAT REWRITE IS A CLAIM, and a claim about
# not-being-fragile is exactly the kind that decays quietly. So this harness
# renumbers every section of the register, renumbers every row, and shifts
# every line number, then requires the suite to stay GREEN while proving the
# citations it printed genuinely changed underneath it.
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree, and nothing here writes to the live record — the drifted
# register is a copy in a temporary directory.
#
# Run directly: scripts/hooks/ceo-ruled.mutation.sh
# Exit 0 = every property is proven load-bearing AND the drift claim holds.

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
SANDBOX="$(cd "$(mktemp -d -t ceo-ruled-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
# The needles arrive from a shell single-quoted string, so a multi-line target
# is written `a\nb`. Decoded here rather than in bash, where the quoting to
# carry a literal newline through three levels is its own bug. A CONSEQUENCE,
# stated because it bit once: a needle may not contain the two characters
# backslash-n as data.
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

# A throwaway engine subtree carrying everything ceo-ruled.test.sh reads.
plant() { # <dir>
    local dir="$1"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib" "$dir/hooks"
    cp "$ENGINE_ROOT/scripts/hooks/ceo-ruled.test.sh" \
       "$ENGINE_ROOT/scripts/hooks/guard-ceo-ruled-ask.sh" \
       "$ENGINE_ROOT/scripts/hooks/notice-ceo-ruled-prose.sh" \
       "$ENGINE_ROOT/scripts/hooks/contract-integrity.test.sh" \
       "$ENGINE_ROOT/scripts/hooks/install.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/ceo-ruled.sh" "$ENGINE_ROOT/scripts/lib/ceo-ruled.py" \
       "$ENGINE_ROOT/scripts/lib/ceo-asks.sh" "$ENGINE_ROOT/scripts/lib/ceo-asks.py" \
       "$ENGINE_ROOT/scripts/lib/ceo-todos.sh" "$ENGINE_ROOT/scripts/lib/ceo-todos.py" \
       "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" \
       "$ENGINE_ROOT/scripts/lib/premise-ask.sh" "$ENGINE_ROOT/scripts/lib/premise-ask.py" "$dir/scripts/lib/"
    cp "$ENGINE_ROOT/scripts/ceo-ruled-exempt.sh" "$ENGINE_ROOT/scripts/demo.sh" "$dir/scripts/"
    cp "$ENGINE_ROOT/hooks/hooks.json" "$dir/hooks/"
    chmod +x "$dir/scripts/hooks/"*.sh "$dir/scripts/"*.sh
}

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
    plant "$dir"

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (scripts/lib/mutant-suite-run.sh,
    # hunt part 3 finding 9): in this suite a printed FAIL line always ends the run
    # red, and the stop matches the TRIMMED want as a fixed string, exactly what the
    # escaped witness test below matches, so it is never looser than that test.
    mutant_suite_run "$dir/out.txt" "$(printf '%s' "$want" | sed 's/[[:space:]]*$//')" bash "$dir/scripts/hooks/ceo-ruled.test.sh"
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

PY="scripts/lib/ceo-ruled.py"
G="scripts/hooks/guard-ceo-ruled-ask.sh"

echo "=== 1. THE CEO-RULED GATE: every property, proven by removing it ==="

# --- the block itself -------------------------------------------------------
mutant gate-does-not-block "8a" "$G" \
    '} >&2\nexit 2' \
    '} >&2\nexit 0' \
    "the whole gate is the refusal. A hook that exits 0 is a hook that watched it happen."

# --- the matcher's narrowness ----------------------------------------------
# The positive control is the only thing standing between this gate and a wall,
# and section 8 is where it is measured against 1,400 lines rather than sixty.
mutant one-word-of-a-title-fires "8d" "$PY" \
    'if not all(w in qset for w in subj):' \
    'if not any(w in qset for w in subj):' \
    "a gate that refuses a question for sharing ONE word with a title refuses everything, and a gate that refuses everything is waived by lunchtime."

# --- the refusal has to be USABLE ------------------------------------------
# These three are the properties section 8 gained on 2026-09-04 when it stopped
# asserting WHICH row answers. Each one reddens section 8 and nothing else can
# see it, which is the point: the live-record section is not decoration.
mutant locator-points-nowhere "8a" "$PY" \
    'how, anchor, d, t, r.line))' \
    'how, anchor, d, t, 1))' \
    "'open the file at the line above' is the instruction the refusal gives. A line number that is not the ruling's sends him to the top of a 1,400-line file."

mutant title-and-locator-disagree "8a" "$PY" \
    'r.label, r.cite, _flatten(r.title), how, anchor, d, t, r.line))' \
    'r.label, r.cite, _flatten(r.title) + " (as amended)", how, anchor, d, t, r.line))' \
    "a citation whose printed title is not the title on the line it points at is a citation the reader cannot check, and checking is the whole reason it is printed."

mutant citation-carries-no-quote "8a" "$PY" \
    '        for q in r.quotes:' \
    '        for q in []:' \
    "NAMED AND QUOTED, NEVER COUNTED. A bare section number sends him hunting through the register, and hunting is what gets a gate waived."

mutant quote-is-not-the-records "8a" "$PY" \
    '% (r.cite, q))' \
    '% (r.cite, "he settled this one already and everyone remembers it slightly differently"))' \
    "a refusal that paraphrases him instead of quoting him is the laundering that produced 'the seven approved splash screens'."

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
echo "=== 2. THE DRIFT DEMONSTRATION — a maintained record must not redden §8 ==="
# Case 8a died on 2026-09-04 of a content pin. This is the proof the pin is
# gone: renumber every ruling, renumber every row, and push every line down the
# page — the exact shape of the maintenance that killed it — then require the
# suite to be green AND require the citations to have genuinely moved. Green
# over a record that did not actually change would be the corpse this whole
# family of files exists to refuse.

LIVE_DIR="${CEO_RULED_LIVE_DIR:-$SANDBOX/register}"
if [ -z "${CEO_RULED_LIVE_DIR:-}" ]; then
    bash "$ENGINE_ROOT/scripts/hooks/ceo-ruled.test.sh" --export-register "$LIVE_DIR" || exit 1
fi

if [ -z "$LIVE_DIR" ]; then
    echo "  NOTE  the live richos-hq record is not on this machine, so the drift"
    echo "        demonstration did NOT run. This is not a pass."
else
    cat >"$SANDBOX/drift.py" <<'DRIFTPY'
"""Copy the live register and age it the way maintenance ages it.

THREE EDITS, EACH ONE A REAL THING THAT HAPPENED TO THIS RECORD:
  the sections are renumbered   (§19 becomes §119, so every cite string moves)
  the rows are renumbered       (row 3.14 becomes row 3.54 — the 2026-09-02
                                 "eight finished rows leave the page" shape)
  the page grows a preamble     (every line number in both files shifts)
Nothing a ruling SAYS is touched, because a record that stopped carrying a
ruling would be a record that genuinely does not answer the question, and
refusing to refuse then is correct rather than fragile.
"""
import os
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
os.makedirs(dst, exist_ok=True)

PREAMBLE = ("# Preamble added by the drift demonstration\n\n"
            + "".join("Filler line %d, so every ruling below sits on a different "
                      "line than it did.\n" % i for i in range(1, 41))
            + "\n")

changed = 0
for name, rownum in (("ceo-decisions.md", False), ("open-items.md", True)):
    path = os.path.join(src, name)
    if not os.path.isfile(path):
        continue
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    before = text
    text = re.sub(r"(?m)^## (\d+)\.", lambda m: "## %d." % (int(m.group(1)) + 100), text)
    if rownum:
        text = re.sub(r"(?m)^\| (\d+)\.(\d+)([a-z]?) \|",
                      lambda m: "| %s.%d%s |" % (m.group(1), int(m.group(2)) + 40, m.group(3)),
                      text)
    text = PREAMBLE + text
    if text != before:
        changed += 1
    with open(os.path.join(dst, name), "w", encoding="utf-8") as fh:
        fh.write(text)

if changed < 1:
    sys.stderr.write("DRIFT APPLIED NOTHING — the register's shape has changed\n")
    sys.exit(3)
DRIFTPY

    DRIFT="$SANDBOX/drifted-register"
    if ! python3 "$SANDBOX/drift.py" "$LIVE_DIR" "$DRIFT" 2>"$SANDBOX/drift.err"; then
        printf '  FAIL  drift-applies — the drifted copy is identical to the live record\n'
        sed 's/^/          /' "$SANDBOX/drift.err"
        FAIL=$((FAIL + 1))
    else
        # The citations the suite actually printed, before and after. Read out of
        # the suite's own --verbose transcript so this measures the shipped path
        # and not a second copy of the payloads.
        cites() { # <log> -> the cites the three live refusals printed
            # The line ABOVE each "in <file>, line <N>" is the citation, and
            # only inside a `live f1/f2/f3` block — the fixture's own refusals
            # cite a record that by design never moves, and including them
            # would let a constant answer stand in for a changed one.
            awk '/^----- /{blk=($0 ~ /^----- live f[123] -----/)}
                 blk==1 && /^      in / {print prev}
                 {prev=$0}' "$1" \
                | sed 's/^ *//; s/  .*//' | sort -u | tr '\n' ' '
        }
        CEO_RULED_LIVE_DIR="$LIVE_DIR" bash "$ENGINE_ROOT/scripts/hooks/ceo-ruled.test.sh" --verbose \
            >"$SANDBOX/live.log" 2>&1
        LIVE_RC=$?
        CEO_RULED_LIVE_DIR="$DRIFT" bash "$ENGINE_ROOT/scripts/hooks/ceo-ruled.test.sh" --verbose \
            >"$SANDBOX/drift.log" 2>&1
        DRIFT_RC=$?
        LIVE_CITES="$(cites "$SANDBOX/live.log")"
        DRIFT_CITES="$(cites "$SANDBOX/drift.log")"

        if [ "$LIVE_RC" -ne 0 ]; then
            printf '  FAIL  drift-baseline — the suite is not green against the live record to begin with\n'
            grep '  FAIL' "$SANDBOX/live.log" | sed 's/^/          /'
            FAIL=$((FAIL + 1))
        else
            printf '  PASS  drift-baseline — green against the live record, citing: %s\n' "$LIVE_CITES"
            PASS=$((PASS + 1))
        fi

        if [ -z "$LIVE_CITES" ] || [ "$LIVE_CITES" = "$DRIFT_CITES" ]; then
            printf '  FAIL  drift-moved-the-citations — the refusals cite the same rulings before and after,\n'
            printf '          so a green run after the drift proves nothing. live=[%s] drifted=[%s]\n' \
                "$LIVE_CITES" "$DRIFT_CITES"
            FAIL=$((FAIL + 1))
        else
            printf '  PASS  drift-moved-the-citations — %s became %s\n' "$LIVE_CITES" "$DRIFT_CITES"
            PASS=$((PASS + 1))
        fi

        if [ "$DRIFT_RC" -eq 0 ]; then
            printf '  PASS  drift-stays-green — a renumbered, re-paginated register still passes 8a-8d\n'
            PASS=$((PASS + 1))
        else
            printf '  FAIL  drift-stays-green — maintenance of the CEO'"'"'s record turned the suite red again\n'
            grep '  FAIL' "$SANDBOX/drift.log" | sed 's/^/          /'
            FAIL=$((FAIL + 1))
        fi
    fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "  $PASS/$PASS properties proven load-bearing"
    exit 0
fi
echo "  $PASS proven, $FAIL SURVIVED — a surviving mutant is a property nothing checks"
exit 1

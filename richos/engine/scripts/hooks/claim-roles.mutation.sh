#!/usr/bin/env bash
#
# claim-roles.mutation.sh — PROVES THE BARE-ROLE AND STATE-CLAIM ARMS OF THE
# CLAIM GATE CAN FAIL.
#
# The bare-role check was added because a measured rule was proposed and the
# measurement said no. That makes its own tests exactly the kind that pass for
# free: almost every one asserts "it did NOT quietly wave something through",
# and a check that never runs satisfies that perfectly. This session already
# shipped one suite that was green over machinery which did not work at all.
#
# So: take the shipped source, remove ONE property at a time, and assert that
#   1. guard-unresolved-claims.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied.
#
# The STATE-CLAIM arms (2026-09-01) are here for the same reason and one more.
# That gate exists because a report was written from the INTENT of a command
# rather than from the repository, and its two-sided canary is the property
# most easily satisfied by a corpse: a check that refuses everything passes
# "a false claim is refused" perfectly. So the mutations below include one that
# makes the gate refuse a TRUE landing claim, and it has to turn the POSITIVE
# half red — not the negative one.
#
# Run directly: scripts/hooks/claim-roles.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t claim-roles-mutation.XXXXXX)" && pwd -P)"
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
    cp "$ENGINE_ROOT/scripts/hooks/guard-unresolved-claims.sh" \
       "$ENGINE_ROOT/scripts/hooks/guard-unresolved-claims.py" \
       "$ENGINE_ROOT/scripts/hooks/guard-unresolved-claims.test.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" \
       "$ENGINE_ROOT/scripts/lib/agent_roster.py" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" "$dir/scripts/lib/"
    # THE STATE-CLAIM ARM DOES NOT EXIST IN A SANDBOX WITHOUT THESE TWO, and
    # without them eighteen of the twenty mutants below were green for a reason
    # that had nothing to do with their mutation. The gate asks point 14's
    # recorded branch through mega-lander/workspaces.py, loaded by path relative
    # to the guard — absent, `_recorded_integration_refs` returns () and every
    # "integrated" claim ABSTAINS into the silent verdict. The suite's own
    # fixture records that branch through mega-lander/workspaces.sh — absent, the
    # recording is swallowed by its `|| true`. So z1, z1b, z6 and zq1-zq4 were
    # red in EVERY sandbox before a single property was removed: a mutant whose
    # witness is one of those was killed by the missing file, and the two whose
    # witness is a NEGATIVE assertion (z2b, z3b — "a true claim was NOT refused")
    # could never go red, because a gate that refuses nothing satisfies them.
    # Those two are the ones that reported the whole thing.
    mkdir -p "$dir/mega-lander"
    cp "$ENGINE_ROOT/mega-lander/workspaces.py" "$dir/mega-lander/"
    cp "$ENGINE_ROOT/mega-lander/workspaces.sh" "$dir/mega-lander/"
    chmod +x "$dir/scripts/hooks/"*.sh "$dir/mega-lander/workspaces.sh"

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (scripts/lib/mutant-suite-run.sh,
    # hunt part 3 finding 9): in this suite a printed FAIL line always ends the run
    # red, and the stop matches the TRIMMED want as a fixed string, exactly what the
    # escaped witness test below matches, so it is never looser than that test.
    mutant_suite_run "$dir/out.txt" "$(printf '%s' "$want" | sed 's/[[:space:]]*$//')" bash "$dir/scripts/hooks/guard-unresolved-claims.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want"
        return 0
    fi
    if [ "$MUTANT_RC" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    # Literal token with a boundary, not a bare regex: an unescaped `.` matches
    # any character, so `z1.` also matched `FAIL  z1b.` and `z9.` matched
    # anything beginning `z9`. Demonstrated on the sibling harnesses the same
    # night -- a mutant reported the case that killed it and that case could not
    # have failed. The token is trimmed first, because a trailing space in the
    # case name puts the boundary one character late.
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

P="scripts/hooks/guard-unresolved-claims.py"
S="scripts/hooks/guard-unresolved-claims.sh"

echo "=== the bare-role arms, proven load-bearing by removing them ==="

mutant no-role-block "y1." "$P" \
    '            if not ever:\n                undispatched_roles.append((role, span))' \
    '            if False:\n                undispatched_roles.append((role, span))' \
    "the one arm that blocks: a role named that was never dispatched at all."

mutant role-block-not-in-verdict "x." "$P" \
    'or undispatched_roles or bad_states) else "pass",' \
    'or bad_states) else "pass",' \
    "a violation recorded as a pass is a violation nobody will ever count."

mutant no-stale-report "y2." "$P" \
    '            elif live is not None and role not in live:' \
    '            elif False:' \
    "the half that sees the real failure; unmeasurable is not the same as unreported."

mutant liveness-always-true "y2." "$P" \
    '        if str(m.get("status") or "").lower() in TERMINAL_STATUS:\n            continue' \
    '        if False:\n            continue' \
    "a liveness set that never shrinks answers 'ever spawned' again, which is the question that misses it."

mutant live-role-still-reported "y3b." "$P" \
    '            elif live is not None and role not in live:' \
    '            elif live is not None:' \
    "reporting on a teammate that IS running is the noise that gets the whole gate muted."

mutant names-do-not-suppress "y4c." "$P" \
    'if names_evaluable and rclaims and not names and "Agent" not in turn_tools:' \
    'if names_evaluable and rclaims and "Agent" not in turn_tools:' \
    "a message that names an identifier belongs to the name check; two verdicts on one sentence is one too many."

mutant agent-call-does-not-suppress "y5." "$P" \
    'and "Agent" not in turn_tools:' \
    'and True:' \
    "a turn that actually dispatched has nothing to answer for."

mutant case-insensitive-roles "y6." "$P" \
    'rx = re.compile(r"(?<![A-Za-z0-9_-])(%s)(?![A-Za-z0-9_-])\s+%s" % (alt, PROGRESSIVE))' \
    'rx = re.compile(r"(?<![A-Za-z0-9_-])(%s)(?![A-Za-z0-9_-])\s+%s" % (alt, PROGRESSIVE), re.I)' \
    "lowercase matching turns 'the mark is fading' into a dispatch claim."

mutant no-progressive-requirement "y6b." "$P" \
    '% (alt, PROGRESSIVE))' \
    '% (alt, ""))' \
    "naming a teammate would become a claim that they are working right now."

mutant nameless-number-hidden "y7." "$P" \
    'lines.append("  ...and it named no agent identifier (10.3% precision -- also never enforced)")' \
    'pass' \
    "a rejected rule shipped without its number is a rule somebody promotes next month."

# A NOTE, NOT AN ECHO. Mutant output is collected at the drain, so a plain
# `echo` here floats to the top of the report and ends up introducing every
# mutant instead of the eleven below it. mut_pool_note keeps its place in the
# sequence without being counted as a mutant.
mut_pool_note ""
mut_pool_note "=== the state-claim arms, proven load-bearing by removing them ==="

# REPOINTED 2026-09-06. The needle moved when state_verdict's consumer gained an
# `if pol == "positive":` wrapper: this block went from 12-space to 16-space
# indentation, and the mutation stopped applying. The harness refused rather than
# passing, which is right, but until it was repointed the arm that catches a
# merge that never ran -- the 2026-09-01 failure itself -- was proven by nothing.
# The 16-space `if v[0] == "violation":` appears TWICE in the source; it is the
# second line that makes this pair unique, which is why both are in the anchor.
mutant no-state-block "z1." "$P" \
    '                if v[0] == "violation":\n                    bad_states.append((kind, sha, sentence, v[1], v[2]))' \
    '                if False:\n                    bad_states.append((kind, sha, sentence, v[1], v[2]))' \
    "the arm that catches a merge that never ran — the 2026-09-01 failure itself."

mutant state-block-not-in-verdict "x." "$P" \
    'or undispatched_roles or bad_states) else "pass",' \
    'or undispatched_roles) else "pass",' \
    "a refusal recorded as a pass is a refusal nobody will ever count."

mutant no-reachability-requirement "z3b." "$P" \
    '        ref = _reachable_from(root, sha, "refs/heads/")\n        if not ref and kind == "integrated":\n            ref = _reachable_from(root, sha, "refs/remotes/")\n        if ref:\n            return ("violation", root, ref)' \
    '        ref = _reachable_from(root, sha, "refs/heads/") or "(none)"\n        if ref:\n            return ("violation", root, ref)' \
    "without it every pre-rewrite citation fires: 41 of them in the corpus, none a false report."

# REPOINTED 2026-09-13, and the old target is why the sandbox above gained two
# files. This mutant emptied `INTEGRATION_REFS`, which stopped being read when
# the gate started asking point 14's recorded branch: the tuple is now a DEAD
# CONSTANT, deliberately kept — workspace-spec-fourteen.test.sh C14.7 asserts it
# has zero code uses, and that suite's own mutation harness makes
# `_recorded_integration_refs` fall back to it — so emptying it changes nothing
# and this mutant could only ever be a misfire. It reported itself as one
# ("the suite went red, but NOT at z2b"), which is the harness working.
#
# THE PROPERTY IS REAL AND IT MOVED, so the mutant moves with it. What z2b
# guards is that a gate which cannot confirm a landing must not refuse a TRUE
# one, and the single line that decides a landing IS confirmed is the
# `merge-base --is-ancestor` success. Break that and the gate refuses the true
# claim and the false one alike — exit 2 both times, which is precisely the
# corpse that satisfies "a false claim is refused".
mutant landing-never-confirmed "z2b." "$P" \
    '                if q.returncode == 0:\n                    return ("ok",)' \
    '                if False:\n                    return ("ok",)' \
    "THE TWO-SIDED CANARY: a gate that can never confirm a landing refuses a TRUE landing claim too, and exit 2 cannot tell you which one it did."

mutant no-push-check "z4." "$P" \
    '            if _reachable_from(root, sha, "refs/remotes/"):\n                return ("ok",)' \
    '            if True:\n                return ("ok",)' \
    "\"pushed\" is a claim about another machine, and this is the only thing that reads it."

mutant no-bare-hex "z6." "$P" \
    '    for tok in _backticked(sentence) + [m.group(1) for m in\n                                        BARE_HEX_RE.finditer(sentence)]:' \
    '    for tok in _backticked(sentence):' \
    "a SHA written without backticks is the same claim; the corpus carries 19 of them."

# REPOINTED 2026-09-06. `(integrated or published)` was renamed `(mi or mp)` in
# state_claims(), so this target did not merely move -- it ceased to exist. Same
# consequence: the verb requirement was proven by nothing until now.
# THIS MUTANT APPLIES NOW AND IS STILL NOT PROVEN, AND THE REASON IS NOT THIS
# FILE. It is left red deliberately: reaching a green tick here would mean
# naming a witness that does not witness, which is the defect this whole family
# of harnesses exists to refuse.
#
# What the repointed string bought is real -- the harness has stopped saying
# "the mutation did not apply", which was false comfort in the other direction.
# What it revealed is that THE SUITE UNDER IT IS NOT DETERMINISTIC. Five runs of
# byte-identical code on 2026-09-06, minutes apart:
#
#   suite exited 0 (mutant "survived")            3 of 5
#   suite went red, but not at z7.                2 of 5
#   and `no-reachability-requirement`, a
#   completely different mutant, flaked too       1 of 5
#
# So `18 killed / 19 killed` out of 20 is a coin toss, not a measurement, and
# claim-roles.mutation.sh is invoked by contract-integrity.test.sh -- which is
# therefore intermittently red for this reason and not for a defect.
#
# What is known about the mechanism, stated as far as it was actually checked
# and no further. Removing the verb test does not merely widen the check:
# execution then reaches `claim_polarity(s, (mi or mp).start())` with both
# matches None, so the analyzer RAISES on the first verb-less sentence carrying
# a hex token. The guard fails open on an analyzer it cannot run, so every case
# asserting only an exit code sees 0 and stays green -- including `z7.`, which
# was written to be this property's witness and asserts nothing but the code.
# The cases that DO notice are the ones asserting a message or a reachability
# verdict, and those are exactly the two that flake.
#
# THE FIX WAS THE SECOND HALF OF THAT SENTENCE, AND IT IS DONE (2026-09-06).
# `z7.` now asserts the absence of an analyzer failure and not only an exit
# code: guard-unresolved-claims.test.sh's run_case gained a FORBIDDEN-needle
# argument, and z7 forbids "Traceback (most recent call last)" on stderr.
#
# Measured before writing it, on z7's own payload at 934f127:
#     clean analyzer    exit 0, stderr EMPTY
#     mutated analyzer  exit 1, "AttributeError: 'NoneType' object has no
#                       attribute 'start'" on stderr
# The wrapper turns that exit 1 into exit 0 because it fails open on an
# analyzer it cannot run, which is exactly why an exit-code-only case could
# never see it. The traceback is deterministic on both hosts, so z7 is now a
# witness that witnesses.
#
# THE FIRST HALF OF THAT SENTENCE IS STILL OPEN, and this file should not be
# read as saying otherwise: the reachability cases still depend on state that
# moves under them, and the header above records `no-reachability-requirement`
# flaking 1 run in 5 for that reason. What changed is that
# `state-claim-needs-no-verb` no longer depends on those cases to be caught.
mutant state-claim-needs-no-verb "z7." "$P" \
    '        if not (mi or mp):\n            continue' \
    '        if False:\n            continue' \
    "without the verb this stops being a state check and becomes an ancestry test on every hex token in the reply."

mutant no-absence-report "z9." "$P" \
    '            for v, sp, fp, _root in absence_reports:' \
    '            for v, sp, fp, _root in []:' \
    "the one fact the 2026-09-01 turn did not have: which spelling was still there."

mutant absence-blocks-instead-of-reporting "z9." "$P" \
    '            or bad_states):' \
    '            or bad_states or absence_reports):' \
    "promoting a 54%-fire signal to a blocker is how a defense becomes a formality; the report must stay a report."

mutant only-the-named-spelling "z9." "$P" \
    '    out = ["#" + h, "#" + h.upper()]' \
    '    return ["#" + h]\n    out = ["#" + h, "#" + h.upper()]' \
    "checking only the spelling the claim named is exactly the mistake being caught."

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
echo "  $PASS mutant(s) killed, $FAIL survived or misfired"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

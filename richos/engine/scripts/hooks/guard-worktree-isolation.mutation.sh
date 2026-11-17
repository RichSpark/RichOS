#!/usr/bin/env bash
# Mutation harness for guard-worktree-isolation.sh, CLAUSE 5 (the staffing gate
# — M1-M11) and CLAUSE 6 (the model-tier gate added the same evening — M12-M19;
# see the block below its own header for why the INCIDENT is a mutant here).
#
# Clause 5 (the staffing gate
# added 2026-09-02 after an engine-wide audit was dispatched to `Explore`, a
# generic built-in, because a roster teammate would have needed a worktree
# created first).
#
# WHY THIS FILE EXISTS AT ALL. Clause 5's suite is 37 green cases. Green is not
# evidence: on 2026-09-02 five separate checks in this engine were found green
# over code that never ran, including a mutation harness that killed 11 of 18
# mutants because its sandboxes lacked a dependency, so the guard REFUSED TO
# START and that read exactly like a guard catching the mutation. Each mutant
# below therefore asserts THREE things, not one:
#   (a) the mutation actually applied     -- a sed that matched nothing gives a
#       green run that looks like a green run;
#   (b) the guard STILL STARTS AND RUNS   -- a control payload (a well-formed
#       isolated roster spawn, which no clause-5 mutant should touch) must still
#       exit 0. A guard that refuses to start exits 2 on everything and would
#       "catch" every mutation while proving nothing;
#   (c) the suite goes red AT THE NAMED CASES, not merely somewhere.
# EVERY MUTANT RUNS IN A THROWAWAY COPY OF THE WHOLE ENGINE. This line used to
# read "there is no sandbox copy anywhere in this harness: every mutant runs
# the REAL file from the REAL engine root, so the missing-dependency trap
# cannot recur." That was a real trap and the wrong cure, and it cost more than
# it bought:
#
#   On 2026-09-05 an engineer working in this repository watched this harness's
#   `-gt` -> `-lt` mutant (M13, the INCIDENT) sitting in the SHIPPED
#   guard-worktree-isolation.sh in a live tree — the spawn gate enforcing the
#   exact inverse of the CEO's model ceiling, refusing upgrades and waving
#   downgrades through. This run finished, so `trap ... EXIT` put it back.
#
# AN `EXIT` TRAP IS A PROMISE CONDITIONAL ON EXITING. `kill -9`, an OOM kill, a
# closed terminal or a power cut between the two writes leaves the operator's
# live enforcement inverted with nothing to say so, and `~/.claude/richos-
# engine` is a symlink to the main checkout. This harness is also invoked by
# contract-integrity.test.sh, so that window was open on every CI verify, not
# only when somebody ran it by hand.
#
# THE MISSING-DEPENDENCY TRAP IS ANSWERED THREE WAYS, none of them a promise:
#   1. the sandbox is the whole mechanical layer (scripts/, hooks/,
#      orchestration.config, .claude/), not one file — see
#      scripts/lib/mutation-harness.sh, whose header carries the full account;
#   2. `alive` below is unchanged, and it is the arm that tells "caught the
#      mutation" apart from "could not start";
#   3. M0 runs the whole suite against the UNMUTATED sandbox and REFUSES TO
#      PROCEED unless it is green — so a deficient sandbox is a loud failure at
#      case zero rather than 21 mutants all scoring PROVEN for the wrong
#      reason. That is the measurement the old sentence asserted instead.
# And M99 witnesses the shipped guard's checksum AND mtime across the whole
# run: not "restored correctly" but NEVER OPENED FOR WRITING.
#
# BOTH DIRECTIONS ARE MUTATED. Under-blocking is the obvious one (M1-M4, M8,
# M9, M10). Over-blocking has its own mutants (M5, M6, M7) because the cheapest
# way to make any guard "pass" is to make it refuse everything, and the cheapest
# way to kill a false positive is to disable the check -- an over-broad staffing
# gate that fires on a statusline change is how a defense becomes a nuisance and
# then a formality typed by reflex.
#
# ONE FINDING FROM THIS HARNESS'S OWN FIRST RUN, kept here because it is the
# whole argument for writing it: M9 originally mutated the `: "${GENERIC_AGENT_
# TYPES:=general-purpose}"` DEFAULT and the suite stayed green. The guard sources
# the entity's orchestration.config BEFORE that line and the config sets the
# variable explicitly, so the default is dead code in any adopted repository.
# The mutant had proven nothing and looked like it had proven the opposite.
# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SRC_ENG="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$SRC_ENG/scripts/lib/mutation-harness.sh"
# scratch_new is used by this harness's own mutant loop, which does not go
# through mutation_begin, so the allocator is sourced explicitly rather than
# inherited.
# shellcheck source=../lib/scratch.sh
. "$SRC_ENG/scripts/lib/scratch.sh"
# shellcheck source=../lib/tree-witness.sh
. "$SRC_ENG/scripts/lib/tree-witness.sh"

# THE SHIPPED FILE, WITNESSED BUT NEVER WRITTEN. Checked again at M99.
SHIPPED_GUARD="$SRC_ENG/scripts/hooks/guard-worktree-isolation.sh"
tw_pick_mtime
SHIPPED_BEFORE="$(tw_file_witness "$SHIPPED_GUARD")"

# THE ONLY TREE THIS HARNESS WRITES TO. A `kill -9` anywhere below destroys a
# directory under TMPDIR and nothing else; there is no window in which the
# operator's enforcement is modified, because it is never opened for writing.
mutation_sandbox_engine "$SRC_ENG"
ENG="$MUT_SANDBOX_ENGINE"
GUARD="$ENG/scripts/hooks/guard-worktree-isolation.sh"
SUITE="$ENG/scripts/hooks/guard-worktree-isolation.test.sh"
# THE PRISTINE REFERENCE SANDBOX, NEVER MUTATED. It exists for two jobs only:
# the M0 baseline, scoped positive controls and the final unchanged-guard check.
# Each mutant builds its OWN copy -- see _mut_worker_sandbox.
#
# `restore` USED TO PUT THIS ONE BACK BETWEEN MUTANTS, and that is exactly what
# made the harness unsafe to run concurrently: two mutants would have written the
# same file, and the second would have measured the first one's mutation. It is
# kept here as a LOUD FAILURE rather than deleted, so a mutant block that still
# calls it stops the run instead of quietly sharing a sandbox again.
restore() {
    echo "ERROR: restore() was called. This harness no longer shares one sandbox" >&2
    echo "       between mutants -- each builds its own. A call here means a mutant" >&2
    echo "       block was not converted, and it would be mutating the reference" >&2
    echo "       sandbox that M0 and the final md5 check depend on." >&2
    exit 2
}

# shellcheck source=../lib/stopwatch.sh
. "$SRC_ENG/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$SRC_ENG/scripts/lib/mutation-pool.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

# _mut_worker_sandbox — a throwaway engine copy for ONE mutant.
#
# It ASSIGNS to the caller's locals (bash is dynamically scoped), so each mutant
# function declares `local ENG GUARD SUITE W_DIR MUT_SCORE` and everything the
# block already says about "$GUARD" and "$SUITE" keeps working unchanged against
# the worker's own files. RICHOS_WORKSPACES_DIR is exported per worker, which is
# safe because a pool worker is its own subshell.
_mut_worker_sandbox() {
    # ALLOCATED, NOT NAMED — see scripts/lib/scratch.sh. `mktemp -d -t
    # mutant-sandbox.XXXXXX` named a sandbox the sweeper could only find if
    # somebody had thought to declare the name, and on 2026-09-17 a sibling
    # harness left 105.3 GB behind exactly that way.
    W_DIR="$(scratch_new mutant-sandbox)" || return 1
    ENG="$W_DIR/engine"
    mkdir -p "$ENG"
    if ! mutation_copy_engine "$ENG" "$SRC_ENG"; then
        echo "UNPROVEN  ??   <- could not build this mutant's sandbox" >&2
        rm -rf "$W_DIR"
        return 1
    fi
    GUARD="$ENG/scripts/hooks/guard-worktree-isolation.sh"
    SUITE="$ENG/scripts/hooks/guard-worktree-isolation.test.sh"
    RICHOS_WORKSPACES_DIR="$W_DIR/workspaces"
    export RICHOS_WORKSPACES_DIR
    MUT_SCORE=unproven
    return 0
}

# _mut_score — the verdict as an EXIT CODE, because a pool worker is a subshell
# and an incremented counter in there is discarded at the closing paren.
_mut_score() {
    rm -rf "$W_DIR"
    [ "$MUT_SCORE" = proven ]
}
# Cleanup is now a tidiness measure, not a correctness one. That is the point:
# if this trap never runs, nothing is broken — previously, if it never ran, the
# spawn gate stayed inverted.
# Clause 7 REGISTERS every allowed file-capable spawn in the workspace registry
# (docs/plans/worktree-spec-2026-09-11.md, point 3); the control payload carries
# a tool_use_id as a real one does, the registry is pinned inside the sandbox so
# this harness never touches the operator's record, and the session is a
# process of this harness's own so no registration borrows a real session's
# identity.
export RICHOS_WORKSPACES_DIR="$MUT_SANDBOX_DIR/workspaces"
RICHOS_SESSION_PID="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
export RICHOS_SESSION_PID
trap 'kill "$RICHOS_SESSION_PID" 2>/dev/null; rm -rf "$MUT_SANDBOX_DIR"' EXIT

PROVEN=0; UNPROVEN=0
BASE_MD5="$(md5 -q "$GUARD" 2>/dev/null || md5sum "$GUARD" | cut -d' ' -f1)"

# applied <id> <desc> — returns 1 (and scores UNPROVEN) if the file is unchanged.
applied() {
    local id="$1" desc="$2" after
    after="$(md5 -q "$GUARD" 2>/dev/null || md5sum "$GUARD" | cut -d' ' -f1)"
    if [ "$after" = "$BASE_MD5" ]; then
        printf 'UNPROVEN  %-4s %s  <- MUTATION DID NOT APPLY\n' "$id" "$desc"
        MUT_SCORE=unproven; return 1
    fi
    return 0
}

# alive <id> <desc> — the mutated guard must still START and run a control
# payload to a normal verdict. This is the arm that would have caught the
# 11-of-18 incident: a guard that cannot start exits 2 on everything.
alive() {
    local id="$1" desc="$2" rc
    if ! bash -n "$GUARD" >/dev/null 2>&1; then
        printf 'UNPROVEN  %-4s %s  <- mutant does not PARSE; a syntax error is not a mutation\n' "$id" "$desc"
        MUT_SCORE=unproven; return 1
    fi
    printf '{"tool_name":"Agent","session_id":"mut00000-0000-4000-8000-000000000000","tool_use_id":"toolu_mut_control","tool_input":{"subagent_type":"dev","name":"dev-sonnet-high-alive","effort":"high","isolation":"worktree","prompt":"control payload"}}' \
        | RICHOS_ENTITY_ROOT="$ENG" "$GUARD" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        printf 'UNPROVEN  %-4s %s  <- mutant guard REFUSED THE CONTROL SPAWN (rc=%s): it is not running, it is dying\n' "$id" "$desc" "$rc"
        MUT_SCORE=unproven; return 1
    fi
    return 0
}

# check <id> <desc> <expected-red-case-regexes...>
check() {
    local id="$1" desc="$2"; shift 2
    local out rc missing="" want prered="" grc fault=""
    # BASELINE SUBTRACTION — see M0. A case that was ALREADY red before any
    # mutation cannot be turned red BY a mutation, so a mutant naming it is
    # scoring off somebody else's failure. Without this, an unrelated
    # regression elsewhere in the suite silently converts every mutant that
    # names an affected case into a PROVEN that proved nothing — this
    # engine's recurring defect, wearing this harness's own output format.
    for want in "$@"; do
        grep -qE "^${want}" "$BASELINE_FAILS" 2>/dev/null && prered="$prered ${want}"
    done
    if [ -n "$prered" ]; then
        printf 'UNPROVEN  %-4s %s  <- case(s) ALREADY RED before any mutation, so this mutant proves nothing about them:%s\n' \
            "$id" "$desc" "$prered"
        MUT_SCORE=unproven; return
    fi
    # A pruned fixture must pass against pristine source before its mutant
    # can score. The normal M0 above still exercises every outer assertion.
    out="$(RICHOS_MUTATION_INNER=1 "$MUT_SANDBOX_ENGINE/scripts/hooks/guard-worktree-isolation.test.sh" --mutation-case "$id" "$@" 2>&1)"; rc=$?
    if [ "$rc" -ne 3 ]; then
        printf 'UNPROVEN  %-4s %s  <- scoped positive control failed (rc=%s)\n%s\n' "$id" "$desc" "$rc" "$out"
        MUT_SCORE=unproven; return
    fi
    out="$(RICHOS_MUTATION_INNER=1 "$SUITE" --mutation-case "$id" "$@" 2>&1)"; rc=$?
    if [ "$rc" -ne 1 ]; then
        printf 'UNPROVEN  %-4s %s  <- witness did not report assertion failure (rc=%s)\n%s\n' "$id" "$desc" "$rc" "$out"
        MUT_SCORE=unproven; return
    fi
    # CASE DETECTION — deliberately NOT `printf '%s' "$out" | grep -q ...`, and
    # deliberately not `|| missing=`. Both halves of that idiom are defects and
    # they compound; this harness and guard-worktree-removal.mutation.sh were
    # the only two files in the engine that carried it, and they are the only
    # two that ever produced the intermittent red.
    #
    # (1) THE PIPE. `grep -q` exits the instant it matches and closes the pipe.
    #     `$out` is ~11 KB, which bash's printf writes in several stdio-sized
    #     write(2)s, so when grep wins the race the NEXT write gets EPIPE and
    #     printf dies of SIGPIPE with 141. `set -o pipefail` is on, so the
    #     PIPELINE reports 141 — nonzero — and `|| missing=` fired on a case
    #     grep had just FOUND. That is the whole of the "red but NOT at: <case>"
    #     flake escalated on 2026-09-13 (WTI1) and again on 2026-09-14 (WTR1).
    #     It contradicted itself in one report because the DUMP below greps
    #     WITHOUT `-q`: that grep reads to EOF, never closes the pipe early,
    #     never SIGPIPEs the writer, and so faithfully printed the very case
    #     detection had just lost.
    #     Measured on ubuntu:24.04 at the real 11 KB size, match on line 1:
    #     1/300 false misses idle, 9/300 under 48-way CPU oversubscription, all
    #     rc=141; 0/300 with the match at the END of the output. That
    #     match-POSITION dependence, not output size, is why clean iteration
    #     counts did not clear it. A here-string puts no second process in the
    #     pipeline, so pipefail has nothing to misreport.
    #
    #     AND THE EXIT CODE IS NOT STABLE ACROSS PLATFORMS, which decides which
    #     half of this fix is load-bearing. On Linux the writer is KILLED by
    #     SIGPIPE and the pipeline reports 141. On macOS bash reports the same
    #     EPIPE as an ordinary stdio write error and the pipeline reports 1 —
    #     and 1 is EXACTLY what grep returns for a genuine no-match. So on that
    #     platform no amount of exit-code triage could have told a lost case
    #     from an absent one. Removing the pipe is the half that fixes this;
    #     the triage in (2) is the half that stops the NEXT transport failure
    #     being laundered into a verdict.
    #
    # (2) THE CONFLATION. `|| missing=` treats EVERY nonzero as "the case is not
    #     in the output", turning a transport failure into a verdict about a
    #     mutant. With the code triaged, "red but NOT at" can only be printed
    #     when grep POSITIVELY returned 1 — no match — so the self-contradicting
    #     report is unreachable rather than merely unlikely.
    for want in "$@"; do
        grep -qE "^  FAIL  ${want}" <<<"$out"; grc=$?
        case "$grc" in
            0) ;;
            1) missing="$missing ${want}" ;;
            *) fault="$fault ${want}(grep rc=$grc)" ;;
        esac
    done
    if [ -n "$fault" ]; then
        printf 'UNPROVEN  %-4s %s  <- HARNESS FAULT: case detection could not run for:%s\n' \
            "$id" "$desc" "$fault"
        MUT_SCORE=unproven; return
    fi
    if [ -z "$missing" ]; then
        printf 'PROVEN    %-4s %s\n' "$id" "$desc"
        MUT_SCORE=proven
    else
        printf 'UNPROVEN  %-4s %s  <- red but NOT at:%s\n' "$id" "$desc" "$missing"
        printf '%s\n' "$out" | grep '^  FAIL' | sed 's/^/            /'
        MUT_SCORE=unproven
    fi
}

echo "=== guard-worktree-isolation CLAUSE 5 + CLAUSE 6 mutation harness ==="

# --- M0 (THE BASELINE, and the reason the sandbox is allowed to exist at all).
# Every mutant below concludes "the suite went red at the named case, so the
# property is load-bearing". That inference needs the named case to have been
# GREEN first, and nothing here used to check it. Two different failures hide
# in that gap:
#   - a sandbox missing a dependency makes the suite red for reasons unrelated
#     to any mutation (the 2026-09-02 incident in this harness's header), and
#   - a pre-existing regression ANYWHERE in this suite does the same thing
#     without a sandbox being involved at all.
# The second is not hypothetical: measured 2026-09-05 at 978a7d1 on the
# pristine main checkout, this suite exits 1 because the harness it invokes at
# its end (worktree-spawn-intent.mutation.sh) scores 3 of 7 mutants unproven.
# That failure predates this change and belongs to clause 7, not here.
#
# SO THE BASELINE IS RECORDED RATHER THAN REQUIRED. Aborting on a red baseline
# would report somebody else's regression as this harness being broken, which
# is a false accusation and gets the harness disabled. Instead `check` refuses
# to score any mutant whose named case is already in this set. A mutant can
# then only ever be PROVEN by a case IT turned red.
#
# This is not the same check as `alive`: `alive` proves the mutated GUARD still
# starts, M0 measures what the unmutated SUITE already says.
BASELINE_FAILS="$MUT_SANDBOX_DIR/baseline-fails.txt"
M0_OUT="$("$SUITE" 2>&1)"; M0_RC=$?
printf '%s\n' "$M0_OUT" | grep '^  FAIL  ' | sed 's/^  FAIL  //' > "$BASELINE_FAILS"
if [ "$M0_RC" -eq 0 ]; then
    printf 'PROVEN    %-4s %s\n' "M0" "the UNMUTATED sandbox suite is GREEN — every red below is a mutation's doing, not the sandbox's"
    printf '%s\n' "$M0_OUT" | tail -n 1
    PROVEN=$((PROVEN+1))
else
    printf 'NOTE      %-4s the UNMUTATED sandbox suite is already RED (rc=%s) at %s case(s) BEFORE any mutation.\n' \
        "M0" "$M0_RC" "$(grep -c . "$BASELINE_FAILS")"
    printf '               No mutant below may score itself on these; each is named here so the reader knows\n'
    printf '               this harness is not the thing that broke them:\n'
    sed 's/^/                 /' "$BASELINE_FAILS"
fi

# --- M1: the whole staffing gate deleted — the state the engine shipped in on
# the morning of 2026-09-02, when READONLY_ALLOWLIST's isolation exemption was
# read as a staffing permission and nothing refused the Explore dispatch.
_mutant_M1() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
start = s.index("NEEDS_STAFFING_HATCH=0\n")
end = s.index("# Read-only agent types are exempt from the teammate contract")
open(p, "w", encoding="utf-8").write(s[:start] + s[end:])
PY
if applied M1 "the staffing gate deleted entirely (the pre-fix engine)" \
   && alive M1 "the staffing gate deleted entirely (the pre-fix engine)"; then
    check M1 "the staffing gate deleted entirely (the pre-fix engine)" \
        "Explore with NO generic-agent: line -> BLOCKED" \
        "Plan with NO generic-agent: line -> BLOCKED" \
        "general-purpose, isolated \+ well-named, NO hatch -> BLOCKED" \
        "undeclared allowlist type waved through"
fi
    _mut_score
}
mut_pool_submit M1 _mutant_M1

# --- M2: the hatch is required, but ANY non-empty text satisfies it — the
# "escape hatch degrades into a formality" failure. A bare marker is still
# empty, so this mutant is caught only by the quality cases.
_mutant_M2() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
s = s.replace("MIN_CHARS = 30", "MIN_CHARS = 0", 1)
s = s.replace("MIN_WORDS = 5", "MIN_WORDS = 0", 1)
s = s.replace("MIN_CONTENT = 3", "MIN_CONTENT = 0", 1)
assert "\n    elif speed:\n" in s, "speed arm anchor not found"
s = s.replace("\n    elif speed:\n", "\n    elif False:\n", 1)
open(p, "w", encoding="utf-8").write(s)
PY
if applied M2 "any non-empty reason accepted (hatch as formality)" \
   && alive M2 "any non-empty reason accepted (hatch as formality)"; then
    check M2 "any non-empty reason accepted (hatch as formality)" \
        "'generic-agent: because' -> BLOCKED" \
        "'generic-agent: n/a' -> BLOCKED" \
        "'generic-agent: -' -> BLOCKED" \
        "a long-but-empty reason \(stopwords only\) -> BLOCKED" \
        "reason 'faster to dispatch' -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M2 _mutant_M2

# --- M3: the SPEED/CONVENIENCE arm alone removed. This is the incident's own
# rationale ("it let him dispatch immediately"), so it gets a mutant to itself
# rather than sharing M2's.
_mutant_M3() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
assert "\n    elif speed:\n" in s, "speed arm anchor not found"
open(p, "w", encoding="utf-8").write(s.replace("\n    elif speed:\n", "\n    elif False:\n", 1))
PY
if applied M3 "the speed/convenience refusal removed" \
   && alive M3 "the speed/convenience refusal removed"; then
    check M3 "the speed/convenience refusal removed" \
        "reason 'faster to dispatch' -> BLOCKED" \
        "reason 'saves time' -> BLOCKED" \
        "reason 'more convenient' -> BLOCKED" \
        "the speed refusal names create-teammate-worktree.sh as the answer"
fi
    _mut_score
}
mut_pool_submit M3 _mutant_M3

# --- M4: the substance (content-word) floor alone removed, so a long string of
# stopwords passes as a justification.
_mutant_M4() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
s2 = s.replace("MIN_CONTENT = 3", "MIN_CONTENT = 0", 1)
assert s2 != s, "MIN_CONTENT anchor not found"
open(p, "w", encoding="utf-8").write(s2)
PY
if applied M4 "the substantive-word floor removed (filler passes)" \
   && alive M4 "the substantive-word floor removed (filler passes)"; then
    check M4 "the substantive-word floor removed (filler passes)" \
        "a long-but-empty reason \(stopwords only\) -> BLOCKED" \
        "the filler refusal says so in those terms"
fi
    _mut_score
}
mut_pool_submit M4 _mutant_M4

# --- M5 (OVER-BLOCKING): the harness-utility exemption removed, so a
# statusline change now demands a staffing justification. A defense that fires
# on harness configuration becomes a nuisance, then a formality, then noise.
_mutant_M5() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'if ! _type_in_set "$SUBAGENT_TYPE" "$HARNESS_UTILITY_TYPES"; then'
assert old in s, "harness-utility anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, "if true; then", 1))
PY
if applied M5 "harness utilities lose their exemption (over-blocking)" \
   && alive M5 "harness utilities lose their exemption (over-blocking)"; then
    check M5 "harness utilities lose their exemption (over-blocking)" \
        "statusline-setup needs NO hatch \(harness utility\)" \
        "claude-code-guide needs NO hatch \(harness utility\)" \
        "read-only type claude-code-guide" \
        "read-only type statusline-setup"
fi
    _mut_score
}
mut_pool_submit M5 _mutant_M5

# --- M6 (INDEPENDENCE): the staffing gate is allowed to swallow the ISOLATION
# exemption — an allowlisted type that PASSES clause 5 is now also required to
# be isolated. The two properties must hold independently; this mutant is the
# only thing that proves the suite would notice if they were collapsed.
_mutant_M6() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = """for a in $READONLY_ALLOWLIST; do
  if [ "$SUBAGENT_TYPE" = "$a" ]; then
    RO_RC=0"""
assert old in s, "readonly early-exit anchor not found"
new = """for a in $READONLY_ALLOWLIST; do
  if [ "$SUBAGENT_TYPE" = "$a" ] && [ "$ISOLATION" = "worktree" ]; then
    RO_RC=0"""
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
if applied M6 "the isolation exemption folded into the staffing gate" \
   && alive M6 "the isolation exemption folded into the staffing gate"; then
    check M6 "the isolation exemption folded into the staffing gate" \
        "read-only type Explore, no isolation/name \(isolation exemption holds\)" \
        "read-only type Plan, no isolation/name \(isolation exemption holds\)" \
        "Explore WITH a well-formed generic-agent: reason -> allowed"
fi
    _mut_score
}
mut_pool_submit M6 _mutant_M6

# --- M7 (OVER-BLOCKING): the staffing gate fires for EVERY subagent_type, so
# roster teammates are taxed too. "Make it stricter until it cannot be wrong"
# is the other way to kill a guard, and it is the way that gets it deleted.
_mutant_M7() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = """  if _type_in_set "$SUBAGENT_TYPE" "$READONLY_ALLOWLIST" \\
     || _type_in_set "$SUBAGENT_TYPE" "$GENERIC_AGENT_TYPES"; then
    NEEDS_STAFFING_HATCH=1
  fi"""
assert old in s, "membership anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, "  NEEDS_STAFFING_HATCH=1", 1))
PY
if applied M7 "the gate fires for every type, roster included (over-blocking)"; then
    # NOTE: `alive` is deliberately NOT called here — this mutant's whole point
    # is that the control roster spawn stops passing, which is exactly what
    # `alive` refuses. The suite check below is the evidence instead.
    check M7 "the gate fires for every type, roster included (over-blocking)" \
        "roster-style type, isolated, no hatch -> allowed \(clause 5 inert\)" \
        "roster-style type, isolated, no hatch, second name -> allowed" \
        "a roster-type refusal is the ISOLATION message, never the staffing one"
fi
    _mut_score
}
mut_pool_submit M7 _mutant_M7

# --- M8: the accepted hatch stops being logged. A waiver nobody can count is a
# waiver that becomes a habit invisibly.
_mutant_M8() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = '>>"$GA_LOG_DIR/generic-agent-dispatches.log" 2>/dev/null || true'
assert old in s, "log anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, ">/dev/null 2>&1 || true", 1))
PY
if applied M8 "the accepted-hatch log removed" \
   && alive M8 "the accepted-hatch log removed"; then
    check M8 "the accepted-hatch log removed" \
        "accepted generic-agent: hatch was NOT logged"
fi
    _mut_score
}
mut_pool_submit M8 _mutant_M8

# --- M9: the GENERIC_AGENT_TYPES ARM removed from the membership test,
# reopening the obvious detour — `general-purpose` is file-capable and NOT on
# the read-only allowlist, so before clause 5 it passed the whole contract on
# isolation and a name alone. Mutating the `:=` default instead proves nothing
# (see this file's header).
_mutant_M9() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = """  if _type_in_set "$SUBAGENT_TYPE" "$READONLY_ALLOWLIST" \\
     || _type_in_set "$SUBAGENT_TYPE" "$GENERIC_AGENT_TYPES"; then"""
assert old in s, "GENERIC_AGENT_TYPES membership anchor not found"
new = '  if _type_in_set "$SUBAGENT_TYPE" "$READONLY_ALLOWLIST"; then'
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
if applied M9 "the general-purpose detour reopened" \
   && alive M9 "the general-purpose detour reopened"; then
    check M9 "the general-purpose detour reopened" \
        "general-purpose, isolated \+ well-named, NO hatch -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M9 _mutant_M9

# --- M10: the refusal stops NAMING THE ALTERNATIVE. It still blocks, so every
# exit-code case stays green; only the message cases notice. A refusal that
# says only "no" gets routed around instead of obeyed.
# ("create-teammate-worktree.sh" is deliberately NOT asserted here: it also
# appears in the speed refusal's own REASON text, which this mutant leaves
# intact, so that assertion is M3's.)
_mutant_M10() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
start = s.index('      echo "  WHY THIS IS REFUSED, not merely discouraged:"')
end = s.index('      echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"')
open(p, "w", encoding="utf-8").write(s[:start] + '      echo "  Refused."\n' + s[end:])
PY
if applied M10 "the refusal no longer names the alternative" \
   && alive M10 "the refusal no longer names the alternative"; then
    check M10 "the refusal no longer names the alternative" \
        "refusal names the roster teammate as the fix" \
        "refusal says a generic agent is invisible in the team display" \
        "refusal says a generic agent leaves no commit" \
        "refusal names the hatch line by its exact shape" \
        "refusal separates the isolation exemption from the staffing question"
fi
    _mut_score
}
mut_pool_submit M10 _mutant_M10

# --- M11 (OVER-BLOCKING, the speed check's OWN false-positive arm): the
# speed/convenience pattern is widened to catch a bare mention of "worktree".
# That is the tempting version -- the incident's reason was "a roster teammate
# would have needed a worktree set up first" -- and it is wrong, because a
# genuine justification may name worktrees innocently ("no worktree is needed
# for a read-only sweep"). Refusing that is a false positive on a TRUE reason,
# which is how an escape hatch stops being usable and starts being lied to.
_mutant_M11() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'r"(fastest|faster|quickest|quicker|save[sd]?\\s+time|saving\\s+time|"'
assert old in s, "speed pattern anchor not found"
new = 'r"(worktree|fastest|faster|quickest|quicker|save[sd]?\\s+time|saving\\s+time|"'
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
if applied M11 "the speed pattern widened to catch any mention of worktrees" \
   && alive M11 "the speed pattern widened to catch any mention of worktrees"; then
    check M11 "the speed pattern widened to catch any mention of worktrees" \
        "a genuine reason mentioning worktrees is NOT read as a speed excuse"
fi
    _mut_score
}
mut_pool_submit M11 _mutant_M11

# ===========================================================================
# CLAUSE 6 — the model-tier gate. THE INCIDENT IS A MUTANT (M13, M18).
# ===========================================================================
# On 2026-09-02 the orchestrator inferred a capability order from alias names,
# read Sonnet -> Fable as a downgrade (it is an upgrade), killed a correctly
# configured teammate on that belief, and commissioned a guard whose fixtures
# would have refused that shape forever. The suite's (n) block pins the
# correct verdicts; these mutants prove those pins are load-bearing by
# planting tonight's exact errors in the shipped guard and watching the suite
# go red at the named cases. Both directions again: under-blocking (M12, M15,
# M16), the inversion itself (M13), over-blocking (M14, M17), inference from
# names (M18), and a silenced fail-open (M19).

# --- M12: clause 6 deleted entirely — the engine as it shipped before tonight,
# when "don't downgrade" was a sentence and nothing read it.
_mutant_M12() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
start = s.index('C6_OVERRIDE_LC="$(printf')
end_marker = '[ -z "$C6_SKIP" ] || _c6_announce_skip "$C6_SKIP"\nfi\n'
end = s.index(end_marker) + len(end_marker)
open(p, "w", encoding="utf-8").write(s[:start] + s[end:])
PY
if applied M12 "clause 6 deleted entirely (the pre-fix engine)" \
   && alive M12 "clause 6 deleted entirely (the pre-fix engine)"; then
    check M12 "clause 6 deleted entirely (the pre-fix engine)" \
        "frank \(opus default\) on sonnet, no reason -> BLOCKED" \
        "frank on haiku, no reason -> BLOCKED" \
        "reed \(sonnet default\) on haiku, no reason -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M12 _mutant_M12

# --- M13: THE INCIDENT — the comparison inverted. A move UP the order is now
# refused and a move DOWN is waved through. This is exactly what the killed
# version of this task would have shipped; the (n) block must go red at the
# Sonnet -> Fable case that its fixtures had backwards.
_mutant_M13() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'elif [ "$C6_RANK_REQUESTED" -gt "$C6_RANK_DEFAULT" ]; then'
assert old in s, "comparison anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, 'elif [ "$C6_RANK_REQUESTED" -lt "$C6_RANK_DEFAULT" ]; then', 1))
PY
if applied M13 "THE INCIDENT: the comparison inverted (upgrade refused, downgrade waved)" \
   && alive M13 "THE INCIDENT: the comparison inverted (upgrade refused, downgrade waved)"; then
    check M13 "THE INCIDENT: the comparison inverted (upgrade refused, downgrade waved)" \
        "reed \(sonnet default\) on fable, no reason -> allowed, SILENT \(an upgrade\)" \
        "reed upgraded to opus -> SILENT" \
        "frank \(opus default\) on sonnet, no reason -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M13 _mutant_M13

# --- M14 (OVER-BLOCKING): same tier taxed. -gt becomes -ge, so an Opus-default
# teammate spawned on Fable — the spawn the CEO ordered himself — now demands a
# justification. A guard that refuses work the CEO explicitly ordered is a
# false positive, and the suite must measure it rather than assume it.
_mutant_M14() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'elif [ "$C6_RANK_REQUESTED" -gt "$C6_RANK_DEFAULT" ]; then'
assert old in s, "comparison anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, 'elif [ "$C6_RANK_REQUESTED" -ge "$C6_RANK_DEFAULT" ]; then', 1))
PY
if applied M14 "same tier taxed (over-blocking; the CEO's own Fable spawns refused)" \
   && alive M14 "same tier taxed (over-blocking; the CEO's own Fable spawns refused)"; then
    # The "frank (opus default) on fable" case is no longer a SAME-tier spawn:
    # b284013 ("Fable is above Opus, not level with it") put fable in its own
    # tier, so -ge cannot tax it and that needle was stale from that commit on.
    # The two explicit same-default spawns are the cases -ge taxes.
    check M14 "same tier taxed (over-blocking; the CEO's own Fable spawns refused)" \
        "frank on opus, its own default, explicit -> SILENT" \
        "reed on sonnet, its own default, explicit -> SILENT"
fi
    _mut_score
}
mut_pool_submit M14 _mutant_M14

# --- M15: the hatch degrades to a bare marker — any "model-downgrade-ack:"
# with nothing after it now exempts. The formality failure, clause 6 edition.
_mutant_M15() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = "model-downgrade-ack:[[:space:]]*[^[:space:]]"
assert s.count(old) == 2, "hatch pattern anchor count %d" % s.count(old)
open(p, "w", encoding="utf-8").write(s.replace(old, "model-downgrade-ack:"))
PY
if applied M15 "a bare model-downgrade-ack: exempts (hatch as formality)" \
   && alive M15 "a bare model-downgrade-ack: exempts (hatch as formality)"; then
    check M15 "a bare model-downgrade-ack: exempts (hatch as formality)" \
        "a BARE model-downgrade-ack: exempts nothing -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M15 _mutant_M15

# --- M16: the accepted ack stops being logged. A waiver nobody can count is a
# waiver that becomes a habit invisibly.
_mutant_M16() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = '>>"$C6_LOG_DIR/model-downgrade-acks.log" 2>/dev/null || true'
assert old in s, "log anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, ">/dev/null 2>&1 || true", 1))
PY
if applied M16 "the accepted-ack log removed" \
   && alive M16 "the accepted-ack log removed"; then
    check M16 "the accepted-ack log removed" \
        "an accepted model-downgrade-ack: is logged to .claude/state/model-downgrade-acks.log with both models"
fi
    _mut_score
}
mut_pool_submit M16 _mutant_M16

# --- M17 (FAIL-CLOSED ON ITS OWN ERROR): an alias the declaration does not
# rank becomes a refusal instead of an announced skip. A crashing guard that
# blocks every spawn is worse than the defect it was built for; the brief
# said so in those words.
_mutant_M17() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'C6_SKIP="MODEL_TIERS=\\"${MODEL_TIERS}\\" ranks no alias named ${C6_UNRANKED}"'
assert old in s, "unranked anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, 'PROBLEMS+=("model tier — unranked alias ${C6_UNRANKED}")', 1))
PY
if applied M17 "an unranked alias refuses instead of failing open" \
   && alive M17 "an unranked alias refuses instead of failing open"; then
    check M17 "an unranked alias refuses instead of failing open" \
        "an alias MODEL_TIERS does not rank -> allowed \(fail-open\)"
fi
    _mut_score
}
mut_pool_submit M17 _mutant_M17

# --- M18 (INFERENCE FROM NAMES — the defect, stated literally): the ranks come
# from a hardcoded table keyed on the alias name (tonight's invented belief:
# fable below sonnet) instead of from the declaration. The (n) block's
# contradicting-declaration cases exist for exactly this mutant.
_mutant_M18() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old_d = 'C6_RANK_DEFAULT="$(model_tier_rank "$C6_DEFAULT" "$MODEL_TIERS")"'
old_r = 'C6_RANK_REQUESTED="$(model_tier_rank "$C6_REQUESTED" "$MODEL_TIERS")"'
assert old_d in s and old_r in s, "rank call anchors not found"
new_d = 'case "$C6_DEFAULT" in opus) C6_RANK_DEFAULT=1;; sonnet) C6_RANK_DEFAULT=2;; fable) C6_RANK_DEFAULT=3;; haiku) C6_RANK_DEFAULT=4;; *) C6_RANK_DEFAULT="";; esac'
new_r = 'case "$C6_REQUESTED" in opus) C6_RANK_REQUESTED=1;; sonnet) C6_RANK_REQUESTED=2;; fable) C6_RANK_REQUESTED=3;; haiku) C6_RANK_REQUESTED=4;; *) C6_RANK_REQUESTED="";; esac'
open(p, "w", encoding="utf-8").write(s.replace(old_d, new_d, 1).replace(old_r, new_r, 1))
PY
if applied M18 "ranks inferred from alias names instead of the declaration" \
   && alive M18 "ranks inferred from alias names instead of the declaration"; then
    check M18 "ranks inferred from alias names instead of the declaration" \
        "reed \(sonnet default\) on fable, no reason -> allowed, SILENT \(an upgrade\)" \
        "the declared order is obeyed even when it contradicts the alias names: opus-default on haiku -> SILENT" \
        "the declared order is obeyed even when it contradicts the alias names: opus-default on fable \(same tier\) -> SILENT"
fi
    _mut_score
}
mut_pool_submit M18 _mutant_M18
# (The "opus-default on sonnet -> BLOCKED" case is deliberately NOT named for
# M18: a name-keyed table that puts opus above sonnet refuses it too, so it
# cannot tell the mutant from the guard. It is M13's kind of case, not M18's.)

# --- M19: the fail-open skip is silenced. Still allows, so every exit-code
# case stays green; only the announcement cases notice. A guard that stops
# guarding without saying so is the failure class this engine keeps finding
# in itself.
_mutant_M19() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = '[ -z "$C6_SKIP" ] || _c6_announce_skip "$C6_SKIP"'
assert old in s, "announce anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, "true", 1))
PY
if applied M19 "the fail-open skip silenced" \
   && alive M19 "the fail-open skip silenced"; then
    check M19 "the fail-open skip silenced" \
        "the fail-open skip is announced, naming the unranked alias" \
        "parser library missing -> allowed \(fail-open\), announced"
fi
    _mut_score
}
mut_pool_submit M19 _mutant_M19

# --- M20: REGISTRATION AT SPAWN deleted — the spawn happens whether or not it
# could be registered, which is the one thing point 3 says it may not do.
_mutant_M20() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'C7_ERR="$(printf ' + "'%s'" + ' "$INPUT" | python3 "$SCRIPT_DIR/../../mega-lander/workspaces.py" --entity "$ENTITY_ROOT" "$C7_VERB" 2>&1 >/dev/null)" || C7_RC=$?'
assert old in s, "clause 7 registration anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, 'C7_ERR=""', 1))
PY
if applied M20 "registration at spawn deleted (clause 7)" \
   && alive M20 "registration at spawn deleted (clause 7)"; then
    check M20 "registration at spawn deleted (clause 7)" \
        "L08  cross-repo-worktree: UNREGISTERED cc/ path" \
        "Q01"
fi
    _mut_score
}
mut_pool_submit M20 _mutant_M20

# --- M21: the cwd-only refusal deleted (clause 7f) — a spawn with no native
# workspace of its own, so its workspaces would not all go together (point 10).
_mutant_M21() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'if [ -n "$SPAWN_CWD" ] && [ "$ISOLATION" != "worktree" ] && [ "$ISOLATION" != "remote" ]; then\n  C7_PROBLEMS+=("cwd-only spawn refused'
assert old in s, "clause 7f anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, 'if false; then\n  C7_PROBLEMS+=("cwd-only spawn refused', 1))
PY
if applied M21 "the cwd-only refusal deleted (clause 7f)" \
   && alive M21 "the cwd-only refusal deleted (clause 7f)"; then
    check M21 "the cwd-only refusal deleted (clause 7f)" \
        "cwd into a REGISTERED cc/ workspace, no isolation -> BLOCKED"
fi
    _mut_score
}
mut_pool_submit M21 _mutant_M21

# --- CLAUSE 7g: A DRY EVALUATION IS NOT A LIVE ONE ---------------------------
# scripts/spawn.sh evaluates this guard against a payload it has not sent yet,
# so a brief with three problems costs one run instead of three dispatches. The
# claim that makes that safe is "the marker cannot turn a live spawn into an
# unregistered one", and a claim in a header is not a property. These two
# mutants are the property, one for each direction it can fail.

# --- M22: the marker alone decides — the tool_use_id half of the test dropped,
# so a LIVE Agent call that happened to carry richos_spawn_check would be
# evaluated and NEVER REGISTERED. This is the exact hole the design exists to
# not have, and under-blocking of the worst kind: point 3 says the spawn does
# not happen without a registration, and here it happens without one.
_mutant_M22() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = 'sys.exit(0 if (isinstance(d.get("richos_spawn_check"), dict)\n               and not str(d.get("tool_use_id") or "")) else 1)'
assert old in s, "clause 7g dry-detection anchor not found"
new = 'sys.exit(0 if isinstance(d.get("richos_spawn_check"), dict) else 1)'
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
if applied M22 "a live call carrying the marker is dry-evaluated (clause 7g)" \
   && alive M22 "a live call carrying the marker is dry-evaluated (clause 7g)"; then
    check M22 "a live call carrying the marker is dry-evaluated (clause 7g)" \
        "Q7G1"
fi
    _mut_score
}
mut_pool_submit M22 _mutant_M22

# --- M23: the dry path removed — every payload goes to register-spawn, which is
# where this started: the pre-flight's permanent false positive. Over-blocking,
# and the reason it matters is that the cheapest way to "fix" a marker you do
# not trust is to ignore it, which silently puts the four-command sequence back.
_mutant_M23() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = '  C7_VERB="check-spawn"'
assert old in s, "clause 7g verb anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, '  C7_VERB="register-spawn"', 1))
PY
if applied M23 "the dry evaluation removed (clause 7g)" \
   && alive M23 "the dry evaluation removed (clause 7g)"; then
    check M23 "the dry evaluation removed (clause 7g)" \
        "Q7G2"
fi
    _mut_score
}
mut_pool_submit M23 _mutant_M23

# --- M24: CLAUSE 2c deleted — the guard falls back to its own looser
# NAME_SHAPE_RE (no length bound, no exact-three-parts requirement), so a name
# the creator (mega-lander/create-teammate-worktree.sh) refuses passes this
# guard again, exactly the 2026-09-17 defect (zach-sonnet-high-multirepodemo1,
# a 14-character identifier) this clause exists to close.
_mutant_M24() {
    local ENG GUARD SUITE W_DIR MUT_SCORE
    _mut_worker_sandbox || return 1
python3 - "$GUARD" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = '  if ! _TN_MSG="$(teammate_name_check "$NAME" "$ALLOWED_MODELS")"; then\n    PROBLEMS+=("$_TN_MSG")\n  fi\n\n'
assert old in s, "clause 2c anchor not found"
open(p, "w", encoding="utf-8").write(s.replace(old, "", 1))
PY
if applied M24 "clause 2c deleted (the creator's stricter shape no longer applied here)" \
   && alive M24 "clause 2c deleted (the creator's stricter shape no longer applied here)"; then
    check M24 "clause 2c deleted (the creator's stricter shape no longer applied here)" \
        "C2C1"
fi
    _mut_score
}
mut_pool_submit M24 _mutant_M24


# --- DELIBERATE PINS (no mutant, and that is the honest answer). Three cases
# hold under every mutation above because they assert that the NORMAL path is
# still normal, and every mutant here changes an ABNORMAL path:
#   - "the hatch is recognized anywhere in the prompt, not only on line 1"
#   - "the hatch tolerates leading whitespace"
#   - "general-purpose, isolated + well-named, WITH hatch -> allowed"
# They are regression pins on the grep anchor and on the hatch actually
# working, not claims of mutation coverage. Naming them here is cheaper than
# letting a future reader assume a silent green means a proven check.

# (the `restore` that used to sit here reset the SHARED sandbox before M99.
#  There is no shared mutated state any more: each mutant had its own copy.)
# --- the mutant verdicts, drained ------------------------------------------
# ADDITIVE: M0 above already scored, and M99 below still will. A worker that left
# no exit code counts as UNPROVEN rather than vanishing from the tally.
mut_pool_drain
# A harness that declared mutants and ran NONE must not exit 0.
mut_pool_require_submissions "$(basename "$0")"
PROVEN=$(( PROVEN + MUT_POOL_PASS ))
UNPROVEN=$(( UNPROVEN + MUT_POOL_FAIL ))
mut_pool_report_line "$(( $(sw_now_ms) - MUT_WALL_T0 ))"
mut_pool_cleanup
echo ""

# --- M99: THE SHIPPED GUARD WAS NEVER OPENED FOR WRITING.
# The check this replaces compared the shipped guard's md5 against its
# starting md5 and printed "guard restored byte-for-byte". That sentence was
# true and it was the wrong guarantee: a harness that mutates and restores
# passes it, and the whole defect is the interval between those two writes.
# The mtime tells the two apart — a restore moves it, an untouched file does
# not — so this asserts NEVER TOUCHED rather than PUT BACK.
#
# WHERE THE MTIME IS NOT AVAILABLE (no format proved itself on this platform;
# see scripts/lib/tree-witness.sh) THIS DEGRADES TO THE CONTENT CHECK AND SAYS
# SO. It is not silently weaker.
SHIPPED_AFTER="$(tw_file_witness "$SHIPPED_GUARD")"
if [ "$SHIPPED_AFTER" != "$SHIPPED_BEFORE" ]; then
    echo "UNPROVEN  M99  THE SHIPPED GUARD WAS WRITTEN TO." >&2
    echo "            before: $SHIPPED_BEFORE" >&2
    echo "            after:  $SHIPPED_AFTER" >&2
    echo "            $SHIPPED_GUARD" >&2
    echo "            This harness must mutate its sandbox and nothing else. A run killed at the" >&2
    echo "            wrong moment would leave the operator's live spawn gate modified." >&2
    UNPROVEN=$((UNPROVEN+1))
elif tw_mtime_available; then
    printf 'PROVEN    %-4s %s\n' "M99" "the shipped guard was never opened for writing (contents AND mtime unchanged): a kill -9 at any point above damages nothing"
    PROVEN=$((PROVEN+1))
else
    printf 'PROVEN    %-4s %s\n' "M99" "the shipped guard's CONTENTS are unchanged. No sub-second mtime format proved itself on this platform, so this run cannot distinguish 'never touched' from 'written and restored' — named rather than assumed"
    PROVEN=$((PROVEN+1))
fi

echo ""
echo "=== summary: $PROVEN proven load-bearing, $UNPROVEN unproven ==="
# The SANDBOX guard must also be back at its pristine bytes, or the mutants ran
# against each other's leftovers instead of against one change at a time.
# THE ASSERTION GOT STRONGER WHEN THE MUTANTS GOT THEIR OWN SANDBOXES. It used
# to say "the shared sandbox guard was RESTORED byte-for-byte", which a harness
# that mutates and puts back satisfies — the whole risk lived in the interval
# between those two writes, and a mutant that ran while another was mid-interval
# measured the wrong file. There is now no interval: the reference sandbox below
# is built once, read by M0, and never opened for writing by anything, because
# every mutant works in a copy of its own. So this is no longer "put back", it is
# NEVER TOUCHED — the same distinction M99 draws about the shipped guard.
FINAL_MD5="$(md5 -q "$GUARD" 2>/dev/null || md5sum "$GUARD" | cut -d' ' -f1)"
if [ "$FINAL_MD5" != "$BASE_MD5" ]; then
    echo "ERROR: the REFERENCE sandbox guard changed during the run (md5 $FINAL_MD5 != $BASE_MD5)." >&2
    echo "       Each mutant is supposed to build its own copy and mutate that. A change here means" >&2
    echo "       something mutated the shared reference, so M0's baseline no longer describes what" >&2
    echo "       the mutants ran against and every verdict above is suspect." >&2
    exit 1
fi
echo "reference sandbox guard never written to (md5 $FINAL_MD5); every mutant used its own copy"
[ "$UNPROVEN" -eq 0 ] || exit 1
exit 0

#!/usr/bin/env bash
#
# owned-state.mutation.sh — EVERY PROPERTY OF "ADDRESSED MEANS SMALLER",
# PROVEN BY REMOVING IT.
#
# owned-state.test.sh section 7 is a set of green cases, and green cases are
# evidence of nothing until each has been watched going red for the RIGHT
# reason. This harness takes the shipped gate, removes one property at a time in
# a throwaway copy of the engine, and asserts that the suite fails AT THE NAMED
# CASE.
#
# THE FIRST MUTANT IS THE INCIDENT. On 2026-09-25 the founder asked whether the
# escalation backlog (143 outstanding, the oldest 19 days, 13 for him) had been
# "purely just discovered by accident". It had not: it was printed at every
# session start for a week, behind a gate that a keyword in any prompt
# satisfied. `keyword-counts-again` puts that keyword pattern back, and 7a must
# go red. If it does not, the suite cannot tell the fix from the defect.
#
# Invoked from owned-state.test.sh, so the runner that discovers *.test.sh runs
# this too. A harness nobody runs proves nothing about anything.
# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

G="scripts/hooks/guard-owned-state.sh"
L="scripts/lib/owned-systems.py"
D="owned-systems.declaration"

mutation_begin "THE OWNED-STATE GATE: addressed means the problem got smaller" \
               "scripts/hooks/owned-state.test.sh"
mutation_focus stop-at-want

# THE DECLARATION IS PART OF WHAT IS UNDER TEST. Section 7 copies the SHIPPED
# `escalations` row out of the engine's own owned-systems.declaration, and the
# shared copier carries scripts/, hooks/ and the config but no root-level
# declaration. Without this the suite would fail at "7-" in every mutant, and a
# red run for that reason would be credited to the mutation. The harness checks
# the NAMED case, so that would be caught, but the copy is what makes the
# mutants mean anything.
_mut_copy_engine() { # <dir>
    mutation_copy_engine "$1" "$MUT_ENGINE_ROOT" || return 1
    cp "$MUT_ENGINE_ROOT/$D" "$1/$D"
}

# --- 1. A KEYWORD NEVER COUNTS ---------------------------------------------
mutant keyword-counts-again "7a" "$D" \
    'triage: escalations\.jsonl' \
    'triage: (escalat|esc-2026|escalate\.sh|--disposition)' \
    "a dispatch that merely talks about escalations would clear the gate for the whole session, which is the exact hole the 143-item backlog grew behind: the escalations row was 'addressed' once in fifteen days, by an unrelated dispatch whose text contained the word."

# --- 2. A REAL DROP COUNTS -------------------------------------------------
mutant drop-not-credited "7c" "$L" \
    'and rec["measure"] < base:' \
    'and rec["measure"] < base - 1000000:' \
    "acknowledging escalations would never clear the gate, so the one piece of real work it asks for would be answered with a waiver every session, and a gate answered with waivers is a gate that dies."

# --- 3. AN OLD BACKLOG REFUSES, AFTER A KEYWORD OR NOT ---------------------
mutant any-backlog-tolerated "7a" "$L" \
    'return r.get("status") == UNHEALTHY and r.get("backlog") == 0' \
    'return r.get("status") == UNHEALTHY and r.get("backlog") is not None' \
    "any escalation backlog, however old, would count as 'within tolerance', so a 19-day-old escalation for the CEO would never be told to anyone."

# --- 4. NOTHING PAST THE TOLERATED AGE IS ENOUGH ---------------------------
mutant tolerance-ignored "7e" "$L" \
    'return r.get("status") == UNHEALTHY and r.get("backlog") == 0' \
    'return False' \
    "a lead who acknowledged every old escalation would still be told of it while an hour-old one was outstanding, so the notice would fire on news rather than neglect and be read past as noise."

# --- 5. A NAMED TRIAGE COUNTS ----------------------------------------------
mutant triage-ignored "7f" "$G" \
    'pattern = d.get("triage") or ""' \
    'pattern = ""' \
    "a dispatch whose whole job is triaging the escalation ledger would be told of the backlog as if it were unrelated work, so the natural answer to the notice would be treated as ignoring it."

# --- 6. THE ACK STOPS NO CLOCK ---------------------------------------------
mutant ack-resets-age "7g" "$L" \
    '    ok = record_disposition(args.entity, args.session, args.system.strip(),' \
    '    _write_json(state_path(args.entity, SEEN_NAME), {}){NL}    ok = record_disposition(args.entity, args.session, args.system.strip(),' \
    "an owned-state-ack would reset the age of the thing it deferred, so a backlog waived once a session would look new every session."

# --- 7. THE BASELINE IS FRESH ----------------------------------------------
mutant baseline-from-cache "7h" "$L" \
    '        top, fresh = current(standing[0]["id"])' \
    '        top, fresh = standing[0], True' \
    "a session starting inside the cache's lifetime would take its baseline from a count made BEFORE the previous session's triage, and be credited with a drop it never made on its first re-check."

# --- 8. THE OLD KEYWORD LEDGER ROWS CLEAR NOTHING --------------------------
mutant legacy-addressed-counts "7j" "$L" \
    'COUNTING = ("ack", "progress", "triage")' \
    'COUNTING = ("ack", "progress", "triage", "addressed")' \
    "the eleven keyword 'addressed' rows already in the live ledger, and any written by an old copy of the hook, would keep clearing sessions."

# --- 9. A MEASURE THAT CANNOT READ A NUMBER IS REFUSED ---------------------
mutant groupless-measure-accepted "1d" "$L" \
    '            if needs_group and rx.groups < 1:' \
    '            if False:' \
    "a measure with no capture group would never read a number, so 'the count fell' could never be true and the row would become a wall that is waived every session."

mutation_end

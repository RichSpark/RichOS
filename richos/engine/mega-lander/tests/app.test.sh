#!/usr/bin/env bash
# Synthetic desktop dispatch across real Git workspaces and the actual ECS store,
# then the mutation harness that proves the land lock's properties can fail
# (app.mutation.sh). A harness nobody runs proves nothing about anything, which
# is why it is invoked from the suite it mutates.
# The second review's app half is exercised here too (slice 4): app.test.py drives
# scripts/lib/app_review.py through scripts/app-engine-hook.py's mid-job delivery, and
# scripts/lib/review_watch.py's app mode (AppWorld) over this fixture's registry.
# A connected repository's own CLAUDE.md or AGENTS.md in a worker's and a reviewer's brief:
# app.test.py drives scripts/lib/instruction_files.py through prepare.
# A closed grant's cause, which work, who closed it and when (P1-A, 2026-10-29): app.test.py
# drives scripts/lib/scope_closure.py through read_scope and scripts/app-engine-hook.py's turn gate.
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B -W ignore "$HERE/app.test.py" "$@" || exit 1
# ONLY BEFORE NIGHTLIES (hunt part 4 finding 19, 2026-09-30; the fourteen-point suite's
# ruling of 2026-09-23 applied to this one): the nightly's gates/workspace-mutants runs this
# unit with RICHOS_MUTATION_PASSES=1. Anywhere else the pass says NOT RUN and why.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$HERE/app.mutation.sh" ]; then
    if [ "${RICHOS_MUTATION_PASSES:-}" = 1 ]; then
        bash "$HERE/app.mutation.sh" || exit 1
    else
        echo "  NOT RUN  the mutation harness: it runs before each nightly (hunt part 4 finding 19); RICHOS_MUTATION_PASSES=1 runs it here"
    fi
fi
exit 0

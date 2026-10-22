#!/usr/bin/env bash
# One test per numbered point of docs/plans/worktree-spec-2026-09-11.md, then
# the mutation harness that proves each of them can fail (workspaces.mutation.sh).
# It also proves scripts/lib/hold_words.py, the words that hold work, through the
# SendMessage parser (workspaces.test.py NoSilentHolds; no silent holds, 2026-10-22).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B -W ignore "$HERE/workspaces.test.py" "$@" || exit 1
# ONLY BEFORE NIGHTLIES (hunt part 4 finding 19, 2026-09-30; the fourteen-point suite's
# ruling of 2026-09-23 applied to this one): the nightly's gates/workspace-mutants runs this
# unit with RICHOS_MUTATION_PASSES=1. Anywhere else the pass says NOT RUN and why.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$HERE/workspaces.mutation.sh" ]; then
    if [ "${RICHOS_MUTATION_PASSES:-}" = 1 ]; then
        bash "$HERE/workspaces.mutation.sh" || exit 1
    else
        echo "  NOT RUN  the mutation harness: it runs before each nightly (hunt part 4 finding 19); RICHOS_MUTATION_PASSES=1 runs it here"
    fi
fi
exit 0

#!/usr/bin/env bash
#
# release-land-leases.mutation.sh: PROVES release-land-leases.test.sh WOULD CATCH
# THE TURN-END RELEASE GOING WRONG (spec r3 e6, F3; Frank G5 point 3). Each
# mutant removes ONE property from a throwaway copy of the engine and demands that
# the NAMED case go red. The loop is scripts/lib/mutation-harness.sh.
#
# Run directly: scripts/hooks/release-land-leases.mutation.sh

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

mutation_begin "the operator fence's turn-end release" "scripts/hooks/release-land-leases.test.sh"
# Each mutant stops at its named FAIL line (see mutation_focus): in this suite a
# printed FAIL line always ends the run red, so the rest of the run buys nothing.
mutation_focus stop-at-want

L="scripts/lib/operator_fences.py"

mutant released-when-not-at-rest "T2 " "$L" \
    '        if rest:{NL}            flock = _Flock(files.lock)' \
    '        if True:{NL}            flock = _Flock(files.lock)' \
    "F3: a lease would be released with a merge half done or commits unpushed, and another land would start on top."
mutant released-under-a-running-land "T8 " "$L" \
    '        lands = running_lands(lease)' \
    '        lands = []' \
    "2026-10-11: the holder's turn end would release the lease between two steps of its own background land, and the fence would refuse the land's next write (twice in richos that day)."
mutant dirty-paths-unnamed "T3 " "$L" \
    '            reasons.append("uncommitted changes (%s)" % ", ".join(dirty[:6] + (["..."] if len(dirty) > 6 else [])))' \
    '            reasons.append("uncommitted changes")' \
    "G5 point 3: a holder could not tell its own unfinished work from another writer's dirt."
mutant other-sessions-lease-released "T5 " "$L" \
    '        if not is_ancestor(h.get("pid", -1), h.get("start", -1), chain):{NL}            continue{NL}        conf, paths = launcher_for' \
    '        if False:{NL}            continue{NL}        conf, paths = launcher_for' \
    "e6: one session's turn end would release another conversation's lease in the middle of its land."
mutant switch-ignored "T6 " "$L" \
    '        if not fenced(conf) or not paths:{NL}            continue{NL}        files = Files(conf)' \
    '        if not conf or not paths:{NL}            continue{NL}        files = Files(conf)' \
    "G12: with the switch off the hook would still speak at every turn end."

mutation_end

#!/usr/bin/env bash
#
# engine_pass.mutation.sh — PROVES THE ONE-PASS SLOT'S SUITE CAN FAIL.
#
# Takes the SHIPPED scripts/lib/engine_pass.py, removes ONE property at a time in a
# throwaway copy of the engine, and asserts that scripts/lib/engine_pass.test.sh fails on
# the SPECIFIC named case. The loop is scripts/lib/mutation-harness.sh; this file is the list
# of properties "one full engine pass on this Mac at a time" rests on.
#
# Invoked by scripts/lib/engine_pass.test.sh, so the runner that discovers *.test.sh runs it
# too. Run directly: scripts/engine_pass.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/mutation-harness.sh
. "$SCRIPT_DIR/lib/mutation-harness.sh"
mutation_begin "engine_pass (one full engine pass at a time)" "scripts/lib/engine_pass.test.sh"
# Each case runs alone when named, so a mutant costs its own case, not the suite.
mutation_focus want-as-argument

P="scripts/lib/engine_pass.py"

# 1. THE SLOT ITSELF. Without the lock two large passes run side by side.
mutant no-lock "EP05" "$P" \
    '                    fcntl.flock(slot, fcntl.LOCK_EX | fcntl.LOCK_NB){NL}                    break' \
    '                    break' \
    "Two large engine passes would run at once despite the serialization policy."

# 2. THE LINE BETWEEN A SCOPED RUN AND A FULL PASS.
mutant threshold-moved "EP01" "$P" \
    'FULL_PASS_UNITS = 20{NL}' \
    'FULL_PASS_UNITS = 30{NL}' \
    "A 29-unit selection would run beside the integration pass without the required slot."

# 3. AN EXPLICIT BOUND IS HONORED. There is no default bound (a queued run waits its turn), but
# a caller that names one (--wait) is refused with its units when it runs out.
mutant wait-unbounded "EP04" "$P" \
    '            if wait is not None and now - start >= wait:' \
    '            if False:' \
    "A pass given an explicit --wait would wait with no end instead of being refused with its units."

# 4. THE LAND RUN GOES FIRST.
mutant no-priority "EP07b" "$P" \
    '            if main or not _locked(priority):' \
    '            if True:' \
    "A teammate arriving at the right moment would take the slot the land run was waiting for."

# 5. A PID IS NOT AN IDENTITY.
mutant pid-without-birth "EP09" "$P" \
    '    if birth(rec["pid"], rows) != rec.get("birth"):{NL}        return False' \
    '    if False:{NL}        return False' \
    "A process whose PID was reused from a dead holder would run a full pass as if it held the slot."

# 6. THE HOLDER'S OWN CHILDREN RUN UNDER IT (otherwise every shard waits for its own parent).
mutant no-inheritance "EP08" "$P" \
    '        if pid == rec["pid"]:{NL}            return True' \
    '        if False:{NL}            return True' \
    "Every engine shard of a held pass would wait for the slot its own runner holds, until refused."

# 7. THE PASS KEEPS THE SLOT EVEN IF ITS WRAPPER DIES.
mutant fd-not-inherited "EP10" "$P" \
    'child = subprocess.Popen(cmd, pass_fds=(slot.fd,), env=inherited_env)' \
    'child = subprocess.Popen(cmd, env=inherited_env)' \
    "Killing the wrapper would free the slot while its pass still ran, and a second pass could start."

# 8. THE HOLDER IS NAMED, so a waiting run can say who it waits for.
mutant holder-unrecorded "EP03" "$P" \
    '    os.replace(tmp, os.path.join(d, "holder.json"))' \
    '    os.remove(tmp)' \
    "A waiting run could not say whose pass it is waiting for."

mutation_end

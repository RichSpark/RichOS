#!/usr/bin/env bash
#
# resource-waits.mutation.sh — PROVES THE RESOURCE-WAIT SUITE CAN FAIL, ONE RULE
#                              AT A TIME, FOR THE RIGHT REASON.
#
# Half of what resource-waits.test.sh asserts is silence (a 9-minute waiter, a
# busy holder with nobody waiting, a wait that ended), and a gate that never
# runs is silent too. So: take the shipped source, remove ONE property in a
# throwaway copy of the engine, and assert that the suite fails AT THE CASE
# THAT NAMES IT. The loop is scripts/lib/mutation-harness.sh;
# resource-waits.test.sh runs this file.
#
# Run directly: scripts/hooks/resource-waits.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

mutation_begin "resource waits: nothing waits in line past ten minutes" "scripts/hooks/resource-waits.test.sh"
# The suite's cases run in sequence and share one sandbox, and a printed
# `FAIL  <case>` always ends it red: each mutant stops at its own line.
mutation_focus stop-at-want

L="scripts/lib/resource_waits.py"
G="scripts/hooks/guard-resource-waits.sh"

# --- 1. THE THRESHOLD: ten minutes, not more and not less ------------------
mutant threshold-doubled "RW10" "$L" \
    '    overdue = [w for w in waits if now - w["since"] >= threshold]' \
    '    overdue = [w for w in waits if now - w["since"] >= threshold * 2]' \
    "an 11-minute waiter would pass: the CEO's ten minutes would silently become twenty."

mutant threshold-lowered "RW11" "$L" \
    '    overdue = [w for w in waits if now - w["since"] >= threshold]' \
    '    overdue = [w for w in waits if now - w["since"] >= threshold - 180]' \
    "a 9-minute waiter would refuse the turn: the gate would fire on waits the rule allows."

mutant wrapper-never-blocks "RW10" "$G" \
    '        [ "$RC" = "2" ] && exit 2' \
    '        [ "$RC" = "2" ] && exit 0' \
    "the analyzer would find the wait and the turn would end anyway."

mutant config-threshold-ignored "RW32" "$G" \
    'RICHOS_RESOURCE_WAIT_MINUTES="$RESOURCE_WAIT_MINUTES"' \
    'RICHOS_RESOURCE_WAIT_MINUTES=""' \
    "RESOURCE_WAIT_MINUTES in orchestration.config would be read and never used."

# --- 2. WHO IS WAITING ----------------------------------------------------
mutant vm-wait-misread "RW13" "$L" \
    '            "resource": str(rec.get("resource") or "unknown"),' \
    '            "resource": "unknown",' \
    "a recorded wait for the VM would not be read as one: the refusal could not say who holds the VM or whether it is in use."

mutant ps-never-read "RW10" "$L" \
    '    if any(now - w["since"] >= threshold for w in waits):{NL}        # Only now' \
    '    if False:{NL}        # Only now' \
    "an overdue wait would be reported with no word on who is using the CPU, and a reused pid could not be told from a live one."

mutant dead-pid-counted "RW18" "$L" \
    '        if not pid_alive(pid):{NL}            continue{AND}        if p is None:{NL}            continue' \
    '        if False:{NL}            continue{AND}        if p is None:{NL}            kept.append(w){NL}            continue' \
    "a record left by a killed process would refuse turns forever: the wait ended and the gate would not know."

mutant reused-pid-counted "RW31" "$L" \
    '        if p.age is not None and (now - p.age) > w.get("began", w["since"]) + 5:' \
    '        if False:' \
    "a dead wait whose pid was reused by an unrelated process would read as a wait still going."

# --- 3. WHO HOLDS IT, AND IS IT IN USE -------------------------------------
mutant idle-slot-unseen "RW14" "$L" \
    '        if not mine:{NL}            lines.append("%s. IN USE RIGHT NOW: NO. The slot is held but no guest is booted." % who)' \
    '        if False:{NL}            lines.append("%s. IN USE RIGHT NOW: NO. The slot is held but no guest is booted." % who)' \
    "a slot held with no guest booted would not be reported as idle, which is the 5-second-fart case."

mutant admitting-unseen "RW44" "$L" \
    '        if not mine and s.get("state") == "admitting":' \
    '        if False:' \
    "a holder still being admitted would be reported as holding an idle slot, hiding that it is about to run."

mutant holding-ignored "RW15" "$L" \
    '                if os.path.realpath(h) == os.path.realpath(s["path"]) and s["pid"] is None:' \
    '                if False:' \
    "a walk holding guest.lock while itself waiting for CPU would never be named as the holder."

mutant hold-walk-unseen "RW16" "$L" \
    '                        hand_held = True' \
    '                        hand_held = False' \
    "a guest held open for hand-driven steps would be reported as a run executing."

# --- 4. ESCALATIONS: an ack never clears; the wait ending does --------------
mutant ack-clears "RW21" "$L" \
    '        if r.get("event") != "Escalation":{NL}            continue{NL}        text =' \
    '        if r.get("event") != "Escalation" or any(a.get("event") == "EscalationAck" and a.get("id") == r.get("id") for a in rows):{NL}            continue{NL}        text =' \
    "a disposition saying \"keep waiting\" would clear the wait: exactly the answer the CEO asked to make impossible."

mutant go-file-ignored "RW22" "$L" \
    '        if go and os.path.exists(os.path.expanduser(go)):' \
    '        if False:' \
    "a wait whose go-file now exists would still refuse turns."

mutant worktree-ignored "RW23" "$L" \
    '        if worktree and not os.path.isdir(worktree):' \
    '        if False:' \
    "a teammate that has ended would keep refusing turns for a wait nobody is in."

mutant keep-waiting-accepted "RW24" "$L" \
    '    if KEEP_WAITING.search(note):' \
    '    if False:' \
    "wait-over would accept \"keep waiting\" as the end of a wait."

mutant wait-over-ignored "RW25" "$L" \
    '        live = [i for i in items if last_end is None or i["raised"] > last_end]' \
    '        live = list(items)' \
    "a wait the waiter recorded as over would refuse turns forever."

mutant negation-ignored "RW01" "$L" \
    '        if NEGATED.search(whole[max(0, cand.start() - 24):cand.start()]):' \
    '        if False:' \
    "\"no longer waiting for the VM\" would be read as a wait."

# --- 4b. A HELD AGENT IS NOT WAITING IN LINE (2026-10-11) -------------------
mutant held-counted "RW45" "$L" \
    '    waits = [w for w in waits if not w["held_by"]]' \
    '    waits = list(waits)' \
    "a teammate the lead froze would refuse the lead's turn at ten minutes, the 04:26Z and 06:19Z incidents."

mutant held-time-counted "RW46" "$L" \
    '        w["since"] = began + total' \
    '        w["since"] = began' \
    "a resumed wait would count the minutes it was held, and refuse the turn the moment its agent is resumed."

mutant held-by-pid-ignored "RW48" "$L" \
    '    return isinstance(birth, (int, float)) and birth <= w.get("began", w["since"]) + HOLD_BIRTH_SLACK' \
    '    return False' \
    "a wait recorded by a process the hold froze would count whenever its waiter name is not the agent's."

# --- 5. THE WRAPPER ---------------------------------------------------------
mutant refire-stands-down "RW27" "$L" \
    '    if payload.get("agent_id"):{NL}        # A teammate' \
    '    if payload.get("agent_id") or payload.get("stop_hook_active"):{NL}        # A teammate' \
    "the re-fire would wave the turn through while the wait goes on."

mutant teammate-refused "RW30" "$L" \
    '    if payload.get("agent_id"):{NL}        # A teammate' \
    '    if False:{NL}        # A teammate' \
    "a teammate would be refused for a wait only the lead can reorder."

mutant stand-down-silent "RW33" "$G" \
    '    stop_notice_abnormal "stood-down" \' \
    '    true "stood-down" \' \
    "CHECK_RESOURCE_WAITS=0 would switch the gate off without a word to the operator."

mutant unreadable-silent "RW35" "$L" \
    '        problems.append("the wait records at %s could not be read" % waits_dir())' \
    '        pass' \
    "an unreadable waits directory would look exactly like a clean one."

# --- 6. THE WRITER ------------------------------------------------------------
mutant record-left-behind "RW42" "$L" \
    '    def close(self):{NL}        try:{NL}            os.unlink(self.path)' \
    '    def close(self):{NL}        try:{NL}            pass' \
    "a wait that ended would leave its record, and the gate would read it as a wait still going."

mutation_end

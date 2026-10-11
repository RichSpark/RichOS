#!/usr/bin/env bash
#
# operator-fences.mutation.sh: PROVES operator-fences.test.sh WOULD CATCH EACH
# RULE OF THE LEASE, THE FENCE AND THE SWITCH GOING WRONG.
#
# Each mutant removes ONE property from a throwaway copy of the engine and demands
# that the NAMED case go red. The loop is scripts/lib/mutation-harness.sh; the
# suite is run with stop-at-want (its cases share state and a printed FAIL line
# always ends it red), under ONE git, the first of OPERATOR_FENCES_MUTATION_GIT
# (default Homebrew, else Apple), because the suite's second git doubles the time
# of every mutant and proves nothing a mutant is about.
#
# WHAT HAS NO MUTANT HERE, AND WHY:
#   F5, F6, F8, F16's positive halves, F24 and F23 are the passes the refusals
#   are measured against; a mutant that makes a pass fail is a mutant of the
#   fixture, not of a rule.
#   F3L and F12L assert Git's own behavior, not ours.
#   The early check, the turn-end release, the supervisor's reaping and the
#   lease-aware unfinished-land guard have their own suites and harnesses:
#   scripts/hooks/land-lease-commands.mutation.sh,
#   scripts/hooks/release-land-leases.mutation.sh,
#   scripts/lib/provider-supervisor.mutation.sh, and femcboost's
#   scripts/hooks/guard-unfinished-land.mutation.sh.
#
# Run directly: scripts/operator-fences.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

for d in ${OPERATOR_FENCES_MUTATION_GIT:-/opt/homebrew/bin /usr/bin}; do
    if [ -x "$d/git" ]; then export OPERATOR_FENCES_GITS="$d"; break; fi
done

mutation_begin "operator fences: the lease, the fence and the switch" "scripts/operator-fences.test.sh"
mutation_focus stop-at-want

L="scripts/lib/operator_fences.py"
T="scripts/lib/operator-fence-launcher.sh"

# --- the fence's decisions (e2, F2, G1, G3, G4, Frank §4 (B)) -------------------
mutant main-move-unfenced "F4 " "$L" \
    '                    need, why = True, "a move of main"' \
    '                    need, why = False, "a move of main"' \
    "a commit, merge or reset of main by a writer without the lease would move main: the fence's first job."
mutant orig-head-unfenced "F1 " "$L" \
    '        elif ref == "ORIG_HEAD" and main_worktree:' \
    '        elif ref == "ORIG_HEAD" and False:' \
    "G1: a merge without the lease would write MERGE_HEAD and the shared tree before any refusal."
mutant same-value-is-a-move "F7 " "$L" \
    '                if current is not None and new == current:' \
    '                if False:' \
    "F2: Git's own ref packing would be refused, so gc and maintenance would fail on every repository."
mutant any-deletion-passes "F9 " "$L" \
    '                if writer_argv()[:1] == ["pack-refs"]:' \
    '                if True:' \
    "G4: git update-ref -d refs/heads/main would pass without the lease whenever loose and packed agree."
mutant packing-deletion-refused "F7 " "$L" \
    '                if writer_argv()[:1] == ["pack-refs"]:' \
    '                if False:' \
    "G4, the other direction: the second half of packing would be refused."
mutant restore-intent-ignored "F10 " "$L" \
    '                elif current is None and _restore_intent_matches(files, new, chain):' \
    '                elif False:' \
    "G3: the engine's own create-only restore of a deleted main would be refused."
mutant head-switch-unfenced "F11 " "$L" \
    '            if leaves_main and writer_argv()[:2] != ["worktree", "add"]:' \
    '            if False:' \
    "Frank §4 (B): a switch of the shared checkout away from main would pass, and every later land would merge into the wrong branch."
mutant worktree-add-refused "F11 " "$L" \
    '            if leaves_main and writer_argv()[:2] != ["worktree", "add"]:' \
    '            if leaves_main:' \
    "every teammate spawn (git worktree add from the main checkout) would be refused: measured, it reports the new worktree's HEAD from the main repository."

# --- the lease (e1, F1, G5, G7, G10) -----------------------------------------------
mutant ancestry-not-checked "F12 " "$L" \
    '    return is_ancestor(h["pid"], h["start"], chain), lease' \
    '    return True, lease' \
    "any process would count as the holder of a live lease, so another session's abort would pass."
mutant start-time-not-checked "F28 " "$L" \
    '    return bool(p) and not p["zombie"] and int(p["start"]) == int(start)' \
    '    return bool(p) and not p["zombie"]' \
    "a recycled pid would be taken for the holder, and a lease could live forever on somebody else's process."
mutant ttl-ignored "F17 " "$L" \
    '    if h.get("kind") != "claude" and lease.get("expires") and now() > float(lease["expires"]):' \
    '    if False:' \
    "e1: an abandoned Codex lease would never expire, even at rest."
mutant takeover-of-live-lease "F29 " "$L" \
    '        if state == "live":' \
    '        if False:' \
    "G5: a live holder's lease could be taken over in the middle of its land."
mutant takeover-without-preserving "F17 " "$L" \
    '        kept = preserve(files, paths, "takeover of a lease whose holder %s" % (' \
    '        kept = "" if True else preserve(files, paths, "takeover of a lease whose holder %s" % (' \
    "G5: an abandoned holder's uncommitted work would change hands with no copy kept."
mutant default-wait-240 "F18 " "$L" \
    'DEFAULT_WAIT = 90.0' \
    'DEFAULT_WAIT = 240.0' \
    "G7: the default wait would outlast the Bash tool's 120 s default, and the holder's name would never reach the waiting lead."
mutant wait-ceiling-gone "F18 " "$L" \
    '    if wait < 0 or wait > MAX_WAIT:' \
    '    if wait < 0:' \
    "e1: one call could wait past the 600 s command cap."
mutant home-not-checked "F19 " "$L" \
    '    if os.path.realpath(home) != os.path.realpath(conf.get("HOME", "")) or key != conf.get("KEY"):' \
    '    if False:' \
    "G10: two callers with different lease homes would each see their own lease as free."

# --- ownership of an unfinished operation (G6) --------------------------------------
mutant orphan-abort-of-a-live-starter "F13 " "$L" \
    '    if v not in ("owner-ended", "refused-mine", "refused-other"):' \
    '    if False:' \
    "G6: the lease holder would abort a cherry-pick whose starter is alive and still resolving it."
mutant stale-refusal-matches "F13 " "$L" \
    '            if started_at is None or float(rec.get("epoch")) < started_at - 0.5:' \
    '            if False:' \
    "an old refusal of the same commit would make a live starter's new cherry-pick look refused, and it would be aborted (measured in this suite before the fix)."

mutant refused-commit-orphans "F33 " "$L" \
    '        if not rewrote and not same_holder(caller, starter):' \
    '        if False:' \
    "Fix 3: another writer's refused commit during the holder's healthy merge would make acquire call that merge orphaned and send the holder to abort-orphan."

# --- the residue a refused writer leaves (Frank's re-check §2, Fixes 1 and 2) -----------
mutant discarded-operation-concluded "F31 " "$L" \
    '    found = discarded_operation(files, gitdir)' \
    '    found = None' \
    "Fix 1: after a refused merge --abort discarded the holder's resolution, its commit would record an empty-diff merge that every 'landed' check accepts (case G)."
mutant residue-committed-silently "F32 " "$L" \
    '    found = residue_committed(files, lease, new, os.getcwd())' \
    '    found = None' \
    "Fix 2: after a refused reset --hard rewrote the index, the holder's next ordinary commit would silently revert the previous commit (case E)."
mutant residue-never-named "F32 " "$L" \
    '    moved = _main_moved_at(paths["common"])' \
    '    return []' \
    "Fix 2 point 4: acquire and status would say nothing about a tree a refused writer had already rewritten."

# --- the ingress takes the lease; it is never exempt (Frank's re-check §4, Fix 4) -------
I="scripts/hooks/commit-ceo-inputs.py"
mutant ingress-exempt "F30 " "$I" \
    '    return LAUNCHER_MARKER in text and \' \
    '    return False and \' \
    "Fix 4: the ingress would commit without the lease, so with the switch on the fence refuses it and his file is not committed (or, exempted, it would move main in the middle of another lead's land)."
mutant ingress-releases-the-leads-lease "F30 " "$I" \
    '        if lease == "acquired":' \
    '        if lease in ("acquired", "renewed"):' \
    "Fix 4: a file handed over mid-land would release the lead's own land lease in the middle of its land."
mutant ingress-swallows-lease-notes "F30 " "$I" \
    '    notes = [ln.strip() for ln in out.strip().splitlines()[1:] if ln.strip()]' \
    '    notes = []' \
    "Fix 4: the ingress's renewal of the lead's lease would lift the residue refusal while what it named went nowhere."
mutant ingress-drops-pending "F30 " "$I" \
    '    cands += [(p, "pending") for p in pending_paths(state_dir) if p not in named] if state_dir else []' \
    '    cands += []' \
    "Fix 4: a file handed over while another land held the lease would never be committed unless he named it again."

# --- the launcher and the switch (e8, G12) --------------------------------------------
mutant off-can-refuse "F20 " "$T" \
    'if [ "$OPERATOR_FENCES_STATE" = "on" ] || [ -n "$OPERATOR_FENCES_REVIEWS" ]; then' \
    'if true; then' \
    "G12: with the switch off the launcher would still run the fence, so a defect in it would reach his terminal before the switch is ever turned on."
mutant chain-skipped-on-refusal "F14 " "$T" \
    '__ofl_rc=0{NL}for __ofl_m in' \
    '[ "$__ofl_refused" = 1 ] && exit 1{NL}__ofl_rc=0{NL}for __ofl_m in' \
    "e2: a refused transaction would leave no forensic record."
mutant fence-defect-fails-open "F21 " "$T" \
    '        if [ "$__ofl_phase" = "prepared" ] && [ "$__ofl_frc" != 0 ]; then' \
    '        if [ "$__ofl_phase" = "prepared" ] && [ "$__ofl_frc" = 1 ]; then' \
    "e8: with the switch on, a fence that cannot decide would pass everything, silently."

# --- the pieces other files carry --------------------------------------------------------
mutant recorder-names-the-launcher "F14 " "scripts/hooks/ref-transaction-forensics.sh" \
    '    p="${RICHOS_REF_HOOK_PARENT:-${PPID:-0}}"' \
    '    p="${PPID:-0}"' \
    "every forensic row under the launcher would name the launcher, not Git, as the writer."
mutant recorder-installed-over-launcher "F26 " "scripts/install-ref-forensics.sh" \
    '    TARGET="$COMMON/hooks/reference-transaction.d/20-ref-transaction-forensics.sh"' \
    '    TARGET="$TARGET"' \
    "install-ref-forensics.sh would copy the recorder over the launcher and silently remove the fence."
mutant land-lock-ignores-lease "F27 " "mega-lander/app.py" \
    '                leased = _live_fence_lease(repo)' \
    '                leased = None' \
    "G2: the product lander, which Git's hooks never see, would merge in the middle of a lease holder's land."
mutant non-utf8-hook-raises "F27 " "mega-lander/app.py" \
    '                  errors="replace") as handle:' \
    '                  errors="strict") as handle:' \
    "Frank's #13: a hook that is not UTF-8 would raise UnicodeDecodeError into the product lander, even with the switch off."
mutant restore-writes-no-intent "F10 " "mega-lander/workspaces.py" \
    '    intent = _fence_restore_intent(repo, branch, tip)' \
    '    intent = ""' \
    "G3: the restore would reach the fence with nothing to recognize it by, and be refused."

mutation_end

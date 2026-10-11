#!/usr/bin/env bash
#
# guard-sealed-worktree.mutation.sh — PROVES the lock-out's suite CAN FAIL, one
# property at a time. Invoked by guard-sealed-worktree.test.sh; the loop is
# scripts/lib/mutation-harness.sh. Case ids (G03 etc.) are the ones that suite
# prints on both PASS and FAIL. The verdict itself (finished, paused,
# registered) is mega-lander/workspaces.py's and is mutated by
# mega-lander/tests/workspaces.mutation.sh; this harness mutates what THIS file does
# with each verdict.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$SCRIPT_DIR/../lib/mutation-harness.sh"
mutation_begin "guard-sealed-worktree (the lock-out)" "scripts/hooks/guard-sealed-worktree.test.sh"
# Each mutant stops at its named FAIL line (see mutation_focus): in this suite a
# printed FAIL line always ends the run red, so the rest of the run buys nothing.
mutation_focus stop-at-want

G="scripts/hooks/guard-sealed-worktree.sh"

mutant finished-passes "G03" "$G" \
    '  REGISTERED){NL}    exit 0 ;;' \
    '  REGISTERED|FINISHED){NL}    exit 0 ;;' \
    "a restarted finished agent could write again (point 9)."

mutant finished-refusal-exits-zero "G03" "$G" \
    '      echo "  is over, and the orchestrator lands or discards it."{NL}      echo "$HOOK_TAG"{NL}    } >&2{NL}    exit 2 ;;' \
    '      echo "  is over, and the orchestrator lands or discards it."{NL}      echo "$HOOK_TAG"{NL}    } >&2{NL}    exit 0 ;;' \
    "the lock-out would print its refusal and let the call through — a warning wearing a guard's clothes."

mutant codex-workspace-writes-pass "G02b" "$G" \
    '  CODEX){NL}    {' \
    '  CODEX){NL}    exit 0; {' \
    "a registered agent's Edit or Write aimed inside a codex/ workspace would pass the only hook that sees it (point 2: an agent never works inside a codex/ workspace)."

mutant unregistered-passes "G08" "$G" \
    '  UNREGISTERED){NL}    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS" && exit 0' \
    '  UNREGISTERED){NL}    exit 0' \
    "a worker whose registration failed would work anyway, so a failed registration would not stop the spawn (point 3)."

mutant unregistered-cannot-read "G09" "$G" \
    '  UNREGISTERED){NL}    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS" && exit 0' \
    '  UNREGISTERED){NL}    :' \
    "an unregistered worker could not even read to report why it stopped."

mutant lead-is-a-worker "G18" "$G" \
    'if aid else "LEAD\t\t%s\t" % tool' \
    'if aid else "WORKER\tlead-as-worker\t%s\t" % tool' \
    "with the registry unreadable, the lead's own calls would be judged as a worker's and refused — the orchestrator bricked by its own guard's failure."

mutant readonly-types-not-exempt "G10" "$G" \
    '        [ -n "$BARE_TYPE" ] && [ "$BARE_TYPE" = "$t" ] && exit 0' \
    '        :' \
    "every Explore and Plan agent would be refused Bash forever: they own no workspace and nothing registers one."

mutant namespace-not-stripped "G11" "$G" \
    '    BARE_TYPE="${AGENT_TYPE##*:}"' \
    '    BARE_TYPE="$AGENT_TYPE"' \
    "a plugin-loaded read-only type would be judged as file-capable and refused."

mutant worktree-session-allowed "G15" "$G" \
    '  FORBIDDEN){NL}    is_readonly_tool' \
    '  FORBIDDEN){NL}    exit 0{NL}    is_readonly_tool' \
    "a claude --worktree session could work (point 3)."

mutant refused-lead-cannot-stop "G15c" "$G" \
    '    # Never refused: a lead that cannot stop its agents is how 2026-09-22 went.{NL}    exit 0 ;;' \
    '    # Never refused: a lead that cannot stop its agents is how 2026-09-22 went.{NL}    exit 2 ;;' \
    "a refused lead could not stop its agents: 2026-09-22, ten agents at 100% CPU for 33 minutes."

mutant refusal-names-no-way-out "G15b" "$G" \
    '      echo "    First prepare the acknowledgement, then make the printed TaskStop calls:"' \
    '      :' \
    "the refusal would not name the one call that can still succeed, so the next call is a retry."

mutant fail-open-without-registry "G16" "$G" \
    '    deny_cannot_evaluate "mega-lander/workspaces.py is missing at $WS_PY" "$TOOL_NAME" "$AGENT_ID"' \
    '    exit 0' \
    "an engine missing its registry would let every worker write, finished or not."

mutant fail-open-on-unparseable "G19" "$G" \
    '    deny_cannot_evaluate "the hook payload is unparseable (' \
    '    exit 0; deny_cannot_evaluate "the hook payload is unparseable (' \
    "a payload nobody could read would be waved through, finished agent or not."

mutation_end

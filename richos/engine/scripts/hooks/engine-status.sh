#!/usr/bin/env bash
#
# engine-status.sh — SessionStart hook. THE ANSWER TO "IS THIS DEFENSE ON?"
#
# WHY THIS HOOK EXISTS
# ====================
# This operation has been burned three times by defenses that reported ON while
# missing a whole failure class. The common shape is not "the guard was wrong";
# it is "nobody could tell the guard was doing nothing". A silent skip is worse
# than no guard at all, because it buys false confidence.
#
# The cure is not "block everything you cannot resolve" — applied literally that
# would brick every session in every directory on the machine, since the engine
# plugin is enabled at USER scope and therefore loads in EVERY project. The cure
# is that the defense must ALWAYS STATE WHETHER IT IS ON, in a place nobody has
# to go looking for.
#
# So this hook runs at every session start and emits:
#
#   * which ENGINE is loaded, and from where
#   * which REPOSITORY it resolved as the one it governs, and via which candidate
#   * whether enforcement is ACTIVE, STOOD DOWN, or BROKEN
#   * the count of guards that will actually run
#
# TWO CHANNELS, AND WHY BOTH ARE NEEDED
# =====================================
# `hookSpecificOutput.additionalContext` reaches the MODEL. It does not reach
# the human. That distinction is not academic: on 2026-08-28 an operator ran a
# real session in an adopted repository, looked at its stdout and stderr, saw no
# engine banner anywhere, and concluded the engine was not loaded. It was
# loaded, and enforcement was on — the announcement had gone somewhere only the
# model could see. "Firing silently" and "not firing" were indistinguishable to
# the one person who needed to tell them apart, which is the exact failure class
# this hook exists to remove, reproduced by the hook itself.
#
# So every status is now emitted on BOTH channels:
#
#   systemMessage                        -> the OPERATOR (Claude Code renders it
#                                           in the UI; the host's own hook schema
#                                           documents it as "Display a message to
#                                           the user (all hooks)")
#   hookSpecificOutput.additionalContext -> the MODEL
#
# The operator line is deliberately emitted for the STOOD-DOWN status too, even
# though the plugin is enabled at USER scope and therefore loads in every
# directory on the machine. One short line per session in an unadopted directory
# is the price of the guarantee, and the guarantee is the whole point: nobody
# can be unguarded without being told. There is deliberately no quiet switch —
# a mute for "this repo is not protected" is the silent skip walking back in
# through the door marked configuration.
#
# It never blocks (SessionStart hooks must not), but a BROKEN status is
# unmissable: it goes to stderr AND to the operator AND into the model's context
# AND names every candidate that was examined.
#
# LOG-ONLY / NEVER BLOCKS: always exits 0.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/engine-status.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 0
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# NOTE ON STDIN — this hook does NOT read the payload.
#
# It is a SessionStart hook AND a plain CLI tool, and in the CLI case stdin is
# an inherited pipe that nobody closes, so an unconditional `cat` hangs forever
# (`[ ! -t 0 ]` does not help: an inherited pipe is not a TTY). Measured: 92
# seconds and counting, inside the contract-integrity probe, before this was
# reverted.
#
# It costs nothing, because the payload's `cwd` is a REDUNDANT resolution
# candidate here: CLAUDE_PROJECT_DIR is measured present and correct in a
# plugin-loaded hook's environment at SessionStart (probe, 2026-08-28), and it
# outranks the payload cwd anyway. Paying a hang risk for a candidate that
# never wins is a bad trade.

# emit_context <model-summary> <operator-line>
#
# ONE call, TWO audiences. The operator line is a short verdict a human reads at
# a glance; the model summary is the full detail. Both are always emitted — a
# call site that supplied only one would recreate the invisible-announcement
# defect for whichever audience it left out, so the operator line is a REQUIRED
# positional argument rather than an option.
emit_context() { # <model-summary> <operator-line>
    local summary="$1"
    local sysmsg="$2"
    if command -v python3 >/dev/null 2>&1; then
        SUMMARY="$summary" SYSMSG="$sysmsg" python3 - <<'PY' 2>/dev/null || true
import json, os
print(json.dumps({
    "systemMessage": os.environ.get("SYSMSG", ""),
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": os.environ.get("SUMMARY", ""),
    }
}))
PY
    else
        # No python3: hand-escape. Both channels still carry a value — the
        # fallback path is exactly where a quietly-dropped field would never be
        # noticed.
        local escaped="${summary//\\/\\\\}"
        escaped="${escaped//\"/\\\"}"
        local escaped_msg="${sysmsg//\\/\\\\}"
        escaped_msg="${escaped_msg//\"/\\\"}"
        printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$escaped_msg" "$escaped"
    fi
}

VERSION="unknown"
[ -f "$ENGINE_ROOT/VERSION" ] && VERSION="$(awk 'NR==1 {gsub(/[[:space:]]/,""); print; exit}' "$ENGINE_ROOT/VERSION" 2>/dev/null || echo unknown)"

# --- THE GUARD FRACTION ----------------------------------------------------
#
# TWO DIFFERENT QUESTIONS, DELIBERATELY NOT THE SAME QUERY:
#
#   denominator  what will the HOST LOAD? -> hooks/hooks.json, the registration
#                surface. Derived, never typed (scripts/lib/registered-hooks.sh
#                carries the full history of why).
#   numerator    of those, how many are actually ON DISK and EXECUTABLE?
#
# Collapsing these into one query would produce a fraction that is always full
# and therefore says nothing. The gap between them IS the signal: a guard the
# host will register whose file is missing or non-executable shows up here as a
# SHORTFALL, at session start, in front of the operator.
#
# A count is still not proof of wiring — contract-integrity-probe.sh owns that
# — but a count that has fallen is a cheap, visible signal that something is
# missing, available a whole probe run earlier.
#
# THE ANNOUNCER IS DELIBERATELY NOT COUNTED. This file is registered in
# hooks.json alongside the guards, and excluding it is a decision, not an
# oversight:
#
#   * it guards nothing — it announces, and the noun in the line below is
#     "guards";
#   * its own term in the fraction could never be anything but satisfied. If
#     engine-status.sh were missing or non-executable there would be no banner
#     to read the fraction in. Padding a reassurance fraction with a term that
#     cannot fail is precisely the pathology the two drift incidents produced,
#     rebuilt on purpose.
#
# The exclusion is computed from THIS FILE'S OWN NAME, so it is one line and it
# cannot go stale — it is not a list of exceptions, which would be the same
# hand-maintained record walking back in through a different door. Note that
# the probe's BR2/BR4 count EVERY registered script, announcer included; their
# subject is the registration, not the enforcing set. Deliberately no number
# here: this comment read "all 16 registered scripts" while the derivation
# three lines below returned 26 — a hand-typed count going stale inside the one
# file whose entire argument is that counts must be derived. Found 2026-08-30.
# If you are about to write a figure into this file, that is the reason not to.
_GI_LIB="$SCRIPT_DIR/../lib/registered-hooks.sh"
GUARD_COUNT="?"
GUARD_EXPECTED="?"
GUARD_NOTE=" WARNING: the guard inventory could NOT be derived from the engine's hooks/hooks.json (missing, unreadable or registering nothing), so the count above is unknown and this engine install is not intact — treat enforcement as unverified until scripts/hooks/contract-integrity-probe.sh says otherwise."
if [ -f "$_GI_LIB" ]; then
    # shellcheck source=../lib/registered-hooks.sh
    . "$_GI_LIB"
    _GI_SELF="${BASH_SOURCE[0]##*/}"
    _GI_ROWS="$(registered_hook_scripts "$ENGINE_ROOT/hooks/hooks.json")" || _GI_ROWS=""
    if [ -n "$_GI_ROWS" ]; then
        GUARD_COUNT=0
        GUARD_EXPECTED=0
        GUARD_NOTE=""
        while IFS= read -r _gi_g; do
            [ -n "$_gi_g" ] || continue
            [ "$_gi_g" = "$_GI_SELF" ] && continue
            GUARD_EXPECTED=$((GUARD_EXPECTED + 1))
            [ -x "$ENGINE_ROOT/scripts/hooks/$_gi_g" ] && GUARD_COUNT=$((GUARD_COUNT + 1))
        done <<GI_EOF
$_GI_ROWS
GI_EOF
        # A registered guard whose file is missing or not executable is not
        # enforcing; the status word below comes from root resolution alone, so
        # the shortfall is said in words next to it (hunt part 3, 29).
        if [ "$GUARD_COUNT" -lt "$GUARD_EXPECTED" ]; then
            GUARD_NOTE=" WARNING: ONLY ${GUARD_COUNT} OF ${GUARD_EXPECTED} registered guards are present and executable, so enforcement is INCOMPLETE; the missing guards enforce nothing. Run scripts/hooks/contract-integrity-probe.sh."
        fi
    fi
fi

# ===========================================================================
# WHICH ENGINE IS ACTUALLY RUNNING — ITS PATH AND ITS HEAD
# ===========================================================================
# 2026-09-10. `installed_plugins.json` named a plugin cache directory as this
# engine's installPath; that directory was REAL, two days stale, and missing
# four scripts. The hooks demonstrably executed the LIVE checkout instead —
# provable because finish rows carried a field only the live ledger writer can
# produce — but the operator joined "stale directory" to "a fix producing no
# output" into a story he told the CEO before verifying it. His own words:
# "I inferred causation from a stale directory without establishing that
# anything reads it."
#
# The banner already named the resolved ENGINE_ROOT, which is the fact that
# settled it. It did not name that engine's HEAD, so "which engine, exactly"
# needed a second command. It does now, and that is the whole change.
#
# WHAT IS DELIBERATELY NOT HERE: a comparison against installed_plugins.json.
# It was written and removed. It needs a JSON parser inside a SessionStart
# hook, and it differs LEGITIMATELY in every engine-development session — a
# worktree of the engine is not the installed copy — which is the population
# that reads this banner most, so it would be wallpaper within a day. The
# failure it would guard against was not a missing warning; it was reasoning
# from a record without establishing that anything reads it. The running path
# and its HEAD, stated plainly, is the answer to that.
ENGINE_HEAD="$(git -C "$ENGINE_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo 'not-a-git-checkout')"
GUARD_NOTE="${GUARD_NOTE} Engine HEAD ${ENGINE_HEAD} (this is the path that RUNS; an installation record naming another path is not evidence until something is shown to read it)."

resolve_entity_root ""
RC=$?

# ===========================================================================
# A DECLARED LAND-LEASE HOLDER THAT WILL NOT BE FOUND
# ===========================================================================
# 2026-09-30 21:58. The ChatGPT app updated itself and moved Codex out from
# under the path LAND_LEASE_HOLDERS declared. Nothing said so; the first anyone
# heard of it was Codex's commits being refused in every fenced checkout. The
# fence now recognizes a holder moved inside its own app bundle by name and
# signing team (operator_fences.py), and this line names a declared holder that
# is gone at session start, before a land needs it. Asked only when the entity's
# own launcher is on (operator-mode.sh reads it with shell builtins, the way the
# operator hooks do), and of that launcher alone: every launcher is installed
# from the one declaration. This banner reads adoption, never config values.
_OM_LIB="$SCRIPT_DIR/../lib/operator-mode.sh"
if [ "${RICHOS_ROOT_STATUS:-}" = "governed" ] && [ -n "${RICHOS_ENTITY_ROOT_RESOLVED:-}" ] \
   && [ -f "$_OM_LIB" ] && [ -f "$ENGINE_ROOT/scripts/lib/operator_fences.py" ]; then
    # shellcheck source=../lib/operator-mode.sh
    . "$_OM_LIB"
    if operator_mode_on "$RICHOS_ENTITY_ROOT_RESOLVED"; then
        FENCE_HOLDERS="$(python3 "$ENGINE_ROOT/scripts/lib/operator_fences.py" admin holders-notice \
            --repo "$RICHOS_ENTITY_ROOT_RESOLVED" 2>/dev/null | tr '\n' ' ')" || FENCE_HOLDERS=""
        if [ -n "${FENCE_HOLDERS// /}" ]; then
            GUARD_NOTE="${GUARD_NOTE} OPERATOR FENCE, LAND-LEASE HOLDERS: ${FENCE_HOLDERS% }"
        fi
    fi
fi

# ===========================================================================
# THE SPAWN INSTRUCTION — ONE COMMAND, NOT FOUR
# ===========================================================================
# 2026-09-13. `scripts/spawn.sh` landed the night before and does the whole
# thing in one command, evaluating every PreToolUse[Agent] guard from every
# surface and reporting ALL failures together before anything is created. It
# was not used. The reason was here: this string — the first instruction every
# session reads — still named the four-step path and had never heard of
# spawn.sh. Following it that night cost 130 s and two guard refusals that
# spawn.sh would have reported together, before a workspace existed. The CEO,
# for the fourth time: "Spinning up the first agent in a new session still
# takes over 3 minutes! I thought that fuckshit was supposed to have been fixed
# at the end of the last session?"
#
# A tool nobody is told to use is a tool that does not exist. So the one
# command is the INSTRUCTION and the old path is named only as a fallback.
#
# WHAT DELIBERATELY DID NOT CHANGE: what any guard ACCEPTS. The four-step path
# still works and is still allowed — this is a change to what we TELL people,
# never to what is permitted. engine-status.test.sh case 7 fails if this string
# stops naming spawn.sh, or names prepare-agent-spawn.py ahead of it.
# ===========================================================================
case "$RICHOS_ROOT_STATUS" in
    governed)
        emit_context \
            "RichOS engine ${VERSION} ACTIVE. Engine: ${ENGINE_ROOT}. Governing: ${RICHOS_ENTITY_ROOT_RESOLVED} (resolved via ${RICHOS_ROOT_SOURCE}). ${GUARD_COUNT}/${GUARD_EXPECTED} guards present (denominator derived from the engine's hooks/hooks.json registration; the status announcer itself is not counted among the guards). Enforcement is ON for this repository.${GUARD_NOTE} STARTING ONE TEAMMATE IS ONE COMMAND: '${ENGINE_ROOT}/scripts/spawn.sh' <teammate-name> --repo <repo> --type <subagent-type> --brief <file> [--model <alias>] (--repo takes an absolute path OR an unambiguous bare repository name). It resolves the repository and the branch this work integrates on, creates and REGISTERS the isolated workspace — including a cross-repository one, with its cross-repo-worktree: <registered-path> prompt line — and assembles the complete Agent payload, supplying native isolation and the required durable inflight acknowledgement contract, including for readers in worktrees. It then evaluates EVERY PreToolUse[Agent] guard from EVERY surface — this engine's hooks/hooks.json AND the governed repository's .claude/settings*.json — against that payload and reports ALL failures together, BEFORE anything is created; on refusal it exits 1 and leaves nothing behind. The finished payload goes to stdout: review it, then make the Agent call with it. Keep isolation:worktree; a cwd-only spawn is refused. FALLBACK ONLY, for something spawn.sh cannot do (it is still accepted and no guard has changed): python3 '${ENGINE_ROOT}/scripts/prepare-agent-spawn.py' --file <input.json>, plus '${ENGINE_ROOT}/mega-lander/create-teammate-worktree.sh' <repo> <teammate-name> and a cross-repo-worktree: <registered-path> prompt line for cross-repository work — four steps instead of one, with every guard evaluated only at the tool call, one refusal per attempt. These are engine paths, not scripts inside the governed repository. Neither helper grants any guard exemption; review the payload before dispatch." \
            "RichOS engine ${VERSION}: ENFORCEMENT ACTIVE for ${RICHOS_ENTITY_ROOT_RESOLVED} (${GUARD_COUNT}/${GUARD_EXPECTED} guards, engine at ${ENGINE_ROOT}, root via ${RICHOS_ROOT_SOURCE}).${GUARD_NOTE}"
        ;;
    engine-self)
        # THE NESTED-ENGINE CASE. When the engine is a subdirectory of a repo
        # that never adopted, "governing ITSELF" is true and dangerously
        # incomplete: it does not say that everything ELSE in the enclosing
        # repository is unprotected. richos read this banner as ENFORCEMENT
        # ACTIVE for a day while its product tree (richos/app/) took writes with
        # no guard at all. Name the tree, or the banner is reassurance.
        ENCL_MODEL=""
        ENCL_OP=""
        if [ -n "${RICHOS_ROOT_UNGOVERNED_ENCLOSING:-}" ]; then
            ENCL_MODEL=" UNPROTECTED: the engine is NESTED inside ${RICHOS_ROOT_UNGOVERNED_ENCLOSING}, which has NOT adopted it. Every protected path is resolved against ${RICHOS_ENTITY_ROOT_RESOLVED}, so NOTHING under ${RICHOS_ROOT_UNGOVERNED_ENCLOSING} outside the engine is guarded — not its source trees, not its main-checkout writes. Adopt it by committing an orchestration.config at its root."
            ENCL_OP=" UNPROTECTED: ${RICHOS_ROOT_UNGOVERNED_ENCLOSING} has NOT adopted the engine — nothing in it outside ${RICHOS_ENTITY_ROOT_RESOLVED} is guarded."
        fi
        emit_context \
            "RichOS engine ${VERSION} ACTIVE — governing ITSELF. Engine: ${ENGINE_ROOT}. Governing: ${RICHOS_ENTITY_ROOT_RESOLVED}. ${GUARD_COUNT}/${GUARD_EXPECTED} guards present (denominator derived from the engine's hooks/hooks.json registration; the status announcer itself is not counted among the guards).${GUARD_NOTE} NOTE: no repository in this session's candidate chain carries orchestration.config, so the guards are acting on the engine's own tree rather than on the session's project directory. That is correct when you are developing the engine and wrong for anything else.${ENCL_MODEL}" \
            "RichOS engine ${VERSION}: ENFORCEMENT ACTIVE, governing ITSELF at ${ENGINE_ROOT} (${GUARD_COUNT}/${GUARD_EXPECTED} guards). No repository in this session's candidate chain carries orchestration.config — right when you are developing the engine, wrong for anything else.${ENCL_OP}${GUARD_NOTE}"
        ;;
    not-adopted)
        # Loud enough to be seen, calm enough not to be noise: this is the
        # normal state in every repository that has not adopted the engine, and
        # the engine loads in all of them.
        emit_context \
            "RichOS engine ${VERSION} loaded but STOOD DOWN — this repository has NOT adopted it. Engine: ${ENGINE_ROOT}. No orchestration.config was found at any candidate root, so NONE of the ${GUARD_COUNT}/${GUARD_EXPECTED} guards will enforce anything in this session: no worktree-isolation contract, no main-checkout write protection, no secret scanning, no definition-drift check. This is a stand-down, not a pass. To adopt, commit an orchestration.config at this repository's root.${GUARD_NOTE}" \
            "RichOS engine ${VERSION}: STOOD DOWN — this repository has not adopted the engine, so NONE of its ${GUARD_COUNT}/${GUARD_EXPECTED} guards will enforce anything in this session. This is a stand-down, not a pass. To adopt, commit an orchestration.config at the repository root.${GUARD_NOTE}"
        ;;
    *)
        BANNER="$(root_failure_banner "scripts/hooks/engine-status.sh")"
        printf '%s\n' "$BANNER" >&2
        # Not "every guard will refuse": the blocking guards refuse, but the
        # advisory notices deliberately allow the call and only say they are
        # off (a Stop hook that blocked here would strand the session). The
        # sentence says both, because both are true (hunt part 3 v3, finding 29).
        emit_context \
            "RichOS engine ${VERSION}: ROOT RESOLUTION FAILURE — ENFORCEMENT IS NOT ACTIVE. ${RICHOS_ROOT_REASON} The blocking guards in this session will refuse rather than guess; the advisory notices will let calls through and only say they are off. Fix the root declaration before doing any work that depends on enforcement." \
            "RichOS engine ${VERSION}: ROOT RESOLUTION FAILURE — ENFORCEMENT IS NOT ACTIVE. ${RICHOS_ROOT_REASON} The blocking guards will refuse rather than guess; the advisory notices let calls through and only say they are off."
        ;;
esac

exit 0

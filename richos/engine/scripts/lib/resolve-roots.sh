#!/usr/bin/env bash
#
# scripts/lib/resolve-roots.sh — THE ROOT-RESOLUTION CONTRACT.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# Every guard under scripts/hooks/ used to resolve exactly ONE thing it called
# "the repo root", from its own on-disk location:
#
#     SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#     REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
#
# That is correct only while the engine IS the repository it governs. The
# moment the engine is loaded BY REFERENCE — as a Claude Code plugin, from a
# directory nested inside some other repository — the single "repo root" has to
# mean two different things at once, and it silently picks the wrong one:
#
#   * `SCRIPT_DIR/../..`                       -> the ENGINE directory
#   * `resolve_main_checkout "$SCRIPT_DIR"`    -> the repository ENCLOSING the
#                                                 engine (one level too high)
#   * neither                                  -> the session's own repository
#
# Measured, not theorized (step-1 audit, 2026-08-28): with the engine loaded as
# a plugin, `guard-worktree-isolation.sh` found the ENGINE's config and carried
# on with the wrong entity's values; `session-start-reap-worktrees.sh` and
# `snapshot-agent-definitions.sh` resolved to the enclosing repository, found
# nothing, and SILENTLY SKIPPED; and `guard-definition-drift.sh` WROTE ITS
# STATE LOG INTO A REPOSITORY THAT HAD NOTHING TO DO WITH THE SESSION.
#
# ===========================================================================
# THE CONTRACT — two roots, never one
# ===========================================================================
#
#   ENGINE_ROOT  Where the engine's own bytes live. Owns: scripts/lib/*,
#                scripts/reap-stale-worktrees.sh, VERSION, the engine's own
#                .claude/agents/ (its shipped meta-roles), hooks/hooks.json.
#                READ-ONLY at run time. NEVER a write target unless it also
#                happens to BE the entity root.
#                Resolution: $RICHOS_ENGINE_ROOT -> $CLAUDE_PLUGIN_ROOT ->
#                            <script dir>/../..   (always succeeds; the script
#                            is by definition on disk)
#
#   ENTITY_ROOT  The MAIN checkout of the repository this session governs.
#                Owns: orchestration.config, .claude/agents/, .claude/state/,
#                .claude/worktrees/, .claude/settings*.json, and every
#                PROTECTED_PATHS tree. This is the ONLY legitimate write target
#                and the ONLY source of per-project configuration.
#                Resolution: candidates, in order —
#                  1. $RICHOS_ENTITY_ROOT   (explicit; also the test affordance)
#                  2. $CLAUDE_PROJECT_DIR   (host-set; VERIFIED to be the
#                                            session's project dir even when the
#                                            hook's code lives in a plugin
#                                            elsewhere — probe, 2026-08-28)
#                  3. the hook payload's `cwd`
#                  4. $PWD
#                A candidate that is the TOP LEVEL of a working tree is
#                normalized to its MAIN checkout (so a linked worktree resolves
#                to the shared checkout). A candidate that is a SUBDIRECTORY of
#                a repository stands for ITSELF first — normalizing it would
#                discard the nesting, which is precisely how an engine at
#                <repo>/engine ends up governing <repo> — and only falls back to
#                its enclosing checkout if it carries no marker of its own.
#                A candidate is ACCEPTED only if it carries the adoption marker.
#
#   ADOPTION MARKER  `orchestration.config` at the root. That file is the one
#                thing an adopter writes to point the engine at their repo, so
#                its presence is exactly the statement "this repository has
#                adopted the engine". Nothing is inferred; adoption is declared.
#
# ===========================================================================
# THE GOVERNING RULE — fail LOUD, never skip
# ===========================================================================
# A guard that cannot resolve its root must FAIL LOUD, never silently skip. A
# silent skip is worse than no guard at all, because it buys false confidence.
#
# But "block everything you cannot resolve" is not the same rule, and applying
# it literally would brick every session in every directory on the machine that
# has not adopted the engine — the plugin is enabled at USER scope, so it loads
# in EVERY project. So the resolver distinguishes two cases that the old code
# could not even ask about, and they get different treatment:
#
#   NOT-ADOPTED  No candidate carries the marker. The engine is loaded but this
#                repository never adopted it. There is no protection to lose,
#                because there was never any here. Guards STAND DOWN — and the
#                stand-down is ANNOUNCED, every session, by engine-status.sh,
#                into the orchestrator's own context. Never silent.
#
#   BROKEN       Someone declared a root (RICHOS_ENTITY_ROOT) that is not an
#                engine root, or a governed root is missing an asset the guard
#                needs to do its job. The guard believes it is governing
#                something and cannot. This BLOCKS (exit 2) in every blocking
#                hook and SCREAMS in every non-blocking one.
#
# The distinction the old code could not draw is exactly "not applicable" vs
# "applicable but broken". Every silent skip this contract replaces was the
# second case wearing the first case's clothes.
#
# ===========================================================================
# STATUS IS ALWAYS OBSERVABLE
# ===========================================================================
# After resolve_entity_root, these are set for the caller:
#
#   RICHOS_ENTITY_ROOT_RESOLVED  the root itself (empty unless rc 0)
#   RICHOS_ROOT_STATUS   governed | engine-self | not-adopted | broken
#   RICHOS_ROOT_SOURCE   which candidate won (env-override|project-dir|payload-cwd|pwd|engine-self)
#   RICHOS_ROOT_TRIED    tab-separated list of candidates actually examined
#   RICHOS_ROOT_REASON   human-readable explanation when status != governed
#
# `engine-self` is the engine developing itself: no candidate carried the
# marker, but the ENGINE_ROOT does and it sits at or under the first candidate.
# It is a legitimate configuration (open a session at the repo that contains
# the engine) and it is REPORTED AS ITS OWN STATUS rather than being quietly
# folded into `governed`, because it is the one case where the guards act on a
# root that is not the session's own project dir.
#
# ===========================================================================
# USAGE
# ===========================================================================
#     SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#     . "$SCRIPT_DIR/../lib/resolve-roots.sh"
#
#     ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"
#     if resolve_entity_root "$PAYLOAD"; then
#         ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"   # governed or engine-self
#     else
#         case "$RICHOS_ROOT_STATUS" in
#           not-adopted) exit 0 ;;                       # stand down, announced elsewhere
#           broken)      root_failure_banner "<hook>" >&2; exit 2 ;;   # blocking hooks
#         esac
#     fi
#
# resolve_entity_root DELIBERATELY returns its answer in a VARIABLE, not on
# stdout. A `$(...)` capture runs in a subshell, so every status variable it
# sets would be discarded at exactly the moment the caller needs it — and the
# caller would then branch on an EMPTY status, i.e. silently take the wrong
# path. That is the same class of quiet failure this whole contract exists to
# remove, so the API makes it unrepresentable rather than documenting it.
#
# Safe to source repeatedly. Never mutates state, never changes the caller's cwd.

if [ -n "${_RESOLVE_ROOTS_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_RESOLVE_ROOTS_SH_SOURCED=1

# The adoption marker. A repository "has adopted the engine" iff this file
# exists at its main-checkout root. One marker, checked one way, everywhere.
: "${RICHOS_ADOPTION_MARKER:=orchestration.config}"

# This file's own directory, captured at source time. It is <engine>/scripts/lib,
# so <that>/../.. is the engine root — resolvable with no cooperation from the
# caller, which matters because resolve_entity_root needs the engine root for
# its engine-self branch and must not depend on the caller passing an anchor.
_RR_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# NOTE ON BASH 3.2: macOS ships bash 3.2.57, which is what `#!/usr/bin/env bash`
# resolves to on this platform. No associative arrays, and `"$@"` is unbound
# under `set -u` when empty. Both constraints are respected below.

# --- dependency: main-checkout normalization ------------------------------
# resolve_main_checkout lives in its own file because it predates this one and
# is sourced directly by scripts outside scripts/hooks/. Source it if it is not
# already present; degrade to identity if it is genuinely absent (a test
# sandbox), which reproduces pre-contract behavior exactly.
if ! command -v resolve_main_checkout >/dev/null 2>&1; then
    _RR_SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ -f "$_RR_SELF_DIR/resolve-main-checkout.sh" ]; then
        # shellcheck source=./resolve-main-checkout.sh
        . "$_RR_SELF_DIR/resolve-main-checkout.sh" 2>/dev/null || true
    fi
    unset _RR_SELF_DIR
fi
if ! command -v resolve_main_checkout >/dev/null 2>&1; then
    resolve_main_checkout() { printf '%s\n' "${2:-$1}"; }
fi

# ---------------------------------------------------------------------------
# resolve_engine_root [anchor_dir]
# ---------------------------------------------------------------------------
# Prints the engine root. ALWAYS succeeds (rc 0): the calling script is on
# disk, so a location-derived answer always exists.
#
# $CLAUDE_PLUGIN_ROOT is preferred over the location-derived answer ONLY when
# it actually looks like this engine (it carries scripts/hooks/). The host sets
# that variable per-plugin, so a hook belonging to some OTHER plugin can never
# hijack it, but the check costs nothing and makes the failure mode "fall back
# to the location-derived root" rather than "point at a stranger's directory".
resolve_engine_root() {
    local anchor="${1:-}" derived=""
    if [ -n "$anchor" ]; then
        derived="$( (cd "$anchor/../.." 2>/dev/null && pwd) || true )"
    fi
    [ -z "$derived" ] && derived="$PWD"

    if [ -n "${RICHOS_ENGINE_ROOT:-}" ]; then
        printf '%s\n' "$RICHOS_ENGINE_ROOT"
        return 0
    fi
    if [ -n "${CLAUDE_PLUGIN_ROOT:-}" ] && [ -d "${CLAUDE_PLUGIN_ROOT}/scripts/hooks" ]; then
        ( cd "$CLAUDE_PLUGIN_ROOT" 2>/dev/null && pwd ) && return 0
    fi
    printf '%s\n' "$derived"
    return 0
}

# ---------------------------------------------------------------------------
# _rr_payload_cwd <payload_json>
# ---------------------------------------------------------------------------
# Extracts `cwd` from a hook payload. Empty on anything unparseable — a missing
# cwd is simply one candidate fewer, never an error, because three other
# candidates remain and the marker check is what actually decides.
_rr_payload_cwd() {
    local payload="${1:-}"
    [ -n "$payload" ] || return 0
    command -v python3 >/dev/null 2>&1 || return 0
    printf '%s' "$payload" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(str(d.get("cwd", "") or "") if isinstance(d, dict) else "")
except Exception:
    print("")
' 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# resolve_entity_root [payload_json]
# ---------------------------------------------------------------------------
# Sets RICHOS_ENTITY_ROOT_RESOLVED (and the status variables) and returns:
#
#   rc 0  RICHOS_ROOT_STATUS = governed | engine-self   -> enforce
#   rc 1  RICHOS_ROOT_STATUS = not-adopted              -> stand down (announced)
#   rc 2  RICHOS_ROOT_STATUS = broken                   -> FAIL LOUD
#
# Returns via a VARIABLE, never stdout — see the USAGE note in the header for
# why a `$(...)` capture would silently discard the status the caller branches
# on.
#
# THE REAL WORK IS HERE; `resolve_entity_root` BELOW IS A MEMOIZING WRAPPER.
# Nothing in this function changed when the wrapper was added, and the wrapper
# is INERT unless a caller sets _RR_MEMO_ENABLE=1. Read the wrapper's own
# comment for why one caller wants it and why nobody else should.
_rr_resolve_entity_root_uncached() {
    local payload="${1:-}"
    local cand norm src
    local cands="" srcs=""

    RICHOS_ENTITY_ROOT_RESOLVED=""
    RICHOS_ROOT_STATUS=""
    RICHOS_ROOT_SOURCE=""
    RICHOS_ROOT_TRIED=""
    RICHOS_ROOT_REASON=""

    local engine_root
    engine_root="$(resolve_engine_root "$_RR_LIB_DIR")"

    # _rr_try <path> <source-label> — record and accept if adopted.
    # Sets RICHOS_ENTITY_ROOT_RESOLVED and returns 0 on acceptance.
    #
    # THE SUBTLETY, and it is not optional. "Normalize to the main checkout"
    # is right for one shape and WRONG for another, and the two are easy to
    # confuse because both are "a path inside a git repository":
    #
    #   C is the TOP LEVEL of a working tree
    #       -> it is either the main checkout (normalizes to itself) or a
    #          LINKED WORKTREE (normalizes to the shared checkout, which is
    #          what state, the worktree registry and "the shared checkout"
    #          all have to mean). Normalize.
    #
    #   C is a SUBDIRECTORY of a repository
    #       -> e.g. the engine at <repo>/engine. Normalizing walks up to
    #          <repo> and silently DISCARDS the nesting, which is exactly how
    #          a nested engine ends up governing its enclosing repository. So
    #          C stands for itself first; only if C carries no marker do we
    #          look upward, and then it is the "session cwd was a subdirectory
    #          of the entity" case, which is a legitimate second chance rather
    #          than a substitution.
    _rr_try() {
        local c="$1" s="$2" top abs n
        if [ ! -d "$c" ]; then
            RICHOS_ROOT_TRIED="${RICHOS_ROOT_TRIED}${c} (${s}: not a directory)"$'\t'
            return 1
        fi
        abs="$( (cd "$c" && pwd) 2>/dev/null || printf '%s' "$c" )"
        top="$(git -C "$abs" rev-parse --show-toplevel 2>/dev/null || true)"
        [ -n "$top" ] && top="$( (cd "$top" && pwd) 2>/dev/null || printf '%s' "$top" )"

        if [ -n "$top" ] && [ "$top" = "$abs" ]; then
            n="$(resolve_main_checkout "$abs" "$abs" 2>/dev/null || printf '%s' "$abs")"
            RICHOS_ROOT_TRIED="${RICHOS_ROOT_TRIED}${n} (${s})"$'\t'
            if [ -f "$n/$RICHOS_ADOPTION_MARKER" ]; then
                RICHOS_ENTITY_ROOT_RESOLVED="$n"; RICHOS_ROOT_SOURCE="$s"; return 0
            fi
            return 1
        fi

        RICHOS_ROOT_TRIED="${RICHOS_ROOT_TRIED}${abs} (${s})"$'\t'
        if [ -f "$abs/$RICHOS_ADOPTION_MARKER" ]; then
            RICHOS_ENTITY_ROOT_RESOLVED="$abs"; RICHOS_ROOT_SOURCE="$s"; return 0
        fi
        if [ -n "$top" ]; then
            n="$(resolve_main_checkout "$top" "$top" 2>/dev/null || printf '%s' "$top")"
            RICHOS_ROOT_TRIED="${RICHOS_ROOT_TRIED}${n} (${s}: enclosing checkout)"$'\t'
            if [ -f "$n/$RICHOS_ADOPTION_MARKER" ]; then
                RICHOS_ENTITY_ROOT_RESOLVED="$n"; RICHOS_ROOT_SOURCE="$s"; return 0
            fi
        fi
        return 1
    }

    # --- candidate 1, and it is EXCLUSIVE -----------------------------------
    # An explicit declaration is an INTENT, not a hint. If RICHOS_ENTITY_ROOT
    # names something that is not an adopted engine root, falling through to
    # the next candidate would silently govern a DIFFERENT repository than the
    # one that was named — substituting a root behind the operator's back is
    # exactly the failure this contract exists to end. So: accept it, or fail.
    if [ -n "${RICHOS_ENTITY_ROOT:-}" ]; then
        if _rr_try "$RICHOS_ENTITY_ROOT" "env-override"; then
            RICHOS_ROOT_STATUS="governed"
            return 0
        fi
        RICHOS_ROOT_STATUS="broken"
        RICHOS_ROOT_SOURCE="env-override"
        RICHOS_ROOT_REASON="RICHOS_ENTITY_ROOT is set to '${RICHOS_ENTITY_ROOT}', but no ${RICHOS_ADOPTION_MARKER} was found at its main checkout. An explicitly declared root MUST be an adopted engine root; the resolver will NOT quietly substitute a different one."
        return 2
    fi

    # --- candidates 2-4, in precedence order --------------------------------
    # Candidate 2: the host's own answer to "what project is this session in?".
    # Verified present and correct inside plugin-loaded hooks (probe 2026-08-28).
    if [ -n "${CLAUDE_PROJECT_DIR:-}" ] && _rr_try "$CLAUDE_PROJECT_DIR" "project-dir"; then
        RICHOS_ROOT_STATUS="governed"; return 0
    fi
    # Candidate 3: the payload's cwd — the subagent case, and any host that
    # does not export the env var.
    cand="$(_rr_payload_cwd "$payload")"
    if [ -n "$cand" ] && _rr_try "$cand" "payload-cwd"; then
        RICHOS_ROOT_STATUS="governed"; return 0
    fi
    # Candidate 4: last resort.
    if _rr_try "$PWD" "pwd"; then
        RICHOS_ROOT_STATUS="governed"; return 0
    fi

    # --- the engine developing itself ---------------------------------------
    # No session candidate carried the marker, but ENGINE_ROOT does and it sits
    # UNDER the session's project dir. Legitimate (this is how the engine is
    # developed in place) and reported under its own status, because it is the
    # one case where the guards act on a root that is not the session's own.
    if [ -f "$engine_root/$RICHOS_ADOPTION_MARKER" ]; then
        local anchor="${CLAUDE_PROJECT_DIR:-$PWD}"
        case "$engine_root/" in
            "$anchor"/*)
                RICHOS_ENTITY_ROOT_RESOLVED="$engine_root"
                RICHOS_ROOT_STATUS="engine-self"
                RICHOS_ROOT_SOURCE="engine-self"
                RICHOS_ROOT_REASON="No session candidate carried ${RICHOS_ADOPTION_MARKER}; the engine's own root (${engine_root}) does, and lies under the session project dir (${anchor}). Governing the engine itself."

                # THE NESTED-ENGINE TRAP, named rather than hidden.
                #
                # `engine-self` is correct when the engine IS the repository. It
                # is a TRAP when the engine is a SUBDIRECTORY of a larger product
                # repository that never adopted: the seat silently becomes the
                # engine subdir, every PROTECTED_PATHS entry is joined onto it,
                # and the guards end up defending <engine>/app while the real
                # <repo>/app sits open. Measured in richos on 2026-08-30 — a
                # write to richos/engine/app/ (WHICH DOES NOT EXIST) was blocked
                # and a write to richos/app/ was allowed.
                #
                # The status still resolves and still returns 0: demoting it
                # would strip enforcement from every live session in a nested
                # setup, which trades a false green for a real hole. What changes
                # is that the enclosing repository is now NAMED, so the announcer
                # can say which tree is unprotected instead of implying none is.
                RICHOS_ROOT_UNGOVERNED_ENCLOSING=""
                local _rr_encl
                _rr_encl="$(git -C "$anchor" rev-parse --show-toplevel 2>/dev/null || true)"
                if [ -n "$_rr_encl" ]; then
                    _rr_encl="$( (cd "$_rr_encl" && pwd) 2>/dev/null || printf '%s' "$_rr_encl" )"
                    if [ "$_rr_encl" != "$engine_root" ] && \
                       [ ! -f "$_rr_encl/$RICHOS_ADOPTION_MARKER" ]; then
                        case "$engine_root/" in
                            "$_rr_encl"/*)
                                RICHOS_ROOT_UNGOVERNED_ENCLOSING="$_rr_encl"
                                RICHOS_ROOT_REASON="${RICHOS_ROOT_REASON} WARNING: the engine is NESTED inside ${_rr_encl}, which has NOT adopted the engine. Every protected path is resolved against ${engine_root}, so nothing under ${_rr_encl} outside the engine is protected."
                                ;;
                        esac
                    fi
                fi
                return 0
                ;;
        esac
    fi

    RICHOS_ROOT_STATUS="not-adopted"
    RICHOS_ROOT_SOURCE=""
    RICHOS_ROOT_REASON="No candidate root carries ${RICHOS_ADOPTION_MARKER}, so this repository has not adopted the engine. Guards stand down here; nothing is being enforced. Adopt by committing an ${RICHOS_ADOPTION_MARKER} at the repository root."
    return 1
}

# ---------------------------------------------------------------------------
# resolve_entity_root [payload_json]   — the public entry point
# ---------------------------------------------------------------------------
# Identical to _rr_resolve_entity_root_uncached in every observable way, with
# one addition that is OFF BY DEFAULT: when _RR_MEMO_ENABLE is set to 1, the
# answer to a repeated identical question is served from the previous answer
# instead of being recomputed.
#
# WHY THIS EXISTS. Answering "which repository does this call govern?" costs
# one python3 fork and up to four git forks — measured 2026-09-15 at 14.64 ms
# and 4.23 ms respectively on macOS 24.6.0. Twelve PreToolUse[Bash] guards each
# asked it, about ONE payload, on every shell call: 12 python3 and 48 git forks
# to reach the same answer twelve times. scripts/hooks/dispatch-pretooluse.sh
# now runs those twelve as subshells of one process, so it can ask once, up
# front, and let every rule inherit the answer.
#
# WHY IT IS OFF BY DEFAULT, AND MUST STAY OFF. The answer depends on the
# FILESYSTEM — which directories exist and which carry the adoption marker —
# and the memo key cannot see the filesystem. Inside one tool call that is
# safe: the call has not run yet, so nothing it does can have moved a root.
# Across a long-lived process it is not: a test suite that builds a fixture
# tree, asks, rebuilds it differently and asks again would be handed the first
# answer. root-contract.test.sh does exactly that. So the memo is opt-in, one
# caller opts in, and every existing caller is byte-for-byte unaffected.
#
# The key carries everything other than the filesystem that can change the
# answer: the payload, the working directory, and the three environment
# variables the resolution consults.
resolve_entity_root() {
    local _rr_key
    if [ "${_RR_MEMO_ENABLE:-0}" != "1" ]; then
        _rr_resolve_entity_root_uncached "${1:-}"
        return $?
    fi
    _rr_key="${1:-}"$'\x1f'"$PWD"$'\x1f'"${RICHOS_ENTITY_ROOT:-}"$'\x1f'"${RICHOS_ENGINE_ROOT:-}"$'\x1f'"${CLAUDE_PROJECT_DIR:-}"$'\x1f'"${CLAUDE_PLUGIN_ROOT:-}"
    if [ "${_RR_MEMO_KEY+set}" = "set" ] && [ "$_RR_MEMO_KEY" = "$_rr_key" ]; then
        RICHOS_ENTITY_ROOT_RESOLVED="$_RR_MEMO_ROOT"
        RICHOS_ROOT_STATUS="$_RR_MEMO_STATUS"
        RICHOS_ROOT_SOURCE="$_RR_MEMO_SOURCE"
        RICHOS_ROOT_TRIED="$_RR_MEMO_TRIED"
        RICHOS_ROOT_REASON="$_RR_MEMO_REASON"
        RICHOS_ROOT_UNGOVERNED_ENCLOSING="$_RR_MEMO_ENCLOSING"
        return "$_RR_MEMO_RC"
    fi
    _rr_resolve_entity_root_uncached "${1:-}"
    _RR_MEMO_RC=$?
    _RR_MEMO_KEY="$_rr_key"
    _RR_MEMO_ROOT="${RICHOS_ENTITY_ROOT_RESOLVED:-}"
    _RR_MEMO_STATUS="${RICHOS_ROOT_STATUS:-}"
    _RR_MEMO_SOURCE="${RICHOS_ROOT_SOURCE:-}"
    _RR_MEMO_TRIED="${RICHOS_ROOT_TRIED:-}"
    _RR_MEMO_REASON="${RICHOS_ROOT_REASON:-}"
    _RR_MEMO_ENCLOSING="${RICHOS_ROOT_UNGOVERNED_ENCLOSING:-}"
    return "$_RR_MEMO_RC"
}

# ---------------------------------------------------------------------------
# root_failure_banner <hook-name> [extra-line ...]
# ---------------------------------------------------------------------------
# The loud failure. Prints to STDOUT so a blocking caller can redirect it to
# stderr and a logging caller can fold it into its own output. Unmistakable by
# construction: a fixed, greppable banner string, the status, every candidate
# that was examined, and the reason.
root_failure_banner() {
    local hook="${1:-<unknown hook>}"; shift || true
    echo "=== RICHOS ENGINE: ROOT RESOLUTION FAILURE — ENFORCEMENT IS NOT ACTIVE ==="
    echo "  hook   : $hook"
    echo "  status : ${RICHOS_ROOT_STATUS:-<unset>}"
    echo "  reason : ${RICHOS_ROOT_REASON:-<unset>}"
    if [ -n "${RICHOS_ROOT_TRIED:-}" ]; then
        echo "  candidates examined (in order):"
        printf '%s' "$RICHOS_ROOT_TRIED" | tr '\t' '\n' | while IFS= read -r line; do
            [ -n "$line" ] && echo "    - $line"
        done
    fi
    local extra
    if [ "$#" -gt 0 ]; then
        for extra in "$@"; do
            [ -n "$extra" ] && echo "  $extra"
        done
    fi
    echo "  This is a HARD failure, not a skip. A guard that cannot resolve its"
    echo "  root must never carry on quietly — that is how a defense reports 'on'"
    echo "  while protecting nothing."
    echo "=========================================================================="
}

# ---------------------------------------------------------------------------
# require_asset <path> <hook-name> <what-it-is>
# ---------------------------------------------------------------------------
# Asserts that an asset a governed guard genuinely needs is present. Prints the
# loud banner and returns 1 when it is not. Callers decide block vs scream; the
# one thing neither may do is continue quietly.
require_asset() {
    local path="$1" hook="$2" what="$3"
    [ -e "$path" ] && return 0
    RICHOS_ROOT_STATUS="broken"
    RICHOS_ROOT_REASON="${what} is missing at '${path}'. The root resolved, so this guard believes it is governing this repository — but it cannot do its job."
    root_failure_banner "$hook"
    return 1
}

# ---------------------------------------------------------------------------
# strip_agent_namespace <subagent_type>
# ---------------------------------------------------------------------------
# `richos-engine:clark` -> `clark`;  `clark` -> `clark`.
# A plugin-supplied agent type is namespaced `<plugin-name>:<role>`. Every
# guard that looks up `.claude/agents/<type>.md` must strip the namespace or it
# stats a path that can never exist and degrades to fail-open.
strip_agent_namespace() {
    local t="${1:-}"
    printf '%s' "${t##*:}"
}

# ---------------------------------------------------------------------------
# agent_namespace <subagent_type>
# ---------------------------------------------------------------------------
# `richos-engine:clark` -> `richos-engine`;  `clark` -> `` (empty).
agent_namespace() {
    local t="${1:-}"
    case "$t" in
        *:*) printf '%s' "${t%%:*}" ;;
        *)   printf '' ;;
    esac
}

# ---------------------------------------------------------------------------
# engine_plugin_name <engine_root>
# ---------------------------------------------------------------------------
# The `name` from the engine's own plugin manifest — i.e. the namespace the
# host prefixes onto this engine's agent types. Read, never guessed. Empty when
# the engine is not packaged as a plugin.
engine_plugin_name() {
    local engine_root="${1:-}" manifest="${1:-}/.claude-plugin/plugin.json"
    [ -f "$manifest" ] || { printf ''; return 0; }
    command -v python3 >/dev/null 2>&1 || { printf ''; return 0; }
    python3 -c '
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        print(str(json.load(f).get("name", "") or ""))
except Exception:
    print("")
' "$manifest" 2>/dev/null || printf ''
}

# ---------------------------------------------------------------------------
# resolve_agent_def <entity_root> <engine_root> <subagent_type>
# ---------------------------------------------------------------------------
# Prints the path of the LIVE agent definition backing a (possibly namespaced)
# subagent_type, or nothing.
#
#   rc 0  found; path on stdout
#   rc 1  not namespaced and not found  — legitimate for host built-ins
#         (general-purpose, Explore, ...), which have no definition file at all.
#         Callers keep their existing "undeterminable -> accept" behavior.
#   rc 2  NAMESPACED and not found — the caller must FAIL LOUD. A namespaced
#         type is by construction supplied by a plugin, so its definition MUST
#         be locatable; if it is not, any check that depends on it is silently
#         degrading, which is the failure class this contract exists to kill.
#
# Search order:
#   1. $RICHOS_AGENT_ROSTER/<bare>.md when set (an app lease's own roster), else
#      $ENTITY_ROOT/.claude/agents/<bare>.md            (the entity's own roster)
#   2. $ENGINE_ROOT/.claude/agents/<bare>.md            (only when the namespace
#                                                        IS this engine's plugin
#                                                        name — read from the
#                                                        manifest, never guessed)
#   3. $AGENT_NAMESPACE_ROOTS                           (orchestration.config:
#                                                        space-separated
#                                                        "<namespace>=<root>"
#                                                        pairs for other plugins
#                                                        supplying agent types)
resolve_agent_def() {
    local entity_root="$1" engine_root="$2" stype="$3"
    local bare ns cand pair

    bare="$(strip_agent_namespace "$stype")"
    ns="$(agent_namespace "$stype")"
    [ -n "$bare" ] || return 1

    # A lease's own roster (the app sets RICHOS_AGENT_ROSTER to the folder its plugin carries)
    # is read in place of the entity folder, which every lease of the app shares and which
    # another company's lease may have rendered its definitions into.
    if [ -n "${RICHOS_AGENT_ROSTER:-}" ]; then
        cand="$RICHOS_AGENT_ROSTER/${bare}.md"
    else
        cand="$entity_root/.claude/agents/${bare}.md"
    fi
    if [ -f "$cand" ]; then printf '%s\n' "$cand"; return 0; fi

    if [ -n "$ns" ]; then
        if [ "$ns" = "$(engine_plugin_name "$engine_root")" ]; then
            cand="$engine_root/.claude/agents/${bare}.md"
            if [ -f "$cand" ]; then printf '%s\n' "$cand"; return 0; fi
        fi
        for pair in ${AGENT_NAMESPACE_ROOTS:-}; do
            case "$pair" in
                "$ns"=*)
                    cand="${pair#*=}/.claude/agents/${bare}.md"
                    if [ -f "$cand" ]; then printf '%s\n' "$cand"; return 0; fi
                    ;;
            esac
        done
        return 2
    fi
    return 1
}

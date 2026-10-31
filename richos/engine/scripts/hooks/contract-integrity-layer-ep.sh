#!/usr/bin/env bash
#
# contract-integrity-layer-ep.sh — LAYER EP: THE STANDING INSTRUCTION NAMES THE
#                                  CURRENT COMMAND, OR THIS FAILS.
#
# Sourced by contract-integrity-probe.sh, which owns emit_fail/emit_pass/
# emit_warn, REPO_ROOT and ENGINE_ROOT. Also RUNNABLE ON ITS OWN, because a
# layer that can only be exercised through a 49-minute suite is a layer nobody
# exercises:
#
#     scripts/hooks/contract-integrity-layer-ep.sh [--root <repo>]
#
# Exit 0 pass, 2 fail — the probe's own convention.
#
# ===========================================================================
# WHAT THIS LAYER IS FOR
# ===========================================================================
# Layer MT keeps the CAPABILITY order honest. Layer MC keeps the SPEND order
# honest. This keeps the ROUTING honest: which command a person is told to run.
#
# The three are the same shape on purpose. A fact that sessions were expected to
# remember gets declared once as data, quoted where it is relied on, and a probe
# layer refuses any drift between the prose and the declaration. MT exists
# because an orchestrator inferred a capability order from alias names. This
# exists because a capability shipped, worked, was tested, went green — and the
# announcement every session boots with kept sending traffic to the thing it
# replaced, so the new path was simply never taken. Failure type V, 2026-09-13:
# 130 s and two sequential guard refusals per session against 1.3 s through
# spawn.sh, reported as success every time, because the session did exactly what
# it was told.
#
# WHAT IT ASSERTS
#   1. the parser (scripts/lib/entrypoints.sh) is present — it is the only thing
#      that turns the declaration into a verdict;
#   2. the lint (scripts/entrypoint-currency-lint.sh) is present and executable;
#   3. ENTRYPOINTS is declared and WELL-FORMED — a declaration that does not
#      parse leaves the lint refusing to decide, which is correct behavior and
#      is not a check;
#   4. no standing instruction names a superseded entrypoint without naming its
#      replacement — IN BOTH ROOTS, the governed repository AND the engine,
#      because the file that caused type V is an ENGINE file
#      (scripts/hooks/engine-status.sh) and the normal deployment is
#      by-reference, where REPO_ROOT is the governed product repository and the
#      engine is somewhere else entirely. A single-root scan looked correct in an
#      engine-development session, where the two roots are the same directory,
#      and would have checked everything EXCEPT the announcement in every session
#      that matters. Caught by the integrity suite's own sandbox, where the
#      entity is a fixture with no instruction surfaces at all and the layer
#      passed by having nothing to look at;
#   5. THE LINT STILL WORKS, two-sided, in a sandbox that is nobody's repository:
#      a known-bad instruction is REFUSED naming the file, and the corrected
#      string is ALLOWED. A gutted lint passes half of that; a lint that refuses
#      everything passes the other half; nothing passes both.
#
# WHY A CANARY AND NOT A HASH SIDECAR. Layers MT and MC hash their parser
# against the install manifest. This layer deliberately does not, for two
# reasons. The manifest is an explicit list inside install.sh, so adding an entry
# means the probe reads RED on a freshly merged tree until somebody re-runs
# install.sh — a known trap in this repository, and one that teaches people to
# ignore a red probe. And the protection a hash buys here is strictly weaker than
# the two-sided canary already above it: a hash proves the bytes are unchanged,
# the canary proves the behavior is intact, and only one of those is the thing
# being relied on. The omission is stated rather than hidden.

# EXECUTED or SOURCED, decided the only way that cannot be wrong: is this file
# the thing bash was asked to run? Guessing from "is emit_fail defined" also
# decided whether to PARSE ARGUMENTS, so sourcing this into any shell that
# happened to carry positional parameters made it refuse them as its own — which
# is how its first regression test failed.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_ep_layer_executed=0
case "${BASH_SOURCE[0]}" in
    "$0") _ep_layer_executed=1 ;;
esac

if ! command -v emit_fail >/dev/null 2>&1; then
    FAIL=0
    emit_pass() { printf '  [pass] %s\n' "$1" >&2; }
    emit_fail() { printf '  [FAIL] %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }
    emit_warn() { printf '  [warn] %s\n' "$1" >&2; }
fi

run_layer_EP() {
    # WHERE THE CODE IS and WHICH ROOTS TO SCAN are two different questions, and
    # answering both with ENGINE_ROOT is a bug waiting for the first caller that
    # points that variable somewhere else. The parser and the lint are found from
    # this file's own location, which cannot be wrong; ENGINE_ROOT is used only
    # as a root to READ (its declaration) and to SCAN (its announcement).
    EP_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    EP_ENGINE="${ENGINE_ROOT:-$(cd "$EP_HOME/.." && pwd)}"
    EP_REPO="${REPO_ROOT:-$EP_ENGINE}"
    EP_LIB="$EP_HOME/lib/entrypoints.sh"
    EP_LINT="$EP_HOME/entrypoint-currency-lint.sh"
    EP_OK=1

    if [ ! -f "$EP_LIB" ]; then
        emit_fail "EP. the entrypoint declaration parser is MISSING: $EP_LIB. Nothing turns ENTRYPOINTS into a verdict without it, so no check asks whether a standing instruction still names a replaced command — which is exactly the state that cost 130 s per session on 2026-09-13."
        EP_OK=0
    fi
    if [ "$EP_OK" -eq 1 ] && [ ! -x "$EP_LINT" ]; then
        emit_fail "EP. the entrypoint currency lint is missing or not executable: $EP_LINT. Restore it (git checkout -- scripts/entrypoint-currency-lint.sh) — a capability whose instruction still names the thing it replaced goes unused while every session reports success."
        EP_OK=0
    fi

    # --- the declaration parses ---------------------------------------------
    if [ "$EP_OK" -eq 1 ]; then
        # shellcheck disable=SC1090
        . "$EP_LIB"
        EP_SPEC="${ENTRYPOINTS:-}"
        EP_SPEC_SRC="$EP_REPO/orchestration.config"
        if [ -z "$(printf '%s' "$EP_SPEC" | tr -d '[:space:]')" ]; then
            EP_SPEC="$(grep -E '^[[:space:]]*ENTRYPOINTS=' "$EP_ENGINE/orchestration.config" 2>/dev/null | tail -1 || true)"
            EP_SPEC="${EP_SPEC#*=}"
            EP_SPEC="${EP_SPEC%\"}"
            EP_SPEC="${EP_SPEC#\"}"
            EP_SPEC_SRC="$EP_ENGINE/orchestration.config"
        fi

        if [ -z "$(printf '%s' "$EP_SPEC" | tr -d '[:space:]')" ]; then
            emit_warn "EP. no ENTRYPOINTS declared in $EP_REPO/orchestration.config or $EP_ENGINE/orchestration.config — supersession is UNDECLARED here, so nothing checks whether a standing instruction still names a replaced command. Declare a row the day a mechanism is replaced: <task> | <the command today> | <what it replaced>."
            EP_OK=0
        else
            EP_PROBLEM="$(entrypoints_problem "$EP_SPEC" 2>/dev/null || true)"
            if [ -n "$EP_PROBLEM" ]; then
                emit_fail "EP. ENTRYPOINTS in $EP_SPEC_SRC: $EP_PROBLEM. Until the declaration parses, the lint refuses to decide and nothing is checked. The declaration is canonical: fix it there, never a consumer."
                EP_OK=0
            fi
        fi
    fi

    # --- the live run, against BOTH roots ------------------------------------
    # The governed repository AND the engine. They are the same directory only
    # when the engine governs itself; in every by-reference session they are not,
    # and the engine's own SessionStart announcement — the file type V was made
    # of — lives in the second one.
    #
    # AN ARRAY, NEVER A JOINED STRING (hunt part 3, finding 38). The roots used
    # to be joined into one space-separated string and iterated unquoted, so a
    # repository at a path with a space in it was split into pieces that do not
    # exist, the lint was asked about each piece, and the layer refused a valid
    # checkout. EP_ROOTS stays a display string for the messages only.
    EP_ROOT_LIST=("$EP_REPO")
    EP_ROOTS="$EP_REPO"
    if [ "$EP_ENGINE" != "$EP_REPO" ]; then
        EP_ROOT_LIST+=("$EP_ENGINE")
        EP_ROOTS="$EP_REPO $EP_ENGINE"
    fi
    EP_SCANNED_ROOTS=0
    for EP_R in "${EP_ROOT_LIST[@]}"; do
        [ "$EP_OK" -eq 1 ] || break
        EP_SCANNED_ROOTS=$((EP_SCANNED_ROOTS + 1))
        set +e
        EP_OUT="$("$EP_LINT" --root "$EP_R" --engine "$EP_ENGINE" 2>&1)"
        EP_RC=$?
        set -e
        if [ "$EP_RC" -eq 1 ]; then
            EP_SITES="$(printf '%s' "$EP_OUT" | grep -c 'STANDING INSTRUCTION NAMES' 2>/dev/null || true)"
            emit_fail "EP. A STANDING INSTRUCTION IN $EP_R STILL NAMES A SUPERSEDED ENTRYPOINT. ${EP_SITES:-0} site(s). Nothing is broken and both paths work, which is why nothing else goes red: the capability ships and the instruction keeps directing traffic to the thing it replaced, so the new path goes unused and every session reports success. Full report: $EP_LINT --root $EP_R"
            printf '%s\n' "$EP_OUT" >&2
            EP_OK=0
        elif [ "$EP_RC" -eq 2 ]; then
            emit_fail "EP. the entrypoint currency lint could not run against $EP_R (exit 2): $EP_OUT"
            EP_OK=0
        fi
    done

    # --- the two-sided canary ------------------------------------------------
    # In a sandbox that is nobody's repository, so the layer proves the
    # MECHANISM rather than the current state of this tree.
    if [ "$EP_OK" -eq 1 ]; then
        EP_SB="$(mktemp -d -t ep-canary.XXXXXX)"
        mkdir -p "$EP_SB/scripts/hooks"
        {
            printf 'ENTRYPOINTS="one command | scripts/epnew.sh | scripts/epold.sh"\n'
            printf 'INSTRUCTION_SURFACES="scripts/hooks/engine-status.sh"\n'
        } >"$EP_SB/orchestration.config"

        printf 'emit_context "prepare the payload with scripts/epold.sh first"\n' \
            >"$EP_SB/scripts/hooks/engine-status.sh"
        set +e
        EP_BAD_OUT="$("$EP_LINT" --root "$EP_SB" --engine "$EP_SB" 2>&1)"
        EP_BAD_RC=$?
        set -e

        printf 'emit_context "start it with scripts/epnew.sh, which does what scripts/epold.sh was run for by hand"\n' \
            >"$EP_SB/scripts/hooks/engine-status.sh"
        set +e
        EP_GOOD_OUT="$("$EP_LINT" --root "$EP_SB" --engine "$EP_SB" 2>&1)"
        EP_GOOD_RC=$?
        set -e

        rm -rf "$EP_SB"

        if [ "$EP_BAD_RC" -ne 1 ]; then
            emit_fail "EP. the currency lint did NOT refuse a standing instruction naming a superseded entrypoint (exit=$EP_BAD_RC, expected 1) — the lint is shimmed or gutted, and a dead check reads exactly like a clean tree. Output: $EP_BAD_OUT"
            EP_OK=0
        elif ! printf '%s' "$EP_BAD_OUT" | grep -q 'engine-status.sh:1:'; then
            emit_fail "EP. the currency lint refused, but did not name the FILE AND LINE ('engine-status.sh:1:') — a refusal that does not say where costs a round trip per fix, which is the failure class next door (type N). Output: $EP_BAD_OUT"
            EP_OK=0
        elif [ "$EP_GOOD_RC" -ne 0 ]; then
            emit_fail "EP. the currency lint refused a CORRECTED instruction that names the current command (exit=$EP_GOOD_RC, expected 0) — a check that refuses everything is waived within a day and then it is gone. Output: $EP_GOOD_OUT"
            EP_OK=0
        fi
    fi

    if [ "$EP_OK" -eq 1 ]; then
        emit_pass "EP. supersession declared as data (ENTRYPOINTS in $EP_SPEC_SRC, $(entrypoints_superseded_names "$EP_SPEC" | wc -w | tr -d ' ') superseded entrypoint(s)), every standing instruction in $EP_SCANNED_ROOTS root(s) — $EP_ROOTS — names the current command, and the lint REFUSES a known-bad instruction naming file:line while ALLOWING the corrected one (two-sided canary)"
    fi
}

if [ "$_ep_layer_executed" -eq 1 ]; then
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --root)   REPO_ROOT="$2"; shift 2 ;;
            --root=*) REPO_ROOT="${1#--root=}"; shift ;;
            *) echo "contract-integrity-layer-ep.sh: unknown argument '$1'" >&2; exit 2 ;;
        esac
    done
    echo "Layer EP (standalone)" >&2
    run_layer_EP
    [ "$FAIL" -eq 0 ] || exit 2
    exit 0
fi

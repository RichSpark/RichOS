#!/usr/bin/env bash
#
# scripts/lib/vendored-material.sh — WHOSE WORK IS THIS? ANSWERED IN ONE PLACE,
#                                     FOR EVERY MECHANISM THAT ASKS.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-09-04 a publication audit found that 15 of this engine's 27 skills
# came from outside the project, and that exactly ONE of those vendorings had
# ever been written down — in a commit message. Reconstructing the other
# fourteen cost a full agent doing byte comparisons against upstream
# repositories, and two verdicts came back short of certain because the
# evidence had decayed.
#
# In a completely unrelated mechanism, dialect-guard.sh Americanized FOURTEEN
# lines across TWO vendored MIT-licensed skills (commit 06f4a8221a61,
# 2026-08-30): `engine/skills/copywriting/references/natural-transitions.md` and
# `engine/skills/landing-page-taste/SKILL.md`. The guard behaved exactly as
# designed. It had no way to know those files were somebody else's — and,
# because it refuses a write carrying a foreign spelling, it also forbade
# re-vendoring them verbatim and forbade quoting the difference in a notice.
#
#   ONE MISSING FACT, TWO UNRELATED FAILURES. The fact is now data, in
#   `.richos/vendored-material`, and this file is the ONLY thing that reads it.
#
# ===========================================================================
# WHY A LIBRARY AND NOT A PARSER IN EACH CALLER
# ===========================================================================
# Two callers today (guard-vendoring-commits.sh, dialect-guard.sh) and more
# later. This engine has already paid for the alternative twice: a typed guard
# inventory drifted in two days (scripts/lib/registered-hooks.sh), and a
# hand-rolled git-target resolver put the same hole in five files
# (scripts/lib/git-jurisdiction.sh). A second parser of one file is a second
# answer to one question, and the two diverge on the day nobody is looking.
#
# ===========================================================================
# NO SILENT DEGRADATION
# ===========================================================================
# Three outcomes, and the caller MUST distinguish them:
#
#   rc 0  loaded    — the repository declares the contract and the registry
#                     parsed. VM_* below are populated.
#   rc 1  absent    — this repository declares nothing. The caller stands down.
#                     A repository that has made no claim about its vendored
#                     material must not have one invented for it.
#   rc 2  BROKEN    — VM_BROKEN_REASON is set and the caller must BLOCK. A
#                     declared-but-unreadable registry is an enforcement outage
#                     that looks exactly like a clean run, which is the shape of
#                     defect this whole file is a response to.
#
# Safe to source repeatedly. Never mutates state, never changes the caller's cwd.

if [ -n "${_VENDORED_MATERIAL_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_VENDORED_MATERIAL_SH_SOURCED=1

# THE DECLARATION NAME, in the engine's `${X_DECLARATION:=.name}` convention.
# publication-completeness.py DERIVES the set of shipped declarations by
# grepping shipped source for exactly this shape, so a capability gated on a
# declaration nobody can find or copy is reported as INERT rather than shipped.
# Changing this line changes what that check looks for; it is not decoration.
: "${VENDORING_DECLARATION:=.vendored-material}"

_VM_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./declaration-path.sh
. "$_VM_LIB_DIR/declaration-path.sh"

# The two recognized origins. `richos` entries are not bookkeeping: without
# them the guard could not tell "we wrote this and said so" from "nobody wrote
# anything down", and adding a skill of our own would be refused.
VM_ORIGINS="third-party richos"

# The field separator, as a variable, because a literal tab inside ${var#*X} is
# invisible in a diff and one stray space away from silently matching nothing.
VM_TAB=$'\t'

# ---------------------------------------------------------------------------
# vm_load <repo_root>
# ---------------------------------------------------------------------------
# On rc 0 sets:
#   VM_REGISTRY               absolute path to the file that was read
#   VM_REDISTRIBUTABLE_PATHS  space-separated governed prefixes
#   VM_COVERED                newline-separated "<path>\t<origin>" pairs
#   VM_ENTRY_COUNT            number of entry LINES (not paths)
# On rc 2 sets VM_BROKEN_REASON.
vm_load() {
    local root="${1:-}" rc=0 line lineno=0 fields path origin known p rest
    # The outer read loop needs IFS empty to preserve leading whitespace; the
    # comma split below needs it set. Saved ONCE, at the top, rather than around
    # the inner loop: an inner `local` of the same name shadows nothing useful
    # and its restore then reads an unset variable under `set -u`.
    local IFS_SAVE="$IFS"
    VM_REGISTRY=""
    VM_REDISTRIBUTABLE_PATHS=""
    VM_COVERED=""
    VM_ENTRY_COUNT=0
    VM_BROKEN_REASON=""

    [ -n "$root" ] || return 1

    decl_find "$root" "$VENDORING_DECLARATION" || rc=$?
    case "$rc" in
        0) ;;
        1) return 1 ;;
        *) VM_BROKEN_REASON="$DECL_BROKEN_REASON"; return 2 ;;
    esac
    VM_REGISTRY="$DECL_PATH"

    if [ ! -r "$VM_REGISTRY" ]; then
        VM_BROKEN_REASON="$VM_REGISTRY exists but cannot be read. A registry that cannot be opened protects nothing while looking switched on."
        return 2
    fi

    while IFS= read -r line || [ -n "$line" ]; do
        lineno=$((lineno + 1))
        case "$line" in
            '#'*) continue ;;
        esac
        [ -n "${line//[[:space:]]/}" ] || continue

        # SETTINGS, in the same `KEY="value"` shape every sibling declaration
        # in .richos/ uses. Read by name rather than sourced: sourcing a
        # committed data file would execute whatever a future edit put in it,
        # and this file is read by a guard that runs before every commit.
        case "$line" in
            REDISTRIBUTABLE_PATHS=*)
                VM_REDISTRIBUTABLE_PATHS="${line#REDISTRIBUTABLE_PATHS=}"
                VM_REDISTRIBUTABLE_PATHS="${VM_REDISTRIBUTABLE_PATHS%\"}"
                VM_REDISTRIBUTABLE_PATHS="${VM_REDISTRIBUTABLE_PATHS#\"}"
                continue ;;
            [A-Z]*=*)
                VM_BROKEN_REASON="$VM_REGISTRY line $lineno declares an unknown setting: ${line%%=*}. A setting this engine does not read is a policy somebody believes is live and is not."
                return 2 ;;
        esac

        # AN ENTRY. Ten tab-separated fields, and the count is checked rather
        # than assumed: a row that lost a tab still LOOKS like a row, and the
        # fields after the gap would silently shift by one — a license read out
        # of the holder column is a wrong answer wearing the right shape.
        #
        # SPLIT WITH PARAMETER EXPANSION ONLY — no fork, and no herestring.
        #
        # This runs inside dialect-guard.sh, which fires on EVERY Write and Edit
        # in every repository on the machine, so the cost of this loop is paid
        # by every edit anybody makes. Both obvious spellings were measured on
        # the machine that ships this, and both were unaffordable:
        #
        #   awk -F'\t' '{print NF}' + two cuts   six forks per row, 204 for
        #                                       this registry -> 755ms/write
        #   IFS=$'\t' read -r -a F <<<"$line"    no fork, but bash 3.2 spools
        #                                       every herestring through a temp
        #                                       FILE -> ~380ms per load
        #   ${line%%...} / ${line#*...}          no fork, no file, no syscall
        #
        # The baseline this is measured against is 231ms per write. A guard that
        # adds half a second to every keystroke-sized edit is a guard somebody
        # removes by the end of the week, and then the rule has no chokepoint
        # again — the same arithmetic the engine already did when it refused to
        # put its test suite in a commit hook, one order of magnitude down.
        fields=1
        rest="$line"
        while [ "${rest#*$VM_TAB}" != "$rest" ]; do
            rest="${rest#*$VM_TAB}"
            fields=$((fields + 1))
        done
        if [ "$fields" -ne 10 ]; then
            VM_BROKEN_REASON="$VM_REGISTRY line $lineno has $fields tab-separated field(s), and an entry has exactly 10 (path, origin, license, holder, upstream, revision, arrived, confidence, modified, evidence). Refusing rather than reading a shifted row."
            return 2
        fi
        path="${line%%$VM_TAB*}"
        rest="${line#*$VM_TAB}"
        origin="${rest%%$VM_TAB*}"
        known=0
        for p in $VM_ORIGINS; do
            [ "$origin" = "$p" ] && known=1
        done
        if [ "$known" -ne 1 ]; then
            VM_BROKEN_REASON="$VM_REGISTRY line $lineno declares origin '$origin', which is neither of the two this engine understands ($VM_ORIGINS). An origin nothing recognizes is an entry that answers no question."
            return 2
        fi
        if [ -z "$path" ]; then
            VM_BROKEN_REASON="$VM_REGISTRY line $lineno declares no path. An entry that covers nothing is an entry that suppresses nothing."
            return 2
        fi
        # ONE LINE MAY NAME SEVERAL PATHS — a font family is two files and a
        # license, and it is still ONE thing. Comma-separated, expanded here so
        # every caller downstream sees a flat list.
        IFS=','
        for p in $path; do
            [ -n "$p" ] || continue
            p="${p#./}"
            p="${p%/}"
            VM_COVERED="${VM_COVERED}${p}	${origin}
"
        done
        IFS="$IFS_SAVE"
        VM_ENTRY_COUNT=$((VM_ENTRY_COUNT + 1))
    done < "$VM_REGISTRY"

    # A REGISTRY THAT GOVERNS NOTHING, and a registry that records nothing, are
    # both "declared and enforcing zero". Neither may pass as healthy: the whole
    # point of the declaration is that its presence means something is checked.
    if [ -z "${VM_REDISTRIBUTABLE_PATHS//[[:space:]]/}" ]; then
        VM_BROKEN_REASON="$VM_REGISTRY declares no REDISTRIBUTABLE_PATHS, so nothing is governed. Delete the file to stand the contract down deliberately and visibly, or name the paths it covers."
        return 2
    fi
    if [ "$VM_ENTRY_COUNT" -eq 0 ]; then
        VM_BROKEN_REASON="$VM_REGISTRY carries no entries at all. Every file already under a governed path would be reported as an unrecorded vendoring, which is a guard nobody could commit past."
        return 2
    fi
    return 0
}

# ---------------------------------------------------------------------------
# vm_load_upward <start_dir>
# ---------------------------------------------------------------------------
# The same three outcomes, for a caller that holds a PATH rather than a
# repository root — dialect-guard.sh is handed a file and has to find out
# whether ANY enclosing repository declares this contract. On rc 0, VM_ROOT is
# the directory that declared it, and relative paths handed to vm_covering must
# be relative to THAT.
#
# It exists because the alternative is every caller doing its own upward walk,
# and an upward walk done twice is done differently twice.
vm_load_upward() {
    local start="${1:-}" rc=0
    VM_ROOT=""
    VM_BROKEN_REASON=""
    [ -n "$start" ] || return 1
    decl_find_upward "$start" "$VENDORING_DECLARATION" || rc=$?
    case "$rc" in
        0) ;;
        1) return 1 ;;
        *) VM_BROKEN_REASON="$DECL_BROKEN_REASON"; return 2 ;;
    esac
    VM_ROOT="$DECL_ROOT"
    vm_load "$DECL_ROOT"
}

# ---------------------------------------------------------------------------
# vm_governed <relpath>
# ---------------------------------------------------------------------------
# rc 0 when <relpath> lies under a REDISTRIBUTABLE_PATHS prefix; VM_PREFIX is
# the prefix that matched. Requires a prior successful vm_load.
#
# PATH-BOUNDARY MATCHING, never a bare string prefix: `engine/skills-archive/x`
# is not under `engine/skills`, and a substring test would say it is.
vm_governed() {
    local rel="${1:-}" pre
    VM_PREFIX=""
    [ -n "$rel" ] || return 1
    for pre in $VM_REDISTRIBUTABLE_PATHS; do
        pre="${pre%/}"
        [ -n "$pre" ] || continue
        if [ "$rel" = "$pre" ] || [ "${rel#"$pre"/}" != "$rel" ]; then
            VM_PREFIX="$pre"
            return 0
        fi
    done
    return 1
}

# ---------------------------------------------------------------------------
# vm_covering <relpath>
# ---------------------------------------------------------------------------
# rc 0 when some registry entry covers <relpath>; VM_MATCH is the entry's path
# and VM_MATCH_ORIGIN its origin. The LONGEST matching entry wins, so a specific
# file entry beats the directory it sits in.
vm_covering() {
    local rel="${1:-}" p origin best="" best_origin=""
    VM_MATCH=""
    VM_MATCH_ORIGIN=""
    [ -n "$rel" ] || return 1
    while IFS=$'\t' read -r p origin; do
        [ -n "$p" ] || continue
        if [ "$rel" = "$p" ] || [ "${rel#"$p"/}" != "$rel" ]; then
            if [ "${#p}" -gt "${#best}" ]; then
                best="$p"
                best_origin="$origin"
            fi
        fi
    done <<EOF
$VM_COVERED
EOF
    [ -n "$best" ] || return 1
    VM_MATCH="$best"
    VM_MATCH_ORIGIN="$best_origin"
    return 0
}

# ---------------------------------------------------------------------------
# vm_is_third_party <relpath>
# ---------------------------------------------------------------------------
# rc 0 ONLY when the covering entry says the work is somebody else's. This is
# the question dialect-guard.sh asks, and the distinction is the whole fix: a
# `richos` entry must NOT exempt our own prose from our own dialect rule.
vm_is_third_party() {
    vm_covering "${1:-}" || return 1
    [ "$VM_MATCH_ORIGIN" = "third-party" ]
}

# ---------------------------------------------------------------------------
# vm_unit <relpath>
# ---------------------------------------------------------------------------
# The thing a registry entry would name for <relpath>: the governed prefix plus
# one more component. `engine/skills/foo/SKILL.md` -> `engine/skills/foo`;
# `app/ui/fonts/Bar.woff2` -> `app/ui/fonts/Bar.woff2`. Requires vm_governed to
# have matched. Reporting the UNIT rather than the file is what makes the
# refusal actionable — you register a skill, not one of its reference files.
vm_unit() {
    local rel="${1:-}" rest
    vm_governed "$rel" || { printf '%s' "$rel"; return 0; }
    rest="${rel#"$VM_PREFIX"/}"
    printf '%s/%s' "$VM_PREFIX" "${rest%%/*}"
}

# ---------------------------------------------------------------------------
# vm_broken_banner <hook-name> <reason>
# ---------------------------------------------------------------------------
# One shape for "this contract is declared and cannot be evaluated", so the
# reader never has to work out whether a guard refused because of the work or
# because of itself.
vm_broken_banner() {
    echo "=== VENDORED-MATERIAL REGISTRY: DECLARED BUT UNREADABLE — REFUSING ==="
    echo "  hook   : ${1:-<unknown>}"
    echo "  reason : ${2:-<unstated>}"
    echo ""
    echo "  This repository declares that it records where its redistributable"
    echo "  material came from, and the record cannot be evaluated. A guard that"
    echo "  carried on here would report 'on' over zero enforcement, which is the"
    echo "  exact failure the registry was built after."
}

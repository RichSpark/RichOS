#!/usr/bin/env bash
#
# install.sh — put the automatic checks into a repository's hooks, and PROVE git will call them.
#
# Usage:
#   install.sh [<repo>]              install (default: the repository this file is in)
#   install.sh --check [<repo>]      report each hook; exit 1 if any is missing or unreachable
#   install.sh --uninstall [<repo>]  remove the hooks this installed, and nothing else
#
# WHAT IT INSTALLS: shim.sh, copied to <git-common-dir>/hooks/ as pre-commit,
# commit-msg, pre-merge-commit, post-commit, post-merge and pre-push. Every linked worktree of the
# repository (native, ~/ab/richos-wt, Codex's, the nightly's) shares that one directory,
# so one install covers every committer. The shim runs the committed autocheck.py; see its
# header for what each hook checks.
#
# WHY "PROVE": on this Mac core.hooksPath is set globally, and it REPLACES .git/hooks. A hook
# placed there runs only because the global dispatcher (femcboost's
# install-git-identity-guard.sh) chains to "$(git rev-parse --git-common-dir)/hooks/<name>".
# So the chain is checked for each hook name, as engine/scripts/install-ref-forensics.sh
# checks it for reference-transaction, and a hook that would never be called is reported.
#
# A hook file that this did not write is never overwritten: it is reported and the install
# refuses, because replacing it would silently remove somebody else's check.
#
# It also registers the merge driver for the two verification maps (merge.richos-verification-pins,
# see below), which --check reports and --uninstall removes.

set -uo pipefail

SELF_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SHIM="$SELF_DIR/shim.sh"
MARKER="richos-autocheck-shim"
HOOKS="pre-commit commit-msg pre-merge-commit post-commit post-merge pre-push"

MODE=install
case "${1:-}" in
    --check) MODE=check; shift ;;
    --uninstall) MODE=uninstall; shift ;;
    -h|--help) sed -n '2,27p' "${BASH_SOURCE[0]}"; exit 0 ;;
esac
REPO="${1:-$SELF_DIR}"

die() { printf 'autocheck install: %s\n' "$*" >&2; exit 2; }

[ -f "$SHIM" ] || die "shim missing at $SHIM"
COMMON="$(git -C "$REPO" rev-parse --git-common-dir 2>/dev/null)" || die "not a git repository: $REPO"
case "$COMMON" in
    /*) ;;
    *) COMMON="$(cd "$REPO" && cd "$COMMON" && pwd)" ;;
esac
DEST="$COMMON/hooks"
HOOKS_PATH="$(git -C "$REPO" config --get core.hooksPath 2>/dev/null || true)"

reachable() {
    local name="$1" dispatch
    [ -z "$HOOKS_PATH" ] && return 0
    dispatch="$HOOKS_PATH/$name"
    [ -e "$dispatch" ] || return 1
    grep -q 'git-common-dir' "$(readlink -f "$dispatch" 2>/dev/null || printf '%s' "$dispatch")" 2>/dev/null
}

ours() { [ -f "$1" ] && grep -q "$MARKER" "$1" 2>/dev/null; }

rc=0

# THE MERGE DRIVER FOR THE TWO VERIFICATION MAPS (2026-10-05). Every branch that changes a pinned
# reader renews its sha256 pin, so two branches built in parallel conflicted in
# docs/development/verification-input-qualifications.json and
# richos/engine/scripts/lib/verification-dependencies.json on every merge, and each time somebody
# took one side and re-ran renew-verification-pins.py by hand. The .gitattributes beside each map
# names this driver; it merges them as JSON, keeps every entry from both sides and regenerates
# the pins from the merged files (merge-verification-pins.py). Repository config, so it reaches
# every linked worktree, like the hooks.
DRIVER="merge.richos-verification-pins"
DRIVER_NAME="verification maps: JSON merge, pins regenerated from the merged files"
# shellcheck disable=SC2016 # the $(...) is for git's shell when it runs the driver, never this one
DRIVER_CMD='python3 "$(git rev-parse --show-toplevel)/richos/engine/scripts/lib/merge-verification-pins.py" %O %A %B %P'
case "$MODE" in
    install)
        if git -C "$REPO" config "$DRIVER.name" "$DRIVER_NAME" && git -C "$REPO" config "$DRIVER.driver" "$DRIVER_CMD"; then
            printf 'INSTALLED  %s.driver\n' "$DRIVER"
        else
            printf 'FAILED     %s.driver\n' "$DRIVER"; rc=1
        fi
        ;;
    check)
        if [ "$(git -C "$REPO" config --get "$DRIVER.driver" 2>/dev/null)" = "$DRIVER_CMD" ]; then
            printf 'INSTALLED  %s.driver\n' "$DRIVER"
        else
            printf 'MISSING    %s.driver (run install.sh)\n' "$DRIVER"; rc=1
        fi
        ;;
    uninstall)
        git -C "$REPO" config --remove-section "$DRIVER" 2>/dev/null && printf 'REMOVED    %s\n' "$DRIVER"
        ;;
esac

for name in $HOOKS; do
    target="$DEST/$name"
    case "$MODE" in
        install)
            if [ -e "$target" ] && ! ours "$target"; then
                printf 'REFUSED    %s exists and was not written by this installer; not replaced\n' "$target"
                rc=1
                continue
            fi
            if [ ! -f "$target" ] || ! cmp -s "$SHIM" "$target"; then
                # Written beside the target, made executable, then renamed over it: a commit running
                # now reads the old hook or the new one, never half of one (Codex, 2026-10-20 10:04Z).
                if ! { mkdir -p "$DEST" && tmp="$(mktemp "$target.new.XXXXXX")" && cp "$SHIM" "$tmp" \
                        && chmod +x "$tmp" && mv -f "$tmp" "$target"; }; then
                    [ -n "${tmp:-}" ] && rm -f "$tmp"
                    printf 'FAILED     %s\n' "$target"
                    rc=1
                    continue
                fi
                printf 'INSTALLED  %s\n' "$target"
            else
                printf 'CURRENT    %s\n' "$target"
            fi
            if ! reachable "$name"; then
                printf 'UNREACHABLE core.hooksPath=%s does not chain to %s\n' "$HOOKS_PATH" "$target"
                rc=1
            fi
            ;;
        check)
            if ! ours "$target"; then
                printf 'MISSING    %s\n' "$target"; rc=1
            elif ! cmp -s "$SHIM" "$target"; then
                printf 'STALE      %s (differs from %s; run install.sh)\n' "$target" "$SHIM"; rc=1
            elif ! reachable "$name"; then
                printf 'UNREACHABLE %s (core.hooksPath=%s does not chain to it)\n' "$target" "$HOOKS_PATH"; rc=1
            else
                printf 'INSTALLED  %s\n' "$target"
            fi
            ;;
        uninstall)
            if ours "$target"; then
                rm -f "$target" && printf 'REMOVED    %s\n' "$target"
            fi
            ;;
    esac
done
exit "$rc"

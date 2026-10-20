#!/bin/sh
# richos-autocheck-shim 1
#
# A repository hook, copied into <git-common-dir>/hooks/<name> by
# richos/app/scripts/autocheck/install.sh for pre-commit, commit-msg, pre-merge-commit, post-commit,
# post-merge and pre-push. Do not edit the installed copy: install.sh rewrites it.
#
# It runs the COMMITTED autocheck.py, read from git's object store, never a working copy:
# the one on main, which decides whether this is a land and hands anything else to the one at
# HEAD when that differs (a branch is checked by its own version); else the one at HEAD (main
# older than the check), else the one being merged (the land that introduces it). When none of
# them has it, the check does not exist in this repository yet and this exits 0. An installed
# copy that differs from the shim on main runs that one (below).
# Every linked worktree shares this one hooks directory, so one install covers them all.

REL=richos/app/scripts/autocheck/autocheck.py
SHIM_REL=richos/app/scripts/autocheck/shim.sh
HOOK=$(basename "$0")

# THE INSTALLED COPY RUNS THE SHIM ON MAIN (verification reliability PRD, T10, 2026-10-20). This
# file is copied into the hooks directory once, by install.sh, and nothing copied it again when it
# changed: the installed copy on the operator's Mac was older than this file for ten days. So a
# copy that differs from the shim committed on main runs that one instead, read from git's object
# store as autocheck.py is, with the same name and arguments; it runs it once (the variable below).
if [ -z "${RICHOS_AUTOCHECK_SHIM_FROM_MAIN:-}" ] && git cat-file -e "refs/heads/main:$SHIM_REL" 2>/dev/null </dev/null; then
    if ! git cat-file -p "refs/heads/main:$SHIM_REL" 2>/dev/null </dev/null | cmp -s - "$0"; then
        MAIN_SHIM=$(git cat-file -p "refs/heads/main:$SHIM_REL" 2>/dev/null </dev/null) && [ -n "$MAIN_SHIM" ] && {
            RICHOS_AUTOCHECK_SHIM_FROM_MAIN=1
            export RICHOS_AUTOCHECK_SHIM_FROM_MAIN
            exec sh -c "$MAIN_SHIM" "$0" "$@"
        }
    fi
fi
unset RICHOS_AUTOCHECK_SHIM_FROM_MAIN

SRC=
# An automatic `git merge` runs pre-merge-commit before it writes MERGE_HEAD and names what it
# merges only in GIT_REFLOG_ACTION ("merge <name>"); a single name is a candidate too.
INCOMING=
case "${GIT_REFLOG_ACTION:-}" in
    "merge "*) INCOMING=$(printf '%s\n' "${GIT_REFLOG_ACTION#merge }" | awk 'NF == 1 { print $1 }') ;;
esac
# A LAND IS JUDGED BY THE GATE ON MAIN, WHATEVER BRANCH IT LANDS ONTO (PRD R8, T10). A land onto
# a release branch runs in a checkout of that branch, so HEAD's copy was the release branch's own:
# v1.3.0 was cut on 2026-10-14, before a land onto a release branch ran the land check (d2c2c28dc),
# and its copy runs only the commit check for every land onto it. And whether a plain `git merge`
# there is a land at all is known only from the recorded integration branches, which main's copy
# asks (second review rv-20261020T073921Z-acb97f5e-ec07, finding 2). So main's copy runs first
# for every hook and decides: a land it judges itself; anything else it hands, unchanged, to the
# copy at HEAD when that differs, so a branch's commits are still checked by its own version.
for rev in refs/heads/main HEAD MERGE_HEAD "${INCOMING:-HEAD}"; do
    if git cat-file -e "$rev:$REL" 2>/dev/null </dev/null; then
        SRC=$rev
        break
    fi
done
[ -n "$SRC" ] || exit 0

case "$HOOK" in
    post-*) REFUSE=0 ;;
    *) REFUSE=1 ;;
esac
mkdir -p "${TMPDIR:-${HOME}/.cache/richos/tmp}" || exit "$REFUSE"
TMP=$(mktemp -d "${TMPDIR:-${HOME}/.cache/richos/tmp}/richos-autocheck.XXXXXX") || {
    echo "autocheck: $HOOK could not make a scratch file; the check did not run" >&2
    exit "$REFUSE"
}
trap 'rm -rf "$TMP"' HUP INT TERM
if ! git cat-file -p "$SRC:$REL" > "$TMP/autocheck.py" 2>/dev/null </dev/null; then
    rm -rf "$TMP"
    echo "autocheck: $HOOK could not read $SRC:$REL; the check did not run" >&2
    exit "$REFUSE"
fi
# Read the shared admission clock from the SAME committed source as the checker. This
# also works when HEAD predates the checker/helper and the shim falls back to main.
HELPER=richos/app/scripts/lib/owned_command.py
if git cat-file -e "$SRC:$HELPER" 2>/dev/null </dev/null; then
    if ! git cat-file -p "$SRC:$HELPER" > "$TMP/owned_command.py" 2>/dev/null </dev/null; then
        rm -rf "$TMP"
        echo "autocheck: $HOOK could not read its committed admission helper" >&2
        exit "$REFUSE"
    fi
fi
python3 "$TMP/autocheck.py" "$HOOK" "$@"
rc=$?
rm -rf "$TMP"
exit "$rc"

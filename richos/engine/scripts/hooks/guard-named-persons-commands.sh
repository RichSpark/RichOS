#!/usr/bin/env bash
#
# guard-named-persons-commands.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# THE THREE SURFACES THE SCRUB MISSED.
#
# A third party's name reached this public repository in a FILE. That much the
# write guard now covers. The remedy is what makes this file necessary: the
# commit that removed his name from the file PUT HIS FULL NAME AND HIS COMPANY
# IN THE COMMIT MESSAGE — on the repository's commit list, more visible than the
# line it was deleting. It had to be amended and force-pushed with branch
# protection temporarily lifted.
#
#   A SCRUB COVERS FOUR SURFACES: file content, commit message, branch name,
#   and PR/issue title.
#
# guard-named-persons-writes.sh covers the first. This covers the other three,
# because all three are typed into a COMMAND and a PreToolUse[Bash] hook is
# handed that command before it runs.
#
# WHAT IT INSPECTS, AND WHY THE WHOLE STRING
# ------------------------------------------
# The entire command text, not a parsed flag. Flag parsing is where the holes
# are: `-m`, `-m<msg>`, `--message=`, `--title`, `-t`, `-b`, a heredoc body
# inside a command substitution, a second `-m` for a trailer. Every one of those
# is IN the string, so reading the string has no hole to miss. The one thing the
# string does not carry is a message in a FILE (`-F <path>`), and the predicate
# reads that file for exactly this reason.
#
# The cost of reading the whole string is precision, and precision is bought
# back by firing on almost nothing: only a command that CREATES A DURABLE,
# PUBLISHABLE ARTIFACT — `git commit`, `git tag`, `git branch`, `git checkout
# -b`, `git switch -c`, `git worktree add`, `git push`, `git merge`, `git notes`
# and any `gh` subcommand. Ordinary shell work — reading a file, grepping,
# building, running tests — is never inspected, so a private note that mentions
# a real person can still be read, searched and edited on this machine. Nothing
# about this guard makes a name unmentionable; it makes a name unpublishable.
#
# WHAT IT CANNOT SEE, said here rather than discovered later:
#   * `git commit` with NO message, which opens the editor. The message is typed
#     into a temporary file no hook is handed.
#   * Commits created by `git merge`, `cherry-pick`, `rebase` and `am`, which
#     make commits without running `git commit`. A merge commit's default
#     message quotes the BRANCH NAME, and branch names are gated at creation.
#   * A branch created, or a PR title typed, in the GitHub web UI. No hook on
#     this machine is anywhere near that path. The release-time check is the
#     backstop for everything typed rather than executed.
#
# NO LIVE OVERRIDE, AND NO ALLOWLIST — DELIBERATELY, AND UNLIKE ITS SIBLINGS.
# guard-publication-commits.sh at least has ALLOWLIST, a committed exemption for
# a PATH. There is no path exemption here, because the object being protected is
# a PERSON and not a location: "this name is fine in this directory" is not a
# statement anybody can make on his behalf. The two ways through are to write
# the artifact without the name, or to take the entry off the deny-list — and
# the second is a decision about a person, made in the operator's own file,
# outside every repository.
#
# FAIL-CLOSED on a missing python3 and on a BROKEN list. FAILS OPEN on an
# unparseable payload, matching the Bash-matcher family.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-named-persons-commands.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-named-persons-commands.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# --- JURISDICTION ----------------------------------------------------------
# Deliberately BELOW the root-resolution bootstrap, never inside it: Layer R of
# contract-integrity-probe.sh extracts that block verbatim and asserts it is
# byte-identical across every rooted hook, so anything added inside it would
# read as divergence.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-commands.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# --- GIT JURISDICTION ------------------------------------------------------
# WHICH repository is this command talking to? Answered by the library that
# already answers it for the other Bash-matcher guards, never by a copy: this is
# the resolution that used to live in five hand-copied blocks, four of which
# resolved the session's repository rather than the one being written to.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-commands.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually write to."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

_PB_LIB="$SCRIPT_DIR/../lib/publication-boundary.sh"
if [ ! -f "$_PB_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-commands.sh"
        echo "  scripts/lib/publication-boundary.sh is missing at: $_PB_LIB"
        echo "  It answers whether the repository this command writes to is one"
        echo "  that gets published, and this guard will not guess at that."
    } >&2
    exit 2
fi
# shellcheck source=../lib/publication-boundary.sh
. "$_PB_LIB"

_NP_LIB="$SCRIPT_DIR/../lib/named-persons.sh"
if [ ! -f "$_NP_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-commands.sh"
        echo "  scripts/lib/named-persons.sh is missing at: $_NP_LIB"
        echo "  It is the whole decision this guard makes."
    } >&2
    exit 2
fi
# shellcheck source=../lib/named-persons.sh
. "$_NP_LIB"

INPUT="$(cat)"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes: the tool-name extraction ends
# in `|| true`, so "this call is not mine" and "I could not tell whose call this
# is" are one exit. That is why 17 of 25 PreToolUse guards were measured passing
# a call in complete silence on 2026-09-05. This separates the two. NO VERDICT
# CHANGES — the exit is the one already taken — only the silence does. The
# measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-named-persons-commands.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this command puts a real person's name into publication-bound material"
fi

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Bash" ] || exit 0

COMMAND="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("command") or "")' 2>/dev/null || true)"
[ -n "$COMMAND" ] || exit 0

# --- PRECISION: does this command create a durable, publishable artifact? ---
# CLASSIFIED IN PYTHON, NOT IN grep -E, and that is a scar rather than a
# preference. The first version used
#
#     grep -Eq '\bgit\b[^\n;|&]*\b(commit|tag)\b'
#
# where `[^\n;|&]` was meant to say "within one shell statement". In a POSIX
# bracket expression `\n` is NOT a newline — it is a backslash and the letter
# n — so the class actually excluded the letter n, and every `git commit` whose
# command string contained an n anywhere between `git` and `commit` failed to
# match. It passed a hand test against `git -C /tmp/x commit` and failed
# silently against every real path. A guard that matches nothing looks exactly
# like a guard that finds nothing.
#
# The verbs are the four surfaces: a commit MESSAGE, a TAG name and message, a
# BRANCH name (created four ways, plus the refspec a push names), a merge
# message, a note, and every `gh` subcommand that carries a title or a body.
# Everything else on the Bash matcher passes untouched.
# Command bodies can be larger than one environment entry. Feed the full
# script on stdin and distinguish classifier failure from a non-publishing call.
# dispatch-words: git gh
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
NP_SURFACE="$(python3 -c '
import re, sys
cmd = sys.stdin.read()
# One shell statement at a time, so an unrelated later command cannot bleed in.
for seg in re.split(r"[;\n|&]+", cmd):
    if re.search(r"\bgit\b.*\b(branch|push)\b", seg) \
       or re.search(r"\bgit\b.*\bcheckout\b.*(?:^|\s)-b\b", seg) \
       or re.search(r"\bgit\b.*\bswitch\b.*(?:^|\s)-c\b", seg) \
       or re.search(r"\bgit\b.*\bworktree\b.*\badd\b", seg):
        print("a branch name"); sys.exit(0)
for seg in re.split(r"[;\n|&]+", cmd):
    if re.search(r"\bgit\b.*\b(commit|tag|notes|merge)\b", seg):
        print("a commit, tag or merge message"); sys.exit(0)
for seg in re.split(r"[;\n|&]+", cmd):
    if re.search(r"(?:^|\s)gh\s", seg):
        print("a GitHub artifact (PR or issue title, release name, comment body)"); sys.exit(0)
' <<< "$COMMAND" 2>/dev/null)" || {
    echo "ERROR: guard-named-persons-commands.sh: command classifier could not run; refusing." >&2
    exit 2
}
[ -n "$NP_SURFACE" ] || exit 0

# --- WHICH REPOSITORY, AND DOES IT PUBLISH? --------------------------------
NP_ANCHOR="$(richos_git_anchor "$INPUT" "commit tag notes merge branch push checkout switch worktree" | sed -n '1p' | cut -f2)"
if [ -z "$NP_ANCHOR" ]; then
    NP_ANCHOR="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("cwd") or "")' 2>/dev/null || true)"
fi
[ -n "$NP_ANCHOR" ] || exit 0

NP_REPO="$(pb_repo_root "$NP_ANCHOR" 2>/dev/null || true)"
[ -n "$NP_REPO" ] || exit 0

NP_BOUND_RC=0
np_publication_bound "$NP_REPO" || NP_BOUND_RC=$?
case "$NP_BOUND_RC" in
  0) ;;
  2) exit 0 ;;   # a broken declaration is the publication guards' contract to report
  *) exit 0 ;;   # this repository does not publish — the precision floor
esac

RESULT="$(printf '%s' "$INPUT" | python3 "$NP_PREDICATE" --scan-payload 2>/dev/null || printf 'PARSEFAIL\n')"
VERDICT="$(printf '%s' "$RESULT" | sed -n '1p' | cut -f1)"

case "$VERDICT" in
  CLEAN|PARSEFAIL)
    exit 0
    ;;
  ABSENT)
    np_announce_absent "scripts/hooks/guard-named-persons-commands.sh" "$NP_REPO"
    exit 0
    ;;
  BROKEN)
    np_broken_banner "scripts/hooks/guard-named-persons-commands.sh" \
        "$(printf '%s' "$RESULT" | sed -n '1p' | cut -f2-)"
    exit 2
    ;;
  FOUND)
    np_block_banner "scripts/hooks/guard-named-persons-commands.sh" \
        "this command — it would publish $NP_SURFACE carrying a listed name" "$RESULT"
    exit 2
    ;;
  *)
    echo "ERROR: guard-named-persons-commands.sh: unexpected predicate output — refusing (fail-closed)" >&2
    exit 2
    ;;
esac

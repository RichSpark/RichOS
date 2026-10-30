#!/usr/bin/env bash
#
# guard-publication-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# The write-time half of this pair (guard-publication-writes.sh) sees content an
# agent AUTHORS. This half sees content that is about to become HISTORY,
# whatever produced it.
#
# WHY BOTH, AND WHY THIS ONE IS NOT OPTIONAL
# ------------------------------------------
# Of the 2026-08-29 leak, THREE files were agent-authored briefs — the write
# hook would have caught those. ONE HUNDRED AND THIRTY-SEVEN were not: they were
# whisper output, ffmpeg measurements and analysis-tool results, produced by
# `tools/run.sh` and friends. No Write tool ever touched them. A PreToolUse
# [Write] guard would have reported a clean session and let every one of them
# through, and the land would have been, on its own terms, telling the truth.
#
# That is the same shape as the "18/18 suites" defect: a check whose scope
# quietly excluded the thing that was actually broken. So the predicate is
# re-run where provenance stops mattering — against the staged INDEX, at
# `git commit`. `git mv`, `cp`, an editor, a generator, a script, another
# agent's leftovers: the index sees all of them identically.
#
# WHY COMMIT AND NOT PUSH: by push time the bytes are already in history, and
# removing them needs a rewrite and a force-push — which is precisely the
# expensive, dangerous remedy the 2026-08-29 removal commit had to defer. A
# commit is free to refuse. A push is not.
#
# THE INDEX IS NOT THE WHOLE ANSWER — measured 2026-08-30. This hook runs
# BEFORE the command it is inspecting, so for
#
#     git add docs/session-notes && git commit -m notes
#
# nothing is staged yet at the moment of the check. The first version read only
# `git diff --cached`, found an empty set, and exited 0: `git add X && git
# commit`, `git add -A && git commit` and `cd repo; git add . ; git commit` ALL
# passed with a wholly-new directory of transcripts in the working tree. So did
# `git commit -m x <path>`, which commits a tracked file's WORKTREE content
# without it ever entering the index.
#
# So the set is derived from what the COMMAND will do, not only from what the
# index already holds: every `git add` in the same command, before the commit,
# in THIS repository, contributes its pathspecs, and so do pathspecs given to
# `git commit` itself. That enumeration uses `git status --porcelain -z
# --untracked-files=all`, and the `-uall` is load-bearing: without it git
# reports a wholly-new directory as ONE entry (`?? docs/session-notes/`), which
# is a path the scanner cannot read bytes out of. A new directory of
# transcripts committed in one go must be refused, and one flag is the
# difference between that and a clean report.
#
# WHAT THIS DOES NOT COVER, said plainly: `git merge`, `git cherry-pick`,
# `git am` and `git rebase` create commits without running `git commit`.
# Merge is the acceptable gap — the content it carries was gated when it was
# committed on the source branch, which is the whole point of gating commits.
# The others are genuine holes and they are named here rather than discovered.
# A `git add` in a SEPARATE tool call is not a gap: by the time the commit runs,
# its files are in the index and the index half sees them.
#
# NO LIVE OVERRIDE — DELIBERATELY. Every other Bash-matcher guard in this engine
# offers an in-prompt escape token (worktree-remove-ack:, main-checkout-run:).
# This one does not, and that is the point: the thing that failed three times in
# a row was in-the-moment human judgment about whether a particular payload was
# safe to publish. An in-the-moment override would rebuild exactly that. The way
# through is ALLOWLIST in .publication-boundary — committed, diffable, reviewed
# by whoever lands it — or a gitignored destination.
#
# PRECISION: fires only on a command that actually contains `git commit`.
# Everything else on the Bash matcher passes untouched, at the cost of one
# regex.

set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-publication-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-publication-commits.sh
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
#
# The seat resolved above answers "am I governed?". It does NOT answer "does
# the artifact I was just handed belong to the repository I govern?" — and
# until 2026-08-30 nothing asked. See scripts/lib/seat-jurisdiction.sh.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-publication-commits.sh"
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
# The question UNDERNEATH the one above: which repository is this git command
# talking to? It was answered in five hand-copied blocks; this file's copy was
# the best of them and still missed `(cd <repo> && git commit)`. REFUSING TO
# START is deliberate — a guard that resolved the repository by guessing would
# be the 2026-09-01 bypass with a nicer error message.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-publication-commits.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually commit to, and the fallback it used"
        echo "  to carry is the exact bypass that library exists to close."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    # CAPTURED, not discarded. This used to be `:` — the seat decided whether
    # this guard ran and then had no say in WHAT it judged, which is the
    # divergence in its purest form.
    SEAT_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # DELIBERATELY NOT AN EXIT. This guard reads its contract out of the TARGET
    # repository, not out of the seat, so an unadopted seat is not a reason to
    # stop — the artifact's own repository still gets to govern itself below
    # by its own declaration. Exiting here is what made richos-hq's committed
    # .row-currency and .ceo-todos readable by nothing at all.
    SEAT_ROOT=""
else
    root_failure_banner "scripts/hooks/guard-publication-commits.sh" >&2
    exit 2
fi

_PB_LIB="$SCRIPT_DIR/../lib/publication-boundary.sh"
if [ ! -f "$_PB_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-publication-commits.sh"
        echo "  scripts/lib/publication-boundary.sh is missing at: $_PB_LIB"
        echo "  This guard's entire predicate lives there. Without it it cannot"
        echo "  tell private material from ordinary work, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/publication-boundary.sh
. "$_PB_LIB"
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
    unevaluated_or_continue "guard-publication-commits.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this commit crosses the publication boundary"
fi

# --- Is this a commit at all, and where? -----------------------------------
# Classified in python, assigned via a quoted heredoc first for the same bash
# 3.2 reason guard-worktree-removal.sh documents: a `)` inside a character
# class mis-scans as the close of a $( ) substitution on macOS's /bin/bash.
# dispatch-words: commit
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
read -r -d '' _PC_CLASSIFIER <<'PYEOF' || true
import json, os, re, sys, shlex

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    print("PASS"); raise SystemExit
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    print("PASS"); raise SystemExit
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""

# `git ... commit`, tolerating -C/-c/flags in between but never crossing a
# statement separator, so an unrelated later `git` cannot bleed in.
if not re.search(r"\bgit\b[^\n;|&]*\bcommit\b", cmd):
    print("PASS"); raise SystemExit

# WHERE the command points is decided by scripts/lib/git-jurisdiction.sh, not
# here. Two answers to one question is how the same hole reached five files, and
# the answer this file used to compute was subtly better than its four siblings'
# and still wrong inside a subshell.
#
# The per-segment `cwd` tracking below stays, because it answers a DIFFERENT
# question — which directory each `git add` runs in, so its pathspecs can be
# resolved. The shell drops any add-anchor that is not the repository being
# committed to, so a disagreement between the two can only ever narrow the scan.

# `-a`/`--all` commits tracked modifications that are NOT in the index yet, so
# the staged set alone would understate what is about to be committed. Said
# once, checked once, rather than assumed.
#
# Quoted spans are stripped FIRST. `git commit -m "handle -a properly"` is not
# a `-a` commit, and treating it as one would drag every unstaged modification
# in the worktree into the scan and block a commit over a file the author was
# not committing. Over-blocking is how a guard gets switched off.
unquoted = re.sub(r'"[^"]*"', " ", cmd)
unquoted = re.sub(r"'[^']*'", " ", unquoted)
stage_all = bool(re.search(r"(?:^|\s)-[a-zA-Z]*a[a-zA-Z]*\b", unquoted)
                 or re.search(r"(?:^|\s)--all\b", unquoted))

# --- WHAT THIS COMMAND WILL STAGE BEFORE IT COMMITS ------------------------
# Everything above answers "is this a commit?". This answers "and what will be
# in it?", because the index alone answers that only when the staging already
# happened in an earlier tool call.
#
# Emitted as one ADD line per pathspec:
#
#     ADD <TAB> anchor-directory <TAB> pathspec-or-empty
#
# The anchor is the directory that `git add` would run in (its own -C, else the
# cwd in effect at that point in the command). The shell resolves it to a
# repository and DROPS anything that is not the repository being committed to,
# so `git -C /other add -A && git -C /here commit` cannot drag a foreign tree
# into this scan. An empty pathspec means "the whole repository" (-A/-u).
#
# A command this cannot tokenize emits nothing and the guard falls back to the
# index alone — the behavior it had before. Silently narrower, never wider.
cwd = str(d.get("cwd", "") or "") or "."


def _abs(base, p):
    return p if p.startswith("/") else os.path.normpath(os.path.join(base, p))


try:
    lex = shlex.shlex(cmd, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    tokens = list(lex)
except Exception:
    tokens = []

segments = []
current = []
# `(` and `)` are separators too. They were not, and that was this guard's own
# share of the 2026-09-01 jurisdiction defect: with a paren glued to the front of
# the segment, `(cd <repo> && git commit)` had seg[0] == "(" rather than "cd", the
# cd was skipped, and the guard judged the SESSION's repository. Measured against
# the four guards that missed every cd form, this one missed only this one.
for t in tokens:
    if t in ("&&", "||", ";", "|", "&", "\n", "(", ")"):
        segments.append(current)
        current = []
    else:
        current.append(t)
segments.append(current)

# Flags that CONSUME the next token, so their value is never mistaken for a
# pathspec. Flags with an OPTIONAL attached value (-S, -u) are absent on
# purpose: git only accepts those glued (`-Skeyid`, `-uall`), so the next token
# is a pathspec and treating it as a value would silently narrow the scan.
COMMIT_VALUE_FLAGS = {
    "-m", "--message", "-F", "--file", "-C", "--reuse-message",
    "-c", "--reedit-message", "--author", "--date", "-t", "--template",
    "--cleanup", "--fixup", "--squash", "--trailer", "--pathspec-from-file",
}

adds = []
commit_seen = False
for seg in segments:
    if not seg:
        continue
    if seg[0] == "cd" and len(seg) > 1 and not seg[1].startswith("-"):
        cwd = _abs(cwd, seg[1])
        continue
    if os.path.basename(seg[0]) != "git":
        continue
    # git's own options come before the subcommand.
    i, anchor = 1, cwd
    while i < len(seg):
        a = seg[i]
        if a in ("-C", "-c", "--git-dir", "--work-tree", "--namespace",
                 "--exec-path", "--config-env"):
            if a == "-C" and i + 1 < len(seg):
                anchor = _abs(anchor, seg[i + 1])
            i += 2
            continue
        if a.startswith("-"):
            i += 1
            continue
        break
    if i >= len(seg):
        continue
    sub = seg[i]
    args = seg[i + 1:]
    if sub == "commit":
        commit_seen = True
        # Pathspecs given to `git commit` itself: they commit the WORKTREE
        # content of those paths, index or no index.
        specs, j, after_ddash = [], 0, False
        while j < len(args):
            a = args[j]
            if a == "--":
                after_ddash = True
                j += 1
                continue
            if not after_ddash and a in COMMIT_VALUE_FLAGS:
                j += 2
                continue
            if not after_ddash and a.startswith("-"):
                j += 1
                continue
            specs.append(a)
            j += 1
        for s in specs:
            adds.append((anchor, _abs(anchor, s)))
        continue
    if sub in ("add", "stage") and not commit_seen:
        whole = False
        specs, after_ddash = [], False
        for a in args:
            if a == "--":
                after_ddash = True
                continue
            if not after_ddash and a.startswith("--"):
                if a in ("--all", "--update", "--no-ignore-removal"):
                    whole = True
                continue
            if not after_ddash and a.startswith("-") and len(a) > 1:
                if "A" in a[1:] or "u" in a[1:]:
                    whole = True
                continue
            specs.append(a)
        if whole or not specs:
            adds.append((anchor, ""))
        else:
            for s in specs:
                # `.` and `:/` are directories/magic for the whole tree; both
                # resolve to a path git status can expand with -uall.
                adds.append((anchor, "" if s in (":/", ":/.") else _abs(anchor, s)))

print("COMMIT\t%s" % ("1" if stage_all else "0"))
for anchor, spec in adds:
    print("ADD\t%s\t%s" % (anchor, spec))
PYEOF

# Payload bytes use stdin: environment strings have a much smaller per-value
# limit on Linux. A failed classifier must never become an unevaluated pass.
if ! CLASS="$(python3 -c "$_PC_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-publication-commits.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
# The classifier answers on its FIRST line and may append ADD lines after it, so
# every field read below is taken from that first line explicitly. `cut -f1` over
# the whole blob would compare "COMMIT<newline>ADD..." against COMMIT and this
# guard would stand down on exactly the commands it was extended to cover.
CLASS_HEAD="$(printf '%s\n' "$CLASS" | sed -n '1p')"
case "$(printf '%s' "$CLASS_HEAD" | cut -f1)" in
  COMMIT) ;;
  *) exit 0 ;;
esac

STAGE_ALL="$(printf '%s' "$CLASS_HEAD" | cut -f2)"

# --- WHICH REPOSITORY IS THIS COMMAND TALKING TO? --------------------------
# ONE resolver, shared by every guard that asks (scripts/lib/git-jurisdiction.sh),
# never a local copy — a copy is how the same hole ended up in five files, and
# this file's own copy was the narrowest of them rather than the absent one.
_PB_GJ="$(richos_git_anchor "$INPUT" "commit")"
PB_ANCHOR="$(printf '%s' "$_PB_GJ" | cut -f2)"
[ -n "$PB_ANCHOR" ] || PB_ANCHOR="$PWD"

PB_REPO="$(pb_repo_root "$PB_ANCHOR" 2>/dev/null || true)"
[ -n "$PB_REPO" ] || exit 0

# --- GOVERNANCE: the artifact's OWN repository decides ---------------------
# "Am I governed?" and "what am I inspecting?" are the same question here, and
# they are now asked of the SAME repository: the declaration loaded immediately
# below is read out of $PB_REPO — which is also the thing being judged. Two
# questions about one repository cannot disagree; that is the whole fix, and it
# needs no extra comparison to hold.
#
# The seat is deliberately given NO VETO. It used to have one: an unadopted seat
# exited before this point, which is exactly why richos-hq's committed
# .row-currency and .ceo-todos were read by nothing at all while the repository
# took 28 commits in a day. The seat is REPORTED when it differs from the
# artifact's repository, and never obeyed — a guard that switched itself off on
# a seat mismatch would have let through a merge that was correctly refused.
if [ -n "${SEAT_ROOT}" ]; then
    richos_assert_jurisdiction "scripts/hooks/guard-publication-commits.sh" "${SEAT_ROOT}" "$PB_REPO" "commit in" "proceeds" || true
fi

PB_DECL_RC=0
pb_load_declaration "$PB_REPO" || PB_DECL_RC=$?
case "$PB_DECL_RC" in
  0) ;;
  1) exit 0 ;;   # this repository declares no publication boundary
  *) pb_broken_banner "guard-publication-commits.sh" "$PB_BROKEN_REASON" >&2; exit 2 ;;
esac

if ! pb_resolve_sources "$PB_REPO"; then
    pb_broken_banner "guard-publication-commits.sh" "$PB_BROKEN_REASON" >&2
    exit 2
fi

# --- What is about to become history ---------------------------------------
# Three sources, because "what this commit will contain" has three answers and
# only the first was ever consulted:
#   1. the index, for staging that already happened;
#   2. tracked modifications, when the commit itself is a `-a`;
#   3. the working-tree paths this same command will stage first.
#
# The two are kept APART, because they answer with different BYTES. What the
# index holds for a path and what the working tree holds for it are the same
# thing only until someone edits the file after staging it — and a `-a` or a
# `git commit -- <path>` commits the WORKING TREE version. Reading `git show
# :path` for those, as the first version did for everything, scans bytes that
# are not the ones becoming history.
STAGED_INDEX="$(git -C "$PB_REPO" diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)"
STAGED_WORKTREE=""
if [ "$STAGE_ALL" = "1" ]; then
    STAGED_WORKTREE="$(git -C "$PB_REPO" diff --name-only --diff-filter=ACMR 2>/dev/null || true)"
fi

# (3) — the ADD lines. Each names an anchor directory and a pathspec; an anchor
# that resolves to a DIFFERENT repository is dropped, so an unrelated `git -C
# elsewhere add -A` in the same command line cannot widen this scan.
PB_SPECS=""
PB_SPEC_ALL=0
while IFS=$'\t' read -r _tag _anchor _spec; do
    [ "$_tag" = "ADD" ] || continue
    [ -n "$_anchor" ] || continue
    _anchor_repo="$(pb_repo_root "$_anchor" 2>/dev/null || true)"
    [ "$_anchor_repo" = "$PB_REPO" ] || continue
    if [ -z "$_spec" ]; then
        PB_SPEC_ALL=1
    else
        PB_SPECS="${PB_SPECS}${_spec}"$'\t'
    fi
done <<CLASS_EOF
$CLASS
CLASS_EOF

if [ "$PB_SPEC_ALL" = "1" ] || [ -n "$PB_SPECS" ]; then
    # -uall is the whole point: without it a wholly-new directory arrives as one
    # entry ("?? docs/notes/") that has no bytes to scan. With it, git names
    # every file under it. -z because a path may contain a space or a newline,
    # and a path this guard cannot parse is a path it does not scan.
    #
    # A pathspec that matches nothing makes git exit non-zero; that is not a
    # reason to refuse a commit, so the status call is allowed to fail and the
    # index half still stands.
    # The status output is PIPED into the parser and never stored in a shell
    # variable on the way. Bash cannot hold a NUL byte, so a command
    # substitution silently DROPS the -z record separators and every path
    # concatenates into one unusable string — measured here on the first run,
    # where two new transcripts arrived as a single path that matched no file
    # and the guard passed. Same class as the NUL-by-byte-count test further
    # down: the shell is the wrong place to carry binary framing.
    _PB_STATUS_PARSER='
import sys
# Porcelain v1 -z: "XY path\0", and for a rename/copy the ORIGINAL path follows
# as its own field. Deletions carry no content to scan.
fields = sys.stdin.buffer.read().split(b"\0")
i = 0
while i < len(fields):
    rec = fields[i]
    i += 1
    if len(rec) < 4:
        continue
    xy = rec[:2].decode("utf-8", "replace")
    path = rec[3:].decode("utf-8", "replace")
    if xy[0] in "RC":
        i += 1          # consume the original path
    if xy[0] == "D" or xy[1] == "D":
        continue
    if not path:
        continue
    if "\n" in path:
        # This list travels to the manifest one path per line. A path with a
        # newline in it cannot, and dropping it quietly would be a file that
        # went unscanned while the run reported clean.
        print("__PB_UNREPRESENTABLE_PATH__")
        continue
    print(path)
'
    if [ "$PB_SPEC_ALL" = "1" ]; then
        PB_PENDING="$(git -C "$PB_REPO" status --porcelain -z --untracked-files=all 2>/dev/null \
                      | python3 -c "$_PB_STATUS_PARSER" 2>/dev/null || true)"
    else
        PB_ARGS=()
        while IFS= read -r _s; do
            [ -n "$_s" ] || continue
            PB_ARGS+=("$_s")
        done <<SPEC_EOF
$(printf '%s' "$PB_SPECS" | tr '\t' '\n')
SPEC_EOF
        PB_PENDING="$(git -C "$PB_REPO" status --porcelain -z --untracked-files=all -- "${PB_ARGS[@]}" 2>/dev/null \
                      | python3 -c "$_PB_STATUS_PARSER" 2>/dev/null || true)"
    fi
    case "$PB_PENDING" in
      *__PB_UNREPRESENTABLE_PATH__*)
        pb_broken_banner "guard-publication-commits.sh" \
            "a path in this repository's working tree contains a newline, and this guard passes paths to its scanner one per line. It will not scan the rest and report the result as clean. Rename the file, then commit." >&2
        exit 2 ;;
    esac
    STAGED_WORKTREE="$STAGED_WORKTREE
$PB_PENDING"
fi

STAGED_INDEX="$(printf '%s\n' "$STAGED_INDEX" | LC_ALL=C sort -u | sed '/^$/d')"
STAGED_WORKTREE="$(printf '%s\n' "$STAGED_WORKTREE" | LC_ALL=C sort -u | sed '/^$/d')"
[ -n "$STAGED_INDEX$STAGED_WORKTREE" ] || exit 0

WORK="$(mktemp -d -t pub-boundary-commit.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

IDX=0
: > "$WORK/manifest"

# pb_materialise <index|worktree> — read repository-relative paths on stdin and
# write "<path><TAB><blob>" manifest rows for the ones this guard must scan.
#
# INDEX rows come from `git show :path`: the exact bytes a plain `git commit`
# will record. WORKTREE rows come from the file on disk: the exact bytes a `-a`,
# a pathspec commit, or a `git add` in this same command will record. A path
# whose worktree copy is byte-identical to its index copy is skipped in the
# worktree pass — it was already scanned once and a second finding for the same
# file would make a refusal read as if two things were wrong.
#
# Binary blobs are skipped by a NUL test: an image or an audio file carries no
# reproducible speech text, and the old check already covered media.
#
# UNLESS PRIVATE_FILES IS DECLARED, in which case a binary blob is carried into
# the scan MARKED identity-only rather than dropped. That is not a hypothetical
# generosity toward images: the first file declared under PRIVATE_FILES is
# UTF-16, so it CONTAINS NUL BYTES and this very test calls it binary. Declaring
# "this exact file never enters the repository" and then dropping the file
# before anything looks at it would be enforcement in name only. Identity-only
# items are hashed and never handed to the text detectors, so the corpus
# predicate is untouched and so is its measured precision.
pb_materialise() {
    local mode="$1" rel BLOB _RAW _TXT _MODE
    while IFS= read -r rel; do
        [ -n "$rel" ] || continue
        if pb_allowlisted "$PB_REPO" "$PB_REPO/$rel"; then
            continue
        fi
        IDX=$((IDX + 1))
        BLOB="$WORK/blob.$IDX"
        if [ "$mode" = "index" ]; then
            if ! git -C "$PB_REPO" show ":$rel" > "$BLOB" 2>/dev/null; then
                IDX=$((IDX - 1)); continue
            fi
        else
            # Tracked AND identical to the index copy -> already covered.
            if git -C "$PB_REPO" ls-files --error-unmatch -- "$rel" >/dev/null 2>&1 &&
               git -C "$PB_REPO" diff --quiet -- "$rel" 2>/dev/null; then
                IDX=$((IDX - 1)); continue
            fi
            [ -f "$PB_REPO/$rel" ] || { IDX=$((IDX - 1)); continue; }
            cp "$PB_REPO/$rel" "$BLOB" 2>/dev/null || { IDX=$((IDX - 1)); continue; }
        fi
        # NUL test, done by byte count rather than by grep: bash cannot carry a
        # NUL in a variable and BSD grep has no portable binary-match flag, so a
        # `grep $'\0'` here silently never matches and every binary blob would
        # be handed to the text scanner.
        _MODE=full
        _RAW="$(head -c 8192 "$BLOB" | wc -c | tr -d ' ')"
        _TXT="$(head -c 8192 "$BLOB" | LC_ALL=C tr -d '\000' | wc -c | tr -d ' ')"
        if [ "$_RAW" != "$_TXT" ]; then
            if [ -z "$PB_PRIVATE_FILES" ]; then
                rm -f "$BLOB"
                IDX=$((IDX - 1))
                continue
            fi
            _MODE=identity
        fi
        # The same path as ALREADY PUBLISHED, when the public remote's main has
        # it: a word-run the public repository already carries is not a new
        # disclosure because a private record quotes it (publication-boundary.py,
        # ALREADY PUBLISHED). No published copy, no exemption.
        _PUB=""
        if [ -n "$PB_PUBLISHED_REF" ] &&
           git -C "$PB_REPO" show "$PB_PUBLISHED_REF:$rel" > "$WORK/published.$IDX" 2>/dev/null; then
            _PUB="$WORK/published.$IDX"
        fi
        printf '%s\t%s\t%s\t%s\n' "$rel" "$BLOB" "$_MODE" "$_PUB" >> "$WORK/manifest"
    done
}

# What is already public: the public remote's main, as this checkout last saw it.
# A stale copy only exempts less. Absent (a repository with no such remote), the
# exemption is simply off.
PB_PUBLISHED_REF=""
if git -C "$PB_REPO" rev-parse -q --verify refs/remotes/origin/main >/dev/null 2>&1; then
    PB_PUBLISHED_REF="refs/remotes/origin/main"
fi

# Piping into these would run them in a subshell and lose IDX; the heredocs keep
# both passes in the shell that owns the counter — the same subshell trap this
# mechanism's test suite documents at the top of its own file.
pb_materialise index <<INDEX_EOF
$STAGED_INDEX
INDEX_EOF
pb_materialise worktree <<WORKTREE_EOF
$STAGED_WORKTREE
WORKTREE_EOF

[ -s "$WORK/manifest" ] || exit 0

# --- The owner's OWN test devices ------------------------------------------
# A serial, UDID, CoreDevice UUID or tailnet hostname of a known test device is
# not private SPEECH and not a person's name, so neither scanner below can see
# it: four public audit files carried an Android serial for two weeks. The list
# lives outside every repository (scripts/lib/device-identifiers.py) and is read
# NOW, at check time. ABSENT is announced, never silently passed.
DI_RESULT="$(python3 "$SCRIPT_DIR/../lib/device-identifiers.py" --scan-manifest "$WORK/manifest" 2>/dev/null || printf 'BROKEN\tthe device-identifier scanner could not run\n')"
case "$(printf '%s' "$DI_RESULT" | sed -n '1p' | cut -f1)" in
  CLEAN) ;;
  ABSENT)
    {
        echo "=== DEVICE-IDENTIFIER DENY-LIST: ABSENT — NOT CHECKED ==="
        echo "  expected : $(printf '%s' "$DI_RESULT" | sed -n '1p' | cut -f2)"
        echo "  This is 'no check ran', not 'no identifiers'. This repository gets"
        echo "  PUBLISHED; the owner's test-device serials, UDIDs and tailnet names"
        echo "  were not looked for in this commit. One identifier per line in that"
        echo "  file, outside every repository."
    } >&2 ;;
  BROKEN)
    echo "=== DEVICE-IDENTIFIER DENY-LIST: BROKEN — REFUSING ===" >&2
    printf '  %s\n' "$(printf '%s' "$DI_RESULT" | sed -n '1p' | cut -f2-)" >&2
    exit 2 ;;
  FOUND)
    {
        echo "=== DEVICE IDENTIFIER: BLOCKED ==="
        echo "  Refusing this commit in $PB_REPO — it carries an identifier of a known test"
        echo "  device (serial, UDID, CoreDevice UUID or tailnet name), redacted below:"
        printf '%s\n' "$DI_RESULT" | tail -n +2 | while IFS="$(printf '\t')" read -r _l _p; do
            [ -n "$_l" ] || continue
            echo "    - $_l (matches $_p)"
        done
        echo ""
        echo "  FIX: replace it with a neutral label — <android-phone>, <iphone>,"
        echo "  tail1a2b3c.ts.net — and commit again. Nothing from the owner's phones"
        echo "  goes into this repository. The list is read from"
        echo "  $(python3 "$SCRIPT_DIR/../lib/device-identifiers.py" --list-path); edit that file, never this"
        echo "  repository, if an entry is wrong."
    } >&2
    exit 2 ;;
  *)
    echo "ERROR: guard-publication-commits.sh: unexpected device-identifier scanner output — refusing (fail-closed)" >&2
    exit 2 ;;
esac

# --- One machine's OWN figures ----------------------------------------------
# A reviewed branch put one Mac's measurements into public code: its total RAM,
# its free memory and its private memory floor in fixtures named for "this Mac"
# and a nightly. Neither the device list nor the speech scanners look for a
# measured value. scripts/lib/machine-figures.py reads the host's RAM size and
# the private limits file NOW and judges only the lines this commit adds.
# Wording that means the user's Mac ("only a restart of this Mac does") is kept.
MF_RESULT="$(python3 -B "$SCRIPT_DIR/../lib/machine-figures.py" --scan-manifest "$WORK/manifest" --repo "$PB_REPO" 2>/dev/null || printf 'BROKEN\tthe machine-figures scanner could not run\n')"
case "$(printf '%s' "$MF_RESULT" | sed -n '1p' | cut -f1)" in
  CLEAN) ;;
  BROKEN)
    echo "=== MACHINE FIGURES: BROKEN — REFUSING ===" >&2
    printf '  %s\n' "$(printf '%s' "$MF_RESULT" | sed -n '1p' | cut -f2-)" >&2
    exit 2 ;;
  FOUND)
    {
        echo "=== MACHINE FIGURES: BLOCKED ==="
        echo "  Refusing this commit in $PB_REPO — it adds one machine's own figures or an"
        echo "  incident label to public code:"
        printf '%s\n' "$MF_RESULT" | tail -n +2 | while IFS="$(printf '\t')" read -r _l _r _w; do
            [ -n "$_l" ] || continue
            echo "    - $_l ($_r: $_w)"
        done
        echo ""
        echo "  FIX: use a synthetic value (a size no real Mac is likely to have), name the"
        echo "  fixture for what it tests rather than for a machine or a nightly, and keep the"
        echo "  dated measurement in a private record. A user-facing \"this Mac\" with no"
        echo "  figure beside it is fine."
    } >&2
    exit 2 ;;
  *)
    echo "ERROR: guard-publication-commits.sh: unexpected machine-figures scanner output — refusing (fail-closed)" >&2
    exit 2 ;;
esac

JOB="$WORK/job.json"
PB_MANIFEST="$WORK/manifest" PB_JOB="$JOB" \
  PB_MIN_SPEECH="$PB_MIN_SPEECH_LINES" PB_MIN_QUOTE="$PB_MIN_QUOTE_WORDS" \
  PB_MAX_FILES="$PB_CORPUS_MAX_FILES" PB_MAX_BYTES="$PB_CORPUS_MAX_BYTES" \
  PB_MAY_BE_EMPTY="$PB_CORPUS_MAY_BE_EMPTY" \
  PB_SOURCES_RAW="$PB_SOURCES_OK" \
  PB_PRIVATE_FILES="$PB_PRIVATE_FILES" \
  python3 -c '
import json, os
items = []
with open(os.environ["PB_MANIFEST"], encoding="utf-8") as fh:
    for line in fh:
        line = line.rstrip("\n")
        if not line:
            continue
        parts = line.split("\t")
        label, path = parts[0], parts[1]
        item = {"label": label, "path": path}
        if len(parts) > 2 and parts[2] == "identity":
            item["identity_only"] = True
        if len(parts) > 3 and parts[3]:
            item["published_path"] = parts[3]
        items.append(item)
job = {
    "min_speech_lines": int(os.environ.get("PB_MIN_SPEECH", "8")),
    "min_quote_words": int(os.environ.get("PB_MIN_QUOTE", "10")),
    "corpus_max_files": int(os.environ.get("PB_MAX_FILES", "4000")),
    "corpus_max_bytes": int(os.environ.get("PB_MAX_BYTES", "67108864")),
    "corpus_may_be_empty": os.environ.get("PB_MAY_BE_EMPTY", "0") == "1",
    "sources": [s for s in os.environ.get("PB_SOURCES_RAW", "").split("\t") if s],
    "private_files": (os.environ.get("PB_PRIVATE_FILES", "") or "").split(),
    "items": items,
}
with open(os.environ["PB_JOB"], "w", encoding="utf-8") as fh:
    json.dump(job, fh)
' || { echo "ERROR: guard-publication-commits.sh: could not build the scan job — refusing (fail-closed)" >&2; exit 2; }

RESULT="$(pb_scan "$JOB" || true)"

case "$(printf '%s' "$RESULT" | sed -n '1p' | cut -f1)" in
  CLEAN)
    pb_unread_note "guard-publication-commits.sh" "$RESULT"
    exit 0 ;;
  BROKEN)
    pb_broken_banner "guard-publication-commits.sh" "$(printf '%s' "$RESULT" | sed -n '1p' | cut -f2-)" >&2
    exit 2 ;;
  BLOCK)
    pb_refusal "guard-publication-commits.sh" \
        "Refusing this commit in $PB_REPO — the staged tree carries private material." \
        "$RESULT" "$PB_REPO" "$PB_SOURCES_SKIPPED" >&2
    exit 2 ;;
  *)
    echo "ERROR: guard-publication-commits.sh: unexpected scanner output — refusing (fail-closed)" >&2
    exit 2 ;;
esac

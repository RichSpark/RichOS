#!/usr/bin/env bash
#
# guard-ceo-todos-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# Refuses a commit into a repository whose CEO TODOs is making a claim it
# cannot back: an item sitting in a "waiting on the CEO" section without the
# four fields, or naming an artifact that does not exist on disk.
#
# The predicate, the two states, the declaration format and the honest list of
# what none of this can catch all live in scripts/lib/ceo-todos.sh — one place,
# because a predicate in two copies is the defect class this engine keeps
# finding in itself.
#
# ===========================================================================
# WHY THE COMMIT, AND WHY *ONLY* THE COMMIT
# ===========================================================================
# A PreToolUse[Write|Edit] guard was the obvious first choice and it was
# rejected on evidence, not taste. In this environment the dominant way a
# markdown record actually changes is the Bash tool — `sed -i`, a heredoc, a
# small script — and agents are explicitly steered that way. A Write-matcher
# guard would therefore miss most real edits while reporting a clean session,
# which is the same shape as the "18/18 suites" defect it is meant to prevent:
# a check whose scope quietly excludes the thing that is actually broken. Its
# sibling guard-publication-writes.sh documents the identical discovery from
# the other direction — 137 of 140 leaked files never touched a Write tool.
#
# At `git commit` provenance stops mattering. sed, heredoc, editor, generator,
# `git mv`, another agent's leftovers: the index sees all of them identically.
#
# ===========================================================================
# EVERY COMMIT, NOT ONLY THE ONES THAT TOUCH THE RECORD
# ===========================================================================
# The original failure was not a bad row being written. It was a bad row
# SITTING there for weeks while everyone read past it. A guard that only fires
# when the record changes cannot see that, and an artifact can be deleted long
# after the row that promises it was committed.
#
# So the record is re-checked at every commit in a declaring repository, and
# the refusal says plainly whether this commit introduced the problem or merely
# ran into one that was already there. The cost is that an unrelated commit can
# be blocked by somebody else's stale row; that is the intended trade. A record
# that lies about the most expensive person in the company is a defect of the
# same severity as the one guard-publication-commits.sh refuses.
#
# ===========================================================================
# SCOPE — DECLARED BY THE DESTINATION, NOT BY THE SESSION
# ===========================================================================
# The repository that owns the record may not have adopted this engine, and in
# the case this guard was built for it has not. So, exactly as the publication
# pair does: the SESSION must be governed, and the DESTINATION repository
# declares its own scope in a committed `.ceo-todos`. A governed session
# committing into an unadopted repository is fully covered.
#
# WHAT THAT CANNOT CATCH, said here rather than discovered later:
#   * A commit made from a session seated IN the unadopted repository. The
#     engine stands down for those sessions and this hook never loads. The one
#     change that would close it is an `orchestration.config` in that
#     repository — a deliberate adoption decision, not something a guard should
#     help itself to.
#   * A repo-local git hook would not close it either, and that was checked
#     rather than assumed: this machine sets core.hooksPath globally to a
#     directory owned by an unrelated identity guard, so `.git/hooks/pre-commit`
#     in the target repository never executes at all.
#   * `git merge`, `git cherry-pick`, `git am`, `git rebase` — they create
#     commits without running `git commit`. Merge is the acceptable gap; the
#     content was gated when it was committed on the source branch.
#   * `--no-verify` does not apply here (this is not a git hook), but a commit
#     made outside any governed session is the same hole by another road.
#
# NO LIVE OVERRIDE — deliberately, and for the publication pair's reason: the
# thing that failed was in-the-moment judgment about whether an item was really
# ready. An in-prompt escape token would rebuild exactly that. The way through
# is to fix the item, or to mark it BLOCKED-ON-RICH and move it to the
# preparer's section — which is the system working.
#
# PRECISION: fires only on a command that actually contains `git commit`.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-ceo-todos-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-ceo-todos-commits.sh
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
        echo "  hook: scripts/hooks/guard-ceo-todos-commits.sh"
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
# talking to? It was answered in five hand-copied blocks and every one of them
# missed `cd <repo> && git commit`. REFUSING TO START is deliberate — a guard
# that resolved the repository by guessing would be the 2026-09-01 bypass with
# a nicer error message.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-ceo-todos-commits.sh"
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
    root_failure_banner "scripts/hooks/guard-ceo-todos-commits.sh" >&2
    exit 2
fi
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
    unevaluated_or_continue "guard-ceo-todos-commits.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this commit leaves the CEO's own list saying something that is no longer true"
fi

_CT_LIB="$SCRIPT_DIR/../lib/ceo-todos.sh"
if [ ! -f "$_CT_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-ceo-todos-commits.sh"
        echo "  scripts/lib/ceo-todos.sh is missing at: $_CT_LIB"
        echo "  This guard's entire predicate lives there. Without it it cannot"
        echo "  tell a prepared item from an unprepared one, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/ceo-todos.sh
. "$_CT_LIB"

# --- Is this a commit at all, and where? -----------------------------------
# Classified in python, assigned via a quoted heredoc first for the same bash
# 3.2 reason guard-worktree-removal.sh documents: a `)` inside a character
# class mis-scans as the close of a $( ) substitution on macOS's /bin/bash.
# dispatch-words: commit
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
read -r -d '' _CT_CLASSIFIER <<'PYEOF' || true
import json, os, re, sys

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    print("PASS"); raise SystemExit
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    print("PASS"); raise SystemExit
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""

if not re.search(r"\bgit\b[^\n;|&]*\bcommit\b", cmd):
    print("PASS"); raise SystemExit

# WHERE the command points is decided by scripts/lib/git-jurisdiction.sh, not
# here. This file used to read an explicit `git -C` and fall back to the payload
# cwd — the resolution that `cd <repo> && git commit` walks straight past.

# `-a`/`--all` commits tracked modifications that are not in the index, so the
# staged blob would not be the bytes that land. Quoted spans are stripped first:
# `git commit -m "handle -a properly"` is not a `-a` commit.
unquoted = re.sub(r'"[^"]*"', " ", cmd)
unquoted = re.sub(r"'[^']*'", " ", unquoted)
stage_all = bool(re.search(r"(?:^|\s)-[a-zA-Z]*a[a-zA-Z]*\b", unquoted)
                 or re.search(r"(?:^|\s)--all\b", unquoted))

print("COMMIT\t%s" % ("1" if stage_all else "0"))
PYEOF

# Payload bytes use stdin: environment strings have a much smaller per-value
# limit on Linux. A failed classifier must never become an unevaluated pass.
if ! CLASS="$(python3 -c "$_CT_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-ceo-todos-commits.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
case "$(printf '%s' "$CLASS" | cut -f1)" in
  COMMIT) ;;
  *) exit 0 ;;
esac

STAGE_ALL="$(printf '%s' "$CLASS" | cut -f2)"

# --- Which removed paths can the record be about? --------------------------
# stdin: the removed paths, repository-relative, one per line. Prints each one
# the record or its CEO-facing surface names; prints nothing for the rest.
# Quoted heredoc for the bash 3.2 reason given above the classifier.
read -r -d '' _CT_NAMED_REMOVALS <<'PYEOF' || true
import os, re, sys

env = os.environ.get
repo = os.path.realpath(env("CT_NR_REPO") or ".")
main = os.path.realpath(env("CT_NR_MAIN") or repo)
roots = {}
for entry in (env("CT_NR_ROOTS") or "").split("\t"):
    if "=" in entry:
        k, v = entry.split("=", 1)
        roots[k] = os.path.realpath(v)
with open(env("CT_NR_RECORD_TEXT"), encoding="utf-8", errors="replace") as fh:
    text = fh.read()

named = set()
for span in re.findall(r"`([^`\n]+)`", text):
    for tok in span.split():
        tok = tok.strip("\"'")
        if "/" not in tok:
            continue
        prefix, rest = tok.split("/", 1)
        if prefix in roots and rest:
            named.add(os.path.normpath(os.path.join(roots[prefix], rest)))
for base in (repo, main):
    for rel in (env("CT_NR_RECORD"), env("CT_NR_VIEW"), env("CT_NR_README")):
        if rel:
            named.add(os.path.normpath(os.path.join(base, rel)))
cold = (env("CT_NR_COLD") or "").strip("/")
colds = [os.path.normpath(os.path.join(b, cold)) for b in (repo, main)] if cold else []

def is_named(path):
    if any(path == c or path.startswith(c + os.sep) for c in colds):
        return True
    return any(path == n or path.startswith(n + os.sep) for n in named)

seen = []
for rel in sys.stdin.read().splitlines():
    rel = rel.strip()
    if not rel or rel in seen:
        continue
    if any(is_named(os.path.normpath(os.path.join(b, rel))) for b in (repo, main)):
        seen.append(rel)
print("\n".join(seen))
PYEOF

# --- WHICH REPOSITORY IS THIS COMMAND TALKING TO? --------------------------
# ONE resolver, shared by every guard that asks (scripts/lib/git-jurisdiction.sh),
# never a local copy — a copy is how the same hole ended up in five files.
_CT_GJ="$(richos_git_anchor "$INPUT" "commit")"
CT_ANCHOR="$(printf '%s' "$_CT_GJ" | cut -f2)"
[ -n "$CT_ANCHOR" ] || CT_ANCHOR="$PWD"

CT_REPO="$(ct_repo_root "$CT_ANCHOR" 2>/dev/null || true)"
[ -n "$CT_REPO" ] || exit 0

# --- GOVERNANCE: the artifact's OWN repository decides ---------------------
# "Am I governed?" and "what am I inspecting?" are the same question here, and
# they are now asked of the SAME repository: the declaration loaded immediately
# below is read out of $CT_REPO — which is also the thing being judged. Two
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
    richos_assert_jurisdiction "scripts/hooks/guard-ceo-todos-commits.sh" "${SEAT_ROOT}" "$CT_REPO" "commit in" "proceeds" || true
fi

CT_DECL_RC=0
ct_load_declaration "$CT_REPO" || CT_DECL_RC=$?
case "$CT_DECL_RC" in
  0) ;;
  1) exit 0 ;;   # this repository declares no CEO TODOs — nothing to enforce
  *) ct_broken_banner "guard-ceo-todos-commits.sh" "$CT_BROKEN_REASON" >&2; exit 2 ;;
esac

# --- Which bytes are the record about to be? -------------------------------
# Staged blob when the record is staged (those are the bytes that land); the
# worktree copy otherwise, including under `-a`, where an unstaged modification
# is what gets committed.
WORK="$(mktemp -d -t ceo-todos-commit.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
SUBJECT="$WORK/record.md"
TOUCHED=0

STAGED_LIST="$(git -C "$CT_REPO" diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)"
case "
$STAGED_LIST
" in
  *"
$CT_TODO_RECORD
"*) TOUCHED=1 ;;
esac

if [ "$TOUCHED" -eq 1 ] && [ "$STAGE_ALL" -eq 0 ]; then
    if ! git -C "$CT_REPO" show ":$CT_TODO_RECORD" > "$SUBJECT" 2>/dev/null; then
        rm -f "$SUBJECT"
    fi
fi
if [ ! -s "$SUBJECT" ]; then
    if [ -f "$CT_REPO/$CT_TODO_RECORD" ]; then
        cp "$CT_REPO/$CT_TODO_RECORD" "$SUBJECT" 2>/dev/null || true
    fi
fi

# --- The SURFACE around the record, resolved the same way ------------------
# THE ENTRY POINT IS CHECKED FROM THE SAME BYTES AS THE RECORD, and it has to
# be: the failure being gated is a view that no longer matches the record, and
# the commonest way to produce one is to stage a record change and leave the
# generated page behind. Comparing a staged record against a WORKTREE view
# would pass exactly that commit — the gate would be looking at two different
# moments in time and calling them consistent.
#
# "-" means: this file will not be there after the commit. Distinct from unset
# (read the worktree), because a DELETED entry point must read as absent, not
# as whatever is still lying on disk.
stage_view() {
    # stage_view <repo-relative path> <scratch name> -> prints the override token
    local rel="$1" tmp="$WORK/$2" staged=0 deleted=0
    [ -n "$rel" ] || { printf ''; return; }
    case "
$STAGED_LIST
" in
        *"
$rel
"*) staged=1 ;;
    esac
    case "
$DELETED_LIST
" in
        *"
$rel
"*) deleted=1 ;;
    esac
    if [ "$staged" -eq 1 ] && [ "$STAGE_ALL" -eq 0 ]; then
        if git -C "$CT_REPO" show ":$rel" > "$tmp" 2>/dev/null; then
            printf '%s' "$tmp"
            return
        fi
    fi
    if [ "$deleted" -eq 1 ] && [ "$STAGE_ALL" -eq 0 ]; then
        printf '%s' '-'
        return
    fi
    if [ -f "$CT_REPO/$rel" ]; then
        printf ''      # unset: the library reads the worktree copy
        return
    fi
    printf '%s' '-'
}

DELETED_LIST="$(git -C "$CT_REPO" diff --cached --name-only --diff-filter=D 2>/dev/null || true)"
CT_OVERRIDE_VIEW="$(stage_view "$CT_TODO_VIEW" view.md)"
CT_OVERRIDE_README="$(stage_view "$CT_ROOT_README" readme.md)"
export CT_OVERRIDE_VIEW CT_OVERRIDE_README

if [ ! -f "$SUBJECT" ] || [ ! -s "$SUBJECT" ]; then
    # The declaration names a record that is not there. That is BROKEN — never
    # a quiet pass. A guard whose subject has vanished protects nothing while
    # looking switched on, which is the failure mode this engine keeps finding.
    {
        echo "=== CEO TODOs: THE DECLARED RECORD IS NOT ON DISK — REFUSING THIS COMMIT ==="
        echo "  repository : $CT_REPO"
        echo "  declared   : $CT_TODO_RECORD  (in $CEO_TODOS_DECLARATION)"
        echo ""
        echo "  Either restore the record, or delete $CEO_TODOS_DECLARATION to stand"
        echo "  this mechanism down deliberately and visibly."
        echo "(hook: scripts/hooks/guard-ceo-todos-commits.sh)"
    } >&2
    exit 2
fi

ct_resolve_roots "$CT_REPO"

RESULT="$(ct_lint_file "$CT_TODO_RECORD" "$SUBJECT" "$CT_REPO")" || {
    echo "ERROR: guard-ceo-todos-commits.sh: the CEO-TODOs predicate could not run — refusing (fail-closed), because a checker that cannot run is not a clean record." >&2
    exit 2
}

VERDICT="$(printf '%s' "$RESULT" | sed -n '1p' | cut -f1)"
BODY="$(printf '%s\n' "$RESULT" | tail -n +2)"

case "$VERDICT" in
  CLEAN)
    # A clean pass still says what it did NOT check. The only way to silence
    # these lines is to declare the thing they name, which is the point: an
    # undeclared check must cost something visible every time, or "clean"
    # quietly grows to mean "checked" and we are back where this started.
    printf '  CEO TODOs — %s\n' "$(ct_dc_census "$BODY")" >&2
    printf '%s\n' "$BODY" | awk -F'\t' '$1=="NOTE" {printf "  CEO TODOs — NOT CHECKED: %s\n         %s\n", $2, $3}' >&2
    exit 0 ;;
  BROKEN)
    ct_broken_banner "guard-ceo-todos-commits.sh" "$(printf '%s' "$RESULT" | sed -n '1p' | cut -f2-)" >&2
    exit 2 ;;
  VIOLATIONS)
    if [ "$TOUCHED" -eq 1 ]; then
        HEADLINE="this commit changes the record, and $(printf '%s' "$RESULT" | sed -n '1p' | cut -f2) thing(s) about these TODOs are not ready"
    else
        HEADLINE="PRE-EXISTING: $(printf '%s' "$RESULT" | sed -n '1p' | cut -f2) thing(s) about this repository's CEO TODOs are not ready (this commit did not touch the record)"
        # A commit that neither edits the record nor removes a file cannot have
        # made a row false, so it must not be held hostage to somebody else's
        # stale row (hunt part 3, finding 1). The reason for checking every
        # commit still holds where a commit CAN break a claim: one that deletes
        # or renames away an artifact a row names. Those keep the refusal; all
        # others get the same report as a notice and go through.
        REMOVED_LIST="$(git -C "$CT_REPO" diff --cached --name-only --no-renames --diff-filter=D 2>/dev/null || true)"
        if [ "$STAGE_ALL" -eq 1 ]; then
            REMOVED_LIST="$REMOVED_LIST
$(git -C "$CT_REPO" diff --name-only --no-renames --diff-filter=D 2>/dev/null || true)"
        fi
        # ONLY A REMOVAL THE RECORD CAN BE ABOUT keeps the refusal (hunt part
        # 3, finding 1, still partial in the v2 re-check: ANY deletion refused,
        # so removing a file no row has ever named was held hostage to another
        # row). A removal can make a row false only if the record or its page
        # NAMES the removed path: an **Open:** or **Done-check:** path (any
        # `<prefix>/...` token inside a backticked span of the record, which is
        # deliberately wider than the CEO sections), a directory such a path
        # names, the record itself, the CEO's page, the README, or a cold-open
        # transcript. Paths are compared against BOTH this checkout and the
        # main checkout, because a declared root such as `repo=.` resolves to
        # the main one while the index belongs to whichever worktree commits.
        # A checker that cannot run refuses, as everywhere in this guard.
        NAMED_REMOVALS=""
        if [ -n "$(printf '%s' "$REMOVED_LIST" | tr -d '[:space:]')" ]; then
            if ! NAMED_REMOVALS="$(printf '%s\n' "$REMOVED_LIST" | \
                CT_NR_RECORD_TEXT="$SUBJECT" CT_NR_REPO="$CT_REPO" \
                CT_NR_MAIN="$(ct_main_checkout "$CT_REPO" 2>/dev/null || printf '%s' "$CT_REPO")" \
                CT_NR_ROOTS="$CT_ROOTS_OK" CT_NR_RECORD="$CT_TODO_RECORD" \
                CT_NR_VIEW="$CT_TODO_VIEW" CT_NR_README="$CT_ROOT_README" \
                CT_NR_COLD="$CT_COLD_OPEN_DIR" python3 -c "$_CT_NAMED_REMOVALS")"; then
                echo "ERROR: guard-ceo-todos-commits.sh: could not tell whether this commit removes a file the CEO TODOs name — refusing (fail-closed)." >&2
                exit 2
            fi
        fi
        # A commit that stages the generated entry point or the README is the
        # commonest way to make the view disagree with the record, so it is
        # judged like a record edit and keeps the refusal.
        SURFACE_STAGED=0
        for surface in "$CT_TODO_VIEW" "$CT_ROOT_README"; do
            [ -n "$surface" ] || continue
            case "
$STAGED_LIST
" in
                *"
$surface
"*) SURFACE_STAGED=1 ;;
            esac
        done
        if [ -z "$NAMED_REMOVALS" ] && [ "$SURFACE_STAGED" -eq 0 ]; then
            ct_refusal "guard-ceo-todos-commits.sh" "$HEADLINE — NOTICE ONLY, this commit removes no file the record or its page names, so it is not blocked" "$BODY" "$CT_REPO/$CT_TODO_RECORD" >&2
            exit 0
        fi
        if [ -n "$NAMED_REMOVALS" ]; then
            HEADLINE="$HEADLINE — and this commit removes $(printf '%s\n' "$NAMED_REMOVALS" | grep -c .) file(s) the record or its page names: $(printf '%s\n' "$NAMED_REMOVALS" | tr '\n' ' ' | sed 's/ *$//')"
        fi
    fi
    ct_refusal "guard-ceo-todos-commits.sh" "$HEADLINE" "$BODY" "$CT_REPO/$CT_TODO_RECORD" >&2
    exit 2 ;;
  *)
    echo "ERROR: guard-ceo-todos-commits.sh: unexpected verdict from the predicate — refusing (fail-closed)" >&2
    exit 2 ;;
esac

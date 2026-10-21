#!/usr/bin/env bash
#
# guard-completeness-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# Makes the PUBLICATION COMPLETENESS contract fire on its own, at `git commit`
# and at `git push`, instead of waiting for somebody to remember it.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# scripts/publication-completeness.sh answers the question the leak guards
# cannot:
#
#       IS EVERYTHING THAT MUST BE THERE, THERE — AND USABLE BY SOMEONE WHO
#       HAS ONLY THE PUBLIC REPOSITORY?
#
# It was built on 2026-08-29, it is correct, and it was wired into exactly two
# places: CI, and an operator's memory. Ten guards in this engine fire at
# PreToolUse without being asked. This check did not, and on 2026-08-30 the
# difference was measured in the only way that counts:
#
#   THE CONTRACT WAS RED ON `main` AND NOBODY KNEW.
#
# docs/measurements/correction-flywheel-2026-08-29/README.md cited
# `docs/plans/richos-techy-mode-2026-08-26.md`, a file that lives only in the
# private record. A reader with nothing but the public repository was being
# sent to a file they will never have. It was committed on a branch, merged to
# main, and pushed. After the merge the lander ran the test suites — because
# the suites are what he remembered — and not the completeness check, because
# nothing made him. It sat on main until an unrelated question happened to
# surface it. Every individual step was done carefully. The defect is not
# carelessness; it is that
#
#   A CHECK THAT RUNS WHEN SOMEONE REMEMBERS IS A RULE ENFORCED BY ATTENTION,
#   AND ATTENTION IS THE THING THIS ENGINE KEEPS PROVING IT CANNOT BUY.
#
# ===========================================================================
# WHERE IT FIRES, AND WHY BOTH
# ===========================================================================
# `git commit` AND `git push`. Its two sibling contract guards fire only on
# commit; this one needs the second arm, and the reason is specific rather than
# belt-and-braces.
#
#   COMMIT is where the claim is AUTHORED. Run against the branch the flywheel
#   README was written on, the commit is refused at the moment the author can
#   still fix it in one keystroke, before it is anybody else's problem. This is
#   also the arm that catches content no Write tool ever touched — `sed -i`, a
#   heredoc, a generator, a rename — which is 137-of-140 of the way real files
#   arrive here, measured by guard-publication-commits.sh's own header.
#
#   PUSH is where the claim becomes PUBLIC, and it is the only arm that can see
#   a MERGE. `git merge` creates a commit without running `git commit`, so the
#   commit arm never sees one; guard-publication-commits.sh names that gap and
#   accepts it, correctly, because the BYTES a merge carries were already gated
#   on the source branch. Completeness is not a property of bytes. It is a
#   property of the WHOLE TREE, and a merge can manufacture a defect that
#   exists on neither side of it: branch A moves a file, branch B cites the old
#   path, both commit clean, the merge is broken. That is precisely "a claim
#   that went stale because something else moved", and no per-commit check of
#   either branch can ever see it.
#
#   The asymmetry with the boundary guard is deliberate and it is about the
#   COST OF THE REMEDY, not about severity. Private bytes in history need a
#   rewrite and a force-push, so the boundary must refuse before they exist and
#   a push-time check would arrive too late to help. A dangling citation in
#   history needs a follow-up commit. So gating the push is cheap, useful, and
#   the last chokepoint before a public reader can hit it — which is exactly
#   where today's miss escaped.
#
# ===========================================================================
# THE WHOLE TREE, EVERY TIME — MEASURED, NOT ASSUMED
# ===========================================================================
# The obvious economy is to check only what the commit touches. It was
# rejected on the measurement, and the measurement is the argument. richos
# @ 6d835be, 589 tracked files, 171 .md/.txt, best-of-5 end to end:
#
#   a Bash call that is neither a commit nor a push          0.014s
#   a commit in femcboost / richos-hq (stand-down)           0.076s
#   a bare commit in richos (full whole-tree check)          0.288s
#   `git add -A && git commit` (+ the scratch index)         0.302s
#   a push in richos (full whole-tree check)                 0.275s
#     of which the predicate itself is                       0.20s
#   guard-publication-commits.sh here, for comparison        0.091s
#
# A scoped check would have bought roughly nothing and cost the only thing that
# matters here. THREE OF THE FOUR CHECKS ARE NOT DIFFABLE EVEN IN PRINCIPLE:
#
#   * a citation goes stale when the file it names MOVES — the commit that
#     breaks it does not touch the citing document at all;
#   * "shipped inert" is a claim about the whole tree's onboarding set, so
#     deleting one README paragraph anywhere breaks it;
#   * a mechanism left behind in the private tree is not in the diff by
#     definition — it is in the OTHER repository. (That one is judged at the
#     PUSH, not at the commit: see "THE COMMIT ARM LEAVES THE PRIVATE TREES TO
#     THE PUSH" below. A commit here can neither change nor fix it.)
#
# Scoping to the diff would have produced a guard that passes the commit that
# introduces the defect and never mentions it again. At three tenths of a second
# there is no trade to make, so none is made, and this guard has no "what a
# scoped check cannot catch" section because it is not scoped.
#
# COST DISCIPLINE, so this stays true on a tree ten times the size: the guard
# times itself and, past COMPLETENESS_SOFT_BUDGET_S, says so on stderr EVEN WHEN
# IT PASSES. Nobody has to discover that the guard has become the slowest thing
# in their commit; the guard reports it. If it ever exceeds the hooks.json
# timeout the host kills it, and a killed hook is a SILENT one — which is the
# defect this file exists to remove, so the advisory fires at a fraction of the
# timeout rather than at it.
#
# ===========================================================================
# `git add -A && git commit` IS ONE BASH CALL, AND THE HOOK RUNS FIRST
# ===========================================================================
# This is not a detail; it is the difference between this guard working and
# this guard being decorative, and it was found by writing the suite rather
# than by reading. A PreToolUse hook fires BEFORE the command, so in the
# dominant commit idiom here — stage and commit in one call — the new files are
# still UNTRACKED at decision time. `git ls-files` does not list them. The
# checker is right to define the published tree as what git tracks, and reading
# it naively at that instant audits the tree MINUS everything the commit is
# about to add.
#
# THE FLYWHEEL README WAS A NEW FILE. Read naively, this guard would have
# reported today's defect clean and let it through — the failure it exists to
# prevent, wearing the costume of a green tick.
#
# So when the command stages, the checker is handed a SCRATCH INDEX with the
# staging already applied. See "THE TREE THAT IS ABOUT TO EXIST" below for how,
# and for why a failure to build that view refuses rather than falls back.
#
# NOTED, BECAUSE IT IS NOT MINE TO FIX HERE: guard-publication-commits.sh reads
# `git diff --cached` at the same instant, so on `git add -A && git commit` it
# sees an EMPTY staged set and exits 0. That is the LEAK guard — the one that
# refuses private material — and the hole is in the same place for the same
# reason. It is reported in this branch's handoff rather than patched, because
# that file is being edited by another agent this session and a blind cross-edit
# is how two correct fixes become one broken merge.
#
# ===========================================================================
# THE COST THIS DOES IMPOSE, STATED PLAINLY
# ===========================================================================
# A whole-tree gate means AN UNRELATED COMMIT CAN BE BLOCKED BY A FINDING
# SOMEBODY ELSE LEFT. That is the same trade guard-ceo-todos-commits.sh makes
# and for the same reason: the original failure was not a bad line being
# written, it was a bad line SITTING there while everyone read past it. So the
# refusal reports the current-tree findings without assigning their origin.
# A local diff cannot establish who introduced a finding or when it arrived.
# In particular, a clean checkout after a merge has no uncommitted diff.
#
# THERE IS NO DEADLOCK, and that was checked rather than hoped: the checker
# reads `git ls-files` (the INDEX — so a staged fix counts) and the WORKTREE's
# bytes (so an edited fix counts). Fixing the citation, adding the missing
# file, or writing a reviewed line into `.publication-completeness` all make
# the very next commit pass. A guard you cannot commit your way out of would be
# removed within the day, and would deserve to be.
#
# NO LIVE OVERRIDE — deliberately, and for its siblings' reason: what failed
# was in-the-moment judgement about whether a particular claim was really
# backed. An in-prompt escape token would rebuild exactly that. The way through
# is `.publication-completeness` — committed, diffable, reviewed by whoever
# lands it, and self-expiring, because an exemption that suppresses nothing is
# itself a failure.
#
# ===========================================================================
# WHAT THIS CANNOT CATCH
# ===========================================================================
# Everything scripts/publication-completeness.sh cannot catch, unchanged — that
# list lives in ONE place, in that file's header, and is not copied here. Add
# the four this chokepoint contributes:
#
#   * A merge, a cherry-pick, an `am` or a rebase in a repository nobody then
#     pushes. Merge is covered in practice because the land sequence pushes; a
#     merge left unpushed is not.
#   * A commit or push from a session this engine does not govern, or from
#     outside any session at all. Same hole every hook in this family has.
#   * The PUSH arm checks the CURRENT WORKING TREE, not the refs being pushed.
#     `git push origin some-other-branch` from a checkout of main audits main.
#     Reading the pushed ref would mean checking out a tree behind the
#     operator's back, which is a worse thing to do than to have this gap.
#   * A capability with NO DECLARATION FILE AT ALL is invisible to the checker
#     by construction, so it is invisible here too — automating a check does
#     not widen it. richos-hq/RICH-TODOs.md is a live instance: an operator
#     backlog with no template, no contract and no gate, and this guard changes
#     nothing about it. It is queued separately, and it is the one hole a
#     reader of this file should leave still knowing about.
#
# PRECISION: fires only on a command that actually contains `git commit` or
# `git push`, and only in a repository that has declared itself publication-
# bound. A repository with no `.publication-boundary` — which is every
# repository on this machine except one — is untouched, at the cost of a
# substring test.

set -eo pipefail

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
  hook: scripts/hooks/guard-completeness-commits.sh
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
        echo "  hook: scripts/hooks/guard-completeness-commits.sh"
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
        echo "  hook: scripts/hooks/guard-completeness-commits.sh"
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

# PLACED ABOVE THE SUBSTRING PRE-FILTER BELOW, NOT AFTER IT. That filter
# tests the RAW payload for "commit"/"push" and exits 0 on anything else,
# so an unreadable payload never reached root resolution and never reached
# this notice either — the only hook of the twenty-seven where the check was
# lost one step earlier than everywhere else. The cost of the earlier
# placement is that no entity root has been resolved yet, so the durable log
# line is skipped; the announcement is not.

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
    unevaluated_or_continue "guard-completeness-commits.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this commit is the whole of the change it claims to be"
fi

# --- The cheap door --------------------------------------------------------
# Every Bash call in the session reaches this file. The overwhelming majority
# are `ls`, `grep`, a test run — nothing this guard has any business costing.
# One substring test over the raw payload retires them before a single
# subprocess is spawned. It is deliberately OVER-inclusive (a commit MESSAGE
# containing the word "push" gets past it and is then classified properly);
# being over-inclusive can only cost time, never coverage.
# dispatch-words: commit push
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
case "$INPUT" in
    *commit*|*push*) ;;
    *) exit 0 ;;
esac

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-completeness-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
    root_failure_banner "scripts/hooks/guard-completeness-commits.sh" >&2
    exit 2
fi
_PB_LIB="$SCRIPT_DIR/../lib/publication-boundary.sh"
if [ ! -f "$_PB_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-completeness-commits.sh"
        echo "  scripts/lib/publication-boundary.sh is missing at: $_PB_LIB"
        echo "  Adoption of the completeness contract is declared by the SAME"
        echo "  .publication-boundary file the leak guards read, through this"
        echo "  library. Without it this guard cannot tell whether the tree it"
        echo "  is looking at is published at all, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/publication-boundary.sh
. "$_PB_LIB"

# --- Is this a commit or a push, and where? --------------------------------
# Assigned via a quoted heredoc first for the bash 3.2 reason
# guard-worktree-removal.sh documents: a `)` inside a character class
# mis-scans as the close of a $( ) substitution on macOS's /bin/bash.
read -r -d '' _CC_CLASSIFIER <<'PYEOF' || true
import json, os, re, sys

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    print("PASS"); raise SystemExit
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    print("PASS"); raise SystemExit
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""

# QUOTED SPANS ARE STRIPPED FIRST, and this is not cosmetic. A commit whose
# MESSAGE is "stop pushing to main" is not a push, and a classifier that
# thought it was would audit the tree on a wholly unrelated verb. Its sibling
# learned the same lesson on `-a`; the fix is the same and is applied before
# any matching.
unquoted = re.sub(r'"[^"]*"', " ", cmd)
unquoted = re.sub(r"'[^']*'", " ", unquoted)

# `git ... commit` / `git ... push`, tolerating -C/-c/flags in between but never
# crossing a statement separator, so an unrelated later `git` cannot bleed in.
verb = ""
if re.search(r"\bgit\b[^\n;|&]*\bcommit\b", unquoted):
    verb = "commit"
elif re.search(r"\bgit\b[^\n;|&]*\bpush\b", unquoted):
    verb = "push"
if not verb:
    print("PASS"); raise SystemExit

# DOES THIS COMMAND STAGE ANYTHING BEFORE IT COMMITS? This is the difference
# between auditing the tree that exists and auditing the tree that is about to.
# `git add -A && git commit -m x` is ONE Bash call, and a PreToolUse hook runs
# BEFORE any of it — so at decision time the new file is still untracked, absent
# from the index, and invisible to a checker that (correctly) defines the
# published tree as what git tracks. The flywheel README was a NEW file. Read
# naively, this guard would have reported it clean and let it through, which is
# the failure it exists to prevent, wearing the costume of a green tick.
stages = bool(re.search(r"\bgit\b[^\n;|&]*\badd\b", unquoted)) or \
         bool(re.search(r"(?:^|\s)-[a-zA-Z]*a[a-zA-Z]*\b", unquoted)) or \
         bool(re.search(r"(?:^|\s)--all\b", unquoted))

# WHERE the command points is NOT decided here any more. It used to be, in this
# file and in four others, by reading an explicit `git -C` and falling back to
# the payload cwd — and that resolution had a hole the size of a shell builtin:
# `cd <repo> && git commit` names no -C, so the session's repository was judged
# instead of the one being committed to. See scripts/lib/git-jurisdiction.sh.
print("%s\t%s" % (verb, "1" if stages else "0"))
PYEOF

# Payload bytes use stdin: environment strings have a much smaller per-value
# limit on Linux. A failed classifier must never become an unevaluated pass.
if ! CLASS="$(python3 -c "$_CC_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-completeness-commits.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
VERB="$(printf '%s' "$CLASS" | cut -f1)"
case "$VERB" in
  commit|push) ;;
  *) exit 0 ;;
esac
STAGES="$(printf '%s' "$CLASS" | cut -f2)"

# --- WHICH REPOSITORY IS THIS COMMAND TALKING TO? --------------------------
# ONE resolver, shared by every guard that asks (scripts/lib/git-jurisdiction.sh),
# never a local copy — a copy is how the same hole ended up in five files.
_CC_GJ="$(richos_git_anchor "$INPUT" "commit push")"
CC_ANCHOR="$(printf '%s' "$_CC_GJ" | cut -f2)"
[ -n "$CC_ANCHOR" ] || CC_ANCHOR="$PWD"

CC_REPO="$(pb_repo_root "$CC_ANCHOR" 2>/dev/null || true)"
[ -n "$CC_REPO" ] || exit 0

# --- GOVERNANCE: the artifact's OWN repository decides ---------------------
# "Am I governed?" and "what am I inspecting?" are the same question here, and
# they are now asked of the SAME repository: the declaration loaded immediately
# below is read out of $CC_REPO — which is also the thing being judged. Two
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
    richos_assert_jurisdiction "scripts/hooks/guard-completeness-commits.sh" "${SEAT_ROOT}" "$CC_REPO" "commit in" "proceeds" || true
fi

# --- Adoption: ONE declaration, read by both contracts ---------------------
# `.publication-boundary` is what says "this repository gets published", and it
# switches on the leak guards AND this one. A second file saying the same thing
# would be a copy of a fact, and copies of facts are what this engine keeps
# deleting. rc 1 is the stand-down for every repository that never goes public,
# which is all of them but one.
CC_DECL_RC=0
pb_load_declaration "$CC_REPO" || CC_DECL_RC=$?
case "$CC_DECL_RC" in
  0) ;;
  1) exit 0 ;;
  *) pb_broken_banner "guard-completeness-commits.sh" "$PB_BROKEN_REASON" >&2; exit 2 ;;
esac

CHECKER="$ENGINE_ROOT/scripts/publication-completeness.sh"
if [ ! -f "$CHECKER" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-completeness-commits.sh"
        echo "  scripts/publication-completeness.sh is missing at: $CHECKER"
        echo "  This guard is a CHOKEPOINT, not a predicate — the entire"
        echo "  decision lives in that script, so that there is exactly one"
        echo "  implementation of the contract and CI and this hook can never"
        echo "  disagree about what 'complete' means. Without it there is"
        echo "  nothing to run, and this guard will not invent a weaker answer."
    } >&2
    exit 2
fi

# --- THE TREE THAT IS ABOUT TO EXIST ---------------------------------------
# When the command stages (`git add …`, or a `-a` commit), the set of published
# files after it runs is not the set before it. The checker asks git what the
# tree is, so it is given a SCRATCH INDEX with the staging already applied:
#
#   copy the real index -> a temp file
#   GIT_INDEX_FILE=<temp> git add -A -N
#
# `-N` records INTENT only: the path enters the index, no blob is written for
# it, and the real index is never touched — `git status` afterwards is what it
# was. `-A` is used rather than replaying the command's own pathspecs because
# re-executing fragments of somebody's command line is a worse idea than being
# slightly over-inclusive, and the over-inclusion is honest: an untracked,
# un-ignored file in a published repository is a file on its way in. It also
# picks up the reverse case, which is the subtler one — `rm docs/x.md` followed
# by `git commit -a` leaves x.md in the real index, so a citation of it would
# still "resolve" against a file that is being deleted.
#
# IF THE AUGMENTATION FAILS, THIS REFUSES. Falling back to the plain index
# would quietly narrow the guard's scope to exactly the blind spot described
# above, and report a green tick over it.
if [ "$STAGES" = "1" ]; then
    CC_IDX="$(mktemp -t completeness-index.XXXXXX 2>/dev/null || true)"
    CC_REAL_IDX="$(git -C "$CC_REPO" rev-parse --git-path index 2>/dev/null || true)"
    case "$CC_REAL_IDX" in
        /*) ;;
        "") ;;
        *) CC_REAL_IDX="$CC_REPO/$CC_REAL_IDX" ;;
    esac
    if [ -n "$CC_IDX" ] && [ -n "$CC_REAL_IDX" ] && [ -f "$CC_REAL_IDX" ] \
       && cp "$CC_REAL_IDX" "$CC_IDX" 2>/dev/null \
       && GIT_INDEX_FILE="$CC_IDX" git -C "$CC_REPO" add -A -N >/dev/null 2>&1; then
        export GIT_INDEX_FILE="$CC_IDX"
        trap 'rm -f "$CC_IDX"' EXIT
    else
        rm -f "$CC_IDX" 2>/dev/null || true
        {
            echo "=== PUBLICATION COMPLETENESS: BROKEN — REFUSING (fail-closed) ==="
            echo ""
            echo "  repository : $CC_REPO"
            echo ""
            echo "  This command stages files, so the tree to audit is the one that"
            echo "  exists AFTER it runs, and building that view (a scratch index"
            echo "  copy plus 'git add -A -N') failed. Carrying on against the"
            echo "  un-staged index would check a smaller tree than the one about"
            echo "  to be committed and report a green tick over the difference —"
            echo "  which is precisely the shape of the defect this guard exists"
            echo "  to catch. Refusing instead."
        } >&2
        exit 2
    fi
fi

# ONE PREDICATE, TWO CALLERS. ci-verify.sh step 7 and this hook run the SAME
# script with the SAME arguments. A second, hook-shaped reimplementation would
# be the defect class this engine keeps finding in itself — a predicate in two
# copies, drifting.
# THE COMMIT ARM LEAVES THE PRIVATE TREES TO THE PUSH (2026-10-01). Check 4 judges
# executables in ANOTHER repository (PRIVATE_SOURCES). A commit here neither
# changed them nor can fix them, and on 2026-10-01 (2026-09-28 before it) a
# teammate's commit in its own richos workspace was refused over a Codex probe
# staged, uncommitted, in richos-hq's main checkout. So the commit arm asks every
# check about the tree being committed and skips Check 4; the PUSH arm (the land:
# Rich pushes main), the land's run on the branch and CI still run it, and Check 4
# now reads the private repository's committed HEAD, never its working tree.
# The flag is passed only to a checker that knows it (an older engine's checker
# runs whole, as before).
CC_SCOPE=""
if [ "$VERB" = "commit" ] && grep -q -- '--skip-private-trees' "$CHECKER" 2>/dev/null; then
    CC_SCOPE="--skip-private-trees"
fi
# shellcheck disable=SC2086
OUT="$(bash "$CHECKER" --root "$CC_REPO" $CC_SCOPE 2>&1)" && CC_RC=0 || CC_RC=$?
ELAPSED="$SECONDS"

# The checker paints its findings; a hook's stderr is read in a transcript, so
# the escapes come off.
OUT="$(printf '%s\n' "$OUT" | sed $'s/\033\\[[0-9;]*m//g')"

: "${COMPLETENESS_SOFT_BUDGET_S:=5}"
if [ "$ELAPSED" -ge "$COMPLETENESS_SOFT_BUDGET_S" ]; then
    {
        echo "NOTE: guard-completeness-commits.sh took ${ELAPSED}s on $CC_REPO."
        echo "  Measured at 0.29s on a 589-file tree. Past the hooks.json"
        echo "  timeout the host KILLS this hook, and a killed hook is a silent"
        echo "  one — which is the exact failure this guard was built to remove."
        echo "  Raise the timeout in hooks.json deliberately, or find out what"
        echo "  got slow. This note is not a failure; it is the guard refusing"
        echo "  to become slow quietly."
    } >&2
fi

case "$CC_RC" in
  0) exit 0 ;;
  2)
    # The checker uses 2 for BOTH "not publication-bound" and "broken". This
    # guard already gated on the declaration, so NOT APPLICABLE here means the
    # declaration moved between the two reads — a race, not a defect, and not
    # something to block a commit over. Anything else is genuinely broken and
    # fails closed, because a completeness checker that degrades quietly is the
    # thing it exists to find.
    if printf '%s' "$OUT" | grep 'NOT APPLICABLE' >/dev/null; then
        exit 0
    fi
    {
        echo "=== PUBLICATION COMPLETENESS: BROKEN — REFUSING (fail-closed) ==="
        echo ""
        echo "  repository : $CC_REPO"
        echo "  checker    : $CHECKER"
        echo ""
        printf '%s\n' "$OUT" | sed 's/^/  /'
        echo ""
        echo "  The check did not run to completion, so nothing is known about"
        echo "  whether this tree delivers what it claims. That is refused"
        echo "  rather than waved through: a green tick over a check that did"
        echo "  not finish is worse than no tick."
    } >&2
    exit 2 ;;
esac

# --- Blocked ---------------------------------------------------------------
# This audit reads the current index and worktree, not a historical comparison
# or the refs selected by push. Neither path overlap nor an empty local diff
# establishes the finding's origin. Do not turn those into blame or history.

{
    echo "=== PUBLICATION COMPLETENESS: REFUSING THIS ${VERB} ==="
    echo ""
    echo "  repository : $CC_REPO"
    echo ""
    echo "  This tree is publication-bound (.publication-boundary), and it is"
    echo "  claiming a capability it does not deliver. A reader who has only the"
    echo "  public repository would be sent to something they will never have."
    echo ""
    printf '%s\n' "$OUT" | sed 's/^/  /'
    echo ""
    echo "  Audit scope: current index and worktree, not a commit or push range."
    echo "  Attribution: unknown. This check does not establish who introduced"
    echo "  a finding, when it arrived or whether an earlier review examined it."
    echo "  A clean checkout after a commit or merge can contain a new finding."
    echo "  Verify Git history separately before reporting an origin or recording"
    echo "  one in ECS or memory. The current-tree findings above still block."
    echo ""
    echo "  THREE WAYS THROUGH, all of them a normal edit — the checker reads the"
    echo "  INDEX and the WORKTREE, so a fix counts the moment you make it:"
    echo "    1. Fix the path, or name the private record in prose so the reader"
    echo "       is told where it is instead of being sent nowhere."
    echo "    2. Ship the missing thing — the template, the paragraph, the file."
    echo "    3. Add a reviewed line to .publication-completeness. It is committed,"
    echo "       diffable and self-expiring: an entry that suppresses nothing FAILS,"
    echo "       so an exemption cannot outlive its reason."
    echo ""
    echo "  There is no in-prompt override, deliberately. What failed on"
    echo "  2026-08-30 was in-the-moment judgement about whether a claim was"
    echo "  backed; an override token would rebuild exactly that."
    echo ""
    echo "  Recheck this repository (including when seated elsewhere):"
    printf '    bash %q --root %q --explain\n' "$CHECKER" "$CC_REPO"
} >&2
exit 2

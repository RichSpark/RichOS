#!/usr/bin/env bash
#
# guard-hook-registration-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# Makes the HOOK REGISTRATION COMPLETENESS contract fire at `git commit` and at
# `git push`, at the moment the registration is written, instead of on `main`
# after the land.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-09-14 CI went red on FIVE units at once, on `main`, all of it
# self-inflicted by the same night's landings. Three hooks landed within two
# hours. Each was correctly registered in hooks/hooks.json. Each was missing
# from a DIFFERENT subset of the other inventories that other suites assert
# against — by-reference BR2, hook-staleness case 11, engine-status case 1b,
# ci-affected-units A5. THREE ENGINEERS HIT IT INDEPENDENTLY THE SAME NIGHT.
# The record is type X, §10h of
# richos-hq/docs/verification/lifecycle-failure-record-2026-09-13.md.
#
# The reason competent people kept getting it wrong is the whole design brief:
#
#   THE REGISTRATION WORKS THE MOMENT hooks.json HAS IT. The hook loads, fires,
#   does its job. NOTHING IS BROKEN FROM THE AUTHOR'S SEAT. The inventories are
#   assertions OTHER suites make about a set the author never sees, and the only
#   thing that says so is CI, after the land.
#
# The record's own conclusion names where the fix has to sit: "the check has to
# fire WHEN THE REGISTRATION IS WRITTEN, not when CI runs." That is here. Its
# sibling guard-completeness-commits.sh states the principle this is another
# instance of:
#
#   A CHECK THAT RUNS WHEN SOMEONE REMEMBERS IS A RULE ENFORCED BY ATTENTION,
#   AND ATTENTION IS THE THING THIS ENGINE KEEPS PROVING IT CANNOT BUY.
#
# THE PREDICATE IS NOT HERE. It is scripts/hook-registration-completeness.sh,
# for its sibling's reason: ONE PREDICATE, MANY CALLERS. That file explains the
# whole decision — why the inventory list is DERIVED by unanimity rather than
# typed, why README.md and install.sh are measured NOT to be inventories, which
# two inventories are conditional and what makes demanding them unconditionally
# worse than having no guard at all. This file decides only WHEN to ask.
#
# ===========================================================================
# WHY THIS ONE BLOCKS, WHEN SO MANY SHOULD NOT
# ===========================================================================
# Three blocking guards were proposed and rejected in this project in one
# afternoon, and the rejection was right each time: each rested on a predicate
# that had to read INTENT out of prose, so each had a false-positive class, so
# the fix on the day would always have been to waive — and habitual waiving is
# how a guard dies while still looking installed.
#
# This predicate is of a different kind, and that is the entire argument:
#
#   A BASENAME IS IN A FILE OR IT IS NOT.
#
# No inference about what the author meant, no prose to parse, no threshold.
# The one place judgment could have crept in — WHICH files are inventories —
# is removed by deriving them: a file that names every one of the other 68
# registered hooks is an inventory of registered hooks, and the next candidate
# below that line sits 27 hits away. Measured, not assumed; see the predicate's
# header for the table.
#
# The cost of being wrong is also asymmetric in the direction that favors
# blocking. A false refusal costs one edit that was owed anyway. A miss costs a
# red `main`, and that is not hypothetical here — it is what happened, five
# times in one night, to three people who were each being careful.
#
# ===========================================================================
# WHERE IT FIRES, AND WHY BOTH ARMS
# ===========================================================================
# COMMIT is where the registration is AUTHORED, and it is the arm the failure
# record asks for. Baseline: HEAD. The subjects are the hooks registered in the
# about-to-exist tree and not in HEAD — so the refusal arrives while the author
# is still holding the change, and it never fires again once the complete
# commit exists.
#
# PUSH is the arm that can see a MERGE. `git merge` creates a commit without
# running `git commit`, so the commit arm never sees one — and a land is a
# merge, which is EXACTLY how the five reds reached `main`. Baseline there is
# the upstream (`@{u}`), so a merge that brings a branch's incomplete
# registration into a pushable state is judged against what the remote already
# has, rather than against a HEAD that now contains the registration and would
# therefore report nothing.
#
# WHEN THE UPSTREAM CANNOT BE RESOLVED the push arm falls back to HEAD, which
# finds nothing new, and IT SAYS SO on stderr rather than passing in silence. A
# gate that could not look and is quiet is indistinguishable from a green one.
#
# ===========================================================================
# THE ESCAPE HATCH, AND WHY THIS GUARD HAS ONE WHEN ITS SIBLING DOES NOT
# ===========================================================================
# guard-completeness-commits.sh refuses to have a live override, correctly: what
# failed there was in-the-moment judgment about whether a claim was backed, and
# an in-prompt token would rebuild exactly that.
#
# Here there is a REAL case that no amount of care can edit away, and the probe
# itself has written it down. From contract-integrity-probe.sh's R2/R3 comment:
# zach-opus-hk1 landed notice-unlanded-branches.sh and "correctly did NOT add it
# here, THIS FILE BEING SOMEONE ELSE'S" — another agent held the probe open on
# another branch. "It went in with the merge." A blocking guard with no hatch
# would have forced a blind cross-edit into a file a second agent was rewriting,
# and this project's own rule for that is that a blind cross-edit is how two
# correct fixes become one broken merge.
#
# So: `hook-inventory-ack: <which inventory is owed, who holds it, when it is
# paid>` on the command line, where a reviewer sees the claim beside the act it
# excuses. A BARE MARKER EXEMPTS NOTHING — a reason under 20 characters is
# refused. Every accepted ack is appended to
# ~/.claude/state/hook-inventory-acks.log and the count is printed back, so a
# hatch that has become a habit says so out loud.
#
# ===========================================================================
# WHAT IT COSTS — MEASURED, NOT ASSUMED
# ===========================================================================
# richos @ f5babcb7, 68 registered hook scripts, 674 tracked files in engine/,
# best of 9, macOS 24.6.0:
#
#   not a commit or a push (the cheap door)                      44 ms
#   a commit in a repository with no engine (stand-down)         87 ms
#   a commit in richos, nothing newly registered                142 ms
#     of which the predicate alone is                            68 ms
#   a commit in richos, ONE incomplete registration             146 ms
#   guard-completeness-commits.sh, same tree, for comparison    672 ms
#
# THE COST IS FLAT IN THE SIZE OF THE DIFF AND NEARLY FLAT IN THE SIZE OF THE
# TREE, and that is a property of the derivation rather than luck: the
# inventory candidates come from ONE `git grep` for a single peer name (22 ms
# on this tree, 12 files), and only those files are read in full. Nothing walks
# the repository.
#
# COST DISCIPLINE, so this stays true on a tree ten times the size: past
# HOOKREG_SOFT_BUDGET_S the guard says so on stderr EVEN WHEN IT PASSES. Past
# the hooks.json timeout the host KILLS the hook, and a killed hook is a SILENT
# one — which is the defect this file exists to remove, so the advisory fires
# at a fraction of the timeout rather than at it.
#
# ===========================================================================
# WHAT THIS CANNOT CATCH
# ===========================================================================
# Everything scripts/hook-registration-completeness.sh cannot catch, unchanged
# — that list lives in ONE place, in that file's header (removals, Layer M's
# CANON, README's table, an inventory's first hook), and is not copied here.
# Add the three this chokepoint contributes:
#
#   * A commit or push from a session this engine does not govern, or from
#     outside any session at all. Same hole every hook in this family has.
#   * A merge that is never pushed. The land sequence pushes, so this is
#     covered in practice and not in principle.
#   * A push whose upstream does not resolve. Reported, not silent; see above.
#
# PRECISION: fires only on a command that actually contains `git commit` or
# `git push`, and only in a repository that carries an engine (hooks/hooks.json
# beside scripts/lib/registered-hooks.sh). Every other repository on this
# machine is untouched at the cost of a substring test and two file tests.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
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
  hook: scripts/hooks/guard-hook-registration-commits.sh
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

# --- GIT JURISDICTION ------------------------------------------------------
# Which repository is this command talking to? ONE resolver, shared by every
# guard that asks — never a local copy. A copy is how the same
# `cd <repo> && git commit` hole ended up in five files.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-hook-registration-commits.sh"
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

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes. That is why 17 of 25
# PreToolUse guards were measured passing a call in complete silence on
# 2026-09-05. This separates the two. NO VERDICT CHANGES — the exit is the one
# already taken — only the silence does. Placed above the substring pre-filter
# for its sibling's reason: the filter exits 0 on anything without
# commit/push, so a payload it cannot read would never reach this notice.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-hook-registration-commits.sh" "$INPUT" \
        "${RICHOS_ENTITY_ROOT_RESOLVED:-}" \
        "whether a newly registered hook reaches every inventory that must name it"
fi

# --- The cheap door --------------------------------------------------------
# Every Bash call in the session reaches this file. One substring test over the
# raw payload retires the overwhelming majority before a subprocess is spawned.
# Deliberately OVER-inclusive (a commit MESSAGE containing "push" gets past it
# and is classified properly below); over-inclusive costs time, never coverage.
# dispatch-words: commit push
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
case "$INPUT" in
    *commit*|*push*) ;;
    *) exit 0 ;;
esac

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-hook-registration-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

# --- Is this a commit or a push? -------------------------------------------
# Assigned via a quoted heredoc first for the bash 3.2 reason
# guard-worktree-removal.sh documents: a `)` inside a character class
# mis-scans as the close of a $( ) substitution on macOS's /bin/bash.
IFS= read -r -d '' _HR_CLASSIFIER <<'PYEOF' || true
import json, re, sys

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    print("PASS"); raise SystemExit
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    print("PASS"); raise SystemExit
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""

# QUOTED SPANS ARE STRIPPED FIRST, and this is not cosmetic: a commit whose
# MESSAGE is "stop pushing to main" is not a push, and a classifier that thought
# it was would audit against the wrong baseline.
unquoted = re.sub(r'"[^"]*"', " ", cmd)
unquoted = re.sub(r"'[^']*'", " ", unquoted)

verb = ""
if re.search(r"\bgit\b[^\n;|&]*\bcommit\b", unquoted):
    verb = "commit"
elif re.search(r"\bgit\b[^\n;|&]*\bpush\b", unquoted):
    verb = "push"
if not verb:
    print("PASS"); raise SystemExit
if "--dry-run" in unquoted:
    print("PASS"); raise SystemExit

# The ack is read off the COMMAND LINE, not out of a file: the claim is then
# visible in the transcript next to the act it excuses.
acks = []
for m in re.finditer(r"hook-inventory-ack:\s*([^\n#]+)", cmd):
    why = m.group(1).strip().rstrip("'\"")
    if why:
        acks.append(why)

print("%s\t%s" % (verb, " ;; ".join(acks)))
PYEOF

if ! CLASS="$(python3 -c "$_HR_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-hook-registration-commits.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
VERB="$(printf '%s' "$CLASS" | cut -f1)"
case "$VERB" in
  commit|push) ;;
  *) exit 0 ;;
esac
ACK="$(printf '%s' "$CLASS" | cut -f2)"

# --- WHICH REPOSITORY? -----------------------------------------------------
_HR_GJ="$(richos_git_anchor "$INPUT" "commit push" 2>/dev/null || true)"
HR_ANCHOR="$(printf '%s' "$_HR_GJ" | cut -f2)"
[ -n "$HR_ANCHOR" ] || HR_ANCHOR="$PWD"
[ -d "$HR_ANCHOR" ] || exit 0

HR_REPO="$(git -C "$HR_ANCHOR" rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$HR_REPO" ] || exit 0

# --- ADOPTION: does this repository carry an engine? ------------------------
# No new declaration file. A repository that registers no hooks has no
# inventories to be missing from, and the question is answered by the same two
# files the predicate resolves the engine with.
HR_ADOPTED=0
for _c in "$HR_REPO/richos/engine" "$HR_REPO/engine" "$HR_REPO"; do
    if [ -f "$_c/hooks/hooks.json" ] && [ -f "$_c/scripts/lib/registered-hooks.sh" ]; then
        HR_ADOPTED=1; break
    fi
done
[ "$HR_ADOPTED" = "1" ] || exit 0

# --- THE BASELINE ----------------------------------------------------------
# COMMIT is judged against HEAD: the subjects are what this commit is adding.
# PUSH is judged against the UPSTREAM, because a push is the only arm that can
# see a MERGE, and a merge's registrations are already in HEAD by then.
BASELINE="HEAD"
if [ "$VERB" = "push" ]; then
    if UP="$(git -C "$HR_REPO" rev-parse --abbrev-ref '@{u}' 2>/dev/null)" && [ -n "$UP" ]; then
        BASELINE="$UP"
    else
        {
            echo "=== HOOK REGISTRATION COMPLETENESS: COULD NOT LOOK — THIS PUSH IS ALLOWED ==="
            echo "  repository : $HR_REPO"
            echo "  No upstream resolves for the current branch, so there is no baseline to"
            echo "  compare the registration surface against and NOTHING WAS CHECKED. That is"
            echo "  not 'the registrations are complete'."
            echo "  Answer it yourself:"
            printf '    bash %q --root %q --baseline origin/main --explain\n' \
                "$ENGINE_ROOT/scripts/hook-registration-completeness.sh" "$HR_REPO"
        } >&2
        exit 0
    fi
fi

CHECKER="$ENGINE_ROOT/scripts/hook-registration-completeness.sh"
if [ ! -f "$CHECKER" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-hook-registration-commits.sh"
        echo "  scripts/hook-registration-completeness.sh is missing at: $CHECKER"
        echo "  This guard is a CHOKEPOINT, not a predicate — the entire decision"
        echo "  lives in that script, so there is exactly one implementation of"
        echo "  the contract and CI and this hook can never disagree about what"
        echo "  'complete' means. Without it there is nothing to run, and this"
        echo "  guard will not invent a weaker answer."
    } >&2
    exit 2
fi

_HR_T0="$SECONDS"
OUT="$(bash "$CHECKER" --root "$HR_REPO" --baseline "$BASELINE" 2>&1)" && HR_RC=0 || HR_RC=$?
ELAPSED=$(( SECONDS - _HR_T0 ))

: "${HOOKREG_SOFT_BUDGET_S:=5}"
if [ "$ELAPSED" -ge "$HOOKREG_SOFT_BUDGET_S" ]; then
    {
        echo "NOTE: guard-hook-registration-commits.sh took ${ELAPSED}s on $HR_REPO."
        echo "  Measured at 0.15s on a 674-file tree with 68 registered hooks."
        echo "  Past the hooks.json timeout the host KILLS this hook, and a killed"
        echo "  hook is a silent one — which is the exact failure this guard was"
        echo "  built to remove. Raise the timeout in hooks.json deliberately, or"
        echo "  find out what got slow. This note is not a failure; it is the guard"
        echo "  refusing to become slow quietly."
    } >&2
fi

case "$HR_RC" in
  0) exit 0 ;;
  2)
    # NOT APPLICABLE means the engine or the baseline moved between the
    # adoption test above and the predicate's own — a race, not a defect.
    # Anything else is genuinely broken and fails closed: a completeness
    # checker that degrades quietly is the thing it exists to find.
    if printf '%s' "$OUT" | grep 'NOT APPLICABLE' >/dev/null; then
        exit 0
    fi
    {
        echo "=== HOOK REGISTRATION COMPLETENESS: BROKEN — REFUSING (fail-closed) ==="
        echo ""
        echo "  repository : $HR_REPO"
        echo "  checker    : $CHECKER"
        echo ""
        printf '%s\n' "$OUT" | sed 's/^/  /'
        echo ""
        echo "  The check did not run to completion, so nothing is known about whether"
        echo "  the registrations in this tree reach the inventories that assert over"
        echo "  them. A green tick over a check that did not finish is worse than none."
    } >&2
    exit 2 ;;
esac

# --- INCOMPLETE ------------------------------------------------------------
ACK_LOG="${HOOK_INVENTORY_ACK_LOG:-$HOME/.claude/state/hook-inventory-acks.log}"

if [ -n "$ACK" ]; then
    ACK_LEN="$(printf '%s' "$ACK" | awk '{print length($0)}')"
    if [ "${ACK_LEN:-0}" -lt 20 ]; then
        {
            echo "=== HOOK REGISTRATION COMPLETENESS: THE ACK IS A MARKER, NOT A REASON ==="
            echo ""
            echo "  hook-inventory-ack: $ACK"
            echo ""
            echo "  ${ACK_LEN} characters. An ack has to name WHICH inventory is owed, WHO holds"
            echo "  the file, and WHEN it is paid — because the only case this hatch exists"
            echo "  for is a second agent holding that file open on another branch. Anything"
            echo "  shorter is the hatch being used to skip the work, which is the thing that"
            echo "  kills a guard."
            echo ""
            printf '%s\n' "$OUT" | sed 's/^/  /'
        } >&2
        exit 2
    fi
    PRIOR=0
    if [ -f "$ACK_LOG" ]; then
        PRIOR="$(grep -c . "$ACK_LOG" 2>/dev/null || echo 0)"
    fi
    mkdir -p "$(dirname "$ACK_LOG")" 2>/dev/null || true
    printf '%s\t%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$HR_REPO" "$VERB" \
        "$(printf '%s' "$ACK" | tr '\t' ' ')" >> "$ACK_LOG" 2>/dev/null || true
    {
        echo "=== HOOK REGISTRATION COMPLETENESS: allowed on a declared ack ==="
        echo ""
        printf '%s\n' "$OUT" | sed 's/^/  /'
        echo ""
        echo "  ack: $ACK"
        echo "  logged to $ACK_LOG"
        if [ "${PRIOR:-0}" -ge 2 ]; then
            echo ""
            echo "  THIS IS ACK NUMBER $((PRIOR + 1)). An escape hatch used $((PRIOR + 1)) times is not an"
            echo "  exception any more; it is the contract being carried. The places above are"
            echo "  still owed, and nothing else will ask for them until CI does, on main."
        fi
    } >&2
    exit 0
fi

{
    echo "=== HOOK REGISTRATION COMPLETENESS: REFUSING THIS ${VERB} ==="
    echo ""
    echo "  repository : $HR_REPO"
    echo "  baseline   : $BASELINE"
    echo ""
    echo "  A hook registration is not one edit. It is an edit plus every inventory"
    echo "  that asserts over the registered set — and NOTHING TELLS YOU WHICH, which"
    echo "  is why on 2026-09-14 three hooks landed within two hours and took five CI"
    echo "  units red on main, each missing a different subset. The list below is not"
    echo "  typed anywhere: each place is a file that already names every one of the"
    echo "  other registered hooks, derived from this tree on this run."
    echo ""
    printf '%s\n' "$OUT" | sed 's/^/  /'
    echo ""
    echo "  THE WAY THROUGH IS THE EDIT. Each fix above is one line in one file, and"
    echo "  the checker reads the WORKTREE, so the fix counts the moment you make it —"
    echo "  no staging, no re-running anything."
    echo ""
    echo "  THE ONE HATCH, for the one case an edit cannot solve: another agent is"
    echo "  holding that inventory file open on another branch, and a blind cross-edit"
    echo "  is how two correct fixes become one broken merge. This has happened here"
    echo "  (contract-integrity-probe.sh's own R2/R3 comment records it, and the entry"
    echo "  went in with the merge). Say so on the command line:"
    echo ""
    echo "    git ${VERB} ...   # hook-inventory-ack: <inventory owed> — <who holds it, when it is paid>"
    echo ""
    echo "  A bare marker exempts nothing: under 20 characters is refused. Every ack is"
    echo "  logged to ${ACK_LOG} and the count is printed back."
    echo ""
    echo "  Re-check this repository at any time:"
    printf '    bash %q --root %q --baseline %q --explain\n' "$CHECKER" "$HR_REPO" "$BASELINE"
} >&2
exit 2

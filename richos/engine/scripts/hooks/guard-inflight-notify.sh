#!/usr/bin/env bash
#
# guard-inflight-notify.sh — PreToolUse guard on the Bash tool; NO LONGER REFUSES.
#
# Since 2026-10-06 a push needs no notice and no waiver: teammates merge main
# themselves before handover. The text below is the history of the old refusal.
#
# The predicate — who is in flight, what moved under them, who was told, and
# what an ack has to contain — lives in scripts/lib/inflight.py and NOWHERE
# ELSE. Read that file first; this one is the wiring and the refusal.
#
# ===========================================================================
# THE FAILURE
# ===========================================================================
# Landing moves `main`. Every teammate still working was cut from an older base
# and is now silently behind. Nothing tells them; the lander is the only thing
# that can. Twice on 2026-08-30 the lander did not, and both are written up by
# name in rich-lander/SKILL.md §8b: a techy renderer landed under an agent
# building the splash screen from the same base (eight conflicting hunks across
# five files, one extra agent to resolve them), and twelve new design
# variations landed under an agent building a library from the seven that
# existed at its spawn (shipped 7 of 19, another agent, another round).
#
# §8b is the specification. It is also a paragraph, and a paragraph is a
# promise. This is the machine underneath it.
#
# ===========================================================================
# WHY `git push`, AND NOTHING ELSE — the chokepoint, argued from evidence
# ===========================================================================
# The land sequence is: merge (step 4), verify (5), PUSH (6), deploy (7),
# remove the worktree (8), sweep the in-flight teammates (8b).
#
#   `git merge` — REJECTED, and it is the obvious candidate, so here is why.
#     The merge is the moment the debt is CREATED, but the notice must name the
#     SHA that main moved TO, and at PreToolUse time on the merge that commit
#     does not exist yet. A guard there could only ever enforce the PREVIOUS
#     land's debt — a whole land of lag, and a refusal aimed at a lander who is
#     midway through discharging it. Worse coverage, worse experience.
#
#   `git push origin main` — CHOSEN. By this point the merge commit exists, so
#     the tip is nameable and the debt is real, current, and dischargeable
#     inside the SAME land: send the messages, then push. It happens exactly
#     once per land, it is the orchestrator's exclusive act (engineers never
#     push), and it is non-optional in the sequence, so it cannot be walked
#     around by doing the work differently.
#
#   Anything else — not matched, and stated as a gap rather than left to be
#     discovered: a land that MERGES AND NEVER PUSHES is not caught here. That
#     tail is covered by notice-inflight-acks.sh, the Stop-hook notice, which
#     reports outstanding debt at the end of every turn regardless of how the
#     turn got there.
#
# ONLY AT A LANDING: the main checkout, attached HEAD, on main/master. An
# engineer pushing a worktree branch has moved nothing anyone is reading, and
# blocking them would block the wrong person entirely. That exit is SCOPE, and
# it is silent for the same reason guard-row-currency-commits.sh's is.
#
# ===========================================================================
# JURISDICTION: THE REPOSITORY BEING PUSHED, NOT THE SEAT
# ===========================================================================
# This operation's own shape is a session seated in one repository landing work
# in another (femcboost seat, richos worktrees). guard-row-currency-commits.sh
# learned this the expensive way — an unadopted seat exited before the check
# and a whole repository's contract was read by nothing at all. So the seat
# decides nothing here: the repository the push is FOR is resolved from `-C` /
# the payload cwd, and that repository is what gets swept.
#
# ===========================================================================
# WHAT IT GUARANTEES, AND THE TWO THINGS IT DOES NOT
# ===========================================================================
# GUARANTEED: no land completes while a live teammate is behind with no
# witnessed notice naming this tip. The witness is written by a hook inside the
# lead's own SendMessage call (notice-inflight-sends.sh), so the only way to
# produce it is to actually send the message.
#
# NOT GUARANTEED, 1 — THE ACK. A missing ack does NOT block. It cannot: at push
# time the message is seconds old and the teammate has not had a tool round
# yet. Blocking a land until another party acts is how a guard wedges a
# session. The ack is surfaced by the Stop-hook notice on a measured timeout,
# and the operator escalates. Said plainly: THE SEND IS ENFORCED, THE ACK IS
# SURFACED.
#
# NOT GUARANTEED, 2 — RELEVANCE. Whether the move actually breaks a given
# teammate's assumptions is a human judgment; see inflight.py on why path
# overlap is reported and never used as a filter. This guard forces every live
# teammate to be CONSIDERED and a decision to be RECORDED. It cannot make the
# decision good.
#
# THE ESCAPE HATCH IS A COMMAND, NOT A TOKEN. Other guards take a live prompt
# line (main-checkout-run:, resume-ack:). A Bash payload is the wrong place for
# one — the marker would sit in the same command line it excuses, forgeable by
# reflex and invisible afterwards. Instead:
#
#     <engine>/scripts/inflight-notify.sh waive <worktree> --reason "<why>"
#
# which appends a dated, attributed row to inflight-waivers.jsonl naming the
# tip, the worktree and the reason. Auditable, never silent, and it survives
# the session that issued it.
#
# FAIL-CLOSED on a missing python3 or a missing predicate: a guard that cannot
# run must not report that it looked.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds it.
#
# A PRINTED REMEDY IS A COMMAND SOMEBODY PASTES. A relative path in one is a
# guess about the reader's cwd, and this engine is loaded BY REFERENCE from
# outside every repository, so there is no cwd from which `scripts/<x>.sh`
# resolves. Reproduced 2026-09-14 from the richos repository root:
#
#   $ scripts/inflight-notify.sh status
#   (eval):2: no such file or directory: scripts/inflight-notify.sh   # rc=127
#
# Every runnable path printed by this file is therefore absolute, built from the
# root this hook already resolved for itself. `(hook: scripts/hooks/<x>.sh)`
# attribution lines are deliberately left relative: they name a file, they are
# not a command, and nobody pastes them.

set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-inflight-notify.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-inflight-notify.sh
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
# byte-identical across every rooted hook.
#
# The seat resolved above answers "am I governed?". It does NOT answer "does the
# repository I was just handed belong to the one I govern?" — and this guard is
# the case where those differ routinely: a session seated in one repository
# landing work in another is the normal shape here, not an edge.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-inflight-notify.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the repository it was"
        echo "  handed belongs to the one it governs, and a guard that cannot"
        echo "  tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

_IF_LIB="$SCRIPT_DIR/../lib/inflight.sh"
if [ ! -f "$_IF_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-inflight-notify.sh"
        echo "  scripts/lib/inflight.sh is missing at: $_IF_LIB"
        echo "  This guard's entire predicate lives behind it. Without it it cannot"
        echo "  tell a notified teammate from a forgotten one, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/inflight.sh
. "$_IF_LIB"

# --- GIT JURISDICTION ------------------------------------------------------
# The question UNDERNEATH the seat: which repository is this git command talking
# to? It was answered in five hand-copied blocks and every one of them missed
# `cd <repo> && git push`. This file's own header already documents the
# jurisdiction lesson; this is the same lesson one layer down. REFUSING TO START
# is deliberate — a guard that resolved the repository by guessing would be the
# 2026-09-01 bypass with a nicer error message.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-inflight-notify.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually push, and the fallback it used to"
        echo "  carry is the exact bypass that library exists to close."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    SEAT_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # DELIBERATELY NOT AN EXIT — the row-currency lesson. The seat answers "am I
    # governed?", not "which repository is this push for?". A session seated in
    # an unadopted repo can still be landing work in an adopted one, and exiting
    # here is exactly how a repository's contract ends up read by nothing.
    SEAT_ROOT=""
else
    root_failure_banner "scripts/hooks/guard-inflight-notify.sh" >&2
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
    unevaluated_or_continue "guard-inflight-notify.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this push leaves in-flight teammates unnotified"
fi

# --- Is this a push of main, and where? ------------------------------------
# Parsed as a COMMAND LINE, not matched as a substring: `echo "git push"` is not
# a push, and a guard that thinks it is teaches people it cries wolf. Assigned
# via a quoted heredoc first for the bash 3.2 reason guard-worktree-removal.sh
# documents — a ')' inside a character class mis-scans as the close of a $( )
# substitution on macOS's /bin/bash.
# dispatch-words: git
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
read -r -d '' _IF_CLASSIFIER <<'PYEOF' || true
import json, os, re, shlex, sys

def out(*fields):
    sys.stdout.write("\t".join(str(f) for f in fields) + "\n")
    raise SystemExit

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    out("PASS")
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    out("PASS")
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""
cwd = str(d.get("cwd", "") or "")
sid = str(d.get("session_id", "") or "")
# The lead's own transcript, carried on every hook payload. It holds the ONLY
# complete name -> agent id join (Agent tool_use -> toolUseResult.agentId), so
# it is passed through to the predicate rather than left to be rediscovered.
tpath = str(d.get("transcript_path", "") or "")

if not re.search(r"\bgit\b", cmd):
    out("PASS")

for seg in re.split(r"(?:\|\||&&|[;\n|])", cmd):
    try:
        argv = shlex.split(seg, comments=False)
    except ValueError:
        continue
    while argv and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", argv[0]):
        argv.pop(0)
    if not argv or os.path.basename(argv[0]) != "git":
        continue
    repo = ""
    k = 1
    sub = ""
    while k < len(argv):
        a = argv[k]
        if a == "-C" and k + 1 < len(argv):
            repo = argv[k + 1]; k += 2; continue
        if a.startswith("--git-dir") or a.startswith("--work-tree"):
            k += 2 if "=" not in a else 1
            continue
        if a == "-c" and k + 1 < len(argv):
            k += 2; continue
        if a.startswith("-"):
            k += 1; continue
        sub = a
        k += 1
        break
    if sub != "push":
        continue
    rest = argv[k:]
    if "--dry-run" in rest or "-n" in rest or "--delete" in rest:
        continue
    positional = [a for a in rest if not a.startswith("-")]
    # positional[0] is the remote; anything after it is a refspec.
    refspecs = positional[1:] if len(positional) > 1 else []
    # `repo` is deliberately NOT emitted. WHERE the push points is resolved by
    # scripts/lib/git-jurisdiction.sh, which understands the `cd <repo> && git
    # push` form this walk cannot see. A second answer here would be the
    # divergent copy that put the same hole in five files.
    out("ACT", sid, " ".join(refspecs), tpath)

out("PASS")
PYEOF

# Payload bytes use stdin: environment strings have a much smaller per-value
# limit on Linux. A failed classifier must never become an unevaluated pass.
if ! CLASS="$(python3 -c "$_IF_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-inflight-notify.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
[ "$(printf '%s' "$CLASS" | cut -f1)" = "ACT" ] || exit 0

SESSION_ID="$(printf '%s' "$CLASS" | cut -f2)"
REFSPECS="$(printf '%s' "$CLASS" | cut -f3)"
TRANSCRIPT="$(printf '%s' "$CLASS" | cut -f4)"

# --- WHICH REPOSITORY IS THIS COMMAND TALKING TO? --------------------------
# ONE resolver, shared by every guard that asks (scripts/lib/git-jurisdiction.sh),
# never a local copy — a copy is how the same hole ended up in five files.
_IF_GJ="$(richos_git_anchor "$INPUT" "push")"
ANCHOR="$(printf '%s' "$_IF_GJ" | cut -f2)"
[ -n "$ANCHOR" ] || ANCHOR="$PWD"

REPO="$(git -C "$ANCHOR" rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$REPO" ] || exit 0

# --- ONLY AT A LANDING (scope, not a skipped check) ------------------------
BRANCH="$(git -C "$REPO" symbolic-ref --short HEAD 2>/dev/null || true)"
case "$BRANCH" in
  main|master) ;;
  *) exit 0 ;;                       # detached, or an engineer's branch
esac
# A refspec that names something other than the current branch is not this land.
if [ -n "$REFSPECS" ] && ! printf '%s' "$REFSPECS" | grep "$BRANCH" >/dev/null; then
    exit 0
fi
MAIN_CHECKOUT="$(IF_LIB_DIR="$SCRIPT_DIR/../lib" IF_REPO="$REPO" python3 -c '
import os, sys
sys.path.insert(0, os.environ["IF_LIB_DIR"])
import inflight
print(inflight.main_checkout(os.environ["IF_REPO"]))
' 2>/dev/null || true)"
[ -n "$MAIN_CHECKOUT" ] || MAIN_CHECKOUT="$REPO"
[ "$(cd "$REPO" && pwd -P)" = "$(cd "$MAIN_CHECKOUT" && pwd -P)" ] || exit 0

# The seat is REPORTED when it differs from the repository being pushed, and
# never obeyed. A guard that switched itself off on a seat mismatch would have
# let through exactly the cross-repository land this whole mechanism exists for.
if [ -n "${SEAT_ROOT}" ]; then
    richos_assert_jurisdiction "scripts/hooks/guard-inflight-notify.sh" "${SEAT_ROOT}" "$REPO" "push in" "proceeds" || true
fi

inflight_require || {
    echo "ERROR: guard-inflight-notify.sh: $INFLIGHT_BROKEN — refusing (fail-closed), because a sweep that cannot run is not a sweep." >&2
    exit 2
}

inflight_resolve_teams_dir "$SESSION_ID"
TEAMS_DIR="$INFLIGHT_TEAMS_DIR_RESOLVED"
TIMEOUT_MIN="$(inflight_timeout_min "${SEAT_ROOT:-$REPO}")"
inflight_register_repo "$TEAMS_DIR" "$REPO"

# CEO 2026-10-06: a push no longer requires a notice to, or a waiver for, any live
# teammate. Each teammate merges main itself as its last step before handover
# (the line prepare-agent-spawn.py puts in every spawn prompt). The repository is
# still registered above, so the Stop hook still watches notices that WERE sent.
exit 0

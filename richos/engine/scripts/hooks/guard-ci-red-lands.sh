#!/usr/bin/env bash
#
# guard-ci-red-lands.sh — BLOCKING PreToolUse[Bash] guard.
#
# REFUSES A LAND INTO A BRANCH WHOSE CI IS RED, UNLESS THE LAND SAYS WHY IT
# CANNOT WAIT.
#
# ===========================================================================
# THE FAILURE THIS EXISTS TO PREVENT
# ===========================================================================
# On 2026-09-10 three workflows were red on main across two repositories, one
# of them for thirteen days, and a fourth — the dependabot workflow — had been
# failing for five days without ever being named in any report.
#
# Thirteen days was possible for one reason and it is not neglect:
#
#     RED COST NOTHING. Work landed on top of it at the same rate it landed on
#     top of green. So the only thing standing between a broken workflow and a
#     fortnight was somebody's attention, and a rule enforced by attention
#     lasts exactly as long as the attention.
#
# This makes red cost something, at the one moment where the cost is
# proportionate: the land. Not the commit — engineers commit dozens of times a
# day and blocking that would be absurd. Not the dispatch — the work is not the
# problem. The LAND, which is the act of putting more on top of a foundation
# that is already failing.
#
# ===========================================================================
# THE HALF THAT MATTERS MORE: WHY THIS DOES NOT DIE THE WAY g11/g12/g13 DIED
# ===========================================================================
# This project killed three guards in a single day by making them broad enough
# that waiving became the daily habit, and a guard that is habitually waived is
# worse than no guard: it costs a keystroke and buys a false sense of a floor.
#
# So the refusal is a CONJUNCTION, and every clause narrows it:
#
#   1. The command is a LAND — `git merge` or `git push`. Not commit, not add,
#      not fetch, not status, not a `push` to a teammate branch.
#   2. It targets the WATCHED BRANCH (main by default). A push of a worktree
#      branch is silent, and that is the overwhelming majority of pushes.
#   3. The TARGET REPOSITORY has red RIGHT NOW. Not another repository's red,
#      not a repository this session merely has open. When CI is clean this
#      guard is structurally incapable of firing.
#   4. Only the RED axis. The six-axis reader (`ci-status.sh`) also reports
#      slow, skipped, never-run, stale and hollow, and today it reports 52
#      findings. Gating on all of them would refuse every land in this
#      repository until an unrelated backlog is cleared, which is precisely how
#      a guard earns a reflexive waiver. Those five axes are REPORTED, loudly
#      and per-workflow. Only red BLOCKS.
#
# And the escape hatch is narrow in a way that makes its own overuse visible:
#
#   ci-red-ack: <workflow> — <why this land cannot wait>
#
#   * It must NAME a workflow that is ACTUALLY RED. An ack naming something
#     green, or naming nothing, is refused — a bare marker exempts nothing,
#     the same discipline the contrast floor and the dialect guard use.
#   * It must carry a reason of real length. "wip" is not a reason.
#   * IT DOES NOT COVER THE OTHER RED WORKFLOWS. Acking `ui-suite-ci.yml` while
#     `packaging-ci.yml` is also red still refuses, and names the one that was
#     not acked. Acking every one of them is possible and is exactly as much
#     typing as there are broken workflows, which is the intended shape.
#   * EVERY ACCEPTED ACK IS LOGGED, and the refusal message PRINTS HOW MANY
#     TIMES THIS SAME WORKFLOW HAS ALREADY BEEN ACKED. That is the anti-habit
#     mechanism: the fifth ack for the same workflow says "this is the 5th ack
#     for ui-suite-ci.yml since 2026-09-10", which turns a habit into evidence
#     without blocking anybody's day.
#
# ===========================================================================
# WHAT IT DOES WHEN IT CANNOT TELL — AND WHY THAT IS exit 0
# ===========================================================================
# `ci-red.py` answers red / clear / UNKNOWN. On UNKNOWN this guard ANNOUNCES,
# loudly, on stderr, and ALLOWS.
#
# That is deliberate and it is the opposite of this engine's usual fail-closed
# instinct, so here is the argument. GitHub being slow, a token expiring, or an
# aeroplane is not evidence of red. Blocking on it would refuse lands for
# reasons unrelated to CI, several times a week, and the fix on the day would
# always be to waive — the g11/g12/g13 death, arrived at by a different road.
# The protection that survives is the one that fires only on a fact it actually
# established.
#
# The announcement is not decoration: it names what could not be read and the
# command that would answer it, and the unattended watch job (ci-surface-
# watch.sh) keeps a fresh reading on disk precisely so that this path is rare.
#
# ---------------------------------------------------------------------------
# THE SECOND CANNOT-TELL: A PAYLOAD THIS GUARD CANNOT READ. SAME VERDICT,
# AND UNTIL 2026-09-10 IT WAS TAKEN IN SILENCE.
# ---------------------------------------------------------------------------
# The paragraph above is about not being able to read CI. This one is about not
# being able to read THE CALL. An empty, truncated or non-JSON payload reaches
# the classifier below, which prints `skip  unparseable payload`, and the very
# next line — `[ "$KIND" = "land" ] || exit 0` — takes the identical silent exit
# that a `ls` takes. Byte for byte, a call this gate never looked at was
# indistinguishable from a call it looked at and approved. That was the defect,
# and it is the defect this whole engine is named after in
# scripts/lib/unevaluated-notice.sh: THE ABSENCE OF A FINDING MUST BE
# DISTINGUISHABLE FROM THE ABSENCE OF A CHECK.
#
# THE PREDICATE, DECIDED AND DECLARED: an unreadable payload ALLOWS, OUT LOUD.
# It is announced on stderr and on the operator channel, logged to
# .claude/state/unevaluated-payloads.log, and the call proceeds. Only the
# silence changed; no verdict did.
#
# WHY NOT FAIL CLOSED, WHEN THE SIBLING BARRIER DOES. guard-sealed-worktree.sh
# refuses an unparseable payload and probe Layer Q6 proves ten arms of it. The
# difference is not nerve, it is the recovery path:
#
#   1. THE POPULATION. That barrier judges a worker's writes inside a sealed
#      transaction. This one is PreToolUse[Bash] — it is handed EVERY shell
#      command in the session, and its predicate is a four-clause conjunction
#      whose FIRST clause is false for almost all of them. Measured over the 12
#      most recently touched transcripts on this machine (2026-09-10): 313 Bash
#      calls, 39 of which so much as mention `git merge` or `git push` — 12.5%,
#      before the branch, the redness and the ack narrow it further. RE-DERIVE
#      IT RATHER THAN TRUST IT: take the 12 most recently modified
#      ~/.claude/projects/*/*.jsonl, count every assistant `tool_use` block whose
#      name is `Bash`, and count how many of their `command` strings match
#      \bgit\s+(merge|push)\b. One machine, one day — what the argument rests on
#      is only that lands are a small minority of shell calls, which is the same
#      premise the classifier below is built on. Refusing on
#      an unreadable payload would refuse 100% of them, because an unreadable
#      payload is exactly the state in which the narrowing cannot be applied. A
#      land gate would have become a session brick.
#   2. THE HATCH IS INSIDE THE THING THAT IS BROKEN. `ci-red-ack:` is read off
#      the command line, and the command line arrives in the payload. If the
#      payload cannot be read, the ack cannot be read either — so a fail-closed
#      arm here has NO reachable escape hatch, and the only way out of a stuck
#      session would be to disable the plugin, which disables all 29 PreToolUse
#      guards at once. The cure would remove more protection than the disease.
#      guard-sealed-worktree.sh has no such problem: its seal lives in the
#      transaction store, out of band, so a refusal there is recoverable
#      without reading the payload. THAT is the line between the two answers —
#      fail closed when the recovery path does not run through the break.
#   3. EVIDENCE. This gate refuses only on a fact it established. "Could not
#      look" is not evidence of red; "could not read the call" is not even
#      evidence that this call is a land.
#
# ALSO REJECTED: scanning the raw payload bytes for a land-shaped command and
# refusing on that. It is tempting because it would keep the blast radius
# narrow, and it is wrong three times over. The classifier below is written in
# python with shlex precisely because a regex over a command line is how a
# classifier acquires holes — and a TRUNCATED payload is a partial command by
# construction, so `git push origin main-fix` cut mid-token reads as a push to
# main. It would refuse a command it cannot quote back, on evidence it cannot
# show, with an ack it also cannot read. And it would buy that risk for a case
# that has never occurred in a live session: every payload in an ordinary
# session is well-formed — the 2026-09-05 survey had to CONSTRUCT the degraded
# ones.
#
# WHAT WOULD CHANGE THE ANSWER: a measured, recurring source of malformed
# payloads in live sessions. Then "rare" stops being true, the announcement
# stops being enough, and the right fix is a bounded, well-formed read at the
# host boundary — not a guess made here. Cases R15/R16 of this guard's suite
# pin both halves of the current answer, and unevaluated-payload.test.sh case
# 4b holds every registered guard to it.
#
# ===========================================================================
# WHY NOT A CHECK INSIDE THE LAND SKILL
# ===========================================================================
# Rejected for the reason the paragraph it replaces failed: enforcement by
# attention. The land skill already says to verify things and the verification
# that gets skipped is always the one nobody is holding a stopwatch on.
#
# Exit codes:  0 allow (silently, or with an announcement)   2 refuse

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 && command -v git >/dev/null 2>&1 || exit 0

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
  hook: scripts/hooks/guard-ci-red-lands.sh
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

# --- WHICH REPOSITORY IS THE COMMAND TALKING TO? ---------------------------
# ONE resolver, shared by every guard that asks — never a local copy. A copy is
# how the same `cd <repo> && git commit` hole ended up in five files.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-ci-red-lands.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually land into."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

# THE REMEDIES THIS GATE PRINTS ARE COMMANDS SOMEBODY PASTES, so they are
# absolute. They used to read `${ENGINE_SCRIPTS}/ci-status.sh`, which resolves only
# from the richos repository root — and this gate's whole reason for existing is
# that a session seated in ONE repository lands work in ANOTHER, where there is
# no `engine/` at all. Measured 2026-09-14 from /Users/alex/ab/femcboost:
# ${ENGINE_SCRIPTS}/ci-status.sh does not exist. Derived from this file's own
# location rather than from a root, because the header above argues at length
# that this gate resolves no entity root and must not start doing git work for
# the sake of a printed line.
ENGINE_SCRIPTS="$(cd "$SCRIPT_DIR/.." && pwd)"

RED_PROBE="$SCRIPT_DIR/../lib/ci-red.py"
[ -f "$RED_PROBE" ] || exit 0

INPUT="$(cat)"

# EMPTY MEANS ASK THE LIBRARY, once the repository is known (section 2
# below). It was `:-main`, which is this gate keeping its own answer to
# "which branch does this work land into" -- and on a body of work whose
# dev branch is the one it integrates on, that answer made this gate watch
# a branch nobody was landing into and stand down on every real land.
# CI_RED_GATE_BRANCH still wins: a CALLER naming the branch is not the
# same thing as this file assuming one.
WATCHED_BRANCH="${CI_RED_GATE_BRANCH:-}"
ACK_LOG="${CI_RED_ACK_LOG:-$HOME/.claude/state/ci-red-acks.log}"
MAX_AGE="${CI_RED_GATE_MAX_AGE:-1800}"
PROBE_TIMEOUT="${CI_RED_GATE_TIMEOUT:-12}"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that any
# ordinary shell command takes: the classifier below answers `skip  unparseable
# payload` and the dispatch line cannot tell that apart from `not a land`. That
# is why 17 of 25 PreToolUse guards were measured passing a call in complete
# silence on 2026-09-05. This separates the two. NO VERDICT CHANGES — the exit
# is the one already taken, and the header above argues at length why allowing
# is the right verdict for THIS gate where guard-sealed-worktree.sh refuses.
# Only the silence changes. The measurement, the channel and the argument:
# scripts/lib/unevaluated-notice.sh.
#
# THE ROOT ARGUMENT IS DELIBERATELY LEFT EMPTY-OR-INHERITED. Every sibling
# passes the root it already resolved; this guard resolves none, because it
# governs by GIT JURISDICTION per command (section 2 below) and resolving an
# entity root here would put git calls in front of EVERY shell command in the
# session for the sake of a log line. So the announcement is the record, and the
# durable line in .claude/state/unevaluated-payloads.log is written only when a
# root is already declared in the environment. An unadopted repository still
# costs nothing and has nothing created inside it.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-ci-red-lands.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this command lands into a branch whose CI is red"
fi

# ---------------------------------------------------------------------------
# 1. IS THIS A LAND?
# ---------------------------------------------------------------------------
# Classified in python because a shell `case` over a command line with
# quoting, `&&` and here-documents is how a classifier acquires holes. It is
# deliberately CONSERVATIVE: anything it is unsure of is not a land, and a
# missed land costs nothing that the six-axis report does not already say out
# loud, while a false land is a refusal on ordinary work.
#
# The payload arrives on STDIN, so the script itself cannot: it is held in a
# variable and passed with `-c`, the same shape scripts/lib/git-jurisdiction.sh
# uses and for the same reason.
# NOTE ON BASH 3.2, which is what `#!/usr/bin/env bash` resolves to on macOS.
# The obvious `VAR="$(cat <<'PY' ... PY)"` DOES NOT PARSE there: 3.2 scans for
# the closing parenthesis of a command substitution before it processes the
# here-document, so an apostrophe or a bracket inside the Python body ends the
# scan in the wrong place and the file dies with "unexpected EOF while looking
# for matching `)'". `read -r -d ''` has no such scan. Found by running this
# guard's own suite under /bin/bash rather than the login shell.
# dispatch-words: merge push
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
IFS= read -r -d '' _CI_CLASSIFIER <<'PY' || true
import json, re, shlex, sys

watched = sys.argv[1]
try:
    payload = json.load(sys.stdin)
except Exception:
    print("skip\tunparseable payload")
    raise SystemExit(0)

if payload.get("tool_name") not in (None, "Bash"):
    print("skip\tnot a Bash call")
    raise SystemExit(0)

cmd = (payload.get("tool_input") or {}).get("command") or ""
if not cmd.strip():
    print("skip\tempty command")
    raise SystemExit(0)

try:
    toks = shlex.split(cmd, comments=False)
except Exception:
    toks = cmd.split()

# The VERB, and only in a `git` invocation. `git merge` always targets the
# CURRENT branch, so a merge is judged whenever the checkout is on the watched
# branch (resolved by the caller). A `git push` is judged only when the watched
# branch is named or when it is a bare `git push` from the watched branch.
verb = None
refs = []
i = 0
while i < len(toks):
    t = toks[i]
    if t in ("git",) or t.endswith("/git"):
        j = i + 1
        while j < len(toks) and toks[j].startswith("-"):
            j += 2 if toks[j] in ("-C", "-c", "--git-dir", "--work-tree") else 1
        if j < len(toks) and toks[j] in ("merge", "push"):
            verb = toks[j]
            refs = [x for x in toks[j + 1:] if not x.startswith("-")]
            break
    i += 1

if verb is None:
    print("skip\tno `git merge` or `git push` in the command")
    raise SystemExit(0)

# `--dry-run` / `-n` lands nothing.
if "--dry-run" in toks or (verb == "push" and "-n" in toks):
    print("skip\ta dry run lands nothing")
    raise SystemExit(0)

named = ""
if verb == "push":
    # `git push origin main`, `git push origin HEAD:main`, `git push -u origin main`
    tails = [r.split(":")[-1].replace("refs/heads/", "") for r in refs[1:]]
    if watched:
        if watched in tails:
            named = watched
        elif tails:
            print("skip\t`git push` names refs and none of them is %s" % watched)
            raise SystemExit(0)
    else:
        # P3-12: the watched branch is not known yet (the default setting asks
        # the library later, once the repository is resolved). Skipping here
        # skipped every explicit push. Pass the named refs on, comma-joined;
        # the check after resolution decides.
        named = ",".join(tails)

# The ack, read off the command line itself — visible in the transcript, so a
# reviewer sees the claim next to the act it excuses.
acks = []
for m in re.finditer(r"ci-red-ack:\s*([^\n#]+)", cmd):
    body = m.group(1).strip().rstrip("'\"")
    parts = re.split(r"\s*(?:—|--|:)\s*", body, maxsplit=1)
    wf = parts[0].strip()
    why = parts[1].strip() if len(parts) > 1 else ""
    # \x1f between the workflow and the reason, NOT a tab. The four fields of
    # the line below are tab-separated and read with `cut -f`, so a tab inside
    # the fourth field silently truncates the reason to nothing — which the
    # guard then rejected as "a reason of 0 characters" while the author had
    # written a perfectly good one. Case G of this guard's suite pins it.
    acks.append("%s\x1f%s" % (wf, why))

print("land\t%s\t%s\t%s" % (verb, named, "|".join(acks)))
PY

CLASSIFY="$(python3 -c "$_CI_CLASSIFIER" "$WATCHED_BRANCH" <<<"$INPUT" 2>/dev/null)" \
    || CLASSIFY="skip	the land classifier could not run"

KIND="$(printf '%s' "$CLASSIFY" | cut -f1)"
[ "$KIND" = "land" ] || exit 0

VERB="$(printf '%s' "$CLASSIFY" | cut -f2)"
NAMED_BRANCH="$(printf '%s' "$CLASSIFY" | cut -f3)"
ACKS="$(printf '%s' "$CLASSIFY" | cut -f4)"

# ---------------------------------------------------------------------------
# 2. WHICH REPOSITORY, AND IS IT ON THE WATCHED BRANCH?
# ---------------------------------------------------------------------------
_GJ="$(richos_git_anchor "$INPUT" "merge push" 2>/dev/null || true)"
ANCHOR="$(printf '%s' "$_GJ" | cut -f2)"
[ -n "$ANCHOR" ] || ANCHOR="$PWD"
[ -d "$ANCHOR" ] || exit 0

REPO_ROOT="$(git -C "$ANCHOR" rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$REPO_ROOT" ] || exit 0

# THE ONE ANSWER (point 14): "Every part of the system that needs to know
# whether work has landed asks the same question: is it in the branch recorded
# for this work? None of them is allowed its own answer, and none of them
# assumes main."
if [ -z "$WATCHED_BRANCH" ]; then
    _WS_LIB="$SCRIPT_DIR/../../mega-lander/workspaces.sh"
    if [ -x "$_WS_LIB" ] || [ -f "$_WS_LIB" ]; then
        WATCHED_BRANCH="$(bash "$_WS_LIB" integration-branch --repo "$REPO_ROOT" 2>/dev/null \
                          | cut -f1 || true)"
    fi
    if [ -z "$WATCHED_BRANCH" ]; then
        # ABSTAIN, and SAY SO. Nothing is recorded as the branch this work
        # integrates on, so there is no branch to watch. Exit 0 allows the
        # land, which is this gate's own "could not look" behavior -- but a
        # gate that cannot answer and is silent about it is indistinguishable
        # from a green one, and that is the shape of every defect this engine
        # is built out of. One line, on a land, which is rare.
        {
            echo "=== CI RED GATE: NO INTEGRATION BRANCH RECORDED — THIS LAND IS ALLOWED ==="
            echo "  Nothing is recorded for ${REPO_ROOT} as the branch this work integrates"
            echo "  on, so there is no branch whose CI to read. NOTHING WAS CHECKED."
            echo "  Record it:  ${ENGINE_SCRIPTS}/workspaces.sh integration --repo ${REPO_ROOT} \\"
            echo "                  --branch <main|dev/...> --why '<this body of work>'"
        } >&2
        exit 0
    fi
fi

# `symbolic-ref`, NOT `rev-parse --abbrev-ref`. On an UNBORN branch — a fresh
# checkout before its first commit — `rev-parse --abbrev-ref HEAD` exits 128
# with "ambiguous argument 'HEAD'" and the branch name is lost, so the guard
# would decide this is not a land and stand down. `symbolic-ref --short HEAD`
# answers `main` correctly there. It fails on a DETACHED head, which is the
# right failure: nothing lands into a detached head. Found by case R5 of this
# guard's suite, whose fixture repository had no commit.
CURRENT_BRANCH="$(git -C "$REPO_ROOT" symbolic-ref --short HEAD 2>/dev/null \
                  || git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null \
                  || echo "")"

# A merge lands into wherever HEAD is. A push lands into what it names, or into
# HEAD's upstream when it names nothing. Either way: if neither the named ref
# nor the current branch is the watched one, this is not the land being gated.
case ",$NAMED_BRANCH," in *",$WATCHED_BRANCH,"*) _NAMED_HIT=1 ;; *) _NAMED_HIT=0 ;; esac
if [ "$_NAMED_HIT" -ne 1 ] && [ "$CURRENT_BRANCH" != "$WATCHED_BRANCH" ]; then
    exit 0
fi

# ---------------------------------------------------------------------------
# 2b. IS THE PREVIOUS LAND FINISHED?
# ---------------------------------------------------------------------------
# A SECOND CHECK IN THIS FILE, AND THE REASON IT IS HERE RATHER THAN IN A HOOK
# OF ITS OWN.
#
# HOOKS SNAPSHOT AT SESSION START. A newly REGISTERED hook is inert until the
# next session, so shipping this as its own registration would have cost the
# founder a restart to get a check whose entire purpose is to save him from
# watching stale rows. An ALREADY-REGISTERED hook's BODY is re-read at every
# invocation, so a change here is live the moment the branch reaches the
# engine. The same route was taken on 2026-09-10 by guard-ceo-ruled-ask.sh and
# by verify-agent-prompt.sh's concealment clause.
#
# AND THIS IS THE RIGHT HOST ON THE MERITS, not merely the available one. The
# requirement (docs/plans/land-completeness-2026-09-10.md R2) is that an
# INCOMPLETE LAND BLOCKS THE NEXT ONE — so the trigger is a land, which is
# precisely the act sections 1 and 2 above have already classified: the verb,
# the repository, the branch, dry runs excluded, `git commit` deliberately not
# matched. The alternatives were considered and are worse:
#   guard-row-currency-commits.sh also fires on `git commit`, which engineers
#     do dozens of times a day and which R2 must never gate; and its header
#     declares NO LIVE OVERRIDE, which a check needing an acknowledgement would
#     contradict.
#   guard-worktree-removal.sh fires on the REMOVAL — the opposite end of the
#     sequence from the one being enforced.
#   guard-owned-state.sh is the shape R2 cites, but its trigger is a spawn.
# The analysis, the refusal text and the acknowledgement all live in
# scripts/lib/land-residue-gate.py so that responsibility stays separated and
# this file gains a dispatch rather than a second personality.
#
# ORDER: BEFORE THE CI PROBE, DELIBERATELY. This check reads git and a local
# ledger and is bounded; the CI probe makes a network call worth up to twelve
# seconds. Running the cheap deterministic one first also means a session with
# no network still gets it. The consequence is stated rather than hidden: when
# a land is refused HERE, the CI axis has not been reported for that command —
# the refusal says so and names `ci-status.sh`.
#
# IT SHIPS REPORTING-ONLY. G4 keeps installation approval with the founder, and
# there is no implicit approval from a merge or from silence.
#
# THE SWITCH IS READ FROM A COMMITTED FILE, NOT FROM THE ENVIRONMENT ALONE.
# This guard deliberately resolves no entity root — its header explains that
# doing so would put git calls in front of every shell command in the session.
# But by this line the repository is already resolved and the command is
# already known to be a land, so sourcing one config file costs nothing that
# was not already spent, and it is what makes the arming decision a diffable
# line rather than an invisible export. The governed repository's config wins;
# the engine's is the fallback, so an adopter can arm or stand this down
# without editing the engine.
#
# PRECEDENCE, AND THE BUG THAT MADE IT EXPLICIT: an explicit setting at the call
# site WINS over the file. Sourcing a config is an assignment, so a naive
# `. config` silently overwrites a variable the caller deliberately exported —
# which on 2026-09-10 made this section run while `CHECK_LAND_COMPLETENESS=0`
# was set in the environment, and report-only while `LAND_COMPLETENESS_ENFORCE=1`
# was. A stand-down that does not stand down is worse than no switch at all. So
# the two keys are snapshotted before the source and restored after it if they
# were set. Order: environment, then the governed repository, then the engine.
_LC_ENV_CHECK="${CHECK_LAND_COMPLETENESS+set:$CHECK_LAND_COMPLETENESS}"
_LC_ENV_ENFORCE="${LAND_COMPLETENESS_ENFORCE+set:$LAND_COMPLETENESS_ENFORCE}"
for _LC_CFG in "$REPO_ROOT/orchestration.config" "$SCRIPT_DIR/../../orchestration.config"; do
    if [ -f "$_LC_CFG" ]; then
        # shellcheck disable=SC1090
        . "$_LC_CFG" 2>/dev/null || true
        break
    fi
done
case "$_LC_ENV_CHECK" in   set:*) CHECK_LAND_COMPLETENESS="${_LC_ENV_CHECK#set:}" ;; esac
case "$_LC_ENV_ENFORCE" in set:*) LAND_COMPLETENESS_ENFORCE="${_LC_ENV_ENFORCE#set:}" ;; esac

if [ "${CHECK_LAND_COMPLETENESS:-1}" != "0" ]; then
    _LRG="$SCRIPT_DIR/../lib/land-residue-gate.py"
    if [ -f "$_LRG" ]; then
        _LC_CMD="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    print((json.load(sys.stdin).get("tool_input") or {}).get("command") or "")
except Exception:
    print("")' 2>/dev/null || true)"
        # A HARD TIMEOUT, because this runs inside the host hook's budget. The
        # analysis measured 2.6s against 14 worktrees on 2026-09-10; the CI
        # probe below may take 12 more, and the registration allows 25. If it
        # ever runs long, the land proceeds unchecked and SAYS SO — a land
        # delayed by a slow checker is the failure mode that gets a guard
        # switched off.
        set +e
        python3 "$_LRG" --repo "$REPO_ROOT" --branch "$WATCHED_BRANCH" \
            --command "$_LC_CMD" \
            --ack-log "${LAND_RESIDUE_ACK_LOG:-$HOME/.claude/state/land-residue-acks.log}" \
            --enforce "${LAND_COMPLETENESS_ENFORCE:-0}" &
        _LRG_PID=$!
        _LRG_WAITED=0
        while kill -0 "$_LRG_PID" 2>/dev/null; do
            [ "$_LRG_WAITED" -ge "${LAND_COMPLETENESS_TIMEOUT:-10}" ] && break
            sleep 1
            _LRG_WAITED=$((_LRG_WAITED + 1))
        done
        if kill -0 "$_LRG_PID" 2>/dev/null; then
            kill "$_LRG_PID" 2>/dev/null
            _LRG_RC=0
            {
                echo "=== LAND-COMPLETENESS CHECK: TIMED OUT — THE LAND IS ALLOWED ==="
                echo "  It did not answer within ${LAND_COMPLETENESS_TIMEOUT:-10}s, so nothing was"
                echo "  checked. That is NOT 'the last land was clean'."
                echo "  Answer it yourself:  ${ENGINE_SCRIPTS}/land-completeness.sh --repo $REPO_ROOT"
            } >&2
        else
            wait "$_LRG_PID"
            _LRG_RC=$?
        fi
        set -e
        [ "$_LRG_RC" = "2" ] && exit 2
    else
        {
            echo "=== LAND-COMPLETENESS CHECK: NOT RUNNING — THE LAND IS ALLOWED ==="
            echo "  The gate is missing at $_LRG. Whether the previous land finished was"
            echo "  NOT checked, and an unchecked land is not a clean one."
        } >&2
    fi
fi

# ---------------------------------------------------------------------------
# 3. THE SLUG — read off the config file, never guessed
# ---------------------------------------------------------------------------
ORIGIN="$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)"
[ -n "$ORIGIN" ] || exit 0
# In python, not sed: the non-greedy quantifier this needs is a GNU extension,
# and on the BSD sed macOS ships the expression fails with "repetition-operator
# operand invalid". Caught by case D of this guard's own suite on its first run,
# which is the only reason it is not still here.
SLUG="$(python3 -c '
import re, sys
m = re.search(r"[:/]([^/:]+)/([^/]+?)(?:\.g" "it)?$", sys.argv[1])
print("%s/%s" % (m.group(1), m.group(2)) if m else "")' "$ORIGIN" 2>/dev/null || true)"
case "$SLUG" in */*) : ;; *) exit 0 ;; esac

# ---------------------------------------------------------------------------
# 4. IS IT RED?
# ---------------------------------------------------------------------------
PROBE_JSON="$(python3 "$RED_PROBE" --repo "$SLUG" --branch "$WATCHED_BRANCH" \
                --max-age-seconds "$MAX_AGE" --timeout "$PROBE_TIMEOUT" --json 2>/dev/null || true)"
[ -n "$PROBE_JSON" ] || {
    {
        echo "=== CI RED GATE: COULD NOT LOOK ==="
        echo "  The red probe produced nothing for ${SLUG} (${WATCHED_BRANCH}). This land is ALLOWED,"
        echo "  because 'could not look' is not evidence of red — but nothing was checked."
        echo "  Answer it yourself:  ${ENGINE_SCRIPTS}/ci-status.sh --repo ${REPO_ROOT}"
    } >&2
    exit 0
}

STATE="$(printf '%s' "$PROBE_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("state","unknown"))' 2>/dev/null || echo unknown)"

if [ "$STATE" = "paused" ]; then
    # The land-completeness check above still runs. Only CI stands down.
    exit 0
fi

if [ "$STATE" = "clear" ]; then
    exit 0
fi

if [ "$STATE" != "red" ]; then
    REASON="$(printf '%s' "$PROBE_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("reason","(none given)"))' 2>/dev/null || echo "(none given)")"
    {
        echo "=== CI RED GATE: COULD NOT LOOK — THE LAND IS ALLOWED, NOTHING WAS CHECKED ==="
        echo "  repository: ${SLUG}   branch: ${WATCHED_BRANCH}"
        echo "  why:        ${REASON}"
        echo ""
        echo "  This is NOT 'CI is clear'. A checker that could not look has found nothing and"
        echo "  proved nothing. Blocking here would refuse lands whenever GitHub is slow, the fix"
        echo "  on the day would always be to waive, and habitual waiving is how three guards died"
        echo "  in this project in one afternoon. So it allows, and it says so where you can see it."
        echo ""
        echo "  Settle it:  ${ENGINE_SCRIPTS}/ci-status.sh --repo ${REPO_ROOT}"
    } >&2
    exit 0
fi

# ---------------------------------------------------------------------------
# 5. RED. MATCH THE ACKS, COUNT THE HABIT, DECIDE.
# ---------------------------------------------------------------------------
IFS= read -r -d '' _CI_ACK_READER <<'PY' || true
import json, os, re, sys, time

doc = json.load(sys.stdin)
red = doc.get("red", []) or []
raw = os.environ.get("ACKS", "")
slug = os.environ["SLUG"]
log_path = os.environ["ACK_LOG"]

acks = []
for chunk in raw.split("|"):
    if not chunk.strip():
        continue
    wf, _, why = chunk.partition("\x1f")
    acks.append((wf.strip(), why.strip()))


def names(entry):
    """Every string this workflow can honestly be called: the file name, the
    file name without its extension, and the workflow's display name. An ack is
    a human typing, and refusing `ui-suite-ci` because the file is
    `ui-suite-ci.yml` would be pedantry that teaches people to distrust the
    gate rather than to use it."""
    out = set()
    for v in (entry.get("workflow"), entry.get("name")):
        if v:
            out.add(v.strip().lower())
            out.add(re.sub(r"\.ya?ml$", "", v.strip().lower()))
    return out


# How often has each of these already been acked? Read BEFORE anything is
# written, so the count in a refusal is the history and not this attempt.
history = {}
try:
    with open(log_path, encoding="utf-8", errors="replace") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) >= 3 and parts[1] == slug:
                history[parts[2].lower()] = history.get(parts[2].lower(), 0) + 1
except Exception:
    pass

problems, accepted = [], []
for entry in red:
    wf = entry.get("workflow") or entry.get("name") or "?"
    match = None
    for given, why in acks:
        if given.strip().lower() in names(entry):
            match = (given, why)
            break
    if match is None:
        problems.append((entry, "not acked"))
    elif len(match[1]) < 15:
        problems.append((entry, "acked with a reason of %d characters (%r) — that is a marker, not a "
                               "reason" % (len(match[1]), match[1])))
    else:
        accepted.append((entry, match[1]))

# An ack naming a workflow that is not red at all: say so. It is either a typo,
# in which case the land is about to be let through on a mistake, or the ack was
# copied from an earlier land, which is the habit this counts.
red_names = set()
for e in red:
    red_names |= names(e)
stray = [(g, w) for g, w in acks if g.strip().lower() not in red_names]

out = {"problems": [], "accepted": [], "stray": stray, "history": history,
       "workflows_seen": doc.get("workflows_seen"), "read_at": doc.get("read_at"),
       "source": doc.get("source")}
for entry, why in problems:
    out["problems"].append({
        "workflow": entry.get("workflow") or entry.get("name"),
        "conclusion": entry.get("conclusion"),
        "days": entry.get("days"),
        "floor": entry.get("days_is_a_floor"),
        "since_run": entry.get("since_run"),
        "last_green": entry.get("last_green_run"),
        "file_backed": entry.get("file_backed", True),
        "path": entry.get("path"),
        "url": entry.get("url"),
        "why": why,
        "acked_before": history.get((entry.get("workflow") or "").lower(), 0),
    })
for entry, why in accepted:
    out["accepted"].append({"workflow": entry.get("workflow") or entry.get("name"), "why": why,
                            "acked_before": history.get((entry.get("workflow") or "").lower(), 0)})

if not out["problems"]:
    # Every red workflow was acked with a real reason. Record it durably — the
    # log is the only thing that can ever show this becoming a habit.
    try:
        os.makedirs(os.path.dirname(log_path), exist_ok=True)
        with open(log_path, "a", encoding="utf-8") as f:
            for a in out["accepted"]:
                f.write("%s\t%s\t%s\t%s\t%s\n"
                        % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), slug,
                           a["workflow"], os.environ.get("VERB", "?"), a["why"].replace("\t", " ")))
    except Exception:
        pass

print(json.dumps(out))
PY

VERDICT="$(ACKS="$ACKS" ACK_LOG="$ACK_LOG" SLUG="$SLUG" VERB="$VERB" \
           REPO_ROOT="$REPO_ROOT" BRANCH="$WATCHED_BRANCH" \
           python3 -c "$_CI_ACK_READER" <<<"$PROBE_JSON" 2>/dev/null)" || VERDICT=""

if [ -z "$VERDICT" ]; then
    {
        echo "=== CI RED GATE: the ack reader failed — the land is ALLOWED, nothing was checked ==="
        echo "  ${SLUG} (${WATCHED_BRANCH}) reads RED, but this guard could not evaluate the acks,"
        echo "  and refusing on its own malfunction would teach everyone to work around it."
    } >&2
    exit 0
fi

N_PROBLEMS="$(printf '%s' "$VERDICT" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["problems"]))')"

if [ "$N_PROBLEMS" = "0" ]; then
    printf '%s' "$VERDICT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
print("=== CI RED GATE: allowed on a declared ack ===", file=sys.stderr)
for a in d["accepted"]:
    n = a["acked_before"]
    print("  %s — %s" % (a["workflow"], a["why"]), file=sys.stderr)
    if n >= 2:
        print("      THIS IS ACK NUMBER %d FOR THIS WORKFLOW. An escape hatch used %d times is not an"
              % (n + 1, n + 1), file=sys.stderr)
        print("      exception any more; it is the workflow being carried. Fix it or delete it.",
              file=sys.stderr)
for g, w in d.get("stray", []):
    print("  NOTE: the ack named %r, which is not red. Check the name — an ack that matches nothing"
          % g, file=sys.stderr)
    print("        excuses nothing, and this one did not have to be written.", file=sys.stderr)
' >&2
    exit 0
fi

# ---------------------------------------------------------------------------
# THE REFUSAL
# ---------------------------------------------------------------------------
{
    echo "=== CI IS RED ON ${SLUG} (${WATCHED_BRANCH}) — THIS LAND IS REFUSED ==="
    echo ""
    printf '%s' "$VERDICT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
for p in d["problems"]:
    age = "%s%s day(s)" % ("at least " if p.get("floor") else "", p.get("days"))
    print("  %s   %s for %s" % (p["workflow"], p["conclusion"], age))
    print("      since run #%s; last green %s"
          % (p.get("since_run"),
             ("run #%s" % p["last_green"]) if p.get("last_green") else "NONE within the runs read"))
    if not p.get("file_backed", True):
        print("      generated by GitHub (%s) — no file in the checkout describes it" % p.get("path"))
    if p.get("url"):
        print("      %s" % p["url"])
    if p.get("acked_before"):
        print("      ALREADY ACKED %d TIME(S) in this repository." % p["acked_before"])
    if p["why"] != "not acked":
        print("      the ack was rejected: %s" % p["why"])
    print("")
print("  read %s (%s), %s workflows seen" % (d.get("read_at"), d.get("source"), d.get("workflows_seen")))
'
    echo ""
    echo "  WHY THIS BLOCKS. Red used to cost nothing here, so red persisted — on 2026-09-10 a"
    echo "  workflow had been failing for thirteen days with work landing on top of it the whole"
    echo "  time. (That is the history this gate was built from, not a claim about the list above;"
    echo "  the ages above are the ones that matter now.)"
    echo "  Landing more onto a failing foundation is the act this refuses — not committing, not"
    echo "  pushing a worktree branch, only a land into ${WATCHED_BRANCH} while ${WATCHED_BRANCH} is red."
    echo ""
    echo "  THE THREE WAYS FORWARD, in the order they should be preferred:"
    echo ""
    echo "  1. FIX IT. Usually the fastest of the three, and the only one that removes the block"
    echo "     for everybody rather than for this command."
    echo ""
    echo "  2. LAND ANYWAY, SAYING WHY. Add one comment to the command, naming EACH red workflow:"
    echo ""
    echo "       git ${VERB} ...   # ci-red-ack: <workflow> — <why this land cannot wait>"
    echo ""
    echo "     A bare marker exempts nothing: the workflow must be one that is actually red and"
    echo "     the reason must be a reason. Every accepted ack is logged to"
    echo "       ${ACK_LOG}"
    echo "     and the count is printed back at you, so a hatch that becomes a habit says so."
    echo ""
    echo "  3. IF THIS LAND IS THE FIX, ack it and say that. That is a true reason and it takes"
    echo "     one line."
    echo ""
    echo "  The full picture across every governed repository, on all six axes:"
    echo "       ${ENGINE_SCRIPTS}/ci-status.sh"
} >&2
exit 2

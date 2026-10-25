#!/usr/bin/env bash
#
# demo.sh — the 60-second proof-of-life. One command, unattended, that shows
# a CEO the engine's enforcement machinery actually working, before they trust
# it with a real repo.
#
# WHAT THIS IS: value-roadmap.md §B2 ("test-drive mode") plus the missing
# "one-command demo button" Frank's hostile-buyer review flagged as the
# single highest-leverage gap ("B2 is the content; this is the button").
# Frank's minimum proof-of-life spec (his review, "#1 build priority"):
# watch (a) the isolation guard block a bad spawn, (b) an engineer commit on
# a worktree branch, (c) a QA agent reject a planted defect, (d) the fix
# pass, (e) the lander merge to main. This script drives exactly that loop,
# unattended, against a throwaway sample repo — then tears the sample down.
#
# DESIGN DECISION — sample project is synthesized at runtime, not shipped:
# the "sample company repo" this script drives is built fresh in a temp dir
# on every run (git init, hooks copied in, a two-file toy product written out)
# rather than a persistent `demo/sample-project/` checked into the engine. This
# keeps the engine tree free of orphan demo data to maintain, guarantees the
# demo can never drift out of sync with the current hooks (it always copies
# THIS commit's hook files), and makes "re-runnable" and "engine repo untouched"
# trivially true by construction — there is nothing in the engine for the demo
# to touch in the first place.
#
# HONEST LABELING (Frank's truth-in-labeling lesson applies to the demo
# itself): every beat below is marked either
#   [REAL ENFORCEMENT]      — the actual hook binary / real git mechanics run,
#                             unmodified, and its real exit code decides
#                             pass/fail.
#   [NARRATED SIMULATION]   — there is no live LLM agent here (this script is
#                             plain bash), so the QA-verdict beat is narrated:
#                             the git/commit mechanics and the QA check script
#                             it runs are REAL and its exit code is real, but
#                             the "QA agent's judgment" is this demo's own
#                             scripted stand-in, not a spawned teammate. Said
#                             explicitly, every time, never presented as a
#                             live agent transcript.
#
# PREREQUISITES (same as the engine's own, nothing extra): bash, git, python3,
# standard coreutils. No Claude API key. No live agents. No network access.
# Deterministic and unattended — safe for a buyer to run sight-unseen.
#
# Usage:
#   scripts/demo.sh
#
# Exit codes:
#   0  every beat passed — the enforcement machinery works end-to-end
#   1  unexpected error (missing prerequisite, setup failure)
#   2  one or more beats failed — the demo doubles as an integrity check

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

command -v git >/dev/null 2>&1 || { echo "ERROR: demo.sh requires git — not found on PATH." >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "ERROR: demo.sh requires python3 — not found on PATH." >&2; exit 1; }

START_EPOCH="$(date +%s)"

C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'

BEATS_TOTAL=0
BEATS_PASSED=0
BEAT_NAMES_FAILED=()

heading() {
    printf '\n%s=== %s ===%s\n' "$C_BOLD" "$1" "$C_RESET"
}
narrate() {
    printf '%s\n' "$1"
}
label_real() {
    printf '  %s[REAL ENFORCEMENT]%s %s\n' "$C_BOLD" "$C_RESET" "$1"
}
label_sim() {
    printf '  %s[NARRATED SIMULATION]%s %s\n' "$C_YELLOW" "$C_RESET" "$1"
}
show_output() {
    # Indent a captured command's output for readability under a beat.
    printf '%s\n' "$1" | sed 's/^/    /'
}
beat_pass() {
    BEATS_TOTAL=$((BEATS_TOTAL + 1))
    BEATS_PASSED=$((BEATS_PASSED + 1))
    printf '  %s✓ Beat %s PASSED%s — %s\n' "$C_GREEN" "$1" "$C_RESET" "$2"
}
beat_fail() {
    BEATS_TOTAL=$((BEATS_TOTAL + 1))
    BEAT_NAMES_FAILED+=("Beat $1: $2")
    printf '  %s✗ Beat %s FAILED%s — %s\n' "$C_RED" "$1" "$C_RESET" "$2" >&2
}

# ---------------------------------------------------------------------------
# Sample repo setup — a throwaway "sample company" in a temp dir, wired with
# THIS commit's real hook files via the real install.sh (not a mock, not a
# copy-pasted re-implementation).
# ---------------------------------------------------------------------------
# pwd -P: canonicalize away macOS's /var -> /private/var (and /tmp) symlink so
# this variable and every hook's own `cd .. && pwd` / git-derived REPO_ROOT
# agree on the same real path from the start (same fix contract-integrity.
# test.sh's make_git_main already applies, for the identical reason).
#
# Portability note: an explicit "${TMPDIR:-/tmp}/template.XXXXXX" path (NOT
# `mktemp -t prefix`) is used deliberately — this is the ONE form that
# substitutes the trailing X's identically on both BSD/macOS mktemp and GNU/
# Linux mktemp. `-t prefix` behaves differently per platform: BSD/macOS
# treats the whole prefix as opaque and appends its OWN random suffix after
# it (harmless, but produces an ugly double-suffixed path this demo narrates
# straight to the CEO — e.g. "...demo.XXXXXX.k3ryLSxK9V"); GNU substitutes
# the X's directly. This form gives byte-for-byte identical, clean output on
# both platforms. (See docs/ci-portability-notes.md.)
# ALLOCATED, NOT NAMED — see scripts/lib/scratch.sh. The portability note above
# is now moot for this line and is kept because it documents why the OLD form
# was written the way it was: the allocator builds the name itself, identically
# on both platforms, so there are no trailing X's to substitute at all.
# shellcheck source=lib/scratch.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/scratch.sh"
SAMPLE_ROOT="$(scratch_new richos-engine-demo)"
# THE WORKSPACE REGISTRY IS PINNED INSIDE THE SAMPLE REPO, for the reason
# CLAUDE_CONFIG_DIR is redirected below: clause 7 of guard-worktree-isolation.sh
# REGISTERS every allowed file-capable spawn (docs/plans/worktree-spec-2026-09-
# 11.md, point 3), and Beat 3 runs that guard for real. Unpinned, a sixty-second
# demo would put demo0000's registrations in the operator's own registry, where
# a later session would find them. And the registration needs a SESSION whose
# end the operating system can report (point 12): this demo's own process is
# that session, so nothing it registers borrows the identity of whatever
# Claude session happens to be running on the machine.
export RICHOS_WORKSPACES_DIR="$SAMPLE_ROOT/.demo-state/workspaces"
export RICHOS_SESSION_PID="$$"
# AND THE CONFIG DIR IS PINNED WITH IT, for the same reason and one this script
# was already asserting without doing. The comment above says "CLAUDE_CONFIG_DIR
# is redirected below" — it was, but only for the two commands that name it
# inline (install.sh and Beat 7's probe). Everything else the beats run resolved
# `${CLAUDE_CONFIG_DIR:-$HOME/.claude}` and wrote the operator's own record: on a
# machine with no ledger yet, a sixty-second demo CREATED
# ~/.claude/state/worktree-ledger.jsonl, plus teammate-idle-events.jsonl and
# teammate-task-events.jsonl beside it. `scripts/lib/record-canary.sh` caught it
# ("ledger: was ABSENT and now EXISTS"), which is precisely the class that wrote
# a false termination for a running agent on 2026-09-11. A demo a buyer runs
# sight-unseen must leave nothing behind anywhere, so the redirection is an
# EXPORT and the two inline uses are now belt and braces.
export CLAUDE_CONFIG_DIR="$SAMPLE_ROOT/.claude-config"
# AND THE THREE RECORDS THAT DO NOT READ CLAUDE_CONFIG_DIR AT ALL are pinned by
# their own knobs, because that is where the leak actually was. Measured with
# HOME and CLAUDE_CONFIG_DIR pointed at two DIFFERENT empty directories: every
# file landed under HOME/.claude and nothing under CLAUDE_CONFIG_DIR.
#   scripts/lib/worktree-ledger.py   ~/.claude/state/worktree-ledger.jsonl
#                                        (RICHOS_WORKTREE_LEDGER)
#   teammate-idle-handoff.sh         ~/.claude/teams/... or the flat fallback
#                                        (TEAMMATE_IDLE_TEAMS_DIR)
#   task-completed-handoff.sh        the same shape (TASK_COMPLETED_TEAMS_DIR)
# The two handoffs fall back to a HOME-anchored flat file when they can resolve
# no team directory, so a pinned-but-empty teams directory would still leak: the
# session directory is created here so they resolve INSIDE the sample repo.
export RICHOS_WORKTREE_LEDGER="$SAMPLE_ROOT/.demo-state/worktree-ledger.jsonl"
export TEAMMATE_IDLE_TEAMS_DIR="$SAMPLE_ROOT/.demo-state/teams"
export TASK_COMPLETED_TEAMS_DIR="$SAMPLE_ROOT/.demo-state/teams"
mkdir -p "$SAMPLE_ROOT/.demo-state/teams/session-demo0000"

cleanup() {
    rm -rf "$SAMPLE_ROOT" 2>/dev/null || true
}
trap cleanup EXIT

heading "Setting up a throwaway sample company (temp dir, deleted on exit)"
narrate "Building a tiny sample repo at $SAMPLE_ROOT — this is NOT your repo and"
narrate "is deleted when this script exits. Wiring in the engine's real, unmodified"
narrate "enforcement hooks from this checkout — the same files that would protect"
narrate "your own repo."

mkdir -p "$SAMPLE_ROOT/scripts/hooks" "$SAMPLE_ROOT/scripts/lib" "$SAMPLE_ROOT/hooks" "$SAMPLE_ROOT/.claude/state" "$SAMPLE_ROOT/app"

# The sample repo IS the governed repository for every hook fired below, so say
# so rather than letting the guards infer it from the demo's own cwd. This is
# exactly what a real session's CLAUDE_PROJECT_DIR does for a real adopter.
CLAUDE_PROJECT_DIR="$SAMPLE_ROOT"
export CLAUDE_PROJECT_DIR
# RICHOS_ENTITY_ROOT outranks CLAUDE_PROJECT_DIR in the shared resolver (an
# explicit declaration is exclusive intent), so one inherited from the caller's
# session would make every hook below judge THAT entity, or refuse its override,
# instead of the sample. The demo names its own governed repository above; drop
# the caller's.
unset RICHOS_ENTITY_ROOT

# ---------------------------------------------------------------------------
# THE SAMPLE REPO'S FILE SET IS DERIVED, NEVER TYPED.
#
# The hook half used to be a hand-written list of twenty-two filenames, and it
# was the FOURTH copy of "the set of guards" in this engine — exactly the reader
# scripts/lib/registered-hooks.sh predicted would eventually drift out of sync.
# It broke in the other direction first, and worse: install.sh stopped taking
# its own inventory from a typed list and started DERIVING it from
# hooks/hooks.json via registered-hooks.sh, so it began requiring two files this
# script provisioned nowhere. install.sh refused, correctly. demo.sh — the
# script whose whole job is to show a buyer the machinery works — died during
# setup with exit 2 and printed no reason at all.
#
# So the demo now derives the same way, from the same file, through the same
# parser. Wiring a new guard needs no edit here; and the sample repo carries the
# registration surface and the parser THEMSELVES, because they are what
# install.sh reads.
# ---------------------------------------------------------------------------
_RH_LIB="$REPO_ROOT/scripts/lib/registered-hooks.sh"
[ -f "$_RH_LIB" ] || { echo "ERROR: demo.sh: $_RH_LIB is missing from this engine checkout — the sample repo's hook set is derived from it and must not be guessed. Refusing." >&2; exit 1; }
# shellcheck source=lib/registered-hooks.sh
. "$_RH_LIB"
# Fail loud, never skip — the same contract install.sh holds itself to. A demo
# that quietly wired "the guards we could work out" would be showing a buyer a
# subset of the enforcement while calling it the whole thing.
if ! _DEMO_REGISTERED_HOOKS="$(registered_hook_scripts "$REPO_ROOT/hooks/hooks.json")"; then
    echo "ERROR: demo.sh: could not derive the managed hook set from $REPO_ROOT/hooks/hooks.json (missing, unreadable, or registering nothing). Refusing rather than demonstrating a partial engine." >&2
    exit 1
fi

DEMO_FILES=()
while IFS= read -r _h; do
    [ -n "$_h" ] || continue
    DEMO_FILES+=("scripts/hooks/$_h")
done <<REGISTERED_EOF
$_DEMO_REGISTERED_HOOKS
REGISTERED_EOF

# The files the sample repo needs that NO hook table names. Each is here for a
# reason stated at its own line — there is no registration surface to derive
# them from, which is precisely why these are the ones that go missing.
DEMO_FILES+=(
    # The registration surface itself, and the one parser that reads it.
    # install.sh derives its sidecar-minting scope from exactly this pair and
    # EXITS 2 if either is absent. Their absence here is what broke this script.
    "hooks/hooks.json"
    "scripts/lib/registered-hooks.sh"
    # The hook-command parser registered-hooks.sh imports (hunt P5-45, c943a0476):
    # every python3 reader in it runs `from hook_command import ...` off its own
    # directory, with stderr discarded. Without it each reader dies at the import,
    # install.sh sees "registering nothing" and exits 2 during setup.
    "scripts/lib/hook_command.py"
    # The root-resolution contract. Every hook's bootstrap looks for it relative
    # to its own location and REFUSES TO START without it — deliberately,
    # because a guard that cannot tell which repository it governs must not
    # guess. It sources resolve-main-checkout.sh in turn.
    "scripts/lib/resolve-roots.sh"
    "scripts/lib/resolve-main-checkout.sh"
    # The jurisdiction predicate. Two Write/Edit guards — scan-secrets.sh and
    # dialect-guard.sh — REFUSE TO START without it, exactly as they refuse
    # without resolve-roots.sh above. It was missing here, and the way it was
    # missing is worth writing down: Layer K's canary asserts the secrets
    # scanner exits 2 on a planted secret, and a hook that cannot start ALSO
    # exits 2. So the layer was green in this sandbox over a scanner that never
    # ran. Layer T now carries a clean-content canary that a dead hook cannot
    # satisfy, and this file makes both of them run for real.
    "scripts/lib/seat-jurisdiction.sh"
    # The git-jurisdiction resolver — WHICH REPOSITORY a `git commit`/`git push`
    # on the Bash tool is actually talking to. All five commit/push guards REFUSE
    # TO START without it, for the same reason they refuse without
    # seat-jurisdiction.sh above. It is one file rather than five copies because
    # it WAS five copies, and four of them resolved the session's repository
    # instead of the one being committed to whenever the command said
    # `cd <repo> && git commit` — which is how an agent in a hand-rolled
    # worktree types a commit by default.
    "scripts/lib/git-jurisdiction.sh"
    # The global-state witness — carried for the READING reason, not the running
    # one, and the difference is worth stating because nothing in this sample
    # repo will refuse without it. It is what the suites that run install.sh use
    # to prove they gave back the operator's engine pointer. A sample engine
    # missing it looks fine and cannot answer "did this run leave the pointer
    # moved?" — which is the question a red-run fixture got wrong on 2026-09-01
    # while reporting success, leaving a double-clicked RichOS with no lease.
    "scripts/lib/global-state-witness.sh"
    # The American spelling table and the fixer that reads it. dialect-guard.sh
    # decides nothing without them and Layer T fails loudly when either is
    # absent — which is how the old dictionary's omission was found, by the
    # probe rather than by a reader.
    "scripts/lib/dialect/dialect-en-US.generated.dict"
    "scripts/lib/dialect/dialect-fix.py"
    # The vendored-material predicate — who wrote this text, and is it ours to
    # judge? guard-vendoring-commits.sh and dialect-guard.sh both REFUSE TO
    # START without it, exactly as they refuse without resolve-roots.sh, so the
    # whole demo died during setup naming two hooks and no cause. It landed on
    # 2026-09-04 with the vendoring guard and this list was not grown with it,
    # which is the standing failure mode this block exists to catch: a new
    # dependency has no registration surface to be derived from.
    "scripts/lib/vendored-material.sh"
    # The model-tier parser. guard-worktree-isolation.sh decides clause 6 (is
    # this explicit model: override a move DOWN the declared capability order?)
    # with it and fails OPEN without it — announced per spawn, into a channel
    # this sample repo has no reader for. Layer MT fails loudly when it is
    # absent, which is how a sample engine missing it stops passing Beat 7.
    "scripts/lib/model-tiers.sh"
    # The named-person deny-list loader. guard-named-persons-commands.sh and
    # guard-named-persons-writes.sh both REFUSE TO START without it, and both
    # landed on 2026-09-05 with the scrub work while this list stood still —
    # the third time in two days that the standing failure mode this block was
    # written for has fired, and the second time it killed the whole demo
    # during setup. The loader reads ~/.richos-privacy/named-persons, outside
    # every repository by construction, so the sample repo carrying it exposes
    # nothing: with no list present the guards start and admit everything.
    "scripts/lib/named-persons.sh"
    # The model resolver — which model does this spawn boot on? On this list for
    # the HARD reason and the strongest version of it: guard-worktree-
    # isolation.sh REFUSES TO START without it (its truthful-name and
    # capability-tier clauses have nothing to judge), so Beat 1 and Beat 3 would
    # both fail in a sample repo missing it. guard-model-ceiling.sh needs it too
    # and fails SOFT — it announces that the ceiling is off, into a channel this
    # sample repo has no reader for.
    "scripts/lib/resolve-model.sh"
    # ---- The four predicate pairs, every one of which was MISSING here. ----
    #
    # Six registered guards — the CEO-TODOs guard, the row-currency guard, the
    # three publication guards and the in-flight notice guard — REFUSE TO START
    # without these, exactly as the hooks above refuse without resolve-roots.sh.
    # Every one of them was dead in this sample repo, and the demo reported
    # 7/7 beats and "your team's enforcement machinery works" over them, because
    # nothing in this script had ever asked whether a hook could start. The
    # completeness check further down now asks; these are its first six answers.
    #
    # Each is a PAIR: the .sh is the caller and the .py is the predicate, and
    # the .sh decides nothing without it. Adding one without the other trades a
    # loud refusal for a quiet one.
    "scripts/lib/ceo-todos.sh"
    "scripts/lib/ceo-todos.py"
    "scripts/lib/row-currency.sh"
    "scripts/lib/row-currency.py"
    "scripts/lib/publication-boundary.sh"
    "scripts/lib/publication-boundary.py"
    # The declaration resolver both of the pairs above source before they can
    # answer whether this repository declares anything. Without it every lookup
    # is BROKEN, which is loud — and a demo that opens with a broken banner
    # demonstrates the wrong engine.
    "scripts/lib/declaration-path.sh"
    "scripts/lib/inflight.sh"
    "scripts/lib/inflight.py"
    # The two CEO gates' predicates, and the escape hatch one of them names.
    # Both gates fail OPEN without these, which puts them on the SOFT side of
    # what the completeness check below can see: a sample repo missing them
    # starts every hook, passes the check, and demonstrates an engine whose two
    # CEO gates decide nothing. The ceo-asks pair was already in that state
    # before ceo-ruled existed — carried now because ceo-ruled.py takes its
    # tokenizer straight out of ceo-asks.py and ceo-ruled.sh sources
    # ceo-asks.sh, so half the pair is not a smaller version of the engine, it
    # is a different one.
    "scripts/lib/ceo-asks.sh"
    "scripts/lib/ceo-asks.py"
    "scripts/lib/ceo-ruled.sh"
    "scripts/lib/ceo-ruled.py"
    "scripts/ceo-ruled-exempt.sh"
    # The teammate-identity module, on this list for a DIFFERENT reason from the
    # eight above, and the difference is the edge of what the completeness check
    # further down can see. Those eight are guards that REFUSE TO START without
    # their predicate, so the check catches their absence by running them. This
    # one fails SOFT: without it the in-flight sweep resolves no teams directory
    # and no teammate names, and says so into a structure nothing reads here. A
    # sample repo missing it would start every hook, pass the check, and
    # demonstrate an engine that cannot name a teammate. Caught by reading, not
    # by running — which is why every entry on this list states its own reason.
    "scripts/lib/teammate-identity.py"
    # The hook-registration completeness predicate — a REFUSE-TO-START
    # dependency of the strongest kind. guard-hook-registration-commits.sh is
    # a CHOKEPOINT that holds no decision of its own: the entire contract
    # lives in this one script, so the guard exits 2 rather than invent a
    # weaker answer, and a sample repo missing it cannot start the guard at
    # all. It landed registered in c730240e (2026-09-14) while this list stood
    # still, and the completeness check below caught it — which is the whole
    # reason that check exists.
    #
    # WHAT IS NEW ABOUT THIS ONE, because it is not the usual shape: the
    # guard's OWN file is derived. It lives in scripts/hooks/ and hooks.json
    # names it, so the loop above copied it faithfully. The thing no
    # registration surface names is the helper ONE DIRECTORY UP. A hook's
    # dependency is not a hook, so no inventory of hooks can ever enumerate
    # it — including the very predicate this file is, whose own design note
    # says the inventories are derived by unanimity over registered hooks.
    # The guard that landed to end this class of omission was itself the
    # omission, and it could not have caught itself.
    "scripts/hook-registration-completeness.sh"
    # Not hooks and registered nowhere: the installer the setup beat runs, and
    # the integrity probe Beat 7 runs.
    "scripts/hooks/install.sh"
    "scripts/hooks/contract-integrity-probe.sh"
    # THE WORKSPACE SPEC (docs/plans/worktree-spec-2026-09-11.md), both halves:
    # the registry library every workspace hook hands its facts and verdicts
    # to, and the one command that lands or discards. Clause 7 of
    # guard-worktree-isolation.sh REFUSES a file-capable spawn it cannot
    # register, so the demo's own spawn beat would be blocked by the engine it
    # is demonstrating; install.sh hashes both, and probe Layers Q and S
    # exercise them, so Beat 7 fails without them for a reason that is not
    # about the demo. (Until 2026-09-11 this slot held the reaper, the
    # transaction store, the nightly reconciler and the removal helper, all
    # removed with every deleter the spec does not have.)
    "mega-lander/workspaces.py"
    "mega-lander/workspaces.sh"
    # The interactive-prompt shape table. guard-interactive-prompt.sh refuses
    # to start without it, so a sample repo missing it would ship a buyer an
    # engine whose newest blocking guard is dead on arrival — while the probe's
    # Layer IP reported it as wired.
    "scripts/lib/interactive-prompt.py"
    # THE STOP-HOOK ANALYZERS, on this list for the teammate-identity.py reason
    # rather than the refuse-to-start one, and it is the harder half to notice.
    # Five registered Stop hooks decide NOTHING themselves: the .sh resolves the
    # two roots, reads config, and hands the entire verdict to a sibling .py.
    # Without it the wrapper does not fail — it STARTS PERFECTLY, announces "NOT
    # RUNNING: the analyzer is missing" into a channel this sample repo has no
    # reader for, and exits 0 on every turn. So the completeness check below,
    # which asks whether every hook can START, would pass a sample repo whose
    # blocking turn gate refuses nothing at all — and it would show a buyer an
    # engine that reports "on" while protecting nothing, which is the one
    # sentence this whole demo is asking to be trusted about.
    "scripts/hooks/guard-idle-land.py"
    "scripts/hooks/guard-unresolved-claims.py"
    "scripts/hooks/guard-agent-state-claims.py"
    "scripts/hooks/guard-unasked-deferral.py"
    "scripts/hooks/turn-manifest.py"
    "ass-kicker/ass-kicker.py"
    "ass-kicker/brief-provenance.py"
    "ass-kicker/brief-scope.py"
    # The waiver-repetition analyzer, for the same reason as the five above:
    # notice-waiver-repetition.sh hands its entire verdict to this file, and
    # without it the sample repo shows a buyer a Stop notice that starts,
    # reports "WATCH IS OFF" into a channel the demo has no reader for, and
    # exits 0 on every turn.
    "scripts/hooks/notice-waiver-repetition.py"
    # The escalation predicate, both halves, for the waiver analyzer's reason
    # exactly: notice-escalations.sh (Stop) and session-start-escalations.sh
    # (SessionStart) decide nothing themselves and hand the whole verdict to
    # scripts/lib/escalations.py. Without it a sample repo starts both hooks
    # perfectly, announces "ESCALATION WATCH IS OFF" where the demo has no
    # reader, and exits 0 — showing a buyer a channel that reports "on" over a
    # ledger nothing is reading, which is the exact defect the channel exists
    # to remove.
    "scripts/lib/escalations.sh"
    "scripts/lib/escalations.py"
    # The mechanical-findings sweep, both halves, for the reason the waiver
    # analyzer above is here: notice-mechanical-findings.sh starts without
    # them and says "MECHANICAL SWEEP IS OFF" where the demo has no reader.
    "scripts/lib/mechanical-findings.sh"
    "scripts/lib/mechanical-findings.py"
    # The agent-liveness resolver, both halves and its operator CLI. It fails
    # SOFT, which puts it in the teammate-identity.py category rather than the
    # refuse-to-start one: guard-agent-state-claims.sh asks it whether an agent
    # a turn called finished is alive, and probe Layer AL verifies and
    # exercises it, so Beat 7 fails without it for a reason that is not about
    # the demo.
    "scripts/lib/agent-liveness.py"
    "scripts/lib/agent-liveness.sh"
    "scripts/agent-liveness.sh"
    # THE OWNERSHIP LEDGER and the cross-repository worktree helper (2026-09-02).
    # The ledger fails SOFT: the three handoff hooks record their advisory
    # signals in it and degrade without it. The helper is how a cross-repository
    # workspace is registered before it is created (points 1, 3), and the
    # route the spawn guard's refusal names.
    "scripts/lib/worktree-ledger.py"
    "mega-lander/create-teammate-worktree.sh"
    # Cosmetic but buyer-facing: without it Beat 7's probe banner opens with
    # "richos-engine (VERSION file absent)", which reads to someone evaluating
    # the engine like a broken install rather than a sample repo the demo built
    # thirty seconds ago. The probe treats a missing VERSION as a banner gap and
    # never fails on it, so this is presentation only -- and it is presentation
    # in the one place the demo is asking to be trusted.
    "VERSION"
)

# ---------------------------------------------------------------------------
# AND EVERY FILE THE REGISTERED HOOKS THEMSELVES REACH FOR — DERIVED.
#
# The list above is the files the DEMO needs: the installer it runs, the probe
# it runs, the VERSION its banner prints, the CLIs its refusals name. Those are
# this script's own, and this script notices immediately when one is missing.
#
# The files the HOOKS need are a different question and it was being answered by
# the same hand-maintained list, which is how 2026-09-14 happened:
# guard-hook-registration-commits.sh landed correctly registered, every
# hooks.json-derived inventory carried it, and the sample repo had never heard
# of scripts/hook-registration-completeness.sh -- the helper it sources, one
# directory up. A HOOK'S DEPENDENCY IS NOT A HOOK, so no inventory of hooks can
# enumerate it, and the guard that landed to end that class of omission WAS the
# omission.
#
# It was not one file's problem. Measured on this tree the same day, the list
# above was missing TWENTY-ONE more, and the two worst are silent rather than
# loud: scripts/lib/stop-hook-notice.sh (18 Stop hooks do `[ -f "$_SHN_LIB" ] ||
# exit 0`) and scripts/lib/unevaluated-notice.sh (36 hooks). With those absent
# this sample repo started every hook, passed the completeness check below --
# which asks whether a hook can START -- and showed a buyer an engine whose
# escalation watch exits 0 in silence on every turn. That is the exact sentence
# this demo exists to be trusted about.
#
# A UNION, NOT A REPLACEMENT, and scripts/lib/hook-dependencies.sh explains why
# at length: the hand list above stays, entry by entry with its reason, because
# a derivation cannot produce the WHY; and a deriver that goes blind refuses
# with rc 4 rather than quietly handing back a shorter list.
# ---------------------------------------------------------------------------
_HD_LIB="$REPO_ROOT/scripts/lib/hook-dependencies.sh"
[ -f "$_HD_LIB" ] || { echo "ERROR: demo.sh: $_HD_LIB is missing from this engine checkout -- the files the hooks themselves need are derived through it and must not be guessed. Refusing." >&2; exit 1; }
# shellcheck source=lib/hook-dependencies.sh
. "$_HD_LIB"
if ! _DEMO_HOOK_DEPS="$(richos_hook_dependency_closure "$REPO_ROOT")"; then
    echo "ERROR: demo.sh: could not derive what the registered hooks depend on (rc $?). Refusing rather than building a sample repo whose hooks start and do nothing." >&2
    exit 1
fi
while IFS= read -r _dep; do
    [ -n "$_dep" ] || continue
    case " ${DEMO_FILES[*]} " in
        *" $_dep "*) continue ;;
    esac
    DEMO_FILES+=("$_dep")
done <<HOOK_DEPS_EOF
$_DEMO_HOOK_DEPS
HOOK_DEPS_EOF

DEMO_MISSING=""
for rel in "${DEMO_FILES[@]}"; do
    if [ -f "$REPO_ROOT/$rel" ]; then
        mkdir -p "$SAMPLE_ROOT/$(dirname "$rel")"
        cp "$REPO_ROOT/$rel" "$SAMPLE_ROOT/$rel"
    else
        DEMO_MISSING="$DEMO_MISSING $rel"
    fi
done
if [ -n "$DEMO_MISSING" ]; then
    echo "ERROR: demo.sh: this engine checkout is missing file(s) the sample repo needs:$DEMO_MISSING" >&2
    echo "       Each is either registered in hooks/hooks.json or named explicitly in demo.sh. Refusing to run a partial demo." >&2
    exit 1
fi
chmod +x "$SAMPLE_ROOT/scripts/hooks/"*.sh "$SAMPLE_ROOT/scripts/"*.sh

cat >"$SAMPLE_ROOT/orchestration.config" <<'CFG'
# Sample orchestration.config for the demo — mirrors the shape of the real
# top-level file you fill in for your own repo.
PROTECTED_PATHS="app"
READONLY_ALLOWLIST="Explore Plan claude-code-guide statusline-setup"
CREATOR_TEAMMATE="dean"
ARTIFACT_MERGE_DIRS="test-results output"
ARTIFACT_REPLACE_DIRS="playwright-report"
ENABLE_QA_INSTALL_FRESH_GATE=0
SECRET_SCAN_MIN_LENGTH=12
SECRET_SCAN_MIN_ENTROPY="3.0"
SECRET_SCAN_ALLOWLIST=""
# Declared so Beat 7's Layer T exercises the dialect guard for real. Blank, it
# would take the layer's "valid but nothing enforced" WARN branch — a demo that
# shows a buyer a guard standing down is showing them a weaker engine than the
# one they would get, which is the same defect the derived hook table below
# exists to prevent.
DIALECT_TARGET="en-US"
DIALECT_SCAN_ALLOWLIST=""
DIALECT_EXEMPT_PATHS=""
# The model capability order as DATA, beside the alias set it must equal.
# Beat 7's Layer MT refuses a blank declaration, a set that drifts from
# ALLOWED_MODELS, and a doctrine file that quotes a different order.
ALLOWED_MODELS="fable opus sonnet haiku"
MODEL_TIERS="fable > opus > sonnet > haiku"
# The COST ceiling, which is a DIFFERENT order from the capability one above:
# the top tier is the most capable alias and also roughly twice the price. Beat
# 7's Layer MC warns on a blank declaration and runs its three-sided canary on a
# declared one — a sample repo with no ceiling would demonstrate an engine whose
# newest blocking guard announces that it is enforcing nothing.
MODEL_CEILING="opus"
CFG

# ---------------------------------------------------------------------------
# THE SAMPLE REPO'S HOOK TABLE IS DERIVED FROM hooks/hooks.json, NEVER TYPED.
#
# This block used to be a hand-transcribed second copy of the engine's own hook
# registrations — every event, every matcher, every timeout, retyped. It was the
# copy most likely to rot silently: a wrong ORDER under PreToolUse[Agent], or a
# guard omitted, still produces a demo that runs and reports 7/7, while showing
# a buyer a weaker engine than the one they would get.
#
# The seated form is a pure, total rewrite of the plugin form — the engine's own
# committed .claude/settings.local.json is byte-for-byte what this transform
# produces from hooks/hooks.json — so there is nothing here to decide and
# therefore nothing to get wrong. Registration order, which probe Layers C and H
# both depend on, is preserved by construction rather than by proofreading.
# ---------------------------------------------------------------------------
python3 - "$SAMPLE_ROOT/.claude/settings.local.json" "$SAMPLE_ROOT/hooks/hooks.json" <<'PY'
import json, sys

out_path, plugin_hooks_path = sys.argv[1], sys.argv[2]

with open(plugin_hooks_path, "r", encoding="utf-8") as f:
    plugin = json.load(f)

def seat(obj):
    """Rewrite the plugin-route command form into the seated form.

    `bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/x.sh` (the engine is loaded from
    wherever it lives) becomes `$CLAUDE_PROJECT_DIR/scripts/hooks/x.sh` (the
    engine's hooks sit inside the governed repo), which is what an adopter with
    a seated install commits. Inline hooks that mention neither placeholder --
    the knowledge-verification echo -- pass through untouched, correctly."""
    if isinstance(obj, dict):
        return {k: seat(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [seat(v) for v in obj]
    if isinstance(obj, str):
        return obj.replace("bash ${CLAUDE_PLUGIN_ROOT}/", "$CLAUDE_PROJECT_DIR/") \
                  .replace("${CLAUDE_PLUGIN_ROOT}", "$CLAUDE_PROJECT_DIR")
    return obj

hooks = plugin.get("hooks")
if not isinstance(hooks, dict) or not hooks:
    sys.stderr.write("ERROR: demo.sh: %s registers no hooks -- refusing to wire a "
                     "sample repo that enforces nothing.\n" % plugin_hooks_path)
    sys.exit(1)

data = {
    # The two critical non-hook keys. install.sh REFUSES to proceed without
    # either (and probe Layers I/J re-check them independently), so the sample
    # repo carries exactly what a real adopter's file must carry.
    "env": {"CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"},
    "worktree": {"baseRef": "head"},
    "hooks": seat(hooks),
}
with open(out_path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
PY

cat >"$SAMPLE_ROOT/.gitignore" <<'GI'
/.claude/settings.json
/.claude/worktrees/
scripts/hooks/*.sha256
scripts/*.sha256
ass-kicker/*.sha256
scripts/lib/*.sha256
GI

# The sample "product": a two-file toy — one source file, one QA check — just
# enough to carry a real, watchable defect -> reject -> fix -> pass story.
cat >"$SAMPLE_ROOT/app/greeting.py" <<'PY'
def greeting(name):
    return f"Hello, {name}!"
PY

cat >"$SAMPLE_ROOT/app/qa_check.py" <<'PY'
"""Sample QA check — the deterministic, scripted stand-in this demo narrates
as "the QA agent's verdict" (see demo.sh's NARRATED SIMULATION labeling).
Verifies greeting() produces the exact spec'd output."""
import sys
import importlib.util

app_dir = sys.argv[1] if len(sys.argv) > 1 else "."
spec = importlib.util.spec_from_file_location("greeting", f"{app_dir}/greeting.py")
greeting_mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(greeting_mod)

expected = "Hello, World!"
actual = greeting_mod.greeting("World")
assert actual == expected, f"QA FAILED: expected {expected!r}, got {actual!r}"
print(f"QA PASSED: greeting('World') == {actual!r}")
PY

git -C "$SAMPLE_ROOT" init -q -b main

# COMMIT IDENTITY — inherit the operator's, do not invent one.
#
# This repo-local override used to be `demo@example.com` unconditionally, and
# that made the demo UNRUNNABLE on any machine with a commit-identity policy:
# the operator who wrote this had a machine-wide pre-commit guard requiring a
# specific author/committer email, so the very first commit was refused, all
# seven beats died before running, and demo.test.sh was red on the author's own
# machine. A shipped demo that cannot run where it was written is its own kind
# of false signal — and "just use --no-verify" would be worse, because the demo
# would then be teaching adopters to walk around a guard on their first contact
# with the engine.
#
# The sample repository is a throwaway in a temp directory and is never pushed,
# so its email is cosmetic. The NAME is not — "Sample Company" is what the
# walkthrough's commit log is supposed to read — so the name is still set and
# only the email is left alone. The fallback exists for the one machine where
# leaving it alone does not work: a fresh box with no git identity configured
# at all, where `git commit` would otherwise stop and ask. Such a machine has
# no identity policy to violate either.
git -C "$SAMPLE_ROOT" config user.name "Sample Company"
if [ -z "$(git -C "$SAMPLE_ROOT" config user.email 2>/dev/null)" ]; then
    git -C "$SAMPLE_ROOT" config user.email "demo@example.com"
fi
git -C "$SAMPLE_ROOT" add -A
# Force-add the committed-by-design canonical settings file: a common GLOBAL
# gitignore convention (~/.config/git/ignore '**/.claude/settings.local.json')
# makes `git add -A` SILENTLY skip it, which would strand the next clone with no
# teammates — and now trips the probe's Layer N. `git add -f` is the remedy the
# probe points adopters at; the demo models it so beat 7's probe stays green on
# any machine (with or without that global rule).
git -C "$SAMPLE_ROOT" add -f .claude/settings.local.json
git -C "$SAMPLE_ROOT" commit -q -m "Initial sample product"
# POINT 14, AND BEAT 3 DOES NOT RUN WITHOUT IT. "The branch a body of work
# integrates on is RECORDED when that work starts, before its first agent is
# spawned. Nothing infers it and nothing guesses it." Since eedfbc7d clause 7
# refuses to register a spawn into a repository with no such record — so the
# COMPLIANT spawn Beat 3 exists to wave through was being refused, the demo
# exited 2, and a buyer running it sight-unseen saw "6/7 beats passed — the
# enforcement machinery has a problem". This is the lead's own one command,
# run where he runs it: before the first spawn.
python3 "$SAMPLE_ROOT/mega-lander/workspaces.py" --entity "$SAMPLE_ROOT" \
    --session demo0000-0000-4000-8000-000000000000 integration \
    --repo "$SAMPLE_ROOT" --branch main --why "the demo's body of work" >/dev/null

# The label used to read "generating .claude/settings.json + hook integrity
# sidecars". install.sh stopped generating settings.json when the double-fire
# bug was fixed — it now REMOVES a stale one — so the very next line of output a
# buyer read was "✓ no settings.json to migrate", contradicting the label above
# it. Small, and precisely the kind of small that makes someone evaluating the
# tool wonder what else the narration is asserting from memory.
label_real "install.sh — minting hook integrity sidecars + verifying the single canonical settings source"
# CLAUDE_CONFIG_DIR is redirected into the sample repo, and this is not a
# detail. install.sh mints the entity-facing engine pointer into the operator's
# config dir; without this line the demo would repoint a REAL operator's pointer
# at a temp directory it deletes on exit, leaving a dangling symlink behind.
# Measured — BR6b caught exactly that, on this machine, from this script. A demo
# a buyer runs sight-unseen must not touch their machine at all.
mkdir -p "$SAMPLE_ROOT/.claude-config"
# rc is captured, NOT left to `set -e`. When install.sh refused this sample repo
# (it required hooks/hooks.json + registered-hooks.sh, which the demo provisioned
# nowhere), `set -e` killed the script on the assignment line — BEFORE
# show_output could print the error install.sh had gone to the trouble of
# writing. What a buyer actually saw was the setup banner, then silence, then
# exit 2: the single least useful failure this engine is capable of producing.
# A setup failure now prints install.sh's own words and says whose fault it is.
set +e
INSTALL_OUT="$(CLAUDE_CONFIG_DIR="$SAMPLE_ROOT/.claude-config" "$SAMPLE_ROOT/scripts/hooks/install.sh" 2>&1)"
INSTALL_RC=$?
set -e
show_output "$INSTALL_OUT"
if [ "$INSTALL_RC" -ne 0 ]; then
    printf '\n%s✗ SETUP FAILED%s — install.sh exited %s wiring the sample repo. None of the seven beats ran.\n' \
        "$C_RED$C_BOLD" "$C_RESET" "$INSTALL_RC" >&2
    narrate ""
    narrate "This is a defect in the engine checkout you are running, not in your machine"
    narrate "and not in your repo — nothing outside the temp directory was touched. The"
    narrate "installer's own message above says exactly what it wanted and did not get."
    narrate "Report it with that message; scripts/demo.test.sh reproduces it."
    # Exit 1 (setup failure), not 2. Exit 2 means "one or more BEATS failed",
    # i.e. the enforcement machinery is broken — a far more alarming claim than
    # the truth, and the one this script used to make.
    exit 1
fi

# ---------------------------------------------------------------------------
# CAN THE SAMPLE REPO ACTUALLY ASSEMBLE THIS ENGINE?
#
# The DEMO_MISSING loop above only proves every file NAMED in the list exists in
# this checkout. It says nothing about whether the list names everything the
# engine needs — and on 2026-08-31 it did not: five guards landed, neither this
# list nor the meta-suite's grew, and the guards that could no longer start
# refused by exiting 2, which is precisely the exit code Layers K and D read as
# "ran and caught something". Beat 7 stayed green over dead guards.
#
# A demo is the worst possible place for that. It is the one run where the
# engine is being ASKED TO BE TRUSTED by someone who cannot check it. So the
# list is not trusted to be complete here either; it is asked, by starting every
# registered hook in the sample repo and refusing if any announces a missing
# file. Shared with the meta-suite so there is one implementation of the
# question, not the second copy this file's own comments keep warning about.
_SC_LIB="$REPO_ROOT/scripts/lib/sandbox-completeness.sh"
[ -f "$_SC_LIB" ] || { echo "ERROR: demo.sh: $_SC_LIB is missing from this engine checkout — the sample repo cannot be checked against the engine it claims to be, and a demo that skips that check is exactly the run that must not skip it. Refusing." >&2; exit 1; }
# shellcheck source=lib/sandbox-completeness.sh
. "$_SC_LIB"
DEMO_HOOK_NAMES=()
while IFS= read -r _h; do
    [ -n "$_h" ] || continue
    DEMO_HOOK_NAMES+=("$_h")
done <<REGISTERED_EOF
$_DEMO_REGISTERED_HOOKS
REGISTERED_EOF
if ! DEMO_CANNOT_START="$(richos_sandbox_start_failures "$SAMPLE_ROOT" "${DEMO_HOOK_NAMES[@]}")"; then
    echo "ERROR: demo.sh: the sample repo cannot assemble this engine — these registered hooks refused to start in it:" >&2
    printf '%s\n' "$DEMO_CANNOT_START" | sed 's/^/       /' >&2
    echo "       Add the file each one names to DEMO_FILES above. Refusing to demonstrate an engine whose guards cannot run." >&2
    exit 1
fi

narrate "Sample company ready."

# ---------------------------------------------------------------------------
# Beat 1 — a file-writing agent spawned WITHOUT isolation is BLOCKED.
# ---------------------------------------------------------------------------
heading "Beat 1 — spawning a file-writing teammate without isolation"
narrate 'Someone (or an over-eager orchestrator) tries to spawn an "engineer" to'
narrate 'fix a bug directly, forgetting isolation: "worktree". Watch what happens:'
label_real "guard-worktree-isolation.sh (the same hook that runs on every real Agent spawn)"

BAD_SPAWN_PAYLOAD='{"tool_name":"Agent","tool_input":{"subagent_type":"engineer","name":"engineer-sonnet-1","prompt":"Fix the greeting bug in app/greeting.py."},"session_id":"demo0000-0000-4000-8000-000000000000"}'
set +e
BEAT1_OUT="$(printf '%s' "$BAD_SPAWN_PAYLOAD" | "$SAMPLE_ROOT/scripts/hooks/guard-worktree-isolation.sh" 2>&1)"
BEAT1_RC=$?
set -e
show_output "$BEAT1_OUT"
if [ "$BEAT1_RC" -eq 2 ]; then
    beat_pass "1" "the guard refused the unsafe spawn — this is what protects your main branch from a stray agent editing it directly"
else
    beat_fail "1" "expected the guard to block (exit 2), got exit $BEAT1_RC"
fi

# ---------------------------------------------------------------------------
# Beat 2 — a write to a protected path from the main checkout is BLOCKED.
# ---------------------------------------------------------------------------
heading "Beat 2 — writing straight into a protected source tree"
narrate 'Now someone tries to edit app/greeting.py directly in the shared checkout'
narrate '(not inside an isolated worktree) — exactly the mistake that corrupts'
narrate 'other agents'"'"' work. Watch what happens:'
label_real "guard-main-checkout-writes.sh (the same hook that runs on every Write/Edit)"

BAD_WRITE_PAYLOAD="$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s/app/greeting.py"}}' "$SAMPLE_ROOT")"
set +e
BEAT2_OUT="$(printf '%s' "$BAD_WRITE_PAYLOAD" | "$SAMPLE_ROOT/scripts/hooks/guard-main-checkout-writes.sh" 2>&1)"
BEAT2_RC=$?
set -e
show_output "$BEAT2_OUT"

# The Write/Edit guard never sees a RAW Bash command, so a compound
# `cd <main> && mkdir app/...` slips past it and would otherwise prompt the
# human operator interactively. The PreToolUse[Bash] guard closes that gap —
# demonstrate it on the same protected tree (folded into this beat, no separate
# beat count, mirroring how scan-secrets rides along under the Write matcher).
narrate ''
narrate 'And the raw-Bash variant of the same mistake — `cd <main> && mkdir app/new`'
narrate '— which the Write/Edit guard never sees, so it used to prompt YOU. The'
narrate 'Bash guard auto-denies it too:'
label_real "guard-bash-main-writes.sh (the same hook that runs on every Bash command)"
BAD_BASH_PAYLOAD="$(printf '{"tool_name":"Bash","tool_input":{"command":"cd %s && mkdir app/new_dir"}}' "$SAMPLE_ROOT")"
set +e
BEAT2B_OUT="$(printf '%s' "$BAD_BASH_PAYLOAD" | "$SAMPLE_ROOT/scripts/hooks/guard-bash-main-writes.sh" 2>&1)"
BEAT2B_RC=$?
set -e
show_output "$BEAT2B_OUT"

if [ "$BEAT2_RC" -eq 2 ] && [ "$BEAT2B_RC" -eq 2 ]; then
    beat_pass "2" "both write-paths refused — the Write/Edit guard AND the raw-Bash guard block protected-source edits; source only changes via an isolated worktree + merge"
elif [ "$BEAT2_RC" -ne 2 ]; then
    beat_fail "2" "expected the write-guard to block (exit 2), got exit $BEAT2_RC"
else
    beat_fail "2" "expected the Bash-guard to block (exit 2), got exit $BEAT2B_RC"
fi

# ---------------------------------------------------------------------------
# Beat 3 — a COMPLIANT spawn (isolation: "worktree", well-formed name) passes
# the SAME chain that just blocked the bad one.
# ---------------------------------------------------------------------------
heading "Beat 3 — the same guard lets a correctly-isolated spawn through"
narrate 'This time the spawn does it right: isolation: "worktree", a proper'
narrate 'truthful "<role>-<model>-<identifier>" name, and the ack contract that'
narrate 'tells this teammate how to acknowledge a land that moves main under it.'
narrate 'It should sail through the full PreToolUse[Agent]'
narrate 'chain untouched:'
label_real "guard-worktree-isolation.sh -> guard-definition-drift.sh -> verify-agent-prompt.sh (the real chain, in the real order)"

GOOD_SPAWN_PAYLOAD='{"tool_name":"Agent","tool_input":{"subagent_type":"engineer","name":"engineer-sonnet-1","isolation":"worktree","prompt":"Fix the greeting bug in app/greeting.py. If I message you that main moved under you, acknowledge it durably with scripts/inflight-ack.sh --sha <sha> --impact <kind> --detail \"...\" --paths \"...\" — I cannot rely on a reply reaching me."},"session_id":"demo0000-0000-4000-8000-000000000000","tool_use_id":"toolu_demo_beat3"}'
BEAT3_OK=1
for hook in guard-worktree-isolation.sh guard-definition-drift.sh verify-agent-prompt.sh; do
    set +e
    HOOK_OUT="$(printf '%s' "$GOOD_SPAWN_PAYLOAD" | "$SAMPLE_ROOT/scripts/hooks/$hook" 2>&1)"
    HOOK_RC=$?
    set -e
    if [ "$HOOK_RC" -ne 0 ]; then
        BEAT3_OK=0
        printf '    %s✗ %s exited %s (expected 0)%s\n' "$C_RED" "$hook" "$HOOK_RC" "$C_RESET"
        show_output "$HOOK_OUT"
    else
        printf '    ✓ %s: exit 0 (allowed)\n' "$hook"
    fi
done
if [ "$BEAT3_OK" -eq 1 ]; then
    beat_pass "3" "the compliant spawn passed every gate — the guard blocks unsafe spawns, not spawning itself"
else
    beat_fail "3" "the compliant spawn was unexpectedly blocked somewhere in the chain"
fi

# ---------------------------------------------------------------------------
# Beat 4 — a real isolated worktree + a real commit (the engineer's handoff).
# ---------------------------------------------------------------------------
heading "Beat 4 — the engineer builds in an isolated worktree and commits"
narrate 'This is what isolation: "worktree" actually produces: a real, linked git'
narrate 'worktree on its own branch. The engineer'"'"'s commit on that branch IS the'
narrate 'handoff — no message required.'
label_real "git worktree add + git commit (real git mechanics, no mock)"

mkdir -p "$SAMPLE_ROOT/.claude/worktrees"
set +e
WT_ADD_OUT="$(git -C "$SAMPLE_ROOT" worktree add -q -b worktree-demo0001 "$SAMPLE_ROOT/.claude/worktrees/agent-demo0001" 2>&1)"
WT_ADD_RC=$?
set -e
WT="$SAMPLE_ROOT/.claude/worktrees/agent-demo0001"

# The engineer's change: a "flourish" that (accidentally) drops the comma —
# the planted defect this story's QA beat exists to catch.
cat >"$WT/app/greeting.py" <<'PY'
def greeting(name):
    return f"Hello {name}!"
PY
git -C "$WT" add app/greeting.py
git -C "$WT" commit -q -m "Add exclamation flourish to greeting"
COMMIT_SHA="$(git -C "$WT" rev-parse --short HEAD)"

if [ "$WT_ADD_RC" -eq 0 ] && git -C "$WT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    beat_pass "4" "worktree agent-demo0001 on branch worktree-demo0001, commit $COMMIT_SHA — the engineer's handoff, no message needed"
else
    beat_fail "4" "worktree/commit setup failed"
fi

# ---------------------------------------------------------------------------
# Beat 5 — the QA check rejects the planted defect (NARRATED SIMULATION).
# ---------------------------------------------------------------------------
heading "Beat 5 — QA reviews the engineer's commit and finds a defect"
label_sim "there is no live QA agent in this demo (this script is plain bash) — the git"
label_sim "history and the QA check's exit code below are REAL; the \"QA verdict\" is"
label_sim "this demo's own scripted stand-in for what a real QA teammate would do."
narrate "Running the QA check against the engineer's commit:"

set +e
BEAT5_OUT="$(python3 "$WT/app/qa_check.py" "$WT/app" 2>&1)"
BEAT5_RC=$?
set -e
show_output "$BEAT5_OUT"
if [ "$BEAT5_RC" -ne 0 ]; then
    beat_pass "5" "QA rejected the defect — the gate has real teeth, it does not rubber-stamp a broken commit"
else
    beat_fail "5" "QA check should have caught the defect but passed"
fi

# ---------------------------------------------------------------------------
# Beat 6 — the fix lands and QA re-passes (FIX-FIRST loop closes).
# ---------------------------------------------------------------------------
heading "Beat 6 — the engineer fixes it, QA re-checks and passes"
label_sim "same caveat as Beat 5 — the fix commit and the re-run QA check are REAL;"
label_sim "the pass/fail verdict narration stands in for a real QA teammate's signoff."

cat >"$WT/app/greeting.py" <<'PY'
def greeting(name):
    return f"Hello, {name}!"
PY
git -C "$WT" add app/greeting.py
git -C "$WT" commit -q -m "Fix greeting regression: restore the comma"
FIX_SHA="$(git -C "$WT" rev-parse --short HEAD)"

set +e
BEAT6_OUT="$(python3 "$WT/app/qa_check.py" "$WT/app" 2>&1)"
BEAT6_RC=$?
set -e
show_output "$BEAT6_OUT"
if [ "$BEAT6_RC" -eq 0 ]; then
    beat_pass "6" "fix $FIX_SHA passes QA — the FIX-FIRST loop closes, only a verified fix proceeds"
else
    beat_fail "6" "QA check should pass after the fix but failed"
fi

# ---------------------------------------------------------------------------
# Beat 7 — the single-writer land: merge to main, then the integrity probe
# runs green.
# ---------------------------------------------------------------------------
heading "Beat 7 — landing: merge to main, then the enforcement machinery re-verifies itself"
label_real "git merge (the land sequence's core mechanic) + contract-integrity-probe.sh"

set +e
MERGE_OUT="$(git -C "$SAMPLE_ROOT" merge --no-ff -q -m "Merge worktree-demo0001: fix greeting regression" worktree-demo0001 2>&1)"
MERGE_RC=$?
set -e
show_output "$MERGE_OUT"
git -C "$SAMPLE_ROOT" worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"

MERGED_CONTENT="$(cat "$SAMPLE_ROOT/app/greeting.py" 2>/dev/null || true)"
if [ "$MERGE_RC" -eq 0 ] && printf '%s' "$MERGED_CONTENT" | grep -q "Hello, {name}"; then
    printf '    ✓ main now has the fixed greeting.py\n'
    MERGE_OK=1
else
    printf '    %s✗ merge did not land the expected fix%s\n' "$C_RED" "$C_RESET"
    MERGE_OK=0
fi

set +e
# The probe reads the SAME redirected config dir install.sh wrote above: the
# sample repo is a sandbox, and a sandboxed probe does not judge the operator's
# own machine (Layer Q7 reads the operator's launchd only for the real config).
PROBE_OUT="$(CLAUDE_CONFIG_DIR="$SAMPLE_ROOT/.claude-config" "$SAMPLE_ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1)"
PROBE_RC=$?
set -e
show_output "$PROBE_OUT"

if [ "$MERGE_OK" -eq 1 ] && [ "$PROBE_RC" -eq 0 ]; then
    beat_pass "7" "merged to main cleanly, and the integrity probe confirms every layer is still wired correctly after the change"
else
    beat_fail "7" "merge and/or post-merge integrity probe failed"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
END_EPOCH="$(date +%s)"
ELAPSED=$((END_EPOCH - START_EPOCH))

heading "Summary"
if [ "$BEATS_PASSED" -eq "$BEATS_TOTAL" ]; then
    printf '%s✓ Your team'"'"'s enforcement machinery works. %s/%s beats passed.%s (in %ss)\n' \
        "$C_GREEN" "$BEATS_PASSED" "$BEATS_TOTAL" "$C_RESET" "$ELAPSED"
    narrate ""
    narrate "What you just watched, real vs. simulated:"
    narrate "  [REAL ENFORCEMENT]      Beats 1, 2, 3, 4, 7 — actual hook binaries and real git"
    narrate "                          mechanics, unmodified from this checkout."
    narrate "  [NARRATED SIMULATION]   Beats 5, 6 — the QA check's git/exit-code mechanics are"
    narrate "                          real; the \"QA agent\" judgment is this script's scripted"
    narrate "                          stand-in, not a spawned teammate."
    exit 0
else
    printf '%s✗ %s/%s beats passed — the enforcement machinery has a problem.%s (in %ss)\n' \
        "$C_RED" "$BEATS_PASSED" "$BEATS_TOTAL" "$C_RESET" "$ELAPSED"
    narrate "Failing beats:"
    for n in "${BEAT_NAMES_FAILED[@]}"; do narrate "  - $n"; done
    exit 2
fi

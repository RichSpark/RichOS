#!/usr/bin/env bash
#
# engine-status.test.sh — the session banner's guard fraction is DERIVED from
#                          the registration surface, and cannot go stale.
#
# WHY THIS FILE EXISTS
# ====================
# The first line an operator reads every session is "N/M guards". It is the
# number most likely to be trusted at a glance and the least likely to be
# re-checked. It has been wrong twice in two days, both times in the most
# dangerous possible direction — a FULL fraction over a STALE inventory:
#
#   13/13  guard-workflow-ban.sh wired, the banner's typed list not updated.
#          Seen live in a real ~/ab/prospects session. d55f54b fixed the
#          arithmetic (numerator and denominator now walked the same list) and
#          left the typing alone.
#   14/14  guard-worktree-removal.sh wired at 79d6958/084eed3. The typed list
#          was not touched. Within HOURS of the previous fix, femcboost
#          announced "14/14 guards" while 15 guards and the announcer loaded.
#
# The lesson of the second occurrence is that fixing the instance does not
# touch the class. So this suite does not assert "the numbers currently agree"
# — 16 == 16 is true today and would stay true after a seventeenth guard was
# added and miscounted, which is precisely the bug. Every case here CONSTRUCTS
# a drift condition and proves the banner tracks it, and case 3 reconstructs
# the historical defect on purpose to prove these assertions can fail at all.
#
# THE TWO QUESTIONS THE FRACTION ANSWERS, which must never collapse into one:
#   denominator  what the HOST WILL LOAD   (hooks/hooks.json)
#   numerator    which of those are ON DISK AND EXECUTABLE
# A fix that made both sides the same query would produce a permanently full
# fraction — reassuring, cheap, and incapable of ever reporting a gap. Case 4
# exists to stop that.
#
# Run directly:  scripts/hooks/engine-status.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ENGINE="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
FAIL_NAMES=()

ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); FAIL_NAMES+=("$1"); printf '  FAIL  %s — %s\n' "$1" "$2"; }

# The launching session's own environment must not leak in as a candidate root.
unset CLAUDE_PROJECT_DIR RICHOS_ENTITY_ROOT RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT

SANDBOX="$(cd "$(mktemp -d -t engine-status.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# ---------------------------------------------------------------------------
# Sandbox: a REAL engine (no stubs — a stubbed engine can pass this suite while
# the shipped one fails) and a separate adopted entity repository.
# ---------------------------------------------------------------------------
ENGINE="$SANDBOX/engine"
ENTITY="$SANDBOX/entity"

mkdir -p "$ENGINE" "$ENTITY"
cp -R "$SRC_ENGINE/scripts" "$ENGINE/scripts"
cp -R "$SRC_ENGINE/mega-lander" "$ENGINE/mega-lander"
cp -R "$SRC_ENGINE/ass-kicker" "$ENGINE/ass-kicker"
cp -R "$SRC_ENGINE/hooks"   "$ENGINE/hooks"
cp "$SRC_ENGINE/orchestration.config" "$ENGINE/orchestration.config"
cp "$SRC_ENGINE/VERSION" "$ENGINE/VERSION" 2>/dev/null || printf '0.0.0-test\n' >"$ENGINE/VERSION"

printf 'PROTECTED_PATHS="src"\nREADONLY_ALLOWLIST="Explore Plan"\n' >"$ENTITY/orchestration.config"
git -C "$ENTITY" init -q -b main >/dev/null 2>&1
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -q -m adopt >/dev/null 2>&1

HOOKS_JSON="$ENGINE/hooks/hooks.json"
PRISTINE_HOOKS_JSON="$SANDBOX/hooks.json.pristine"
PRISTINE_LIB="$SANDBOX/registered-hooks.sh.pristine"
cp "$HOOKS_JSON" "$PRISTINE_HOOKS_JSON"
cp "$ENGINE/scripts/lib/registered-hooks.sh" "$PRISTINE_LIB"

restore() {
    cp "$PRISTINE_HOOKS_JSON" "$HOOKS_JSON"
    cp "$PRISTINE_LIB" "$ENGINE/scripts/lib/registered-hooks.sh"
    rm -f "$ENGINE/scripts/hooks/guard-brand-new.sh"
    chmod +x "$ENGINE/scripts/hooks/scan-secrets.sh" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
# banner [extra env...] -> sets OUT, SYS_FRAC, MODEL_FRAC
#
# Runs the hook exactly as the host does: no arguments, stdin closed. Both
# channels are read separately and on purpose — the announcement reaching only
# one audience is the defect this hook was rebuilt around, and a fraction that
# silently stopped appearing on the operator's channel would be that defect in
# miniature.
banner() {
    OUT="$(cd "$SANDBOX" && env -u CLAUDE_PROJECT_DIR -u CLAUDE_PLUGIN_ROOT \
        -u RICHOS_ENGINE_ROOT "RICHOS_ENTITY_ROOT=$ENTITY" "$@" \
        bash "$ENGINE/scripts/hooks/engine-status.sh" </dev/null 2>/dev/null)"
    local sys="${OUT%%\"hookSpecificOutput\"*}"
    local model="${OUT#*\"hookSpecificOutput\"}"
    SYS_FRAC="$(printf '%s' "$sys"   | grep -o '[0-9?][0-9?]*/[0-9?][0-9?]* guards' | head -1)"
    MODEL_FRAC="$(printf '%s' "$model" | grep -o '[0-9?][0-9?]*/[0-9?][0-9?]* guards' | head -1)"
}

# expect_fraction <case> <want> — both channels must carry it, identically.
expect_fraction() {
    local case_name="$1" want="$2 guards"
    if [ "$SYS_FRAC" = "$want" ] && [ "$MODEL_FRAC" = "$want" ]; then
        ok "$case_name"
        return 0
    fi
    bad "$case_name" "want '$want' on both channels; operator='$SYS_FRAC' model='$MODEL_FRAC'"
    return 1
}

# wire_extra_guard — add a SEVENTEENTH registered hook to the plugin table and
# put it on disk, executable. This is the exact move that broke the banner
# twice; the shipped code must absorb it with no second edit anywhere.
wire_extra_guard() {
    printf '#!/usr/bin/env bash\nexit 0\n' >"$ENGINE/scripts/hooks/guard-brand-new.sh"
    chmod +x "$ENGINE/scripts/hooks/guard-brand-new.sh"
    python3 - "$HOOKS_JSON" <<'PY'
import json, sys
p = sys.argv[1]
with open(p, encoding="utf-8") as fh:
    d = json.load(fh)
d["hooks"].setdefault("PreToolUse", []).append({
    "matcher": "Bash",
    "hooks": [{
        "type": "command",
        "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-brand-new.sh",
        "timeout": 10,
    }],
})
with open(p, "w", encoding="utf-8") as fh:
    json.dump(d, fh, indent=2)
PY
}

echo "=== the session banner's guard fraction is derived, not remembered ==="
echo ""

# ===========================================================================
# 1. BASELINE — and the expectation is itself derived, never typed.
#
# A test that asserted a literal "15/15" would be a THIRD hand-maintained
# inventory, failing on the next legitimate guard for the same reason the
# banner did. So the expected value is computed here from hooks.json by a
# method deliberately unlike the shipped one (a text scan, not a JSON parse):
# two independent readings of the registration surface must agree.
# ===========================================================================
#
# THE TEXT SCAN IMPLEMENTS THE DISPATCHER RULE TOO, independently. Since
# 2026-09-15 a registered `dispatch-pretooluse.sh <key>` stands for every rule
# scripts/hooks/dispatch-pretooluse.manifest gives that key, and those rules are
# enforcing exactly as much as they were when each had its own entry. A reading
# that skipped them would be a SECOND method that disagrees with the shipped one
# for a reason that is not drift — which is the false alarm this file exists to
# stop producing. The manifest is read here with `sed`, sharing nothing with
# scripts/lib/registered-hooks.sh, so the two readings stay independent.
_es_dispatch_rules() { # <hooks.json> -> the rule basenames its dispatcher runs
    local hj="$1" mf key
    mf="$(dirname "$(dirname "$hj")")/scripts/hooks/dispatch-pretooluse.manifest"
    [ -f "$mf" ] || return 0
    for key in $(grep -o 'dispatch-pretooluse\.sh [A-Za-z]*' "$hj" 2>/dev/null \
                 | awk '{print $2}' | LC_ALL=C sort -u); do
        awk -F'|' -v k="$key" '
            /^[[:space:]]*#/ {next}
            NF>1 && $1==k {gsub(/^[[:space:]]+|[[:space:]]+$/,"",$2); print $2}
        ' "$mf"
    done
}
REGISTERED_LIST="$(printf '%s\n%s\n' \
    "$(grep -o 'scripts/hooks/[A-Za-z0-9._+-]*\.sh' "$HOOKS_JSON" | sed 's|.*/||')" \
    "$(_es_dispatch_rules "$HOOKS_JSON")" | grep -v '^$' | LC_ALL=C sort -u)"
REGISTERED_N="$(printf '%s\n' "$REGISTERED_LIST" | grep -c .)"
# The announcer is deliberately excluded from a count of GUARDS: it guards
# nothing, and its own term could never be unsatisfied (no announcer, no
# banner). engine-status.sh excludes it self-referentially, by its own
# filename, so the exclusion cannot itself become a stale list.
#
# 24 -> 25 on 2026-08-30: guard-completeness-commits.sh and
# guard-row-currency-commits.sh were wired on the same day, by two engineers who
# each bumped this line to 24 without knowing about the other. The merge
# conflicted here, which is the tripwire doing precisely its job: two additive
# registrations that a careless union would have counted once. Fourth firing.
EXPECT_N=$((REGISTERED_N - 1))
# The three fractions the mutations below produce, derived from the same
# reading rather than typed. They used to be literals, and every legitimate
# new guard turned six passing cases red at once for no reason connected to
# what they test — the same hand-maintained-inventory failure this suite was
# written to kill, reproduced inside the suite itself.
EXPECT_PLUS=$((EXPECT_N + 1))    # cases 2/3/6b: one extra guard wired
EXPECT_MINUS=$((EXPECT_N - 1))   # case 4: one guard unavailable or unwired
# Case 3's mutation ships its own hand-typed list; this is ITS length, a
# property of the mutation and not of the engine, so it stays a literal.
STALE_N=14

banner
expect_fraction "1a  baseline: banner reports ${EXPECT_N}/${EXPECT_N}, matching hooks.json minus the announcer" \
    "${EXPECT_N}/${EXPECT_N}"

# 20 -> 22 on 2026-08-29: guard-publication-writes.sh and
# guard-publication-commits.sh were wired, making the public/private repo split
# machinery instead of judgment. This tripwire did its job — it went red on the
# change and made the new count something a human had to acknowledge rather than
# absorb.
#
# 23 -> 24 on 2026-08-29: guard-row-currency-commits.sh was wired, making "the
# working record still describes the work" checkable at every landing.
# 22 -> 23 on 2026-08-29: guard-ceo-todos-commits.sh was wired, making "waiting
# on the CEO" a checkable claim instead of an unfalsifiable one. The tripwire
# fired again, exactly as intended — this line is the acknowledgement.
#
# 23 -> 24 on 2026-08-30: guard-completeness-commits.sh was wired, so the
# publication-COMPLETENESS contract — is everything the public tree claims
# actually delivered — stops being a check somebody has to remember to run. It
# had been CI-and-memory only, and it was red on main for a day because after a
# merge the lander ran the suites and not the check. Third firing, third
# acknowledgement; the tripwire has now caught every guard added since it was
# written, which is the only evidence that it works.
# 23 -> 24 on 2026-08-30: guard-unresolved-claims.sh was wired on Stop — the
# first guard here that reads the orchestrator's own turn rather than the
# repository's state. Tripwire fired, acknowledged.
#
# 25 -> 26 on 2026-08-30: THREE guards were wired on one day by three
# engineers, and each of them bumped this line to 24 without knowing about
# the other two. Every one of those merges conflicted HERE. That is the whole
# value of a typed count beside a derived one: the derivation absorbs a new
# guard silently, and this line refuses to.
# 26 -> 27 on 2026-08-30: turn-manifest.sh was wired on Stop. It is the first
# entry in this count that is NOT a guard — it refuses nothing and renders
# only, printing each turn's real tool statuses beneath whatever the turn
# claimed. The noun in the banner is therefore now one wide, and that is a
# deliberate choice rather than an oversight: the announcer is excluded because
# its term could never be UNSATISFIED (no announcer, no banner to read the
# fraction in), and that reasoning does not transfer. turn-manifest.sh can
# absolutely be missing or non-executable, and a shortfall there is a real
# signal an operator should see at session start. Excluding it would need a
# second, hand-typed exception list beside the self-referential one — the same
# stale inventory walking back in through a different door. Counted, and the
# imprecision named here instead. Fifth firing.
#
# 26 -> 28 on 2026-08-30: the hook-staleness PAIR was wired —
# snapshot-enforcing-hooks.sh on SessionStart and notice-hook-staleness.sh on
# Stop — which together tell the operator, mid-session, that a guard landed
# since this session booted is enforcing nothing and that RESTARTING THE SESSION
# is what arms it. Two scripts, hence two, and the tripwire fired as designed;
# this paragraph is the acknowledgement it demanded. Note the recursion, which
# is the joke this pair cannot afford to play straight: the mechanism that
# reports inert hooks is itself a hook, so it is inert in the session that lands
# it, and arming it is the same request it makes of everyone else — re-run
# install.sh, then restart.
#
# 27 -> 29 on 2026-08-30: notice-hook-staleness.sh and its sibling were wired.
# Both acknowledgement paragraphs above are kept because both are true history;
# main said 27 and the branch said 28, and the merged truth is 29. That is the
# fifth time today two engineers each bumped this line blind and the merge
# conflicted HERE, which is the entire point of a typed count sitting beside a
# derived one: the derivation absorbs a new hook silently and this line refuses
# to, so a human has to look.
#
# 26 -> 27 on 2026-08-30: guard-idle-land.sh was wired on Stop — the SECOND
# guard that reads the orchestrator's turn rather than the repository, and the
# first that refuses a turn for what it did NOT do. The tripwire went red on
# the change, as designed, and this paragraph is a human having looked. Sixth
# firing. It has now caught every guard added since it was written; nothing
# else in this engine has that record, which is why nobody should ever be
# tempted to derive this line too.
#
# 29 -> 30 on 2026-08-30: guard-idle-land.sh was wired -- the guard that refuses
# a turn which landed work and started nothing. Sixth firing today. Six guards,
# six blind bumps of this same line, six conflicts here. The count is not the
# point; being made to look is.
#
# 30 -> 33 on 2026-08-30: the in-flight sweep, wired as three, because the two
# guarantees it has to make are two different problems and the third is the
# timeout underneath them. notice-inflight-sends.sh witnesses the lead's send on
# PostToolUse[SendMessage]; guard-inflight-notify.sh refuses a push that leaves a
# live teammate behind with no such witness; notice-inflight-acks.sh surfaces an
# acknowledgement that never arrived. Seventh firing, and the first one to move
# this line by more than one -- which is itself worth having been made to look at.
#
# 34 -> 38 on 2026-08-31: the CEO-ask gate, wired as FOUR, because the four
# questions it has to answer happen on four different events and no one of them
# can answer another. notice-ceo-asks.sh witnesses the lead actually asking, on
# PostToolUse[AskUserQuestion]; guard-ceo-ask-first.sh REFUSED a teammate
# dispatch while nothing had been asked (removed 2026-10-24); notice-ceo-unasked.sh will not let a
# turn end quietly with a prepared decision never surfaced; and
# session-start-ceo-ask.sh opens with the question rather than with a count of
# the questions. Eighth firing, and the largest single move this line has taken
# — which is exactly the size of change a human should be made to look at.
#
# 38 -> 40 on 2026-08-31: TWO guards in one wave, from two engineers, and the
# number is set to the post-merge truth rather than to either branch. The lead
# named the arithmetic in an in-flight notice: main had already moved 38 -> 39
# with notice-unasked-deferral.sh, and guard-agent-state-claims.sh — the Stop
# hook that will not let a turn end quietly claiming a NAMED agent has finished
# while its isolation-worktree lock is held — makes 40.
#
# WHICH MEANS THIS CASE IS RED IN THE BRANCH THAT WRITES IT, deliberately, and
# that is stated rather than discovered: the worktree it was written in
# registers 39, so case 1b fails there and goes green at the merge. The
# alternative was to type 39, be green in isolation, and hand the lander a
# tripwire that fires on a correct merge — which is the tripwire lying, and the
# one thing a tripwire may never do. Ninth firing.
# 40 -> 41 on 2026-08-31: dialect-guard.sh was wired — the guard that refuses a
# write introducing a word outside the repository's declared dialect. Ninth
# firing, and one guard this time, but the reason it exists is the reason this
# tripwire exists: the rule it enforces was written down, swept across 654
# sites, and undone within HOURS, because nothing made anyone look at the moment
# of the write. This line made me look. That is the mechanism working on its own
# author.
#
# The tick's text used to say "registers 33 scripts" beside a test for 34. It
# had been wrong for as long as it had been green, because a literal inside a
# PASS message is a second inventory that nothing checks — the same defect this
# very tripwire exists to catch, hiding inside the tripwire. The number is
# derived from REGISTERED_N now; the THRESHOLD stays typed, because being made
# to look is the whole point of it.
# SET AGAINST MAIN, NOT AGAINST THIS BRANCH'S BASE, ON THE LANDER'S INSTRUCTION
# (2026-08-31). This branch was cut at 38 registered scripts. TWO other guard
# branches landed while it ran — 38 -> 39 -> 40 — and dialect-guard.sh makes it
# 41. Verified against `git show fb4a5628:engine/hooks/hooks.json`, not taken on
# trust from the notice. So THIS LINE IS RED IN ISOLATION on this branch and
# green only at main + this branch. Deliberate: the alternative, 39, was green
# here and would have been wrong the instant it landed — a tripwire that fires
# on a CORRECT merge, which is worse than one that fires on a wrong one.
#
# 41 -> 42 on 2026-09-01: guard-interactive-prompt.sh — the first guard in this
# engine that asks whether a COMMAND CAN WAIT ON A HUMAN, rather than what a
# file or a name says. Tenth firing, and the one where the tripwire's premise is
# most literally true: forty-one guards were registered on the night a macOS
# password window appeared on the CEO's screen, and the count is the only place
# a reader is made to notice that a forty-second kind of question now exists.
# Verified post-merge rather than assumed: main registers 41 at 62507d0, and no
# other live branch (zach-opus-c1, zach-opus-n1, the three echo branches) adds a
# hook to hooks.json, so 42 is the merged truth and is green here too.
# 42 -> 42 on 2026-09-01, and the NON-MOVE is the thing to have been made to
# look at. The idle-land gate's predicate was rewritten — a second completion
# trigger, the removal of a stand-down that was waving through 41% of landing
# turns, three new routes for a legitimate stop — and the engine's first
# FUNCTIONAL probe layer on the Stop event (IL) landed with it. Not one of those
# is a new registration, so this line does not move.
#
# That is worth a paragraph rather than silence, because the tripwire's own
# premise invites the wrong inference. It exists so a guard cannot slip in
# unregistered; it says nothing about whether the forty-two already registered
# are ENFORCING. This gate was registered, hashed, executable and counted here
# for two days while refusing almost nothing, and this number was green
# throughout. A count of guards is not a measure of enforcement, and eleventh
# firing or not, the thing that caught the defect was a functional canary.
#
# 42 -> 45 on 2026-09-02, and the jump is THREE because the tripwire had gone
# stale by one before this branch touched it. MEASURED, not assumed, against
# the merge that will actually happen:
#
#   main registers 43. The forty-third is agent-finished-reap-worktrees.sh,
#     wired on TeammateIdle and TaskCompleted by another branch that landed
#     without moving this line — so case 1b has been RED ON MAIN since, and
#     that is the tripwire working, not failing. Its whole job is to make a new
#     registration something a human acknowledges.
#   this branch adds TWO: guard-ceo-ruled-ask.sh on PreToolUse[AskUserQuestion]
#     and notice-ceo-ruled-prose.sh on Stop.
#   no other live branch adds one. Checked by counting the registration in
#     every branch of this repository, not by asking: the maximum anywhere is
#     main's 43.
#
# Twelfth firing, and the pair is worth the paragraph because they are the
# engine's FIRST guards on the AskUserQuestion event, and the first anywhere
# that refuse a turn for what the RECORD already says rather than for what the
# turn does. Forty-two guards were registered on the evening the CEO answered
# three questions he had already answered, in his own words, in files this
# session wrote — and every one of them was green. A count of guards is not a
# measure of what is being checked; the two-sided canary in ceo-ruled.test.sh
# is, and it is the thing that would catch this pair going dead.
# 45 -> 47 on 2026-09-02, in two branches that landed the same day and merged
# before this one was handed off: notice-waiver-repetition.sh (the escape-hatch
# ledgers, read at last) and notice-mechanical-findings.sh (the Stop-time sweep
# that turns a skipped suite, an unrun harness or an untested hook into a row of
# the working record).
# 47 -> 48 on 2026-09-02: ass-kicker.sh was wired on Stop — the
# guard that refuses a turn whose REPORT does not match its ACTIONS: a stated
# dispatch the turn never made ("Frank breaks it first", no Agent call), or a
# teammate's completion answered with a report and nothing started or declared.
# Seven narrated-not-taken actions in one day, six "where is the next Sage"
# messages from the CEO, and guard-idle-land's own log showing it stood down on
# every one of them because the backlog had no free row. This paragraph is a
# human having looked.
# 48 -> 50 on 2026-09-03: the worktree lifecycle's two new hooks
# (docs/plans/worktree-real-fix-2026-09-03.md) — record-subagent-start.sh on
# SubagentStart (the nonblocking start-fact writer the seal needs) and
# guard-sealed-worktree.sh, matcherless and FIRST under PreToolUse (the write
# barrier: a worker whose worktree manifest is not sealed can read and cannot
# write). Both are on the probe's managed set and both surfaces.
# 50 -> 51 on 2026-09-03: terminalize-agent-worktrees.sh, the terminal ingress,
# wired on SubagentStop and on WorktreeRemove (one script, two events, one
# compare-and-set claim between them).
# 51 -> 50 on 2026-09-03: agent-finished-reap-worktrees.sh REMOVED. TeammateIdle
# and TaskCompleted are diagnostic only (every one of their 580 ledger rows is
# a test fixture; never fired for a real agent) and hold no destructive
# authority any more; the only removal path is the terminal ingress plus the
# reconciler.
# 50 -> 51 on 2026-09-03: guard-model-ceiling.sh, wired LAST on
# PreToolUse[Agent] at b96b1f7 -- the guard that refuses a spawn above the
# declared MODEL_CEILING. Thirteenth firing, and the FIRST one nobody
# acknowledged: b96b1f7's own message says "Counted, in every inventory that
# exists" and names five (Layer R, BR2, Layer C, Layer M, install.sh's managed
# set). This one is not among them, so case 1b was RED ON MAIN from 19:31 that
# evening until it was read. The tripwire worked; the list of places to look
# was the thing that was short.
#
# 51 -> 52 on 2026-09-04: guard-vendoring-commits.sh, wired last on
# PreToolUse[Bash] -- it refuses a commit that adds redistributable material
# with no entry in .richos/vendored-material saying where it came from. The
# occasion was an audit that found 15 of this engine's 27 skills came from
# outside the project with exactly ONE of them ever written down, in a commit
# message. Fourteenth firing, and this time the derivation above was used
# rather than guessed at: `grep -rn guard-row-currency-commits engine/` named
# every inventory a Bash-matcher guard has to appear in, and this set was one
# of them.
#
# WHY THE THRESHOLD IS NOW A SET AND NOT A NUMBER
# ===============================================
# It stays TYPED -- deriving both sides would make this "X == X", true forever
# and incapable of firing, which is deleting the case rather than fixing it.
# But an integer was the wrong resolution for the job, in two ways this file
# had already been bitten by:
#
#   * A COUNT CANNOT SEE A SWAP. Remove one registration and add another in the
#     same land and 51 is still 51: a new guard slips in unacknowledged and a
#     removed one goes unmourned, with a green tick over both. The line
#     directly above this block is exactly that shape -- 51 -> 50 was a
#     REMOVAL, and had an addition ridden along with it nothing here would have
#     said a word.
#   * A COUNT MADE THE FAILURE MESSAGE LIE. The threshold counts the
#     registration surface with the announcer IN; the banner's fraction has it
#     OUT. They differ by exactly one, so when the true registration was 51 the
#     message read "hooks.json registers 51 scripts -- the banner should now
#     read 50/50" at a moment when the banner ALREADY read 50/50 and was
#     correct. An instruction that looks satisfied on arrival is an instruction
#     nobody carries out. Both scales are named explicitly below.
#
# So the acknowledged thing is the SET, one name per line: a human still has to
# look, the diff says WHICH script arrived or left instead of leaving the next
# reader to bisect hooks.json for it, and a merge of two branches that each
# wired a guard conflicts per-line rather than over one contested integer.
#
# AND IF YOU ARRIVED HERE BECAUSE THIS CASE WENT RED, READ THIS PARAGRAPH.
# This set is one of SEVERAL inventories a newly registered script has to be
# added to, and the reason it fell behind on 2026-09-03 is that the engineer
# who wired the fifty-first knew about five of them and not this one. There is
# deliberately NO CHECKLIST of the others here -- a typed list of places to
# look is the same object this whole file exists to refuse, and it would go
# stale in exactly the way the count did. DERIVE it instead:
#
#     grep -rn <an-already-fully-wired-script.sh> engine/
#
# Pick a script that is registered on the same event and matcher as yours and
# has been in place for a while; every file that names it is a file that has to
# name yours. That answer is computed from the tree at the moment you ask,
# which is the only kind of answer that cannot rot.
#
# 52 -> 54 on 2026-09-05: the escalation channel's two halves —
# notice-escalations.sh on Stop and session-start-escalations.sh on
# SessionStart. Two teammates wrote a correct `BLOCKED.md` on 2026-09-02 and a
# worktree cleanup found them on 2026-09-04, because a file on a teammate's
# branch is read only by whoever merges that branch. The escalation now goes to
# a ledger outside every repository and these two are what make it ARRIVE: the
# turn-end half for the session it was raised in, the session-start half for
# every session after — which is the one the incident turned on, because the
# session that could have surfaced them ended. The derivation above was used
# rather than guessed at: `grep -rln notice-waiver-repetition engine/` and
# `grep -rln session-start-ceo-ask engine/` named both surfaces, this set,
# Layer R and BR2.
#
# 54 -> 56 on 2026-09-05: the named-person deny-list's two halves —
# guard-named-persons-commands.sh on PreToolUse[Bash] and
# guard-named-persons-writes.sh on PreToolUse[Write|Edit|MultiEdit]. They
# landed with the scrub work after a contractor's name reached a public
# repository, and the list they read lives at ~/.richos-privacy/named-persons,
# outside every repository by construction — the loader refuses a path inside
# a git work tree. FIFTEENTH firing, and this one is worth reading because
# the tripwire was the SECOND consumer to catch the same omission rather than
# the first. demo.sh died in setup the same day for the identical reason —
# scripts/lib/named-persons.sh missing from its DEMO_FILES, so both guards
# refused to start and the sample company could not be assembled. That is a
# public repository's first-run script, broken for everyone who tried it.
# The derivation above was used rather than guessed at:
# `grep -rln guard-named-persons-writes engine/` named eleven files, and every
# one of them already carried the guards except this set. So the registration
# was thorough and this inventory was the single gap — which is precisely the
# shape this case exists to catch, and why it says never to silence it.
# 56 -> 57 on 2026-09-06: notice-unlanded-branches.sh on Stop. A session ended
# reporting "Everything is clean and pushed" while six FINISHED branches sat
# outside main, one of them the fix for the CEO's own complaint that the app
# steals keyboard focus. Both checks that session ran were green and both were
# CORRECT -- the working tree was clean and main did match origin/main -- and
# neither of those two facts can see an unlanded branch. This hook names them
# at the turn end. The derivation above was used rather than guessed at:
# `grep -rln notice-unstarted-rows engine/` named twenty-one files; of those,
# the ones that are INVENTORIES rather than prose about that particular hook
# are hooks/hooks.json, .claude/settings.local.json, this set, and the probe's
# Layer R list in contract-integrity-probe.sh. The first three carry it. THE
# FOURTH DOES NOT, deliberately and with the cost stated: that file was owned
# by another engineer at the moment this landed, so `notice-unlanded-branches`
# still has to be added to R_ROOTED_HOOKS. Nothing goes red without it -- that
# list is typed, not derived -- which is exactly why it is written down here
# instead of left to be noticed.
# THAT DEBT IS SETTLED, 2026-09-10: R_ROOTED_HOOKS now carries
# `notice-unlanded-branches` and `notice-land-disposition` both.
# AND THE DEBT CLASS IS GONE, 2026-09-14. Four other rooted hooks made the same
# note and were never paid, so Layer R was walking 52 of the 57 registered hooks
# that resolve a root, one of them already carrying a divergent bootstrap and
# nothing red. Layer R now DERIVES the hooks it walks from hooks/hooks.json, so
# a rooted hook owes it nothing and the only way out of the check is an explicit
# entry in R_ROOTLESS_HOOKS. Every "still has to be added to R_ROOTED_HOOKS"
# note below this line is history, not an instruction.
# notice-land-disposition.sh, added 2026-09-10 on Stop, and it is the OTHER
# half of the hook above rather than a louder version of it. That one answers
# "is anything ahead of main that nobody holds" -- a fact about branches. This
# one answers "is anything OWED" -- a fact about records: finished work is
# LANDED, or it is HELD FOR A REASON SOMEBODY WROTE DOWN, and there is no third
# state. The third state is what happened four days after the incident above:
# five finished agents, five workspaces held because their branches were
# unmerged, no land, no deadline, no demand, and nobody the hold belonged to.
# The founder found them in his own IDE. Every mechanism involved was behaving
# correctly -- holding an unmerged branch is right, and the branch notice had
# already reported them, once, accurately, that morning. What was missing was a
# requirement on the work. The FOUR inventories a Stop registration has to
# reach were derived the way the note above says rather than guessed:
# hooks/hooks.json, .claude/settings.local.json, this set, and the probe (both
# its Layer R list and its BR registration table). ALL FOUR CARRY IT, so this
# one leaves no inventory owing. The second surface was found by
# stop-hook-visibility.test.sh case 1b rather than by memory -- a hook on one
# surface and not the other is enforcement that exists in one installation mode
# and not the other.
# shell-evidence.sh was registered on 2026-09-06 to preserve Bash pipeline and
# sequence failures; acknowledge it here as well as in the derived probe inventory.
# guard-pierce.sh, added 2026-10-06 (Codex): the PreToolUse[Agent] brief inspector. It
# runs scripts/lib/pierce.py on a real dispatch (never spawn.sh's dry runs) and returns
# findings to the brief's author; it never rewrites the prompt. Registered in
# hooks/hooks.json and .claude/settings.local.json; Layer R exempts it as rootless.
# strip-ack-lines.sh, added 2026-10-06: a PreToolUse[Agent] input transformer, last in
# the chain. It returns the spawn prompt without acknowledgment lines (owned-state-ack:,
# reference: and the like); the guards run in parallel on the original input, so they
# still read those lines and the teammate does not.
# guard-stale-staging.sh, added 2026-09-06: the staging-staleness gate on
# PreToolUse[Agent]. It refuses a dispatch that works on, or tests against, a
# product tree whose landed commits have not reached staging. The OTHER
# inventories a registration has to be added to were derived the way the note
# further up says rather than guessed: `grep -rln guard-model-ceiling engine/`
# named hooks/hooks.json, .claude/settings.local.json, this set, and in
# contract-integrity-probe.sh the SC1 guard list, the BR_EXPECTED spec table,
# the BR2 Agent-chain order, CANONICAL_AGENT_CHAIN and the double-registration
# list, plus contract-integrity.test.sh's own sandbox chain. All eight carry it,
# so unlike the notice-unlanded-branches note above, this one leaves no
# inventory owing.
# guard-brief-scope.sh, added 2026-09-14: the ninth PreToolUse[Agent] gate. It
# refuses a dispatch whose brief has drifted outside the scope the CEO actually
# set, and it is silent on every body of work with no recorded spec. It is also
# the hook that PROVED this acknowledgement list is not the expensive half of
# adding a hook: it landed registered in hooks/hooks.json alone, and four
# separate suites went red on `main` for inventories nobody had told its author
# about. Two of those inventories are now DERIVED (the probe's Layer C chain and
# BR2's order rule, both from hooks/hooks.json via scripts/lib/registered-
# hooks.sh), so the places a tenth hook must be named by hand are fewer than the
# places this one was.
# guard-owned-state.sh, added 2026-09-10: the standing-ownership gate on
# PreToolUse[Agent]. It refuses ONE dispatch per session while the oldest
# system whose health the orchestrator owns — CI, worktree reclamation,
# unacknowledged escalations, ECS capture, staging freshness, unlanded work —
# is standing with nothing done about it. It exists because every defect the
# founder found himself on 2026-09-10 was UNASSIGNED, and because the worst of
# them was already printed in that session's own start-up notice and was read
# past: surfacing had already been tried and had already failed. The OTHER
# inventories were derived the way the note further up says rather than
# guessed: `grep -rln guard-stale-staging engine/` named hooks/hooks.json,
# .claude/settings.local.json, this set, README.md's guard table, and in
# contract-integrity-probe.sh the BR_EXPECTED spec table, the BR2 Agent-chain
# order, CANONICAL_AGENT_CHAIN and the double-registration list, plus
# contract-integrity.test.sh's own sandbox chain. All nine carry it, so this
# note leaves no inventory owing.
# guard-ci-red-lands.sh and session-start-ci-surface.sh, added 2026-09-10: the
# two halves of the CI surface — a PreToolUse[Bash] gate that refuses a land
# into a red workflow, and the SessionStart pass that reports the surface. They
# landed at 19037049 registered in hooks/hooks.json and named in NO other
# inventory, and this case is the one that said so, exactly as designed. The
# rest of that landing's debt was cleared at b2354c5d (BR_EXPECTED, and the
# executable bit on both files, which were committed 644 and therefore loaded
# nothing while the banner read 60/62); this set was the last one owing.
# Recorded rather than fixed silently: esc-20260910T080736Z-3a5c8f97 names all
# five duties and stands as the record of how a landing came to owe five.
# deliver-review-verdict.sh, added 2026-10-09: a matcherless PreToolUse delivery that
# hands a second-review verdict to the running teammate whose work it is, once, as
# additionalContext (CEO §113, slice 3 of the automatic second review). It never blocks.
# Registered in hooks/hooks.json and .claude/settings.local.json; Layer R exempts it
# as rootless.
# guard-stop-live-work.sh, added 2026-09-10: the PreToolUse[TaskStop] gate that
# refuses to destroy a teammate that is provably still running. Its inventories
# were derived the way this note prescribes rather than guessed --
# `grep -rln guard-stale-staging engine/` -- and it is named in hooks.json,
# BR_EXPECTED, this set, README.md's guard table and install.sh's hashed set
# (the predicate and the ack recorder, since the guard itself is derived from
# hooks.json).
# 66 -> 64 on 2026-09-11: the CEO's workspace spec
# (docs/plans/worktree-spec-2026-09-11.md). REMOVED: record-subagent-start.sh,
# terminalize-agent-worktrees.sh, session-start-reap-worktrees.sh and
# notice-land-disposition.sh -- the transaction store's writers, the terminal
# ingress, the session-start deleter and the land-disposition demand, none of
# which is on the page. ADDED: workspace-lifecycle.sh (the platform's facts on
# six events) and guard-workspace-gate.sh (the page's point-5 turn-end gate on
# Stop). The other inventories were derived the way the note above says:
# hooks.json, .claude/settings.local.json, the probe's BR_EXPECTED, Layer R and
# Layer M lists, and Layer Q, all of which carry the change.
# observe-created-refs.sh, added 2026-09-13 for a registration landed at
# 84e12d32 (2026-09-12): the POST half of the created-refs pair, matcherless on
# PostToolUse. guard-sealed-worktree.sh snapshots every repository's refs when
# an agent's tool call starts; this one reads them again when it ends, so a ref
# that is new AND carries that agent's unlanded work was CREATED by it (the
# workspace spec's points 3 and 10). It landed in hooks/hooks.json and in NO
# other inventory, and four suites said so — this case, hook-staleness case 11,
# session-evidence's registration test and the probe's BR2 — which is the case
# working, not four failures. The other inventories were derived the way this
# note prescribes rather than guessed: `grep -rln guard-sealed-worktree engine/`
# (its own PRE half) named hooks/hooks.json, .claude/settings.local.json, this
# set, the probe's BR_EXPECTED and README.md's table. THE SEATED SURFACE WAS A
# REAL GAP, not paperwork: the pair's PRE half was wired in
# .claude/settings.local.json and the POST half was not, so in this repository's
# own sessions the snapshot was taken and never read.
# guard-ci-turn-gate.sh, added 2026-09-13: the THIRD leg of the CI surface, and
# the only one that costs anything at the moment somebody is about to walk away.
# The gate (guard-ci-red-lands.sh) makes red cost something at a land; the
# notice (session-start-ci-surface.sh) reports it at session start; between
# them a richos-hq workflow was red for SIXTEEN DAYS, named in every session
# banner, and nobody moved. This one refuses to end the orchestrator's TURN
# while a commit THIS SESSION PUSHED is failing CI — derived from the
# transcript's own `git push` calls, judged per commit rather than per
# repository, with a 2-second wall-clock budget that ALLOWS on expiry and a
# logged `ci-red-ack:` hatch that refuses a bare marker. The other inventories
# were derived the way the note above prescribes rather than guessed:
# `grep -rln guard-workspace-gate engine/` named hooks/hooks.json,
# .claude/settings.local.json, this set, and in contract-integrity-probe.sh the
# BR_EXPECTED table, Layer R's R_ROOTED_HOOKS and the double-registration list.
# README.md's table carries it too. It sits immediately after
# guard-workspace-gate.sh in the Stop chain on both surfaces: the workspace
# spec's structural hold is settled first, and only then is the turn asked
# about the state of what it pushed.
# handoff-facts-annotate.sh and notice-protected-ref-moves.sh, added 2026-09-14
# for two registrations landed the same night (ee2d3797 and d2be875f). The first
# is PostToolUse[Bash|Write|Edit|MultiEdit|NotebookEdit]: it appends a measured
# facts block to a restart note as the note is written, so the numbers in a
# handoff are taken by the machine rather than recalled by the session least able
# to verify them. The second is a non-blocking Stop hook: it refuses to let a
# turn end quietly while a protected branch is missing commits it held, after the
# old check MOVED refs/heads/main three times in one night with no
# compare-and-swap. The OTHER inventories were derived the way the note further
# up prescribes rather than guessed: `grep -rln guard-ci-turn-gate engine/` named
# hooks/hooks.json, .claude/settings.local.json, this set, README.md's table,
# contract-integrity-probe.sh and install.sh. Against that list the pair owed
# exactly two things and both are now paid: the seated surface carried
# notice-protected-ref-moves.sh but NOT handoff-facts-annotate.sh (a real gap —
# in this repository's own sessions the block was never written), and the probe's
# BR_EXPECTED named neither. README.md's table is a per-SYSTEM table rather than
# a per-hook one and names neither; install.sh hashes the root-resolution library
# and its dependants, and NEITHER hook resolves a root, so neither belongs in
# R_ROOTED_HOOKS — putting them there would make Layer R assert something false.
#   CORRECTION, 2026-09-14: that sentence is half wrong and it was believed for
#   a day. handoff-facts-annotate.sh resolves no root and the claim holds for it.
#   notice-protected-ref-moves.sh DOES — measured, it both sources
#   resolve-roots.sh and assigns ENGINE_ROOT, and it was in R_ROOTED_HOOKS at the
#   moment this paragraph said it should not be. The same sentence was copied
#   into hook-registration-completeness.sh's header, where it is corrected too.
#   Both hooks are now derived rather than typed, so neither claim decides
#   anything any more; the correction is here because the reasoning is what a
#   later reader copies.
# Four suites said all of this rather than memory: this case, hook-staleness case
# 11, session-evidence's registration test (which derives its set FROM the probe,
# so the BR_EXPECTED line fixed it too) and the probe's own BR2.
# notice-claim-capability.sh, added 2026-09-14. PostToolUse[Bash|Write|Edit|
# MultiEdit|NotebookEdit], non-blocking: when a RECORD is written it states what
# the cited command is capable of establishing, beside what the sentence asserts.
# It is a DELIVERY SURFACE, not a new check — it imports brief-provenance.py and
# calls check_capability, because that check was wired into the spawn path and
# measured there at ONE row across 23 briefs, a row that is itself a false
# positive, while not one of the four real instances happened in a brief. Its
# inventories were derived by running hook-registration-completeness.sh rather
# than guessed, and it owes nothing to R_ROOTED_HOOKS (it resolves no root) or
# to CANONICAL_AGENT_CHAIN (it is not a PreToolUse[Agent] hook). Suites:
# notice-claim-capability.test.sh (14 cases, six of them negatives) and
# claim-capability-delivery.mutation.sh (8 mutants), the latter run FROM the
# suite it mutates so it cannot go unrun. Record:
# docs/verification/claim-capability-delivery-2026-09-14.md.
#
# guard-hook-registration-commits.sh, added 2026-09-14: the PreToolUse[Bash]
# gate that answers THIS CASE'S OWN COMPLAINT. Case 1b has said for months that
# "this set is NOT the only inventory a registration has to be added to" and
# pointed at a comment for deriving the rest — and on 2026-09-14 that prose cost
# five CI units red on main at once, three hooks landing within two hours, each
# missing a different subset, three engineers hitting it independently (type X,
# §10h of the 2026-09-13 lifecycle failure record). The guard refuses a commit
# or a push whose newly registered hook is absent from any inventory, and names
# each missing place with its fix. THE INVENTORY LIST IS DERIVED, NOT TYPED: a
# file that names every one of the other registered hooks is an inventory, which
# on this tree picks out exactly hooks.json, .claude/settings.local.json, the
# probe and THIS FILE — and measures README.md (41/68) and install.sh (20/68)
# out, which is the pair a hand audit had to argue about. Its own other
# inventories were derived by running it against itself rather than guessed:
# hooks/hooks.json, .claude/settings.local.json, this set, the probe's
# BR_EXPECTED table and Layer R's R_ROOTED_HOOKS (it resolves a root), plus
# scripts/hook-registration-completeness.test.sh so ci-affected-units A5 has a
# suite naming it. Layer M's CANON list carries it too — not demanded by any
# suite, so the guard reports that one as advice rather than refusing over it.
#
# reader-teammate-hint.sh, REMOVED 2026-09-14 — the first entry this list has
# ever LOST, and the removal is the point. It fired only when `subagent_type`
# was a generic agent type (Explore / Plan / general-purpose / claude); clause 5
# of guard-worktree-isolation.sh, which runs FIRST in the same PreToolUse[Agent]
# chain, already exits 2 on exactly that condition. So it could only ever be
# reached on a spawn whose `generic-agent:` hatch had already been accepted and
# logged — one occurrence in .claude/state/generic-agent-dispatches.log in the
# ledger's whole life, and that one carried no reading task. Its invariant is
# subsumed; the ordering makes it unreachable. Removing it cost edits in 13
# files, which is the measurement that motivated docs/verification/
# verification-layer-design-2026-09-14.md.
# left-off-report.sh, ADDED 2026-09-15 — SessionStart + UserPromptSubmit, and
# the only entry here that refuses nothing. When he comes back after a gap it
# puts his own last pre-gap message, and what happened across the gap as GIT
# reports it, in front of the assistant before he asks. Lifecycle failure types
# 60 and 61: he returned after nine hours, asked "TLDR, plain English", was
# answered about something else, and it took sixteen messages and thirty-nine
# minutes to get one line that two commands produce in a second.
# session-start-scratch.sh, ADDED 2026-09-17 — SessionStart, refuses nothing,
# and SILENT on a healthy machine. It is the third leg of the scratch reaper:
# scripts/scratch-reaper.sh deletes dead scratch on a launchd schedule and
# writes ~/.claude/state/scratch-reaper.log, and neither of those puts one word
# in front of a person. It speaks only when more than the declared
# SCRATCH_NOTICE_BYTES is reclaimable, when more than that much is UNDECIDABLE
# (the pile the scheduled job will never clear on its own), or when the
# scheduled job is not installed or has stopped completing passes. The morning
# it exists for: 2026-09-17, 1.8 GB free of 460 GB, 19 GB of dead scratch, and
# a person deleting it by hand after the operating system shouted. It resolves
# no entity root, so it is declared in the probe's R_ROOTLESS_HOOKS; suite:
# session-start-scratch.test.sh (9 cases, four of them silence).
# session-start-quota.sh, ADDED 2026-09-25 — SessionStart, refuses nothing. It
# tells every lead the CEO's 93% quota rule in his words (ruling §87), the
# threshold as declared (QUOTA_PAUSE_PERCENT in orchestration.config), the
# reading now, and the command that starts scripts/quota-watch.sh --watch. On
# additionalContext only, never systemMessage: the budget design notes keep
# the percentage out of the CEO's view (R4). Silent where the engine is not
# adopted. It resolves no entity root itself (quota-watch.sh does), so it is
# declared in the probe's R_ROOTLESS_HOOKS; suite: session-start-quota.test.sh.
# session-start-stall.sh, ADDED 2026-09-28 — SessionStart, refuses nothing. It
# tells every lead that the stall watcher (scripts/stall-watch.sh --monitor,
# started by the engine's plugin monitor) runs with the session, what it
# reports, that it only reports, and the one-line --alive check. It exists
# because three teammates sat 70-85 minutes queued behind one proof-run slot
# on 2026-09-28 and nothing woke the lead. additionalContext only; silent where
# the engine is not adopted; it resolves no entity root itself (stall-watch.sh
# does), so it is in the probe's R_ROOTLESS_HOOKS; suite: stall-watch.test.sh.
# notice-disk-alert.sh, ADDED 2026-09-19 — SessionStart, refuses nothing, and
# the CEO's own §54: when the disk falls below the declared DISK_CEO_NOTIFY_GB
# it tells him at session start and keeps telling him until the space is back,
# because a cleanup that failed must reach a person rather than a log. IT WAS
# WIRED WITHOUT BEING ADDED HERE, and case 1b has been RED on main ever since:
# 74 registered against 73 acknowledged. Nothing noticed, because
# hook-registration-completeness.sh only evaluates this file's unanimity when a
# NEW hook script appears — so the first person to add a hook inherits it, and
# on 2026-09-19 that was the §61 home-network guard, whose commit the
# completeness predicate refused fail-closed until this line existed.
# guard-no-home-network-phone.sh, ADDED 2026-09-19 — BLOCKING, and the first
# RULE on BOTH surfaces: PreToolUse[Agent] in hooks.json and the Write chain's
# manifest, because ceo-decisions §61's addendum names "any brief, code write
# or spawn". It refuses a text that ties the phone surface to a home-network
# path unless a `ceo-ruled-home-network:` citation RESOLVES against
# wiki/ceo-decisions.md. §61 ruled on 2026-09-18 that a mobile app is for
# outside the home network; three briefs on 2026-09-19 kept the "At home only"
# route anyway and one invented a ruling to justify it, and he asked how many
# more times it would happen. Measured over 269 real briefs and spawn prompts:
# 7 refused, all of them home-path work. Suite:
# guard-no-home-network-phone.test.sh (48 cases); harness:
# home-network-phone.mutation.sh (12 properties). It appears TWICE in the
# probe's BR_EXPECTED, once per registration, and is deliberately absent from
# Layer M's single-registration list.
# guard-host-display-power.sh, ADDED 2026-09-19 — BLOCKING, and the MOST
# registered rule in the engine: FOUR entries, on Agent, on SendMessage, and in
# BOTH dispatcher chains (Bash and Write), because ceo-decisions §65 says "no
# agent, and no brief, MESSAGE or script" may touch the host's display, sleep,
# lock, session or input. The SendMessage entry is not symmetry — the
# `pmset displaysleepnow` that blacked out the CEO's monitors reached the
# teammate through the mailbox, and its spawn prompt does not contain the word.
# Suite: guard-host-display-power.test.sh; harness:
# host-display-power.mutation.sh.
# IT WAS WIRED WITHOUT BEING ADDED HERE, exactly as notice-disk-alert.sh was
# the day before, and for the same structural reason: this file's unanimity is
# only evaluated when a NEW hook script appears, so the debt sits invisible
# until the next author inherits it. On 2026-09-20 that was the §66
# reference-ledger guard, whose commit the completeness predicate refused
# fail-closed until this entry existed. Twice in two days is a pattern, not an
# accident: the acknowledgement belongs in the same commit as the registration.
# guard-reference-ledger.sh, ADDED 2026-09-20 — BLOCKING, PreToolUse[Agent]
# only. It refuses a spawn whose own BUILD SECTION names a surface the adoption
# ledger already answered, unless the prompt carries a live `reference:` line
# naming the ledger AND a section that EXISTS in it (or an argued
# `reference: none — <40+ characters>`, logged). ceo-decisions §66: Reed named
# the connection runtime and the outbox on 2026-09-18 and three phone briefs
# rebuilt them from scratch the next day, which is what "you ARE FUCKING
# REINVENTING THE FUCKING WHEEL FROM SCRATCH" is about. The SURFACES ARE DATA,
# in scripts/hooks/adoption-ledger.surfaces, so adding an area never edits the
# guard. Measured over 302 real briefs and spawn prompts: 11 refused, 9 of them
# build dispatches onto a ledger surface. Suite:
# guard-reference-ledger.test.sh (39 cases); harness:
# reference-ledger.mutation.sh.
# guard-public-record-repo.sh, ADDED 2026-09-20 — BLOCKING, on TWO surfaces:
# PreToolUse[Agent] in hooks.json and the Bash chain's manifest. It refuses a
# dispatch whose deliverable is a research read, a brief or a plan into a
# repository carrying a publication declaration, and a commit that ADDS a file
# under docs/research/, docs/briefs/ or docs/plans/ there. The CEO, 2026-09-20:
# "how many more times will the wrong shit be put into the Git history in the
# public repo????" — a read was dispatched with --repo richos and its ledger
# entered published history, which a later move does not undo. Its sibling
# publication guards scan what a file SAYS; this one is about CLASS, and
# docs/verification/ is deliberately out of scope. Measured over 302 real
# prompts and briefs: ONE refused, and it is the dispatch that caused the
# incident. Suite: guard-public-record-repo.test.sh (30 cases); harness:
# public-record-repo.mutation.sh.
# release-land-leases.sh, ADDED 2026-09-24 — Stop, NEVER BLOCKS. The operator
# back end's land lease ends at its holder's turn end once the land is at rest
# (clean, nothing in progress, pushed); otherwise it is kept and the hold is
# announced, naming the dirty paths (richos-hq spec r3 e6, Frank F3 and G5).
# Behind OPERATOR_FENCES: with the switch off no lease exists and it prints
# nothing. Suite: release-land-leases.test.sh; harness: release-land-leases.mutation.sh.
# guard-land-lease-commands.sh, ADDED 2026-09-24 — BLOCKING, a module of the Bash
# chain's manifest. The operator fence's early check: it refuses, before it runs,
# a cherry-pick, revert, am, rebase, stash, commit, reset or merge --abort aimed at
# a fenced main checkout by a session without that repository's land lease
# (richos-hq spec r3 e7, Frank G1; reset and merge --abort because the Git fence
# stops them only after the shared tree is rewritten, measured on both gits).
# Off or absent fence: never refuses. Suite: guard-land-lease-commands.test.sh;
# harness: land-lease-commands.mutation.sh.
# guard-foreign-app-data.sh, ADDED 2026-09-25 — BLOCKING, a module of the Bash
# chain's manifest. It refuses a command that reads another app's Containers or
# Group Containers folder, or walks the home folder, its Library, /Users or /
# deep enough to open one (find, du, ls -R, grep -r, rg). Inside a RichOS session
# such a command makes macOS ask the USER whether RichOS may access data from
# other apps; the CEO, 2026-09-25: "a regular user of RichOS is not expected to
# keep clicking those things". Measured over 166,915 distinct Bash commands from
# this Mac's transcripts: 113 refused. Suite: guard-foreign-app-data.test.sh;
# harness: foreign-app-data.mutation.sh.
# guard-shared-scratchpad.sh, ADDED 2026-10-01 — BLOCKING, a module of the Bash
# chain's manifest. A teammate (a payload carrying agent_id) may not rm, rmdir,
# unlink, trash or find -delete inside the lead session's directory under a Claude
# scratch root: the scratchpad the harness advertises to every in-process teammate
# as its own is shared, and on 2026-10-01 two teammates' end-of-task cleanups
# (rm -rf / rm -f <session>/scratchpad/*) destroyed the lead's briefs and a live
# teammate's files. The lead's calls pass. Suite: guard-shared-scratchpad.test.sh.
# guard-unguarded-rm.sh, ADDED 2026-10-02 — BLOCKING, a module of the Bash chain's manifest. It refuses
# an rm or rmdir (or find -delete) whose path starts with an unguarded variable, or names a critical
# path, which is Claude Code's own circuit breaker: it asked the CEO a Yes/No even with bypass
# permissions on. The refusal carries the exact rewrite. Suite: guard-unguarded-rm.test.sh.
# guard-host-sleep.sh, ADDED 2026-10-29 — BLOCKING, a module of the Bash chain's manifest. It refuses
# every caffeinate, every pmset write and the other sleep-assertion changes on the host (§65; an agent
# held caffeinate -i -w for seven minutes on 2026-10-29), and says what to do instead. Suite:
# guard-host-sleep.test.sh.
# guard-headed-browser.sh, ADDED 2026-10-29 — BLOCKING, a module of the Bash chain's manifest and
# registered on mcp__* tools. It keeps browser automation on the host headless: --headed, headless:
# false in the code a command runs, Playwright's window-only commands, a browser started without
# --headless and a browser MCP server configured without it (§65; a visible Chrome window reached the
# CEO's screen on 2026-10-28). Suite: guard-headed-browser.test.sh.
# guard-no-shell-c.sh, ADDED 2026-10-09 — BLOCKING, a module of the Bash chain's manifest. It refuses
# a command that wraps an inline script in bash -c, sh -c or zsh -c, which Claude Code cannot check and
# so asks the user ("This shell -c script ... could not be checked"). The refusal names the rewrite:
# separate commands, or a script file run with bash <file>. Suite: guard-no-shell-c.test.sh.
# operator-claim.sh, guard-operator-claim.sh, guard-shared-writes.sh,
# release-shared-writes.sh and guard-live-names.sh, ADDED 2026-09-25 — the engine
# side of several operator leads on one Mac (richos-hq spec r3 (e) the claim, e3,
# e5; r4 §2.4). operator-claim.sh (SessionStart, never blocks) writes his
# terminal's claim; guard-operator-claim.sh (BLOCKING: PreToolUse[Agent|
# SendMessage] and the Bash and Write chains; never TaskStop) refuses a terminal's team
# calls while the app's claim is live; guard-shared-writes.sh (BLOCKING, the Write
# chain) serializes memory writes and needs the land lease for a record write;
# release-shared-writes.sh (PostToolUse and PostToolUseFailure, never blocks)
# releases the memory lease; guard-live-names.sh (BLOCKING, PreToolUse[Agent])
# refuses a name a live agent of another session holds. All behind
# OPERATOR_FENCES: with the entity's launcher off they never start their checks
# and never refuse. Suite: operator-leads.test.sh; harness: operator-leads.mutation.sh.
# failure-type-lookup.sh and guard-failure-type-answer.sh, ADDED 2026-09-26 —
# the operator's own rule for his words "type" and "failure" close together.
# failure-type-lookup.sh (UserPromptSubmit, refuses nothing) hands the lead the
# failure register's live `Type N: heading` list and the rule;
# guard-failure-type-answer.sh (Stop, BLOCKING, does not stand down on the
# re-fire, carries an unpaid answer into the next turn) refuses the turn end
# until the register was read, a type from it was named with its heading, and
# the record was committed. The register is declared per entity
# (FAILURE_TYPE_REGISTER). Suite: failure-type.test.sh; harness:
# failure-type.mutation.sh.
# guard-resource-waits.sh, ADDED 2026-09-27 — the CEO's "A free Mac is used
# whole; nothing waits in a line", after three 90-minute VM waits were each
# answered "keep waiting". Stop, BLOCKING, does not stand down on the re-fire:
# the lead's turn does not end while any job has waited more than
# RESOURCE_WAIT_MINUTES (10) on a shared resource (the test VM, CPU admission,
# or a wait an escalation reports). No acknowledgement clears it; only the
# wait ending does. Suite: resource-waits.test.sh; harness:
# resource-waits.mutation.sh.
# guard-record-owner-memory.sh, ADDED 2026-09-28 — one writer at cut-over
# (daily-driver plan step 8; two-installs spec points 25-27). BLOCKING, the
# Write chain: once the record's owner line (ROW_RECORD_OWNER="app <dir>" in the
# record's .row-currency) says the RichOS app owns his record, his terminal is
# refused a write into the named memory directory. With no owner line, the state
# until cut-over, it refuses nothing and says nothing. Suite:
# scripts/record-owner.test.sh; harness: scripts/record-owner.mutation.sh.
# notice-unanswered-question.sh, ADDED 2026-10-01 — Stop, NEVER blocks. While a
# question the lead put to the CEO as a final message under `QUESTION FOR YOU:`
# (the route guard-ceo-ruled-ask.sh's deaf-lead check sends it, instead of an
# AskUserQuestion that held the lead deaf for 37 minutes that day) has no later
# human prompt, it repeats the question in the Stop systemMessage at every turn
# end, so it never scrolls away under the turns the lead is woken for (Sage's
# review, richos-hq 8ba32b71, item 9). Derived inventories, all carrying it:
# hooks/hooks.json, .claude/settings.local.json, this set, and the probe's
# BR_EXPECTED. Suite: blocking-ask.test.sh (Q01-Q04).
# guard-ceo-ask-first.sh, REMOVED 2026-10-24 — it refused nothing after orchestration
# reliability R3 (an unrelated prepared decision never vetoes a dispatch); the turn-end
# notice to the lead (notice-ceo-unasked.sh) carries the ask. Suite: ceo-asks.test.sh.
ACKNOWLEDGED_SCRIPTS="$(LC_ALL=C sort <<'ACK'
notice-unanswered-question.sh
guard-record-owner-memory.sh
failure-type-lookup.sh
guard-resource-waits.sh
guard-failure-type-answer.sh
guard-foreign-app-data.sh
guard-shared-scratchpad.sh
guard-unguarded-rm.sh
guard-host-sleep.sh
guard-headed-browser.sh
guard-no-shell-c.sh
operator-claim.sh
guard-operator-claim.sh
guard-shared-writes.sh
release-shared-writes.sh
guard-live-names.sh
guard-host-display-power.sh
guard-land-lease-commands.sh
guard-public-record-repo.sh
release-land-leases.sh
guard-reference-ledger.sh
guard-no-home-network-phone.sh
notice-disk-alert.sh
left-off-report.sh
guard-brief-scope.sh
guard-pierce.sh
deliver-review-verdict.sh
guard-hook-registration-commits.sh
commit-ceo-inputs.sh
handoff-facts-annotate.sh
notice-claim-capability.sh
notice-protected-ref-moves.sh
guard-ci-turn-gate.sh
detect-nonnative-worktree.sh
dispatch-pretooluse.sh
engine-status.sh
guard-agent-state-claims.sh
guard-bash-main-writes.sh
guard-ceo-ruled-ask.sh
guard-ceo-todos-commits.sh
guard-ci-red-lands.sh
guard-completeness-commits.sh
guard-definition-drift.sh
dialect-guard.sh
guard-idle-land.sh
guard-inflight-notify.sh
guard-interactive-prompt.sh
guard-main-checkout-writes.sh
guard-model-ceiling.sh
guard-named-persons-commands.sh
guard-named-persons-writes.sh
guard-owned-state.sh
guard-publication-commits.sh
guard-publication-writes.sh
guard-resume-isolation.sh
guard-row-currency-commits.sh
guard-sealed-worktree.sh
guard-stale-staging.sh
ass-kicker.sh
guard-stop-live-work.sh
guard-unresolved-claims.sh
guard-vendoring-commits.sh
guard-workflow-ban.sh
guard-workspace-gate.sh
guard-worktree-isolation.sh
guard-worktree-removal.sh
notice-ceo-asks.sh
notice-ceo-inputs-unheld.sh
notice-ceo-ruled-prose.sh
notice-ceo-unasked.sh
notice-escalations.sh
notice-hook-staleness.sh
notice-inflight-acks.sh
notice-inflight-sends.sh
notice-mechanical-findings.sh
notice-unasked-deferral.sh
notice-unlanded-branches.sh
notice-unstarted-rows.sh
notice-waiver-repetition.sh
observe-created-refs.sh
scan-secrets.sh
session-start-ceo-ask.sh
session-start-ci-surface.sh
session-start-escalations.sh
session-start-quota.sh
session-start-scratch.sh
session-start-stall.sh
shell-evidence.sh
strip-ack-lines.sh
snapshot-agent-definitions.sh
snapshot-enforcing-hooks.sh
task-completed-handoff.sh
teammate-idle-handoff.sh
turn-manifest.sh
verify-agent-prompt.sh
worker-created-handoff.sh
worker-ended-handoff.sh
worker-started-handoff.sh
worker-updated-handoff.sh
workspace-lifecycle.sh
ACK
)"
ACKNOWLEDGED_N="$(printf '%s\n' "$ACKNOWLEDGED_SCRIPTS" | grep -c .)"
NEWLY_REGISTERED="$(comm -13 \
    <(printf '%s\n' "$ACKNOWLEDGED_SCRIPTS") \
    <(printf '%s\n' "$REGISTERED_LIST") | tr '\n' ' ')"
NO_LONGER_REGISTERED="$(comm -23 \
    <(printf '%s\n' "$ACKNOWLEDGED_SCRIPTS") \
    <(printf '%s\n' "$REGISTERED_LIST") | tr '\n' ' ')"

if [ -z "${NEWLY_REGISTERED// /}${NO_LONGER_REGISTERED// /}" ]; then
    ok "1b  sanity: hooks.json registers exactly the $REGISTERED_N scripts acknowledged here (announcer included), so the banner reads ${EXPECT_N}/${EXPECT_N} guards"
else
    bad "1b  sanity" "the registration surface MOVED and nothing in this file acknowledged it. NEWLY REGISTERED: ${NEWLY_REGISTERED:-none}. NO LONGER REGISTERED: ${NO_LONGER_REGISTERED:-none}. hooks.json registers $REGISTERED_N scripts against $ACKNOWLEDGED_N acknowledged -- that scale COUNTS THE ANNOUNCER; the banner's ${EXPECT_N}/${EXPECT_N} does not, and is already right either way, so do not go looking for a wrong banner. The fix is to add or remove the named script in ACKNOWLEDGED_SCRIPTS in scripts/hooks/engine-status.test.sh and write one line above it saying what it is and why. This set is NOT the only inventory a registration has to be added to; the comment above ACKNOWLEDGED_SCRIPTS says how to derive the rest rather than listing them. Never silence this case."
fi

case "$OUT" in
    *ENFORCEMENT\ ACTIVE*) ok "1c  the adopted entity still gets ENFORCEMENT ACTIVE" ;;
    *) bad "1c  ENFORCEMENT ACTIVE" "not reported: ${OUT:0:200}" ;;
esac

# ===========================================================================
# 2. THE REGRESSION — a SEVENTEENTH guard, counted with no second edit.
#
# THIS IS THE CASE THE SUITE EXISTS FOR. Against the pre-fix hook — a typed
# list of 14 names — this sandbox announces "14/14 guards" no matter what is
# in hooks.json, which is exactly what femcboost was showing in production.
# ===========================================================================
wire_extra_guard
banner
expect_fraction "2a  one more wired guard is counted immediately: ${EXPECT_PLUS}/${EXPECT_PLUS}" "${EXPECT_PLUS}/${EXPECT_PLUS}"

case "$OUT" in
    *"?/?"*) bad "2b  the extra guard did not break the derivation" "inventory came back unknown" ;;
    *) ok "2b  the extra guard did not break the derivation" ;;
esac

# ===========================================================================
# 3. NEGATIVE CONTROL — prove case 2 can fail.
#
# A green tick means nothing if the assertion cannot go red. So the historical
# defect is rebuilt here on purpose: the derivation library is replaced by a
# HAND-TYPED inventory, the same 14 names the banner carried before this fix.
# The sandbox still has 17 scripts registered. If case 2's assertion is
# load-bearing, the banner must now be visibly, measurably wrong.
# ===========================================================================
cat >"$ENGINE/scripts/lib/registered-hooks.sh" <<'STALE'
#!/usr/bin/env bash
# MUTATION (test-only): the pre-fix banner inventory — a list a human typed.
registered_hook_scripts() {
    printf '%s\n' \
        guard-worktree-isolation.sh guard-definition-drift.sh guard-ceo-ask-first.sh \
        verify-agent-prompt.sh guard-main-checkout-writes.sh scan-secrets.sh \
        guard-resume-isolation.sh guard-bash-main-writes.sh guard-workflow-ban.sh \
        detect-nonnative-worktree.sh workspace-lifecycle.sh \
        snapshot-agent-definitions.sh teammate-idle-handoff.sh task-completed-handoff.sh
}
STALE
banner
if [ "$SYS_FRAC" = "${STALE_N}/${STALE_N} guards" ] && [ "$MODEL_FRAC" = "${STALE_N}/${STALE_N} guards" ]; then
    ok "3a  NEGATIVE CONTROL: a hand-typed inventory reports a full ${STALE_N}/${STALE_N} while ${EXPECT_PLUS} guards are wired — the historical defect, reproduced"
else
    bad "3a  NEGATIVE CONTROL" "expected the stale mutation to yield '${STALE_N}/${STALE_N} guards'; got operator='$SYS_FRAC' model='$MODEL_FRAC'. If this cannot be reproduced, case 2 is not proving anything."
fi
if [ "$SYS_FRAC" != "${EXPECT_PLUS}/${EXPECT_PLUS} guards" ]; then
    ok "3b  NEGATIVE CONTROL: case 2's assertion is load-bearing — it goes red under the mutation"
else
    bad "3b  NEGATIVE CONTROL" "the mutated banner still read ${EXPECT_PLUS}/${EXPECT_PLUS}, so case 2 passes for a reason unrelated to the derivation"
fi
restore

# ===========================================================================
# 4. THE TWO QUESTIONS MUST STAY SEPARATE.
#
# The denominator answers "what will the host load?", the numerator "which of
# those are actually on disk and executable?". A fix that collapsed them into
# one query would give a permanently full fraction that could never report a
# gap — which is exactly as useless as the stale list it replaced, and harder
# to notice. These cases construct real gaps.
# ===========================================================================
chmod -x "$ENGINE/scripts/hooks/scan-secrets.sh"
banner
expect_fraction "4a  SHORTFALL: a registered guard that is not executable drops the numerator only (${EXPECT_MINUS}/${EXPECT_N})" "${EXPECT_MINUS}/${EXPECT_N}"
# 4a2 (hunt part 3, 29): the shortfall is said in words beside "ENFORCEMENT ACTIVE".
if printf '%s' "$OUT" | grep -q 'enforcement is INCOMPLETE'; then
    ok "4a2 a shortfall says in words that enforcement is INCOMPLETE"
else
    bad "4a2 a shortfall says in words that enforcement is INCOMPLETE" "banner carries only the fraction"
fi
restore

mv "$ENGINE/scripts/hooks/scan-secrets.sh" "$SANDBOX/scan-secrets.parked"
banner
expect_fraction "4b  SHORTFALL: a registered guard missing from disk drops the numerator only (${EXPECT_MINUS}/${EXPECT_N})" "${EXPECT_MINUS}/${EXPECT_N}"
mv "$SANDBOX/scan-secrets.parked" "$ENGINE/scripts/hooks/scan-secrets.sh"
restore

# 4c — the mirror image. A script sitting on disk that nobody wired enforces
# nothing, and must not inflate the count. Deriving from a directory listing
# instead of from hooks.json would get this wrong (and would have hidden the
# original defect completely, since the file was always present).
#
# THE TARGET IS A DIRECTLY-REGISTERED SCRIPT, and that is not incidental. This
# case used to unwire scan-secrets.sh, which stopped being a hooks.json entry on
# 2026-09-15 when it became a rule of dispatch-pretooluse.sh — the mutation went
# on applying cleanly and changed nothing, and the case went red testing a
# surface its target no longer lives on. Case 4d below unwires a rule from the
# surface it DOES live on.
python3 - "$HOOKS_JSON" <<'PY'
import json, sys
p = sys.argv[1]
with open(p, encoding="utf-8") as fh:
    d = json.load(fh)
found = False
for entries in d["hooks"].values():
    for entry in entries:
        keep = [h for h in entry.get("hooks", [])
                if "guard-workflow-ban.sh" not in h.get("command", "")]
        if len(keep) != len(entry.get("hooks", [])):
            found = True
        entry["hooks"] = keep
if not found:
    sys.stderr.write("4c: guard-workflow-ban.sh is not registered in hooks.json\n")
    sys.exit(3)
with open(p, "w", encoding="utf-8") as fh:
    json.dump(d, fh, indent=2)
PY
banner
expect_fraction "4c  a guard UNWIRED from hooks.json leaves the count (present on disk, loads nothing): ${EXPECT_MINUS}/${EXPECT_MINUS}" "${EXPECT_MINUS}/${EXPECT_MINUS}"
restore

# 4d — the same property, on the other registration surface. A rule deleted from
# scripts/hooks/dispatch-pretooluse.manifest stops running exactly as completely
# as one deleted from hooks.json, and the fraction has to say so. Without this
# case the manifest would be a place a guard can be switched off while the banner
# goes on reporting a full, reassuring fraction — which is the 14/14 defect this
# whole file was written about, rebuilt on a new surface.
#
# `restore` puts hooks.json back and knows nothing about the manifest, so this
# case restores its own file. It cost cases 6a and 6b a full hour before that
# was noticed: they read 70/70 against an expected 71/71, and the missing guard
# was the one THIS case had removed and never put back — a mutation leaking
# forward into every later case, which is the same class of defect as the
# silently-skipped rule the whole change is about.
MANIFEST_FILE="$ENGINE/scripts/hooks/dispatch-pretooluse.manifest"
if [ -f "$MANIFEST_FILE" ] && grep -q '^Write|scan-secrets\.sh$' "$MANIFEST_FILE"; then
    cp "$MANIFEST_FILE" "$SANDBOX/manifest.pristine"
    grep -v '^Write|scan-secrets\.sh$' "$MANIFEST_FILE" > "$SANDBOX/manifest.cut"
    cp "$SANDBOX/manifest.cut" "$MANIFEST_FILE"
    banner
    expect_fraction "4d  a rule UNWIRED from the dispatcher manifest leaves the count (${EXPECT_MINUS}/${EXPECT_MINUS})" "${EXPECT_MINUS}/${EXPECT_MINUS}"
    cp "$SANDBOX/manifest.pristine" "$MANIFEST_FILE"
    restore
    # The restore is asserted, not assumed: a case that leaves the sandbox
    # changed makes every case after it test something nobody wrote.
    if grep -q '^Write|scan-secrets\.sh$' "$MANIFEST_FILE"; then
        ok "4d2 the manifest mutation is put back, so later cases see the shipped inventory"
    else
        bad "4d2 the manifest mutation is put back" "Write|scan-secrets.sh is still missing from $MANIFEST_FILE"
    fi
else
    bad "4d  a rule UNWIRED from the dispatcher manifest leaves the count" "Write|scan-secrets.sh is not in $MANIFEST_FILE — the case cannot run, which is not a pass"
fi

# ===========================================================================
# 5. NO REASSURING NUMBER WHEN THE INVENTORY CANNOT BE READ.
#
# Fail loud, never skip. If hooks.json is gone or malformed the honest answer
# is "unknown", said out loud on both channels — never a plausible integer,
# and never a silently scraped one.
# ===========================================================================
mv "$HOOKS_JSON" "$SANDBOX/hooks.json.parked"
banner
if [ "$SYS_FRAC" = "?/? guards" ] && [ "$MODEL_FRAC" = "?/? guards" ]; then
    ok "5a  a missing hooks.json yields '?/?', not a number"
else
    bad "5a  missing hooks.json" "operator='$SYS_FRAC' model='$MODEL_FRAC'"
fi
case "$OUT" in
    *"could NOT be derived"*) ok "5b  and says so, loudly, in the announcement" ;;
    *) bad "5b  missing hooks.json warning" "no warning in: ${OUT:0:300}" ;;
esac
mv "$SANDBOX/hooks.json.parked" "$HOOKS_JSON"

printf '{ this is not json\n' >"$HOOKS_JSON"
banner
if [ "$SYS_FRAC" = "?/? guards" ]; then
    ok "5c  a MALFORMED hooks.json yields '?/?' too — no count scraped out of a file the host cannot load either"
else
    bad "5c  malformed hooks.json" "operator='$SYS_FRAC'"
fi
restore

# A SessionStart hook must never block, whatever it finds.
mv "$HOOKS_JSON" "$SANDBOX/hooks.json.parked"
( cd "$SANDBOX" && env -u CLAUDE_PROJECT_DIR -u CLAUDE_PLUGIN_ROOT -u RICHOS_ENGINE_ROOT \
    "RICHOS_ENTITY_ROOT=$ENTITY" bash "$ENGINE/scripts/hooks/engine-status.sh" </dev/null ) >/dev/null 2>&1
RC=$?
if [ "$RC" -eq 0 ]; then
    ok "5d  still exits 0 — a broken inventory is announced, never blocking"
else
    bad "5d  exit code" "rc=$RC"
fi
mv "$SANDBOX/hooks.json.parked" "$HOOKS_JSON"
restore

# ===========================================================================
# 6. THE python3-ABSENT PATH.
#
# emit_context already carries a hand-escaping fallback, so "no interpreter" is
# a supported environment for this hook — which means the derivation has to
# survive it too. A fallback path is exactly where a quietly-dropped field, or
# a quietly-different answer, would never be noticed.
# ===========================================================================
NOPY="$SANDBOX/nopy-bin"
mkdir -p "$NOPY"
for d in /bin /usr/bin /usr/local/bin /opt/homebrew/bin; do
    [ -d "$d" ] || continue
    for f in "$d"/*; do
        b="${f##*/}"
        case "$b" in python|python3|python3.*) continue ;; esac
        [ -e "$NOPY/$b" ] || ln -s "$f" "$NOPY/$b" 2>/dev/null
    done
done
# The check runs in a CHILD shell on purpose. `command -v` in THIS shell would
# consult bash's command hash table, which already holds /opt/homebrew/bin/
# python3 from the case-2 setup above, and would report python3 "found" on a
# PATH that does not contain it — declaring the sandbox unbuildable while it was
# in fact correct. A fresh bash starts with an empty hash table.
if [ -x "$NOPY/bash" ] && ! PATH="$NOPY" "$NOPY/bash" -c 'command -v python3' >/dev/null 2>&1; then
    banner "PATH=$NOPY"
    expect_fraction "6a  without python3 the fraction is unchanged: ${EXPECT_N}/${EXPECT_N}" \
        "${EXPECT_N}/${EXPECT_N}"
    wire_extra_guard
    banner "PATH=$NOPY"
    expect_fraction "6b  without python3 one more wired guard is still counted: ${EXPECT_PLUS}/${EXPECT_PLUS}" "${EXPECT_PLUS}/${EXPECT_PLUS}"
    restore
else
    bad "6   python3-absent sandbox" "could not build a python3-free PATH; the fallback went untested"
fi

# ===========================================================================
# 7. THE SPAWN INSTRUCTION NAMES THE ONE COMMAND — AND NAMES IT FIRST.
#
# This banner is the first instruction every session reads, and on 2026-09-13
# it was the reason `scripts/spawn.sh` went unused the night after it landed:
# the string named the four-step path and had never heard of the one-command
# one. A tool nobody is told to use is a tool that does not exist, and the cost
# was measured in front of the CEO — 130 s and two guard refusals that spawn.sh
# would have reported together, before a workspace existed.
#
# So this section asserts PRIMACY, not mere presence. "prepare-agent-spawn.py
# ... or, if you like, spawn.sh" would satisfy a naive grep and would reproduce
# the exact defect, so the old path must appear AFTER the new one and be marked
# as the fallback it is. The facts the old string carried and that are still
# true are asserted too — dropping one while rewriting is the quiet way this
# announcement gets worse.
#
# It asserts nothing about what any guard ACCEPTS. The four-step path still
# works; this is a check on what we TELL people.
# ===========================================================================

# spawn_instruction_problems — reads the MODEL channel of the current $OUT and
# echoes every problem it finds, one per line; silence means correct. A verdict
# function rather than inline asserts, so case 7z can prove it can fail.
spawn_instruction_problems() {
    local model="${OUT#*\"hookSpecificOutput\"}"
    local i_new i_old
    case "$model" in
        *"scripts/spawn.sh"*) : ;;
        *) echo "the announcement never names scripts/spawn.sh" ;;
    esac
    # Length arithmetic on the prefixes, on purpose: presence is not primacy.
    i_new="${model%%scripts/spawn.sh*}"
    i_old="${model%%prepare-agent-spawn.py*}"
    case "$model" in
        *"scripts/spawn.sh"*"prepare-agent-spawn.py"*) : ;;
        *"prepare-agent-spawn.py"*)
            echo "prepare-agent-spawn.py is named ahead of scripts/spawn.sh (${#i_old} < ${#i_new}) — the old path reads as the instruction" ;;
    esac
    case "$model" in
        *FALLBACK*) : ;;
        *"prepare-agent-spawn.py"*) echo "the four-step path is named without being marked FALLBACK" ;;
    esac
    local fact
    for fact in \
        "--repo <repo> --type <subagent-type> --brief <file>" \
        "EVERY PreToolUse[Agent] guard from EVERY surface" \
        "reports ALL failures together" \
        "leaves nothing behind" \
        "isolation:worktree" \
        "a cwd-only spawn is refused" \
        "inflight acknowledgement contract" \
        "grants any guard exemption" \
        "engine paths, not scripts inside the governed repository"
    do
        case "$model" in
            *"$fact"*) : ;;
            *) echo "dropped a fact the instruction must carry: \"$fact\"" ;;
        esac
    done
}

banner
PROBLEMS="$(spawn_instruction_problems)"
if [ -z "$PROBLEMS" ]; then
    ok "7a  the spawn instruction names scripts/spawn.sh FIRST, marks the four-step path as the fallback, and keeps every fact the old string carried"
else
    bad "7a  spawn instruction" "$(printf '%s' "$PROBLEMS" | tr '\n' ';')"
fi

# 7z — NEGATIVE CONTROL. Reconstruct the 2026-09-13 defect in the sandbox
# engine (an announcement naming only the old path) and prove 7a's verdict
# function goes red. Without this, 7a is a green tick over a check that might
# be incapable of failing — the wall-of-green problem cases 3 and 4 exist to
# kill.
cp "$ENGINE/scripts/hooks/engine-status.sh" "$SANDBOX/engine-status.sh.pristine"
python3 - "$ENGINE/scripts/hooks/engine-status.sh" <<'MUT'
import io, sys
p = sys.argv[1]
s = io.open(p, encoding="utf-8").read()
i = s.index("    governed)")
head, tail = s[:i], s[i:]
tail = tail.replace("STARTING ONE TEAMMATE IS ONE COMMAND:",
                    "Before each new Agent call, prepare its task-specific JSON with python3", 1)
tail = tail.replace("'${ENGINE_ROOT}/scripts/spawn.sh'",
                    "'${ENGINE_ROOT}/scripts/prepare-agent-spawn.py'", 1)
tail = tail.replace("FALLBACK ONLY", "Also", 1)
io.open(p, "w", encoding="utf-8").write(head + tail)
MUT
banner
PROBLEMS="$(spawn_instruction_problems)"
case "$PROBLEMS" in
    *"never names scripts/spawn.sh"*|*"named ahead of"*|*"without being marked FALLBACK"*)
        ok "7z  NEGATIVE CONTROL: an announcement naming only the old path is caught (these assertions can fail)" ;;
    *)
        bad "7z  negative control" "the historical defect was reported as: '${PROBLEMS:-no problems at all}'" ;;
esac
cp "$SANDBOX/engine-status.sh.pristine" "$ENGINE/scripts/hooks/engine-status.sh"
restore

# 8 — THE ROOT-FAILURE BANNER PROMISES NOTHING THE ENGINE DOES NOT DO (hunt part
# 3 v3, finding 29). Advisory notices return 0 on a root failure, so "every
# guard in this session will refuse" is false; the banner must say which do.
banner "RICHOS_ENTITY_ROOT=$SANDBOX/no-such-repository"
case "$OUT" in
    *"ROOT RESOLUTION FAILURE"*)
        if printf '%s' "$OUT" | grep -q 'Every guard in this session will refuse'; then
            bad "8a  the root-failure banner does not claim every guard refuses" "it still does"
        elif printf '%s' "$OUT" | grep -q 'advisory notices'; then
            ok "8a  the root-failure banner says blocking guards refuse and advisory notices only report"
        else
            bad "8a  the root-failure banner names the advisory notices" "$(printf '%s' "$OUT" | cut -c1-200)"
        fi ;;
    *)  bad "8a  the root-failure banner (fixture)" "no root failure reached: $(printf '%s' "$OUT" | cut -c1-200)" ;;
esac

echo ""
if [ "$FAIL" -eq 0 ]; then
    printf 'engine-status: %d/%d cases pass\n' "$PASS" "$PASS"
    exit 0
fi
printf 'engine-status: %d passed, %d FAILED\n' "$PASS" "$FAIL"
for n in "${FAIL_NAMES[@]}"; do printf '  - %s\n' "$n"; done
exit 1

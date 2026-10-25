#!/usr/bin/env bash
#
# inflight-notify.test.sh — the in-flight sweep, end to end, in a sandbox.
#
# Builds a real git repository with a real linked worktree, moves main under it,
# and drives the SHIPPED hooks with synthetic payloads. Nothing is stubbed
# except the session team directory (INFLIGHT_TEAMS_DIR), because that is the
# one thing a test must not share with the live session.
#
# THE FOUR TRANSCRIPTS THE BRIEF ASKS FOR, and where each is asserted:
#   1. a land with a live teammate behind and un-notified -> BLOCKED, named
#        cases 2a/2b
#   2. the same land after the notification is recorded    -> ALLOWED
#        cases 4a/4b
#   3. an ack absent -> detected; an ack present -> verified from durable state
#        cases 5a-5f
#   4. no live worktrees -> silent no-op, PROVEN BY A POSITIVE PROBE so the
#      guard cannot pass by never running at all
#        cases 6a/6b/6c
#
# Run directly:  scripts/hooks/inflight-notify.test.sh [--verbose]

set -uo pipefail

VERBOSE=0
[ "${1:-}" = "--verbose" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

# IN2, and the sandbox path is deliberately reached THROUGH A SYMLINK.
#
# Case 5j proves that the notice predicate resolves paths with realpath rather
# than abspath. That property is only load-bearing when the path it is given
# actually passes through a symlink — and on 2026-09-05 `zach-opus-rd1` proved
# with a four-arm controlled experiment that it was not being exercised: with a
# REAL sandbox path the abspath-not-realpath mutant SURVIVES; with a symlinked
# one it is killed at 5j; both unmutated controls are green on both shapes, so
# the difference is the mutation and not the path.
#
# It passed on this Mac only because `/var/folders` happens to be reached
# through `/private/var` — an accident of the host, not a property of the test.
# On a Linux runner where the sandbox path is real, IN2 went green over a check
# that could not fail. Forcing the symlink makes it load-bearing on EVERY host.
#
# Do not "simplify" this back to a bare mktemp, and do not try to arrange it via
# TMPDIR: macOS `mktemp -d -t` ignores TMPDIR outright, which is how rd1's own
# first reproduction came out invalid and was discarded.
SANDBOX_REAL="$(mktemp -d -t inflight-notify.XXXXXX)"
SANDBOX="${SANDBOX_REAL}.link"
ln -s "$SANDBOX_REAL" "$SANDBOX"
trap 'rm -f "$SANDBOX"; rm -rf "$SANDBOX_REAL"' EXIT

REPO="$SANDBOX/repo"
TEAMS="$SANDBOX/teams"
TEAM_DIR="$TEAMS/session-deadbeef"
mkdir -p "$REPO" "$TEAM_DIR"

# --- the sandbox engine copy ----------------------------------------------
# The hooks resolve their libraries relative to their own location, so the
# sandbox hosts both. A copy, not a symlink: a mutation test rewrites one.
mkdir -p "$REPO/scripts/hooks" "$REPO/scripts/lib"
for h in guard-inflight-notify.sh notice-inflight-sends.sh notice-inflight-acks.sh; do
    cp "$SRC_DIR/$h" "$REPO/scripts/hooks/$h"
    chmod +x "$REPO/scripts/hooks/$h"
done
# git-jurisdiction.sh is on this list for the same reason seat-jurisdiction.sh
# is: the guard REFUSES TO START without it. A sandbox missing it would model an
# engine that cannot run, and every case here would be reading a BROKEN INSTALL
# banner rather than a decision.
for l in inflight.sh inflight.py teammate-identity.py agent-liveness.py resolve-roots.sh resolve-main-checkout.sh seat-jurisdiction.sh git-jurisdiction.sh stop-hook-notice.sh; do
    cp "$SRC_DIR/../lib/$l" "$REPO/scripts/lib/$l" 2>/dev/null || true
done
cp "$ENGINE_ROOT/scripts/inflight-notify.sh" "$REPO/scripts/"
cp "$ENGINE_ROOT/scripts/inflight-ack.sh" "$REPO/scripts/"
chmod +x "$REPO/scripts/inflight-notify.sh" "$REPO/scripts/inflight-ack.sh"
printf 'PROTECTED_PATHS="src"\n' > "$REPO/orchestration.config"

GUARD="$REPO/scripts/hooks/guard-inflight-notify.sh"
WITNESS="$REPO/scripts/hooks/notice-inflight-sends.sh"
STOPHOOK="$REPO/scripts/hooks/notice-inflight-acks.sh"
RUNNER="$REPO/scripts/inflight-notify.sh"
ACKER="$REPO/scripts/inflight-ack.sh"

export RICHOS_ENTITY_ROOT="$REPO"
export INFLIGHT_TEAMS_DIR="$TEAMS"
# --- THE DURABLE ACK LEDGER, REDIRECTED INTO THE SANDBOX -------------------
# An ack is now written to TWO places: the readable file in the worktree, and
# an append-only row in ~/.claude/state/inflight-acks.jsonl, which is the one
# that survives the worktree being removed (row 3.19). Without this redirect
# every ACKER call below would append to the OPERATOR'S REAL LEDGER — a suite
# that sandboxes one output while a second still follows the home directory.
export RICHOS_INFLIGHT_ACK_LEDGER="$SANDBOX/state/inflight-acks.jsonl"
mkdir -p "$SANDBOX/state"
# An empty ledger, for the cases below that need to model a teammate which has
# NOT acked. Deleting the ack file no longer achieves that on its own, and that
# is the fix rather than a side effect: an ack that was written and then had its
# file deleted is still an ack, which is the whole of row 3.19.
NO_ACK_LEDGER="$SANDBOX/state/no-durable-acks.jsonl"
: > "$NO_ACK_LEDGER"

# --- a real repository, a real worktree -----------------------------------
git -C "$REPO" init -q -b main
# Inherit the operator's identity rather than inventing one: this machine runs a
# global pre-commit identity guard, and a sandbox that hard-codes an address
# fails on the guard instead of on the thing under test.
git -C "$REPO" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$REPO" config user.name "$(git config user.name 2>/dev/null || echo tester)"
mkdir -p "$REPO/src"
echo "one" > "$REPO/src/a.txt"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m "base"
BASE_SHA="$(git -C "$REPO" rev-parse HEAD)"

WT="$SANDBOX/wt/norm-sonnet-feature1"
mkdir -p "$SANDBOX/wt"
git -C "$REPO" worktree add -q -b worktree-norm "$WT" >/dev/null 2>&1
echo "teammate work" > "$WT/src/b.txt"
git -C "$WT" add -A
git -C "$WT" commit -q -m "teammate commit"

# main moves under the teammate — the whole failure, reproduced.
echo "two" >> "$REPO/src/a.txt"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m "a land that moves main"
TIP="$(git -C "$REPO" rev-parse HEAD)"

# --- payload builders ------------------------------------------------------
push_payload() { # [cwd]
    CWD="${1:-$REPO}" python3 -c '
import json, os
print(json.dumps({"tool_name": "Bash", "cwd": os.environ["CWD"],
                  "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "tool_input": {"command": "git push origin main"}}))'
}
send_payload() { # <to> <body> [agent_id]
    TO="$1" BODY="$2" AID="${3:-}" python3 -c '
import json, os
d = {"tool_name": "SendMessage", "hook_event_name": "PostToolUse",
     "session_id": "deadbeef-1111-4000-8000-000000000000",
     "cwd": os.environ.get("CWD", ""),
     "tool_input": {"to": os.environ["TO"], "message": os.environ["BODY"],
                    "summary": "in-flight notice"}}
if os.environ.get("AID"):
    d["agent_id"] = os.environ["AID"]
print(json.dumps(d))'
}
stop_payload() {
    python3 -c '
import json
print(json.dumps({"hook_event_name": "Stop", "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "cwd": "", "stop_hook_active": False, "last_assistant_message": "Done.",
                  "background_tasks": [], "session_crons": []}))'
}

# Since 2026-10-06 the push guard never refuses (case 2a). The debt predicate it used
# to enforce is still computed -- the runner's `check` reads it -- and every case
# below asks THAT, with exit 1 (debt) mapped to GRC=2 as before.
run_guard() { # -> sets GRC / GOUT
    GOUT="$(bash "$RUNNER" check --repo "${1:-$REPO}" --session deadbeef-1111-4000-8000-000000000000 2>&1)"
    GRC=$?
    [ "$GRC" -eq 1 ] && GRC=2
    return 0
}

echo "=== in-flight sweep: guard, witness, ack, and the silent case ==="

# ==========================================================================
# 1. the sweep sees the world
# ==========================================================================
SOUT="$(bash "$RUNNER" status --repo "$REPO" 2>&1)"
say "1a status" "$SOUT"
case "$SOUT" in
    *"OWED-NO-NOTICE"*) ok "1a. the runner names the live teammate that main moved under" ;;
    *) bad "1a. the runner names the live teammate" "$SOUT" ;;
esac
case "$SOUT" in
    *"OVERLAPS"*) bad "1b. no false overlap is reported (the teammate touched a different file)" "$SOUT" ;;
    *) ok "1b. no false overlap — the teammate changed src/b.txt, the land changed src/a.txt" ;;
esac

# ==========================================================================
# 2. TRANSCRIPT ONE — a land with a live teammate behind and un-notified
# ==========================================================================
run_guard
say "2 predicate owes a notice" "$GOUT"
[ "$GRC" -eq 2 ] && ok "2a0. the predicate still sees the debt (exit 1 from check)" \
                 || bad "2a0. the predicate sees the debt" "exit $GRC: $GOUT"
# 2a. THE CHANGE (CEO 2026-10-06): a push needs no notice and no waiver. The same
# push, the same live un-notified teammate, and the guard lets it through
# silently. It still leaves its footprint (6c), so it ran.
GOUT="$(push_payload "$REPO" | bash "$GUARD" 2>&1)"; GRC=$?
[ "$GRC" -eq 0 ] && ok "2a. the push is ALLOWED with a live un-notified teammate (exit 0)" \
                 || bad "2a. the push is allowed with a live un-notified teammate" "exit $GRC: $GOUT"
case "$GOUT" in
    *"REFUSING"*|*"OWED-NO-NOTICE"*) bad "2b. the guard prints no refusal" "$GOUT" ;;
    *) ok "2b. the guard prints no refusal and demands no notice or waiver" ;;
esac
case "$GOUT" in
    *"inflight-notify.sh waive"*|*"inflight-notify.sh notice"*) bad "2d. the guard points at no notice generator or waiver" "$GOUT" ;;
    *) ok "2d. the guard points at no notice generator or waiver" ;;
esac
run_guard

# ==========================================================================
# 2f-2k. THE GENERATOR ITSELF
# ==========================================================================
# `notice` emits a ready-to-send body per owed teammate. sha, paths and
# teammate come from the SAME predicate the guard refuses on; impact and detail
# are the only two operator fields. THE POINT IS THAT THERE IS NO BLANK SPACE
# TO INFLATE, so the shape is asserted as well as the content.
NOUT="$(bash "$RUNNER" notice --repo "$REPO" 2>&1)"; NRC=$?
say "2f notice" "$NOUT"
[ "$NRC" -eq 0 ] && ok "2f. the generator runs clean when a notice is owed" \
                 || bad "2f. the generator runs clean" "exit $NRC: $NOUT"
case "$NOUT" in
    *"main moved to $TIP"*) ok "2g. the generated body carries the FULL tip sha, pre-filled" ;;
    *) bad "2g. the generated body carries the tip sha" "$NOUT" ;;
esac
# ADDRESSED to the EXACT resolved name, not merely mentioning it somewhere: a
# notice addressed to a directory the witness cannot join to a teammate
# discharges nothing, and the generator must say so out loud when the join fails.
if printf '%s' "$NOUT" | grep -q 'SendMessage to : norm-sonnet-feature1' \
   && ! printf '%s' "$NOUT" | grep -q 'NO EXACT NAME JOIN'; then
    ok "2h. the generated notice is ADDRESSED to the resolved teammate, pre-filled"
else
    bad "2h. the generated notice is addressed to the resolved teammate" "$NOUT"
fi
case "$NOUT" in
    *"paths:"*) ok "2i. the generated body carries a paths: line, pre-filled from the predicate" ;;
    *) bad "2i. the generated body carries paths:" "$NOUT" ;;
esac
# EXACTLY TWO FIELDS ARE THE OPERATOR'S. Not "few". Two.
FILLS="$(printf '%s\n' "$NOUT" | grep -c '<FILL:')"
[ "$FILLS" -eq 2 ] && ok "2j. EXACTLY two fields are left for the operator (impact, detail)" \
                   || bad "2j. exactly two operator fields" "found $FILLS <FILL: markers"
# ...and they are those two, by name.
if printf '%s' "$NOUT" | grep -q '  impact: <FILL:' && printf '%s' "$NOUT" | grep -q '  detail: <FILL:'; then
    ok "2k. and they are impact and detail — sha, paths and teammate are never blank"
else
    bad "2k. the two operator fields are impact and detail" "$NOUT"
fi
# Supplying them bakes them in, and nothing is left to fill.
NOUT2="$(bash "$RUNNER" notice --repo "$REPO" --impact stale-record \
    --detail "the seven variations you were given became nineteen; size your library for all of them" 2>&1)"
say "2l notice filled" "$NOUT2"
if printf '%s' "$NOUT2" | grep -q '  impact: stale-record' \
   && printf '%s' "$NOUT2" | grep -q 'the seven variations you were given became nineteen' \
   && ! printf '%s' "$NOUT2" | grep -q '<FILL:'; then
    ok "2l. --impact/--detail bake in, and nothing is left to fill"
else
    bad "2l. --impact/--detail bake in" "$NOUT2"
fi
# THE BODY IS FIVE LINES. That is the whole mechanism: a generated notice with
# two operator fields cannot become twenty-five.
BODY_LINES="$(printf '%s\n' "$NOUT2" | awk '/--------------------------- message body/{f=1;next} /^  ------------------------------------/{f=0} f' | wc -l | tr -d ' ')"
[ "$BODY_LINES" -eq 5 ] && ok "2m. the generated body is EXACTLY 5 lines" \
                        || bad "2m. the generated body is exactly 5 lines" "got $BODY_LINES lines"
# The two operator fields are VALIDATED, not merely requested.
bash "$RUNNER" notice --repo "$REPO" --impact "sort of" >/dev/null 2>&1
[ $? -eq 2 ] && ok "2n. an --impact outside the four kinds is refused" \
             || bad "2n. an --impact outside the four kinds is refused" "it was accepted"
bash "$RUNNER" notice --repo "$REPO" --detail "$(printf 'line one\nline two')" >/dev/null 2>&1
[ $? -eq 2 ] && ok "2o. a MULTI-LINE --detail is refused — that is how five fields became twenty-five" \
             || bad "2o. a multi-line --detail is refused" "it was accepted"
# AND IT DOES NOT SEND. The generator must leave the debt exactly where it was:
# if running it cleared anything, the send would have stopped being the record.
run_guard
[ "$GRC" -eq 2 ] && ok "2p. GENERATING a notice does not SEND it — the debt is untouched" \
                 || bad "2p. generating a notice does not send it" "exit $GRC: $GOUT"

# ==========================================================================
# 3. what does NOT satisfy it
# ==========================================================================
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "main moved, please rebase")" | bash "$WITNESS"
run_guard
[ "$GRC" -eq 2 ] && ok "3a. a message with NO SHA in it does not clear the debt" \
                 || bad "3a. a message with no SHA does not clear the debt" "exit $GRC"

printf '%s' "$(CWD="$REPO" send_payload "someone-else-entirely" "main moved to $TIP")" | bash "$WITNESS"
run_guard
[ "$GRC" -eq 2 ] && ok "3b. a message to a DIFFERENT teammate does not clear the debt" \
                 || bad "3b. a message to someone else does not clear the debt" "exit $GRC"

printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "main moved to $BASE_SHA")" | bash "$WITNESS"
run_guard
[ "$GRC" -eq 2 ] && ok "3c. a message naming the OLD sha does not clear the debt" \
                 || bad "3c. a message naming the old sha does not clear the debt" "exit $GRC"

printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "main moved to $TIP" "aworker-1234")" | bash "$WITNESS"
run_guard
[ "$GRC" -eq 2 ] && ok "3d. a WORKER's message is not the lead's notice (attribution gate holds)" \
                 || bad "3d. a worker's message is not the lead's notice" "exit $GRC"

# ==========================================================================
# 4. TRANSCRIPT TWO — the same land, after the notification is recorded
# ==========================================================================
NOTICE_BODY="Main moved to $TIP while you were working. It changed src/a.txt.
Acknowledge with: scripts/inflight-ack.sh --sha $TIP --impact <kind> --detail \"...\" --paths \"...\""
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "$NOTICE_BODY")" | bash "$WITNESS"
run_guard
say "4 guard ALLOWED" "$GOUT"
[ "$GRC" -eq 0 ] && ok "4a. once the notice naming this tip is witnessed, the push is ALLOWED" \
                 || bad "4a. the push is allowed after the notice" "exit $GRC: $GOUT"
GOUT="$(push_payload "$REPO" | bash "$GUARD" 2>&1)"
[ -z "$GOUT" ] && ok "4b. and it is allowed SILENTLY — a guard that talks on success gets muted" \
               || bad "4b. allowed silently" "$GOUT"

# THE WITNESS IS NOT A SELF-REPORT: it is the hook, on the send, not a file the
# lander chose to write. Prove the ledger came from the hook and carries the sha.
LEDGER="$TEAM_DIR/inflight-notices.jsonl"
if [ -s "$LEDGER" ] && grep -q '"source_hook": "PostToolUse\[SendMessage\]"' "$LEDGER" \
   && grep -q "$TIP" "$LEDGER"; then
    ok "4c. the record was written by the SEND itself, and carries the tip"
else
    bad "4c. the record was written by the send itself" "$(cat "$LEDGER" 2>/dev/null | tail -2)"
fi
if grep -q "Acknowledge with" "$LEDGER"; then
    bad "4d. the message BODY is never logged" "the body leaked into the ledger"
else
    ok "4d. the message BODY is never logged — only the recipient, the shas, a digest and a length"
fi

# ==========================================================================
# 5. TRANSCRIPT THREE — the ack: absent, malformed, then verified
# ==========================================================================
AOUT="$(bash "$RUNNER" status --repo "$REPO" 2>&1)"
say "5a ack absent" "$AOUT"
case "$AOUT" in
    *"NOTIFIED-NO-ACK"*) ok "5a. an ABSENT ack is detected and reported, not assumed" ;;
    *) bad "5a. an absent ack is detected" "$AOUT" ;;
esac

# The teammate writes a bad one: the shape check is real.
bash "$ACKER" --sha "$BASE_SHA" --impact conflict --detail "x" --paths none --worktree "$WT" >/dev/null 2>&1
[ $? -ne 0 ] && ok "5b. the ack writer REFUSES a too-short detail rather than writing a hollow ack" \
             || bad "5b. the ack writer refuses a too-short detail"
bash "$ACKER" --sha "abc123" --impact conflict --detail "$(printf 'x%.0s' $(seq 1 50))" --paths none --worktree "$WT" >/dev/null 2>&1
[ $? -ne 0 ] && ok "5c. the ack writer REFUSES a short sha — it has to be checkable against the tip" \
             || bad "5c. the ack writer refuses a short sha"

# A well-formed ack for the WRONG commit must not count.
bash "$ACKER" --sha "$BASE_SHA" --impact none \
    --detail "I read the notice and nothing in it affects the file I am editing." \
    --paths none --worktree "$WT" >/dev/null 2>&1
AOUT="$(bash "$RUNNER" status --repo "$REPO" 2>&1)"
case "$AOUT" in
    *"NOTIFIED-NO-ACK"*) ok "5d. an ack naming a DIFFERENT commit is not an ack for this one" ;;
    *) bad "5d. an ack for a different commit does not count" "$AOUT" ;;
esac

# A path that exists nowhere is caught.
bash "$ACKER" --sha "$TIP" --impact stale-record \
    --detail "My copy of the plan predates this land, so the count I was given is wrong." \
    --paths "src/does-not-exist.txt" --worktree "$WT" >/dev/null 2>&1
AOUT="$(bash "$RUNNER" acks --repo "$REPO" 2>&1)"
say "5e invalid ack" "$AOUT"
case "$AOUT" in
    *"verified : NO"*) ok "5e. an ack citing a path in neither the changeset nor the worktree is INVALID" ;;
    *) bad "5e. an ack citing an impossible path is invalid" "$AOUT" ;;
esac

# The real thing.
bash "$ACKER" --sha "$TIP" --impact stale-record \
    --detail "My copy of src/a.txt is one revision behind; I will re-read it before I touch it." \
    --paths "src/a.txt src/b.txt" --worktree "$WT" >/dev/null 2>&1
AOUT="$(bash "$RUNNER" acks --repo "$REPO" 2>&1)"
say "5f verified ack" "$AOUT"
case "$AOUT" in
    *"verified : YES"*) ok "5f. a well-formed ack VERIFIES — from a file on disk, with no message received" ;;
    *) bad "5f. a well-formed ack verifies" "$AOUT" ;;
esac
case "$AOUT" in
    *"HUMAN JUDGMENT REQUIRED"*) ok "5g. and the verifier SAYS which part of it no machine here checked" ;;
    *) bad "5g. the verifier names the unmachine-checkable part" "$AOUT" ;;
esac
SOUT="$(bash "$RUNNER" status --repo "$REPO" 2>&1)"
case "$SOUT" in
    *"NOTIFIED-ACKED"*) ok "5h. the sweep reports the teammate as told AND proved" ;;
    *) bad "5h. the sweep reports told and proved" "$SOUT" ;;
esac

# The filename carries the sha, so a wrong-commit ack lands under a different
# name and is simply absent (5d). This is the other half: the RIGHT filename
# with a STALE sha INSIDE, which is what copying an old ack forward produces.
#
# Found by GLOB, not by construction: the name is `<sha12>.<teammate>.ack` and
# this suite must not become the second place that knows how to spell it.
ACKFILE="$(ls "$WT/.claude/inflight-acks/$(printf '%s' "$TIP" | cut -c1-12)".*.ack 2>/dev/null | head -1)"
[ -n "$ACKFILE" ] || ACKFILE="$WT/.claude/inflight-acks/$(printf '%s' "$TIP" | cut -c1-12).ack"
python3 - "$ACKFILE" "$BASE_SHA" <<'STALEPY'
import re, sys
p, stale = sys.argv[1], sys.argv[2]
src = open(p).read()
open(p, "w").write(re.sub(r"^sha: .*$", "sha: " + stale, src, count=1, flags=re.M))
STALEPY
# READ AGAINST AN EMPTY DURABLE LEDGER, deliberately: this case is about the
# FILE reader rejecting a stale sha inside a correctly named file. The durable
# row written at 5f is untouched and still valid — correctly, because corrupting
# a copy of an ack does not un-ack it — so leaving it in the picture would make
# this case assert the opposite of what it is named for.
AOUT="$(RICHOS_INFLIGHT_ACK_LEDGER="$NO_ACK_LEDGER" bash "$RUNNER" acks --repo "$REPO" 2>&1)"
say "5m stale sha inside a correctly named ack" "$AOUT"
case "$AOUT" in
    *"verified : NO"*) ok "5m. the right FILENAME with a stale sha INSIDE is still invalid" ;;
    *) bad "5m. a stale sha inside a correctly named ack is invalid" "$AOUT" ;;
esac
bash "$ACKER" --sha "$TIP" --impact stale-record \
    --detail "My copy of src/a.txt is one revision behind; I will re-read it before I touch it." \
    --paths "src/a.txt src/b.txt" --worktree "$WT" >/dev/null 2>&1

# ==========================================================================
# 5i. the waiver — the only escape hatch, and it is loud
# ==========================================================================
rm -f "$TEAM_DIR/inflight-notices.jsonl"
run_guard
[ "$GRC" -eq 2 ] && ok "5i. with the notices gone the debt is back (the ledger is what is trusted)" \
                 || bad "5i. removing the ledger restores the debt" "exit $GRC"
WOUT="$(bash "$RUNNER" waive "$WT" --reason "abandoned experiment, agent confirmed dead" --repo "$REPO" 2>&1)"
say "5i waiver" "$WOUT"
run_guard
[ "$GRC" -eq 0 ] && ok "5j. a recorded waiver clears the debt" \
                 || bad "5j. a recorded waiver clears the debt" "exit $GRC: $GOUT"
if grep -q "abandoned experiment" "$TEAM_DIR/inflight-waivers.jsonl" 2>/dev/null; then
    ok "5k. and the waiver is on the record with its reason and its actor"
else
    bad "5k. the waiver is recorded with its reason"
fi
WOUT="$(bash "$RUNNER" waive "$WT" --repo "$REPO" 2>&1)"
case "$WOUT" in
    *"--reason is required"*) ok "5l. a waiver with no reason is refused — that is a silent skip in a costume" ;;
    *) bad "5l. a reasonless waiver is refused" "$WOUT" ;;
esac

# ==========================================================================
# 6. TRANSCRIPT FOUR — no live worktrees: a SILENT no-op, positively probed
# ==========================================================================
BARE="$SANDBOX/bare"
mkdir -p "$BARE/scripts/hooks" "$BARE/scripts/lib"
cp "$GUARD" "$BARE/scripts/hooks/"
cp "$REPO/scripts/lib/"* "$BARE/scripts/lib/" 2>/dev/null || true
printf 'PROTECTED_PATHS="src"\n' > "$BARE/orchestration.config"
git -C "$BARE" init -q -b main
git -C "$BARE" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$BARE" config user.name "$(git config user.name 2>/dev/null || echo tester)"
echo x > "$BARE/f.txt"; git -C "$BARE" add -A; git -C "$BARE" commit -q -m one

BARE_TEAMS="$SANDBOX/teams-bare/session-deadbeef"
mkdir -p "$BARE_TEAMS"
GOUT="$(RICHOS_ENTITY_ROOT="$BARE" INFLIGHT_TEAMS_DIR="$SANDBOX/teams-bare" \
        push_payload "$BARE" | RICHOS_ENTITY_ROOT="$BARE" INFLIGHT_TEAMS_DIR="$SANDBOX/teams-bare" \
        bash "$BARE/scripts/hooks/guard-inflight-notify.sh" 2>&1)"
GRC=$?
say "6 no worktrees" "rc=$GRC out=[$GOUT]"
[ "$GRC" -eq 0 ] && ok "6a. with no teammate worktrees the guard allows the push" \
                 || bad "6a. no worktrees -> allowed" "exit $GRC: $GOUT"
[ -z "$GOUT" ] && ok "6b. and says NOTHING — silence is the whole requirement here" \
               || bad "6b. no worktrees -> silent" "$GOUT"
# THE POSITIVE PROBE. 6a and 6b would both pass if the guard never ran at all,
# which is precisely how a guard passes for the wrong reason. It leaves a
# footprint when it runs: the repository it swept is registered for the Stop
# hook to watch. No footprint, no run.
if grep -qxF "$(cd "$BARE" && pwd -P)" "$SANDBOX/teams-bare/session-deadbeef/inflight-repos.txt" 2>/dev/null; then
    ok "6c. POSITIVE PROBE: the guard demonstrably RAN — it registered the repo it swept"
else
    bad "6c. POSITIVE PROBE: the guard actually ran" \
        "no inflight-repos.txt entry — 6a/6b may be passing because nothing executed"
fi

# ==========================================================================
# 7. scope — the guard is silent everywhere it has no business
# ==========================================================================
scope_case() { # <name> <command> <cwd>
    local name="$1" cmd="$2" cwd="$3" out rc
    out="$(CMD="$cmd" CWD="$cwd" python3 -c '
import json, os
print(json.dumps({"tool_name": "Bash", "cwd": os.environ["CWD"],
                  "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "tool_input": {"command": os.environ["CMD"]}}))' | bash "$GUARD" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ] && [ -z "$out" ]; then ok "$name"; else bad "$name" "exit $rc: $out"; fi
}
rm -f "$TEAM_DIR/inflight-waivers.jsonl"
scope_case "7a. an ordinary command is not a push"            "ls -la"                      "$REPO"
scope_case "7b. talking about a push is not a push"           "echo 'git push origin main'" "$REPO"
scope_case "7c. --dry-run is not a land"                      "git push --dry-run origin main" "$REPO"
scope_case "7d. pushing a teammate branch is not a land"      "git push origin worktree-norm"  "$REPO"
scope_case "7e. a push from inside a WORKTREE is not a land"  "git push origin main"           "$WT"
scope_case "7f. a merge is not the chokepoint (stated gap)"   "git merge worktree-norm"        "$REPO"
# A linked worktree can sit on a branch that IS in the main/master set, and a
# bare `git push` names no refspec — so neither the branch test nor the refspec
# test stops it there. Only "am I the main checkout?" does.
WT_MASTER="$SANDBOX/wt/other-sonnet-master1"
git -C "$REPO" worktree add -q -b master "$WT_MASTER" >/dev/null 2>&1
scope_case "7g. a bare push from a worktree on 'master' is not a land" "git push" "$WT_MASTER"
git -C "$REPO" worktree remove --force "$WT_MASTER" >/dev/null 2>&1

# ==========================================================================
# 8. the Stop-hook notice — surfaced, never enforced
# ==========================================================================
NOTICE_BODY="Main moved to $TIP. Ack with scripts/inflight-ack.sh."
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "$NOTICE_BODY")" | bash "$WITNESS"
rm -rf "$WT/.claude/inflight-acks"
# Backdate the notice past the timeout.
python3 - "$TEAM_DIR/inflight-notices.jsonl" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
p = sys.argv[1]
rows = [json.loads(l) for l in open(p) if l.strip()]
old = (datetime.now(timezone.utc) - timedelta(hours=3)).isoformat()
for r in rows:
    r["timestamp"] = old
open(p, "w").write("\n".join(json.dumps(r) for r in rows) + "\n")
PY
printf '%s\n' "$(cd "$REPO" && pwd -P)" > "$TEAM_DIR/inflight-repos.txt"
rm -rf "$REPO/.claude/state/stop-hook-notices"
# THE DURABLE LEDGER IS EMPTIED FOR THIS SECTION TOO. `rm -rf` on the ack
# directory above used to be sufficient to model "this teammate never acked";
# since row 3.19 it is not, because the durable row outlives the file on
# purpose. Both halves have to be absent to model a teammate that genuinely
# never answered — which is what section 8 is about.
NOUT="$(stop_payload | RICHOS_INFLIGHT_ACK_LEDGER="$NO_ACK_LEDGER" bash "$STOPHOOK" 2>/dev/null)"
NRC=$?
say "8 stop notice" "rc=$NRC out=$NOUT"
[ "$NRC" -eq 0 ] && ok "8a. the Stop hook never blocks — it exits 0 on every path" \
                 || bad "8a. the Stop hook exits 0" "exit $NRC"
if printf '%s' "$NOUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "no ack past the timeout" in d.get("systemMessage","") else 1)' 2>/dev/null; then
    ok "8b. an overdue ack is SURFACED to the operator on the channel that reaches him"
else
    bad "8b. an overdue ack is surfaced" "$NOUT"
fi
if printf '%s' "$NOUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "TaskStop" in d.get("systemMessage","") else 1)' 2>/dev/null; then
    ok "8c. and it names the ACTION and the ACTOR, not just the condition"
else
    bad "8c. the notice names the action" "$NOUT"
fi
NOUT2="$(stop_payload | RICHOS_INFLIGHT_ACK_LEDGER="$NO_ACK_LEDGER" bash "$STOPHOOK" 2>/dev/null)"
[ -z "$NOUT2" ] && ok "8d. the same unchanged condition says nothing again — a notice every turn gets muted" \
               || bad "8d. an unchanged condition is not re-announced" "$NOUT2"

# ==========================================================================
# 9. an AMBIGUOUS recipient credits nothing — a notice attached to the wrong
#    worktree is a guard reporting a teammate was told when it was not
# ==========================================================================
rm -f "$TEAM_DIR/inflight-notices.jsonl" "$TEAM_DIR/inflight-waivers.jsonl"
WT2="$SANDBOX/wt/norm-sonnet-feature2"
git -C "$REPO" worktree add -q -b worktree-norm2 "$WT2" >/dev/null 2>&1
echo "second teammate" > "$WT2/src/c.txt"
git -C "$WT2" add -A
git -C "$WT2" commit -q -m "second teammate commit"
# "norm-sonnet-feature" shares {norm, feature} with BOTH worktree basenames, so
# it names neither of them.
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature" "main moved to $TIP")" | bash "$WITNESS"
run_guard
say "9 ambiguous recipient" "$GOUT"
[ "$GRC" -eq 2 ] && ok "9a. a recipient that matches TWO live worktrees credits NEITHER" \
                 || bad "9a. an ambiguous recipient credits nothing" "exit $GRC: $GOUT"
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature1" "main moved to $TIP")" | bash "$WITNESS"
printf '%s' "$(CWD="$REPO" send_payload "norm-sonnet-feature2" "main moved to $TIP")" | bash "$WITNESS"
run_guard
[ "$GRC" -eq 0 ] && ok "9b. telling BOTH of them, by name, clears it" \
                 || bad "9b. naming both clears it" "exit $GRC: $GOUT"
git -C "$REPO" worktree remove --force "$WT2" >/dev/null 2>&1

# ==========================================================================
# 10. THE ROLE/NAME MISMATCH — the 2026-08-31 false positive, reproduced
# ==========================================================================
# MEASURED, not imagined. Two notices were sent, witnessed and written to
# inflight-notices.jsonl at 22:34:43Z and 22:34:49Z with full 40-character
# sha_tokens naming the tip. The guard still reported OWED-NO-NOTICE with
# `notified-but-unacked: 0`, and the land went through on two recorded waivers.
#
# The shape below is that shape exactly, taken from session 374e6f14's own
# files rather than invented:
#   * a NATIVE isolation worktree, .claude/worktrees/agent-<id>, locked by the
#     harness with a live pid — the only liveness signal a native worktree has;
#   * worker-events.jsonl carrying WorkerStarted rows ONLY, whose `agent_type`
#     is the ROLE `zach` (the live log has no WorkerCreated row at all, because
#     the PostToolUse[Agent] emitter is not registered in that session);
#   * the unique spawn name `zach-opus-s1` living where it actually lives — the
#     orchestrator's transcript, joined on tool_use_id;
#   * a notice addressed, as SendMessage always is, to that unique name.
#
# Before the fix, 10b FAILS: the debt side resolved `zach`, the witness
# recorded `zach-opus-s1`, and no reading could join them.
rm -f "$TEAM_DIR/inflight-notices.jsonl" "$TEAM_DIR/inflight-waivers.jsonl"
git -C "$REPO" worktree remove --force "$WT" >/dev/null 2>&1

native_teammate() { # <agent-id> <branch> <file> -> echoes the worktree path
    local aid="$1" br="$2" f="$3" wt="$REPO/.claude/worktrees/agent-$1"
    # From the BASE, not the tip: a teammate cut from the tip is behind nothing
    # and is owed nothing, which is a test that passes by testing nothing.
    git -C "$REPO" worktree add -q -b "$br" "$wt" "$BASE_SHA" >/dev/null 2>&1
    echo "native teammate work" > "$wt/src/$f"
    git -C "$wt" add -A
    git -C "$wt" commit -q -m "native teammate commit $br"
    # The harness locks a native worktree while its agent runs and the lock
    # line carries the pid. $$ is this test, which is alive, so the sweep sees
    # the same LIVE signal it sees in production.
    git -C "$REPO" worktree lock --reason "agent running (pid $$)" "$wt" 2>/dev/null
    printf '%s' "$wt"
}

AID_S1="a5a1c0ffee1122334"
NWT_S1="$(native_teammate "$AID_S1" worktree-zach-s1 d.txt)"

# worker-events.jsonl, in the live shape: the ROLE, and nothing else.
python3 - "$TEAM_DIR/worker-events.jsonl" "$AID_S1" "$NWT_S1" <<'WEPY'
import json, sys
from datetime import datetime, timezone
path, aid, cwd = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "a", encoding="utf-8") as fh:
    fh.write(json.dumps({
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "event": "WorkerStarted", "lifecycle_state": "started",
        "source_hook": "SubagentStart", "agent_id": aid,
        "agent_type": "zach",            # THE ROLE. Never the name.
        "session_id": "deadbeef-1111-4000-8000-000000000000",
        "cwd": cwd, "decision": "logged"}) + "\n")
WEPY

# The transcript, in the shape agent-liveness.py:names_to_ids reads: the Agent
# tool_use carries the spawn name, the matching tool_result carries the id.
TRANSCRIPT="$SANDBOX/transcript.jsonl"
: > "$TRANSCRIPT"
transcript_spawn() { # <name> <agent-id>
    NM="$1" AID="$2" python3 -c '
import json, os
nm, aid = os.environ["NM"], os.environ["AID"]
tu = "toolu_" + aid[:8]
print(json.dumps({"type": "assistant", "message": {"content": [
    {"type": "tool_use", "id": tu, "name": "Agent",
     "input": {"name": nm, "subagent_type": nm.split("-")[0],
               "isolation": "worktree"}}]}}))
print(json.dumps({"type": "user", "message": {"content": [
    {"type": "tool_result", "tool_use_id": tu}]},
    "toolUseResult": {"agentId": aid}}))' >> "$TRANSCRIPT"
}
transcript_spawn zach-opus-s1 "$AID_S1"
export INFLIGHT_TRANSCRIPT="$TRANSCRIPT"

# A push payload that carries the transcript, exactly as a real hook payload does.
push_payload_t() { # [cwd]
    CWD="${1:-$REPO}" TP="$TRANSCRIPT" python3 -c '
import json, os
print(json.dumps({"tool_name": "Bash", "cwd": os.environ["CWD"],
                  "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "transcript_path": os.environ["TP"],
                  "tool_input": {"command": "git push origin main"}}))'
}
run_guard_t() {
    local tp; tp="$(push_payload_t "${1:-$REPO}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("transcript_path",""))')"
    GOUT="$(bash "$RUNNER" check --repo "${1:-$REPO}" --session deadbeef-1111-4000-8000-000000000000 --transcript "$tp" 2>&1)"; GRC=$?
    [ "$GRC" -eq 1 ] && GRC=2
    return 0
}

run_guard_t
say "10a native debt" "$GOUT"
[ "$GRC" -eq 2 ] && ok "10a. a LIVE native worktree behind the tip is owed a notice" \
                 || bad "10a. a live native worktree is owed a notice" "exit $GRC: $GOUT"

# THE REPRODUCTION. The notice is addressed the only way SendMessage can be
# addressed: the unique spawn name.
printf '%s' "$(CWD="$REPO" send_payload "zach-opus-s1" "Main moved to $TIP while you were working.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
say "10b native notice credited" "$GOUT"
[ "$GRC" -eq 0 ] && ok "10b. a notice to the UNIQUE SPAWN NAME clears the debt of the worktree that name belongs to" \
                 || bad "10b. a notice to zach-opus-s1 clears agent-$AID_S1's debt" "exit $GRC: $GOUT"

SOUT="$(bash "$RUNNER" status --repo "$REPO" 2>&1)"
say "10c status names the teammate" "$SOUT"
case "$SOUT" in
    *"teammate   : zach-opus-s1"*) ok "10c. the sweep calls the teammate by the name it is addressed by, not by its role" ;;
    *) bad "10c. the sweep names zach-opus-s1" "$SOUT" ;;
esac

# THE ROLE IS NOT AN ADDRESS — and this is checked NOW, while exactly ONE
# Zach is live, because that is the only arrangement in which crediting by
# role would look right. Once a second Zach exists the ambiguity rule catches
# it for a different reason, and a property defended by the wrong rule is a
# property that is not defended.
rm -f "$TEAM_DIR/inflight-notices.jsonl"
printf '%s' "$(CWD="$REPO" send_payload "zach" "Main moved to $TIP.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
say "10d bare role, one live zach" "$GOUT"
[ "$GRC" -eq 2 ] && ok "10d. with ONE live Zach, a notice to the bare role 'zach' still clears nothing" \
                 || bad "10d. the bare role clears nothing even when it is unambiguous" "exit $GRC: $GOUT"
printf '%s' "$(CWD="$REPO" send_payload "zach-opus-s1" "Main moved to $TIP while you were working.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
[ "$GRC" -eq 0 ] && ok "10e. and the same land clears the moment the unique name is used instead" \
                 || bad "10e. the unique name clears it" "exit $GRC: $GOUT"

# THE OTHER HALF, and the one that matters more: a guard that stopped
# false-positiving by no longer firing would be worse than the bug. A SECOND
# live teammate OF THE SAME ROLE, un-notified, must still block — which is
# also what makes a role-prefix "fix" impossible to pass this suite with.
AID_T1="a7b1decafbad5566"
NWT_T1="$(native_teammate "$AID_T1" worktree-zach-t1 e.txt)"
python3 - "$TEAM_DIR/worker-events.jsonl" "$AID_T1" "$NWT_T1" <<'WEPY'
import json, sys
from datetime import datetime, timezone
path, aid, cwd = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "a", encoding="utf-8") as fh:
    fh.write(json.dumps({
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "event": "WorkerStarted", "lifecycle_state": "started",
        "source_hook": "SubagentStart", "agent_id": aid, "agent_type": "zach",
        "session_id": "deadbeef-1111-4000-8000-000000000000",
        "cwd": cwd, "decision": "logged"}) + "\n")
WEPY
transcript_spawn zach-opus-t1 "$AID_T1"
run_guard_t
say "10f second same-role teammate" "$GOUT"
[ "$GRC" -eq 2 ] && ok "10f. a SECOND live teammate of the same role, un-notified, STILL BLOCKS the push" \
                 || bad "10f. an un-notified same-role teammate still blocks" "exit $GRC: $GOUT"
case "$GOUT" in
    *"zach-opus-t1"*) ok "10g. and the refusal names the one that was NOT told, by its unique name" ;;
    *) bad "10g. the refusal names zach-opus-t1" "$GOUT" ;;
esac

# The bare ROLE is not an address. A guard that credited it would be back to
# the defect: three Zachs ran at once on the day this was measured.
printf '%s' "$(CWD="$REPO" send_payload "zach" "Main moved to $TIP.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
[ "$GRC" -eq 2 ] && ok "10h. a notice to the bare ROLE 'zach' clears nothing — a role is not an address" \
                 || bad "10h. the bare role clears nothing" "exit $GRC: $GOUT"

printf '%s' "$(CWD="$REPO" send_payload "zach-opus-t1" "Main moved to $TIP.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
[ "$GRC" -eq 0 ] && ok "10i. telling the second one by name clears the land" \
                 || bad "10i. telling both by name clears the land" "exit $GRC: $GOUT"

# The witness resolved the recipient to an agent id AT SEND TIME, from the same
# module the guard resolves worktrees with. That is the join being exact on
# both sides rather than on one.
if grep -q "\"to_agent_id\": \"$AID_T1\"" "$TEAM_DIR/inflight-notices.jsonl"; then
    ok "10j. the witness recorded the recipient's AGENT ID, resolved in the lead's own execution"
else
    bad "10j. the witness records to_agent_id" "$(tail -1 "$TEAM_DIR/inflight-notices.jsonl")"
fi


# SendMessage'S OTHER LEGAL ADDRESS — the bare agentId. Its own documentation
# says to use it "when the agent has no name", and on 2026-09-01 the lead used
# it for every one of the four notices in this machine's live ledger:
#     "to": "a41785cc3086260c0"   (not "zach-opus-n1")
# The recipient here is a HAND-ROLLED worktree, which has no agent id of its
# own, so the id can only reach it by being expanded through the identity index
# into the name the directory carries. Without that expansion the sweep reports
# OWED-NO-NOTICE against a notice that was sent, witnessed and logged — the
# 2026-08-31 defect wearing the other address.
AID_U1="a9c0ffee42424242"
HWT_U1="$SANDBOX/wt/zach-opus-u1"
git -C "$REPO" worktree add -q -b worktree-zach-u1 "$HWT_U1" "$BASE_SHA" >/dev/null 2>&1
echo "hand-rolled teammate work" > "$HWT_U1/src/f.txt"
git -C "$HWT_U1" add -A
git -C "$HWT_U1" commit -q -m "hand-rolled teammate commit"
transcript_spawn zach-opus-u1 "$AID_U1"

run_guard_t
[ "$GRC" -eq 2 ] && ok "10k. the hand-rolled teammate is owed a notice too" \
                 || bad "10k. the hand-rolled teammate is owed a notice" "exit $GRC: $GOUT"

printf '%s' "$(CWD="$REPO" send_payload "$AID_U1" "Main moved to $TIP.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
say "10l notice addressed by raw agent id" "$GOUT"
[ "$GRC" -eq 0 ] && ok "10l. a notice addressed by RAW AGENT ID reaches the teammate that id belongs to, even when its worktree carries only a name" \
                 || bad "10l. a raw agent id reaches the right teammate" "exit $GRC: $GOUT"

# The expansion is the index, so it can only ever name a real teammate: an id
# nobody was spawned under resolves to nothing and clears nothing.
git -C "$REPO" worktree add -q -b worktree-zach-v1 "$SANDBOX/wt/zach-opus-v1" "$BASE_SHA" >/dev/null 2>&1
echo x > "$SANDBOX/wt/zach-opus-v1/src/g.txt"
git -C "$SANDBOX/wt/zach-opus-v1" add -A
git -C "$SANDBOX/wt/zach-opus-v1" commit -q -m "another hand-rolled teammate"
printf '%s' "$(CWD="$REPO" send_payload "adeadbeefdeadbeef" "Main moved to $TIP.")" \
    | INFLIGHT_TRANSCRIPT="$TRANSCRIPT" bash "$WITNESS"
run_guard_t
[ "$GRC" -eq 2 ] && ok "10m. an agent id that names no spawned teammate clears nothing" \
                 || bad "10m. an unknown agent id clears nothing" "exit $GRC: $GOUT"
git -C "$REPO" worktree remove --force "$SANDBOX/wt/zach-opus-v1" >/dev/null 2>&1

# ==========================================================================
# 11. THE OPERATOR'S OWN INSTRUMENTS — they must work when he runs them
# ==========================================================================
# `inflight-notify.sh status`, run by hand on this machine, printed
# `notice ledger: <no team dir resolved>` and `teammate: <unresolved>`: the
# diagnostic was blind at exactly the moment it was needed. There is no
# CLAUDE_SESSION_ID in a terminal, and four session directories exist.
# A second session directory on the machine — four exist on the real one —
# with no session id in the environment, which is every by-hand run.
DECOY_TEAM_DIR="$TEAMS/session-cafe1234"
mkdir -p "$DECOY_TEAM_DIR"
printf 'someone-else-sonnet-1\n' > "$DECOY_TEAM_DIR/spawned-names.log"
SOUT="$(CLAUDE_SESSION_ID="" bash "$RUNNER" status --repo "$REPO" 2>&1)"
say "11a by-hand status, 2 session dirs, no session id" "$SOUT"
case "$SOUT" in
    *"<no team dir resolved>"*) bad "11a. status resolves a real ledger path with no CLAUDE_SESSION_ID set" "$SOUT" ;;
    *"$TEAM_DIR/inflight-notices.jsonl"*) ok "11a. status names the REAL ledger path with no session id in the environment" ;;
    *) bad "11a. status names the real ledger path" "$SOUT" ;;
esac
case "$SOUT" in
    *"resolved by    :"*) ok "11b. and it says HOW it chose that directory — a guess the operator can see is a guess" ;;
    *) bad "11b. status reports its team-dir resolution" "$SOUT" ;;
esac

# INFLIGHT_TEAMS_DIR pointing STRAIGHT AT a team directory: the reading its own
# error message tells the operator to use.
WOUT="$(INFLIGHT_TEAMS_DIR="$TEAM_DIR" CLAUDE_SESSION_ID="" bash "$RUNNER" waive "$NWT_S1" \
        --reason "documented escape hatch, exercised as documented" --repo "$REPO" 2>&1)"
WRC=$?
say "11c waive via INFLIGHT_TEAMS_DIR" "rc=$WRC out=$WOUT"
if [ "$WRC" -eq 0 ] && printf '%s' "$WOUT" | grep -q "recorded a waiver in $TEAM_DIR/inflight-waivers.jsonl"; then
    ok "11c. waive works via INFLIGHT_TEAMS_DIR pointed straight at a team directory — the variable its own error message names"
else
    bad "11c. waive honors INFLIGHT_TEAMS_DIR as documented" "exit $WRC: $WOUT"
fi
unset INFLIGHT_TRANSCRIPT

# ==========================================================================
# 12. TWO TEAMMATES, ONE LAND — the filename collision, and the merge
#
# Two teammates acknowledging the same land is CORRECT: both were notified and
# both answered. Keyed on the sha alone their branches carried two different
# files at ONE path, and the merge was an add/add conflict — twice on
# 2026-09-01. A lander in a hurry resolves that with `--ours`, and the evidence
# that a teammate acknowledged a land is gone with no trace left anywhere.
#
# 12a/12b are the fix. 12c is the NEGATIVE CONTROL, and it is not optional: it
# reproduces the old key in the same fixture and shows the conflict and the
# silent loss, so 12a cannot pass by the merge being trivial for some other
# reason.
# ==========================================================================
MERGE_REPO="$(mktemp -d -t inflight-merge.XXXXXX)"
git -C "$MERGE_REPO" init -q -b main 2>/dev/null || { git init -q "$MERGE_REPO"; git -C "$MERGE_REPO" checkout -q -b main 2>/dev/null; }
printf 'seed\n' > "$MERGE_REPO/seed.txt"
git -C "$MERGE_REPO" add -A >/dev/null 2>&1
git -C "$MERGE_REPO" commit -qm seed >/dev/null 2>&1
MERGE_TIP="$(git -C "$MERGE_REPO" rev-parse HEAD)"

ack_on_branch() { # <branch> <teammate> [--legacy]
    git -C "$MERGE_REPO" checkout -q main
    git -C "$MERGE_REPO" checkout -q -b "$1" 2>/dev/null || git -C "$MERGE_REPO" checkout -q "$1"
    if [ "${3:-}" = "--legacy" ]; then
        mkdir -p "$MERGE_REPO/.claude/inflight-acks"
        {
            printf 'sha: %s\n' "$MERGE_TIP"
            printf 'impact: none\n'
            # The detail names its author, because in life two teammates never
            # write the same sentence — and identical bytes at one path is the
            # ONE case git merges without complaint, which would make this
            # control pass for the wrong reason.
            printf 'detail: %s here: I read the land and it does not touch anything I am holding open.\n' "$2"
            printf 'paths: none\n'
            printf 'worktree: %s\n' "$MERGE_REPO"
        } > "$MERGE_REPO/.claude/inflight-acks/$(printf '%s' "$MERGE_TIP" | cut -c1-12).ack"
    else
        bash "$ACKER" --sha "$MERGE_TIP" --impact none \
            --detail "I read the land and it does not touch anything I am holding open here." \
            --paths "none" --worktree "$MERGE_REPO" --teammate "$2" >/dev/null 2>&1
    fi
    git -C "$MERGE_REPO" add -A >/dev/null 2>&1
    git -C "$MERGE_REPO" commit -qm "ack from $2" >/dev/null 2>&1
    rm -rf "$MERGE_REPO/.claude/inflight-acks"
    git -C "$MERGE_REPO" checkout -q main
}

ack_on_branch zach-branch zach-opus-c1
ack_on_branch norm-branch norm-sonnet-feature1
git -C "$MERGE_REPO" merge -q --no-edit zach-branch >/dev/null 2>&1
MRC=0
git -C "$MERGE_REPO" merge --no-edit norm-branch >/dev/null 2>&1 || MRC=$?
NACKS="$(ls "$MERGE_REPO/.claude/inflight-acks/" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$MRC" -eq 0 ] && [ "$NACKS" = "2" ]; then
    ok "12a. two teammates acking ONE sha merge with NO conflict, and both records survive"
else
    bad "12a. two acks for one sha merge cleanly" "merge rc=$MRC, $NACKS record(s): $(ls "$MERGE_REPO/.claude/inflight-acks/" 2>/dev/null | tr '\n' ' ')"
fi

# Both records are READABLE as two records, not silently collapsed to one. A
# writer that stopped colliding while the reader still handled one file would
# have moved the loss from the merge to the sweep.
BOTH=1
for who in zach-opus-c1 norm-sonnet-feature1; do
    [ -f "$MERGE_REPO/.claude/inflight-acks/$(printf '%s' "$MERGE_TIP" | cut -c1-12).$who.ack" ] || BOTH=0
done
READ_N="$(python3 -c '
import os, sys
sys.path.insert(0, sys.argv[1])
import importlib.util as ilu
spec = ilu.spec_from_file_location("inflight", os.path.join(sys.argv[1], "inflight.py"))
m = ilu.module_from_spec(spec); spec.loader.exec_module(m)
print(len(m.ack_paths(sys.argv[2], sys.argv[3])))
' "$REPO/scripts/lib" "$MERGE_REPO" "$MERGE_TIP" 2>/dev/null || echo 0)"
if [ "$BOTH" = 1 ] && [ "$READ_N" = "2" ]; then
    ok "12b. the READER returns both records — the multi-teammate case is the normal case"
else
    bad "12b. the reader handles more than one record" "named-files=$BOTH reader-count=$READ_N"
fi

# 12c — THE NEGATIVE CONTROL. The old key, same fixture: the merge conflicts,
# and `--ours` resolves it by throwing one teammate's answer away in silence.
git -C "$MERGE_REPO" checkout -q main
git -C "$MERGE_REPO" branch -q -D legacy-a legacy-b 2>/dev/null || true
ack_on_branch legacy-a zach-opus-c1 --legacy
ack_on_branch legacy-b norm-sonnet-feature1 --legacy
git -C "$MERGE_REPO" merge -q --no-edit legacy-a >/dev/null 2>&1
LRC=0
git -C "$MERGE_REPO" merge --no-edit legacy-b >/dev/null 2>&1 || LRC=$?
if [ "$LRC" -ne 0 ]; then
    git -C "$MERGE_REPO" checkout --ours -- .claude/inflight-acks >/dev/null 2>&1
    git -C "$MERGE_REPO" add -A >/dev/null 2>&1
    git -C "$MERGE_REPO" commit -qm "resolved with --ours" >/dev/null 2>&1
    LEGACY_N="$(ls "$MERGE_REPO/.claude/inflight-acks/"*.ack 2>/dev/null | grep -c '\.ack$' || true)"
    ok "12c. NEGATIVE CONTROL: the OLD sha-only key conflicts on the merge, and --ours leaves $LEGACY_N of the 2 answers"
else
    bad "12c. the old key must conflict — otherwise 12a proves nothing" "merge rc=$LRC"
fi

# 12d/12e — ATTRIBUTION, exercised on the predicate directly so the two answers
# cannot be confused with anything else the sweep decides.
#
#   12d  every ack already on main is called `<sha12>.ack` and carries no
#        teammate line at all. Those must keep counting — orphaning them would
#        delete the only proof that those teammates ever answered.
#   12e  a record that says out loud it came from ANOTHER teammate must not
#        discharge this one's debt. That is what a merge deposits in a worktree,
#        and crediting it would let one answer cover a silence.
ATTR="$(python3 - "$REPO/scripts/lib" <<'ATTRPY' 2>&1
import os, sys, importlib.util as ilu, tempfile
libdir = sys.argv[1]
spec = ilu.spec_from_file_location("inflight", os.path.join(libdir, "inflight.py"))
m = ilu.module_from_spec(spec); spec.loader.exec_module(m)

tip = "a" * 40
root = tempfile.mkdtemp()
d = os.path.join(root, ".claude", "inflight-acks")
os.makedirs(d)
detail = "I read the land and none of my open assumptions depend on it at all."


def write(name, extra):
    with open(os.path.join(d, name), "w") as fh:
        fh.write("sha: %s\nimpact: none\ndetail: %s\npaths: none\n%s" % (tip, detail, extra))


wt = {"path": root, "addresses": ["zach-opus-ss1"], "resolved_name": "zach-opus-ss1",
      "moved_paths": []}

write("%s.ack" % tip[:12], "worktree: %s\n" % root)
legacy = m.ack_status(wt, tip, "", [], 30)
os.remove(os.path.join(d, "%s.ack" % tip[:12]))

write("%s.norm-sonnet-f1.ack" % tip[:12], "teammate: norm-sonnet-f1\nworktree: /elsewhere\n")
foreign = m.ack_status(wt, tip, "", [], 30)

print("legacy=%s foreign=%s says=%s"
      % (legacy["verified"], foreign["verified"],
         "named" if any("belong to" in p for p in foreign["problems"]) else "silent"))
ATTRPY
)"
case "$ATTR" in
    *"legacy=True"*) ok "12d. an ack in the OLD sha-only name, with no teammate line, still verifies" ;;
    *) bad "12d. pre-existing acks keep working" "$ATTR" ;;
esac
case "$ATTR" in
    *"foreign=False"*"says=named"*) ok "12e. another teammate's record does not discharge this one's debt, and is NAMED" ;;
    *) bad "12e. a foreign record is not credited, and says so" "$ATTR" ;;
esac
rm -rf "$MERGE_REPO"

# ==========================================================================
# 13. A NOTICE'S AGE IS READ AS UTC, IN DAYLIGHT SAVING TOO
#
# Measured 2026-09-26 on this Mac, in BST: a notice sent seconds earlier read
# "60 min since the notice (OVERDUE)" against the 30-minute ack timeout, because
# the age was time.mktime(t) - time.timezone, and time.timezone is the STANDARD
# offset. containers.py fixed the same defect on 2026-09-13; its C10 explains
# why the zone must be PINNED in a child process: in a zone not in daylight
# saving the two readings are arithmetically identical, and the case proves
# nothing. So the precondition is checked before anything rests on it.
# ==========================================================================
AGE="$(TZ=Europe/London LIB_DIR="$SRC_DIR/../lib" python3 - <<'AGEPY' 2>&1
import calendar, os, sys, time
sys.path.insert(0, os.environ["LIB_DIR"])
import inflight
summer = calendar.timegm((2026, 7, 1, 12, 0, 0, 0, 0, 0))
if time.localtime(summer).tm_isdst != 1:
    sys.exit("precondition: Europe/London is not in daylight saving at the moment under test")
inflight.time.time = lambda: summer + 5
wt = {"path": "/nowhere", "resolved_name": "", "addresses": [], "identity_tokens": set(),
      "identity_tokens_strict": set(), "agent_id": ""}
ack = inflight.ack_status(wt, "0" * 40, "2026-07-01T12:00:00+00:00", [], 30)
print("age=%s ack_age=%s overdue=%s" % (inflight._age_of_iso("2026-07-01T12:00:00+00:00"),
                                         ack.get("age_sec"), ack.get("overdue")))
AGEPY
)"
say "13 age in BST" "$AGE"
case "$AGE" in
    "age=5 ack_age=5 overdue=False") ok "13. a notice 5 s old reads 5 s old in BST, and is not overdue" ;;
    *) bad "13. a notice's age is read as UTC under daylight saving" "$AGE" ;;
esac

# ==========================================================================
# 14. AN ASSESSMENT THAT RETURNS AN ERROR IS NOT CHECKED, NEVER "NOTHING
#     OUTSTANDING"
#
# assess() reports some failures by RETURNING a result with "error" set (no
# tip commit, an unreadable worktree inventory, git gaps) rather than raising.
# The Stop notice used to read only the raised kind, so a returned error fell
# through to "Nothing outstanding." Its own repository, so the state before
# each case is known: a teammate behind and never told (CEO 2026-10-06: no
# teammate is owed a notice, so the Stop notice says nothing about it), then a
# swept directory whose tip cannot be resolved, then the clean answer again.
# ==========================================================================
CLEAN="$SANDBOX/clean-repo"
CLEAN_TEAMS="$SANDBOX/clean-teams"
mkdir -p "$CLEAN" "$CLEAN_TEAMS/session-deadbeef" "$SANDBOX/not-a-repo"
printf 'PROTECTED_PATHS="src"\n' > "$CLEAN/orchestration.config"
git -C "$CLEAN" init -q -b main
git -C "$CLEAN" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$CLEAN" config user.name "$(git config user.name 2>/dev/null || echo tester)"
mkdir -p "$CLEAN/src"
echo "one" > "$CLEAN/src/a.txt"
git -C "$CLEAN" add -A
git -C "$CLEAN" commit -q -m "base"
CLEAN_WT="$SANDBOX/wt/mark-sonnet-clean1"
git -C "$CLEAN" worktree add -q -b worktree-mark "$CLEAN_WT" >/dev/null 2>&1
echo "teammate work" > "$CLEAN_WT/src/b.txt"
git -C "$CLEAN_WT" add -A
git -C "$CLEAN_WT" commit -q -m "teammate commit"
echo "two" >> "$CLEAN/src/a.txt"
git -C "$CLEAN" add -A
git -C "$CLEAN" commit -q -m "a land that moves main"
: > "$CLEAN_TEAMS/session-deadbeef/inflight-repos.txt"
clean_stop() {
    stop_payload | RICHOS_ENTITY_ROOT="$CLEAN" INFLIGHT_TEAMS_DIR="$CLEAN_TEAMS" \
        RICHOS_INFLIGHT_ACK_LEDGER="$NO_ACK_LEDGER" bash "$STOPHOOK" 2>/dev/null
}
notice_has() { # <output> <text>
    printf '%s' "$1" | NEEDLE="$2" python3 -c 'import json,os,sys; d=json.load(sys.stdin); sys.exit(0 if os.environ["NEEDLE"] in d.get("systemMessage","") else 1)' 2>/dev/null
}
COUT="$(clean_stop)"
say "14 teammate never told" "$COUT"
case "$COUT" in
    *mark-sonnet-clean1*|*"NEVER TOLD"*|*TaskStop*)
        bad "14a. main moved past a live teammate with no notice sent: the Stop notice says nothing about it" "$COUT" ;;
    *)  ok "14a. main moved past a live teammate with no notice sent: the Stop notice says nothing about it" ;;
esac
git -C "$CLEAN" worktree remove --force "$CLEAN_WT" >/dev/null 2>&1
printf '%s\n' "$SANDBOX/not-a-repo" > "$CLEAN_TEAMS/session-deadbeef/inflight-repos.txt"
COUT="$(clean_stop)"
say "14 returned error" "$COUT"
if notice_has "$COUT" "NOT CHECKED, the assessment failed for: not-a-repo" \
        && ! notice_has "$COUT" "Nothing outstanding."; then
    ok "14b. an assessment that RETURNS an error is NOT CHECKED, naming the repository"
else
    bad "14b. an assessment that returns an error is NOT CHECKED, naming the repository" "$COUT"
fi
: > "$CLEAN_TEAMS/session-deadbeef/inflight-repos.txt"
COUT="$(clean_stop)"
say "14 checked again" "$COUT"
notice_has "$COUT" "Nothing outstanding." \
    && ok "14c. once the assessment runs again with nothing waiting, it says nothing outstanding" \
    || bad "14c. once the assessment runs again with nothing waiting, it says nothing outstanding" "$COUT"

# ==========================================================================
# 15. A SENT NOTICE SURVIVES MAIN MOVING ON (review rv-20261025T001946Z#1)
#
# Notices were credited only when they named the CURRENT tip, so a real,
# unacked notice for commit A went silent the moment main moved to B. No
# notice sent is still no line at all (CEO 2026-10-06); a sent one stays until
# a valid ack for its own commit, or the work's end, clears it.
# ==========================================================================
PEND="$SANDBOX/pend-repo"
PEND_TEAMS="$SANDBOX/pend-teams"
PEND_LEDGER="$SANDBOX/state/pend-acks.jsonl"
PEND_WT="$SANDBOX/wt/mark-sonnet-pend1"
mkdir -p "$PEND" "$PEND_TEAMS/session-deadbeef"
: > "$PEND_LEDGER"
printf 'PROTECTED_PATHS="src"\n' > "$PEND/orchestration.config"
git -C "$PEND" init -q -b main
git -C "$PEND" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$PEND" config user.name "$(git config user.name 2>/dev/null || echo tester)"
mkdir -p "$PEND/src"
echo "one" > "$PEND/src/a.txt"
git -C "$PEND" add -A
git -C "$PEND" commit -q -m "base"
git -C "$PEND" worktree add -q -b worktree-pend "$PEND_WT" >/dev/null 2>&1
echo "teammate work" > "$PEND_WT/src/b.txt"
git -C "$PEND_WT" add -A
git -C "$PEND_WT" commit -q -m "teammate commit"
echo "two" >> "$PEND/src/a.txt"
git -C "$PEND" add -A
git -C "$PEND" commit -q -m "land A"
PEND_A="$(git -C "$PEND" rev-parse HEAD)"
: > "$PEND_TEAMS/session-deadbeef/inflight-repos.txt"
pend_env() { RICHOS_ENTITY_ROOT="$PEND" INFLIGHT_TEAMS_DIR="$PEND_TEAMS" RICHOS_INFLIGHT_ACK_LEDGER="$PEND_LEDGER" "$@"; }
pend_stop() { stop_payload | pend_env bash "$STOPHOOK" 2>/dev/null; }

POUT="$(pend_stop)"
say "15a no notice" "$POUT"
case "$POUT" in
    *mark-sonnet-pend1*) bad "15a. no notice sent: no debt and no line for the teammate" "$POUT" ;;
    *) ok "15a. no notice sent: no debt and no line for the teammate" ;;
esac

printf '%s' "$(CWD="$PEND" send_payload "mark-sonnet-pend1" "Main moved to $PEND_A. Ack with scripts/inflight-ack.sh.")" \
    | pend_env bash "$WITNESS" >/dev/null 2>&1
python3 - "$PEND_TEAMS/session-deadbeef/inflight-notices.jsonl" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
p = sys.argv[1]
rows = [json.loads(l) for l in open(p) if l.strip()]
old = (datetime.now(timezone.utc) - timedelta(hours=3)).isoformat()
for r in rows:
    r["timestamp"] = old
open(p, "w").write("\n".join(json.dumps(r) for r in rows) + "\n")
PY
POUT="$(pend_stop)"
say "15b notice for A at A" "$POUT"
notice_has "$POUT" "no ack past the timeout: mark-sonnet-pend1" \
    && ok "15b. precondition: the sent notice for A, unacked past the timeout, is told at tip A" \
    || bad "15b. precondition: the sent notice for A is told at tip A" "$POUT"

echo "three" >> "$PEND/src/a.txt"
git -C "$PEND" add -A
git -C "$PEND" commit -q -m "land B, an ordinary commit"
POUT="$(pend_stop)"
say "15c notice for A at B" "$POUT"
notice_has "$POUT" "mark-sonnet-pend1 (notice for ${PEND_A:0:12})" \
    && ok "15c. the unacked notice for A is still told after main moves on to B" \
    || bad "15c. the unacked notice for A is still told after main moves on to B" "$POUT"
SOUT="$(pend_env bash "$RUNNER" status --repo "$PEND" 2>&1)"
case "$SOUT" in
    *"notice open: sent"*"${PEND_A:0:12}"*) ok "15d. status shows the open notice and its commit" ;;
    *) bad "15d. status shows the open notice and its commit" "$SOUT" ;;
esac

pend_env bash "$ACKER" --sha "$PEND_A" --impact stale-record \
    --detail "My copy of src/a.txt is behind commit A; I will re-read it before I touch it." \
    --paths "src/a.txt src/b.txt" --worktree "$PEND_WT" >/dev/null 2>&1
POUT="$(pend_stop)"
say "15e acked" "$POUT"
if notice_has "$POUT" "Nothing outstanding." && ! notice_has "$POUT" "mark-sonnet-pend1"; then
    ok "15e. a valid ack for A clears it"
else
    bad "15e. a valid ack for A clears it" "$POUT"
fi

rm -rf "$PEND_WT/.claude/inflight-acks"
: > "$PEND_LEDGER"
POUT="$(pend_stop)"
notice_has "$POUT" "mark-sonnet-pend1 (notice for ${PEND_A:0:12})" \
    && ok "15f. precondition: with the ack gone the open notice is told again" \
    || bad "15f. precondition: with the ack gone the open notice is told again" "$POUT"
git -C "$PEND" worktree remove --force "$PEND_WT" >/dev/null 2>&1
POUT="$(pend_stop)"
say "15g work ended" "$POUT"
if notice_has "$POUT" "Nothing outstanding." && ! notice_has "$POUT" "mark-sonnet-pend1"; then
    ok "15g. the work's real end clears it"
else
    bad "15g. the work's real end clears it" "$POUT"
fi

# ==========================================================================
# 16. WHICH NOTICE, AND WHICH OF ITS COMMITS (review rv-20261025T012040Z#1, #2)
#
# A "main moved from BASE to A" notice names two commits; the teammate's head
# already holds BASE, so only A can clear it. And a later notice about another
# repository must not replace the outstanding one about this repository.
# ==========================================================================
MQ="$SANDBOX/multi-repo"
MQ_TEAMS="$SANDBOX/multi-teams"
MQ_WT="$SANDBOX/wt/mark-sonnet-multi1"
mkdir -p "$MQ" "$MQ_TEAMS/session-deadbeef"
printf 'PROTECTED_PATHS="src"\n' > "$MQ/orchestration.config"
git -C "$MQ" init -q -b main
git -C "$MQ" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$MQ" config user.name "$(git config user.name 2>/dev/null || echo tester)"
mkdir -p "$MQ/src"
echo "one" > "$MQ/src/a.txt"
git -C "$MQ" add -A
git -C "$MQ" commit -q -m "base"
MQ_BASE="$(git -C "$MQ" rev-parse HEAD)"
git -C "$MQ" worktree add -q -b worktree-multi "$MQ_WT" >/dev/null 2>&1
echo "teammate work" > "$MQ_WT/src/b.txt"
git -C "$MQ_WT" add -A
git -C "$MQ_WT" commit -q -m "teammate commit"
echo "two" >> "$MQ/src/a.txt"
git -C "$MQ" add -A
git -C "$MQ" commit -q -m "land A"
# The witness records tokens SORTED. Make BASE sort first, as it does about half
# the time, so the case that picked BASE and cleared the notice is always met.
n=0
while [ "$(git -C "$MQ" rev-parse HEAD)" \< "$MQ_BASE" ] && [ "$n" -lt 64 ]; do
    n=$((n + 1))
    git -C "$MQ" commit -q --amend -m "land A ($n)"
done
MQ_A="$(git -C "$MQ" rev-parse HEAD)"
: > "$MQ_TEAMS/session-deadbeef/inflight-repos.txt"
mq_env() { RICHOS_ENTITY_ROOT="$MQ" INFLIGHT_TEAMS_DIR="$MQ_TEAMS" RICHOS_INFLIGHT_ACK_LEDGER="$NO_ACK_LEDGER" "$@"; }
mq_stop() { rm -rf "$MQ/.claude/state/stop-hook-notices"; stop_payload | mq_env bash "$STOPHOOK" 2>/dev/null; }
mq_backdate() {
    python3 - "$MQ_TEAMS/session-deadbeef/inflight-notices.jsonl" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
p = sys.argv[1]
rows = [json.loads(l) for l in open(p) if l.strip()]
now = datetime.now(timezone.utc)
for i, r in enumerate(rows):
    r["timestamp"] = (now - timedelta(hours=3) + timedelta(minutes=i)).isoformat()
open(p, "w").write("\n".join(json.dumps(r) for r in rows) + "\n")
PY
}
printf '%s' "$(CWD="$MQ" send_payload "mark-sonnet-multi1" "Main moved from $MQ_BASE to $MQ_A. Ack with scripts/inflight-ack.sh.")" \
    | mq_env bash "$WITNESS" >/dev/null 2>&1
mq_backdate
echo "three" >> "$MQ/src/a.txt"
git -C "$MQ" add -A
git -C "$MQ" commit -q -m "land B, an ordinary commit"
MOUT="$(mq_stop)"
say "16a two-commit notice at B" "$MOUT"
[ "$MQ_BASE" \< "$MQ_A" ] || bad "16a. precondition: BASE sorts before A" "BASE=$MQ_BASE A=$MQ_A"
notice_has "$MOUT" "mark-sonnet-multi1 (notice for ${MQ_A:0:12})" \
    && ok "16a. a 'from BASE to A' notice stays told by A when the teammate already holds BASE" \
    || bad "16a. a 'from BASE to A' notice stays told by A when the teammate already holds BASE" "$MOUT"

printf '%s' "$(CWD="$MQ" send_payload "mark-sonnet-multi1" "Main moved to $PEND_A in the other repository.")" \
    | mq_env bash "$WITNESS" >/dev/null 2>&1
mq_backdate
MOUT="$(mq_stop)"
say "16b later notice about another repository" "$MOUT"
notice_has "$MOUT" "mark-sonnet-multi1 (notice for ${MQ_A:0:12})" \
    && ok "16b. a later notice about another repository does not hide the open one about this repository" \
    || bad "16b. a later notice about another repository does not hide the open one about this repository" "$MOUT"

echo ""
if [ "$FAIL" -eq 0 ]; then
    printf '\033[32m✓ in-flight sweep: all %s checks passed.\033[0m\n' "$PASS"
    exit 0
fi
printf '\033[31m✗ in-flight sweep: %s/%s passed, %s FAILED.\033[0m\n' "$PASS" "$((PASS + FAIL))" "$FAIL" >&2
exit 1

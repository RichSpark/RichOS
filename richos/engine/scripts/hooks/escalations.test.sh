#!/usr/bin/env bash
#
# escalations.test.sh — the suite for the escalation channel:
#                       scripts/lib/escalations.{py,sh}, scripts/escalate.sh,
#                       scripts/hooks/notice-escalations.sh and
#                       scripts/hooks/session-start-escalations.sh.
#
# ===========================================================================
# WHAT THIS SUITE HAS TO PROVE, AND WHY EACH HALF EXISTS
# ===========================================================================
# The claim is not "a file was written". THAT WAS THE OLD MECHANISM, and it
# worked perfectly: on 2026-09-02 two teammates wrote `BLOCKED.md`, committed
# it, and the files sat unread until a worktree cleanup found them on
# 2026-09-04. A test that asserted "the escalation file exists" would have been
# green through the entire incident.
#
# So every case here asserts ARRIVAL, and the two hardest ones are built to
# reproduce the incident rather than the happy path:
#
#   CASE 3   THE BRANCH IS NEVER MERGED AND THE WORKTREE IS DELETED. This is
#            the incident, mechanically. If the escalation still arrives in
#            full, delivery does not depend on a merge — which is the whole
#            defect being fixed.
#   CASE 6   NOBODY ACKNOWLEDGES IT AND A DAY PASSES. If the notice went quiet
#            after announcing once, this mechanism would have reproduced the
#            two-day silence with a better audit trail. The age bucket must
#            make it speak again, LOUDER, and a new session must re-announce
#            it from scratch.
#
# TWO-SIDED THROUGHOUT. A checker that reports everything gets muted in a week;
# a checker that reports nothing passes every test it has. So the positive case
# has a negative twin one variable away, and case 12 is an explicit NEGATIVE
# CONTROL that rebuilds a mechanism which reports "clear" over an outstanding
# escalation — if the suite cannot go red there, cases 4 and 5 prove nothing.
#
# Usage:  scripts/hooks/escalations.test.sh [-v]

set -uo pipefail
# ERREXIT DELIBERATELY OFF. Most commands here are EXPECTED to exit non-zero —
# `list` is exit 1 when something is outstanding, a refused `raise` is exit 2 —
# so a stray `set -e` would end the run at the first case that works, which is
# how a suite reports green over the half it never reached.

VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t escalations.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# ===========================================================================
# THE LEAK CANARY — THIS SUITE MUST WRITE NOTHING OUTSIDE ITS OWN SANDBOX
# ===========================================================================
# On 2026-09-05 a fixture from case 14 was found as an untracked file in a
# working engineer's worktree:
#
#   docs/verification/escalations/2026-09-05-zach-opus-e1corrupt-a-good-row-
#   beside-a-bad-one.md
#
# It reads exactly like a live escalation — an id, a `from:`, a HEAD sha, a
# "close it with" command — and the engineer who found it could not tell it
# from a genuine one. He was right to refuse to commit it, and he spent part of
# a handoff explaining a file he had not written.
#
# The cause was one argument away from a fix that was already in the file. This
# suite redirects the LEDGER into its sandbox with RICHOS_ESCALATION_LEDGER,
# but `escalate.sh raise` writes a SECOND output — the markdown record — whose
# path comes from --worktree, and --worktree DEFAULTS TO THE CURRENT DIRECTORY.
# Most of the raise calls passed it. One did not.
#
# THAT IS THE GENERIC DEFECT, AND IT IS WHY THIS EXISTS AS A CANARY RATHER THAN
# AS THREE CORRECTED LINES: a test that redirects ONE output to a sandbox while
# a SECOND output still follows the working directory is invisible in review,
# because the redirect that IS there reads as care. Three corrected lines are a
# fix. This is the guarantee — the fourth call, added next month by somebody
# who copied one of the five that were already right, is caught the day it
# lands instead of by a stranger reading a handoff.
#
# WHAT IT WATCHES, STATED RATHER THAN IMPLIED:
#   - the INVOCATION ROOT: the git toplevel of the directory this suite was
#     started in, or that directory when it is not a repository. That is
#     exactly where the 2026-09-05 fixture landed, because run-all-tests.sh
#     does not cd and every suite inherits the operator's working directory.
#   - the ENGINE'S OWN ROOT, when that resolves to a different tree.
#   - the operator's REAL escalation ledger, in case a subprocess ever ignores
#     RICHOS_ESCALATION_LEDGER.
#
# AND WHAT IT DOES NOT, because a canary that overstates its reach is the same
# lie in the other direction:
#   - NOT gitignored paths inside a git root. The snapshot is `git status`, so
#     build output churning under another agent cannot turn this red — at the
#     cost that a leak into a gitignored path is missed. Such a leak would also
#     be invisible to the engineer this protects, which is the trade being made.
#   - NOT the rest of the filesystem, and NOT $HOME beyond the one ledger file.
#     A canary claiming to watch $HOME would be claiming something it cannot
#     check in the time a suite is allowed to take.
#
# ITS ONE FALSE-POSITIVE VECTOR, named so nobody has to rediscover it: another
# writer adding a NON-ignored file to the same checkout while this suite runs
# is indistinguishable from a leak, and case 17n will name it. That is the
# right trade — the failure prints the path, so a reader sees at once that it
# is not an escalation record, whereas the opposite trade (staying quiet to
# avoid the noise) is the whole defect. It is also why 17o counts FIXTURE rows
# in the real ledger rather than total rows: there, a concurrent real
# escalation is likely enough that a total-row check would be false-alarming
# regularly, and a check that cries wolf gets muted.
#
# A ROOT IT CANNOT READ IS A FAILURE, NEVER A QUIET PASS. An unreadable root
# yields an empty snapshot both times and therefore an empty diff, which is the
# "green over something that never ran" shape this engine keeps finding. So the
# health of the baseline is recorded when it is taken, and case 17f refuses to
# report a pass without it.
# ---------------------------------------------------------------------------

CANARY_ROOTS=""
canary_add_root() { # <dir> — resolve to a tree root and remember it once
    local r="$1" top
    [ -d "$r" ] || return 0
    top="$(git -C "$r" rev-parse --show-toplevel 2>/dev/null || true)"
    [ -n "$top" ] || top="$(cd "$r" 2>/dev/null && pwd -P)"
    [ -n "$top" ] || return 0
    printf '%s\n' "$CANARY_ROOTS" | grep -qxF "$top" && return 0
    CANARY_ROOTS="${CANARY_ROOTS}${top}
"
    return 0
}

# canary_snapshot <root> — the watchable state of <root> on stdout, one entry
# per line. EXIT 1 IF IT CANNOT BE READ OR IS TOO LARGE TO WATCH HONESTLY, so
# that an unreadable root can never be mistaken for a root with nothing in it.
#
# EVERY UNTRACKED ENTRY CARRIES A WITNESS OF ITS CONTENTS, and that is not
# decoration. The first version of this canary compared PATHS ONLY, and was
# caught on the day it was written: run it twice against a checkout with the
# leak still present and THE SECOND RUN REPORTED CLEAN. `raise` derives the
# record filename from the date, the teammate and the title, so every run
# writes the SAME path — run one's file was already in the baseline, run two
# overwrote it, nothing was new, and the canary announced a clean sheet over a
# leak happening in front of it. A canary that only catches the FIRST
# occurrence goes quiet exactly when residue proves it is needed.
#
# Tracked files need no witness: `git status` already reports them as ` M `
# when their contents change. It is the `??` entries that are invisible.
#
# The witness is a checksum AND, where the platform can give one, an mtime.
# A file rewritten with identical bytes is something a checksum cannot tell
# apart from the untouched file. A sub-second mtime can. Case 17d is built
# not to depend on either one being available.
CANARY_MAX_ENTRIES=2000

# PICK AN MTIME FORMAT, THEN PROVE IT. `stat -f FMT` is an mtime format on BSD
# and a FILESYSTEM query on GNU, and BOTH EXIT 0 — so a probe that only checks
# the exit code would put the same constant filesystem string into every
# witness on GNU, and the mtime half of this canary would be dead while looking
# alive. That is the defect class this whole change is about, so the candidate
# is accepted only if it returns a NUMBER that actually CHANGES when the same
# file is written twice. A second-resolution format fails that and is
# correctly rejected: it could not have distinguished the same-second case
# either, which is the only case the checksum does not already cover.
canary_pick_mtime() {
    local probe="$SANDBOX/canary-mtime-probe" c t1 t2
    for c in "stat -f %Fm" "stat -c %.9Y"; do
        printf 'a\n' > "$probe"
        t1="$($c "$probe" 2>/dev/null)"
        printf 'bb\n' > "$probe"
        t2="$($c "$probe" 2>/dev/null)"
        case "$t1" in ''|*[!0-9.]*) continue ;; esac
        [ "$t1" != "$t2" ] || continue
        rm -f "$probe"
        printf '%s' "$c"
        return 0
    done
    rm -f "$probe"
    return 1
}
CANARY_MTIME="$(canary_pick_mtime || true)"
canary_witness() { # <path> — contents, and an mtime where one was proven
    printf '%s' "$(cksum < "$1" 2>/dev/null || printf 'unreadable')"
    [ -n "$CANARY_MTIME" ] && printf ' @%s' "$($CANARY_MTIME "$1" 2>/dev/null || printf '?')"
    return 0
}
canary_snapshot() { # <root>
    local root="$1" line path n raw="$SANDBOX/canary-raw.now"
    if git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
        git -C "$root" status --porcelain --untracked-files=all > "$raw" 2>/dev/null || return 1
    else
        ( cd "$root" 2>/dev/null || exit 1; find . -print 2>/dev/null ) > "$raw" || return 1
    fi
    # A root too big to witness is REFUSED, never sampled. Sampling would be a
    # canary that watches some of the tree and reports on all of it. In this
    # engine's own checkouts this count is 0 or 1.
    n="$(wc -l < "$raw" | tr -d ' ')"
    [ "$n" -le "$CANARY_MAX_ENTRIES" ] || return 1
    # A path with a space or a control character comes back from git in double
    # quotes, so the name below does not resolve and the entry keeps its status
    # line WITHOUT a witness — it degrades to path-only for that one file
    # rather than dropping it. A NEW escaped path is still caught; a same-path
    # OVERWRITE of a file whose name needs quoting is not. Named because it is
    # the kind of thing that is otherwise rediscovered the hard way.
    while IFS= read -r line; do
        case "$line" in
            '?? '*) path="$root/${line#?? }" ;;
            ./*)    path="$root/${line#./}" ;;
            *)      printf '%s\n' "$line"; continue ;;
        esac
        if [ -f "$path" ]; then
            printf '%s  [%s]\n' "$line" "$(canary_witness "$path")"
        else
            printf '%s\n' "$line"
        fi
    done < "$raw" | LC_ALL=C sort
    return 0
}

# canary_escaped <baseline-file> <root> — every entry that APPEARED in <root>
# since the baseline, one per line. Empty output means nothing escaped.
canary_escaped() { # <baseline-file> <root>
    local baseline="$1" root="$2" after rel
    after="$SANDBOX/canary-after.now"
    if ! canary_snapshot "$root" > "$after" 2>/dev/null; then
        printf 'UNREADABLE: %s\n' "$root"
        return 0
    fi
    # The sandbox itself is not an escape, in the pathological case where
    # TMPDIR sits inside a watched tree.
    rel=""
    case "$SANDBOX/" in "$root"/*) rel="${SANDBOX#"$root"/}" ;; esac
    if [ -n "$rel" ]; then
        comm -13 "$baseline" "$after" | grep -vF "$rel"
    else
        comm -13 "$baseline" "$after"
    fi
    return 0
}

# The operator's REAL ledger, witnessed by the number of FIXTURE rows in it.
# Counting total rows would go red whenever a real agent raises a real
# escalation while this suite runs, which is a false alarm and would get the
# check muted. Counting this suite's own fixture names goes red only when this
# suite has written where it must not.
CANARY_REAL_LEDGER="${HOME:-/nonexistent}/.claude/state/escalations.jsonl"
canary_fixture_rows() {
    [ -f "$CANARY_REAL_LEDGER" ] || { printf '0'; return 0; }
    grep -c 'zach-opus-e1' "$CANARY_REAL_LEDGER" 2>/dev/null || printf '0'
}
CANARY_REAL_LEDGER_BEFORE="$(canary_fixture_rows)"

canary_add_root "$PWD"
canary_add_root "$ENGINE_ROOT"

CANARY_BASELINES="$SANDBOX/canary-baselines"
mkdir -p "$CANARY_BASELINES"
CANARY_HEALTHY=1
CANARY_N=0
while IFS= read -r canary_root; do
    [ -n "$canary_root" ] || continue
    CANARY_N=$((CANARY_N + 1))
    if ! canary_snapshot "$canary_root" > "$CANARY_BASELINES/$CANARY_N.txt"; then
        CANARY_HEALTHY=0
        printf '  NOTE  the leak canary cannot read %s — case 17f will FAIL rather than pass\n' "$canary_root"
    fi
done <<CANARY_ROOT_LIST
$CANARY_ROOTS
CANARY_ROOT_LIST

# --- the engine copy under test --------------------------------------------
ENG="$SANDBOX/engine"
mkdir -p "$ENG/scripts/hooks" "$ENG/scripts/lib" "$ENG/.claude/state"
cp "$SRC_DIR/notice-escalations.sh" "$SRC_DIR/session-start-escalations.sh" "$ENG/scripts/hooks/"
chmod +x "$ENG/scripts/hooks/notice-escalations.sh" "$ENG/scripts/hooks/session-start-escalations.sh"
cp "$ENGINE_ROOT/scripts/escalate.sh" "$ENG/scripts/"
chmod +x "$ENG/scripts/escalate.sh"
for l in escalations.py escalations.sh resolve-roots.sh resolve-main-checkout.sh \
         seat-jurisdiction.sh stop-hook-notice.sh audit-report-watch.py; do
    cp "$SRC_DIR/../lib/$l" "$ENG/scripts/lib/$l" 2>/dev/null || true
done
# The Stop hook runs audit-report-watch.py before it reads the ledger (eefae0cef).
# Without its copy here every Stop-hook case read "AUDIT REPORT WATCH FAILED"
# instead of the escalation line (15 cases red from 2026-09-30). It is pointed at
# a record repository that does not exist, so it watches nothing and raises
# nothing, and its state stays in the sandbox: never the operator's richos-hq or
# ~/.claude/state/audit-report-watch.
export RICHOS_AUDIT_RECORD_REPO="$SANDBOX/no-audit-record-repo"
export RICHOS_AUDIT_WATCH_STATE="$SANDBOX/state/audit-report-watch"
# The adoption marker. Without it every hook here stands down, and a suite that
# ran against a stood-down engine would be green over nothing at all.
printf 'PROTECTED_PATHS="app"\n' > "$ENG/orchestration.config"

ESCALATE="$ENG/scripts/escalate.sh"
STOP_HOOK="$ENG/scripts/hooks/notice-escalations.sh"
START_HOOK="$ENG/scripts/hooks/session-start-escalations.sh"

# --- the ledger under test, outside every repository ------------------------
LEDGER="$SANDBOX/state/escalations.jsonl"
export RICHOS_ESCALATION_LEDGER="$LEDGER"
# Every hook resolves the sandbox engine as its own governed root.
export RICHOS_ENTITY_ROOT="$ENG"

# --- THE TEAMMATE'S WORKTREE — a real git repository on its own branch -------
# Not a directory pretending to be one. The claim under test is about a branch
# that is never merged, so there has to be a branch.
WT="$SANDBOX/agent-worktree"
mkdir -p "$WT/docs"
git init -q "$WT" 2>/dev/null
git -C "$WT" config user.email "t@example.invalid" 2>/dev/null
git -C "$WT" config user.name "Test" 2>/dev/null
git -C "$WT" checkout -q -b worktree-zach-opus-e1demo 2>/dev/null
printf 'seed\n' > "$WT/docs/seed.md"
git -C "$WT" add -A 2>/dev/null
git -C "$WT" commit -q -m "seed" 2>/dev/null

stop_payload() { # <session-id>
    printf '{"session_id":"%s","hook_event_name":"Stop","cwd":"%s","transcript_path":""}' \
        "$1" "$ENG"
}

# ===========================================================================
# 1. RAISE — one call, and everything mechanical is derived
# ===========================================================================
OUT="$("$ESCALATE" raise \
    --title "A premise in my brief is contradicted by the evidence" \
    --state work-complete --for ceo \
    --question "Do I follow the brief or the evidence? Only you can decide that." \
    --tried "read both records end to end and diffed them against the tree" \
    --meanwhile "finished every part of the row that does not depend on the answer" \
    --worktree "$WT" --teammate zach-opus-e1demo 2>&1)"
RC=$?
say "1 raise" "$OUT"
if [ "$RC" -eq 0 ]; then ok "1a  raise exits 0"; else bad "1a  raise exits 0" "rc=$RC: $OUT"; fi
ID="$(printf '%s\n' "$OUT" | sed -n 's/^escalation RAISED: //p' | head -1)"
case "$ID" in
    esc-*) ok "1b  it returns a derived id ($ID)" ;;
    *)     bad "1b  derived id" "got '$ID'" ;;
esac
if [ -f "$LEDGER" ] && grep -q '"event": "Escalation"' "$LEDGER"; then
    ok "1c  a ledger row was appended at $LEDGER"
else
    bad "1c  ledger row" "nothing at $LEDGER"
fi
# The fields nobody typed.
for f in '"teammate": "zach-opus-e1demo"' '"branch": "worktree-zach-opus-e1demo"' '"state": "work-complete"' '"for": "ceo"'; do
    if grep -qF "$f" "$LEDGER"; then ok "1d  derived from the workspace: $f"
    else bad "1d  derived field" "missing $f from $LEDGER"; fi
done
# THE ROOT IS CLOSED (ceo-decisions.md 27). The record goes under
# docs/verification/ and the worktree root must be untouched.
if [ -f "$WT/BLOCKED.md" ]; then
    bad "1e  no new root entry" "raise wrote BLOCKED.md at the repository root — the ruling this replaces"
else
    ok "1e  nothing was written at the repository root"
fi
REC="$(find "$WT/docs/verification/escalations" -name '*.md' 2>/dev/null | head -1)"
if [ -n "$REC" ]; then ok "1f  the record file went to docs/verification/escalations/"
else bad "1f  record file" "not found under $WT/docs/verification/escalations"; fi
# TWO RAISES IN ONE SECOND ARE TWO ESCALATIONS. The id's timestamp has one-second
# resolution, so the same teammate raising the same title twice inside one second
# (the disk watchdog replacing an acknowledged alarm does exactly that) must
# still get two ids, or the second is merged into the first. The clock is pinned
# here so the case cannot pass by straddling a second.
SAME_SEC="$(python3 - "$ENG/scripts/lib" "$WT" <<'PY' 2>&1
import sys, types
from datetime import datetime, timezone
sys.path.insert(0, sys.argv[1])
import escalations
when = datetime(2026, 10, 24, 18, 57, 2, tzinfo=timezone.utc)
args = types.SimpleNamespace(
    teammate="zach-opus-e1same", title="the same alarm, raised twice", worktree=sys.argv[2],
    branch="", repo="", head="", state="proceeding", audience="lead",
    question="q", tried="", meanwhile="", record="", session="s")
a = escalations.build_row(args, when)["id"]
b = escalations.build_row(args, when)["id"]
ok = a != b and all(escalations.ID_RX.fullmatch(i) for i in (a, b))
print(("DISTINCT " if ok else "SAME ") + a + " " + b)
PY
)"
case "$SAME_SEC" in
    DISTINCT*) ok "1g  two raises of the same escalation in the same second get two ids, both in the readable format (${SAME_SEC#DISTINCT })" ;;
    *)         bad "1g  same-second ids are distinct" "$SAME_SEC" ;;
esac

# ===========================================================================
# 2. THE STOP HOOK ANNOUNCES IT — same session, one turn later
# ===========================================================================
S1="aaaaaaaa-1111-2222-3333-444444444444"
OUT="$(stop_payload "$S1" | "$STOP_HOOK" 2>&1)"
say "2 stop hook" "$OUT"
case "$OUT" in
    *systemMessage*) ok "2a  the Stop hook emits on the channel measured to reach the operator" ;;
    *) bad "2a  systemMessage" "got: $OUT" ;;
esac
case "$OUT" in
    *"ESCALATION OUTSTANDING"*) ok "2b  it names the condition" ;;
    *) bad "2b  names the condition" "got: $OUT" ;;
esac
case "$OUT" in
    *zach-opus-e1demo*) ok "2c  it names WHO raised it" ;;
    *) bad "2c  names the teammate" "got: $OUT" ;;
esac
case "$OUT" in
    *"for the CEO"*) ok "2d  it says the answer belongs to the CEO" ;;
    *) bad "2d  names the audience" "got: $OUT" ;;
esac
# THE STALL DISTINCTION. Both originals said "this is not a stall"; a channel
# that reads every escalation as a failure teaches teammates not to use it.
case "$OUT" in
    *"none is a stall"*) ok "2e  it says explicitly that this is NOT a stall" ;;
    *) bad "2e  not-a-stall" "got: $OUT" ;;
esac

# ===========================================================================
# 3. THE INCIDENT, MECHANICALLY: the branch is never merged and the worktree
#    is DELETED. If the escalation still arrives, delivery does not depend on
#    a merge — which is the entire defect.
# ===========================================================================
MERGE_BASE="$SANDBOX/main-checkout"
git init -q "$MERGE_BASE" 2>/dev/null
rm -rf "$WT"
if [ -d "$WT" ]; then bad "3a  worktree removed" "still present"; else ok "3a  the teammate's worktree is GONE (branch never merged)"; fi

OUT="$("$ESCALATE" list 2>&1)"
RC=$?
say "3 list after deletion" "$OUT"
if [ "$RC" -eq 1 ]; then ok "3b  list still reports it outstanding (exit 1)"; else bad "3b  list exit" "rc=$RC"; fi
case "$OUT" in
    *"Do I follow the brief or the evidence"*)
        ok "3c  THE WHOLE QUESTION SURVIVED the worktree that raised it" ;;
    *)  bad "3c  question survives" "got: $OUT" ;;
esac
case "$OUT" in
    *"read both records end to end"*) ok "3d  what was already tried survived too" ;;
    *) bad "3d  tried survives" "got: $OUT" ;;
esac
OUT="$(stop_payload "$S1" | "$STOP_HOOK" 2>&1)"
say "3 stop hook after deletion" "$OUT"
# Same session, same state key, so silence here is CORRECT de-duplication —
# what matters is that the predicate still sees it, which case 3b just proved,
# and that a NEW session speaks (case 7).
if [ -z "$OUT" ]; then ok "3e  same session, unchanged state: it does not repeat itself"
else ok "3e  same session: emitted again (state changed) — $OUT"; fi

# ===========================================================================
# 4. THE SessionStart HALF — the escalation reaches the MODEL's context, in
#    full, with nothing merged and no worktree left to read.
# ===========================================================================
OUT="$("$START_HOOK" </dev/null 2>&1)"
say "4 session start" "$OUT"
CTX="$(printf '%s' "$OUT" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin)
    print(d.get("hookSpecificOutput",{}).get("additionalContext",""))
except Exception:
    print("")' 2>/dev/null)"
SYS="$(printf '%s' "$OUT" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("systemMessage",""))
except Exception:
    print("")' 2>/dev/null)"
case "$CTX" in
    *"Do I follow the brief or the evidence"*)
        ok "4a  the model's context carries the teammate's own question, verbatim" ;;
    *)  bad "4a  additionalContext" "got: $CTX" ;;
esac
case "$CTX" in
    *"$ID"*) ok "4b  it carries the id to acknowledge" ;;
    *) bad "4b  id in context" "got: $CTX" ;;
esac
case "$CTX" in
    *"NONE of these is a stall"*) ok "4c  and it says, to the model, that this is not a stall" ;;
    *) bad "4c  not-a-stall in context" "got: $CTX" ;;
esac
case "$CTX" in
    *"nothing has to be"*merged*) ok "4d  it states that nothing has to be merged to read it" ;;
    *) bad "4d  no-merge claim" "got: $CTX" ;;
esac
case "$SYS" in
    *"ESCALATION(S) OUTSTANDING"*) ok "4e  and the operator gets a line too — both channels, always" ;;
    *) bad "4e  systemMessage at SessionStart" "got: $SYS" ;;
esac
# THE FIRST LINE IS THE COUNT AND THE CEO'S SHARE (2026-09-25). 143 were
# outstanding, 13 of them for the CEO, and the CEO's number sat at the end of a
# block listing every one in full; a week of session starts read past it. So the
# assertion is on the FIRST line, not on "somewhere in the output".
CTX_FIRST="$(printf '%s\n' "$CTX" | head -1)"
case "$CTX_FIRST" in
    "1 ESCALATION(S) OUTSTANDING, 1 FOR THE CEO (for=ceo), "*)
        ok "4f  the model's FIRST line is the outstanding count and the count for the CEO" ;;
    *)  bad "4f  first line of the SessionStart context" "got: $CTX_FIRST" ;;
esac
case "$SYS" in
    "1 ESCALATION(S) OUTSTANDING, 1 FOR THE CEO — "*)
        ok "4g  the operator's line OPENS with the same two counts" ;;
    *)  bad "4g  operator line opens with the counts" "got: $SYS" ;;
esac

# ===========================================================================
# 5. THE NEGATIVE TWIN: an EMPTY ledger must produce SILENCE on both hooks.
#    Without this, cases 2 and 4 pass for a mechanism that shouts constantly.
# ===========================================================================
EMPTY="$SANDBOX/state/empty.jsonl"
OUT="$(RICHOS_ESCALATION_LEDGER="$EMPTY" "$START_HOOK" </dev/null 2>&1)"
if [ -z "$OUT" ]; then ok "5a  empty ledger: SessionStart says nothing"
else bad "5a  empty ledger silence" "got: $OUT"; fi
OUT="$(RICHOS_ESCALATION_LEDGER="$EMPTY" stop_payload "bbbbbbbb-0000-0000-0000-000000000000" | RICHOS_ESCALATION_LEDGER="$EMPTY" "$STOP_HOOK" 2>&1)"
if [ -z "$OUT" ]; then ok "5b  empty ledger: the Stop hook says nothing"
else bad "5b  empty ledger silence" "got: $OUT"; fi

# ===========================================================================
# 6. IT GETS LOUDER — the half that stops this becoming the two-day silence
#    with better paperwork. Backdate the row and the same session speaks again.
# ===========================================================================
python3 - "$LEDGER" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
p = sys.argv[1]
rows = [json.loads(l) for l in open(p, encoding="utf-8") if l.strip()]
old = (datetime.now(timezone.utc) - timedelta(hours=25)).replace(microsecond=0)
for r in rows:
    if r.get("event") == "Escalation":
        r["raised"] = old.isoformat().replace("+00:00", "Z")
with open(p, "w", encoding="utf-8") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")
PY
OUT="$(stop_payload "$S1" | "$STOP_HOOK" 2>&1)"
say "6 aged" "$OUT"
case "$OUT" in
    *"1d old"*)
        ok "6a  SAME SESSION, ALREADY ANNOUNCED, IT SPEAKS AGAIN because the age bucket crossed 24h" ;;
    *)  bad "6a  age bucket re-announce" "got: $OUT" ;;
esac
# And a further turn at the same age is quiet again — the loudness is a
# transition, not a nag. A hook that repeated every turn would be muted.
OUT2="$(stop_payload "$S1" | "$STOP_HOOK" 2>&1)"
if [ -z "$OUT2" ]; then ok "6b  and it goes quiet again at the same age — loudness is a transition, not a nag"
else bad "6b  quiet at the same age" "got: $OUT2"; fi

# ===========================================================================
# 7. A NEW SESSION RE-ANNOUNCES FROM SCRATCH. This is the case the incident
#    turned on: the session that could have surfaced them ENDED.
# ===========================================================================
S2="cccccccc-9999-8888-7777-666666666666"
OUT="$(stop_payload "$S2" | "$STOP_HOOK" 2>&1)"
say "7 new session" "$OUT"
case "$OUT" in
    *"ESCALATION OUTSTANDING"*) ok "7a  a NEW session is told about an escalation raised in an old one" ;;
    *) bad "7a  new session re-announce" "got: $OUT" ;;
esac

# ===========================================================================
# 8. THE ACK CLOSES IT — and only a real one does
# ===========================================================================
OUT="$("$ESCALATE" ack "$ID" --disposition "too short" 2>&1)"
RC=$?
if [ "$RC" -eq 2 ]; then ok "8a  a one-word disposition is REFUSED (a dismissal wearing a ledger row)"
else bad "8a  short disposition refused" "rc=$RC: $OUT"; fi
OUT="$("$ESCALATE" ack "esc-does-not-exist" --disposition "this id was never raised by anyone at all" 2>&1)"
RC=$?
if [ "$RC" -eq 2 ]; then ok "8b  acknowledging an id nobody raised is REFUSED"
else bad "8b  unknown id refused" "rc=$RC: $OUT"; fi
OUT="$("$ESCALATE" ack "$ID" --disposition "Put it to the CEO; he ruled the evidence wins and the brief was stale." 2>&1)"
RC=$?
if [ "$RC" -eq 0 ]; then ok "8c  a real ack is recorded"
else bad "8c  ack recorded" "rc=$RC: $OUT"; fi
"$ESCALATE" list >/dev/null 2>&1
RC=$?
if [ "$RC" -eq 0 ]; then ok "8d  and nothing is outstanding afterwards (exit 0)"
else bad "8d  list clean after ack" "rc=$RC"; fi
if grep -q '"event": "EscalationAck"' "$LEDGER" && grep -q '"event": "Escalation"' "$LEDGER"; then
    ok "8e  the ack APPENDED — the original row is still there, nothing was rewritten"
else
    bad "8e  append-only" "one of the two rows is missing from $LEDGER"
fi
OUT="$(stop_payload "$S2" | "$STOP_HOOK" 2>&1)"
case "$OUT" in
    *"clear again"*) ok "8f  the operator is told the story ended, having been told it started" ;;
    *) bad "8f  recovery line" "got: $OUT" ;;
esac
OUT="$("$START_HOOK" </dev/null 2>&1)"
if [ -z "$OUT" ]; then ok "8g  and SessionStart is silent again"
else bad "8g  silent after ack" "got: $OUT"; fi

# ===========================================================================
# 8h-8l. A HOLD CAN EXPIRE — failure type J, 2026-09-10
# ===========================================================================
# "A claim baked into a record with no condition that voids it will outlive its
# truth." The worked example is a lock reason ending "the agent is live": when
# that agent finished the sentence became false, nothing could void it, and it
# blocked the fix until a person removed it by hand. A disposition has the same
# shape — "held pending a decision that is the CEO's" closes an item forever,
# INCLUDING on the day after he decides.
OUT="$("$ESCALATE" raise --teammate zach-opus-j1 --title "a hold with a shelf life" \
    --state proceeding --question "does a hold reopen when its reason expires?" \
    --worktree "$SANDBOX" 2>&1)"
JID="$(python3 "$ENG/scripts/lib/escalations.py" list --format json 2>/dev/null | python3 -c '
import json,sys
o=[e for e in json.load(sys.stdin)["outstanding"] if e.get("teammate")=="zach-opus-j1"]
print(o[0]["id"] if o else "")' 2>/dev/null)"
if [ -n "$JID" ]; then
    OUT="$("$ESCALATE" ack "$JID" --disposition "Held pending a decision that is the CEO's own." \
        --until "when the CEO decides" 2>&1)"
    RC=$?
    if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "is not a date"; then
        ok "8h  --until refuses a free-text condition: an expiry nothing can evaluate never fires"
    else
        bad "8h  free-text --until refused" "rc=$RC: $OUT"
    fi
    OUT="$("$ESCALATE" ack "$JID" --disposition "Held pending a decision that is the CEO's own." \
        --until "2099-01-01" 2>&1)"
    RC=$?
    if [ "$RC" -eq 0 ] && "$ESCALATE" list >/dev/null 2>&1; then
        ok "8i  an --until in the future closes it exactly like an ordinary ack"
    else
        bad "8i  future --until closes" "rc=$RC: $OUT"
    fi
    # Backdate the expiry rather than sleeping: the ack carries until_epoch, so
    # rewriting it is the same fact a day later.
    python3 - "$LEDGER" "$JID" <<'PY'
import datetime, json, sys
path, jid = sys.argv[1], sys.argv[2]
rows = [json.loads(l) for l in open(path) if l.strip()]
past = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=1)
for r in rows:
    if r.get("event") == "EscalationAck" and str(r.get("id")) == jid:
        r["until"] = past.isoformat().replace("+00:00", "Z")
        r["until_epoch"] = past.timestamp()
open(path, "w").write("".join(json.dumps(r) + "\n" for r in rows))
PY
    REOPENED="$(python3 "$ENG/scripts/lib/escalations.py" list --format json 2>/dev/null | python3 -c '
import json,sys
o=[e for e in json.load(sys.stdin)["outstanding"] if e.get("teammate")=="zach-opus-j1"]
print("yes" if o and o[0].get("reopened_by_expiry") else "no")' 2>/dev/null)"
    if [ "$REOPENED" = "yes" ]; then
        ok "8j  once the date passes the item REOPENS, carrying the disposition that expired"
    else
        bad "8j  expiry reopens" "the item did not come back (reopened=$REOPENED)"
    fi
    # THE NEGATIVE CONTROL: an ack with no --until must still be permanent, or
    # 8j would pass because acks stopped closing anything at all.
    "$ESCALATE" ack "$JID" --disposition "Decided for good: this one is closed and stays closed." >/dev/null 2>&1
    if "$ESCALATE" list >/dev/null 2>&1; then
        ok "8k  an ack with NO --until is still permanent — nothing else changed"
    else
        bad "8k  plain ack still permanent" "the item is still outstanding after a plain ack"
    fi
    # A HOLD MAY BE CLOSED FOR GOOD before its date: a permanent ack replaces a
    # live timed one, so the hold cannot expire and come back. A second hold, or
    # an ack after a permanent one, is still refused.
    OUT="$("$ESCALATE" raise --teammate zach-opus-j2 --title "a hold closed for good" \
        --state proceeding --question "can a held item be closed permanently?" \
        --worktree "$SANDBOX" 2>&1)"
    J2="$(python3 "$ENG/scripts/lib/escalations.py" list --format json 2>/dev/null | python3 -c '
import json,sys
o=[e for e in json.load(sys.stdin)["outstanding"] if e.get("teammate")=="zach-opus-j2"]
print(o[0]["id"] if o else "")' 2>/dev/null)"
    "$ESCALATE" ack "$J2" --disposition "Held while the cause is diagnosed, for the test." --until "2099-01-01" >/dev/null 2>&1
    "$ESCALATE" ack "$J2" --disposition "Another hold on top of the first, for the test." --until "2099-02-01" >/dev/null 2>&1; RC_HOLD=$?
    "$ESCALATE" ack "$J2" --disposition "Recovered: the cause is gone, closed for good." >/dev/null 2>&1; RC_PERM=$?
    "$ESCALATE" ack "$J2" --disposition "A further acknowledgement after the close." >/dev/null 2>&1; RC_AGAIN=$?
    python3 - "$LEDGER" "$J2" <<'PY'
import datetime, json, sys
path, jid = sys.argv[1], sys.argv[2]
rows = [json.loads(l) for l in open(path) if l.strip()]
past = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=1)
for r in rows:
    if r.get("event") == "EscalationAck" and str(r.get("id")) == jid and r.get("until_epoch"):
        r["until"] = past.isoformat().replace("+00:00", "Z")
        r["until_epoch"] = past.timestamp()
open(path, "w").write("".join(json.dumps(r) + "\n" for r in rows))
PY
    BACK="$(python3 "$ENG/scripts/lib/escalations.py" list --format json 2>/dev/null | python3 -c '
import json,sys
print(len([e for e in json.load(sys.stdin)["outstanding"] if e.get("teammate")=="zach-opus-j2"]))' 2>/dev/null)"
    if [ -n "$J2" ] && [ "$RC_HOLD" = "1" ] && [ "$RC_PERM" = "0" ] && [ "$RC_AGAIN" = "1" ] && [ "$BACK" = "0" ]; then
        ok "8l  a live hold can be closed for good, and then never comes back; holds and repeats are refused"
    else
        bad "8l  closing a hold for good" "id=$J2 hold=$RC_HOLD perm=$RC_PERM again=$RC_AGAIN outstanding=$BACK"
    fi
else
    bad "8h  --until refuses a free-text condition" "fixture: could not raise the type-J escalation"
    bad "8i  future --until closes" "fixture failed"
    bad "8j  expiry reopens" "fixture failed"
    bad "8k  plain ack still permanent" "fixture failed"
fi

# ===========================================================================
# 9. A RAISE THAT DID NOT LAND SAYS SO — the failure mode that would rebuild
#    the whole defect quietly.
# ===========================================================================
UNWRITABLE="$SANDBOX/readonly/escalations.jsonl"
mkdir -p "$SANDBOX/readonly"
chmod 500 "$SANDBOX/readonly"
OUT="$(RICHOS_ESCALATION_LEDGER="$UNWRITABLE" "$ESCALATE" raise \
        --title "this one cannot be written" --state proceeding \
        --question "does a failed raise tell the teammate it failed?" \
        --worktree "$SANDBOX" --teammate zach-opus-e1demo 2>&1)"
RC=$?
chmod 700 "$SANDBOX/readonly"
say "9 unwritable" "$OUT"
if [ "$RC" -ne 0 ]; then ok "9a  a raise that could not be written exits non-zero"
else bad "9a  failed raise exits non-zero" "rc=$RC: $OUT"; fi
case "$OUT" in
    *"HAS NOT BEEN DELIVERED"*) ok "9b  and it says so in those words, with what to do instead" ;;
    *) bad "9b  says not delivered" "got: $OUT" ;;
esac

# ===========================================================================
# 10. ARGUMENT VALIDATION — the two fields that keep the channel usable
# ===========================================================================
OUT="$("$ESCALATE" raise --title "no state given" \
        --question "is the state field really required here?" \
        --worktree "$SANDBOX" 2>&1)"
RC=$?
if [ "$RC" -eq 2 ]; then ok "10a --state is required"; else bad "10a --state required" "rc=$RC"; fi
case "$OUT" in
    *"separates an escalation from a stall"*) ok "10b and the refusal explains WHY it is required" ;;
    *) bad "10b refusal explains" "got: $OUT" ;;
esac
OUT="$("$ESCALATE" raise --title "no question" --state stopped --question "eh?" \
        --worktree "$SANDBOX" 2>&1)"
RC=$?
if [ "$RC" -eq 2 ]; then ok "10c a record with no real question is refused — nothing could ever close it"
else bad "10c question required" "rc=$RC: $OUT"; fi

# ===========================================================================
# 11. A STOPPED ESCALATION READS DIFFERENTLY FROM A COMPLETE ONE.
#     The whole point of the `state` field, asserted rather than assumed.
# ===========================================================================
L2="$SANDBOX/state/stopped.jsonl"
OUT="$(RICHOS_ESCALATION_LEDGER="$L2" "$ESCALATE" raise \
        --title "the whole task depends on this answer" --state stopped \
        --question "which of the two records governs? I cannot proceed either way." \
        --worktree "$SANDBOX" --teammate zach-opus-e1stopped 2>&1)"
OUT="$(RICHOS_ESCALATION_LEDGER="$L2" "$ESCALATE" list --format hook-summary 2>&1)"
say "11 stopped" "$OUT"
case "$OUT" in
    *"has STOPPED work"*) ok "11a a stopped escalation is reported as stopped" ;;
    *) bad "11a stopped reported" "got: $OUT" ;;
esac
case "$OUT" in
    *"none is a stall"*) bad "11b stall wording" "a STOPPED escalation was described as not a stall: $OUT" ;;
    *) ok "11b and the not-a-stall wording is correctly absent for it" ;;
esac

# ===========================================================================
# 12. NEGATIVE CONTROL — prove cases 2, 4 and 7 can fail.
#
# A green tick means nothing if the assertion cannot go red. So the historical
# defect is rebuilt on purpose: the predicate is replaced by one that reports
# "clear" over an outstanding escalation, exactly as a mechanism that writes a
# file nobody reads does. Both hooks must go silent — and if they do NOT, then
# their earlier output was not coming from the ledger at all.
# ===========================================================================
cp "$ENG/scripts/lib/escalations.py" "$SANDBOX/escalations.py.real"
cat > "$ENG/scripts/lib/escalations.py" <<'STUB'
#!/usr/bin/env python3
# MUTATION (test-only): the pre-fix world — an escalation exists, and the thing
# that is supposed to surface it reports a clean sheet.
import sys
if "hook-summary" in sys.argv:
    sys.stdout.write("clear\n0\n")
sys.exit(0)
STUB
L3="$SANDBOX/state/control.jsonl"
cp "$SANDBOX/escalations.py.real" "$SANDBOX/real.py"
# This one calls the module directly, so it writes no record file at all —
# escalate.sh is what writes records. --worktree is here anyway so the ledger
# row describes the SANDBOX rather than whichever checkout the suite was
# started in: a row that quietly records a live worktree, branch and HEAD is a
# fixture wearing a real agent's identity, which is how the 2026-09-05 file
# fooled its reader in the first place.
RICHOS_ESCALATION_LEDGER="$L3" python3 "$SANDBOX/real.py" raise --title "the control escalation" \
    --state proceeding --question "does the negative control actually go silent?" \
    --worktree "$SANDBOX" --teammate zach-opus-e1control >/dev/null 2>&1
if [ -s "$L3" ]; then ok "12a  the control escalation really is in the ledger"
else bad "12a  control fixture" "nothing written to $L3"; fi
OUT="$(RICHOS_ESCALATION_LEDGER="$L3" stop_payload "dddddddd-0000-0000-0000-000000000000" | RICHOS_ESCALATION_LEDGER="$L3" "$STOP_HOOK" 2>&1)"
if [ -z "$OUT" ]; then
    ok "12b  NEGATIVE CONTROL: with a predicate that reports 'clear', the Stop hook is SILENT over a live escalation — the historical defect, reproduced"
else
    bad "12b  NEGATIVE CONTROL" "the stub predicate still produced output, so cases 2/6/7 are not proving the ledger is being read: $OUT"
fi
OUT="$(RICHOS_ESCALATION_LEDGER="$L3" "$START_HOOK" </dev/null 2>&1)"
if [ -z "$OUT" ]; then
    ok "12c  NEGATIVE CONTROL: SessionStart is silent too"
else
    bad "12c  NEGATIVE CONTROL SessionStart" "got: $OUT"
fi
cp "$SANDBOX/escalations.py.real" "$ENG/scripts/lib/escalations.py"
OUT="$(RICHOS_ESCALATION_LEDGER="$L3" stop_payload "eeeeeeee-0000-0000-0000-000000000000" | RICHOS_ESCALATION_LEDGER="$L3" "$STOP_HOOK" 2>&1)"
case "$OUT" in
    *"ESCALATION OUTSTANDING"*) ok "12d  and with the real predicate restored, the same fixture is announced — the control was the mutation, not the fixture" ;;
    *) bad "12d  restored predicate" "got: $OUT" ;;
esac

# ===========================================================================
# 13. A MISSING PREDICATE IS ANNOUNCED, NEVER SWALLOWED.
#     Layer K's lesson: a hook that is wired, hashed, executable and reading
#     nothing looks exactly like a hook with nothing to report.
# ===========================================================================
mv "$ENG/scripts/lib/escalations.py" "$SANDBOX/escalations.py.hidden"
OUT="$(RICHOS_ESCALATION_LEDGER="$L3" stop_payload "ffffffff-0000-0000-0000-000000000000" | RICHOS_ESCALATION_LEDGER="$L3" "$STOP_HOOK" 2>&1)"
case "$OUT" in
    *"ESCALATION WATCH IS OFF"*) ok "13a  a missing predicate is ANNOUNCED, not swallowed" ;;
    *) bad "13a  missing predicate announced" "got: $OUT" ;;
esac
OUT="$("$START_HOOK" </dev/null 2>&1)"
case "$OUT" in
    *"ESCALATION WATCH IS OFF"*) ok "13b  and SessionStart says the same rather than opening quietly" ;;
    *) bad "13b  SessionStart announces" "got: $OUT" ;;
esac
mv "$SANDBOX/escalations.py.hidden" "$ENG/scripts/lib/escalations.py"

# ===========================================================================
# 14. AN UNREADABLE LINE IS COUNTED, NOT DISCARDED. A half-corrupt ledger must
#     never report as a clean empty world.
# ===========================================================================
L4="$SANDBOX/state/corrupt.jsonl"
printf 'this is not json\n' > "$L4"
# ITS OWN WORKTREE, AND THE REASON IS THE 2026-09-05 LEAK. This case is about a
# half-corrupt LEDGER and has no opinion about where the record file goes — but
# `raise` writes TWO things, and only one of them is the ledger. The record's
# path comes from --worktree, which DEFAULTS TO THE CURRENT DIRECTORY, so this
# line wrote a real-looking escalation into whatever live checkout the suite
# happened to be started from. One was found as an untracked file in a working
# engineer's worktree, and he could not tell it from a genuine one.
WT14="$SANDBOX/agent-worktree-corrupt"
mkdir -p "$WT14/docs"
RICHOS_ESCALATION_LEDGER="$L4" "$ESCALATE" raise --title "a good row beside a bad one" \
    --state proceeding --question "is the malformed line reported to anyone?" \
    --worktree "$WT14" --teammate zach-opus-e1corrupt >/dev/null 2>&1
OUT="$(RICHOS_ESCALATION_LEDGER="$L4" "$ESCALATE" list 2>&1)"
case "$OUT" in
    *"malformed line"*) ok "14a  a malformed ledger line is reported, with a count" ;;
    *) bad "14a  malformed reported" "got: $OUT" ;;
esac
case "$OUT" in
    *"1 raised in all, 1 OUTSTANDING"*) ok "14b  and the good row beside it is still read" ;;
    *) bad "14b  good row survives" "got: $OUT" ;;
esac
# CONTAINED, NOT SUPPRESSED. The cheap way to stop this case leaking is
# --no-record, and it would be wrong: the record write is a code path this
# `raise` should still take, and a suppressed write proves nothing about where
# an unsuppressed one goes. So the record must EXIST, inside the sandbox.
if find "$WT14/docs/verification/escalations" -name '*.md' 2>/dev/null | grep -q .; then
    ok "14c  and its record file landed in the SANDBOX worktree — the write still happened, it just no longer follows \$PWD"
else
    bad "14c  record contained, not suppressed" "no record under $WT14; if --no-record was added here instead, put it back and pass --worktree"
fi

# ===========================================================================
# 15. A REPOSITORY WITH NO docs/ GETS THE LEDGER ROW AND NO NEW ROOT ENTRY.
#     ceo-decisions.md 27 is the reason, and it is not negotiable.
# ===========================================================================
NODOCS="$SANDBOX/nodocs"
mkdir -p "$NODOCS"
BEFORE="$(ls -A "$NODOCS" | wc -l | tr -d ' ')"
L5="$SANDBOX/state/nodocs.jsonl"
OUT="$(RICHOS_ESCALATION_LEDGER="$L5" "$ESCALATE" raise --title "no docs directory here" \
        --state proceeding --question "does it invent a root entry when there is nowhere to put a file?" \
        --worktree "$NODOCS" --teammate zach-opus-e1nodocs 2>&1)"
RC=$?
AFTER="$(ls -A "$NODOCS" | wc -l | tr -d ' ')"
say "15 nodocs" "$OUT"
if [ "$RC" -eq 0 ]; then ok "15a raise still succeeds without a docs/ directory"; else bad "15a raise succeeds" "rc=$RC: $OUT"; fi
if [ "$BEFORE" = "$AFTER" ]; then ok "15b and it added NOTHING to the repository root ($BEFORE entries before and after)"
else bad "15b root untouched" "root went from $BEFORE to $AFTER entries"; fi
if [ -s "$L5" ]; then ok "15c the escalation was still delivered"; else bad "15c delivered" "nothing in $L5"; fi
case "$OUT" in
    *"no record file was written"*) ok "15d and it says why, rather than failing silently" ;;
    *) bad "15d explains" "got: $OUT" ;;
esac

# ===========================================================================
# 18. FREE TEXT FROM A FILE — ANY WORDING ARRIVES, AND NONE OF IT IS ON THE
#     COMMAND LINE. (Numbered 18 but run here, so case 17's canary watches it.)
#
# Measured 2026-09-28 on Claude Code 2.1.283: for a worktree-isolated agent the
# host refuses, before escalate.sh ever runs, a command whose free-text argument
# starts with the word git or carries an apostrophe next to it. The teammate
# esctest1 was refused raising "--tried 'git status, git add, git commit ...'".
# That check cannot be reached from a suite, so this proves the other half: the
# SAME wording, written to a file, arrives byte for byte, and the command line
# that carries it holds nothing but a path.
# ===========================================================================
L18="$SANDBOX/state/fields.jsonl"
# Its own worktree: case 3 deleted $WT on purpose (the unmerged, removed branch).
WT18="$SANDBOX/wt18"
mkdir -p "$WT18/docs"
F18="$SANDBOX/esc18.json"
cat > "$F18" <<'JSON'
{"title": "git commit refused in the richos-hq workspace (can't run git)",
 "state": "proceeding",
 "for": "lead",
 "question": "git status works; why can't I run \"git commit\" there? It's a digit-free $HOME `question`\nwith a second line.",
 "tried": "git status, git add, git commit in the richos-hq workspace; sh -c 'git log'",
 "meanwhile": "git-free work only: we're writing the tests"}
JSON
OUT="$(RICHOS_ESCALATION_LEDGER="$L18" "$ESCALATE" raise --fields "$F18" \
        --worktree "$WT18" --teammate zach-opus-e1fields 2>&1)"
RC=$?
say "18 fields raise" "$OUT"
if [ "$RC" -eq 0 ]; then ok "18a raise --fields with git wording, quotes, \$, a backtick and a newline in every field exits 0"
else bad "18a raise --fields" "rc=$RC: $OUT"; fi
ID18="$(printf '%s\n' "$OUT" | sed -n 's/^escalation RAISED: //p' | head -1)"
case "$ID18" in
    esc-*) ok "18b and it returns an id ($ID18)" ;;
    *)     bad "18b id" "got '$ID18'" ;;
esac
# Byte for byte: every field in the ledger row equals the file's value.
CHECK18="$(python3 - "$F18" "$L18" <<'PY'
import json, sys
want = json.load(open(sys.argv[1]))
rows = [json.loads(l) for l in open(sys.argv[2]) if l.strip()]
row = [r for r in rows if r.get("event") == "Escalation"][-1]
bad = [k for k in ("title", "state", "for", "question", "tried", "meanwhile") if row.get(k) != want[k]]
print("MISMATCH " + " ".join(bad) if bad else "EXACT")
PY
)"
if [ "$CHECK18" = "EXACT" ]; then ok "18c the ledger row carries all six fields exactly as written in the file"
else bad "18c fields arrive verbatim" "$CHECK18 (ledger $L18)"; fi
REC18="$(find "$WT18/docs/verification/escalations" -name '*zach-opus-e1fields*' 2>/dev/null | head -1)"
if [ -n "$REC18" ] && grep -qF "git status, git add, git commit in the richos-hq workspace" "$REC18"; then
    ok "18d the record file carries the same wording"
else bad "18d record wording" "record '$REC18' lacks the tried text"; fi

rows18() { [ -f "$L18" ] && grep -c . "$L18" || printf '0'; }
N18="$(rows18)"
# A field from both places is refused: which one wins would be a guess.
OUT="$(RICHOS_ESCALATION_LEDGER="$L18" "$ESCALATE" raise --fields "$F18" --state stopped \
        --worktree "$WT18" --teammate zach-opus-e1fields 2>&1)"
RC=$?
if [ "$RC" -eq 2 ] && [ "$(rows18)" = "$N18" ] && printf '%s' "$OUT" | grep -q "given both"; then
    ok "18e a field given in the file AND as an option is refused (exit 2), nothing appended"
else bad "18e both places refused" "rc=$RC rows $N18 -> $(rows18): $OUT"; fi
# Unknown key, non-string value, not JSON, no such file: each refused, none appends.
printf '{"title": "t", "state": "proceeding", "question": "q", "questions": "typo"}' > "$SANDBOX/esc18-unknown.json"
printf '{"title": "t", "state": "proceeding", "question": ["q"]}' > "$SANDBOX/esc18-list.json"
printf 'title: not json\n' > "$SANDBOX/esc18-text.json"
for pair in "esc18-unknown.json:unknown field 'questions'" "esc18-list.json:must be a string" \
            "esc18-text.json:not a JSON object" "esc18-missing.json:cannot read"; do
    f="${pair%%:*}"; want="${pair#*:}"
    OUT="$(RICHOS_ESCALATION_LEDGER="$L18" "$ESCALATE" raise --fields "$SANDBOX/$f" \
            --worktree "$WT18" --teammate zach-opus-e1fields 2>&1)"
    RC=$?
    if [ "$RC" -eq 2 ] && [ "$(rows18)" = "$N18" ] && printf '%s' "$OUT" | grep -qF "$want"; then
        ok "18f $f is refused with '$want', nothing appended"
    else bad "18f $f refused" "rc=$RC rows $N18 -> $(rows18): $OUT"; fi
done
# The values are data. A value built to break out of shell quoting runs nothing.
P18="$SANDBOX/pwned18"
python3 - "$SANDBOX/esc18-inject.json" "$P18" <<'PY'
import json, sys
p = sys.argv[2]
json.dump({"title": "inject '; touch %s; '" % p, "state": "proceeding",
           "question": "$(touch %s) `touch %s`" % (p, p), "tried": "\"; touch %s; \"" % p},
          open(sys.argv[1], "w"))
PY
OUT="$(RICHOS_ESCALATION_LEDGER="$L18" "$ESCALATE" raise --fields "$SANDBOX/esc18-inject.json" \
        --worktree "$WT18" --teammate zach-opus-e1fields --no-record 2>&1)"
RC=$?
if [ "$RC" -eq 0 ] && [ ! -e "$P18" ]; then
    ok "18g quote-breaking and substitution text in a field is stored, never run"
else bad "18g values are data" "rc=$RC, $P18 exists: $([ -e "$P18" ] && echo yes || echo no): $OUT"; fi
# The lead's half takes a file too.
printf '{"disposition": "answered: git commit works from the plain command; we can'"'"'t fix the host"}' > "$SANDBOX/esc18-ack.json"
OUT="$(RICHOS_ESCALATION_LEDGER="$L18" "$ESCALATE" ack "$ID18" --fields "$SANDBOX/esc18-ack.json" 2>&1)"
RC=$?
if [ "$RC" -eq 0 ] && grep -qF "we can't fix the host" "$L18"; then
    ok "18h ack --fields records the disposition verbatim"
else bad "18h ack --fields" "rc=$RC: $OUT"; fi
# A teammate who reads the help finds the file form first.
OUT="$("$ESCALATE" --help 2>&1)"
case "$OUT" in
    *"raise --fields"*"Write tool"*|*"WRITE TOOL"*"raise --fields"*) ok "18i --help teaches the file form and why" ;;
    *) bad "18i help teaches --fields" "help text does not show 'raise --fields' with the Write tool" ;;
esac

# ===========================================================================
# 19. needs: ceo-hands — WHOSE HANDS, which `for` cannot say (2026-10-01).
#     (Numbered 19 but run here, so case 17's canary watches it.)
#
# On 2026-10-01 a teammate's question that needed the CEO at the test iPhone
# was `for: lead`, rightly: the lead fetches him. `for` names whose DECISION it
# is; nothing named whose HANDS. The stall watcher tells a `needs: ceo-hands`
# escalation first and every 10 minutes, so the field must arrive exactly, and
# any other value must be refused rather than stored as a word nobody reads.
# ===========================================================================
L19="$SANDBOX/state/needs.jsonl"
WT19="$SANDBOX/wt19"
mkdir -p "$WT19/docs"
rows19() { [ -f "$L19" ] && grep -c . "$L19" || printf '0'; }
printf '%s' '{"title": "the test iPhone will ask for Touch ID at the phone", "state": "proceeding", "for": "lead", "needs": "ceo-hands", "question": "Can the CEO be at the test iPhone within the next ten minutes?"}' > "$SANDBOX/esc19.json"
OUT="$(RICHOS_ESCALATION_LEDGER="$L19" "$ESCALATE" raise --fields "$SANDBOX/esc19.json" \
        --worktree "$WT19" --teammate zach-opus-e1needs --no-record 2>&1)"
RC=$?
NEEDS19="$(python3 -c 'import json,sys; r=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]; print(r[-1].get("needs",""))' "$L19" 2>/dev/null)"
if [ "$RC" -eq 0 ] && [ "$NEEDS19" = "ceo-hands" ]; then
    ok "19a a fields file with \"needs\": \"ceo-hands\" raises, and the row carries needs=ceo-hands"
else bad "19a needs from the fields file" "rc=$RC needs='$NEEDS19': $OUT"; fi
N19="$(rows19)"
printf '%s' '{"title": "t19b", "state": "proceeding", "needs": "ceo", "question": "a question long enough to pass the floor"}' > "$SANDBOX/esc19-bad.json"
OUT="$(RICHOS_ESCALATION_LEDGER="$L19" "$ESCALATE" raise --fields "$SANDBOX/esc19-bad.json" \
        --worktree "$WT19" --teammate zach-opus-e1needs --no-record 2>&1)"
RC=$?
if [ "$RC" -eq 2 ] && [ "$(rows19)" = "$N19" ] && printf '%s' "$OUT" | grep -qF "needs must be"; then
    ok "19b any other needs value is refused (exit 2), nothing appended"
else bad "19b needs vocabulary" "rc=$RC rows $N19 -> $(rows19): $OUT"; fi
OUT="$(RICHOS_ESCALATION_LEDGER="$L19" "$ESCALATE" raise --title "t19c by option" --state stopped \
        --question "a question long enough to pass the floor" --needs ceo-hands \
        --worktree "$WT19" --teammate zach-opus-e1needs --no-record 2>&1)"
RC=$?
NEEDS19="$(python3 -c 'import json,sys; r=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]; print(r[-1].get("needs",""), r[-1].get("title",""))' "$L19" 2>/dev/null)"
if [ "$RC" -eq 0 ] && [ "$NEEDS19" = "ceo-hands t19c by option" ]; then
    ok "19c --needs ceo-hands on the command line does the same"
else bad "19c --needs option" "rc=$RC got '$NEEDS19': $OUT"; fi
CTX19="$(RICHOS_ESCALATION_LEDGER="$L19" python3 "$ENG/scripts/lib/escalations.py" list --format session-context 2>&1)"
if printf '%s' "$CTX19" | grep -qF "NEEDS THE CEO AT A DEVICE"; then
    ok "19d the lead's model reads it: the entry says NEEDS THE CEO AT A DEVICE"
else bad "19d needs in model_entry" "$CTX19"; fi
printf '%s' '{"title": "t19e", "state": "proceeding", "question": "a question long enough to pass the floor"}' > "$SANDBOX/esc19-none.json"
OUT="$(RICHOS_ESCALATION_LEDGER="$L19" "$ESCALATE" raise --fields "$SANDBOX/esc19-none.json" \
        --worktree "$WT19" --teammate zach-opus-e1needs --no-record 2>&1)"
RC=$?
HAS19="$(python3 -c 'import json,sys; r=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]; print("needs" in r[-1])' "$L19" 2>/dev/null)"
if [ "$RC" -eq 0 ] && [ "$HAS19" = "False" ]; then
    ok "19e without needs nothing changes: the row carries no needs key"
else bad "19e no needs" "rc=$RC has_needs=$HAS19: $OUT"; fi

# ===========================================================================
# 20. THE DELIVERY UNIT CASES RUN HERE (2026-10-01).
#     scripts/lib/escalations-delivery.test.py holds the cases that need host
#     transcript rows (P5-37's host cap; D01-D07, what the stall watcher's
#     wake already put in front of the model). No suite named it, so none of
#     them ran in any build: proof-for.sh refused the change that added D01
#     with "named by NO suite". It writes only under its own temporary
#     directories, and it runs inside case 17's canary window.
# ===========================================================================
OUT="$(python3 "$SRC_DIR/../lib/escalations-delivery.test.py" 2>&1)"
RC=$?
say "20 escalations-delivery.test.py" "$OUT"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q '^OK'; then
    ok "20  escalations-delivery.test.py: $(printf '%s\n' "$OUT" | grep -m1 '^Ran ')"
else bad "20  escalations-delivery.test.py" "rc=$RC: $(printf '%s' "$OUT" | tail -20)"; fi

# ===========================================================================
# 16. SOURCE MUTATION — is the LOUDNESS carried by the age buckets, or by luck?
#
# Case 6 shows an aged escalation speaking again. That is worth nothing until
# somebody has watched it NOT speak for the right reason, and the right reason
# has to be found by breaking the SHIPPED SOURCE rather than the fixture.
#
# So AGE_BUCKETS is collapsed to a single bucket — every age reads as "new" —
# which is exactly the mechanism this engine would have had if the notice were
# de-duplicated on the id alone. An escalation then ages a whole day in silence,
# which is the 2026-09-02 incident reproduced inside the suite that always runs.
#
# This is deliberately NOT a *.mutation.sh. Of the thirteen mutation harnesses
# this engine has written, eight were run by nothing at all — a proof nobody
# executes is a paragraph. The mutation lives in the suite the runner
# discovers, so it is re-derived on every run rather than on request.
# ===========================================================================
L6="$SANDBOX/state/loud.jsonl"
age_l6() { # <minutes>
    python3 - "$L6" "$1" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
p, mins = sys.argv[1], int(sys.argv[2])
rows = [json.loads(l) for l in open(p, encoding="utf-8") if l.strip()]
when = (datetime.now(timezone.utc) - timedelta(minutes=mins)).replace(microsecond=0)
for r in rows:
    if r.get("event") == "Escalation":
        r["raised"] = when.isoformat().replace("+00:00", "Z")
open(p, "w", encoding="utf-8").write("".join(json.dumps(r) + "\n" for r in rows))
PY
}
# --no-record because this case only needs a ledger row to age, AND
# --worktree because --no-record is one edit away from being dropped by
# somebody who needs the record here. The belt is the argument that makes the
# write land in the sandbox; the braces are the argument that suppresses it.
RICHOS_ESCALATION_LEDGER="$L6" "$ESCALATE" raise --title "nobody has looked at this yet" \
    --state proceeding --question "is the loudness carried by the age buckets?" \
    --worktree "$SANDBOX" --teammate zach-opus-e1loud --no-record >/dev/null 2>&1
cp "$ENG/scripts/lib/escalations.py" "$SANDBOX/escalations.py.real2"
python3 - "$ENG/scripts/lib/escalations.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
# MUTATION (test-only): one bucket, so age is never a state change.
s = s.replace('AGE_BUCKETS = ((72 * 60, "72h"), (24 * 60, "24h"), (60, "1h"), (0, "new"))',
              'AGE_BUCKETS = ((0, "new"),)')
open(p, "w", encoding="utf-8").write(s)
PY
if grep -q 'AGE_BUCKETS = ((0, "new"),)' "$ENG/scripts/lib/escalations.py"; then
    ok "16a  the mutation APPLIED — a replacement that matched nothing would be a green run that looks green"
else
    bad "16a  mutation applied" "AGE_BUCKETS was not collapsed; every assertion below is examining the unmutated source"
fi
S6="12345678-0000-0000-0000-000000000000"
OUT="$(RICHOS_ESCALATION_LEDGER="$L6" stop_payload "$S6" | RICHOS_ESCALATION_LEDGER="$L6" "$STOP_HOOK" 2>&1)"
case "$OUT" in
    *"ESCALATION OUTSTANDING"*) ok "16b  under the mutation it is still announced ONCE, so the mutation did not simply break the hook" ;;
    *) bad "16b  mutant still announces once" "got: $OUT" ;;
esac
age_l6 1500
OUT="$(RICHOS_ESCALATION_LEDGER="$L6" stop_payload "$S6" | RICHOS_ESCALATION_LEDGER="$L6" "$STOP_HOOK" 2>&1)"
if [ -z "$OUT" ]; then
    ok "16c  THE INCIDENT, REPRODUCED: with one bucket the escalation ages a full DAY and the operator is told nothing"
else
    bad "16c  mutant goes silent as it ages" "the single-bucket mutant still spoke, so case 6a is not proving the buckets carry the loudness: $OUT"
fi
cp "$SANDBOX/escalations.py.real2" "$ENG/scripts/lib/escalations.py"
OUT="$(RICHOS_ESCALATION_LEDGER="$L6" stop_payload "$S6" | RICHOS_ESCALATION_LEDGER="$L6" "$STOP_HOOK" 2>&1)"
case "$OUT" in
    *"1d old"*) ok "16d  and the shipped source, on the same ledger and the same session, speaks — the buckets are the mechanism" ;;
    *) bad "16d  restored source speaks" "got: $OUT" ;;
esac

# ===========================================================================
# 18. THE LEAD'S MODEL IS TOLD — the 2026-09-28 defect.
#
# Every case above asserts on `systemMessage`, and a Stop hook's systemMessage
# reaches the PERSON, never the model. So all of them were green on 2026-09-28
# while a teammate's escalation sat unread by the lead for the rest of the
# session: the hook fired four times and the model received nothing. These
# cases assert on the field that reaches the model —
# hookSpecificOutput.additionalContext — and on WHICH escalations it carries:
# the new one by id, teammate, title, state and question; never one the
# session-start block already delivered; never the same one twice.
# ===========================================================================
L8="$SANDBOX/state/deliver.jsonl"
S8="18181818-0000-4000-8000-000000000000"
T8="$SANDBOX/transcript-18.jsonl"

# jfield <json> <expr> — one field of the hook's JSON, or "" when absent.
jfield() {
    printf '%s' "$1" | python3 -c '
import json, sys
expr = sys.argv[1]
try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    print("<NOT JSON>"); raise SystemExit
for k in expr.split("."):
    d = d.get(k) if isinstance(d, dict) else None
print("" if d is None else d)
' "$2"
}
stop_payload_t() { # <session-id> <transcript> [agent-id]
    python3 -c '
import json, sys
d = {"session_id": sys.argv[1], "hook_event_name": "Stop", "cwd": sys.argv[3],
     "transcript_path": sys.argv[2], "stop_hook_active": False,
     "last_assistant_message": "Done."}
if len(sys.argv) > 4 and sys.argv[4]:
    d["agent_id"] = sys.argv[4]
print(json.dumps(d))
' "$1" "$2" "$ENG" "${3:-}"
}
# The host's own record of the SessionStart block, in the shape the lead's
# transcript carries it (measured on 356e9552, line 15): an attachment whose
# stdout is session-start-escalations.sh's JSON, byte for byte.
record_session_start() { # <transcript> <hook-stdout>
    python3 -c '
import json, sys
row = {"type": "attachment", "timestamp": "2026-09-28T13:44:26.236Z",
       "attachment": {"type": "hook_success", "hookName": "SessionStart:startup",
                      "hookEvent": "SessionStart", "content": "", "stdout": sys.argv[2],
                      "stderr": "", "exitCode": 0,
                      "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/session-start-escalations.sh"}}
with open(sys.argv[1], "a", encoding="utf-8") as fh:
    fh.write(json.dumps(row) + "\n")
' "$1" "$2"
}
raise8() { # <teammate> <title> [ledger]
    RICHOS_ESCALATION_LEDGER="${3:-$L8}" "$ESCALATE" raise --title "$2" --state proceeding --for lead \
        --question "is this delivered to the lead's model by id?" \
        --worktree "$SANDBOX" --teammate "$1" --no-record 2>/dev/null \
        | sed -n 's/^escalation RAISED: //p' | head -1
}

# An escalation that was outstanding when the session started, delivered by
# the SessionStart block and recorded in the transcript the way the host does.
OLD8="$(raise8 zach-opus-e1before "raised before this session started")"
SS_OUT="$(RICHOS_ESCALATION_LEDGER="$L8" "$START_HOOK" </dev/null 2>/dev/null)"
: > "$T8"
record_session_start "$T8" "$SS_OUT"
case "$SS_OUT" in
    *"$OLD8"*) ok "18a  fixture: the SessionStart block really carries $OLD8, and the transcript records it" ;;
    *)         bad "18a  SessionStart fixture" "the session-start output does not name $OLD8: $SS_OUT" ;;
esac

# Mid-session, a teammate raises a NEW one.
sleep 1
NEW8="$(raise8 zach-opus-e1during "raised during the lead's session")"
OUT="$(stop_payload_t "$S8" "$T8" | RICHOS_ESCALATION_LEDGER="$L8" "$STOP_HOOK" 2>/dev/null)"
say "18 first turn end" "$OUT"
CTX="$(jfield "$OUT" hookSpecificOutput.additionalContext)"
EVT="$(jfield "$OUT" hookSpecificOutput.hookEventName)"
if [ "$EVT" = "Stop" ] && [ -n "$CTX" ]; then
    ok "18b  the turn end carries hookSpecificOutput.additionalContext for event Stop — the field that reaches the MODEL"
else
    bad "18b  model channel present" "hookEventName='$EVT', additionalContext empty=$([ -z "$CTX" ] && echo yes || echo no): $OUT"
fi
MISSING=""
for want in "$NEW8" "zach-opus-e1during" "raised during the lead's session" "state=proceeding" \
            "is this delivered to the lead's model by id?"; do
    case "$CTX" in *"$want"*) ;; *) MISSING="$MISSING [$want]" ;; esac
done
if [ -z "$MISSING" ] && [ -n "$NEW8" ]; then
    ok "18c  it names the NEW escalation by id, teammate, title, state and question"
else
    bad "18c  names the new escalation" "missing:$MISSING in: $CTX"
fi
case "$CTX" in
    *"$OLD8"*) bad "18d  not repeated from session start" "$OLD8 was already delivered by the SessionStart block and was sent again: $CTX" ;;
    *)         ok "18d  the one the SessionStart block already delivered is NOT repeated" ;;
esac
case "$(jfield "$OUT" systemMessage)" in
    *"ESCALATION OUTSTANDING"*) ok "18e  and the operator's line is still in the same JSON object" ;;
    *) bad "18e  operator line kept" "got: $OUT" ;;
esac

OUT="$(stop_payload_t "$S8" "$T8" | RICHOS_ESCALATION_LEDGER="$L8" "$STOP_HOOK" 2>/dev/null)"
if [ -z "$(jfield "$OUT" hookSpecificOutput.additionalContext)" ]; then
    ok "18f  the next turn end does NOT deliver it again — once per session, so the lead's turn is not held"
else
    bad "18f  delivered once" "delivered a second time: $OUT"
fi

sleep 1
NEW8B="$(raise8 zach-opus-e1second "a second one, later in the session")"
OUT="$(stop_payload_t "$S8" "$T8" | RICHOS_ESCALATION_LEDGER="$L8" "$STOP_HOOK" 2>/dev/null)"
CTX="$(jfield "$OUT" hookSpecificOutput.additionalContext)"
case "$CTX" in
    *"$NEW8B"*)
        case "$CTX" in
            *"$NEW8"*) bad "18g  only the newest" "the first one was repeated beside the second: $CTX" ;;
            *)         ok "18g  a later escalation is delivered ALONE — and the count of those already told is stated" ;;
        esac ;;
    *) bad "18g  later escalation delivered" "got: $OUT" ;;
esac

# A WORKER'S turn is not the lead's. Stop is the main thread's event, but a
# payload that carries agent_id is a subagent's and must not be handed other
# teammates' escalations.
sleep 1
NEW8C="$(raise8 zach-opus-e1third "raised while a worker's turn ends")"
OUT="$(stop_payload_t "$S8" "$T8" "agent-abc123" | RICHOS_ESCALATION_LEDGER="$L8" "$STOP_HOOK" 2>/dev/null)"
if [ -z "$(jfield "$OUT" hookSpecificOutput.additionalContext)" ]; then
    ok "18h  a payload carrying agent_id (a worker) gets NO model delivery"
else
    bad "18h  worker not told" "got: $OUT"
fi
OUT="$(stop_payload_t "$S8" "$T8" | RICHOS_ESCALATION_LEDGER="$L8" "$STOP_HOOK" 2>/dev/null)"
case "$(jfield "$OUT" hookSpecificOutput.additionalContext)" in
    *"$NEW8C"*) ok "18i  and the lead still gets it at its own next turn end — the worker's turn consumed nothing" ;;
    *) bad "18i  lead still told after a worker turn" "got: $OUT" ;;
esac

# THE HOST CAP. A SessionStart block over 10,000 characters is replaced by a
# path and a 2,000-character preview, so only the ids in that preview reached
# the model. The rest must be delivered at the first turn end, not assumed.
L8B="$SANDBOX/state/deliver-cap.jsonl"
T8B="$SANDBOX/transcript-18cap.jsonl"
for i in $(seq 1 60); do
    raise8 "zach-opus-e1cap$i" "cap fixture number $i with a title long enough to fill the block" "$L8B" >/dev/null
done
SS_OUT="$(RICHOS_ESCALATION_LEDGER="$L8B" "$START_HOOK" </dev/null 2>/dev/null)"
: > "$T8B"
record_session_start "$T8B" "$SS_OUT"
SS_LEN="$(jfield "$SS_OUT" hookSpecificOutput.additionalContext | wc -c | tr -d ' ')"
sleep 1
CAPNEW="$(raise8 zach-opus-e1capnew "raised mid-session beside a cut-off backlog" "$L8B")"
OUT="$(stop_payload_t "19191919-0000-4000-8000-000000000000" "$T8B" \
       | RICHOS_ESCALATION_LEDGER="$L8B" "$STOP_HOOK" 2>/dev/null)"
CTX="$(jfield "$OUT" hookSpecificOutput.additionalContext)"
if [ "$SS_LEN" -gt 10000 ] && [ -n "$CTX" ] && [ "${#CTX}" -le 10000 ] \
   && printf '%s' "$CTX" | grep -q 'cap fixture number'; then
    ok "18j  a SessionStart block of $SS_LEN chars was cut by the host; the ids past its preview are delivered, in ${#CTX} chars (under the cap)"
else
    bad "18j  host cap honored" "session-start=$SS_LEN chars, delivery=${#CTX} chars: $OUT"
fi
FIRST_ENTRY="$(printf '%s\n' "$CTX" | sed -n '2p')"
case "$FIRST_ENTRY" in
    *"$CAPNEW"*) ok "18m  the escalation raised mid-session is delivered FIRST, ahead of the cut-off backlog" ;;
    *) bad "18m  newest first" "first entry is not $CAPNEW: $FIRST_ENTRY" ;;
esac

# MUTATION — is the model half carried by additionalContext, or by luck? Put
# back the pre-fix world (the predicate adds nothing for the model) and the
# cases above must lose it.
cp "$ENG/scripts/lib/escalations.py" "$SANDBOX/escalations.py.real3"
python3 - "$ENG/scripts/lib/escalations.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
s = s.replace('        out["hookSpecificOutput"] = {"hookEventName": "Stop", "additionalContext": context}',
              '        pass  # MUTATION (test-only): the operator line alone, as before 2026-09-28')
open(p, "w", encoding="utf-8").write(s)
PY
L8M="$SANDBOX/state/deliver-mutant.jsonl"
raise8 zach-opus-e1mutant "the mutant must not reach the model" "$L8M" >/dev/null
OUT="$(stop_payload_t "20202020-0000-4000-8000-000000000000" "" | RICHOS_ESCALATION_LEDGER="$L8M" "$STOP_HOOK" 2>/dev/null)"
if [ -n "$(jfield "$OUT" systemMessage)" ] && [ -z "$(jfield "$OUT" hookSpecificOutput.additionalContext)" ]; then
    ok "18k  NEGATIVE CONTROL: with the model half removed the operator is still told and the model gets NOTHING — the 2026-09-28 defect, reproduced"
else
    bad "18k  negative control" "the mutant still reached the model or lost the operator line: $OUT"
fi
cp "$SANDBOX/escalations.py.real3" "$ENG/scripts/lib/escalations.py"
OUT="$(stop_payload_t "21212121-0000-4000-8000-000000000000" "" | RICHOS_ESCALATION_LEDGER="$L8M" "$STOP_HOOK" 2>/dev/null)"
case "$(jfield "$OUT" hookSpecificOutput.additionalContext)" in
    *"the mutant must not reach the model"*) ok "18l  and the shipped source delivers the same fixture to the model" ;;
    *) bad "18l  restored source delivers" "got: $OUT" ;;
esac

# ===========================================================================
# 17. THE LEAK CANARY, PROVEN IN BOTH DIRECTIONS BEFORE IT IS TRUSTED.
#
# A canary that cannot go red is worse than no canary: it is a green tick
# printed over the defect, which is precisely what this suite did on every run
# while it was leaking — 59 passed, 0 failed, one fixture in a stranger's
# worktree. So the detector is shown to FIRE on the real defect and to STAY
# SILENT on the real fix, one argument apart, both entirely inside the sandbox
# so that proving it requires writing nothing to a live tree.
# ===========================================================================
FAKE="$SANDBOX/fake-live-checkout"
mkdir -p "$FAKE/docs"
git init -q "$FAKE" 2>/dev/null
printf 'seed\n' > "$FAKE/docs/seed.md"
L7="$SANDBOX/state/canary.jsonl"

C0="$SANDBOX/canary-fake-before.txt"
if canary_snapshot "$FAKE" > "$C0"; then
    ok "17a  the canary can READ the root it is about to watch — an unreadable root is a failure here, never a quiet pass"
else
    bad "17a  canary root readable" "canary_snapshot could not read $FAKE, so nothing below would be comparing anything against anything"
fi

# --- RED: the 2026-09-05 defect, reproduced exactly ------------------------
( cd "$FAKE" && RICHOS_ESCALATION_LEDGER="$L7" "$ESCALATE" raise \
      --title "the canary leak fixture" --state proceeding \
      --question "does the canary notice a record written into the current directory?" \
      --teammate zach-opus-e1canary ) >/dev/null 2>&1
ESCAPED="$(canary_escaped "$C0" "$FAKE")"
say "17 red" "$ESCAPED"
case "$ESCAPED" in
    *docs/verification/escalations/*)
        ok "17b  RED: with no --worktree the record escapes into the current directory and the canary NAMES the file" ;;
    "")
        bad "17b  canary goes RED on the real defect" "a record was written into $FAKE and the canary reported nothing — this canary could not have caught the leak it exists for" ;;
    *)
        bad "17b  canary names the escaped file" "it saw an escape but did not name the record: $ESCAPED" ;;
esac

# --- RED AGAIN, WITH THE RESIDUE ALREADY IN THE BASELINE ------------------
# The hole this canary fell into on the day it was written, now a case. Take a
# fresh baseline WITH the escaped record already present — which is what the
# second run of a leaking suite sees — and leak again to the same filename. A
# path-only snapshot reports clean here, because nothing is new. Only a
# content witness sees it.
C0B="$SANDBOX/canary-fake-residue.txt"
canary_snapshot "$FAKE" > "$C0B"
if [ -s "$C0B" ] && grep -q 'docs/verification/escalations/' "$C0B"; then
    ok "17c  the residue really is in the new baseline — so 17d below is asking the hard question, not the easy one"
else
    bad "17c  residue is in the baseline" "the escaped record is not in the re-taken baseline, so 17d would just be repeating 17b"
fi
# One second, deliberately. In the field the two leaking runs are minutes
# apart and their records differ by an id and a `raised:` line, so content
# alone would catch the repeat. Back to back inside one second they are
# byte-identical, and this case would then be proving only that the machine
# has a nanosecond clock. The sleep makes it prove the thing it is named for
# on any machine.
sleep 1
( cd "$FAKE" && RICHOS_ESCALATION_LEDGER="$L7" "$ESCALATE" raise \
      --title "the canary leak fixture" --state proceeding \
      --question "does the canary notice a record written into the current directory?" \
      --teammate zach-opus-e1canary ) >/dev/null 2>&1
ESCAPED="$(canary_escaped "$C0B" "$FAKE")"
case "$ESCAPED" in
    *docs/verification/escalations/*)
        ok "17d  RED AGAIN: a SECOND leak to the same filename is still named — the canary does not go quiet once residue exists" ;;
    "")
        bad "17d  RED on a repeat leak" "the record was overwritten in place and the canary saw nothing. This is the path-only bug: on a second consecutive leaking run it would report a clean sheet over a live leak" ;;
    *)
        bad "17d  RED on a repeat leak names the file" "saw an escape but not the record: $ESCAPED" ;;
esac

# --- reset, AND PROVE THE RESET -------------------------------------------
# Without this, the green half would be vacuous. Leave the escaped file in
# place and it simply becomes part of the next baseline, so "nothing new
# appeared" becomes true for the wrong reason — a pass bought by absorbing the
# very evidence the case is about.
rm -rf "$FAKE/docs/verification"
C1="$SANDBOX/canary-fake-reset.txt"
canary_snapshot "$FAKE" > "$C1"
if diff -q "$C0" "$C1" >/dev/null 2>&1; then
    ok "17e  the fixture really is gone — the baseline is back to what it was, so 17f cannot pass by absorbing the leak"
else
    bad "17e  the reset is real" "baseline differs after cleanup: $(diff "$C0" "$C1" | tr '\n' ' ')"
fi

# --- GREEN: the same call, one argument different --------------------------
( cd "$FAKE" && RICHOS_ESCALATION_LEDGER="$L7" "$ESCALATE" raise \
      --title "the canary fixed fixture" --state proceeding \
      --question "does the record stay in the sandbox once --worktree names where it goes?" \
      --worktree "$WT14" --teammate zach-opus-e1canary ) >/dev/null 2>&1
ESCAPED="$(canary_escaped "$C1" "$FAKE")"
if [ -z "$ESCAPED" ]; then
    ok "17f  GREEN: with --worktree the same call leaves the current directory untouched — so 17b was a LEAK, not a detector that reports everything"
else
    bad "17f  canary GREEN once the leak is fixed" "still escaping: $ESCAPED"
fi

# --- the non-git branch of the snapshot is watched too ---------------------
# A suite started from a plain directory must be watched as closely as one
# started from a checkout, and an unexercised branch inside a canary is a hole
# in the thing doing the guaranteeing.
PLAIN="$SANDBOX/plain-directory"
mkdir -p "$PLAIN"
C2="$SANDBOX/canary-plain-before.txt"
canary_snapshot "$PLAIN" > "$C2"
printf 'x\n' > "$PLAIN/escaped-record.md"
ESCAPED="$(canary_escaped "$C2" "$PLAIN")"
case "$ESCAPED" in
    *escaped-record.md*) ok "17g  the non-git branch of the snapshot catches an escape too, and names it" ;;
    *) bad "17g  non-git root is watched" "a file appeared in a plain watched directory and the canary did not name it: '$ESCAPED'" ;;
esac
rm -f "$PLAIN/escaped-record.md"
ESCAPED="$(canary_escaped "$C2" "$PLAIN")"
if [ -z "$ESCAPED" ]; then
    ok "17h  and it is silent when nothing appeared — both branches report escapes, not activity"
else
    bad "17h  non-git branch is quiet when clean" "got: $ESCAPED"
fi

# --- the REFUSAL path, driven rather than assumed --------------------------
# `canary_snapshot` refuses a root it cannot witness honestly, and that refusal
# is what turns case 17n into a FAILURE instead of a quiet pass. An else-branch
# nothing ever drives is an else-branch nobody knows the state of, so drive it:
# drop the ceiling under a root that is already above it.
CANARY_MAX_SAVED="$CANARY_MAX_ENTRIES"
CANARY_MAX_ENTRIES=0
if canary_snapshot "$FAKE" >/dev/null 2>&1; then
    bad "17i  a root it cannot witness is REFUSED" "canary_snapshot accepted a root above its own ceiling, so CANARY_HEALTHY can never reach 0 and 17n's failure branch is unreachable code"
else
    ok "17i  a root it cannot witness is REFUSED, never sampled — so 17n's failure branch is reachable rather than decorative"
fi
ESCAPED="$(canary_escaped "$C2" "$PLAIN")"
case "$ESCAPED" in
    UNREADABLE*) ok "17j  and it is reported as UNREADABLE by name, never as 'nothing escaped' — a root that could not be checked is not a clean root" ;;
    "")          bad "17j  an unwitnessable root is named" "it reported nothing at all, which reads identically to a clean sheet — the exact confusion this canary exists to remove" ;;
    *)           bad "17j  an unwitnessable root is named" "got: $ESCAPED" ;;
esac
CANARY_MAX_ENTRIES="$CANARY_MAX_SAVED"
if canary_snapshot "$FAKE" >/dev/null 2>&1; then
    ok "17k  and the ceiling really was what refused it: restored, the same root reads fine again"
else
    bad "17k  the refusal was the ceiling" "the root is still unreadable after restoring CANARY_MAX_ENTRIES, so 17i proved something other than what it claims"
fi

# --- THE GUARANTEE ---------------------------------------------------------
# Everything above proves the detector works. This is the thing it was built
# for: the whole run, measured against the baselines taken before case 1.
if [ "$CANARY_N" -gt 0 ]; then
    ok "17l  the canary is watching $CANARY_N root(s), starting with the directory this suite was run from — a canary watching zero roots would pass every run forever"
else
    bad "17l  the canary has roots to watch" "no watched root resolved, so 17n below would be green over an unwatched filesystem"
fi
# Outside the branch above deliberately: the run that found no roots is exactly
# the run whose reader most needs to be told what the witness was.
if [ -n "$CANARY_MTIME" ]; then
    ok "17m  its witness is contents and a proven sub-second mtime (\`$CANARY_MTIME\`)"
else
    ok "17m  its witness is contents ALONE: no sub-second mtime format proved itself on this platform, so two writes of identical bytes inside one second would look like one. Named here rather than assumed"
fi

CANARY_ESCAPED_ALL=""
CANARY_I=0
while IFS= read -r canary_root; do
    [ -n "$canary_root" ] || continue
    CANARY_I=$((CANARY_I + 1))
    E="$(canary_escaped "$CANARY_BASELINES/$CANARY_I.txt" "$canary_root")"
    [ -n "$E" ] && CANARY_ESCAPED_ALL="$CANARY_ESCAPED_ALL
  under $canary_root:
$E"
done <<CANARY_ROOT_LIST
$CANARY_ROOTS
CANARY_ROOT_LIST

if [ "$CANARY_HEALTHY" -ne 1 ]; then
    bad "17n  NOTHING ESCAPED THE SANDBOX" "the canary could not read (or refused as too large) one of its roots when it took its baselines, so it has nothing to compare against and is NOT reporting a pass"
elif [ -z "$CANARY_ESCAPED_ALL" ]; then
    ok "17n  NOTHING ESCAPED THE SANDBOX — every file this run wrote is under $SANDBOX"
else
    bad "17n  NOTHING ESCAPED THE SANDBOX" "this run wrote outside its sandbox, which is the 2026-09-05 defect happening again:$CANARY_ESCAPED_ALL"
fi

CANARY_REAL_LEDGER_AFTER="$(canary_fixture_rows)"
if [ "$CANARY_REAL_LEDGER_AFTER" = "$CANARY_REAL_LEDGER_BEFORE" ]; then
    ok "17o  and no fixture row reached the operator's REAL ledger at $CANARY_REAL_LEDGER"
else
    bad "17o  real ledger untouched" "fixture rows in $CANARY_REAL_LEDGER went $CANARY_REAL_LEDGER_BEFORE -> $CANARY_REAL_LEDGER_AFTER; something ignored RICHOS_ESCALATION_LEDGER"
fi

# ===========================================================================
printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

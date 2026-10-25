#!/usr/bin/env bash
#
# protected-ref-moves.test.sh — THE READER FOR A CHECK THAT STOPPED WRITING.
#
# ===========================================================================
# WHAT IS UNDER TEST
# ===========================================================================
#   scripts/lib/protected-ref-moves.py            the predicate
#   scripts/hooks/notice-protected-ref-moves.sh   the Stop notice
#
# On 2026-09-14 the engine's protected-ref check stopped moving a ref back and
# started only reporting. The report went to three places and a person read
# none of them; the third and loudest, a `=== PROTECTED REF MOVED ... ===` line
# from observe-created-refs.sh, goes to the STDERR of a PostToolUse hook that
# exits 0 — measured invisible on the operator's stream in the channel table in
# scripts/lib/stop-hook-notice.sh, and printed into the session of the agent
# that made the move. This suite exists to prove the replacement is audible,
# that it is audible BECAUSE OF ITS CHANNEL, and that it is quiet when it
# should be.
#
# ===========================================================================
# EVERY FIRING CASE HAS A SILENT TWIN, AND THAT IS THE WHOLE DESIGN
# ===========================================================================
# A notice that always fires and a notice that never fires are both useless and
# both look like a passing suite. So each case here is paired with one built to
# be as similar as possible, with a single fact changed.
#
#   P01/P02  a tip that is OFF the branch fires  <->  the same event row with
#            the tip still ON the branch is silent. This is the pair that
#            matters: P02 is the lead's ordinary land, which is what the old
#            writing check could not tell from abuse, and it must say nothing.
#   P03/P04  a branch that is gone, and a repository that is gone, are each
#            announced as CANNOT BE CHECKED. Undecidable is never collapsed
#            into clear.
#   P05/P06  the same finding twice in one session is announced once  <->  a
#            NEW finding in that same session speaks again.
#   P07/P08  a settlement with a reason silences it  <->  a settlement with a
#            blank reason is refused and silences nothing.
#   P09/P10  the notice announces every way it can stop working: no predicate,
#            and no resolvable root.
#   P11      an unadopted directory is silent — the noise control.
#   P12      THE NEGATIVE CONTROL FOR THE CHANNEL. The same hook, reaching the
#            same finding, announcing the way the OLD surface announces (stderr
#            at exit 0), puts NOTHING on the operator's stream. Without this,
#            P01 proves a message exists and not that anybody can see it.
#   P13      the predicate never writes a ref. Its reachability call is run
#            against a repository whose refs are recorded before and after.
#
# Usage: scripts/hooks/protected-ref-moves.test.sh
# Exit:  0 all cases passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PRED="$ENGINE_ROOT/scripts/lib/protected-ref-moves.py"
NOTICE="$SCRIPT_DIR/notice-protected-ref-moves.sh"
OBSERVE="$SCRIPT_DIR/observe-created-refs.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; FAIL=$((FAIL + 1)); return 0; }

command -v python3 >/dev/null 2>&1 || { echo "ERROR: needs python3" >&2; exit 1; }
command -v git     >/dev/null 2>&1 || { echo "ERROR: needs git" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t protectedref.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# THE STORE IS SANDBOXED, AND THAT IS NOT TIDINESS. Without this export every
# fixture row this suite writes lands in the OPERATOR's real
# ~/.claude/state/workspaces/events.jsonl, where the live Stop hook would read
# it and announce a finding about a temp directory that stopped existing when
# the trap fired. A test never writes to the operator's state.
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"
mkdir -p "$RICHOS_WORKSPACES_DIR"
EVENTS="$RICHOS_WORKSPACES_DIR/events.jsonl"

echo "=== a protected ref that lost commits: the predicate, and who hears about it ==="
echo ""

SEAT="$SANDBOX/seat"            # the adopted repository the session is seated in
TARGET="$SANDBOX/target"        # the repository holding the protected branch
NOHOOKS="$SANDBOX/nohooks"
mkdir -p "$NOHOOKS"

mk_repo() { # <path>
    mkdir -p "$1"
    git -C "$1" init -q -b main
    git -C "$1" config user.email "tester@example.invalid"
    git -C "$1" config user.name  "tester"
    # A global core.hooksPath (an identity guard, a linter) would fail this
    # suite for reasons that have nothing to do with what is under test.
    git -C "$1" config core.hooksPath "$NOHOOKS"
    git -C "$1" commit -q --allow-empty -m "root"
}

mk_repo "$SEAT"
mk_repo "$TARGET"
: > "$SEAT/orchestration.config"        # the adoption marker: hooks do not stand down here

# Four commits on the target's main. C2 is the tip an agent's window recorded.
git -C "$TARGET" commit -q --allow-empty -m "c1"
git -C "$TARGET" commit -q --allow-empty -m "c2"
SNAP="$(git -C "$TARGET" rev-parse HEAD)"
git -C "$TARGET" commit -q --allow-empty -m "c3"
AHEAD="$(git -C "$TARGET" rev-parse HEAD)"

# The event the check writes today, byte-for-byte in its real shape. Copied
# from a REAL row in the operator's log (2026-09-14T00:51:25Z) rather than
# invented, so a change to the writer's field names fails this suite instead of
# passing it.
write_event() { # <repo> <branch> <tip> <found> [event] [ts]
    python3 - "$EVENTS" "$1" "$2" "$3" "$4" "${5:-protected-ref-moved}" "${6:-}" <<'PY'
import json, sys, time
path, repo, branch, tip, found, ev, ts = sys.argv[1:8]
if not ts:
    ts = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
row = {"action": "reported: the engine does not move a ref back",
       "branch": branch, "event": ev, "found": found,
       "key": "feedface-0000-4000-8000-000000000000--zach-opus-n4",
       "repo": repo, "tip": tip, "ts": ts,
       "why": "moved to %s, which carries this agent's own unlanded work "
              "(it committed or merged onto it)" % found[:12]}
with open(path, "a", encoding="utf-8") as f:
    f.write(json.dumps(row, sort_keys=True) + "\n")
PY
}

mk_payload() { # <session-id> [cwd] -> a real Stop payload shape
    python3 - "$1" "${2:-$SEAT}" <<'PY'
import json, sys
print(json.dumps({
    "hook_event_name": "Stop", "session_id": sys.argv[1],
    "transcript_path": "/nonexistent/transcript.jsonl", "cwd": sys.argv[2],
    "prompt_id": "deadbeef-0000-4000-8000-000000000000",
    "permission_mode": "default", "stop_hook_active": False,
    "last_assistant_message": "Landed the branch and deployed.",
    "background_tasks": [], "session_crons": [],
}))
PY
}

# drive <hook> <session-id> — sets RUN_OUT, RUN_ERR, RUN_RC.
drive() {
    local hook="$1" sid="$2" o e
    o="$(mktemp "$SANDBOX/o.XXXXXX")"; e="$(mktemp "$SANDBOX/e.XXXXXX")"
    mk_payload "$sid" | env RICHOS_ENTITY_ROOT="$SEAT" CLAUDE_PROJECT_DIR="$SEAT" \
        bash "$hook" >"$o" 2>"$e"
    RUN_RC=$?
    RUN_OUT="$(cat "$o")"; RUN_ERR="$(cat "$e")"
    rm -f "$o" "$e"
    return 0
}

# system_message — the systemMessage from RUN_OUT, or "" if it is not one.
system_message() {
    printf '%s' "$RUN_OUT" | python3 -c '
import json, sys
try:
    d = json.loads(sys.stdin.read() or "{}")
    v = d.get("systemMessage")
    print(v if isinstance(v, str) else "")
except Exception:
    print("")
' 2>/dev/null
}

# ===========================================================================
# P01 — THE FINDING FIRES, ON THE OPERATOR'S CHANNEL
# ===========================================================================
# main is rewound behind the tip the agent's window recorded: exactly the
# `--mode destruction` shape of the reproduction, where the lead's merge is
# dropped. One commit is off the branch and a person has to decide about it.
git -C "$TARGET" reset -q --hard "$SNAP~1"
write_event "$TARGET" main "$SNAP" "$(git -C "$TARGET" rev-parse HEAD)"

drive "$NOTICE" "aaaa0001-0000-4000-8000-000000000000"
MSG="$(system_message)"
if [ "$RUN_RC" -ne 0 ]; then
    bad "P01 an OPEN finding is announced" "the hook exited $RUN_RC; a Stop notice must never refuse a turn"
elif [ -z "$MSG" ]; then
    bad "P01 an OPEN finding is announced" "nothing on the operator channel. stdout=[$RUN_OUT] stderr=[$(printf '%s' "$RUN_ERR" | head -c 200)]"
elif printf '%s' "$MSG" | grep -q "main in $TARGET no longer contains ${SNAP:0:12}" \
  && printf '%s' "$MSG" | grep -q "1 commit is off the branch" \
  && printf '%s' "$MSG" | grep -q "min ago"; then
    ok "P01 a protected branch missing a tip it held is announced to the OPERATOR, naming repo, branch, tip, what is off it and since when"
else
    bad "P01 an OPEN finding is announced" "message does not name the finding: $MSG"
fi
# P01b — THE NOTICE CLAIMS ONLY WHAT IS ESTABLISHED (2026-10-24). Off the branch
# is a fact; lost, "nothing was put back" and "nobody has acted" are not: main's
# 0899f054bf3d moved a ledger to the private record on purpose, kept byte for
# byte there, and this notice called it lost and ignored for 35 days.
if [ -n "$MSG" ] && ! printf '%s' "$MSG" | grep -qiE "nothing was put back|nobody has acted|put the commits back|\blost\b"; then
    ok "P01b the notice makes no loss or inaction claim that non-ancestry does not establish"
else
    bad "P01b the notice asserts loss or inaction" "$MSG"
fi

# ===========================================================================
# P02 — THE PAIRED TWIN: THE SAME EVENT, THE TIP STILL ON THE BRANCH, SILENT
# ===========================================================================
# One fact changed: main is forward of the snapshot tip rather than behind it.
# This is the lead's ordinary land — the single most common write that branch
# ever gets, and the shape the old writing check could not tell from abuse.
# If this fires, the replacement is noise and nobody will read it either.
git -C "$TARGET" reset -q --hard "$AHEAD"
drive "$NOTICE" "aaaa0002-0000-4000-8000-000000000000"
if [ -z "$(system_message)" ] && [ -z "$RUN_OUT" ]; then
    ok "P02 PAIRED TWIN: the identical event with the tip still ON the branch (the lead's ordinary land) says NOTHING"
else
    bad "P02 a healed finding is silent" "spoke anyway: $(system_message)"
fi

# ===========================================================================
# P03 — THE BRANCH IS GONE: UNDECIDABLE, AND ANNOUNCED
# ===========================================================================
git -C "$TARGET" checkout -q -b side
git -C "$TARGET" branch -q -D main
drive "$NOTICE" "aaaa0003-0000-4000-8000-000000000000"
MSG="$(system_message)"
if printf '%s' "$MSG" | grep -q "CANNOT BE CHECKED" && printf '%s' "$MSG" | grep -q "does not exist"; then
    ok "P03 a branch that no longer exists is UNDECIDABLE and is announced as loudly as an open finding"
else
    bad "P03 undecidable is announced" "got: $MSG"
fi
git -C "$TARGET" checkout -q -b main "$AHEAD"
git -C "$TARGET" branch -q -D side

# ===========================================================================
# P04 — THE REPOSITORY IS GONE: UNDECIDABLE, AND ANNOUNCED
# ===========================================================================
GONE="$SANDBOX/gone"
mk_repo "$GONE"
GONE_TIP="$(git -C "$GONE" rev-parse HEAD)"
write_event "$GONE" main "$GONE_TIP" "$GONE_TIP"
rm -rf "$GONE"
drive "$NOTICE" "aaaa0004-0000-4000-8000-000000000000"
MSG="$(system_message)"
if printf '%s' "$MSG" | grep -q "CANNOT BE CHECKED" && printf '%s' "$MSG" | grep -q "not on disk"; then
    ok "P04 a repository that is not on disk is UNDECIDABLE and announced — absence is never read as clear"
else
    bad "P04 a missing repository is announced" "got: $MSG"
fi

# ===========================================================================
# P05/P06 — SAID ONCE PER SESSION, AND A NEW FINDING SPEAKS AGAIN
# ===========================================================================
SID="aaaa0005-0000-4000-8000-000000000000"
drive "$NOTICE" "$SID"
FIRST="$(system_message)"
drive "$NOTICE" "$SID"
SECOND="$(system_message)"
if [ -n "$FIRST" ] && [ -z "$SECOND" ]; then
    ok "P05 the same finding in the same session is announced once — a line under every turn is a line the eye is trained to skip"
else
    bad "P05 de-duplication" "first=[$(printf '%s' "$FIRST" | head -c 80)] second=[$(printf '%s' "$SECOND" | head -c 120)]"
fi

OTHER="$SANDBOX/other"
mk_repo "$OTHER"
git -C "$OTHER" commit -q --allow-empty -m "o1"
O_SNAP="$(git -C "$OTHER" rev-parse HEAD)"
git -C "$OTHER" reset -q --hard "$O_SNAP~1"
write_event "$OTHER" main "$O_SNAP" "$(git -C "$OTHER" rev-parse HEAD)"
drive "$NOTICE" "$SID"
if [ -n "$(system_message)" ]; then
    ok "P06 PAIRED TWIN: a NEW finding in that same session speaks again — silence only ever means 'still what I told you'"
else
    bad "P06 a new finding speaks again" "silent after a second repository lost commits"
fi

# ===========================================================================
# P07/P08 — SETTLING IT, AND THE MUTE BUTTON THAT IS REFUSED
# ===========================================================================
BLANK_RC=0
python3 "$PRED" review --repo "$OTHER" --branch main --tip "$O_SNAP" --why "" \
    >/dev/null 2>"$SANDBOX/rev.err" || BLANK_RC=$?
if [ "$BLANK_RC" -eq 2 ] && grep -q "mute button" "$SANDBOX/rev.err"; then
    ok "P08 a settlement with a blank reason is REFUSED — the only way to silence a finding carries a reason"
else
    bad "P08 a reasonless settlement is refused" "exit $BLANK_RC: $(head -c 200 "$SANDBOX/rev.err")"
fi

REV_RC=0
python3 "$PRED" review --repo "$OTHER" --branch main --tip "${O_SNAP:0:12}" \
    --why "rewound on purpose to drop a bad land" >"$SANDBOX/rev.out" 2>&1 || REV_RC=$?
drive "$NOTICE" "aaaa0007-0000-4000-8000-000000000000"
STILL="$(system_message)"
if [ "$REV_RC" -ne 0 ]; then
    bad "P07 a settled finding goes quiet" "review exited $REV_RC: $(head -c 200 "$SANDBOX/rev.out")"
elif ! grep -q "protected-ref-move-reviewed" "$EVENTS"; then
    bad "P07 a settled finding goes quiet" "no settlement row reached the store's event log"
elif printf '%s' "$STILL" | grep -q "$OTHER"; then
    bad "P07 a settled finding goes quiet" "still named after being settled: $STILL"
else
    ok "P07 a settlement (with its reason, in the SAME event log) silences that finding and nothing else"
fi

# ===========================================================================
# P09 — NO PREDICATE, NO SILENCE
# ===========================================================================
MIRROR="$SANDBOX/mirror"
mkdir -p "$MIRROR/scripts/hooks" "$MIRROR/scripts/lib"
cp "$NOTICE" "$MIRROR/scripts/hooks/"
for f in resolve-roots.sh stop-hook-notice.sh; do cp "$ENGINE_ROOT/scripts/lib/$f" "$MIRROR/scripts/lib/"; done
# protected-ref-moves.py is deliberately NOT copied.
drive "$MIRROR/scripts/hooks/notice-protected-ref-moves.sh" "aaaa0009-0000-4000-8000-000000000000"
MSG="$(system_message)"
if printf '%s' "$MSG" | grep -q "PROTECTED REF WATCH IS OFF" \
  && printf '%s' "$MSG" | grep -q "Do not take this silence"; then
    ok "P09 with its predicate missing the hook SAYS SO — an absent reader and a clean report never look the same"
else
    bad "P09 a missing predicate is announced" "got: $MSG"
fi

# ===========================================================================
# P10 — A ROOT IT CANNOT RESOLVE IS ANNOUNCED, AND DOES NOT BLOCK
# ===========================================================================
UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"
O="$(mktemp "$SANDBOX/o.XXXXXX")"; E="$(mktemp "$SANDBOX/e.XXXXXX")"
mk_payload "aaaa0010-0000-4000-8000-000000000000" \
    | env RICHOS_ENTITY_ROOT="$UNADOPTED" bash "$NOTICE" >"$O" 2>"$E"
RC10=$?
RUN_OUT="$(cat "$O")"; RUN_ERR="$(cat "$E")"; rm -f "$O" "$E"
MSG="$(system_message)"
if [ "$RC10" -eq 2 ]; then
    bad "P10 a broken root is announced" "exited 2 — a Stop hook that blocks on a broken install re-fires to the block cap and strands the session"
elif printf '%s' "$MSG" | grep -q "PROTECTED REF WATCH IS OFF"; then
    ok "P10 a declared root that is not adopted is announced to the operator, and the turn is not refused"
else
    bad "P10 a broken root is announced" "got: $MSG"
fi

# ===========================================================================
# P11 — THE NOISE CONTROL
# ===========================================================================
O="$(mktemp "$SANDBOX/o.XXXXXX")"
mk_payload "aaaa0011-0000-4000-8000-000000000000" "$UNADOPTED" \
    | (cd "$UNADOPTED" && env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR bash "$NOTICE") \
        >"$O" 2>/dev/null
if [ -s "$O" ]; then
    bad "P11 noise control" "spoke in a directory that never adopted the engine: $(head -c 200 "$O")"
else
    ok "P11 NOISE CONTROL: a directory that never adopted the engine is not-applicable, and this hook is silent there"
fi
rm -f "$O"

# ===========================================================================
# P12 — THE NEGATIVE CONTROL FOR THE CHANNEL
# ===========================================================================
# THIS IS THE CASE THAT MAKES P01 MEAN SOMETHING. A copy of this hook with its
# operator notice put back on STDERR at exit 0 — the channel the pre-existing
# `=== PROTECTED REF MOVED ... ===` line from observe-created-refs.sh uses —
# reaches the same finding and puts NOTHING on the operator's stream. The
# finding is identical; only the channel changed; the visibility is the whole
# difference. Without this, P01 proves a message exists and not that anyone can
# see it.
MUT="$MIRROR/scripts/hooks/mutant.sh"
cp "$ENGINE_ROOT/scripts/lib/protected-ref-moves.py" "$MIRROR/scripts/lib/"
mkdir -p "$MIRROR/mega-lander"
cp "$ENGINE_ROOT/mega-lander/workspaces.py" "$MIRROR/mega-lander/"
sed 's/^\( *\)stop_notice_abnormal_recurring .*$/\1echo "=== PROTECTED REF MOVED: $LINE ===" >\&2/' \
    "$NOTICE" > "$MUT"
drive "$MUT" "aaaa0012-0000-4000-8000-000000000000"
if grep -q 'stop_notice_abnormal_recurring "\$KEY"' "$MUT"; then
    bad "P12 negative control" "the mutation did not replace the notice call — P01's result is unproven"
elif ! printf '%s' "$RUN_ERR" | grep -q "PROTECTED REF MOVED"; then
    bad "P12 negative control" "the mutant never reached the finding at all: $(printf '%s' "$RUN_ERR" | head -c 200)"
elif [ -n "$RUN_OUT" ]; then
    bad "P12 negative control" "the mutant put something on the operator channel: $(printf '%s' "$RUN_OUT" | head -c 200)"
else
    ok "P12 NEGATIVE CONTROL: the same finding announced the OLD way (stderr, exit 0) reaches the transcript and NOTHING on the operator's stream"
fi

# P12b — and that IS the old way. Every line of observe-created-refs.sh that
# carries the MOVED notice is redirected to stderr, and the hook exits 0 on
# every path, which is the combination the channel table in
# scripts/lib/stop-hook-notice.sh measured invisible.
if [ ! -f "$OBSERVE" ]; then
    bad "P12b the old surface is stderr at exit 0" "observe-created-refs.sh is not on disk"
else
    MOVED_LINES="$(grep -c 'PROTECTED REF MOVED' "$OBSERVE" 2>/dev/null || echo 0)"
    MOVED_STDERR="$(grep 'PROTECTED REF MOVED' "$OBSERVE" 2>/dev/null | grep -c '>&2' || echo 0)"
    if [ "$MOVED_LINES" -gt 0 ] && [ "$MOVED_LINES" = "$MOVED_STDERR" ] && [ "$(tail -1 "$OBSERVE")" = "exit 0" ]; then
        ok "P12b the pre-existing surface is exactly that: $MOVED_LINES MOVED notice line(s), all on stderr, in a hook that ends 'exit 0'"
    else
        bad "P12b the old surface is stderr at exit 0" \
            "moved-lines=$MOVED_LINES on-stderr=$MOVED_STDERR last-line=$(tail -1 "$OBSERVE")"
    fi
fi

# ===========================================================================
# P13 — THE READER NEVER WRITES A REF
# ===========================================================================
# The defect being fixed was a check that wrote. Its reader must never acquire
# the same power by accident, so the refs of the repository it inspects are
# recorded before and after and compared byte for byte.
REFS_BEFORE="$(git -C "$TARGET" for-each-ref --format='%(refname) %(objectname)')"
python3 "$PRED" list >/dev/null 2>&1 || true
REFS_AFTER="$(git -C "$TARGET" for-each-ref --format='%(refname) %(objectname)')"
if [ "$REFS_BEFORE" = "$REFS_AFTER" ]; then
    ok "P13 the reader inspected the repository and left every ref exactly where it was"
else
    bad "P13 the reader never writes a ref" "refs changed: $(diff <(printf '%s' "$REFS_BEFORE") <(printf '%s' "$REFS_AFTER") | head -5)"
fi

# ===========================================================================
# P14 — THE AGE IS READ AS UTC, WHICH IS HOW THE LOG WRITES IT
# ===========================================================================
# The age is not decoration: it is the rung the notice escalates on, and the
# sentence quotes it to a person deciding whether to act. `time.mktime(...) -
# time.timezone` gets the STANDARD offset rather than the one in force, so on a
# machine observing summer time every finding is reported an hour older than it
# is. Caught on 2026-09-14 by the demonstration, where a row written seconds
# earlier came out as "1 h ago".
AGED="$SANDBOX/aged"
mk_repo "$AGED"
git -C "$AGED" commit -q --allow-empty -m "a1"
A_SNAP="$(git -C "$AGED" rev-parse HEAD)"
git -C "$AGED" reset -q --hard "$A_SNAP~1"
NOW_UTC="$(python3 -c 'import time; print(time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))')"
write_event "$AGED" main "$A_SNAP" "$(git -C "$AGED" rev-parse HEAD)" protected-ref-moved "$NOW_UTC"
AGE_LINE="$(python3 "$PRED" list 2>/dev/null | grep "$AGED" | head -1)"
if printf '%s' "$AGE_LINE" | grep -q "0 min ago"; then
    ok "P14 a finding written seconds ago is '0 min ago' — the timestamp is read as the UTC the log writes, not as local standard time"
else
    bad "P14 the age is read as UTC" "a row stamped $NOW_UTC came out as: $AGE_LINE"
fi

# ===========================================================================
# P15 — UNRELATED TRAFFIC DOES NOT MAKE AN UNANSWERED FINDING DISAPPEAR
# ===========================================================================
# The log is shared with every workspace event. An open protected-ref row that
# has more than MAX_LINES ordinary rows after it must still be reported: the
# bound is on protected-ref rows, not on the file's last lines (P5-62). The
# control is the same row with no traffic after it, which is reported either way.
BUSY="$SANDBOX/busy"
mk_repo "$BUSY"
git -C "$BUSY" commit -q --allow-empty -m "b1"
B_SNAP="$(git -C "$BUSY" rev-parse HEAD)"
git -C "$BUSY" reset -q --hard "$B_SNAP~1"
write_event "$BUSY" main "$B_SNAP" "$(git -C "$BUSY" rev-parse HEAD)"
BEFORE_LINE="$(python3 "$PRED" list 2>/dev/null | grep -c "$BUSY")"
python3 - "$EVENTS" <<'PY'
import json, sys
with open(sys.argv[1], "a", encoding="utf-8") as f:
    for i in range(20050):
        f.write(json.dumps({"event": "workspace-heartbeat", "n": i, "ts": "2026-09-30T00:00:00Z"}) + "\n")
PY
AFTER_LINE="$(python3 "$PRED" list 2>/dev/null | grep -c "$BUSY")"
if [ "${BEFORE_LINE:-0}" -ge 1 ] && [ "${AFTER_LINE:-0}" -ge 1 ]; then
    ok "P15 an unanswered finding is still reported after 20,050 unrelated log rows follow it"
else
    bad "P15 unrelated traffic erased an open finding" "reported before=$BEFORE_LINE after=$AFTER_LINE"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    printf '  %s/%s cases passed\n' "$PASS" "$PASS"
    exit 0
fi
printf '  %s passed, %s FAILED\n' "$PASS" "$FAIL"
exit 1

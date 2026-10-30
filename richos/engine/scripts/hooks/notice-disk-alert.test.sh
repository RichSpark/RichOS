#!/usr/bin/env bash
#
# notice-disk-alert.test.sh — THE ALERT MUST REACH RICH, AND MUST NOT BECOME
#                             WALLPAPER ON A HEALTHY MACHINE.
#
#   D1  a healthy machine: SILENT on both events, exit 0.
#   D2  SessionStart renders the WHOLE BLOCK on BOTH channels (systemMessage
#       for the operator, additionalContext for the model). stderr is never
#       used — measured 2026-09-14 to reach no channel at SessionStart.
#   D3  Stop renders ONE LINE. The host prefixes every line with "Stop says: ",
#       so a block there is noise.
#   D4  the Stop line carries the FACTS: the volume, the free space, the stuck
#       paths and the classification.
#   D5  IT REPEATS EVERY TURN while the condition holds. Every other Stop notice
#       in this engine announces on entry and goes quiet; the CEO asked for this
#       one to repeat "every turn until the condition clears", so the case that
#       matters is the SECOND consecutive turn.
#   D6  ...and goes silent the moment it clears.
#   D7  THE ALERT CANNOT BE SILENCED BY CONFIGURATION. DISK_WATCHDOG_ENABLE=0
#       stands down the scheduled reading and the CEO's notifications; an alert
#       with an off switch is not an alert.
#   D8  the hook never writes the watchdog's state file, so a turn end cannot
#       destroy the baseline the drop arithmetic subtracts from.
#   D9  exit is ALWAYS 0. A notice that takes the session down with it is worse
#       than no notice.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$HOOK_DIR/notice-disk-alert.sh"
SCRIPTS="$(cd "$HOOK_DIR/.." && pwd)"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/disk-alert-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

echo "=== notice-disk-alert tests ==="

CFG="$SANDBOX/orchestration.config"
STATE="$SANDBOX/state.json"
LOG="$SANDBOX/watchdog.log"
LA="$SANDBOX/LaunchAgents"
FAILS="$SANDBOX/scratch-failures.json"
LEDGER_DIR="$SANDBOX/claude"
mkdir -p "$LA" "$LEDGER_DIR/state"
touch "$LA/com.richos.disk-watchdog.plist"      # installed, so D1 can be silent
# AND A HEALTHY SCHEDULED CLEANUP (cleanup PRD R4): its job installed and a result
# it published just now. A missing or manual result is a cleanup alarm (D11).
touch "$LA/com.richos.scratch-reaper.plist"
healthy_reaper() {         # <path> [extra python dict items]
    python3 - "$1" "${2:-}" <<'PY'
import json, sys, time
extra = eval("{%s}" % sys.argv[2]) if sys.argv[2] else {}
st = {"source": "launchd", "outcome": "healthy", "unresolved": [], "roots": [],
      "last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
      "last_apply_epoch": int(time.time()), "deleted": 0, "freed": 0,
      "undecidable": 0, "undecidable_bytes": 0, "skipped": 0, "skipped_bytes": 0,
      "failures": 0, "verdict": "decided"}
st.update(extra)
json.dump(st, open(sys.argv[1], "w"))
PY
}
healthy_reaper "$LEDGER_DIR/state/scratch-reaper-state.json"

# write_cfg <rich-floor-gb> [enable]
write_cfg() {
    cat >"$CFG" <<CFG
DISK_WATCHDOG_ENABLE="${2:-1}"
DISK_PRIMARY_VOLUME="/System/Volumes/Data"
DISK_EXTRA_VOLUMES=""
DISK_RICH_ALERT_GB="$1"
DISK_DROP_ALERT_GB="999999"
# The per-hour fall rate has its own threshold (default 80). Without this line the
# suite read the HOST's real fall rate on the machine's own working drive (an extra
# volume taken from machine settings) and failed whenever proof runs wrote to it.
DISK_DROP_ALERT_GB_PER_HOUR="999999999"
DISK_CEO_NOTIFY_GB="0"
DISK_CEO_URGENT_GB="0"
DISK_CEO_REPEAT_HOURS="24"
DISK_SCALE_EXTRA_VOLUMES="1"
DISK_WATCHDOG_MINUTES="15"
DISK_CONSUMER_CANDIDATES="$SANDBOX"
DISK_STATE_JSON="$STATE"
SCRATCH_ROOT_NAME="richos-scratch"
SCRATCH_CLAUDE_ROOTS="$SANDBOX/claude-%u"
SCRATCH_FAILURES_STATE="$FAILS"
CFG
}

# The hook's own ledger lives under CLAUDE_CONFIG_DIR, so the de-duplication
# state is this suite's and never the operator's.
run_hook() {                # <event>
    DISK_WATCHDOG_CONFIG="$CFG" \
    DISK_WATCHDOG_LOG="$LOG" \
    RICHOS_LAUNCH_AGENTS_DIR="$LA" \
    CLAUDE_CONFIG_DIR="$LEDGER_DIR" \
    bash "$HOOK" --event "$1" </dev/null 2>"$SANDBOX/err.txt"
}

# ---------------------------------------------------------------------------
# D1 — a healthy machine is silent on both events
# ---------------------------------------------------------------------------
write_cfg 1
OUT_SS="$(run_hook SessionStart)"; RC_SS=$?
OUT_ST="$(run_hook Stop)"; RC_ST=$?
if [ "$RC_SS" = "0" ] && [ -z "$OUT_SS" ]; then
    ok "D1  SessionStart is silent on a healthy machine"
else
    bad "D1  SessionStart spoke on a healthy machine (rc=$RC_SS)"
    printf '%s\n' "$OUT_SS" | sed 's/^/        /' | head -5
fi
# Stop may legitimately emit a recovery notice; what it must not do is alert.
if [ "$RC_ST" = "0" ] && ! printf '%s' "$OUT_ST" | grep -q 'MASSIVE ALERT'; then
    ok "D1b Stop raises no alert on a healthy machine"
else
    bad "D1b Stop alerted on a healthy machine (rc=$RC_ST)"
    printf '%s\n' "$OUT_ST" | sed 's/^/        /' | head -5
fi

# ---------------------------------------------------------------------------
# D2 — SessionStart carries the whole block on both channels
# ---------------------------------------------------------------------------
write_cfg 999999                                  # an unreachable floor
python3 - "$FAILS" <<'PY'
import json, sys
json.dump({"/tmp/stuck-forever": {
    "first": "2026-09-18T07:40:05Z", "last": "2026-09-18T08:00:00Z",
    "error": "[Errno 1] Operation not permitted", "attempts": 7}},
    open(sys.argv[1], "w"), indent=1)
PY
OUT="$(run_hook SessionStart)"; RC=$?
if [ "$RC" = "0" ] && printf '%s' "$OUT" | python3 -c '
import json, sys
o = json.load(sys.stdin)
sm = o.get("systemMessage") or ""
ac = (o.get("hookSpecificOutput") or {}).get("additionalContext") or ""
assert "MASSIVE ALERT" in sm, "systemMessage carries no alert"
assert sm == ac, "the two channels disagree"
assert sm.count("\n") > 3, "the block was flattened to a line"
' 2>/dev/null; then
    ok "D2  SessionStart emits the whole block on BOTH channels"
else
    bad "D2  the SessionStart payload is wrong (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -5
fi
if [ ! -s "$SANDBOX/err.txt" ]; then
    ok "D2b nothing is written to stderr, which reaches no channel"
else
    bad "D2b the hook wrote to stderr"
    sed 's/^/        /' "$SANDBOX/err.txt" | head -4
fi

# ---------------------------------------------------------------------------
# D3 / D4 — Stop is one line, and the line carries the facts
# ---------------------------------------------------------------------------
OUT="$(run_hook Stop)"; RC=$?
MSG="$(printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin).get("systemMessage") or ""))
except Exception:
    print("")
' 2>/dev/null)"
if [ "$RC" = "0" ] && [ "$(printf '%s' "$MSG" | wc -l | tr -d ' ')" = "0" ] \
   && [ -n "$MSG" ]; then
    ok "D3  Stop emits exactly ONE line"
else
    bad "D3  the Stop message is empty or multi-line (rc=$RC)"
    printf '%s\n' "$MSG" | sed 's/^/        /' | head -6
fi
if printf '%s' "$MSG" | grep -q 'MASSIVE ALERT' \
   && printf '%s' "$MSG" | grep -q '/System/Volumes/Data' \
   && printf '%s' "$MSG" | grep -q 'free' \
   && printf '%s' "$MSG" | grep -q 'could not be deleted' \
   && printf '%s' "$MSG" | grep -q 'classification'; then
    ok "D4  the Stop line carries volume, free space, stuck paths, class"
else
    bad "D4  the Stop line is missing a fact"
    printf '%s\n' "$MSG" | sed 's/^/        /'
fi

# ---------------------------------------------------------------------------
# D5 — IT REPEATS. The second consecutive turn is the case that matters.
# ---------------------------------------------------------------------------
# Every other Stop notice in this engine de-duplicates and would be SILENT here.
sleep 1
OUT2="$(run_hook Stop)"
MSG2="$(printf '%s' "$OUT2" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin).get("systemMessage") or ""))
except Exception:
    print("")
' 2>/dev/null)"
if printf '%s' "$MSG2" | grep -q 'MASSIVE ALERT'; then
    ok "D5  the alert repeats on the NEXT turn while the condition holds"
else
    bad "D5  the alert went quiet on the second turn. The CEO asked for it to be"
    bad "     repeated every turn until the condition clears."
    printf '%s\n' "$MSG2" | sed 's/^/        /' | head -4
fi

# ---------------------------------------------------------------------------
# D6 — and it clears
# ---------------------------------------------------------------------------
write_cfg 1
rm -f "$FAILS"
sleep 1
OUT3="$(run_hook Stop)"
if ! printf '%s' "$OUT3" | grep -q 'MASSIVE ALERT'; then
    ok "D6  the alert stops the moment the condition clears"
else
    bad "D6  the alert persisted after the condition cleared"
    printf '%s\n' "$OUT3" | sed 's/^/        /' | head -4
fi

# ---------------------------------------------------------------------------
# D7 — it cannot be silenced by configuration
# ---------------------------------------------------------------------------
write_cfg 999999 0                                # unreachable floor, ENABLE=0
OUT="$(run_hook SessionStart)"
if printf '%s' "$OUT" | grep -q 'MASSIVE ALERT'; then
    ok "D7  DISK_WATCHDOG_ENABLE=0 does NOT silence the alert"
else
    bad "D7  the alert was silenced by configuration. An alert with an off"
    bad "     switch is not an alert."
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -4
fi
# THE CONTROL: the same flag DOES stand down a scheduled reading, so D7 is
# measuring the alert path specifically and not a flag that does nothing at all.
OUT="$(DISK_WATCHDOG_CONFIG="$CFG" DISK_WATCHDOG_LOG="$LOG" \
       RICHOS_LAUNCH_AGENTS_DIR="$LA" bash "$SCRIPTS/disk-watchdog.sh" --check 2>&1)"
if printf '%s' "$OUT" | grep -q 'STOOD DOWN'; then
    ok "D7b CONTROL: the same flag DOES stand down the scheduled reading"
else
    bad "D7b the flag stands nothing down, so D7 proves nothing"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -4
fi

# ---------------------------------------------------------------------------
# D8 — the hook never moves the drop baseline
# ---------------------------------------------------------------------------
write_cfg 999999
DISK_WATCHDOG_CONFIG="$CFG" DISK_WATCHDOG_LOG="$LOG" \
    RICHOS_LAUNCH_AGENTS_DIR="$LA" bash "$SCRIPTS/disk-watchdog.sh" --check >/dev/null 2>&1
SUM_BEFORE="$(python3 -c "
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$STATE")"
run_hook SessionStart >/dev/null 2>&1
run_hook Stop >/dev/null 2>&1
SUM_AFTER="$(python3 -c "
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$STATE")"
if [ "$SUM_BEFORE" = "$SUM_AFTER" ]; then
    ok "D8  the hook leaves the watchdog state byte-identical"
else
    bad "D8  the hook rewrote the state file and moved the drop baseline"
fi

# ---------------------------------------------------------------------------
# D9 — exit is always 0, even with everything broken
# ---------------------------------------------------------------------------
BAD_CFG="$SANDBOX/not-a-config"
RC=0
DISK_WATCHDOG_CONFIG="$BAD_CFG" CLAUDE_CONFIG_DIR="$LEDGER_DIR" \
    RICHOS_LAUNCH_AGENTS_DIR="$LA" bash "$HOOK" --event SessionStart >/dev/null 2>&1 || RC=$?
RC2=0
DISK_WATCHDOG_CONFIG="$BAD_CFG" CLAUDE_CONFIG_DIR="$LEDGER_DIR" \
    RICHOS_LAUNCH_AGENTS_DIR="$LA" bash "$HOOK" --event Stop </dev/null >/dev/null 2>&1 || RC2=$?
if [ "$RC" = "0" ] && [ "$RC2" = "0" ] ; then
    ok "D9  a missing config still exits 0 on both events"
else
    bad "D9  the hook exited non-zero (SessionStart=$RC Stop=$RC2)"
fi

# ---------------------------------------------------------------------------
# D10 — THE GARBAGE ALARM SURVIVES THE ONE-LINE SUMMARY
# ---------------------------------------------------------------------------
# The Stop line is assembled by a parser, which means every new condition in the
# block has to be taught to it or it vanishes at the turn end. Proven while this
# was written: the first draft of that parser dropped the CLASSIFICATION line and
# D4 caught it. A summary that omits the only condition present is worse than no
# summary, because it teaches the eye that the line carries nothing.
write_cfg 1                                   # no volume or failure condition
rm -f "$FAILS"
mkdir -p "$SANDBOX/rstate"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"skipped": 2934, "skipped_bytes": 1125454451'
{
    echo "SCRATCH_REAPER_STATE=\"$SANDBOX/rstate/scratch-reaper-state.json\""
    echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1073741824"'
    echo 'SCRATCH_UNDECIDABLE_NOTICE_BYTES="67108864"'
} >>"$CFG"
sleep 1
OUT="$(run_hook Stop)"; RC=$?
MSG="$(printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin).get("systemMessage") or ""))
except Exception:
    print("")
' 2>/dev/null)"
# AMENDED 2026-09-19, and the amendment is the CEO's own question. This case
# used to assert that 1.05 GB of OTHER PROGRAMS' temporary files reached the
# turn-end line, and the line it produced was the one he read out: "MASSIVE
# ALERT — DISK: ... 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT ...".
# Visual Studio Code's updater and Codex's dot-directories are not ours to
# delete under any circumstances, so a line about them under every turn is
# wallpaper by construction — and wallpaper is what makes the next real alert
# get skipped. It stays in the SessionStart block, where it is information.
if [ "$RC" = "0" ] && [ -z "$MSG" ]; then
    ok "D10  other programs' temp does NOT reach the turn-end line (it is not ours)"
else
    bad "D10  the turn end still reports files nobody may delete (rc=$RC)"
    printf '%s\n' "$MSG" | sed 's/^/        /' | head -4
fi
# ...AND THE HALF THAT DOES REACH IT. A campaign root past its retention is
# OURS, it is never deleted automatically by design, and a person gets the space
# back by running a command the full reading prints. That is worth a turn-end
# line, and it must NOT wear the words MASSIVE ALERT: nothing is broken.
python3 - "$SANDBOX/rstate/scratch-reaper-state.json" <<'PY'
import json, sys, time
json.dump({"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "last_apply_epoch": int(time.time()),
           "source": "launchd", "outcome": "healthy", "unresolved": [], "roots": [],
           "deleted": 0, "freed": 0, "undecidable": 0, "undecidable_bytes": 0,
           "skipped": 2935, "skipped_bytes": 14958789966,
           "skipped_split": {
               "by_hand": {"count": 1, "bytes": 13749244608, "paths": [
                   {"path": "/Users/x/ab/campaign-done", "bytes": 13749244608,
                    "why": "PAST ITS RETENTION ... a person removes it:  "
                           "rm -rf /Users/x/ab/campaign-done"}]},
               "foreign": {"count": 2934, "bytes": 1125454451}},
           "failures": 0, "verdict": "decided"}, open(sys.argv[1], "w"))
PY
sleep 1
OUT="$(run_hook Stop)"; RC=$?
MSG="$(printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin).get("systemMessage") or ""))
except Exception:
    print("")
' 2>/dev/null)"
if [ "$RC" = "0" ] && printf '%s' "$MSG" | grep -q 'waits for a person to remove it' \
   && ! printf '%s' "$MSG" | grep -q 'MASSIVE ALERT'; then
    ok "D10a OUR by-hand pile DOES reach it, and without the words MASSIVE ALERT"
else
    bad "D10a the reclaimable pile is missing, or it is still an alarm (rc=$RC)"
    printf '%s\n' "$MSG" | sed 's/^/        /' | head -4
fi
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"skipped": 2934, "skipped_bytes": 1125454451'
# THE CONTROL: with the pile below its threshold the turn end is silent again.
sed -i.bak 's/^SCRATCH_SKIPPED_NOTICE_BYTES=.*/SCRATCH_SKIPPED_NOTICE_BYTES="9999999999999"/' "$CFG"
sleep 1
OUT="$(run_hook Stop)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'MASSIVE ALERT'; then
    ok "D10b CONTROL: below the threshold the turn end raises nothing at all"
else
    bad "D10b the turn end alerted with nothing above its threshold (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -4
fi

# ---------------------------------------------------------------------------
# D11 — AN UNRESOLVED SCHEDULED CLEANUP IS ONE SHORT ITEM ON THE TURN-END LINE
# ---------------------------------------------------------------------------
# Cleanup PRD R5: the notice reads the same current health as the watchdog and
# points at the same owner action, at most one short summary, never a list.
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": [{"scope": "/x/a", "cause": "2 undecided entries"}, {"scope": "/x/b", "cause": "1 deletion failed"}]'
OUT="$(run_hook Stop)"; RC=$?
MSG="$(printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin).get("systemMessage") or ""))
except Exception:
    print("")
' 2>/dev/null)"
if [ "$RC" = "0" ] && [ "$(printf '%s' "$MSG" | grep -o 'cleanup has not recovered:' | wc -l | tr -d ' ')" = "1" ] \
   && printf '%s' "$MSG" | grep -q '(+1 more)' && [ "$(printf '%s' "$MSG" | wc -l | tr -d ' ')" = "0" ]; then
    ok "D11  an unresolved scheduled cleanup is ONE item on the one turn-end line"
else
    bad "D11  the unresolved cleanup is missing from the turn-end line, or repeated (rc=$RC)"
    printf '%s\n' "$MSG" | sed 's/^/        /' | head -4
fi
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json"
OUT="$(run_hook Stop)"
if ! printf '%s' "$OUT" | grep -q 'cleanup has not recovered'; then
    ok "D11b CONTROL: with the scheduled result healthy again it is gone"
else
    bad "D11b the cleanup item stayed after the scheduled result recovered"
fi

# ---------------------------------------------------------------------------
# D12 — KEPT LEFTOVERS ARE TOLD ONCE PER SET; EVERY ALARM STILL REPEATS
# ---------------------------------------------------------------------------
# CEO, 2026-10-24: "remove obsolete instructions and repeated administrative
# noise without hiding genuine failures". The mixed state of 16:00:44Z that day
# had no failure, only leftovers the scheduled cleanup kept on purpose, and the
# turn end printed MASSIVE ALERT about them every turn. A REAL session here: a
# payload with a session id and an adopted root, so the ledger exists and the
# once-per-set memory is the one a lead's session has.
SESSION_PAYLOAD="$SANDBOX/payload.json"
printf '{"session_id": "d12sess-0001", "cwd": "%s"}\n' "$SANDBOX" >"$SESSION_PAYLOAD"
run_session_hook() {
    DISK_WATCHDOG_CONFIG="$CFG" \
    DISK_WATCHDOG_LOG="$LOG" \
    RICHOS_LAUNCH_AGENTS_DIR="$LA" \
    CLAUDE_CONFIG_DIR="$LEDGER_DIR" \
    RICHOS_ENTITY_ROOT="$SANDBOX" \
    bash "$HOOK" --event Stop <"$SESSION_PAYLOAD" 2>"$SANDBOX/err.txt" | python3 -c '
import json, sys
text = sys.stdin.read().strip()
try:
    print((json.loads(text).get("systemMessage") or "") if text else "")
except Exception:
    print("UNPARSEABLE: " + text)
'
}
KEPT_ROWS='{"scope": "/v/ab/worktrees", "class": "codex-worktree", "condition": "main-unreadable", "count": 1, "bytes": 0, "identity": "aaaa", "cause": "1 undecided codex-worktree entry (0 B), e.g. /v/ab/worktrees/hh/prospects: main of /u/prospects cannot be read"}, {"scope": "/v/codex/worktrees", "class": "codex-worktree", "condition": "chat-attached", "count": 13, "bytes": 0, "identity": "bbbb", "cause": "13 undecided codex-worktree entries (0 B), e.g. /v/codex/worktrees/a/richos: finished, but a Codex chat still has it attached, so it is kept"}, {"scope": "/v/tmp/codex", "class": "tmp-unknown", "condition": "wall-2", "count": 13, "bytes": 6299191717, "identity": "cccc", "cause": "13 undecided tmp-unknown entries (5.9 GB), e.g. /v/tmp/codex/h: wall 2: this tree contains a .git"}'
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "failures": 0, "undecidable": 27, "verdict": "undecided", "unresolved": ['"$KEPT_ROWS"']'
M1="$(run_session_hook)"; M2="$(run_session_hook)"
if printf '%s' "$M1" | grep -q '^Cleanup, kept or undecided: 27 entries in 3 group(s) the scheduled run of .* left in place and will not delete on its own; told once per set' \
   && ! printf '%s' "$M1" | grep -q 'MASSIVE ALERT' && [ -z "$M2" ]; then
    ok "D12  kept leftovers with no failure: one honest short line, then quiet the next turn"
else
    bad "D12  kept leftovers alarmed or repeated (turn 1: $M1 | turn 2: $M2)"
fi
# Only sizes and times change: still the same set, still quiet.
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "last_apply_epoch": int(time.time()) - 600, "unresolved": ['"$(printf '%s' "$KEPT_ROWS" | sed 's/6299191717/7299191717/; s/(5.9 GB)/(6.8 GB)/')"']'
M3="$(run_session_hook)"
if [ -z "$M3" ]; then
    ok "D12a an unchanged set whose sizes and times moved stays quiet"
else
    bad "D12a a size or time change re-told the same kept set: $M3"
fi
# A new leftover changes the set: told once more, then quiet again.
NEW_ROWS="$(printf '%s' "$KEPT_ROWS" | sed 's/"count": 13, "bytes": 0, "identity": "bbbb"/"count": 14, "bytes": 0, "identity": "bbbd"/')"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$NEW_ROWS"']'
M4="$(run_session_hook)"; M5="$(run_session_hook)"
if printf '%s' "$M4" | grep -q '^Cleanup, kept or undecided: 28 entries' && [ -z "$M5" ]; then
    ok "D12b a new leftover is told once more, and the new set then goes quiet"
else
    bad "D12b a changed kept set was not told once (turn 1: $M4 | turn 2: $M5)"
fi
# The SAME entries in a changed condition are a changed set: told once more.
COND_ROWS="$(printf '%s' "$NEW_ROWS" | sed 's/"condition": "main-unreadable", "count": 1, "bytes": 0, "identity": "aaaa"/"condition": "chat-attached", "count": 1, "bytes": 0, "identity": "aaab"/')"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$COND_ROWS"']'
M6="$(run_session_hook)"; M7="$(run_session_hook)"
if printf '%s' "$M6" | grep -q '^Cleanup, kept or undecided: 28 entries' && [ -z "$M7" ]; then
    ok "D12b2 the same entries in a changed condition are told once more"
else
    bad "D12b2 a changed condition on the same entries was not told (turn 1: $M6 | turn 2: $M7)"
fi
NEW_ROWS="$COND_ROWS"
# EVERY ALARM STILL REPEATS EVERY TURN beside the told set: a real deletion
# failure, a failed (refused) run, a stale run, and low disk.
alarm_twice() {          # <label> ; prints PASS/FAIL for two consecutive turns
    local a b
    a="$(run_session_hook)"; b="$(run_session_hook)"
    if printf '%s' "$a" | grep -q "^MASSIVE ALERT — DISK: .*$2" \
       && printf '%s' "$b" | grep -q "^MASSIVE ALERT — DISK: .*$2" \
       && ! printf '%s' "$b" | grep -q 'Cleanup, kept or undecided'; then
        ok "$1"
    else
        bad "$1 (turn 1: $a | turn 2: $b)"
    fi
}
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "failures": 1, "unresolved": ['"$NEW_ROWS"', {"scope": "/v/tmp", "class": "deletion", "count": 1, "cause": "1 deletion(s) failed, e.g. /v/tmp/x: permission denied"}]'
alarm_twice "D12c a new real deletion failure beside the told set alerts, every turn" "cleanup has not recovered: /v/tmp: 1 deletion(s) failed"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": [{"scope": "run", "cause": "the run refused: lsof is not on PATH"}]'
alarm_twice "D12d a failed scheduled run alerts, every turn" "cleanup has not recovered: run: the run refused"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "last_apply_epoch": int(time.time()) - 13 * 3600, "unresolved": ['"$NEW_ROWS"']'
alarm_twice "D12e a stale scheduled run alerts, every turn" "past the 12 h window"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$NEW_ROWS"']'
cp "$CFG" "$CFG.d12"
sed -i.bak 's/^DISK_RICH_ALERT_GB=.*/DISK_RICH_ALERT_GB="999999"/' "$CFG"
alarm_twice "D12f low disk beside the told set alerts, every turn" "below the declared"
cp "$CFG.d12" "$CFG"
# A failed test device cleanup is an alarm too (the review of 6d7802f2e found
# it silent at the turn end): beside the told set it alerts every turn.
printf '%s\n' '{"sim-1": {"name": "Test iPhone", "verdict": "removal failed", "why": "simctl delete refused", "command": "xcrun simctl delete sim-1", "first": "2026-10-24T18:00:00Z", "attempts": 2}}' >"$LEDGER_DIR/state/test-device-failures.json"
alarm_twice "D12g a failed test device cleanup beside the told set alerts, every turn" "1 test device cleanup issue(s) need attention"
rm -f "$LEDGER_DIR/state/test-device-failures.json"
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json"

# ---------------------------------------------------------------------------
# D13 — OUR PILE WAITING FOR A PERSON IS ADMINISTRATION TOO: TOLD ONCE PER SET
# ---------------------------------------------------------------------------
# A campaign root past its retention is not an alarm (D10a). It is told once a
# session, and again only when WHICH places it is changes; sizes never do. A
# real alarm beside it still repeats every turn.
by_hand_sized() {        # <path>=<bytes> [<path>=<bytes> ...]
    # The producer's shape (scratch-reaper.py by_hand_record): the five largest
    # places as examples, and an identity over EVERY place, sizes left out.
    python3 - "$SANDBOX/rstate/scratch-reaper-state.json" "$@" <<'PY'
import hashlib, json, sys, time
out = sys.argv[1]
rows = [(p, int(b)) for p, b in (a.rsplit("=", 1) for a in sys.argv[2:])]
total = sum(b for _p, b in rows)
shown = sorted(rows, key=lambda r: -r[1])[:5]
identity = hashlib.sha1("\n".join(sorted(p for p, _b in rows)).encode(
    "utf-8", "surrogateescape")).hexdigest()[:16]
json.dump({"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "last_apply_epoch": int(time.time()),
           "source": "launchd", "outcome": "healthy", "unresolved": [], "roots": [],
           "deleted": 0, "freed": 0, "undecidable": 0, "undecidable_bytes": 0,
           "skipped": len(rows), "skipped_bytes": total,
           "skipped_split": {
               "by_hand": {"count": len(rows), "bytes": total, "identity": identity,
                           "paths": [
                   {"path": p, "bytes": b,
                    "why": "PAST ITS RETENTION ... a person removes it:  rm -rf " + p}
                   for p, b in shown]},
               "foreign": {"count": 0, "bytes": 0}},
           "failures": 0, "verdict": "decided"}, open(out, "w"))
PY
}
by_hand() {              # <bytes> <path> [<path> ...] — every place the same size
    local size="$1" p
    local args=()
    shift
    for p in "$@"; do args+=("$p=$size"); done
    by_hand_sized "${args[@]}"
}
by_hand 13749244608 /Users/x/ab/campaign-a
T1="$(run_session_hook)"; T2="$(run_session_hook)"
by_hand 14749244608 /Users/x/ab/campaign-a
T3="$(run_session_hook)"
if printf '%s' "$T1" | grep -q '^Disk: .* is ours and waits for a person to remove it; told once per set' \
   && [ -z "$T2" ] && [ -z "$T3" ]; then
    ok "D13  an unchanged by-hand pile is told once, and its size changing does not re-tell it"
else
    bad "D13  an unchanged by-hand pile repeated (turn 1: $T1 | turn 2: $T2 | bigger: $T3)"
fi
by_hand 13749244608 /Users/x/ab/campaign-a /Users/x/ab/campaign-b
T4="$(run_session_hook)"; T5="$(run_session_hook)"
if printf '%s' "$T4" | grep -q '^Disk: .* in 2 place(s) is ours and waits' && [ -z "$T5" ]; then
    ok "D13a a changed by-hand pile is told once more, then quiet"
else
    bad "D13a a changed by-hand pile was not told exactly once (turn 1: $T4 | turn 2: $T5)"
fi
# Mixed: low disk beside the told pile alerts every turn and does not repeat it;
# a new pile arriving beside the alarm is told once, inside the alarm line.
cp "$CFG" "$CFG.d13"
sed -i.bak 's/^DISK_RICH_ALERT_GB=.*/DISK_RICH_ALERT_GB="999999"/' "$CFG"
T6="$(run_session_hook)"; T7="$(run_session_hook)"
by_hand 13749244608 /Users/x/ab/campaign-c
T8="$(run_session_hook)"; T9="$(run_session_hook)"
cp "$CFG.d13" "$CFG"
if printf '%s' "$T6" | grep -q '^MASSIVE ALERT — DISK: .*below the declared' \
   && printf '%s' "$T7" | grep -q '^MASSIVE ALERT — DISK: .*below the declared' \
   && ! printf '%s%s' "$T6" "$T7" | grep -q 'waits for a person' \
   && printf '%s' "$T8" | grep -q '^MASSIVE ALERT — DISK: .*below the declared.*| Disk: .* is ours and waits' \
   && printf '%s' "$T9" | grep -q '^MASSIVE ALERT — DISK: .*below the declared' \
   && ! printf '%s' "$T9" | grep -q 'waits for a person'; then
    ok "D13b beside a real alarm the alarm repeats every turn and the pile is told once per set"
else
    bad "D13b mixed alarm and pile (T6: $T6 | T7: $T7 | T8: $T8 | T9: $T9)"
fi
# MORE THAN FIVE PLACES: the alert shows the five largest, but WHICH places it is
# covers all of them (review rv-20261024T184407Z-f26655a1-5189). A place hidden
# below the five is replaced: told once. The same places with their sizes
# reordered, so a different five are shown: quiet.
G="/Users/x/ab/campaign"
by_hand_sized "$G-1=6000000000" "$G-2=5000000000" "$G-3=4000000000" "$G-4=3000000000" "$G-5=2000000000" "$G-6=1000000000"
T10="$(run_session_hook)"
by_hand_sized "$G-1=6000000000" "$G-2=5000000000" "$G-3=4000000000" "$G-4=3000000000" "$G-5=2000000000" "$G-7=1000000000"
T11="$(run_session_hook)"; T12="$(run_session_hook)"
if printf '%s' "$T10" | grep -q '^Disk: .* in 6 place(s) is ours and waits' \
   && printf '%s' "$T11" | grep -q '^Disk: .* in 6 place(s) is ours and waits' && [ -z "$T12" ]; then
    ok "D13c a place hidden below the five shown is replaced: told once, then quiet"
else
    bad "D13c a hidden membership change was not told exactly once (before: $T10 | replaced: $T11 | next: $T12)"
fi
by_hand_sized "$G-1=1000000000" "$G-2=2000000000" "$G-3=3000000000" "$G-4=4000000000" "$G-5=5000000000" "$G-7=6000000000"
T13="$(run_session_hook)"
if [ -z "$T13" ]; then
    ok "D13d the same places with reordered sizes (a different five shown) stay quiet"
else
    bad "D13d reordered sizes of the same places told the pile again: $T13"
fi
healthy_reaper "$SANDBOX/rstate/scratch-reaper-state.json"

echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

#!/usr/bin/env bash
#
# codex-watch.test.sh: CODEX'S MESSAGES WAKE THE LEAD (scripts/codex-watch.sh
# running scripts/lib/codex_watch.py), on fixture channels and fixture state
# only. Never the operator's ~/.richos-coordination files or ~/.claude/state.
#
# 2026-10-05: Codex's 08:11Z "READY TO LAND" entry sat unlanded for about five
# hours because the watcher on to-rich.md had to be started by hand. Every case
# below fails on a checkout without scripts/codex-watch.sh.
#
#   C01  END TO END: --monitor, an entry appended to rich-codex/to-rich.md:
#        exactly ONE wake, carrying every line of the entry, and none of the
#        backlog that was there before
#   C02  two entries appended at once to the questions channel: two wakes,
#        each with its own lines, naming that channel
#   C03  an entry written in two parts: nothing while it grows, then ONE wake
#        with both parts
#   C04  the next look says nothing (told once)
#   C05  a NEW session starts where the last one stopped: an entry written
#        while nothing watched is told, the older backlog is not
#   C06  the first start on a machine replays no backlog, and the next entry
#        is told
#   C07  monitors.json starts `codex-watch.sh --monitor` always
#   C08  a repository that never adopted the engine: silent, exit 0
#   C09  --monitor --host-json, the RichOS app's own child (richos-core
#        codex_watch.rs runs scripts/lib/codex_watch.py so, with no --config):
#        an appended entry is ONE JSON line carrying every line of it, and the
#        watcher ends once its host (the parent whose pid RICHOS_WATCH_HOST
#        names) is gone
#   C10  the user's setting: CODEX_CHANNELS in orchestration.config names the
#        channel folders; an entry in a configured folder is told, one in a
#        folder the setting leaves out is not; an empty value watches none
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/.." && pwd)"
CW="$SCRIPT_DIR/codex-watch.sh"

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

# shellcheck source=lib/scratch.sh
. "$SCRIPT_DIR/lib/scratch.sh"
SB="$(scratch_new codex-watch-test)" || { echo "FATAL: no scratch" >&2; exit 1; }

# Every process this suite signals is one it started, its pid captured at spawn.
PIDS=()
spawn_sleeper() { sh -c 'sleep 900 >/dev/null 2>&1 & echo $!'; }
cleanup() {
    local p
    for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
    scratch_release "$SB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }
check() { if [ "$2" -eq 0 ]; then ok "$1"; else bad "$1 -- $3"; fi; }
has()   { printf '%s' "$1" | grep -qF -- "$2"; }
count() { printf '%s' "$1" | grep -cF -- "$2" || true; }
wait_for() { # <seconds> <test command...>
    local n="$1"; shift
    local i=0
    while [ "$i" -lt $((n * 10)) ]; do "$@" && return 0; sleep 0.1; i=$((i + 1)); done
    return 1
}

# --- isolation -------------------------------------------------------------
unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT
SLEEPER="$(spawn_sleeper)"; PIDS+=("$SLEEPER")
export RICHOS_ENTITY_ROOT="$SB/entity"
export RICHOS_WORKSPACES_DIR="$SB/workspaces"
export RICHOS_SESSIONS_DIR="$SB/platform-sessions"
export RICHOS_SESSION_PID="$SLEEPER"
export RICHOS_SESSION_ID="beadfeed-0000-4000-8000-00000000c001"
export CODEX_WATCH_STATE_DIR="$SB/state"
CH1="$SB/coord/rich-codex/to-rich.md"
CH2="$SB/coord/rich-codex-questions/to-rich.md"
export CODEX_WATCH_CHANNELS="$CH1:$CH2"
mkdir -p "$RICHOS_ENTITY_ROOT" "$RICHOS_WORKSPACES_DIR" "$RICHOS_SESSIONS_DIR" "$(dirname "$CH1")" "$(dirname "$CH2")"
printf 'PROTECTED_PATHS=""\n' >"$RICHOS_ENTITY_ROOT/orchestration.config"
printf '## 2026-10-05 07:00Z — OLD BACKLOG ENTRY\nold-backlog-line\n\n' >"$CH1"
printf '## 2026-10-05 06:00Z — OLD QUESTION\nold-question-line\n\n' >"$CH2"

tick() { OUT="$(bash "$CW" --tick 2>&1)"; RC=$?; }
headers() { printf '%s\n' "$1" | grep -c '^CODEX-WATCH' || true; }

echo "=== codex-watch tests ==="
[ -f "$CW" ] || echo "  (scripts/codex-watch.sh is missing: every case below fails)"

# --- C01: the monitor, end to end -----------------------------------------
( CODEX_WATCH_POLL_SECONDS=1 exec bash "$CW" --monitor >"$SB/monitor.out" 2>"$SB/monitor.err" ) &
MON=$!; PIDS+=("$MON")
sleep 2.5                                   # its first looks: the backlog is where it starts
printf '## 2026-10-05 08:11Z — READY TO LAND: codex/split-proof-evidence\nBranch tip 3ec820348.\nfixture-line-three\n\n' >>"$CH1"
wait_for 15 grep -q '^CODEX-WATCH' "$SB/monitor.out"
sleep 3.5                                   # several more looks: it must not be told again
kill "$SLEEPER" 2>/dev/null                 # the session ends: so does its monitor
wait_for 15 sh -c "! kill -0 $MON 2>/dev/null"; ENDED=$?
MOUT="$(cat "$SB/monitor.out")"
check "C01  END TO END: one appended entry is ONE wake carrying all its lines, none of the backlog; it ends with its session" \
    "$( [ "$(headers "$MOUT")" -eq 1 ] && has "$MOUT" "rich-codex/to-rich.md" \
        && has "$MOUT" "## 2026-10-05 08:11Z — READY TO LAND: codex/split-proof-evidence" \
        && has "$MOUT" "Branch tip 3ec820348." && has "$MOUT" "fixture-line-three" \
        && ! has "$MOUT" "old-backlog-line" && [ "$ENDED" -eq 0 ]; echo $?)" \
    "out=$MOUT err=$(cat "$SB/monitor.err") ended=$ENDED"
SLEEPER="$(spawn_sleeper)"; PIDS+=("$SLEEPER"); export RICHOS_SESSION_PID="$SLEEPER"

# --- C02-C04: one session, look by look -----------------------------------
export RICHOS_SESSION_ID="beadfeed-0000-4000-8000-00000000c002"
tick; tick
printf '## 2026-10-05 09:00Z — Q1\nq1-a\nq1-b\n\n## 2026-10-05 09:01Z — Q2\nq2-a\n' >>"$CH2"
tick; FIRST="$OUT"; tick
check "C02  two entries at once: nothing while unsettled, then two wakes, each with its own lines, naming the channel" \
    "$( [ -z "$FIRST" ] && [ "$(headers "$OUT")" -eq 2 ] && [ "$(count "$OUT" "rich-codex-questions/to-rich.md")" -eq 2 ] \
        && has "$OUT" "q1-a" && has "$OUT" "q1-b" && has "$OUT" "q2-a" && ! has "$OUT" "old-question-line" \
        && [ "$(printf '%s\n' "$OUT" | awk '/^CODEX-WATCH/{n++} /q2-a/{print n}')" = 2 ]; echo $?)" \
    "first=$FIRST out=$OUT"
printf '## 2026-10-05 09:30Z — PARTIAL\npart-one\n' >>"$CH1"
tick; A="$OUT"
printf 'part-two\n\n' >>"$CH1"
tick; B="$OUT"; tick; C="$OUT"
check "C03  an entry written in two parts is ONE wake with both parts" \
    "$( [ -z "$A" ] && [ -z "$B" ] && [ "$(headers "$C")" -eq 1 ] && has "$C" "part-one" && has "$C" "part-two"; echo $?)" \
    "a=$A b=$B c=$C"
tick
check "C04  the next look says nothing" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"

# --- C05: a new session starts where the last one stopped -------------------
printf '## 2026-10-05 10:00Z — WRITTEN WHILE NOTHING WATCHED\nbetween-sessions\n\n' >>"$CH1"
export RICHOS_SESSION_ID="beadfeed-0000-4000-8000-00000000c003"
tick; X="$OUT"; tick; Y="$OUT"
check "C05  a new session tells what was written while nothing watched, and none of what was already told" \
    "$( [ "$(headers "$X$Y")" -eq 1 ] && has "$X$Y" "between-sessions" && ! has "$X$Y" "part-one" \
        && ! has "$X$Y" "old-backlog-line"; echo $?)" "x=$X y=$Y"

# --- C06: the very first start replays nothing ------------------------------
export CODEX_WATCH_STATE_DIR="$SB/state-fresh"
tick; X="$OUT"; tick; Y="$OUT"
printf '## 2026-10-05 11:00Z — AFTER A FRESH START\nfresh-line\n' >>"$CH2"
tick; tick; Z="$OUT"
check "C06  the first start on a machine replays no backlog; the next entry is told" \
    "$( [ -z "$X$Y" ] && [ "$(headers "$Z")" -eq 1 ] && has "$Z" "fresh-line"; echo $?)" "x=$X y=$Y z=$Z"

# --- C07: wiring --------------------------------------------------------------
python3 - "$ENGINE" <<'PY'; RC=$?
import json, os, sys
mons = json.load(open(os.path.join(sys.argv[1], "monitors", "monitors.json")))
m = [x for x in mons if x.get("name") == "codex-watch"]
sys.exit(0 if len(m) == 1 and m[0].get("when") == "always"
         and m[0].get("command", "").endswith("/scripts/codex-watch.sh --monitor") else 1)
PY
check "C07  monitors.json starts codex-watch.sh --monitor always" "$RC" "monitors.json wiring"

# --- C08: not adopted ---------------------------------------------------------
mkdir -p "$SB/not-adopted"
OUT="$(RICHOS_ENTITY_ROOT="$SB/not-adopted" bash "$CW" --monitor 2>&1)"; RC=$?
check "C08  a repository that never adopted the engine: the monitor is silent and exits 0" \
    "$( [ "$RC" -eq 0 ] && [ -z "$OUT" ] && [ -f "$CW" ]; echo $?)" "rc=$RC out=$OUT"

# --- C09: the app host's child -----------------------------------------------
# A stand-in host: a process that starts the watcher as its child, names itself in
# RICHOS_WATCH_HOST, and waits. Ending it is the app ending without a quit.
python3 - "$CW" "$SB" <<'PY' &
import os, subprocess, sys, time
cw, sb = sys.argv[1:3]
env = dict(os.environ, RICHOS_WATCH_HOST=str(os.getpid()), CODEX_WATCH_POLL_SECONDS="0.3",
           CODEX_WATCH_STATE_DIR=os.path.join(sb, "state-host"))
lib = os.path.join(os.path.dirname(cw), "lib", "codex_watch.py")
child = subprocess.Popen([sys.executable, lib, "--monitor", "--host-json"], env=env,
                         stdout=open(os.path.join(sb, "host.out"), "w"), stderr=open(os.path.join(sb, "host.err"), "w"))
with open(os.path.join(sb, "host.child.tmp"), "w") as fh:
    fh.write(str(child.pid))
os.replace(os.path.join(sb, "host.child.tmp"), os.path.join(sb, "host.child"))
time.sleep(900)
PY
HOST=$!; PIDS+=("$HOST")
wait_for 10 test -s "$SB/host.child"
CHILD="$(cat "$SB/host.child")"; PIDS+=("$CHILD")
sleep 1.5                                   # its first looks: where the channels end
printf '## 2026-10-05 12:00Z — READY TO LAND: codex/host-child\nhost-line-two\n' >>"$CH1"
wait_for 15 grep -q 'host-line-two' "$SB/host.out"
kill "$HOST" 2>/dev/null                    # the app is gone: so is its watcher
wait "$HOST" 2>/dev/null
wait_for 15 sh -c "! kill -0 $CHILD 2>/dev/null"; ENDED=$?
python3 - "$SB/host.out" <<'PY'; RC=$?
import json, sys
lines = [l for l in open(sys.argv[1]).read().splitlines() if l.strip()]
text = json.loads(lines[0])["text"] if len(lines) == 1 else ""
sys.exit(0 if text.startswith("CODEX-WATCH") and "## 2026-10-05 12:00Z — READY TO LAND: codex/host-child" in text
         and text.rstrip().endswith("host-line-two") else 1)
PY
check "C09  --host-json: ONE JSON line with every line of the entry; the watcher ends once its host is gone" \
    "$( [ "$RC" -eq 0 ] && [ "$ENDED" -eq 0 ]; echo $?)" \
    "out=$(cat "$SB/host.out") err=$(cat "$SB/host.err") ended=$ENDED"

# --- C10: the user's setting ---------------------------------------------------
# The setting names folders. CODEX_WATCH_CHANNELS, which the cases above and the app set, is unset.
CFG="$SB/configured"
mkdir -p "$CFG/team-codex" "$CFG/left-out"
printf '## 2026-10-05 06:00Z — CONFIGURED BACKLOG\nconfigured-backlog\n\n' >"$CFG/team-codex/to-rich.md"
printf '## 2026-10-05 06:00Z — LEFT OUT BACKLOG\nleft-out-backlog\n\n' >"$CFG/left-out/to-rich.md"
printf 'PROTECTED_PATHS=""\nexport CODEX_CHANNELS="%s" # the user setting\n' "$CFG/team-codex" \
    >"$RICHOS_ENTITY_ROOT/orchestration.config"
cfg_tick() { OUT="$(env -u CODEX_WATCH_CHANNELS CODEX_WATCH_STATE_DIR="$SB/state-config" bash "$CW" --tick 2>&1)"; }
cfg_tick; cfg_tick
printf '## 2026-10-05 13:00Z — FROM THE CONFIGURED CHANNEL\nconfigured-line\n' >>"$CFG/team-codex/to-rich.md"
printf '## 2026-10-05 13:00Z — FROM A FOLDER NOT CONFIGURED\nleft-out-line\n' >>"$CFG/left-out/to-rich.md"
cfg_tick; cfg_tick; X="$OUT"
printf 'PROTECTED_PATHS=""\nCODEX_CHANNELS=""\n' >"$RICHOS_ENTITY_ROOT/orchestration.config"
printf '## 2026-10-05 14:00Z — WHILE NONE IS WATCHED\nnone-watched-line\n' >>"$CFG/team-codex/to-rich.md"
cfg_tick; cfg_tick; Y="$OUT"
check "C10  CODEX_CHANNELS names the watched folders: the configured one is told, one it leaves out is not; empty watches none" \
    "$( [ "$(headers "$X")" -eq 1 ] && has "$X" "team-codex/to-rich.md" && has "$X" "configured-line" \
        && ! has "$X" "left-out-line" && ! has "$X" "configured-backlog" && [ -z "$Y" ]; echo $?)" "x=$X y=$Y"

if [ "$FAIL" -gt 0 ]; then
    echo "=== codex-watch tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== codex-watch tests: all $PASS passed ==="
exit 0

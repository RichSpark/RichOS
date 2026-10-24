#!/usr/bin/env bash
#
# session-start-scratch.test.sh — THE NOTICE MUST BE SILENT, AUDIBLE, AND
#                                 HARMLESS, IN THAT ORDER OF DIFFICULTY.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# A SessionStart notice has three ways to fail and only one of them is loud:
#
#   1. IT SPEAKS WHEN THERE IS NOTHING TO SAY. Every banner this engine ships
#      competes for the same two seconds of attention, and the one that talks
#      on a healthy machine is the one that trains people to skip all of them.
#   2. IT SPEAKS WHERE NOBODY HEARS. Measured 2026-09-14 across this engine's
#      hooks: stderr at SessionStart renders to NO channel, and the CI notice
#      spent its life writing a live finding to it
#      (docs/verification/check-census-2026-09-14.md). This one carries its
#      text on systemMessage AND additionalContext, and the suite checks both
#      rather than checking that "something was printed".
#   3. IT BREAKS THE SESSION. A hook that exits non-zero, or that hangs on a
#      slow disk, costs every session start on the machine.
#
#   N1  a healthy machine gets SILENCE and exit 0.
#   N2  more than the declared threshold reclaimable -> it says so, on BOTH
#       channels, and still exits 0.
#   N3  the scheduled job NOT INSTALLED -> it says so. This is the
#       who-watches-the-watchman leg: a deleter that was never scheduled looks
#       exactly like a machine with no garbage on it.
#   N4  the job installed but with NO record of a pass -> it says so.
#   N5  the job installed and its last pass long ago -> it says so, with the
#       age.
#   N6  a BROKEN reaper (deleted, or refusing) is silent about sizes and still
#       exits 0. A notice that takes the session down with it is worse than no
#       notice.
#   N7  STOOD DOWN by declaration -> completely silent.
#   N8  nothing is ever written to stderr.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$HOOK_DIR/session-start-scratch.sh"
ENGINE="$(cd "$HOOK_DIR/../.." && pwd)"

PASS=0; FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/scratch-notice-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

echo "=== session-start-scratch tests ==="

SCRATCH="$SANDBOX/scratch"
SESSIONS="$SANDBOX/sessions"
HOME_DIR="$SANDBOX/claude"
LAUNCHD="$SANDBOX/LaunchAgents"
NIGHTLY="$SANDBOX/nightly"
TMPD="$SANDBOX/tmp"
CFG="$SANDBOX/orchestration.config"
mkdir -p "$SCRATCH" "$SESSIONS" "$HOME_DIR/state" "$LAUNCHD" "$NIGHTLY" "$TMPD"

write_config() {           # <notice-bytes>
    {
        echo 'SCRATCH_REAPER_ENABLE="1"'
        echo "SCRATCH_SESSION_PROCESS_NAMES=\"$SUITE_PROC\""
        echo "SCRATCH_CLAUDE_ROOTS=\"$SCRATCH\""
        echo 'SCRATCH_TMP_PATTERNS="richos-*-workspace"'
        echo 'SCRATCH_AGE_FLOOR_MINUTES="0"'
        echo "SCRATCH_NIGHTLY_DIR=\"$NIGHTLY\""
        echo 'SCRATCH_NIGHTLY_KEEP="3"'
        echo "SCRATCH_NOTICE_BYTES=\"$1\""
        echo 'SCRATCH_REAPER_HOURS="4"'
        echo 'SCRATCH_REAPER_MINUTE="40"'
    } >"$CFG"
}

# THE SESSION PROCESS NAME IS UNIQUE TO THIS RUN, and nothing on the machine
# carries it — so the reaper sees an empty set of session processes and every
# dead scratch directory in the sandbox is decidable. This suite tests the
# NOTICE, not liveness; scratch-reaper.test.sh owns liveness and explains at
# length why a shared name makes both suites read the whole machine's process
# table and assert nothing.
SUITE_PROC="zsn${$}claude"
attribute() {
    local pid i=0 start
    for pid in $(LC_ALL=C LANG=C TZ=UTC0 ps -Ao pid=,comm= \
                 | awk -v want="$SUITE_PROC" \
                       '{ n=split($2, p, "/"); if (p[n] == want) print $1 }'); do
        i=$((i + 1))
        start="$(LC_ALL=C LANG=C TZ=UTC0 ps -o lstart= -p "$pid" | tr -s ' ' | sed 's/^ *//;s/ *$//')"
        python3 - "$SESSIONS/$pid.json" "$pid" "$i" "$start" <<'PY'
import json, sys
path, pid, i, start = sys.argv[1:5]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({"pid": int(pid), "procStart": start, "cwd": "/",
               "sessionId": "f0000000-0000-0000-0000-%012d" % int(i)}, fh)
PY
    done
}
attribute

OUT=""; ERR=""; RC=0
run_hook() {
    ERR="$SANDBOX/stderr.txt"
    OUT="$(SCRATCH_REAPER_CONFIG="$CFG" \
        RICHOS_SESSIONS_DIR="$SESSIONS" \
        CLAUDE_CONFIG_DIR="$HOME_DIR" \
        RICHOS_LAUNCH_AGENTS_DIR="$LAUNCHD" \
        SCRATCH_REAPER_STATE="$HOME_DIR/state/scratch-reaper-state.json" \
        TMPDIR="$TMPD" \
        bash "$HOOK" 2>"$ERR")"
    RC=$?
}

says() {                   # <needle> -> is it on BOTH audible channels?
    printf '%s' "$OUT" | python3 -c '
import json, sys
needle = sys.argv[1]
try:
    doc = json.load(sys.stdin)
except Exception:
    sys.exit(2)
a = needle in (doc.get("systemMessage") or "")
b = needle in (doc.get("hookSpecificOutput", {}).get("additionalContext") or "")
sys.exit(0 if (a and b) else 1)
' "$1"
}

# ===========================================================================
# N3 / N4 / N5 — THE SCHEDULED JOB'S HEALTH IS NOT REPEATED HERE (2026-10-23)
# ===========================================================================
# Uninstalled, never run, or thirty hours stale: each is the disk watchdog's
# cleanup alarm now (disk-watchdog.test.sh W23b, W23, W23c), delivered once with
# its owner action at this same session start (cleanup PRD R5). This notice says
# nothing about it, so the session start carries one summary of one failure.
write_config 1099511627776          # 1 TiB: nothing will ever reach it
run_hook
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "N3  an uninstalled job is not repeated here (the watchdog says it once)"
else
    bad "N3  the scratch notice still reports the scheduled job's health (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
: >"$LAUNCHD/com.richos.scratch-reaper.plist"
python3 - "$HOME_DIR/state/scratch-reaper-state.json" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
old = datetime.now(timezone.utc) - timedelta(hours=30)
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump({"last_apply": old.strftime("%Y-%m-%dT%H:%M:%SZ"), "deleted": 0,
               "freed": 0, "undecidable": 0, "verdict": "decided"}, fh)
PY
run_hook
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "N5  a stale scheduled pass is not repeated here either"
else
    bad "N5  the scratch notice still reports the scheduled job's age (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# N1 — a healthy machine is SILENT
# ===========================================================================
python3 - "$HOME_DIR/state/scratch-reaper-state.json" <<'PY'
import json, sys, time
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump({"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "deleted": 0, "freed": 0, "undecidable": 0,
               "verdict": "decided"}, fh)
PY
run_hook
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "N1  a healthy machine gets complete silence, exit 0"
else
    bad "N1  the notice spoke with nothing to say (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# N2 — over the threshold, it speaks
# ===========================================================================
write_config 1                       # one byte: anything dead qualifies
mkdir -p "$SCRATCH/-a-project/99999999-9999-9999-9999-999999999999/scratchpad"
echo payload >"$SCRATCH/-a-project/99999999-9999-9999-9999-999999999999/scratchpad/f"
run_hook
if [ "$RC" = "0" ] && says "is reclaimable now"; then
    ok "N2  reclaimable scratch over the threshold is reported on both channels"
else
    bad "N2  dead scratch over the threshold was not reported (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
if says "--apply"; then
    ok "N2b the notice carries the command that fixes it"
else
    bad "N2b the notice named no remedy"
fi

# ===========================================================================
# N6 — a broken reaper never takes the session with it
# ===========================================================================
BROKEN="$SANDBOX/broken-engine"
mkdir -p "$BROKEN/scripts/hooks" "$BROKEN/scripts/lib"
cp "$HOOK" "$BROKEN/scripts/hooks/"
cp "$ENGINE/orchestration.config" "$BROKEN/orchestration.config"
printf '#!/usr/bin/env bash\nexit 7\n' >"$BROKEN/scripts/scratch-reaper.sh"
chmod +x "$BROKEN/scripts/scratch-reaper.sh"
ERR="$SANDBOX/stderr-broken.txt"
OUT="$(SCRATCH_REAPER_CONFIG="$CFG" RICHOS_SESSIONS_DIR="$SESSIONS" \
    CLAUDE_CONFIG_DIR="$HOME_DIR" RICHOS_LAUNCH_AGENTS_DIR="$LAUNCHD" \
    SCRATCH_REAPER_STATE="$HOME_DIR/state/scratch-reaper-state.json" \
    bash "$BROKEN/scripts/hooks/session-start-scratch.sh" 2>"$ERR")"
RC=$?
if [ "$RC" = "0" ]; then
    ok "N6  a reaper that fails outright still leaves the session start at 0"
else
    bad "N6  a broken reaper made the hook exit $RC"
fi

# ===========================================================================
# N7 — stood down by declaration
# ===========================================================================
write_config 1
python3 - "$CFG" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace('SCRATCH_REAPER_ENABLE="1"', 'SCRATCH_REAPER_ENABLE="0"')
open(p, "w").write(s)
PY
run_hook
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "N7  SCRATCH_REAPER_ENABLE=0 is complete silence"
else
    bad "N7  a stood-down reaper still spoke (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# N8 — stderr reaches nobody, so nothing is written there
# ===========================================================================
write_config 1
run_hook
if [ ! -s "$ERR" ]; then
    ok "N8  nothing is written to stderr, which renders to no channel"
else
    bad "N8  the notice wrote to stderr:"
    sed 's/^/        /' "$ERR" | head -5
fi

echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

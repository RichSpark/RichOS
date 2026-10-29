#!/usr/bin/env bash
#
# disk-watchdog.test.sh — AN ALARM IS JUDGED ON WHEN IT STAYS SILENT.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# A watchdog has four ways to fail and only one of them is visible:
#
#   1. IT DOES NOT FIRE when it should. The disk fills and nobody is told.
#   2. IT FIRES WHEN IT SHOULD NOT. Worse than 1 over any length of time,
#      because an alarm that cries wolf is one somebody switches off, and then
#      failure 1 arrives permanently and silently.
#   3. IT IS NOT THERE AT ALL. A watchdog that was never scheduled looks
#      EXACTLY like a disk that never gets low. This is the failure nothing
#      inside the job can detect, which is why its absence is its own alert.
#   4. IT READS NONE OF ITS DECLARATIONS and looks fine because a fallback
#      happened to match. This is not hypothetical: the first run of this
#      program read no configuration at all — orchestration.config was sourced
#      into SHELL variables and never exported, so the python child saw none of
#      it — and it LOOKED correct because the fallback for the primary volume is
#      the same string the config declares. What exposed it was the second
#      volume silently missing.
#
#   W1   the declarations are actually READ — an unusual threshold comes back
#        out of --json. The case that would have caught failure 4, and it uses
#        a deliberately silly number for exactly that reason.
#   W2   a healthy machine: no alert, exit 0.
#   W3   below the floor: the alert fires and names the free space.
#   W4   a fall FASTER THAN THE DECLARED RATE alerts even when absolute space is
#        fine. This is the arm that would have caught 2026-09-17.
#   W5   the FIRST reading is not a drop. No previous state must not read as a
#        fall from zero, or every fresh machine alerts once for nothing.
#   W6   the CEO is notified, on the machine, via osascript, and it is logged.
#   W7   ...and NOT AGAIN within the declared repeat window.
#   W8   ...and only the most severe level per volume per run: below the urgent
#        floor both levels are true and sending both is two alarms for one event.
#   W9   A TIMER THAT IS NOT INSTALLED IS ITSELF A MASSIVE ALERT (failure 3).
#   W10  a timer installed but not reporting for many intervals is an alert —
#        a loaded-but-disabled launchd job fires on time and executes nothing.
#   W11  an UNMOUNTED extra volume is skipped, never reported as 0 bytes free.
#   W12  the sweeper's standing failures reach Rich's alert, with the manual
#        instruction, because that is the CEO rule's clean-up-failed case.
#   W13  the alert is CLASSIFIED: RichOS's own scratch says DEFECT, anything
#        else says other.
#   W14  --alert DOES NOT WRITE THE STATE FILE. A turn end that moved the
#        previous reading forward would destroy the baseline the drop
#        arithmetic subtracts from, silently turning a 15-minute rate alarm
#        into a per-turn-end one.
#   W15  IT RUNS WITH NO SESSION AT ALL — `env -i`, nothing in the
#        environment — and still takes a reading and writes its log line.
#   W16  --install REFUSES from a worktree or a temp directory, because the
#        plist would bake in a path that stops existing at land time.
#   W4c-W4f  THE DIVISOR (Frank's D7). The drop used to be a subtraction against a
#        baseline of any age, so a ten-hour-old reading printed "DROPPED 30.0 GB"
#        and called a normal working day our defect. It is a RATE now, and across a
#        gap longer than a declared number of intervals NO RATE IS COMPUTED at all —
#        the gap is reported instead (W4c/W4e, control W4d). W4f is the hole the fix
#        itself opened, found by this suite: a sub-second gap made a 100 GB fall
#        read as 496,862 GB/hour, so there is a MINIMUM gap too.
#   W17  ONE ENVIRONMENT VARIABLE MUST NOT SILENCE THE FAILURE ALERT. The
#        sweeper's failure record honored CLAUDE_CONFIG_DIR and this program's
#        reader did not, so an exported variable — and the engine's own suites
#        export it — sent the two to different directories and turned the MASSIVE
#        ALERT off with no sign that it was off (frank-opus-garbage1, D9).
#   W18  THE GARBAGE ALARM. His verdict on everything above it: "its alarm is a
#        disk-space alarm and a delete-failure alarm; it has no garbage alarm",
#        so ~53 GiB produced neither a cleanup nor a word. Garbage the sweeper
#        will never collect now reaches Rich, on its own declared threshold, with
#        the silence control that matters more than the alarm (W18b) and a
#        separate lower threshold for the undecidable pile (W18c, his D12).
#
# --- added 2026-09-19, after the CEO read the alert this mechanism made -----
# "And what is this all about: MASSIVE ALERT — DISK: 218 path(s) could not be
# deleted (delete BY HAND) | 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT
# | 11.4 GB in 2 place(s) UNDECIDABLE — no run will clear it". Under one heading:
# 200 paths a defect in the sweeper could not delete, 18 the kernel owns, 12.8 GB
# of our own campaign roots each already printing its `rm -rf`, 1.15 GB of Visual
# Studio Code's and Codex's temporary files, and one figure six hours stale that
# he had already cleared by hand. FOUR DIFFERENT RESPONSES, presented as one
# alarm — and failure 2 in the list above is the one that gets an alarm ignored.
#
#   W19  THREE PILES, THREE HEADINGS. Our failed clean-up is the MASSIVE ALERT
#        and the banner names it (W19/W19a — it said "DISK SPACE" with 198 GB
#        free); the by-hand pile gets its own heading and the command that
#        clears it (W19b); and W19c is the control that decides whether the
#        split is real — the same 13.9 GB, alone, reports and exits 0.
#   W20  EVERY FIGURE CARRIES THE TIME IT WAS MEASURED, and past a declared
#        window it is disowned rather than quoted (W20b). W18/W18a are the
#        amended cases: other programs' temp is reported and is NOT an alarm.
#   W21  --status RUNS the sweeper, so a person who asks gets this second's
#        answer (W21), it is not an ALERT (W21a), and when the sweeper cannot be
#        run the fallback says how old its number is (W21b).
#
# --- added 2026-09-25, after "python3.14 would like to access data from other
# apps" kept appearing on the CEO's screen -------------------------------------
#   W22  THE WATCHDOG NEVER WALKS ANOTHER APP'S DATA. Its `du` walked the folder
#        where macOS keeps other apps' sandboxed data, and macOS asked the CEO for
#        permission on every alerting run; allowing it did not stick. W22 plants
#        that folder, its holders ($HOME, ~/Library) and an app-group folder in
#        the candidate list of a forced alert, with a `du` stub that records every
#        path it is handed: none of them may reach `du`, each must be reported as
#        refused, and an ordinary folder beside them MUST still be measured
#        (the positive probe). W22d is the declaration itself: no entry of the
#        REAL orchestration.config may enter that data.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WD="$SCRIPT_DIR/disk-watchdog.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$WD" ] || { echo "FATAL: missing $WD" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/disk-watchdog-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

echo "=== disk-watchdog tests ==="

# ---------------------------------------------------------------------------
# A world: its own config, its own state file, its own log, its own
# LaunchAgents directory, and its own `osascript` on PATH.
# ---------------------------------------------------------------------------
# THE osascript STUB IS WHY THIS SUITE IS SAFE TO RUN. The real one posts a
# notification to the operator's screen, and a test suite that did that on every
# CI run would be the most annoying program on the machine. The stub records what
# it was asked to post, which is also strictly more testable.
W=0
world() {                  # <name>
    W=$((W + 1))
    W_DIR="$SANDBOX/$1"
    W_CFG="$W_DIR/orchestration.config"
    W_STATE="$W_DIR/state.json"
    W_LOG="$W_DIR/watchdog.log"
    W_LA="$W_DIR/LaunchAgents"
    W_BIN="$W_DIR/bin"
    W_NOTIFY="$W_DIR/notifications.txt"
    W_FAILS="$W_DIR/scratch-failures.json"
    unset W_MEASURED_ROOT W_PROOF_ROOT W_IOS_ROOT W_CARGO_ROOT
    mkdir -p "$W_DIR" "$W_LA" "$W_BIN" "$W_DIR/measured" "$W_DIR/proof" "$W_DIR/ios" "$W_DIR/cargo"
    cat >"$W_BIN/osascript" <<STUB
#!/bin/sh
# Records the AppleScript it was handed instead of posting it.
printf '%s\n' "\$*" >>"$W_NOTIFY"
exit 0
STUB
    chmod +x "$W_BIN/osascript"
    # EVERY WORLD STARTS WITH A HEALTHY SCHEDULED CLEANUP: the reaper's plist is
    # installed and its job published a healthy result just now. Cleanup health is
    # an alarm of its own (cleanup PRD R4/R5), and a case about something else must
    # not trip it. Cases about cleanup health replace this file.
    : >"$W_LA/com.richos.scratch-reaper.plist"
    healthy_reaper_state "$W_DIR/reaper-state.json"
}

# healthy_reaper_state <path> [extra python dict items] — a healthy scheduled result.
healthy_reaper_state() {
    python3 - "$1" "${2:-}" <<'PY'
import json, sys, time
extra = eval("{%s}" % sys.argv[2]) if sys.argv[2] else {}
st = {"source": "launchd", "outcome": "healthy", "unresolved": [], "roots": [],
      "last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
      "last_apply_epoch": int(time.time()), "result_epoch": int(time.time()),
      "deleted": 0, "freed": 0, "undecidable": 0, "undecidable_bytes": 0,
      "skipped": 0, "skipped_bytes": 0, "failures": 0, "verdict": "decided"}
st.update(extra)
json.dump(st, open(sys.argv[1], "w"))
PY
}

# write_cfg <rich_gb> <drop_gb> <ceo_gb> <urgent_gb> [extra-volumes] [repeat-h]
write_cfg() {
    cat >"$W_CFG" <<CFG
DISK_WATCHDOG_ENABLE="1"
DISK_PRIMARY_VOLUME="${W_PRIMARY:-/System/Volumes/Data}"
DISK_EXTRA_VOLUMES="${5:-}"
DISK_RICH_ALERT_GB="$1"
DISK_DROP_ALERT_GB="$2"
# THE DROP TEST BECAME A RATE ON 2026-09-18 (D7) AND THIS LINE KEEPS EVERY
# EXISTING CALLER'S MEANING. Readings are 15 minutes apart, so N GB per reading
# IS 4N GB/hour: every call site below still says what it always said, and the
# cases that follow still measure the threshold rather than "it alerts".
DISK_DROP_ALERT_GB_PER_HOUR="$(( $2 * 4 ))"
DISK_DROP_MAX_GAP_INTERVALS="${7:-3}"
DISK_CEO_NOTIFY_GB="$3"
DISK_CEO_URGENT_GB="$4"
DISK_CEO_REPEAT_HOURS="${6:-24}"
DISK_SCALE_EXTRA_VOLUMES="1"
DISK_WATCHDOG_MINUTES="15"
DISK_CONSUMER_CANDIDATES="$W_DIR"
DISK_STATE_JSON="$W_STATE"
SCRATCH_ROOT_NAME="richos-scratch"
SCRATCH_CLAUDE_ROOTS="$W_DIR/claude-%u"
SCRATCH_FAILURES_STATE="$W_FAILS"
# THE SWEEPER'S PUBLISHED NUMBERS BELONG TO THE WORLD TOO, and leaving this out
# was a hermeticity hole this suite had from the day the garbage report was
# added: with no declaration the program fell back to
# ~/.claude/state/scratch-reaper-state.json and W2 — "a healthy machine gets
# COMPLETE SILENCE" — failed on the operator's own 1.2 GB of Visual Studio
# Code's temporary files. A suite whose result depends on the machine it runs on
# is the same defect as a program that reads none of its declarations (W1), from
# the other side. Cases that want their own figures append this key AFTER, and a
# later declaration wins.
SCRATCH_REAPER_STATE="$W_DIR/reaper-state.json"
# The same hermeticity for the test-device record: without it W2 would read the
# operator's own ~/.claude/state/test-device-failures.json.
TEST_DEVICE_FAILURES_STATE="$W_DIR/test-device-failures.json"
CFG
}

run_wd() {                 # <mode-flags...>
    # The escalation ledger is ALWAYS the world's own, whatever runs here.
    RICHOS_ESCALATION_LEDGER="$W_DIR/escalations.jsonl" \
    RICHOS_MACHINE_SETTINGS_RESOLVED=1 \
    RICHOS_BULK_ROOT="$W_DIR" \
    RICHOS_BUILD_CACHE="${W_MEASURED_ROOT:-$W_DIR/measured}" \
    RICHOS_PROOF_RUN_DIR="${W_PROOF_ROOT:-$W_DIR/proof}" \
    RICHOS_NATIVE_IOS_CACHE="${W_IOS_ROOT:-$W_DIR/ios}" \
    RICHOS_CARGO_CACHE_ROOT="${W_CARGO_ROOT:-$W_DIR/cargo}" \
    CARGO_TARGET_DIR="$W_DIR/cargo" \
    PATH="$W_BIN:$PATH" \
    DISK_WATCHDOG_CONFIG="$W_CFG" \
    DISK_WATCHDOG_LOG="$W_LOG" \
    RICHOS_LAUNCH_AGENTS_DIR="$W_LA" \
    bash "$WD" "$@" 2>&1
}

# THE INSTALLED JOB'S TICK, as disk-watchdog.sh runs the program once launchd has
# named it (cleanup PRD R5). launchd's XPC_SERVICE_NAME cannot be handed to a child
# on this OS (any child reads "0"; measured 2026-10-23), so this does what the shell
# does after deciding: export the world's declarations and DISK_WATCHDOG_SCHEDULED=1
# and run the check. W23d proves the shell refuses a caller's own handoff.
run_wd_scheduled() {
    (
        set -a
        # shellcheck disable=SC1090
        . "$W_CFG"
        set +a
        export DISK_WATCHDOG_SCHEDULED=1
        RICHOS_ESCALATION_LEDGER="$W_DIR/escalations.jsonl" \
        RICHOS_MACHINE_SETTINGS_RESOLVED=1 \
        RICHOS_BULK_ROOT="$W_DIR" \
        RICHOS_BUILD_CACHE="${W_MEASURED_ROOT:-$W_DIR/measured}" \
        RICHOS_PROOF_RUN_DIR="${W_PROOF_ROOT:-$W_DIR/proof}" \
        RICHOS_NATIVE_IOS_CACHE="${W_IOS_ROOT:-$W_DIR/ios}" \
        RICHOS_CARGO_CACHE_ROOT="${W_CARGO_ROOT:-$W_DIR/cargo}" \
        CARGO_TARGET_DIR="$W_DIR/cargo" \
        PATH="$W_BIN:$PATH" \
        RICHOS_LAUNCH_AGENTS_DIR="$W_LA" \
        MODE=check STATE="$W_STATE" LOG="$W_LOG" \
        python3 "$SCRIPT_DIR/lib/disk-watchdog.py" 2>&1
    )
}

# ===========================================================================
# W1 — the declarations are READ (the case that catches an unexported config)
# ===========================================================================
world decls
# 4242 is deliberately absurd. A test written with the REAL threshold would pass
# against a program that read no configuration at all, which is exactly the bug
# this case exists for.
write_cfg 4242 4243 4244 4245
OUT="$(run_wd --json)"
if printf '%s' "$OUT" | grep -q '"rich_alert_gb": 4242' \
   && printf '%s' "$OUT" | grep -q '"drop_alert_gb": 4243' \
   && printf '%s' "$OUT" | grep -q '"ceo_notify_gb": 4244'; then
    ok "W1  the declared thresholds reach the program (not a fallback)"
else
    bad "W1  the declarations are NOT being read — a fallback is in use"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -20
fi

# Volumes follow effective settings. A retired list must not add an unrelated
# mount, and the selected component directory itself is not a mount.
world decls2
write_cfg 1 1 1 1 "$SANDBOX/unrelated"
OUT="$(run_wd --json)"
if printf '%s' "$OUT" | python3 -c '
import json,sys
from pathlib import Path
report=json.load(sys.stdin); root=Path(sys.argv[1]).resolve(); mounts={v["mount"] for v in report["volumes"]}
assert str(root/"unrelated") not in mounts
if root.parts[:2]==("/","Volumes"):
    assert str(Path(*root.parts[:3])) in mounts, (root,mounts)
' "$SANDBOX"; then
    ok "W1b effective storage selects its mounted volume and ignores the retired list"
else
    bad "W1b effective storage monitoring lost its volume or used the retired list"
fi

# ===========================================================================
# W2 — a healthy machine is SILENT
# ===========================================================================
# 1 GB floors against a volume with well over 100 GB free. Silence here is the
# property that keeps the alarm worth reading.
world healthy
write_cfg 1 9999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"    # installed, so W9 is not in play
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "W2  a healthy machine gets complete silence and exit 0"
else
    bad "W2  a healthy machine produced an alert (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# ===========================================================================
# W3 — below the floor, it fires and names the number
# ===========================================================================
world low
write_cfg 999999 999999 0 0            # a floor nothing can satisfy
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'MASSIVE ALERT — DISK SPACE' \
   && printf '%s' "$OUT" | grep -q 'FREE of'; then
    ok "W3  below the floor: MASSIVE ALERT naming the free space, exit 1"
else
    bad "W3  the low-space alert did not fire (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# ===========================================================================
# W4 / W5 — the DROP arm, and the first reading that must not look like one
# ===========================================================================
# THE ARM THAT WOULD HAVE CAUGHT 2026-09-17: 105 GB arrived while the volume
# still had 49 GB free, so an absolute floor alone would have warned very late.
world drop
write_cfg 1 20 1 1                     # absolute floor unreachable; drop = 20 GB
touch "$W_LA/com.richos.disk-watchdog.plist"

# W5 FIRST, because it is about the absence of state: with no previous reading,
# the fall from "nothing" must be zero and not the size of the disk.
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W5  the FIRST reading is not a drop — no state is not a fall from zero"
else
    bad "W5  a first reading alerted as a drop"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# Now plant a previous reading 100 GB higher than reality, ONE DECLARED INTERVAL
# OLD, and re-read.
#
# THE AGE OF THE BASELINE IS PART OF THE FIXTURE NOW, because the drop test is a
# RATE. Back-to-back readings gave a gap under a second, and 100 GB across that is
# 496,862 GB/hour — which exceeds every threshold and made W4b's control
# meaningless. The suite found that, and the program grew a
# DISK_DROP_MIN_GAP_SECONDS because of it: a denominator that is too small is
# exactly as meaningless as one that is too large, and it errs toward a false
# alarm rather than toward silence.
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 100 * 1024 ** 3
s["reading_epoch"] = s["reading_epoch"] - 900
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W4  a 100 GB fall between readings alerts with the absolute level fine"
else
    bad "W4  the drop arm did not fire (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
if printf '%s' "$OUT" | grep -q 'GB/hour'; then
    ok "W4a and it states a RATE, so the number can be argued with"
else
    bad "W4a the alert gives a fall with no elapsed time in it (D7)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# THE CONTROL for W4: the same run with the drop threshold raised above the fall
# must be silent, so W4 is measuring the threshold and not just "it alerts".
write_cfg 1 500 1 1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 100 * 1024 ** 3
s["reading_epoch"] = s["reading_epoch"] - 900
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W4b CONTROL: the same 100 GB fall under a 500 GB threshold is silent"
else
    bad "W4b CONTROL FAILED: it alerts regardless of the threshold"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# ===========================================================================
# W4c / W4d — A STALE BASELINE IS NOT A DROP (D7)
# ===========================================================================
# frank-opus-garbage1 aged the previous reading ten hours and the watchdog printed
# "DROPPED 30.0 GB since the last reading ... CLASSIFICATION: RichOS garbage
# (DEFECT)". The arithmetic had no elapsed-time term at all, so a closed laptop, a
# wake from sleep, an upgrade window or a launchd job unloaded and reloaded
# manufactured an alert out of a normal working day — 3 GB/hour on this machine.
#
# A fall averaged over ten hours says nothing about any hour inside it, so the
# program now REFUSES TO COMPUTE A RATE across a gap that long and reports the gap.
world dropstale
write_cfg 1 20 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
# 30 GB higher than reality, and the reading is TEN HOURS old.
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 30 * 1024 ** 3
s["reading_epoch"] = s["reading_epoch"] - 10 * 3600
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W4c a 30 GB fall across a TEN-HOUR gap raises no drop alert (D7)"
else
    bad "W4c a stale baseline still manufactures a drop alert (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# THE CONTROL, and it is the one that stops W4c passing against an arm that has
# simply been switched off: the SAME 30 GB fall inside one interval must alert.
world dropfresh
write_cfg 1 20 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 30 * 1024 ** 3
s["reading_epoch"] = s["reading_epoch"] - 900
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W4d CONTROL: the same 30 GB fall inside one interval DOES alert"
else
    bad "W4d CONTROL FAILED: the drop arm is off, so W4c proves nothing (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# And the gap must be REPORTED rather than merely swallowed, inside a block that
# is already being printed for another reason.
world dropstalesay
write_cfg 999999 20 1 1                 # the absolute floor fires, so a block prints
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 30 * 1024 ** 3
s["reading_epoch"] = s["reading_epoch"] - 10 * 3600
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if printf '%s' "$OUT" | grep -q 'NO COMPARABLE PREVIOUS READING' \
   && printf '%s' "$OUT" | grep -q '600 min'; then
    ok "W4e the gap is REPORTED with its size, instead of divided by"
else
    bad "W4e the stale gap was swallowed silently (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -14
fi

# ===========================================================================
# W4f — A GAP THAT IS TOO SHORT IS NOT A DROP EITHER
# ===========================================================================
# THE HOLE THE D7 FIX OPENED, and the suite is what found it. A rate has a
# denominator, and a denominator is dangerous at BOTH ends: with back-to-back
# readings the gap was under a second and a 100 GB fall read as
# "FALLING at 496862.1 GB/hour" — above every conceivable threshold. In production
# that is a launchd job firing twice in quick succession, or a person running
# --check a moment after the timer did. It errs toward a FALSE ALARM, which is the
# worse of the two directions, so it is refused like the stale case.
world dropquick
write_cfg 1 20 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
# A large fall, and the baseline is from THIS SECOND.
for v in s["volumes"]:
    v["free_bytes"] = v["free_bytes"] + 100 * 1024 ** 3
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'FALLING at'; then
    ok "W4f a 100 GB fall across a SUB-SECOND gap raises no rate alert"
else
    bad "W4f a double-fire manufactured a rate alert (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi

# ===========================================================================
# W6 / W7 / W8 — the CEO's channel
# ===========================================================================
world notify
write_cfg 1 999999 999999 1            # notify floor unreachable -> always notify
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
if [ -f "$W_NOTIFY" ] && grep -q 'display notification' "$W_NOTIFY"; then
    ok "W6  the CEO is notified through osascript"
else
    bad "W6  no notification was posted"
fi
if grep -q 'free on' "$W_NOTIFY" 2>/dev/null && grep -q 'Top:' "$W_NOTIFY" 2>/dev/null; then
    ok "W6b the text carries the free space AND the top consumers"
else
    bad "W6b the notification text is missing the number or the consumers"
    sed 's/^/        /' "$W_NOTIFY" 2>/dev/null | head -3
fi
if grep -q 'NOTIFIED ceo' "$W_LOG" 2>/dev/null; then
    ok "W6c the notification is recorded in the log"
else
    bad "W6c the notification was not logged"
fi

BEFORE="$(wc -l <"$W_NOTIFY" 2>/dev/null | tr -d ' ')"
run_wd --check >/dev/null 2>&1
AFTER="$(wc -l <"$W_NOTIFY" 2>/dev/null | tr -d ' ')"
if [ "$BEFORE" = "$AFTER" ]; then
    ok "W7  a second run inside the repeat window does NOT notify again"
else
    bad "W7  the CEO was notified twice inside the repeat window ($BEFORE -> $AFTER)"
fi

# THE CONTROL for W7: with the window set to zero hours it DOES notify again, so
# W7 proves the window rather than proving notifications happen once ever.
write_cfg 1 999999 999999 1 "" 0
run_wd --check >/dev/null 2>&1
AFTER2="$(wc -l <"$W_NOTIFY" 2>/dev/null | tr -d ' ')"
if [ "$AFTER2" -gt "$AFTER" ]; then
    ok "W7b CONTROL: with a zero-hour window it notifies again"
else
    bad "W7b CONTROL: a zero-hour window did not re-notify ($AFTER -> $AFTER2), so W7 cannot tell a working window from permanent suppression"
fi

# W8 — below BOTH the notify and urgent floors, exactly one notification per
# volume per run. Both conditions are true; two alarms for one event is noise.
world severity
write_cfg 1 999999 999999 999999       # both floors unreachable -> both true
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
if python3 - "$W_STATE" "$W_NOTIFY" <<'PYNOTIFY'
import json,sys
report=json.load(open(sys.argv[1]));lines=open(sys.argv[2]).read().splitlines()
assert report['volumes']
for volume in report['volumes']:
    assert sum(('free on '+volume['mount']+' ') in line for line in lines)==1, (volume,lines)
assert len(lines)==len(report['volumes']), lines
PYNOTIFY
then
    ok "W8  below both floors, exactly ONE notification is sent per selected volume"
else
    bad "W8  duplicate or missing notification for a selected volume"
    sed 's/^/        /' "$W_NOTIFY" 2>/dev/null | head -4
fi
if grep -q 'CRITICAL' "$W_NOTIFY" 2>/dev/null; then
    ok "W8b and it is the MORE SEVERE of the two"
else
    bad "W8b the less severe notification was the one sent"
    sed 's/^/        /' "$W_NOTIFY" 2>/dev/null | head -3
fi

# ===========================================================================
# W9 — THE WATCHMAN'S OWN ABSENCE (failure 3)
# ===========================================================================
world uninstalled
write_cfg 1 999999 1 1                 # everything healthy
# NOTE: no plist is created in this world.
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'THE DISK WATCHDOG IS NOT INSTALLED'; then
    ok "W9  a missing timer is itself a MASSIVE ALERT"
else
    bad "W9  a missing timer went unreported (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
if run_wd --installed >/dev/null 2>&1; then
    bad "W9b --installed reported success with no plist"
else
    ok "W9b --installed exits non-zero with no plist"
fi
touch "$W_LA/com.richos.disk-watchdog.plist"
if run_wd --installed >/dev/null 2>&1; then
    ok "W9c CONTROL: --installed exits 0 once the plist is there"
else
    bad "W9c --installed still fails with the plist present"
fi

# ===========================================================================
# W10 — installed but not reporting
# ===========================================================================
world stale
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
python3 - "$W_STATE" <<'PY'
import json, sys, time
p = sys.argv[1]
s = json.load(open(p))
s["reading_epoch"] = int(time.time()) - 60 * 60 * 24      # a day ago
json.dump(s, open(p, "w"), indent=1, sort_keys=True)
PY
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'HAS STOPPED REPORTING'; then
    ok "W10  a timer that stopped reporting is an alert"
else
    bad "W10  a day-old reading was accepted as current (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# ===========================================================================
# W11 — an unmounted volume is skipped, never 0 bytes free
# ===========================================================================
# An external disk being unplugged is not a disk-space event. Reporting it as
# 0 free would be an instant, permanent, false alarm — failure 2.
world unmounted
write_cfg 1 999999 1 1 "/Volumes/DefinitelyNotMounted$$"
touch "$W_LA/com.richos.disk-watchdog.plist"
OUT="$(run_wd --json)"; RC=$?
if ! printf '%s' "$OUT" | grep -q 'DefinitelyNotMounted'; then
    ok "W11  an unmounted volume is skipped entirely"
else
    bad "W11  an unmounted volume was reported"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
if [ "$RC" = "0" ]; then
    ok "W11b ...and it does not alert"
else
    bad "W11b an unmounted volume produced an alert (rc=$RC)"
fi

# ===========================================================================
# W12 — the sweeper's standing failures are Rich's business
# ===========================================================================
# The CEO's rule: "if the clean-up fails or impossible for some reason, then Rich
# must get a MASSIVE ALERT about it and get on with manually deleting".
world failures
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
python3 - "$W_FAILS" <<'PY'
import json, sys
json.dump({"/tmp/a-thing-that-will-not-die": {
    "first": "2026-09-18T07:40:05Z", "last": "2026-09-18T08:00:00Z",
    "error": "[Errno 1] Operation not permitted", "attempts": 4}},
    open(sys.argv[1], "w"), indent=1)
PY
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] \
   && printf '%s' "$OUT" | grep -q 'COULD NOT BE DELETED' \
   && printf '%s' "$OUT" | grep -q 'a-thing-that-will-not-die' \
   && printf '%s' "$OUT" | grep -q 'BY HAND'; then
    ok "W12  a standing sweep failure alerts, names the path, and says BY HAND"
else
    bad "W12  a sweep failure did not reach the alert (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
if printf '%s' "$OUT" | grep -q '4 attempt'; then
    ok "W12b and carries how long it has been stuck"
else
    bad "W12b the alert does not say how many attempts have failed"
fi
rm -f "$W_FAILS"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "W12c CONTROL: with the failure resolved the alert clears completely"
else
    bad "W12c the alert did not clear after the failure was resolved (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# ===========================================================================
# W12d — a test simulator left running is Rich's business too (§54, 2026-09-22)
# ===========================================================================
# Three simulators booted by killed test runs outlived the session that
# 2026-09-22 ended with. What scripts/lib/testdevices.py cannot remove, or cannot
# prove the owner of while it runs, is a row here, and the alert carries the
# exact command.
world devices
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
python3 - "$W_DIR/test-device-failures.json" <<'PY'
import json, sys
json.dump({"ios:FF09044D-0619-442E-95F6-9CCB6EA6A3E0": {
    "kind": "ios", "id": "FF09044D-0619-442E-95F6-9CCB6EA6A3E0", "name": "RichOS native-ios 60e53bf487-app",
    "verdict": "owner cannot be proven", "why": "no checkout the engine has recorded derives the key",
    "command": "xcrun simctl shutdown FF09044D-0619-442E-95F6-9CCB6EA6A3E0; xcrun simctl delete FF09044D-0619-442E-95F6-9CCB6EA6A3E0",
    "first": "2026-09-23T00:40:00Z", "last": "2026-09-23T00:40:00Z", "attempts": 1}},
    open(sys.argv[1], "w"), indent=1)
PY
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] \
   && printf '%s' "$OUT" | grep -q 'TEST DEVICE CLEANUP NEEDS ATTENTION' \
   && printf '%s' "$OUT" | grep -q 'xcrun simctl delete FF09044D-0619-442E-95F6-9CCB6EA6A3E0' \
   && printf '%s' "$OUT" | grep -q 'BY HAND'; then
    ok "W12d a test simulator left running alerts, names it, and carries the command to end it BY HAND"
else
    bad "W12d a test-device failure did not reach the alert (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
rm -f "$W_DIR/test-device-failures.json"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "W12e CONTROL: with the device gone the alert clears completely"
else
    bad "W12e the alert did not clear after the device row was resolved (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# Scheduled checks retry cleanup even when no session survives to emit an end.
world device-timer
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
printf '#!/bin/sh\nexit 1\n' > "$W_BIN/fake-simctl"; chmod +x "$W_BIN/fake-simctl"
OUT="$(CLAUDE_CONFIG_DIR="$W_DIR/claude" RICHOS_SIMCTL="$W_BIN/fake-simctl" run_wd --check)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'TEST DEVICE CLEANUP NEEDS ATTENTION' && [ -f "$W_DIR/test-device-failures.json" ]; then
    ok "W12f scheduled check persists and displays failed inventory without a session"
else
    bad "W12f scheduled device cleanup did not create an alert: $OUT"
fi
cat > "$W_BIN/fake-simctl" <<'FAKE'
#!/bin/sh
printf '%s\n' '{"devices":{}}'
FAKE
OUT="$(CLAUDE_CONFIG_DIR="$W_DIR/claude" RICHOS_SIMCTL="$W_BIN/fake-simctl" run_wd --check)"; RC=$?
if [ "$RC" = "0" ] && [ ! -f "$W_DIR/test-device-failures.json" ]; then
    ok "W12g next successful scheduled check clears the collector alert"
else
    bad "W12g scheduled recovery did not clear the alert: $OUT"
fi

# W12h — THE WATCHDOG NEVER HOLDS THE DEVICE REGISTRY LOCK ACROSS A HANG.
# 2026-10-07 23:45Z the scheduled pass blocked inside a filesystem open() while
# holding test-devices/.lock, and every device lease cleanup and test run on the
# machine failed "registry lock busy" for 35 minutes. Here the hang is an
# emulator.json that is a FIFO with no writer: open() blocks in the kernel, as
# the real one did, and the pass's 8-second budget does not reach it. The lock
# must be free within the collector's hard limit, the watchdog must finish its
# disk check, and the failure must be recorded.
world device-hang
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
cat > "$W_BIN/fake-simctl" <<'FAKE'
#!/bin/sh
printf '%s\n' '{"devices":{}}'
FAKE
chmod +x "$W_BIN/fake-simctl"
mkdir -p "$W_DIR/caches/hung" "$W_DIR/claude/state/test-devices"
mkfifo "$W_DIR/caches/hung/emulator.json"
CLAUDE_CONFIG_DIR="$W_DIR/claude" RICHOS_SIMCTL="$W_BIN/fake-simctl" \
    RICHOS_ANDROID_CACHES_ROOT="$W_DIR/caches" RICHOS_DEVICE_COLLECT_SECONDS=2 \
    run_wd --check >"$W_DIR/out.txt" 2>&1 &
W12H_PID=$!
W12H_DONE=0
for _ in $(seq 1 40); do
    if ! kill -0 "$W12H_PID" 2>/dev/null; then W12H_DONE=1; break; fi
    sleep 0.25
done
W12H_LOCK="$(python3 - "$W_DIR/claude/state/test-devices/.lock" <<'PY'
import fcntl, sys
try:
    with open(sys.argv[1], "a") as h:
        try:
            fcntl.flock(h, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("HELD"); raise SystemExit
        fcntl.flock(h, fcntl.LOCK_UN)
        print("FREE")
except OSError as e:
    print("UNKNOWN %s" % e)
PY
)"
# Release a reader still blocked on the FIFO (the unfixed program) so the case
# ends; with no reader the non-blocking open fails and nothing waits.
python3 - "$W_DIR/caches/hung/emulator.json" <<'PY'
import os, sys
try:
    fd = os.open(sys.argv[1], os.O_WRONLY | os.O_NONBLOCK)
    os.write(fd, b"{}")
    os.close(fd)
except OSError:
    pass
PY
wait "$W12H_PID" 2>/dev/null
if [ "$W12H_DONE" = "1" ] && [ "$W12H_LOCK" = "FREE" ] \
   && grep -q 'registry lock' "$W_DIR/test-device-failures.json" 2>/dev/null; then
    ok "W12h a hung device cleanup is stopped at its limit: the registry lock is free and the failure is recorded"
else
    bad "W12h a hung device cleanup kept the watchdog and the registry lock (finished=$W12H_DONE lock=$W12H_LOCK)"
    sed 's/^/        /' "$W_DIR/out.txt" | head -8
fi

# W12i — A COLLECTION STILL RUNNING IS NOT A CLEANUP FAILURE (2026-10-24).
# The collector marks its start so a killed pass leaves an alert. At 23:50:16Z
# that mark, from a pass that was simply still running, published
# test_device_failures 1 with nothing wrong. The mark names its process: while
# that process runs, inside the bound, it is not counted; once the process is
# gone, or past the bound, it is a failure again (W12j, W12k).
device_mark() {            # <pid> <seconds-ago>
    python3 - "$W_DIR/test-device-failures.json" "$1" "$2" <<'PY2'
import json, os, subprocess, sys, time
pid, ago = int(sys.argv[2]), float(sys.argv[3])
out = subprocess.run(["ps", "-o", "stat=,lstart=", "-p", str(pid)], capture_output=True, text=True,
                     env=dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")).stdout.strip()
start = " ".join(out.partition(" ")[2].split())
json.dump({"collector:incomplete": {
    "kind": "collector", "id": "incomplete", "name": "test-device collector",
    "verdict": "collection running", "why": "cleanup started but has not completed",
    "command": "python3 testdevices.py collect --apply", "first": "2026-10-24T23:50:00Z",
    "last": "2026-10-24T23:50:00Z", "attempts": 1,
    "pid": pid, "pid_start": start, "started": time.time() - ago}}, open(sys.argv[1], "w"))
PY2
}
world device-running
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
# load-bound: the sleep is only a live process for the mark to name; no verdict waits on it
sleep 600 & W12I_PID=$!
device_mark "$W12I_PID" 5
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'TEST DEVICE'; then
    ok "W12i a device collection still running is not published as a cleanup failure"
else
    bad "W12i a running device collection was published as a failure (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
device_mark "$W12I_PID" 100000
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'TEST DEVICE CLEANUP NEEDS ATTENTION'; then
    ok "W12k CONTROL: a collection running past its bound is still a failure"
else
    bad "W12k an overdue device collection was hidden (rc=$RC)"
fi
kill "$W12I_PID" 2>/dev/null; wait "$W12I_PID" 2>/dev/null
device_mark "$W12I_PID" 5
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'TEST DEVICE CLEANUP NEEDS ATTENTION'; then
    ok "W12j CONTROL: a collection whose process is gone is still a failure"
else
    bad "W12j an abandoned device collection was hidden (rc=$RC)"
fi
rm -f "$W_DIR/test-device-failures.json"

# ===========================================================================
# W13 — the classification (CEO addendum 2)
# ===========================================================================
# "the disk watchdog will NEVER bark because the RichOS app failed to clean up
# its garbage" — so a bark has to say whether that is what happened, because a
# defect report and a disk warning want different responses.
world classify
W_PRIMARY="/System/Volumes/Data"
write_cfg 999999 999999 0 0            # force an alert
touch "$W_LA/com.richos.disk-watchdog.plist"
OUT="$(run_wd --check)"
if printf '%s' "$OUT" | grep -q 'CLASSIFICATION:'; then
    ok "W13  the alert carries a classification"
else
    bad "W13  the alert has no classification line"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -14
fi
# With no RichOS scratch in this world at all, it must NOT blame RichOS. A
# classifier that always said DEFECT would pass W13 and be useless.
if printf '%s' "$OUT" | grep -q 'CLASSIFICATION: other'; then
    ok "W13b CONTROL: with no RichOS scratch present it says 'other', not DEFECT"
else
    bad "W13b it blamed RichOS with no RichOS scratch in the world"
    printf '%s' "$OUT" | grep -A2 'CLASSIFICATION' | sed 's/^/        /'
fi

# W13c — NO SHORTFALL, NO LOW-DISK DEFECT (2026-10-24). The shortfall was clamped
# to one byte, so with 74 GB free any RichOS scratch at all read "RichOS garbage
# (DEFECT)" beside an alert that was about something else. Here the disk is far
# above its floor, a device row raises the alert and RichOS scratch exists.
world classify-noshort
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
mkdir -p "$W_DIR/claude-$(id -u)/left"
head -c 1048576 /dev/zero >"$W_DIR/claude-$(id -u)/left/blob"
printf '%s\n' '{"ios:X": {"kind": "ios", "id": "X", "name": "Test iPhone", "verdict": "would not close", "why": "simctl delete refused", "command": "xcrun simctl delete X", "first": "2026-10-24T18:00:00Z", "attempts": 1}}' >"$W_DIR/test-device-failures.json"
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'TEST DEVICE CLEANUP NEEDS ATTENTION' \
   && ! printf '%s' "$OUT" | grep -q 'DEFECT' && ! printf '%s' "$OUT" | grep -q 'DISK SPACE'; then
    ok "W13c with no space shortfall RichOS scratch is not called a low-disk DEFECT; the real alarm stays"
else
    bad "W13c no shortfall, yet the alert called RichOS scratch a DEFECT or lost its real alarm (rc=$RC)"
    printf '%s' "$OUT" | grep -E 'MASSIVE|CLASSIFICATION|DEFECT' | sed 's/^/        /'
fi
rm -f "$W_DIR/test-device-failures.json"
# W13d CONTROL — REAL LOW SPACE IS STILL TOLD, AND CLASSIFIED, with the same scratch.
write_cfg 999999 999999 0 0
OUT="$(run_wd --check)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'MASSIVE ALERT — DISK SPACE' \
   && printf '%s' "$OUT" | grep -q 'CLASSIFICATION:'; then
    ok "W13d CONTROL: below the floor the disk alarm and its classification are still told"
else
    bad "W13d real low space lost its alarm or classification (rc=$RC)"
    printf '%s' "$OUT" | grep -E 'MASSIVE|CLASSIFICATION' | sed 's/^/        /'
fi

# ===========================================================================
# W14 — --alert MUST NOT MOVE THE BASELINE
# ===========================================================================
# The drop arithmetic subtracts the previous reading. A turn end that wrote a
# fresh reading would destroy that baseline every turn, and the 15-minute rate
# alarm would silently become a per-turn-end one that can never see a drop.
world baseline
write_cfg 1 20 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
run_wd --check >/dev/null 2>&1
BEFORE_SUM="$(python3 -c "
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$W_STATE")"
run_wd --alert >/dev/null 2>&1
run_wd --json >/dev/null 2>&1
AFTER_SUM="$(python3 -c "
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$W_STATE")"
if [ "$BEFORE_SUM" = "$AFTER_SUM" ]; then
    ok "W14  --alert and --json leave the state file byte-identical"
else
    bad "W14  a consumer mode rewrote the state file and moved the drop baseline"
fi
# THE CONTROL: --check DOES write it, so W14 is not passing against a program
# that never writes state at all.
sleep 1
run_wd --check >/dev/null 2>&1
CHECK_SUM="$(python3 -c "
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$W_STATE")"
if [ "$CHECK_SUM" != "$AFTER_SUM" ]; then
    ok "W14b CONTROL: --check does write it, so W14 measures something"
else
    bad "W14b --check did not write the state file either"
fi

# ===========================================================================
# W15 — IT RUNS WITH NO SESSION AT ALL
# ===========================================================================
# This is what the launchd timer actually does: fires at 03:40 with nobody
# logged into anything, no CLAUDE_* variables, no project directory, no TMPDIR
# from a shell profile. `env -i` is that condition, reproduced exactly.
world nosession
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
LINES_BEFORE=0
[ -f "$W_LOG" ] && LINES_BEFORE="$(wc -l <"$W_LOG" | tr -d ' ')"
env -i \
    PATH="$W_BIN:/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$W_DIR" \
    DISK_WATCHDOG_CONFIG="$W_CFG" \
    DISK_WATCHDOG_LOG="$W_LOG" \
    RICHOS_LAUNCH_AGENTS_DIR="$W_LA" \
    /bin/bash "$WD" --check >/dev/null 2>&1
RC=$?
LINES_AFTER=0
[ -f "$W_LOG" ] && LINES_AFTER="$(wc -l <"$W_LOG" | tr -d ' ')"
if [ "$LINES_AFTER" -gt "$LINES_BEFORE" ] && grep -q 'reading ' "$W_LOG" 2>/dev/null; then
    ok "W15  with an EMPTY environment it still takes a reading and logs it"
else
    bad "W15  it needs a session's environment to work (rc=$RC)"
    tail -3 "$W_LOG" 2>/dev/null | sed 's/^/        /'
fi

# A scheduled service has no interactive shell PATH. Bake in the same runtime
# search path as the reaper instead of selecting Apple's bundled Python.
world runtime-path
write_cfg 1 999999 1 1
FAKE_INSTALL="$W_DIR/persistent/scripts"
mkdir -p "$FAKE_INSTALL" "$W_DIR/home"
cp "$WD" "$FAKE_INSTALL/disk-watchdog.sh"
HOME="$W_DIR/home" DISK_WATCHDOG_CONFIG="$W_CFG" RICHOS_LAUNCH_AGENTS_DIR="$W_LA" \
    bash "$FAKE_INSTALL/disk-watchdog.sh" --print-plist > "$W_LA/com.richos.disk-watchdog.plist" 2>"$W_DIR/render.err"
if python3 - "$W_LA/com.richos.disk-watchdog.plist" <<'PYTHON'
import plistlib, sys
with open(sys.argv[1], 'rb') as f:
    config = plistlib.load(f)
assert config['EnvironmentVariables']['PATH'].split(':')[:2] == ['/opt/homebrew/bin', '/usr/local/bin']
PYTHON
then
    ok "W15b the install renderer uses the reaper runtime PATH"
else
    bad "W15b service renderer did not emit the runtime PATH"
fi

# ===========================================================================
# W16 — --install refuses from a place whose path will stop existing
# ===========================================================================
# The plist bakes in an absolute path. An agent worktree is deleted at land time,
# and from that moment the job fires on time, every time, and executes nothing.
world refuse
write_cfg 1 999999 1 1
FAKE="$W_DIR/.claude/worktrees/agent-atest/engine/scripts"
mkdir -p "$FAKE/lib"
cp "$WD" "$FAKE/disk-watchdog.sh"
cp "$SCRIPT_DIR/lib/disk-watchdog.py" "$FAKE/lib/"
OUT="$(PATH="$W_BIN:$PATH" DISK_WATCHDOG_CONFIG="$W_CFG" \
       RICHOS_LAUNCH_AGENTS_DIR="$W_LA" \
       bash "$FAKE/disk-watchdog.sh" --install 2>&1)"; RC=$?
if [ "$RC" != "0" ] && printf '%s' "$OUT" | grep -q 'REFUSING TO SCHEDULE'; then
    ok "W16  --install refuses from inside .claude/worktrees"
else
    bad "W16  it scheduled itself from a worktree (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
if [ ! -f "$W_LA/com.richos.disk-watchdog.plist" ]; then
    ok "W16b ...and wrote no plist"
else
    bad "W16b a plist was written despite the refusal"
fi

# ===========================================================================
# W17 — ONE ENVIRONMENT VARIABLE MUST NOT SILENCE THE FAILURE ALERT (D9)
# ===========================================================================
# frank-opus-garbage1 turned the whole MASSIVE ALERT off with a single exported
# variable and no sign that it was off. The sweeper's failures_path() honored
# CLAUDE_CONFIG_DIR; this program's reader did not; and SCRATCH_FAILURES_STATE was
# declared in no config file, so nothing reconciled them. The sweeper wrote its
# failure under $CLAUDE_CONFIG_DIR/state/ and `--alert` read ~/.claude/state/ and
# printed nothing, exit 0.
#
# The engine's own suites export CLAUDE_CONFIG_DIR (run-all-tests.test.sh:43), so
# this was a live knob rather than a hypothetical one.
world configdir
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
# The config here deliberately does NOT declare SCRATCH_FAILURES_STATE, which is
# the state Frank found the shipped engine in: the DEFAULT has to honor
# CLAUDE_CONFIG_DIR, or the declaration is the only thing standing between this
# alert and silence.
grep -v 'SCRATCH_FAILURES_STATE' "$W_CFG" >"$W_CFG.tmp" && mv "$W_CFG.tmp" "$W_CFG"
mkdir -p "$W_DIR/cfg/state"
python3 - "$W_DIR/cfg/state/scratch-failures.json" <<'PY'
import json, sys
json.dump({"/tmp/silenced-by-an-env-var": {
    "first": "2026-09-18T09:04:35Z", "last": "2026-09-18T09:04:35Z",
    "error": "[Errno 13] Permission denied", "attempts": 1}},
    open(sys.argv[1], "w"), indent=1)
PY
OUT="$(CLAUDE_CONFIG_DIR="$W_DIR/cfg" run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'silenced-by-an-env-var'; then
    ok "W17  CLAUDE_CONFIG_DIR is honored — the failure alert cannot be silenced"
else
    bad "W17  one exported variable still turns the MASSIVE ALERT off (D9, rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
# THE CONTROL. Without it this passes against a program that alerts on anything:
# the same run with nothing in that directory must be silent.
rm -f "$W_DIR/cfg/state/scratch-failures.json"
OUT="$(CLAUDE_CONFIG_DIR="$W_DIR/cfg" run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "W17b CONTROL: with no failure in that directory it is silent"
else
    bad "W17b it alerted with nothing to alert about (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# ===========================================================================
# W18 — THE GARBAGE ALARM (Frank's Fix 2)
# ===========================================================================
# His verdict on everything above this case: "Its alarm is a disk-space alarm and
# a delete-failure alarm. IT HAS NO GARBAGE ALARM. So garbage that no arm looks at
# produces neither a cleanup nor an alert" — while ~53 GiB sat on the disk and
# every report read green. This is the missing channel: the sweeper publishes what
# it will never collect, and this job says so without ever running the sweeper.
world garbagealarm
write_cfg 999999999 999999 1 1     # floors so low nothing else can alert
sed -i.bak 's/^DISK_RICH_ALERT_GB=.*/DISK_RICH_ALERT_GB="1"/' "$W_CFG"
touch "$W_LA/com.richos.disk-watchdog.plist"
mkdir -p "$W_DIR/state"
# A HEALTHY scheduled result carrying the foreign pile: this case is about the
# report, and cleanup health has its own case (W23).
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"deleted": 3, "freed": 100, "skipped": 2934, "skipped_bytes": 1125454451'
{
    echo "SCRATCH_REAPER_STATE=\"$W_DIR/state/scratch-reaper-state.json\""
    echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1073741824"'
    echo 'SCRATCH_UNDECIDABLE_NOTICE_BYTES="67108864"'
} >>"$W_CFG"
OUT="$(run_wd --alert)"; RC=$?
# AMENDED 2026-09-19. It used to assert rc=1 and the heading "GARBAGE NOTHING
# WILL EVER COLLECT", and the CEO read the result of that: "And what is this all
# about: MASSIVE ALERT — DISK: ... 13.9 GB in 4122 place(s) NOTHING WILL EVER
# COLLECT". Measured the same hour, that 13.9 GB was 12.8 GB of OUR OWN campaign
# roots, each already printing the `rm -rf` that reclaims it, summed with 1.15 GB
# of Visual Studio Code's and Codex's temporary files that nobody should ever
# touch. Two opposite responses under one MASSIVE ALERT.
#
# THE PILE IS STILL REPORTED — the reporting is the half of §54 Fix 2 added, and
# removing it would be going back to silence. What changes is that other
# programs' temp is not an ALARM: it appears under its own heading and it does
# NOT raise the exit code. The alarm is reserved for the branch where a person
# must go and delete something, which this is not.
if [ "$RC" = "0" ] \
   && printf '%s' "$OUT" | grep -q 'GARBAGE REPORT (not an alert)' \
   && printf '%s' "$OUT" | grep -q 'BELONGS TO OTHER PROGRAMS' \
   && printf '%s' "$OUT" | grep -q '2934'; then
    ok "W18  other programs' temp is REPORTED with its count and is not an alarm"
else
    bad "W18  1.05 GB of foreign temp did not report, or reported as an alarm (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -14
fi
# AND IT MUST NOT WEAR THE ALARM'S WORDS. `grep -q 'MASSIVE ALERT'` is the
# assertion that would have caught the line he actually read.
case "$OUT" in
    *"MASSIVE ALERT"*)
        bad "W18a other programs' temporary files are still under a MASSIVE ALERT"
        printf '%s\n' "$OUT" | sed 's/^/        /' | head -6 ;;
    *)  ok "W18a ...and the words MASSIVE ALERT appear nowhere near it" ;;
esac
# THE CONTROL, and it is the one that matters most for an alarm: below the
# declared threshold it must be COMPLETELY silent. An alarm that always fires is
# the failure this suite's own header ranks as worse than not firing at all.
sed -i.bak 's/^SCRATCH_SKIPPED_NOTICE_BYTES=.*/SCRATCH_SKIPPED_NOTICE_BYTES="9999999999999"/' "$W_CFG"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "W18b CONTROL: below the declared threshold it is completely silent"
else
    bad "W18b the garbage alarm ignores its own threshold (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
# THE UNDECIDABLE PILE, CHANGED 2026-10-23 (cleanup PRD R4). It used to be a line
# to READ that never raised the exit code, and that is how days of a broken
# scheduled cleanup ("the open-file table could not be read") sat in a report
# nobody acted on. An undecided entry in the scheduled result is an UNRESOLVED
# cleanup: an alarm, at any size, with one owner action.
#
# CHANGED 2026-10-24 (CEO: "remove obsolete instructions and repeated
# administrative noise without hiding genuine failures"). A group the reaper
# names by its class and KEPT on purpose is reported under its own heading and is
# not an alarm. A run that could not read what every decision depends on (the
# open-file table, above) still is, and so is a row without a class.
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "undecidable": 23, "undecidable_bytes": 164580800, "verdict": "undecided", "unresolved": [{"scope": "/tmp/x", "cause": "23 undecided tmp-unknown entries (157.0 MB), e.g. /tmp/x/a: wall 2"}]'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: /tmp/x: 23 undecided'; then
    ok "W18c an unresolved row the reaper did not class is still an alarm"
else
    bad "W18c an unclassed unresolved row did not alarm (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
# THE MIXED STATE OF 2026-10-24 16:00:44Z: no failure, an undecidable repository
# whose main cannot be read, finished checkouts a Codex chat still has attached,
# and checkouts under temp roots. Each row carries the reaper's own condition
# code; honest and short, and no alarm.
KEPT_ROWS='{"scope": "/v/ab/worktrees", "class": "codex-worktree", "condition": "main-unreadable", "count": 1, "bytes": 0, "identity": "aaaa", "cause": "1 undecided codex-worktree entry (0 B), e.g. /v/ab/worktrees/hh/prospects: main of /u/prospects cannot be read"}, {"scope": "/v/codex/worktrees", "class": "codex-worktree", "condition": "chat-attached", "count": 13, "bytes": 0, "identity": "bbbb", "cause": "13 undecided codex-worktree entries (0 B), e.g. /v/codex/worktrees/a/richos: finished, but a Codex chat still has it attached, so it is kept"}, {"scope": "/v/tmp/codex", "class": "tmp-unknown", "condition": "wall-2", "count": 13, "bytes": 6299191717, "identity": "cccc", "cause": "13 undecided tmp-unknown entries (5.9 GB), e.g. /v/tmp/codex/h: wall 2: this tree contains a .git"}'
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "failures": 0, "undecidable": 27, "verdict": "undecided", "unresolved": ['"$KEPT_ROWS"']'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q 'MASSIVE ALERT\|CLEANUP HAS NOT RECOVERED' \
   && printf '%s' "$OUT" | grep -q 'KEPT OR UNDECIDED (not an alert): 27 entries in 3 group(s) the scheduled run of .* left in place and will not delete on its own (set [0-9a-f]*)' \
   && printf '%s' "$OUT" | grep -q "/v/ab/worktrees (undecided: its repository's main cannot be read): 1 undecided codex-worktree entry" \
   && ! printf '%s' "$OUT" | grep -qi 'nothing failed\|safe'; then
    ok "W18d kept or undecided leftovers are reported under their own heading, not an alarm, never called safe"
else
    bad "W18d kept leftovers alarmed, went unreported or were called safe (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# --status runs the reaper live; a command that is not there keeps it on the file.
echo "SCRATCH_REAPER_CMD=\"$W_DIR/nothing-here.sh\"" >>"$W_CFG"
OUT="$(run_wd --status)"; RC=$?
if printf '%s' "$OUT" | grep -q '^kept    /v/codex/worktrees (a finished checkout a Codex chat still has attached): 13 undecided' \
   && ! printf '%s' "$OUT" | grep -q 'cleanup has not recovered'; then
    ok "W18e --status still lists every kept leftover, marked kept, not ALERT"
else
    bad "W18e --status lost the kept leftovers or alarmed on them"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# ONLY THE REAPER'S ESTABLISHED CODES ARE QUIET. A classed row with no condition
# (a reading the run could not make, an older result), an unknown code and a
# malformed one all stay alarms, beside the kept rows.
for EXTRA in '{"scope": "/v/tmp", "class": "tmp-unknown", "count": 1, "bytes": 0, "cause": "1 undecided tmp-unknown entry (0 B), e.g. /v/tmp: THE OPEN-FILE TABLE COULD NOT BE READ"}' \
             '{"scope": "/v/tmp", "class": "codex-worktree", "condition": "some-new-code", "count": 1, "bytes": 0, "cause": "1 undecided codex-worktree entry (0 B), e.g. /v/tmp: its status cannot be read"}' \
             '{"scope": "/v/tmp", "class": "codex-worktree", "condition": ["wall-2"], "count": 1, "bytes": 0, "cause": "1 undecided codex-worktree entry (0 B), e.g. /v/tmp: malformed"}'; do
    healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$KEPT_ROWS"', '"$EXTRA"']'
    OUT="$(run_wd --alert)"; RC=$?
    if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: /v/tmp: 1 undecided'; then
        ok "W18f an undecided row without an established condition code is still an alarm ($(printf '%s' "$EXTRA" | cut -c60-110)...)"
    else
        bad "W18f an undecided row without an established code was quieted (rc=$RC): $EXTRA"
        printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
    fi
done
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "failures": 1, "unresolved": ['"$KEPT_ROWS"', {"scope": "/v/tmp", "class": "deletion", "count": 1, "cause": "1 deletion(s) failed, e.g. /v/tmp/x: permission denied"}]'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: /v/tmp: 1 deletion(s) failed' \
   && ! printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: .*(+' \
   && printf '%s' "$OUT" | grep -q 'KEPT OR UNDECIDED (not an alert): 27 entries' \
   && ! printf '%s' "$OUT" | grep -qi 'nothing failed'; then
    ok "W18g a failed deletion beside kept leftovers is the alarm, and the kept heading claims nothing about it"
else
    bad "W18g a failed deletion beside kept leftovers did not alarm alone, or the kept heading denied it (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -14
fi
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "last_apply_epoch": int(time.time()) - 13 * 3600, "unresolved": ['"$KEPT_ROWS"']'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: the last completed scheduled run at .* is past the 12 h window'; then
    ok "W18h a stale scheduled run with kept leftovers is still an alarm"
else
    bad "W18h a stale run hid behind its kept leftovers (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# THE SET IS ITS IDENTITY: sizes and times never change it; a new entry, and the
# same entries in a changed condition, do.
kept_set() { run_wd --json 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("cleanup_kept_set") or "")'; }
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$KEPT_ROWS"']'
SET1="$(kept_set)"
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "last_apply_epoch": int(time.time()) - 3600, "unresolved": ['"$(printf '%s' "$KEPT_ROWS" | sed 's/6299191717/7299191717/; s/(5.9 GB)/(6.8 GB)/')"']'
SET2="$(kept_set)"
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$(printf '%s' "$KEPT_ROWS" | sed 's/"count": 13, "bytes": 0, "identity": "bbbb"/"count": 14, "bytes": 0, "identity": "bbbd"/')"']'
SET3="$(kept_set)"
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"outcome": "unresolved", "unresolved": ['"$(printf '%s' "$KEPT_ROWS" | sed 's/"condition": "main-unreadable"/"condition": "chat-attached"/')"']'
SET4="$(kept_set)"
if [ -n "$SET1" ] && [ "$SET1" = "$SET2" ] && [ "$SET3" != "$SET1" ] && [ "$SET4" != "$SET1" ]; then
    ok "W18i the kept set is keyed by which leftovers in which condition, never by bytes or time"
else
    bad "W18i the kept set id moved with a size or time, or missed a change ($SET1 $SET2 $SET3 $SET4)"
fi
# THE PRODUCER: the reaper publishes each undecided group's condition code, and
# the group identity moves when the same entry's condition changes.
if python3 - "$SCRIPT_DIR/lib/scratch-reaper.py" <<'PYID'
import importlib.util, sys
from types import SimpleNamespace
sys.path.insert(0, sys.argv[1].rsplit("/", 1)[0])
spec = importlib.util.spec_from_file_location("srt_reaper_id", sys.argv[1])
sr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sr)
wall = sr.Refusal(2, "wall 2: this tree contains a .git")
assert wall.condition == "wall-2" and wall.startswith("wall 2:"), wall
def rows(cond):
    e = sr.Entry("/r/a", "codex-worktree", 0, sr.INDETERMINATE, "why", condition=cond)
    return sr.unresolved_rows(SimpleNamespace(roots=["/r"], entries=[e], deferred_skipped=0), [])
a, b, none = rows("main-unreadable"), rows("chat-attached"), rows(None)
assert a[0]["condition"] == "main-unreadable" and "condition" not in none[0], (a, none)
assert a[0]["identity"] != b[0]["identity"], (a, b)
assert rows("main-unreadable")[0]["identity"] == a[0]["identity"]
assert sr.Entry("/r/b", "tmp-unknown", 5, sr.INDETERMINATE, wall).condition == "wall-2"
PYID
then
    ok "W18j the reaper publishes each undecided group's condition, and the identity follows it"
else
    bad "W18j the reaper does not publish the condition, or its identity ignores it"
fi
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json"

# ===========================================================================
# W19 — THREE PILES, THREE HEADINGS, AND ONLY ONE OF THEM IS AN ALARM
# ===========================================================================
# The line the CEO read on 2026-09-19 ran all three together:
#
#   MASSIVE ALERT — DISK: 218 path(s) could not be deleted (delete BY HAND)
#   | 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT
#   | 11.4 GB in 2 place(s) UNDECIDABLE — no run will clear it
#
# Under one heading: 200 paths a defect could not delete, 18 the kernel owns,
# 12.8 GB of our own campaign roots with a printed command, 1.15 GB of another
# program's temp, and one figure six hours stale. Four different responses. An
# alarm whose items do not share a response is one nobody can act on — and this
# suite's own header ranks firing wrongly as worse than not firing.
world threepiles
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
mkdir -p "$W_DIR/state"
python3 - "$W_FAILS" <<'PY'
import json, sys
json.dump({"/tmp/ours-that-would-not-go": {
    "first": "2026-09-19T01:00:00Z", "last": "2026-09-19T12:00:00Z",
    "error": "[Errno 13] Permission denied", "attempts": 4}}, open(sys.argv[1], "w"))
PY
python3 - "$W_DIR/state/scratch-reaper-state.json" <<'PY'
import json, sys, time
json.dump({"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "last_apply_epoch": int(time.time()),
           "source": "launchd", "outcome": "healthy", "unresolved": [], "roots": [],
           "deleted": 3, "freed": 100, "undecidable": 0, "undecidable_bytes": 0,
           "skipped": 4122, "skipped_bytes": 14958789966,
           "skipped_split": {
               "by_hand": {"count": 2, "bytes": 13750465408,
                           "label": "OURS", "paths": [
                               {"path": "/Users/x/ab/campaign-done",
                                "bytes": 13749244608,
                                "why": "PAST ITS RETENTION ... a person removes it:  "
                                       "rm -rf /Users/x/ab/campaign-done"}]},
               "foreign": {"count": 4120, "bytes": 1208324558, "label": "theirs"}},
           "failures": 1, "verdict": "decided"}, open(sys.argv[1], "w"))
PY
{
    echo "SCRATCH_REAPER_STATE=\"$W_DIR/state/scratch-reaper-state.json\""
    echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1073741824"'
    echo 'SCRATCH_UNDECIDABLE_NOTICE_BYTES="67108864"'
} >>"$W_CFG"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] \
   && printf '%s' "$OUT" | grep -q 'MASSIVE ALERT — CLEAN-UP FAILED' \
   && printf '%s' "$OUT" | grep -q 'ours-that-would-not-go'; then
    ok "W19  OUR failed deletion is the MASSIVE ALERT, and the banner names it"
else
    bad "W19  a failed clean-up did not raise the alarm, or the banner still says DISK SPACE (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
# THE BANNER IS PART OF THE ASSERTION. It said "MASSIVE ALERT — DISK SPACE"
# while firing on 218 undeletable paths with 198 GB free, so the first line a
# reader saw was about a thing that was not happening.
case "$OUT" in
    *"MASSIVE ALERT — DISK SPACE"*)
        bad "W19a the banner still says DISK SPACE when the disk is fine" ;;
    *)  ok "W19a ...and it does not say DISK SPACE when the disk is not the problem" ;;
esac
if printf '%s' "$OUT" | grep -q 'IS OURS AND IS NEVER DELETED' \
   && printf '%s' "$OUT" | grep -q 'rm -rf /Users/x/ab/campaign-done'; then
    ok "W19b the by-hand pile is its OWN heading and carries the command that clears it"
else
    bad "W19b 12.8 GB a person could reclaim is still summed into somebody else's number"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -20
fi
# AND THE ONE THAT DECIDES WHETHER THE SPLIT IS REAL: with the failure removed,
# the SAME 13.9 GB is reported and the run exits 0.
python3 - "$W_FAILS" <<'PY'
import json, sys
json.dump({}, open(sys.argv[1], "w"))
PY
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && printf '%s' "$OUT" | grep -q 'GARBAGE REPORT (not an alert)'; then
    ok "W19c CONTROL: the same 13.9 GB alone reports and exits 0"
else
    bad "W19c the garbage report still raises the exit code on its own (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi

# ===========================================================================
# W20 — EVERY FIGURE CARRIES THE TIME IT WAS MEASURED
# ===========================================================================
# CEO, 2026-09-19: "11.4 GB in 2 place(s) UNDECIDABLE — no run will clear it".
# He had deleted both of those piles BY HAND an hour earlier, and the live sweep
# said one place at 2,176 bytes. The number came from the state file the 12:10Z
# scheduled pass wrote, with nothing on the line to say it was six hours old. A
# cached number printed in the present tense is the stale-artifact failure, and
# it costs one clause to avoid.
world dated
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
mkdir -p "$W_DIR/state"
# A HEALTHY scheduled result six hours old, carrying a foreign pile to report
# (the undecided pile is a cleanup alarm since 2026-10-23, W18c/W23).
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 6 * 3600)), "last_apply_epoch": int(time.time() - 6 * 3600), "skipped": 4122, "skipped_bytes": 12228131110'
{
    echo "SCRATCH_REAPER_STATE=\"$W_DIR/state/scratch-reaper-state.json\""
    echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1073741824"'
    echo 'SCRATCH_UNDECIDABLE_NOTICE_BYTES="67108864"'
    echo 'SCRATCH_REAPER_STATE_MAX_AGE_HOURS="12"'
} >>"$W_CFG"
OUT="$(run_wd --alert)"
if printf '%s' "$OUT" | grep -q 'measured .* ago' \
   && printf '%s' "$OUT" | grep -q '6 h'; then
    ok "W20  a figure from the last scheduled pass says when it was measured"
else
    bad "W20  the 11.4 GB is still printed as though it were the present"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
case "$OUT" in
    *"OUT OF DATE"*) bad "W20a six hours is inside the declared 12 h window and is not stale" ;;
    *) ok "W20a ...and inside the declared window it is dated, not disowned" ;;
esac
# PAST THE WINDOW IT IS DISOWNED, not quoted. Two cadences of the six-hourly
# sweep is the declared point at which a number stops being evidence.
sed -i.bak 's/^SCRATCH_REAPER_STATE_MAX_AGE_HOURS=.*/SCRATCH_REAPER_STATE_MAX_AGE_HOURS="1"/' "$W_CFG"
OUT="$(run_wd --alert)"
if printf '%s' "$OUT" | grep -q 'THESE FIGURES ARE OUT OF DATE'; then
    ok "W20b past the declared window the figure is disowned, not quoted as fact"
else
    bad "W20b a six-hour-old number past a one-hour window still reads as current"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
# AND A SCHEDULED RESULT PAST THE WINDOW IS A CLEANUP ALARM (cleanup PRD R4): a
# job that stopped publishing looks exactly like a machine with nothing to clean.
if printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: the last completed scheduled run at .* is past the 1 h window'; then
    ok "W20c past the window the stale scheduled result is also a cleanup alarm"
else
    bad "W20c a stale scheduled result did not alarm"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi

# ===========================================================================
# W21 — --status RUNS THE SWEEPER; IT NEVER QUOTES A CACHED NUMBER
# ===========================================================================
# The other half of the same defect. `--alert` and `--check` fire from launchd
# and from a session hook and must stay cheap, so they read the published file
# and date it. `--status` is a PERSON WAITING FOR AN ANSWER, and the honest
# answer there is this second's, which costs about nine seconds once.
world statuslive
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
mkdir -p "$W_DIR/state"
healthy_reaper_state "$W_DIR/state/scratch-reaper-state.json" '"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 6 * 3600)), "last_apply_epoch": int(time.time() - 6 * 3600), "undecidable": 2, "undecidable_bytes": 12228131110, "skipped": 4122, "skipped_bytes": 14958789966'
# A STUB SWEEPER WHOSE ANSWER IS UNMISTAKABLY DIFFERENT from the file's. If
# --status prints the file's 11.4 GB, it did not run anything.
cat >"$W_DIR/fake-reaper.sh" <<'STUB'
#!/bin/sh
cat <<'JSON'
{"verdict": "decided", "applied": false, "deleted": 0, "freed": 0,
 "undecidable": 0, "undecidable_bytes": 0,
 "skipped": 7, "skipped_bytes": 7654321,
 "skipped_split": {"by_hand": {"count": 0, "bytes": 0, "paths": []},
                   "foreign": {"count": 7, "bytes": 7654321}},
 "failures": 0}
JSON
STUB
chmod +x "$W_DIR/fake-reaper.sh"
{
    echo "SCRATCH_REAPER_STATE=\"$W_DIR/state/scratch-reaper-state.json\""
    echo "SCRATCH_REAPER_CMD=\"$W_DIR/fake-reaper.sh\""
    echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1"'
    echo 'SCRATCH_UNDECIDABLE_NOTICE_BYTES="1"'
} >>"$W_CFG"
OUT="$(run_wd --status)"
if printf '%s' "$OUT" | grep -q 'measured just now' \
   && ! printf '%s' "$OUT" | grep -q '11.4 GB'; then
    ok "W21  --status reports the sweeper's CURRENT verdict, not the cached one"
else
    bad "W21  --status is still quoting a six-hour-old figure as the present"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
case "$OUT" in
    *"ALERT"*)
        bad "W21a --status calls another program's temp an ALERT" ;;
    *)  ok "W21a ...and the foreign pile reads as information, not as an alarm" ;;
esac
# THE FALLBACK, AND IT SAYS SO. A sweeper that cannot be run is not a reason to
# print nothing and it is not a reason to pretend the old number is new.
sed -i.bak "s|^SCRATCH_REAPER_CMD=.*|SCRATCH_REAPER_CMD=\"$W_DIR/nothing-here.sh\"|" "$W_CFG"
OUT="$(run_wd --status)"
if printf '%s' "$OUT" | grep -q 'measured .* ago'; then
    ok "W21b when the sweeper cannot be run, --status says how old the figure is"
else
    bad "W21b the fallback prints a stale figure with no date on it"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi

# ===========================================================================
# W22 — never another app's data (2026-09-25)
# ===========================================================================
world appdata
write_cfg 999999 999999 0 0            # force an alert, so attribution runs
touch "$W_LA/com.richos.disk-watchdog.plist"
FAKE_HOME="$W_DIR/home"
C_APPS="$FAKE_HOME/Library/Containers"  # foreign-app-data-exempt: a fake one, inside this suite's sandbox
C_GROUP="$FAKE_HOME/Library/Group Containers/group.example"  # foreign-app-data-exempt: a fake one, inside this suite's sandbox
mkdir -p "$C_APPS/com.example.other/Data" "$C_GROUP" "$W_DIR/measured"
DU_LOG="$W_DIR/du-calls.txt"
cat >"$W_BIN/du" <<STUB
#!/bin/sh
# Records every path it is asked to measure, and measures nothing.
for a in "\$@"; do case "\$a" in -*) ;; *) printf '%s\n' "\$a" >>"$DU_LOG"; printf '4\t%s\n' "\$a" ;; esac; done
exit 0
STUB
chmod +x "$W_BIN/du"
# The group folder has a space in its name, which a space-separated list cannot
# carry; its parent's holder (~/Library) and the apps' folder cover the rule.
W_MEASURED_ROOT="$W_DIR/measured"
W_PROOF_ROOT="$C_APPS"
W_IOS_ROOT="$FAKE_HOME/Library"
W_CARGO_ROOT="$FAKE_HOME"
mkdir -p "$SANDBOX/appdata-process-tmp"
OUT="$(TMPDIR="$SANDBOX/appdata-process-tmp" HOME="$FAKE_HOME" run_wd --check)"
if grep -qx "$W_DIR/measured" "$DU_LOG" 2>/dev/null; then
    ok "W22  POSITIVE PROBE: an ordinary candidate is still measured by du"
else
    bad "W22  the ordinary candidate was not measured — the guard refuses everything"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
if grep -q "^$FAKE_HOME\(/Library\)\{0,1\}\(/Containers.*\)\{0,1\}$" "$DU_LOG" 2>/dev/null; then
    bad "W22a du was handed another app's data (or a folder holding it)"
    sed 's/^/        /' "$DU_LOG"
else
    ok "W22a du is never handed another app's data, ~/Library or the home folder"
fi
REFUSED="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1])).get("consumers_refused") or []))' "$W_STATE" 2>/dev/null)"
if [ "$REFUSED" = "3" ] && grep -q 'refused-candidate' "$W_LOG"; then
    ok "W22b each refused candidate is visible: state consumers_refused=3 and a log line"
else
    bad "W22b refused candidates are not reported (consumers_refused=$REFUSED)"
fi
# An entry INSIDE an app-group folder is refused by the same rule.
if python3 "$SCRIPT_DIR/lib/foreign_app_data.py" check "$C_GROUP" --home "$FAKE_HOME" >/dev/null; then
    bad "W22c an app-group folder was allowed"
else
    ok "W22c an app-group folder is refused too"
fi
# Check actual portable defaults with no private settings. This calls the same
# candidate function as main, not a copied list or the retired config key.
W22D_HOME="$(python3 -c 'import os, pwd; print(pwd.getpwuid(os.getuid()).pw_dir)')"
REAL_CANDIDATES="$(HOME="$W22D_HOME" RICHOS_MACHINE_SETTINGS_RESOLVED=1 python3 - "$SCRIPT_DIR/lib" <<'PYDEFAULT'
import os, runpy, sys
sys.path.insert(0,sys.argv[1])
import machine_settings as M
for key in M.KEYS:
    os.environ.pop(key,None)
watch=runpy.run_path(sys.argv[1]+'/disk-watchdog.py')
candidates, _volumes=watch['storage_candidates']()
allowed, refused=watch['measurable'](candidates)
assert candidates and not refused, refused
print('\n'.join(allowed))
PYDEFAULT
)"
if [ -n "$REAL_CANDIDATES" ]; then
    ok "W22d actual portable candidates preserve foreign-app data boundaries"
else
    bad "W22d actual default candidates enter foreign app data or could not be read"
fi

# A missing optional storage volume must not suppress the primary disk reading.
if python3 - "$SCRIPT_DIR/lib" "$SANDBOX" <<'PYABSENT'
import contextlib, importlib.util, io, json, os, sys
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import machine_settings as M
spec = importlib.util.spec_from_file_location('watchdog', Path(sys.argv[1]) / 'disk-watchdog.py')
watch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(watch)
env = {'HOME': sys.argv[2], 'TMPDIR': sys.argv[2], 'MODE': 'json',
       'DISK_PRIMARY_VOLUME': sys.argv[2], M.RESOLVED: '1',
       'RICHOS_BULK_ROOT': '/Volumes/WatchdogAbsentDrive/storage',
       'RICHOS_BUILD_CACHE': '/Volumes/WatchdogAbsentDrive/cache'}
with patch.dict(os.environ, env, clear=True), patch.object(Path, 'is_mount', return_value=False), \
        patch.object(watch, 'read_volume', return_value=(500 * watch.GB, 300 * watch.GB)) as read:
    candidates, extras = watch.storage_candidates()
    assert not extras and not any('/Volumes/WatchdogAbsentDrive' in p for p in candidates)
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        result = watch.main()
    report = json.loads(out.getvalue())
    # This home has no scheduled cleanup result, which is an unresolved cleanup
    # (cleanup PRD R4) and the only thing allowed to raise the exit code here.
    assert result == (1 if report['cleanup_problems'] else 0), result
    assert not any(v['below_rich'] or v['drop_alert'] for v in report['volumes']), report
    assert [v['mount'] for v in report['volumes']] == [sys.argv[2]], report
    read.assert_any_call(sys.argv[2])
PYABSENT
then
    ok "W22e primary disk monitoring survives an unplugged working drive"
else
    bad "W22e missing working storage stopped primary disk monitoring"
fi

echo ""
# ===========================================================================
# W23 — CLEANUP HEALTH, AND ITS ONE OWNER ACTION (cleanup PRD R4/R5, A3, A7)
# ===========================================================================
# On 2026-10-23 the scheduled cleanup had failed for days (lsof was not on
# launchd's PATH) and this job printed it as "GARBAGE REPORT (not an alert)". Now
# the scheduled result alone decides cleanup health; missing, manual, stale,
# uninstalled or unresolved is an alarm; and the installed job (launchd's service
# name) keeps ONE escalation for the lead in the existing ledger: raised once,
# unchanged after acknowledgement, reopened for a changed failure, left alone under a timed one,
# acknowledged by the job itself when a scheduled run covering the failed scope
# is healthy again.
world cleanup
write_cfg 1 999999 1 1
touch "$W_LA/com.richos.disk-watchdog.plist"
RS="$W_DIR/reaper-state.json"
LEDGER="$W_DIR/escalations.jsonl"
esc_rows() {               # <python expression over rows> -> printed value
    python3 - "$LEDGER" "$1" <<'PY'
import json, sys
rows = []
try:
    rows = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
except OSError:
    pass
print(eval(sys.argv[2], {"rows": rows}))
PY
}
wd_assoc() {
    python3 -c 'import json,sys; a=json.load(open(sys.argv[1])).get("cleanup_escalation") or {}; print(a.get("id") or "")' "$W_STATE"
}
esc() {
    RICHOS_ESCALATION_LEDGER="$LEDGER" bash "$SCRIPT_DIR/escalate.sh" "$@" >/dev/null 2>&1
}

rm -f "$RS"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'CLEANUP HAS NOT RECOVERED: no unattended cleanup result'; then
    ok "W23  no published scheduled result is an alarm, not a healthy silence"
else
    bad "W23  a missing scheduled result did not alarm (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
python3 - "$RS" <<'PY'
import json, sys, time
json.dump({"last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "last_apply_epoch": int(time.time()), "deleted": 1, "freed": 1,
           "undecidable": 0, "failures": 0, "verdict": "decided"}, open(sys.argv[1], "w"))
PY
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'does not hold a result of the scheduled job'; then
    ok "W23a a manual run's result does not stand in for the scheduled job's"
else
    bad "W23a a result with no scheduled source was read as health (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
healthy_reaper_state "$RS"
rm -f "$W_LA/com.richos.scratch-reaper.plist"
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'scheduled cleanup job is not installed'; then
    ok "W23b an uninstalled cleanup job is an alarm even beside a healthy old result"
else
    bad "W23b a missing cleanup job did not alarm (rc=$RC)"
fi
: >"$W_LA/com.richos.scratch-reaper.plist"
healthy_reaper_state "$RS" '"last_apply_epoch": int(time.time()) - 13 * 3600'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'past the 12 h window'; then
    ok "W23c a result past the declared 12 h window is an alarm (a stopped job)"
else
    bad "W23c a 13 h old scheduled result was read as healthy (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# --- the owner action -------------------------------------------------------
healthy_reaper_state "$RS" '"outcome": "unresolved", "unresolved": [{"scope": "/x/root", "cause": "1 undecided tmp-unknown entry: THE OPEN-FILE TABLE COULD NOT BE READ"}]'
# Through the shell, with the handoff forged by the caller: the shell decides from
# launchd's own name, so a manual --check writes nothing.
( export DISK_WATCHDOG_SCHEDULED=1; run_wd ) >/dev/null 2>&1
if [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "0" ]; then
    ok "W23d a --check outside the installed job writes no escalation, even with the handoff forged"
else
    bad "W23d a manual --check wrote to the escalation ledger"
fi
run_wd_scheduled >/dev/null 2>&1
ID1="$(wd_assoc)"
if [ -n "$ID1" ] && [ "$(esc_rows '[(r["teammate"], r["state"], r["for"]) for r in rows if r.get("event") == "Escalation"]')" = "[('scratch-reaper', 'proceeding', 'lead')]" ]; then
    ok "W23e the installed job raises ONE escalation for the lead, state proceeding, and remembers it"
else
    bad "W23e no single proceeding escalation for the lead (assoc=$ID1)"
    cat "$LEDGER" 2>/dev/null | sed 's/^/        /' | head -5
fi
run_wd_scheduled >/dev/null 2>&1
if [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "1" ] && [ "$(wd_assoc)" = "$ID1" ]; then
    ok "W23f the next tick raises nothing new while that escalation is open"
else
    bad "W23f a second tick raised another escalation for the same failure"
fi
OUT="$(run_wd --alert)"
if [ "$(printf '%s\n' "$OUT" | grep -c 'CLEANUP HAS NOT RECOVERED:')" = "1" ] \
   && printf '%s' "$OUT" | grep -q "Owner action: escalation $ID1"; then
    ok "W23g the notice is ONE short line naming the owner action and the detail"
else
    bad "W23g the cleanup notice is not one line with its escalation"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi
esc ack "$ID1" --disposition "acknowledged permanently before any repair, for the test"
run_wd_scheduled >/dev/null 2>&1
run_wd_scheduled >/dev/null 2>&1
if [ "$(wd_assoc)" = "$ID1" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "1" ]; then
    ok "W23h acknowledgement suppresses new escalations for the unchanged failed result"
else
    bad "W23h an unchanged acknowledged failure woke the lead again"
fi
# A different failure still reaches the lead. Acknowledgement is not health.
healthy_reaper_state "$RS" '"outcome": "unresolved", "roots": ["/x/root"], "unresolved": [{"scope": "/x/root", "cause": "permission denied"}]'
run_wd_scheduled >/dev/null 2>&1
ID2="$(wd_assoc)"
if [ -n "$ID2" ] && [ "$ID2" != "$ID1" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "2" ]; then
    ok "W23h2 a materially changed failure gets a new escalation after acknowledgement"
else
    bad "W23h2 a new failure was hidden by the old acknowledgement (id1=$ID1 id2=$ID2)"
fi
esc ack "$ID2" --disposition "deferred for an hour while the cause is diagnosed" --until "$(date -u -v+1d +%Y-%m-%dT%H:%M:%SZ)"
run_wd_scheduled >/dev/null 2>&1
if [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "2" ] && [ "$(wd_assoc)" = "$ID2" ]; then
    ok "W23i a TIMED acknowledgement gets no replacement; the same id stays associated"
else
    bad "W23i a timed acknowledgement was answered with a replacement"
fi
# Recovery before the hold expires, to a refreshed result holding ONLY a known
# kept leftover (the state of 2026-10-24): health clears and the HELD id is closed
# permanently at once, so its hold cannot expire and tell the lead again.
# (Changed 2026-10-24; it used to keep the id until the hold expired.)
healthy_reaper_state "$RS" '"outcome": "unresolved", "roots": ["/x/root"], "unresolved": [{"scope": "/x/root", "class": "codex-worktree", "condition": "chat-attached", "count": 1, "bytes": 0, "identity": "k1", "cause": "1 undecided codex-worktree entry (0 B), e.g. /x/root/a: finished, but a Codex chat still has it attached, so it is kept"}]'
OUT="$(run_wd --alert)"; RC=$?
run_wd_scheduled >/dev/null 2>&1
if [ "$RC" = "0" ] && [ -z "$(wd_assoc)" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "EscalationAck" and r.get("id") == "'"$ID2"'" and "Recovered" in r.get("disposition", "") and not r.get("until")])')" = "1" ]; then
    ok "W23j recovery during a hold, to only a kept leftover, closes the held id permanently"
else
    bad "W23j a held escalation stayed open after recovery (rc=$RC assoc=$(wd_assoc))"
    cat "$LEDGER" 2>/dev/null | sed 's/^/        /' | tail -4
fi
# The hold's date then passes: the permanent close outlives it; nothing reopens
# and no new id is raised.
python3 - "$LEDGER" "$ID2" <<'PY'
import json, sys, time
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
for r in rows:
    if r.get("event") == "EscalationAck" and r.get("id") == sys.argv[2] and r.get("until_epoch"):
        r["until_epoch"] = time.time() - 60
        r["until"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(r["until_epoch"]))
open(sys.argv[1], "w").write("".join(json.dumps(r) + "\n" for r in rows))
PY
run_wd_scheduled >/dev/null 2>&1
if [ -z "$(wd_assoc)" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "EscalationAck" and r.get("id") == "'"$ID2"'" and "Recovered" in r.get("disposition", "") and not r.get("until")])')" = "1" ] \
   && [ "$(python3 - "$LEDGER" "$ID2" "$SCRIPT_DIR/lib" <<'PYOUT'
import json, sys
sys.path.insert(0, sys.argv[3])
import escalations
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
print(len([e for e in escalations.outstanding(rows) if e.get("id") == sys.argv[2]]))
PYOUT
)" = "0" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')" = "2" ]; then
    ok "W23k after the hold's date passes the closed id stays closed; no new id"
else
    bad "W23k the expired hold reopened the closed id, or a new one was raised"
    cat "$LEDGER" 2>/dev/null | sed 's/^/        /' | tail -4
fi
# Only a run that inspected the scope that failed clears it.
healthy_reaper_state "$RS" '"outcome": "unresolved", "unresolved": [{"scope": "/y/root", "cause": "1 deletion failed"}]'
run_wd_scheduled >/dev/null 2>&1
ID3="$(wd_assoc)"
healthy_reaper_state "$RS" '"roots": []'
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q 'did not inspect it'; then
    ok "W23l a healthy run that never inspected the failed scope does not clear it"
else
    bad "W23l a no-op run cleared a failure in a scope it did not inspect (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
healthy_reaper_state "$RS" '"roots": ["/y/root"]'
run_wd_scheduled >/dev/null 2>&1
if [ -n "$ID3" ] && [ -z "$(wd_assoc)" ]; then
    ok "W23m a healthy run covering the failed scope clears it and closes its escalation"
else
    bad "W23m recovery covering the scope did not close $ID3"
fi

# W24 — EVERY FAILED ROOT STAYS ON THE ACTION UNTIL IT IS INSPECTED HEALTHY
# ===========================================================================
# Mid-job review rv-20261023T222724Z-e6524552-e592 #2: the action recorded failed
# scopes only while its list was empty, so a second root's failure never joined,
# and a failed deletion was published as the label "deletion", which no run can
# inspect. A healthy run of root A then cleared the action while root B's residue
# stayed. Every failed root now joins the action and clears only when a healthy
# scheduled run has inspected it.
REAL="$(cd "$W_DIR" && pwd -P)"
SA="$REAL/scope-a"; SB="$REAL/scope-b"; SC="$REAL/scope-c"
mkdir -p "$SA" "$SB" "$SC"
healthy_reaper_state "$RS" '"outcome": "unresolved", "roots": ["'"$SA"'", "'"$SB"'"], "unresolved": [{"scope": "'"$SA"'", "cause": "inspection unavailable"}]'
run_wd_scheduled >/dev/null 2>&1
ID4="$(wd_assoc)"
healthy_reaper_state "$RS" '"outcome": "unresolved", "roots": ["'"$SA"'", "'"$SB"'"], "unresolved": [{"scope": "'"$SB"'", "cause": "inspection unavailable"}]'
run_wd_scheduled >/dev/null 2>&1
healthy_reaper_state "$RS" '"roots": ["'"$SA"'"]'
run_wd_scheduled >/dev/null 2>&1
OUT="$(run_wd --alert)"; RC=$?
if [ -n "$ID4" ] && [ "$(wd_assoc)" = "$ID4" ] && [ "$RC" = "1" ] \
   && printf '%s' "$OUT" | grep -q "scope-b: the last healthy run did not inspect it"; then
    ok "W24  a second root's failure joins the action; a healthy run of the first root alone does not clear it"
else
    bad "W24  root B's failure was lost (rc=$RC assoc=$(wd_assoc) first=$ID4)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi
# A failed deletion, published by the reaper's own grouping, names its root.
python3 - "$RS" "$SC" "$SCRIPT_DIR/lib/scratch-reaper.py" <<'PY'
import importlib.util, json, sys, time
from types import SimpleNamespace
sys.path.insert(0, sys.argv[3].rsplit("/", 1)[0])
spec = importlib.util.spec_from_file_location("srt_reaper", sys.argv[3])
sr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sr)
rows = sr.unresolved_rows(SimpleNamespace(roots=[sys.argv[2]], entries=[], deferred_skipped=0),
                          [sys.argv[2] + "/leftover: permission denied"])
now = int(time.time())
json.dump({"source": "launchd", "outcome": "unresolved", "roots": [sys.argv[2]],
           "unresolved": rows, "last_apply_epoch": now, "result_epoch": now,
           "last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now))},
          open(sys.argv[1], "w"))
PY
run_wd_scheduled >/dev/null 2>&1
healthy_reaper_state "$RS" '"roots": ["'"$SA"'", "'"$SB"'"]'
run_wd_scheduled >/dev/null 2>&1
OUT="$(run_wd --alert)"; RC=$?
if [ "$(wd_assoc)" = "$ID4" ] && [ "$RC" = "1" ] \
   && printf '%s' "$OUT" | grep -q "scope-c: the last healthy run did not inspect it"; then
    ok "W24a a failed deletion is held by its root until a healthy run inspects that root"
else
    bad "W24a a failed deletion's root was dropped (rc=$RC assoc=$(wd_assoc))"
    python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("unresolved"))' "$RS" | sed 's/^/        /'
fi
healthy_reaper_state "$RS" '"roots": ["'"$SA"'", "'"$SB"'", "'"$SC"'"]'
run_wd_scheduled >/dev/null 2>&1
OUT="$(run_wd --alert)"; RC=$?
if [ "$RC" = "0" ] && [ -z "$(wd_assoc)" ] \
   && [ "$(esc_rows 'len([r for r in rows if r.get("event") == "EscalationAck" and r.get("id") == "'"$ID4"'" and "Recovered" in r.get("disposition", "")])')" = "1" ]; then
    ok "W24b a healthy run inspecting every failed root clears it and closes the one action"
else
    bad "W24b recovery of all failed roots did not close $ID4 (rc=$RC assoc=$(wd_assoc))"
fi

# W25 — ONE FINDING, ONE ID, HOWEVER ITS COUNT MOVES (2026-10-28/29)
# ===========================================================================
# "/private/tmp/claude-501: N undecided claude-session entries (S), e.g. <path>: <why>"
# reached the lead under 26 escalation ids, each acknowledged as the same finding again.
# Its count, size and example move with every second review's 96-byte folder; its root
# and its kind of failure do not. Acknowledged once, it stays acknowledged while only
# those move: a repeat keeps its id. (W23h2: another kind of failure is still new.)
SD="$REAL/scope-d"; mkdir -p "$SD"
claude_rows() {            # <count> <size> <example> <entry|entries>: the reaper's own shape
    healthy_reaper_state "$RS" '"outcome": "unresolved", "roots": ["'"$SD"'"], "unresolved": [{"scope": "'"$SD"'", "class": "claude-session", "count": '"$1"', "bytes": 0, "identity": "i'"$1"'", "cause": "'"$1"' undecided claude-session '"$4"' ('"$2"'), e.g. '"$SD/$3"': a running claude process is named by no session file and started before this scratch was last written, so it could own it"}]'
}
N0="$(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])')"
claude_rows 2 "320 B" a entries
run_wd_scheduled >/dev/null 2>&1
ID5="$(wd_assoc)"
esc ack "$ID5" --disposition "known: empty second-review session folders, filed as a follow-up"
claude_rows 15 "37.8 MB" b entries
run_wd_scheduled >/dev/null 2>&1
claude_rows 1 "96 B" c entry
run_wd_scheduled >/dev/null 2>&1
N5="$(( $(esc_rows 'len([r for r in rows if r.get("event") == "Escalation"])') - N0 ))"
if [ -n "$ID5" ] && [ "$(wd_assoc)" = "$ID5" ] && [ "$N5" = "1" ]; then
    ok "W25  an acknowledged finding whose count, size and example change keeps its one id (no new escalation)"
else
    bad "W25  the same finding was raised again under a new id (first=$ID5 now=$(wd_assoc) raised=$N5)"
fi

echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

#!/usr/bin/env bash
#
# run-suite.sh — run ONE host-screen suite in a test-VM guest, from executables this Mac
#                built, and answer with that suite's own result.
#
#   testvm/run-suite.sh <gui-host> <path to the suite> [--existing-guest <vm>] [--release <dir>]
# --existing-guest attaches to a guest the caller owns and cleans up, under its held slot.
# --release additionally runs front-door against that archive before the named suite.
#
# `run-tests.sh` calls this for a suite that sources lib/gui-launch.sh when RICHOS_GUI_HOST
# names the test VM; <gui-host> is that value. Nothing appears on this Mac's screen: the
# suite's boots happen on the guest's own display.
#
# =========================================================================================
# WHAT HAPPENS, IN ORDER
# =========================================================================================
#
#   1. BUILD HERE. The guest is a clean macOS with no cargo and no checkout, on purpose, like
#      the one gui-proof-in-vm.sh uses, so this Mac builds what the suite launches:
#      `gui_prebuild` (lib/gui-launch.sh) makes richos-tauri, carrying this checkout's
#      commit, and the gui_boot_machine example. A build that fails is exit 1, a verdict
#      about the code, exactly as it is when the suite builds for itself. No guest is held
#      while anything compiles.
#   2. STAGE the payload: those two executables, this scripts directory (the suite, its
#      libraries and the guest half, suite-in-guest.sh) and the source files the suite's
#      static cases read (src-tauri/src for S1, tauri.conf.json for C3, the updater's lib.rs
#      for C2). The engine the suite provisions from travels beside it, and so does the
#      delivered runtime (RICHOS_RUNTIME_DIR) when the engine carries none of its own.
#   3. GUEST, for the suite's run only. run-walk.py takes one of the two guest slots, boots a
#      fresh clone of the test VM's base with no app (--no-app), runs suite-walk.sh, and
#      stops and deletes the clone however the run ends. suite-walk.sh pushes the payload,
#      runs the suite in the guest's GUI session in PREBUILT MODE, and brings back its exit
#      code and its results folder.
#
# <gui-host> requests a guest. By default this runner owns a fresh clone of TESTVM_BASE_VM.
# --existing-guest uses the caller's already booted clone and leaves its cleanup to that caller.
#
# =========================================================================================
# EXIT
# =========================================================================================
#
#   the suite's own exit code, when it ran in the guest
#   1  the executables did not build (a verdict about the code), or the guest was NOT
#      cleaned up: a clone left on this Mac is a failure whatever the suite said (§54)
#   2  this Mac could not give the suite a guest: no slot within TESTVM_SUITE_WAIT seconds,
#      a guest that did not boot, a payload that did not arrive, a suite that never reached
#      its end, or a suite that passed but whose results folder did not come back (a failing
#      suite keeps its own exit code then). A fact about this machine, which run-tests.sh refuses unless declared.
#   64 a usage error
#
# A suite that declares `# run-tests: host-only: <reason>` is refused with exit 2 and that
# reason; front-door.test.sh is the one today (it drives a published release on this Mac).
#
# SETTINGS: TESTVM_SUITE_WAIT (seconds to wait for a guest slot, default 1800).
# RICHOS_GUI_PREBUILT names executables already built and builds nothing (said in the
# output). TESTVM_RUN_WALK replaces run-walk.py and TESTVM_GUEST_EXEC replaces guest.sh:
# run-tests.test.sh's stand-in guest uses both.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS="$(cd "$HERE/.." && pwd)"
APP_DIR="$(cd "$SCRIPTS/.." && pwd)"
REPO_DIR="$(cd "$APP_DIR/.." && pwd)"

say() { echo "run-suite.sh: $*"; }
no_guest() { echo "run-suite.sh: $*" >&2; exit 2; }
usage() { echo "run-suite.sh: usage: run-suite.sh <gui-host> <path to a suite in $SCRIPTS> [--existing-guest <vm>] [--release <dir>]" >&2; exit 64; }

GUI_HOST="${1:-}"; SUITE_PATH="${2:-}"
[ -n "$GUI_HOST" ] && [ -n "$SUITE_PATH" ] || usage
shift 2
EXISTING_GUEST=""; RELEASE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --existing-guest) [ -n "${2:-}" ] || usage; EXISTING_GUEST="$2"; shift 2 ;;
    --release) [ -n "${2:-}" ] || usage; RELEASE="$2"; shift 2 ;;
    *) usage ;;
  esac
done
if [ -n "$EXISTING_GUEST" ]; then
  [ -n "${TESTVM_SLOT:-}" ] || no_guest "--existing-guest requires the caller's held guest slot"
  [ -n "${RICHOS_GUI_PREBUILT:-}" ] || no_guest "build the executables before booting the existing guest (RICHOS_GUI_PREBUILT)"
fi
if [ -n "$RELEASE" ]; then
  [ -f "$RELEASE/RichOS.app.tar.gz" ] || no_guest "--release has no RichOS.app.tar.gz: $RELEASE"
  CLICLICK="$(command -v cliclick || true)"
  [ -n "$CLICLICK" ] || no_guest "front-door requires cliclick on PATH for transfer to the guest"
fi
[ -f "$SUITE_PATH" ] || { echo "run-suite.sh: no suite at $SUITE_PATH" >&2; exit 64; }
NAME="$(basename "$SUITE_PATH")"
[ "$(cd "$(dirname "$SUITE_PATH")" && pwd)" = "$SCRIPTS" ] \
  || { echo "run-suite.sh: $SUITE_PATH is not a suite in $SCRIPTS, the directory this runner stages" >&2; exit 64; }

HOST_ONLY="$(sed -n 's/^# run-tests: host-only:[[:space:]]*//p' "$SUITE_PATH" | head -1)"
if [ -n "$HOST_ONLY" ]; then
  no_guest "$NAME runs on this Mac's own screen only and has no guest route: $HOST_ONLY"
fi
if ! grep -qE '^[[:space:]]*(\.|source)[[:space:]]+[^[:space:]]*lib/gui-launch\.sh' "$SUITE_PATH"; then
  no_guest "$NAME does not launch through lib/gui-launch.sh, so it has no prebuilt mode to run in a guest with"
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/run-suite.XXXXXX")" || no_guest "cannot create a scratch directory"
trap 'rm -rf "$WORK"' EXIT
trap 'rm -rf "$WORK"; exit 130' INT TERM
PAYLOAD="$WORK/payload"
mkdir -p "$PAYLOAD/prebuilt" "$WORK/home"

# -- 1. build here ------------------------------------------------------------------------
GUI_APP_DIR="$APP_DIR"
export GUI_APP_DIR
# shellcheck source=../lib/gui-launch.sh
. "$SCRIPTS/lib/gui-launch.sh"
T0=$(date +%s)
if [ -n "${RICHOS_GUI_PREBUILT:-}" ]; then
  say "using the executables RICHOS_GUI_PREBUILT names, $RICHOS_GUI_PREBUILT; nothing is built"
  gui_prebuilt_check || no_guest "RICHOS_GUI_PREBUILT does not hold every executable the suite launches"
  for n in $GUI_PREBUILT_NAMES; do cp -p "$RICHOS_GUI_PREBUILT/$n" "$PAYLOAD/prebuilt/$n" || no_guest "could not copy $n"; done
else
  say "building what $NAME launches, on this Mac (the guest has no cargo)"
  gui_prebuild "$PAYLOAD/prebuilt"; rc=$?
  case "$rc" in
    0) ;;
    1) exit 1 ;;
    *) no_guest "the executables for the guest could not be produced (gui_prebuild exit $rc)" ;;
  esac
fi
BUILD_S=$(( $(date +%s) - T0 ))

# -- 2. stage ----------------------------------------------------------------------------
# The source files below are the ones the suite's static cases read; a case whose file is
# missing fails in the guest by name, so a new one shows up as a red case, never as a pass.
T="$PAYLOAD/tree/richos/app"
mkdir -p "$T/src-tauri" "$T/crates/richos-user-update" || no_guest "cannot stage the payload"
cp -R "$SCRIPTS" "$T/scripts" || no_guest "could not stage $SCRIPTS"
[ -d "$APP_DIR/src-tauri/src" ] && cp -R "$APP_DIR/src-tauri/src" "$T/src-tauri/src"
[ -f "$APP_DIR/src-tauri/tauri.conf.json" ] && cp "$APP_DIR/src-tauri/tauri.conf.json" "$T/src-tauri/"
[ -d "$APP_DIR/crates/richos-user-update/src" ] && cp -R "$APP_DIR/crates/richos-user-update/src" "$T/crates/richos-user-update/src"
if [ -n "$RELEASE" ]; then
  mkdir -p "$PAYLOAD/release" "$PAYLOAD/bin" || no_guest "cannot stage release inputs"
  cp "$RELEASE/RichOS.app.tar.gz" "$PAYLOAD/release/" || no_guest "could not stage the release archive"
  cp -p "$CLICLICK" "$PAYLOAD/bin/cliclick" || no_guest "could not stage cliclick"
fi

ENGINE="${RICHOS_GUI_ENGINE_SOURCE:-$REPO_DIR/engine}"
[ -d "$ENGINE" ] || { echo "run-suite.sh: no engine at $ENGINE for the suite to provision from" >&2; exit 1; }
RUNTIME=""
if [ ! -f "$ENGINE/runtime/delivery.json" ] && [ -n "${RICHOS_RUNTIME_DIR:-}" ]; then
  [ -d "$RICHOS_RUNTIME_DIR" ] || no_guest "RICHOS_RUNTIME_DIR names $RICHOS_RUNTIME_DIR, which is not a directory"
  RUNTIME="$RICHOS_RUNTIME_DIR"
fi
# With neither, the suite itself says what it needs (it exits 2 naming both) — in the guest,
# where the question is asked, rather than here in a second wording.

# -- 3. the guest, for the suite's run only ------------------------------------------------
RUN_WALK="${TESTVM_RUN_WALK:-$HERE/run-walk.py}"
WAIT="${TESTVM_SUITE_WAIT:-1800}"
# The engine travels in the payload with everything else the suite reads, so run.sh is
# given none to install.
# No network flag: this guest has nothing to do with the phone, and a walk does not join the
# tailnet unless it asks to (run-walk.py --tailnet).
WALK=(--no-app --home "$WORK/home" --engine "" --report "$WORK/walk.json" --wait "$WAIT")

STEP=(--stage "$PAYLOAD" --suite "$NAME" --engine "$ENGINE" --rc "$WORK/suite.rc")
[ -n "$RUNTIME" ] && STEP+=(--runtime "$RUNTIME")
# The pinned speech models gui-boot's healthy machine needs (the video tools are a setup
# essential; paths joined by ":"); pushed into the guest with the rest. Unset, the suite refuses
# there by name.
if [ -n "${RICHOS_GUI_SPEECH_MODELS:-}" ]; then
  IFS=':' read -r -a SPEECH_MODELS <<<"$RICHOS_GUI_SPEECH_MODELS"
  for model in "${SPEECH_MODELS[@]}"; do
    [ -f "$model" ] || no_guest "RICHOS_GUI_SPEECH_MODELS names $model, which is not a file"
    STEP+=(--speech-model "$model")
  done
fi
[ -n "${RICHOS_TEST_RESULTS_DIR:-}" ] && STEP+=(--results "$RICHOS_TEST_RESULTS_DIR")
if [ -n "$EXISTING_GUEST" ]; then
  say "using caller-owned guest $EXISTING_GUEST; the caller retains cleanup ownership"
  "$HERE/suite-walk.sh" "$EXISTING_GUEST" "${STEP[@]}"
  WALK_RC=$?
  W_VM="$EXISTING_GUEST"; W_CLEAN=yes; W_TIMES="existing guest; cleanup belongs to caller"
  CLEANUP_STATE="the caller retains guest cleanup ownership"
else
  say "running $NAME in a fresh guest for RICHOS_GUI_HOST=$GUI_HOST"
  CLEANUP_STATE="the guest was deleted"
  "$RUN_WALK" "${WALK[@]}" -- "$HERE/suite-walk.sh" "${STEP[@]}"
  WALK_RC=$?

if [ ! -f "$WORK/walk.json" ]; then
  # run-walk.py writes its report inside the slot, so no report means no guest was booted.
  case "$WALK_RC" in
    75) no_guest "no guest slot and CPU/memory admission within ${WAIT}s (run-walk.py exit 75); $NAME did not run" ;;
    *)  no_guest "no guest was booted (run-walk.py exit $WALK_RC); $NAME did not run" ;;
  esac
fi
eval "$(python3 - "$WORK/walk.json" <<'PY'
import json, shlex, sys
r = json.load(open(sys.argv[1]))
def num(k):
    v = r.get(k)
    return "%.0f" % v if isinstance(v, (int, float)) else "?"
print("W_VM=%s" % shlex.quote(str(r.get("vm", "?"))))
print("W_CLEAN=%s" % ("yes" if r.get("cleanup_complete") is True else "no"))
print("W_CLEAN_WHY=%s" % shlex.quote(str(r.get("cleanup_error", ""))))
print("W_TIMES=%s" % shlex.quote("boot %ss, suite %ss, cleanup %ss" % (num("boot_seconds"), num("scenario_seconds"), num("cleanup_seconds"))))
PY
)" || no_guest "run-walk.py's report $WORK/walk.json could not be read"
fi

if [ "$W_CLEAN" != yes ]; then
  echo "" >&2
  echo "  ############################################################" >&2
  echo "  ##  CLEANUP FAILED: A GUEST IS LEFT ON THIS MAC            ##" >&2
  echo "  ############################################################" >&2
  echo "  The clone '$W_VM' was not deleted: $W_CLEAN_WHY" >&2
  echo "  Delete it by hand:  $HERE/stop.sh $W_VM   (then $HERE/reap.sh)" >&2
  exit 1
fi
if [ ! -s "$WORK/suite.rc" ]; then
  no_guest "$NAME did not run to its end in guest $W_VM (suite-walk.sh exit $WALK_RC; $W_TIMES); $CLEANUP_STATE"
fi
SUITE_RC="$(cat "$WORK/suite.rc")"
if [ "$WALK_RC" = 4 ]; then
  # The suite ran to the end but the results folder asked for (RICHOS_TEST_RESULTS_DIR) did not
  # come back, and the guest is deleted: a failing verdict still stands as itself, a passing one
  # cannot stand with its evidence missing.
  if [ "$SUITE_RC" != 0 ]; then
    echo "run-suite.sh: $NAME exited $SUITE_RC in guest $W_VM and its results folder was lost (not brought back to ${RICHOS_TEST_RESULTS_DIR:-the results folder}); $CLEANUP_STATE" >&2
    exit "$SUITE_RC"
  fi
  no_guest "$NAME passed in guest $W_VM but its results folder could not be brought back to ${RICHOS_TEST_RESULTS_DIR:-the results folder}; $CLEANUP_STATE"
fi
if [ -n "$EXISTING_GUEST" ]; then
  say "$NAME exited $SUITE_RC in guest $W_VM; caller must stop and delete the guest"
else
  say "$NAME exited $SUITE_RC in guest $W_VM; built here in ${BUILD_S}s, $W_TIMES; the guest was stopped and deleted"
fi
exit "$SUITE_RC"

#!/usr/bin/env bash
#
# gui-proof-in-vm.sh — watch a candidate's own bundle start, in a guest, and write down
#                      what was seen, so `nightly-local.py publish` has something to check.
#
#   gui-proof-in-vm.sh --run <run-id> [--vm <name>] [--out <path>] [--wait <seconds>]
#   gui-proof-in-vm.sh --bundle <RichOS.app.zip> --commit <sha> [--screen-suites] [--vm <name>] [--out <path>] [--wait <seconds>]
# --run also runs front-door --release and gui-boot in this guest. --screen-suites does
# the same for --bundle, using RichOS.app.tar.gz beside the zip. Each suite keeps its own
# verdict and logs; a refusal or failure in either makes the combined proof fail.
#
# --wait: how long to wait for one of the test VM's two guest slots (testvm/slots.py) and
# its CPU admission; default 3600, so a build that is walked away from still gets its proof.
#
# =========================================================================================
# WHY THIS EXISTS
# =========================================================================================
#
# The CEO, 2026-09-19: *"So, every engineer will keep opening the app making me unable to do
# anything here or WHAT???"*
#
# `nightly-local.py build --no-host-screen` answers that by holding back every suite that
# boots the app, so a candidate is built, signed, notarized and walkable without one pixel
# reaching his Mac. The price is that nobody has watched the thing start. `publish` refuses
# such a candidate until `--gui-proof` names evidence that somebody did, and THIS is what
# produces that evidence — inside a `testvm` guest, on the guest's own virtual display,
# with nothing on the host's screen.
#
# =========================================================================================
# WHAT THIS PROVES, AND WHAT IT DOES NOT — the distinction is the whole value of the file
# =========================================================================================
#
# It boots THE ARTIFACT THE RELEASE WILL PUBLISH: the signed, notarized, stapled bundle out
# of the candidate's own staging directory, on a clean macOS 15 guest with no developer
# environment, no cargo, no repository and no `~/.claude` of the operator's. That is closer
# to a customer's first run than anything on a developer's Mac can be, and it is the
# question `publish` actually needs answered: does the thing about to become installable
# start, and does it draw a window.
#
# The initial boot names suite=shipped-bundle-boot because it checks the published artifact.
# The subsequent front-door suite drives that release archive. gui-boot checks separate
# debug executables built from this checkout on a synthetic machine. Their individual
# exit codes and gui-boot's source commit are recorded separately, so publication can
# require all three checks without attributing the debug suite to the signed archive.
#
# =========================================================================================
# THE GARBAGE (CEO ruling §54) AND THE HOST'S SCREEN
# =========================================================================================
#
# The guest clone, the app inside it and the fixture home are all garbage the moment the
# verdict is read, and every one of them is cleaned up however this script ends — including
# on INT and TERM. Where cleanup fails it SAYS SO LOUDLY and names what to delete by hand,
# rather than exiting 0 over a 25 GB surprise.
#
# Nothing here touches the host's display, power, session or input state: no `pmset`, no
# `caffeinate`, no keystroke, no `open` on this machine. The app is launched, watched and
# killed inside the guest, by pid (`testvm/stop.sh`'s own rule, written after a Command-Q
# through System Events landed on the CEO's Terminal).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTVM="$HERE/testvm"
STATE_DIR="${RICHOS_NIGHTLY_STATE:-$HOME/.richos-nightly}"

ORIG_ARGS=("$@")
RUN_ID=""; BUNDLE=""; COMMIT=""; VERSION=""; VM=""; OUT=""; WAIT=3600; SCREEN_SUITES=0
while [ $# -gt 0 ]; do
  case "$1" in
    --run)    RUN_ID="${2:-}"; SCREEN_SUITES=1; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    --screen-suites) SCREEN_SUITES=1; shift ;;
    --bundle) BUNDLE="${2:-}"; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    --commit) COMMIT="${2:-}"; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    --vm)     VM="${2:-}"; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    --out)    OUT="${2:-}"; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    --wait)   WAIT="${2:-}"; shift 2 || { echo "gui-proof-in-vm.sh: $1 needs a value" >&2; exit 2; } ;;
    # A kept guest is a guest held after its run, which the CEO ruled out on 2026-09-27;
    # the slot it ran in stops it on release anyway. Said here rather than ignored.
    --keep)   echo "gui-proof-in-vm.sh: --keep is retired: a guest is never kept after its run (testvm/slots.py). Read the proof and run.sh's output instead." >&2; exit 2 ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "gui-proof-in-vm.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

die() { echo "gui-proof-in-vm.sh: $*" >&2; exit 2; }

[ "$(uname -s)" = "Darwin" ] || die "the guest harness is macOS virtualization; this is $(uname -s)."
[ -x "$TESTVM/run.sh" ]  || die "no $TESTVM/run.sh — run testvm/setup.sh first (see docs/testvm.md)."
[ -x "$TESTVM/stop.sh" ] || die "no $TESTVM/stop.sh; refusing to start a guest I cannot clean up."

# ---------------------------------------------------------------------------------------
# Which candidate, and which commit it was built from
# ---------------------------------------------------------------------------------------
if [ -n "$RUN_ID" ]; then
  [ -z "$BUNDLE" ] || die "--run and --bundle name the same thing two ways; give one."
  POINTER="$STATE_DIR/runs/$RUN_ID.json"
  [ -f "$POINTER" ] || die "no recorded build for run $RUN_ID at $POINTER; run \`nightly-local.py build\` first."
  CAND_DIR="$(/usr/bin/python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["out"])' "$POINTER")" \
    || die "$POINTER is not readable as a run pointer"
  CANDIDATE="$CAND_DIR/candidate.json"
  [ -f "$CANDIDATE" ] || die "$CAND_DIR has no candidate.json; the build for run $RUN_ID did not finish."
  # THE COMMIT IS READ OUT OF THE CANDIDATE, never passed in beside it. A sha a caller
  # types is a sha a caller can mistype, and the one thing `publish` checks is that this
  # proof is about THAT tree.
  eval "$(/usr/bin/python3 - "$CANDIDATE" <<'PY'
import json, shlex, sys
info = json.load(open(sys.argv[1]))["info"]
commit = info.get("build_commit") or info["source_commit"]
print(f"COMMIT={shlex.quote(commit)}")
print(f"VERSION={shlex.quote(info['version'])}")
print(f"TAG={shlex.quote(info['tag'])}")
PY
)" || die "could not read $CANDIDATE"
  # make-release.sh's own ARCH mapping; this Mac is Apple Silicon only.
  BUNDLE="$CAND_DIR/RichOS-$VERSION-macos-aarch64.zip"
  [ -f "$BUNDLE" ] || die "the candidate has no first-install archive at $BUNDLE"
else
  [ -n "$BUNDLE" ] || die "one of --run <run-id> or --bundle <zip> is required."
  [ -e "$BUNDLE" ] || die "no such bundle: $BUNDLE"
  [ -n "$COMMIT" ] || die "--bundle needs --commit <sha>: a proof that does not name the tree it was taken against proves nothing about any particular one."
  TAG="${TAG:-unversioned}"
  CAND_DIR="$(cd "$(dirname "$BUNDLE")" && pwd)"
fi
if [ "$SCREEN_SUITES" = 1 ]; then
  [ -f "$CAND_DIR/RichOS.app.tar.gz" ] || die "screen suites need $CAND_DIR/RichOS.app.tar.gz"
  command -v cliclick >/dev/null 2>&1 || die "screen suites need cliclick on PATH to transfer into the guest"

  # THE RUNTIME AND THE SPEECH MODELS gui-boot's healthy machine needs, named the way the nightly
  # names them (nightly-local.py Runner.runtime; lib/runtime_cache.py). A worktree has no
  # extracted runtime (it is gitignored), so a retake of a boot proof from a worker's worktree
  # stopped at "provide an extracted engine with runtimes via RICHOS_GUI_ENGINE_SOURCE, or a
  # verified RICHOS_RUNTIME_DIR" until both variables were set by hand (2026-10-28). A caller's
  # own value still wins. Read here, before any guest or slot exists, so a missing runtime is
  # refused in a second with the one command that builds it.
  if [ -z "${RICHOS_RUNTIME_DIR:-}" ]; then
    RECIPE="$HERE/runtime-sources.json"
    RUNTIME="$(python3 "$HERE/lib/runtime_cache.py" path "$STATE_DIR" "$RECIPE")" \
      || die "cannot name the nightly's runtime for $RECIPE"
    BUILD_RUNTIME="python3 $(printf '%q %q' "$HERE/build-runtimes.py" "$RUNTIME")"
    [ -d "$RUNTIME" ] \
      || die "there is no runtime at $RUNTIME for this checkout's recipe. Build it once with: $BUILD_RUNTIME"
    python3 "$HERE/verify-runtime.py" "$RUNTIME" "$RECIPE" >/dev/null 2>&1 \
      || die "the runtime at $RUNTIME does not verify against $RECIPE. Rebuild it with: $BUILD_RUNTIME (run verify-runtime.py on it to see why)"
    export RICHOS_RUNTIME_DIR="$RUNTIME"
  fi
  if [ -z "${RICHOS_GUI_SPEECH_MODELS:-}" ] && SPEECH_MODELS="$(python3 "$HERE/lib/runtime_cache.py" models 2>/dev/null)"; then
    export RICHOS_GUI_SPEECH_MODELS="$SPEECH_MODELS"
  fi
fi

# ONE GUEST SLOT, FOR THIS RUN ONLY (testvm/slots.py; the CEO, 2026-09-27). Everything
# above only reads; from here on a guest exists, so the rest of this script runs again
# under a held slot and the slot is released the moment it exits.
if [ -z "${TESTVM_SLOT:-}" ]; then
  exec python3 "$TESTVM/slots.py" run --wait "$WAIT" --purpose "gui-proof ${TAG:-$COMMIT}" -- \
    "$0" ${ORIG_ARGS[@]+"${ORIG_ARGS[@]}"}
fi

[ -n "$VM" ] || VM="richos-gui-proof-$$"
[ -n "$OUT" ] || OUT="$STATE_DIR/gui-proofs/${RUN_ID:-$COMMIT}.proof"
mkdir -p "$(dirname "$OUT")" || die "cannot create $(dirname "$OUT")"

# ---------------------------------------------------------------------------------------
# Scratch, and the promise that none of it outlives this script (CEO ruling §54)
# ---------------------------------------------------------------------------------------
WORK="$(mktemp -d "${TMPDIR:-/tmp}/richos-gui-proof.XXXXXX")" || die "cannot create a scratch directory"
STOPPED=1
# cleanup returns the run's own status, unless that status was 0 and the guest could not be
# stopped: then 1. A failed run keeps its own code (the proof's result is not overwritten by
# the cleanup's), and a passing run never exits 0 over a guest left on this Mac (§54).
cleanup() {
  local rc=$?
  if [ "$STOPPED" = 0 ]; then
    STOPPED=1
    echo "gui-proof-in-vm.sh: stopping the guest and deleting the clone..."
    if ! "$TESTVM/stop.sh" "$VM" >"$WORK/stop.log" 2>&1; then
      # LOUD, never a footnote. A failed cleanup that exits quietly is how 105 GB sat in
      # $TMPDIR overnight, which is the incident §54 was written for.
      echo "" >&2
      echo "  ############################################################" >&2
      echo "  ##  CLEANUP FAILED — THERE IS A GUEST LEFT ON THIS MAC    ##" >&2
      echo "  ############################################################" >&2
      echo "  The clone '$VM' and/or the app inside it did not stop. Delete it by hand:" >&2
      echo "      $TESTVM/stop.sh $VM" >&2
      echo "      $TESTVM/reap.sh" >&2
      sed 's/^/      /' "$WORK/stop.log" >&2 2>/dev/null
      if [ "$rc" -eq 0 ]; then
        echo "  The proof's own result stands in its file; this command exits 1 because the cleanup failed." >&2
        rc=1
      fi
      echo "" >&2
    fi
  fi
  rm -rf "$WORK"
  return $rc
}
# An EXIT trap's return status does not become the script's; only an `exit` inside it does.
trap 'cleanup; exit $?' EXIT
trap 'cleanup; exit 130' INT TERM

# A fixture HOME: empty, and OUTSIDE the operator's own. The app under test must not read
# or write anything of his (README.md's activation invariant D), and a first run on an
# empty home is the shape a customer's first run has.
FIXTURE="$WORK/home"
mkdir -p "$FIXTURE"

# Compile before booting the guest. run-suite reuses these exact executables and attaches
# to our held guest instead of taking another slot or cloning another machine.
if [ "$SCREEN_SUITES" = 1 ]; then
  GUI_APP_DIR="$(cd "$HERE/.." && pwd)"; export GUI_APP_DIR
  . "$HERE/lib/gui-launch.sh"
  gui_prebuild "$WORK/prebuilt"; PREBUILD_RC=$?
  [ "$PREBUILD_RC" = 0 ] || exit "$PREBUILD_RC"
  export RICHOS_GUI_PREBUILT="$WORK/prebuilt"
  GUI_COMMIT="$(git -C "$GUI_APP_DIR" rev-parse HEAD)" || die "cannot identify gui-boot source"
fi

echo "gui-proof-in-vm.sh: booting ${TAG:-the bundle} in guest '$VM'"
echo "  bundle : $BUNDLE"
echo "  commit : $COMMIT"
echo "  nothing here opens a window on this Mac; the display is the guest's."
echo ""

RESULT="fail:no-window"
WINDOWS=0
FRONT_RC=not-run; GUI_RC=not-run
STOPPED=0
# --no-tailnet: this proof is "does the app draw a window", nothing to do with the phone; a
# failed network join must not fail it (run.sh exits 1 on one, and says so at its :36-41).
if "$TESTVM/run.sh" --bundle "$BUNDLE" --home "$FIXTURE" --vm "$VM" --no-tailnet > "$WORK/run.out" 2>&1; then
  # `run.sh` prints `windows=N`, and N is the point: a pid is not a proof, because the app
  # can be running and drawing nothing — which is precisely the state a screenshot would
  # photograph and call a pass (run.sh's own words).
  WINDOWS="$(sed -n 's/.*windows=\([0-9][0-9]*\).*/\1/p' "$WORK/run.out" | tail -1)"
  [ -n "$WINDOWS" ] || WINDOWS=0
  if [ "$WINDOWS" -ge 1 ]; then
    RESULT="pass"
  fi
else
  RESULT="fail:run.sh"
fi
cat "$WORK/run.out"

if [ "$RESULT" = pass ] && [ "$SCREEN_SUITES" = 1 ]; then
  RESULTS="$OUT.suites"
  # This is the pid returned by this run.sh, inside this guest. End only that app before
  # front-door owns the foreground. stop.sh still owns the eventual clone cleanup.
  APP_PID="$(sed -n 's/^vm=.* pid=\([0-9][0-9]*\) .*$/\1/p' "$WORK/run.out" | tail -1)"
  case "$APP_PID" in
    ''|*[!0-9]*) RESULT="fail:boot-pid" ;;
    *)
      if ! "$TESTVM/guest.sh" "$VM" "kill -TERM $APP_PID 2>/dev/null; for i in 1 2 3 4 5; do kill -0 $APP_PID 2>/dev/null || exit 0; sleep 1; done; kill -KILL $APP_PID 2>/dev/null; for i in 1 2 3 4 5; do kill -0 $APP_PID 2>/dev/null || exit 0; sleep 1; done; exit 1"; then
        RESULT="fail:boot-stop"
      fi ;;
  esac
  if [ "$RESULT" = pass ]; then
    mkdir -p "$RESULTS" || die "cannot create screen results at $RESULTS"
    # Remove only these previous verdicts. A failed transfer must not read an old pass.
    rm -f "$RESULTS/front-door.rc" "$RESULTS/gui-boot.test.sh.rc"
    RICHOS_TEST_RESULTS_DIR="$RESULTS" "$TESTVM/run-suite.sh" "$VM" "$HERE/gui-boot.test.sh" \
      --existing-guest "$VM" --release "$CAND_DIR" 2>&1 | tee "$WORK/suites.out"
    SUITE_PIPE=("${PIPESTATUS[@]}")
    SUITES_RC=${SUITE_PIPE[0]}
    [ "${SUITE_PIPE[1]}" = 0 ] || SUITES_RC=2
    FRONT_RC="$(cat "$RESULTS/front-door.rc" 2>/dev/null || echo not-run)"
    GUI_RC="$(cat "$RESULTS/gui-boot.test.sh.rc" 2>/dev/null || echo not-run)"
    if [ "$SUITES_RC" != 0 ] || [ "$FRONT_RC" != 0 ] || [ "$GUI_RC" != 0 ]; then
      RESULT="fail:screen-suites"
    fi
  fi
fi

{
  echo "richos-gui-proof 1"
  echo "suite=shipped-bundle-boot"
  echo "commit=$COMMIT"
  echo "where=vm:$VM"
  echo "result=$RESULT"
  echo "windows=$WINDOWS"
  if [ "$SCREEN_SUITES" = 1 ]; then
    echo "front-door_exit=$FRONT_RC"
    echo "gui-boot_exit=$GUI_RC"
    echo "gui-boot_commit=$GUI_COMMIT"
  fi
  echo "bundle=$(basename "$BUNDLE")"
  echo "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "--- output ---"
  cat "$WORK/run.out" 2>/dev/null
  cat "$WORK/suites.out" 2>/dev/null || true
} > "$OUT"

echo ""
if [ "$RESULT" = "pass" ]; then
  echo "  PASS  the candidate's own signed bundle started in a clean guest and drew $WINDOWS window(s)"
  [ "$SCREEN_SUITES" = 0 ] || echo "  PASS  front-door --release and gui-boot both passed in that same guest; results: $RESULTS"
  echo "  proof: $OUT"
  echo ""
  echo "  nightly-local.py publish --run ${RUN_ID:-<run-id>} --gui-proof $OUT"
  exit 0
fi
echo "  FAIL  the candidate's guest proof did not complete ($RESULT)"
echo "        The proof records the failure rather than deleting it: $OUT"
echo "        publish will refuse it. Read the recorded boot and screen-suite output."
exit 1

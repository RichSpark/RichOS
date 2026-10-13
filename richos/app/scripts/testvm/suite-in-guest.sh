#!/bin/bash
#
# suite-in-guest.sh — the guest's half of run-suite.sh. It runs INSIDE a test-VM guest, from
#                     the payload run-suite.sh pushed there, and never on this Mac.
#
#   bash "$HOME/run-suite/tree/richos/app/scripts/testvm/suite-in-guest.sh" <suite file name>
#
# THE PAYLOAD, as suite-walk.sh lays it out under the guest user's $HOME/run-suite:
#
#   prebuilt/                 richos-tauri and gui_boot_machine, built on the host
#   tree/richos/app/...       the scripts directory and the source files the suite reads
#   engine/                   the engine the suite provisions its machines from
#   runtime/                  the delivered runtime, when the engine carries none
#   results/                  the suite's results folder (RICHOS_TEST_RESULTS_DIR)
#   suite.rc                  written here: the suite's exit code, and nothing else
#
# WHY IT RE-ENTERS ITSELF IN THE GUI SESSION. An ssh login is its own bootstrap and security
# session, not the Aqua session the console user is logged into, and a GUI process started
# from it may find no window server. `launchctl asuser <uid>` puts the suite in the same
# context `open -a` launches the app into; keychain.sh does the same for the same reason.
# `sudo -n`: the guest image's passwordless sudo is required, and a guest without it refuses
# at once instead of waiting for a password nobody will type.
#
# THE ENVIRONMENT IS STATED, NOT INHERITED. sudo resets it, and the host's never reaches a
# guest, so everything the suite reads is set below and nothing else is: PATH is the
# delivered runtime's bin first and the system directories after it, and RICHOS_GUI_PREBUILT
# puts the suite in PREBUILT MODE (lib/gui-launch.sh).
#
# THE RUNTIME COMES FIRST, MEASURED (2026-09-30, guest walk-16770d5c625a): a clean guest's
# only `node` is the delivered one, and its /usr/bin/python3 is the Command Line Tools' 3.9,
# on which verify-runtime.py stops at `hashlib.file_digest` (3.11 and later). The delivered
# runtime is the python 3.13, node, git and jq this product ships, so it is also the one the
# suite should meet.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
# testvm -> scripts -> app -> richos -> tree -> the payload root
ROOT="$(cd "$HERE/../../../../.." && pwd -P)"

if [ "${1:-}" != "--in-session" ]; then
  SUITE="${1:-}"
  [ -n "$SUITE" ] || { echo "suite-in-guest.sh: usage: suite-in-guest.sh <suite file name>" >&2; exit 64; }
  rm -f "$ROOT/suite.rc"
  exec sudo -n launchctl asuser "$(id -u)" sudo -n -u "$(id -un)" /bin/bash "$0" --in-session "$SUITE"
fi

SUITE="${2:-}"
SUITE_FILE="$ROOT/tree/richos/app/scripts/$SUITE"
[ -f "$SUITE_FILE" ] || { echo "suite-in-guest.sh: no suite at $SUITE_FILE in the payload" >&2; exit 64; }

# The payload root is $HOME/run-suite (suite-walk.sh), so HOME is its parent.
HOME="$(dirname "$ROOT")"
USER="$(id -un)"
TMPDIR="$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || echo /tmp)"
PATH="$ROOT/bin:$ROOT/engine/runtime/bin:$ROOT/runtime/bin:/usr/bin:/bin:/usr/sbin:/sbin"
RICHOS_GUI_PREBUILT="$ROOT/prebuilt"
RICHOS_GUI_ENGINE_SOURCE="$ROOT/engine"
RICHOS_TEST_RESULTS_DIR="$ROOT/results"
export HOME USER TMPDIR PATH RICHOS_GUI_PREBUILT RICHOS_GUI_ENGINE_SOURCE RICHOS_TEST_RESULTS_DIR
if [ -d "$ROOT/runtime" ]; then
  RICHOS_RUNTIME_DIR="$ROOT/runtime"
  export RICHOS_RUNTIME_DIR
fi
# The speech models suite-walk.sh pushed, if any (gui-boot's healthy machine needs both),
# joined by ":" as RICHOS_GUI_SPEECH_MODELS.
for model in "$ROOT"/speech-model/ggml-*.bin; do
  if [ -f "$model" ]; then
    RICHOS_GUI_SPEECH_MODELS="${RICHOS_GUI_SPEECH_MODELS:+$RICHOS_GUI_SPEECH_MODELS:}$model"
    export RICHOS_GUI_SPEECH_MODELS
  fi
done
mkdir -p "$RICHOS_TEST_RESULTS_DIR" || exit 2

echo "  [guest $(hostname -s) as $USER, GUI session] $SUITE in prebuilt mode"
front_rc=0
if [ -d "$ROOT/release" ]; then
  bash "$ROOT/tree/richos/app/scripts/front-door.test.sh" --release "$ROOT/release" \
    --evidence "$ROOT/results/front-door" 2>&1 | tee "$ROOT/results/front-door.log"
  statuses=("${PIPESTATUS[@]}")
  front_rc=${statuses[0]}
  [ "$front_rc" != 0 ] || [ "${statuses[1]}" = 0 ] || front_rc=2
  echo "$front_rc" > "$ROOT/results/front-door.rc" || front_rc=2
fi
bash "$SUITE_FILE" 2>&1 | tee "$ROOT/results/$SUITE.log"
statuses=("${PIPESTATUS[@]}")
rc=${statuses[0]}
[ "$rc" != 0 ] || [ "${statuses[1]}" = 0 ] || rc=2
echo "$rc" > "$ROOT/results/$SUITE.rc" || rc=2
# Keep both verdicts, including refusals. Neither screen suite may turn the other's red green.
[ "$front_rc" -eq 0 ] || rc="$front_rc"
echo "$rc" > "$ROOT/suite.rc" || exit 2
exit "$rc"

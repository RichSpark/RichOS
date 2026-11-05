#!/usr/bin/env bash
# gui-launch-env.test.sh — gui_boot starts the app on ONE home, and nothing else.
#
# gui-boot.test.sh boots the real app under a scratch home. Until 2026-09-24 it set HOME
# alone, and HOME alone is not a scratch home: Foundation answers NSHomeDirectory() from the
# account record, so WebKit's store and the URL cache of a boot landed in the REAL
# ~/Library/WebKit/com.richos.app and ~/Library/Caches/com.richos.app, which his daily driver
# (the same bundle identifier) uses. Measured in a test guest (richos-hq
# docs/verification/2026-09-24-nightly-launcher/, run 1 finding 3).
#
# This suite needs no build and puts nothing on a screen: the "app" is lib/home-probe.sh, a
# stand-in that records its environment and asks Foundation for NSHomeDirectory(), started
# through the real gui_boot. So it runs on any Mac in seconds, and it fails against the
# gui-launch.sh that set HOME alone (GUI_LAUNCH_LIB=<that file> reproduces it).
#
#   E1  the boot's environment is HOME, CFFIXED_USER_HOME, USER and launchd's PATH, and
#       nothing from the calling shell
#   E2  HOME and CFFIXED_USER_HOME are both the machine's home
#   E3  Foundation, inside the boot, answers the machine's home and not the real one
#   E4  the process gui_boot started is gone when it returns
#
# run-tests: inputs richos/app/scripts/gui-boot.test.sh richos/app/scripts/gui-launch-env.test.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/lib/home-probe.sh
# run-tests: covers richos/app/scripts/lib/gui-launch.sh richos/app/scripts/lib/home-probe.sh
# run-tests: no-host-screen: the "app" is lib/home-probe.sh, a shell stand-in that records its environment and exits; nothing is drawn
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

if [ "$(uname -s)" != "Darwin" ]; then
  echo "gui-launch-env.test.sh: NSHomeDirectory() is Foundation's answer; this needs macOS." >&2
  exit 2
fi

# The library is sourced through a variable ON PURPOSE. run-tests.sh classifies a suite that
# sources lib/gui-launch.sh by name as one that boots the app on the host's screen, and holds
# it back under --no-host-screen. This one boots only lib/home-probe.sh, which draws nothing,
# and says so in its `no-host-screen:` declaration above (held to by run-tests.test.sh S6).
LAUNCH_LIB="${GUI_LAUNCH_LIB:-$DIR/lib/gui-launch.sh}"
# shellcheck source=lib/gui-launch.sh
. "$LAUNCH_LIB"

# Canonical, as gui-boot.test.sh makes its own: the product refuses a home that is not its
# own realpath, and macOS's temporary folder sits behind the /var symlink.
TMP="$(cd "$(mktemp -d -t gui-launch-env.XXXXXX)" && pwd -P)"
GUI_LAUNCHED_PIDS="$TMP/launched.pids"
export GUI_LAUNCHED_PIDS
: > "$GUI_LAUNCHED_PIDS"
cleanup() {
  local left
  left="$(gui_reap_all)"
  [ "${left:-0}" -gt 0 ] && echo "  gui-launch-env.test.sh: $left launched process(es) SURVIVED cleanup" >&2
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

MACHINE="$TMP/machine.noindex"
mkdir -p "$MACHINE/Applications/RichOS.app/Contents/MacOS"
cp "$DIR/lib/home-probe.sh" "$MACHINE/Applications/RichOS.app/Contents/MacOS/richos-tauri"
chmod 755 "$MACHINE/Applications/RichOS.app/Contents/MacOS/richos-tauri"

# The calling shell carries things that must not reach the boot.
export GUI_LAUNCH_ENV_CANARY=from-the-shell
export CFFIXED_USER_HOME=/nonexistent/from-the-shell

echo "=== gui-launch-env: gui_boot's home ==="
if gui_boot "$MACHINE" "$TMP/boot.log" 30; then
  RC=0
else
  RC=$?
fi
PROBE="$MACHINE/.home-probe"
if [ "$RC" -ne 0 ] || [ ! -f "$PROBE/env" ]; then
  bad "E0 the stand-in booted through gui_boot" "gui_boot exit $RC; log: $(tr '\n' ' ' < "$TMP/boot.log" | cut -c1-300)"
  echo "=== gui-launch-env.test.sh: $FAIL FAILED, $PASS passed ==="
  exit 1
fi

# bash itself exports PWD, SHLVL and _ into the probe's own children; nothing else is bash's.
KEYS="$(sed -n 's/=.*//p' "$PROBE/env" | grep -vxE 'PWD|SHLVL|_' | sort | tr '\n' ' ')"
WANT="CFFIXED_USER_HOME HOME PATH USER "
if [ "$KEYS" = "$WANT" ] && grep -qx 'PATH=/usr/bin:/bin:/usr/sbin:/sbin' "$PROBE/env" \
   && ! grep -q 'from-the-shell' "$PROBE/env"; then
  ok "E1 the boot's environment is HOME, CFFIXED_USER_HOME, USER and launchd's PATH, and nothing from the shell"
else
  bad "E1 the boot's environment is HOME, CFFIXED_USER_HOME, USER and launchd's PATH" "it carried: $KEYS"
fi

VHOME="$(sed -n 's/^HOME=//p' "$PROBE/env")"
VFIXED="$(sed -n 's/^CFFIXED_USER_HOME=//p' "$PROBE/env")"
if [ "$VHOME" = "$MACHINE" ] && [ "$VFIXED" = "$MACHINE" ]; then
  ok "E2 HOME and CFFIXED_USER_HOME are both the machine's home"
else
  bad "E2 HOME and CFFIXED_USER_HOME are both the machine's home" "HOME=$VHOME CFFIXED_USER_HOME=${VFIXED:-(unset)}"
fi

NSHOME="$(cat "$PROBE/nshome" 2>/dev/null)"
# Foundation hands back the path standardized (/var rather than /private/var), so compare the
# folder it names, not the spelling.
NSHOME_REAL="$( [ -d "$NSHOME" ] && cd "$NSHOME" && pwd -P)"
REAL_HOME="$(dscl . -read "/Users/$(id -un)" NFSHomeDirectory 2>/dev/null | sed 's/^NFSHomeDirectory: //')"
if [ -n "$NSHOME_REAL" ] && [ "$NSHOME_REAL" = "$MACHINE" ]; then
  ok "E3 Foundation inside the boot answers the machine's home, so WebKit's store goes there"
else
  bad "E3 Foundation inside the boot answers the machine's home" \
      "NSHomeDirectory() answered '${NSHOME:-(nothing)}' (the account's real home is ${REAL_HOME:-unknown}); $(head -c 200 "$PROBE/nshome.err" 2>/dev/null)"
fi

STILL=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  kill -0 "$p" 2>/dev/null && STILL=$((STILL + 1))
done < "$GUI_LAUNCHED_PIDS"
if [ "$STILL" -eq 0 ] && [ -s "$GUI_LAUNCHED_PIDS" ]; then
  ok "E4 the process gui_boot started is gone when it returns"
else
  bad "E4 the process gui_boot started is gone when it returns" "$STILL still running"
fi


# The complete-machine fixture must carry the same sidecar as the shipped bundle.
# These are executable stand-ins: only the identity probe runs, never an app or capture.
PREBUILT="$TMP/prebuilt"; ENGINE="$TMP/fixture-engine"
mkdir -p "$PREBUILT" "$ENGINE/loro/bin" "$ENGINE/runtime"
printf '{}\n' > "$ENGINE/runtime/delivery.json"
: > "$ENGINE/loro/bin/loro-context.mjs"; : > "$ENGINE/loro/bin/loro-write.mjs"
cat > "$PREBUILT/richos-tauri" <<'IDENTITY'
#!/bin/sh
[ "$1" = --richos-internal-update-identity ] || exit 64
printf '{"identifier":"com.richos.app","version":"1.3.0"}\n'
IDENTITY
printf '#!/bin/sh\nexit 0\n' > "$PREBUILT/gui_boot_machine"
printf '#!/bin/sh\n# capture companion stand-in, never launched\nexit 0\n' > "$PREBUILT/richos-companion"
chmod 755 "$PREBUILT"/*
export RICHOS_GUI_PREBUILT="$PREBUILT"
if gui_prebuilt_check; then ok "E5 the prebuilt transport accepts every fixture executable"
else bad "E5 the prebuilt transport accepts every fixture executable"; fi
mv "$PREBUILT/richos-companion" "$TMP/companion.saved"
if gui_prebuilt_check > "$TMP/missing-companion.log" 2>&1; then
  bad "E6 a missing capture companion refuses the prebuilt payload"
else
  if grep -q 'richos-companion' "$TMP/missing-companion.log"; then
    ok "E6 a missing capture companion refuses the prebuilt payload by name"
  else bad "E6 a missing capture companion refuses the prebuilt payload by name"; fi
fi
mv "$TMP/companion.saved" "$PREBUILT/richos-companion"
GUI_APP_DIR="$DIR/.."; GUI_ENGINE_DIR="$ENGINE"; GUI_BINARY="$PREBUILT/richos-tauri"
printf 'pinned model stand-in, never opened\n' > "$TMP/model.bin"
export RICHOS_GUI_SPEECH_MODELS="$TMP/model.bin"
if gui_machine "$TMP/complete-machine" > "$TMP/machine.log" 2>&1 \
  && cmp -s "$PREBUILT/richos-companion" "$TMP/complete-machine/Applications/RichOS.app/Contents/MacOS/richos-companion" \
  && [ -x "$TMP/complete-machine/Applications/RichOS.app/Contents/MacOS/richos-companion" ]; then
  ok "E7 the complete-machine app carries the executable companion beside the shell"
else bad "E7 the complete-machine app carries the executable companion beside the shell" "$(tail -5 "$TMP/machine.log")"; fi

# Execute gui-boot's real accounting cases without its product-launch section.
# This runs even when the author gate defers the screen suites to a guest.
if python3 - "$DIR/gui-boot.test.sh" "$TMP/accounting.sh" <<'PYTHON'
from pathlib import Path
import sys
src = Path(sys.argv[1]).read_text()
functions = src[src.index("DECL_ERRORS=0\n"):src.index("# S1 — the prefix")]
cases = src[src.index("healthy_log() {"):src.index('GUI_APP_DIR="$APP_DIR"')]
Path(sys.argv[2]).write_text('set -uo pipefail\nTMP="$1"; PASS=0; FAIL=0\nok() { printf \'  PASS  %s\\n\' "$1"; PASS=$((PASS+1)); }\nbad() { printf \'  FAIL  %s: %s\\n\' "$1" "${2:-}"; FAIL=$((FAIL+1)); }\n' + functions + cases + '\n[ "$FAIL" -eq 0 ]\n')
PYTHON
then
  if bash "$TMP/accounting.sh" "$TMP" > "$TMP/accounting.log" 2>&1; then
    ok "E8 the boot checker accepts healthy meeting startup and rejects missing or failed seams"
  else bad "E8 the boot accounting cases pass without a product launch" "$(cat "$TMP/accounting.log")"; fi
else bad "E8 the boot accounting cases could be extracted"; fi

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== gui-launch-env.test.sh: $FAIL FAILED, $PASS passed ==="
  exit 1
fi
echo "=== gui-launch-env.test.sh: all $PASS passed ==="

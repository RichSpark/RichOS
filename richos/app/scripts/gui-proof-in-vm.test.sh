#!/usr/bin/env bash
# run-tests: no-host-screen: drives gui-proof-in-vm.sh through refusals and stand-in guest controls; no guest is booted and nothing is launched
# run-tests: inputs richos/app/scripts/gui-proof-in-vm.test.sh richos/app/scripts/gui-proof-in-vm.sh richos/app/scripts/testvm/slots.py richos/app/scripts/testvm/reserve.py
# run-tests: covers richos/app/scripts/gui-proof-in-vm.sh
#
# gui-proof-in-vm.test.sh — the gui proof takes one of the test VM's two guest slots for its
# own run, and holds no guest after it (the CEO, 2026-09-27: a slot is held only while a run
# executes; nothing is kept "for later").
#
#   1. --keep, which left a guest running after the proof, is refused before anything runs;
#   2. with both slots executing somebody's run and --wait 0, the proof is refused with exit 75
#      and the reason, before a guest, a scratch directory or a proof file exists;
#   3. --wait reaches the slot admission: with --wait 3 it waits, says so, and is refused only
#      when the wait is spent.
# What a live guest does is gui-proof's own run (docs/testvm.md), not this suite's.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/gui-proof-test.XXXXXX")"
HOLDER=""
cleanup() {
  # Owned: the one holder process this suite started, by the pid captured at its spawn.
  if [ -n "$HOLDER" ]; then kill -TERM "$HOLDER" 2>/dev/null; wait "$HOLDER" 2>/dev/null; fi
  rm -rf "$TMP"
}
trap cleanup EXIT
trap 'cleanup; trap - EXIT; exit 130' INT
trap 'cleanup; trap - EXIT; exit 143' TERM

export TESTVM_ROOT="$TMP/testvm" RICHOS_NIGHTLY_STATE="$TMP/nightly"
unset TESTVM_SLOT TESTVM_SLOTS
fail=0
check() {  # check <name> <0 when it held> [detail]
  if [ "$2" -eq 0 ]; then echo "  ok    $1"; else echo "  FAIL  $1"; [ -n "${3:-}" ] && printf '%s\n' "$3" | sed 's/^/        /'; fail=1; fi
}
has() {  # has <text> <needle>...: 0 when every needle is in the text
  local text="$1"; shift
  for needle in "$@"; do case "$text" in *"$needle"*) ;; *) return 1 ;; esac; done
}
: > "$TMP/bundle.zip"

out="$("$HERE/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --keep 2>&1)"; rc=$?
[ "$rc" -eq 2 ] && has "$out" "--keep is retired"; ok=$?
check "--keep is refused (exit 2) and says a guest is never kept after its run" "$ok" "$out"

# Both slots held, the way two running walks hold them: one flock each, by a process of ours.
mkdir -p "$TESTVM_ROOT"
python3 -c '
import fcntl, json, os, sys, time
held = []
for name in ("guest.lock", "guest-2.lock"):
    f = open(os.path.join(sys.argv[1], name), "a+")
    fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
    f.write(json.dumps({"pid": os.getpid(), "since": time.time(), "purpose": "a walk " + name}) + "\n"); f.flush()
    held.append(f)
open(sys.argv[2], "w").close()
time.sleep(300)
' "$TESTVM_ROOT" "$TMP/held" &
HOLDER=$!
for _ in $(seq 1 100); do [ -e "$TMP/held" ] && break; sleep 0.1; done

out="$("$HERE/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --out "$TMP/proof" --wait 0 2>&1)"; rc=$?
[ "$rc" -eq 75 ] && has "$out" "every slot is executing a run" "a walk guest.lock" "a walk guest-2.lock"; ok=$?
check "both slots executing runs: refused with exit 75, naming both holders" "$ok" "rc=$rc $out"
if [ ! -e "$TMP/proof" ] && [ ! -e "$TESTVM_ROOT/run" ]; then ok=0; else ok=1; fi
check "...before a guest, run state or proof file exists" "$ok"

began=$(date +%s)
out="$("$HERE/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --out "$TMP/proof" --wait 3 2>&1)"; rc=$?
waited=$(( $(date +%s) - began ))
[ "$rc" -eq 75 ] && [ "$waited" -ge 3 ] && has "$out" "slot waiting: every slot is executing a run"; ok=$?
check "--wait 3 waits for a slot, says so, and is refused only when the wait is spent" "$ok" "rc=$rc waited=${waited}s $out"

# The window proof has nothing to do with the phone, so it must never join the tailnet: run.sh
# exits 1 on a failed join, which used to record fail:run.sh and block publishing. A sandbox
# copy of the script runs against stand-ins: a run.sh that joins (through a tailnet.sh that
# always fails) unless it is given --no-tailnet, exactly as the real one does.
SB="$TMP/sandbox"; mkdir -p "$SB/testvm"
cp "$HERE/gui-proof-in-vm.sh" "$SB/gui-proof-in-vm.sh"
cat > "$SB/testvm/tailnet.sh" <<'EOF'
#!/usr/bin/env bash
echo "tailnet.sh $*" >> "$STUB_LOG"; echo "stand-in: join refused" >&2; exit 1
EOF
cat > "$SB/testvm/run.sh" <<'EOF'
#!/usr/bin/env bash
T=1; for a in "$@"; do [ "$a" = "--no-tailnet" ] && T=0; done
if [ "$T" -eq 1 ]; then "$(dirname "$0")/tailnet.sh" join vm || { echo "REFUSED: NOT ON THE TAILNET" >&2; exit 1; }; fi
echo "windows=1"
EOF
# The stop.sh stand-in exits STUB_STOP_RC (0 unless a case below says otherwise).
cat > "$SB/testvm/stop.sh" <<'EOF'
#!/usr/bin/env bash
echo "stand-in stop.sh $*"
exit "${STUB_STOP_RC:-0}"
EOF
chmod +x "$SB"/gui-proof-in-vm.sh "$SB"/testvm/*.sh
export STUB_LOG="$TMP/stub.log"; : > "$STUB_LOG"
out="$(TESTVM_SLOT=1 "$SB/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --out "$TMP/proof2" 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && has "$out" "PASS" "windows=1" && [ ! -s "$STUB_LOG" ]; ok=$?
check "a failing tailnet join cannot fail the window proof: it passes and never asks tailnet.sh to join" "$ok" "rc=$rc log=$(cat "$STUB_LOG") $out"

# A guest that could not be stopped is garbage left on this Mac (CEO ruling §54), so the
# command that made it must not exit 0, even when the window proof itself passed. The
# proof file still records what the run saw.
out="$(TESTVM_SLOT=1 STUB_STOP_RC=1 "$SB/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --out "$TMP/proof3" 2>&1)"; rc=$?
[ "$rc" -ne 0 ] && has "$out" "CLEANUP FAILED" && grep -q '^result=pass$' "$TMP/proof3"; ok=$?
check "a failed guest stop after a passing proof exits nonzero, and the proof still records the pass" "$ok" "rc=$rc $out"

# A run that already failed keeps its own status: the cleanup failure is added, never swapped in.
cat > "$SB/testvm/run.sh" <<'EOF'
#!/usr/bin/env bash
echo "stand-in run.sh: no window"; exit 1
EOF
chmod +x "$SB/testvm/run.sh"
out="$(TESTVM_SLOT=1 STUB_STOP_RC=1 "$SB/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" --commit abc123 --out "$TMP/proof4" 2>&1)"; rc=$?
[ "$rc" -eq 1 ] && has "$out" "CLEANUP FAILED" "fail:run.sh"; ok=$?
check "a failed proof whose guest stop also failed keeps its own exit status (1)" "$ok" "rc=$rc $out"

# The shipped boot and both screen suites use one guest. Failure/refusal in either suite
# must survive a healthy window and a successful cleanup, including missing result copies.
mkdir -p "$SB/lib" "$TMP/bin"
: > "$TMP/RichOS.app.tar.gz"
cat > "$SB/lib/gui-launch.sh" <<'EOF'
gui_prebuild() { echo prebuild >> "$STUB_LOG"; mkdir -p "$1"; }
EOF
cat > "$TMP/bin/cliclick" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$TMP/bin/git" <<'EOF'
#!/bin/bash
echo abc123abc123abc123abc123abc123abc123abcd
EOF
cat > "$SB/testvm/run.sh" <<'EOF'
#!/bin/bash
echo boot >> "$STUB_LOG"
echo "vm=proof-vm ip=guest pid=123 ssh=admin@guest windows=1"
EOF
cat > "$SB/testvm/guest.sh" <<'EOF'
#!/bin/bash
echo "guest $1" >> "$STUB_LOG"
[ "$1" = proof-vm ]
EOF
cat > "$SB/testvm/run-suite.sh" <<'EOF'
#!/bin/bash
echo "suites $*" >> "$STUB_LOG"
[ "$1" = proof-vm ] || exit 64
[ "$3" = --existing-guest ] && [ "$4" = proof-vm ] || exit 64
[ "$5" = --release ] && [ -f "$6/RichOS.app.tar.gz" ] || exit 64
echo "${STUB_FRONT_RC:-0}" > "$RICHOS_TEST_RESULTS_DIR/front-door.rc"
[ "${STUB_LOST_RESULT:-0}" = 1 ] || echo "${STUB_GUI_RC:-0}" > "$RICHOS_TEST_RESULTS_DIR/gui-boot.test.sh.rc"
echo "front=${STUB_FRONT_RC:-0} gui=${STUB_GUI_RC:-0}"
exit 0
EOF
cat > "$SB/testvm/stop.sh" <<'EOF'
#!/bin/bash
echo "stop $1" >> "$STUB_LOG"
exit 0
EOF
chmod +x "$TMP/bin/"* "$SB/testvm/"*.sh
screen_proof() {
  TESTVM_SLOT=1 PATH="$TMP/bin:$PATH" "$SB/gui-proof-in-vm.sh" --bundle "$TMP/bundle.zip" \
    --commit abc123 --screen-suites --vm proof-vm --out "$TMP/screen.proof"
}
: > "$STUB_LOG"
out="$(screen_proof 2>&1)"; rc=$?
[ "$rc" = 0 ] && grep -q '^result=pass$' "$TMP/screen.proof" \
  && grep -q '^front-door_exit=0$' "$TMP/screen.proof" \
  && grep -q '^gui-boot_exit=0$' "$TMP/screen.proof" \
  && [ "$(head -1 "$STUB_LOG")" = prebuild ] \
  && [ "$(grep -c '^boot$' "$STUB_LOG")" = 1 ] \
  && [ "$(grep -c '^stop proof-vm$' "$STUB_LOG")" = 1 ]; ok=$?
check "screen suites run in the boot guest, after prebuild, with one cleanup and two green verdicts" "$ok" "rc=$rc $out"
for refused in 1 2; do
  out="$(STUB_FRONT_RC=$refused screen_proof 2>&1)"; rc=$?
  [ "$rc" = 1 ] && grep -q '^result=fail:screen-suites$' "$TMP/screen.proof" \
    && grep -q "^front-door_exit=$refused$" "$TMP/screen.proof"; ok=$?
  check "front-door exit $refused fails a healthy boot" "$ok" "rc=$rc $out"
  out="$(STUB_GUI_RC=$refused screen_proof 2>&1)"; rc=$?
  [ "$rc" = 1 ] && grep -q '^result=fail:screen-suites$' "$TMP/screen.proof" \
    && grep -q "^gui-boot_exit=$refused$" "$TMP/screen.proof"; ok=$?
  check "gui-boot exit $refused fails a healthy boot" "$ok" "rc=$rc $out"
done
out="$(STUB_LOST_RESULT=1 screen_proof 2>&1)"; rc=$?
[ "$rc" = 1 ] && grep -q '^gui-boot_exit=not-run$' "$TMP/screen.proof"; ok=$?
check "a missing result cannot inherit the preceding run's pass" "$ok" "rc=$rc $out"

if [ "$fail" -eq 0 ]; then echo "gui-proof-in-vm.test.sh: all passed"; else echo "gui-proof-in-vm.test.sh: FAILED"; fi
cleanup; HOLDER=""; trap - EXIT
exit "$fail"

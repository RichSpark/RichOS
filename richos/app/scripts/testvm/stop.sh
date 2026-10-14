#!/usr/bin/env bash
# stop.sh — quit the app, stop the VM, leave NOTHING behind.
#
#   testvm/stop.sh <vm> [--keep-vm]
#
# ===========================================================================
# WHY THIS QUITS BY PID AND NEVER BY KEYSTROKE
# ===========================================================================
# 2026-09-19, 18:10Z: a harness quit an app by sending Command-Q through System
# Events. System Events addresses the FRONTMOST process, the app under test was
# not frontmost, and the keystroke landed on the CEO's Terminal. A quit that
# depends on which window happens to be in front is not a quit, it is a coin
# toss with somebody else's work on the other side.
#
# `kill -TERM <pid>` addresses the process that was actually launched. It
# cannot hit a bystander, and it works whether or not the app has a window,
# whether or not it is frontmost, and whether or not it is responding.
#
# CEO §54: a test instance of the app is garbage, and garbage is always cleaned
# up. If cleanup fails, this script says so LOUDLY and names what to delete by
# hand, rather than exiting 0 and leaving a 25 GB surprise.
# ===========================================================================
set -uo pipefail   # not -e: cleanup must attempt EVERY step even if one fails
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh" 2>/dev/null || { echo "[testvm] cannot source lib.sh"; exit 1; }
set +e

VM="${1:-}"; [ -n "$VM" ] || die "usage: stop.sh <vm> [--keep-vm]"
KEEP_VM=0; [ "${2:-}" = "--keep-vm" ] && KEEP_VM=1
[ "$VM" = "$TESTVM_BASE_VM" ] && die "refusing to delete the base VM (that is the 25 GB template). Use: tart stop $TESTVM_BASE_VM"

STATE="$TESTVM_RUN/$VM"
PROBLEMS=()

# --- 1. stop only the recorded app process ---------------------------------
if vm_running "$VM" 2>/dev/null; then
  PID="$(cat "$STATE/app.pid" 2>/dev/null || true)"
  case "$PID" in
    *[!0-9]*|"") ;;
    *)
      COMMAND="$(guest_ssh "$VM" "ps -p $PID -o comm= 2>/dev/null || true")"
      case "$COMMAND" in
        */richos-tauri)
          log "quitting recorded app pid $PID"
          guest_ssh "$VM" "kill -TERM $PID 2>/dev/null || true"
          for i in $(seq 1 15); do
            guest_ssh "$VM" "kill -0 $PID 2>/dev/null" || break
            sleep 1
          done
          guest_ssh "$VM" "kill -0 $PID 2>/dev/null && kill -KILL $PID 2>/dev/null || true"
          ;;
        *) log "recorded app pid $PID has exited; no process selected by name" ;;
      esac
      ;;
  esac
fi

# --- 1b. leave the tailnet, while there is still a guest to leave it from -----
# ORDER MATTERS, and this is the only place it can go: a signed-in node can
# only sign itself out, and in two lines' time this guest stops existing.
#
# The node is EPHEMERAL, so a guest that dies without ever reaching this line
# still disappears from the tailnet on the control plane's own clock — that is
# what ephemeral means and it is the reason the key is created that way. This
# call makes the disappearance immediate and, more to the point, makes it
# something the harness DID rather than something it assumed. §54 again: the
# node is garbage, and garbage is cleaned up by whoever made it.
if vm_running "$VM" 2>/dev/null; then
  "$HERE/tailnet.sh" logout "$VM" 2>/dev/null \
    || PROBLEMS+=("$VM may still be signed in to the tailnet — check: $HERE/tailnet.sh nodes")
fi

# --- 2. stop the VM -----------------------------------------------------------
if vm_running "$VM" 2>/dev/null; then
  log "stopping VM $VM..."
  tart stop "$VM" 2>/dev/null
  for i in $(seq 1 20); do vm_running "$VM" 2>/dev/null || break; sleep 1; done
  vm_running "$VM" 2>/dev/null && { log "VM did not stop; killing the host process"; tart stop "$VM" --timeout 5 2>/dev/null; }
fi
# The host-side `tart run` process is what actually holds the VM's memory.
# A stopped VM whose runner survives is 7 GB of RAM nobody can see.
#
# ONLY THE RECORDED PID, AND ONLY IF IT IS STILL THIS VM'S RUNNER. A run state
# can outlive its runner (a crashed harness, a reboot), and macOS reuses pids,
# so by the time this reads vm.pid the number may belong to somebody else's
# work. run.sh launched `"$TART_BIN" run "$VM" --no-graphics`,
# so that command line is what the pid must still show; anything else is left
# alone, the same rule the login keeper below follows. If the real runner were
# gone and the VM still up, the vm_running check after this block says so.
VMPID="$(cat "$STATE/vm.pid" 2>/dev/null || true)"
case "$VMPID" in
  *[!0-9]*|"") ;;
  *)
    VMCMD="$(ps -ww -p "$VMPID" -o command= 2>/dev/null || true)"
    case "$VMCMD" in
      "") ;;   # the recorded runner has exited
      *"$TART_BIN run $VM --no-graphics"*)
        # End owned children before the runner. This also covers older recorded
        # wrapper processes without selecting unrelated processes by name.
        log "reaping the host-side VM runner (pid $VMPID and its children)"
        CHILDREN="$(pgrep -P "$VMPID" 2>/dev/null || true)"
        for c in $CHILDREN; do kill -TERM "$c" 2>/dev/null; done
        sleep 2
        for c in $CHILDREN; do kill -0 "$c" 2>/dev/null && kill -KILL "$c" 2>/dev/null; done
        kill -TERM "$VMPID" 2>/dev/null; sleep 1
        kill -0 "$VMPID" 2>/dev/null && kill -KILL "$VMPID" 2>/dev/null
        ;;
      *)
        log "recorded runner pid $VMPID is no longer this VM's runner (it is now: $VMCMD); left alone"
        ;;
    esac
    ;;
esac
vm_running "$VM" 2>/dev/null && PROBLEMS+=("VM $VM is still running")

# --- 2b. end this run's login keeper, by its recorded pid ---------------------
# run.sh started it and wrote its pid. Only that pid, and only if its recorded
# launch is this run's keeper (its command line names `claude-login.sh keep`
# and this VM); a pid reused by anything else is left alone. It would also end
# by itself once the state below is removed.
KEEPPID="$(cat "$STATE/claude-keep.pid" 2>/dev/null || true)"
case "$KEEPPID" in
  *[!0-9]*|"") ;;
  *)
    KEEPCMD="$(ps -p "$KEEPPID" -o command= 2>/dev/null || true)"
    case "$KEEPCMD" in
      *"claude-login.sh keep $VM "*)
        log "ending the login keeper (pid $KEEPPID)"
        kill -TERM "$KEEPPID" 2>/dev/null
        for i in 1 2 3 4 5; do kill -0 "$KEEPPID" 2>/dev/null || break; sleep 1; done
        kill -0 "$KEEPPID" 2>/dev/null && PROBLEMS+=("the login keeper pid $KEEPPID is still running")
        ;;
    esac
    ;;
esac

# --- 3. delete the ephemeral clone -------------------------------------------
if [ "$KEEP_VM" -eq 0 ] && [ -f "$STATE/ephemeral" ]; then
  log "deleting ephemeral clone $VM..."
  tart delete "$VM" 2>/dev/null
  vm_exists "$VM" 2>/dev/null && PROBLEMS+=("could not delete the VM clone '$VM' (tart delete $VM)")
fi
rm -rf "$STATE" 2>/dev/null
[ -e "$STATE" ] && PROBLEMS+=("could not remove run state $STATE")

# --- 4. report ----------------------------------------------------------------
if [ ${#PROBLEMS[@]} -eq 0 ]; then
  log "clean: app quit, VM stopped, clone deleted, state removed."
  exit 0
fi

# §54: "if the clean-up fails ... then Rich must get a MASSIVE ALERT about it".
ALERT="$TESTVM_ROOT/CLEANUP-FAILED-$VM-$(date -u +%Y%m%dT%H%M%SZ).txt"
{
  echo "TESTVM CLEANUP FAILED — $(date -u +%FT%TZ)"
  echo "VM: $VM"
  for p in "${PROBLEMS[@]}"; do echo "  - $p"; done
  echo
  echo "DELETE BY HAND:"
  echo "  export TART_HOME=$TART_HOME"
  echo "  $TART_BIN stop $VM ; $TART_BIN delete $VM"
  echo "  rm -rf $STATE"
} | tee "$ALERT" >&2
cat >&2 <<EOF

########################################################################
#  MASSIVE ALERT: testvm cleanup FAILED and left garbage behind.
#  Rich: delete it by hand using the commands above.
#  Record: $ALERT
########################################################################
EOF
exit 1

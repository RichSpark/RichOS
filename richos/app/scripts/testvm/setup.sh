#!/usr/bin/env bash
# setup.sh — ONE-TIME host setup: install tart, pull the guest image, provision
#            a base VM that can prove things about the app.
#
# Run once per machine. Idempotent: re-running skips whatever is already done,
# so it doubles as a repair tool. Costs a ~25 GB download the first time.
#
#   testvm/setup.sh              full setup
#   testvm/setup.sh --reprovision  re-run guest provisioning on the base VM only
#
# Nothing here opens a window on the host. The base VM boots headless
# (--no-graphics): the guest still has a full virtual display and a real
# WindowServer — the host simply does not render it into a window. That single
# flag is what keeps the CEO's screen clear.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"

REPROVISION_ONLY=0
[ "${1:-}" = "--reprovision" ] && REPROVISION_ONLY=1

mkdir -p "$TESTVM_BIN" "$TESTVM_LOG" "$TESTVM_RUN" "$TART_HOME"

# --- 1. tart, pinned and PROVEN BEFORE IT IS EXECUTED ------------------------
install_tart() {
  log "installing tart $TESTVM_TART_VERSION..."
  local tmp="$TESTVM_ROOT/.tart-download"
  rm -rf "$tmp"; mkdir -p "$tmp"
  # Homebrew is NOT used: the cirruslabs tap's formula calls `depends_on` in a
  # form current Homebrew refuses outright, so `brew install tart` errors before
  # it downloads anything. The official release tarball is the supported path.
  curl -fsSL -o "$tmp/tart.tar.gz" \
    "https://github.com/cirruslabs/tart/releases/download/$TESTVM_TART_VERSION/tart.tar.gz"
  tar -xzf "$tmp/tart.tar.gz" -C "$tmp"
  rm -rf "$TESTVM_BIN/tart.app"
  mv "$tmp/tart.app" "$TESTVM_BIN/tart.app"
  [ -f "$tmp/LICENSE" ] && mv -f "$tmp/LICENSE" "$TESTVM_BIN/tart.LICENSE"
  rm -rf "$tmp"
}

if [ ! -x "$TART_BIN" ]; then
  install_tart
else
  # A pin only means something if it is checked.
  have="$(python3 - "$TESTVM_BIN/tart.app/Contents/Info.plist" <<'PY'
import plistlib,sys
try: print(plistlib.load(open(sys.argv[1],'rb')).get('CFBundleShortVersionString',''))
except Exception: print('')
PY
)"
  [ "$have" = "$TESTVM_TART_VERSION" ] || { log "tart $have != pinned $TESTVM_TART_VERSION; reinstalling"; install_tart; }
fi

# THE gate. Proves the binary's dynamic dependencies resolve without running it.
# Skipping this is exactly how a crash dialog reached the CEO's screen on
# 2026-09-19. It is never optional and it is never after the first execution.
python3 "$HERE/preflight-binary.py" "$TART_BIN" || die "tart failed its launch preflight — refusing to execute it"
log "tart $("$TART_BIN" --version) ready at $TART_BIN"

# --- 2. ssh key ---------------------------------------------------------------
# A dedicated key, never the CEO's. It authenticates to disposable VMs only.
if [ ! -f "$TESTVM_SSH_KEY" ]; then
  log "generating a dedicated ssh key for the guests..."
  ssh-keygen -t ed25519 -N '' -C 'richos-testvm' -f "$TESTVM_SSH_KEY" >/dev/null
fi

# --- 3. the base image --------------------------------------------------------
if [ "$REPROVISION_ONLY" -eq 0 ] && ! vm_exists "$TESTVM_BASE_VM"; then
  log "pulling $TESTVM_IMAGE (~25 GB compressed; this is the slow part, once)"
  tart clone "$TESTVM_IMAGE" "$TESTVM_BASE_VM"
fi
vm_exists "$TESTVM_BASE_VM" || die "base VM $TESTVM_BASE_VM does not exist"

# Record WHAT we actually pulled. ghcr publishes only a `latest` tag for this
# image, so the digest is the only pin with meaning — a rebuild upstream would
# otherwise change the guest silently between one agent's proof and the next.
if [ ! -f "$TESTVM_ROOT/IMAGE_DIGEST" ]; then
  python3 - "$TESTVM_ROOT/IMAGE_DIGEST" <<'PY' || true
import json,subprocess,sys,urllib.request
repo="cirruslabs/macos-sequoia-base"
try:
    tok=json.load(urllib.request.urlopen(
        "https://ghcr.io/token?scope=repository:%s:pull&service=ghcr.io"%repo))["token"]
    req=urllib.request.Request("https://ghcr.io/v2/%s/manifests/latest"%repo,
        headers={"Authorization":"Bearer "+tok,
                 "Accept":"application/vnd.oci.image.manifest.v1+json"})
    r=urllib.request.urlopen(req)
    open(sys.argv[1],"w").write(r.headers.get("docker-content-digest","unknown")+"\n")
except Exception as e:
    open(sys.argv[1],"w").write("unresolved: %s\n"%e)
PY
fi
log "base image digest: $(cat "$TESTVM_ROOT/IMAGE_DIGEST" 2>/dev/null || echo unknown)"

# --- 4. shape the base VM -----------------------------------------------------
# Explicit, never the image's baked defaults: the numbers are justified in lib.sh.
log "configuring base VM: ${TESTVM_CPU} cpu, ${TESTVM_RAM_MB} MB, display $TESTVM_DISPLAY"
tart set "$TESTVM_BASE_VM" --cpu "$TESTVM_CPU" --memory "$TESTVM_RAM_MB" --display "$TESTVM_DISPLAY"

# --- 5. boot it headless and provision ---------------------------------------
boot_base() {
  if vm_running "$TESTVM_BASE_VM"; then log "base VM already running"; return; fi
  log "booting base VM headless (no window on the host)..."
  # Respect the host's power and sleep policy, including during provisioning.
  nohup "$TART_BIN" run "$TESTVM_BASE_VM" --no-graphics \
    > "$TESTVM_LOG/$TESTVM_BASE_VM.log" 2>&1 &
  echo $! > "$TESTVM_RUN/$TESTVM_BASE_VM.pid"
}
mkdir -p "$TESTVM_RUN"
boot_base

log "waiting for the guest to boot and report an IP (up to 180s)..."
IP="$(vm_ip "$TESTVM_BASE_VM")" || die "base VM never reported an IP — see $TESTVM_LOG/$TESTVM_BASE_VM.log"
log "guest IP: $IP"

# Install our key so every later call is passwordless. The image's stock
# admin/admin password is used exactly once, here, and never again.
if ! ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" -o BatchMode=yes \
       "$TESTVM_GUEST_USER@$IP" true 2>/dev/null; then
  log "installing the ssh key into the guest..."
  # /usr/bin/expect, not sshpass: expect ships with macOS, and sshpass is not
  # in Homebrew core (it needs a third-party tap). One fewer dependency on a
  # machine where every install is a thing that can break later.
  #
  # The stock admin/admin password is used HERE ONCE and never again; every
  # later call in this harness authenticates with the dedicated key. It is the
  # image's published default and reaches nothing but a disposable guest.
  expect <<EXPECT_EOF >/dev/null 2>&1
set timeout 60
spawn ssh-copy-id -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i $TESTVM_SSH_KEY.pub $TESTVM_GUEST_USER@$IP
expect {
  -re "assword:" { send "$TESTVM_GUEST_PASS\r"; exp_continue }
  eof
}
EXPECT_EOF
  ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" -o BatchMode=yes \
      "$TESTVM_GUEST_USER@$IP" true 2>/dev/null \
    || die "could not install the ssh key into the guest"
fi
log "passwordless ssh works"

# --- 6. guest provisioning ----------------------------------------------------
log "provisioning the guest (TCC grants, tesseract, tailscale, display pin)..."
scp "${TESTVM_SSH_OPTS[@]}" -O -i "$TESTVM_SSH_KEY" "$HERE/provision-guest.sh" \
  "$TESTVM_GUEST_USER@$IP:/tmp/provision-guest.sh" >/dev/null
PROV_RC=0
ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$IP" \
  "TART_DISPLAY_SIZE=$TESTVM_DISPLAY bash /tmp/provision-guest.sh" || PROV_RC=$?

# Put the host's claude binary in the BASE image — the BINARY only, never a
# credential. The app's spine shells out to `claude`; without it on PATH the app
# reports "no claude install could be found on this Mac".
#
# CHANGED 2026-09-20. This used to be the ONLY place the binary was ever copied,
# and that made the version in the VM a snapshot of whatever the host happened
# to run on the day the image was built — while the host's Claude Code
# auto-updated underneath it. `run.sh` now re-checks and re-copies at EVERY run
# (claude-sync.sh), so this call is no longer what keeps the guest current; it
# seeds the base so the usual per-run answer is "already in sync, nothing to
# copy". Same script, same comparison, one place.
#
# The LOGIN is copied per run too, and never into the base image: it belongs to
# the fixture home, which arrives fresh with every run. See claude-login.sh.
if [ -n "$(host_claude_path 2>/dev/null)" ]; then
  log "seeding the base image with this Mac's claude binary (no credentials)..."
  "$HERE/claude-sync.sh" "$TESTVM_BASE_VM" || {
    log "WARN: the claude binary did not sync into the base image (above)."
    PROV_RC=1
  }
fi

# TCC caches decisions per process; the ssh session that WROTE the grants is
# still running under the old ones. A reboot is the reliable way to make them
# take, and it costs ~30 s on a VM.
log "rebooting the guest so the TCC grants take effect..."
ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$IP" \
  'sudo shutdown -r now' >/dev/null 2>&1 || true
wait_for_guest_reboot "$IP" 30 90 1 || die "guest did not come back after reboot"
IP="$(vm_ip "$TESTVM_BASE_VM")" || die "guest did not come back after reboot"

# --- 7. prove the capture path, HERE, before anyone depends on it -------------
log "verifying screen capture inside the guest..."
if "$HERE/shot.sh" "$TESTVM_BASE_VM" "$TESTVM_LOG/setup-verify.png" >/dev/null 2>&1; then
  log "capture verified: $TESTVM_LOG/setup-verify.png"
else
  log "WARN: capture verification failed — see docs/testvm.md 'black frame' section"
  PROV_RC=1
fi

cat > "$TESTVM_ROOT/MANIFEST" <<EOF
richos testvm host
created:      $(date -u +%FT%TZ)
tart:         $TESTVM_TART_VERSION  ($TART_BIN)
image:        $TESTVM_IMAGE
digest:       $(cat "$TESTVM_ROOT/IMAGE_DIGEST" 2>/dev/null || echo unknown)
base VM:      $TESTVM_BASE_VM
per-VM:       ${TESTVM_CPU} cpu, ${TESTVM_RAM_MB} MB, display $TESTVM_DISPLAY
guest user:   $TESTVM_GUEST_USER
delete it all: rm -rf $TESTVM_ROOT
EOF

if [ "$PROV_RC" -eq 0 ]; then
  log "SETUP COMPLETE. Base VM '$TESTVM_BASE_VM' is provisioned and left RUNNING."
  log "Next: testvm/run-walk.py --bundle <RichOS.app.zip> --home <fixture-home> --engine <engine.tar.gz> --report <run.json> -- <script>"
else
  log "setup finished WITH PROBLEMS (see above)."
fi
exit "$PROV_RC"

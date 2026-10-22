#!/usr/bin/env bash
# lib.sh — shared ground for the testvm harness. Sourced, never run.
#
# ===========================================================================
# WHAT THE HARNESS IS FOR
# ===========================================================================
# 2026-09-19. The CEO, twice within the hour:
#
#   "So, every engineer will keep opening the app making me unable to do
#    anything here or WHAT???"
#   "WHEN THE FUCK WILL THERE BE A PROPER FUCKING SETUP THAT WILL ENABLE
#    SUPER FAST DEVELOPMENT AND NOT PATHETICALLY SLOW SNAIL PACE DEVELOPMENT?"
#
# Until today an on-screen proof of the RichOS app meant a window on HIS Mac,
# taking HIS focus, one agent at a time, serialized behind his working day.
# This harness moves every such proof into a macOS virtual machine on the same
# Mac: the app opens on the guest's virtual display, nothing appears on his
# screen, and two proofs can run at once because they run in two guests.
#
# ===========================================================================
# EVERY DECISION HERE IS DELIBERATE — NO THIRD-PARTY DEFAULT IS TRUSTED
# ===========================================================================
# Each constant below that differs from the tool's own default says why.
# ===========================================================================

set -euo pipefail

# --- Where everything lives. ONE declared place (CEO §54). --------------------
# The engine's scratch-reaper only deletes inside DECLARED scratch roots, and
# this is not one of them — deliberately. The base image costs a 25 GB download
# and must survive every sweep. Ephemeral per-run clones are cleaned by stop.sh
# and swept by this harness's OWN reap.sh, which is where §54 is honored.
TESTVM_ROOT="${TESTVM_ROOT:-$HOME/.richos-testvm}"
TESTVM_BIN="$TESTVM_ROOT/bin"
TESTVM_LOG="$TESTVM_ROOT/log"
TESTVM_RUN="$TESTVM_ROOT/run"
TART_BIN="$TESTVM_BIN/tart.app/Contents/MacOS/tart"

# TART_HOME keeps images and VM disks inside our one declared directory instead
# of tart's default ~/.tart, so "where is the disk space" has one answer and
# "delete it all" is one rm.
export TART_HOME="${TART_HOME:-$TESTVM_ROOT/tart}"

# --- Pins. --------------------------------------------------------------------
# tart 2.33.1, NOT "latest". 2.36.0 and 2.37.0 hard-link
# @rpath/libswiftCompatibilitySpan.dylib — a Swift 6.2 runtime shipping only on
# macOS 26 / Xcode 26. On this macOS 15.6 host they abort in dyld before main
# and macOS puts a crash dialog on the CEO's screen. 2.33.1 weak-links it.
# Re-check this pin when the host is upgraded past macOS 15.
TESTVM_TART_VERSION="${TESTVM_TART_VERSION:-2.33.1}"

# The guest. Sequoia = macOS 15, matching the host's major version, so what the
# app does in the guest is what it does on his Mac. `base` (not `vanilla`)
# because it ships Homebrew and, critically, has SIP DISABLED — which is the
# only reason the TCC grants in provision-guest.sh can be written at all.
# ghcr publishes only a `latest` tag for this image, so the pin that actually
# means something is the digest, recorded at pull time in IMAGE_DIGEST.
TESTVM_IMAGE="${TESTVM_IMAGE:-ghcr.io/cirruslabs/macos-sequoia-base:latest}"
TESTVM_BASE_VM="${TESTVM_BASE_VM:-richos-base}"

# Guest display. The app derives a 1400x880 window and refuses below 1024x700,
# so the screen must clear 1400x950 with room for the menu bar. tart's default
# is 1024x768 — too small, the window would be clipped and every screenshot
# would be a lie. 1680x1050 leaves margin without wasting guest memory.
TESTVM_DISPLAY="${TESTVM_DISPLAY:-1680x1050}"

# Per-VM resources. Host has 10 cores / 24 GB. TWO guests must run at once
# (that is the point), so a guest gets 4 cores and 5 GB.
#
# 5 GB IS MEASURED, NOT GUESSED (2026-09-27, mem-walk.py inside the guest). The
# old 7 GB rested on "below ~6 GB a macOS guest swaps and boots slowly", which
# was never measured, and two 7 GB guests did not fit beside this Mac's usual
# load: at 18:56Z two of them took it from 71% available to 32% and 120 MB/s
# swap-out. A real walk (spelling-walk: first run, then a real model turn) at
# 5120 MB peaked at 3218 MB Memory Used (app 102 MB, claude 698 MB in all),
# with ZERO guest swap-outs and a 70 s boot (68 s at 7 GB). 5120 is that peak
# plus 1.9 GB. At 7 GB the same guest used 3.4-4.1 GB only because macOS
# spreads out when it has room.
# 4096 was the default from 2026-10-14 (a 12-step voice walk passed in it) and is
# NOT ENOUGH for a first run: in the first two minutes after boot a 4096 MB guest
# thrashes (walk-56dbcca781c4: 500k compressions, 353k decompressions, 52k
# pageouts in 129 s), where a 5120 MB guest of the same walk did 181k, 64k and 3k
# in 82 s (walk-7eb7ae8b5a94; esc-20261021T133408Z-5d70a3e4, acked in scope).
# So 5120 again; slots.py still admits a guest only if all of it fits beside the
# running guests with its 4,096 MB floor kept.
# tart's defaults (whatever the image baked) are not trusted.
TESTVM_CPU="${TESTVM_CPU:-4}"
TESTVM_RAM_MB="${TESTVM_RAM_MB:-5120}"

# The guest account the app runs as. It must be the console (auto-login) user:
# a window only exists inside the GUI session, and screencapture over ssh can
# only see the session its user owns.
TESTVM_GUEST_USER="${TESTVM_GUEST_USER:-admin}"
TESTVM_GUEST_PASS="${TESTVM_GUEST_PASS:-admin}"

TESTVM_SSH_KEY="${TESTVM_SSH_KEY:-$TESTVM_ROOT/id_testvm}"

# --- The tailnet. What makes the PHONE path walkable in here. -----------------
# The app refuses to open a pairing code unless `serving_plan` returns Some
# (src-tauri/src/phone/mod.rs:563), which needs a tailnet name, an origin and a
# certificate the control plane will issue. A guest with Tailscale signed out
# has none of the three, so until today every phone proof was a window on the
# CEO's Mac. The auth key below is the one thing an agent cannot produce — the
# account is his — so it lives OUTSIDE every repository, is read and never
# echoed, and its absence refuses ONE step rather than the whole run.
TESTVM_AUTHKEY="${TESTVM_AUTHKEY:-$TESTVM_ROOT/tailscale.authkey}"

# Where a cert issued for a test node is kept between runs, per DNS name.
# Let's Encrypt caps DUPLICATE certificates — the same exact name set — at a
# handful per week, and a fresh ephemeral node asks for its name's certificate
# from an empty cache every single time. Reusing the one already issued is what
# keeps "run it again" from becoming "wait until next week".
# `unverified:` — the cap's exact number is Let's Encrypt's, not measured here;
# what IS measured is that a fresh clone starts with an empty daemon cert store
# (/Library/Tailscale on a guest, probed 2026-09-19).
TESTVM_CERTCACHE="${TESTVM_CERTCACHE:-$TESTVM_ROOT/certs}"

# The guest's Tailscale command line. Homebrew on Apple silicon, which is
# candidate 3 of the four the app itself looks at (phone/tailnet.rs:82). MEASURED
# in a booted guest 2026-09-19: /opt/homebrew/bin/tailscale -> ../Cellar/
# tailscale/1.102.4/bin/tailscale, and `is_file()` follows the symlink, so the
# app finds it. If this ever moves, the app stops finding it too.
TESTVM_GUEST_TS="${TESTVM_GUEST_TS:-/opt/homebrew/bin/tailscale}"

# The same 720h the app asks `tailscale cert` for — phone/mod.rs:119
# (TAILNET_MIN_VALIDITY). Warming the cache with a SHORTER validity than the app
# demands would hand it a cached certificate it then refuses, which is the
# quietest possible way to fail.
TESTVM_MIN_VALIDITY="${TESTVM_MIN_VALIDITY:-720h}"

# How long to wait for the daemon to reach Running. `tailscale up`'s own default
# is 0s, documented as "blocks forever" — in a harness with no terminal that is
# not a wait, it is a hang nobody sees. 90s is generous for a join that normally
# takes ten.
TESTVM_TAILNET_TIMEOUT="${TESTVM_TAILNET_TIMEOUT:-90s}"

# How long to wait for the guest's `tailscaled` to ANSWER AT ALL, before asking
# it to do anything.
#
# THIS IS THE FIX FOR A DEFECT RAY RAISED TWICE AND ECHO REPRODUCED. `run.sh`
# joins the tailnet as soon as ssh answers, which is roughly nine to eleven
# seconds after the guest reports an IP — and the Homebrew launch daemon is not
# up yet. MEASURED, in Ray's own words on nightly .8: *"run.sh attempted the join
# 9 s after the guest reported an IP and got 'failed to connect to local
# tailscaled … /var/run/tailscaled.socket: no such file or directory'"*. Echo saw
# the same sentence on `.9`. Both times a later `tailnet.sh join` worked — in 4 s
# for Ray, 37 s for Echo, which is the spread that tells you a fixed sleep is the
# wrong instrument: whatever number is picked is too long for one machine and too
# short for the other.
#
# So the join waits for a FACT — the CLI getting an answer out of the daemon —
# and these two knobs only bound how long it is prepared to wait for it. 60 × 1 s
# covers Echo's 37 s with room, and is overridable ONLY so the tests can drive
# the never-answers path without spending a minute to prove a refusal.
TESTVM_DAEMON_POLLS="${TESTVM_DAEMON_POLLS:-60}"
TESTVM_DAEMON_POLL_SECONDS="${TESTVM_DAEMON_POLL_SECONDS:-1}"

# ssh options: no host-key prompt or pollution — a guest is disposable and its
# key changes every clone, so StrictHostKeyChecking=no plus a THROWAWAY known
# hosts file keeps the CEO's ~/.ssh/known_hosts untouched.
# BatchMode=yes is not optional: without it, an ssh whose key auth fails falls
# back to asking for a password. An agent has no terminal, so that either hangs
# the harness until somebody notices, or macOS draws an askpass WINDOW — on the
# CEO's screen, which is the one outcome this entire harness exists to prevent.
# Failing fast with an error is always the right answer here.
TESTVM_SSH_OPTS=(
  -o BatchMode=yes
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
  -o LogLevel=ERROR
  -o ConnectTimeout=10
  -o ServerAliveInterval=15
)

log()  { printf '[testvm] %s %s\n' "$(date -u +%H:%M:%SZ)" "$*" >&2; }
die()  { printf '[testvm] ERROR: %s\n' "$*" >&2; exit 1; }

# --- Never execute an unproven binary. ----------------------------------------
# See preflight-binary.py's header: a bare `tart --version` put a crash dialog
# on the CEO's screen on 2026-09-19. Every entry point calls this BEFORE tart.
preflight_tart() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  [ -x "$TART_BIN" ] || die "tart is not installed at $TART_BIN — run testvm/setup.sh"
  python3 "$script_dir/preflight-binary.py" "$TART_BIN" --quiet \
    || die "tart failed its launch preflight (above). Refusing to execute it: running it anyway is what puts a crash dialog on the CEO's screen."
}

tart() { "$TART_BIN" "$@"; }

# Parsed, never grepped. tart pretty-prints its JSON as `"Name" : "value"`, so
# a grep for "Name":"value" silently finds nothing and every caller concludes
# the VM does not exist — which is exactly what happened the first time this
# ran. Source is checked too: an OCI row is the downloaded IMAGE, not a local
# VM, and the two share a name space.
vm_exists() {
  tart list --format json 2>/dev/null | python3 -c "
import json,sys
want=sys.argv[1]
try: rows=json.load(sys.stdin)
except Exception: sys.exit(1)
sys.exit(0 if any(r.get('Name')==want and r.get('Source')=='local' for r in rows) else 1)
" "$1"
}

vm_running() {
  tart list --format json 2>/dev/null \
    | python3 -c "
import json,sys
want=sys.argv[1]
try: rows=json.load(sys.stdin)
except Exception: sys.exit(1)
sys.exit(0 if any(r.get('Name')==want and r.get('Source')=='local' and r.get('Running') for r in rows) else 1)
" "$1"
}

# The guest's IP. tart reports it only once the guest agent is up, which is the
# real signal that the guest has finished booting.
vm_ip() { tart ip "$1" --wait 180 2>/dev/null; }

guest_ssh() {
  local vm="$1"; shift
  local ip
  ip="$(cat "$TESTVM_RUN/$vm/ip" 2>/dev/null || vm_ip "$vm")" || die "no IP for VM $vm"
  ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$ip" "$@"
}

# wait_until_vm_stopped <vm> [polls] [interval-seconds]
# Returns 0 the moment tart no longer reports <vm> running, looking BEFORE any sleep (a base
# that is already stopped costs nothing), 1 when the polls run out.
wait_until_vm_stopped() {
  local vm="$1" polls="${2:-60}" interval="${3:-0.5}" i
  for i in $(seq 1 "$polls"); do
    vm_running "$vm" || return 0
    [ "$i" -lt "$polls" ] && sleep "$interval"
  done
  return 1
}

# wait_for_guest_reboot <ip> [down-polls] [up-polls] [interval-seconds]
# After `shutdown -r now`: first see the guest go DOWN (ssh stops answering), then see it come
# back UP (ssh answers). Replaces a fixed 25 s sleep that cost its full price when the guest was
# back in ten and was too short when it was slow. A guest that never goes down within the first
# polls is reported on stderr and still gets the up check, so a shutdown that did not take
# is not mistaken for a completed reboot silently. Returns 0 up, 1 never came back.
wait_for_guest_reboot() {
  local ip="$1" down_polls="${2:-30}" up_polls="${3:-90}" interval="${4:-1}" i seen_down=0
  for i in $(seq 1 "$down_polls"); do
    if ! ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$ip" true >/dev/null 2>&1; then
      seen_down=1; break
    fi
    [ "$i" -lt "$down_polls" ] && sleep "$interval"
  done
  [ "$seen_down" -eq 1 ] || printf '[testvm] WARN: the guest never stopped answering ssh after the reboot request\n' >&2
  for i in $(seq 1 "$up_polls"); do
    if ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$ip" true >/dev/null 2>&1; then
      return 0
    fi
    [ "$i" -lt "$up_polls" ] && sleep "$interval"
  done
  return 1
}

# wait_for_app_window <vm> <pid> [polls] [interval-seconds]
# A pid is not a proof of a started app: it can be running and drawing nothing, which is what
# a screenshot would photograph and call a pass. Polls the guest for a window of <pid>, checking
# BEFORE any sleep. Sets WINDOWS to the last count read; returns 0 once it is at least 1 and 1
# when the polls run out, so a caller can refuse a windowless boot.
wait_for_app_window() {
  local vm="$1" pid="$2" polls="${3:-45}" interval="${4:-1}" i
  WINDOWS=0
  for i in $(seq 1 "$polls"); do
    WINDOWS="$(guest_ssh "$vm" "osascript -e 'tell application \"System Events\" to count windows of (first process whose unix id is $pid)' 2>/dev/null || echo 0" | tr -d '[:space:]')" || WINDOWS=0
    [ "${WINDOWS:-0}" -gt 0 ] 2>/dev/null && return 0
    [ "$i" -lt "$polls" ] && sleep "$interval"
  done
  return 1
}

guest_scp() {
  local vm="$1"; shift
  local ip
  ip="$(cat "$TESTVM_RUN/$vm/ip" 2>/dev/null || vm_ip "$vm")" || die "no IP for VM $vm"
  # -O forces the legacy SCP protocol: macOS guests ship an sftp-server that
  # some images leave disabled, and the modern default silently fails there.
  scp "${TESTVM_SSH_OPTS[@]}" -O -i "$TESTVM_SSH_KEY" -r "$@" "$TESTVM_GUEST_USER@$ip:"
}

require_vm_running() {
  vm_exists "$1" || die "no such VM: $1"
  vm_running "$1" || die "VM $1 is not running — start it with testvm/run.sh"
}

# ============================================================================
# THE TAILNET — pure helpers, so the decisions can be tested without a tailnet
# ============================================================================

# The node name a VM joins the tailnet under, or refusal.
#
# REFUSES rather than sanitizes. Tailscale would happily mangle an unusable
# hostname into something legal and different, and a node whose name does not
# match its VM is a node reap.sh cannot recognize as ours — the exact garbage
# §54 forbids. The `richos-test-` prefix is load-bearing for the same reason:
# it is reap.sh's WALL 2, and now the tailnet's marker too.
tailnet_node_name() {
  local vm="${1:-}" name
  [ -n "$vm" ] || return 1
  # LOWERCASED FIRST, THEN PREFIXED, and the order is the whole point: a VM
  # called `RichOS-Test-C` is the already-prefixed case, and prefixing before
  # folding the case would have made it `richos-test-richos-test-c` — a second
  # node, under a name nothing else in the harness would recognize. Found by
  # the test of the same name, which is why it is a test and not a comment.
  name="$(printf '%s' "$vm" | tr '[:upper:]' '[:lower:]')"
  case "$name" in
    richos-test-*) ;;
    *)             name="richos-test-$name" ;;
  esac
  case "$name" in *[!a-z0-9-]*) return 1 ;; esac
  case "$name" in -*|*-)        return 1 ;; esac
  [ "${#name}" -le 63 ] || return 1   # DNS label ceiling
  printf '%s\n' "$name"
}

# Never let a key reach a log, a terminal or a commit. Used on EVERY stream that
# came back from a command that was handed one.
redact_keys() {
  sed -E 's/tskey-[A-Za-z0-9_-]+/tskey-<redacted>/g'
}

# What is wrong with the auth-key file, in the CEO's terms — or nothing.
#
# Prints ONE action he can take, and exits nonzero. It never prints the file's
# contents, not even a prefix: a key fragment in a transcript is a key in a
# transcript.
authkey_problem() {
  local path="${1:-$TESTVM_AUTHKEY}" mode first lines
  if [ ! -e "$path" ]; then
    echo "there is no Tailscale auth key at $path"
    return 1
  fi
  if [ ! -f "$path" ]; then
    echo "$path is not a regular file"
    return 1
  fi
  mode="$(stat -f '%Lp' "$path" 2>/dev/null || echo '???')"
  case "$mode" in
    600|400) ;;
    *) echo "$path is mode $mode — a key readable by anything else is not a secret (chmod 600 '$path')"; return 1 ;;
  esac
  # `grep -c .` counts NON-EMPTY lines and exits 1 when there are none. The
  # `|| echo 0` that used to be here appended a second line to its own output,
  # so the empty-file case compared "0\n0" against an integer, errored, fell
  # through, and refused the file for the WRONG reason — a key that was simply
  # empty was reported as "not a Tailscale key". Found by the test named
  # "an empty file is refused", which is what a test of a refusal is for.
  lines="$(grep -c . "$path" 2>/dev/null)"
  lines="${lines:-0}"
  if [ "$lines" -eq 0 ]; then
    echo "$path is empty"
    return 1
  fi
  if [ "$lines" -gt 1 ]; then
    echo "$path has $lines non-empty lines — it must hold the key and nothing else"
    return 1
  fi
  first="$(grep -m1 . "$path" 2>/dev/null || true)"
  case "$first" in
    tskey-*) ;;
    *) echo "$path does not start with 'tskey-', so it is not a Tailscale auth key"; return 1 ;;
  esac
  return 0
}

# ---------------------------------------------------------------------------
# the certificate cache
# ---------------------------------------------------------------------------
# WHY ONE EXISTS. Every clone is a fresh node with an empty certificate store,
# so every run would ask the control plane to issue a NEW certificate for the
# same name. Public CAs rate-limit exactly that — the duplicate-certificate
# limit is single digits per week for one name set — and the failure would land
# days later, on somebody else's run, looking like a broken harness. Keeping
# the certificate that was already issued turns "run it again" back into a
# local operation.
#
# IT IS A PRIVATE KEY ON DISK, and that cost is taken deliberately: mode 0600,
# inside the one declared testvm root, never in a repository, for a throwaway
# node reachable only from inside the CEO's own tailnet. reap.sh prunes it the
# moment it stops being usable.
cert_cache_dir() { printf '%s/%s\n' "$TESTVM_CERTCACHE" "$1"; }

# Is this cached certificate usable for this exact name, for as long as the app
# demands? Answered on the HOST, with openssl, BEFORE anything is pushed into a
# guest: a cache that hands over a certificate for the wrong name, or one
# expiring inside the app's own 720h floor, is worse than an empty cache —
# it fails at the phone instead of here.
cert_cache_usable() {
  local name="$1" dir cert hours seconds
  dir="$(cert_cache_dir "$name")"
  cert="$dir/cert.pem"
  [ -s "$cert" ] && [ -s "$dir/key.pem" ] || return 1
  command -v openssl >/dev/null 2>&1 || return 1
  # Read from TESTVM_MIN_VALIDITY so this cannot drift from the app's own
  # TAILNET_MIN_VALIDITY on its own.
  hours="${TESTVM_MIN_VALIDITY%h}"
  case "$hours" in ''|*[!0-9]*) hours=720 ;; esac
  seconds=$(( hours * 3600 ))
  openssl x509 -in "$cert" -noout -checkend "$seconds" >/dev/null 2>&1 || return 1
  # `-checkhost` asks the question a browser asks — the name must be in the
  # certificate's subject alternative names — rather than grepping the text
  # form, which would match a name that merely appears in an issuer field.
  openssl x509 -in "$cert" -noout -checkhost "$name" >/dev/null 2>&1 || return 1
  return 0
}

# Store a PEM bundle (certificate chain + key, concatenated, as `tailscale cert`
# emits them) under the cached name. Split by PEM LABEL rather than by order,
# for the same reason `fetch_cert` does: both streams arrive on one descriptor
# and nothing promises which comes first.
cert_cache_store() {
  local name="$1" pem="$2" dir
  dir="$(cert_cache_dir "$name")"
  mkdir -p "$dir" || return 1
  chmod 700 "$dir" 2>/dev/null
  rm -f "$dir/cert.pem.new" "$dir/key.pem.new"
  printf '%s\n' "$pem" | awk -v certf="$dir/cert.pem.new" -v keyf="$dir/key.pem.new" '
    /^-----BEGIN / { inblock=1; out = ($0 ~ /PRIVATE KEY/) ? keyf : certf }
    inblock        { print >> out }
    /^-----END /   { inblock=0 }
  '
  if [ ! -s "$dir/cert.pem.new" ] || [ ! -s "$dir/key.pem.new" ]; then
    rm -f "$dir/cert.pem.new" "$dir/key.pem.new"
    return 1
  fi
  mv "$dir/cert.pem.new" "$dir/cert.pem" || return 1
  mv "$dir/key.pem.new"  "$dir/key.pem"  || return 1
  chmod 600 "$dir/cert.pem" "$dir/key.pem"
  # Stored, then re-read and validated. A cache entry that cannot pass the
  # check its reader will apply is not a cache entry, it is a trap.
  cert_cache_usable "$name" || { rm -rf "$dir"; return 1; }
  return 0
}

# The node name out of `tailnet.sh join`'s output, or nothing.
#
# A FUNCTION rather than a line of sed inside run.sh, because run.sh's summary
# and tailnet.sh's output are a contract between two files, and a contract with
# no test drifts. `join` prints `tailnet=<name>` on success and
# `tailnet=<name> certificates=off` when the tailnet will not certify the node,
# and the name is wanted in both cases.
tailnet_name_from_join_output() {
  sed -n 's/.*tailnet=\([^ ]*\).*/\1/p' | tail -1
}

# WHY the join did not produce a usable tailnet, in one line, out of the same
# output — the other half of the same contract, and the reason run.sh no longer
# prints a bare `tailnet=not-joined`.
#
# Ray, nightly .8, defect 3: *"A tester who misses that line walks the whole path
# against a Mac that cannot serve."* The line was there; the CAUSE was thirty
# lines further up, in a refusal block on stderr, and the summary the caller
# actually reads said only `not-joined`. A summary that names no cause is a
# summary that gets skimmed.
#
# Three shapes, in the order they are looked for:
#   1. `[testvm] tailnet: NOT JOINED — <why>`   every `refuse_tailnet`
#   2. `tailnet=<name> certificates=off`        joined, but will not be certified
#   3. anything else: the last non-empty line, so an unclassified failure still
#      arrives with its own words rather than with a shrug.
tailnet_cause_from_join_output() {
  local out; out="$(cat)"
  local why
  why="$(printf '%s\n' "$out" | sed -n 's/.*NOT JOINED[[:space:]]*[—-][[:space:]]*\(.*\)/\1/p' | tail -1)"
  if [ -n "$why" ]; then printf '%s\n' "$why"; return 0; fi
  case "$out" in
    *certificates=off*)
      printf '%s\n' "this tailnet will not issue the node a certificate (HTTPS Certificates is off in the admin console)"
      return 0 ;;
  esac
  why="$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | tail -1 | sed 's/^[[:space:]]*//')"
  printf '%s\n' "${why:-the join produced no output at all}"
}

# The Tailscale command line on the HOST, in the same order the app looks
# (phone/tailnet.rs:82) — so "what the harness sees" and "what the app sees"
# cannot drift apart.
host_ts_cli() {
  local candidate
  # One override, and it exists so the tests can drive `nodes` against a
  # recorded status document instead of the CEO's live tailnet.
  if [ -n "${TESTVM_HOST_TS:-}" ]; then
    [ -x "$TESTVM_HOST_TS" ] || return 1
    printf '%s\n' "$TESTVM_HOST_TS"; return 0
  fi
  for candidate in /usr/local/bin/tailscale \
                   /Applications/Tailscale.app/Contents/MacOS/Tailscale \
                   /opt/homebrew/bin/tailscale \
                   /usr/bin/tailscale; do
    [ -f "$candidate" ] && { printf '%s\n' "$candidate"; return 0; }
  done
  command -v tailscale 2>/dev/null && return 0
  return 1
}

# ============================================================================
# THE CLAUDE BINARY — the guest runs the one the CEO runs, at EVERY run
# ============================================================================
# The CEO's question, 2026-09-20: "What happens with the Claude binary in the
# VM? Will it always stay on the same version regardless of any updates?"
#
# Until this section existed the answer was yes, and that was the defect: the
# binary was copied into the base image ONCE, at setup, and `run.sh` never
# looked at it again. The host's Claude Code auto-updates itself (four versions
# landed in ~/.local/share/claude/versions in the two days before this was
# written), so the guest tested the app against a `claude` the CEO no longer
# runs, and nothing in the output said so.
#
# `~/.local/bin/claude` IS A SYMLINK into ~/.local/share/claude/versions/<v>,
# and its target changes at every host update. Resolve it before hashing or
# copying: hashing the link reads 48 bytes of path text, which would compare
# equal forever and be wrong forever.
TESTVM_HOST_CLAUDE="${TESTVM_HOST_CLAUDE:-$HOME/.local/bin/claude}"
TESTVM_GUEST_CLAUDE="${TESTVM_GUEST_CLAUDE:-/Users/$TESTVM_GUEST_USER/.local/bin/claude}"

# The ONE pin that works. Claude Code's own check, read out of the 2.1.277
# binary, in its order:
#
#   if (DISABLE_UPDATES) ...; if (DISABLE_AUTOUPDATER) ...;
#   if (CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC) ...;
#   if (config.autoUpdates === false &&
#       (config.installMethod !== "native" ||
#        config.autoUpdatesProtectedForNative !== true)) ...
#
# The last clause is why this is an env var and not a settings file. The host's
# `claude` is a NATIVE install, so the copy in the guest is one too, and the
# native updater writes `installMethod:"native"` with
# `autoUpdatesProtectedForNative:true` — from that moment `autoUpdates:false`
# in the config is IGNORED. A config pin would look set and do nothing, which
# is the worst shape a pin can have. The env var is checked first and nothing
# inside the process overrides it.
TESTVM_CLAUDE_PIN_VAR="DISABLE_AUTOUPDATER"
TESTVM_CLAUDE_PIN_VALUE="1"

# The host binary, symlinks resolved. Refuses rather than guessing: a harness
# that cannot say which binary it is copying has nothing to compare against.
host_claude_path() {
  [ -e "$TESTVM_HOST_CLAUDE" ] || return 1
  python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$TESTVM_HOST_CLAUDE" 2>/dev/null
}

sha256_file() {
  [ -f "$1" ] || return 1
  shasum -a 256 "$1" 2>/dev/null | awk '{print $1}'
}

# `claude --version` prints `2.1.277 (Claude Code)`; the version is field one.
# The deadline comes from perl's `alarm` (perl ships with macOS; `timeout(1)`
# does not, as the tailnet code already found). A 217 MB single-file binary on
# a cold guest disk is the one call in here that can sit there, and a harness
# that hangs is worse than one that refuses.
claude_version_of() {  # <path-on-this-machine>
  local out
  out="$(env "$TESTVM_CLAUDE_PIN_VAR=$TESTVM_CLAUDE_PIN_VALUE" \
         perl -e 'alarm shift; exec @ARGV' "${TESTVM_CLAUDE_TIMEOUT:-90}" \
         "$1" --version 2>/dev/null)" || return 1
  out="$(printf '%s\n' "$out" | awk 'NF{print $1; exit}')"
  [ -n "$out" ] || return 1
  printf '%s\n' "$out"
}

# The line every run prints. Written in one place so the summary and the
# refusal cannot disagree about what was measured.
# TESTVM_APP_ENV: extra `KEY=VALUE` pairs, space-separated, for the app's FIRST launch, as the
# `--env` arguments `open` takes — for a walk that must start the app with one of its own test
# knobs (command-walk.py's late-approval step: RICHOS_PERMISSION_DEADLINE_MS, which can only
# shorten the 300 s permission deadline). Pure, so it is tested without a virtual machine.
#
# Deliberately narrow: a KEY must be `RICHOS_` and capitals, a VALUE letters, digits and
# `._-`, because the pairs go into a command line built for the guest's shell. The voice
# fixture alone also accepts an absolute path with slash separators. Spaces and shell
# metacharacters remain refused, including for that path.
app_env_args() {  # <space-separated KEY=VALUE pairs>
  local pair out=""
  for pair in ${1:-}; do
    if ! printf '%s' "$pair" | grep -Eq '^RICHOS_[A-Z_]+=[A-Za-z0-9._-]+$|^RICHOS_VOICE_INPUT_WAV=/[A-Za-z0-9._/-]+$'; then
      printf 'TESTVM_APP_ENV entry refused (plain RICHOS_ value or absolute voice fixture path required): %s\n' "$pair" >&2
      return 1
    fi
    out="$out --env $pair"
  done
  printf '%s\n' "${out# }"
}

claude_version_line() {  # <host-version> <guest-version>
  printf 'claude: host %s guest %s\n' "${1:-unknown}" "${2:-unknown}"
}

# The verdict, as a word, from what was measured — pure, so the decision is
# tested without a virtual machine.
#   in-sync   identical bytes, both versions read, and they agree
#   copy      the guest has something else, or nothing: copy the host's in
#   refuse    no host binary, or still not identical, or a version unreadable
claude_sync_verdict() {  # <host-sha> <guest-sha> <host-version> <guest-version>
  local hsha="${1:-}" gsha="${2:-}" hver="${3:-}" gver="${4:-}"
  [ -n "$hsha" ] || { printf 'refuse\n'; return 1; }
  if [ "$hsha" != "$gsha" ]; then printf 'copy\n'; return 0; fi
  if [ -z "$hver" ] || [ -z "$gver" ] || [ "$hver" != "$gver" ]; then
    printf 'refuse\n'; return 1
  fi
  printf 'in-sync\n'; return 0
}

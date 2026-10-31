#!/usr/bin/env bash
# run-tests.sh — the testvm harness, tested without a virtual machine.
#
#   testvm/test/run-tests.sh            run everything
#   testvm/test/run-tests.sh <pattern>  run the tests whose names match
#
# ===========================================================================
# WHY THESE EXIST, AND WHY THEY DO NOT BOOT A GUEST
# ===========================================================================
# At 07cb236b the harness had no tests at all: the only way to find out whether
# a change to it worked was to clone a 25 GB image, boot it, and read a
# screenshot — five minutes per attempt, and the interesting failures (a key
# leaking into a command line, a refusal that names the wrong action, an exit
# code read off the wrong end of a pipe) are invisible in a screenshot anyway.
#
# Everything here runs against a STUB GUEST and a temporary root, in under a
# second, and asserts the things that would otherwise be found by a person:
#
#   * the auth key never appears in a command line, and never in output;
#   * a missing key refuses exactly ONE step, names ONE action, and does not
#     touch the guest at all;
#   * the name is READ BACK from the daemon, never constructed from the name we
#     asked for;
#   * a certificate is only re-used when it is for the right name and outlives
#     the app's own 720h floor;
#   * `stop.sh`'s sign-out actually signs out.
#
# What they deliberately do NOT cover: whether a real tailnet accepts a real
# key. That needs the CEO's account. `tailnet.sh doctor <vm>` answers it in one
# command against a live guest, which is where that question belongs.
# ===========================================================================
set -uo pipefail

HERE="${RUN_TESTS_HERE:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
TESTVM_DIR="$(cd "$HERE/.." && pwd)"
FILTER="${1:-}"

# A named run runs ONLY the named bodies. The filter used to sit in t_done, after the body
# had already executed, so `run-tests.sh "claude login"` still paid for every unrelated
# body (crypto, sleeps, samplers), five times over under the mutation runner, and a failure in
# an unselected body never showed. Here the script re-executes a copy of itself with the
# `t "name"` ... `t_done` blocks whose name does not contain the pattern cut out before
# bash sees them. Setup code between blocks is kept. (Header: "run the tests whose names match".)
if [ -n "$FILTER" ] && [ -z "${RUN_TESTS_FILTERED:-}" ]; then
  export RUN_TESTS_FILTERED=1 RUN_TESTS_HERE="$HERE"
  exec bash <(awk -v pat="$FILTER" '
    /^t "/ { name = $0; sub(/^t "/, "", name); sub(/".*$/, "", name)
             skipping = (index(name, pat) == 0) }
    !skipping { print }
    /^t_done$/ { skipping = 0 }
  ' "${BASH_SOURCE[0]}") "$FILTER"
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/testvm-tests.XXXXXX")"
# §54: this scratch directory goes however this script ends — pass, fail, or
# an interrupt from a keyboard.
#
# A signal ENDS the run. A trap that only cleaned up let bash carry on after a
# TERM, against the directory it had just deleted: on 2026-09-27 the proof
# runner's supervisor sent TERM 0.6 s in, and every later case "failed" on a
# missing file — the claude-login cases among them — which read as a login
# regression when nothing about the login had changed. 130/143 are the codes
# the shell itself would have exited with.
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT
trap 'cleanup; trap - EXIT; exit 130' INT
trap 'cleanup; trap - EXIT; exit 143' TERM

PASS=0; FAIL=0; FAILED_NAMES=()

t() {  # t <name> — begin a test; body follows, ended by `t_done`
  CURRENT="$1"
  CURRENT_FAILED=0
}
t_done() {
  case "$CURRENT" in *"$FILTER"*) ;; *) return 0 ;; esac
  if [ "$CURRENT_FAILED" -eq 0 ]; then
    PASS=$((PASS+1)); printf '  ok   %s\n' "$CURRENT"
  else
    FAIL=$((FAIL+1)); FAILED_NAMES+=("$CURRENT"); printf '  FAIL %s\n' "$CURRENT"
  fi
}
ok()   { if [ "$1" -eq 0 ]; then :; else CURRENT_FAILED=1; printf '       expected success: %s\n' "${2:-}"; fi; }
no()   { if [ "$1" -ne 0 ]; then :; else CURRENT_FAILED=1; printf '       expected failure: %s\n' "${2:-}"; fi; }
eq()   { if [ "$1" = "$2" ]; then :; else CURRENT_FAILED=1; printf '       expected [%s] got [%s]  %s\n' "$2" "$1" "${3:-}"; fi; }
has()  { case "$1" in *"$2"*) ;; *) CURRENT_FAILED=1; printf '       expected to contain [%s]:\n%s\n' "$2" "$1" ;; esac; }
hasnt(){ case "$1" in *"$2"*) CURRENT_FAILED=1; printf '       expected NOT to contain [%s]:\n%s\n' "$2" "$1" ;; *) ;; esac; }

# --- the harness under test, rooted somewhere disposable --------------------
export TESTVM_ROOT="$TMP/root"
export TART_HOME="$TMP/root/tart"
mkdir -p "$TESTVM_ROOT"
export TESTVM_AUTHKEY="$TESTVM_ROOT/tailscale.authkey"
export TESTVM_CERTCACHE="$TESTVM_ROOT/certs"
export TESTVM_GUEST_EXEC="$HERE/stub-guest.sh"
export STUB_GUEST_FS="$TMP/guestfs"
mkdir -p "$STUB_GUEST_FS"

. "$TESTVM_DIR/lib.sh"
set +e

# A key that is shaped like a real one and is not one: hand-made, and its second
# half says so (notarealkeyatall, which every `hasnt` below looks for). It is
# ASSEMBLED at run time from parts, so no line of this file is key-shaped: GitHub's
# secret scanning flagged the old single literal as a Tailscale API key (alert #4,
# 2026-10-01). The value the tests see is unchanged in shape.
FAKE_KEY="$(printf '%s-%s-%s-%s' tskey auth kFakeKeyIdCNTRL notarealkeyatall)"

write_key() {  # write_key <mode> <contents...>
  local mode="$1"; shift
  printf '%s' "$*" > "$TESTVM_AUTHKEY"
  chmod "$mode" "$TESTVM_AUTHKEY"
}

# A real certificate, made here, so the cache rules are tested against x509 and
# not against a string. Two names and two lifetimes: the one the cache should
# accept, and the two it must refuse.
make_cert() {  # make_cert <dir> <cn> <days>
  local dir="$1" cn="$2" days="$3"
  mkdir -p "$dir"
  openssl req -x509 -newkey rsa:2048 -nodes -days "$days" \
    -subj "/CN=$cn" -addext "subjectAltName=DNS:$cn" \
    -keyout "$dir/key.pem" -out "$dir/cert.pem" >/dev/null 2>&1
}

echo "testvm tests  (root: $TESTVM_ROOT)"

# ===========================================================================
# tailnet_node_name — the name a VM joins under
# ===========================================================================
t "node name: a bare VM name gets the richos-test- prefix reap.sh looks for"
  eq "$(tailnet_node_name a)" "richos-test-a"
t_done

t "node name: an already-prefixed VM name is left alone, never doubled"
  eq "$(tailnet_node_name richos-test-b)" "richos-test-b"
t_done

t "node name: uppercase is lowercased, because DNS names are"
  eq "$(tailnet_node_name RichOS-Test-C)" "richos-test-c"
t_done

t "node name: an illegal character is REFUSED, not sanitized into a different node"
  tailnet_node_name "a_b" >/dev/null 2>&1; no $? "underscore"
  tailnet_node_name "a.b" >/dev/null 2>&1; no $? "dot"
  tailnet_node_name "a b" >/dev/null 2>&1; no $? "space"
  tailnet_node_name ""    >/dev/null 2>&1; no $? "empty"
t_done

t "node name: over a DNS label's 63 characters is refused"
  long="$(printf 'x%.0s' $(seq 1 70))"
  tailnet_node_name "$long" >/dev/null 2>&1; no $? "70 characters"
t_done

# ===========================================================================
# redact_keys — nothing that has seen a key prints one
# ===========================================================================
t "redaction: a key in a line of output never survives the filter"
  out="$(printf 'backend error: %s is invalid\n' "$FAKE_KEY" | redact_keys)"
  hasnt "$out" "notarealkeyatall"
  has   "$out" "tskey-<redacted>"
t_done

# ===========================================================================
# authkey_problem — the refusal, and what it says
# ===========================================================================
t "auth key: a missing file names the file and does not pretend otherwise"
  rm -f "$TESTVM_AUTHKEY"
  out="$(authkey_problem)"; no $? "missing key must be a problem"
  has "$out" "$TESTVM_AUTHKEY"
t_done

t "auth key: a world-readable key is refused with the chmod that fixes it"
  write_key 644 "$FAKE_KEY"
  out="$(authkey_problem)"; no $?
  has "$out" "chmod 600"
t_done

t "auth key: an empty file is refused"
  write_key 600 ""
  out="$(authkey_problem)"; no $?
  has "$out" "empty"
t_done

t "auth key: a file with more than the key in it is refused"
  printf '%s\nand a note about it\n' "$FAKE_KEY" > "$TESTVM_AUTHKEY"; chmod 600 "$TESTVM_AUTHKEY"
  out="$(authkey_problem)"; no $?
  has "$out" "non-empty lines"
t_done

t "auth key: something that is not a tailscale key is refused BEFORE it is used"
  write_key 600 "hunter2"
  out="$(authkey_problem)"; no $?
  has "$out" "tskey-"
t_done

t "auth key: no refusal ever prints the key itself"
  for bad in "$FAKE_KEY" "$FAKE_KEY extra" ""; do
    printf '%s' "$bad" > "$TESTVM_AUTHKEY"; chmod 644 "$TESTVM_AUTHKEY"
    out="$(authkey_problem 2>&1)"
    hasnt "$out" "notarealkeyatall"
  done
t_done

t "auth key: a well-formed key at mode 600 is no problem at all"
  write_key 600 "$FAKE_KEY"
  out="$(authkey_problem)"; ok $? "a good key must pass"
  eq "$out" ""
t_done

# ===========================================================================
# the certificate cache — validated against x509, not against a filename
# ===========================================================================
t "cert cache: a certificate for the right name, good for longer than 720h, is used"
  make_cert "$(cert_cache_dir good.example.ts.net)" "good.example.ts.net" 90
  cert_cache_usable "good.example.ts.net"; ok $?
t_done

t "cert cache: a certificate for a DIFFERENT name is refused"
  make_cert "$(cert_cache_dir wrong.example.ts.net)" "somebody-else.example.ts.net" 90
  cert_cache_usable "wrong.example.ts.net"; no $?
t_done

t "cert cache: a certificate expiring inside the app's own 720h floor is refused"
  make_cert "$(cert_cache_dir soon.example.ts.net)" "soon.example.ts.net" 10
  cert_cache_usable "soon.example.ts.net"; no $?
t_done

t "cert cache: a missing key file makes the entry unusable, not half-usable"
  make_cert "$(cert_cache_dir half.example.ts.net)" "half.example.ts.net" 90
  rm -f "$(cert_cache_dir half.example.ts.net)/key.pem"
  cert_cache_usable "half.example.ts.net"; no $?
t_done

t "cert cache: a concatenated PEM is split by LABEL, not by order"
  src="$TMP/src"; make_cert "$src" "split.example.ts.net" 90
  # key FIRST, which is the order nothing promises and everything must survive
  bundle="$(cat "$src/key.pem" "$src/cert.pem")"
  cert_cache_store "split.example.ts.net" "$bundle"; ok $?
  cert_cache_usable "split.example.ts.net"; ok $?
  head -1 "$(cert_cache_dir split.example.ts.net)/cert.pem" | grep -q 'BEGIN CERTIFICATE'; ok $?
t_done

t "cert cache: a bundle with no key is not stored at all"
  src="$TMP/src2"; make_cert "$src" "nokey.example.ts.net" 90
  cert_cache_store "nokey.example.ts.net" "$(cat "$src/cert.pem")"; no $?
  [ -e "$(cert_cache_dir nokey.example.ts.net)/cert.pem" ]; no $? "a rejected bundle must leave nothing behind"
t_done

t "cert cache: the stored key is not readable by anyone else"
  src="$TMP/src3"; make_cert "$src" "mode.example.ts.net" 90
  cert_cache_store "mode.example.ts.net" "$(cat "$src/cert.pem" "$src/key.pem")" >/dev/null
  eq "$(stat -f '%Lp' "$(cert_cache_dir mode.example.ts.net)/key.pem")" "600"
t_done

# ===========================================================================
# tailnet.sh join — the refusal path, which is the path without his key
# ===========================================================================
t "join: with no key it refuses ONE step, names the admin console, and exits nonzero"
  rm -f "$TESTVM_AUTHKEY"
  out="$(env STUB_LOG="$TMP/log.norefuse" "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; no $?
  has "$out" "NOT JOINED"
  has "$out" "login.tailscale.com/admin/settings/keys"
  has "$out" "Ephemeral: ON"
  has "$out" "Everything else in this VM still works"
t_done

t "join: with no key it does not touch the guest at all"
  rm -f "$TESTVM_AUTHKEY"
  : > "$TMP/log.untouched"
  STUB_LOG="$TMP/log.untouched" "$TESTVM_DIR/tailnet.sh" join richos-test-a >/dev/null 2>&1
  eq "$(wc -l < "$TMP/log.untouched" | tr -d ' ')" "0" "the guest must be left alone"
t_done

t "join: an unusable VM name is refused before the key is even read"
  write_key 600 "$FAKE_KEY"
  : > "$TMP/log.badname"
  out="$(env STUB_LOG="$TMP/log.badname" "$TESTVM_DIR/tailnet.sh" join "bad name" 2>&1)"; no $?
  has "$out" "legal tailnet node name"
  eq "$(wc -l < "$TMP/log.badname" | tr -d ' ')" "0"
t_done

# ===========================================================================
# tailnet.sh join — the path with a key, against the stub guest
# ===========================================================================
t "join: the key reaches the guest as a FILE and never as a command argument"
  write_key 600 "$FAKE_KEY"
  : > "$TMP/log.key"
  src="$TMP/certsrc"; make_cert "$src" "richos-test-a.tail1a2b3c.ts.net" 90
  cat "$src/cert.pem" "$src/key.pem" > "$TMP/cert.bundle"
  out="$(env STUB_LOG="$TMP/log.key" STUB_CERT_PEM="$TMP/cert.bundle" "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  log="$(cat "$TMP/log.key")"
  hasnt "$log" "notarealkeyatall"          # not in any command line
  has   "$log" "--auth-key 'file:"          # passed by reference
  hasnt "$out" "notarealkeyatall"          # not in any output
t_done

t "join: the staged key held the real bytes, and is deleted from the guest afterwards"
  write_key 600 "$FAKE_KEY"
  : > "$TMP/log.staged"; rm -f "$TMP/staged.copy"
  STUB_LOG="$TMP/log.staged" STUB_CERT_PEM="$TMP/cert.bundle" STUB_KEEP_STAGED="$TMP/staged.copy" \
    "$TESTVM_DIR/tailnet.sh" join richos-test-a >/dev/null 2>&1
  # ARRIVED: the guest really was handed the key, so "the file is gone" below
  # is not equally true of a key that was never written.
  eq "$(cat "$TMP/staged.copy" 2>/dev/null)" "$FAKE_KEY" "the key must reach the guest intact"
  # AND LEFT.
  has "$(cat "$TMP/log.staged")" "rm -f '/Users/$TESTVM_GUEST_USER/.richos-testvm-authkey'"
  [ -e "$STUB_GUEST_FS/Users/$TESTVM_GUEST_USER/.richos-testvm-authkey" ]; no $? "the key must not survive the join"
t_done

t "join: every flag the guest is given is the one that was argued for"
  write_key 600 "$FAKE_KEY"
  : > "$TMP/log.flags"
  STUB_LOG="$TMP/log.flags" STUB_CERT_PEM="$TMP/cert.bundle" \
    "$TESTVM_DIR/tailnet.sh" join richos-test-a >/dev/null 2>&1
  log="$(cat "$TMP/log.flags")"
  has "$log" "--hostname 'richos-test-a'"
  has "$log" "--operator '$TESTVM_GUEST_USER'"     # so the APP can fetch a cert
  has "$log" "--reset"                             # a known state, not an inherited one
  has "$log" "--accept-dns=true"                   # Safari in the guest resolves the name
  has "$log" "--shields-up=false"                  # the phone's connection is inbound
  has "$log" "--ssh=false"
  has "$log" "--timeout 90s"                       # never `up`'s "blocks forever" default
t_done

t "join: it prints the name the DAEMON reports, not the one it asked for"
  write_key 600 "$FAKE_KEY"
  out="$(env STUB_LOG=/dev/null STUB_CERT_PEM="$TMP/cert.bundle" STUB_DNSNAME="richos-test-a-1.tail1a2b3c.ts.net" "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>/dev/null)"
  eq "$(printf '%s\n' "$out" | tailnet_name_from_join_output)" "richos-test-a-1.tail1a2b3c.ts.net"
t_done

t "join: a collided name is CALLED OUT, because every URL built from it would be wrong"
  write_key 600 "$FAKE_KEY"
  err="$(env STUB_LOG=/dev/null STUB_CERT_PEM="$TMP/cert.bundle" STUB_DNSNAME="richos-test-a-1.tail1a2b3c.ts.net" "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1 >/dev/null)"
  has "$err" "asked for 'richos-test-a'"
t_done

t "join: a certificate that came back is cached under the name the daemon gave"
  rm -rf "$TESTVM_CERTCACHE/richos-test-a.tail1a2b3c.ts.net"
  write_key 600 "$FAKE_KEY"
  STUB_LOG=/dev/null STUB_CERT_PEM="$TMP/cert.bundle" \
    "$TESTVM_DIR/tailnet.sh" join richos-test-a >/dev/null 2>&1
  cert_cache_usable "richos-test-a.tail1a2b3c.ts.net"; ok $? "the cache must hold a usable cert"
t_done

t "join: the second run offers the cached certificate back to the guest"
  write_key 600 "$FAKE_KEY"
  : > "$TMP/log.reuse"
  STUB_LOG="$TMP/log.reuse" STUB_CERT_PEM="$TMP/cert.bundle" \
    "$TESTVM_DIR/tailnet.sh" join richos-test-a >/dev/null 2>&1
  has "$(cat "$TMP/log.reuse")" "certs/richos-test-a.tail1a2b3c.ts.net.crt"
t_done

t "join: a key the tailnet refuses is reported as a key problem, not as a mystery"
  write_key 600 "$FAKE_KEY"
  out="$(env STUB_LOG=/dev/null STUB_MODE=invalid-key "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; no $?
  has "$out" "expired, revoked, or single-use"
  has "$out" "Generate auth key"
t_done

t "join: a tailnet that will not certify the node says so, and says whose switch it is"
  write_key 600 "$FAKE_KEY"
  out="$(env STUB_LOG=/dev/null STUB_MODE=certs-off "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; no $?
  has "$out" "will not issue it a certificate"
  has "$out" "admin/dns"
  has "$out" "certificates=off"
t_done

t "join: a daemon that never reaches Running is a refusal, never a silent pass"
  write_key 600 "$FAKE_KEY"
  # Two polls, no sleep between them: the refusal is what is under test, not
  # the patience in front of it.
  out="$(env STUB_LOG=/dev/null STUB_MODE=never-running TESTVM_JOIN_POLLS=2 TESTVM_JOIN_POLL_SECONDS=0 "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; no $?
  has "$out" "never reached Running"
t_done

# ===========================================================================
# tailnet.sh join — the daemon start-up race (Ray .7 and .8 defect 3, Echo .9)
# ===========================================================================
# The guest Ray and Echo met: booted, ssh answering, `tailscaled` not up yet.
# `STUB_DAEMON_READY_AFTER=3` is that guest — every tailscale call fails with
# the socket error until the third status read, exactly as `up` did for them.
t "daemon: a join issued before tailscaled is up WAITS for it instead of failing"
  write_key 600 "$FAKE_KEY"
  rm -f "$STUB_GUEST_FS/.stub-daemon-reads"
  : > "$TMP/log.late"
  out="$(env STUB_LOG="$TMP/log.late" STUB_CERT_PEM="$TMP/cert.bundle" \
             STUB_DAEMON_READY_AFTER=3 TESTVM_DAEMON_POLL_SECONDS=0 \
             "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; ok $? "a late daemon must not lose the join"
  has "$out" "tailnet=richos-test-a.tail1a2b3c.ts.net"
  has "$out" "answered after 3 read(s)"
t_done

t "daemon: and the join is not issued until it has answered — the ordering, not the patience"
  # THE POINT OF THE WAIT. A version that waited and then joined anyway in the
  # wrong order would pass the test above. This reads the guest's command log and
  # requires every status read that found nothing to come BEFORE the join.
  log="$(cat "$TMP/log.late")"
  first_up="$(printf '%s\n' "$log" | grep -n "tailscale up" | head -1 | cut -d: -f1)"
  reads_before="$(printf '%s\n' "$log" | head -n "$((first_up - 1))" | grep -c "tailscale status")"
  eq "$reads_before" "3" "up must come after the three status reads, not before them"
t_done

t "daemon: the key is never staged in a guest whose daemon never came up"
  write_key 600 "$FAKE_KEY"
  rm -f "$STUB_GUEST_FS/.stub-daemon-reads" "$TMP/staged.never"
  : > "$TMP/log.never"
  out="$(env STUB_LOG="$TMP/log.never" STUB_MODE=daemon-never STUB_KEEP_STAGED="$TMP/staged.never" \
             TESTVM_DAEMON_POLLS=2 TESTVM_DAEMON_POLL_SECONDS=0 \
             "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"; no $? "a dead daemon must be a refusal"
  has  "$out" "NOT JOINED"
  has  "$out" "tailscaled never answered"
  has  "$out" "/var/run/tailscaled.socket"
  hasnt "$(cat "$TMP/log.never")" "--auth-key"
  [ -e "$TMP/staged.never" ]; no $? "a credential must not be handed to a guest that cannot use it"
t_done

t "daemon: the refusal names a cause a summary can print, never a bare not-joined"
  write_key 600 "$FAKE_KEY"
  rm -f "$STUB_GUEST_FS/.stub-daemon-reads"
  out="$(env STUB_LOG=/dev/null STUB_MODE=daemon-never \
             TESTVM_DAEMON_POLLS=2 TESTVM_DAEMON_POLL_SECONDS=0 \
             "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  cause="$(printf '%s\n' "$out" | tailnet_cause_from_join_output)"
  has "$cause" "tailscaled never answered"
  eq  "$(printf '%s\n' "$cause" | wc -l | tr -d ' ')" "1" "a cause is one line, so a summary can carry it"
t_done

t "join: no certificate means a loud warning — the app would refuse to pair"
  write_key 600 "$FAKE_KEY"
  out="$(env STUB_LOG=/dev/null STUB_MODE=no-cert "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  has "$out" "would not hand over a certificate"
t_done

# ===========================================================================
# keychain.sh — the fixture's login keychain (Ray .7 defect 4, .8 defect 5)
# ===========================================================================
GUEST_HOME_UNDER_TEST="/Users/$TESTVM_GUEST_USER/testvm/richos-test-a/home"

t "keychain: a path outside the guest's home is REFUSED, never sanitized"
  # THE ONE THING THIS FILE MUST NEVER DO. `.7` defect 4 was the fixture's
  # Library/Keychains symlinked at the CEO's real keychain directory. A check
  # that tried to normalize a host path into a guest one would be a check that
  # sometimes guesses; this one refuses and says which prefix it wanted.
  : > "$TMP/log.kcbad"
  for bad in "$HOME/Library/Keychains" "/Users/alex/testvm/x/home" "../../../etc" ""; do # machine-path-exempt: isolated test VM account/path; never a customer default
    out="$(env STUB_LOG="$TMP/log.kcbad" "$TESTVM_DIR/keychain.sh" prepare richos-test-a "$bad" 2>&1)"
    no $? "must refuse: $bad"
  done
  eq "$(wc -l < "$TMP/log.kcbad" | tr -d ' ')" "0" "a refused path must not reach the guest at all"
t_done

t "keychain: prepare issues the search list, the default, the no-timeout setting and BOTH unlocks"
  : > "$TMP/log.kc"
  env STUB_LOG="$TMP/log.kc" "$TESTVM_DIR/keychain.sh" prepare richos-test-a "$GUEST_HOME_UNDER_TEST" >/dev/null 2>&1
  log="$(cat "$TMP/log.kc")"
  has "$log" "security create-keychain"
  has "$log" "security list-keychains -d user -s"          # or `security` finds nothing
  has "$log" "security default-keychain -d user -s"        # or the app's default is nothing
  has "$log" "security set-keychain-settings"              # the 300s default would re-lock mid-walk
  hasnt "$log" "set-keychain-settings -t"                  # a timeout is the thing being removed
  has "$log" "launchctl asuser"                            # THE unlock that is load-bearing
  eq "$(printf '%s\n' "$log" | grep -c "unlock-keychain")" "2" "both sessions, ssh and the app's"
t_done

t "keychain: every path it touches is under the guest's payload home"
  log="$(cat "$TMP/log.kc")"
  eq "$(printf '%s\n' "$log" | grep -c "/Users/alex/")" "0" "no host path may appear in any guest command" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$log" "$GUEST_HOME_UNDER_TEST/Library/Keychains/login.keychain-db"
t_done

t "keychain: every security call is bounded, because an unbounded one HANGS"
  # MEASURED 2026-09-20: against a home with no keychain, `security` raises
  # SecurityAgent's Keychain Not Found dialog inside the guest and waits for a
  # click nothing can give it. The first version of check hung for over ten
  # minutes on that case. The supervisor must terminate the whole owned group.
  log="$(cat "$TMP/log.kc")"
  security_calls="$(printf '%s\n' "$log" | grep -c "security ")"
  bounded_calls="$(printf '%s\n' "$log" | grep "security " | grep -c "start_new_session=True")"
  eq "$bounded_calls" "$security_calls" "every security call must carry the process-group supervisor"
  [ "$security_calls" -ge 6 ]; ok $? "and there must be some to bound ($security_calls found)"
t_done

t "keychain: a missing keychain is answered WITHOUT calling security at all"
  # The anti-hang guarantee, as a fact rather than a bound: the case that hangs
  # never reaches the command that hangs.
  : > "$TMP/log.kcabsent"
  out="$(env STUB_LOG="$TMP/log.kcabsent" STUB_KEYCHAIN=absent \
             "$TESTVM_DIR/keychain.sh" check richos-test-a "$GUEST_HOME_UNDER_TEST" 2>&1)"; no $?
  has "$out" "no keychain at that path"
  has "$out" "Keychain Not Found"
  eq "$(grep -c "generic-password" "$TMP/log.kcabsent" | tr -d ' ')" "0" \
     "the probe that hangs must not be issued"
t_done

t "keychain: check answers for the APP's session, not the ssh one"
  # The measurement that decided the design, reproduced here as a contract: the
  # answer that matters is the one from the session `open -n -a` launches into.
  # In a real guest (2026-09-20, zach-kc3) an ssh-only unlock gave
  # gui-session=UNUSABLE while the file existed and was the default — exactly
  # what Ray saw as "it then prompted for its password twice more".
  : > "$TMP/log.kccheck"
  out="$(env STUB_LOG="$TMP/log.kccheck" "$TESTVM_DIR/keychain.sh" check richos-test-a "$GUEST_HOME_UNDER_TEST" 2>&1)"; ok $?
  has "$out" "gui-session=usable"
  has "$(printf '%s\n' "$(grep "generic-password" "$TMP/log.kccheck" | head -1)")" "launchctl asuser"
t_done

t "keychain: the supervisor's own timing lines never reach the verdict"
  # Nightly 59: every fresh guest printed gui-session=UNUSABLE
  # ({"ax_phase":"guest"}probe{...}) for a keychain that answered "probe". The
  # guest-side supervisor writes those lines on stderr, and the answer merged
  # them in. The stub guest above never runs the supervisor, so this case runs
  # the REAL ax-deadline.py on this machine, behind a fake `security` and `sudo`;
  # every other guest command still goes to the stub.
  kcl="$TMP/kc-local"; mkdir -p "$kcl/bin"
  cat > "$kcl/bin/security" <<'SEC'
#!/usr/bin/env bash
case "$1" in
  add-generic-password|delete-generic-password) exit 0 ;;
  find-generic-password) echo probe; exit 0 ;;
  show-keychain-info) echo "Keychain \"$2\" no-timeout" >&2; exit 0 ;;
  default-keychain) printf '    "%s"\n' "$HOME/Library/Keychains/login.keychain-db"; exit 0 ;;
esac
exit 0
SEC
  cat > "$kcl/bin/sudo" <<'SUDO'
#!/usr/bin/env bash
while [ $# -gt 0 ]; do
  case "$1" in
    launchctl) shift 2; case "${1:-}" in sudo|env) ;; *) shift ;; esac ;;
    sudo) shift ;;
    -u) shift 2 ;;
    *) break ;;
  esac
done
exec "$@"
SUDO
  cat > "$kcl/guest" <<GUEST
#!/usr/bin/env bash
vm="\$1"; shift
case "\$*" in
  *start_new_session*) PATH="$kcl/bin:\$PATH" exec bash -c "\$*" ;;
  *) exec "$HERE/stub-guest.sh" "\$vm" "\$@" ;;
esac
GUEST
  chmod +x "$kcl/bin/security" "$kcl/bin/sudo" "$kcl/guest"
  out="$(env STUB_LOG="$TMP/log.kclocal" TESTVM_GUEST_EXEC="$kcl/guest" \
           "$TESTVM_DIR/keychain.sh" check richos-test-a "$GUEST_HOME_UNDER_TEST" 2>/dev/null)"; ok $?
  has   "$out" "gui-session=usable"
  has   "$out" "default=$GUEST_HOME_UNDER_TEST/Library/Keychains/login.keychain-db"
  hasnt "$out" "ax_phase"
  hasnt "$out" "ax_guest_seconds"
t_done

t "keychain: run.sh prepares it after the fixture home lands and before the app launches"
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" "keychain.sh"
  # The ordering, read off the file: the home is copied, the keychain is made,
  # THEN the app is launched with the same HOME. Any other order prepares a
  # keychain the app will never look at.
  home_in="$(printf '%s\n' "$src" | grep -n "copying the fixture home in" | head -1 | cut -d: -f1)"
  kc_at="$(printf '%s\n' "$src" | grep -n 'keychain.sh" prepare' | head -1 | cut -d: -f1)"
  launch="$(printf '%s\n' "$src" | grep -n "launching the app in the guest" | head -1 | cut -d: -f1)"
  [ "$home_in" -lt "$kc_at" ] && [ "$kc_at" -lt "$launch" ]
  ok $? "order must be: fixture home ($home_in) -> keychain ($kc_at) -> launch ($launch)"
t_done

# ===========================================================================
# logout — §54 at the tailnet
# ===========================================================================
t "logout: it signs the guest out and then CHECKS that it is signed out"
  marker="$TMP/logged-out"; rm -f "$marker"
  : > "$TMP/log.logout"
  out="$(env STUB_LOG="$TMP/log.logout" STUB_LOGOUT_MARKER="$marker" "$TESTVM_DIR/tailnet.sh" logout richos-test-a 2>&1)"; ok $?
  has "$(cat "$TMP/log.logout")" "tailscale logout"
  has "$out" "signed out"
t_done

t "logout: a guest that stays signed in is reported as a failure, not as done"
  : > "$TMP/log.stuck"
  out="$(env STUB_LOG="$TMP/log.stuck" "$TESTVM_DIR/tailnet.sh" logout richos-test-a 2>&1)"; no $?
  has "$out" "STILL signed in"
t_done

t "logout: a guest that was never signed in is a no-op, not an error"
  STUB_LOG=/dev/null STUB_MODE=signed-out \
    "$TESTVM_DIR/tailnet.sh" logout richos-test-a >/dev/null 2>&1; ok $?
t_done

# ===========================================================================
# nodes — the host's view, which is the only one that outlives a dead guest
# ===========================================================================
t "nodes: richos-test-* peers are listed, and nobody else's machines are"
  cat > "$TMP/fake-ts" <<'STUB'
#!/usr/bin/env bash
cat <<'JSON'
{"BackendState":"Running","Self":{"DNSName":"mm1.tail1a2b3c.ts.net."},
 "Peer":{"k1":{"HostName":"richos-test-a","DNSName":"richos-test-a.tail1a2b3c.ts.net.","Online":true},
         "k2":{"HostName":"richos-test-b","DNSName":"richos-test-b.tail1a2b3c.ts.net.","Online":false},
         "k3":{"HostName":"test-iphone","DNSName":"test-iphone.tail1a2b3c.ts.net.","Online":true}}}
JSON
STUB
  chmod +x "$TMP/fake-ts"
  out="$(TESTVM_HOST_TS="$TMP/fake-ts" "$TESTVM_DIR/tailnet.sh" nodes 2>&1)"
  has   "$out" "richos-test-a"
  has   "$out" "richos-test-b"
  has   "$out" "offline"
  hasnt "$out" "test-iphone"
t_done

t "nodes: a Mac with no tailscale on it answers calmly instead of failing"
  out="$(TESTVM_HOST_TS="$TMP/nothing-here" "$TESTVM_DIR/tailnet.sh" nodes 2>&1)"; ok $?
  has "$out" "no tailscale command line"
t_done

# ===========================================================================
# the contract between tailnet.sh's output and run.sh's summary
# ===========================================================================
t "summary: run.sh reads the name out of join's output, in both of its shapes"
  eq "$(printf 'tailnet=richos-test-a.tail1a2b3c.ts.net\n' | tailnet_name_from_join_output)" "richos-test-a.tail1a2b3c.ts.net"
  eq "$(printf 'tailnet=richos-test-b.tail1a2b3c.ts.net certificates=off\n' | tailnet_name_from_join_output)" "richos-test-b.tail1a2b3c.ts.net"
  eq "$(printf 'no tailnet here\n' | tailnet_name_from_join_output)" ""
t_done

t "summary: run.sh still accepts --no-tailnet and still requires its real arguments"
  out="$("$TESTVM_DIR/run.sh" --no-tailnet 2>&1)"; no $?
  has "$out" "--bundle is required"
t_done

# EVERY refusal shape, through the real refusals rather than over made-up
# strings: whatever `tailnet.sh` prints, run.sh's summary must be able to say
# WHY in one line. Ray's defect 3 was not that the cause did not exist — it was
# that the line the caller reads said `not-joined` and nothing else.
t "summary: every shape of a failed join yields a one-line cause, never a shrug"
  write_key 600 "$FAKE_KEY"
  rm -f "$STUB_GUEST_FS/.stub-daemon-reads"
  out="$(env STUB_LOG=/dev/null STUB_MODE=invalid-key "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  has "$(printf '%s\n' "$out" | tailnet_cause_from_join_output)" "expired, revoked, or single-use"

  out="$(env STUB_LOG=/dev/null STUB_MODE=certs-off "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  has "$(printf '%s\n' "$out" | tailnet_cause_from_join_output)" "will not issue the node a certificate"

  out="$(env STUB_LOG=/dev/null STUB_MODE=never-running TESTVM_JOIN_POLLS=2 TESTVM_JOIN_POLL_SECONDS=0 \
             "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  has "$(printf '%s\n' "$out" | tailnet_cause_from_join_output)" "never reached Running"

  rm -f "$TESTVM_AUTHKEY"
  out="$("$TESTVM_DIR/tailnet.sh" join richos-test-a 2>&1)"
  has "$(printf '%s\n' "$out" | tailnet_cause_from_join_output)" "$TESTVM_AUTHKEY"

  # And the negative control: nothing at all still produces a sentence rather
  # than an empty line that would print as `cause:` with nothing after it.
  eq "$(printf '' | tailnet_cause_from_join_output)" "the join produced no output at all"
t_done

t "summary: a SUCCESSFUL join is not mistaken for a cause"
  write_key 600 "$FAKE_KEY"
  rm -f "$STUB_GUEST_FS/.stub-daemon-reads"
  out="$(env STUB_LOG=/dev/null STUB_CERT_PEM="$TMP/cert.bundle" "$TESTVM_DIR/tailnet.sh" join richos-test-a 2>/dev/null)"; ok $?
  eq "$(printf '%s\n' "$out" | tailnet_name_from_join_output)" "richos-test-a.tail1a2b3c.ts.net"
t_done

t "summary: run.sh carries the refusal block and the escape hatch, in that order"
  # Read off the script rather than run, because reaching this branch needs a
  # 7 GB guest. What is asserted is what a reader of a failed run would see:
  # the word REFUSED, the cause line, and the flag that makes it cost nothing.
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" "REFUSED: \$VM IS NOT ON THE TAILNET"
  has "$src" "cause:  \$WHY"
  has "$src" "tailnet_cause_from_join_output"
  has "$src" "--no-tailnet"
  hasnt "$src" 'JOIN_OUT="$("$HERE/tailnet.sh" join "$VM" 2>&1)" && true'
t_done

t "window wait: a boot with zero windows FAILS after its polls (finding 40), it is not 'ready'"
  # shellcheck disable=SC2329  # the stub is invoked by wait_for_app_window inside the subshell
  out="$( guest_ssh() { echo "call" >> "$TMP/ww-calls"; echo 0; }
          rm -f "$TMP/ww-calls"
          wait_for_app_window vm 123 3 0; echo "rc=$? windows=$WINDOWS calls=$(wc -l < "$TMP/ww-calls" | tr -d ' ')" )"
  eq "$out" "rc=1 windows=0 calls=3"
  src="$(cat "$TESTVM_DIR/run.sh")"
  # shellcheck disable=SC2016  # the $ names are literal source text
  has "$src" 'if ! wait_for_app_window "$VM" "$PID" 45 1; then'
  hasnt "$src" 'WARNING: the app is running'
t_done

t "window wait: a window that is already there returns at the first look, with no sleep before it"
  start=$SECONDS
  # shellcheck disable=SC2329  # the stub is invoked by wait_for_app_window inside the subshell
  out="$( guest_ssh() { echo 2; }
          wait_for_app_window vm 123 45 30; echo "rc=$? windows=$WINDOWS" )"
  eq "$out" "rc=0 windows=2"
  elapsed=$((SECONDS - start))
  quick=0; [ "$elapsed" -lt 5 ] || quick=1
  ok "$quick" "waited ${elapsed}s instead of returning at once"
t_done

t "readiness waits: the base stop and the reboot are polled, not slept (finding 41)"
  # shellcheck disable=SC2329  # the stub is invoked by wait_until_vm_stopped inside the subshell
  out="$( vm_running() { return 1; }
          start=$SECONDS; wait_until_vm_stopped base 60 30; echo "rc=$? took=$((SECONDS - start))" )"
  eq "$out" "rc=0 took=0"
  # shellcheck disable=SC2329  # the stub is invoked by wait_until_vm_stopped inside the subshell
  out="$( vm_running() { return 0; }
          wait_until_vm_stopped base 3 0; echo "rc=$?" )"
  eq "$out" "rc=1"
  # shellcheck disable=SC2329  # the stub is invoked by wait_for_guest_reboot inside the subshell
  out="$( n=0; ssh() { n=$((n+1)); [ "$n" -ge 3 ] && [ "$n" -lt 5 ] && return 255; [ "$n" -ge 5 ] && return 0; return 0; }
          wait_for_guest_reboot 10.0.0.1 30 90 0 2>&1; echo "rc=$?" )"
  eq "$out" "rc=0"
  # shellcheck disable=SC2329  # the stub is invoked by wait_for_guest_reboot inside the subshell
  out="$( ssh() { return 255; }; wait_for_guest_reboot 10.0.0.1 2 3 0 2>&1; echo "rc=$?" )"
  has "$out" "rc=1"
  src="$(cat "$TESTVM_DIR/setup.sh" "$TESTVM_DIR/run.sh")"
  hasnt "$src" "sleep 25"
  hasnt "$src" "    sleep 3"
t_done

t "summary: the join's exit status survives set -e, so the refusal block is reachable at all"
  # FOUND WHILE WRITING THE REFUSAL ABOVE, and it would have been invisible: under
  # `set -e` a plain `VAR=$(failing-cmd)` KILLS THE SCRIPT at that line, before any
  # `$?` can be read. run.sh would have exited silently on exactly the failure it
  # was being taught to shout about. The compound form is the fix, and the two
  # probes below are the measurement rather than the belief.
  src="$(cat "$TESTVM_DIR/run.sh")"
  has   "$src" 'JOIN_RC=0 || JOIN_RC=$?'
  hasnt "$src" 'join "$VM" 2>&1)"; JOIN_RC=$?'
  eq "$(bash -c 'set -euo pipefail; X="$(exit 7)" && RC=0 || RC=$?; echo "rc=$RC"' 2>/dev/null)" "rc=7"
  bash -c 'set -euo pipefail; X="$(exit 7)"; RC=$?; echo "rc=$RC"' >/dev/null 2>&1
  no $? "the plain form must die under set -e — that is the whole reason for the compound one"
t_done


# ===========================================================================
# claude-sync.sh — the guest runs the binary this Mac runs, at EVERY run
# ===========================================================================
# The defect these are about is not a crash: it is a harness that keeps
# working while quietly testing last week's `claude`. So every one of these
# asserts a COMPARISON and what was printed, never an exit code alone.

# A fake host binary: a shell script that answers --version like the real one.
# Its sha256 is a real sha256 of a real file, which is what the sync compares.
make_host_claude() {  # make_host_claude <version>
  mkdir -p "$TMP/hostbin"
  printf '#!/bin/sh\necho "%s (Claude Code)"\n' "$1" > "$TMP/hostbin/claude"
  chmod 755 "$TMP/hostbin/claude"
}
# The copy, stubbed: it records that it was called and moves the guest's state
# to the copied file's sha and version, so the re-measure after a copy is the
# real thing rather than a canned answer.
cat > "$TMP/guest-put.sh" <<'PUT'
#!/usr/bin/env bash
printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "${PUT_LOG:-/dev/null}"
[ -n "${PUT_REFUSES:-}" ] && exit 1
shasum -a 256 "$2" | awk '{print $1}' > "$STUB_GUEST_FS/claude-state.sha"
"$2" --version 2>/dev/null | awk '{print $1}' > "$STUB_GUEST_FS/claude-state.version"
exit 0
PUT
chmod 755 "$TMP/guest-put.sh"
sync_env() {  # the fixed half of every claude-sync invocation
  printf '%s\n' \
    "TESTVM_HOST_CLAUDE=$TMP/hostbin/claude" \
    "TESTVM_GUEST_PUT=$TMP/guest-put.sh"
}
reset_guest_claude() { rm -f "$STUB_GUEST_FS/claude-state.sha" "$STUB_GUEST_FS/claude-state.version"; }

t "app env: TESTVM_APP_ENV becomes --env pairs for the app's own RICHOS_ knobs, and nothing else passes"
  eq "$(app_env_args "")" ""
  eq "$(app_env_args "RICHOS_PERMISSION_DEADLINE_MS=20000")" "--env RICHOS_PERMISSION_DEADLINE_MS=20000"
  eq "$(app_env_args "RICHOS_A=1 RICHOS_B_C=x.y-z")" "--env RICHOS_A=1 --env RICHOS_B_C=x.y-z"
  eq "$(app_env_args "RICHOS_VOICE_INPUT_WAV=/Users/admin/testvm/ship-tomorrow.wav")" "--env RICHOS_VOICE_INPUT_WAV=/Users/admin/testvm/ship-tomorrow.wav" # machine-path-exempt: isolated test VM account/path; never a customer default
  # Literal $(...) and backticks are the refusal inputs under test.
  # shellcheck disable=SC2016
  for refused in 'RICHOS_OTHER=/Users/admin/input.wav' 'RICHOS_VOICE_INPUT_WAV=/a;b' 'RICHOS_VOICE_INPUT_WAV=/a$(id)' 'RICHOS_VOICE_INPUT_WAV=/a`id`' 'RICHOS_VOICE_INPUT_WAV=/a b' 'RICHOS_VOICE_INPUT_WAV=/a*'; do # machine-path-exempt: isolated test VM account/path; never a customer default
    app_env_args "$refused" >/dev/null 2>&1; no $? "$refused passed"
  done
  for refused in "PATH=/tmp" "RICHOS_X=a;b" "RICHOS_X=\$(id)" "RICHOS_X='a'" "richos_x=1" "RICHOS_X="; do
    app_env_args "$refused" >/dev/null 2>&1; no $? "$refused passed"
  done
  src="$(cat "$TESTVM_DIR/run.sh")"
  # shellcheck disable=SC2016  # the $ names are literal source text
  has "$src" 'APP_EXTRA_ENV="$(app_env_args "${TESTVM_APP_ENV:-}")" || die'
  # shellcheck disable=SC2016  # the $ names are literal source text: only the open line uses it
  has "$src" '$APP_EXTRA_ENV'
t_done

t "claude verdict: identical bytes and equal versions is the only in-sync there is"
  eq "$(claude_sync_verdict abc abc 2.1.277 2.1.277)" "in-sync"
t_done

t "claude verdict: different bytes means copy, whatever the versions say"
  eq "$(claude_sync_verdict abc def 2.1.277 2.1.277)" "copy"
  eq "$(claude_sync_verdict abc "" "" "")" "copy"
t_done

t "claude verdict: same bytes but a version that cannot be read is REFUSED, not assumed"
  # A guest binary that will not answer --version cannot serve a model turn
  # either. Assuming the host's version here is exactly the lie this file exists
  # to stop being told.
  eq "$(claude_sync_verdict abc abc 2.1.277 "")" "refuse"
  eq "$(claude_sync_verdict abc abc "" "")" "refuse"
t_done

t "claude verdict: no host binary is a refusal, never an in-sync by coincidence"
  eq "$(claude_sync_verdict "" "" "" "")" "refuse"
t_done

t "claude line: always the same shape, and says unknown rather than nothing"
  eq "$(claude_version_line 2.1.277 2.1.277)" "claude: host 2.1.277 guest 2.1.277"
  eq "$(claude_version_line 2.1.277 "")"      "claude: host 2.1.277 guest unknown"
t_done

t "claude sync: a guest already holding the same binary copies NOTHING"
  make_host_claude 2.1.277
  reset_guest_claude
  HSHA="$(shasum -a 256 "$TMP/hostbin/claude" | awk '{print $1}')"
  out="$(env $(sync_env) PUT_LOG="$TMP/put.none" STUB_CLAUDE_SHA="$HSHA" STUB_CLAUDE_VERSION=2.1.277 \
         "$TESTVM_DIR/claude-sync.sh" richos-test-a 2>/dev/null)"; ok $? "in sync must succeed"
  eq "$out" "claude: host 2.1.277 guest 2.1.277"
  [ -s "$TMP/put.none" ]; no $? "nothing should have been copied"
t_done

t "claude sync: a STALE guest binary is replaced, and the new version is the one printed"
  # This is the CEO's question in one test: the host updated, the guest did not,
  # and the guest must not be left on the old build.
  make_host_claude 2.1.277
  reset_guest_claude
  out="$(env $(sync_env) PUT_LOG="$TMP/put.stale" STUB_CLAUDE_SHA=deadbeef STUB_CLAUDE_VERSION=2.1.274 \
         "$TESTVM_DIR/claude-sync.sh" richos-test-a 2>/dev/null)"; ok $? "a copy that lands must succeed"
  eq "$out" "claude: host 2.1.277 guest 2.1.277"
  # The RESOLVED host path, which is the point: ~/.local/bin/claude is a symlink
  # into ~/.local/share/claude/versions/<v> and it is the target that gets copied.
  has "$(cat "$TMP/put.stale")" "hostbin/claude"
  has "$(cat "$TMP/put.stale")" ".local/bin/claude.new"
t_done

t "claude sync: a guest with NO claude gets one"
  make_host_claude 2.1.277
  reset_guest_claude
  out="$(env $(sync_env) PUT_LOG="$TMP/put.empty" STUB_CLAUDE_SHA="" STUB_CLAUDE_VERSION="" \
         "$TESTVM_DIR/claude-sync.sh" richos-test-a 2>/dev/null)"; ok $?
  eq "$out" "claude: host 2.1.277 guest 2.1.277"
t_done

t "claude sync: a copy that does not take is a REFUSAL that names both versions"
  make_host_claude 2.1.277
  reset_guest_claude
  out="$(env $(sync_env) PUT_LOG="$TMP/put.fail" PUT_REFUSES=1 \
         STUB_CLAUDE_SHA=deadbeef STUB_CLAUDE_VERSION=2.1.274 \
         "$TESTVM_DIR/claude-sync.sh" richos-test-a 2>"$TMP/sync.err")"; no $? "a failed copy must not exit 0"
  eq "$out" "claude: host 2.1.277 guest 2.1.274"
  has "$(cat "$TMP/sync.err")" "REFUSED"
t_done

t "claude sync: no host binary refuses BEFORE it touches the guest"
  out="$(env TESTVM_HOST_CLAUDE="$TMP/hostbin/not-installed" STUB_LOG="$TMP/log.nohost" \
         "$TESTVM_DIR/claude-sync.sh" richos-test-a 2>"$TMP/nohost.err")"; no $?
  has "$(cat "$TMP/nohost.err")" "no claude binary on this host"
  [ -s "$TMP/log.nohost" ]; no $? "the guest must not have been asked anything"
t_done

t "claude sync: --check reports and copies nothing, whatever it finds"
  make_host_claude 2.1.277
  reset_guest_claude
  out="$(env $(sync_env) PUT_LOG="$TMP/put.check" STUB_CLAUDE_SHA=deadbeef STUB_CLAUDE_VERSION=2.1.274 \
         "$TESTVM_DIR/claude-sync.sh" --check richos-test-a 2>/dev/null)"; no $? "out of sync must be non-zero"
  eq "$out" "claude: host 2.1.277 guest 2.1.274"
  [ -s "$TMP/put.check" ]; no $? "--check must copy nothing"
t_done

t "claude sync: the auto-updater pin is the ENV VAR, because the config key is ignored for a native install"
  # Read out of the 2.1.277 binary: `autoUpdates:false` is skipped when
  # installMethod==="native" && autoUpdatesProtectedForNative===true — which the
  # native updater sets ITSELF. A config pin would look set and do nothing.
  eq "$TESTVM_CLAUDE_PIN_VAR" "DISABLE_AUTOUPDATER"
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" '--env $TESTVM_CLAUDE_PIN_VAR=$TESTVM_CLAUDE_PIN_VALUE'
t_done

t "claude sync: run.sh prints both versions at EVERY run, and refuses when they differ"
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" 'CLAUDE_RC=0 || CLAUDE_RC=$?'
  has "$src" 'echo "$CLAUDE_LINE"'
  has "$src" "IS NOT RUNNING THE CLAUDE BINARY THIS MAC RUNS"
t_done

# ===========================================================================
# claude-login.sh — the guest is signed in as this Mac is, and the value is
#                   only ever on a pipe
# ===========================================================================
# A `security` for the HOST side that is not the CEO's. The tests never read,
# and never could read, his real keychain item.
FAKE_CRED='{"claudeAiOauth":{"accessToken":"not-a-real-token-0123456789","refreshToken":"also-not-real-9876543210","expiresAt":4102444800000,"refreshTokenExpiresAt":4105044800000},"unrelatedCredential":"also-not-real-other"}'
cat > "$TMP/host-security.sh" <<PLACEHOLDER
#!/usr/bin/env bash
# find-generic-password [-s svc] [-a acct] [-w]
WANT_W=0; SVC=""; PREV=""
for a in "\$@"; do
  [ "\$a" = "-w" ] && WANT_W=1
  [ "\$PREV" = "-s" ] && SVC="\$a"
  PREV="\$a"
done
# WHICH host item was asked for, by name: one line per call, never a value.
[ -n "\${HOST_SECURITY_SERVICES_LOG:-}" ] && printf '%s\n' "\$SVC" >> "\$HOST_SECURITY_SERVICES_LOG"
[ -n "\${HOST_SECURITY_LOGGED_OUT:-}" ] && exit 44
if [ "\$WANT_W" -eq 1 ]; then
  if [ -n "\${HOST_SECURITY_CREDENTIAL_FILE:-}" ]; then
    cat "\$HOST_SECURITY_CREDENTIAL_FILE"
  else
    printf '%s\n' '$FAKE_CRED'
  fi
elif [ -n "\${HOST_SECURITY_MDAT_FILE:-}" ] && [ -f "\$HOST_SECURITY_MDAT_FILE" ]; then
  # ATTRIBUTES, the way the real one prints them without -w: no value here.
  printf '    "mdat"<timedate>=0x00  "%sZ\\\\000"\n' "\$(cat "\$HOST_SECURITY_MDAT_FILE")"
fi
exit 0
PLACEHOLDER
chmod 755 "$TMP/host-security.sh"
# THE HOST RENEWAL, stubbed for the WHOLE suite: no case may start the CEO's
# real `claude`. It records each request; with HOST_RENEW_TO set it plays this
# Mac renewing its login (the credential file and the item's write time change).
cat > "$TMP/host-renew.sh" <<'RENEW'
#!/usr/bin/env bash
# The folder it was asked under, so a case can tell which account renewed.
printf 'renew %s\n' "${CLAUDE_CONFIG_DIR:-}" >> "${HOST_RENEW_LOG:-/dev/null}"
if [ -n "${HOST_RENEW_TO:-}" ] && [ -n "${HOST_SECURITY_CREDENTIAL_FILE:-}" ]; then
  cp "$HOST_RENEW_TO" "$HOST_SECURITY_CREDENTIAL_FILE"
  [ -n "${HOST_SECURITY_MDAT_FILE:-}" ] && date -u +%Y%m%d%H%M%S > "$HOST_SECURITY_MDAT_FILE"
fi
exit 0
RENEW
chmod 755 "$TMP/host-renew.sh"
export TESTVM_HOST_CLAUDE_RENEW="$TMP/host-renew.sh"
# THE APP'S ACCOUNT LIST, isolated for the WHOLE suite: claude-login.sh follows
# the account the app has in use, and no case may read this Mac's real record.
# A path that does not exist is "no list", which is Home, as it always was.
export TESTVM_CLAUDE_ACCOUNTS="$TMP/no-claude-accounts.json"
login_env() {
  printf '%s\n' "TESTVM_HOST_SECURITY=$TMP/host-security.sh"
  printf '%s\n' "TESTVM_HOST_CLAUDE_RENEW=$TMP/host-renew.sh"
}

t "engine payload: untarred with --strip-components 1, and CHECKED afterwards"
  # Ray's vm9 audit: `--engine` had never worked. The release tarball's only
  # top-level member is `engine/`, so the guest got $PAYLOAD/engine/engine/... and
  # the app said "the RichOS engine isn't on this Mac" — which is what .7 and .8
  # both reported. The flag is the fix; the check is the reason it cannot quietly
  # rot again, and it asks the app's own question.
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" "--strip-components 1 -xzf"
  has "$src" "/engine/scripts/hooks"
  has "$src" "/engine/VERSION"
  has "$src" "THE ENGINE PAYLOAD IS NOT AN ENGINE IN THE GUEST"
  hasnt "$src" "tar -C '\$PAYLOAD/engine' -xzf"
t_done

t "claude login: the host being signed out is a refusal that names the one action"
  err="$(env $(login_env) HOST_SECURITY_LOGGED_OUT=1 "$TESTVM_DIR/claude-login.sh" host-check 2>&1)"; no $?
  has "$err" "/login on the host first"
t_done

t "claude login: a signed-in host passes host-check without reading the value"
  env $(login_env) "$TESTVM_DIR/claude-login.sh" host-check >/dev/null 2>&1; ok $?
t_done

t "claude login: the credential reaches the guest ON STDIN, and is in no command line"
  # THE WHOLE POINT OF THE FILE. The stub records the command string it was
  # handed separately from what arrived on stdin, so both halves are asserted:
  # the value IS in the pipe, and it is NOT in anything a process table or a log
  # would show.
  : > "$TMP/login.log"
  out="$(env $(login_env) STUB_LOG="$TMP/login.log" STUB_SECURITY_STDIN="$TMP/login.stdin" \
         STUB_CLAUDE_LOGIN=present \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "/Users/admin/testvm/richos-test-a/home" 2>/dev/null)" # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $? "a present readback must report logged in"
  eq "$out" "claude login: guest logged in"
  GUEST_CRED='{"claudeAiOauth":{"accessToken":"not-a-real-token-0123456789","expiresAt":4102444800000}}'
  HEXCRED="$(printf '%s' "$GUEST_CRED" | xxd -p | tr -d '\n')"
  has   "$(cat "$TMP/login.stdin")" "$HEXCRED"
  hasnt "$(cat "$TMP/login.log")"   "$HEXCRED"
  hasnt "$(cat "$TMP/login.log")"   "not-a-real-token"
  hasnt "$out" "not-a-real-token"
t_done

t "claude login: the guest never receives the host refresh token in either store"
  # Decode the actual stdin payload, including both keychain rows. An unchanged
  # copy passes all old storage tests but must fail this capability boundary.
  python3 - "$TMP/login.stdin" "$STUB_GUEST_FS/credentials.json" <<'PYTEST'
import json, pathlib, re, sys
payload = pathlib.Path(sys.argv[1]).read_text()
rows = re.findall(r'-X "([0-9a-f]+)"', payload)
assert len(rows) == 2
for raw in [pathlib.Path(sys.argv[2]).read_text()] + [bytes.fromhex(x).decode() for x in rows]:
    data = json.loads(raw)
    assert 'also-not-real' not in raw
    assert 'refreshToken' not in raw
    assert data['claudeAiOauth']['accessToken'] == 'not-a-real-token-0123456789'
    assert data['claudeAiOauth']['expiresAt'] == 4102444800000
PYTEST
  ok $? "only access capability may cross into the guest"
t_done

t "claude login: invalid or expired snapshots never reach a guest store"
  for invalid in \
    'not-json' \
    '{}' \
    '{"claudeAiOauth":{"accessToken":"fake","expiresAt":1}}' \
    '{"claudeAiOauth":{"accessToken":"fake","expiresAt":true}}' \
    '{"claudeAiOauth":{"accessToken":"fake","expiresAt":NaN}}' \
    '{"claudeAiOauth":{"accessToken":"","expiresAt":4102444800000}}'; do
    printf '%s' "$invalid" > "$TMP/invalid-credential.json"
    : > "$TMP/login-refused.log"
    # shellcheck disable=SC2046  # login_env prints NAME=value words; splitting them is intended, as on every other login_env line
    out="$(env $(login_env) HOST_SECURITY_CREDENTIAL_FILE="$TMP/invalid-credential.json" \
      STUB_LOG="$TMP/login-refused.log" \
      "$TESTVM_DIR/claude-login.sh" push richos-test-a "/Users/admin/testvm/richos-test-a/home" 2>"$TMP/login-refused.err")" # machine-path-exempt: isolated test VM account/path; never a customer default
    no $? "invalid/expired credentials must refuse"
    eq "$out" "claude login: guest NOT logged in"
    eq "$(wc -c < "$TMP/login-refused.log" | tr -d ' ')" "0" "a rejected credential reached the guest"
    has "$(cat "$TMP/login-refused.err")" "No host refresh token was sent"
  done
t_done

t "claude login: the item is written under BOTH names the guest could look up"
  # `run.sh` launches the app WITH CLAUDE_CONFIG_DIR, which makes claude scope
  # the keychain service name by a hash of that directory; a `claude` started by
  # hand over ssh has no such variable and asks for the bare name.
  payload="$(cat "$TMP/login.stdin")"
  has "$payload" '-s "Claude Code-credentials"'
  scoped="Claude Code-credentials-$(printf '%s' "/Users/admin/testvm/richos-test-a/home/.claude" | shasum -a 256 | awk '{print substr($1,1,8)}')" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$payload" "-s \"$scoped\""
  has "$payload" '-a "admin"'
  hasnt "$payload" '-a "alex"'
t_done

t "claude login: a second sign-in (--account) crosses as its own item, access token only, Home's name untouched"
  WORK_GUEST="/Users/admin/testvm/richos-test-a/home/claude-accounts/2" # machine-path-exempt: isolated test VM account/path; never a customer default
  : > "$TMP/work.log"
  # shellcheck disable=SC2046  # login_env prints NAME=value words; splitting them is intended
  out="$(env $(login_env) STUB_LOG="$TMP/work.log" STUB_SECURITY_STDIN="$TMP/work.stdin" STUB_CLAUDE_LOGIN=present \
         "$TESTVM_DIR/claude-login.sh" push --account "Claude Code-credentials-6392715e" --host-folder "/nonexistent/work" --guest-home "/Users/admin/testvm/richos-test-a/home"  richos-test-a "$WORK_GUEST" 2>/dev/null)" # machine-path-exempt: isolated guest or negative fixture input; no storage default
  ok $? "a second sign-in must report logged in"
  eq "$out" "claude login: guest logged in"
  work_payload="$(cat "$TMP/work.stdin")"
  wscoped="Claude Code-credentials-$(printf '%s' "$WORK_GUEST" | shasum -a 256 | awk '{print substr($1,1,8)}')"
  has   "$work_payload" "-s \"$wscoped\""
  hasnt "$work_payload" '-s "Claude Code-credentials"'
  hasnt "$work_payload" 'alex'
  has   "$work_payload" '"/Users/admin/testvm/richos-test-a/home/Library/Keychains/login.keychain-db"' # machine-path-exempt: isolated test VM account/path; never a customer default
  eq "$(printf '%s\n' "$work_payload" | grep -c '^add-generic-password')" "1" "exactly one guest item"
  hasnt "$(cat "$TMP/work.stdin" "$TMP/work.log")" "not-a-real-token"
  hasnt "$(cat "$TMP/work.stdin" "$TMP/work.log")" "also-not-real"
t_done

t "claude login: the keychain route alone is still enough — two stores, either one"
  # The file is the primary since Ray's finding, and the keychain write is kept
  # because it is a DIFFERENT mechanism from the one that failed him: `security
  # -i` with `-X <hex>` on stdin, not `add-generic-password -w`. Here the file
  # store holds nothing and the keychain item is there, and that is a login.
  out="$(env $(login_env) STUB_LOG=/dev/null STUB_SECURITY_STDIN="$TMP/login.stdin2" \
         STUB_CLAUDE_LOGIN=present STUB_CREDENTIAL_EMPTY=1 \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "/Users/admin/testvm/richos-test-a/home" 2>/dev/null)" # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $? "the keychain item alone must be enough"
  eq "$out" "claude login: guest logged in"
t_done

t "claude login: the credential file is the primary route, and it carries the VALUE"
  # Ray's vm9 audit, 2026-09-20: `security add-generic-password -w` from a pipe
  # with no tty stores an EMPTY password and exits 0 — success reported, nothing
  # stored. So the file store is the primary and its SIZE is checked against the
  # host's, which is the one check that catches a store that lies.
  : > "$TMP/login.log"
  out="$(env $(login_env) STUB_LOG="$TMP/login.log" STUB_SECURITY_STDIN="$TMP/login.stdin3" \
         STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "/Users/admin/testvm/richos-test-a/home" 2>/dev/null)" # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $? "the file route alone must be enough"
  eq "$out" "claude login: guest logged in"
  eq "$(cat "$STUB_GUEST_FS/credentials.json")" "$GUEST_CRED"
  hasnt "$(cat "$STUB_GUEST_FS/credentials.json")" "refreshToken"
  has "$(cat "$TMP/login.log")" ".claude/.credentials.json"
  has "$(cat "$TMP/login.log")" "umask 077"
  hasnt "$(cat "$TMP/login.log")" "not-a-real-token"
t_done

t "claude login: a store that reports success and holds NOTHING is caught here, not at the first model turn"
  out="$(env $(login_env) STUB_LOG=/dev/null STUB_SECURITY_STDIN="$TMP/login.stdin4" \
         STUB_CLAUDE_LOGIN=absent STUB_CREDENTIAL_EMPTY=1 \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "/Users/admin/testvm/richos-test-a/home" 2>"$TMP/login.err")" # machine-path-exempt: isolated test VM account/path; never a customer default
  no $? "an empty store must not be reported as a login"
  eq "$out" "claude login: guest NOT logged in"
  has "$(cat "$TMP/login.err")" "bytes where"
t_done

t "claude login: a home that is not the guest's is refused before anything is read"
  err="$(env $(login_env) "$TESTVM_DIR/claude-login.sh" push richos-test-a "$HOME" 2>&1)"; no $?
  has "$err" "not under the guest user's home"
t_done

# --- a long run outlives one access token (2026-09-26) -----------------------
cred_at() {  # <file> <token> <seconds-from-now>
  printf '{"claudeAiOauth":{"accessToken":"%s","refreshToken":"also-not-real-9876543210","expiresAt":%s}}' \
    "$2" "$(( ( $(date +%s) + $3 ) * 1000 ))" > "$1"
}
KEEP_HOME="/Users/admin/testvm/richos-test-a/home" # machine-path-exempt: isolated test VM account/path; never a customer default
# The same words login_env prints, as an array: nothing to word-split.
LOGIN_ENV=("TESTVM_HOST_SECURITY=$TMP/host-security.sh" "TESTVM_HOST_CLAUDE_RENEW=$TMP/host-renew.sh")

# --- the guest's primary login is the account the app has IN USE (2026-10-28) --
# Nightly 78: the app had moved the team to Work (Home at 99% of its week) and
# every guest was still signed in as Home, so its walks stalled on "Your Claude
# account has used 99% of this week's limit". The fixture lists below are the
# app's own shape (richos-core claude_accounts.rs): Account 1 has no folder and
# is the bare item; a foldered account's item is named for its folder.
ACCT_FOLDER="/nonexistent/fixture-data/claude-accounts/2"
ACCT_ITEM="Claude Code-credentials-$(printf '%s' "$ACCT_FOLDER" | shasum -a 256 | cut -c1-8)"
ACCT_HOME_SCOPED="Claude Code-credentials-$(printf '%s' "$KEEP_HOME/.claude" | shasum -a 256 | cut -c1-8)"
accounts_list() {  # <file> <inUse>
  printf '{"accounts":[{"id":"1","label":"Home"},{"id":"2","label":"Work","folder":"%s"}],"inUse":"%s","atThreshold":"pause"}\n' \
    "$ACCT_FOLDER" "$2" > "$1"
}

t "claude login: the guest's primary login is the account the app has IN USE, read from that account's own item"
  accounts_list "$TMP/accounts-work.json" 2
  : > "$TMP/acct.services"; : > "$TMP/acct.log"
  # shellcheck disable=SC2046  # login_env prints NAME=value words; splitting them is intended
  out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-work.json" HOST_SECURITY_SERVICES_LOG="$TMP/acct.services" \
         STUB_LOG="$TMP/acct.log" STUB_SECURITY_STDIN="$TMP/acct.stdin" STUB_CLAUDE_LOGIN=present \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>"$TMP/acct.err")"
  ok $? "pushing the in-use account must report logged in"
  eq "$out" "claude login: guest logged in"
  # Every host read named Work's item, and not one named Home's bare item.
  eq "$(sort -u "$TMP/acct.services")" "$ACCT_ITEM" "every host read must name the in-use account's item"
  # The GUEST's names are the ones it has always looked up: bare + its own scoped.
  acct_payload="$(cat "$TMP/acct.stdin")"
  has "$acct_payload" '-s "Claude Code-credentials" '
  has "$acct_payload" "-s \"$ACCT_HOME_SCOPED\""
  hasnt "$acct_payload" "$ACCT_ITEM"
  eq "$(printf '%s\n' "$acct_payload" | grep -c '^add-generic-password')" "2" "the same two guest rows as a Home push"
  has "$(cat "$TMP/acct.log")" "$KEEP_HOME/.claude/.credentials.json"
  hasnt "$(cat "$TMP/acct.stdin" "$TMP/acct.log" "$TMP/acct.err" "$TMP/acct.services")" "also-not-real"
  # host-check asks about the same item, by name only.
  : > "$TMP/acct.services"
  # shellcheck disable=SC2046
  hc="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-work.json" HOST_SECURITY_SERVICES_LOG="$TMP/acct.services" \
        "$TESTVM_DIR/claude-login.sh" host-check 2>&1)"; ok $? "$hc"
  has "$hc" "'$ACCT_ITEM'"
  has "$hc" "Work"
  eq "$(sort -u "$TMP/acct.services")" "$ACCT_ITEM" "host-check must ask about the in-use account's item"
t_done

t "claude login: the in-use account is renewed under its OWN folder, never Home's"
  accounts_list "$TMP/accounts-work.json" 2
  cred_at "$TMP/cred-acct-short.json" "short-lived-token-0123456789" 120
  cred_at "$TMP/cred-acct-renewed.json" "renewed-access-token-0123456789" 28800
  : > "$TMP/acct-renew.log"
  out="$(env "${LOGIN_ENV[@]}" TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-work.json" \
         HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-acct-short.json" HOST_RENEW_TO="$TMP/cred-acct-renewed.json" \
         HOST_RENEW_LOG="$TMP/acct-renew.log" STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "a renewed token is a login"
  eq "$(cat "$TMP/acct-renew.log")" "renew $ACCT_FOLDER" "renewal must run under the in-use account's folder"
  # Home in use, asked by a caller that itself runs on another account's folder
  # (an agent on Work): Home's bare item is renewed under NO folder.
  accounts_list "$TMP/accounts-home.json" 1
  cred_at "$TMP/cred-acct-short.json" "short-lived-token-0123456789" 120
  : > "$TMP/acct-renew.log"
  out="$(env "${LOGIN_ENV[@]}" CLAUDE_CONFIG_DIR="/nonexistent/callers-own-account" TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-home.json" \
         HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-acct-short.json" HOST_RENEW_TO="$TMP/cred-acct-renewed.json" \
         HOST_RENEW_LOG="$TMP/acct-renew.log" STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "a renewed Home token is a login"
  eq "$(cat "$TMP/acct-renew.log")" "renew " "Home's renewal must not inherit the caller's folder"
t_done

t "claude login: with Home in use, or no account list at all, the primary is Home's bare item as before"
  accounts_list "$TMP/accounts-home.json" 1
  for list in "$TMP/accounts-home.json" "$TMP/no-such-accounts.json"; do
    : > "$TMP/acct.services"
    # shellcheck disable=SC2046
    out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$list" HOST_SECURITY_SERVICES_LOG="$TMP/acct.services" \
           STUB_LOG=/dev/null STUB_SECURITY_STDIN="$TMP/acct-home.stdin" STUB_CLAUDE_LOGIN=present \
           "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
    ok $? "a Home push must report logged in ($list)"
    eq "$(sort -u "$TMP/acct.services")" "Claude Code-credentials" "Home's bare item ($list)"
    eq "$(grep -c '^add-generic-password' "$TMP/acct-home.stdin")" "2" "the same two guest rows ($list)"
  done
t_done

t "claude login: a second sign-in (--account) reads ITS item whatever account is in use"
  accounts_list "$TMP/accounts-work.json" 2
  : > "$TMP/acct.services"
  # shellcheck disable=SC2046
  out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-work.json" HOST_SECURITY_SERVICES_LOG="$TMP/acct.services" \
         STUB_LOG=/dev/null STUB_SECURITY_STDIN="$TMP/acct-second.stdin" STUB_CLAUDE_LOGIN=present \
         "$TESTVM_DIR/claude-login.sh" push --account "Claude Code-credentials-0badc0de" --host-folder "/nonexistent/other" \
         --guest-home "$KEEP_HOME" richos-test-a "$KEEP_HOME/claude-accounts/2" 2>/dev/null)"
  ok $? "a second sign-in must report logged in"
  eq "$(sort -u "$TMP/acct.services")" "Claude Code-credentials-0badc0de" "--account names the host item, the list does not"
  eq "$(grep -c '^add-generic-password' "$TMP/acct-second.stdin")" "1" "exactly one guest item"
t_done

t "claude login: an account list it cannot read is a refusal, never a guess at the account"
  printf '{"accounts": not json' > "$TMP/accounts-bad.json"
  : > "$TMP/acct-bad.log"
  # shellcheck disable=SC2046
  out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-bad.json" STUB_LOG="$TMP/acct-bad.log" \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>"$TMP/acct-bad.err")"
  no $? "an unreadable account list must refuse"
  eq "$out" "claude login: guest NOT logged in"
  eq "$(wc -c < "$TMP/acct-bad.log" | tr -d ' ')" "0" "nothing reached the guest"
  has "$(cat "$TMP/acct-bad.err")" "$TMP/accounts-bad.json"
t_done

t "claude login: an account list the app itself would refuse is refused, never read around"
  # Tom's review of 9bbb8777. The app's Stored (claude_accounts.rs) requires inUse and a string
  # id and label on every account, Accounts::open refuses a list without Account 1, and a
  # non-ASCII folder is refused rather than name another item for the same spelling. The script
  # read the first two more leniently than the app, and none of the four had a case.
  printf '{"accounts":[{"id":"1","label":"Home"},{"id":"2","label":"Work","folder":"%s"}],"atThreshold":"pause"}\n' \
    "$ACCT_FOLDER" > "$TMP/accounts-no-inuse.json"
  # The account without a label is NOT the one in use: the app refuses the whole list anyway.
  printf '{"accounts":[{"id":"1"},{"id":"2","label":"Work","folder":"%s"}],"inUse":"2"}\n' \
    "$ACCT_FOLDER" > "$TMP/accounts-no-label.json"
  printf '{"accounts":[{"id":"2","label":"Work","folder":"%s"}],"inUse":"2"}\n' "$ACCT_FOLDER" > "$TMP/accounts-no-one.json"
  printf '{"accounts":[{"id":"1","label":"Home"},{"id":"2","label":"Work","folder":"/nonexistent/caf\\u00e9/2"}],"inUse":"2"}\n' \
    > "$TMP/accounts-non-ascii.json"
  for case in no-inuse no-label no-one non-ascii; do
    : > "$TMP/acct-refused.log"
    # shellcheck disable=SC2046  # login_env prints NAME=value words; splitting them is intended
    out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-$case.json" STUB_LOG="$TMP/acct-refused.log" \
           "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>"$TMP/acct-refused.err")"
    no $? "a list the app would refuse ($case) must refuse"
    eq "$out" "claude login: guest NOT logged in" "$case"
    eq "$(wc -c < "$TMP/acct-refused.log" | tr -d ' ')" "0" "nothing reached the guest ($case)"
    has "$(cat "$TMP/acct-refused.err")" "$TMP/accounts-$case.json"
  done
t_done

t "claude login: an inUse the list does not name is the list's first account, as the app's in_use picks"
  printf '{"accounts":[{"id":"2","label":"Work","folder":"%s"},{"id":"1","label":"Home"}],"inUse":"9"}\n' \
    "$ACCT_FOLDER" > "$TMP/accounts-unlisted.json"
  : > "$TMP/acct.services"
  # shellcheck disable=SC2046
  out="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-unlisted.json" HOST_SECURITY_SERVICES_LOG="$TMP/acct.services" \
         STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=present \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "an unlisted inUse is the first account, not a refusal"
  eq "$(sort -u "$TMP/acct.services")" "$ACCT_ITEM" "the first account listed (Work) is the one read"
t_done

t "claude login: run.sh passes on host-check's own reason, never a fixed 'not logged in'"
  # Tom's review of 9bbb8777: run.sh sent host-check's stderr to /dev/null, so an unreadable
  # account list was reported as "not logged in — /login on the host first".
  src="$(cat "$TESTVM_DIR/run.sh")"
  # shellcheck disable=SC2016  # the $ names are literal source text
  has "$src" 'HOST_LOGIN_REFUSAL="$("$HERE/claude-login.sh" host-check 2>&1 >/dev/null)"'
  hasnt "$src" 'host-check >/dev/null 2>&1'
  # And the reason it passes on names the list, not a sign-in.
  # shellcheck disable=SC2046
  err="$(env $(login_env) TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-bad.json" "$TESTVM_DIR/claude-login.sh" host-check 2>&1 >/dev/null)"
  no $? "host-check refuses an unreadable list"
  has "$err" "cannot be read"
  hasnt "$err" "/login on the host first"
t_done

# The mutation pass below is run by hand, so a change to claude-login.sh or to these cases
# reaches the merge only through this runner: its text check (each mutant's text found once,
# each case it names still here) runs no case and takes well under a second. Not named
# "claude login", so the pass's own `run-tests.sh "claude login"` never runs it.
t "mutation pass: claude-login.mutation.py still matches claude-login.sh and the cases it names"
  out="$(RICHOS_MUTATION_PASSES=0 python3 "$TESTVM_DIR/test/claude-login.mutation.py" 2>&1)"
  ok $? "$out"
  has "$out" "all match"
t_done

t "claude login: a token inside the renewal window is renewed by THIS Mac first, and the fresh one crosses"
  cred_at "$TMP/cred-short.json" "short-lived-token-0123456789" 120
  cred_at "$TMP/cred-renewed.json" "renewed-access-token-0123456789" 28800
  : > "$TMP/renew.log"
  out="$(env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-short.json" HOST_RENEW_TO="$TMP/cred-renewed.json" \
         HOST_RENEW_LOG="$TMP/renew.log" STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "a renewed token is a login"
  eq "$out" "claude login: guest logged in"
  eq "$(wc -l < "$TMP/renew.log" | tr -d ' ')" "1" "this Mac is asked exactly once"
  has   "$(cat "$STUB_GUEST_FS/credentials.json")" "renewed-access-token-0123456789"
  hasnt "$(cat "$STUB_GUEST_FS/credentials.json")" "short-lived-token"
  hasnt "$(cat "$STUB_GUEST_FS/credentials.json")" "refreshToken"
t_done

t "claude login: an expired host token is renewed by THIS Mac rather than refused"
  cred_at "$TMP/cred-expired.json" "expired-token-0123456789" -60
  cred_at "$TMP/cred-renewed.json" "renewed-access-token-0123456789" 28800
  : > "$TMP/renew.log"
  out="$(env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-expired.json" HOST_RENEW_TO="$TMP/cred-renewed.json" \
         HOST_RENEW_LOG="$TMP/renew.log" STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "renewed, not refused"
  eq "$out" "claude login: guest logged in"
  has "$(cat "$STUB_GUEST_FS/credentials.json")" "renewed-access-token-0123456789"
t_done

t "claude login: a token with hours left is handed over as it is, without asking this Mac to renew"
  cred_at "$TMP/cred-long.json" "long-lived-token-0123456789" 7200
  : > "$TMP/renew.log"
  out="$(env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-long.json" HOST_RENEW_LOG="$TMP/renew.log" \
         STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "a long-lived token is a login"
  eq "$(wc -c < "$TMP/renew.log" | tr -d ' ')" "0" "no renewal was needed"
  has "$(cat "$STUB_GUEST_FS/credentials.json")" "long-lived-token-0123456789"
t_done

t "claude login: push records WHEN the guest's copy runs out and WHICH host write it came from, and no token"
  mkdir -p "$TESTVM_ROOT/run/richos-test-a"
  printf '20260926110315\n' > "$TMP/host-mdat"
  cred_at "$TMP/cred-long.json" "long-lived-token-0123456789" 7200
  out="$(env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-long.json" HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" \
         STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
         "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" 2>/dev/null)"
  ok $? "push"
  exp="$(cat "$TESTVM_ROOT/run/richos-test-a/claude-login.expires" 2>/dev/null)"
  case "$exp" in ''|*[!0-9]*) no 0 "expires must be a number, got [$exp]" ;; esac
  eq "$(cat "$TESTVM_ROOT/run/richos-test-a/claude-login.host-written" 2>/dev/null)" "20260926110315"
  hasnt "$(cat "$TESTVM_ROOT/run/richos-test-a/"claude-login.* 2>/dev/null)" "token"
t_done

t "claude login: keep hands the guest each token THIS Mac renews, and ends when the run state goes"
  mkdir -p "$TESTVM_ROOT/run/richos-test-a"
  printf '20260926110315\n' > "$TMP/host-mdat"
  cred_at "$TMP/cred-keep.json" "first-token-0123456789" 7200
  env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-keep.json" HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" \
      STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
      "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" >/dev/null 2>&1
  env "${LOGIN_ENV[@]}" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-keep.json" HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" \
      STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent TESTVM_CLAUDE_KEEP_SECONDS=1 \
      "$TESTVM_DIR/claude-login.sh" keep richos-test-a "$KEEP_HOME" >"$TMP/keep.out" 2>&1 &
  KEEP_PID=$!
  sleep 2
  cred_at "$TMP/cred-keep.json" "second-token-0123456789" 28800   # THIS Mac renews
  printf '20260926190000\n' > "$TMP/host-mdat"
  for _ in 1 2 3 4 5 6 7 8; do
    grep -q second-token "$STUB_GUEST_FS/credentials.json" 2>/dev/null && break
    sleep 1
  done
  has   "$(cat "$STUB_GUEST_FS/credentials.json")" "second-token-0123456789"
  hasnt "$(cat "$STUB_GUEST_FS/credentials.json")" "refreshToken"
  rm -rf "$TESTVM_ROOT/run/richos-test-a"                         # stop.sh's last act
  for _ in 1 2 3 4 5 6; do kill -0 "$KEEP_PID" 2>/dev/null || break; sleep 1; done
  if kill -0 "$KEEP_PID" 2>/dev/null; then
    no 0 "the keeper outlived its run"; kill "$KEEP_PID" 2>/dev/null
  fi
  wait "$KEEP_PID" 2>/dev/null
  has "$(cat "$TMP/keep.out")" "the run state is gone"
t_done

t "claude login: keep asks THIS Mac to renew once when the guest's copy enters the window unrenewed"
  mkdir -p "$TESTVM_ROOT/run/richos-test-a"
  printf '20260926110315\n' > "$TMP/host-mdat"
  printf '20260926110315\n' > "$TESTVM_ROOT/run/richos-test-a/claude-login.host-written"
  printf '%s\n' "$(( ( $(date +%s) + 100 ) * 1000 ))" > "$TESTVM_ROOT/run/richos-test-a/claude-login.expires"
  : > "$TMP/renew.log"
  env "${LOGIN_ENV[@]}" HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" HOST_RENEW_LOG="$TMP/renew.log" \
      TESTVM_CLAUDE_KEEP_SECONDS=1 \
      "$TESTVM_DIR/claude-login.sh" keep richos-test-a "$KEEP_HOME" >"$TMP/keep2.out" 2>&1 &
  KEEP_PID=$!
  sleep 4
  rm -rf "$TESTVM_ROOT/run/richos-test-a"
  for _ in 1 2 3 4 5 6; do kill -0 "$KEEP_PID" 2>/dev/null || break; sleep 1; done
  kill -0 "$KEEP_PID" 2>/dev/null && { no 0 "the keeper outlived its run"; kill "$KEEP_PID" 2>/dev/null; }
  wait "$KEEP_PID" 2>/dev/null
  eq "$(wc -l < "$TMP/renew.log" | tr -d ' ')" "1" "asked once per token, never in a loop"
t_done

t "claude login: keep follows a mid-run account switch, handing the guest the new account's token"
  # Tom's review of 9bbb8777: the keeper watched and renewed the account read when it started,
  # so after the app moved the team to Work the guest kept Home's token. Here Home is in use at
  # the push; mid-run the app moves to Work (this Mac's write time does not change, so nothing
  # else would push). The guest must get Work's token, read from Work's own item.
  mkdir -p "$TESTVM_ROOT/run/richos-test-a"
  printf '20260926110315\n' > "$TMP/host-mdat"
  accounts_list "$TMP/accounts-switch.json" 1
  cred_at "$TMP/cred-switch.json" "home-token-0123456789" 7200
  env "${LOGIN_ENV[@]}" TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-switch.json" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-switch.json" \
      HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent \
      "$TESTVM_DIR/claude-login.sh" push richos-test-a "$KEEP_HOME" >/dev/null 2>&1
  has "$(cat "$STUB_GUEST_FS/credentials.json")" "home-token-0123456789"
  : > "$TMP/switch.services"
  env "${LOGIN_ENV[@]}" TESTVM_CLAUDE_ACCOUNTS="$TMP/accounts-switch.json" HOST_SECURITY_CREDENTIAL_FILE="$TMP/cred-switch.json" \
      HOST_SECURITY_MDAT_FILE="$TMP/host-mdat" HOST_SECURITY_SERVICES_LOG="$TMP/switch.services" \
      STUB_LOG=/dev/null STUB_SECURITY_STDIN=/dev/null STUB_CLAUDE_LOGIN=absent TESTVM_CLAUDE_KEEP_SECONDS=1 \
      "$TESTVM_DIR/claude-login.sh" keep richos-test-a "$KEEP_HOME" >"$TMP/keep3.out" 2>&1 &
  KEEP_PID=$!
  sleep 2
  cred_at "$TMP/cred-switch.json" "work-token-0123456789" 7200   # what Work's item holds
  accounts_list "$TMP/accounts-switch.json" 2                     # the app moves the team to Work
  for _ in 1 2 3 4 5 6 7 8; do
    grep -q work-token "$STUB_GUEST_FS/credentials.json" 2>/dev/null && break
    sleep 1
  done
  has   "$(cat "$STUB_GUEST_FS/credentials.json")" "work-token-0123456789"
  hasnt "$(cat "$STUB_GUEST_FS/credentials.json")" "refreshToken"
  has "$(cat "$TMP/switch.services")" "$ACCT_ITEM"
  rm -rf "$TESTVM_ROOT/run/richos-test-a"
  for _ in 1 2 3 4 5 6; do kill -0 "$KEEP_PID" 2>/dev/null || break; sleep 1; done
  kill -0 "$KEEP_PID" 2>/dev/null && { no 0 "the keeper outlived its run"; kill "$KEEP_PID" 2>/dev/null; }
  wait "$KEEP_PID" 2>/dev/null
  has "$(cat "$TMP/keep3.out")" "moved the team to Work"
t_done

t "claude login: run.sh starts ONE keeper after a successful push and records its pid; stop.sh ends that pid"
  src="$(cat "$TESTVM_DIR/run.sh")"
  # shellcheck disable=SC2016  # the $ names are literal source text
  has "$src" 'claude-login.sh" keep "$VM" "$GUEST_HOME"'
  # shellcheck disable=SC2016  # the $ names are literal source text
  has "$src" '"$STATE/claude-keep.pid"'
  lg="$(printf '%s\n' "$src" | grep -n 'claude-login.sh" push' | head -1 | cut -d: -f1)"
  kp="$(printf '%s\n' "$src" | grep -n 'claude-login.sh" keep' | head -1 | cut -d: -f1)"
  if [ -n "$lg" ] && [ -n "$kp" ] && [ "$lg" -lt "$kp" ]; then ok 0; else ok 1 "the keeper starts after the push"; fi
  stop="$(cat "$TESTVM_DIR/stop.sh")"
  has "$stop" 'claude-keep.pid'
  kk="$(printf '%s\n' "$stop" | grep -n 'claude-keep.pid' | head -1 | cut -d: -f1)"
  # shellcheck disable=SC2016  # the $ names are literal source text
  rs="$(printf '%s\n' "$stop" | grep -n 'rm -rf "$STATE"' | head -1 | cut -d: -f1)"
  if [ -n "$kk" ] && [ -n "$rs" ] && [ "$kk" -lt "$rs" ]; then ok 0; else ok 1 "the keeper is ended before the state goes"; fi
t_done

# ===========================================================================
# stop.sh — the host-side runner, by its recorded pid AND its own command line
# ===========================================================================
# run.sh records the pid of `<tart> run <vm> --no-graphics`. A run
# state can outlive that process (a crashed harness, a reboot), and macOS reuses
# pids, so a recorded pid alone may name somebody else's work by the time stop.sh
# reads it. Both cases run stop.sh against their own root and their own processes,
# started here; nothing else on this Mac is signaled.
STOP_ROOT="$TMP/stoproot"; STOP_VM="richos-test-stoppid"
STOP_TART="$STOP_ROOT/bin/tart.app/Contents/MacOS/tart"

t "host power: both VM launchers run Tart directly without a wake lock"
  for launcher in run.sh setup.sh; do
    src="$(cat "$TESTVM_DIR/$launcher")"
    has "$src" "nohup \"\$TART_BIN\" run "
    if printf '%s\n' "$src" | grep -E '^[[:space:]]*[^#[:space:]].*caffeinate'; then
      ok 1 "$launcher must not hold the host awake"
    fi
  done
t_done

t "stop: a recorded runner pid that now belongs to other work is left alone"
  mkdir -p "$STOP_ROOT/run/$STOP_VM"
  sleep 60 &                 # owned stand-in: alive, and NOT a VM runner
  BYSTANDER=$!
  echo "$BYSTANDER" > "$STOP_ROOT/run/$STOP_VM/vm.pid"
  out="$(env TESTVM_ROOT="$STOP_ROOT" TART_HOME="$STOP_ROOT/tart" "$TESTVM_DIR/stop.sh" "$STOP_VM" 2>&1)"; ok $? "$out"
  kill -0 "$BYSTANDER" 2>/dev/null; ok $? "stop.sh signaled pid $BYSTANDER, which was not this VM's runner: $out"
  has "$out" "left alone"
  kill -TERM "$BYSTANDER" 2>/dev/null; wait "$BYSTANDER" 2>/dev/null
t_done

t "stop: the recorded runner, identified by its command line, is ended with its child"
  mkdir -p "$(dirname "$STOP_TART")" "$STOP_ROOT/run/$STOP_VM"
  cat > "$STOP_TART" <<'EOF'
#!/usr/bin/env bash
# A stand-in tart: lists no VMs, and `run` holds one owned child.
case "${1:-}" in
  list) echo '[]' ;;
  run)  sleep 60 & echo $! > "$STUB_RUNNER_CHILD"; wait ;;
esac
EOF
  chmod 755 "$STOP_TART"
  rm -f "$TMP/runner.child"
  STUB_RUNNER_CHILD="$TMP/runner.child" "$STOP_TART" run "$STOP_VM" --no-graphics &
  RUNNER=$!
  for _ in $(seq 1 50); do [ -s "$TMP/runner.child" ] && break; sleep 0.1; done
  RUNNER_CHILD="$(cat "$TMP/runner.child" 2>/dev/null)"
  echo "$RUNNER" > "$STOP_ROOT/run/$STOP_VM/vm.pid"
  out="$(env TESTVM_ROOT="$STOP_ROOT" TART_HOME="$STOP_ROOT/tart" "$TESTVM_DIR/stop.sh" "$STOP_VM" 2>&1)"; ok $? "$out"
  has "$out" "reaping the host-side VM runner"
  kill -0 "$RUNNER" 2>/dev/null; no $? "the runner (pid $RUNNER) survived stop.sh: $out"
  if [ -n "$RUNNER_CHILD" ]; then
    kill -0 "$RUNNER_CHILD" 2>/dev/null; no $? "the runner's child (pid $RUNNER_CHILD) survived stop.sh"
  else
    ok 1 "the stand-in runner never recorded its child"
  fi
  # Owned pids only, captured above, in case the assertion failed and they live on.
  kill -TERM "$RUNNER" ${RUNNER_CHILD:+"$RUNNER_CHILD"} 2>/dev/null; wait "$RUNNER" 2>/dev/null
  rm -f "$STOP_TART"
t_done

t "claude login: run.sh copies the login in after the keychain exists, and prints the line"
  src="$(cat "$TESTVM_DIR/run.sh")"
  has "$src" 'claude-login.sh" push "$VM" "$GUEST_HOME"'
  has "$src" 'echo "$CLAUDE_LOGIN_LINE"'
  has "$src" 'host-check'
  # order: the keychain is prepared BEFORE the login is pushed into it
  kc="$(printf '%s\n' "$src" | grep -n 'keychain.sh" prepare' | head -1 | cut -d: -f1)"
  lg="$(printf '%s\n' "$src" | grep -n 'claude-login.sh" push' | head -1 | cut -d: -f1)"
  [ -n "$kc" ] && [ -n "$lg" ] && [ "$kc" -lt "$lg" ]; ok $? "keychain prepare must come first"
t_done

# ===========================================================================
# ax.sh tree / find / click — the guest's screen, as text
# ===========================================================================
# WHAT THESE ARE FOR. Three walks wrote roughly fourteen versions of this one
# job (`axdump.js`, `axpress{,2,3}.js`, `axproc.js`, `axsafari.js`, `axmac.js`,
# `axdialog.js`, `findsafari.sh`, `findmac.sh`, `findphone.sh` …), and the two
# defects that cost the most time are the two asserted hardest here:
#
#   1. GEOMETRY. AppleScript renders {1024, 700} as "1024700", so a fused token
#      looks like a number and gets quoted in an audit. The fix is structural
#      (ax.py unpacks AXPosition/AXSize into numbers) and the renderer REFUSES a fused
#      token anyway, because a silent wrong number is the failure mode.
#   2. MATCHING. `axpress.js` matched AXTitle, `axpress2.js` added a role and
#      still found nothing, and `axpress3.js` — the third rewrite — added
#      AXDescription, where an aria-label actually lands.
#
# The guest is the stub: ax.sh sends the program on stdin and the stub keeps it,
# so what was ASKED is asserted separately from what was answered.
# ===========================================================================
AXVM="richos-test-ax"
mkdir -p "$TESTVM_RUN/$AXVM"
echo 717 > "$TESTVM_RUN/$AXVM/app.pid"

cat > "$TMP/ax-tree.ndjson" <<'AXTREE'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":4,"truncated":false,"mode":"tree","matches":null}
{"d":0,"role":"AXWindow","sub":"AXStandardWindow","title":"RichOS","desc":"","value":"","enabled":null,"x":0,"y":25,"w":1024,"h":700}
{"d":1,"role":"AXGroup","sub":"","title":"","desc":"","value":"","enabled":null,"x":0,"y":25,"w":1024,"h":700}
{"d":2,"role":"AXButton","sub":"","title":"Settings","desc":"","value":"","enabled":true,"x":974,"y":43,"w":32,"h":32}
{"d":2,"role":"AXButton","sub":"","title":"","desc":"Dark theme","value":"","enabled":true,"x":900,"y":43,"w":32,"h":32}
AXTREE

# The same window, as an AppleScript dump would have delivered it: the two
# numbers already fused into one token by list coercion.
cat > "$TMP/ax-fused.ndjson" <<'AXFUSED'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":1,"truncated":false,"mode":"tree","matches":null}
{"d":0,"role":"AXWindow","sub":"","title":"RichOS","desc":"","value":"","enabled":null,"x":0,"y":25,"w":"1024700","h":null}
AXFUSED

cat > "$TMP/ax-find.ndjson" <<'AXFIND'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":312,"truncated":false,"mode":"find","matches":1}
{"d":2,"role":"AXButton","sub":"","title":"Settings","desc":"","value":"","enabled":true,"x":974,"y":43,"w":32,"h":32}
AXFIND

cat > "$TMP/ax-click-ok.ndjson" <<'AXCLICK'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":312,"truncated":false,"mode":"click","matches":1}
{"clicked":true,"action":"AXPress","matches":1,"node":{"d":2,"role":"AXButton","sub":"","title":"Settings","desc":"","value":"","enabled":true,"x":974,"y":43,"w":32,"h":32}}
AXCLICK

cat > "$TMP/ax-click-none.ndjson" <<'AXNONE'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":312,"truncated":false,"mode":"click","matches":0}
{"error":"notfound","detail":"nothing matched","matches":0}
AXNONE

cat > "$TMP/ax-click-noaction.ndjson" <<'AXNOACT'
{"meta":true,"app":"RichOS","pid":717,"windows":1,"nodes":312,"truncated":false,"mode":"click","matches":1}
{"error":"noaction","detail":"the matched element has no AXPress","actions":["AXShowMenu"],"matches":1,"node":{"role":"AXGroup"}}
AXNOACT

cat > "$TMP/ax-notfront.ndjson" <<'AXNF'
{"error":"notfrontmost","detail":"frontmost pid is 401, the target (RichOS) is 717 — a click at a point would land in another process","frontmost":401,"target":717}
AXNF

cat > "$TMP/ax-truncated.ndjson" <<'AXTRUNC'
{"meta":true,"app":"Safari","pid":880,"windows":1,"nodes":4000,"truncated":true,"mode":"tree","matches":null}
{"d":0,"role":"AXWindow","sub":"","title":"RichOS","desc":"","value":"","enabled":null,"x":0,"y":0,"w":1680,"h":1050}
AXTRUNC

axrun() {  # axrun <fixture> <args...> — run ax.sh against the stub guest
  local fixture="$1"; shift
  STUB_AX_FIXTURE="$fixture" STUB_AX_SCRIPT="$TMP/ax-sent.py" STUB_LOG="$TMP/ax.log" \
    "$TESTVM_DIR/ax.sh" "$AXVM" "$@" 2>"$TMP/ax.err"
}

axparams() {  # the parameter line the guest received, decoded exactly as ax.py decodes it
  python3 -c 'import ast, json, sys
line = open(sys.argv[1]).readline()
name, _, literal = line.partition(" = ")
assert name == "AX_PARAMS_JSON", line[:80]
print(json.dumps(json.loads(ast.literal_eval(literal)), ensure_ascii=False))' "$TMP/ax-sent.py"
}

t "ax tree: geometry is FOUR numbers, and the fused AppleScript token never appears"
  out="$(axrun "$TMP/ax-tree.ndjson" tree)"; ok $? "$(cat "$TMP/ax.err")"
  has   "$out" "size=1024 700"
  has   "$out" "pos=974 43"
  has   "$out" "size=32 32"
  hasnt "$out" "1024700"
t_done

t "ax tree: the shape of the screen survives — one indent level per depth"
  out="$(axrun "$TMP/ax-tree.ndjson" tree)"
  eq "$(printf '%s\n' "$out" | grep -c '^AXWindow/AXStandardWindow ')" "1"
  eq "$(printf '%s\n' "$out" | grep -c '^  AXGroup ')" "1"
  eq "$(printf '%s\n' "$out" | grep -c '^    AXButton ')" "2"
t_done

t "ax tree: role, title, description, value and enabled are all on the line"
  out="$(axrun "$TMP/ax-tree.ndjson" tree)"
  has "$out" "title='Settings'"
  has "$out" "desc='Dark theme'"
  has "$out" "enabled=yes"
  has "$out" "# app=RichOS pid=717 windows=1 nodes=4 mode=tree"
t_done

t "ax tree: a FUSED geometry token is refused, not printed as a plausible number"
  # The whole reason the renderer checks: "1024700" is six characters that look
  # like a measurement. A walk would quote it and nobody could tell.
  out="$(axrun "$TMP/ax-fused.ndjson" tree)"; no $? "a fused token must not be a success"
  hasnt "$out" "1024700"
  has   "$out" "size=? -"
  has   "$(cat "$TMP/ax.err")" "REFUSED to print a geometry value"
  has   "$(cat "$TMP/ax.err")" "1024700"
t_done

t "ax tree: a walk that hit its node cap says so — an incomplete tree is not a tree"
  out="$(axrun "$TMP/ax-truncated.ndjson" tree)"; no $? "incomplete traversal must not claim success"
  has "$(cat "$TMP/ax.err")" "hit its node cap"
t_done

t "ax tree: --json hands the guest's own lines through untouched"
  out="$(axrun "$TMP/ax-tree.ndjson" tree --json)"; ok $?
  has "$out" '"role":"AXButton","sub":"","title":"Settings"'
  hasnt "$out" "pos="
t_done

t "ax tree: --app names the process to read, and the parameters travel on STDIN"
  axrun "$TMP/ax-tree.ndjson" tree --app Safari --depth 18 >/dev/null; ok $?
  sent="$(axparams)"
  has "$sent" '"app": "Safari"'
  has "$sent" '"mode": "tree"'
  has "$sent" '"depth": 18'
  # ...and NOT in the command line the guest was handed.
  hasnt "$(cat "$TMP/ax.log")" '"app": "Safari"'
  # The guest runs ax.py from stdin: the command line ENDS with the interpreter.
  case "$(tail -1 "$TMP/ax.log")" in *" python3 -") ok 0 ;; *) ok 1 "the read must end with: python3 -" ;; esac
t_done

t "ax: the read carries a deadline — a wedged app cannot hang the harness"
  axrun "$TMP/ax-tree.ndjson" tree >/dev/null
  has "$(cat "$TMP/ax.log")" "start_new_session=True"
t_done

t "ax: what is sent to the guest is a VALID Python program, parameters and all"
  # The params line and ax.py are concatenated on this Mac and run in the guest;
  # a syntax error would otherwise surface only as a python3 failure inside a VM,
  # which is the slowest place to find one.
  axrun "$TMP/ax-tree.ndjson" find --title "it's \"quoted\" & {braced}"$'\n'"line two" >/dev/null 2>&1
  python3 -B -c 'import sys; compile(open(sys.argv[1]).read(), sys.argv[1], "exec")' "$TMP/ax-sent.py" 2>"$TMP/ax-compile.err"
  ok $? "the generated program must compile: $(cat "$TMP/ax-compile.err")"
  # The title is DATA in the program, a string literal decoded back to the exact
  # label: a quote, a brace, an apostrophe and a newline in a button's label are
  # ordinary and must not be able to end the string or the program.
  eq "$(axparams | python3 -c 'import json,sys; print(repr(json.load(sys.stdin)["text"]))')" \
     "$(python3 -c 'print(repr("it'"'"'s \"quoted\" & {braced}\nline two"))')"
  eq "$(wc -l < "$TMP/ax-sent.py" | tr -d ' ')" "$(( $(wc -l < "$TESTVM_DIR/ax.py" | tr -d ' ') + 1 ))" \
     "the parameters are ONE line in front of ax.py"
t_done

t "ax find: the criteria reach the guest and the hit comes back with its box"
  out="$(axrun "$TMP/ax-find.ndjson" find --title Settings --role AXButton)"; ok $?
  sent="$(axparams)"
  has "$sent" '"mode": "find"'
  has "$sent" '"text": "Settings"'
  has "$sent" '"role": "AXButton"'
  has "$out" "matches=1"
  has "$out" "pos=974 43 size=32 32"
t_done

t "ax find: --contains asks for a substring, not an equality"
  axrun "$TMP/ax-find.ndjson" find --title Sett --contains >/dev/null; ok $?
  has "$(axparams)" '"contains": true'
t_done

t "ax find: something to match is required — an unfiltered find is a tree"
  axrun "$TMP/ax-find.ndjson" find >/dev/null 2>&1; no $?
  has "$(cat "$TMP/ax.err")" "needs something to match"
t_done

t "ax matcher: text matches AXDescription as well as AXTitle — axpress3's lesson"
  # Driven directly, in Python, because this rule cost three rewrites of the same
  # script and a grep of the source would prove only that a word is present. The
  # eight cases are ax-search.test.py's Matcher class, run against ax.py itself.
  mout="$(python3 -B "$HERE/ax-search.test.py" -v Matcher 2>&1)"; ok $? "$mout"
  has "$mout" "test_the_eight_matcher_cases"
  has "$mout" "OK"
t_done

t "ax click: presses by AXPress and reports what it pressed, with the box"
  out="$(axrun "$TMP/ax-click-ok.ndjson" click --title Settings)"; ok $?
  has "$(axparams)" '"mode": "click"'
  has "$out" "pressed AXButton"
  has "$out" "title='Settings'"
  has "$out" "pos=974 43 size=32 32"
t_done

t "ax click: nothing matched is a FAILURE with a count, never a quiet success"
  out="$(axrun "$TMP/ax-click-none.ndjson" click --title Nope)"; no $?
  has "$(cat "$TMP/ax.err")" "notfound"
  has "$out" "matches=0"
t_done

t "ax click: an element with no AXPress says so, and lists what it CAN do"
  axrun "$TMP/ax-click-noaction.ndjson" click --title Something >/dev/null; no $?
  has "$(cat "$TMP/ax.err")" "no AXPress"
  has "$(cat "$TMP/ax.err")" "AXShowMenu"
t_done

t "ax click --at: a coordinate click carries the frontmost refusal, like --key"
  axrun "$TMP/ax-notfront.ndjson" click --at 974,43 >/dev/null; no $?
  has "$(axparams)" '"mode": "clickat"'
  has "$(axparams)" '"atx": 974'
  has "$(cat "$TMP/ax.err")" "would land in another process"
t_done

t "ax click --at: x,y or a refusal — never a guess"
  axrun "$TMP/ax-click-ok.ndjson" click --at 974 >/dev/null 2>&1; no $?
  has "$(cat "$TMP/ax.err")" "--at takes x,y"
t_done

t "ax: an unknown flag is refused rather than silently ignored"
  axrun "$TMP/ax-tree.ndjson" tree --window-title Home >/dev/null 2>&1; no $?
  has "$(cat "$TMP/ax.err")" "unknown argument"
t_done

t "ax: --windows still punctuates the list items by hand"
  # The old AppleScript path is the one place a fused token could still be born,
  # and it is kept correct here rather than by memory.
  src="$(cat "$TESTVM_DIR/ax.sh")"
  has   "$src" "(item 1 of sz)"
  hasnt "$src" "(size of w as string)"
t_done

# ===========================================================================
# guest.sh — one word into the guest
# ===========================================================================
# Eleven wrappers around `guest_ssh` were written across three walks because
# `guest_ssh` is a shell FUNCTION in a sourced file and nothing on a command
# line could call it. These cases drive the real script against a fake `ssh` on
# PATH, so what is asserted is the CALL IT BUILDS — the options, the address,
# the command string, the exit code — rather than a mock of it.
# ===========================================================================
GVM="richos-test-g"
mkdir -p "$TMP/bin" "$TESTVM_RUN/$GVM"
echo "10.0.0.9" > "$TESTVM_RUN/$GVM/ip"
: > "$TESTVM_SSH_KEY"

cat > "$TMP/bin/ssh" <<'FAKESSH'
#!/usr/bin/env bash
# One argument per line, so a test can assert an exact argument rather than
# a substring of a joined string.
printf '%s\n' "$@" >> "${FAKE_SSH_LOG:-/dev/null}"
[ -n "${FAKE_SSH_STDIN:-}" ] && cat > "$FAKE_SSH_STDIN"
exit "${FAKE_SSH_RC:-0}"
FAKESSH
cat > "$TMP/bin/scp" <<'FAKESCP'
#!/usr/bin/env bash
printf '%s\n' "$@" >> "${FAKE_SCP_LOG:-/dev/null}"
exit "${FAKE_SCP_RC:-0}"
FAKESCP
chmod 755 "$TMP/bin/ssh" "$TMP/bin/scp"

grun() {  # grun <args...> — guest.sh with the fake ssh first on PATH
  : > "$TMP/ssh.log"
  PATH="$TMP/bin:$PATH" FAKE_SSH_LOG="$TMP/ssh.log" \
    "$TESTVM_DIR/guest.sh" "$GVM" "$@" 2>"$TMP/guest.err"
}

t "guest.sh: the ssh call carries the harness's own options, key and guest user"
  grun 'uname -a' >/dev/null; ok $? "$(cat "$TMP/guest.err")"
  log="$(cat "$TMP/ssh.log")"
  has "$log" "BatchMode=yes"
  has "$log" "StrictHostKeyChecking=no"
  has "$log" "UserKnownHostsFile=/dev/null"
  has "$log" "$TESTVM_SSH_KEY"
  has "$log" "admin@10.0.0.9"
t_done

t "guest.sh: ONE argument is the guest's shell command, passed through verbatim"
  # Pipes and redirections belong to the GUEST. Quoting them here would break
  # `guest.sh vm 'cat > /tmp/x'`, which is how a secret is written.
  grun 'ls -la /tmp | wc -l' >/dev/null; ok $?
  eq "$(tail -1 "$TMP/ssh.log")" "ls -la /tmp | wc -l"
t_done

t "guest.sh: SEVERAL arguments are quoted, so a path with a space stays one path"
  # ssh joins its arguments with spaces and lets the guest re-split them, which
  # is how `ls '/a b'` silently becomes two arguments.
  grun ls -la '/a b' >/dev/null; ok $?
  eq "$(tail -1 "$TMP/ssh.log")" 'ls -la /a\ b'
t_done

t "guest.sh: -- forces the literal reading even for a single argument"
  grun -- 'my file.txt' >/dev/null; ok $?
  eq "$(tail -1 "$TMP/ssh.log")" 'my\ file.txt'
t_done

t "guest.sh: the exit code is the guest command's, untouched"
  PATH="$TMP/bin:$PATH" FAKE_SSH_LOG=/dev/null FAKE_SSH_RC=7 \
    "$TESTVM_DIR/guest.sh" "$GVM" 'exit 7' >/dev/null 2>&1
  eq "$?" "7"
t_done

t "guest.sh: stdin goes through, and the value it carries is in NO command line"
  : > "$TMP/ssh.log"
  printf '%s' "$FAKE_KEY" | PATH="$TMP/bin:$PATH" FAKE_SSH_LOG="$TMP/ssh.log" \
    FAKE_SSH_STDIN="$TMP/ssh.stdin" "$TESTVM_DIR/guest.sh" "$GVM" \
    "cat > '/tmp/x'" >/dev/null 2>&1
  ok $?
  eq    "$(cat "$TMP/ssh.stdin")" "$FAKE_KEY"
  hasnt "$(cat "$TMP/ssh.log")"   "notarealkeyatall"
t_done

t "guest.sh: ssh's own 255 is explained and still returned as 255"
  PATH="$TMP/bin:$PATH" FAKE_SSH_LOG=/dev/null FAKE_SSH_RC=255 \
    "$TESTVM_DIR/guest.sh" "$GVM" true >/dev/null 2>"$TMP/guest.err"
  eq "$?" "255"
  has "$(cat "$TMP/guest.err")" "ssh's own failure code"
t_done

t "guest.sh --pull: the legacy scp protocol, and the guest side is the source"
  : > "$TMP/scp.log"
  PATH="$TMP/bin:$PATH" FAKE_SCP_LOG="$TMP/scp.log" \
    "$TESTVM_DIR/guest.sh" "$GVM" --pull /Users/admin/app.log "$TMP/app.log" >/dev/null 2>&1 # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $?
  log="$(cat "$TMP/scp.log")"
  has "$log" "-O"
  has "$log" "admin@10.0.0.9:/Users/admin/app.log" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$log" "$TMP/app.log"
t_done

t "guest.sh --push: the host side is the source, and it must exist here first"
  : > "$TMP/scp.log"
  echo hello > "$TMP/payload.txt"
  PATH="$TMP/bin:$PATH" FAKE_SCP_LOG="$TMP/scp.log" \
    "$TESTVM_DIR/guest.sh" "$GVM" --push "$TMP/payload.txt" /Users/admin/payload.txt >/dev/null 2>&1 # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $?
  has "$(cat "$TMP/scp.log")" "admin@10.0.0.9:/Users/admin/payload.txt" # machine-path-exempt: isolated test VM account/path; never a customer default

  PATH="$TMP/bin:$PATH" FAKE_SCP_LOG=/dev/null \
    "$TESTVM_DIR/guest.sh" "$GVM" --push "$TMP/not-here.txt" /Users/admin/x >/dev/null 2>"$TMP/guest.err" # machine-path-exempt: isolated test VM account/path; never a customer default
  no $? "pushing a file that is not on this Mac must refuse"
  has "$(cat "$TMP/guest.err")" "no such file on this Mac"
t_done

t "guest.sh copies literal remote paths through legacy scp's shell parse"
  : > "$TMP/scp.log"
  # $HOME is an intentional literal remote filename.
  # shellcheck disable=SC2016
  PATH="$TMP/bin:$PATH" FAKE_SCP_LOG="$TMP/scp.log" \
    "$TESTVM_DIR/guest.sh" "$GVM" --pull '/Users/admin/Application Support/$HOME' "$TMP/app.log" >/dev/null 2>&1 # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $?
  # Assert the literal escaped remote filename.
  # shellcheck disable=SC2016
  has "$(cat "$TMP/scp.log")" 'admin@10.0.0.9:/Users/admin/Application\ Support/\$HOME' # machine-path-exempt: isolated test VM account/path; never a customer default
  : > "$TMP/scp.log"
  # $HOME is an intentional literal remote filename.
  # shellcheck disable=SC2016
  PATH="$TMP/bin:$PATH" FAKE_SCP_LOG="$TMP/scp.log" \
    "$TESTVM_DIR/guest.sh" "$GVM" --push "$TMP/payload.txt" '/Users/admin/Application Support/$HOME' >/dev/null 2>&1 # machine-path-exempt: isolated test VM account/path; never a customer default
  ok $?
  # Assert the literal escaped remote filename.
  # shellcheck disable=SC2016
  has "$(cat "$TMP/scp.log")" 'admin@10.0.0.9:/Users/admin/Application\ Support/\$HOME' # machine-path-exempt: isolated test VM account/path; never a customer default
t_done

t "guest.sh: a copy with one path is refused, and names both"
  PATH="$TMP/bin:$PATH" "$TESTVM_DIR/guest.sh" "$GVM" --pull /only/one >/dev/null 2>"$TMP/guest.err"
  no $?
  has "$(cat "$TMP/guest.err")" "needs two paths"
t_done

t "guest.sh: nothing to run is refused rather than opening an interactive shell"
  # A bare `ssh host` would hand back a login shell no agent can drive, and the
  # call would simply hang.
  PATH="$TMP/bin:$PATH" "$TESTVM_DIR/guest.sh" "$GVM" >/dev/null 2>"$TMP/guest.err"
  no $?
  has "$(cat "$TMP/guest.err")" "nothing to run"
t_done

t "guest.sh: a guest with no recorded address refuses by name"
  PATH="$TMP/bin:$PATH" "$TESTVM_DIR/guest.sh" richos-test-nosuch --pull /a /b \
    >/dev/null 2>"$TMP/guest.err"
  no $?
  has "$(cat "$TMP/guest.err")" "no address for richos-test-nosuch"
t_done

t "AX search avoids history before first hit and keeps explicit matching semantics"
  python3 -B "$HERE/ax-search.test.py" >"$TMP/search.log" 2>&1; ok $? "$(cat "$TMP/search.log")"
  python3 -B "$HERE/ax-contract.test.py" >"$TMP/ax-contract.log" 2>&1; ok $? "$(cat "$TMP/ax-contract.log")"
t_done

t "hand-file: a drag of an updated file never hands over the older Desktop copy of the same name"
  node "$HERE/hand-file.test.js" >"$TMP/handfile-node.log" 2>&1; ok $? "$(cat "$TMP/handfile-node.log")"
t_done

t "pointer-drag: a left drag, a right-click (--right) with no dragged event, and the frontmost refusal"
  node "$HERE/pointer-drag.test.js" >"$TMP/pointer-drag.log" 2>&1; ok $? "$(cat "$TMP/pointer-drag.log")"
t_done

t "relay-log-check: a walk's relay log through the real relay's checks; a token, a second report or a wrong schema fails"
  node "$HERE/relay-log-check.test.mjs" >"$TMP/relay-log-check.log" 2>&1; ok $? "$(cat "$TMP/relay-log-check.log")"
t_done

t "keychain: a GUI setting that did not take is a failed prerequisite"
  out="$(STUB_KEYCHAIN_SETTINGS='lock-on-sleep timeout=300s' "$TESTVM_DIR/keychain.sh" prepare richos-test-a "$GUEST_HOME_UNDER_TEST" 2>&1)"
  no $?; has "$out" "did not retain no-timeout"
t_done

t "keychain: no-timeout does not excuse a remaining lock-on-sleep flag"
  out="$(STUB_KEYCHAIN_SETTINGS='lock-on-sleep no-timeout' "$TESTVM_DIR/keychain.sh" check richos-test-a "$GUEST_HOME_UNDER_TEST" 2>&1)"
  no $?; has "$out" "lock-on-sleep"
t_done

t "walk fixtures, receipt prerequisites, event clocks and actual-key access boundaries"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/walk.test.py" >"$TMP/walk.log" 2>&1; ok $? "$(cat "$TMP/walk.log")"
t_done

t "scenario outcomes, turn budgets, held reservation and owned timeout cleanup"
  python3 "$HERE/scenario.test.py" >"$TMP/scenario.log" 2>&1; ok $? "$(cat "$TMP/scenario.log")"
t_done

t "reserve: a CPU-admission wait is recorded for the lead's turn-end gate for exactly as long as it lasts"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/reserve-waits.test.py" >"$TMP/reserve-waits.log" 2>&1; ok $? "$(cat "$TMP/reserve-waits.log")"
t_done

t "reserve: stopping a command that ignores SIGTERM never raises, even when killpg reports EPERM or a vanished group"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/reserve-stop.test.py" >"$TMP/reserve-stop.log" 2>&1; ok $? "$(cat "$TMP/reserve-stop.log")"
t_done

t "reserve: stopping a command ends its whole group, a descendant that ignores SIGTERM included (leader exit is not group exit)"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/reserve-stop-descendant.test.py" >"$TMP/reserve-stop-descendant.log" 2>&1; ok $? "$(cat "$TMP/reserve-stop-descendant.log")"
t_done

t "run-walk --wait reaches the guest-slot admission; the default still refuses at once; hold-walk.py is retired"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/run-walk-wait.test.py" >"$TMP/run-walk-wait.log" 2>&1; ok $? "$(cat "$TMP/run-walk-wait.log")"
t_done

t "slots: two runs at once, a third waits; a slot is held only for its run and a guest never outlives it"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/slots.test.py" >"$TMP/slots.log" 2>&1; ok $? "$(cat "$TMP/slots.log")"
t_done

t "guest memory: the floor is a machine setting, the kernel leak is named, and a nightly waiting for its slot holds other admissions"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/guest-memory.test.py" >"$TMP/guest-memory.log" 2>&1; ok $? "$(cat "$TMP/guest-memory.log")"
t_done

t "slots: a caller waiting for a slot is recorded, and past ten minutes the engine's gate refuses the lead's turn"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/slots-wait-gate.test.py" >"$TMP/slots-wait-gate.log" 2>&1; ok $? "$(cat "$TMP/slots-wait-gate.log")"
t_done

t "slot-proof: a run still going at the finish deadline is ended there, its cleanup given a grace, never waited on forever"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/slot-proof-finish.test.py" >"$TMP/slot-proof-finish.log" 2>&1; ok $? "$(cat "$TMP/slot-proof-finish.log")"
t_done

t "mem-walk: a guest probe reads as used, need, swap-outs and the app's and claude's memory"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/mem-walk.test.py" >"$TMP/mem-walk.log" 2>&1; ok $? "$(cat "$TMP/mem-walk.log")"
t_done

t "slots: run.sh boots no guest outside a slot, and names the one command that does"
  : > "$TMP/slot-bundle.zip"; mkdir -p "$TMP/slot-home"
  out="$(env -u TESTVM_SLOT "$TESTVM_DIR/run.sh" --bundle "$TMP/slot-bundle.zip" --home "$TMP/slot-home" --vm richos-test-slot 2>&1)"; no $?
  has "$out" "no guest is booted outside a guest slot"
  has "$out" "run-walk.py"
  if [ -e "$TESTVM_RUN/richos-test-slot" ]; then ok 1 "a refused run.sh must leave no run state"; fi
  out="$(TESTVM_SLOT="$TESTVM_ROOT/guest.lock" "$TESTVM_DIR/run.sh" --bundle "$TMP/slot-bundle.zip" --home "$TMP/slot-home" --vm richos-test-slot 2>&1)"; no $?
  has "$out" "is not held"
t_done

t "speckle-walk: single speckle points are counted once; rules, strokes, fills and sub-delta noise are not"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/speckle-walk.test.py" >"$TMP/speckle-walk.log" 2>&1; ok $? "$(cat "$TMP/speckle-walk.log")"
t_done

t "files-since: every class, the baseline window, and a control that must be found and flagged"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/files-since.test.py" >"$TMP/files-since.log" 2>&1; ok $? "$(cat "$TMP/files-since.log")"
t_done

t "frame-probe: an all-black frame measures black, a lit one lit, an undecodable one refuses (shot.sh's black-frame gate)"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/frame-probe.test.py" >"$TMP/frame-probe.log" 2>&1; ok $? "$(cat "$TMP/frame-probe.log")"
t_done

t "steps-walk: steps run in order and are recorded, the first failure stops the walk unless allowed, an unknown step fails"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/steps-walk.test.py" >"$TMP/steps-walk.log" 2>&1; ok $? "$(cat "$TMP/steps-walk.log")"
t_done

t "steps-walk-refused: an until step whose ax.sh request is refused fails after one call, never polled to its deadline"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/steps-walk-refused.test.py" >"$TMP/steps-walk-refused.log" 2>&1; ok $? "$(cat "$TMP/steps-walk-refused.log")"
t_done

t "fill-first-walk: a walk whose guest operations or captures fail exits nonzero; a complete one exits 0 with every frame (R37)"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/fill-first-walk.test.py" >"$TMP/fill-first-walk.log" 2>&1; ok $? "$(cat "$TMP/fill-first-walk.log")"
t_done

t "fbcopy-walk: the feedback 2026-10-06 walk exits with its verdict; a dash on screen or the splash row back in the quick settings fails it"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/fbcopy-walk.test.py" >"$TMP/fbcopy-walk.log" 2>&1; ok $? "$(cat "$TMP/fbcopy-walk.log")"
t_done

t "adopt-walk: phone-only, left Registered at the boundary, each way to fail named"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/adopt-walk.test.py" >"$TMP/adopt-walk.log" 2>&1; ok $? "$(cat "$TMP/adopt-walk.log")"
t_done

t "walk-phone: the walks' phone pairs over TLS, waits for the Mac's press, sends on the chosen conversation, refuses the words"
  node --test "$HERE/walk-phone.test.mjs" >"$TMP/walk-phone.log" 2>&1; ok $? "$(cat "$TMP/walk-phone.log")"
t_done

t "ax-wait: every walk's waiting find reads a find the app did not answer (unresponsive, a deadline) as not yet, through axwait.py alone; a click is sent once"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/ax-wait.test.py" >"$TMP/ax-wait.log" 2>&1; ok $? "$(cat "$TMP/ax-wait.log")"
t_done

t "work-lease-walk: a failed keep fails the walk; after the kill, an ordinary request settles and its turn end (never called a lease close) raises no failure row; the monitor probe never counts itself or its shell"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/work-lease-walk.test.py" >"$TMP/work-lease-walk.log" 2>&1; ok $? "$(cat "$TMP/work-lease-walk.log")"
t_done

t "review-walk: the verdict reads the records; the name card is answered once by the shared helper; a worker dispatching 5 minutes stops the walk with one line naming what it waits for"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/review-walk.test.py" >"$TMP/review-walk.log" 2>&1; ok $? "$(cat "$TMP/review-walk.log")"
t_done

t "asked-again-walk: two proposals for the entity with a torn answer between; a torn line is never read as a decline"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/asked-again-walk.test.py" >"$TMP/asked-again-walk.log" 2>&1; ok $? "$(cat "$TMP/asked-again-walk.log")"
t_done

t "reap: WALL 2 admits the walk-* and probes-* clone names the harnesses create (R53)"
  # The stub-tart route is closed: reap.sh refuses an unsigned tart at its launch preflight, so
  # the wall is read from source, the way the claude-login tests above read theirs.
  wall="$(grep 'WALL 2' -A3 "$TESTVM_DIR/reap.sh" | grep 'richos-test-')"
  has "$wall" 'richos-test-*'
  has "$wall" 'walk-*'
  has "$wall" 'probes-*'
t_done

t "shot: the OCR reads the saved frame's own guest file, never a second capture (R54)"
  n="$(grep -c 'screencapture -x' "$TESTVM_DIR/shot.sh")"
  eq "$n" "1" "shot.sh takes more than one screencapture"
t_done

t "reap-walk: unrecorded pids are refused, late or surviving commands fail, a finished command is never cut short"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/reap-walk.test.py" >"$TMP/reap-walk.log" 2>&1; ok $? "$(cat "$TMP/reap-walk.log")"
t_done

t "connect-folder: no walk presses the Connected folders row as a menu item or keeps its own connect step (guest_deadline, exit 124)"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/connect-folder.test.py" >"$TMP/connect-folder.log" 2>&1; ok $? "$(cat "$TMP/connect-folder.log")"
t_done

t "command-walk: a settled task carrying the command's result passes; the 2026-09-27 'No work was started' record fails"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/command-walk.test.py" >"$TMP/command-walk.log" 2>&1; ok $? "$(cat "$TMP/command-walk.log")"
t_done

t "screen-wait-walk: the locked step fails without the turn's 'Waiting for the screen to unlock' line, and the unlock step without 'The screen is back' in its place"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/screen-wait-walk.test.py" >"$TMP/screen-wait-walk.log" 2>&1; ok $? "$(cat "$TMP/screen-wait-walk.log")"
t_done

t "skills-walk: a settled job whose worker's Skill call and page open completed and got the page back passes; the second review's hollow record fails"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/skills-walk.test.py" >"$TMP/skills-walk.log" 2>&1; ok $? "$(cat "$TMP/skills-walk.log")"
t_done

t "write-checks-walk: a refused key write and a fixed British word pass; a teammate that never wrote, a key on disk or a word left unfixed fails"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/write-checks-walk.test.py" >"$TMP/write-checks-walk.log" 2>&1; ok $? "$(cat "$TMP/write-checks-walk.log")"
t_done

t "first-send-walk: a first send after a relaunch that the ledger received passes; an emptied composer with no PromptReceived row is named swallowed"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/first-send-walk.test.py" >"$TMP/first-send-walk.log" 2>&1; ok $? "$(cat "$TMP/first-send-walk.log")"
t_done

t "hire-walk: the CEO's end-of-interview team message with a list passes; the old 'cannot hire' ending, a proposed teammate not saved, or a saved one he was not told about fails"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/hire-walk.test.py" >"$TMP/hire-walk.log" 2>&1; ok $? "$(cat "$TMP/hire-walk.log")"
t_done

t "spelling-walk: a British model reply or document that reaches him American passes; British left on a surface, his words rewritten, or nothing British to fix fail"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/spelling-walk.test.py" >"$TMP/spelling-walk.log" 2>&1; ok $? "$(cat "$TMP/spelling-walk.log")"
t_done

t "voice-walk: only a voice prompt with words is a transcript (never a re-prime), and the decoder is named by its sha"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/voice-walk.test.py" >"$TMP/voice-walk.log" 2>&1; ok $? "$(cat "$TMP/voice-walk.log")"
t_done

t "setup-walk: the boot verdict, the video tools outcome and the download timeline verdict are read from their own lines"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/setup-walk.test.py" >"$TMP/setup-walk.log" 2>&1; ok $? "$(cat "$TMP/setup-walk.log")"
t_done

t "new-user-walk: every dead end a new user hit fails its step, the frame's own words are read, and the way out passes"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/new-user-walk.test.py" >"$TMP/new-user-walk.log" 2>&1; ok $? "$(cat "$TMP/new-user-walk.log")"
t_done

t "video-watch-walk: a background download is read from the boot line, every setup-walk name it calls exists, and glued sentences are told from paths"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/video-watch-walk.test.py" >"$TMP/video-watch-walk.log" 2>&1; ok $? "$(cat "$TMP/video-watch-walk.log")"
t_done

t "dictation-walk: the expected words use stt.rs's own decode flags and annotation rule, only this payload's tool is the tool, and every step is a method"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/dictation-walk.test.py" >"$TMP/dictation-walk.log" 2>&1; ok $? "$(cat "$TMP/dictation-walk.log")"
t_done

t "permission-provider: only the exact native decision releases a request; interruption never approves"
  PYTHONDONTWRITEBYTECODE=1 python3 "$HERE/permission-provider.test.py" >"$TMP/permission-provider.log" 2>&1; ok $? "$(cat "$TMP/permission-provider.log")"
t_done

# ===========================================================================
# hand-file.sh — a FILE handed to the app, pasted or dragged, the way a person does
# ===========================================================================
# Added 2026-09-24 with the Mac composer's attachments (CEO §86). What is asserted is what
# the stub can see and what matters most: the program goes to the guest on stdin (a guest
# path with a space in it never becomes part of a command line), it is addressed to the PID
# run.sh recorded and carries the frontmost check, a refusal from the guest is a failure,
# and the arguments that would send input to the wrong place are refused before the guest
# is touched. Whether a real WKWebView takes the paste and whether Finder starts a real drag
# is the live walk's question, answered in the walk's record.
HFVM="richos-test-hf"
mkdir -p "$TESTVM_RUN/$HFVM"
echo 718 > "$TESTVM_RUN/$HFVM/app.pid"
printf '%s\n' '{"handed":"paste","file":"shot 1.png","as":"image data (public.png)","pid":718}' > "$TMP/hf-paste.json"
printf '%s\n' '{"handed":"drag","file":"a.pdf","from":{"x":1600,"y":80},"to":{"x":900,"y":640.5},"pid":718}' > "$TMP/hf-drag.json"
printf '%s\n' '{"error":"notfront","reason":"refusing: the app under test is not frontmost, so no key was sent"}' > "$TMP/hf-notfront.json"

hfrun() {  # hfrun <fixture> <args...> — run hand-file.sh against the stub guest
  local fixture="$1"; shift
  STUB_AX_FIXTURE="$fixture" STUB_AX_SCRIPT="$TMP/hf-sent.js" STUB_LOG="$TMP/hf.log" \
    "$TESTVM_DIR/hand-file.sh" "$HFVM" "$@" 2>"$TMP/hf.err"
}

t "hand-file: a paste goes on stdin, to the recorded pid, behind the frontmost check"
  : > "$TMP/hf.log"
  out="$(hfrun "$TMP/hf-paste.json" paste '/Users/admin/walk/shot 1.png')"; ok $? "$(cat "$TMP/hf.err")" # machine-path-exempt: isolated test VM account/path; never a customer default
  has   "$out" '"handed":"paste"'
  has   "$(cat "$TMP/hf-sent.js")" '"pid": 718'
  has   "$(cat "$TMP/hf-sent.js")" '"path": "/Users/admin/walk/shot 1.png"' # machine-path-exempt: isolated test VM account/path; never a customer default
  has   "$(cat "$TMP/hf-sent.js")" 'bringToFront'
  has   "$(cat "$TMP/hf-sent.js")" 'refusing: the app under test is not frontmost'
  hasnt "$(cat "$TMP/hf.log")" 'shot 1.png'
t_done

t "hand-file: the guest refusing (app not frontmost) is a failure, never a success"
  out="$(hfrun "$TMP/hf-notfront.json" paste /Users/admin/walk/a.pdf)"; no $? "a refusal must not exit 0" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$out" '"error":"notfront"'
t_done

t "hand-file: a drag carries its target into the program as numbers"
  out="$(hfrun "$TMP/hf-drag.json" drag /Users/admin/walk/a.pdf --to 900,640.5)"; ok $? "$(cat "$TMP/hf.err")" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$out" '"handed":"drag"'
  has "$(cat "$TMP/hf-sent.js")" '"tox": 900.0'
  has "$(cat "$TMP/hf-sent.js")" '"toy": 640.5'
t_done

t "hand-file: arguments that would put input in the wrong place are refused before the guest"
  : > "$TMP/hf.log"
  hfrun "$TMP/hf-paste.json" drag /Users/admin/walk/a.pdf >/dev/null; no $? # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$(cat "$TMP/hf.err")" "drag needs --to"
  hfrun "$TMP/hf-paste.json" drag /Users/admin/walk/a.pdf --to 1,2x >/dev/null; no $? # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$(cat "$TMP/hf.err")" "takes numbers"
  hfrun "$TMP/hf-paste.json" paste /Users/admin/walk/a.pdf --to 1,2 >/dev/null; no $? # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$(cat "$TMP/hf.err")" "belongs to drag"
  hfrun "$TMP/hf-paste.json" paste walk/relative.pdf >/dev/null; no $?
  has "$(cat "$TMP/hf.err")" "absolute path"
  eq "$(wc -l < "$TMP/hf.log" | tr -d ' ')" "0" "a refused argument reached the guest"
t_done

t "hand-file: the guest program runs under its own deadline, so a timed-out call cannot act later"
  : > "$TMP/hf.log"
  hfrun "$TMP/hf-paste.json" paste /Users/admin/walk/a.png >/dev/null; ok $? "$(cat "$TMP/hf.err")" # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$(cat "$TMP/hf.log")" "python3 -c"
  has "$(cat "$TMP/hf.log")" "osascript -l JavaScript -"
t_done

t "hand-file: the drag never scripts Finder (an Apple Event raises a consent dialog nobody answers)"
  eq "$(grep -v '^[[:space:]]*//' "$TESTVM_DIR/hand-file.js" | grep -c 'Application("Finder")' | tr -d ' ')" "0" "Finder is scripted"
t_done

t "hand-file: no recorded pid, no input sent"
  out="$(STUB_AX_FIXTURE="$TMP/hf-paste.json" STUB_LOG="$TMP/hf2.log" "$TESTVM_DIR/hand-file.sh" richos-test-nopid paste /Users/admin/a.png 2>&1)"; no $? # machine-path-exempt: isolated test VM account/path; never a customer default
  has "$out" "no app pid recorded"
t_done

# ===========================================================================
echo
if [ "$FAIL" -eq 0 ]; then
  echo "$PASS passed, 0 failed."
  exit 0
fi
echo "$PASS passed, $FAIL FAILED:"
for n in ${FAILED_NAMES[@]+"${FAILED_NAMES[@]}"}; do echo "  - $n"; done
exit 1

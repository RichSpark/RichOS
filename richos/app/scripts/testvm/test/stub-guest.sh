#!/usr/bin/env bash
# stub-guest.sh — a guest that is a text file.
#
#   $TESTVM_GUEST_EXEC <vm> <shell command string>
#
# `tailnet.sh` reaches its guest through ONE indirection (`tsg`), so pointing
# that at this script exercises every decision in the join — the flags, the
# order, the key handling, the refusal classification, the name read-back —
# against a machine that boots in no seconds and costs no memory.
#
# It records every command it was handed to $STUB_LOG, which is how the tests
# assert the things that matter most and are hardest to see: that the auth key
# never appears in a command line, and that it is deleted from the guest.
#
# Behavior is steered by $STUB_MODE:
#   ok             a tailnet that works
#   invalid-key    `tailscale up` refuses the key the way 1.102.4 really does
#   certs-off      joined, but the tailnet will not certify the node
#   never-running  `up` returns 0 and the daemon never reaches Running
#   no-cert        Running and certified, but `cert` hands back nothing
#   daemon-never   `tailscaled` is not running and never will be
#
# And one knob that is not a mode, because it composes with every one of them:
#   STUB_DAEMON_READY_AFTER=<n>   the daemon is ABSENT until the n-th status
#                                 read, then present. This is the guest Ray and
#                                 Echo actually met: booted, ssh answering,
#                                 Homebrew's launch daemon still coming up.
set -uo pipefail
VM="$1"; shift
CMD="$*"
STUB_LOG="${STUB_LOG:-/dev/null}"
STUB_MODE="${STUB_MODE:-ok}"
STUB_DNSNAME="${STUB_DNSNAME:-richos-test-a.tail1a2b3c.ts.net}"
# One log record per call, including multi-line supervisor source. `tr`, not
# "${CMD//$'\n'/ }": in bash 3.2 that substitution grows with the square of
# the text, and a 16 KB supervisor source took 7.4 s of CPU per call.
{ printf '%s\t' "$VM"; printf '%s' "$CMD" | tr '\n' ' '; printf '\n'; } >> "$STUB_LOG"

# A REDIRECTION INSIDE THE COMMAND STRING IS EXECUTED BY THE GUEST, so what ssh
# hands back already has it applied. Honored here, because getting it wrong
# changes what a caller is measured as having seen: `tailnet.sh` asks `up` for
# `2>&1` precisely so a failure arrives ON STDOUT and can be classified, and a
# stub that dropped the error to its own stderr would make every `up` failure
# look like a refusal with no words in it.
case "$CMD" in
  *"2>&1"*)      exec 2>&1 ;;
  *"2>/dev/null"*) exec 2>/dev/null ;;
esac

# --- the accessibility read, for ax.sh, hand-file.sh and pointer-drag.sh -----
# ax.sh sends the PROGRAM on stdin (an `AX_PARAMS_JSON = '{...}'` line followed
# by ax.py, run by the trailing `python3 -`); hand-file.sh and pointer-drag.sh
# send a JXA program to `osascript -l JavaScript -`. Each expects NDJSON back.
# The stub keeps what arrived on stdin so a test can assert what was ASKED — the
# mode, the title, the process — separately from what was answered, and answers
# with a fixture.
#   STUB_AX_SCRIPT=<path>    where to keep the program that arrived on stdin
#   STUB_AX_FIXTURE=<path>   the NDJSON to answer with
#   STUB_AX_RC=<n>           what the read exits with (default 0)
case "$CMD" in
  *"osascript -l JavaScript"*|*" python3 -")
    cat > "${STUB_AX_SCRIPT:-/dev/null}"
    [ -n "${STUB_AX_FIXTURE:-}" ] && [ -f "${STUB_AX_FIXTURE:-}" ] && cat "$STUB_AX_FIXTURE"
    exit "${STUB_AX_RC:-0}" ;;
esac

# --- is there a daemon on the other end of the socket? ----------------------
# A guest whose `tailscaled` is not up yet fails EVERY tailscale call the same
# way, `up` exactly as much as `status` — which is the whole reason the join
# raced it rather than just reading a stale answer. MEASURED wording, from Ray
# on .8 and Echo on .9: "failed to connect to local tailscaled …
# /var/run/tailscaled.socket: no such file or directory".
DAEMON_COUNT_FILE="${STUB_GUEST_FS:-/tmp}/.stub-daemon-reads"
daemon_is_absent() {
  case "$STUB_MODE" in daemon-never) return 0 ;; esac
  [ -n "${STUB_DAEMON_READY_AFTER:-}" ] || return 1
  local seen=0
  [ -f "$DAEMON_COUNT_FILE" ] && seen="$(cat "$DAEMON_COUNT_FILE")"
  case "$CMD" in
    *"tailscale status"*) seen=$((seen + 1)); printf '%s' "$seen" > "$DAEMON_COUNT_FILE" ;;
  esac
  [ "$seen" -lt "$STUB_DAEMON_READY_AFTER" ]
}
no_daemon() {
  echo "failed to connect to local tailscaled; it doesn't appear to be running (sudo systemctl start tailscaled ?)" >&2
  echo "dial unix /var/run/tailscaled.socket: connect: no such file or directory" >&2
  exit 1
}
# Only the calls that actually need the socket. `ls -l <path>` is how `doctor`
# asks whether the BINARY is there, and a missing daemon has never stopped that
# question being answerable.
case "$CMD" in
  *"tailscale up"*|*"tailscale status"*|*"tailscale cert"*|*"tailscale logout"*|*"tailscale down"*)
    daemon_is_absent && no_daemon ;;
esac

case "$CMD" in
  *".credentials.json"*)
    # The FILE store, which is the route Ray's vm9 audit named: the write takes
    # the value on stdin and the guest answers with its SIZE. `STUB_CREDENTIAL_
    # EMPTY=1` is his failure mode exactly — a store that takes the write,
    # reports success, and holds nothing.
    cred="${STUB_GUEST_FS:-/tmp}/credentials.json"
    case "$CMD" in
      *"cat > "*)
        if [ -n "${STUB_CREDENTIAL_EMPTY:-}" ]; then cat >/dev/null; : > "$cred"
        else cat > "$cred"; fi ;;
    esac
    case "$CMD" in
      *"wc -c"*) wc -c < "$cred" 2>/dev/null || echo 0 ;;
    esac
    exit 0 ;;
esac

# stdin is consumed for the commands that take it, exactly as ssh would, so a
# test can assert what was written rather than what was intended.
case "$CMD" in
  *"cat > "*)
    target="$(printf '%s' "$CMD" | sed -n "s/.*cat > '\([^']*\)'.*/\1/p")"
    mkdir -p "${STUB_GUEST_FS:-/tmp}$(dirname "$target")"
    cat > "${STUB_GUEST_FS:-/tmp}$target"
    # A copy that survives the deletion, so a test can prove the key ARRIVED
    # as well as proving it left. Without it, "the file is gone" is equally
    # true of a key that was never written.
    [ -n "${STUB_KEEP_STAGED:-}" ] && cp "${STUB_GUEST_FS:-/tmp}$target" "$STUB_KEEP_STAGED"
    exit 0 ;;
  *"rm -f "*)
    target="$(printf '%s' "$CMD" | sed -n "s/.*rm -f '\([^']*\)'.*/\1/p")"
    rm -f "${STUB_GUEST_FS:-/tmp}$target"
    exit 0 ;;
  *"tee "*)
    cat > /dev/null
    exit 0 ;;
esac

# --- the claude binary, for claude-sync.sh ----------------------------------
# The guest's copy is two facts and nothing else: its sha256 and what it says
# when asked its version. Both are FILES under $STUB_GUEST_FS when a copy has
# happened, so a test can drive the real cycle — measure, copy, MEASURE AGAIN —
# rather than a single canned answer that would make the re-measure untestable.
#   STUB_CLAUDE_SHA=<sha|"">   what the guest's binary hashes to before any copy
#   STUB_CLAUDE_VERSION=<v>    what it answers to --version before any copy
STUB_CLAUDE_STATE="${STUB_GUEST_FS:-/tmp}/claude-state"
stub_claude_sha() {
  if [ -f "$STUB_CLAUDE_STATE.sha" ]; then cat "$STUB_CLAUDE_STATE.sha"; return; fi
  printf '%s' "${STUB_CLAUDE_SHA:-}"
}
stub_claude_version() {
  if [ -f "$STUB_CLAUDE_STATE.version" ]; then cat "$STUB_CLAUDE_STATE.version"; return; fi
  printf '%s' "${STUB_CLAUDE_VERSION:-}"
}
case "$CMD" in
  *"shasum -a 256"*)
    s="$(stub_claude_sha)"
    [ -n "$s" ] || exit 1
    printf '%s\n' "$s"
    exit 0 ;;
  *"--version"*)
    v="$(stub_claude_version)"
    [ -n "$v" ] || exit 1
    printf '%s (Claude Code)\n' "$v"
    exit 0 ;;
esac

# --- the claude login, for claude-login.sh ----------------------------------
# `security -i` READS ITS COMMANDS FROM STDIN, which is the whole point of the
# mechanism: the credential is never in a command line. The stub keeps what
# arrived on stdin, separately from the command it was given, so a test can
# assert both halves of that sentence — the value IS on stdin, and it is NOT in
# the command string this guest was handed.
#   STUB_SECURITY_STDIN=<path>      where to keep what arrived on stdin
#   STUB_CLAUDE_LOGIN=present|absent  what a read-back finds
case "$CMD" in
  *"security -i"*)
    cat > "${STUB_SECURITY_STDIN:-/dev/null}"
    printf 'password has been added.\n'
    exit 0 ;;
  *"find-generic-password"*"Claude Code-credentials"*)
    if [ "${STUB_CLAUDE_LOGIN:-absent}" = "present" ]; then
      printf 'keychain: "login.keychain-db"\n    "acct"<blob>="admin"\n    "svce"<blob>="Claude Code-credentials"\n'
      exit 0
    fi
    printf 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.\n'
    exit 44 ;;
esac

# --- the keychain, for keychain.sh ------------------------------------------
# STUB_KEYCHAIN=present|absent — whether a login keychain file is at the path
# `keychain.sh` was given. `absent` is Ray's fixture on .8, and the case where
# the real `security` does not return an error but raises a dialog and waits.
STUB_KEYCHAIN="${STUB_KEYCHAIN:-present}"
case "$CMD" in
  *"test -f "*)
    [ "$STUB_KEYCHAIN" = "present" ] && exit 0
    exit 1 ;;
  *"show-keychain-info"*)
    echo "${STUB_KEYCHAIN_SETTINGS:-no-timeout}"; exit 0 ;;
  *"create-keychain"*)
    [ "$STUB_KEYCHAIN" = "present" ] && { echo "security: SecKeychainCreate: A keychain with the same name already exists."; exit 1; }
    exit 0 ;;
  *"find-generic-password"*)
    # The probe reads back what the add wrote. A stub guest is always unlocked;
    # what the tests here are about is WHICH CALLS ARE MADE and in what shape.
    echo "probe"; exit 0 ;;
  *"default-keychain -d user -s"*) exit 0 ;;
  *"default-keychain -d user"*)
    printf '    "%s"\n' "$(printf '%s' "$CMD" | sed -n "s|.*HOME='\([^']*\)'.*|\1|p")/Library/Keychains/login.keychain-db"
    exit 0 ;;
  *"list-keychains"*|*"set-keychain-settings"*|*"unlock-keychain"*) exit 0 ;;
esac

case "$CMD" in
  *"tailscale up"*)
    case "$STUB_MODE" in
      invalid-key) echo "backend error: invalid key: unable to validate API key"; exit 1 ;;
      *) echo "Success."; exit 0 ;;
    esac ;;
  *"tailscale status --json"*)
    # A guest that has logged out answers as a signed-out guest, whatever mode
    # it was in before — which is the whole thing `logout` has to verify.
    if [ -n "${STUB_LOGOUT_MARKER:-}" ] && [ -f "$STUB_LOGOUT_MARKER" ]; then
      printf '{"BackendState":"NeedsLogin","Self":{"DNSName":""},"CertDomains":null}\n'
      exit 0
    fi
    case "$STUB_MODE" in
      never-running) printf '{"BackendState":"NeedsLogin","Self":{"DNSName":""},"CertDomains":null}\n'; exit 0 ;;
      certs-off)     printf '{"BackendState":"Running","Self":{"DNSName":"%s.","Online":true,"TailscaleIPs":["100.64.0.9"]},"CertDomains":null}\n' "$STUB_DNSNAME"; exit 0 ;;
      signed-out)    printf '{"BackendState":"NeedsLogin","Self":{"DNSName":""},"CertDomains":null}\n'; exit 0 ;;
      *)             printf '{"BackendState":"Running","Self":{"DNSName":"%s.","Online":true,"TailscaleIPs":["100.64.0.9","fd7a:115c:a1e0::9"]},"CertDomains":["%s"]}\n' "$STUB_DNSNAME" "$STUB_DNSNAME"; exit 0 ;;
    esac ;;
  *"tailscale cert"*)
    case "$STUB_MODE" in
      no-cert) exit 1 ;;
      *) [ -f "${STUB_CERT_PEM:-}" ] && cat "$STUB_CERT_PEM"; exit 0 ;;
    esac ;;
  *"tailscale logout"*|*"tailscale down"*)
    # The tests point STUB_MODE at signed-out for the second status read.
    [ -n "${STUB_LOGOUT_MARKER:-}" ] && : > "$STUB_LOGOUT_MARKER"
    exit 0 ;;
  *"ls -l"*) echo "-rwxr-xr-x 1 root wheel 0 tailscale"; exit 0 ;;
  *) exit 0 ;;
esac

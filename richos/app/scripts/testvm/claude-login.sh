#!/usr/bin/env bash
# claude-login.sh — the guest is signed in to `claude` as the host is, at every
#                   run, and the value never appears anywhere but a pipe.
#
#   testvm/claude-login.sh host-check                 is THIS Mac signed in?
#   testvm/claude-login.sh push  <vm> <guest-home>    copy the login in, verify
#   testvm/claude-login.sh check <vm> <guest-home>    verify only, copy nothing
#   testvm/claude-login.sh keep  <vm> <guest-home>    for the run's lifetime: hand
#                                                     the guest each access token
#                                                     this Mac renews (run.sh starts it)
#
#   push|check|keep --account <host keychain item> [--host-folder <dir>] --guest-home <home> <vm> <guest-folder>
#       the same for a SECOND sign-in (Work): <host keychain item> is the host
#       item (`Claude Code-credentials-<sha8 of the host folder>`), <host-folder>
#       is that account's CLAUDE_CONFIG_DIR on this Mac (used only to renew it),
#       <home> is the guest fixture home (its keychain), <guest-folder> is the guest app's folder for the account. Access token only,
#       read through /usr/bin/security; the guest item is named for the GUEST folder.
#       Home's bare item and files are never touched in this mode.
#
#   WHICH ACCOUNT the primary login (push, keep and host-check without --account;
#   check reads the guest only) comes from: the account the app has IN USE, read from
#   the app's own list, $HOME/Library/Application Support/com.richos.app/
#   claude-accounts.json (TESTVM_CLAUDE_ACCOUNTS overrides the path). `inUse`
#   among `accounts`, else the first, as richos-core claude_accounts.rs
#   Accounts::in_use decides. An account with a `folder` is read from ITS host
#   item (`Claude Code-credentials-<sha8 of the folder>`) and renewed under that
#   folder; Account 1 has none and is the bare item, as before. No list at all is
#   Account 1. A list that cannot be read is a refusal, never a guess. The names
#   the GUEST looks its login up by do not change with the account.
#
# Prints ONE line on stdout:
#
#   claude login: guest logged in
#   claude login: guest NOT logged in
#
# ===========================================================================
# THE QUESTION THIS EXISTS FOR
# ===========================================================================
# The CEO, 2026-09-20: *"What happens when the Claude login expires in the
# VM?"*
#
# The honest old answer was "it never had one". `docs/testvm.md` said signing
# in was a human action — `ssh admin@<ip> 'claude /login'`, once per guest, on
# a guest that is DELETED at the end of every run. That is a human errand per
# proof, which is the same shape as the Tailscale URL sign-in this harness
# already replaced with a key.
#
# The new answer is that the guest never holds a login longer than one run: it
# is projected from the host at the start of every run and destroyed with the
# clone. ONLY the access token and its metadata cross this boundary. A copied
# refresh token lets the guest rotate the host login without sharing its lock
# or writing the replacement back. A fresh clone does not prevent that race.
# The access snapshot can expire during a run; it cannot renew the host login.
#
# ===========================================================================
# WHY THE VALUE IS NEVER ON A COMMAND LINE — AND HOW CLAUDE CODE ITSELF DOES IT
# ===========================================================================
# A secret in argv is in the process table of the machine that runs it and in
# every log that records a command line. This harness already refuses that for
# the Tailscale key (`--auth-key file:<path>`, never inline), and the same rule
# binds here.
#
# `security` has a mode for it, and Claude Code uses that mode itself. Read out
# of the 2.1.277 binary:
#
#   i = `add-generic-password -U -a "${account}" -s "${service}" -X "${hex}"\n`
#   if (i.length <= 4032) security -i   <- the command arrives on STDIN
#   else … argv …                          (their fallback; NOT taken here)
#
# So this file does the same: the payload is hex-encoded and handed to
# `security -i` on stdin, in the guest, over ssh. Measured here on 2026-09-20:
# the host's credential is 524 bytes, so the line is ~1.1 kB — comfortably
# inside the 4032 the same code treats as the limit. If it ever exceeds that,
# this REFUSES rather than taking the argv fallback.
#
# ===========================================================================
# THE NAME THE GUEST WILL LOOK UP, AND WHY TWO ARE WRITTEN
# ===========================================================================
# From the same binary, the service name is built as:
#
#   `Claude Code` + OAUTH_FILE_SUFFIX + "-credentials" + scope
#   scope = ""  when CLAUDE_CONFIG_DIR is unset
#         = "-" + sha256(configDir).hex.slice(0,8)   when it is set
#
# and the account is `process.env.USER || os.userInfo().username`.
#
# OAUTH_FILE_SUFFIX is "" in the shipped build (the non-empty variants are the
# `-local-oauth` / `-custom-oauth` development configurations). `run.sh`
# launches the app WITH `CLAUDE_CONFIG_DIR=<fixture home>/.claude`, so the name
# the app resolves is the scoped one — and a `claude` a tester starts by hand
# over ssh, with no such variable, resolves the bare one.
#
# Both are written, with the same value, into the same disposable keychain.
# Writing two rows of a throwaway keychain costs nothing; predicting which one
# the process under test will ask for, and being wrong, costs a whole walk.
#
# The ACCOUNT is the GUEST's user, never the host's: the app runs as `admin` in
# there, so an item filed under `alex` would never be found.
# ===========================================================================
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"
set +e

# Claude Code's own name for its keychain item: this, then "-<sha8 of
# CLAUDE_CONFIG_DIR>" when that is set (header). The GUEST's names are built from
# it whichever item this Mac reads, so choosing the host item never renames them.
CLAUDE_KC_PREFIX="Claude Code-credentials"
# The service and account on THIS Mac, where the CEO signed in once: Home's
# (Account 1's) bare item. Overriding it changes the host read only.
TESTVM_CLAUDE_KC_SERVICE="${TESTVM_CLAUDE_KC_SERVICE:-$CLAUDE_KC_PREFIX}"
TESTVM_HOST_CLAUDE_ACCOUNT="${TESTVM_HOST_CLAUDE_ACCOUNT:-${USER:-alex}}"
# Same default, same override name as keychain.sh — a throwaway phrase for a
# throwaway keychain in a guest that is deleted at the end of the run.
TESTVM_KEYCHAIN_PHRASE="${TESTVM_KEYCHAIN_PHRASE:-testvm-throwaway}"
# Every `security` call is bounded. Against a locked or missing keychain the
# real one raises a dialog and waits for a click — on the HOST that would be a
# window on the CEO's screen, which is the one thing this harness never does.
TESTVM_CLAUDE_LOGIN_SECONDS="${TESTVM_CLAUDE_LOGIN_SECONDS:-15}"
# `security -i`'s own stdin limit, as Claude Code's code treats it.
TESTVM_SECURITY_STDIN_MAX="${TESTVM_SECURITY_STDIN_MAX:-4032}"

VM=""; GUEST_HOME=""
CMD="${1:-}"; shift 2>/dev/null
# A second sign-in: host item, host folder (renewal only). Empty = Home.
ACCOUNT_ITEM=""; HOST_FOLDER=""; KC_HOME=""
while :; do
  case "${1:-}" in
    --account)     ACCOUNT_ITEM="${2:-}"; shift 2 2>/dev/null || die "--account needs a value" ;;
    --host-folder) HOST_FOLDER="${2:-}"; shift 2 2>/dev/null || die "--host-folder needs a value" ;;
    --guest-home)  KC_HOME="${2:-}"; shift 2 2>/dev/null || die "--guest-home needs a value" ;;
    *) break ;;
  esac
done
# The host item every host-side read names; Home's by default.
HOST_SERVICE="${ACCOUNT_ITEM:-$TESTVM_CLAUDE_KC_SERVICE}"
# Per-account run-state names, so Home's keeper state is never overwritten.
STATE_SUFFIX=""; [ -n "$ACCOUNT_ITEM" ] && STATE_SUFFIX=".account"

sha8() { printf '%s' "$1" | shasum -a 256 2>/dev/null | awk '{print substr($1,1,8)}'; }

# ===========================================================================
# THE PRIMARY LOGIN IS THE ACCOUNT THE APP HAS IN USE (2026-10-28)
# ===========================================================================
# The app moves the team to its next Claude account when one is nearly used up.
# Nightly 78: it had moved to Work, Home was at 99% of its week, and every guest
# was still signed in as Home, so the guest app stopped on "Your Claude account
# has used 99% of this week's limit". So the primary login follows the app's own
# record, the list the app keeps in its data folder (Tauri's app_data_dir, which
# run.sh and richos-core entity.rs::app_config_dir both spell as below).
TESTVM_CLAUDE_ACCOUNTS="${TESTVM_CLAUDE_ACCOUNTS:-${HOME:+$HOME/Library/Application Support/com.richos.app/claude-accounts.json}}"
IN_USE_LABEL=""

# The account in use, as three lines: id, label, folder (empty for Account 1).
# Prints nothing when there is no list (Account 1, as before the list existed).
# Returns 1 for a list the app itself would refuse to open.
in_use_account() {
  [ -n "$TESTVM_CLAUDE_ACCOUNTS" ] && [ -e "$TESTVM_CLAUDE_ACCOUNTS" ] || return 0
  python3 -I - "$TESTVM_CLAUDE_ACCOUNTS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        stored = json.load(f)
    listed = [a for a in stored["accounts"] if isinstance(a, dict) and isinstance(a.get("id"), str)]
    if not any(a["id"] == "1" for a in listed):
        raise ValueError("the account list lost Account 1")  # Accounts::open refuses it too
    # `inUse` among `accounts`, else the first: Accounts::in_use.
    a = next((a for a in listed if a["id"] == stored.get("inUse")), listed[0])
    folder = a.get("folder") or ""
    label = a.get("label") if isinstance(a.get("label"), str) else a["id"]
    # Claude Code names the item for the folder string exactly as the app passes it
    # (CLAUDE_CONFIG_DIR = folder, engine_profile.rs); the app refuses non-ASCII
    # (reset_transport.rs) rather than name a different item for the same spelling.
    if not isinstance(folder, str) or (folder and (not folder.startswith("/") or not folder.isascii()
                                                   or "\n" in folder)):
        raise ValueError("unusable folder")
    print(a["id"].replace("\n", " ")); print(label.replace("\n", " ")); print(folder)
except (OSError, ValueError, KeyError, IndexError, TypeError, AttributeError):
    sys.exit(1)
PY
}

if [ -z "$ACCOUNT_ITEM" ] && [ "$CMD" != "check" ]; then
  if ! IN_USE="$(in_use_account)"; then
    [ "$CMD" = "push" ] && printf 'claude login: guest NOT logged in\n'
    printf '[testvm] REFUSED: the app'"'"'s account list %s cannot be read, so which Claude account is in use is unknown; nothing was read or pushed rather than guess.\n' \
      "$TESTVM_CLAUDE_ACCOUNTS" >&2
    exit 5
  fi
  if [ -n "$IN_USE" ]; then
    { IFS= read -r _; IFS= read -r IN_USE_LABEL; IFS= read -r IN_USE_FOLDER; } <<EOF
$IN_USE
EOF
    if [ -n "$IN_USE_FOLDER" ]; then
      HOST_SERVICE="$CLAUDE_KC_PREFIX-$(sha8 "$IN_USE_FOLDER")"
      HOST_FOLDER="$IN_USE_FOLDER"
    else
      HOST_FOLDER=""
    fi
  fi
fi

cg() {  # one command in the guest, the harness's single indirection
  if [ -n "${TESTVM_GUEST_EXEC:-}" ]; then
    "$TESTVM_GUEST_EXEC" "$VM" "$@"
  else
    guest_ssh "$VM" "$@"
  fi
}

# The same, with something on stdin. `guest_ssh` already passes stdin through,
# so this exists to name what is happening at the call site: THE SECRET GOES
# HERE, and nowhere else.
cg_stdin() { cg "$@"; }

guest_service_scoped() {  # the name the app resolves, given its config dir
  local h
  h="$(sha8 "$1")"
  [ -n "$h" ] || return 1
  printf '%s-%s\n' "$CLAUDE_KC_PREFIX" "$h"
}

# The host's `security`, behind ONE override — the same seam `host_ts_cli` has,
# and for the same reason: so the tests can drive every decision in this file
# without reading, or risking, the CEO's real keychain.
TESTVM_HOST_SECURITY="${TESTVM_HOST_SECURITY:-/usr/bin/security}"

# Is THIS Mac signed in? Attributes only — `-w` is deliberately absent, so this
# question can be asked without the answer ever being in a variable.
host_logged_in() {
  perl -e 'alarm shift; exec @ARGV' "$TESTVM_CLAUDE_LOGIN_SECONDS" \
    "$TESTVM_HOST_SECURITY" find-generic-password \
      -s "$HOST_SERVICE" -a "$TESTVM_HOST_CLAUDE_ACCOUNT" \
    >/dev/null 2>&1
}

# ===========================================================================
# A LONG RUN OUTLIVES ONE ACCESS TOKEN (2026-09-26)
# ===========================================================================
# The guest holds an access token and never a refresh token, so it cannot renew
# anything. An access token lives 8 hours (the host's current one: written
# 11:03:15Z, expiring 19:03:15Z), Claude Code renews it once it is inside five
# minutes of expiry (`now + 300000 >= expiresAt` in 2.1.283), and walks here run
# up to 105 minutes (ray10). So a run that starts late in a token's life would
# see its model turns fail halfway with "Login expired" in the guest.
#
# The answer keeps the one rule (only THIS Mac ever renews this Mac's login, with
# its own lock and its own store) and adds two things:
#   1. push: when the token about to be handed over is inside that five-minute
#      window, or already past it, THIS Mac's own `claude` is asked to renew its
#      own login first (host_renew), and the fresh token is what crosses.
#   2. keep: for the run's lifetime, the host item's MODIFICATION TIME is read
#      every TESTVM_CLAUDE_KEEP_SECONDS (attributes only, never the value); when
#      this Mac has renewed, the new access snapshot is pushed. When the guest's
#      copy enters the window and this Mac has not renewed (an idle Mac makes no
#      model calls), the host is asked once, the same way.
# The guest's `claude` rereads its store at least every 30 s (its credential
# cache), so a pushed token is in use before the old one runs out.
TESTVM_CLAUDE_RENEW_BELOW_SECONDS="${TESTVM_CLAUDE_RENEW_BELOW_SECONDS:-300}"
TESTVM_CLAUDE_KEEP_SECONDS="${TESTVM_CLAUDE_KEEP_SECONDS:-60}"
TESTVM_CLAUDE_KEEP_MAX_SECONDS="${TESTVM_CLAUDE_KEEP_MAX_SECONDS:-43200}"
# The host renewal, behind ONE override so the suites never start the CEO's
# real `claude`. Empty means this Mac's own `claude` (host_claude_path).
TESTVM_HOST_CLAUDE_RENEW="${TESTVM_HOST_CLAUDE_RENEW:-}"

# When THIS Mac's credential item was last written: its ATTRIBUTES, never -w.
# Every /login and every renewal rewrites it, so a change means a new token.
host_written_at() {
  perl -e 'alarm shift; exec @ARGV' "$TESTVM_CLAUDE_LOGIN_SECONDS" \
    "$TESTVM_HOST_SECURITY" find-generic-password \
      -s "$HOST_SERVICE" -a "$TESTVM_HOST_CLAUDE_ACCOUNT" 2>/dev/null \
    | sed -n 's/.*"mdat"<timedate>=[^"]*"\([0-9]\{14\}\)Z.*/\1/p' | head -1
}

# Ask THIS Mac's own `claude` to renew its own login: a control-only process
# (no prompt, no tools, no settings, no MCP, no session) asks for get_usage,
# which goes through Claude Code's normal authenticated path, and Claude Code
# renews under its own lock, into its own store, when the token is inside its
# window. The process is this call's own group and is ended on every path. It
# costs no model turn. Returns 0 when it answered.
host_renew() {
  # Under the folder of the account whose item is read, and under NO folder for
  # Account 1's bare item: a caller that itself runs on another account (an
  # agent on Work has CLAUDE_CONFIG_DIR set) must not renew that one instead.
  unset CLAUDE_SECURESTORAGE_CONFIG_DIR
  if [ -n "$HOST_FOLDER" ]; then
    export CLAUDE_CONFIG_DIR="$HOST_FOLDER"
  elif [ -z "$ACCOUNT_ITEM" ]; then
    unset CLAUDE_CONFIG_DIR
  fi
  if [ -n "$TESTVM_HOST_CLAUDE_RENEW" ]; then
    "$TESTVM_HOST_CLAUDE_RENEW"
    return $?
  fi
  local bin
  bin="$(host_claude_path)"
  [ -n "$bin" ] && [ -x "$bin" ] || return 1
  python3 - "$bin" <<'PY'
import json, os, selectors, signal, subprocess, sys, time
args = ["--print", "--input-format=stream-json", "--output-format=stream-json", "--verbose",
        "--setting-sources", "", "--no-session-persistence", "--tools", "",
        "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}']
env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
p = subprocess.Popen([sys.argv[1]] + args, cwd="/", env=env, stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
ok = False
try:
    sel = selectors.DefaultSelector(); sel.register(p.stdout, selectors.EVENT_READ)
    buf = b""
    for n, sub in ((1, "initialize"), (2, "get_usage")):
        p.stdin.write((json.dumps({"type": "control_request", "request_id": "renew-%d" % n,
                                   "request": {"subtype": sub, "hooks": {}}}) + "\n").encode())
        p.stdin.flush()
        until = time.time() + 30
        answered = False
        while not answered and time.time() < until:
            if not sel.select(timeout=max(0.1, until - time.time())):
                continue
            chunk = os.read(p.stdout.fileno(), 65536)
            if not chunk:
                break
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                try:
                    v = json.loads(line)
                except ValueError:
                    continue
                r = v.get("response") if isinstance(v, dict) else None
                if isinstance(r, dict) and r.get("request_id") == "renew-%d" % n:
                    answered = True
        if not answered:
            break
        ok = n == 2
finally:
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(p.pid, sig)
        except (ProcessLookupError, PermissionError):
            break
        try:
            p.wait(timeout=5)
            break
        except subprocess.TimeoutExpired:
            continue
sys.exit(0 if ok else 1)
PY
}

# THE ONLY PLACE THE VALUE EXISTS: read from the host keychain straight into
# this process's memory, written to no file on this Mac, printed nowhere, and
# reduced to an access snapshot before anything else sees it. `-w` prints the
# value, so this pipeline is the one place in the harness that must never gain a
# `tee`, a log, or a debug echo. Sets SECRET (the snapshot) and EXPIRES_MS.
# Returns 0 for a usable snapshot, 1 when the credential cannot be read, 2 when
# it is invalid or expired.
take_snapshot() {
  SECRET=""; EXPIRES_MS=""
  local raw
  raw="$(perl -e 'alarm shift; exec @ARGV' "$TESTVM_CLAUDE_LOGIN_SECONDS" \
          "$TESTVM_HOST_SECURITY" find-generic-password \
            -s "$HOST_SERVICE" -a "$TESTVM_HOST_CLAUDE_ACCOUNT" -w 2>/dev/null \
         | perl -0777 -pe 's/\n\z//')"
  [ -n "$raw" ] || return 1
  # Drop refresh capability BEFORE either guest store is written. Use an explicit
  # field allowlist so future host credentials cannot accidentally cross as well.
  # Invalid/expired snapshots refuse provisioning; no token or parser input is logged.
  SECRET="$(printf '%s' "$raw" | python3 -c '
import json, math, sys, time
try:
    data = json.load(sys.stdin)["claudeAiOauth"]
    token = data["accessToken"]
    expiry = data["expiresAt"]
    if not isinstance(token, str) or not token:
        raise ValueError()
    if isinstance(expiry, bool) or not isinstance(expiry, (int, float)):
        raise ValueError()
    if not math.isfinite(expiry) or expiry <= time.time() * 1000:
        raise ValueError()
    fields = ("accessToken", "expiresAt", "scopes", "subscriptionType", "rateLimitTier")
    snapshot = {k: data[k] for k in fields if k in data}
    sys.stdout.write(json.dumps({"claudeAiOauth": snapshot}, separators=(",", ":")))
except (KeyError, TypeError, ValueError, OverflowError):
    sys.exit(1)
' 2>/dev/null)" || { SECRET=""; raw=""; return 2; }
  raw=""
  EXPIRES_MS="$(printf '%s' "$SECRET" | python3 -c 'import json,sys; print(int(json.load(sys.stdin)["claudeAiOauth"]["expiresAt"]))' 2>/dev/null)"
  return 0
}

guest_logged_in() {  # <service>
  local out
  out="$(cg "env HOME='$KC_HOME' perl -e 'alarm shift; exec @ARGV' $TESTVM_CLAUDE_LOGIN_SECONDS \
             security find-generic-password -a '$TESTVM_GUEST_USER' -s '$1' \
               '$KC_HOME/Library/Keychains/login.keychain-db' 2>&1")"
  case "$out" in
    *"\"svce\""*) return 0 ;;
    *) return 1 ;;
  esac
}

case "$CMD" in
  host-check)
    if host_logged_in; then
      log "host claude is logged in (keychain item '$HOST_SERVICE', account $TESTVM_HOST_CLAUDE_ACCOUNT${IN_USE_LABEL:+; the account the app has in use: $IN_USE_LABEL})"
      exit 0
    fi
    printf '[testvm] host claude is not logged in — /login on the host first\n' >&2
    exit 1
    ;;
  push|check|keep) ;;
  *) die "usage: claude-login.sh host-check | push <vm> <guest-home> | check <vm> <guest-home> | keep <vm> <guest-home>" ;;
esac

VM="${1:-}"; GUEST_HOME="${2:-}"
[ -n "$VM" ] || die "usage: claude-login.sh $CMD <vm> <guest-home>"
[ -n "$GUEST_HOME" ] || die "usage: claude-login.sh $CMD <vm> <guest-home>"
# A guest path, always — the same refusal keychain.sh makes, for the same
# reason: nothing in this harness may address the CEO's own home.
case "$GUEST_HOME" in
  "/Users/$TESTVM_GUEST_USER/"*) ;;
  *) die "refusing: '$GUEST_HOME' is not under the guest user's home (/Users/$TESTVM_GUEST_USER/)" ;;
esac

# Home: the app's config dir is <home>/.claude. A second sign-in: the guest
# folder IS the config dir.
CRED_DIR="$GUEST_HOME/.claude"; [ -n "$ACCOUNT_ITEM" ] && CRED_DIR="$GUEST_HOME"
# The guest's keychain lives in the fixture HOME, which for a second sign-in is not
# the account folder: --guest-home names it (a Home push needs none).
[ -n "$ACCOUNT_ITEM" ] && [ -z "$KC_HOME" ] && die "--account needs --guest-home <the guest's fixture home, where its keychain is>"
KC_HOME="${KC_HOME:-$GUEST_HOME}"
case "$KC_HOME" in "/Users/$TESTVM_GUEST_USER/"*) ;; *) die "refusing: '$KC_HOME' is not under the guest user's home" ;; esac
SVC_SCOPED="$(guest_service_scoped "$CRED_DIR")"
[ -n "$SVC_SCOPED" ] || die "could not derive the scoped keychain service name"

report() {  # report <logged in|NOT logged in>
  printf 'claude login: guest %s\n' "$1"
}

# The credential file's size in the guest, and nothing about its contents. A
# size is a fact that can be compared with the host's without the value ever
# being read back out of the guest — and it is precisely the check that catches
# Ray's failure mode, where a store reports success and holds nothing.
guest_credential_bytes() {
  cg "wc -c < '$CRED_DIR/.credentials.json' 2>/dev/null || echo 0" 2>/dev/null \
    | tr -d '[:space:]'
}

if [ "$CMD" = "check" ]; then
  FB="$(guest_credential_bytes)"
  if [ "${FB:-0}" -gt 1 ] 2>/dev/null; then
    report "logged in"; exit 0
  fi
  if guest_logged_in "$SVC_SCOPED" || { [ -z "$ACCOUNT_ITEM" ] && guest_logged_in "$CLAUDE_KC_PREFIX"; }; then
    report "logged in"; exit 0
  fi
  report "NOT logged in"; exit 1
fi

# --- keep ---------------------------------------------------------------------
# One process per run, started by run.sh after a successful push, pid recorded
# in the run state and ended by stop.sh by that pid. It also ends by itself when
# the run state is gone (stop.sh removes it) and after
# TESTVM_CLAUDE_KEEP_MAX_SECONDS, so a run that died without stop.sh cannot leave
# it behind for longer than that.
if [ "$CMD" = "keep" ]; then
  STATE="$TESTVM_RUN/$VM"
  [ -d "$STATE" ] || die "no run state at $STATE: keep runs only beside a run"
  BEGAN="$(date +%s)"
  LAST_WRITTEN="$(cat "$STATE/claude-login${STATE_SUFFIX}.host-written" 2>/dev/null)"
  ASKED_FOR=""
  log "keeping $VM's access token current (every ${TESTVM_CLAUDE_KEEP_SECONDS}s, until the run ends)"
  while :; do
    sleep "$TESTVM_CLAUDE_KEEP_SECONDS"
    if [ ! -d "$STATE" ]; then
      log "the run state is gone: the keeper for $VM ends"
      exit 0
    fi
    if [ $(( $(date +%s) - BEGAN )) -ge "$TESTVM_CLAUDE_KEEP_MAX_SECONDS" ]; then
      log "the keeper for $VM ends after ${TESTVM_CLAUDE_KEEP_MAX_SECONDS}s"
      exit 0
    fi
    W="$(host_written_at)"
    if [ -n "$W" ] && [ "$W" != "$LAST_WRITTEN" ]; then
      log "this Mac's login was renewed: handing $VM the new access token"
      if "$0" push ${ACCOUNT_ITEM:+--account "$ACCOUNT_ITEM"} ${HOST_FOLDER:+--host-folder "$HOST_FOLDER"} ${ACCOUNT_ITEM:+--guest-home "$KC_HOME"} "$VM" "$GUEST_HOME" >/dev/null; then
        LAST_WRITTEN="$W"
      fi
      continue
    fi
    EXP="$(cat "$STATE/claude-login${STATE_SUFFIX}.expires" 2>/dev/null)"
    case "$EXP" in *[!0-9]*|"") continue ;; esac
    LEFT=$(( EXP / 1000 - $(date +%s) ))
    if [ "$LEFT" -lt "$TESTVM_CLAUDE_RENEW_BELOW_SECONDS" ] && [ "$ASKED_FOR" != "$EXP" ]; then
      ASKED_FOR="$EXP"
      log "$VM's access token has ${LEFT}s left and this Mac has not renewed: asking this Mac's own claude to renew"
      host_renew || log "this Mac's claude did not answer the renewal request"
    fi
  done
fi

# --- push ---------------------------------------------------------------------
[ -n "$IN_USE_LABEL" ] && log "the guest's login is the app's account in use, $IN_USE_LABEL (this Mac's keychain item '$HOST_SERVICE')"
if ! host_logged_in; then
  report "NOT logged in"
  printf '[testvm] host claude is not logged in — /login on the host first\n' >&2
  exit 2
fi

take_snapshot; SNAP_RC=$?
if [ "$SNAP_RC" -eq 1 ]; then
  report "NOT logged in"
  printf '[testvm] the host credential could not be read (locked keychain, or a dialog was waiting)\n' >&2
  exit 3
fi
# Inside Claude Code's renewal window, or past it: THIS Mac renews its own
# login first, so the guest starts with a full-length token (see "A LONG RUN
# OUTLIVES ONE ACCESS TOKEN" above).
LEFT=-1
[ "$SNAP_RC" -eq 0 ] && [ -n "$EXPIRES_MS" ] && LEFT=$(( EXPIRES_MS / 1000 - $(date +%s) ))
if [ "$SNAP_RC" -ne 0 ] || [ "$LEFT" -lt "$TESTVM_CLAUDE_RENEW_BELOW_SECONDS" ]; then
  log "this Mac's access token is $([ "$SNAP_RC" -eq 0 ] && echo "${LEFT}s from expiry" || echo "expired or unusable"): asking this Mac's own claude to renew its own login first"
  host_renew || log "this Mac's claude did not answer the renewal request"
  take_snapshot; SNAP_RC=$?
fi
if [ "$SNAP_RC" -ne 0 ]; then
  SECRET=""
  report "NOT logged in"
  printf '[testvm] REFUSED: no unexpired Claude access credential; refresh the host login before retrying. No host refresh token was sent to the guest.\n' >&2
  exit 3
fi
SECRET_BYTES="${#SECRET}"
HEX="$(printf '%s' "$SECRET" | xxd -p | tr -d '\n')"

# --- THE FILE STORE, which is the route that is known to work in a guest ------
# Ray's vm9 audit, 2026-09-20: `security add-generic-password -w` reading from a
# pipe with no tty stores an EMPTY password AND EXITS 0 — a sign-in that reports
# success and is not one. That is the route this file never took (it uses
# `security -i` with `-X <hex>`, which round-trips a real value; proved on this
# host against a throwaway keychain), but his finding names the right primary:
# `claude` keeps a FILE store as well, and a file has no tty, no securityd and no
# session to be on the wrong side of.
#
# Where: `.credentials.json` inside the config dir — CLAUDE_CONFIG_DIR when set,
# `~/.claude` otherwise. `run.sh` sets it to the fixture home's `.claude`, and a
# `claude` started by hand over ssh uses the guest user's own. Both are written,
# for the same reason both keychain service names are.
#
# The value goes in on STDIN once and is copied inside the guest, so it is never
# an argument and never crosses the wire twice. `umask 077` before the write,
# because the file IS the credential.
log "writing the access snapshot into the guest's credential file (no refresh token; may expire during the run)"
COPY_HOME_DIR="\$HOME/.claude"; COPY_HOME_FILE="\$HOME/.claude/.credentials.json"
COPY_HOME_CP="cp '$CRED_DIR/.credentials.json' \$HOME/.claude/.credentials.json &&"
if [ -n "$ACCOUNT_ITEM" ]; then COPY_HOME_DIR=""; COPY_HOME_FILE=""; COPY_HOME_CP=""; fi
FILE_BYTES="$(printf '%s' "$SECRET" | cg_stdin "umask 077; \
  mkdir -p '$CRED_DIR' $COPY_HOME_DIR && \
  cat > '$CRED_DIR/.credentials.json' && \
  $COPY_HOME_CP \
  chmod 600 '$CRED_DIR/.credentials.json' $COPY_HOME_FILE && \
  wc -c < '$CRED_DIR/.credentials.json'" 2>/dev/null | tr -d '[:space:]')"

PAYLOAD="$(printf 'add-generic-password -U -a "%s" -s "%s" -X "%s" "%s"\n' \
  "$TESTVM_GUEST_USER" "$SVC_SCOPED" "$HEX" "$KC_HOME/Library/Keychains/login.keychain-db")"
# The bare name is Home's: a second sign-in never writes it.
if [ -z "$ACCOUNT_ITEM" ]; then
  PAYLOAD="$PAYLOAD
$(printf 'add-generic-password -U -a "%s" -s "%s" -X "%s" "%s"\n' \
  "$TESTVM_GUEST_USER" "$CLAUDE_KC_PREFIX" "$HEX" "$KC_HOME/Library/Keychains/login.keychain-db")"
fi

# Each line is one `security -i` command; the limit is per line, as the code
# quoted in the header treats it.
LONGEST=0
while IFS= read -r line; do
  [ "${#line}" -gt "$LONGEST" ] && LONGEST="${#line}"
done <<EOF
$PAYLOAD
EOF
if [ "$LONGEST" -gt "$TESTVM_SECURITY_STDIN_MAX" ]; then
  HEX=""
  report "NOT logged in"
  printf '[testvm] REFUSED: the credential is %s characters of hex, past security -i'"'"'s %s-character stdin limit.\n' \
    "$LONGEST" "$TESTVM_SECURITY_STDIN_MAX" >&2
  printf '         The argv fallback Claude Code takes here is not taken here: a secret on a\n' >&2
  printf '         command line is in the guest process table and in every log of it.\n' >&2
  exit 4
fi

log "copying a Claude access snapshot into $VM (no refresh token; stdin only)"
# The unlock is repeated here rather than trusted. `keychain.sh prepare` unlocked
# this keychain, but in ITS ssh session; lock state is securityd's and an
# already-unlocked keychain makes this a no-op that costs nothing. The
# alternative is a write that fails for a reason the read-back below can only
# call "not logged in".
printf '%s\n' "$PAYLOAD" | cg_stdin "env HOME='$KC_HOME' perl -e 'alarm shift; exec @ARGV' \
  $TESTVM_CLAUDE_LOGIN_SECONDS sh -c \"security unlock-keychain -p '$TESTVM_KEYCHAIN_PHRASE' \
  '$KC_HOME/Library/Keychains/login.keychain-db' >/dev/null 2>&1; security -i\"" >/dev/null 2>&1
PUSH_RC=$?
# Gone from this process the moment it is no longer needed.
HEX=""; PAYLOAD=""; SECRET=""

# Read BACK, because "the write returned 0" and "the store holds what the app
# will ask for" are two different claims — the whole lesson of this harness's
# freshness rules, and exactly the gap Ray found in the `-w` route, which
# reports success while storing nothing.
#
# The file's SIZE is compared with the host credential's, so a store that took
# the write and kept an empty value fails here instead of at the first model
# turn. The contents are never read back out of the guest.
FILE_OK=0
if [ "${FILE_BYTES:-0}" = "$SECRET_BYTES" ]; then
  FILE_OK=1
else
  GB="$(guest_credential_bytes)"
  [ "${GB:-0}" = "$SECRET_BYTES" ] && FILE_OK=1
fi
KC_OK=0
guest_logged_in "$SVC_SCOPED" && KC_OK=1

if [ "$FILE_OK" -eq 1 ] || [ "$KC_OK" -eq 1 ]; then
  log "login stored: credential file $([ "$FILE_OK" -eq 1 ] && echo "yes ($SECRET_BYTES bytes, matching the access snapshot)" || echo no), keychain item $([ "$KC_OK" -eq 1 ] && echo yes || echo no)"
  # For `keep`: WHEN the guest's copy runs out and WHICH host write it came from.
  # Two timestamps, no token.
  if [ -d "$TESTVM_RUN/$VM" ]; then
    printf '%s\n' "$EXPIRES_MS" > "$TESTVM_RUN/$VM/claude-login${STATE_SUFFIX}.expires"
    host_written_at > "$TESTVM_RUN/$VM/claude-login${STATE_SUFFIX}.host-written"
  fi
  [ -n "$EXPIRES_MS" ] && log "the guest's access token expires $(date -u -r $(( EXPIRES_MS / 1000 )) +%H:%MZ 2>/dev/null) (no refresh token; keep renews it from this Mac)"
  report "logged in"
  exit 0
fi

report "NOT logged in"
cat >&2 <<EOF
[testvm] WARNING: $VM is NOT signed in to claude (write rc=$PUSH_RC).
         Neither store took it: the credential file is ${FILE_BYTES:-0} bytes where
         the access snapshot is $SECRET_BYTES, and no keychain item came back.
         Everything that needs no model turn still renders and still
         screenshots. A model turn in the guest will answer
         "Not logged in - Please run /login".
         Most likely cause: this run's login keychain is locked or absent —
         ask it:  $HERE/keychain.sh check $VM $GUEST_HOME
EOF
exit 1

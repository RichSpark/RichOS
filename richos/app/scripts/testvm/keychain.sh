#!/usr/bin/env bash
# keychain.sh — give the guest's test user a login keychain that EXISTS and is
#               UNLOCKED, inside the fixture home the app will run against.
#
#   testvm/keychain.sh prepare <vm> <guest-home-path>
#   testvm/keychain.sh check   <vm> <guest-home-path>
#
# ===========================================================================
# THE DEFECT THIS EXISTS FOR
# ===========================================================================
# Ray, nightly `.7` defect 4 and `.8` defect 5; Echo again on `.9`. The fixture
# home's `Library/Keychains` used to be a SYMLINK to the CEO's real keychain
# directory — which was rightly removed. What replaced it is an EMPTY directory,
# and an empty directory is not a keychain:
#
#     "It is now a real empty directory, so /usr/bin/security still finds no
#      keychain and still raises the Keychain Not Found dialog. I created a
#      throwaway keychain inside the guest to finish the walk, and it then
#      prompted for its password twice more, once per pairing. On the CEO's Mac
#      the login keychain exists and is unlocked at login, so this is a fixture
#      gap and not his problem — but it costs every future walk the same detour.
#      The fixture should ship a real, unlocked, empty login keychain."
#                                                    — Ray, .8 audit, defect 5
#
# That detour is not free in the way a detour usually is. `.8` defect 2 measured
# **120 seconds of "Getting this Mac ready…"** with the `Keychain Not Found`
# dialog sitting behind the sheet, and a tester who does not know to go looking
# for that window reads it as the app hanging.
#
# ===========================================================================
# WHY THE KEYCHAIN IS MADE HERE AND NOT IN provision-guest.sh
# ===========================================================================
# It belongs to the FIXTURE HOME, and the fixture home arrives per run: run.sh
# copies it in at step 3 and the app is launched with `HOME` pointing at that
# copy. `provision-guest.sh` runs once, against the base image, and has never
# heard of a payload directory. A keychain baked into the base image would be in
# the wrong home and would be found by nothing.
#
# ===========================================================================
# WHY THE UNLOCK IS DONE IN THE GUI SESSION, AND NOT ONLY OVER ssh
# ===========================================================================
# Keychain lock state is held by `securityd`, and an ssh login is its OWN
# security session — a different one from the Aqua session `open -n -a` launches
# the app into. So an unlock performed in the ssh session is not obviously the
# unlock the app will meet. `launchctl asuser <uid>` puts the unlock in the same
# bootstrap context the app runs in, which is the one that has to be right.
#
# Both are done, and `check` reports which one the app would actually see, so
# this file states a measurement rather than a belief. Never the host's
# keychain, in any branch: every path here is prefixed by the guest's payload
# home and executed inside the guest.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"
set +e

# The password on a throwaway keychain inside a disposable guest that is deleted
# at the end of every run. It is not a secret and there is nothing behind it: the
# keychain is created empty, lives in a VM clone, and the clone is destroyed by
# `stop.sh`. It is overridable only so a caller can prove it is the value being
# used rather than a value being guessed.
TESTVM_KEYCHAIN_PHRASE="${TESTVM_KEYCHAIN_PHRASE:-testvm-throwaway}"

# Run a command in the guest. The same single indirection `tailnet.sh` uses, so
# the tests can drive every decision here against a stub instead of a guest.
kcg() {
  local vm="$1"; shift
  if [ -n "${TESTVM_GUEST_EXEC:-}" ]; then
    "$TESTVM_GUEST_EXEC" "$vm" "$@"
  else
    guest_ssh "$vm" "$@"
  fi
}

# **NO `security` CALL IN THIS FILE IS UNBOUNDED**, and the reason is measured
# rather than cautious — see `cmd_check`'s header: against a home with no
# keychain in it, `security` raises `SecurityAgent`'s `Keychain Not Found`
# dialog inside the guest and waits forever for a click nothing can give it.
# The shared Python supervisor kills the owned process group at the deadline,
# including a security command waiting behind its shell. A call that is killed produces no output, and every caller here reads
# no output as a failure.
TESTVM_KEYCHAIN_PROBE_SECONDS="${TESTVM_KEYCHAIN_PROBE_SECONDS:-20}"

# One bounded `security` call in the guest's ssh session, with this fixture
# home's HOME. Output (both streams) comes back for the caller to classify.
kc_bound() {
  local vm="$1" home="$2" cmd="$3" who="$4" remote
  remote="$(python3 - "$home" "$cmd" "$who" "$TESTVM_GUEST_USER" "$TESTVM_KEYCHAIN_PROBE_SECONDS" "$HERE/ax-deadline.py" <<'BOUND'
import pathlib, shlex, sys
home,command,who,user,seconds,source=sys.argv[1:]
args=['env','HOME='+home,'python3','-c',pathlib.Path(source).read_text(),seconds,'sh','-c','{ '+command+'\n} 2>&1']
if who=='gui':
    args=['sudo','launchctl','asuser','__GUEST_UID__','sudo','-u',user]+args
print(shlex.join(args).replace('__GUEST_UID__','$(id -u '+shlex.quote(user)+')'))
BOUND
)" || return 1
  kcg "$vm" "$remote"
}

kcs() { kc_bound "$1" "$2" "$3" ssh; }
kcg_gui() { kc_bound "$1" "$2" "$3" gui; }

# ---------------------------------------------------------------------------
# prepare
# ---------------------------------------------------------------------------
cmd_prepare() {
  local vm="${1:-}" home="${2:-}"
  [ -n "$vm" ]   || die "usage: keychain.sh prepare <vm> <guest-home-path>"
  [ -n "$home" ] || die "usage: keychain.sh prepare <vm> <guest-home-path>"
  # A guest path, always. A path that could be the host's is refused rather than
  # sanitized: the one thing this file must never do is touch the CEO's keychain,
  # and "it looked like a guest path" is not a check.
  case "$home" in
    "/Users/$TESTVM_GUEST_USER/"*) ;;
    *) die "refusing: '$home' is not under the guest user's home (/Users/$TESTVM_GUEST_USER/)" ;;
  esac

  local kc="$home/Library/Keychains/login.keychain-db"
  log "keychain: preparing an empty, unlocked login keychain at $kc"

  # `create-keychain` FAILS on a keychain that already exists, and a fixture home
  # copied in fresh each run normally has none — but a re-run against a guest
  # kept with --keep does. Existing is fine; what matters is that it ends up
  # unlocked and default.
  kcg "$vm" "mkdir -p '$home/Library/Keychains'" >/dev/null 2>&1
  local made
  made="$(kcs "$vm" "$home" "security create-keychain -p '$TESTVM_KEYCHAIN_PHRASE' '$kc'")"
  case "$made" in
    *"already exists"*) log "keychain: one was already there — reusing it" ;;
  esac

  # THE SEARCH LIST AND THE DEFAULT, both scoped `-d user`, both written into
  # this HOME's own preferences. Without the search list, `security` finds the
  # file and still reports no keychain; without the default, a caller that asks
  # for "the default keychain" — which is what the app's Security calls do —
  # gets nothing.
  kcs "$vm" "$home" "security list-keychains -d user -s '$kc'"   >/dev/null 2>&1
  kcs "$vm" "$home" "security default-keychain -d user -s '$kc'" >/dev/null 2>&1

  # The app uses the GUI session's default/search list, not an explicit file.
  # A successful SSH configuration does not establish those GUI settings.
  kcg_gui "$vm" "$home" "security list-keychains -d user -s '$kc'" || return 1
  kcg_gui "$vm" "$home" "security default-keychain -d user -s '$kc'" || return 1
  kcs "$vm" "$home" "security unlock-keychain -p '$TESTVM_KEYCHAIN_PHRASE' '$kc'" >/dev/null 2>&1 || true
  kcg_gui "$vm" "$home" "security unlock-keychain -p '$TESTVM_KEYCHAIN_PHRASE' '$kc'" || return 1
  kcg_gui "$vm" "$home" "security set-keychain-settings '$kc'" || return 1
  local settings
  settings="$(kcg_gui "$vm" "$home" "security show-keychain-info '$kc'")" || return 1
  case "$settings" in
    *no-timeout*) ;;
    *) echo "keychain: GUI session did not retain no-timeout: $settings" >&2; return 1 ;;
  esac
  case "$settings" in
    *lock-on-sleep*) echo "keychain: GUI session still locks on sleep" >&2; return 1 ;;
  esac
  echo "gui-settings=no-timeout"

  cmd_check "$vm" "$home"
}

# ---------------------------------------------------------------------------
# check — asked IN THE APP'S OWN SESSION, because that is the only answer that
#         predicts what the app will meet
# ---------------------------------------------------------------------------
# The probe is a WRITE AND A READ of a throwaway item, not `show-keychain-info`:
# use the GUI default search list and the app's normal ACL behavior. Do not
# grant all applications access with -A or specify an explicit keychain file.
# This prerequisite does not replace observing the real app keys after pairing.
#
# **EVERY PROBE IS BOUNDED, AND THAT IS NOT DEFENSIVE PROGRAMMING — IT IS THE
# DEFECT ITSELF.** Measured 2026-09-20 while testing this file: run against a
# home whose `Library/Keychains` is empty — Ray's fixture exactly —
# `security add-generic-password` DOES NOT RETURN AN ERROR. It raises
# `SecurityAgent`'s `Keychain Not Found` dialog inside the guest and waits for a
# click that no headless guest can produce. The first version of this function
# hung for over ten minutes on that case and had to be killed. That is the same
# stall Ray timed from the other side as *"120 seconds of Getting this Mac
# ready…"* (.8 defect 2) — so a diagnostic that inherits it is a diagnostic that
# reproduces the bug it is meant to report.
#
# Two things prevent it, and the first is the cheap one:
#   1. a keychain FILE that is not there is answered from `test -f`, with no
#      `security` call made at all;
#   2. anything that runs has a process-group deadline, including its children. A probe that is killed is reported as "no answer", which
#      is the truth: a keychain that needs a click is unusable to this app.

# One bounded probe, in one session. `who`: gui | ssh.
keychain_probe() {
  local vm="$1" home="$2" kc="$3" who="$4"
  local tag="richos-testvm-probe-$who"
  local inner="security add-generic-password -a richos-testvm -s $tag -w probe -U >/dev/null 2>&1; \
               security find-generic-password -a richos-testvm -s $tag -w 2>&1; \
               security delete-generic-password -a richos-testvm -s $tag >/dev/null 2>&1"
  kc_bound "$vm" "$home" "$inner" "$who" | tr -d '[:space:]'

}

cmd_check() {
  local vm="${1:-}" home="${2:-}"
  [ -n "$vm" ] && [ -n "$home" ] || die "usage: keychain.sh check <vm> <guest-home-path>"
  local kc="$home/Library/Keychains/login.keychain-db"

  echo "keychain=$kc"
  if ! kcg "$vm" "test -f '$kc'" >/dev/null 2>&1; then
    # The control case, and the one that hangs if it is asked of `security`.
    echo "default=(not asked — there is no keychain file)"
    echo "gui-session=UNUSABLE (no keychain at that path; 'security' would raise Keychain Not Found and wait)"
    echo "ssh-session=UNUSABLE (no keychain at that path)"
    return 1
  fi

  local settings
  settings="$(kcg_gui "$vm" "$home" "security show-keychain-info '$kc'")" || return 1
  case "$settings" in *no-timeout*) ;; *) echo "gui-settings=UNUSABLE ($settings)"; return 1 ;; esac
  case "$settings" in *lock-on-sleep*) echo "gui-settings=UNUSABLE (lock-on-sleep)"; return 1 ;; esac
  echo "gui-settings=no-timeout"
  local gui ssh_ default
  gui="$(keychain_probe "$vm" "$home" "$kc" gui)"
  ssh_="$(keychain_probe "$vm" "$home" "$kc" ssh)"
  default="$(kcs "$vm" "$home" "security default-keychain -d user" | tr -d '[:space:]"')"

  echo "default=$default"
  echo "gui-session=$([ "$gui" = "probe" ] && echo usable || echo "UNUSABLE (${gui:-no answer within ${TESTVM_KEYCHAIN_PROBE_SECONDS}s})")"
  echo "ssh-session=$([ "$ssh_" = "probe" ] && echo usable || echo "UNUSABLE (${ssh_:-no answer within ${TESTVM_KEYCHAIN_PROBE_SECONDS}s})")"
  # Said out loud so a green run is not read as half a failure: the ssh line is
  # a different security session from the app's and is UNUSABLE on a healthy
  # guest as often as not. `gui-session` is the verdict, and it is the one this
  # function's exit status reports.
  echo "note: gui-session is the verdict — it is the session 'open -n -a' launches the app into."
  echo "      ssh-session is informational; a different securityd session, frequently UNUSABLE by design."
  [ "$gui" = "probe" ]
}

case "${1:-}" in
  prepare) shift; cmd_prepare "$@" ;;
  check)   shift; cmd_check   "$@" ;;
  *) die "usage: keychain.sh prepare|check <vm> <guest-home-path>" ;;
esac

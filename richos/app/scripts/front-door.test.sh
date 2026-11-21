#!/usr/bin/env bash
#
# front-door.test.sh — drive the SHIPPED window's front door through AppKit, not through an
# automation protocol, and hold the one row `ui/tests` structurally cannot reach.
#
# =========================================================================================
# WHY THIS EXISTS, AND WHY IT IS NOT A CHECK IN `ui/tests`
# =========================================================================================
#
# `ui/tests/escape.js` has ten checks and thirty-nine assertions about the CEO's rule — "the
# user must always be able to close any popup of any kind by simply tapping the escape key"
# (2026-09-17, item 1) — and every one of them is green. Ray then walked the candidate-.9
# bundle and reported the settings menu surviving Escape
# (`docs/verification/2026-09-18-nightly-1.2.0-nightly.20260918.3-onscreen-audit.md` §4).
#
# The two are not in contradiction, because they are not measuring the same thing.
# `page.keyboard.press("Escape")` hands a synthesized event to the page through WebKit's
# automation protocol. The shipped window has no such path: a key travels
#
#     the keyboard (or a CGEvent)  ->  the window server  ->  NSApplication
#       ->  the key window  ->  the WKWebView as first responder  ->  the web content
#       ->  `document`'s keydown listeners
#
# and every arrow in that chain is a place the press can be lost, all of them below the layer
# `escape.js` starts at. Three suites' worth of green says the HANDLER is right; it can never
# say the key arrives. This is the only file in this repository that can.
#
# =========================================================================================
# WHAT READING THE NATIVE TREE WILL AND WILL NOT TELL YOU — measured, 2026-09-18
# =========================================================================================
#
# **System Events reports nodes for subtrees that are `display: none` in the renderer.** This
# is not a suspicion; it was measured both ways on the same build in the same hour.
#
# In the .9 window's own tree, at rest, with nothing open:
#
#     AXGroup | name=Connected repositories
#     AXGroup | name=Allow this action?
#     AXGroup | name=Quit while work is running?
#
# and in the real renderer under WebKit, at the same point in the same page:
#
#     #repositories-sheet  -> { hiddenAttr: true, display: "none" }
#     #permission-sheet    -> { hiddenAttr: true, display: "none" }
#     #quit-question       -> { hiddenAttr: true, display: "none" }
#
# (`[hidden] { display: none !important }`, `style.css:280`, which the app sets precisely
# because several of these declare their own `display`.)
#
# SO A NODE'S PRESENCE IN THAT TREE IS NOT EVIDENCE THAT IT IS ON SCREEN, and a check that
# asserts on presence will report a closed sheet as open. Ray's row 2 rested partly on "the
# tree still returns the sheet's 5 nodes"; that half of it is void, and his screenshots are
# what the row actually stands on. Every assertion below is on the node's SIZE.
#
# AND THE SIZE IS NOT 0x0 — the first draft of this file said it was, and it is not. Measured
# on the .9 window at 16:27:45Z, all three of those closed sheets report `260,130 1400x10`:
# the WEB AREA's own origin and a degenerate full-width 10pt strip. A rule that rejected only
# a zero box would have called every closed sheet in this app open, so the strip is calibrated
# from a known-closed sheet on each run (C5) and rejected by value.
#
# =========================================================================================
# THE PRECONDITIONS, DECLARED, BECAUSE THIS CHECK IS WORTHLESS WITHOUT THEM
# =========================================================================================
#
# P1  AT LEAST ONE ACTIVE DISPLAY. `gui_display_counts` in `lib/gui-launch.sh` carries the
#     whole diagnosis: macOS ACTIVE means connected, AWAKE and available for drawing, so a
#     Mac whose screens have gone to sleep reports every display ONLINE and NO display
#     ACTIVE, and the app truthfully prints `window: the runtime reported no attached
#     display`. Measured here on 2026-09-18: with the screens asleep this suite's own boot
#     produced exactly that line and `count of windows` 0.
#
# P2  THE SESSION MUST NOT BE LOCKED. This one cost a full measurement before it was found.
#     With `CGSSessionScreenIsLocked = true` the app boots, derives a real geometry on a real
#     monitor — `window: derived 1400x880 pt at (260,102) ... Monitor #30942` — and yet
#     System Events reports `windows: 0` and cannot answer "which process is frontmost" at
#     all. Synthesized keys go nowhere. A run in that state can only produce a false red on
#     every check below.
#
#         $ ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked
#         	<key>CGSSessionScreenIsLocked</key>
#         	<true/>
#
#     CEO ruling 2026-09-18 (§56) is that screen-bound work WAITS for the unlock as a state —
#     it never fails and it never nags. `--wait-for-screen <seconds>` is that wait. Without
#     it, an unmet precondition is a NAMED REFUSAL and exit 2: not a pass, not a failure of
#     the product, and never silence.
#
# P3  `cliclick`. The keys have to come from somewhere outside the app.
#
# P4  THE SCRATCH HOME MUST BE CANONICAL, AND THIS ONE IS NOT ABOUT THE MACHINE — IT IS
#     ABOUT WHAT THIS FILE PUTS ON THE PERSON'S SCREEN. This harness asks for
#     `RICHOS_ACTIVATION=regular`, which `activation.rs:72` grants and which
#     `startup_alert.rs:110-112` uses as the EXACT condition for arming the modal alert. So
#     this is the one harness in the repository that can show "RichOS could not open" to the
#     person at the Mac, and on 2026-09-18 at 16:11:56Z it did, for two minutes, because the
#     scratch HOME was built under `$TMPDIR` and `/var` is a symlink to `private/var`:
#
#         failure: application startup: Could not establish application update exclusion:
#                  home is not a canonical directory
#
#     `richos-user-update::root` (lib.rs:147) refuses a HOME that is not already its own
#     `canonicalize()`. `pwd -P` at creation makes it canonical; P4 then re-asks the same
#     question before the launch, so a future `$TMPDIR` this harness did not anticipate is a
#     named refusal with nothing on screen, rather than a dialog he has to dismiss.
#
#     THE FIRST ACCOUNT OF THIS FAILURE BLAMED THE DOUBLED SLASH IN `${TMPDIR}/...`, AND IT
#     IS WRONG — measured on rustc 1.98.0: `Path::new("/a//b") == Path::new("/a/b")` is
#     TRUE, `/private/var/…/T//x//home` canonicalizes EQUAL, and `/var/…/T/x/home` with no
#     doubled slash anywhere canonicalizes UNEQUAL. Trimming the slash alone would have
#     fixed nothing.
#
# And if a boot fails anyway, for a reason none of the four preconditions can see, the boot
# watch below ends the instance at the stderr line — which `cannot_start` writes BEFORE it
# raises the alert — rather than letting a ten-minute modal stand for the length of the run.
#
# =========================================================================================
# A BUILD HOST HAS NO BUNDLE, AND THAT IS A GAP — NOT A USAGE ERROR (exit 2, not 64)
# =========================================================================================
#
# `run-tests.sh` discovers this file from disk and runs it WITH NO ARGUMENTS, because that is
# the only way an inventory can be honest (see its header: there is no second place to edit,
# so a suite dropped beside it runs). A build host therefore invokes this suite with no
# bundle at all — and until 2026-09-18 that produced exit 64 from the argument check below,
# which the runner reads as a FAILED SUITE. It stopped the nightly build of candidate .10
# (run log `~/.richos-nightly/logs/20260918T171138Z-c26afbc2.log:2571`), and the thing it
# reported was not a defect in the product or in this file: there was simply no app to drive.
#
# The two states are different and now exit differently:
#
#   NO BUNDLE WAS NAMED          -> exit 2, "this host cannot answer". A fact about THIS
#     (neither --bundle nor         INVOCATION, not a verdict about the code. Nothing is
#      --release)                   launched, no key is synthesized, the screen is untouched.
#                                   `run-tests.sh` tolerates it only under a caller's
#                                   declaration by name WITH a reason, and goes red the day
#                                   this suite starts answering there instead.
#
#   A BUNDLE WAS NAMED AND IS    -> exit 64, unchanged. Somebody pointed this suite at a
#     NOT A RichOS.app              thing and the thing is wrong; that is an answer about
#     (or a --release with no       the argument, and no declaration may tolerate it.
#      RichOS.app.tar.gz)
#
# THE GAP IS DELIBERATELY DECIDED FIRST, ahead of P1-P4. On a build host the true and only
# interesting fact is "no bundle was named"; refusing over an asleep screen or a missing
# `cliclick` first would report the second-most-relevant thing and make the repair look like
# a machine to fix rather than a run to make by hand.
#
# AND THE HONEST LIMIT, because `run-tests.sh`'s header names it: a gap over a suite with no
# host-independent cases is a gap over silence. Every case in this file needs a window, so
# under a declaration this suite contributes NOTHING to a build. It is not a build-time check
# and never was — it is run by hand, or from a walk, against a published release.
#
# =========================================================================================
# CASES
# =========================================================================================
#
#   P0  a bundle was named at all    -> otherwise THIS HOST CANNOT ANSWER, exit 2, and
#                                        nothing is launched, sent or put on the screen
#   P1  a display is awake            -> otherwise a named refusal, exit 2
#   P2  the session is unlocked       -> otherwise a named refusal, exit 2  (or wait for it)
#   P3  cliclick is on this machine   -> otherwise a named refusal, exit 2
#   P4  the scratch HOME is canonical -> otherwise a named refusal, exit 2, NOTHING LAUNCHED
#   P5  the boot reached its window   -> otherwise the instance is ENDED AT ONCE and named,
#                                        exit 2, so no modal alert stands on his screen
#   C0  the bundle boots and puts exactly one window on screen
#   C1  POSITIVE CONTROL — a native keystroke reaches the web content at all
#   C2  POSITIVE CONTROL — the same DOCUMENT-level keydown listener that owns Escape runs on
#       a native key (Cmd-K opens the search overlay; it is three lines above the Escape arm
#       in the same listener in `main.js`)
#   C3  Escape closes the search overlay
#   C4  Escape closes the settings menu — audit-9 row 2, the row this file exists for
#   C5b the word `Enter` is a promise — Return, on the SYSTEM EVENTS path, opens the door
#       (audit-10 row 2; Ray's own measurement went through `cliclick`, which he then proved
#       does not deliver special keys to this app, and he asked for exactly this re-run)
#   C5  the tree is read by SIZE and the calibration above is re-proved on this run: a sheet
#       that is `display:none` is present and 0x0
#   Z   this run left nothing running, and the pid is named
#
# The app is QUIT through its own menu and the pid is proved gone (CEO §54 addendum 4): "Test
# app windows must always close/quit when testing is finished. Same hygiene as with any other
# garbage."
#
# Usage:
#   scripts/front-door.test.sh --bundle /path/to/RichOS.app
#   scripts/front-door.test.sh                       (no bundle named -> P0, exit 2, nothing run)
#   scripts/front-door.test.sh --release ~/.richos-nightly/releases/v1.2.0-nightly.20260918.3
#   scripts/front-door.test.sh --bundle … --wait-for-screen 900
#   scripts/front-door.test.sh --bundle … --evidence docs/verification/<dir>   (keeps the dumps)
#
# macOS only.

# run-tests: inputs richos/app/scripts/front-door.test.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/package-app.sh richos/app/src-tauri richos/app/crates richos/app/ui
# run-tests: covers richos/app/ui/main.js richos/app/src-tauri/src/activation.rs
# run-tests: host-only: it drives a published release's bundle on a person's unlocked screen with cliclick and System Events, and a build host has no such bundle (a declared gap in every build), so testvm/run-suite.sh has nothing to run it with; it is run by hand or from a walk with --release
set -uo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/gui-launch.sh
. "$APP_DIR/scripts/lib/gui-launch.sh"

BUNDLE="" RELEASE="" WAIT_FOR_SCREEN=0 EVIDENCE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) BUNDLE="${2:-}"; shift 2 || { echo "front-door.test.sh: $1 needs a value" >&2; exit 64; } ;;
    --release) RELEASE="${2:-}"; shift 2 || { echo "front-door.test.sh: $1 needs a value" >&2; exit 64; } ;;
    --wait-for-screen) WAIT_FOR_SCREEN="${2:-0}"; shift 2 || { echo "front-door.test.sh: $1 needs a value" >&2; exit 64; } ;;
    --evidence) EVIDENCE="${2:-}"; shift 2 || { echo "front-door.test.sh: $1 needs a value" >&2; exit 64; } ;;
    *) echo "front-door.test.sh: unknown argument $1" >&2; exit 64 ;;
  esac
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo "REFUSED: this drives AppKit and a WKWebView, which exist only on macOS." >&2
  exit 2
fi

# ---------------------------------------------------------------------------------------
# P0 — NO BUNDLE WAS NAMED. See the header: this is a gap, not a usage error.
# ---------------------------------------------------------------------------------------
# Decided before P1-P4 and before anything is created, launched or synthesized, so a build
# host's run of this suite is a dozen lines of output and no process. The exit-64 refusal for
# a bundle that WAS named and is wrong is further down and is untouched.
if [ -z "$BUNDLE" ] && [ -z "$RELEASE" ]; then
  echo "== The front door, through AppKit (audit-9 row 2) =="
  echo "  GAP      no bundle was named — this invocation passed neither --bundle nor --release,"
  echo "           and this suite drives the SHIPPED window: a real app, on a real screen, with"
  echo "           keys synthesized from outside it. There was nothing to drive, so NOTHING was"
  echo "           launched, no key was sent, and the screen was not touched."
  echo "           That is a fact about THIS INVOCATION, not a verdict about the code."
  echo "           Run it by hand against a published release, on an unlocked screen:"
  echo "             scripts/front-door.test.sh --release ~/.richos-nightly/releases/<version>"
  echo "           A bundle that IS named and is not a RichOS.app is a different answer: exit 64."
  exit 2
fi

PASS=0 FAIL=0
ok()   { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  FAIL  %s\n' "$*"; }
note() { printf '        %s\n' "$*"; }

# ---------------------------------------------------------------------------------------
# P1/P2/P3 — the preconditions, each with its own repair
# ---------------------------------------------------------------------------------------
screen_locked() {
  ioreg -n Root -d1 -a 2>/dev/null | grep -A1 'CGSSessionScreenIsLocked' | grep -q '<true/>'
}

echo "== The front door, through AppKit (audit-9 row 2) =="

read -r ACTIVE ONLINE <<< "$(gui_display_counts)"
VERDICT="$(gui_display_verdict "$ACTIVE")"
DISPLAY_RC=$?
if [ "$DISPLAY_RC" -ne 0 ]; then
  echo "  REFUSED  $VERDICT (online: $ONLINE)"
  echo "           Wake the screen and run this again. Nothing about the product was measured."
  exit 2
fi
note "$VERDICT"

WAITED=0
while screen_locked; do
  if [ "$WAITED" -ge "$WAIT_FOR_SCREEN" ]; then
    echo "  REFUSED  the session is LOCKED (CGSSessionScreenIsLocked = true)."
    echo "           A locked session reports 0 windows for an app that has one, cannot say"
    echo "           which process is frontmost, and delivers no synthesized keys. Every check"
    echo "           below would be a false red. Nothing about the product was measured."
    echo "           Unlock the screen, or pass --wait-for-screen <seconds> and this will wait"
    echo "           for it rather than failing (CEO 2026-09-18, §56)."
    exit 2
  fi
  [ "$WAITED" -eq 0 ] && note "the session is locked; waiting up to ${WAIT_FOR_SCREEN}s for the unlock, as a state"
  sleep 5
  WAITED=$((WAITED + 5))
done
note "the session is unlocked (waited ${WAITED}s)"

CLICLICK="$(command -v cliclick || true)"
if [ -z "$CLICLICK" ]; then
  echo "  REFUSED  cliclick is not on this machine, and the keys have to come from outside the"
  echo "           app. \`brew install cliclick\`. Nothing about the product was measured."
  exit 2
fi
note "keys from $CLICLICK"

# ---------------------------------------------------------------------------------------
# The bundle
# ---------------------------------------------------------------------------------------
# THE SCRATCH TREE IS CANONICALIZED, AND THE REASON IS THE PRODUCT'S OWN RULE.
#
# `richos-user-update`'s `root()` (crates/richos-user-update/src/lib.rs:147-149) refuses a
# HOME that is not already its own `canonicalize()`, and the app stops before its window
# exists when it does. On macOS `/var` is a symlink to `private/var` and `$TMPDIR` lives
# under it, so a scratch HOME built from `$TMPDIR` fails that check EVERY TIME.
#
# MEASURED rather than reasoned, with a five-line Rust probe on the same toolchain the app
# builds with (1.98.0), because the first account of this failure blamed the doubled slash
# `${TMPDIR}/...` produces and that account is WRONG:
#
#   Path::new("/a//b") == Path::new("/a/b")                                      ->  true
#   given /var/folders/…/T/pathprobe/home      canonical /private/var/…/home     ->  false
#   given /private/var/folders/…/T//pathprobe//home                              ->  TRUE
#
# The doubled separator is collapsed by `Path`'s component-wise equality and is harmless;
# the `/var` symlink is the whole of it. `pwd -P` resolves both, so both are gone.
TMP="$(mktemp -d "${TMPDIR:-/tmp}/richos-front-door-XXXXXX")"
TMP="$(cd "$TMP" && pwd -P)"
GUI_LAUNCHED_PIDS="$TMP/pids"
: > "$GUI_LAUNCHED_PIDS"
export GUI_LAUNCHED_PIDS
cleanup() {
  local survivors
  # THE DUMPS OUTLIVE THE RUN ONLY IF ASKED. The scratch tree is deleted either way (CEO §54:
  # garbage is always cleaned up); `--evidence <dir>` copies out the accessibility dumps and
  # the boot log FIRST, which are a few kB of text, and never the unpacked bundle or the
  # scratch HOME, which are not.
  if [ -n "$EVIDENCE" ]; then
    mkdir -p "$EVIDENCE" \
      && cp "$TMP"/ax-*.txt "$EVIDENCE/" 2>/dev/null
    [ -f "$TMP/app.log" ] && cp "$TMP/app.log" "$EVIDENCE/" 2>/dev/null
    echo "  evidence copied to $EVIDENCE"
  fi
  survivors="$(gui_reap_all)"
  [ "$survivors" -eq 0 ] || echo "  RESIDUE  $survivors process(es) survived both signals" >&2
  rm -rf "$TMP"
}
trap cleanup EXIT

if [ -n "$RELEASE" ]; then
  [ -f "$RELEASE/RichOS.app.tar.gz" ] || { echo "REFUSED: no RichOS.app.tar.gz under $RELEASE" >&2; exit 64; }
  tar -xzf "$RELEASE/RichOS.app.tar.gz" -C "$TMP" || { echo "REFUSED: the release archive would not extract" >&2; exit 64; }
  BUNDLE="$TMP/RichOS.app"
fi
if [ -z "$BUNDLE" ] || [ ! -x "$BUNDLE/Contents/MacOS/richos-tauri" ]; then
  echo "REFUSED: --bundle must name a RichOS.app with an executable in it, or --release a" >&2
  echo "         published directory holding RichOS.app.tar.gz." >&2
  exit 64
fi

HOMEDIR="$TMP/home"
mkdir -p "$HOMEDIR/Applications"

# P4 — THE SCRATCH HOME MUST PASS THE APP'S OWN RULE BEFORE THE APP IS ASKED.
#
# This harness sets `RICHOS_ACTIVATION=regular` (see the boot below) because a key window and
# a frontmost process are exactly what it is measuring. `activation.rs:72` says that forces
# the front, and `startup_alert.rs:110-112` arms the CEO-FACING MODAL on precisely
# `Presentation::Regular` and on nothing else. So this file is the one harness in the
# repository that can put "RichOS could not open" on the person's screen, and it must
# therefore prove the boot can succeed BEFORE it launches, rather than discover it after.
#
# It happened: the run at 16:11:56Z on 2026-09-18 left that alert up for two minutes and the
# CEO sent a screenshot of it.
#
# `home_rule_violation` MIRRORS `root()` (crates/richos-user-update/src/lib.rs:141-168) rather
# than approximating it — canonical, then every ancestor a directory owned by root or by me
# and not writable by anyone else unless it is root-owned AND sticky, then the home itself
# private to its owner. `stat -L` follows symlinks because Rust's `metadata()` does. It prints
# the violation and returns 0 when there is one; returns 1 when the HOME would be accepted.
home_rule_violation() {
  local home="$1" me p uid mode
  me="$(id -u)"
  case "$home" in
    /*) ;;
    *) printf 'the scratch HOME is not an absolute path: %s\n' "$home"; return 0 ;;
  esac
  if [ "$(cd "$home" && pwd -P)" != "$home" ]; then
    printf 'the scratch HOME is not canonical: %s resolves to %s (lib.rs:147)\n' \
      "$home" "$(cd "$home" && pwd -P)"
    return 0
  fi
  uid="$(stat -L -f '%u' "$home")"
  mode="$(stat -L -f '%p' "$home")"
  if [ "$uid" != "$me" ] || [ $((8#$mode & 8#22)) -ne 0 ]; then
    printf 'the scratch HOME is not private to its owner: %s is uid %s mode %s (lib.rs:130)\n' \
      "$home" "$uid" "$mode"
    return 0
  fi
  p="$(dirname "$home")"
  while :; do
    uid="$(stat -L -f '%u' "$p")"
    mode="$(stat -L -f '%p' "$p")"
    if [ "$uid" != 0 ] && [ "$uid" != "$me" ]; then
      printf 'an ancestor of the scratch HOME belongs to neither root nor me: %s is uid %s (lib.rs:156)\n' "$p" "$uid"
      return 0
    fi
    if [ $((8#$mode & 8#22)) -ne 0 ] && { [ "$uid" != 0 ] || [ $((8#$mode & 8#1000)) -eq 0 ]; }; then
      printf 'an ancestor of the scratch HOME is writable by others and not root-sticky: %s is uid %s mode %s (lib.rs:153-157)\n' "$p" "$uid" "$mode"
      return 0
    fi
    [ "$p" = "/" ] && break
    p="$(dirname "$p")"
  done
  return 1
}

if VIOLATION="$(home_rule_violation "$HOMEDIR")"; then
  echo "  REFUSED  $VIOLATION"
  echo "           richos-user-update::root refuses exactly that and the app would stop before"
  echo "           its window exists — and because this harness asks for a regular activation,"
  echo "           the person at this Mac would get a modal alert about it. Nothing was"
  echo "           launched and nothing about the product was measured."
  echo "           Point \$TMPDIR at a directory of your own that no one else can write to."
  exit 2
fi
note "scratch HOME passes the app's own rule: $HOMEDIR"

cp -a "$BUNDLE" "$HOMEDIR/Applications/RichOS.app"
EXE="$HOMEDIR/Applications/RichOS.app/Contents/MacOS/richos-tauri"
LOG="$TMP/app.log"
: > "$LOG"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$BUNDLE/Contents/Info.plist" 2>/dev/null || echo unknown)"
SOURCE="$(/usr/libexec/PlistBuddy -c 'Print :RichOSSourceCommit' "$BUNDLE/Contents/Info.plist" 2>/dev/null || echo unknown)"
note "bundle $VERSION, built from $SOURCE"

# ---------------------------------------------------------------------------------------
# The tree, read with position AND size
# ---------------------------------------------------------------------------------------
#
# THE TREE IS READ BY UNIX ID, NEVER BY NAME. `tell process "richos-tauri"` would just as
# happily read — and `set frontmost` would just as happily RAISE — the CEO's own installed
# RichOS if he has one open. Every osascript in this file is addressed at the pid this run
# launched and at nothing else.
AXDUMP="$TMP/axdump.applescript"
cat > "$AXDUMP" <<'APPLESCRIPT'
on run argv
  set thePid to (item 1 of argv) as integer
tell application "System Events"
  set procs to (every process whose unix id is thePid)
  if (count of procs) is 0 then return "windows:0" & linefeed & "ERR no process has unix id " & thePid & linefeed
  tell item 1 of procs
    set out to ""
    try
      set out to out & "windows:" & (count of windows) & linefeed
    end try
    try
      set d to entire contents of window 1
      repeat with e in d
        try
          set r to (role of e as text)
          set nm to ""
          try
            set nm to (name of e as text)
          end try
          set vl to ""
          try
            set vl to (value of e as text)
          end try
          -- THE PROPERTY IS FETCHED INTO A LOCAL LIST BEFORE IT IS INDEXED, AND THAT IS THE
          -- WHOLE OF IT. `item 1 of (position of e)` reads like an index into a list and is
          -- not: `e` is a REFERENCE into `entire contents`, so AppleScript composes one
          -- specifier — `item 1 of «class posn» of item 1 of {…}` — sends it to System
          -- Events, and System Events cannot resolve it. Every element failed that way, so
          -- every `pz` in the dump was `? ?`, so `size_of`/`on_screen`/`middle_of` were
          -- empty for every node in the window, so the door was never clicked and C1/C2/C4
          -- were red about a harness bug. Measured both forms against the .9 window in one
          -- run on 2026-09-18:
          --   A  ((item 1 of (position of e)) as text)  ->  ERR Can't make item 1 of
          --      «class posn» of item 1 of {…}          (4 of 4 elements)
          --   B  set p to position of e, then item 1 of p
          --                                             ->  260,102 1400x881   (4 of 4)
          set pz to "? ?"
          try
            set p to position of e
            set z to size of e
            set pz to ((item 1 of p) as text) & "," & ((item 2 of p) as text) & " " & ((item 1 of z) as text) & "x" & ((item 2 of z) as text)
          end try
          if nm is not "" or vl is not "" then
            set out to out & r & " | " & pz & " | name=" & nm & " | value=" & vl & linefeed
          end if
        end try
      end repeat
    on error errm
      set out to out & "ERR " & errm & linefeed
    end try
    return out
  end tell
end tell
end run
APPLESCRIPT

ax() {
  osascript "$AXDUMP" "$PID" > "$TMP/ax-$1.txt" 2>&1
  # The opening tree may contain only setup. Calibrate in the first dump
  # that includes three Settings sheets this walk never opens.
  if [ -z "$CLOSED_BOX" ]; then
    local quota accounts folders
    quota="$(size_of "$1" 'Claude Code quota' AXGroup)"
    accounts="$(size_of "$1" 'Claude accounts' AXGroup)"
    folders="$(size_of "$1" 'Connected project folders (repositories)' AXGroup)"
    if [ -n "$quota" ] && [ "$quota" = "$accounts" ] && [ "$quota" = "$folders" ]; then
      CLOSED_BOX="$quota"
    fi
  fi
}
windows_in() { head -1 "$TMP/ax-$1.txt" | sed 's/^windows://'; }
# A dump line is `role | x,y WxH | name=… | value=…`, so the NAME is field 3. It was read
# from field 4 — the value — which never equals `name=<anything>`, so `size_of`, `on_screen`
# and `middle_of` returned EMPTY for every node in every dump this file has ever taken. That
# is what made the door unclickable and `Settings` look absent from a tree it was plainly in:
#
#   $ awk -F' \| ' 'NR==69 {for(i=1;i<=NF;i++) printf "$%d=[%s]\n", i, $i}' ax-0-rest.txt
#     $1=[AXButton]  $2=[294,679 174x52]  $3=[name=Talk to Rich]  $4=[value=missing value]
#
# The node named $2 in dump $1, as "WxH", optionally restricted to role $3.
size_of() {
  awk -F' \\| ' -v want="$2" -v role="${3:-}" '
    $3 == "name=" want && (role == "" || $1 == role) { split($2, a, " "); print a[2]; exit }
  ' "$TMP/ax-$1.txt"
}
# Does dump $1 carry a node named $2 with a REAL box? THIS is "on screen".
#
# THE HEADER'S CLAIM THAT AN UNRENDERED SUBTREE IS 0x0 IS WRONG, AND IT IS WRONG IN THE
# DIRECTION THAT MATTERS. Measured on the .9 window at 16:27:45Z, at rest, nothing open:
#
#   AXGroup | 260,130 1400x10 | name=Connected repositories
#   AXGroup | 260,130 1400x10 | name=Allow this action?
#   AXGroup | 260,130 1400x10 | name=Quit while work is running?
#
# all three `display: none` in the renderer, all three reporting the WEB AREA's own origin
# (260,130) and a degenerate full-width 10pt strip — never 0x0. A rule that only rejected
# zero would have called every closed sheet in this app OPEN.
#
# So the degenerate box is not a constant typed here; `ax` measures it from three sheets
# that stay closed in the same run and `on_screen` rejects that size. On a
# build where that calibration cannot be taken the rule falls back to rejecting only zero,
# which is stricter about nothing and is said out loud at C5 rather than assumed.
CLOSED_BOX=""
on_screen() {
  local s; s="$(size_of "$@")"
  [ -n "$CLOSED_BOX" ] && [ "$s" = "$CLOSED_BOX" ] && return 1
  case "$s" in ""|0x0|0x*|*x0) return 1 ;; *) return 0 ;; esac
}
middle_of() {
  awk -F' \\| ' -v want="$2" -v role="${3:-}" '
    $3 == "name=" want && (role == "" || $1 == role) {
      split($2, a, " "); split(a[1], p, ","); split(a[2], z, "x");
      if (z[1] + 0 > 0 && z[2] + 0 > 0) { printf "%d,%d\n", p[1] + z[1] / 2, p[2] + z[2] / 2; exit }
    }' "$TMP/ax-$1.txt"
}

# ---------------------------------------------------------------------------------------
# C0 — boot, one window, and the front
# ---------------------------------------------------------------------------------------
( cd / && exec env -i HOME="$HOMEDIR" USER="${USER:-unknown}" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    RICHOS_ACTIVATION=regular "$EXE" >> "$LOG" 2>&1 ) &
PID=$!
printf '%s\n' "$PID" >> "$GUI_LAUNCHED_PIDS"

# THE BOOT WATCH ALSO WATCHES FOR THE FAILURE, AND ENDS THE INSTANCE THE MOMENT IT LANDS.
#
# `startup_alert::cannot_start` writes stderr FIRST and raises the modal AFTER
# (startup_alert.rs:222-228), and the modal's timeout is TEN MINUTES
# (`ALERT_TIMEOUT_SECONDS: f64 = 600.0`, startup_alert.rs:161). So a failing boot under this
# harness puts a dialog on the person's screen for as long as the rest of this file takes to
# run — which on 2026-09-18 was two minutes — unless something kills the process, and the
# line that says to is already in the log a tenth of a second earlier.
#
# The old loop's only exit conditions were `boot complete` and the process being GONE. A
# process sitting on a modal is neither, so it ran its full 60s and then measured a window
# that does not exist. This arm is the one that matters for the person at the Mac.
BOOT_FAILURE=""
W=0
while [ "$W" -lt 600 ]; do
  grep -q '^\[richos\] boot complete' "$LOG" 2>/dev/null && break
  BOOT_FAILURE="$(grep -m1 '^\[richos\] application startup:' "$LOG" 2>/dev/null || true)"
  [ -n "$BOOT_FAILURE" ] && break
  kill -0 "$PID" 2>/dev/null || break
  sleep 0.1
  W=$((W + 1))
done
if [ -n "$BOOT_FAILURE" ]; then
  gui_kill "$PID" >/dev/null 2>&1
  if kill -0 "$PID" 2>/dev/null; then
    echo "  RESIDUE  pid $PID survived both signals and may still be showing a modal alert." >&2
  fi
  echo "  REFUSED  the bundle stopped before its window existed, $((W / 10)).$((W % 10))s after launch:"
  echo "             $BOOT_FAILURE"
  echo "           Its alert was armed (this harness asks for a regular activation), so the"
  echo "           instance was ended at once rather than left on the person's screen. Nothing"
  echo "           about the front door was measured — this is a verdict on the boot, not on"
  echo "           Escape."
  exit 2
fi
sleep 3
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true" >/dev/null 2>&1
sleep 1
ax 0-rest
if [ "$(windows_in 0-rest)" = "1" ]; then
  ok "C0  the bundle boots and puts exactly one window on screen  (pid $PID)"
else
  bad "C0  the bundle did not put one window on screen: $(head -2 "$TMP/ax-0-rest.txt" | tr '\n' ' ')"
  note "the boot's own window line: $(grep -m1 '^\[richos\] window' "$LOG" 2>/dev/null)"
  note "NO KEY AND NO CLICK IS SENT. Everything below C0 synthesizes system-wide events, and"
  note "with no window of ours to receive them they land in whatever the person has open."
  echo
  echo "  $PASS passed, $FAIL failed"
  exit 1
fi

# ---------------------------------------------------------------------------------------
# EVERY SYNTHESIZED EVENT GOES THROUGH `send`, AND `send` PROVES ITS TARGET FIRST.
#
# `cliclick` does not type into an application. It posts events to the WINDOW SERVER, which
# delivers them to whatever is frontmost at that instant — the CEO's mail, his editor, his
# Finder. On 2026-09-18 the 16:11:56Z run's app copy had already quit, so C1's probe text was
# typed into his desktop and C2's Cmd-K opened Finder's "Connect to Server" window and left
# it standing. Both of those are this harness reaching outside the app it launched.
#
# So: immediately before every key and every click, the frontmost process is read BY UNIX ID
# and compared to the pid this run launched. By id and not by name, because a name matches
# the CEO's own installed RichOS too. A mismatch is a named refusal and the event is never
# posted — never a warning, never a retry that might land somewhere else.
# ---------------------------------------------------------------------------------------
frontmost_pid() {
  osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true' 2>/dev/null
}
frontmost_name() {
  osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null
}
send() {
  local fp
  fp="$(frontmost_pid)"
  if [ "$fp" != "$PID" ]; then
    echo "  REFUSED  the frontmost process is pid ${fp:-unknown} ($(frontmost_name)), not the"
    echo "           instance this run launched (pid $PID). cliclick posts SYSTEM-WIDE events:"
    echo "           '$*' would have gone into whatever window the person has open. It was not"
    echo "           sent. Nothing about the front door was measured."
    exit 2
  fi
  "$CLICLICK" "$@" >/dev/null 2>&1
}

# ---------------------------------------------------------------------------------------
# ESCAPE IS SENT BY A DIFFERENT MECHANISM, AND THAT IS A MEASUREMENT, NOT A PREFERENCE.
#
# `cliclick kp:esc` DOES NOT REACH THIS APP. Measured against the .9 window on 2026-09-18,
# one launch, each step's tree dumped:
#
#   A  Cmd-K   by cliclick (kd:cmd t:k ku:cmd)   ->  search overlay OPEN
#   B  Escape  by cliclick (kp:esc)              ->  search overlay STILL OPEN
#   C  Escape  by System Events (key code 53)    ->  search overlay CLOSED
#
# Same window, same second, same frontmost process. cliclick's typing and its Cmd-K arrive
# and its Escape does not, so an Escape sent that way can only ever produce a false red — and
# it did: the 16:30:15Z run reported "Escape did not close the search overlay" and the run
# before it reported C4 red, both about a key this harness never delivered. That is the same
# shape as the row this file was written to settle, one layer further out.
#
# `key code 53` is still a synthesized key that travels window server -> NSApplication -> key
# window -> WKWebView -> document, which is the chain this file exists to exercise; it is not
# WebKit's automation protocol and nothing here talks to the page directly. It is NOT a
# person's finger either, and C3 below is what keeps that honest: if the Escape this harness
# sends stops arriving, C3 goes red and C4's verdict is void rather than wrong.
send_key() {
  local fp
  fp="$(frontmost_pid)"
  if [ "$fp" != "$PID" ]; then
    echo "  REFUSED  the frontmost process is pid ${fp:-unknown} ($(frontmost_name)), not the"
    echo "           instance this run launched (pid $PID). The key '$2' was not sent."
    exit 2
  fi
  osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to key code $1" >/dev/null 2>&1
}

# ---------------------------------------------------------------------------------------
# C5 — the calibration, re-proved on THIS run rather than quoted from the header
# ---------------------------------------------------------------------------------------
# C5 is reported at the desk, after the first-run overlays have been cleared.

# ---------------------------------------------------------------------------------------
# C5b — THE WORD `Enter` IS A PROMISE. DOES THE KEY KEEP IT, ON A REAL WINDOW? — audit-10 row 2
# ---------------------------------------------------------------------------------------
#
# Ray, candidate .10 §4: "With the window frontmost I pressed Return on the opening screen at
# 18:51:32. Nothing. The word `Enter` is clickable with a mouse — I clicked it at 18:51:43 and
# it let me in — and that is the only way it works." And then, to his credit, the caveat:
# "that Return went through `cliclick`, which I later proved unreliable for this app ... the
# keyboard half of it deserves a re-run on the System Events path."
#
# IT DOES, AND THIS IS THE ONLY PLACE IN THIS REPOSITORY THAT CAN GIVE IT ONE. `ui/tests`
# holds the handler green — `home.js`'s check "the word 'Enter' is a promise the key keeps,
# from anywhere on the screen" asserts it and passes — and that suite hands keys to the page
# through WebKit's automation protocol, a path the shipped window does not have. Three suites'
# worth of green says the HANDLER is right and can never say the key ARRIVES. That is this
# file's whole thesis, applied to a second row.
#
# IT COSTS THE RUN NOTHING, which is why it goes here rather than in a case of its own. The
# harness has to leave the opening screen anyway, and it has always done that with a CLICK
# precisely because key delivery was still in doubt. So: press Return first, look, and fall
# back to the click. A run in which the key works reports it; a run in which it does not still
# reaches every case below.
#
# C1 IS WHAT KEEPS THIS HONEST IN THE OTHER DIRECTION. If the System Events path itself stops
# arriving, C1 goes red and this verdict is void rather than wrong — the same relationship C4
# already has with C3.
# THE DOOR IS IDENTIFIED BY ITS BOX, NOT BY ITS NAME, AND THE FIRST DRAFT OF THIS CASE PROVED
# WHY. Run against the .5 bundle on 2026-09-18 it reported "the door is still on screen at
# 40x40" — and the door is 174x52 (this file's own `size_of` example measures it). 40x40 is the
# DESK's own `Settings` box, so a SECOND node carrying this name exists once the home screen
# has gone, and a name-only test reads the door as still up on a run where it opened. The home
# screen's door has one box, taken from `0-rest`; anything else by that name is another control.
#
# AND IT SPENDS ONE TREE DUMP, NOT THREE, WHICH IS ALSO MEASURED RATHER THAN TIDINESS. The
# first draft took its own before-dump and its own fallback-dump, added about six seconds
# before `drain_first_run`, and the first-run company question then surfaced DURING C1 — so C1
# went red on the .5 bundle in both runs of that draft while the pristine file was 7-for-7 on
# the same bundle in the same hour. A case that costs the run a positive control is a case that
# has broken the suite to ask its question. `0-rest` is already taken above and `0c` serves both
# the verdict and the fallback.
DOOR_BOX="$(size_of 0-rest 'Talk to Rich')"
if on_screen 0-rest 'Talk to Rich'; then
  send_key 36 return
  sleep 2
  ax 0c-after-return
  AFTER_BOX="$(size_of 0c-after-return 'Talk to Rich')"
  if [ "$AFTER_BOX" = "$DOOR_BOX" ]; then
    bad "C5b the word 'Enter' is on the opening screen and Return did not open the door"
    note "    the door measured $DOOR_BOX before the key and $AFTER_BOX after it — unchanged —"
    note "    with a System Events Return (key code 36), NOT cliclick. audit-10 row 2 reproduces"
    note "    on the reliable key path, and the caption is a promise the window does not keep."
  else
    ok "C5b Return on the opening screen opened the door — the caption is a promise kept"
    note "    the door measured $DOOR_BOX before the key; after it the name measures"
    note "    ${AFTER_BOX:-nothing} (the desk carries a control of its own by that name). Sent"
    note "    with System Events key code 36, the path C2 and C3 prove arrives on this run."
  fi
else
  note "C5b the door was not on screen before the key, so Return was never put to it. Nothing"
  note "    about audit-10 row 2 was measured on this run."
  ax 0c-after-return
fi

# ---------------------------------------------------------------------------------------
# Leave the opening screen by CLICKING the door at its own box — the FALLBACK now, because
# C5b above has already tried the key. Skipped when the key already got us in.
# ---------------------------------------------------------------------------------------
if [ "$(size_of 0c-after-return 'Talk to Rich')" = "$DOOR_BOX" ] && [ -n "$DOOR_BOX" ]; then
  DOOR="$(middle_of 0c-after-return 'Talk to Rich')"
  if [ -n "$DOOR" ]; then
    send "c:$DOOR"
    sleep 2
  fi
fi

# ---------------------------------------------------------------------------------------
# WebKit can expose the background Settings button while Search is open.
# The overlay's Search combobox is distinct from the rail's search button.
search_open() { on_screen "$1" 'Search' AXComboBox; }
desk_open() {
  on_screen "$1" 'Settings' AXPopUpButton &&
    ! search_open "$1" && ! on_screen "$1" "$FIRST_RUN" AXGroup &&
    ! on_screen "$1" "There's a bit of setting up to do first." AXGroup &&
    ! on_screen "$1" 'Where should I keep what you tell me?' AXGroup
}

# ---------------------------------------------------------------------------------------
# A SCRATCH HOME IS A FIRST RUN, AND A FIRST RUN ASKS A QUESTION — cleared BEFORE the checks
# rather than in the middle of them.
#
# The company question is modal and takes the rest of the page out of the tree. It does not
# arrive with the desk; it surfaces a few seconds later, which at 16:35:36Z put it on screen
# between C2 and C3 and made two assertions read the wrong state. It is waited for, then
# cleared with ESCAPE — that is the CEO's rule for it too, and `setup.js` case 14 holds that
# Escape there is equivalent to its Not now button. The button is the fallback, so a failure
# to dismiss it cannot silently cost the row this file exists for.
# ---------------------------------------------------------------------------------------
FIRST_RUN='Which company is this copy of Rich for?'
# A FIRST RUN IS A QUEUE OF QUESTIONS, NOT ONE QUESTION, AND CLOSING ONE RAISES THE NEXT.
# `main.js:7294` defers the company question behind the setup and memory ones and
# `closeMemorySetup`/`closeSetup` ask it "the moment that one is answered"
# (main.js:5250-5256, 4710-4717). So a single Escape on a fresh HOME does not return the
# desk — it advances the queue, which is exactly what made the 16:41:22Z run read
# `Which company is this copy of Rich for?` in the dump it took right after C3's Escape and
# conclude the desk never came back.
#
# The queue is therefore DRAINED before any check, and Escape is what drains it (the CEO's
# rule applies to every one of them). A named button is the fallback for any question Escape
# will not close, so a first-run dialog that ignores the key cannot silently cost the row
# this file exists for — it is reported and stepped over.
drain_first_run() {
  local tag="$1" deadline=$((SECONDS + 120)) n=0 before after
  while [ "$SECONDS" -lt "$deadline" ] && [ "$n" -lt 8 ]; do
    ax "$tag-settle-$n"
    # A fresh home has no company. Dismissing its picker leaves the composer
    # disabled, so typing cannot be a key-delivery control until one is chosen.
    # Create only a synthetic company in this run's isolated home, through the UI.
    if on_screen "$tag-settle-$n" "$FIRST_RUN"; then
      local field add
      field="$(middle_of "$tag-settle-$n" "What's the company called?" AXTextField)"
      add="$(middle_of "$tag-settle-$n" 'Add this company')"
      if [ -n "$field" ] && [ -n "$add" ]; then
        send "c:$field" kd:cmd t:a ku:cmd 't:Front door fixture'
        send "c:$add"
        sleep 1.5
        n=$((n + 1))
        continue
      fi
    fi
    if desk_open "$tag-settle-$n"; then
      sleep 3
      ax "$tag-settle-${n}b"
      if desk_open "$tag-settle-${n}b"; then
        [ "$n" -gt 0 ] && note "the first-run queue is drained after $n Escape(s); the desk is clear"
        return 0
      fi
      n=$((n + 1))
      continue
    fi
    before="$(wc -l < "$TMP/ax-$tag-settle-$n.txt")"
    send_key 53 Escape
    sleep 1.5
    ax "$tag-settle-${n}esc"
    after="$(wc -l < "$TMP/ax-$tag-settle-${n}esc.txt")"
    if [ "$before" = "$after" ] && cmp -s "$TMP/ax-$tag-settle-$n.txt" "$TMP/ax-$tag-settle-${n}esc.txt"; then
      note "a first-run question did not change under Escape; trying its own button"
      for label in 'Not now' 'Close' 'Skip' 'Later'; do
        BTN="$(middle_of "$tag-settle-${n}esc" "$label")"
        if [ -n "$BTN" ]; then
          note "  pressing '$label'"
          send "c:$BTN"
          sleep 1.5
          break
        fi
      done
    fi
    n=$((n + 1))
  done
  return 1
}
if drain_first_run 1a; then
  ax 1-desk
else
  ax 1-desk
  note "the first-run questions could not be cleared within the deadline; a check below that"
  note "finds one on screen will say so rather than blame the key."
fi
if [ -n "$CLOSED_BOX" ]; then
  ok "C5  the three closed Settings sheets agree on the ${CLOSED_BOX} box"
else
  note "C5  the three closed Settings sheets could not be measured in the same dump."
fi

# ---------------------------------------------------------------------------------------
# C1 — a native keystroke reaches the web content AT ALL
# ---------------------------------------------------------------------------------------
PROBE="front door probe $$"
COMPOSER="$(middle_of 1-desk 'Message to Rich')"
if [ -n "$COMPOSER" ]; then send "c:$COMPOSER"; fi
send "t:$PROBE"
sleep 1
ax 2-typed
if grep -qF "$PROBE" "$TMP/ax-2-typed.txt"; then
  ok "C1  positive control — a native keystroke reaches the web content"
else
  bad "C1  positive control FAILED — nothing typed from outside the app reached the page"
  note "Every check below this line is about a key that never arrived, so they say nothing"
  note "about the product. Fix the control before reading them."
fi

# AND THE QUEUE IS DRAINED AGAIN HERE, because C1's own typing is what raises it. Waiting for
# it before C1 does not work and was measured not to: the 16:41:22Z run polled the tree for
# 75 s with the desk clear and never saw the company question, then found it on screen four
# seconds after C1 typed into the composer. The composer is blocked until a company is
# chosen (`requireCompanyChoice`, main.js:4421), so putting text in it is the trigger. A
# second drain is cheap and it is the only placement that holds.
drain_first_run 2a || note "a first-run question is still up after C1; the checks below will say so"

# ---------------------------------------------------------------------------------------
# C2 — the SAME document-level keydown listener that owns Escape, on a native key
# ---------------------------------------------------------------------------------------
# A LINE COUNT WAS THE WRONG INSTRUMENT AND IT LIED IN BOTH DIRECTIONS. The search overlay
# is modal, so opening it takes the REST of the page out of the tree: the .9 window's dump
# went from 66 lines to 17 when Cmd-K landed. "More lines than before" therefore reported a
# working Cmd-K as broken, and "no more lines than before" reported a search overlay that
# was still plainly on screen as closed — a false PASS on C3 at 16:27:45Z, where `ax-4-esc`
# was byte-identical to `ax-3-cmdk`. Every assertion here names its node and reads its box.
send kd:cmd t:k ku:cmd
sleep 1.5
ax 3-cmdk
if search_open 3-cmdk; then
  ok "C2  positive control — Cmd-K opened the Search overlay"
  note "    Search input measures $(size_of 3-cmdk 'Search' AXComboBox)"
else
  bad "C2  positive control FAILED — Cmd-K opened nothing, so that listener is not running on"
  note "native keys. Escape is three lines below Cmd-K in the SAME listener, so C4 cannot"
  note "distinguish a broken handler from a key that never arrived."
  note "Search measures '$(size_of 3-cmdk 'Search')', Settings measures '$(size_of 3-cmdk 'Settings')'"
fi

# ---------------------------------------------------------------------------------------
# C3 — Escape closes what Cmd-K opened. THIS IS ALSO THE POSITIVE CONTROL FOR THE ESCAPE
# KEY ITSELF: C4 below is only meaningful while this one is green.
# ---------------------------------------------------------------------------------------
send_key 53 Escape
sleep 1.5
ax 4-esc
# Closing Search can uncover a setup prompt. Measure the Search input itself.
if search_open 3-cmdk && ! search_open 4-esc; then
  ok "C3  positive control — Escape closed the Search overlay"
else
  bad "C3  Escape did not close the search overlay — the key did not arrive, or the handler"
  note "did not act. Either way C4 below cannot tell a product defect from a dead key, so"
  note "read it as void rather than as a verdict."
  note "Search measured '$(size_of 3-cmdk 'Search')' open and '$(size_of 4-esc 'Search')' after Escape"
fi

# The queue advanced under the overlay, so it is drained once more before the row itself.
drain_first_run 3a || note "a first-run question is still up; C4 below will say so"

# ---------------------------------------------------------------------------------------
# C4 — THE ROW. The settings menu, opened by its own control, closed by Escape.
# ---------------------------------------------------------------------------------------
# THE SETTINGS CONTROL IS LOCATED IN THE CURRENT DUMP, NEVER IN `0-rest`. `0-rest` is the
# OPENING SCREEN, four states ago; clicking a box read off it is clicking where a control
# used to be. At 16:30:15Z that put the click under a search overlay that was still up, and
# C4 reported "the settings menu did not open" about a click that never reached it.
ax 4b-before-settings
SET="$(middle_of 4b-before-settings 'Settings' AXPopUpButton)"
if ! desk_open 4b-before-settings; then
  bad "C4  Settings was unavailable after leaving Search and clearing setup"
elif [ -z "$SET" ]; then
  bad "C4  the settings control has no box in the tree, so the menu could not be opened"
  note "the tree carries $(wc -l < "$TMP/ax-4b-before-settings.txt" | tr -d ' ') nodes; the first-run question is $(on_screen 4b-before-settings "$FIRST_RUN" && echo 'ON SCREEN' || echo 'not up')"
else
  send "c:$SET"
  sleep 1.5
  ax 5-setmenu
  if ! on_screen 5-setmenu 'Technical view'; then
    bad "C4  the settings menu did not open, so Escape was never put to it"
    note "Technical view measures '$(size_of 5-setmenu 'Technical view')'"
  else
    OPEN_SIZE="$(size_of 5-setmenu 'Technical view')"
    send_key 53 Escape
    sleep 1.5
    ax 6-setmenu-esc
    AFTER="$(size_of 6-setmenu-esc 'Technical view')"
    [ -n "$AFTER" ] || AFTER="absent"
    if on_screen 6-setmenu-esc 'Technical view'; then
      bad "C4  the settings menu survived Escape — audit-9 row 2 reproduces"
      note "Technical view measured $OPEN_SIZE open and $AFTER after Escape"
    else
      ok "C4  Escape closed the settings menu  (Technical view $OPEN_SIZE -> $AFTER)"
    fi
  fi
fi

# ---------------------------------------------------------------------------------------
# Z — the instance is closed when the test ends (CEO §54 addendum 4)
#
# BY PID, AND NEVER BY A COMMAND KEYSTROKE. Until 2026-09-19 this line was
#
#   osascript -e 'tell application "System Events" to tell (first process whose unix id
#                 is $PID) to keystroke "q" using command down'
#
# and on that day it quit the CEO's Terminal. The `tell process` wrapper reads as though
# it addresses the keystroke, and it does not: System Events' `keystroke` is delivered to
# whatever application is FRONTMOST at that instant, whatever process the enclosing block
# names. Every other osascript in this file addresses an ELEMENT of the process (line 418's
# own note), which genuinely is targeted; a keystroke is the one form that is not. So a
# run whose window had lost the front -- the app already gone, a Space switch, anything at
# all -- sent ⌘Q to a bystander. `send_key` (line 644) is the same shape and is safe only
# because Escape and Return do nothing to a bystander; a Command chord is never safe.
#
# `gui_kill` is TERM, then KILL, then proof that the pid is gone (lib/gui-launch.sh:513).
# It cannot reach a process this run did not start, which is the whole property wanted
# here. The app's own Quit path is exercised by gui-boot.test.sh, from its own harness.
# ---------------------------------------------------------------------------------------
gui_kill "$PID" >/dev/null 2>&1
if kill -0 "$PID" 2>/dev/null; then
  bad "Z   pid $PID SURVIVED both signals — there is a window of this test still on his screen"
else
  ok "Z   pid $PID is gone; nothing this run started is still running"
fi

echo
echo "  evidence: $TMP  (removed on exit; copy it out of here if a check went red)"
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

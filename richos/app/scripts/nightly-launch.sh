#!/usr/bin/env bash
#
# nightly-launch.sh — run a published nightly beside your own copy of RichOS, in its
#                     own folder, without touching your own copy or its data.
#
#   nightly-launch.sh a|b|c <RichOS-<version>-macos-aarch64.zip> [--replace] [--roster <page.md>] [--engine <engine.tar.gz>]
#   nightly-launch.sh a|b|c --again [--roster <page.md>]
#   nightly-launch.sh status
#
#   a  ~/myrichos-nightly-a   a newborn: empty data, the experience a customer gets
#   b  ~/myrichos-nightly-b   the same nightly on data carrying the team roster page
#   c  ~/myrichos-nightly-c   a newborn for a build that is not published yet (with --engine)
#
# =========================================================================================
# WHY THIS EXISTS
# =========================================================================================
#
# The CEO, 2026-09-17 (ruling §48): he tests brand-new nightly versions in their own
# folders "so that I can finally stop using this terminal here and start using the app".
# The §48 addenda fix the folders: `~/myrichos-nightly-a` is a newborn, `~/myrichos-nightly-b`
# has the roster page imported as memory, and his daily driver stays on his real HOME with
# `~/myrichos` as its folder. Nothing this script does may reach the daily driver.
#
# Adoption ledger §2.5 "Developer loop" (ADOPT AS-IS): a state directory derived per install,
# so a test install never shares data with the real app. Here the "state directory" is the
# folder's own HOME, and it makes sense for RichOS because a nightly under test must never
# write into his daily driver's memory or settings (§48).
#
# =========================================================================================
# THE LAYOUT, AND WHY BOTH HALVES END IN .noindex
# =========================================================================================
#
#   ~/myrichos-nightly-a/
#     app.noindex/RichOS.app   the app, unpacked from the ZIP
#     home.noindex/            the app's HOME: its data, its engine, its updater's copy
#     memory/                  folder b only: wiki/team-roster.md and loro/
#     engine.noindex/          only with --engine: the engine the app boots instead of its pin
#     logs/                    the app's output, one file per start
#     nightly-launch.txt       what was unpacked: version, commit, the ZIP's digest
#
# Spotlight does not index a folder whose name ends in `.noindex` (the convention Xcode's
# own build folders use). Without it, typing "RichOS" into Spotlight would offer this
# copy beside yours, and a nightly opened that way runs on your REAL HOME, against your
# own copy's data, with an updater that publishes into your real ~/Applications. The
# home is excluded for the same reason: the updater installs the nightly's next version
# at home.noindex/Applications/RichOS.app.
#
# =========================================================================================
# WHAT THE APP IS STARTED WITH, AND WHY EACH LINE IS THERE
# =========================================================================================
#
#   HOME=<folder>/home.noindex       the app's data, engine, updater and memory pointer all
#                                    derive from HOME, so the folder owns all of them.
#   CFFIXED_USER_HOME=(the same)     HOME alone is NOT enough, and this was measured
#                                    (2026-09-24, this Mac, JXA through osascript):
#                                    with HOME pointed elsewhere, Foundation's
#                                    NSHomeDirectory() still answered /Users/<him>, so
#                                    WebKit's store, preferences and caches would land in
#                                    his REAL ~/Library under the same bundle identifier his
#                                    daily driver uses. With CFFIXED_USER_HOME set as well
#                                    it answered the fake home. Both are set.
#   RICHOS_ACTIVATION=regular        a nightly under a folder HOME fails activation fact D,
#                                    so without this it would boot as an invisible
#                                    accessory (README.md, "Booting the app in a test").
#   DISABLE_AUTOUPDATER=1            `claude` updates itself into $HOME/.local; under the
#                                    folder HOME that would install a second copy of it
#                                    inside the folder and retarget the link below. The
#                                    link already follows his own `claude` as it updates.
#   LORO_CORPUS= LORO_ROOT=          explicit and exclusive in the app (loro.rs, "resolve
#                                    corpus"): a stray value would point the nightly at a
#                                    memory that is not the folder's. Empty is unset there.
#   RICHOS_ENGINE_DIR= and
#   RICHOS_ENGINE_ROOT=              EMPTY, unless the folder holds engine.noindex/. A pinned
#                                    nightly boots only the engine installed from its own
#                                    asset, and an explicit statement outranks the pin
#                                    (setup.rs, `engine_boot_refusal`), so setting one would
#                                    put the moving engine back. Empty is unset there too.
#                                    --engine <tar.gz> is that statement, made on purpose: a
#                                    build tested before it is published still carries the
#                                    LAST nightly's pin, so on its own it would download that
#                                    nightly's engine. The release tarball (one top-level
#                                    `engine/`) is unpacked into <folder>/engine.noindex and
#                                    RICHOS_ENGINE_DIR names it, exactly as the test VM's
#                                    `run.sh --engine` does; --again boots that engine too.
#                                    A ZIP named without --engine removes engine.noindex, so
#                                    that build boots its own pin, never an earlier build's
#                                    engine.
#   RICHOS_UPDATE_ENDPOINT=          EMPTY (the build's own channel), unless the folder boots
#                                    engine.noindex/: then the committed placeholder
#                                    (updates.rs, `PLACEHOLDER_ENDPOINT_HOST`), so the app
#                                    offers no update. RICHOS_ENGINE_DIR lives through an
#                                    in-app update, and the updated app would boot this test
#                                    engine instead of its own pin.
#   RICHOS_CLAUDE_BIN=              explicit and exclusive (setup.rs `find_claude`); the
#                                    folder's own link below is what the app should find.
#
#   TMPDIR=<home>/tmp/               the app's own temporary files stay in the folder.
#
# The launch goes through LaunchServices (`open -n -a`), so the app is launchd's child
# exactly as a double-clicked app is, and the terminal can be closed afterwards. `open`
# DOES carry the caller's environment into the app (measured in a guest, 2026-09-24), so
# it runs under `env -i` with only what launchd gives an app opened from Finder; see
# "Start it" below. CLAUDE_CONFIG_DIR is refused rather than cleared: if it is set, your
# Claude configuration is not ~/.claude, so the link below would be the wrong one, and
# `claude` reads an empty value as a path.
#
# THE SIGN-IN. Under a folder HOME `claude` answers "Not logged in" unless three things
# of his are reachable from it (richos docs/verification/first-words-on-the-window-
# 2026-09-19.md §7, three probes in order): `~/.claude` linked, `~/.claude.json` copied,
# and `~/Library/Keychains` linked. All three are made here, plus `~/.local/bin/claude`
# linked, because that is the one place a double-clicked app looks for `claude`
# (setup.rs `find_claude`: $RICHOS_CLAUDE_BIN, then $HOME/.local/bin/claude, then PATH,
# and launchd's PATH has no `claude` on it). The cost is stated plainly: `~/.claude` is
# his whole Claude configuration and history, shared with the terminal by design.
#
# =========================================================================================
# WHAT IT REFUSES
# =========================================================================================
#
#   * a ZIP that is missing, is not a zip, or does not hold exactly one RichOS.app;
#   * a folder holding another version (another ZIP, by digest) unless --replace, and a
#     folder holding anything this script did not record, always;
#   * ANY path under ~/myrichos — the folder, its home, the ZIP, the roster page — after
#     following symlinks. ~/myrichos-nightly-a is a sibling of ~/myrichos, not inside it,
#     so this is a path-component test and never a string prefix;
#   * a second start of a folder whose nightly is already running (two writers, one data);
#   * CLAUDE_CONFIG_DIR set anywhere the app would inherit it;
#   * a missing piece of the sign-in, named, rather than a window that says "Not logged in".
#
# THE WAYS TO DEFEAT IT, stated so nobody has to find them: opening
# `~/myrichos-nightly-*/app.noindex/RichOS.app` from Finder, from the Dock, or from the
# Apple menu's Recent Items (macOS adds it there on every start; measured in a guest)
# starts it on your REAL HOME, against your daily driver's data. Spotlight is closed by
# the .noindex folders; the rest this script cannot prevent. It can only be the one way
# you start a nightly. `status` says what each folder holds.
#
# THE WAY OUT: quit the nightly as you quit any app. To start a folder over as a newborn,
# quit it and move the whole folder to the Trash; nothing outside it refers to it.
#
# TEST SEAMS (the suite beside this file; never needed in use):
#   RICHOS_NIGHTLY_TEST_REAL_HOME   stands in for your account's home
#   RICHOS_NIGHTLY_OPEN / _PS / _LAUNCHCTL   stand in for /usr/bin/open, /bin/ps, /bin/launchctl
#   RICHOS_NIGHTLY_WAIT_SECONDS     how long to wait for the started app's pid (default 30)

set -uo pipefail

PROG="nightly-launch.sh"
say()  { printf '%s\n' "$*"; }
warn() { printf '%s: warning: %s\n' "$PROG" "$*" >&2; }
die()  { printf '%s: %s\n' "$PROG" "$*" >&2; exit 2; }
usage() { sed -n '6,12p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

OPEN_BIN="${RICHOS_NIGHTLY_OPEN:-/usr/bin/open}"
PS_BIN="${RICHOS_NIGHTLY_PS:-/bin/ps}"
LAUNCHCTL_BIN="${RICHOS_NIGHTLY_LAUNCHCTL:-/bin/launchctl}"
WAIT_SECONDS="${RICHOS_NIGHTLY_WAIT_SECONDS:-30}"
MARKER_NAME="nightly-launch.txt"
BUNDLE_ID="com.richos.app"
# ROSTER-PAGE HOOK. The page folder b carries, read from the record your daily driver
# already reads (its `loro-root` pointer), so no private path is written into this public
# script. The page is `wiki/team-roster.md` in that record; `--roster <file>` names any
# other file. While the page does not exist, folder b refuses with this sentence.
ROSTER_RELATIVE="wiki/team-roster.md"

[ "$(uname -s)" = "Darwin" ] || die "this is for macOS, where RichOS runs; this is $(uname -s)."

# ---------------------------------------------------------------------------------------
# Whose home is real
# ---------------------------------------------------------------------------------------
account_home() {
  local h
  h="$(/usr/bin/dscl . -read "/Users/$(id -un)" NFSHomeDirectory 2>/dev/null | sed -n 's/^NFSHomeDirectory: //p')"
  [ -n "$h" ] || h="${HOME:-}"
  printf '%s' "$h"
}
if [ -n "${RICHOS_NIGHTLY_TEST_REAL_HOME:-}" ]; then
  REAL_HOME="$RICHOS_NIGHTLY_TEST_REAL_HOME"
else
  REAL_HOME="$(account_home)"
  [ -n "$REAL_HOME" ] || die "cannot tell which home is yours (dscl gave nothing and HOME is unset)."
  # Run from inside a nightly, HOME would be that nightly's folder, and every link below
  # would point at the nightly instead of at you.
  [ "${HOME:-}" = "$REAL_HOME" ] || die "HOME is '${HOME:-}', but your account's home is '$REAL_HOME'. Run this from your own shell, not from inside a nightly or a scratch HOME."
fi
[ -d "$REAL_HOME" ] || die "your home '$REAL_HOME' is not a directory."
REAL_HOME="$(cd "$REAL_HOME" && pwd -P)"
# Pick the operator file before the app replaces HOME. An absent default means
# portable defaults; an explicit selector is always forwarded for reader refusal.
REAL_SETTINGS="${RICHOS_ENV_FILE-$REAL_HOME/Library/Application Support/RichOS/.env}"
SETTINGS_ENV=()
if [ "${RICHOS_ENV_FILE+x}" = x ] || [ -e "$REAL_SETTINGS" ]; then
  SETTINGS_ENV=("RICHOS_ENV_FILE=$REAL_SETTINGS")
fi
DAILY="$REAL_HOME/myrichos"

# ---------------------------------------------------------------------------------------
# Paths: resolve through symlinks, then refuse anything under ~/myrichos
# ---------------------------------------------------------------------------------------
# resolved <path>: the physical path. An existing directory is resolved whole; an existing
# file (or symlink to one) through its target; a path that does not exist yet through its
# nearest existing ancestor, with the missing tail appended.
resolved() {
  local p="$1" tail="" hops=0 target
  case "$p" in /*) ;; *) p="$PWD/$p" ;; esac
  while [ -L "$p" ] && [ "$hops" -lt 40 ]; do
    target="$(readlink "$p")"
    case "$target" in /*) p="$target" ;; *) p="$(dirname "$p")/$target" ;; esac
    hops=$((hops + 1))
  done
  while [ ! -e "$p" ] && [ "$p" != "/" ]; do
    tail="/$(basename "$p")$tail"
    p="$(dirname "$p")"
  done
  if [ -d "$p" ]; then
    p="$(cd "$p" && pwd -P)"
  else
    p="$(cd "$(dirname "$p")" && pwd -P)/$(basename "$p")"
  fi
  [ "$p" = "/" ] && p=""
  printf '%s%s' "$p" "$tail"
}
DAILY_RESOLVED="$(resolved "$DAILY")"
under_daily() {  # <path> — true when the path is ~/myrichos or anything inside it
  local r; r="$(resolved "$1")"
  case "$r/" in "$DAILY/"*|"$DAILY_RESOLVED/"*) return 0 ;; esac
  return 1
}
refuse_daily() {  # <what> <path>
  if under_daily "$2"; then
    die "refusing: the $1 '$2' is under ~/myrichos ($(resolved "$2")), which is your daily driver's folder. A nightly never reads or writes there."
  fi
}

# ---------------------------------------------------------------------------------------
# Which processes belong to a folder (read only; nothing here ever stops a process)
# ---------------------------------------------------------------------------------------
# Every process whose executable sits under the folder: the unpacked RichOS.app, the
# updater's copy in <folder>/home.noindex/Applications, and the helpers the app runs from itself.
folder_pids() {  # <folder>
  "$PS_BIN" -axo pid=,ppid=,comm= 2>/dev/null | while read -r pid ppid exe; do
    case "$exe" in "$1/"*) printf '%s\n' "$pid" ;; esac
  done
}
# The app itself: launchd's child (ppid 1), executable under the folder, ending in the
# bundle's executable name. Helpers are the app's children and never ppid 1.
folder_app_pids() {  # <folder>
  "$PS_BIN" -axo pid=,ppid=,comm= 2>/dev/null | while read -r pid ppid exe; do
    [ "$ppid" = "1" ] || continue
    case "$exe" in "$1/"*"/Contents/MacOS/"*) printf '%s\n' "$pid" ;; esac
  done
}

plist_value() {  # <plist> <key>
  # A missing file or key yields nothing: PlistBuddy prints "File Doesn't Exist, Will Create"
  # on stdout for a missing file, which must never be taken for a value.
  [ -f "$1" ] || return 0
  /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null || true
}
marker_value() {  # <marker> <key>
  sed -n "s/^$2=//p" "$1" 2>/dev/null | head -1
}

# ---------------------------------------------------------------------------------------
# status
# ---------------------------------------------------------------------------------------
status_one() {  # <slot>
  local slot="$1" folder="$REAL_HOME/myrichos-nightly-$1" m v pids ptr
  say "folder $slot: $folder"
  if [ ! -e "$folder" ]; then say "  empty: nothing installed"; return 0; fi
  m="$folder/$MARKER_NAME"
  if [ -f "$m" ]; then
    say "  unpacked: $(marker_value "$m" version) (commit $(marker_value "$m" commit), from $(marker_value "$m" zip), sha256 $(marker_value "$m" zip_sha256 | cut -c1-12)) at $(marker_value "$m" unpacked_at)"
  else
    say "  NOT made by this launcher: no $MARKER_NAME"
  fi
  v="$(plist_value "$folder/home.noindex/Applications/RichOS.app/Contents/Info.plist" CFBundleShortVersionString)"
  [ -n "$v" ] && say "  updated in place to: $v (the app starts this copy when it is newer)"
  pids="$(folder_app_pids "$folder" | tr '\n' ' ')"
  if [ -n "$pids" ]; then say "  running: pid $pids"; else say "  running: no"; fi
  ptr="$folder/home.noindex/Library/Application Support/RichOS/loro-root"
  if [ -L "$ptr" ]; then
    say "  memory: $(readlink "$ptr")"
    if [ -f "$folder/memory/$ROSTER_RELATIVE" ]; then
      say "  roster page: present ($(wc -c < "$folder/memory/$ROSTER_RELATIVE" | tr -d ' ') bytes)"
    fi
  fi
  if [ -f "$folder/engine.noindex/VERSION" ]; then
    say "  engine: $folder/engine.noindex (RICHOS_ENGINE_DIR; VERSION $(head -1 "$folder/engine.noindex/VERSION"); updates off)"
  fi
  if [ -d "$folder/home.noindex/Library/Application Support/RichOS/corpus" ]; then
    say "  note: the app provisioned its own memory at .../RichOS/corpus, which outranks the pointer above"
  fi
}

# ---------------------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------------------
[ $# -ge 1 ] || usage
if [ "$1" = "status" ]; then
  [ $# -eq 1 ] || usage
  status_one a; status_one b; status_one c
  exit 0
fi
case "$1" in -h|--help) usage ;; esac

SLOT="$1"; shift
case "$SLOT" in
  a|b|c) ;;
  *) die "the first word is the folder, a, b or c (or 'status'); got '$SLOT'." ;;
esac
ZIP=""; AGAIN=0; REPLACE=0; ROSTER=""; ENGINE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --again)   AGAIN=1; shift ;;
    --replace) REPLACE=1; shift ;;
    --roster)  [ $# -ge 2 ] || die "--roster needs a file."; ROSTER="$2"; shift 2 ;;
    --engine)  [ $# -ge 2 ] || die "--engine needs the engine's .tar.gz."; ENGINE="$2"; shift 2 ;;
    -h|--help) usage ;;
    -*)        die "unknown option '$1'." ;;
    *)         [ -z "$ZIP" ] || die "one ZIP at a time; got '$ZIP' and '$1'."; ZIP="$1"; shift ;;
  esac
done
[ "$SLOT" = "b" ] || [ -z "$ROSTER" ] || die "--roster is for folder b; folder a is a newborn and carries no memory."
if [ "$AGAIN" -eq 1 ]; then
  [ -z "$ZIP" ] || die "--again starts what the folder already holds; give a ZIP or --again, not both."
  [ "$REPLACE" -eq 0 ] || die "--again replaces nothing."
  [ -z "$ENGINE" ] || die "--again starts the engine the folder already holds; give --engine with the ZIP."
else
  [ -n "$ZIP" ] || die "name the nightly ZIP to start (or --again to start what folder $SLOT holds)."
fi

FOLDER="$REAL_HOME/myrichos-nightly-$SLOT"
refuse_daily "folder" "$FOLDER"
refuse_daily "folder's home" "$FOLDER/home.noindex"
FOLDER_R="$(resolved "$FOLDER")"
[ "$FOLDER_R" != "$REAL_HOME" ] || die "refusing: '$FOLDER' resolves to your home itself."
case "$REAL_HOME/" in "$FOLDER_R/"*) die "refusing: '$FOLDER' resolves to '$FOLDER_R', which contains your home." ;; esac
FOLDER="$FOLDER_R"
MARKER="$FOLDER/$MARKER_NAME"
APP="$FOLDER/app.noindex/RichOS.app"
NHOME="$FOLDER/home.noindex"

# ---------------------------------------------------------------------------------------
# Refusals that need nothing unpacked
# ---------------------------------------------------------------------------------------
if [ -n "${CLAUDE_CONFIG_DIR:-}" ]; then
  die "refusing: CLAUDE_CONFIG_DIR is set in this shell ('$CLAUDE_CONFIG_DIR'). The nightly's claude must use the folder's ~/.claude link, and an empty value cannot clear it (claude reads an empty value as a path). Unset it and run again."
fi
LAUNCHD_CCD="$("$LAUNCHCTL_BIN" getenv CLAUDE_CONFIG_DIR 2>/dev/null || true)"
if [ -n "$LAUNCHD_CCD" ]; then
  die "refusing: CLAUDE_CONFIG_DIR is set for every app you open ('launchctl getenv' says '$LAUNCHD_CCD'), and the nightly would inherit it. Remove it with 'launchctl unsetenv CLAUDE_CONFIG_DIR' and run again."
fi
for need in ".claude" ".claude.json" "Library/Keychains" ".local/bin/claude"; do
  if [ ! -e "$REAL_HOME/$need" ]; then
    die "refusing: $REAL_HOME/$need does not exist. The nightly signs in to Claude through your ~/.claude, ~/.claude.json and ~/Library/Keychains, and finds claude at ~/.local/bin/claude; without it the window opens to 'Not logged in' or 'Claude Code missing'."
  fi
done

RUNNING="$(folder_pids "$FOLDER" | tr '\n' ' ')"
if [ -n "$RUNNING" ]; then
  die "refusing: the nightly in $FOLDER is already running (pid $RUNNING). Two copies on one folder would be two writers to one set of data. Quit it first."
fi

# ---------------------------------------------------------------------------------------
# Scratch that never outlives this script
# ---------------------------------------------------------------------------------------
WORK=""; STAGE=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  [ -n "$STAGE" ] && rm -rf "$STAGE"
  return 0
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM
WORK="$(mktemp -d "${TMPDIR:-/tmp}/nightly-launch.XXXXXX")" || die "cannot create a scratch directory."

# ---------------------------------------------------------------------------------------
# The ZIP: what it is, before anything is written
# ---------------------------------------------------------------------------------------
if [ "$AGAIN" -eq 0 ]; then
  refuse_daily "ZIP" "$ZIP"
  [ -e "$ZIP" ] || die "refusing: no such ZIP: '$ZIP'."
  [ -f "$ZIP" ] || die "refusing: '$ZIP' is not a file."
  ENTRIES="$WORK/entries"
  /usr/bin/unzip -Z1 "$ZIP" > "$ENTRIES" 2>/dev/null || die "refusing: '$ZIP' is not a readable zip archive."
  OUTSIDE="$(grep -v -e '^RichOS\.app/' -e '^__MACOSX/' "$ENTRIES" | head -3 | tr '\n' ' ')"
  [ -z "$OUTSIDE" ] || die "refusing: '$ZIP' holds more than RichOS.app ($OUTSIDE). A nightly install ZIP holds exactly one app."
  grep -qx 'RichOS.app/Contents/Info.plist' "$ENTRIES" || die "refusing: '$ZIP' has no RichOS.app/Contents/Info.plist, so it is not a RichOS app."
  /usr/bin/unzip -p "$ZIP" RichOS.app/Contents/Info.plist > "$WORK/Info.plist" 2>/dev/null \
    || die "refusing: could not read the Info.plist inside '$ZIP'."
  ZIP_ID="$(plist_value "$WORK/Info.plist" CFBundleIdentifier)"
  [ "$ZIP_ID" = "$BUNDLE_ID" ] || die "refusing: the app in '$ZIP' is '${ZIP_ID:-unidentified}', not $BUNDLE_ID."
  ZIP_VERSION="$(plist_value "$WORK/Info.plist" CFBundleShortVersionString)"
  [ -n "$ZIP_VERSION" ] || die "refusing: the app in '$ZIP' carries no version."
  ZIP_COMMIT="$(plist_value "$WORK/Info.plist" RichOSSourceCommit)"
  ZIP_SHA="$(/usr/bin/shasum -a 256 "$ZIP" | cut -d' ' -f1)"
  [ ${#ZIP_SHA} -eq 64 ] || die "could not hash '$ZIP'."
fi
if [ -n "$ENGINE" ]; then
  refuse_daily "engine" "$ENGINE"
  [ -f "$ENGINE" ] || die "refusing: no such engine file: '$ENGINE'."
  ENGINE_TOP="$(/usr/bin/tar -tzf "$ENGINE" 2>/dev/null | head -1)"
  [ "$ENGINE_TOP" = "engine/" ] || die "refusing: '$ENGINE' is not a RichOS engine tarball (its first member is '${ENGINE_TOP:-nothing}', not 'engine/')."
fi

# ---------------------------------------------------------------------------------------
# The folder: what it already holds
# ---------------------------------------------------------------------------------------
UNPACK=0
if [ -e "$FOLDER" ] && [ ! -d "$FOLDER" ]; then
  die "refusing: '$FOLDER' exists and is not a folder."
fi
if [ -f "$MARKER" ]; then
  HAVE_VERSION="$(marker_value "$MARKER" version)"
  HAVE_SHA="$(marker_value "$MARKER" zip_sha256)"
  [ -d "$APP" ] || die "refusing: $MARKER records $HAVE_VERSION but $APP is missing. Move the folder to the Trash and start it again from the ZIP."
  if [ "$AGAIN" -eq 0 ] && [ "$HAVE_SHA" != "$ZIP_SHA" ]; then
    if [ "$REPLACE" -eq 0 ]; then
      die "refusing: $FOLDER holds $HAVE_VERSION (sha256 $(printf '%s' "$HAVE_SHA" | cut -c1-12)), and '$ZIP' is $ZIP_VERSION (sha256 $(printf '%s' "$ZIP_SHA" | cut -c1-12)). Pass --replace to swap the app and keep the folder's data, or move the folder to the Trash for a newborn."
    fi
    UNPACK=1
  fi
elif [ -d "$FOLDER" ] && [ -n "$(find "$FOLDER" -mindepth 1 -maxdepth 1 -print 2>/dev/null | head -1)" ]; then
  die "refusing: $FOLDER holds files this launcher did not put there (no $MARKER_NAME). It is not replaced by anything; move it aside yourself."
else
  [ "$AGAIN" -eq 0 ] || die "refusing: folder $SLOT holds no nightly yet; name the ZIP to install one."
  UNPACK=1
fi

# ---------------------------------------------------------------------------------------
# Unpack (only when the folder is new, or --replace said so)
# ---------------------------------------------------------------------------------------
( umask 077; mkdir -p "$FOLDER" ) || die "cannot create $FOLDER."
chmod 700 "$FOLDER" 2>/dev/null
if [ "$UNPACK" -eq 1 ]; then
  STAGE="$FOLDER/.unpacking.$$"
  rm -rf "$STAGE"; mkdir "$STAGE" || die "cannot create $STAGE."
  /usr/bin/ditto -x -k "$ZIP" "$STAGE" || die "could not unpack '$ZIP'."
  [ -d "$STAGE/RichOS.app" ] || die "'$ZIP' unpacked without a RichOS.app."
  if [ -d "$APP" ]; then rm -rf "$APP" || die "could not remove the old $APP."; fi
  mkdir -p "$(dirname "$APP")" || die "cannot create $(dirname "$APP")."
  mv "$STAGE/RichOS.app" "$APP" || die "could not move the new app into $FOLDER."
  rm -rf "$STAGE"; STAGE=""
  {
    echo "richos-nightly-launch 1"
    echo "slot=$SLOT"
    echo "version=$ZIP_VERSION"
    echo "commit=${ZIP_COMMIT:-unknown}"
    echo "zip=$(basename "$ZIP")"
    echo "zip_sha256=$ZIP_SHA"
    echo "unpacked_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$MARKER.tmp" || die "could not write $MARKER.tmp."
  mv "$MARKER.tmp" "$MARKER" || die "could not record what was unpacked in $MARKER."
  UPDATED="$(plist_value "$NHOME/Applications/RichOS.app/Contents/Info.plist" CFBundleShortVersionString)"
  if [ -n "$UPDATED" ] && [ "$UPDATED" != "$ZIP_VERSION" ]; then
    warn "this folder updated itself to $UPDATED earlier ($NHOME/Applications/RichOS.app). The app starts that copy instead when it is newer than $ZIP_VERSION."
  fi
fi
VERSION="$(marker_value "$MARKER" version)"
COMMIT="$(marker_value "$MARKER" commit)"

# ---------------------------------------------------------------------------------------
# --engine: unpack it into the folder, checked the way the app checks it
# ---------------------------------------------------------------------------------------
ENGINE_DIR="$FOLDER/engine.noindex"
if [ -n "$ENGINE" ]; then
  STAGE="$FOLDER/.engine-unpacking.$$"
  rm -rf "$STAGE"; mkdir "$STAGE" || die "cannot create $STAGE."
  /usr/bin/tar -C "$STAGE" --strip-components 1 -xzf "$ENGINE" || die "could not unpack '$ENGINE'."
  # Same predicate as the app's `engine_looks_valid` and testvm/run.sh: a wrong shape would
  # boot to "the RichOS engine isn't on this Mac".
  [ -d "$STAGE/scripts/hooks" ] && [ -f "$STAGE/VERSION" ] \
    || die "refusing: '$ENGINE' unpacked without scripts/hooks and VERSION, so the app would not take it as an engine."
  rm -rf "$ENGINE_DIR" || die "could not remove the old $ENGINE_DIR."
  mv "$STAGE" "$ENGINE_DIR" || die "could not move the engine into $FOLDER."
  STAGE=""
elif [ "$AGAIN" -eq 0 ] && [ -e "$ENGINE_DIR" ]; then
  # A ZIP named without --engine boots its own pin: an engine left from an earlier
  # --engine belongs to that build, not this one. Only --again keeps it.
  rm -rf "$ENGINE_DIR" || die "could not remove $ENGINE_DIR, the engine an earlier --engine left."
  say "removed $ENGINE_DIR: no --engine was given, so the app boots the engine its own build pins."
fi
ENGINE_ENV="RICHOS_ENGINE_DIR="
UPDATE_ENV="RICHOS_UPDATE_ENDPOINT="
if [ -f "$ENGINE_DIR/VERSION" ]; then
  ENGINE_ENV="RICHOS_ENGINE_DIR=$ENGINE_DIR"
  # No updates while a test engine boots: the override would outlive the update, and the
  # updated app would boot this engine instead of its own pin. The placeholder host reads as
  # "unconfigured" in the app (updates.rs), so it checks nothing and offers nothing.
  UPDATE_ENV="RICHOS_UPDATE_ENDPOINT=https://updates.richos.invalid/latest.json"
fi

# ---------------------------------------------------------------------------------------
# The folder's home: the three sign-in links, the copy, and claude
# ---------------------------------------------------------------------------------------
( umask 077; mkdir -p "$NHOME/Library" "$NHOME/.local/bin" ) || die "cannot create $NHOME."
chmod 700 "$NHOME" 2>/dev/null
link() {  # <link> <target>
  if [ -L "$1" ]; then
    [ "$(readlink "$1")" = "$2" ] && return 0
    die "refusing: $1 points at '$(readlink "$1")', not '$2'. Something changed it; move the folder to the Trash and start it again."
  fi
  [ ! -e "$1" ] || die "refusing: $1 exists and is not the link to '$2'. Something replaced it; move the folder to the Trash and start it again."
  ln -s "$2" "$1" || die "could not link $1 -> $2."
}
link "$NHOME/.claude"            "$REAL_HOME/.claude"
link "$NHOME/Library/Keychains"  "$REAL_HOME/Library/Keychains"
link "$NHOME/.local/bin/claude"  "$REAL_HOME/.local/bin/claude"
# Copied on every start, so a change to your sign-in reaches the nightly; whatever the
# nightly's claude wrote into its copy is disposable.
( umask 077; cp "$REAL_HOME/.claude.json" "$NHOME/.claude.json.tmp" ) \
  || die "could not copy $REAL_HOME/.claude.json into $NHOME."
mv "$NHOME/.claude.json.tmp" "$NHOME/.claude.json" || die "could not put the copy of .claude.json in place in $NHOME."

# ---------------------------------------------------------------------------------------
# Folder b: a small memory folder carrying the roster page
# ---------------------------------------------------------------------------------------
# The app reads a record-shaped folder (`wiki/` and `loro/`) through the
# `loro-root` pointer, which is exactly how your daily driver reads your record.
if [ "$SLOT" = "b" ]; then
  if [ -z "$ROSTER" ]; then
    ROSTER="$REAL_HOME/Library/Application Support/RichOS/loro-root/$ROSTER_RELATIVE"
    ROSTER_FROM="the record your daily driver reads"
  else
    ROSTER_FROM="--roster"
  fi
  refuse_daily "roster page" "$ROSTER"
  if [ ! -f "$ROSTER" ]; then
    if [ -f "$FOLDER/memory/$ROSTER_RELATIVE" ]; then
      warn "the roster page is not at '$ROSTER' any more; folder b keeps the copy it already has."
    else
      die "refusing: folder b carries the team roster page, and there is none at '$ROSTER' ($ROSTER_FROM). The page is $ROSTER_RELATIVE in your record; until it exists, name a file with --roster <page.md>."
    fi
  else
    ( umask 077; mkdir -p "$FOLDER/memory/wiki" "$FOLDER/memory/loro" ) || die "cannot create $FOLDER/memory."
    cp "$ROSTER" "$FOLDER/memory/$ROSTER_RELATIVE.tmp" || die "could not copy the roster page into $FOLDER/memory."
    mv "$FOLDER/memory/$ROSTER_RELATIVE.tmp" "$FOLDER/memory/$ROSTER_RELATIVE" \
      || die "could not put the roster page in place in $FOLDER/memory."
  fi
  ( umask 077; mkdir -p "$NHOME/Library/Application Support/RichOS" ) || die "cannot create the memory pointer's folder."
  link "$NHOME/Library/Application Support/RichOS/loro-root" "$FOLDER/memory"
  if [ -d "$NHOME/Library/Application Support/RichOS/corpus" ]; then
    warn "folder b's app provisioned its own memory at .../RichOS/corpus, which outranks the roster memory. Move that folder aside to read the roster again."
  fi
fi

# ---------------------------------------------------------------------------------------
# Start it
# ---------------------------------------------------------------------------------------
mkdir -p "$FOLDER/logs" || die "cannot create $FOLDER/logs."
LOG="$FOLDER/logs/$(date -u +%Y%m%dT%H%M%SZ).log"
BEFORE="$WORK/before"
"$PS_BIN" -axo pid= 2>/dev/null | tr -d ' ' > "$BEFORE"
( umask 077; mkdir -p "$NHOME/tmp" ) || die "cannot create $NHOME/tmp."

# `open` hands the app ITS OWN environment. MEASURED in a test guest, 2026-09-24: a
# variable exported in the shell before this script ran was in the started app's
# environment (`ps -wwE`). So whatever this shell carries — a CLAUDECODE from a Claude Code
# session, a PATH with Homebrew on it, a stray RICHOS_* — would reach the nightly and every
# `claude` it starts. `open` therefore runs under `env -i`, with only what launchd gives an
# app opened from Finder: launchd's PATH, who you are, your shell, your ssh agent, and a
# TMPDIR, which is the folder's own so the app's temporary files stay inside it.
LAUNCH_ENV=("PATH=/usr/bin:/bin:/usr/sbin:/sbin" "HOME=$REAL_HOME" "USER=$(id -un)" "LOGNAME=$(id -un)"
            "SHELL=${SHELL:-/bin/zsh}" "TMPDIR=$NHOME/tmp/")
LAUNCH_ENV+=("${SETTINGS_ENV[@]+"${SETTINGS_ENV[@]}"}")
[ -n "${SSH_AUTH_SOCK:-}" ] && LAUNCH_ENV+=("SSH_AUTH_SOCK=$SSH_AUTH_SOCK")
[ -n "${__CF_USER_TEXT_ENCODING:-}" ] && LAUNCH_ENV+=("__CF_USER_TEXT_ENCODING=$__CF_USER_TEXT_ENCODING")

/usr/bin/env -i "${LAUNCH_ENV[@]}" "$OPEN_BIN" -n -a "$APP" \
  --env "HOME=$NHOME" \
  --env "CFFIXED_USER_HOME=$NHOME" \
  --env "TMPDIR=$NHOME/tmp/" \
  --env "RICHOS_ACTIVATION=regular" \
  --env "DISABLE_AUTOUPDATER=1" \
  --env "LORO_CORPUS=" \
  --env "LORO_ROOT=" \
  --env "$ENGINE_ENV" \
  --env "$UPDATE_ENV" \
  --env "RICHOS_ENGINE_ROOT=" \
  --env "RICHOS_CLAUDE_BIN=" \
  --stdout "$LOG" --stderr "$LOG" \
  || die "macOS refused to open $APP (see above). Nothing else was changed."

PID=""
deadline=$(( $(date +%s) + WAIT_SECONDS ))
while [ "$(date +%s)" -le "$deadline" ]; do
  for p in $(folder_app_pids "$FOLDER"); do
    grep -qx "$p" "$BEFORE" || { PID="$p"; break; }
  done
  [ -n "$PID" ] && break
  sleep 1
done
if [ -z "$PID" ]; then
  die "opened $APP, but no app process from $FOLDER appeared within ${WAIT_SECONDS}s. If macOS is asking whether to open an app downloaded from the internet, answer it, then '$PROG status' shows the pid. The app's output is in $LOG."
fi

say "slot=$SLOT version=$VERSION commit=${COMMIT:-unknown} pid=$PID"
say "folder=$FOLDER"
say "home=$NHOME"
say "log=$LOG"
[ "$ENGINE_ENV" = "RICHOS_ENGINE_DIR=" ] || say "engine=$ENGINE_DIR (updates off; a ZIP started without --engine turns them back on)"

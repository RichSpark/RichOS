#!/usr/bin/env bash
# Build, sign and VERIFY the RichOS desktop bundle. One command, one final line.
#
#   richos/app/scripts/package-app.sh                      # ad-hoc signed
#   richos/app/scripts/package-app.sh --sign developer-id  # discovers the identity
#   RICHOS_NOTARIZE=1 richos/app/scripts/package-app.sh --sign developer-id
#                                                   # ...and notarizes and staples
#   richos/app/scripts/package-app.sh --verify-only <path/to/RichOS.app> [--expect-notarized]
#   richos/app/scripts/package-app.sh --sign developer-id --dry-run   # resolve, do not build
#   richos/app/scripts/package-app.sh --updater            # ...and the signed update artifacts
#   richos/app/scripts/package-app.sh --release             # THIS build IS the release make-release.sh tags
#   richos/app/scripts/package-app.sh --nightly <version>   # THIS build IS the nightly nightly.py reserved
#
# DECLARED INTENT, AND WHAT A PLAIN RUN NOW IS
# ---------------------------------------------
# Every run is release, nightly, or (the default, with neither flag) a DEVELOPMENT BUILD —
# and it is declared by the caller, never guessed. make-release.sh passes --release or
# --nightly <version> itself, derived from the same VERSION shape it already branches on;
# nothing here inspects the environment to decide.
#
# A development build's version can never be mistaken for a release: it is stamped
# `{cargo-version}-dev.{shortsha}`, with `.dirty` appended when the tree has uncommitted
# changes — the same shape `nightly.py` already uses for `-nightly.{day}.{n}`, with `dev` in
# place of `nightly`. This is what closes CEO 2026-09-17 item 4's open half
# (esc-20260917T081214Z-b466c998): the 2026-09-08 build he was handed was a plain run with
# no declared intent, and it shipped Cargo.toml's bare, unreleased "1.0.3" verbatim because
# nothing before this stopped it. `verify_bundle` now refuses that bare number outright
# unless `--release` was declared; a `-dev.` or `-nightly.` version was never bare in the
# first place, so neither path needs to declare anything to pass.
#
# WHY THIS FILE EXISTS
# --------------------
# `app/src-tauri/build.rs` has carried a real, two-way-proven icon gate since the
# icon pipeline landed: it warns on an ordinary `cargo build` and PANICS when
# `RICHOS_REQUIRE_REAL_ICONS=1`. Nothing anywhere set that variable, because there
# was no bundling script — so the gate protected no release path at all and a
# bundle could still ship the placeholder set. This script is the caller the gate
# was written for. It exports RICHOS_REQUIRE_REAL_ICONS=1, and it refuses rather
# than warns.
#
# It also checks the icons BEFORE the build, so a placeholder set costs you two
# seconds instead of a full release compile. Both checks stay: the pre-flight is
# for legibility, the environment variable is the structural guarantee. If the
# pre-flight were ever deleted, `build.rs` still stops the build.
#
# SIGNING — TWO PATHS, NEITHER GUESSED
# ------------------------------------
# Which signing path runs is decided by explicit configuration and by what is
# actually on this machine. It is never inferred and never silently downgraded:
# asking for `developer-id` without a usable identity is a hard refusal, not a
# quiet fall back to ad-hoc.
#
#   adhoc         (default) — no certificate needed. The bundler does NOT do this
#                 for you: with no identity it leaves the .app unsigned around a
#                 linker-signed executable, which `codesign --verify` rejects. So
#                 this script signs ad-hoc deliberately. WHAT IT COSTS is printed at
#                 the moment it is incurred: an ad-hoc bundle embeds no designated
#                 requirement, so macOS's privacy database (TCC) binds microphone
#                 and accessibility grants to the raw code hash. A build that
#                 changes a shipped byte is a different application to macOS and
#                 both grants die. Toggling the permission in System Settings
#                 provably does not migrate a grant. Measured 2026-08-24 and
#                 refined 2026-08-30; richos-hq/wiki/packaging-and-signing.md.
#
#   developer-id  the real path. It DISCOVERS the identity: with exactly one
#                 "Developer ID Application" identity on the machine it uses that
#                 one and prints it; with none, or with more than one, it refuses
#                 and names them, because picking between two certificates is not a
#                 thing a script may do quietly. RICHOS_SIGNING_IDENTITY still
#                 overrides and is still checked against
#                 `security find-identity -v -p codesigning`.
#
#                 IT STILL SELECTS ITSELF NEVER. `--sign developer-id` (or
#                 RICHOS_SIGN_MODE) is required; an identity appearing on the
#                 machine does not change what a plain run does. What changed on
#                 2026-08-31 is only the REASON: the CEO enrolled in the Apple
#                 Developer Program, so decision 1.1 is closed and the old refusal
#                 text pointing at it as open was retired with it.
#
# NOTARIZATION IS RUN HERE, NOT BY THE BUNDLER, and that is a deliberate choice
# with a measured reason. tauri-bundler 2.9.4's `bundle/macos/app.rs:135-148`
# notarizes the .app when it can resolve credentials — and on ANY credential error
# but one it does this:
#
#     Err(e) => { ... log::warn!("skipping app notarization, {e}"); }
#
# A typo in a key path therefore produces a WARNING inside a wall of build output
# and an un-notarized bundle, from a run that exits 0. This script's whole premise
# is that the artifact is checked rather than the builder's exit code, so it
# unsets every APPLE_* variable the bundler reads before invoking it, and submits
# to the notary itself where a failure is a failure.
#
# (Unsetting also disarms a live trap in the same function: APPLE_ID and
# APPLE_PASSWORD set with no APPLE_TEAM_ID is the ONE case the bundler treats as
# fatal, so an operator with those two exported for unrelated reasons currently
# cannot build at all, with a message about notarization during a build that never
# asked for it.)
#
# A SECOND MEASURED REASON TO SIGN IT OURSELVES: tauri-macos-sign 2.3.4's
# `keychain.rs:221-226` builds the codesign argv as
# `--force -s <identity> [--options runtime] [--entitlements <plist>]` and never
# passes `--timestamp`. codesign's own man page calls the unspecified default
# "a system-specific default behavior ... may result in some but not all code
# signatures being timestamped" — and a secure timestamp is a NOTARIZATION
# PREREQUISITE. So after the bundler has signed inside-out (which it does
# correctly, and which is why it still does the signing), this script re-signs the
# bundle with the full explicit argv including `--timestamp`, before notarizing.
# Deterministic rather than dependent on a documented non-guarantee.
#
# CREDENTIALS FOR THE NOTARY, in the order preferred:
#
#   RICHOS_NOTARY_KEY / RICHOS_NOTARY_KEY_ID / RICHOS_NOTARY_ISSUER
#                     an App Store Connect API key (.p8), its Key ID and the
#                     issuer UUID. No Apple ID and no password anywhere.
#   RICHOS_NOTARY_PROFILE
#                     a keychain profile previously stored with
#                     `xcrun notarytool store-credentials`. The credential lives in
#                     the keychain; nothing is in the environment.
#
# THERE IS DELIBERATELY NO APPLE_PASSWORD PATH. An app-specific password in an
# environment variable is readable by every process this build spawns and lands in
# shell history. The same password can be put in a keychain profile ONCE, and then
# RICHOS_NOTARY_PROFILE reaches it — so nothing is lost by not supporting it and a
# standing credential leak is.
#
# The .p8 is refused if it sits inside a git worktree. Apple lets that file be
# downloaded exactly once; a copy in a repository is a copy in the history.
#
# Without RICHOS_NOTARIZE=1 a developer-id build is signed but NOT notarized, and
# says so — Gatekeeper still blocks a downloaded copy.
#
# WHAT IT VERIFIES AFTER THE BUILD — the artefact, not the exit code of the
# builder. The bundle exists and is executable; its signature verifies; the
# signature is the KIND that was asked for; the icon that actually shipped inside
# Contents/Resources is the icon this repository generated; and the bundled
# Info.plist carries NSMicrophoneUsageDescription (without it the app cannot ask
# for the microphone at all, and Tauri's plist merge for that key had never been
# checked by anyone).
#
# ON THE developer-id PATH IT ALSO CHECKS THE DESIGNATED REQUIREMENT ITSELF, which
# is the only property any of this was bought for. A Developer ID signature is
# worth the $99 because macOS stores
#
#     identifier "com.richos.app" and anchor apple generic
#       and certificate leaf[subject.OU] = "<TEAMID>"
#
# against every permission it grants — an expression every FUTURE build satisfies.
# A signature can be a perfectly valid Developer ID signature and still carry a
# cdhash-shaped requirement (that is what an ad-hoc one is), in which case the
# grants die on the next build exactly as before and nothing else in this script
# would notice. So the requirement is read back and joined to the bundle
# identifier on disk, and a `cdhash` literal in it is a hard failure.
#
# And when notarization was asked for, the STAPLED TICKET is validated on the
# bundle. "Notarized" without a stapled ticket means the app must reach Apple over
# the network to be admitted, which on a machine that is offline or behind a
# captive portal is indistinguishable from not being notarized at all.
#
# `--verify-only` runs that whole check set against a bundle built earlier, so a
# bundle can be re-checked without a rebuild.
#
# THE .app IS THE ARTEFACT THAT MATTERS. It defaults to `--bundles app` rather
# than the `"targets": "all"` in tauri.conf.json: DMG creation is a separate,
# flakier downstream step (it mounts a disk image) whose failure says nothing
# about whether the application is correct. Pass `--bundles app,dmg` when you
# want the installer too.
#
# THE UPDATE ARTIFACTS (--updater), and the vendor defect that decides HOW.
# ------------------------------------------------------------------------
# `tauri.conf.json` sets `bundle.createUpdaterArtifacts: true`, so
# tauri-bundler 2.9.4 tars the `.app` into `RichOS.app.tar.gz`
# (`bundle.rs:206-225`) and tauri-cli 2.11.4 then minisigns it
# (`bundle.rs:sign_updaters`). BOTH RUN INSIDE `cargo tauri build`, which is
# BEFORE this script ad-hoc signs the bundle — the bundler signs a macOS
# bundle only when it has a real identity, and with none it leaves the `.app`
# unsigned around a linker-signed executable.
#
# So the artifact the vendor produces on the ad-hoc path is a tarball of an
# UNSIGNED application, minisigned to say it is genuine. It would install, and
# it would install a bundle `codesign --verify` rejects.
#
# This script therefore passes `--no-sign` to the build ALWAYS — which on
# macOS disables only the updater signature, not codesigning
# (tauri-bundler `bundle.rs:301-306` gates it on Windows) — DELETES whatever
# tarball the bundler left, and, on `--updater`, re-creates it from the
# verified bundle and signs that. The archive is built by
# `scripts/lib/updater_tar.py` rather than by `tar`, because macOS's bsdtar
# writes AppleDouble `._*` sidecars that the updater would unpack into the
# installed app.
#
# `--no-sign` is also what lets an ordinary packaging run work with no signing
# key at all: without it, tauri-cli REFUSES to finish any build that has a
# `pubkey` and no `TAURI_SIGNING_PRIVATE_KEY` (`bundle.rs:277-279`).
#
# THE KEY NEVER LIVES IN A REPOSITORY. `TAURI_SIGNING_PRIVATE_KEY_PATH` is
# refused if it points inside a git worktree, and refused if the file is
# readable by other users — the same two rules the notary key already has.
#
# WHAT IT VERIFIES ABOUT THE ARTIFACTS, because a signer's exit code says a
# signature was WRITTEN and not that it VERIFIES: the archive is re-read and
# asserted installable (one `.app` root, an Info.plist, an executable bit, no
# AppleDouble members), and then `cargo run --example
# verify_update_signature` checks the `.sig` against the pubkey THIS
# REPOSITORY SHIPS. A release signed with the wrong key is otherwise
# indistinguishable from a correct one until it reaches a customer.
#
# THE MANIFEST is written only when RICHOS_UPDATE_BASE_URL says where the
# archive will be reachable. WHERE RICHOS UPDATES ARE HOSTED IS THE CEO'S
# DECISION AND HAS NOT BEEN MADE, and a manifest carrying a guessed URL is
# worse than no manifest: it is a file that looks publishable.
#
# Exit codes: 0 success. 1 the produced bundle failed verification. 2 refused
# before building (icons not shippable, or the signing configuration is not
# usable). 3 a prerequisite is missing. 4 the build itself failed. 5 the
# bundle is good and the update artifacts are not.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app_dir="$(cd "$here/.." && pwd)"
src_tauri="$app_dir/src-tauri"

sign_mode="${RICHOS_SIGN_MODE:-adhoc}"
bundles="app"
verify_only=""
expect_notarized=""
dry_run=""
updater="${RICHOS_UPDATER:-}"
release_flag=""
nightly_flag=""
nightly_version=""

while [ $# -gt 0 ]; do
  case "$1" in
    --sign)        sign_mode="${2:-}"; shift 2 ;;
    --sign=*)      sign_mode="${1#*=}"; shift ;;
    --bundles)     bundles="${2:-}"; shift 2 ;;
    --bundles=*)   bundles="${1#*=}"; shift ;;
    --verify-only) verify_only="${2:-}"; shift 2 ;;
    --verify-only=*) verify_only="${1#*=}"; shift ;;
    --expect-notarized) expect_notarized=1; shift ;;
    --updater)     updater=1; shift ;;
    --dry-run)     dry_run=1; shift ;;
    --release)     release_flag=1; shift ;;
    --nightly)     nightly_flag=1; nightly_version="${2:-}"
                   # `shift 2` with nothing left after `--nightly` is a `set -e` exit under
                   # bash's own exit-status-1-on-short-shift behavior, before the
                   # "--nightly requires a version" refusal below ever runs.
                   if [ "$#" -ge 2 ]; then shift 2; else shift; fi ;;
    --nightly=*)   nightly_flag=1; nightly_version="${1#*=}"; shift ;;
    -h|--help)     sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
done

case "$sign_mode" in
  adhoc|developer-id) ;;
  *) echo "error: --sign must be 'adhoc' or 'developer-id', got: ${sign_mode}" >&2; exit 2 ;;
esac

# ---------------------------------------------------------------------------
# DECLARED INTENT — release, nightly, or (the default, with neither flag) a
# development build. Never guessed: make-release.sh passes --release or
# --nightly <version> itself (derived from the VERSION shape it already
# branches on), and nightly.py never calls this script directly. CEO
# 2026-09-17 item 4, the open half of richos 62f716c3, closed by
# esc-20260917T081214Z-b466c998.
# ---------------------------------------------------------------------------
if [ -n "$release_flag" ] && [ -n "$nightly_flag" ]; then
  echo "error: --release and --nightly are mutually exclusive" >&2
  exit 2
fi
if [ -n "$nightly_flag" ] && [ -z "$nightly_version" ]; then
  echo "error: --nightly requires the version it is building, e.g. --nightly 1.2.0-nightly.20260917.1" >&2
  exit 2
fi
if [ -n "$release_flag" ]; then
  intent="release"
elif [ -n "$nightly_flag" ]; then
  intent="nightly"
else
  intent="dev"
fi

say()  { printf '%s\n' "$*"; }
warn() { printf '%s\n' "$*" >&2; }

BUNDLE_CDHASH=""
BUNDLE_MIC=""
BUNDLE_DR=""
BUNDLE_VERSION=""

# ---------------------------------------------------------------------------
# Verification of a finished bundle. Shared by the build path and --verify-only,
# so the two can never drift into checking different things.
# ---------------------------------------------------------------------------
verify_bundle() {
  local app="$1" mode="$2" expect_stapled="${3:-}" intent="${4:-dev}" expected_version="${5:-}"
  local failures=()

  if [ ! -d "$app" ]; then
    warn "  - $app does not exist or is not a bundle directory"
    return 1
  fi

  local exe_name exe
  exe_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' \
                "$app/Contents/Info.plist" 2>/dev/null || true)"
  if [ -z "$exe_name" ]; then
    failures+=("$app/Contents/Info.plist has no CFBundleExecutable — macOS cannot launch this bundle")
  else
    exe="$app/Contents/MacOS/$exe_name"
    [ -x "$exe" ] || failures+=("Contents/MacOS/$exe_name is missing or not executable")
  fi

  # 1. The signature verifies at all. --deep so a broken sidecar or nested
  #    framework counts, --strict so a sealed-resource mismatch is not waved through.
  local cs_out
  if ! cs_out="$(codesign --verify --deep --strict --verbose=2 "$app" 2>&1)"; then
    failures+=("codesign --verify --deep --strict FAILED: $(printf '%s' "$cs_out" | tr '\n' '; ')")
  fi

  # 2. The signature is the KIND that was asked for. A developer-id run that
  #    somehow produced an ad-hoc signature must fail loudly, not pass because
  #    "it is signed".
  local desc
  desc="$(codesign -dvvv "$app" 2>&1 || true)"
  local cdhash
  cdhash="$(printf '%s\n' "$desc" | sed -n 's/^CDHash=//p' | head -1)"

  case "$mode" in
    adhoc)
      if ! printf '%s\n' "$desc" | grep -q '^Signature=adhoc'; then
        failures+=("expected an ad-hoc signature; codesign reports: $(printf '%s\n' "$desc" | grep -E '^(Signature|Authority)=' | tr '\n' '; ')")
      fi
      # `Signature=adhoc` ALONE IS NOT ENOUGH, and this is the trap this check
      # exists for. An arm64 Mach-O gets an ad-hoc signature from the LINKER,
      # automatically, and `codesign -dvvv` reports `Signature=adhoc` for it. But
      # that signature covers the executable only: the surrounding bundle has no
      # sealed resources, its identifier is the linker's mangled crate name
      # (`richos_tauri-6a5998b21aa29388`) rather than CFBundleIdentifier, and
      # `codesign --verify` REJECTS it outright — "code has no resources but
      # signature indicates they must be present". Measured on the first bundle
      # this script ever produced. So the three discriminators below are what
      # separate a genuinely ad-hoc-signed bundle from an unsigned bundle wrapping
      # a linker-signed binary.
      if printf '%s\n' "$desc" | grep -qE '^CodeDirectory .*flags=.*linker-signed'; then
        failures+=("this is the LINKER's automatic signature on the executable, not a signature over the bundle — the .app itself was never signed")
      fi
      if printf '%s\n' "$desc" | grep -q '^Sealed Resources=none'; then
        failures+=("the bundle has no sealed resources, so nothing binds Info.plist or the icon to the signature — macOS rejects this bundle")
      fi
      local declared_id signed_id
      declared_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
                       "$app/Contents/Info.plist" 2>/dev/null || true)"
      signed_id="$(printf '%s\n' "$desc" | sed -n 's/^Identifier=//p' | head -1)"
      if [ -n "$declared_id" ] && [ "$signed_id" != "$declared_id" ]; then
        failures+=("the signature's identifier is '$signed_id' but the bundle declares '$declared_id' — the signed identity is not the app's identity")
      fi
      ;;
    developer-id)
      if ! printf '%s\n' "$desc" | grep -q '^Authority=Developer ID Application:'; then
        failures+=("expected a Developer ID Application signature; codesign reports: $(printf '%s\n' "$desc" | grep -E '^(Signature|Authority)=' | tr '\n' '; ')")
      fi
      # Hardened runtime is a notarization prerequisite, and codesign prints it as
      # a flag on the CodeDirectory line.
      if ! printf '%s\n' "$desc" | grep -qE '^CodeDirectory .*flags=.*runtime'; then
        failures+=("the hardened runtime is NOT enabled — Apple will refuse to notarize this bundle")
      fi
      if ! printf '%s\n' "$desc" | grep -q '^Timestamp='; then
        failures+=("no secure timestamp on the signature — required for notarization")
      fi
      local ents
      ents="$(codesign -d --entitlements :- --xml "$app" 2>/dev/null || true)"
      if ! printf '%s' "$ents" | grep -q 'com.apple.security.device.audio-input'; then
        failures+=("com.apple.security.device.audio-input is not in the signed entitlements — under the hardened runtime the app cannot open the microphone at all")
      fi

      # THE DESIGNATED REQUIREMENT — the only property the certificate was bought
      # for. macOS stores this expression against every permission it grants, so
      # whether the CEO's microphone survives the next build is decided here and
      # nowhere else. A signature can be a valid Developer ID signature and still
      # carry a cdhash-shaped requirement, which is ad-hoc's failure wearing a
      # certificate; nothing else in this script would notice.
      local dr declared
      dr="$(codesign -d -r- "$app" 2>/dev/null | sed -n 's/^#\{0,1\} *designated => //p' | head -1)"
      declared="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
                    "$app/Contents/Info.plist" 2>/dev/null || true)"
      if [ -z "$dr" ]; then
        failures+=("codesign printed no designated requirement, so there is nothing for macOS to bind a permission grant to")
      else
        BUNDLE_DR="$dr"
        if printf '%s' "$dr" | grep -q 'cdhash'; then
          failures+=("the designated requirement is a cdhash expression — this build's permission grants die on the next build exactly as an ad-hoc one's do. Requirement: $dr")
        fi
        if [ -n "$declared" ] && ! printf '%s' "$dr" | grep -Fq "identifier \"$declared\""; then
          failures+=("the designated requirement does not pin the bundle identifier '$declared' — requirement: $dr")
        fi
        printf '%s' "$dr" | grep -q 'anchor apple generic' \
          || failures+=("the designated requirement has no 'anchor apple generic' clause — requirement: $dr")
        printf '%s' "$dr" | grep -q 'certificate leaf\[subject.OU\]' \
          || failures+=("the designated requirement does not pin the team (certificate leaf[subject.OU]) — requirement: $dr")
      fi

      # A stapled ticket, when notarization was asked for. "Notarized" without one
      # means the app must reach Apple over the network to be admitted, which on an
      # offline machine is indistinguishable from not being notarized at all.
      if [ "$expect_stapled" = "1" ]; then
        local staple_out
        if ! staple_out="$(xcrun stapler validate "$app" 2>&1)"; then
          failures+=("no valid stapled notarization ticket: $(printf '%s' "$staple_out" | tr '\n' '; ')")
        fi
      fi
      ;;
  esac

  # 3. The icon that SHIPPED is the icon this repository generated. The build
  #    gate checks the source files; this checks what came out the other end,
  #    which is the only copy the CEO ever sees.
  local shipped="$app/Contents/Resources/icon.icns"
  local source_icns="$src_tauri/icons/icon.icns"
  if [ ! -f "$shipped" ]; then
    failures+=("Contents/Resources/icon.icns is missing — the bundle ships with no macOS icon")
  elif [ ! -f "$source_icns" ]; then
    failures+=("$source_icns does not exist, so the shipped icon cannot be checked against its source")
  elif ! cmp -s "$shipped" "$source_icns"; then
    failures+=("Contents/Resources/icon.icns differs from src-tauri/icons/icon.icns — the bundle is carrying an icon this repository did not generate")
  fi

  # 3b. The third-party notices SHIPPED inside the bundle. The executable compiles in the
  #     American spelling table, a modified version of VarCon, whose Ispell notice requires
  #     its terms to travel with binaries built from modified source; tauri.conf.json's
  #     bundle.resources copies docs/legal/THIRD-PARTY-NOTICES.md to Contents/Resources.
  #     Checked by content, not bytes, so --verify-only still reads an older bundle: the
  #     page must be there and must carry its bundled-data section.
  local notices="$app/Contents/Resources/THIRD-PARTY-NOTICES.md"
  if [ ! -f "$notices" ]; then
    failures+=("Contents/Resources/THIRD-PARTY-NOTICES.md is missing — the binary compiles in VarCon-derived data whose notice must ship with it")
  elif ! grep -q '^## Bundled data compiled into the application' "$notices"; then
    failures+=("Contents/Resources/THIRD-PARTY-NOTICES.md has no \"Bundled data compiled into the application\" section — the notice for the compiled-in spelling table is not in it")
  fi

  # 4. The microphone usage string survived Tauri's Info.plist merge. Flagged
  #    unverified in src-tauri/Info.plist's own comment and in the signing wiki;
  #    without it macOS never shows the permission prompt.
  local mic
  mic="$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' \
           "$app/Contents/Info.plist" 2>/dev/null || true)"
  if [ -z "$mic" ]; then
    failures+=("Contents/Info.plist has no NSMicrophoneUsageDescription — voice mode cannot even ask for the microphone")
  fi

  # 5. THE VERSION THIS BUNDLE WILL CALL ITSELF, and whether any release is that version.
  #
  # CEO, 2026-09-17, item 4: *"The RichOS app is currently lying about the current version
  # saying "RichOS 1.0.3 is up to date." even though the current version is 1.0.2. Make sure
  # the app always displays the correct version."*
  #
  # WHAT ACTUALLY HAPPENED, and it is why this check reports rather than refuses. The bundle
  # he was running IS 1.0.3 — `~/Applications/RichOS.app/Contents/Info.plist` reports
  # CFBundleShortVersionString = 1.0.3, dated 2026-09-08 12:01 — and `updates.rs` printed
  # that number honestly, because `app.package_info().version` is exactly what the bundle was
  # compiled as. The number entered the tree at 7bd1e727 (2026-09-07, "prepare version
  # 1.0.3"), which set Cargo.toml to the NEXT UNRELEASED stable version, the convention
  # `nightly.py` states in as many words. No v1.0.3 tag and no 1.0.3 release ever existed;
  # main went on to 1.2.0 at d9cd1b6c. So a development build was stamped with a version
  # reserved for a release that never happened, handed to him, and had no way to say so.
  #
  # THIS IS THE MOMENT THAT FACT IS KNOWABLE, so it is the moment it is checked. IT USED TO
  # ONLY REPORT, on the theory that intent could not be read: `make-release.sh` builds the
  # bundle BEFORE it creates `v$VERSION`, so every genuine release build ran with no tag
  # either, and nothing distinguished it from the 2026-09-08 bundle at this point. That
  # theory is now false — the caller declares intent (`$intent`, resolved from --release /
  # --nightly / neither, above) — so the bare-version case below is a REFUSAL, closing the
  # open half of this incident (CEO item 4, esc-20260917T081214Z-b466c998).
  local short_version
  short_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
                     "$app/Contents/Info.plist" 2>/dev/null || true)"
  if [ -z "$short_version" ]; then
    failures+=("Contents/Info.plist has no CFBundleShortVersionString — this bundle cannot tell anyone which version it is, and the settings menu would render \"RichOS \" with nothing after it")
  else
    BUNDLE_VERSION="$short_version"
    # THE RAW, UNRELEASED VERSION IN Cargo.toml — kept separate from `intended` below, and
    # read regardless of any override, because it is the specific value that must never ship
    # undeclared: `make-release.sh` sets it to the NEXT unreleased number (7bd1e727, "prepare
    # version 1.0.3", is exactly that), and a plain build used to inherit it verbatim.
    local cargo_version
    cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$src_tauri/Cargo.toml" 2>/dev/null | head -1 || true)"

    # The version this run INTENDED. `expected_version`, when the caller supplies it (the
    # real build path always does — package-app.sh resolves it before compiling, from the
    # same declared intent as the refusal below), is authoritative and skips both fallbacks.
    # Without it — the --verify-only path, checking a bundle nobody here just built —
    # Cargo.toml is the only copy (`make-release.sh` refuses a `version` key in
    # tauri.conf.json for exactly that reason) — except for the one caller that overrides it
    # on purpose: `updater-e2e.sh` builds 0.1.0 and 0.1.1 from one tree through
    # RICHOS_EXTRA_TAURI_CONFIG, and both of those bundles are correct.
    local intended="$expected_version"
    if [ -z "$intended" ]; then
      intended="$cargo_version"
      if [ -n "${RICHOS_EXTRA_TAURI_CONFIG:-}" ]; then
        local overridden
        overridden="$(printf '%s' "$RICHOS_EXTRA_TAURI_CONFIG" \
          | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("version") or "")
except Exception: print("")' 2>/dev/null || true)"
        [ -n "$overridden" ] && intended="$overridden"
      fi
    fi
    if [ -n "$expected_version" ]; then
      # The real build path already knows exactly what it intended (`target_version`,
      # which accounts for --release / --nightly / the development stamp /
      # RICHOS_EXTRA_TAURI_CONFIG, in that precedence) — checked exactly, no exemption.
      if [ "$short_version" != "$intended" ]; then
        failures+=("the bundle says it is $short_version and this build intended $intended. A manifest announcing one version over a bundle that is another is how an installed copy reports a version nobody shipped — the first version of the updater artifact line hit exactly this and announced 0.1.0 over a 0.1.1 bundle.")
      fi
    else
      # No `expected_version` — a --verify-only call, checking a bundle nobody here just
      # built — so `intended` fell back to the bare `cargo_version` above. A `-dev.` or
      # `-nightly.` stamp of THAT SAME base is deliberately not equal to it and is not the
      # thing this check exists to catch: that is a DIFFERENT base being announced (0.1.0
      # over a 0.1.1 bundle), not a sanctioned pre-release suffix on the correct one.
      case "$short_version" in
        "${cargo_version}-dev."*|"${cargo_version}-nightly."*) ;;
        *)
          if [ -n "$intended" ] && [ "$short_version" != "$intended" ]; then
            failures+=("the bundle says it is $short_version and this build intended $intended. A manifest announcing one version over a bundle that is another is how an installed copy reports a version nobody shipped — the first version of the updater artifact line hit exactly this and announced 0.1.0 over a 0.1.1 bundle.")
          fi
          ;;
      esac
    fi

    # THE REFUSAL ITSELF. A version that is EXACTLY Cargo.toml's bare, unreleased number —
    # plain X.Y.Z, no pre-release identifier at all — with nothing declaring --release for
    # it, is precisely what he was handed: it tells whoever runs it "RichOS $short_version is
    # up to date" about a number nobody ever shipped. Only --release exempts this;
    # declaring --nightly does not, because a nightly build's Cargo.toml already carries its
    # `-nightly.` suffix by the time this runs (nightly.py writes it in before calling
    # make-release.sh) — if a nightly build ever produced a bare number anyway, that is a bug
    # this refusal should also catch. A `-dev.` or `-nightly.` version never matches this
    # pattern in the first place, so neither an ordinary development build nor a real
    # nightly needs to declare anything to pass it.
    if [ "$intent" != "release" ] && [ -n "$cargo_version" ] && [ "$short_version" = "$cargo_version" ] \
       && printf '%s' "$short_version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      failures+=("this bundle's version is $short_version — Cargo.toml's bare, unreleased next version — and nothing declared --release for it. Pass --release for an actual release build (make-release.sh and nightly.py already do), or leave this as an ordinary run and it will stamp itself \"${short_version}-dev.<commit>\" instead, the way an undeclared build now does by default.")
    fi
  fi

  # 6. THE BUNDLE HOLDS NO STATE — spec point 17, checked at BUILD TIME rather than at a
  #    customer's next update.
  #
  # Point 17 is already enforced structurally by the updater: the receipt carries a
  # `tree_sha256` over the bundle tree and the installer refuses when the tree no longer
  # matches. That failure is correct and it is loud, but it arrives at the worst possible
  # moment — on the CEO's machine, at the next update. The spec asks for the same fact to be
  # discovered here, where it costs a build instead of an install.
  #
  # ## THE ASSERTION THE SPEC NAMES CANNOT BE WRITTEN, AND THIS IS WHAT REPLACES IT
  #
  # Point 17 says: "a packaging-test assertion that no shipped path inside `Contents/` is
  # owner-writable". Measured on this Mac, 2026-09-17, with
  # `find <app>/Contents -perm -u+w | wc -l` against `find <app>/Contents | wc -l`:
  #
  #     ~/Applications/RichOS.app          8 of 8        owner-writable
  #     /System/Applications/Calculator.app  111 of 111  owner-writable
  #     /Applications/Safari.app         2044 of 2044    owner-writable
  #
  # 2163 of 2163. Owner-writable is the NORMAL macOS bundle layout — 0755 directories and
  # 0644 files — so that assertion would refuse every correct bundle ever built, Apple's
  # included. It is not a strict check that needs softening; it is the wrong predicate.
  #
  # The same command found group- or world-writable: 0 of 2163. THAT is the permission bit
  # that is always a defect inside a signed bundle — another user able to write into the app
  # is a privilege-escalation path and breaks the seal for everyone. So the writability half
  # is checked as `-g+w -o -o+w`.
  #
  # And the half that is actually point 17 — "the bundle holds no STATE" — is a question
  # about WHICH PATHS SHIPPED, not about their mode. Every durable store this app owns is
  # enumerated in the spec's own point 16 table and in `ledger.rs`'s survey; none of them is
  # a plausible bundle resource, and a `.jsonl` file inside an application bundle has no
  # honest reason to exist at all. A bundle that ships one is a state-in-bundle defect
  # whatever its permissions say, and that is the case this catches.
  local unsafe_paths=()
  while IFS= read -r p; do
    [ -n "$p" ] && unsafe_paths+=("${p#$app/}")
  done < <(find "$app/Contents" \( -perm -g+w -o -perm -o+w \) 2>/dev/null)
  if [ ${#unsafe_paths[@]} -gt 0 ]; then
    failures+=("$(printf '%d path(s) inside Contents/ are writable by group or other, so another account on this Mac can rewrite the signed app: %s' \
                  "${#unsafe_paths[@]}" "$(printf '%s ' "${unsafe_paths[@]}")")")
  fi

  local state_paths=()
  while IFS= read -r p; do
    [ -n "$p" ] && state_paths+=("${p#$app/}")
  done < <(find "$app/Contents" \( \
             -name '*.jsonl' -o -name 'config.json' -o -name 'launches.json' \
             -o -name 'navigation.json' -o -name 'entities.json' \
             -o -name 'owned-work-baseline.json' -o -name 'engine-state' \
             -o -name 'run-notices' -o -name 'voice-scratch' -o -name 'runs' \
             \) 2>/dev/null)
  if [ ${#state_paths[@]} -gt 0 ]; then
    failures+=("$(printf 'the bundle ships %d path(s) that are this app'"'"'s own durable state, which must live in the data directory and survive a rollback: %s. Spec point 17 — the bundle holds no state.' \
                  "${#state_paths[@]}" "$(printf '%s ' "${state_paths[@]}")")")
  fi

  # Finished-bundle checks also run for --verify-only. Engine delivery, phone
  # embedding and release-channel rules remain with their existing owners.
  if ! python3 "$here/lib/bundle_invariants.py" "$app"; then
    failures+=("named bundle invariants failed (see diagnostics above)")
  fi

  if [ ${#failures[@]} -gt 0 ]; then
    warn ""
    warn "FAILED — the bundle is not shippable:"
    warn ""
    local f
    for f in "${failures[@]}"; do warn "  - $f"; done
    warn ""
    return 1
  fi

  BUNDLE_CDHASH="$cdhash"
  BUNDLE_MIC="$mic"
  return 0
}

# ---------------------------------------------------------------------------
# WHAT THIS BUNDLE WILL TELL THE CEO IT IS, said out loud at the moment it is
# knowable. Called from both exits of this script, so the build path and
# --verify-only cannot drift into saying different things.
#
# It states a fact and draws no conclusion, because the fact has two innocent
# readings and one bad one, and only the person running it knows which:
#
#   a release of this version exists    -> this is that release, or a rebuild of it
#   it does not, and this IS a release  -> normal: make-release.sh tags AFTER it builds
#   it does not, and this is not        -> the 2026-09-08 bundle. A build stamped with
#                                          a version reserved for a release that never
#                                          happened, telling him "RichOS 1.0.3 is up to
#                                          date" about a number nobody ever shipped.
# ---------------------------------------------------------------------------
say_version_identity() {
  [ -n "$BUNDLE_VERSION" ] || return 0
  say ""
  say "  version         : $BUNDLE_VERSION   (app/src-tauri/Cargo.toml, the only copy)"
  case "$BUNDLE_VERSION" in
    *-dev.*)
      say "                    development build — not a release."
      return 0
      ;;
  esac
  local tagged=""
  if command -v git >/dev/null 2>&1; then
    tagged="$(git -C "$src_tauri" rev-parse -q --verify "refs/tags/v$BUNDLE_VERSION^{commit}" 2>/dev/null || true)"
  fi
  if [ -n "$tagged" ]; then
    say "                    v$BUNDLE_VERSION is a release in this repository ($tagged)."
  else
    say "                    NO v$BUNDLE_VERSION RELEASE EXISTS in this repository. This bundle"
    say "                    will tell the CEO it is \"RichOS $BUNDLE_VERSION\". That is expected"
    say "                    while MAKING that release — make-release.sh tags after it builds —"
    say "                    and it means a development build stamped with an unreleased number"
    say "                    at any other time. Do not hand this copy to him as a release."
  fi
}

# ---------------------------------------------------------------------------
# --verify-only: re-check a bundle built earlier, no compile.
# ---------------------------------------------------------------------------
if [ -n "$verify_only" ]; then
  say "verifying an existing bundle: $verify_only"
  say "  expected signing mode: $sign_mode"
  say "  declared intent      : $intent"
  [ -n "$expect_notarized" ] && say "  expected: a stapled notarization ticket"
  if ! verify_bundle "$verify_only" "$sign_mode" "$expect_notarized" "$intent"; then
    exit 1
  fi
  say_version_identity
  say ""
  say "OK: $(basename "$verify_only") verifies — ${sign_mode} signature, cdhash ${BUNDLE_CDHASH}, version ${BUNDLE_VERSION}, shipped icon matches this repository, microphone usage string present.${BUNDLE_DR:+ Designated requirement: ${BUNDLE_DR}}"
  exit 0
fi

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------
if [ "$(uname -s)" != "Darwin" ]; then
  warn "error: this entrypoint builds the macOS bundle and runs on macOS only."
  warn "       (uname -s reports $(uname -s).) No Windows bundle has ever been"
  warn "       built for RichOS and there is no Windows signing certificate —"
  warn "       richos-hq/wiki/packaging-and-signing.md, 'Windows'."
  exit 3
fi

# cargo is frequently absent from a non-login shell's PATH even when it is
# installed. Adding its own well-known location is not a guess about signing;
# it is stated out loud when it happens.
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
  say "note: cargo was not on PATH; using $HOME/.cargo/bin (source \$HOME/.cargo/env to make that permanent)"
fi

if ! command -v cargo >/dev/null 2>&1; then
  warn "error: cargo not found on PATH. Install Rust (https://rustup.rs) and re-run."
  exit 3
fi

if ! cargo tauri --version >/dev/null 2>&1; then
  warn "error: the Tauri CLI is not installed for this cargo."
  warn ""
  warn "  cargo install tauri-cli --version '^2'"
  warn ""
  exit 3
fi

if ! command -v python3 >/dev/null 2>&1; then
  warn "error: python3 not found on PATH; it runs the icon pre-flight."
  exit 3
fi

# ---------------------------------------------------------------------------
# ONE PACKAGING RUN AT A TIME IN THIS BUNDLE OUTPUT, from any checkout.
# ---------------------------------------------------------------------------
# `app/src-tauri/target` is a symlink into one cache for every checkout
# (`shared-build-cache.sh`), and CARGO_TARGET_DIR can name one directory for several.
# Cargo's own lock covers the compile and ends when cargo returns; everything after it
# here (the host-path check, the commit stamp, both signatures, the updater archive) reads
# and rewrites `release/bundle` by name. So a second checkout's build could replace the
# bundle this run is signing. Held until this script and everything it started have exited;
# a caller that holds it already (make-release.sh app) is not blocked by it (hunt part 2,
# section 21). Taken before the signing mode is resolved, so no step below runs unheld. A
# --dry-run stops at that resolution and writes nothing, so it neither waits nor holds.
. "$here/lib/cargo-target.sh"
. "$here/lib/output-lock.sh"
if [ -z "$dry_run" ]; then
  hold_output_lock "$(cargo_target_dir "$src_tauri")/release" 9 "package-app.sh" || exit 3
fi

# ---------------------------------------------------------------------------
# Signing mode: resolved before the build, so a refusal costs no compile time.
# ---------------------------------------------------------------------------
identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
identity_count="$(printf '%s\n' "$identities" | sed -n 's/^ *\([0-9][0-9]*\) valid identities* found/\1/p' | tail -1)"
identity_count="${identity_count:-0}"

tauri_config_overlay=""

# Every Developer ID Application identity this machine reports, one per line, as
# the quoted string codesign wants. Read once; the count is what decides.
devid_lines="$(printf '%s\n' "$identities" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p')"
devid_count="$(printf '%s' "$devid_lines" | grep -c . || true)"

notary_args=()
notary_desc=""

if [ "$sign_mode" = "developer-id" ]; then
  wanted="${RICHOS_SIGNING_IDENTITY:-}"

  # ---- Discovery. Exactly one is an answer; none and several are questions. ----
  if [ -z "$wanted" ]; then
    if [ "$devid_count" -eq 1 ]; then
      wanted="$devid_lines"
      say "note: discovered the one Developer ID Application identity on this machine:"
      say "        $wanted"
      say "      (set RICHOS_SIGNING_IDENTITY to pin a different one.)"
    elif [ "$devid_count" -eq 0 ]; then
      warn ""
      warn "REFUSING — --sign developer-id was requested and this machine has no"
      warn "Developer ID Application identity."
      warn ""
      warn "  'security find-identity -v -p codesigning' reports ${identity_count} codesigning"
      warn "  identit(y/ies), none of them Developer ID Application."
      warn ""
      warn "  The Apple Developer Program membership exists as of 2026-08-31; the"
      warn "  CERTIFICATE is a separate step and has not been created. Two commands:"
      warn ""
      warn "    richos/app/scripts/make-signing-csr.sh                      (already run? --show)"
      warn "    ...upload the CSR at developer.apple.com/account, download the .cer..."
      warn "    richos/app/scripts/install-signing-cert.sh <the .cer>"
      warn ""
      warn "  docs/ceo/developer-id-setup-2026-08-31.md is the middle step in plain"
      warn "  language. This script will NOT quietly fall back to ad-hoc when you"
      warn "  asked for Developer ID."
      warn ""
      exit 2
    else
      warn ""
      warn "REFUSING — ${devid_count} Developer ID Application identities are on this machine:"
      warn ""
      printf '%s\n' "$devid_lines" | sed 's/^/    /' >&2
      warn ""
      warn "  Choosing between two signing certificates is not a decision a script may"
      warn "  make by sorting. Pin one:"
      warn ""
      warn "    export RICHOS_SIGNING_IDENTITY='$(printf '%s\n' "$devid_lines" | head -1)'"
      warn ""
      exit 2
    fi
  elif ! printf '%s\n' "$identities" | grep -Fq -- "$wanted"; then
    warn ""
    warn "REFUSING — RICHOS_SIGNING_IDENTITY is set to:"
    warn ""
    warn "    $wanted"
    warn ""
    warn "  but 'security find-identity -v -p codesigning' does not report it on this"
    warn "  machine (${identity_count} valid identit(y/ies) found). Signing with an identity"
    warn "  that is not there cannot work, and falling back to ad-hoc would silently"
    warn "  produce a bundle whose permissions die on the next rebuild — exactly the"
    warn "  failure Developer ID was asked for. Refusing instead."
    warn ""
    exit 2
  fi

  ents="$src_tauri/Entitlements.plist"
  if [ ! -f "$ents" ]; then
    warn "error: $ents is missing; the hardened runtime needs it for the microphone."
    exit 2
  fi
  # Applied ONLY on this path, as a config overlay, so an ordinary build and the
  # ad-hoc path are untouched by it.
  tauri_config_overlay="$(python3 - "$wanted" "$ents" <<'PY'
import json, sys
print(json.dumps({"bundle": {"macOS": {
    "signingIdentity": sys.argv[1],
    "entitlements": sys.argv[2],
    "hardenedRuntime": True,
}}}))
PY
)"
  export APPLE_SIGNING_IDENTITY="$wanted"

  # ---- Notary credentials, resolved BEFORE the build so a typo costs no compile ----
  if [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
    if ! xcrun --find notarytool >/dev/null 2>&1; then
      warn "error: 'xcrun notarytool' is not available. Install the Xcode command line"
      warn "       tools (xcode-select --install); notarytool ships with them."
      exit 3
    fi

    nkey="${RICHOS_NOTARY_KEY:-}"
    nkey_id="${RICHOS_NOTARY_KEY_ID:-}"
    nissuer="${RICHOS_NOTARY_ISSUER:-}"
    nprofile="${RICHOS_NOTARY_PROFILE:-}"

    if [ -n "$nkey" ] || [ -n "$nkey_id" ] || [ -n "$nissuer" ]; then
      missing=""
      [ -n "$nkey" ]    || missing="$missing RICHOS_NOTARY_KEY"
      [ -n "$nkey_id" ] || missing="$missing RICHOS_NOTARY_KEY_ID"
      [ -n "$nissuer" ] || missing="$missing RICHOS_NOTARY_ISSUER"
      if [ -n "$missing" ]; then
        warn ""
        warn "REFUSING — an App Store Connect API key was started and not finished."
        warn "Unset:$missing"
        warn ""
        warn "  All three come from the same page (App Store Connect -> Users and"
        warn "  Access -> Integrations -> Keys): the .p8 file, its Key ID, and the"
        warn "  Issuer ID shown above the table."
        warn ""
        exit 2
      fi
      if [ ! -f "$nkey" ]; then
        warn ""
        warn "REFUSING — RICHOS_NOTARY_KEY does not exist: $nkey"
        warn ""
        warn "  Apple permits that .p8 to be downloaded exactly once. If it is gone,"
        warn "  revoke the key in App Store Connect and create a new one; there is no"
        warn "  second download."
        warn ""
        exit 2
      fi
      nkey_dir="$(cd "$(dirname "$nkey")" && pwd)"
      if inside="$(cd "$nkey_dir" && git rev-parse --show-toplevel 2>/dev/null)" && [ -n "$inside" ]; then
        warn ""
        warn "REFUSING — the notary key is inside a git worktree:"
        warn ""
        warn "    $nkey"
        warn "    is under $inside"
        warn ""
        warn "  Apple lets that file be downloaded exactly once, and a copy inside a"
        warn "  repository is one 'git add -f' from being in the history forever. Move"
        warn "  it outside every checkout — \$HOME/.richos-signing is already 0700 and"
        warn "  holds the signing key."
        warn ""
        exit 2
      fi
      nkey_mode="$(stat -f '%Lp' "$nkey")"
      # Group digit OR other digit carrying a read bit: a 640 key is readable by every member
      # of its group, which is other users of this Mac whenever the directory lets them in.
      case "$nkey_mode" in
        *[4567]?|*?[4567])
          warn ""
          warn "REFUSING — $nkey is mode $nkey_mode: readable by other users of this Mac."
          warn "  chmod 600 it and re-run."
          warn ""
          exit 2 ;;
      esac
      notary_args=(--key "$nkey" --key-id "$nkey_id" --issuer "$nissuer")
      notary_desc="App Store Connect API key $nkey_id"
    elif [ -n "$nprofile" ]; then
      notary_args=(--keychain-profile "$nprofile")
      notary_desc="keychain profile '$nprofile'"
    else
      warn ""
      warn "REFUSING — RICHOS_NOTARIZE=1 with no notary credentials."
      warn ""
      warn "  Preferred, and what docs/ceo/developer-id-setup-2026-08-31.md sets up:"
      warn "    RICHOS_NOTARY_KEY=\$HOME/.richos-signing/AuthKey_XXXXXXXXXX.p8"
      warn "    RICHOS_NOTARY_KEY_ID=XXXXXXXXXX"
      warn "    RICHOS_NOTARY_ISSUER=<the issuer UUID>"
      warn ""
      warn "  Or, if credentials are already in the keychain:"
      warn "    RICHOS_NOTARY_PROFILE=<the profile name>"
      warn "    (created once with: xcrun notarytool store-credentials)"
      warn ""
      warn "  There is deliberately no APPLE_PASSWORD path — an app-specific password"
      warn "  in the environment is readable by every process this build spawns and"
      warn "  lands in shell history. Put it in a keychain profile once instead;"
      warn "  nothing is lost and a standing credential leak is."
      warn ""
      warn "  Refusing rather than producing a bundle that would be called notarized"
      warn "  and would not be."
      warn ""
      exit 2
    fi
  fi
fi

# ---------------------------------------------------------------------------
# The updater signing key, resolved BEFORE the build for the same reason the
# notary credentials are: a typo must not cost a release compile.
#
# It is a MINISIGN key and has nothing to do with Apple. Tauri's updater verifies
# every downloaded byte against `plugins.updater.pubkey` before it installs
# anything, and that check is independent of codesigning — which is why the whole
# update path is provable today, on an ad-hoc bundle, with no certificate.
# ---------------------------------------------------------------------------
updater_key_path=""
updater_key_desc=""

if [ -n "$updater" ]; then
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]; then
    updater_key_path="$TAURI_SIGNING_PRIVATE_KEY_PATH"
    if [ ! -f "$updater_key_path" ]; then
      warn ""
      warn "REFUSING — TAURI_SIGNING_PRIVATE_KEY_PATH does not exist: $updater_key_path"
      warn ""
      warn "  Generate one with:  cargo tauri signer generate -w \$HOME/.richos-signing/<name>.key"
      warn "  and put its PUBLIC half in tauri.conf.json's plugins.updater.pubkey."
      warn ""
      exit 2
    fi
    key_dir="$(cd "$(dirname "$updater_key_path")" && pwd)"
    if inside="$(cd "$key_dir" && git rev-parse --show-toplevel 2>/dev/null)" && [ -n "$inside" ]; then
      warn ""
      warn "REFUSING — the updater signing key is inside a git worktree:"
      warn ""
      warn "    $updater_key_path"
      warn "    is under $inside"
      warn ""
      warn "  This key is the ONLY thing standing between a manifest URL and code"
      warn "  running on the CEO's Mac. A copy inside a repository is one 'git add -f'"
      warn "  from being in the history forever, and a leaked updater key cannot be"
      warn "  revoked — the public half is compiled into every installed copy."
      warn "  \$HOME/.richos-signing is already 0700."
      warn ""
      exit 2
    fi
    key_mode="$(stat -f '%Lp' "$updater_key_path")"
    # Same rule as the notary key above: the group digit counts, not only the other digit.
    case "$key_mode" in
      *[4567]?|*?[4567])
        warn ""
        warn "REFUSING — $updater_key_path is mode $key_mode: readable by other users of this Mac."
        warn "  chmod 600 it and re-run."
        warn ""
        exit 2 ;;
    esac
    updater_key_desc="$updater_key_path (mode $key_mode)"
  elif [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    # The key as a STRING, which is how a CI secret arrives. Nothing is written to
    # disk and nothing is echoed; only the fact that it was found is reported.
    updater_key_desc="TAURI_SIGNING_PRIVATE_KEY (in the environment, not written to disk)"
  else
    warn ""
    warn "REFUSING — --updater was requested and no updater signing key is set."
    warn ""
    warn "  Set ONE of:"
    warn "    TAURI_SIGNING_PRIVATE_KEY_PATH=\$HOME/.richos-signing/<name>.key"
    warn "    TAURI_SIGNING_PRIVATE_KEY=<the key's contents>"
    warn "  ...plus TAURI_SIGNING_PRIVATE_KEY_PASSWORD (empty string for a passwordless key)."
    warn ""
    warn "  There is no fallback and there must not be: an UNSIGNED update artifact is"
    warn "  refused by every installation, so producing one silently would ship a"
    warn "  release that cannot be installed and would not say so until it was public."
    warn ""
    exit 2
  fi
fi

# The bundler notarizes on its own when it can resolve APPLE_* credentials, and
# warns-and-continues on nearly every credential error — see this file's header.
# This script owns that step, so the bundler must not find them. Unset for the
# whole run: the ad-hoc path has no business with them either.
unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH

# ---------------------------------------------------------------------------
# THE VERSION THIS BUILD WILL CARRY, decided by the intent declared above and
# never inferred. release/nightly builds are UNCHANGED: Cargo.toml already
# holds the number they will ship (make-release.sh refuses a `version` key in
# tauri.conf.json for exactly that reason, and nightly.py writes its
# `-nightly.` suffix into Cargo.toml before this script ever runs — --nightly
# only cross-checks that the caller and Cargo.toml agree). A development
# build — the default, with neither flag — gets a version nothing can mistake
# for a release: Cargo.toml's base plus this commit,
# `{cargo-version}-dev.{shortsha}`, with `.dirty` appended when the tree is
# not clean. Applied the same way `updater-e2e.sh`'s own version override
# already is proven to work: as an EARLIER --config overlay, so
# RICHOS_EXTRA_TAURI_CONFIG (that one caller) still wins if it also sets
# `version` — reflected below too, so what verify_bundle is told to expect is
# what will actually get compiled.
# ---------------------------------------------------------------------------
cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$src_tauri/Cargo.toml" 2>/dev/null | head -1 || true)"
if [ -z "$cargo_version" ]; then
  warn "error: could not read a version from $src_tauri/Cargo.toml"
  exit 3
fi

version_overlay=""
target_version="$cargo_version"

case "$intent" in
  release)
    : # unchanged — the bundle ships whatever Cargo.toml already says
    ;;
  nightly)
    if [ -n "$nightly_version" ] && [ "$nightly_version" != "$cargo_version" ]; then
      warn ""
      warn "REFUSING — --nightly $nightly_version was declared and"
      warn "$src_tauri/Cargo.toml says $cargo_version. nightly.py always writes the nightly"
      warn "version into Cargo.toml before building; the two disagreeing means either the"
      warn "wrong tree is checked out, or the wrong version was declared."
      warn ""
      exit 2
    fi
    ;;
  dev)
    dev_repo_root="$(git -C "$app_dir" rev-parse --show-toplevel 2>/dev/null || true)"
    dev_sha=""
    [ -n "$dev_repo_root" ] && dev_sha="$(git -C "$dev_repo_root" rev-parse --short HEAD 2>/dev/null || true)"
    if [ -z "$dev_sha" ]; then
      warn ""
      warn "REFUSING — this is a development build (neither --release nor --nightly was"
      warn "declared) and this checkout has no git HEAD to stamp it with. A development"
      warn "build is stamped {version}-dev.{commit} precisely so it can never be mistaken"
      warn "for a release; run from a git checkout, or pass --release / --nightly <version>"
      warn "for a build that genuinely is one."
      warn ""
      exit 2
    fi
    dev_dirty=""
    if [ -n "$(git -C "$dev_repo_root" status --porcelain --untracked-files=all 2>/dev/null || true)" ]; then
      dev_dirty=".dirty"
    fi
    target_version="${cargo_version}-dev.${dev_sha}${dev_dirty}"
    version_overlay="{\"version\": \"${target_version}\"}"
    ;;
esac

if [ -n "${RICHOS_EXTRA_TAURI_CONFIG:-}" ]; then
  extra_version="$(printf '%s' "$RICHOS_EXTRA_TAURI_CONFIG" \
    | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("version") or "")
except Exception: print("")' 2>/dev/null || true)"
  [ -n "$extra_version" ] && target_version="$extra_version"
fi

# ---------------------------------------------------------------------------
# THE COMPILE RUNS UNDER THE CPU GUARD'S release-build ROLE, BY DEFAULT.
# ---------------------------------------------------------------------------
# rustc's LLVM phase uses a core per jobserver token, and the guard's ordinary rule stops a
# process above 3 cores for 10 s. On 2026-10-11 it stopped three engineers' development
# builds (rustc at 4.5 to 7.3 cores) and each rebuilt by hand under
# `proc_tree.py run <pid> --guard-role release-build`, which is what nightly-local.py does
# for its build step (owned_command.py). The cargo build below now runs under that same
# supervisor and role: the compiler gets the guard's longer window (cpu_guard.BUILD_WINDOW),
# the limit still holds, and the tree is stopped if this script is. The supervisor is owned
# by this script's pid. Nothing else here runs under it.
#
# WHERE MANAGED VERIFICATION IS ON, the supervisor admits only work under a current proof-run
# plan or its live admitted ancestor, and refuses everything else before it starts (exit 75,
# proc_tree.supervise). A packaging run started by hand has neither, so it would refuse before
# cargo ran (review rv-20261011T142043Z-4d2b357d-d8c1 #1). That case is asked FIRST, with the
# supervisor's own two questions (cpu_guard.verification_enabled and
# inherited_verification_owner), and the compile then runs as it did before the supervisor:
# directly, under the ordinary CPU guard rule, said so on screen. The admission rule itself is
# untouched: nothing here registers or claims admission. A probe that cannot answer keeps the
# supervisor, whose own refusal then names the reason.
build_launch=()
build_launch_why=""
proc_tree="$(cd "$app_dir/.." && pwd)/engine/scripts/lib/proc_tree.py"
if [ -f "$proc_tree" ]; then
  build_launch=(python3 "$proc_tree" run "$$" --guard-role release-build --)
  admission_rc=0
  python3 -B - "$(dirname "$proc_tree")" <<'PY' || admission_rc=$?
import os
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cpu_guard
managed = cpu_guard.verification_enabled()
owner = cpu_guard.inherited_verification_owner(os.environ.get("RICHOS_VERIFICATION_OWNER"))
sys.exit(3 if managed and owner is None else 0)
PY
  if [ "$admission_rc" = 3 ]; then
    build_launch=()
    build_launch_why="no supervisor: managed verification is on and this run is not under a proof-run plan, so the supervisor would refuse it; the 10-second CPU guard rule applies"
  fi
else
  build_launch_why="no supervisor ($proc_tree is missing), so the 10-second CPU guard rule applies"
fi

# ---------------------------------------------------------------------------
# --dry-run: everything decided, nothing built.
#
# The signing configuration is the part of a release that is wrong for weeks
# before anyone finds out, and it is fully determined by this point: the mode,
# the identity, the entitlements, and whether the notary credentials resolve.
# A release compile costs a quarter of an hour and answers none of it, so this
# stops here and prints what it resolved. It exits 0 because a resolution that
# refuses has already exited 2 above.
# ---------------------------------------------------------------------------
if [ -n "$dry_run" ]; then
  say ""
  say "DRY RUN — the signing configuration resolved, and nothing was built."
  say ""
  say "  intent              : $intent"
  say "  version             : $target_version$( [ "$intent" = "dev" ] && printf ' (app/src-tauri/Cargo.toml + this commit; pass --release or --nightly <version> to change that)' )"
  say "  signing mode        : $sign_mode"
  if [ "$sign_mode" = "developer-id" ]; then
    say "  identity            : $wanted"
    say "  identity came from  : ${RICHOS_SIGNING_IDENTITY:+RICHOS_SIGNING_IDENTITY}${RICHOS_SIGNING_IDENTITY:-discovery (exactly one on this machine)}"
    say "  entitlements        : $src_tauri/Entitlements.plist"
    say "  hardened runtime    : yes (and the bundle is re-signed with --timestamp)"
    if [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
      say "  notarization        : ON via $notary_desc, submitted and stapled by this script"
    else
      say "  notarization        : OFF (RICHOS_NOTARIZE is not 1)"
    fi
  else
    say "  identity            : none — ad-hoc, and its grants die on the next shipped byte"
    if [ "$devid_count" != "0" ]; then
      say ""
      say "  LOUD NOTE: this machine has ${devid_count} Developer ID Application identit(y/ies), so a"
      say "  signed build IS possible. This run would NOT use one, because nothing asked."
      say "  Re-run with --sign developer-id."
    fi
  fi
  say "  bundle targets      : $bundles"
  say "  build runs under    : ${build_launch[*]:-$build_launch_why}"
  if [ -n "$updater" ]; then
    say "  update artifacts    : ON — signed with $updater_key_desc"
    say "  update manifest     : ${RICHOS_UPDATE_BASE_URL:+at ${RICHOS_UPDATE_BASE_URL}}${RICHOS_UPDATE_BASE_URL:-NOT written (RICHOS_UPDATE_BASE_URL is unset, and a guessed URL is worse than none)}"
  else
    say "  update artifacts    : OFF (pass --updater)"
  fi
  say ""
  exit 0
fi

# ---------------------------------------------------------------------------
# Icon pre-flight — fast and legible. build.rs is the structural guarantee.
# ---------------------------------------------------------------------------
say "RichOS packaging entrypoint"
say "  repository app dir : $app_dir"
say "  signing mode       : $sign_mode"
say "  bundle targets     : $bundles"
say ""
say "checking the app icons before building (the build gate re-checks independently)..."

if ! python3 "$here/lib/app_icons.py" verify; then
  warn ""
  warn "REFUSING TO PACKAGE — the icon set above is not shippable."
  warn ""
  warn "  A bundle built now would carry no icon, or the wrong one, at the sizes"
  warn "  named. This is the single failure this entrypoint exists to make"
  warn "  impossible, so it stops here rather than warning and continuing."
  warn ""
  warn "  Fix: richos/app/scripts/generate-app-icons.sh /path/to/artwork.png"
  warn "  One square PNG, at least 1024x1024, transparent background."
  warn "  The artwork itself is an open CEO item (2.6) — it is not something to"
  warn "  work around with new placeholder art."
  warn ""
  exit 2
fi

# ---------------------------------------------------------------------------
# What the chosen signature costs — said at the moment it is incurred.
# ---------------------------------------------------------------------------
say ""
if [ "$sign_mode" = "adhoc" ]; then
  say "SIGNING: ad-hoc. What that costs, every time:"
  say ""
  say "  An ad-hoc bundle embeds no designated requirement, so macOS stores the raw"
  say "  code hash as the identity of every permission it grants — and nothing else."
  say "  The moment this build differs by one shipped byte from the installed one, it"
  say "  is A DIFFERENT APPLICATION to macOS: microphone and accessibility grants"
  say "  start at zero, and toggling the switch in System Settings provably does not"
  say "  migrate a grant (measured 2026-08-24, richos-hq/wiki/packaging-and-signing.md)."
  say "  Voice mode and paste-at-cursor will need granting again after installing this."
  say ""
  say "  Measured here 2026-08-30, because the distinction matters: what moves the"
  say "  hash is the BYTES, not the act of rebuilding. Three consecutive runs of this"
  say "  script over an unchanged tree — and one over a tree whose only edit was dead"
  say "  code the optimizer removed — all produced cdhash fc5051ac…, i.e. the same"
  say "  application and a surviving grant. Changing ONE shipped string moved it to"
  say "  27a561ef…. So this is not a tax on rebuilding; it is a tax on every change"
  say "  you would actually ship."
  say ""
  say "  The fix is a Developer ID certificate. The MEMBERSHIP that gates it exists as"
  say "  of 2026-08-31 (CEO decision 1.1, closed); the certificate is a separate step."
  if [ "$devid_count" != "0" ]; then
    say ""
    say "  LOUD NOTE: this machine has ${devid_count} Developer ID Application identit(y/ies), so"
    say "  a signed build IS possible now. It was NOT used, because nothing asked for"
    say "  it. Re-run with --sign developer-id (the identity is discovered)."
  else
    say "  This machine has no Developer ID Application certificate yet:"
    say "    richos/app/scripts/make-signing-csr.sh          then the portal, then"
    say "    richos/app/scripts/install-signing-cert.sh <the .cer>"
    say "  docs/ceo/developer-id-setup-2026-08-31.md is the middle step, in plain language."
  fi
else
  say "SIGNING: Developer ID — $wanted"
  say "  hardened runtime + secure timestamp + $src_tauri/Entitlements.plist"
  if [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
    say "  notarization: ON, via $notary_desc — submitted and stapled by THIS script,"
    say "  not by the bundler, and verified on the artefact afterwards."
  else
    say "  notarization: OFF — set RICHOS_NOTARIZE=1 with RICHOS_NOTARY_KEY /"
    say "  RICHOS_NOTARY_KEY_ID / RICHOS_NOTARY_ISSUER, or RICHOS_NOTARY_PROFILE."
    say "  Without it this bundle is SIGNED BUT NOT NOTARIZED: Gatekeeper still blocks"
    say "  it on a machine that downloaded it."
  fi
fi

# ---------------------------------------------------------------------------
# Build. THIS is the line the whole file exists for.
# ---------------------------------------------------------------------------
export RICHOS_REQUIRE_REAL_ICONS=1

# THE SOURCE-SIDE HALF OF "NO BUILD-MACHINE PATH SHIPS", run BEFORE the compile rather
# than after the bundle.
#
# The artifact check below (`no_host_paths.py`) is still the guarantee and nothing here
# weakens it. This runs first because of what the 2026-09-18 nightly cost: an hour of
# compiling and a full bundle, refused at the end over ONE line of Rust
# (`env!("CARGO_MANIFEST_DIR")` in `src/phone/mod.rs`) that was readable in the source the
# whole time. Same defect, two orders of magnitude cheaper to find.
#
# It is fatal here for the same reason the icon gate is: this is the path that ships.
say ""
say "checking the source for compile-time paths..."
if ! python3 "$app_dir/scripts/lib/no_compile_time_paths.py" "$src_tauri/src"; then
  warn "refusing to build a bundle from source that bakes in a build-machine path."
  warn "The artifact check would catch it after the compile; this catches it before."
  exit 3
fi

# KEEP THE BUILDER'S HOME DIRECTORY OUT OF THE BINARY.
#
# Rust bakes the compile-time path of every crate into the executable as panic-location
# and debug metadata: `$CARGO_HOME/registry/src/...` for a registry dependency, and the
# absolute source path for a path dependency that lives outside the package directory —
# which `richos-core` and `richos-voice` both do. MEASURED on the release build kept at
# `~/.richos-signing/rebuild-survival/builds/build-1/RichOS.app`: 657 occurrences of the
# builder's home directory, including 23 paths naming an internal agent worktree
# (`.worktrees/zach-opus-c1`). None of it is a file, so no bundle inspection could see
# it; `strings` reads all of it in a second, on the customer's machine.
#
# `--remap-path-prefix` and not cargo's `trim-paths`, because trim-paths is not
# available: cargo 1.95.0 refuses the key with "that feature is not stabilized in this
# version of Cargo". Established by running it.
#
# ORDER MATTERS AND IS NOT COSMETIC: rustc applies the LAST matching rule, so general
# comes first and specific comes last. And the replacements must not themselves look
# like a home directory — remapping $HOME to `/home` produced `/home/.cargo/...`, which
# is the same disclosure shape wearing a different name, and `no_host_paths.py` correctly
# refused it.
#
# This is exported HERE and not in a committed `.cargo/config.toml` on purpose: an
# ordinary `cargo build` during development keeps real paths, so a compiler diagnostic
# still points at a file the engineer can open. Only the release path is sanitized.
#
# AND THE REMAP IS NOT THE WHOLE FIX — MEASURED 2026-09-04. It rewrites the compiler's
# path metadata, so a path a MACRO already produced as a string literal is program data it
# cannot reach. Exactly one survived: `tauri::generate_context!()` emits
# `context.with_config_parent("<CARGO_MANIFEST_DIR>")` whenever the `tauri` crate's
# `custom-protocol` feature is off (tauri-codegen-2.6.3/src/context.rs:439-445). A release
# build with this remap in full force still carried it — one host path out of 657, and the
# only one left.
#
# `cargo tauri build`, which is what runs below, was never affected: the CLI turns that
# feature on itself (tauri-cli-2.11.4/src/interface/rust.rs:485). A plain
# `cargo build --release` was. Both now produce the same program, because
# `app/src-tauri/Cargo.toml` declares `custom-protocol` as a DEFAULT feature — read the
# comment there before removing it; the leak is the smaller half of what it prevents.
#
# If that default is ever removed, nothing here changes and no build fails for it. The
# check below is what catches it, on the artifact, which is the entire reason the check
# exists on the artifact rather than on the flags.
richos_cargo_home="${CARGO_HOME:-$HOME/.cargo}"
# Cargo's encoded form preserves spaces in selected storage paths.
CARGO_ENCODED_RUSTFLAGS="$(python3 - "$app_dir" "$richos_cargo_home" <<'PYFLAGS'
import os, sys
separator = chr(31)
flags = os.environ.get('CARGO_ENCODED_RUSTFLAGS')
flags = flags.split(separator) if flags is not None else os.environ.get('RUSTFLAGS', '').split()
roots = [(os.environ['HOME'], '/build'),
         (os.environ.get('RICHOS_BULK_ROOT'), '/build/storage'),
         (os.environ.get('RICHOS_CARGO_CACHE_ROOT') or os.environ.get('CARGO_TARGET_DIR'), '/build/cache'),
         (os.environ.get('TMPDIR'), '/build/tmp'),
         (sys.argv[2], '/build/cargo'), (sys.argv[1], '/build/app')]
for root, mapped in roots:
    if root:
        for path in dict.fromkeys([root, os.path.realpath(root)]):
            flags.append('--remap-path-prefix=' + path.rstrip('/') + '=' + mapped)
print(separator.join(flags), end='')
PYFLAGS
)" || die "cannot prepare portable compiler paths"
export CARGO_ENCODED_RUSTFLAGS

# THE STAGED FRONTEND HAS TO EXIST BEFORE THE CLI LOOKS FOR IT, and on a tree nobody has
# built yet it does not. `build.rs` fills `app/ui-dist` (see its `stage_frontend`), but
# build.rs runs during the cargo build and tauri-cli checks the directory BEFORE that:
# `tauri-cli-2.11.4/src/build.rs:198-207` bails with "Unable to find your web assets" on
# nothing more than `!web_asset_path.exists()`. So a FRESH CLONE could not be packaged at
# all — measured 2026-09-04, exit 4 on the first build in a new worktree — while the same
# command succeeded on this machine's older checkout only because an earlier build had left
# the directory behind. A release cut from a clean checkout is the normal case, not the odd
# one.
#
# The fix is the DIRECTORY and not its contents, deliberately: the CLI tests existence only,
# and build.rs then stages the real tree and panics if anything about it is wrong. Copying
# the frontend here would be a second staging implementation that could drift from the one
# that decides what ships.
staged_frontend="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["build"]["frontendDist"])' "$src_tauri/tauri.conf.json")"
case "$staged_frontend" in
  /*) staged_frontend_abs="$staged_frontend" ;;
  *)  staged_frontend_abs="$src_tauri/$staged_frontend" ;;
esac
if [ ! -d "$staged_frontend_abs" ]; then
  mkdir -p "$staged_frontend_abs" || { warn "could not create the staged frontend directory $staged_frontend_abs"; exit 3; }
  say ""
  say "  created the staged frontend directory $staged_frontend_abs — it did not exist, and"
  say "  tauri-cli refuses to start a build without one. build.rs fills it during the build."
fi

say ""
say "building (release) with RICHOS_REQUIRE_REAL_ICONS=1 — the icon gate is FATAL for this build..."
say "  build paths remapped out of the binary (\$HOME, \$CARGO_HOME, $app_dir);"
say "  the produced bundle is checked for leftovers before it is signed."
say ""

# `--no-sign` is NOT about codesigning. On macOS it disables exactly one thing:
# the CLI signing the updater tarball it just made from a bundle this script has
# not signed yet (see the header). Codesigning is gated on Windows only
# (tauri-bundler `bundle.rs:301-306`), and this script owns the macOS signature
# either way. It is also what lets a plain run work with no updater key at all.
build_args=(tauri build --no-sign --bundles "$bundles")
[ -n "$tauri_config_overlay" ] && build_args+=(--config "$tauri_config_overlay")
# THE DEVELOPMENT-BUILD VERSION STAMP, resolved above. Applied as an overlay — not by
# editing Cargo.toml — so release and nightly builds (which never set this) are byte-for-byte
# what they always were, and so RICHOS_EXTRA_TAURI_CONFIG below can still win if it also
# names `version` (order is what decides that, per tauri-cli's own --config merge).
if [ -n "$version_overlay" ]; then
  build_args+=(--config "$version_overlay")
  say ""
  say "  development build — stamping the version $target_version so it can never be"
  say "  mistaken for a release. Pass --release or --nightly <version> to ship this as one."
fi
# ONE MORE OVERLAY, FOR THE ONE CALLER THAT NEEDS IT.
#
# `richos/app/scripts/updater-e2e.sh` builds two versions of the app and points them at a
# manifest on 127.0.0.1, and both of those are configuration rather than code:
# `{"version": "0.1.1"}` and `dangerousInsecureTransportProtocol`, which the plugin
# requires before it will fetch over http (`config.rs:validate_endpoints` — a release
# build REFUSES a non-https endpoint without it, and that refusal is correct).
#
# It is an env var and not a flag because it is not a thing to type at a prompt: the
# `--config` values tauri merges are read in order and a wrong one silently changes the
# app. It is echoed in full below, every run, so a build carrying one says so.
if [ -n "${RICHOS_EXTRA_TAURI_CONFIG:-}" ]; then
  build_args+=(--config "$RICHOS_EXTRA_TAURI_CONFIG")
  say ""
  say "  EXTRA CONFIG OVERLAY IN FORCE — this build is NOT the shipping configuration:"
  say "    $RICHOS_EXTRA_TAURI_CONFIG"
fi

# The exact pinned connector is included by every packaged Mac build. Preparation is
# cache-backed on the external SSD and refuses an unexpected published asset digest.
connect_overlay="$(python3 "$here/prepare-connect-helper.py")" || exit 1
build_args+=(--config "$connect_overlay")

# ---------------------------------------------------------------------------
# RICHDOCTOR: THE SEPARATE SUPPORT APP THIS RICHOS CARRIES AND INSTALLS BESIDE ITSELF
# ---------------------------------------------------------------------------
# richos-hq docs/prds/2026-10-22-richdoctor-v1.md §3: "the existing RichOS download/update
# carries the signed RichDoctor app as a sealed install payload". It is built here from
# crates/richdoctor, signed the same way as RichOS (ad-hoc, or the same Developer ID with the
# hardened runtime and a timestamp, and notarized and stapled when RichOS is), verified, and
# archived with ONE RichDoctor.app root by the same archiver the update payload uses
# (lib/updater_tar.py). The archive and its version go into RichOS's Resources/RichDoctor/
# through a config overlay, so RichOS's own signature seals them; src-tauri/src/
# richdoctor_install.rs installs it at launch. The verified RichDoctor.app is also left where
# it was built, as the separate download (PRD §3: "the separate download supplies the
# identical signed app directly").
#
# RICHOS_WITHOUT_RICHDOCTOR=1 skips it, loudly: that RichOS installs no RichDoctor.
richdoctor_dir="$app_dir/crates/richdoctor"
if [ "${RICHOS_WITHOUT_RICHDOCTOR:-}" = "1" ]; then
  say ""
  say "  RICHOS_WITHOUT_RICHDOCTOR=1 — this RichOS carries NO RichDoctor and installs none."
else
  say ""
  say "building RichDoctor, the separate support app this RichOS installs beside itself..."
  if ! (cd "$richdoctor_dir" && ${build_launch[@]+"${build_launch[@]}"} cargo tauri build --no-sign --bundles app); then
    warn ""
    warn "RichDoctor did not build — nothing was packaged. The output above is the reason."
    exit 4
  fi
  . "$here/lib/cargo-target.sh"
  rd_app="$(cargo_target_dir "$richdoctor_dir")/release/bundle/macos/RichDoctor.app"
  if [ ! -d "$rd_app" ]; then
    warn "the RichDoctor build exited 0 but $rd_app is not there — refusing to continue."
    exit 1
  fi
  # The same build-machine path check as RichOS's, before the signature.
  if ! python3 "$here/lib/no_host_paths.py" "$rd_app" --reviewed-text && [ "${RICHOS_ALLOW_HOST_PATHS:-}" != "1" ]; then
    warn "RichDoctor carries build-machine paths — refusing to sign it."
    exit 1
  fi
  if [ "$sign_mode" = "developer-id" ]; then
    codesign --force --sign "$wanted" --options runtime --timestamp "$rd_app" || { warn "RichDoctor could not be signed."; exit 1; }
    if [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
      rd_zip="$(mktemp -d)/RichDoctor.zip"
      /usr/bin/ditto -c -k --keepParent "$rd_app" "$rd_zip" || exit 1
      rd_submit="$(xcrun notarytool submit "$rd_zip" "${notary_args[@]}" --wait --output-format json 2>&1 || true)"
      rm -rf "$(dirname "$rd_zip")"
      printf '%s\n' "$rd_submit" | sed 's/^/    /'
      if [ "$(printf '%s' "$rd_submit" | sed -n 's/.*"status" *: *"\([^"]*\)".*/\1/p' | head -1)" != "Accepted" ]; then
        warn "RichDoctor's notarization was not accepted — refusing to carry an un-notarized RichDoctor."
        exit 1
      fi
      xcrun stapler staple "$rd_app" || { warn "RichDoctor's ticket would not staple."; exit 1; }
    fi
  else
    codesign --force --sign - --timestamp=none "$rd_app" || { warn "RichDoctor could not be ad-hoc signed."; exit 1; }
  fi
  if ! codesign --verify --deep --strict "$rd_app"; then
    warn "RichDoctor's signature does not verify — refusing to carry it."
    exit 1
  fi
  rd_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$rd_app/Contents/Info.plist" 2>/dev/null || true)"
  rd_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$rd_app/Contents/Info.plist" 2>/dev/null || true)"
  rd_floor="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$rd_app/Contents/Info.plist" 2>/dev/null || true)"
  rd_ui="$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$rd_app/Contents/Info.plist" 2>/dev/null || true)"
  if [ "$rd_id" != "com.richos.richdoctor" ] || [ -z "$rd_version" ] || [ "$rd_floor" != "13.0" ] || [ "$rd_ui" != "true" ]; then
    warn "RichDoctor's Info.plist is not what it must be: identifier '$rd_id', version '$rd_version', macOS floor '$rd_floor', menu bar only '$rd_ui'."
    exit 1
  fi
  rd_payload="$(mktemp -d)/RichDoctor"
  mkdir -p "$rd_payload"
  python3 "$here/lib/updater_tar.py" build "$rd_app" "$rd_payload/RichDoctor.app.tar.gz" || exit 1
  python3 "$here/lib/updater_tar.py" verify "$rd_payload/RichDoctor.app.tar.gz" || exit 1
  printf '%s\n' "$rd_version" > "$rd_payload/version"
  build_args+=(--config "{\"bundle\":{\"resources\":{\"$rd_payload/RichDoctor.app.tar.gz\":\"RichDoctor/RichDoctor.app.tar.gz\",\"$rd_payload/version\":\"RichDoctor/version\"}}}")
  say "  RichDoctor $rd_version: signed ($sign_mode), verified, carried as Resources/RichDoctor/RichDoctor.app.tar.gz"
  say "  the separate download: $rd_app"
fi

# ---------------------------------------------------------------------------
# THE SOURCE COMMIT, DERIVED BEFORE THE BUILD so it can be compiled INTO it.
# ---------------------------------------------------------------------------
#
# `option_env!("RICHOS_SOURCE_SHA")` is a COMPILE-time read (`engine.rs::source_commit`), so
# this has to be in the environment of the cargo invocation below — not of a later step. The
# bundle is stamped from the same two variables after the build, so the plist and the
# executable cannot disagree about which commit this is: there is one derivation, used twice.
source_sha="$(git -C "$app_dir" rev-parse HEAD 2>/dev/null || true)"
source_dirty=""
if [ -n "$source_sha" ]; then
  source_root="$(git -C "$app_dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [ -n "$(git -C "$source_root" status --porcelain --untracked-files=all 2>/dev/null || true)" ]; then
    source_dirty="-dirty"
  fi
fi
source_stamp="${source_sha}${source_dirty}"
if [ -n "$source_stamp" ]; then
  say ""
  say "  source commit       : $source_stamp"
fi

if [ "${#build_launch[@]}" -eq 0 ]; then
  warn "  note: this compile runs without the release-build role ($build_launch_why),"
  warn "  so the CPU guard may stop rustc after 10 s above 3 cores."
fi
if ! (cd "$src_tauri" && ${build_launch[@]+"${build_launch[@]}"} env RICHOS_SOURCE_SHA="$source_stamp" cargo "${build_args[@]}"); then
  warn ""
  warn "the build did not complete — nothing was packaged. The output above is the reason."
  exit 4
fi

# ---------------------------------------------------------------------------
# Find and verify what was actually produced.
# ---------------------------------------------------------------------------
. "$here/lib/cargo-target.sh"
target_dir="$(cargo_target_dir "$src_tauri")"
bundle_dir="$target_dir/release/bundle/macos"
app_bundle="$(ls -d "$bundle_dir"/*.app 2>/dev/null | head -1 || true)"

if [ -z "$app_bundle" ]; then
  warn ""
  warn "the builder exited 0 but no .app exists under $bundle_dir — refusing to"
  warn "report success for a bundle that is not there."
  exit 1
fi

# ---------------------------------------------------------------------------
# The remap above is the fix. THIS is the proof.
# ---------------------------------------------------------------------------
# A flag that quietly stops covering everything — a new dependency shape, a vendored C
# library baking in `__FILE__`, a toolchain that changes what it embeds — puts the
# builder's home directory straight back with nothing to notice it. So the guarantee is
# checked on the bundle that was actually produced, and it is checked BEFORE the
# signature, because a signature over a bundle nobody inspected is confidence rather
# than evidence.
#
# Fatal, in the same posture build.rs takes toward the staged frontend: shipping the
# builder's account name and repository layout to a stranger is not cosmetic, and there
# is nothing to wait on. `RICHOS_ALLOW_HOST_PATHS=1` is the deliberate, logged override.
say ""
say "checking the bundle for build-machine paths..."
if ! python3 "$here/lib/no_host_paths.py" "$app_bundle" --reviewed-text --upstream-policy "$here/runtime-sources.json" --upstream-runtime-root "$app_bundle/Contents/Resources/runtime"; then
  if [ "${RICHOS_ALLOW_HOST_PATHS:-}" = "1" ]; then
    warn ""
    warn "  RICHOS_ALLOW_HOST_PATHS=1 — packaging a bundle that carries the above"
    warn "  anyway. Whoever installs this can read them."
  else
    warn ""
    warn "refusing to sign or package a bundle that carries the build machine's home"
    warn "directory. Nothing was signed."
    exit 5
  fi
fi

# ---------------------------------------------------------------------------
# WHICH COMMIT IS THIS? — stamped into the bundle, and proven inside the executable.
# ---------------------------------------------------------------------------
#
# THE DEFECT THIS CLOSES, measured on candidate .8 (2026-09-18): `strings` on the shipped
# executable found the version, `1.2.0-nightly.20260918.2`, and NO source commit anywhere;
# `Contents/Info.plist` carried CFBundleShortVersionString / CFBundleVersion and nothing
# else. So the only identity a walk could read off a candidate was a version string somebody
# typed, which is exactly what the Freshness Contract refuses: *"Every artifact carries a
# commit SHA baked INSIDE it; every consumer verifies that SHA before trusting the artifact.
# No timestamps, no 'the deploy said success' — identity or refuse."*
#
# A `-dev.` build has carried its commit in the VERSION since 2026-09-17, and a release or
# nightly has not — the two builds that go to the CEO were the two with no commit in them.
#
# WHY IT IS STAMPED HERE, BETWEEN THE BUILD AND THE SIGNING. Tauri's bundler signs the .app
# during `cargo tauri build` when it has an identity, so a PlistBuddy write after that would
# invalidate the signature — and on a notarized build, the notarization with it. But this
# script already re-signs with the full explicit argv below (developer-id at the `--timestamp`
# re-sign; ad-hoc immediately after), so a write placed HERE is covered by the signature that
# actually ships. That is the whole reason this block sits where it sits, and it must stay
# above both signing branches.
#
# `src-tauri/Info.plist`, which Tauri MERGES at bundle time, is deliberately not the place:
# the value changes every commit, that file is tracked, and a build that rewrites a tracked
# file leaves the tree dirty — which the freshness gates read as a build nobody can identify.
if [ -z "$source_sha" ]; then
  # A build outside a checkout cannot name its source, and saying so is the honest outcome.
  # It is not fatal here: the `dev` intent already REFUSES for the same reason (it cannot
  # compose its version), and a release/nightly always comes from make-release.sh in a
  # checkout — so a hard failure would only ever fire on a case that is already refused.
  warn ""
  warn "  this build has no git HEAD to stamp, so the bundle will not name its source"
  warn "  commit. Anything verifying this artifact can only go on its version string."
else
  if ! /usr/libexec/PlistBuddy \
        -c "Add :RichOSSourceCommit string $source_stamp" \
        "$app_bundle/Contents/Info.plist" >/dev/null 2>&1; then
    /usr/libexec/PlistBuddy \
      -c "Set :RichOSSourceCommit $source_stamp" \
      "$app_bundle/Contents/Info.plist" >/dev/null 2>&1 \
      || { warn "could not stamp the source commit into Contents/Info.plist"; exit 1; }
  fi
  # READ IT BACK OFF THE BUNDLE. "PlistBuddy exited 0" is not the artifact; the plist is.
  stamped="$(/usr/libexec/PlistBuddy -c 'Print :RichOSSourceCommit' \
    "$app_bundle/Contents/Info.plist" 2>/dev/null || true)"
  [ "$stamped" = "$source_stamp" ] \
    || { warn "Contents/Info.plist says RichOSSourceCommit=$stamped, expected $source_stamp"; exit 1; }

  # AND INSIDE THE EXECUTABLE, where the signature covers it and a plist edit cannot reach.
  # Same proof `make-release.sh` applies to the engine digest, for the same reason: the value
  # is compiled in with `option_env!`, which is worth nothing if the environment never
  # reached the compiler. An `option_env!` string survives an optimized build as plain bytes.
  exe_for_stamp="$app_bundle/Contents/MacOS/richos-tauri"
  if [ -f "$exe_for_stamp" ] && ! grep -a -q "$source_sha" "$exe_for_stamp"; then
    warn ""
    warn "the source commit $source_sha is NOT inside $exe_for_stamp — RICHOS_SOURCE_SHA did"
    warn "not reach the compiler, so the boot log cannot name the commit this app was built"
    warn "from and only the plist would carry it. Refusing to sign a bundle that cannot"
    warn "identify itself from the inside."
    exit 1
  fi
  say ""
  say "  source commit       : $source_stamp (Info.plist + compiled into the executable)"
fi

# The bundler signs the bundle ONLY when it has an identity. With none it leaves the
# .app unsigned around a linker-signed executable, which `codesign --verify` refuses
# (measured — see verify_bundle's adhoc arm). So the ad-hoc path signs deliberately
# and says it is doing so. This is the act that incurs the cost printed above.
if [ "$sign_mode" = "adhoc" ]; then
  say ""
  say "ad-hoc signing the bundle (the bundler does not: it signs only with a real identity)..."
  codesign --force --sign - --timestamp=none "$app_bundle/Contents/MacOS/cloudflared" || exit 1
  if ! codesign --force --sign - --timestamp=none "$app_bundle" 2>&1; then
    warn "ad-hoc signing failed — refusing to report a bundle as signed when it is not."
    exit 1
  fi
  # Show the cost rather than only describing it. macOS stores THIS expression as
  # the identity of every permission it grants the app, so the CEO can read for
  # himself that it is a hash of this build and nothing else — no bundle
  # identifier, no team. A Developer ID signature would print
  # `identifier "com.richos.app" and anchor apple generic and certificate
  # leaf[subject.OU] = "<TEAMID>"` here, which every future build satisfies.
  # codesign prints it commented, as `# designated => ...`.
  say ""
  say "  the designated requirement macOS will store against every grant:"
  dr="$(codesign -d -r- "$app_bundle" 2>/dev/null | sed -n 's/^#\{0,1\} *designated => //p' | head -1)"
  if [ -n "$dr" ]; then
    say "    $dr"
    say "  — a hash of THIS build. No bundle identifier, no team, nothing a later"
    say "    build can satisfy. That is the whole of the grant problem, in one line."
  else
    say "    (codesign printed no designated requirement)"
  fi
fi

# ---------------------------------------------------------------------------
# Developer ID: re-sign with an explicit, complete argv, then notarize and staple.
# ---------------------------------------------------------------------------
if [ "$sign_mode" = "developer-id" ]; then
  # WHY RE-SIGN SOMETHING THE BUNDLER JUST SIGNED. tauri-macos-sign 2.3.4
  # (keychain.rs:221-226) builds `codesign --force -s <id> [--options runtime]
  # [--entitlements <plist>]` and never passes --timestamp. codesign's man page on
  # the unspecified default: "a system-specific default behavior is invoked. This
  # may result in some but not all code signatures being timestamped." A secure
  # timestamp is a NOTARIZATION PREREQUISITE, so this is not a thing to leave to a
  # documented non-guarantee. The bundler still does the FIRST pass because it
  # signs inside-out — frameworks and sidecars before the bundle — which is Apple's
  # required order and which this re-sign does not attempt to replace.
  say ""
  say "re-signing the bundle with the full argv (adds --timestamp, which the bundler omits)..."
  codesign --force --sign "$wanted" --options runtime --timestamp "$app_bundle/Contents/MacOS/cloudflared" || exit 1
  if ! codesign --force --sign "$wanted" --options runtime --timestamp \
                --entitlements "$src_tauri/Entitlements.plist" "$app_bundle"; then
    warn ""
    warn "the explicit re-sign failed. A signature without a secure timestamp cannot be"
    warn "notarized, so this is fatal rather than something to continue past."
    warn "If the failure is 'The timestamp service is not available', it is Apple's"
    warn "server and not this bundle; retry."
    exit 1
  fi

  say ""
  say "  the designated requirement macOS will store against every grant:"
  dr="$(codesign -d -r- "$app_bundle" 2>/dev/null | sed -n 's/^#\{0,1\} *designated => //p' | head -1)"
  if [ -n "$dr" ]; then
    say "    $dr"
    say "  — an identifier and a team, not a hash. THIS is what every future build"
    say "    satisfies, and it is the whole reason the certificate exists."
  else
    say "    (codesign printed no designated requirement — verification below will fail)"
  fi

  if [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
    # notarytool takes an archive, not a bundle directory. ditto --keepParent is the
    # form Apple documents; `zip -r` loses symlinks inside frameworks and produces a
    # submission that is rejected for reasons that read like a signing problem.
    zip_path="$(mktemp -d)/$(basename "$app_bundle" .app).zip"
    say ""
    say "submitting to Apple's notary service via $notary_desc (this waits; minutes, sometimes longer)..."
    if ! /usr/bin/ditto -c -k --keepParent "$app_bundle" "$zip_path"; then
      warn "error: could not archive the bundle for submission."
      exit 1
    fi

    submit_out="$(xcrun notarytool submit "$zip_path" "${notary_args[@]}" --wait --output-format json 2>&1 || true)"
    rm -rf "$(dirname "$zip_path")"
    printf '%s\n' "$submit_out" | sed 's/^/    /'

    sub_status="$(printf '%s' "$submit_out" | sed -n 's/.*"status" *: *"\([^"]*\)".*/\1/p' | head -1)"
    sub_id="$(printf '%s' "$submit_out" | sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' | head -1)"

    if [ "$sub_status" != "Accepted" ]; then
      warn ""
      warn "NOTARIZATION FAILED — status: ${sub_status:-<none reported>}"
      if [ -n "$sub_id" ]; then
        warn ""
        warn "Apple's own reasons, fetched rather than guessed at:"
        xcrun notarytool log "$sub_id" "${notary_args[@]}" 2>&1 | sed 's/^/    /' >&2 || true
      fi
      warn ""
      warn "Refusing to continue. The bundle on disk is signed and NOT notarized, and"
      warn "nothing in this run will say otherwise."
      exit 1
    fi

    say ""
    say "stapling the ticket into the bundle..."
    if ! xcrun stapler staple "$app_bundle"; then
      warn ""
      warn "Apple accepted the submission and the ticket would not staple. The app is"
      warn "notarized on Apple's side but carries no ticket, so a machine that is"
      warn "offline or behind a captive portal treats it as un-notarized. That is not"
      warn "a shippable artefact; refusing."
      exit 1
    fi
  fi
fi

say ""
say "verifying the artefact that was produced (not the builder's exit code)..."
expect_stapled=""
[ "$sign_mode" = "developer-id" ] && [ "${RICHOS_NOTARIZE:-}" = "1" ] && expect_stapled=1
if ! verify_bundle "$app_bundle" "$sign_mode" "$expect_stapled" "$intent" "$target_version"; then
  exit 1
fi

# ---------------------------------------------------------------------------
# THE UPDATE ARTIFACTS — built from the VERIFIED bundle, never from the builder's
# intermediate. See this file's header for the vendor ordering defect this is
# working around; the short version is that the archive the bundler makes is a
# tarball of an application nothing had signed yet.
# ---------------------------------------------------------------------------
vendor_tar="${app_bundle}.tar.gz"
if [ -f "$vendor_tar" ]; then
  # It exists whether or not --updater was asked for, because the config turns the
  # artifact on. UNCONDITIONALLY REMOVED: an unsigned tarball of an unsigned app,
  # sitting next to a verified bundle with a plausible name, is exactly the file
  # somebody publishes at 2am.
  rm -f "$vendor_tar" "${vendor_tar}.sig"
  say ""
  say "  removed the bundler's own ${app_bundle##*/}.tar.gz: it was made from the"
  say "  bundle BEFORE this script signed it, so it packaged an unsigned application."
fi

if [ -n "$updater" ]; then
  say ""
  say "building the update artifacts from the bundle that was just verified..."

  if ! python3 "$here/lib/updater_tar.py" build "$app_bundle" "$vendor_tar"; then
    warn ""
    warn "the update archive could not be built. The bundle above is fine and is"
    warn "still on disk; nothing publishable was produced."
    exit 5
  fi

  if ! python3 "$here/lib/updater_tar.py" verify "$vendor_tar"; then
    warn ""
    warn "the update archive is not installable — see the reasons above. Refusing to"
    warn "sign it: a signature over a broken archive is a signature that says a"
    warn "broken archive is genuine."
    exit 5
  fi

  # Sign. `--ci` is not passed; the password comes from the environment, and an
  # empty string is the correct value for a passwordless key.
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
  sign_args=(tauri signer sign)
  if [ -n "$updater_key_path" ]; then
    sign_args+=(-f "$updater_key_path")
  else
    sign_args+=(-k "$TAURI_SIGNING_PRIVATE_KEY")
  fi
  sign_args+=(-p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$vendor_tar")

  say ""
  say "signing it with $updater_key_desc..."
  if ! (cd "$src_tauri" && cargo "${sign_args[@]}" >/dev/null 2>&1); then
    warn ""
    warn "the updater signature could not be produced. Re-run the same command"
    warn "without the output suppressed to see the signer's own reason:"
    warn "  (cd $src_tauri && cargo tauri signer sign -f <key> -p <password> $vendor_tar)"
    exit 5
  fi

  sig_path="${vendor_tar}.sig"
  if [ ! -f "$sig_path" ]; then
    warn "the signer exited 0 and wrote no $sig_path — refusing to report a signed artifact."
    exit 5
  fi

  # THE CHECK THE SIGNER'S EXIT CODE DOES NOT MAKE. It says a signature was
  # written. This says the signature VERIFIES, against the public key compiled
  # into the app — the same `minisign_verify` call the updater makes on the CEO's
  # machine before it installs a byte. A release signed with the wrong key is
  # otherwise indistinguishable from a correct one until it is public.
  say ""
  say "verifying that signature against plugins.updater.pubkey in tauri.conf.json..."
  if ! (cd "$src_tauri" && cargo run -q --example verify_update_signature -- "$vendor_tar" "$sig_path"); then
    warn ""
    warn "REFUSING — the artifact and its signature do not agree under the key this"
    warn "repository ships. Nothing here is publishable."
    exit 5
  fi

  # The platform key the updater actually looks for: `{os}-{arch}` from
  # tauri-plugin-updater 2.11.0 `updater.rs:updater_os/updater_arch`, derived from
  # THIS machine rather than assumed.
  case "$(uname -m)" in
    arm64|aarch64) upd_arch="aarch64" ;;
    x86_64)        upd_arch="x86_64" ;;
    *)             upd_arch="$(uname -m)" ;;
  esac
  upd_target="darwin-${upd_arch}"
  # READ OFF THE ARTEFACT, NOT THE CONFIG. The first version of this line read
  # `version` out of tauri.conf.json and was WRONG THE FIRST TIME IT RAN: a build
  # carrying a `--config` overlay (which is how updater-e2e.sh makes a second
  # version at all, and how a release could be built from a tag) produced a 0.1.1
  # bundle and a manifest announcing 0.1.0. A manifest that announces a version the
  # archive does not contain is a release that either never installs or installs
  # something other than what it said. The bundle's own Info.plist is the only
  # copy that shipped.
  upd_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app_bundle/Contents/Info.plist" 2>/dev/null)"
  if [ -z "$upd_version" ]; then
    warn "the produced bundle has no CFBundleShortVersionString — there is no version to announce."
    exit 5
  fi
  upd_bytes="$(stat -f '%z' "$vendor_tar")"
  upd_sha="$(shasum -a 256 "$vendor_tar" | awk '{print $1}')"

  say ""
  say "  update artifact : $vendor_tar"
  say "  signature       : $sig_path"
  say "  version         : $upd_version"
  say "  platform key    : $upd_target"
  say "  bytes           : $upd_bytes"
  say "  sha256          : $upd_sha"

  # ---- the manifest -------------------------------------------------------
  manifest="$(dirname "$vendor_tar")/latest.json"
  if [ -z "${RICHOS_UPDATE_BASE_URL:-}" ]; then
    rm -f "$manifest"
    say ""
    say "  NO MANIFEST WAS WRITTEN, and that is deliberate. RICHOS_UPDATE_BASE_URL is"
    say "  unset, so the URL the CEO's copy would fetch this archive from is not known."
    say "  WHERE RICHOS UPDATES ARE HOSTED IS THE CEO'S DECISION AND HAS NOT BEEN MADE"
    say "  (app/UPDATES.md lists the options and the trade-off). A manifest carrying a"
    say "  guessed URL is worse than no manifest: it is a file that looks publishable."
    say ""
    say "  Re-run with RICHOS_UPDATE_BASE_URL=https://<wherever>/ to emit latest.json."
  else
    base="${RICHOS_UPDATE_BASE_URL%/}"
    python3 - "$manifest" "$upd_version" "$upd_target" "$base/$(basename "$vendor_tar")" "$sig_path" "${RICHOS_UPDATE_NOTES:-}" <<'PY'
import json, sys, datetime
manifest, version, target, url, sig_path, notes = sys.argv[1:7]
signature = open(sig_path).read().strip()
doc = {
    "version": version,
    # The CEO reads this verbatim in the update row. It is never generated from a
    # commit log: a release note assembled from subject lines is a changelog, and a
    # changelog is not a sentence a non-technical reader can act on.
    "notes": notes or "",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {target: {"signature": signature, "url": url}},
}
with open(manifest, "w") as fh:
    json.dump(doc, fh, indent=2)
    fh.write("\n")
print("  manifest        : " + manifest)
print("  announced url   : " + url)
PY
    if [ -z "${RICHOS_UPDATE_NOTES:-}" ]; then
      say ""
      say "  NOTE: RICHOS_UPDATE_NOTES was unset, so \"notes\" is an empty string and the"
      say "  update row will fall back to \"You are running <version>\". That is honest and"
      say "  it is also a missed sentence — set it to what changed, in the CEO's language."
    fi
  fi
fi

# Gatekeeper's own verdict, reported rather than judged. An ad-hoc or un-notarized
# bundle is REJECTED here and that is not a defect in the build — it is the
# definition of not being notarized, and it is what a machine that DOWNLOADED this
# app would do with it. Copying it locally is not the same test, which is why the
# real verdict is printed instead of inferred.
say ""
say "  Gatekeeper assessment (spctl): $(spctl -a -vv "$app_bundle" 2>&1 | sed -n 's/^.*: //p' | head -1)"

say_version_identity

if [ "$sign_mode" = "adhoc" ]; then
  say ""
  say "OK: $(basename "$app_bundle") is bundled, ad-hoc signed and verified — real icons, cdhash ${BUNDLE_CDHASH}, microphone usage string present; NOT notarized, Gatekeeper rejects it, and its permission grants die on the next build that changes a shipped byte. Path: $app_bundle"
elif [ "${RICHOS_NOTARIZE:-}" = "1" ]; then
  say ""
  say "OK: $(basename "$app_bundle") is bundled, Developer ID signed, notarized, STAPLED and verified — real icons, cdhash ${BUNDLE_CDHASH}, hardened runtime, secure timestamp, microphone entitlement and usage string present, and a designated requirement macOS can carry across builds: ${BUNDLE_DR}. Path: $app_bundle"
  say ""
  say "  THIS IS NOT YET EVIDENCE THAT GRANTS SURVIVE. The requirement above is the"
  say "  MECHANISM; the property is only exercised by a SECOND install. Run"
  say "  richos/app/scripts/rebuild-survival.sh — a build that has only ever been installed"
  say "  once has never tested the thing that breaks."
else
  say ""
  say "OK: $(basename "$app_bundle") is bundled, Developer ID signed and verified but NOT NOTARIZED — real icons, cdhash ${BUNDLE_CDHASH}, hardened runtime, secure timestamp, microphone entitlement and usage string present, designated requirement ${BUNDLE_DR}; Gatekeeper will still block a downloaded copy. Path: $app_bundle"
fi

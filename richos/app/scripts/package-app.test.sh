#!/usr/bin/env bash
#
# package-app.test.sh — the signing configuration and the bundle verification,
# exercised as far as they can be without a Developer ID certificate.
#
# WHAT THIS SUITE CAN AND CANNOT PROVE, stated first because the distinction is
# the whole reason it exists.
#
#   IT PROVES     every refusal, by running it; the identity DISCOVERY rule on 0,
#                 1 and 2 identities; that discovery never self-selects the
#                 developer-id path; the notary credential rules including the
#                 in-repo and world-readable refusals; and every branch of
#                 verify_bundle's developer-id arm, fired against a REAL signed
#                 bundle.
#
#   IT CANNOT     prove that a Developer ID signature works. No certificate exists
#                 on any machine here (`security find-identity -v -p codesigning`
#                 reports 0), so nothing has ever been signed with one, notarized
#                 or stapled. Sections B and C therefore run against a `security`
#                 SHIM that reports synthetic identities. That shim proves what the
#                 SCRIPT does with an inventory; it proves nothing about codesign.
#                 Where the shim is used, the case name says so.
#
# THE FIXTURE BUNDLE IS REAL, and this is the part that surprised the author.
# package-app.sh cannot build anything on this tree at all: the icon gate refuses
# on the placeholder icon set (CEO item 2.6, the artwork, still open), so there was
# no bundle to verify against. But verify_bundle takes a PATH — it does not care
# who produced it. So section D builds a minimal .app by hand, ad-hoc signs it with
# the real codesign, and points the real --verify-only at it. Every developer-id
# check then fires against a genuine signature that is genuinely not a Developer ID
# one, which is exactly the negative half, and the mutations in D3-D7 turn the
# positive half red one reason at a time.
#
# Cases:
#   A1  an unknown argument is refused, never ignored
#   A2  --sign with a value that is neither mode is refused
#   A3  --help prints the usage, including the notarize and dry-run forms
#   B1  developer-id with NO Developer ID identity refuses, and names the two scripts [shim]
#   B2  developer-id with RICHOS_SIGNING_IDENTITY the machine does not report refuses
#   B3  developer-id with exactly ONE identity DISCOVERS it   [shim]
#   B4  developer-id with TWO refuses and lists both rather than sorting  [shim]
#   B5  a plain (ad-hoc) run with an identity present does NOT use it     [shim]
#   C1  RICHOS_NOTARIZE=1 with no credentials refuses                     [shim]
#   C2  a half-supplied API key refuses and names the missing parts       [shim]
#   C3  a key path that does not exist refuses                            [shim]
#   C4  a key INSIDE a git worktree refuses                               [shim]
#   C5  a world-readable key refuses                                      [shim]
#   C6  a complete key, outside a repo, mode 600, resolves                 [shim]
#   C7  RICHOS_NOTARY_PROFILE alone resolves                              [shim]
#   C8  APPLE_ID + APPLE_PASSWORD + APPLE_TEAM_ID are NOT accepted        [shim]
#   D1  a real ad-hoc bundle passes --verify-only in adhoc mode
#   D2  ...and FAILS in developer-id mode on all EIGHT developer-id grounds —
#       authority, hardened runtime, timestamp, entitlement, and the four separate
#       clauses of the designated requirement
#   D3  a missing shipped icon fails
#   D4  a shipped icon that is not this repository's fails
#   D5  a missing NSMicrophoneUsageDescription fails
#   D6  a bundle whose signature was removed fails
#   D7  --expect-notarized fires the stapled-ticket check
#   D8  a bundle with no CFBundleShortVersionString fails — it cannot say which version
#       it is, and the settings menu would render "RichOS " with nothing after it
#   D9  a bundle whose version is not the one this build intended fails
#   D10 a version with no release behind it SAYS SO, declared --release, at the build
#   D11 ...and the one caller that overrides the version on purpose is not refused
#   D12 ...and the other branch: a version that IS a release says so, and names the commit
#   D13 a correct bundle is owner-writable THROUGHOUT and still verifies — the measurement
#       that disqualifies spec point 17's own wording, kept as a case so it is not re-argued
#   D14 a group-writable path inside Contents/ fails
#   D15 a world-writable directory inside Contents/ fails
#   D16 a shipped .jsonl fails — state in the bundle, caught at build time
#   D17 a shipped config.json fails
#   F1  a bare, undeclared version is refused unless --release says this bundle is one
#   F2  ...and --release accepts the identical bare version
#   F3  a nightly-suffixed version needs no declared intent — it was never "bare"
#   F4  --nightly on that same version resolves
#   F5  --nightly does NOT excuse a bare version — only --release does
#   F6  --release and --nightly cannot both be declared
#   F7  --nightly with no value is refused
#   F8  a plain run with no declared intent is a development build, stamped {version}-dev.{commit}
#   F9  --release keeps the bare Cargo.toml version, unstamped
#   F10 --nightly refuses when Cargo.toml disagrees with the declared version
#   F11 --nightly with the version Cargo.toml already carries resolves
#   F12 a clean fixture tree's development stamp carries no .dirty suffix
#   F13 an uncommitted change appends .dirty to the development stamp
#   F14 a -dev. bundle is accepted with no declared intent, and says so
#   F15 the compile runs under the CPU guard's release-build role, registered by
#       proc_tree.py exactly as nightly-local.py registers its build step
#   E1  an exported API key does not satisfy the no-credentials case       [shim]
#   E2  exported halves do not complete a half-supplied key                [shim]
#   E3  an exported key does not outrank the case's own profile            [shim]
#   E4  an exported key does not turn the Apple-ID refusal into a pass     [shim]
#   E5  an exported RICHOS_SIGNING_IDENTITY does not reach discovery       [shim]
#   Z   the operator's real keychain inventory is unchanged by this suite
#
# No application window is opened. Signed bundles contain a native fixture
# that prints a source stamp. The isolation test runs a headless file reader.
# run-tests: no-host-screen: bundles contain a native fixture; the isolated probe runs only that fixture and the headless compiler
# run-tests: inputs richos/app/scripts/package-app.test.sh richos/app/scripts/package-app.sh richos/app/scripts/lib/bundle_invariants.py richos/app/scripts/lib/no_host_paths.py richos/app/scripts/lib/probe_packaged.py richos/app/scripts/lib/probe_packaged.test.py richos/engine/scripts/lib/named-persons.py richos/app/scripts/install-signing-cert.sh richos/app/scripts/nightly-local.py richos/engine/scripts/lib/proc_tree.py richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Info.plist richos/app/src-tauri/Entitlements.plist richos/app/src-tauri/tauri.conf.json
# run-tests: covers richos/app/scripts/package-app.sh richos/app/scripts/lib/bundle_invariants.py richos/app/scripts/lib/no_host_paths.py richos/app/scripts/lib/probe_packaged.py
set -uo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SRC_DIR/package-app.sh"

TMP="$(python3 -c 'import tempfile; print(tempfile.mkdtemp(prefix="package-app-test-"))')"
trap 'rm -rf "$TMP"' EXIT
# The private roster is never read by these synthetic bundle cases.
export RICHOS_NAMED_PERSONS_FILE="$TMP/named-persons"
printf 'name: Synthetic Forbiddenperson\n' > "$RICHOS_NAMED_PERSONS_FILE"
chmod 600 "$RICHOS_NAMED_PERSONS_FILE"

# Native, headless fixture with a real compiled source stamp.
cat > "$TMP/fixture.c" <<'C'
#include <stdio.h>
int main(void) { puts("0123456789abcdef0123456789abcdef01234567"); return 0; }
C
cc "$TMP/fixture.c" -o "$TMP/fixture" || exit 2

REAL_IDS_BEFORE="$(security find-identity -v -p codesigning 2>/dev/null || true)"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

# --- run the script, capture output and code ------------------------------
#
# EVERY CASE RUNS WITH THE OPERATOR'S OWN SIGNING VARIABLES REMOVED, and that is
# not tidiness. On 2026-09-16 this suite was the single red gate of the first
# nightly release attempt: C1, C2, C7 and C8 failed on the CEO's Mac and on no
# other machine, because nightly-local.py exported his real App Store Connect API
# key into every gate. "No credentials" then had a complete key; "half a key" got
# its missing halves from the environment; the profile case lost to the key that
# package-app.sh prefers. The refusals were right. Their environment was not.
#
# A verdict that depends on what the operator happens to have exported is the
# failure this repository keeps paying for in both directions: green over a check
# that never ran, red over a defect that never happened. So the suite states its
# own environment instead of inheriting one.
#
# The unset runs inside the command substitution's subshell, BEFORE the case's own
# `env NAME=value` assignments are applied, so a case still gets exactly what it
# sets and nothing more. The parent shell is untouched, which is what lets section
# E export a hostile credential set and prove the isolation from the outside.
isolate() { unset "${!RICHOS_NOTARY_@}" "${!TAURI_SIGNING_@}" "${!APPLE_@}" RICHOS_NOTARIZE RICHOS_SIGNING_IDENTITY; }
run() { OUT="$(isolate; "$@" 2>&1)"; CODE=$?; return 0; }

expect() {   # expect <name> <wanted-code> <substring>
  local name="$1" want="$2" needle="$3"
  if [ "$CODE" != "$want" ]; then
    bad "$name" "exit $CODE, wanted $want. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
  elif ! printf '%s' "$OUT" | grep -Fq -- "$needle"; then
    bad "$name" "exit $want as wanted, but the output never said '$needle'"
  else
    ok "$name"
  fi
}

# --------------------------------------------------------------------------
# The shim. `security` and `cargo` only, on a PATH prefix, for the cases whose
# names say [shim]. Nothing else is faked: python3, codesign, PlistBuddy and the
# script itself are the real ones.
# --------------------------------------------------------------------------
SHIM="$TMP/shim"; mkdir -p "$SHIM"
cat > "$SHIM/security" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "find-identity" ]; then
  n="${SHIM_IDENTITIES:-0}"
  i=1
  while [ "$i" -le "$n" ]; do
    printf '  %d) DEADBEEF%04d "Developer ID Application: Test Person (TEAM%05d)"\n' "$i" "$i" "$i"
    i=$((i + 1))
  done
  printf '     %s valid identities found\n' "$n"
  exit 0
fi
exit 1
EOF
cat > "$SHIM/cargo" <<'EOF'
#!/usr/bin/env bash
[ "${1:-}" = "tauri" ] && [ "${2:-}" = "--version" ] && { echo "tauri-cli 2.11.4"; exit 0; }
echo "shimmed cargo refuses to do anything else" >&2
exit 1
EOF
chmod +x "$SHIM/security" "$SHIM/cargo"
shimmed() { env PATH="$SHIM:$PATH" "$@"; }

echo ""
echo "=== A. arguments ==="
run bash "$SCRIPT" --wat
expect "A1 an unknown argument is refused" 2 "unknown argument"
run bash "$SCRIPT" --sign sortof
expect "A2 --sign takes only the two modes" 2 "must be 'adhoc' or 'developer-id'"
run bash "$SCRIPT" --help
expect "A3 --help shows the notarize form" 0 "RICHOS_NOTARIZE=1 richos/app/scripts/package-app.sh --sign developer-id"

echo ""
echo "=== B. identity discovery ==="
# SHIMMED, since 2026-09-01. This ran unshimmed and depended on the OPERATOR'S keychain
# holding no Developer ID identity — true when it was written, false from the moment the
# CEO's certificate was installed, which turned a real check into a permanent red line on
# the only machine that matters. The shim reports 0 identities, so the refusal is exercised
# on any machine, with or without a certificate.
run shimmed env SHIM_IDENTITIES=0 bash "$SCRIPT" --sign developer-id --dry-run
expect "B1 [shim] no Developer ID identity -> refuse, and name the setup scripts" 2 "install-signing-cert.sh"
run env RICHOS_SIGNING_IDENTITY="Developer ID Application: Ghost (NOPE)" bash "$SCRIPT" --sign developer-id --dry-run
expect "B2 a pinned identity the machine does not report -> refuse" 2 "does not report it on this"

run shimmed env SHIM_IDENTITIES=1 bash "$SCRIPT" --sign developer-id --dry-run
expect "B3 [shim] exactly one identity is DISCOVERED" 0 "discovered the one Developer ID Application identity"
if printf '%s' "$OUT" | grep -q "identity came from  : discovery"; then
  ok "B3b [shim] ...and the dry run says WHERE the identity came from"
else
  bad "B3b the dry run did not attribute the identity to discovery" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi

run shimmed env SHIM_IDENTITIES=2 bash "$SCRIPT" --sign developer-id --dry-run
expect "B4 [shim] two identities -> refuse rather than sort" 2 "2 Developer ID Application identities are on this machine"

run shimmed env SHIM_IDENTITIES=1 bash "$SCRIPT" --dry-run
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -q "signing mode        : adhoc" \
   && printf '%s' "$OUT" | grep -q "LOUD NOTE"; then
  ok "B5 [shim] an identity on the machine does not make a plain run use it"
else
  bad "B5 a plain run either used the identity or stopped saying one was available" "exit $CODE"
fi

echo ""
echo "=== C. notary credentials ==="
notarize() { shimmed env SHIM_IDENTITIES=1 RICHOS_NOTARIZE=1 "$@" bash "$SCRIPT" --sign developer-id --dry-run; }

run notarize
expect "C1 [shim] notarize with no credentials -> refuse" 2 "no APPLE_PASSWORD path"
run notarize RICHOS_NOTARY_KEY_ID=ABCDE12345
expect "C2 [shim] half an API key -> refuse, naming the missing halves" 2 "RICHOS_NOTARY_KEY RICHOS_NOTARY_ISSUER"

KEYDIR="$TMP/keys"; mkdir -p "$KEYDIR"
KEY="$KEYDIR/AuthKey_ABCDE12345.p8"
printf 'not a real key\n' > "$KEY"; chmod 600 "$KEY"

run notarize RICHOS_NOTARY_KEY="$KEYDIR/gone.p8" RICHOS_NOTARY_KEY_ID=ABCDE12345 RICHOS_NOTARY_ISSUER=uuid
expect "C3 [shim] a key path that does not exist -> refuse" 2 "downloaded exactly once"

# C4 needs a real git worktree, because the check asks git rather than the string.
REPO="$TMP/repo"; mkdir -p "$REPO"
git -C "$REPO" init -q 2>/dev/null
printf 'not a real key\n' > "$REPO/AuthKey_ABCDE12345.p8"; chmod 600 "$REPO/AuthKey_ABCDE12345.p8"
run notarize RICHOS_NOTARY_KEY="$REPO/AuthKey_ABCDE12345.p8" RICHOS_NOTARY_KEY_ID=ABCDE12345 RICHOS_NOTARY_ISSUER=uuid
expect "C4 [shim] a notary key inside a git worktree -> refuse" 2 "is inside a git worktree"

chmod 644 "$KEY"
run notarize RICHOS_NOTARY_KEY="$KEY" RICHOS_NOTARY_KEY_ID=ABCDE12345 RICHOS_NOTARY_ISSUER=uuid
expect "C5 [shim] a world-readable notary key -> refuse" 2 "readable by other users of this Mac"
chmod 640 "$KEY"
run notarize RICHOS_NOTARY_KEY="$KEY" RICHOS_NOTARY_KEY_ID=ABCDE12345 RICHOS_NOTARY_ISSUER=uuid
expect "C5b [shim] a group-readable (640) notary key -> refuse" 2 "readable by other users of this Mac"
chmod 600 "$KEY"

# The updater key has the same rule: a group member can read a 640 key as well as a 644 one.
UKEY="$KEYDIR/updater.key"
printf 'not a real key\n' > "$UKEY"
updater_dry() { shimmed env SHIM_IDENTITIES=1 TAURI_SIGNING_PRIVATE_KEY_PATH="$UKEY" bash "$SCRIPT" --updater --dry-run; }
for mode in 640 644 604; do
  chmod "$mode" "$UKEY"
  run updater_dry
  expect "C5c [shim] a mode-$mode updater key -> refuse" 2 "readable by other users of this Mac"
done
chmod 600 "$UKEY"
run updater_dry
expect "C5d [shim] a mode-600 updater key resolves" 0 "(mode 600)"

run notarize RICHOS_NOTARY_KEY="$KEY" RICHOS_NOTARY_KEY_ID=ABCDE12345 RICHOS_NOTARY_ISSUER=uuid
expect "C6 [shim] a complete API key resolves" 0 "notarization        : ON via App Store Connect API key ABCDE12345"
run notarize RICHOS_NOTARY_PROFILE=richos
expect "C7 [shim] a keychain profile resolves" 0 "keychain profile 'richos'"
run notarize APPLE_ID=a@b.com APPLE_PASSWORD=abcd-efgh-ijkl-mnop APPLE_TEAM_ID=TEAM00001
expect "C8 [shim] APPLE_ID/PASSWORD/TEAM_ID are NOT a credential path here" 2 "no APPLE_PASSWORD path"

echo ""
echo "=== D. verify_bundle, against a real signed bundle ==="
# A sandbox whose layout package-app.sh derives from its own location: the real
# script, symlinked, beside a synthetic src-tauri holding the icon it compares
# against. The bundle below is signed by the real codesign.
SB="$TMP/sandbox"; mkdir -p "$SB/scripts" "$SB/src-tauri/icons"
ln -s "$SCRIPT" "$SB/scripts/package-app.sh"
ln -s "$SRC_DIR/lib" "$SB/scripts/lib"
head -c 4096 /dev/urandom > "$SB/src-tauri/icons/icon.icns"
# THE VERSION THE SANDBOX INTENDS. `verify_bundle` compares the bundle's
# CFBundleShortVersionString against this, which is what stops a manifest announcing one
# version over a bundle that is another. 9.9.9 is deliberately a version no release of
# RichOS has ever had, so D10's "no release exists" line is a fact about the fixture and
# not about whatever this repository happens to be tagged at today.
printf '[package]\nname = "richos-tauri"\nversion = "9.9.9"\n' > "$SB/src-tauri/Cargo.toml"
SBS="$SB/scripts/package-app.sh"

make_bundle() {   # make_bundle <dir>
  local b="$1"
  rm -rf "$b"; mkdir -p "$b/Contents/MacOS" "$b/Contents/Resources"
  cp "$TMP/fixture" "$b/Contents/MacOS/RichOS"
  cp "$SB/src-tauri/icons/icon.icns" "$b/Contents/Resources/icon.icns"
  printf '# Third-party notices\n\n## Bundled data compiled into the application\n' \
    > "$b/Contents/Resources/THIRD-PARTY-NOTICES.md"
  cat > "$b/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>RichOS</string>
<key>CFBundleIdentifier</key><string>com.richos.app</string>
<key>CFBundleName</key><string>RichOS</string>
<key>RichOSSourceCommit</key><string>0123456789abcdef0123456789abcdef01234567</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSMicrophoneUsageDescription</key><string>Rich listens when you tap the talk button.</string>
<key>CFBundleShortVersionString</key><string>9.9.9</string>
</dict></plist>
PLIST
  codesign --force --sign - --timestamp=none "$b" >/dev/null 2>&1
}

BUNDLE="$TMP/RichOS.app"
make_bundle "$BUNDLE"

run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D1 a real ad-hoc bundle verifies in adhoc mode" 0 "OK: RichOS.app verifies"

run bash "$SBS" --verify-only "$BUNDLE" --sign developer-id
missing=""
for needle in \
  "expected a Developer ID Application signature" \
  "hardened runtime is NOT enabled" \
  "no secure timestamp on the signature" \
  "com.apple.security.device.audio-input is not in the signed entitlements" \
  "designated requirement is a cdhash expression" \
  "does not pin the bundle identifier 'com.richos.app'" \
  "has no 'anchor apple generic' clause" \
  "does not pin the team (certificate leaf[subject.OU])"
do
  printf '%s' "$OUT" | grep -Fq -- "$needle" || missing="$missing | $needle"
done
if [ "$CODE" = 1 ] && [ -z "$missing" ]; then
  ok "D2 ...and fails in developer-id mode on all EIGHT grounds, four of them on the requirement"
else
  bad "D2 the developer-id arm did not fire completely" "exit $CODE; not reported:$missing"
fi

make_bundle "$BUNDLE"; rm -f "$BUNDLE/Contents/Resources/icon.icns"
run bash "$SBS" --verify-only "$BUNDLE"
expect "D3 a bundle with no shipped icon fails" 1 "Contents/Resources/icon.icns is missing"

make_bundle "$BUNDLE"; head -c 4096 /dev/urandom > "$BUNDLE/Contents/Resources/icon.icns"
run bash "$SBS" --verify-only "$BUNDLE"
expect "D4 a shipped icon this repository did not generate fails" 1 "carrying an icon this repository did not generate"

make_bundle "$BUNDLE"; rm -f "$BUNDLE/Contents/Resources/THIRD-PARTY-NOTICES.md"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D4b a bundle with no third-party notices fails" 1 "Contents/Resources/THIRD-PARTY-NOTICES.md is missing"

make_bundle "$BUNDLE"; printf '# Third-party notices\n' > "$BUNDLE/Contents/Resources/THIRD-PARTY-NOTICES.md"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D4c notices without the compiled-in data section fail" 1 "no \"Bundled data compiled into the application\" section"

make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Delete :NSMicrophoneUsageDescription' "$BUNDLE/Contents/Info.plist" >/dev/null 2>&1
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D5 no NSMicrophoneUsageDescription fails" 1 "voice mode cannot even ask for the microphone"

make_bundle "$BUNDLE"; codesign --remove-signature "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D6 a bundle whose signature was removed fails" 1 "codesign --verify --deep --strict FAILED"

make_bundle "$BUNDLE"
run bash "$SBS" --verify-only "$BUNDLE" --sign developer-id --expect-notarized
expect "D7 --expect-notarized runs the stapled-ticket check" 1 "no valid stapled notarization ticket"

# Each added invariant is defeated independently with a freshly signed bundle.
resign() { codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1; }
make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Delete :RichOSSourceCommit' "$BUNDLE/Contents/Info.plist"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I1 missing source stamp refuses" 1 "bundle-source-identity"
make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Set :RichOSSourceCommit ffffffffffffffffffffffffffffffffffffffff' "$BUNDLE/Contents/Info.plist"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I2 inconsistent compiled source stamp refuses" 1 "bundle-source-identity"
make_bundle "$BUNDLE"
printf '\0/Users/fixture-builder/project\0' > "$BUNDLE/Contents/Resources/leak.bin"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I3 binary home path refuses during verify-only" 1 "bundle-home-paths"
make_bundle "$BUNDLE"
printf '\0Synthetic Forbiddenperson\0' > "$BUNDLE/Contents/Resources/leak.bin"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I4 binary private name refuses" 1 "bundle-private-names"
make_bundle "$BUNDLE"
touch "$BUNDLE/Contents/Resources/Synthetic Forbiddenperson"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I5 private member name refuses" 1 "bundle-private-names"
make_bundle "$BUNDLE"
run env RICHOS_NAMED_PERSONS_FILE="$TMP/absent-list" bash "$SBS" --verify-only "$BUNDLE" --release
expect "I6 missing private list cannot look clean" 1 "bundle-private-names"
make_bundle "$BUNDLE"
ln -s "$SB/src-tauri/icons/icon.icns" "$BUNDLE/Contents/Resources/outside"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I7 a member resolving outside the bundle refuses" 1 "bundle-contained-paths"
make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable ../MacOS/RichOS' "$BUNDLE/Contents/Info.plist"
resign
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I8 executable path traversal refuses" 1 "bundle-executable-path"
make_bundle "$BUNDLE"
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "I9 intact positive control recovers" 0 "diagnostic only"

# ---- D13-D17: THE BUNDLE HOLDS NO STATE (spec point 17) -------------------------------
#
# WHY THESE ARE NOT THE ASSERTION THE SPEC ASKED FOR. Point 17 names "a packaging-test
# assertion that no shipped path inside `Contents/` is owner-writable". D13 is that
# assertion, run as a MEASUREMENT rather than as a check, and it is what disqualifies it:
# every path in a correct macOS bundle is owner-writable, because 0755 directories and 0644
# files are what a bundle is made of. Measured the same day across three real bundles —
# RichOS 8/8, Calculator 111/111, Safari 2044/2044, so 2163 of 2163 — and group- or
# world-writable across the same 2163 paths: zero. So the predicate that is always a defect
# is the group/other one, and the STATE half of point 17 is a question about which paths
# shipped, not about their mode. D14-D17 hold both.

CLEAN_UW="$(find "$BUNDLE/Contents" -perm -u+w 2>/dev/null | wc -l | tr -d ' ')"
CLEAN_ALL="$(find "$BUNDLE/Contents" 2>/dev/null | wc -l | tr -d ' ')"
CLEAN_GW="$(find "$BUNDLE/Contents" \( -perm -g+w -o -perm -o+w \) 2>/dev/null | wc -l | tr -d ' ')"
make_bundle "$BUNDLE"
run bash "$SBS" --verify-only "$BUNDLE" --release
if [ "$CODE" = 0 ] && [ "$CLEAN_UW" = "$CLEAN_ALL" ] && [ "$CLEAN_GW" = 0 ]; then
  ok "D13 a correct bundle is owner-writable throughout ($CLEAN_UW/$CLEAN_ALL) and still verifies — which is why the check is group/other, not owner"
else
  bad "D13 the owner-writable measurement" \
      "exit $CODE; owner-writable $CLEAN_UW of $CLEAN_ALL; group-or-world-writable $CLEAN_GW"
fi

make_bundle "$BUNDLE"
chmod g+w "$BUNDLE/Contents/Info.plist"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D14 a group-writable path inside Contents/ fails" 1 "writable by group or other"

make_bundle "$BUNDLE"
chmod o+w "$BUNDLE/Contents/Resources"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D15 a world-writable directory inside Contents/ fails" 1 "writable by group or other"

make_bundle "$BUNDLE"
printf '{"event":"ThreadCreated"}\n' > "$BUNDLE/Contents/Resources/conversation-ledger.jsonl"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D16 a shipped .jsonl is a state-in-bundle defect and fails at BUILD time" 1 "the bundle holds no state"

make_bundle "$BUNDLE"
printf '{"theme":"dark"}\n' > "$BUNDLE/Contents/Resources/config.json"
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D17 a shipped config.json fails — his preferences must never ride inside the app" 1 "config.json"

# ---- THE VERSION THE BUNDLE WILL CALL ITSELF (CEO, 2026-09-17, item 4) -----
#
# He was handed a bundle stamped 1.0.3 — a version that has never been released and
# never will be under that number — and the window told him "RichOS 1.0.3 is up to
# date" because that IS what the bundle is. These four cases are the moment that fact
# becomes visible: at the build, not on his Mac.

make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Delete :CFBundleShortVersionString' "$BUNDLE/Contents/Info.plist" >/dev/null 2>&1
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D8 a bundle with no CFBundleShortVersionString fails" 1 "cannot tell anyone which version it is"

make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 1.0.3' "$BUNDLE/Contents/Info.plist" >/dev/null 2>&1
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
run bash "$SBS" --verify-only "$BUNDLE"
expect "D9 a bundle whose version is not the one this build intended fails" 1 "the bundle says it is 1.0.3 and this build intended 9.9.9"

make_bundle "$BUNDLE"
run bash "$SBS" --verify-only "$BUNDLE" --release
expect "D10 a version with no release says so, declared --release, in the run that makes the bundle" 0 "NO v9.9.9 RELEASE EXISTS"

# AND THE ONE CALLER THAT OVERRIDES THE VERSION ON PURPOSE STILL WORKS. `updater-e2e.sh`
# builds 0.1.0 and 0.1.1 out of one tree through RICHOS_EXTRA_TAURI_CONFIG; both of those
# bundles are correct and neither may be refused by the check above.
make_bundle "$BUNDLE"
/usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 0.1.1' "$BUNDLE/Contents/Info.plist" >/dev/null 2>&1
codesign --force --sign - --timestamp=none "$BUNDLE" >/dev/null 2>&1
RICHOS_EXTRA_TAURI_CONFIG='{"version": "0.1.1"}' run bash "$SBS" --verify-only "$BUNDLE"
expect "D11 an explicit version overlay is the intended version, not a mismatch" 0 "version 0.1.1"

# AND THE OTHER BRANCH OF THAT LINE, which is the half a reporting check usually ships
# untested: when a release of this version DOES exist, it says so and names the commit.
# A second sandbox, because the first one is deliberately not a repository at all.
SBG="$TMP/sandbox-tagged"; mkdir -p "$SBG/scripts" "$SBG/src-tauri/icons"
ln -s "$SCRIPT" "$SBG/scripts/package-app.sh"
ln -s "$SRC_DIR/lib" "$SBG/scripts/lib"
cp "$SB/src-tauri/icons/icon.icns" "$SBG/src-tauri/icons/icon.icns"
printf '[package]\nname = "richos-tauri"\nversion = "9.9.9"\n' > "$SBG/src-tauri/Cargo.toml"
# HOOKS OFF FOR THE FIXTURE, and not as a convenience. This machine sets
# `core.hooksPath` globally to an identity guard, which refuses a commit in a repository
# it does not recognize — so a fixture that inherited it was red on the operator's Mac and
# green everywhere else, which is precisely the environment-dependent verdict section C's
# own note was written about. The fixture states its own git environment instead.
FIXTURE_GIT=(git -c core.hooksPath=/dev/null -c commit.gpgsign=false \
             -c user.email=fixture@localhost -c user.name=fixture)
"${FIXTURE_GIT[@]}" -C "$SBG" init -q . >/dev/null 2>&1
"${FIXTURE_GIT[@]}" -C "$SBG" commit -q --allow-empty -m fixture >/dev/null 2>&1
"${FIXTURE_GIT[@]}" -C "$SBG" tag v9.9.9 >/dev/null 2>&1
SBG_SHA="$("${FIXTURE_GIT[@]}" -C "$SBG" rev-parse -q --verify HEAD 2>/dev/null || true)"
if [ -z "$SBG_SHA" ]; then
  bad "D12 a version that IS a release says so, and names the commit" \
      "the fixture repository could not be created, so this branch was never exercised"
else
  make_bundle "$BUNDLE"
  run bash "$SBG/scripts/package-app.sh" --verify-only "$BUNDLE" --release
  expect "D12 a version that IS a release says so, and names the commit" 0 "v9.9.9 is a release in this repository ($SBG_SHA)"
fi

echo ""
echo "=== F. version intent — release, nightly, and the development-build default ==="
# CEO 2026-09-17 item 4, the open half closed by esc-20260917T081214Z-b466c998: intent is
# now DECLARED (--release / --nightly <version> / neither), never guessed, and a bare,
# undeclared version is refused rather than merely reported.

make_bundle "$BUNDLE"
run bash "$SBS" --verify-only "$BUNDLE"
expect "F1 a bare, undeclared version is refused unless --release says this bundle is one" 1 "nothing declared --release for it"

run bash "$SBS" --verify-only "$BUNDLE" --release
expect "F2 --release declares intent and the same bare version is accepted" 0 "OK: RichOS.app verifies"

# A nightly-suffixed version was never "bare" in the first place — the regex the refusal
# checks for is plain X.Y.Z, and a `-nightly.` version never matches it. Its own sandbox,
# because `make_bundle`'s fixed 9.9.9 Cargo.toml would otherwise disagree with it.
NIGHTLY_SB="$TMP/sandbox-nightly"; mkdir -p "$NIGHTLY_SB/scripts" "$NIGHTLY_SB/src-tauri/icons"
ln -s "$SCRIPT" "$NIGHTLY_SB/scripts/package-app.sh"
ln -s "$SRC_DIR/lib" "$NIGHTLY_SB/scripts/lib"
cp "$SB/src-tauri/icons/icon.icns" "$NIGHTLY_SB/src-tauri/icons/icon.icns"
printf '[package]\nname = "richos-tauri"\nversion = "9.9.9-nightly.20260917.1"\n' > "$NIGHTLY_SB/src-tauri/Cargo.toml"
NIGHTLY_BUNDLE="$TMP/RichOS-nightly.app"
make_bundle "$NIGHTLY_BUNDLE"
/usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 9.9.9-nightly.20260917.1' "$NIGHTLY_BUNDLE/Contents/Info.plist" >/dev/null 2>&1
codesign --force --sign - --timestamp=none "$NIGHTLY_BUNDLE" >/dev/null 2>&1

run bash "$NIGHTLY_SB/scripts/package-app.sh" --verify-only "$NIGHTLY_BUNDLE"
expect "F3 a nightly-suffixed version needs no declared intent — it was never 'bare'" 0 "OK: RichOS-nightly.app verifies"

run bash "$NIGHTLY_SB/scripts/package-app.sh" --verify-only "$NIGHTLY_BUNDLE" --nightly 9.9.9-nightly.20260917.1
expect "F4 --nightly on that same version resolves" 0 "OK: RichOS-nightly.app verifies"

# --nightly does NOT excuse a BARE version — only --release does. Back on the plain 9.9.9
# fixture, declaring --nightly must not launder the exact failure --release is for.
run bash "$SBS" --verify-only "$BUNDLE" --nightly 9.9.9
expect "F5 --nightly does NOT excuse a bare version — only --release does" 1 "nothing declared --release for it"

run bash "$SBS" --verify-only "$BUNDLE" --release --nightly 9.9.9
expect "F6 --release and --nightly cannot both be declared" 2 "mutually exclusive"

run bash "$SBS" --verify-only "$BUNDLE" --nightly
expect "F7 --nightly with no value is refused" 2 "requires the version"

# ---- F8-F11: the real script, --dry-run, against this repository's own tree -----------
# --dry-run resolves the version without building, so these run the REAL script rather
# than a sandbox: there is no bundle to check, only the declared configuration.
APP_DIR="$(cd "$SRC_DIR/.." && pwd)"
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$APP_DIR/src-tauri/Cargo.toml" | head -1)"
REAL_SHA="$(git -C "$SRC_DIR" rev-parse --short HEAD 2>/dev/null || true)"

run bash "$SCRIPT" --dry-run
if [ "$CODE" = 0 ] && [ -n "$REAL_SHA" ] \
   && printf '%s' "$OUT" | grep -q "intent              : dev" \
   && printf '%s' "$OUT" | grep -Eq "version             : ${CARGO_VERSION}-dev\.${REAL_SHA}(\.dirty)? "; then
  ok "F8 a plain run with no declared intent is a development build, stamped {version}-dev.{commit}"
else
  bad "F8 the default dry run did not show a dev-stamped version" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"
fi

# F15: the CPU circuit breaker stopped rustc at 4.5-7.3 cores in three engineers' dev builds
# on 2026-10-11, each then rebuilt by hand under `proc_tree.py run <pid> --guard-role
# release-build`. The dry run prints the launcher the build uses, and the build line itself
# must run cargo through that same array, so the printed line is the one that runs.
# The regex matches the script's literal `${build_launch[@]}` text, so it must not expand.
# shellcheck disable=SC2016
BUILD_LINE_RE='"\$\{build_launch\[@\]\}"\}? env RICHOS_SOURCE_SHA="\$source_stamp" cargo "\$\{build_args\[@\]\}"'
run bash "$SCRIPT" --dry-run
PROC_TREE="$(cd "$APP_DIR/.." && pwd)/engine/scripts/lib/proc_tree.py"
if [ "$CODE" != 0 ]; then
  bad "F15 the compile runs under the release-build role" "the dry run exited $CODE"
elif ! printf '%s' "$OUT" | grep -Fq "build runs under    : python3 $PROC_TREE run "; then
  bad "F15 the compile runs under the release-build role" "no supervisor line naming $PROC_TREE: $(printf '%s' "$OUT" | grep 'build runs under' | head -1)"
elif ! printf '%s' "$OUT" | grep 'build runs under' | grep -Fq -- "--guard-role release-build --"; then
  bad "F15 the compile runs under the release-build role" "the supervisor line does not name --guard-role release-build"
elif ! grep -Eq "$BUILD_LINE_RE" "$SCRIPT"; then
  bad "F15 the compile runs under the release-build role" "the cargo build line does not run through build_launch"
else
  ok "F15 the compile runs under the CPU guard's release-build role, as the nightly's build step does"
fi

# F15b: where managed verification is on, the supervisor refuses a run with no proof-run plan
# before cargo starts (exit 75; review rv-20261011T142043Z-4d2b357d-d8c1 #1). A packaging run
# started by hand then compiles directly, as it did before the supervisor, and says why; the
# launcher it prints is checked against the supervisor itself under the same CPU guard state.
F15B_STATE="$TMP/f15b-cpu-guard"; mkdir -p "$F15B_STATE"
printf '{"protocol": 1}\n' > "$F15B_STATE/verification-enabled.json"
run env RICHOS_CPU_GUARD_STATE="$F15B_STATE" RICHOS_VERIFICATION_OWNER= bash "$SCRIPT" --dry-run
F15B_LINE="$(printf '%s' "$OUT" | grep 'build runs under' | head -1)"
F15B_SUPERVISOR_RC="$(cd "$TMP" && RICHOS_CPU_GUARD_STATE="$F15B_STATE" RICHOS_VERIFICATION_OWNER='' \
  python3 -B "$PROC_TREE" run "$$" --guard-role release-build -- true >/dev/null 2>&1; echo $?)"
if [ "$CODE" != 0 ]; then
  bad "F15b a hand-started packaging run under managed verification compiles directly" "the dry run exited $CODE"
elif [ "$F15B_SUPERVISOR_RC" != 75 ]; then
  bad "F15b a hand-started packaging run under managed verification compiles directly" \
      "the fixture did not reproduce the supervisor's refusal (it exited $F15B_SUPERVISOR_RC, not 75)"
elif printf '%s' "$F15B_LINE" | grep -Fq "$PROC_TREE"; then
  bad "F15b a hand-started packaging run under managed verification compiles directly" \
      "it still launches through the supervisor, which refuses it before cargo (exit 75): $F15B_LINE"
elif ! printf '%s' "$F15B_LINE" | grep -Fq "managed verification is on"; then
  bad "F15b a hand-started packaging run under managed verification compiles directly" "the line does not say why: $F15B_LINE"
else
  ok "F15b a hand-started packaging run under managed verification compiles directly and says why; the supervisor would have refused it (75)"
fi

run bash "$SCRIPT" --release --dry-run
expect "F9 --release keeps the bare Cargo.toml version, unstamped" 0 "version             : $CARGO_VERSION"

run bash "$SCRIPT" --nightly "0.0.1-nightly.20260101.1" --dry-run
expect "F10 --nightly refuses when Cargo.toml disagrees with the declared version" 2 "Cargo.toml says $CARGO_VERSION"

run bash "$SCRIPT" --nightly "$CARGO_VERSION" --dry-run
expect "F11 --nightly with the version Cargo.toml already carries resolves" 0 "version             : $CARGO_VERSION"

# ---- F12-F14: a real, disposable git fixture, so .dirty is provable without touching
# this repository's own tree -------------------------------------------------------------
DEV_SB="$TMP/sandbox-dev"; mkdir -p "$DEV_SB/scripts" "$DEV_SB/src-tauri/icons"
ln -s "$SCRIPT" "$DEV_SB/scripts/package-app.sh"
  ln -s "$SRC_DIR/lib" "$DEV_SB/scripts/lib"
cp "$SB/src-tauri/icons/icon.icns" "$DEV_SB/src-tauri/icons/icon.icns"
printf '[package]\nname = "richos-tauri"\nversion = "9.9.9"\n' > "$DEV_SB/src-tauri/Cargo.toml"
"${FIXTURE_GIT[@]}" -C "$DEV_SB" init -q . >/dev/null 2>&1
"${FIXTURE_GIT[@]}" -C "$DEV_SB" add -A >/dev/null 2>&1
"${FIXTURE_GIT[@]}" -C "$DEV_SB" commit -q -m fixture >/dev/null 2>&1
DEV_SHA="$("${FIXTURE_GIT[@]}" -C "$DEV_SB" rev-parse --short HEAD 2>/dev/null || true)"

if [ -z "$DEV_SHA" ]; then
  bad "F12 a clean fixture tree's development stamp carries no .dirty suffix" \
      "the fixture repository could not be created, so this branch was never exercised"
  bad "F13 an uncommitted change appends .dirty to the development stamp" \
      "the fixture repository could not be created, so this branch was never exercised"
else
  run bash "$DEV_SB/scripts/package-app.sh" --dry-run
  expect "F12 a clean fixture tree's development stamp carries no .dirty suffix" 0 "version             : 9.9.9-dev.$DEV_SHA "

  echo "dirty" >> "$DEV_SB/README-fixture.txt"
  run bash "$DEV_SB/scripts/package-app.sh" --dry-run
  expect "F13 an uncommitted change appends .dirty to the development stamp" 0 "version             : 9.9.9-dev.$DEV_SHA.dirty"

  DEV_BUNDLE="$TMP/RichOS-dev.app"
  make_bundle "$DEV_BUNDLE"
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString 9.9.9-dev.$DEV_SHA" "$DEV_BUNDLE/Contents/Info.plist" >/dev/null 2>&1
  codesign --force --sign - --timestamp=none "$DEV_BUNDLE" >/dev/null 2>&1
  run bash "$DEV_SB/scripts/package-app.sh" --verify-only "$DEV_BUNDLE"
  expect "F14 a -dev. bundle is accepted with no declared intent, and says so" 0 "development build — not a release"
fi

echo ""
echo "=== E. the operator's environment cannot reach a case ==="
# The negative control for the isolation above, and the regression test for the
# 2026-09-16 nightly. Export a COMPLETE, hostile credential set — the shape
# nightly-local.py actually put in front of this suite — and re-run the four cases
# that went red, plus one identity case. Every verdict must be the C-section one.
# Without `isolate` in `run`, E1, E2, E4 and E5 go red here for the same reason
# their C counterparts did, so this section cannot pass by accident.
export RICHOS_NOTARIZE=1
export RICHOS_NOTARY_KEY="$KEY" RICHOS_NOTARY_KEY_ID=LEAKED12345 RICHOS_NOTARY_ISSUER=leaked-issuer
export RICHOS_NOTARY_PROFILE=leaked-profile
export APPLE_ID=leaked@example.invalid APPLE_PASSWORD=leak-leak-leak-leak APPLE_TEAM_ID=LEAKTEAM01
export TAURI_SIGNING_PRIVATE_KEY_PATH="$KEY" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=leaked
export RICHOS_SIGNING_IDENTITY="Developer ID Application: Leaked (LEAK00000)"

run notarize
expect "E1 [shim] an exported API key does not satisfy 'no credentials'" 2 "no APPLE_PASSWORD path"
run notarize RICHOS_NOTARY_KEY_ID=ABCDE12345
expect "E2 [shim] exported halves do not complete a half-supplied key" 2 "RICHOS_NOTARY_KEY RICHOS_NOTARY_ISSUER"
run notarize RICHOS_NOTARY_PROFILE=richos
expect "E3 [shim] an exported key does not outrank the case's profile" 0 "keychain profile 'richos'"
run notarize APPLE_ID=a@b.com APPLE_PASSWORD=abcd-efgh-ijkl-mnop APPLE_TEAM_ID=TEAM00001
expect "E4 [shim] an exported key does not excuse the Apple-ID refusal" 2 "no APPLE_PASSWORD path"
run shimmed env SHIM_IDENTITIES=0 bash "$SCRIPT" --sign developer-id --dry-run
expect "E5 [shim] an exported RICHOS_SIGNING_IDENTITY does not reach discovery" 2 "install-signing-cert.sh"

unset RICHOS_NOTARIZE RICHOS_NOTARY_KEY RICHOS_NOTARY_KEY_ID RICHOS_NOTARY_ISSUER RICHOS_NOTARY_PROFILE
unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
unset TAURI_SIGNING_PRIVATE_KEY_PATH TAURI_SIGNING_PRIVATE_KEY_PASSWORD RICHOS_SIGNING_IDENTITY

echo ""
echo "=== Z. this suite touched nothing of the operator's ==="
REAL_IDS_AFTER="$(security find-identity -v -p codesigning 2>/dev/null || true)"
if [ "$REAL_IDS_AFTER" = "$REAL_IDS_BEFORE" ]; then
  ok "Z the real keychain's codesigning inventory is identical before and after"
else
  bad "Z THE SUITE CHANGED THE REAL KEYCHAIN" "before='$REAL_IDS_BEFORE' after='$REAL_IDS_AFTER'"
fi

run python3 "$SRC_DIR/lib/probe_packaged.test.py"
expect "I10 filesystem guard: intact, outside copy blocked, defense defeated, restored" 0 "OK"

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== package-app tests: $FAIL FAILED, $PASS passed ==="
  exit 1
fi
echo "=== package-app tests: all $PASS passed ==="

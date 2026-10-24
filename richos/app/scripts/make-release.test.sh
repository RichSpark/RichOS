#!/usr/bin/env bash
#
# make-release.test.sh — the release chain's REFUSALS, exercised without a release build
# and without a GitHub release.
#
# WHAT THIS SUITE CAN AND CANNOT PROVE, first, because the distinction decides what belongs
# here.
#
#   IT PROVES     that the order is enforced rather than documented: that a pin cannot be
#                 built into an app until the published bytes have been read back, that a
#                 stale receipt is caught, and that a digest that does not match what a URL
#                 serves stops the release. It runs in seconds, against a local HTTP server
#                 standing in for the release host.
#
#   IT CANNOT     prove that a release installs. That is a claim about a signed bundle
#                 replacing a running one, and it belongs to `richos/app/scripts/updater-e2e.sh`.
#                 Nor can it prove the checks that only exist once a real bundle has been
#                 built — the digest inside the executable, the stapled ticket inside the
#                 first-install archive. Those run inside `make-release.sh app`, on the
#                 artifact, every time.
#
# Cases:
#   R1  `plan` names the version, the tag and every URL, and creates nothing
#   R2  `verify-engine` with no pin refuses
#   R3  `verify-engine` against a URL that does not serve the asset refuses, and says the
#       failure would otherwise ship inside a signed binary
#   R4  `verify-engine` against bytes that are NOT the pinned bytes refuses
#   R5  `verify-engine` against the pinned bytes writes a receipt naming that digest
#   R5a `app` with no named-person deny-list stops at the names gate, BEFORE the pin check —
#       the gate's precedence, asserted rather than assumed
#   R6  `app` with no pin refuses
#   R7  `app` with a pin and no receipt refuses — the digest would be a claim about a local file
#   R8  `app` with a receipt for a DIFFERENT digest refuses — the asset was rebuilt after it
#       was verified
#   R9  `app` with a good receipt and no signing key refuses
#   R10 `verify-release` with nothing staged refuses
#   R11-R14 standalone Doctor round-trip, missing payload, invalid signature and missing ticket
#
# The refusal journeys use the real release script and names gate in a small tracked
# tree. Scanning the entire working repository four times tests no additional ordering
# behavior. A planted hit below proves the real gate still refuses before the pin check.
# Full-tree privacy remains a production release gate and has its own owning suite.
# run-tests: inputs richos/app/scripts/make-release.sh richos/app/scripts/lib/updater_tar.py richos/app/scripts/lib/worktree-resource.sh richos/app/src-tauri/Cargo.toml richos/app/src-tauri/tauri.conf.json richos/engine/VERSION richos/engine/scripts/named-persons.sh richos/engine/scripts/lib/named-persons.sh richos/engine/scripts/lib/named-persons.py
# run-tests: covers richos/app/scripts/make-release.sh
# run-tests: no-host-screen: its /Contents/MacOS/ matches are a fixture RichDoctor.app under mktemp that is copied, signed, archived and verified, never executed or opened
set -uo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$SRC_DIR/.." && pwd)"

TMP="$(python3 -c 'import tempfile; print(tempfile.mkdtemp(prefix="make-release-test-"))')"
SERVER_PID=""
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null; rm -rf "$TMP"; }
trap cleanup EXIT

FIXTURE="$TMP/repo"
mkdir -p "$FIXTURE/richos/app/scripts" "$FIXTURE/richos/app/src-tauri" \
         "$FIXTURE/richos/engine/scripts/lib"
cp "$SRC_DIR/make-release.sh" "$FIXTURE/richos/app/scripts/"
mkdir -p "$FIXTURE/richos/app/scripts/lib"
cp "$SRC_DIR/lib/updater_tar.py" "$FIXTURE/richos/app/scripts/lib/"
cp "$APP/src-tauri/Cargo.toml" "$APP/src-tauri/tauri.conf.json" "$FIXTURE/richos/app/src-tauri/"
cp "$APP/../engine/VERSION" "$FIXTURE/richos/engine/"
cp "$APP/../engine/scripts/named-persons.sh" "$FIXTURE/richos/engine/scripts/"
cp "$APP/../engine/scripts/lib/named-persons.sh" "$APP/../engine/scripts/lib/named-persons.py" \
   "$FIXTURE/richos/engine/scripts/lib/"
git -C "$FIXTURE" init -q
git -C "$FIXTURE" add .
SCRIPT="$FIXTURE/richos/app/scripts/make-release.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

run() { OUT="$("$@" 2>&1)"; CODE=$?; return 0; }
expect() {   # expect <name> <wanted-code> <substring>
  local name="$1" want="$2" needle="$3"
  if [ "$CODE" != "$want" ]; then
    bad "$name" "exit $CODE, wanted $want. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-220)"
  elif ! printf '%s' "$OUT" | grep -Fq -- "$needle"; then
    bad "$name" "exit $want as wanted, but the output never said '$needle'"
  else
    ok "$name"
  fi
}

OUTDIR="$TMP/staging"
TAG="v0.0.0-test"
REL=(bash "$SCRIPT")

# ---------------------------------------------------------------------------------------
# THE NAMES GATE, PUT INTO A KNOWN STATE INSTEAD OF INHERITED FROM THE MACHINE
# ---------------------------------------------------------------------------------------
#
# `make-release.sh app` runs `names_gate` before it looks at anything else, and that gate
# REFUSES when the operator's deny-list is absent — deliberately, because "no list" must
# never be able to look like "no names found". The list lives outside every repository, at
# `$HOME/.richos-privacy/named-persons`, and a machine that is not the owner's does not
# have one.
#
# So R6-R9 below, which are about the PIN/RECEIPT/KEY ordering and have nothing to do with
# names, were passing on exactly one machine in the world and failing everywhere else.
# MEASURED on a GitHub `macos-latest` runner, 2026-09-10, run 34444955517: all four red,
# each one "exit 2 as wanted, but the output never said ..." — the right exit code, from
# the wrong refusal, which is precisely the confusion `expect`'s needle exists to catch.
# It caught it. The suite was the thing that was wrong.
#
# `RICHOS_NAMED_PERSONS_FILE` is the deny-list's own documented override
# (`engine/scripts/lib/named-persons.sh::np_list_path`). A `sha256:` entry is a hashed
# window rather than a name, so this fixture discloses nothing and — being a digest no
# two-token window in any tree will ever produce — matches nothing. The gate therefore runs
# for real and returns CLEAN, and the four cases below reach the refusals they are named
# after.
#
# The absent case is not thrown away by this; it becomes R5a, which asserts the precedence
# on purpose instead of by accident.
NPLIST="$TMP/named-persons"
printf '# fixture: a digest that matches nothing, so the gate runs and finds nothing\n' > "$NPLIST"
printf 'sha256:2:%s\n' "0000000000000000000000000000000000000000000000000000000000000000" >> "$NPLIST"
chmod 600 "$NPLIST"
# Every `app` invocation below goes through this: no signing key, and the names gate in
# whichever state the case is about.
APP_ENV=(env -u TAURI_SIGNING_PRIVATE_KEY -u TAURI_SIGNING_PRIVATE_KEY_PATH)

echo ""
echo "=== R. the release chain refuses out of order ==="

run "${REL[@]}" plan --tag "$TAG" --out "$OUTDIR"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$APP/src-tauri/Cargo.toml" | head -1)"
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Fq "$VERSION" \
   && printf '%s' "$OUT" | grep -Fq "releases/latest/download/latest.json" \
   && printf '%s' "$OUT" | grep -Eq 'RichDoctor-macos-(aarch64|x86_64)\.zip' \
   && [ ! -d "$OUTDIR" ]; then
  ok "R1 plan names the version ($VERSION) and the endpoint, and creates nothing"
else
  bad "R1 plan names the version and the endpoint, and creates nothing" \
      "exit $CODE; staging dir exists: $( [ -d "$OUTDIR" ] && echo yes || echo no )"
fi

run "${REL[@]}" verify-engine --tag "$TAG" --out "$OUTDIR"
expect "R2 verify-engine with no pin refuses" 2 "no pin at"

# A local stand-in for the release host. The asset is a file whose only job is to have a
# digest; nothing here cares that it is not an engine.
SERVE="$TMP/serve"; mkdir -p "$SERVE" "$OUTDIR"
ENGINE_VERSION="$(tr -d '[:space:]' < "$APP/../engine/VERSION")"
ASSET="richos-engine-$ENGINE_VERSION.tar.gz"
printf 'the bytes that were published\n' > "$SERVE/$ASSET"
cp "$SERVE/$ASSET" "$OUTDIR/$ASSET"
GOOD_DIGEST="$(/usr/bin/shasum -a 256 "$SERVE/$ASSET" | awk '{print $1}')"

# DERIVED FROM THIS CHECKOUT'S PATH, never a fixed 8975. This is the only port anything in
# this directory binds, and `run-tests.sh`'s own INDEPENDENCE section recorded what a fixed
# one costs: *"Two SIMULTANEOUS run-tests.sh runs on one host still would"* collide. This
# Mac runs a worktree per engineer plus the nightly's own checkout, so "two simultaneous
# runs" is the normal state of the machine, not an edge case. Stable per worktree (the same
# checkout gets the same port every run, so a failure is reproducible), distinct between
# siblings. See `lib/worktree-resource.sh`; RICHOS_RELEASE_TEST_PORT still wins.
. "$SRC_DIR/lib/worktree-resource.sh"
PORT="${RICHOS_RELEASE_TEST_PORT:-$(worktree_port "$(worktree_root "$SRC_DIR")" 8975 1000)}"
( cd "$SERVE" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1 ) &
SERVER_PID=$!
# Job control's own "Terminated" line would otherwise be the last thing this suite prints,
# after its verdict, which reads like a failure and is not one.
disown $SERVER_PID 2>/dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do
  curl -sS -o /dev/null "http://127.0.0.1:$PORT/$ASSET" 2>/dev/null && break
  sleep 0.3
done
if ! curl -sS -o /dev/null "http://127.0.0.1:$PORT/$ASSET" 2>/dev/null; then
  echo "make-release.test.sh: could not serve a fixture on port $PORT (set RICHOS_RELEASE_TEST_PORT)." >&2
  exit 3
fi

write_pin() {  # write_pin <url> <digest>
  cat > "$OUTDIR/engine-pin.env" <<PIN
export RICHOS_ENGINE_VERSION=$ENGINE_VERSION
export RICHOS_ENGINE_URL=$1
export RICHOS_ENGINE_SHA256=$2
PIN
}

write_pin "http://127.0.0.1:$PORT/not-published.tar.gz" "$GOOD_DIGEST"
run "${REL[@]}" verify-engine --tag "$TAG" --out "$OUTDIR"
expect "R3 verify-engine against a URL that serves nothing refuses" 1 "inside a signed binary"

write_pin "http://127.0.0.1:$PORT/$ASSET" "0000000000000000000000000000000000000000000000000000000000000000"
run "${REL[@]}" verify-engine --tag "$TAG" --out "$OUTDIR"
expect "R4 verify-engine against bytes that are not the pinned bytes refuses" 1 "THE PUBLISHED BYTES ARE NOT THE PINNED BYTES"

write_pin "http://127.0.0.1:$PORT/$ASSET" "$GOOD_DIGEST"
run "${REL[@]}" verify-engine --tag "$TAG" --out "$OUTDIR"
if [ "$CODE" = 0 ] && grep -q "sha256=$GOOD_DIGEST" "$OUTDIR/engine-published.ok" 2>/dev/null; then
  ok "R5 verify-engine against the pinned bytes writes a receipt naming that digest"
else
  bad "R5 verify-engine against the pinned bytes writes a receipt naming that digest" \
      "exit $CODE; receipt: $(cat "$OUTDIR/engine-published.ok" 2>/dev/null | tr '\n' ' ')"
fi

# `app` never reaches a build in any case below: each refusal happens before it would.
NOPIN="$TMP/nopin"; mkdir -p "$NOPIN"

# R5a — THE GATE COMES FIRST, and this is the case that used to be made by accident, in red,
# on every machine that is not the owner's. With no deny-list the refusal must NOT be the pin
# refusal: `app` has to stop at the names gate before it has an opinion about staging.
run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$TMP/a-list-that-does-not-exist" \
        bash "$SCRIPT" app --tag "$TAG" --out "$NOPIN"
#
# Both halves are asserted. The positive one is the sentence the gate prints on ABSENT —
# outside its once-per-session banner, so it appears on every invocation. The negative one
# is that "no pin at" is NOT in the output, which is what makes this a statement about
# ORDER rather than merely about a refusal happening.
if [ "$CODE" != 2 ]; then
  bad "R5a app with no deny-list refuses at the names gate, before the pin" \
      "exit $CODE, wanted 2. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-220)"
elif ! printf '%s' "$OUT" | grep -Fq "nothing is checking for names"; then
  bad "R5a app with no deny-list refuses at the names gate, before the pin" \
      "exit 2, but not from the names gate: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-220)"
elif printf '%s' "$OUT" | grep -Fq "no pin at"; then
  bad "R5a app with no deny-list refuses at the names gate, before the pin" \
      "it reached the pin check as well — the gate did not stop the run"
else
  ok "R5a app with no deny-list refuses at the names gate, before the pin"
fi

# A real tracked hit must also stop before the pin. No gate stub or skip flag is used.
printf 'Aster Quill\n' > "$FIXTURE/fixture-name.txt"
git -C "$FIXTURE" add fixture-name.txt
printf 'Aster Quill\n' > "$TMP/matching-list"
chmod 600 "$TMP/matching-list"
run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$TMP/matching-list" \
        bash "$SCRIPT" app --tag "$TAG" --out "$NOPIN"
if [ "$CODE" = 1 ] && printf '%s' "$OUT" | grep -Fq 'NAMED-PERSON DENY-LIST: BLOCKED' \
   && ! printf '%s' "$OUT" | grep -Fq 'no pin at'; then
  ok "R5b a tracked name refuses at the real names gate, before the pin"
else
  bad "R5b a tracked name refuses at the real names gate, before the pin" "exit $CODE; $OUT"
fi

run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$NPLIST" \
        bash "$SCRIPT" app --tag "$TAG" --out "$NOPIN"
expect "R6 app with no pin refuses" 2 "no pin at"

NORECEIPT="$TMP/noreceipt"; mkdir -p "$NORECEIPT"
cp "$OUTDIR/engine-pin.env" "$NORECEIPT/engine-pin.env"
run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$NPLIST" \
        bash "$SCRIPT" app --tag "$TAG" --out "$NORECEIPT"
expect "R7 app with a pin and no receipt refuses" 2 "claim about a local file"

STALE="$TMP/stale"; mkdir -p "$STALE"
cp "$OUTDIR/engine-pin.env" "$STALE/engine-pin.env"
sed 's/^sha256=.*/sha256=1111111111111111111111111111111111111111111111111111111111111111/' \
    "$OUTDIR/engine-published.ok" > "$STALE/engine-published.ok"
run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$NPLIST" \
        bash "$SCRIPT" app --tag "$TAG" --out "$STALE"
expect "R8 app with a receipt for a different digest refuses" 2 "rebuilt after it was verified"

run "${APP_ENV[@]}" RICHOS_NAMED_PERSONS_FILE="$NPLIST" \
        bash "$SCRIPT" app --tag "$TAG" --out "$OUTDIR"
expect "R9 app with a good receipt and no signing key refuses" 2 "no updater signing key"

EMPTY="$TMP/empty"; mkdir -p "$EMPTY"
run "${REL[@]}" verify-release --tag "$TAG" --out "$EMPTY"
expect "R10 verify-release with nothing staged refuses" 2 "no SHA256SUMS"

# The standalone artifact uses the sealed payload, not a second build. Exercise the real
# archiver and macOS signature verifier with a tiny ad-hoc signed app, never the user's app.
DOCTOR_HOST="$TMP/host/RichOS.app"
DOCTOR_DIR="$TMP/original/RichDoctor.app"
DOCTOR_OUT="$TMP/doctor-staging"
DOCTOR_PAYLOAD="$DOCTOR_HOST/Contents/Resources/RichDoctor/RichDoctor.app.tar.gz"
mkdir -p "$DOCTOR_DIR/Contents/MacOS" "$(dirname "$DOCTOR_PAYLOAD")" "$DOCTOR_OUT"
cp /usr/bin/true "$DOCTOR_DIR/Contents/MacOS/richdoctor"
python3 - "$DOCTOR_DIR/Contents/Info.plist" <<'PY'
import plistlib, sys
with open(sys.argv[1], 'wb') as out:
    plistlib.dump({'CFBundleIdentifier': 'com.richos.richdoctor', 'CFBundleExecutable': 'richdoctor',
                  'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': '1.0.0'}, out)
PY
codesign --force --sign - --timestamp=none "$DOCTOR_DIR" >/dev/null 2>&1 || exit 1
python3 "$SRC_DIR/lib/updater_tar.py" build "$DOCTOR_DIR" "$DOCTOR_PAYLOAD" >/dev/null || exit 1
stage_doctor_fixture() (
  local notarize="${1:-0}"
  export TMPDIR="$TMP/doctor-scratch/"
  mkdir -p "$TMPDIR"
  # `plan` is read-only and leaves the real staging functions available to this shell.
  set -- plan --out "$DOCTOR_OUT" --no-notarize
  # SCRIPT is the isolated copy of make-release.sh made above.
  # shellcheck disable=SC1090
  . "$SCRIPT" >/dev/null
  # Consumed by stage_richdoctor in that sourced release script.
  # shellcheck disable=SC2034
  NOTARIZE="$notarize"
  stage_richdoctor "$DOCTOR_HOST" || return $?
  write_sums
)
run stage_doctor_fixture
expect "R11 standalone Doctor archive is staged and checksummed" 0 "standalone download: RichDoctor-macos-"
doctor_zip="$(find "$DOCTOR_OUT" -name 'RichDoctor-macos-*.zip' -type f)"
if [ -n "$doctor_zip" ] \
   && /usr/bin/ditto -x -k "$doctor_zip" "$TMP/doctor-download" \
   && cmp -s "$DOCTOR_DIR/Contents/MacOS/richdoctor" "$TMP/doctor-download/RichDoctor.app/Contents/MacOS/richdoctor" \
   && codesign --verify --deep --strict "$TMP/doctor-download/RichDoctor.app" 2>/dev/null \
   && (cd "$DOCTOR_OUT" && /usr/bin/shasum -a 256 -c SHA256SUMS >/dev/null); then
  ok "R11 downloaded archive preserves the signed executable and has a valid checksum"
else
  bad "R11 downloaded archive preserves the signed executable and has a valid checksum"
fi
run stage_doctor_fixture 1
expect "R12 notarized release refuses an unstapled standalone Doctor" 1 "no valid stapled ticket"
rm "$DOCTOR_PAYLOAD"
run stage_doctor_fixture
expect "R13 missing bundled Doctor cannot silently omit the separate download" 1 "payload is missing"
printf 'damaged fixture\n' >> "$DOCTOR_DIR/Contents/MacOS/richdoctor"
python3 "$SRC_DIR/lib/updater_tar.py" build "$DOCTOR_DIR" "$DOCTOR_PAYLOAD" >/dev/null || exit 1
run stage_doctor_fixture
expect "R14 damaged Doctor signature cannot be published" 1 "fails signature verification"
if [ -z "$(find "$TMP/doctor-scratch" -mindepth 1 -print -quit)" ]; then
  ok "R15 standalone archive workspaces are removed on success and refusal"
else
  bad "R15 standalone archive workspaces are removed on success and refusal"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== make-release.test.sh: $FAIL FAILED, $PASS passed ==="
  exit 1
fi
echo "=== make-release.test.sh: all $PASS passed ==="

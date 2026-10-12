#!/usr/bin/env bash
#
# output-lock.test.sh — two packaging runs never write one output directory at once.
#
# Hunt part 2, section 21. Every developer checkout's `app/target` and `src-tauri/target` is
# a symlink into one shared cache (shared-build-cache.sh), so two checkouts packaging at the
# same moment named the same engine tarball, pin file and Tauri bundle. Cargo's lock covers
# the compile only; the packaging after it is shell. `lib/output-lock.sh` is the lock those
# steps now hold, and these cases are the ways it could fail to be one.
#
# Cases:
#   O1  a second hold on a held directory waits, and refuses past RICHOS_OUTPUT_LOCK_WAIT
#   O2  the same directory reached through a symlink (another checkout's shape) is one lock
#   O3  the lock is released when its holder exits, and the next run takes it
#   O4  a child of the holder is not blocked by its own caller's lock
#   O5  two different directories do not block each other
#   O6  make-engine-asset.sh writes nothing into an output directory another run holds
#   O7  ...and builds normally once that run is gone
#   O8  package-app.sh goes no further while another run holds its bundle output
#   O9  a caller holding the bundle output (make-release.sh app) does not block its own
#       package-app.sh
#
# run-tests: inputs richos/app/scripts/output-lock.test.sh richos/app/scripts/lib/output-lock.sh richos/app/scripts/make-engine-asset.sh richos/app/scripts/verify-engine-asset-members.sh richos/app/scripts/package-app.sh richos/app/scripts/make-release.sh richos/app/scripts/lib/cargo-target.sh LICENSE richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh
# run-tests: covers richos/app/scripts/lib/output-lock.sh
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
LOCK_LIB="$DIR/lib/output-lock.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/output-lock-test.XXXXXX")" || exit 2
HOLDERS=()
cleanup() {
    local pid
    for pid in "${HOLDERS[@]+"${HOLDERS[@]}"}"; do
        kill "$pid" 2>/dev/null
        wait "$pid" 2>/dev/null
    done
    rm -rf "$WORK"
}
trap cleanup EXIT

# A holder that is NOT this library: a plain flock from another process, which is what a
# second checkout's run is to this one. Returns once the lock is held. $2 is a file whose
# removal releases it.
hold_from_outside() {  # hold_from_outside <dir> <release-file>
    : > "$2"
    python3 - "$1" "$2" "$WORK/held.$$" <<'PY' &
import fcntl, os, sys, time
fd = os.open(sys.argv[1], os.O_RDONLY)
fcntl.flock(fd, fcntl.LOCK_EX)
open(sys.argv[3], "w").close()
while os.path.exists(sys.argv[2]):
    time.sleep(0.1)
PY
    HOLDERS+=("$!")
    for _ in $(seq 1 100); do
        [ -e "$WORK/held.$$" ] && { rm -f "$WORK/held.$$"; return 0; }
        sleep 0.1
    done
    return 1
}

# ---------------------------------------------------------------------------------------
# O1-O5: the library
# ---------------------------------------------------------------------------------------
[ -f "$LOCK_LIB" ] || { bad "O1-O5 lib/output-lock.sh exists" "no $LOCK_LIB"; }

# One shared cache directory, and a second checkout's symlink into it.
mkdir -p "$WORK/shared/engine-asset"
ln -s "$WORK/shared" "$WORK/checkout-b-target"

if [ -f "$LOCK_LIB" ]; then
    hold_from_outside "$WORK/shared/engine-asset" "$WORK/release-1" || bad "O1 fixture holder" "never took the lock"

    OUT="$(RICHOS_OUTPUT_LOCK_WAIT=1 bash -c '. "$1"; hold_output_lock "$2" 9 o1 && echo HELD' _ "$LOCK_LIB" "$WORK/shared/engine-asset" 2>&1)"; CODE=$?
    if [ "$CODE" -ne 0 ] && ! printf '%s' "$OUT" | grep -q HELD \
            && printf '%s' "$OUT" | grep -q 'waiting for it to finish' \
            && printf '%s' "$OUT" | grep -q 'REFUSING'; then
        ok "O1 a held directory makes a second run wait, then refuse at its bound"
    else
        bad "O1 a held directory makes a second run wait, then refuse at its bound" "exit $CODE: $OUT"
    fi

    OUT="$(RICHOS_OUTPUT_LOCK_WAIT=1 bash -c '. "$1"; hold_output_lock "$2" 9 o2 && echo HELD' _ "$LOCK_LIB" "$WORK/checkout-b-target/engine-asset" 2>&1)"; CODE=$?
    if [ "$CODE" -ne 0 ] && ! printf '%s' "$OUT" | grep -q HELD; then
        ok "O2 the same directory through another checkout's symlink is the same lock"
    else
        bad "O2 the same directory through another checkout's symlink is the same lock" "exit $CODE: $OUT"
    fi

    rm -f "$WORK/release-1"
    wait "${HOLDERS[0]}" 2>/dev/null
    OUT="$(RICHOS_OUTPUT_LOCK_WAIT=5 bash -c '. "$1"; hold_output_lock "$2" 9 o3 && echo HELD' _ "$LOCK_LIB" "$WORK/checkout-b-target/engine-asset" 2>&1)"; CODE=$?
    if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q HELD; then
        ok "O3 the lock is released when its holder exits"
    else
        bad "O3 the lock is released when its holder exits" "exit $CODE: $OUT"
    fi

    # The holder takes the lock, then runs a child that asks for the same directory. The
    # child must not wait on its own caller; an outside run still must.
    OUT="$(RICHOS_OUTPUT_LOCK_WAIT=1 bash -c '
        . "$1"
        hold_output_lock "$2" 8 parent || exit 3
        RICHOS_OUTPUT_LOCK_WAIT=1 bash -c ". \"\$1\"; hold_output_lock \"\$2\" 9 child && echo CHILD-HELD" _ "$1" "$2"
        RICHOS_OUTPUT_LOCKS= RICHOS_OUTPUT_LOCK_WAIT=1 bash -c ". \"\$1\"; hold_output_lock \"\$2\" 9 outsider && echo OUTSIDER-HELD" _ "$1" "$2"
        exit 0' _ "$LOCK_LIB" "$WORK/shared/engine-asset" 2>&1)"; CODE=$?
    if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q CHILD-HELD && ! printf '%s' "$OUT" | grep -q OUTSIDER-HELD; then
        ok "O4 a child of the holder is not blocked by its caller, and an outsider still is"
    else
        bad "O4 a child of the holder is not blocked by its caller, and an outsider still is" "exit $CODE: $OUT"
    fi

    mkdir -p "$WORK/other-checkout/engine-asset"
    hold_from_outside "$WORK/shared/engine-asset" "$WORK/release-2" || bad "O5 fixture holder" "never took the lock"
    OUT="$(RICHOS_OUTPUT_LOCK_WAIT=1 bash -c '. "$1"; hold_output_lock "$2" 9 o5 && echo HELD' _ "$LOCK_LIB" "$WORK/other-checkout/engine-asset" 2>&1)"; CODE=$?
    if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q HELD; then
        ok "O5 a different directory is not blocked"
    else
        bad "O5 a different directory is not blocked" "exit $CODE: $OUT"
    fi
    rm -f "$WORK/release-2"
fi

# ---------------------------------------------------------------------------------------
# O6-O7: make-engine-asset.sh, on a synthetic checkout (the real archive is ~30 s a build)
# ---------------------------------------------------------------------------------------
F="$WORK/engine-fixture"
mkdir -p "$F/richos/app/scripts/lib" "$F/docs/legal" "$F/richos/engine/scripts/hooks"
cp "$DIR/make-engine-asset.sh" "$DIR/verify-engine-asset-members.sh" "$F/richos/app/scripts/"
cp "$DIR/lib/no_host_paths.py" "$F/richos/app/scripts/lib/"
cp "$DIR/runtime-sources.json" "$F/richos/app/scripts/"
[ -f "$LOCK_LIB" ] && cp "$LOCK_LIB" "$F/richos/app/scripts/lib/"
cp "$ROOT/LICENSE" "$F/LICENSE"
printf 'notices for the fixture\n' > "$F/docs/legal/THIRD-PARTY-NOTICES.md"
printf '1.0.0\n' > "$F/richos/engine/VERSION"
printf '#!/usr/bin/env bash\ntrue\n' > "$F/richos/engine/scripts/hooks/guard-demo.sh"
env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE git -c init.defaultBranch=main init -q "$F" >/dev/null 2>&1
env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE git -C "$F" add -A >/dev/null 2>&1
env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE git -C "$F" -c user.name=fixture \
    -c user.email=fixture@example.invalid commit -q --no-gpg-sign -m fixture >/dev/null 2>&1

ASSET_OUT="$WORK/shared/asset-out"
mkdir -p "$ASSET_OUT"
hold_from_outside "$ASSET_OUT" "$WORK/release-3" || bad "O6 fixture holder" "never took the lock"
OUT="$(RICHOS_RUNTIME_DIR='' RICHOS_OUTPUT_LOCK_WAIT=2 bash "$F/richos/app/scripts/make-engine-asset.sh" \
    --out "$WORK/checkout-b-target/asset-out" 2>&1)"; CODE=$?
if [ "$CODE" -ne 0 ] && [ ! -e "$ASSET_OUT/richos-engine-1.0.0.tar.gz" ] && [ ! -e "$ASSET_OUT/engine-pin.env" ]; then
    ok "O6 make-engine-asset.sh writes nothing into an output directory another run holds"
else
    bad "O6 make-engine-asset.sh writes nothing into an output directory another run holds" \
        "exit $CODE; in $ASSET_OUT: $(find "$ASSET_OUT" -mindepth 1 -maxdepth 1 -exec basename {} \; | tr '\n' ' '); $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
fi
rm -f "$WORK/release-3"
wait "${HOLDERS[${#HOLDERS[@]}-1]}" 2>/dev/null
OUT="$(RICHOS_RUNTIME_DIR='' RICHOS_OUTPUT_LOCK_WAIT=5 bash "$F/richos/app/scripts/make-engine-asset.sh" \
    --out "$WORK/checkout-b-target/asset-out" 2>&1)"; CODE=$?
if [ "$CODE" -eq 0 ] && [ -s "$ASSET_OUT/richos-engine-1.0.0.tar.gz" ] && [ -s "$ASSET_OUT/engine-pin.env" ]; then
    ok "O7 make-engine-asset.sh builds normally once the other run is gone"
else
    bad "O7 make-engine-asset.sh builds normally once the other run is gone" "exit $CODE: $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
fi

# ---------------------------------------------------------------------------------------
# O8-O9: the Tauri bundle, stopped at the first refusal after the lock
# ---------------------------------------------------------------------------------------
# package-app.sh takes the bundle output's lock right after its prerequisites, before the
# signing mode is resolved. So `--sign developer-id` on a machine reporting no identity (a
# `security` shim, as package-app.test.sh B1 uses) is a refusal that can only be reached
# PAST the lock, and no case here compiles, downloads or signs anything. The cargo shim only
# answers the prerequisite `cargo tauri --version`. CARGO_TARGET_DIR is the shared directory
# both checkouts would resolve.
SHIM="$WORK/shim"; mkdir -p "$SHIM"
printf '#!/usr/bin/env bash\ncase "$*" in *--version*) echo "tauri-cli 2.11.4"; exit 0 ;; esac\nexit 9\n' > "$SHIM/cargo"
printf '#!/usr/bin/env bash\necho "     0 valid identities found"\n' > "$SHIM/security"
chmod +x "$SHIM/cargo" "$SHIM/security"
PAST_THE_LOCK='install-signing-cert.sh'
TARGET="$WORK/shared-cargo-target"
. "$DIR/lib/cargo-target.sh"
PRIVATE_TARGET="$(CARGO_TARGET_DIR="$TARGET" cargo_target_dir "$DIR/../src-tauri")"
mkdir -p "$PRIVATE_TARGET/release"
hold_from_outside "$PRIVATE_TARGET/release" "$WORK/release-4" || bad "O8 fixture holder" "never took the lock"
OUT="$(PATH="$SHIM:$PATH" CARGO_TARGET_DIR="$TARGET" RICHOS_OUTPUT_LOCK_WAIT=2 \
    bash "$DIR/package-app.sh" --sign developer-id 2>&1)"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'another packaging run' \
        && ! printf '%s' "$OUT" | grep -q "$PAST_THE_LOCK"; then
    ok "O8 package-app.sh goes no further while another run holds its bundle output"
else
    bad "O8 package-app.sh goes no further while another run holds its bundle output" \
        "exit $CODE: $(printf '%s' "$OUT" | grep -v '^ *$' | tail -3 | tr '\n' ' ')"
fi
rm -f "$WORK/release-4"
wait "${HOLDERS[${#HOLDERS[@]}-1]}" 2>/dev/null

# make-release.sh app takes the bundle output first, and holds it across package-app.sh and
# its own use of the bundle afterwards. package-app.sh, its child, must go past the lock
# rather than wait on its own caller.
OUT="$(PATH="$SHIM:$PATH" CARGO_TARGET_DIR="$TARGET" RICHOS_OUTPUT_LOCK_WAIT=2 \
    bash -c '. "$1"; hold_output_lock "$2" 8 "make-release.sh app" || exit 3; bash "$3" --sign developer-id 2>&1; exit 0' \
    _ "$LOCK_LIB" "$PRIVATE_TARGET/release" "$DIR/package-app.sh" 2>&1)"; CODE=$?
if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q "$PAST_THE_LOCK" \
        && ! printf '%s' "$OUT" | grep -q 'another packaging run'; then
    ok "O9 a caller's hold on the bundle output does not block its own package-app.sh"
else
    bad "O9 a caller's hold on the bundle output does not block its own package-app.sh" \
        "exit $CODE: $(printf '%s' "$OUT" | grep -v '^ *$' | tail -3 | tr '\n' ' ')"
fi

# ---------------------------------------------------------------------------------------
# O10-O11: updater-e2e.sh reads the bundle output AFTER package-app.sh returns (recheck R21)
# ---------------------------------------------------------------------------------------
# updater-e2e.sh copies the archive, signature and manifest out of the shared bundle directory
# once package-app.sh has exited, and package-app.sh's own hold ended with it. So the test
# takes the lock itself, as make-release.sh does. The sandbox is the real updater-e2e.sh beside
# a package-app.sh that only records whether its caller held the bundle output, then fails the
# build, so no case compiles, signs or installs anything.
SB="$WORK/updater-sandbox"; mkdir -p "$SB/scripts" "$SB/src-tauri"
cp "$DIR/updater-e2e.sh" "$SB/scripts/updater-e2e.sh"
ln -s "$DIR/lib" "$SB/scripts/lib"
printf 'fixture key\n' > "$SB/key"
# The cargo wrapper keys the private output on the Cargo workspace, so the sandbox needs one.
printf '[package]\nname = "sandbox"\nversion = "0.0.0"\n' > "$SB/src-tauri/Cargo.toml"
cat > "$SB/scripts/package-app.sh" <<EOF
#!/usr/bin/env bash
printf 'called\n' >> "$SB/called"
printf '%s\n' "\${RICHOS_OUTPUT_LOCKS:-}" >> "$SB/locks-seen"
exit 1
EOF
UPD_TARGET="$WORK/updater-cargo-target"
UPD_PRIVATE="$(CARGO_TARGET_DIR="$UPD_TARGET" cargo_target_dir "$SB/src-tauri")"
mkdir -p "$UPD_PRIVATE/release"
UPD_PHYSICAL="$(cd "$UPD_PRIVATE/release" && pwd -P)"
run_updater() {
    PATH="$SHIM:$PATH" CARGO_TARGET_DIR="$UPD_TARGET" TAURI_SIGNING_PRIVATE_KEY_PATH="$SB/key" \
        RICHOS_OUTPUT_LOCK_WAIT=2 bash "$SB/scripts/updater-e2e.sh" 2>&1
}

hold_from_outside "$UPD_PRIVATE/release" "$WORK/release-5" || bad "O10 fixture holder" "never took the lock"
OUT="$(run_updater)"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'another packaging run' && [ ! -e "$SB/called" ]; then
    ok "O10 updater-e2e.sh builds nothing while another run holds the bundle output"
else
    bad "O10 updater-e2e.sh builds nothing while another run holds the bundle output" \
        "exit $CODE, package-app.sh called: $([ -e "$SB/called" ] && echo yes || echo no); $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
fi
rm -f "$WORK/release-5"
wait "${HOLDERS[${#HOLDERS[@]}-1]}" 2>/dev/null

rm -f "$SB/called" "$SB/locks-seen"
OUT="$(run_updater)"; CODE=$?
if [ -e "$SB/called" ] && grep -qF "$UPD_PHYSICAL" "$SB/locks-seen" 2>/dev/null; then
    ok "O11 updater-e2e.sh holds the bundle output across its package-app.sh builds"
else
    bad "O11 updater-e2e.sh holds the bundle output across its package-app.sh builds" \
        "package-app.sh saw RICHOS_OUTPUT_LOCKS='$(cat "$SB/locks-seen" 2>/dev/null | tr '\n' ' ')', wanted $UPD_PHYSICAL"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== output-lock tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== output-lock tests: all $PASS passed ==="

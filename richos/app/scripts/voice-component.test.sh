#!/usr/bin/env bash
# Source and consumer integration for engine/voice, including Rust's compiled metadata.
# run-tests: inputs richos/app/scripts/voice-component.test.sh richos/engine/voice richos/engine/.env.example richos/engine/scripts/lib/machine_settings.fixtures.json richos/app/crates/richos-voice richos/app/crates/richos-core richos/app/crates/richos-command richos/engine/scripts/lib/dialect/dialect-en-US.generated.dict richos/app/Cargo.toml richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/cargo-target.sh
# run-tests: covers richos/app/crates/richos-voice/build.rs
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VOICE="$HERE/../../engine/voice"
node "$VOICE/tests/consumers.test.mjs"
node "$VOICE/tests/relocation.test.mjs"
node "$VOICE/tests/mutation-runner.test.mjs"

# A source build needs the model subtree, but no service or HUD source.
APP="$(cd "$HERE/.." && pwd)"
SCRATCH="$(mktemp -d "${TMPDIR:?}/voice-build.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
PRODUCT="$SCRATCH/product with spaces"
mkdir -p "$PRODUCT/app/crates" "$PRODUCT/engine/voice"
cp "$APP/Cargo.toml" "$APP/Cargo.lock" "$PRODUCT/app/"
# Every member the workspace manifest lists must exist in the copy, or Cargo refuses
# the whole workspace before building anything (richos-command joined with the command rule).
cp -R "$APP/crates/richos-core" "$APP/crates/richos-voice" "$APP/crates/richos-command" "$PRODUCT/app/crates/"
cp -R "$VOICE/models" "$PRODUCT/engine/voice/"
# richos-core's library embeds the American-spelling table from the engine
# (american_spelling.rs, since 8a11f174), so the copied product carries it too.
DICT="engine/scripts/lib/dialect/dialect-en-US.generated.dict"
mkdir -p "$PRODUCT/$(dirname "$DICT")"
cp "$HERE/../../$DICT" "$PRODUCT/$DICT"
for SETTINGS_INPUT in engine/.env.example engine/scripts/lib/machine_settings.fixtures.json; do
  mkdir -p "$PRODUCT/$(dirname "$SETTINGS_INPUT")"
  cp "$HERE/../../$SETTINGS_INPUT" "$PRODUCT/$SETTINGS_INPUT"
done
CARGO_BIN="$HERE/bin/cargo"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$APP/target}"
"$CARGO_BIN" run --quiet --locked --manifest-path "$APP/Cargo.toml" -p richos-voice --example model_metadata > "$SCRATCH/original.json"
"$CARGO_BIN" run --quiet --locked --manifest-path "$PRODUCT/app/Cargo.toml" -p richos-voice --example model_metadata > "$SCRATCH/copied.json"
cmp "$SCRATCH/original.json" "$SCRATCH/copied.json"
echo '  PASS  Rust source build without tools embeds the same metadata'
rm "$PRODUCT/engine/voice/models/model-pins.json"
if "$CARGO_BIN" check --quiet --locked --manifest-path "$PRODUCT/app/Cargo.toml" -p richos-voice > "$SCRATCH/missing.log" 2>&1; then
    echo '  FAIL  Rust build accepted missing canonical model data' >&2
    exit 1
fi
grep -q 'model-pins.json' "$SCRATCH/missing.log"
echo '  PASS  Rust rebuild refuses missing canonical model data'

#!/usr/bin/env bash
# RichDoctor's own suite (the separate support app, crates/richdoctor and crates/richdoctor-core).
# The core crate's unit and integration tests are selected per file by proof-for.sh; this suite
# adds what no per-file rule reaches:
#   1. the core crate's examples: the live adapter probe's own unit test (its live run is
#      `cargo run --example adapter_probe`, measured in crates/richdoctor-core/MEASURED.md, and
#      never run here: it uses the person's Claude Code account), and that the committed PNG and
#      JPEG screenshot fixtures are byte for byte what the encoder writes today;
#   2. the app crate's unit tests (single instance, the menu bar picture, the engine), and its
#      delivery over the real HTTPS client against the loopback relay stand-in
#      (examples/relay_probe.rs drives scripts/testvm/report-relay-stand-in.py);
#   3. the bundle's declared identity: identifier, macOS 13 floor, menu bar only, the one
#      capability, the icon set and its generator, the generated faces and tokens;
#   4. the window itself, walked in WebKit with its own contrast probe in both themes
#      (scripts/richdoctor-ui-walk.js);
#   5. the guest walk script parses (scripts/testvm/richdoctor-walk.sh runs only inside a test-VM
#      guest, through run-walk.py; here it is syntax-checked, never run).
# run-tests: no-host-screen: it builds, runs unit tests, a loopback stand-in and an offscreen WebKit; it launches no app
# run-tests: inputs richos/app/scripts/richdoctor.test.sh richos/app/scripts/richdoctor-ui-walk.js richos/app/scripts/testvm/report-relay-stand-in.py richos/app/scripts/testvm/richdoctor-walk.sh richos/app/crates/richdoctor-core richos/app/crates/richdoctor richos/app/crates/richos-core/src/bug_report.rs richos/app/ui/style.css richos/app/ui/fonts richos/app/ui/tests/lib/contrast.js richos/mobile/service/report/fixtures/client
# run-tests: covers richos/app/scripts/richdoctor-ui-walk.js richos/app/crates/richdoctor-core/examples/adapter_probe.rs richos/app/crates/richdoctor-core/examples/profile_fixtures.rs richos/app/crates/richdoctor/src/main.rs richos/app/crates/richdoctor/src/mac.rs richos/app/crates/richdoctor/src/relay.rs richos/app/crates/richdoctor/examples/relay_probe.rs richos/app/crates/richdoctor/build.rs richos/app/crates/richdoctor/tauri.conf.json richos/app/crates/richdoctor/Info.plist richos/app/crates/richdoctor/capabilities/default.json richos/app/crates/richdoctor/icons/generate.sh richos/app/crates/richdoctor/ui/index.html richos/app/crates/richdoctor/ui/doctor.css richos/app/crates/richdoctor/ui/doctor.js richos/app/scripts/testvm/report-relay-stand-in.py richos/app/scripts/testvm/richdoctor-walk.sh
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"
rd="$app/crates/richdoctor"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "== 1. the core crate's examples (their own unit tests), and the shared screenshot fixtures =="
cargo test --offline --manifest-path "$app/crates/richdoctor-core/Cargo.toml" --examples
mkdir -p "$work/fixtures"
cargo run -q --offline --manifest-path "$app/crates/richdoctor-core/Cargo.toml" --example profile_fixtures -- "$work/fixtures"
for f in accept-final.png accept-final.jpg reject-text-chunk.png reject-exif-and-comment.jpg; do
  cmp "$work/fixtures/$f" "$app/../mobile/service/report/fixtures/client/$f"
done
echo "the four committed fixtures are what the encoder writes today: ok"

echo "== 2. the app crate (builds the binary, main.rs, mac.rs and relay.rs with it) =="
cargo test --offline --manifest-path "$rd/Cargo.toml" --lib
cargo build --offline --manifest-path "$rd/Cargo.toml" --bin RichDoctor
cargo run -q --offline --manifest-path "$rd/Cargo.toml" --example relay_probe -- "$work/relay"

echo "== 3. the bundle's declared identity =="
python3 - "$rd" <<'PY'
import json, pathlib, plistlib, sys
rd = pathlib.Path(sys.argv[1])
conf = json.loads((rd / "tauri.conf.json").read_text())
assert conf["identifier"] == "com.richos.richdoctor", conf["identifier"]
assert conf["productName"] == "RichDoctor"
assert conf["bundle"]["macOS"]["minimumSystemVersion"] == "13.0"
assert conf["build"]["frontendDist"] == "ui"
assert conf["app"]["windows"] == [], "the window is made in code, never at launch (a quiet login launch shows none)"
plist = plistlib.loads((rd / "Info.plist").read_bytes())
assert plist.get("LSUIElement") is True, "menu bar only"
assert not [k for k in plist if k.startswith("NS") and k.endswith("UsageDescription")], "no permission prompt"
cap = json.loads((rd / "capabilities" / "default.json").read_text())
assert cap["permissions"] == ["core:default"] and cap["windows"] == ["main"], "the page gets no file or dialog command"
for icon in conf["bundle"]["icon"]:
    assert (rd / icon).is_file(), icon
for f in ("tokens.css", "fonts/fonts.css", "fonts/Inter-Variable.woff2"):
    assert (rd / "ui" / f).is_file(), f"build.rs did not generate {f}"
tokens = (rd / "ui" / "tokens.css").read_text()
assert ":root {" in tokens and ':root[data-theme="light"] {' in tokens and "--gold:" in tokens
print("identity, floor, menu bar, capability, icons and generated files: ok")
PY
mkdir -p "$work/hq/design/mockups/rounds/round-38/v3/menubar" "$work/icons"
sips -s format png -z 1024 1024 "$rd/icons/128x128@2x.png" --out "$work/hq/design/mockups/rounds/round-38/v3/richdoctor-icon-1024.png" >/dev/null
cp "$rd/icons/menubar@2x.png" "$work/hq/design/mockups/rounds/round-38/v3/menubar/richdoctor-menubar@2x.png"
cp "$rd/icons/generate.sh" "$work/icons/generate.sh"
bash "$work/icons/generate.sh" "$work/hq" >/dev/null
for f in icon.icns 32x32.png 128x128.png 128x128@2x.png menubar@2x.png; do
  [ -s "$work/icons/$f" ] || { echo "generate.sh did not write $f" >&2; exit 1; }
done
[ "$(sips -g pixelWidth "$work/icons/128x128@2x.png" | awk '/pixelWidth/{print $2}')" = 256 ] || { echo "generate.sh: wrong @2x size" >&2; exit 1; }
echo "icons/generate.sh writes the five icons at their sizes: ok"

echo "== 4. the window, walked in WebKit =="
node "$app/scripts/richdoctor-ui-walk.js"

echo "== 5. the guest walk script parses =="
bash -n "$app/scripts/testvm/richdoctor-walk.sh"
echo "richdoctor.test.sh: passed"

#!/usr/bin/env bash
# Actual watcher owner, filesystem safety, event adapter and support producer/collector.
# run-tests: no-host-screen: Rust filesystem tests and plist inspection only; no app launch or host permission change
# run-tests: inputs richos/tools/richpass/RichPass.html richos/tools/richpass/releases.json richos/app/crates/richos-core/src/richpass.rs richos/app/src-tauri/src/richpass_host.rs richos/app/crates/richdoctor-core/src/richpass_log.rs richos/app/crates/richdoctor-core/src/collect.rs richos/app/src-tauri/Info.plist
# run-tests: covers richos/app/src-tauri/Info.plist
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$here/lib/cargo-cache-env.sh"
cargo test --manifest-path "$here/../Cargo.toml" --locked -p richos-core --lib richpass::
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --locked --bin richos-tauri richpass_host::
cargo test --manifest-path "$here/../crates/richdoctor-core/Cargo.toml" --locked --lib richpass
python3 - "$here/../src-tauri/Info.plist" <<'PY'
import plistlib, sys
with open(sys.argv[1], 'rb') as f:
    info = plistlib.load(f)
for folder in ('DesktopFolder', 'DocumentsFolder', 'DownloadsFolder', 'RemovableVolumes'):
    assert 'RichPass.html' in info['NS' + folder + 'UsageDescription']
print('PASS RichPass folder consent descriptions in source plist; signed bundle/TCC behavior remains a release gate')
PY

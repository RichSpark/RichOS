#!/usr/bin/env python3
"""Build the macOS capture companion for the app bundle. Print a Tauri overlay.

Granola PRD V4: "Build the Swift companion in the Mac release process and sign it inside the app
bundle." The meeting watcher and the capture run as the app's child (`src-tauri/src/meetings.rs`
looks for `Contents/MacOS/richos-companion`), so macOS attributes their microphone and system
audio use to RichOS and its one permission grant.

The same shape as `prepare-connect-helper.py`: the binary is built outside the checkout (the
machine's cache, keyed by a hash of the package's sources, so an unchanged companion is never
rebuilt), `src-tauri/companion-helper` links to it, and the overlay names it as an `externalBin`
for `cargo tauri build` only. A development build of the shell never needs it, and the app runs
without meeting notes when it is absent. `package-app.sh` signs it inside the bundle, before the
bundle, with the app's own entitlements (each process that opens the microphone under the
hardened runtime is checked for `com.apple.security.device.audio-input`).

  prepare-companion.py            build if needed, link, print {"bundle": {"externalBin": [...]}}
  prepare-companion.py --check    print the source key and whether a build is cached; build nothing
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import machine_storage as M

HERE = Path(__file__).resolve().parent
APP = HERE.parent
PACKAGE = APP.parent / "tools/richos-service/companion-macos"
NATIVE_WORK = APP.parent / "engine/scripts/lib/native-work.py"
TRIPLES = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}


def source_key():
    """sha256 over the package's build inputs (Package.swift and every file under Sources/),
    path and bytes, in a fixed order. Tests are not a build input."""
    digest = hashlib.sha256()
    files = [PACKAGE / "Package.swift"] + sorted(p for p in (PACKAGE / "Sources").rglob("*") if p.is_file())
    for f in files:
        digest.update(str(f.relative_to(PACKAGE)).encode() + b"\0" + f.read_bytes() + b"\0")
    return digest.hexdigest()[:16]


def prepare(directory, triple):
    target = directory / ("richos-companion-" + triple)
    if target.is_file() and os.access(target, os.X_OK):
        return target
    scratch = directory / "build"
    subprocess.run([sys.executable, str(NATIVE_WORK), "--", "swift", "build", "-c", "release",
                    "--package-path", str(PACKAGE), "--scratch-path", str(scratch)],
                   check=True, stdout=sys.stderr, stderr=sys.stderr)
    built = scratch / "release" / "richos-companion"
    if not built.is_file():
        raise SystemExit(f"the companion did not build: no {built}")
    pending = target.with_suffix(".pending")
    shutil.copyfile(built, pending)
    pending.chmod(0o755)
    pending.replace(target)
    shutil.rmtree(scratch, ignore_errors=True)
    return target


def link(link_path, directory, root):
    if link_path.is_symlink():
        if not link_path.resolve().is_relative_to(root):
            raise SystemExit("Refusing to replace an unrelated companion link")
        link_path.unlink()
    elif link_path.exists():
        raise SystemExit("Refusing to replace an existing companion directory")
    link_path.symlink_to(directory, target_is_directory=True)


if __name__ == "__main__":
    if platform.system() != "Darwin" or platform.machine() not in TRIPLES:
        raise SystemExit("The capture companion is macOS only")
    triple = TRIPLES[platform.machine()]
    key = source_key()
    root = M.cache("richos-companion/bundle")
    directory = root / key
    if sys.argv[1:] == ["--check"]:
        print(json.dumps({"key": key, "cached": (directory / ("richos-companion-" + triple)).is_file()}))
        raise SystemExit(0)
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / "prepare.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        prepare(directory, triple)
    link(APP / "src-tauri/companion-helper", directory, root)
    # A relative source link keeps build-machine paths out of the executable (as the connect
    # helper's does); the bytes stay on the machine's cache volume.
    print(json.dumps({"bundle": {"externalBin": ["companion-helper/richos-companion"]}}))

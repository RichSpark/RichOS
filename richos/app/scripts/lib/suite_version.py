#!/usr/bin/env python3
"""Resolve the macOS suite version and verify the stamp on shipped bundles."""

import argparse
import json
from pathlib import Path
import plistlib
import re
import sys
import tarfile


BASE = r"[0-9]+\.[0-9]+\.[0-9]+"
NIGHTLY = re.compile(rf"({BASE})-nightly\.([0-9]{{8}}\.[1-9][0-9]*)")


def resolve(version, provenance=None):
    nightly = NIGHTLY.fullmatch(version)
    if nightly:
        return nightly.groups()
    base = re.split(r"[-+]", version, maxsplit=1)[0]
    if not re.fullmatch(BASE, base) or "-nightly." in version:
        raise ValueError(f"invalid suite version {version!r}")
    if provenance is None or not provenance.exists():
        # Development and direct test builds have no published nightly number.
        return base, "0"
    info = json.loads(provenance.read_text())
    if not isinstance(info, dict):
        raise ValueError(f"{provenance} must contain a build provenance object")
    promoted = info.get("promoted_from_version", "")
    nightly = NIGHTLY.fullmatch(promoted) if isinstance(promoted, str) else None
    if (info.get("channel") != "stable" or info.get("version") != version or version != base
            or nightly is None or nightly[1] != base or info.get("promoted_from") != f"v{promoted}"):
        raise ValueError(f"{provenance} does not name the promoted nightly for {version}")
    return nightly.groups()


def check(info, name, version, build, suite_version=None):
    expected = {"CFBundleShortVersionString": version, "CFBundleVersion": build}
    if suite_version is not None:
        expected["RichOSSuiteVersion"] = suite_version
    failures = [f"{name}: {key} is {info.get(key)!r}, expected {value!r}"
                for key, value in expected.items() if info.get(key) != value]
    for failure in failures:
        print(failure, file=sys.stderr)
    return not failures


def verify(app, version, base, build, with_doctor=True):
    files = app / "Contents/Applications/RichFiles.app"
    glance = files / "Contents/Applications/RichGlance.app"
    companions = {
        files,
        glance,
        glance / "Contents/Library/LoginItems/RichGlance Capture.app",
        files / "Contents/Library/LoginItems/RichFiles Window Gestures.app",
    }
    companions.update(files.parent.rglob("*.app"))
    results = [check(plistlib.loads((app / "Contents/Info.plist").read_bytes()), app.name, version, build)]
    for bundle in sorted(companions):
        info = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
        results.append(check(info, bundle.name, base, build, version))
    if with_doctor:
        payload = app / "Contents/Resources/RichDoctor"
        with tarfile.open(payload / "RichDoctor.app.tar.gz") as archive:
            with archive.extractfile("RichDoctor.app/Contents/Info.plist") as plist:
                results.append(check(plistlib.load(plist), "RichDoctor.app", version, build))
        if (payload / "version").read_text().strip() != version:
            print(f"RichDoctor payload version does not match {version}", file=sys.stderr)
            results.append(False)
    if all(results):
        print(f"Verified suite bundle versions: {base} ({build}), update identity {version}")
        return 0
    return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    resolution = commands.add_parser("resolve")
    resolution.add_argument("version")
    resolution.add_argument("provenance", nargs="?", type=Path)
    verification = commands.add_parser("verify")
    verification.add_argument("app", type=Path)
    verification.add_argument("version")
    verification.add_argument("base")
    verification.add_argument("build")
    verification.add_argument("--without-doctor", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "resolve":
            print(*resolve(args.version, args.provenance))
            return 0
        return verify(args.app, args.version, args.base, args.build, not args.without_doctor)
    except (OSError, ValueError, KeyError, tarfile.TarError, plistlib.InvalidFileException) as error:
        parser.exit(1, f"suite version: {error}\n")


if __name__ == "__main__":
    sys.exit(main())

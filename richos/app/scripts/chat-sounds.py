#!/usr/bin/env python3
"""Stage the approved notification media without tracking it in public source."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

FILES = (
    "BRRR-Plop-pass-sound-1-second.mp3",
    "perforated-paper-towel-tear-sound-effect-2-slow-1.0-seconds.mp3",
    "satisfying-double-pop.mp3",
    "satisfying-stamp-sound.mp3",
    "satisfying-triple-bubble-pop.mp3",
)


def source():
    explicit = os.environ.get("RICHOS_CHAT_SOUNDS")
    if explicit:
        return Path(explicit).resolve()
    checkout = Path(__file__).resolve().parent
    result = subprocess.run(
        ["git", "-C", str(checkout), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        text=True, capture_output=True, check=False,
    )
    if result.returncode == 0:
        sibling = Path(result.stdout.strip()).parent.parent / "richos-hq" / "design" / "sounds"
        if sibling.is_dir():
            return sibling
    return checkout.parents[3] / "richos-hq" / "design" / "sounds"


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--overlay", action="store_true")
    mode.add_argument("--copy", type=Path, metavar="RESOURCES")
    parser.add_argument("--required", action="store_true")
    args = parser.parse_args()
    folder = source()
    if any(not (folder / name).is_file() or not (folder / name).stat().st_size for name in FILES):
        print("Chat sounds are unavailable. Set RICHOS_CHAT_SOUNDS to the approved five-file directory.", file=sys.stderr)
        if args.required or os.environ.get("RICHOS_CHAT_SOUNDS"):
            return 1
        if args.overlay:
            print("{}")
        return 0
    if args.overlay:
        print(json.dumps({"bundle": {"resources": {
            str(folder / name): f"ChatSounds/{name}" for name in FILES
        }}}))
    else:
        target = args.copy / "ChatSounds"
        target.mkdir(parents=True, exist_ok=True)
        for name in FILES:
            shutil.copyfile(folder / name, target / name)
    return 0


if __name__ == "__main__":
    sys.exit(main())

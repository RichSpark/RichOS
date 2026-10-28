#!/usr/bin/env python3
"""Summarize Cargo output without counting doc tests as ordinary test passes.

This inspects a captured log; it never claims the cargo process exited zero.
Use with a successful, complete cargo invocation and retain its exit status."""
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from devtools_bridge import request

def summarize(text):
    return request("cargo-summary", text)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("file", type=Path)
    args = parser.parse_args()
    try:
        result = summarize(args.file.read_text())
    except (ValueError, OSError) as exc:
        parser.exit(2, str(exc) + "\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(1 if any(result[k]["failed"] for k in ("ordinary", "documentation")) else 0)

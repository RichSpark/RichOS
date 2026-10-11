#!/usr/bin/env python3
"""Verify a prepared runtime inventory against the tracked public-source recipe."""
import argparse
import hashlib
import json
import os
from pathlib import Path


def recipe_mismatch(root, built, recipe, sources):
    """The refusal for a runtime built from another recipe: which sources differ, and the
    directory name a runtime built from THIS recipe has (`lib/runtime_cache.py` keys it to the
    recipe's SHA-256), so the log alone says what to point at (nightly 51, 2026-10-11)."""
    old = built.get("sources") if isinstance(built, dict) else None
    new = recipe.get("sources") if isinstance(recipe, dict) else None
    if isinstance(old, dict) and isinstance(new, dict):
        parts = [f"{label}: {', '.join(sorted(names))}" for label, names in (
            ("missing", new.keys() - old.keys()),
            ("extra", old.keys() - new.keys()),
            ("changed", {name for name in old.keys() & new.keys() if old[name] != new[name]}),
        ) if names]
        if {k: v for k, v in built.items() if k != "sources"} != {k: v for k, v in recipe.items() if k != "sources"}:
            parts.append("header fields differ")
        detail = "; ".join(parts)
    else:
        detail = "recipe layout differs"
    keyed = "runtime-" + hashlib.sha256(sources.read_bytes()).hexdigest()[:12]
    return (f"runtime sources do not match the tracked public recipe: {root} was built from "
            f"another recipe ({detail}); the runtime for this recipe is {keyed} in the nightly "
            f"state directory, which nightly-local.py builds and uses when --runtime-dir is not given")


def verify(root, sources):
    root = root.resolve(strict=True)
    identity = json.loads((root / "delivery.json").read_text())
    if identity.get("schema") != 1 or identity.get("platform") != "aarch64-apple-darwin":
        raise ValueError("unsupported runtime inventory")
    built, recipe = json.loads((root / "runtime-sources.json").read_text()), json.loads(sources.read_text())
    if built != recipe:
        raise ValueError(recipe_mismatch(root, built, recipe, sources))
    for name in [*identity["files"], *identity["links"]]:
        if not name or "\n" in name or Path(name).is_absolute() or any(part in ("..", ".") for part in name.split("/")):
            raise ValueError("invalid runtime member name")
    if set(identity["files"]) & set(identity["links"]):
        raise ValueError("duplicate runtime member")
    expected = set(identity["files"]) | set(identity["links"]) | {"delivery.json"}
    actual = {str(p.relative_to(root)) for p in root.rglob("*") if not p.is_dir() or p.is_symlink()}
    if expected != actual:
        raise ValueError(f"runtime inventory mismatch: {len(actual - expected)} unexpected and {len(expected - actual)} missing files")
    for name, wanted in identity["files"].items():
        path = root / name
        if not path.resolve(strict=True).is_relative_to(root) or path.is_symlink() or not path.is_file():
            raise ValueError(f"invalid runtime file: {name}")
        with path.open("rb") as handle:
            got = hashlib.file_digest(handle, "sha256").hexdigest()
        if got != wanted:
            raise ValueError(f"runtime content changed: {name}")
    for name, target in identity["links"].items():
        path = root / name
        if not path.is_symlink() or os.readlink(path) != target or not path.resolve(strict=True).is_relative_to(root):
            raise ValueError(f"invalid runtime link: {name}")
    for name in ("python3", "node", "git", "jq", "whisper-cli", "ffmpeg", "ffprobe"):
        if not os.access(root / "bin" / name, os.X_OK):
            raise ValueError(f"runtime is not executable: {name}")
    return identity


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("sources", type=Path)
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    inventory = verify(args.root, args.sources)
    if args.list:
        for name in sorted(set(inventory["files"]) | set(inventory["links"]) | {"delivery.json"}):
            if "\n" in name or name.startswith("/") or ".." in Path(name).parts:
                raise ValueError("invalid runtime member name")
            print("runtime/" + name)
    else:
        print(json.dumps({"verified": True, "versions": inventory["versions"]}))

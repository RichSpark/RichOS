# SPDX-License-Identifier: AGPL-3.0-only
"""Read verified nightly facts from the same channel the app updates from."""

import json
import re
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen


REPO = "RichSpark/RichOS"
ASSETS = f"https://github.com/{REPO}/releases/download"
API = f"https://api.github.com/repos/{REPO}"
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-nightly\.([0-9]{8})\.([1-9][0-9]*)")


def version_key(value):
    match = VERSION.fullmatch(value.removeprefix("v"))
    if not match:
        raise ValueError("invalid nightly release version")
    return tuple(map(int, match.groups()))


def matches(reference, version):
    if reference.isdecimal():
        return int(reference) == version_key(version)[-1]
    return version_key(reference) == version_key(version)


def later(version, reference):
    if reference.isdecimal():
        return version_key(version)[-1] > int(reference)
    return version_key(version) > version_key(reference)


def read(url):
    request = Request(url, headers={"User-Agent": "RichOS", "Accept": "application/json"})
    try:
        with urlopen(request, timeout=5) as response:
            raw = response.read(1024 * 1024 + 1)
    except HTTPError as error:
        if error.code == 404:
            return None
        raise
    if len(raw) > 1024 * 1024:
        raise ValueError("nightly evidence is too large")
    return json.loads(raw)


def served_version(fetch=read):
    url = f"{ASSETS}/nightly/latest.json"
    return manifest_version(fetch(url))


def manifest_version(manifest):
    if not isinstance(manifest, dict) or not isinstance(manifest.get("version"), str):
        raise ValueError("no valid nightly manifest")
    version = manifest["version"]
    version_key(version)
    platforms = manifest.get("platforms")
    if (not isinstance(platforms, dict) or not platforms or any(
            not isinstance(asset, dict) or not asset.get("signature")
            or asset.get("url") != f"{ASSETS}/v{version}/RichOS.app.tar.gz"
            for asset in platforms.values())):
        raise ValueError("nightly manifest does not identify the released asset")
    return version


def publication_fact(condition, served, fetch=read):
    """Return ready, skipped or waiting and the owner evidence. Failures stay unknown."""
    reference = condition["release"]
    if condition["match"] == "later":
        return {"state": "ready", "publication": served} if later(served, reference) else {"state": "waiting"}
    if matches(reference, served):
        return {"state": "ready", "publication": served}
    if not later(served, reference):
        return {"state": "waiting"}

    # Every channel move is a single-parent Publish commit. A move can precede
    # serving, so its presence alone cannot release a waiting obligation.
    history = fetch(f"{API}/commits?sha=nightly&per_page=100")
    if not isinstance(history, list) or not history:
        raise ValueError("nightly channel history is unavailable")
    reached_served = False
    for entry in history:
        title = entry["commit"]["message"].splitlines()[0]
        if not title.startswith("Publish v"):
            raise ValueError("unrecognized nightly channel history")
        version = title.removeprefix("Publish v")
        version_key(version)
        if version == served:
            reached_served = True
        if not reached_served:
            continue
        if matches(reference, version):
            proof = fetch(f"{ASSETS}/v{version}/published.json")
            if proof is not None and manifest_version(proof) == version:
                return {"state": "ready", "publication": version}
            return {"state": "waiting", "reason": "The release took the channel but proof it was served is unavailable."}
        if not later(version, reference):
            return {"state": "skipped", "publication": served,
                    "reason": "The channel advanced without publishing the named release."}
    return {"state": "waiting", "reason": "The available channel history does not settle the named release."}


if __name__ == "__main__":
    # The terminal backup uses the installed engine's verifier without opening app state.
    request = json.loads(sys.stdin.buffer.read(4096))
    served = served_version()
    result = {"current": served}
    if request:
        if set(request) != {"match", "release"} or request["match"] not in {"exact", "later"}:
            raise ValueError("invalid publication query")
        result.update(publication_fact(request, served))
    print(json.dumps(result))

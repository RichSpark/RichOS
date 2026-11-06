#!/usr/bin/env bash
# run-tests: no-host-screen: Python temporary-file fixtures only
# run-tests: inputs richos/app/scripts/search-resources.test.sh richos/app/scripts/build-search-resources.py
# run-tests: covers richos/app/scripts/build-search-resources.py
set -euo pipefail
PYTHONDONTWRITEBYTECODE=1 python3 - "$(dirname "${BASH_SOURCE[0]}")/build-search-resources.py" <<'PY'
import json
from pathlib import Path
import runpy
import sys
import tempfile

builder = runpy.run_path(sys.argv[1])
model = builder["MODEL"]
pins = builder["PINS"]
digest = builder["digest"]
verify = builder["verify"]

with tempfile.TemporaryDirectory(prefix="richfinder-resource-test-") as temp:
    directory = Path(temp)
    for name in ("llama-server", model, "BGE-LICENSE", "LLAMA-LICENSE"):
        (directory / name).write_text(name)
    manifest = {path.name: digest(path) for path in directory.iterdir()}
    (directory / "manifest.json").write_text(json.dumps(manifest))

    try:
        builder["pinned_input"](directory, model)
    except RuntimeError as error:
        assert str(error) == f"The cached {model} failed its integrity check."
    else:
        raise AssertionError("A substituted download was accepted.")

    pins[model] = (pins[model][0], manifest[model])
    assert builder["pinned_input"](directory, model) == directory / model
    verify(directory)
    (directory / "llama-server").write_text("changed helper")
    try:
        verify(directory)
    except RuntimeError as error:
        assert str(error) == "The built RichFinder resource llama-server failed its integrity check."
    else:
        raise AssertionError("A substituted helper was accepted.")
PY

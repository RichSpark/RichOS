#!/usr/bin/env bash
# run-tests: no-host-screen: Python temporary-file fixtures only
# run-tests: inputs richos/app/scripts/search-resources.test.sh richos/app/scripts/build-search-resources.py
# run-tests: covers richos/app/scripts/build-search-resources.py
set -euo pipefail
PYTHONDONTWRITEBYTECODE=1 python3 - "$(dirname "${BASH_SOURCE[0]}")/build-search-resources.py" <<'PY'
import hashlib
import io
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tarfile
import tempfile
from unittest.mock import patch

builder = runpy.run_path(sys.argv[1])
model = builder["MODEL"]
pins = builder["PINS"]
digest = builder["digest"]
revision = builder["LLAMA_REV"]
pinned_input = builder["pinned_input"]
build = builder["build"]
verify = builder["verify"]


def expect_failure(check, error_type, message):
    try:
        check()
    except error_type as error:
        assert message in str(error), error
    else:
        raise AssertionError(f"Expected failure: {message}")


with tempfile.TemporaryDirectory(prefix="richfinder-resource-test-") as temp:
    cache = Path(temp)
    path = cache / "fixture"
    url = "https://example.invalid/fixture"
    contents = b"verified bytes"
    fixture_pins = {"fixture": (url, hashlib.sha256(contents).hexdigest())}

    path.write_bytes(b"old, invalid bytes")
    with patch("urllib.request.urlopen", return_value=io.BytesIO(b"invalid download")) as fetch:
        expect_failure(lambda: pinned_input(cache, path.name, fixture_pins), RuntimeError,
                       "The downloaded fixture failed its integrity check.")
        fetch.assert_called_once_with(url, timeout=60)
    assert path.read_bytes() == b"old, invalid bytes"
    assert sorted(cache.iterdir()) == [path]

    with patch("urllib.request.urlopen", return_value=io.BytesIO(contents)) as fetch:
        assert pinned_input(cache, path.name, fixture_pins) == path
        fetch.assert_called_once_with(url, timeout=60)
    with patch("urllib.request.urlopen") as fetch:
        assert pinned_input(cache, path.name, fixture_pins) == path
        fetch.assert_not_called()
    assert path.read_bytes() == contents

    path.unlink()
    with patch("urllib.request.urlopen", side_effect=OSError("fixture unavailable")):
        expect_failure(lambda: pinned_input(cache, path.name, fixture_pins), OSError, "fixture unavailable")
    assert not list(cache.iterdir())

    (cache / model).write_text("fixture model")
    (cache / "BGE-LICENSE").write_text("fixture model license")
    archive = cache / "llama-source.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for name, text in {
            "LICENSE": "fixture helper license",
            "tools/server/server.cpp": (
                "#include <thread> // for std::thread::hardware_concurrency\n"
                "int main(int argc, char ** argv) {\n}\n"
            ),
        }.items():
            data = text.encode()
            entry = tarfile.TarInfo(f"llama.cpp-{revision}/{name}")
            entry.size = len(data)
            tar.addfile(entry, io.BytesIO(data))
    for name in (model, "BGE-LICENSE", archive.name):
        pins[name] = (pins[name][0], digest(cache / name))

    def compile_fixture(args, **kwargs):
        if "--build" in args:
            executable = args[2] / "bin/llama-server"
            executable.parent.mkdir(parents=True)
            executable.write_text("fixture helper")

    def build_fixture():
        with patch("shutil.which", return_value="cmake"), \
                patch("subprocess.check_output", side_effect=["/fixture-sdk", "helper:\n"]), \
                patch("subprocess.run", side_effect=compile_fixture) as compile, \
                patch("urllib.request.urlopen") as fetch:
            result = build(cache)
            assert compile.call_count == 2
            fetch.assert_not_called()
            return result

    destination = build_fixture()
    verify(destination)
    with patch("shutil.which") as which:
        assert build(cache) == destination
        which.assert_not_called()

    helper = destination / "llama-server"
    helper.write_text("changed helper")
    with patch("shutil.which", return_value="cmake"), \
            patch("subprocess.check_output", return_value="/fixture-sdk"), \
            patch("subprocess.run", side_effect=subprocess.CalledProcessError(1, "cmake")):
        expect_failure(lambda: build(cache), subprocess.CalledProcessError, "cmake")
    expect_failure(lambda: verify(destination), RuntimeError, "llama-server failed its integrity check.")
    assert not list(cache.glob("richfinder-resources-*"))
    assert build_fixture() == destination
    assert helper.read_text() == "fixture helper"
    verify(destination)

    (destination / "manifest.json").write_text(json.dumps(None))
    assert build_fixture() == destination
    verify(destination)
PY

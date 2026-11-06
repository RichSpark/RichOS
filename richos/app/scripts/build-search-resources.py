#!/usr/bin/env python3
"""Build the pinned offline RichFinder model and its static macOS helper."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
LLAMA_REV = "eed25bc6b052c363aa760d0055282cc2222ccf6e"
MODEL_REV = "d32f8c040ea3b516330eeb75b72bcc2d3a780ab7"
MODEL = "bge-small-en-v1.5-q8_0.gguf"
RANK_REV = "eeed17e3bfc6fa06a790f2d12a9501fec587fccf"
RANK_URL = f"https://huggingface.co/cross-encoder/ms-marco-MiniLM-L6-v2/resolve/{RANK_REV}"
RANK_PINS = {
    "model_qint8_arm64.onnx": (
        f"{RANK_URL}/onnx/model_qint8_arm64.onnx",
        "3573b6b9593cb2f75987a31815d409ca3dd8808629118fd20451bb1a5d90cec7",
    ),
    "tokenizer.json": (
        f"{RANK_URL}/tokenizer.json",
        "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
    ),
    "MINILM-LICENSE": (
        "https://www.apache.org/licenses/LICENSE-2.0.txt",
        "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
    ),
}
PINS = {
    "llama-source.tar.gz": (
        f"https://codeload.github.com/ggml-org/llama.cpp/tar.gz/{LLAMA_REV}",
        "90102a1e114f7c26ae41026c0b984cb145cef13bf80320da33d084d9a82a1b71",
    ),
    MODEL: (
        f"https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/{MODEL_REV}/{MODEL}",
        "ec38e8da142596baa913124ae50550de284b6916bf59577ef2f0cb9660c2f514",
    ),
    "BGE-LICENSE": (
        "https://raw.githubusercontent.com/FlagOpen/FlagEmbedding/897baa67c0ffddbfccf4e6776ff55310d67809d4/LICENSE",
        "587a673933425dbc36ec61268d3b954051b2d3ef3c9b322ede357976055ffdd5",
    ),
}
FLAGS = (
    "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF",
    "-DGGML_NATIVE=OFF", "-DGGML_OPENMP=OFF", "-DGGML_METAL=ON",
    "-DGGML_METAL_EMBED_LIBRARY=ON", "-DGGML_BLAS=ON",
    "-DLLAMA_BUILD_TESTS=OFF", "-DLLAMA_BUILD_EXAMPLES=OFF",
    "-DLLAMA_BUILD_TOOLS=ON", "-DLLAMA_BUILD_SERVER=ON", "-DLLAMA_OPENSSL=OFF",
    "-DCMAKE_OSX_ARCHITECTURES=arm64", "-DCMAKE_OSX_DEPLOYMENT_TARGET=13.0",
)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def pinned_input(cache, name, pins=PINS):
    url, expected = pins[name]
    path = cache / name
    if not path.exists():
        part = cache / (name + ".download")
        try:
            with urllib.request.urlopen(url, timeout=60) as response, part.open("wb") as out:
                remaining = 128 * 1024 * 1024
                while data := response.read(1024 * 1024):
                    remaining -= len(data)
                    if remaining < 0:
                        raise RuntimeError("The RichFinder resource download exceeded its limit.")
                    out.write(data)
            if digest(part) != expected:
                raise RuntimeError(f"The downloaded {name} failed its integrity check.")
            part.replace(path)
        finally:
            part.unlink(missing_ok=True)
    if digest(path) != expected:
        raise RuntimeError(f"The cached {name} failed its integrity check.")
    return path


def verify(directory):
    files = json.loads((directory / "manifest.json").read_text())
    required = {"llama-server", MODEL, "BGE-LICENSE", "LLAMA-LICENSE"}
    if set(files) != required or files[MODEL] != PINS[MODEL][1]:
        raise RuntimeError("The cached RichFinder resources are incomplete.")
    for name, expected in files.items():
        if digest(directory / name) != expected:
            raise RuntimeError(f"The built RichFinder resource {name} failed its integrity check.")


def watch_owner(source):
    server = source / "tools/server/server.cpp"
    code = server.read_text()
    include = "#include <thread> // for std::thread::hardware_concurrency\n"
    entry = "int main(int argc, char ** argv) {\n"
    if code.count(include) != 1 or code.count(entry) != 1:
        raise RuntimeError("The pinned RichFinder server entry point changed.")
    code = code.replace(include, include + "#include <cerrno>\n#include <cstdlib>\n#include <unistd.h>\n")
    server.write_text(code.replace(entry, entry + """    // The application owns the write end of stdin. EOF survives a forced quit.
    if (const char * socket = std::getenv("RICHFINDER_OWNER_SOCKET")) {
        std::thread([path = std::string(socket)] {
            char byte;
            ssize_t result;
            do {
                result = read(STDIN_FILENO, &byte, 1);
            } while (result > 0 || (result < 0 && errno == EINTR));
            unlink(path.c_str());
            rmdir(path.substr(0, path.find_last_of('/')).c_str());
            _exit(0);
        }).detach();
    }
"""))


def build(cache):
    destination = cache / f"b7870-{digest(Path(__file__))[:16]}"
    if destination.exists():
        verify(destination)
        return destination
    cmake = shutil.which("cmake")
    if not cmake:
        raise RuntimeError("CMake is required to build the RichFinder runtime.")
    with tempfile.TemporaryDirectory(prefix="richfinder-build-") as temp:
        work = Path(temp)
        with tarfile.open(pinned_input(cache, "llama-source.tar.gz")) as source:
            source.extractall(work, filter="data")
        source = work / f"llama.cpp-{LLAMA_REV}"
        watch_owner(source)
        compiled = work / "build"
        path_map = f"-ffile-prefix-map={source}=llama.cpp-source"
        env = {**os.environ, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
               "SDKROOT": subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip()}
        log_path = work / "compile.log"
        try:
            with log_path.open("w") as log:
                subprocess.run([cmake, "-S", source, "-B", compiled, *FLAGS,
                                "-DCMAKE_C_FLAGS=" + path_map, "-DCMAKE_CXX_FLAGS=" + path_map],
                               env=env, stdout=log, stderr=log, check=True, timeout=120)
                subprocess.run([cmake, "--build", compiled, "--target", "llama-server", "-j", "4"],
                               env=env, stdout=log, stderr=log, check=True, timeout=1200)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            shutil.copyfile(log_path, cache / "last-failed-compile.log")
            print(log_path.read_text()[-8000:], file=sys.stderr)
            raise
        executable = compiled / "bin/llama-server"
        libraries = subprocess.check_output(["otool", "-L", executable], text=True).splitlines()[1:]
        if any(not line.strip().startswith(("/usr/lib/", "/System/Library/")) for line in libraries):
            raise RuntimeError("RichFinder's helper must link only macOS system libraries.")
        with tempfile.TemporaryDirectory(prefix="richfinder-resources-", dir=cache) as stage:
            result = Path(stage) / "resources"
            result.mkdir()
            shutil.copy2(executable, result / "llama-server")
            shutil.copyfile(source / "LICENSE", result / "LLAMA-LICENSE")
            for name in (MODEL, "BGE-LICENSE"):
                shutil.copyfile(pinned_input(cache, name), result / name)
            files = {f.name: digest(f) for f in sorted(result.iterdir())}
            (result / "manifest.json").write_text(json.dumps(files, indent=2) + "\n")
            verify(result)
            result.replace(destination)
    return destination


def build_ranking(cache):
    destination = cache / f"minilm-{RANK_REV[:12]}"
    destination.mkdir(exist_ok=True)
    for name in RANK_PINS:
        pinned_input(destination, name, RANK_PINS)
    return destination


def main():
    cache = Path(subprocess.check_output([
        sys.executable, HERE / "../../engine/scripts/lib/machine_settings.py",
        "cache", "richfinder/resources",
    ], text=True, timeout=30).strip())
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if sys.argv[1:] == ["--ranking"]:
            print(build_ranking(cache))
        elif len(sys.argv) == 1:
            print(build(cache))
        else:
            raise SystemExit("Usage: build-search-resources.py [--ranking]")


if __name__ == "__main__":
    main()

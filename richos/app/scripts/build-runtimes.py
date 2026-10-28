#!/usr/bin/env python3
"""Prepare pinned macOS runtimes from public sources into a new staging directory.

Build-time tooling is allowed here. Installed execution must use only the delivered
runtimes and macOS system libraries. This does not install or activate an engine.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import unicodedata
import urllib.request
import zipfile


# What the delivered Python must be able to import, run inside every fresh runtime build.
# ctypes and libproc: provider-supervisor.py reads the process table through them to reap a
# lease's tool commands, and a runtime without them would silently stop reaping (reap gap C8).
CLOSURE_CHECK = ("import sqlite3, fcntl, ssl, ctypes; ctypes.CDLL('/usr/lib/libproc.dylib'); "
                 "print('Python dependency closure OK')")


# THE DELIVERED GIT MUST WORK IN AN ORDINARY MAC REPOSITORY. `git init` on macOS writes
# core.precomposeunicode=true, and with it Git converts every non-ASCII name it reads from a
# directory through iconv_open("UTF-8", "UTF-8-MAC") (compat/precompose_utf8.c). Only Apple's
# system libiconv knows "UTF-8-MAC"; a Git linked against stock GNU libiconv dies with
# "iconv_open(UTF-8,UTF-8-MAC) failed, but needed: precomposed unicode is not supported" the
# first time `git status` meets such a name (measured 2026-10-28: richos-hq, and the runtime
# the nightly built from runtime-sources.json eb026316eb08). An ASCII-only repository never
# reaches that code, which is why the version check alone never saw it. build() runs this
# against every fresh runtime and build-runtimes.test.sh runs it against RICHOS_RUNTIME_DIR.
PRECOMPOSE_NAME = "café.txt"


def check_git_precompose(git, env=None):
    """`git status` in a fresh repository with core.precomposeunicode=true and a file whose
    name is stored decomposed (NFD) must exit 0 and report the name precomposed (NFC).
    Returns the porcelain record; raises RuntimeError quoting Git's own output otherwise.
    Isolated from the caller's Git configuration and home."""
    with tempfile.TemporaryDirectory(prefix="richos-git-precompose-") as temporary:
        root = Path(temporary)
        (root / "home").mkdir()
        isolated = {**(env if env is not None else os.environ), "HOME": str(root / "home"),
                    "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
                    "LC_ALL": "C", "LANG": "C"}
        repo = root / "repo"

        def git_run(*arguments):
            return subprocess.run([str(git), *arguments], env=isolated, capture_output=True)

        for arguments in (("init", "-q", str(repo)), ("-C", str(repo), "config", "core.precomposeunicode", "true")):
            result = git_run(*arguments)
            if result.returncode != 0:
                raise RuntimeError(f"git {arguments[0]} failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace").strip())
        (repo / unicodedata.normalize("NFD", PRECOMPOSE_NAME)).write_text("fictional\n")
        result = git_run("-C", str(repo), "status", "--porcelain", "-z")
        if result.returncode != 0:
            raise RuntimeError(f"git status in a core.precomposeunicode=true repository exited "
                               f"{result.returncode}: " + result.stderr.decode(errors="replace").strip())
        wanted = ("?? " + unicodedata.normalize("NFC", PRECOMPOSE_NAME) + "\0").encode()
        if result.stdout != wanted:
            raise RuntimeError(f"git status did not report the precomposed name: {result.stdout!r}, wanted {wanted!r}")
        return result.stdout.decode().rstrip("\0")


# Every download names itself. ffmpeg.martin-riedl.de answers Python's default User-Agent
# ("Python-urllib/3.x") with HTTP 403 and any other with the file (measured 2026-10-07: the
# first runtime build with ffmpeg in the recipe stopped there). The pinned sha256 is what is
# trusted, never the header.
USER_AGENT = "richos-build-runtimes"


def fetch(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": USER_AGENT}), timeout=60)


def sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


# whisper-cli, the speech decoder voice mode runs (richos-voice `stt.rs`). Built here from the
# pinned whisper.cpp source so every user's Mac gets it with the engine, and so it is the same
# version the decode settings were measured on (`engine/voice/models/model-pins.json`,
# `toolchain.whisperCppVersion`). One static executable: whisper and its vendored ggml are linked
# in (BUILD_SHARED_LIBS=OFF), the Metal shaders are embedded, and it links only macOS frameworks.
# GGML_NATIVE=OFF so the build machine's own CPU features (an M4's) are not required on an M1;
# OpenMP is off because Apple's clang has none and ggml then uses its own thread pool.
WHISPER_CMAKE_FLAGS = (
    "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF", "-DGGML_NATIVE=OFF", "-DGGML_OPENMP=OFF",
    "-DGGML_METAL=ON", "-DGGML_METAL_EMBED_LIBRARY=ON", "-DGGML_BLAS=ON",
    "-DWHISPER_BUILD_TESTS=OFF", "-DWHISPER_BUILD_SERVER=OFF", "-DWHISPER_SDL2=OFF", "-DWHISPER_CURL=OFF",
    "-DCMAKE_OSX_ARCHITECTURES=arm64", "-DCMAKE_OSX_DEPLOYMENT_TARGET=13.0",
)


def build_whisper_cli(source, scratch, runtime):
    # CMake is build-time tooling only, like Xcode's compilers; nothing it produces links to it.
    cmake = shutil.which("cmake")
    if not cmake:
        raise RuntimeError("whisper-cpp: cmake is required to build the runtime and is not on PATH")
    build_dir = scratch / "whisper-build"
    prefix_map = "-ffile-prefix-map=" + str(source) + "=whisper.cpp-source"
    clean_env = {**os.environ, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
                 "SDKROOT": subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip()}
    log_path = scratch / "whisper-build.log"
    try:
        with log_path.open("w") as log:
            subprocess.run([cmake, "--version"], env=clean_env, stdout=log, stderr=log, check=True)
            subprocess.run([cmake, "-S", str(source), "-B", str(build_dir), *WHISPER_CMAKE_FLAGS,
                            "-DCMAKE_C_FLAGS=" + prefix_map, "-DCMAKE_CXX_FLAGS=" + prefix_map],
                           env=clean_env, stdout=log, stderr=log, check=True)
            subprocess.run([cmake, "--build", str(build_dir), "--config", "Release", "--target", "whisper-cli", "-j", "4"],
                           env=clean_env, stdout=log, stderr=log, check=True)
    except subprocess.CalledProcessError:
        print(log_path.read_text()[-10000:], flush=True)
        raise
    shutil.copyfile(build_dir / "bin/whisper-cli", runtime / "bin/whisper-cli")
    (runtime / "bin/whisper-cli").chmod(0o755)
    # MIT: the copyright and permission notice travels with the binary. Its one LICENSE ("The
    # ggml authors") covers whisper.cpp and the ggml it vendors.
    shutil.copyfile(source / "LICENSE", runtime / "sources/WHISPER-CPP-LICENSE")


# ffmpeg and ffprobe: an upstream static build, pinned like jq rather than built here. Each zip
# must hold exactly the one executable it is named for; anything else is refused, not unpacked.
def install_zipped_executable(archive, name, target):
    with zipfile.ZipFile(archive) as bundle:
        members = bundle.infolist()
        if [member.filename for member in members] != [name] or members[0].is_dir():
            raise RuntimeError(f"{name}: the zip must hold exactly one file named {name}")
        target.write_bytes(bundle.read(members[0]))
    target.chmod(0o755)


def fetch_pinned(url, wanted, label):
    with fetch(url) as incoming:
        data = incoming.read()
    if hashlib.sha256(data).hexdigest() != wanted:
        raise RuntimeError(f"{label} digest mismatch")
    return data


# The ffmpeg build is configured --enable-gpl --enable-version3, so as a whole it is GPLv3 (or
# later). Its corresponding source is about 304 MB of archives (measured 2026-10-07: 304,272,357
# bytes for the 34 listed in the recipe), more than twice today's whole engine asset, so it is not
# copied into every user's runtime the way git's one tarball is. The runtime carries the license
# text and these directions instead: every archive, pinned by SHA-256, at the exact version the
# build used (GPLv3 section 6(d), source on a different server with clear directions). The
# nightly publish step (`nightly.py` `attach_gpl_sources`) attaches every one of them to each
# RichOS release whose engine carries ffmpeg, under the row's `asset` name, which these
# directions print; `nightly.test.py` keeps the recipe's names equal to the ones it uploads.
def ffmpeg_source_directions(source):
    lines = [
        "bin/ffmpeg and bin/ffprobe are the unmodified static macOS arm64 build of FFmpeg "
        f"{source['version']} published by Martin Riedl at {source['url'].rsplit('/', 1)[0]}/ "
        "(signed with Developer ID team KU3N25YGLU). It is configured with --enable-gpl and "
        "--enable-version3, so the programs are distributed under the GNU General Public License "
        "version 3 or later; the full text is FFMPEG-COPYING.GPLv3 beside this file. `ffmpeg -version` "
        "prints the complete configuration.",
        "",
        "Corresponding Source: FFmpeg and every library linked into these programs, at the exact "
        "version the build used, and the build script at the revision that produced it. Each archive "
        "is identified by its SHA-256. rav1e's Rust dependencies are fixed by the Cargo.lock inside "
        "its archive. x264 was built from its master branch; the commit listed is master at the build "
        "time (2026-09-20 19:18 UTC), unchanged since 2025-09-10.",
        "",
        "Where to get it: RichOS publishes a byte-identical copy of every archive below as an asset "
        "of each RichOS release whose engine carries these programs, under the name shown as "
        "\"RichOS copy\", at https://github.com/RichSpark/RichOS/releases/download/<release>/<name>. "
        "<release> is \"v\" followed by the RichOS version that installed this engine, for example "
        "v1.2.0-nightly.20261007.45; https://github.com/RichSpark/RichOS/releases/tag/<release> "
        "lists them. The upstream URL is where each archive was first published.",
        "",
    ]
    for row in source["corresponding_source"]:
        lines += [f"{row['name']} {row['version']}", f"  RichOS copy {row['asset']}",
                  f"  upstream {row['url']}", f"  sha256 {row['sha256']}"]
    return "\n".join(lines) + "\n"


def write_launcher(runtime, name, target, setup="", flags=""):
    # The packaged probe allows exec but forbids fork. Shell parameter expansion
    # locates the sibling runtime without spawning dirname or a subshell.
    wrapper = runtime / "bin" / name
    wrapper.write_text('#!/bin/sh\ncase "$0" in\n  */*) runtime_bin=${0%/*} ;;\n'
                       '  *) runtime_bin=. ;;\nesac\n' + setup +
                       'exec "$runtime_bin/../' + target + '" ' + flags + '"$@"\n')
    wrapper.chmod(0o755)


def prepare_portable_node(runtime, recipe):
    """Standard debug stripping and portable OpenSSL module lookup; no byte edits."""
    binary = runtime / "node/bin/node"
    if recipe.get("strip_node_debug_symbols"):
        subprocess.run(["/usr/bin/strip", "-S", str(binary)], check=True)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(binary)], check=True)
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(binary)], check=True)
    (runtime / "node/lib/ossl-modules").mkdir()
    write_launcher(runtime, "node", "node/bin/node",
                   ': "${OPENSSL_MODULES:=$runtime_bin/../node/lib/ossl-modules}"\nexport OPENSSL_MODULES\n')


def prepare_runtime_launchers(runtime, recipe):
    prepare_portable_node(runtime, recipe)
    # Relocation can invalidate upstream bytecode. Do not rewrite the delivery.
    write_launcher(runtime, "python3", "python/bin/python3",
                   'export PYTHONDONTWRITEBYTECODE=1\n', flags="-B ")
    # Git resolves its helper prefix from the real installed entry point.
    write_launcher(runtime, "git", "git/bin/git")


def build(destination):
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("this runtime recipe supports macOS arm64 only")
    manifest = json.loads(Path(__file__).with_name("runtime-sources.json").read_text())
    destination = destination.absolute()
    if destination.exists():
        raise RuntimeError("destination must be absent; existing delivery is never overwritten")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="richos-public-runtime-build-") as temporary:
        scratch = Path(temporary)
        runtime = scratch / "runtime"
        runtime.mkdir()
        (runtime / "bin").mkdir()
        (runtime / "sources").mkdir()
        for name, source in manifest["sources"].items():
            archive = scratch / f"{name}.download"
            print(f"Fetching verified {name} {source['version']}", flush=True)
            with fetch(source["url"]) as incoming, archive.open("wb") as out:
                shutil.copyfileobj(incoming, out)
            if sha(archive) != source["sha256"]:
                raise RuntimeError(f"{name}: upstream digest mismatch")
            if name == "jq":
                shutil.copyfile(archive, runtime / "bin/jq")
                (runtime / "bin/jq").chmod(0o755)
                with fetch(source["license_url"]) as incoming:
                    license_text = incoming.read()
                if hashlib.sha256(license_text).hexdigest() != source["license_sha256"]:
                    raise RuntimeError("jq license digest mismatch")
                (runtime / "sources/JQ-COPYING").write_bytes(license_text)
                continue
            if name in ("ffmpeg", "ffprobe"):
                install_zipped_executable(archive, name, runtime / "bin" / name)
                if "license_url" in source:
                    (runtime / "sources/FFMPEG-COPYING.GPLv3").write_bytes(
                        fetch_pinned(source["license_url"], source["license_sha256"], f"{name} license"))
                    (runtime / "sources/FFMPEG-SOURCE.txt").write_text(ffmpeg_source_directions(source))
                continue
            unpacked = scratch / f"{name}-unpacked"
            unpacked.mkdir()
            with tarfile.open(archive) as tar:
                tar.extractall(unpacked, filter="data")
            entries = list(unpacked.iterdir())
            if len(entries) != 1 or not entries[0].is_dir():
                raise RuntimeError(f"{name}: unexpected upstream archive root")
            if name in ("python", "node"):
                shutil.move(entries[0], runtime / name)
                continue
            if name == "whisper-cpp":
                build_whisper_cli(entries[0], scratch, runtime)
                continue
            if name == "iconv":
                iconv_source = entries[0]
                iconv_prefix = scratch / "iconv-static"
                clean_env = {**os.environ, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
                    "CFLAGS": "-O2 -mmacosx-version-min=13.0 -ffile-prefix-map=" + str(iconv_source) + "=iconv-source"}
                with (scratch / "iconv-build.log").open("w") as log:
                    subprocess.run([str(iconv_source / "configure"), "--prefix=" + str(iconv_prefix),
                        "--disable-shared", "--enable-static", "--disable-nls"], cwd=iconv_source, env=clean_env, stdout=log, stderr=log, check=True)
                    subprocess.run(["make", "-j4"], cwd=iconv_source, env=clean_env, stdout=log, stderr=log, check=True)
                    subprocess.run(["make", "install"], cwd=iconv_source, env=clean_env, stdout=log, stderr=log, check=True)
                shutil.copyfile(archive, runtime / "sources/libiconv-1.18.tar.gz")
                shutil.copyfile(iconv_source / "COPYING.LIB", runtime / "sources/ICONV-COPYING.LIB")
                continue
            # Retain the exact source archive with the GPL implementation. No
            # developer checkout, system Git binary or private configuration is copied.
            shutil.copyfile(archive, runtime / "sources/git-2.55.0.tar.xz")
            git_source = entries[0]
            flags = ["prefix=/", "RUNTIME_PREFIX=YesPlease", "NO_GETTEXT=YesPlease",
                     "NO_TCLTK=YesPlease", "NO_PERL=YesPlease", "NO_OPENSSL=YesPlease",
                     "USE_HOMEBREW_LIBICONV=", "ICONVDIR=" + str(iconv_prefix),
                     "NO_PYTHON=YesPlease", "NO_RUST=YesPlease", "NO_INSTALL_HARDLINKS=YesPlease",
                     "CFLAGS=-O2 -mmacosx-version-min=13.0 -ffile-prefix-map=" + str(git_source) + "=git-source"]
            # Static iconv needs -L at build time, not an installed build-directory rpath.
            if manifest.get("portable_git_link_paths"):
                flags.append("CC_LD_DYNPATH=")
            clean_env = {**os.environ, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "SDKROOT": subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip()}
            try:
                with (scratch / "git-build.log").open("w") as log:
                    subprocess.run(["make", "-j4", *flags, "all"], cwd=git_source, env=clean_env, stdout=log, stderr=log, check=True)
                    subprocess.run(["make", *flags, "DESTDIR=" + str(runtime / "git"), "install"], cwd=git_source, env=clean_env, stdout=log, stderr=log, check=True)
            except subprocess.CalledProcessError:
                print((scratch / "git-build.log").read_text()[-10000:], flush=True)
                raise
            shutil.copyfile(git_source / "COPYING", runtime / "sources/GIT-COPYING")
        prepare_runtime_launchers(runtime, manifest)
        # License files from binary upstream distributions are retained in full.
        (runtime / "sources/README.txt").write_text("Pinned upstream distributions are listed in runtime-sources.json. Git's exact corresponding source and COPYING are included. Python and Node license notices remain in their distributions. jq is distributed under the MIT license; see https://github.com/jqlang/jq/blob/jq-1.8.2/COPYING. whisper-cli is built from the pinned whisper.cpp source and is distributed under the MIT license in WHISPER-CPP-LICENSE. ffmpeg and ffprobe are an unmodified upstream static build distributed under GPLv3 or later in FFMPEG-COPYING.GPLv3; FFMPEG-SOURCE.txt says where their exact corresponding source is. macOS supplies bash and system libraries.\n")
        shutil.copyfile(__file__, runtime / "sources/RICHOS-RUNTIME-BUILDER.py")
        shutil.copyfile(Path(__file__).parent / "runtime-sources.json", runtime / "sources/runtime-sources.json")
        (runtime / "runtime-sources.json").write_text(json.dumps(manifest, indent=2) + "\n")
        env = {**os.environ, "PATH": str(runtime / "bin") + ":/usr/bin:/bin:/usr/sbin:/sbin",
               "GIT_EXEC_PATH": str(runtime / "git/libexec/git-core"), "PYTHONDONTWRITEBYTECODE": "1"}
        versions = {}
        for name in ("python3", "node", "git", "jq", "whisper-cli"):
            versions[name] = subprocess.check_output([str(runtime / "bin" / name), "--version"], text=True, env=env).strip()
        # ffmpeg and ffprobe answer -version, and print their whole configuration after the first line.
        for name in ("ffmpeg", "ffprobe"):
            versions[name] = subprocess.check_output([str(runtime / "bin" / name), "-version"], text=True, env=env).splitlines()[0]
        subprocess.run([str(runtime / "bin/python3"), "-c", CLOSURE_CHECK], env=env, check=True)
        # Reject any dynamic dependency on Homebrew or the developer's home.
        for path in runtime.rglob("*"):
            if path.is_file() and not path.is_symlink():
                with path.open("rb") as handle:
                    magic = handle.read(4)
                if magic in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xfe\xed\xfa\xcf"):
                    linked = subprocess.check_output(["otool", "-L", str(path)], text=True)
                    dependencies = "\n".join(linked.splitlines()[1:])
                    if "/opt/homebrew/" in dependencies or "/usr/local/" in dependencies or "/Users/" in dependencies:
                        raise RuntimeError(f"nonportable linked dependency: {path.relative_to(runtime)}\n{linked}")
        files = {str(p.relative_to(runtime)): sha(p) for p in runtime.rglob("*") if p.is_file() and not p.is_symlink()}
        links = {str(p.relative_to(runtime)): os.readlink(p) for p in runtime.rglob("*") if p.is_symlink()}
        (runtime / "delivery.json").write_text(json.dumps({"schema": 1, "platform": manifest["platform"], "versions": versions,
            "files": files, "links": links}, indent=2) + "\n")
        shutil.move(runtime, destination)
    print(json.dumps({"runtime": str(destination), "versions": versions, "activated": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    build(parser.parse_args().destination)

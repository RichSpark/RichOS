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


# HOW GIT GETS ITS ICONV: from macOS, /usr/lib/libiconv.2.dylib (the SDK's -liconv), the only
# iconv that has "UTF-8-MAC" (check_git_precompose above), present on every Mac like libz and
# libSystem, which the delivered git already links. Not GNU libiconv, which RichOS no longer
# builds or ships. ICONVDIR empty: no -I/-L to any other iconv. NO_HOMEBREW, NO_FINK and
# NO_DARWIN_PORTS: git's Makefile otherwise adds -L/opt/homebrew/..., /sw/lib and /opt/local/lib
# when those directories exist on the build Mac, and a libiconv.dylib there would be linked
# instead. NEEDS_GOOD_LIBICONV set on every build Mac, not only on a Darwin 24+ one as git's
# config.mak.uname decides: with no "good" (Homebrew/Fink/MacPorts) iconv it compiles
# ICONV_RESTART_RESET, git's own workaround for macOS 14+ system iconv's piecemeal-conversion
# fault (utf8.c reencode_string_iconv), so the bytes do not depend on the build Mac's macOS.
#
# HAS_GOOD_LIBICONV is what turns that workaround OFF (Makefile: `ifdef NEEDS_GOOD_LIBICONV` /
# `ifndef HAS_GOOD_LIBICONV` / `BASIC_CFLAGS += -DICONV_RESTART_RESET`), and make takes every
# variable of its environment as its own, so a HAS_GOOD_LIBICONV exported by the builder's shell
# would drop it with no sign anywhere. Two defenses, and a check that proves them before the build:
# the build runs under an allowlisted environment (git_build_environment), the flag is emptied on
# the command line (which outranks both the environment and the Makefile's own assignments), and
# check_git_iconv_workaround asks make for its variable database and refuses to build without the
# -D (the 2026-10-28 review of the bundled-git iconv fix, finding 1).
GIT_ICONV_FLAGS = ("ICONVDIR=", "USE_HOMEBREW_LIBICONV=", "NEEDS_GOOD_LIBICONV=UnfortunatelyYes",
                   "HAS_GOOD_LIBICONV=",
                   "NO_HOMEBREW=YesPlease", "NO_FINK=YesPlease", "NO_DARWIN_PORTS=YesPlease")
SYSTEM_ICONV = "/usr/lib/libiconv.2.dylib"
# The only variables of the builder's environment that reach git's make; make reads every other one
# (LDFLAGS, LIBRARY_PATH, CPPFLAGS, HAS_GOOD_LIBICONV, MAKEFLAGS, ...) as its own setting.
GIT_BUILD_ENV_ALLOWED = ("HOME", "TMPDIR", "DEVELOPER_DIR")


def git_build_environment(base, sdkroot):
    """The environment git is built under: the allowlist above, the system tool path, the SDK the
    caller found, and the C locale."""
    environment = {name: base[name] for name in GIT_BUILD_ENV_ALLOWED if name in base}
    environment.update({"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "SDKROOT": sdkroot,
                        "LC_ALL": "C", "LANG": "C"})
    return environment


def check_git_iconv_workaround(git_source, flags, environment):
    """Raise RuntimeError unless make, run in `git_source` with these flags and environment, would
    compile git with -DICONV_RESTART_RESET (a dry run that prints make's variable database; nothing
    is compiled). Run before the build so a lost workaround costs seconds, not a nightly."""
    result = subprocess.run(["make", "-pn", *flags, "all"], cwd=git_source, env=environment,
                            capture_output=True, text=True)
    if "-DICONV_RESTART_RESET" in result.stdout:
        return
    detail = ("make could not describe the build: " + result.stderr.strip()[-400:]) if not result.stdout.strip() else \
        "a HAS_GOOD_LIBICONV, or another iconv setting, is turning git's workaround off"
    raise RuntimeError("git would be built without -DICONV_RESTART_RESET, git's own workaround for "
                       "the macOS system iconv's piecemeal-conversion fault (utf8.c "
                       "reencode_string_iconv): " + detail)


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


# EVERY ENTRY IN runtime/bin IS A MACH-O EXECUTABLE (or a symlink to one), NEVER A `#!` SCRIPT.
# macOS leaks about 1 KiB of wired kernel memory (zone data.kalloc.1024, freed only by reboot)
# for each `#!` script exec'd from a process that was itself started from a script, and the
# runtime's bin is first on every session's PATH (runtime-launcher.c has the measurement).
# python3, git and node therefore get runtime-launcher.c compiled per entry: it sets what the
# old `#!/bin/sh` wrappers set and execs the real binary, with no fork (the packaged probe allows
# exec only). build() refuses a runtime with any other kind of bin entry (script_entries).
LAUNCHER_SOURCE = Path(__file__).with_name("runtime-launcher.c")
MACHO_MAGICS = (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xfe\xed\xfa\xcf")


def is_macho(path):
    with Path(path).open("rb") as handle:
        return handle.read(4) in MACHO_MAGICS


def script_entries(bin_dir):
    """The names in bin_dir that are neither a Mach-O executable nor a symlink to one inside
    the runtime (bin_dir's parent). Empty for a runtime whose bin execs no `#!` script."""
    root = Path(bin_dir).parent.resolve()
    refused = []
    for entry in sorted(Path(bin_dir).iterdir()):
        resolved = entry.resolve()
        if not resolved.is_relative_to(root) or not resolved.is_file() or not is_macho(resolved):
            refused.append(entry.name)
    return refused


def write_launcher(runtime, name, target, *, set_variable=None, default_path=None, flag=None):
    """Compile runtime-launcher.c to runtime/bin/<name>, execing runtime/<target>.
    set_variable (name, value) is always set; default_path (name, path under the runtime) is
    set only when the caller's value is unset or empty; flag precedes the caller's arguments."""
    def string(value):
        if '"' in value or "\\" in value or "\n" in value:
            raise ValueError(f"launcher definition must be a plain string: {value!r}")
        return '"' + value + '"'
    definitions = ["-DLAUNCH_TARGET=" + string(target)]
    if set_variable:
        definitions += ["-DLAUNCH_SET_NAME=" + string(set_variable[0]), "-DLAUNCH_SET_VALUE=" + string(set_variable[1])]
    if default_path:
        definitions += ["-DLAUNCH_DEFAULT_NAME=" + string(default_path[0]), "-DLAUNCH_DEFAULT_PATH=" + string(default_path[1])]
    if flag:
        definitions.append("-DLAUNCH_FLAG=" + string(flag))
    launcher = runtime / "bin" / name
    # Build-time tooling only, like git's make: Xcode's clang, a clean PATH, the SDK named.
    # -Wl,-S -Wl,-x: no debug map or local symbols, so no build path is carried in the bytes.
    clean_env = {**os.environ, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
                 "SDKROOT": subprocess.check_output(["/usr/bin/xcrun", "--show-sdk-path"], text=True).strip()}
    subprocess.run(["/usr/bin/xcrun", "clang", "-std=c11", "-Os", "-Wall", "-Wextra", "-Werror",
                    "-arch", "arm64", "-mmacosx-version-min=13.0", "-Wl,-S", "-Wl,-x", *definitions,
                    "-o", str(launcher), str(LAUNCHER_SOURCE)], env=clean_env, check=True)
    launcher.chmod(0o755)


def check_launched_entries(runtime):
    """What the old wrappers did, proven through runtime/bin with NONE of their variables in
    the caller's environment: git finds its own exec-path and templates and runs its libexec
    helpers, Python writes no bytecode (nor does a Python it starts through sys.executable),
    and node gets the runtime's OpenSSL module directory. Returns one line per entry; raises
    RuntimeError naming the first that fails. Isolated from the caller's Git config and home."""
    runtime = Path(runtime).resolve()
    scripts = script_entries(runtime / "bin")
    if scripts:
        raise RuntimeError("runtime/bin entries that are not Mach-O: " + ", ".join(scripts))
    lines = []
    with tempfile.TemporaryDirectory(prefix="richos-launched-entries-") as temporary:
        root = Path(temporary).resolve()
        (root / "home").mkdir()
        env = {"PATH": str(runtime / "bin") + ":/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(root / "home"),
               "TMPDIR": str(root), "LC_ALL": "C", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
               "GIT_AUTHOR_NAME": "Fictional", "GIT_AUTHOR_EMAIL": "fictional@example.invalid",
               "GIT_COMMITTER_NAME": "Fictional", "GIT_COMMITTER_EMAIL": "fictional@example.invalid"}

        def run(*arguments, cwd=None, environment=None):
            result = subprocess.run([str(a) for a in arguments], cwd=cwd, env=environment or env,
                                    capture_output=True, text=True)
            if result.returncode != 0:
                raise RuntimeError(f"{' '.join(map(str, arguments))} exited {result.returncode}: "
                                   + (result.stderr or result.stdout).strip())
            return result

        git = runtime / "bin/git"
        exec_path = Path(run(git, "--exec-path").stdout.strip()).resolve()
        if exec_path != runtime / "git/libexec/git-core":
            raise RuntimeError(f"git --exec-path is {exec_path}, not the runtime's git/libexec/git-core")
        repo = root / "repo"
        initialized = run(git, "init", "-q", repo)
        if "templates not found" in initialized.stderr or not (repo / ".git/description").is_file():
            raise RuntimeError("git init found no templates: " + initialized.stderr.strip())
        run(git, "-C", repo, "commit", "-q", "--allow-empty", "-m", "fictional")
        # A local ls-remote runs git-upload-pack, which lives only in the exec-path.
        advertised = run(git, "ls-remote", repo).stdout
        if "\tHEAD\n" not in advertised:
            raise RuntimeError(f"git ls-remote through git-upload-pack advertised no HEAD: {advertised!r}")
        lines.append(f"git: exec-path {exec_path}, templates copied, git-upload-pack ran; "
                     f"precomposed {check_git_precompose(git, env)!r}")

        modules = root / "modules"
        modules.mkdir()
        (modules / "parent_probe.py").write_text("VALUE = 1\n")
        (modules / "child_probe.py").write_text("VALUE = 2\n")
        child = f"import sys; sys.path.insert(0, {str(modules)!r}); import child_probe; print(sys.dont_write_bytecode)"
        flags = run(runtime / "bin/python3", "-c",
                    f"import subprocess, sys; sys.path.insert(0, {str(modules)!r}); import parent_probe; "
                    f"print(sys.dont_write_bytecode); subprocess.run([sys.executable, '-c', {child!r}], check=True)").stdout.split()
        written = sorted(str(p.relative_to(modules)) for p in modules.rglob("*") if p.suffix == ".pyc" or p.name == "__pycache__")
        if flags != ["True", "True"] or written:
            raise RuntimeError(f"python3 wrote bytecode or did not disable it: dont_write_bytecode {flags}, written {written}")
        lines.append("python3: no .pyc written by it or by a Python started through sys.executable")

        node = runtime / "bin/node"
        modules_dir = run(node, "-p", "process.env.OPENSSL_MODULES").stdout.strip()
        if Path(modules_dir).resolve() != runtime / "node/lib/ossl-modules" or not Path(modules_dir).is_dir():
            raise RuntimeError(f"node's OPENSSL_MODULES is {modules_dir!r}, not the runtime's node/lib/ossl-modules")
        explicit = run(node, "-p", "process.env.OPENSSL_MODULES",
                       environment={**env, "OPENSSL_MODULES": str(root / "explicit")}).stdout.strip()
        if explicit != str(root / "explicit"):
            raise RuntimeError(f"an explicit OPENSSL_MODULES was replaced: {explicit!r}")
        md4 = run(node, "--openssl-legacy-provider", "-p",
                  "require('crypto').createHash('md4').update('x').digest('hex')").stdout.strip()
        if md4 != "51b834b7c1ef0b59ea50888fcb39ace2":
            raise RuntimeError(f"node's OpenSSL legacy provider did not load: md4 {md4!r}")
        lines.append(f"node: OPENSSL_MODULES {modules_dir}, an explicit value kept, legacy provider loaded")
    return lines


def prepare_portable_node(runtime, recipe):
    """Standard debug stripping and portable OpenSSL module lookup; no byte edits."""
    binary = runtime / "node/bin/node"
    if recipe.get("strip_node_debug_symbols"):
        subprocess.run(["/usr/bin/strip", "-S", str(binary)], check=True)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(binary)], check=True)
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(binary)], check=True)
    (runtime / "node/lib/ossl-modules").mkdir()
    # node's compiled-in MODULESDIR is the upstream build machine's; an explicit value wins.
    write_launcher(runtime, "node", "node/bin/node", default_path=("OPENSSL_MODULES", "node/lib/ossl-modules"))


def prepare_runtime_launchers(runtime, recipe):
    prepare_portable_node(runtime, recipe)
    # Relocation can invalidate upstream bytecode. Do not rewrite the delivery: -B for this
    # interpreter, and the variable for any Python it starts through sys.executable.
    write_launcher(runtime, "python3", "python/bin/python3",
                   set_variable=("PYTHONDONTWRITEBYTECODE", "1"), flag="-B")
    # Not a symlink: git takes its exec-path and templates from the path it was executed by,
    # and the launcher executes the real git/bin/git (runtime-launcher.c).
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
            # Retain the exact source archive with the GPL implementation. No
            # developer checkout, system Git binary or private configuration is copied.
            shutil.copyfile(archive, runtime / "sources/git-2.55.0.tar.xz")
            git_source = entries[0]
            flags = ["prefix=/", "RUNTIME_PREFIX=YesPlease", "NO_GETTEXT=YesPlease",
                     "NO_TCLTK=YesPlease", "NO_PERL=YesPlease", "NO_OPENSSL=YesPlease",
                     "NO_PYTHON=YesPlease", "NO_RUST=YesPlease", "NO_INSTALL_HARDLINKS=YesPlease",
                     *GIT_ICONV_FLAGS,
                     "CFLAGS=-O2 -mmacosx-version-min=13.0 -ffile-prefix-map=" + str(git_source) + "=git-source"]
            # No installed build-directory rpath.
            if manifest.get("portable_git_link_paths"):
                flags.append("CC_LD_DYNPATH=")
            clean_env = git_build_environment(os.environ, subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip())
            check_git_iconv_workaround(git_source, flags, clean_env)
            try:
                with (scratch / "git-build.log").open("w") as log:
                    subprocess.run(["make", "-j4", *flags, "all"], cwd=git_source, env=clean_env, stdout=log, stderr=log, check=True)
                    subprocess.run(["make", *flags, "DESTDIR=" + str(runtime / "git"), "install"], cwd=git_source, env=clean_env, stdout=log, stderr=log, check=True)
            except subprocess.CalledProcessError:
                print((scratch / "git-build.log").read_text()[-10000:], flush=True)
                raise
            shutil.copyfile(git_source / "COPYING", runtime / "sources/GIT-COPYING")
        prepare_runtime_launchers(runtime, manifest)
        scripts = script_entries(runtime / "bin")
        if scripts:
            raise RuntimeError("runtime/bin must hold only Mach-O executables (a #! script there leaks "
                               "kernel memory on every exec): " + ", ".join(scripts))
        # License files from binary upstream distributions are retained in full.
        (runtime / "sources/README.txt").write_text("Pinned upstream distributions are listed in runtime-sources.json. Git's exact corresponding source and COPYING are included. Python and Node license notices remain in their distributions. jq is distributed under the MIT license; see https://github.com/jqlang/jq/blob/jq-1.8.2/COPYING. whisper-cli is built from the pinned whisper.cpp source and is distributed under the MIT license in WHISPER-CPP-LICENSE. ffmpeg and ffprobe are an unmodified upstream static build distributed under GPLv3 or later in FFMPEG-COPYING.GPLv3; FFMPEG-SOURCE.txt says where their exact corresponding source is. macOS supplies bash and system libraries.\n")
        shutil.copyfile(__file__, runtime / "sources/RICHOS-RUNTIME-BUILDER.py")
        shutil.copyfile(LAUNCHER_SOURCE, runtime / "sources/RICHOS-RUNTIME-LAUNCHER.c")
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
        if SYSTEM_ICONV not in subprocess.check_output(["otool", "-L", str(runtime / "git/bin/git")], text=True):
            raise RuntimeError(f"git/bin/git does not link {SYSTEM_ICONV}, the iconv that has UTF-8-MAC")
        print("git precomposed unicode OK: " + check_git_precompose(runtime / "bin/git", env), flush=True)
        for line in check_launched_entries(runtime):
            print("launched entry OK: " + line, flush=True)
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

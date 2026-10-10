"""Durable plans and provenance for retries through the existing proof runner.

Engine green remains a ci-receipts verdict. This module selects an immutable
receipt for each obligation; it never edits one or converts a wrapper exit into
an engine verdict. Input contracts are explicit and unqualified work stays fresh.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import tempfile


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def file_digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def atomic(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream, indent=2, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


# A BUILD CACHE IS A CHECK'S OUTPUT, NEVER ANOTHER CHECK'S INPUT (2026-09-29).
#
# The land of cc/zach-opus-autocheck1 into main (6ef73abf) was refused with
# `no-foreign-app-data` state `invalid`, "execution inputs changed during the check", while its
# own output said "3 passed, 0 failed". Its declared inputs include `richos/app/crates`; the
# lint check running beside it ran Clippy on `crates/richos-user-update`, a standalone Cargo
# package whose default target directory is `crates/richos-user-update/target`, and 536 MB of
# build products written there changed the reader's fingerprint (09:39:27, the rmeta files).
# The reader never looks inside it (foreign_app_data.py skips `target`), and nothing is
# specific to that pair: any check that builds into the tree invalidated any check whose
# declared root holds the build directory, during the check or, through finalize(), after it.
#
# So a directory that is BOTH (a) tagged under the Cache Directory Tagging Specification
# (bford.info/cachedir: a `CACHEDIR.TAG` beginning with the signature below, which Cargo writes
# into every target directory it creates, atomically with the directory) AND (b) ignored by git
# with nothing inside it tracked, is left out of every identity: not its bytes, not its
# listing, not its existence. The specification's meaning of the tag is "regenerable output";
# (b) guarantees no byte git would land is ever skipped. A directory with the tag and without
# (b), or with (b) and without the tag, is bound exactly as before, so the fingerprint still
# binds every source file, untracked fixture, bytecode file and tool it bound.
CACHEDIR_SIGNATURE = b"Signature: 8a477f597d28d172789f06886806bc55"


# A CHECK'S OWN TEST OUTPUT IS NEVER ANOTHER CHECK'S INPUT EITHER (2026-09-29).
#
# The push of main at c7491c34 was refused with `no-foreign-app-data` state `invalid`,
# "execution inputs changed during the check; changed: richos/app/ui/tests/.shots/...": its
# declared inputs include `richos/app/ui`, and the UI suites running beside it write their
# per-run screenshots into `ui/tests/.shots/`. Alone it passes 3 of 3. Those directories are
# output by the repository's own declaration (their .gitignore rules say so: "Screenshot
# evidence for ONE run", shard receipts are "Evidence OF a run, never an input to one", the
# vouch download is a digest-verified cache), but the writers are node suites that do not tag
# what they write, so build_cache() could not see it. Tagging from the writers would not fix
# it either: the first run to add a tag changes the directory it tags, mid-run.
#
# So these paths, relative to the repository root, are left out of every identity on the
# same safety condition as a build cache: git ignores the directory and nothing inside it is
# tracked. A directory that fails that condition is bound exactly as before.
DECLARED_OUTPUT_DIRECTORIES = (
    "richos/app/ui/tests/.shots",
    "richos/app/ui/tests/receipts",
    "richos/app/ui/tests/.vouch",
    # The reference pictures a UI gate's self-test creates and removes inside the tests tree
    # (gate-honesty.js, tracked-tree.js), one `<pid>` folder per run (recheck R07 v2: a
    # `shots-gate-honesty-<pid>` folder beside them was an unignored input, so a proof that
    # overlapped it lost a passing result when the gate cleaned up).
    "richos/app/ui/tests/.generated-references",
    # The engine's own runtime record (2026-10-05). Hooks append to <entity>/.claude/state/ and
    # a suite that seated its hooks on the engine wrote there; 71 engine checks declare the
    # whole `richos/engine`, so one appended line made every engine check beside the writer
    # "invalid: execution inputs changed during the check" (501 finished results, 26,512 s, in
    # 34 of 211 merge attempts; richos-hq docs/operations/2026-10-04-merge-check-speed.md). It
    # is the live plugin's audit state, written by sessions as well as tests, and never an input
    # of a check: ci-shard.sh now fails, by name, a unit that writes it (STATE-WRITTEN).
    "richos/engine/.claude/state",
)


def declared_output(path):
    """True for a DECLARED_OUTPUT_DIRECTORIES directory that git ignores and tracks nothing in."""
    path = Path(path)
    location = str(path.absolute())
    if not any(location.endswith(os.sep + rel.replace("/", os.sep)) for rel in DECLARED_OUTPUT_DIRECTORIES):
        return False
    try:
        if path.is_symlink() or not path.is_dir():
            return False
    except OSError:
        return False
    return ignored_and_untracked(path)


def excluded(path):
    """Output, never input: a tagged build cache or a declared test-output directory."""
    return build_cache(path) or declared_output(path) or bytecode_cache(path)


def bytecode_cache(path):
    """True for a `__pycache__` directory that git ignores and tracks nothing in.

    Compiled bytecode is derived from the source beside it and Python recompiles it when the
    source changes, so it is output, never an input (2026-09-30: a hook test wrote
    hooks/__pycache__/guard-idle-land.cpython-314.pyc and six checks whose declared inputs held
    that directory became "invalid: execution inputs changed during the check"). The same
    safety condition as a build cache: a directory git would land is still bound."""
    path = Path(path)
    if path.name != "__pycache__":
        return False
    try:
        if path.is_symlink() or not path.is_dir():
            return False
    except OSError:
        return False
    return ignored_and_untracked(path)


def build_cache(path):
    """True for a git-ignored, untracked directory tagged as a cache (see above)."""
    path = Path(path)
    tag = path / "CACHEDIR.TAG"
    try:
        if not path.is_dir() or tag.is_symlink() or not tag.is_file():
            return False
        with open(tag, "rb") as stream:
            if stream.read(len(CACHEDIR_SIGNATURE)) != CACHEDIR_SIGNATURE:
                return False
    except OSError:
        return False
    return ignored_and_untracked(path)


def ignored_and_untracked(path):
    """True when git ignores the directory `path` and tracks nothing inside it."""
    path = Path(path)
    # Never the hook's GIT_DIR/GIT_INDEX_FILE: the question is about the checkout on disk.
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    def git(*args):
        return subprocess.run(["git", "-C", str(path.parent), *args], env=env, stdin=subprocess.DEVNULL,
                              capture_output=True, timeout=30)
    try:
        ignored = git("check-ignore", "-q", "--", path.name)
        if ignored.returncode != 0:
            return False  # not ignored, or no repository here: bound like any directory
        tracked = git("ls-files", "-z", "--", path.name)
        return tracked.returncode == 0 and not tracked.stdout
    except (OSError, subprocess.SubprocessError):
        return False


def path_identity(path, ancestors=()):
    """Includes inventories, absence, executable modes and untracked fixture data.

    Build caches inside a directory (build_cache above) are not part of its identity."""
    path = Path(path)
    location = str(path.absolute())
    if location in ancestors:
        raise ValueError(f"cyclic input link: {path}")
    ancestors = (*ancestors, location)
    if path.is_symlink():
        target = os.readlink(path)
        return {"link": target, "target": path_identity(Path(os.path.normpath(path.parent / target)), ancestors)}
    if not path.exists():
        return {"absent": True}
    if path.is_file():
        return {"sha256": file_digest(path), "mode": path.stat().st_mode & 0o777}
    if path.is_dir():
        return {entry.name: path_identity(entry, ancestors) for entry in sorted(path.iterdir())
                if not excluded(entry)}
    raise ValueError(f"unsupported input type: {path}")


def identity_differences(before, after, prefix):
    """The paths whose path_identity() differs, for a note a reader can act on."""
    def leaf(value):
        return (not isinstance(value, dict) or not value
                or set(value) <= {"sha256", "mode"} or set(value) == {"absent"} or set(value) <= {"link", "target"})
    if before == after:
        return []
    if leaf(before) or leaf(after):
        return [prefix]
    found = []
    for name in sorted(set(before) | set(after)):
        if before.get(name) != after.get(name):
            found += identity_differences(before.get(name, {"absent": True}), after.get(name, {"absent": True}),
                                          prefix + "/" + name)
    return found


def inventory_identity(path):
    """Bind discovery and existence without hashing unrelated regular-file bytes.

    Links retain their target contents because a reader may traverse them. This
    conservative exception also rejects cycles through the normal path reader.
    """
    path = Path(path)
    if path.is_symlink():
        return {"link": path_identity(path)}
    if not path.exists():
        return {"absent": True}
    mode = path.stat().st_mode & 0o777
    if path.is_dir():
        return {"mode": mode, "directory": {
            p.name: inventory_identity(p) for p in sorted(path.iterdir()) if not excluded(p)}}
    if path.is_file():
        return {"file": True, "mode": mode}
    raise ValueError(f"unsupported inventory input: {path}")


PRIVATE_PROFILE = "private-home-v1"
GIT_FIXTURE = '[user]\n\tname = Verification Fixture\n\temail = verification@example.invalid\n'


def python_runtime(executable):
    """The private profile disables site startup; bind its remaining import roots."""
    program = ('import json,os,sys,sysconfig; print(json.dumps({"version":sys.version,'
               '"paths":[p for p in sys.path if p],"prefix":sys.base_prefix,'
               '"credentials":[os.geteuid(),os.getegid(),sorted(os.getgroups())],'
               '"umask":os.umask(0)}))')
    result = subprocess.run([executable, "-B", "-I", "-S", "-c", program],
                            capture_output=True, text=True, timeout=15, check=True)
    runtime = json.loads(result.stdout)
    # -S excludes site-packages and .pth execution. Source imports ignore bytecode
    # only when it is absent, so include existing bytecode as well as source.
    def inputs(path):
        path = Path(path)
        if path.is_dir() and not path.is_symlink():
            return {p.name: inputs(p) for p in sorted(path.iterdir())
                    if p.name not in ("site-packages", "dist-packages")}
        return path_identity(path)
    runtime["paths"] = {p: digest(inputs(p)) for p in runtime["paths"]}
    framework = Path(runtime["prefix"]) / "Python"
    runtime["framework"] = path_identity(framework)
    return runtime


# A CHECK WITH NO REVIEWED INPUT CONTRACT IS KEYED BY THE WHOLE CHECKOUT (2026-09-29).
#
# Until today such a check was `fresh`: never reused, so every retry ran it again. The land
# of cc/echo-opus-speckle3 ran the same 78-check selection five times on one tree (three
# refused merges, a `--resume` whose help says it preserves valid results, and the push),
# and each time re-ran the ~70 unqualified checks that had already passed on that tree.
# A reviewed contract (proof-inputs.json) says which bytes a check reads, so its result can
# be reused across DIFFERENT trees. Without one, the only safe statement is the strongest:
# nothing git would land differs. So the identity of an unqualified check is:
#   * the checkout's CONTENT: every path git would land, with its mode and blob id, read
#     from the index where the working tree agrees and from the working tree where it does
#     not, plus every untracked file git does not ignore. Never HEAD: a merge being checked
#     and the commit that concludes it are the same content. Any byte change anywhere in the
#     checkout re-runs every unqualified check;
#   * the checkout's path (a result is never shared between worktrees: what git ignores,
#     node_modules and build products, differs between them);
#   * the command (proof-run adds it), the runner's settings, the resolved interpreters, the
#     environment variables named below, and the platform;
#   * the INSTALLED DEPENDENCIES a check runs against (installed_dependencies below).
#
# INSTALLED DEPENDENCIES ARE INPUTS (2026-09-30, hunt part 2, finding 11). This used to say
# that what git ignores and what lives outside the checkout "are the same limits a rerun on
# this Mac has". They are not: a rerun reads the dependency as it is now. A check passed with
# an ignored node_modules package, only that package changed, running the check again failed,
# and the saved pass was reused and finalized as passed. So the identity also binds, by
# content or by install identity, what a check executes that git does not track: every
# ignored `node_modules` directory in the checkout, the Playwright package the UI harness loads
# from outside the checkout (external_playwright, part 2 recheck R11), the Playwright browser
# installs the UI suites launch, and the Rust toolchain `cargo` resolves. It still binds nothing else git
# ignores: bytecode caches, build caches, test output and hook sidecars are written by checks
# and tools as they run, and binding them would invalidate every pass on every run.
WHOLE_CHECKOUT = "whole-checkout-v2"
CHECKOUT_TOOLS = ("bash", "sh", "node", "python3", "git")
CHECKOUT_ENVIRONMENT_NAMES = ("PATH", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TZ", "DEVELOPER_DIR", "SDKROOT")
CHECKOUT_ENVIRONMENT_PREFIXES = ("RICHOS_", "RUN_TESTS_", "NODE_", "NPM_CONFIG_", "PYTHON", "CARGO_",
                                 "RUST", "SCCACHE_", "PLAYWRIGHT_")
# Set per run or per process by the runner and the land hook; never part of an identity.
RUN_SCOPED_ENVIRONMENT = frozenset(("RICHOS_AUTOCHECK_ACTIVE", "RICHOS_TEST_RESULTS_ROOT",
                                    "RICHOS_TEST_DEVICE_RUN_ID", "RICHOS_VERIFICATION_CONTAMINATION",
                                    # Ownership identities, descriptors and output addresses change
                                    # per invocation. Budget sizes, namespaces and semantic settings
                                    # remain bound below; explicit recipes still bind their own keys.
                                    "RICHOS_VERIFICATION_OWNER", "RICHOS_WORKER_BORROW_LOCK",
                                    "RICHOS_PROOF_RUN_SLOT_HELD", "RICHOS_PROOF_RUN_SLOT_FDS",
                                    "RICHOS_UI_TESTS_LEDGER"))
_TOOL_DIGESTS = {}


def _git_blob(data):
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def _worktree_entry(path):
    """(mode, blob id) for a working-tree path as git would store it; None when absent.

    A path listed by git and gone before it is read is absent, never a crash (the same race
    as proof-run.py source_identity(), 2026-09-30)."""
    try:
        if path.is_symlink():
            return "120000", _git_blob(os.fsencode(os.readlink(path)))
        if not path.exists():
            return None
        if not path.is_file():
            raise ValueError("unsupported checkout entry (a nested repository?): " + str(path))
        with open(path, "rb") as stream:
            data = stream.read()
        return ("100755" if path.stat().st_mode & 0o111 else "100644"), _git_blob(data)
    except FileNotFoundError:
        return None


def checkout_content(root):
    """sha256 over the content git would land from the checkout at `root` (see above)."""
    root = Path(root)
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    def listing(*args):
        result = subprocess.run(["git", "-C", str(root), *args], env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=120)
        if result.returncode != 0:
            raise ValueError("cannot read the checkout's content: git " + " ".join(args[:2]))
        return [name for name in result.stdout.split(b"\0") if name]
    entries = {}
    for row in listing("ls-files", "--stage", "-z"):
        meta, name = row.split(b"\t", 1)
        mode, blob, stage = meta.split(b" ")
        entries.setdefault(name, {})[stage] = (mode.decode(), blob.decode())
    differing = set(listing("diff", "--name-only", "--no-renames", "-z"))
    differing |= set(listing("ls-files", "--others", "--exclude-standard", "-z"))
    for name in differing:
        entry = _worktree_entry(root / os.fsdecode(name))
        entries[name] = {b"0": entry} if entry else {}
    digest = hashlib.sha256()
    for name in sorted(entries):
        for stage, (mode, blob) in sorted(entries[name].items()):
            digest.update(b"%s\0%s\0%s\0%s\n" % (name, stage, mode.encode(), blob.encode()))
    return digest.hexdigest()


# What an install is, by the name of the directory it lives in. A directory of this name that
# git ignores and tracks nothing in is bound by content (path_identity, which still leaves out a
# tagged build cache inside it).
INSTALLED_DEPENDENCY_DIRECTORIES = ("node_modules",)
# A Playwright browser install is a `<browser>-<revision>` directory (webkit-2311,
# chromium_headless_shell-1234, ffmpeg-1011). Anything else beside them (a browser profile
# another tool keeps there, the CLI's update-check file) is not an install and is not bound.
PLAYWRIGHT_INSTALL = re.compile(r"[a-z][a-z0-9_]*-[0-9]+")


def checkout_installs(root):
    """{relative path: digest} for every ignored, untracked dependency install in the checkout."""
    root = Path(root)
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    result = subprocess.run(["git", "-C", str(root), "ls-files", "--others", "--ignored", "--exclude-standard",
                             "--directory", "-z"], env=env, stdin=subprocess.DEVNULL, capture_output=True,
                            timeout=120)
    if result.returncode != 0:
        raise ValueError("cannot list the checkout's installed dependencies: git ls-files --ignored")
    found = {}
    for raw in result.stdout.split(b"\0"):
        name = os.fsdecode(raw).rstrip("/")
        if not name or Path(name).name not in INSTALLED_DEPENDENCY_DIRECTORIES:
            continue
        path = root / name
        if path.is_dir() and not path.is_symlink():
            found[name] = digest(path_identity(path))
    return found


def playwright_installs(environment):
    """The Playwright browser installs a check would launch, by install identity.

    Bind the recursive install metadata, including ctime, so in-place writes and signing
    changes invalidate passes even when the revision directory survives. This is input
    drift detection, not content authentication. It avoids reading gigabytes per check.
    PLAYWRIGHT_BROWSERS_PATH=0 is already covered by checkout_installs."""
    configured = environment.get("PLAYWRIGHT_BROWSERS_PATH")
    if configured == "0":
        return {"inside": "node_modules"}
    if configured:
        base = Path(configured)
    else:
        home = Path(environment.get("HOME") or Path.home())
        base = home / ("Library/Caches" if platform.system() == "Darwin" else ".cache") / "ms-playwright"
    try:
        entries = sorted(os.scandir(base), key=lambda entry: entry.name)
    except FileNotFoundError:
        return {"base": str(base), "absent": True}
    installs = {}
    for entry in entries:
        if PLAYWRIGHT_INSTALL.fullmatch(entry.name) and entry.is_dir(follow_symlinks=False):
            rows = []
            for directory, dirs, files in os.walk(entry.path, followlinks=False):
                for name in sorted(dirs + files):
                    path = Path(directory) / name
                    info = path.lstat()
                    rows.append([str(path.relative_to(entry.path)), info.st_mode, info.st_ino,
                                 info.st_size, info.st_mtime_ns, info.st_ctime_ns,
                                 os.readlink(path) if path.is_symlink() else None])
            installs[entry.name] = {"inode": entry.inode(), "metadata": digest(sorted(rows)),
                                    "complete": os.path.exists(os.path.join(entry.path, "INSTALLATION_COMPLETE"))}
    return {"base": str(base), "installs": installs}


# THE UI SUITES' PLAYWRIGHT PACKAGE IS AN INSTALLED DEPENDENCY WHEREVER IT LIVES (part 2 recheck,
# R11). richos/app/ui/tests/lib/harness.js loadPlaywright deliberately shares one install per
# machine: it takes the package from RICHOS_PLAYWRIGHT, then from Node's own lookup, then from
# the main checkout's install, so a worktree needs no `node_modules` of its own. Only the
# checkout's own installs were bound (checkout_installs), so a pass saved in a worktree was
# reused after the shared package changed at the same path. The sharing stays; what it can load
# is bound instead.
UI_TESTS = Path("richos/app/ui/tests")
PLAYWRIGHT_PACKAGE = "playwright"


def _main_checkout(root):
    """The main checkout of the repository `root` is a linked worktree of, or None."""
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    try:
        result = subprocess.run(["git", "-C", str(root), "rev-parse", "--path-format=absolute",
                                 "--git-common-dir", "--show-toplevel"], env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    lines = result.stdout.splitlines()
    if result.returncode != 0 or len(lines) != 2:
        return None
    main, top = Path(lines[0]).parent, Path(lines[1])
    return None if main.resolve() == top.resolve() else main


def playwright_package_candidates(root, environment):
    """Every path harness.js may load the Playwright package from, in the order it tries them.

    RICHOS_PLAYWRIGHT first (a path, or a name Node looks up); then `require("playwright")`
    from the harness directory: every ancestor's node_modules, each NODE_PATH entry and Node's
    global folders ($HOME/.node_modules, $HOME/.node_libraries, <node prefix>/lib/node); then
    the main checkout's install when `root` is a linked worktree.

    Each is (package path, module directory): the directory Node found it in, whose other
    packages its own `require`s also resolve from (playwright loads playwright-core that way).
    A path named outright has no such directory (None): its nearest node_modules is used."""
    root = Path(root)
    harness = root / UI_TESTS / "lib"
    node = shutil.which("node", path=environment.get("PATH"))

    def lookup(name):
        folders = [directory / "node_modules" for directory in (harness, *harness.parents)
                   if directory.name != "node_modules"]
        folders += [Path(entry) for entry in (environment.get("NODE_PATH") or "").split(os.pathsep) if entry]
        if environment.get("HOME"):
            folders += [Path(environment["HOME"]) / ".node_modules", Path(environment["HOME"]) / ".node_libraries"]
        if node:
            folders.append(Path(os.path.realpath(node)).parent.parent / "lib" / "node")
        return [(folder / name, folder) for folder in folders]

    candidates = []
    named = environment.get("RICHOS_PLAYWRIGHT")
    if named:
        if os.path.isabs(named):
            candidates.append((Path(named), None))
        elif named in (".", "..") or named.startswith(("./", "../")):
            candidates.append((harness / named, None))
        else:
            candidates += lookup(named)
    candidates += lookup(PLAYWRIGHT_PACKAGE)
    main = _main_checkout(root)
    if main is not None:
        installed = main / UI_TESTS / "node_modules"
        candidates.append((installed / PLAYWRIGHT_PACKAGE, installed))
    return candidates


def _node_module_root(package):
    """For a package named by path: its nearest enclosing node_modules, which its own
    `require`s resolve against, or the package alone when it is installed outside one."""
    for parent in package.parents:
        if parent.name == "node_modules":
            return parent
    return package


def external_playwright(root, environment):
    """{candidate: identity} for every Playwright package harness.js may load outside the checkout.

    A present candidate is bound by the content of the module directory it was found in
    (path_identity, the same reader checkout_installs uses); an absent one by its absence, so an
    install that appears where the harness looks first is a different input. Every present
    candidate is bound, not only the first: which one loads depends on whether the ones before it
    load, and over-binding costs only a rerun. A node_modules inside the checkout is the
    checkout's content or one of checkout_installs, and is not bound twice; a symlink out of the
    checkout is followed, so what it points at is bound here."""
    root = Path(root).resolve()
    inside = lambda path: path == root or root in path.parents
    found, roots = {}, {}
    for candidate, folder in playwright_package_candidates(root, environment):
        real = Path(os.path.realpath(candidate))
        if folder is not None and Path(folder).name == "node_modules" and inside(real):
            continue
        # What `require` accepts at that path: the path itself, or it with Node's extensions.
        target = next((path for path in (Path(str(real) + suffix) for suffix in ("", ".js", ".json", ".node"))
                       if path.exists()), None)
        if target is None:
            found[str(candidate)] = {"absent": True}
            continue
        module_root = Path(os.path.realpath(folder)) if folder is not None else _node_module_root(target)
        if module_root.name == "node_modules" and inside(module_root):
            continue
        if str(module_root) not in roots:
            roots[str(module_root)] = digest(path_identity(module_root))
        found[str(candidate)] = {"root": str(module_root), "sha256": roots[str(module_root)]}
    return found


def rust_toolchain(root, environment):
    """What `rustc -vV` says in the checkout: a rustup shim's bytes do not change with the toolchain."""
    rustc = shutil.which("rustc", path=environment.get("PATH"))
    if not rustc:
        return {"absent": True}
    # Never let asking install a toolchain: a missing one is an identity of its own.
    env = dict(environment, RUSTUP_AUTO_INSTALL="0")
    try:
        result = subprocess.run([rustc, "-vV"], cwd=str(root), env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=60)
    except (OSError, subprocess.SubprocessError) as exc:
        raise ValueError("cannot read the Rust toolchain: " + str(exc)) from None
    identity = {"path": rustc, "exit": result.returncode, "sha256": hashlib.sha256(result.stdout).hexdigest()}
    wrapper = environment.get("RUSTC_WRAPPER") or environment.get("CARGO_BUILD_RUSTC_WRAPPER")
    if wrapper:
        resolved = shutil.which(wrapper, path=environment.get("PATH"))
        identity["compiler_wrapper"] = ({"path": resolved, "sha256": _tool_digest(resolved)}
                                        if resolved else {"missing": wrapper})
    return identity


INSTALL_ENVIRONMENT_NAMES = ("PATH", "HOME", "PLAYWRIGHT_BROWSERS_PATH", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN",
                             "CARGO_HOME", "RICHOS_PLAYWRIGHT", "NODE_PATH")


def installed_dependencies(root, environment):
    """What a check executes that git does not track (WHOLE_CHECKOUT above)."""
    return {"checkout": checkout_installs(root), "playwright": playwright_installs(environment),
            "playwright_package": external_playwright(root, environment),
            "rust": rust_toolchain(root, environment)}


def _tool_digest(path):
    """A resolved tool's content digest, read again whenever its file changes."""
    real = os.path.realpath(path)
    info = os.stat(real)
    key = (real, info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_ino)
    if key not in _TOOL_DIGESTS:
        _TOOL_DIGESTS[key] = file_digest(real)
    return _TOOL_DIGESTS[key]


def checkout_identity(root, argv, environment, snapshot=None):
    """The identity of a check with no reviewed input contract (WHOLE_CHECKOUT above)."""
    snapshot = snapshot or InputSnapshot()
    root = Path(root).resolve()
    names = list(CHECKOUT_TOOLS)
    if argv and os.sep not in argv[0] and argv[0] not in names:
        names.append(argv[0])
    tools = {}
    for name in names:
        resolved = shutil.which(name, path=environment.get("PATH"))
        tools[name] = {"path": resolved, "sha256": _tool_digest(resolved)} if resolved else {"absent": True}
    values = {name: digest(value) for name, value in sorted(environment.items())
              if name not in RUN_SCOPED_ENVIRONMENT and (name in CHECKOUT_ENVIRONMENT_NAMES
                                                          or name.startswith(CHECKOUT_ENVIRONMENT_PREFIXES))}
    return {"contract": WHOLE_CHECKOUT, "checkout": str(root), "tree": snapshot.checkout(root),
            "installed": snapshot.installed(root, environment),
            "paths": {}, "tools": tools, "profile": None, "inventories": {}, "external_paths": {},
            "git_inputs": {}, "environment": values, "external": {},
            "platform": [platform.system(), platform.release(), platform.machine(), platform.mac_ver()[0], os.cpu_count()]}


class InputSnapshot:
    """Share identical reads within one validation pass, never across passes.

    The caller creates a new instance for planning, finalization or coverage.
    There is no persistent metadata cache: changed bytes, inventories and tools
    are read again at the next boundary even if their stat fields are unchanged.
    """
    def __init__(self):
        self.paths = {}
        self.runtimes = {}
        self.inventories = {}
        self.checkouts = {}
        self.installs = {}

    def checkout(self, root):
        key = str(root)
        if key not in self.checkouts:
            self.checkouts[key] = checkout_content(root)
        return self.checkouts[key]

    def installed(self, root, environment):
        key = (str(root), *(environment.get(name) for name in INSTALL_ENVIRONMENT_NAMES))
        if key not in self.installs:
            self.installs[key] = installed_dependencies(root, environment)
        return self.installs[key]

    def path(self, path):
        key = str(Path(path).absolute())
        if key not in self.paths:
            self.paths[key] = path_identity(path)
        return self.paths[key]

    def runtime(self, executable):
        if executable not in self.runtimes:
            self.runtimes[executable] = python_runtime(executable)
        return self.runtimes[executable]

    def inventory(self, path):
        key = str(Path(path).absolute())
        if key not in self.inventories:
            self.inventories[key] = inventory_identity(path)
        return self.inventories[key]


def prepare_environment(item, root, logdir, environment, create=True):
    recipe = contract_for(root, item.label)
    if not recipe.get("isolation"):
        return
    if recipe["isolation"] != PRIVATE_PROFILE:
        raise ValueError("unsupported execution profile: " + recipe["isolation"])
    private = Path(logdir).resolve() / "fixtures" / digest(item.label)
    home, tmp = private / "home", private / "tmp"
    tool_path = environment["PATH"]
    python = shutil.which("python3", path=tool_path)
    if not python:
        raise ValueError("private execution profile requires python3")
    if create:
        home.mkdir(parents=True, mode=0o700)
        tmp.mkdir(mode=0o700)
        (home / ".claude/state").mkdir(parents=True)
        # Allocation appends operational bookkeeping. Seed its file before the
        # record canary witnesses this directory; protected records stay absent.
        (home / ".claude/state/scratch-ledger.jsonl").touch(exist_ok=False)
        (home / ".gitconfig").write_text(GIT_FIXTURE)
        (private / "bin").mkdir(mode=0o700)
        (private / "git-template").mkdir(mode=0o700)
        wrapper = private / "bin/python3"
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(python) + ' -s -S "$@"\n')
        wrapper.chmod(0o700)
    base = {"PATH": str(private / "bin") + os.pathsep + tool_path,
        "RICHOS_VERIFICATION_TOOL_PATH": tool_path, "HOME": str(home), "TMPDIR": str(tmp),
        "CLAUDE_CONFIG_DIR": str(home / ".claude"), "LANG": "C", "LC_ALL": "C", "TZ": "UTC",
        "PYTHONDONTWRITEBYTECODE": "1", "PYTHONNOUSERSITE": "1", "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": str(home / ".gitconfig"), "GIT_TEMPLATE_DIR": str(private / "git-template"),
        "RICHOS_VERIFICATION_FIXTURE_ROOT": str(private), "RICHOS_VERIFICATION_MODE": "fixture"}
    # Only named stable inputs enter this profile. Per-run ownership/worker fields
    # are attached by launch(), after this environment has been fingerprinted.
    for name in set(recipe["environment"]) | set(recipe["external"]):
        if name in environment and name not in base:
            base[name] = environment[name]
    item.private_environment = base
    item.env = {key: value for key, value in item.env.items() if key in base}


class UnqualifiedReader(ValueError):
    """Changed reviewed code must execute fresh until its input review is renewed."""


def qualify_recipe(root, recipe):
    """Check the execution recipe against its independently reviewed read contract.

    This is a finite input floor, not inference about arbitrary programs. Removing
    a known tool or outside-root dependency must fail before reuse or execution.
    """
    root = Path(root)
    def relative(value):
        if not isinstance(value, str) or not value or Path(value).is_absolute() or ".." in Path(value).parts:
            raise ValueError("input must be repository-relative: " + str(value))
        return value
    qualification = relative(recipe["qualification"])
    if not (root / qualification).is_file():
        raise ValueError("input qualification is missing: " + qualification)
    contract = json.loads((root / qualification).read_text())
    fields = {"paths", "tools", "environment", "external"}
    if (not isinstance(contract, dict) or contract.get("schema") != 1
            or not isinstance(contract.get("requires"), dict)
            or set(contract["requires"]) != fields):
        raise ValueError("invalid reviewed input qualification: " + qualification)
    if contract.get("isolation") and recipe.get("isolation") != contract["isolation"]:
        raise ValueError("input qualification requires isolation: " + contract["isolation"])
    review = relative(contract.get("review"))
    if not (root / review).is_file():
        raise ValueError("input review is missing: " + review)
    unit = recipe.get("qualification_unit")
    extra = None
    if unit is not None:
        extra = contract.get("units", {}).get(unit)
        if (not isinstance(extra, dict) or not extra.get("review")
                or not extra.get("sources") or set(extra.get("requires", {})) != fields):
            raise ValueError("missing reviewed unit qualification: " + str(unit))
        for source, expected in extra["sources"].items():
            source = relative(source)
            if not (root / source).is_file() or file_digest(root / source) != expected:
                raise UnqualifiedReader("changed unit reader requires qualification: " + source)
    for field in sorted(fields):
        for values in (recipe[field], contract["requires"][field]):
            if not isinstance(values, list) or any(not isinstance(v, str) or not v for v in values):
                raise ValueError("invalid input list: " + field)
        required = list(contract["requires"][field])
        if extra is not None:
            values = extra["requires"][field]
            if not isinstance(values, list) or any(not isinstance(v, str) or not v for v in values):
                raise ValueError("invalid unit input list: " + field)
            required.extend(values)
            if field == "paths":
                required.extend(extra["sources"])
        declared = recipe[field]
        if field == "paths":
            for path in [*required, *declared]:
                relative(path)
            missing = [path for path in required if not any(
                Path(path).is_relative_to(parent) for parent in declared)]
        else:
            missing = sorted(set(required) - set(declared))
        if missing:
            raise ValueError("input qualification omits " + field + ": " + ", ".join(missing))
    external_paths = recipe.get("external_paths", [])
    required_external = contract.get("external_paths", [])
    if extra is not None:
        required_external = [*required_external, *extra.get("external_paths", [])]
    for values in (external_paths, required_external):
        if (not isinstance(values, list) or any(not isinstance(v, str) or not Path(v).is_absolute()
                                               or ".." in Path(v).parts for v in values)):
            raise ValueError("external paths must be absolute literals")
    missing = sorted(set(required_external) - set(external_paths))
    if missing:
        raise ValueError("input qualification omits external_paths: " + ", ".join(missing))
    git_inputs = recipe.get("git_inputs", {})
    git_required = [contract.get("git_inputs", {})]
    if extra is not None:
        git_required.append(extra.get("git_inputs", {}))
    for declaration in [git_inputs, *git_required]:
        if not isinstance(declaration, dict) or set(declaration) - {"tracked", "last_change"}:
            raise ValueError("invalid Git input declaration")
        for values in declaration.values():
            if not isinstance(values, list):
                raise ValueError("invalid Git input paths")
            for value in values:
                relative(value)
    for declaration in git_required:
        for kind, required_paths in declaration.items():
            missing = sorted(set(required_paths) - set(git_inputs.get(kind, [])))
            if missing:
                raise ValueError("input qualification omits git_inputs " + kind + ": " + ", ".join(missing))
    if git_inputs and "git" not in recipe["tools"]:
        raise ValueError("Git inputs require the git tool identity")
    if "subset" in recipe:
        subset = recipe["subset"]
        reviewed = extra.get("subset") if extra else None
        common = contract.get("subset_requires")
        if (not isinstance(reviewed, dict) or not reviewed.get("review")
                or not isinstance(common, dict)):
            raise ValueError("missing reviewed subset qualification")
        for declaration in (subset, common, reviewed.get("requires")):
            if not isinstance(declaration, dict) or set(declaration) != {"paths", "inventories"}:
                raise ValueError("invalid subset input declaration")
            for values in declaration.values():
                if not isinstance(values, list):
                    raise ValueError("invalid subset input list")
                for value in values:
                    relative(value)
        for field in ("paths", "inventories"):
            floor = [*common[field], *reviewed["requires"][field]]
            if field == "paths":
                floor.extend(extra["sources"])
                floor.extend(extra["requires"]["paths"])
            missing = [p for p in floor if not any(
                Path(p).is_relative_to(parent) for parent in subset[field])]
            if missing:
                raise ValueError("subset qualification omits " + field + ": " + ", ".join(missing))
        # A subset cannot secretly introduce a new outside-root input while
        # claiming that its Tier 1 parent declaration already covered it.
        for path in [*subset["paths"], *subset["inventories"]]:
            if not any(Path(path).is_relative_to(parent) for parent in recipe["paths"]):
                raise ValueError("subset input is outside declared roots: " + path)
    return [qualification, review]


def git_input_identity(root, declaration, environment):
    """Read only the qualified index/history inputs, without keying on HEAD.

    File contents are independently covered by paths. Tracked membership and
    commit timestamps can change while those bytes stay identical. Whole-repo
    HEAD would instead invalidate results for unrelated documentation commits.
    """
    if not declaration:
        return {}
    executable = shutil.which("git", path=environment.get("PATH"))
    if not executable:
        raise ValueError("Git input identity requires an available git")
    def read(*args, absent=False):
        result = subprocess.run([executable, "-C", str(root), *args], env=environment,
                                capture_output=True, timeout=10)
        if result.returncode != 0 and not (absent and result.returncode == 1):
            raise ValueError("cannot read qualified Git input: " + " ".join(args))
        return {"exit": result.returncode, "sha256": hashlib.sha256(result.stdout).hexdigest()}
    identity = {}
    if declaration.get("tracked"):
        identity["tracked"] = read("ls-files", "--stage", "-z", "--", *declaration["tracked"])
        # The packaging suite also uses line-oriented ls-files. Its quoting is
        # controlled by local repository config even under the private profile.
        identity["quote_path"] = read("config", "--get", "core.quotepath", absent=True)
    if declaration.get("last_change"):
        identity["last_change"] = {path: read("log", "-1", "--format=%ct", "--", path)
                                   for path in declaration["last_change"]}
    return identity


def recipe_identity(root, recipe, environment, snapshot=None):
    """Validate the finite declaration, then fingerprint every declared input.

    An absent contract is deliberately not a reusable identity. The declaration
    and its qualification are versioned source inputs, so changing either cannot
    inherit a previous result. Secret environment values are hashed, never logged.
    """
    if recipe.get("fresh"):
        return {"fresh": recipe["fresh"]}
    required = {"paths", "tools", "environment", "external", "qualification"}
    if set(recipe) - {"isolation", "qualification_unit", "external_paths", "git_inputs", "subset"} != required or not recipe["qualification"]:
        raise ValueError("incomplete verification input contract")
    root = Path(root).resolve()
    snapshot = snapshot or InputSnapshot()
    try:
        qualification_paths = qualify_recipe(root, recipe)
    except UnqualifiedReader as exc:
        return {"fresh": str(exc)}
    paths = {}
    selected_paths = recipe.get("subset", {}).get("paths", recipe["paths"])
    for rel in [*selected_paths, *qualification_paths]:
        path = root / rel
        if Path(rel).is_absolute() or ".." in Path(rel).parts:
            raise ValueError(f"input must be repository-relative: {rel}")
        paths[rel] = digest(snapshot.path(path))
    tools = {}
    for name in recipe["tools"]:
        resolved = shutil.which(name, path=environment.get("RICHOS_VERIFICATION_TOOL_PATH", environment.get("PATH")))
        tools[name] = ({"path": resolved, "input": snapshot.path(resolved)}
                       if resolved else {"absent": True})
        if name == "python3" and resolved and recipe.get("isolation"):
            tools[name]["runtime"] = snapshot.runtime(resolved)
    external = {}
    for name in recipe["external"]:
        value = environment.get(name)
        external[name] = digest(snapshot.path(value)) if value else {"unset": True}
    values = {name: digest(environment[name]) if name in environment else None
              for name in recipe["environment"]}
    profile = None
    if recipe.get("isolation"):
        if recipe["isolation"] != PRIVATE_PROFILE:
            raise ValueError("unsupported execution profile")
        private = environment.get("RICHOS_VERIFICATION_FIXTURE_ROOT")
        if not private or environment.get("HOME") != str(Path(private) / "home"):
            raise ValueError("qualified private execution environment is absent")
        names = {"PATH", "HOME", "TMPDIR", "CLAUDE_CONFIG_DIR", "LANG", "LC_ALL", "TZ",
                 "PYTHONDONTWRITEBYTECODE", "PYTHONNOUSERSITE", "GIT_CONFIG_NOSYSTEM", "GIT_CONFIG_GLOBAL",
                 "GIT_TEMPLATE_DIR", "RICHOS_VERIFICATION_TOOL_PATH", "RICHOS_VERIFICATION_FIXTURE_ROOT",
                 "RICHOS_VERIFICATION_MODE",
                 *recipe["environment"], *recipe["external"]}
        values = {name: digest(environment[name].replace(private, "$FIXTURE"))
                  if name in environment else None for name in names}
        profile = {"name": PRIVATE_PROFILE, "git_fixture": digest(GIT_FIXTURE),
                   "scratch_ledger_seed": digest("")}
    literal_external = {path: digest(snapshot.path(path)) for path in recipe.get("external_paths", [])}
    return {"contract": digest(recipe), "paths": paths, "tools": tools, "profile": profile,
            "inventories": {path: digest(snapshot.inventory(root / path))
                            for path in recipe.get("subset", {}).get("inventories", [])},
            "external_paths": literal_external,
            "git_inputs": git_input_identity(root, recipe.get("git_inputs"), environment),
            "environment": values, "external": external,
            "platform": [platform.system(), platform.release(), platform.machine(), platform.mac_ver()[0], os.cpu_count()]}


def contract_for(root, label):
    path = Path(root) / "richos/app/scripts/proof-inputs.json"
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        return {"fresh": "no committed input contract"}
    if data.get("schema") != 1 or not isinstance(data.get("checks"), dict):
        raise ValueError("invalid proof-inputs.json")
    return data["checks"].get(label, {"fresh": "input qualification not yet declared for " + label})


def encode_item(item, root, logdir):
    def relative(arg):
        prefix = str(Path(logdir).resolve()) + os.sep
        return "$RUN/" + arg[len(prefix):] if arg.startswith(prefix) else arg
    return {"check": item.label, "cwd": os.path.relpath(item.cwd, root),
            "argv": [relative(arg) for arg in item.argv], "lane": item.lane,
            "weight": item.weight, "after": sorted(item.after), "requires": sorted(item.requires)}


def decode_item(row, factory, root, logdir):
    argv = [str(Path(logdir) / arg[5:]) if arg.startswith("$RUN/") else arg for arg in row["argv"]]
    return factory(row["check"], str(Path(root) / row["cwd"]), argv,
                   row["lane"], row["weight"], row["after"], row["requires"])


def command_identity(item, root, logdir):
    # Placement and predicted duration are scheduling metadata. They do not
    # change an exact unit's command, assertions or declared execution inputs.
    row = encode_item(item, root, logdir)
    return {key: row[key] for key in ("check", "cwd", "argv")}


def receipt_path(item):
    if item.engine_unit:
        return Path(item.argv[item.argv.index("--receipt") + 1])
    return None


def completed_receipt(item, sha, allow_known_red=False):
    path = receipt_path(item)
    if path is None:
        return None
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    unit = item.argv[item.argv.index("--only-units") + 1]
    if (len(rows) != 1 or rows[0].get("unit") != unit or rows[0].get("sha") != sha
            or rows[0].get("schema") != 2 or rows[0].get("execution_status") != "completed"
            or not (rows[0].get("verdict") == "PASS" and rows[0].get("rc") == rows[0].get("expected_rc")
                    or allow_known_red and rows[0].get("verdict") == "KNOWN-RED")):
        raise ValueError("receipt is not a completed PASS for " + unit)
    return {"path": str(path), "sha256": file_digest(path)}


# A CHANGE INVALIDATES ONLY THE CHECKS WHOSE INPUTS IT TOUCHES (2026-09-29, part-2 hunt
# section 07).
#
# `source` is the whole checkout: HEAD, the tracked diff and every untracked file git does not
# ignore. It used to decide for every check at once: any change to it during a run marked the
# run contaminated, stopped every running and waiting check, and invalidated every pass,
# including passes whose own declared inputs had not changed by a byte. So one editor save
# anywhere in the checkout (a doc, an unrelated script) refused a land and threw away its
# evidence. Now each check is judged by its OWN identity, which already says what it reads:
#   * a check with a reviewed contract (proof-inputs.json): its declared paths, tools,
#     environment and Git inputs. A change outside them leaves its pass standing;
#   * a check with no contract is keyed by the whole checkout's content (WHOLE_CHECKOUT), so for
#     it every change IS an input change and its pass is invalidated, on its own, without
#     stopping any other check;
#   * a check with no identity at all (`fresh`: the receipts verifier, or a check with no
#     command) has only the whole source to bind it, so a source change invalidates it
#     exactly as before. An engine unit without a contract was one until 2026-10-02; it now
#     has the whole-checkout identity (proof-run.py input_identity says why).
# What stays global is HEAD: a commit or a checkout switch in the verified checkout is not an
# editor save, receipts are bound to the commit, and it still marks the run contaminated.
#
# One `fresh` check is not bound to the whole source: `engine receipts`. It is fresh so that it
# always runs (it is the coverage verifier over this run's receipts), not because its inputs are
# unknown: verify_target_receipts() re-reads every covered unit's own identity and HEAD itself,
# so its pass is exactly as valid as the units it covers. Binding it to the whole source would
# make one unrelated save fail the coverage of every engine unit in the run.
# The pin validator reads current external bindings and discovered inventories as well as
# tracked readers. It takes under a second and must execute on every attempt, never reuse a
# whole-checkout pass that cannot bind those external bytes.
VERIFIES_OWN_INPUTS = frozenset(("engine receipts", "engine scripts/verification-pins.test.sh"))


class Record:
    def __init__(self, root, logdir, items, source, identities, previous=None,
                 current_source=None, current_identity=None, current_identities=None, explain=None,
                 current_commit=None):
        refuse_duplicates(items)  # before anything is written: a saved plan never holds one
        self.root, self.logdir = str(root), Path(logdir)
        # What changed and who could have changed it, appended to an invalidation note, so a
        # check invalidated by another check's writes names the paths instead of only saying so.
        self.explain = explain or (lambda item: "")
        self.source, self.identities = source, identities
        self.previous = previous
        self.current_source = current_source or (lambda: self.source)
        # HEAD alone, read cheaply while the run goes (the block above says why only HEAD).
        self.current_commit = current_commit or (lambda: self.current_source()["commit"])
        self.current_identity = current_identity or (lambda item: self.identities[item.label])
        self.current_identities = current_identities or (
            lambda selected: {item.label: self.current_identity(item) for item in selected})
        self.source_invalidated = False
        self.lease = Lease(logdir)
        self.plan = {"schema": 1, "root": self.root, "source": source,
                     "items": [encode_item(item, root, logdir) for item in items],
                     "identities": identities, "previous": str(previous) if previous else None}
        atomic(self.logdir / "plan.json", self.plan)
        self.results = {}
        for item in items:
            item.evidence = self
        self.checkpoint(items)

    def checkpoint(self, items):
        for item in items:
            if item.label not in self.identities:
                continue  # Synthetic run findings never become reusable obligations.
            old = self.results.get(item.label, {})
            if old.get("state") == item.state and old.get("exit") == item.rc:
                continue
            self.save(item, self.current_source())

    def save(self, item, source):
        """Called before launch and after completion, independently of the summary."""
        moved = source["commit"] != self.source["commit"]
        if moved:
            self.source_invalidated = True
        identity = self.identities[item.label]
        result = {"state": item.state, "exit": item.rc, "source": source,
                  "attempts": getattr(item, 'attempts', []),
                  "input": identity, "log": item.log,
                  "receipt": None, "seconds": item.seconds, "reused_from": getattr(item, "reused_from", None)}
        if item.state == "passed":
            if moved:
                result["invalid"] = "the checkout's commit changed during execution"
            elif identity.get("fresh") and item.label not in VERIFIES_OWN_INPUTS and source != self.source:
                result["invalid"] = ("source changed during execution (this check declares no inputs, "
                                     "so every change is one of its inputs)")
            else:
                try:
                    if self.current_identity(item) != self.identities[item.label]:
                        raise ValueError("execution inputs changed during the check" + self.explain(item))
                    result["receipt_sha"] = getattr(item, "receipt_sha", source["commit"])
                    result["receipt"] = completed_receipt(item, result["receipt_sha"],
                        allow_known_red=not getattr(item, "reused_from", None))
                    result["log_sha256"] = file_digest(item.log)
                except (OSError, ValueError, KeyError, TypeError) as exc:
                    result["invalid"] = "evidence unavailable: " + str(exc)
        if result.get("invalid"):
            item.state, item.rc = "invalid", 125
            item.notes.append(result["invalid"])
            result.update(state=item.state, exit=item.rc)
        if getattr(item, "provenance", None):
            result["provenance"] = item.provenance
        self.results[item.label] = result
        atomic(self.logdir / "outcomes.json", self.results)

    def close(self):
        self.lease.close()

    def finalize(self, items):
        """Later checks must not invalidate an earlier pass or its copied proof."""
        before = self.current_source()
        selected = [item for item in items if item.state == 'passed' and item.label in self.results]
        input_error = None
        try:
            identities = again = self.current_identities(selected)
            source = self.current_source()
            if source != before:
                # The checkout changed while the inputs were being read. Read them once more: a
                # check whose own inputs read differently the second time was not holding still;
                # one whose inputs read the same both times is judged on them like any other.
                again = self.current_identities(selected)
                source = self.current_source()
        except (OSError, ValueError, KeyError, TypeError) as exc:
            input_error = str(exc)
        for item in items:
            if item.state != "passed" or item.label not in self.results:
                continue
            row = self.results[item.label]
            try:
                if input_error is not None:
                    raise ValueError(input_error)
                if source["commit"] != row["source"]["commit"]:
                    raise ValueError("the checkout's commit changed after this check completed")
                if (row["input"].get("fresh") and item.label not in VERIFIES_OWN_INPUTS
                        and source != row["source"]):
                    raise ValueError("inputs changed after this check completed (it declares no inputs)")
                if identities[item.label] != again[item.label]:
                    raise ValueError("inputs changed while they were being read" + self.explain(item))
                if identities[item.label] != row["input"]:
                    raise ValueError("inputs changed after this check completed" + self.explain(item))
                if file_digest(item.log) != row["log_sha256"]:
                    raise ValueError("completed check log changed")
                if row["receipt"] and file_digest(row["receipt"]["path"]) != row["receipt"]["sha256"]:
                    raise ValueError("completed check receipt changed")
            except (OSError, ValueError, KeyError, TypeError) as exc:
                item.state, item.rc = "invalid", 125
                reason = "final evidence validation: " + str(exc)
                item.notes.append(reason)
                row.update(state=item.state, exit=item.rc, invalid=reason)
        atomic(self.logdir / "outcomes.json", self.results)


def read_plan(path):
    plan = json.loads((Path(path) / "plan.json").read_text())
    if plan.get("schema") != 1:
        raise ValueError("unsupported or missing saved plan schema")
    # A row repeated exactly is one obligation saved twice (plans written before plan() and
    # Record refused it, 2026-10-05: attempt-keq_bkth held `cargo --bin richos-tauri` twice),
    # so it is read once. Two different rows under one name stay refused: which one passed
    # cannot be known, since outcomes are keyed by name.
    rows = {}
    for row in plan["items"]:
        if rows.setdefault(row["check"], row) != row:
            raise ValueError("saved plan contains duplicate obligations")
    plan["items"] = list(rows.values())
    return plan


def refuse_duplicates(items):
    """Outcomes, identities and reuse are keyed by check name: one name is one obligation."""
    seen = set()
    for item in items:
        if item.label in seen:
            raise ValueError("plan contains duplicate obligations: " + item.label)
        seen.add(item.label)


def pool_directory(root, history):
    common = subprocess.check_output(["git", "-C", str(root), "rev-parse",
        "--path-format=absolute", "--git-common-dir"], text=True).strip()
    return Path(history).parent / "input-owners" / digest(os.path.realpath(common))


class Pool:
    """One kernel-held owner per qualified input, pointing to existing evidence.

    There is no cached verdict here. Every hit goes through reuse() and the
    normal target coverage verifier. A completed unit can be joined while its
    original run continues with other units: its own lock is the publication
    boundary. Supervisors inherit it until owned cleanup has finished.
    """
    def __init__(self, directory, record, retry_reason=None):
        self.directory, self.record = Path(directory), record
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.retry_reason = retry_reason
        self.held = {}

    def claim(self, item):
        identity = self.record.identities[item.label]
        if identity.get("fresh") or item.label == "engine receipts":
            return True
        if item.label in self.held:
            return True
        key = digest({"check": item.label, "input": identity})
        fd = os.open(self.directory / (key + ".lock"), os.O_CREAT | os.O_RDWR, 0o600)
        path = self.directory / (key + ".json")
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            try:
                owner = json.loads(path.read_text())
                description = str(owner.get("run", "owner is publishing its identity"))
            except (OSError, ValueError):
                description = "owner is publishing its identity"
            note = "join identical input owner: " + description
            if note not in item.notes:
                item.notes.append(note)
                print("proof-run: %s: %s" % (item.label, note), flush=True)
            return False
        try:
            try:
                previous = json.loads(path.read_text())
            except FileNotFoundError:
                previous = {}
            # Corrupt counters are not absence: refuse rather than reset a budget.
            failures = previous.get("functional_failures", [])
            if not isinstance(failures, list):
                raise ValueError("invalid durable retry history: " + str(path))
            if previous.get("run") and previous["run"] != str(self.record.logdir):
                try:
                    reuse(previous["run"], [item], self.record, exact=False)
                except (OSError, ValueError, KeyError, TypeError) as exc:
                    item.notes.append("prior evidence unavailable: " + str(exc))
            if item.state not in ("waiting", "passed"):
                os.close(fd)
                return False
            # A reviewed contract gets two attempts: the first failure needs --retry-reason
            # with the diagnosis, the second failure ends it. A check with no contract (a
            # whole-checkout identity) keeps the reason that exempted it: a land retried on
            # the same tree after one failure runs again, with no --retry-reason to give
            # (autocheck's land plan has no way to pass one). Its budget starts one failure
            # later: a second failure on the unchanged tree needs the diagnosis, a third ends
            # it. Hunt part 2, finding 12: it used to have none, so a known failure repeated
            # without limit.
            whole = identity.get("contract") == WHOLE_CHECKOUT
            free, limit = (1, 3) if whole else (0, 2)
            if item.state != "passed" and len(failures) > free:
                reason = ("unchanged inputs exhausted their functional retry budget"
                          if len(failures) >= limit else
                          "unchanged failed inputs require --retry-reason with the diagnosis")
                if len(failures) >= limit or not self.retry_reason:
                    item.finish_queue()
                    item.state, item.rc = "blocked", 75
                    item.notes.append(reason + "; prior evidence: " + failures[-1]["run"])
                    os.close(fd)
                    return False
            row = {"run": str(self.record.logdir), "check": item.label,
                   "pid": os.getpid(), "functional_failures": failures,
                   "retry_reason": self.retry_reason, "state": item.state}
            atomic(path, row)
            self.held[item.label] = (fd, path, row)
            item.input_owner_fd = fd
            if item.state == "passed":
                item.finish_queue()
                self.finish(item)
                return False
            return True
        except BaseException:
            os.close(fd)
            raise

    def finish(self, item):
        held = self.held.pop(item.label, None)
        if held is None:
            return
        fd, path, row = held
        try:
            row["state"] = item.state
            if self.functional_failure(item):
                row["functional_failures"].append({"run": str(self.record.logdir),
                    "exit": item.rc, "diagnosis": self.retry_reason})
            atomic(path, row)
        finally:
            # close, not LOCK_UN: an inherited supervisor still owns this flock
            # if the caller is interrupted before descendant cleanup finishes.
            os.close(fd)
            del item.input_owner_fd

    @staticmethod
    def functional_failure(item):
        if item.state != "failed":
            return False
        receipt = receipt_path(item)
        if receipt:
            try:
                rows = [json.loads(line) for line in receipt.read_text().splitlines() if line.strip()]
                return (len(rows) == 1 and rows[0].get("execution_status") == "completed"
                        and rows[0].get("verdict") == "FAIL")
            except (OSError, ValueError):
                return False  # Missing execution evidence is not an assertion failure.
        return item.rc not in (75, 124, 125, 126, 127, 130) and 0 < item.rc < 128

    def close(self, items):
        for item in items:
            self.finish(item)


def reuse(previous, items, record, exact=True):
    """Select exact applicable passes; leave failed evidence in its original run."""
    previous = Path(previous)
    plan = read_plan(previous)
    if exact and plan["root"] != record.root:
        raise ValueError("exact resume requires the saved checkout")
    if exact and plan["items"] != record.plan["items"]:
        raise ValueError("resume cannot change the frozen plan or execution recipe")
    try:
        outcomes = json.loads((previous / "outcomes.json").read_text())
    except FileNotFoundError:
        outcomes = {}
    for item in items:
        if item.state == "passed":
            continue
        old = outcomes.get(item.label, {})
        item.retry_first = old.get("state") != "passed"
        identity = record.identities[item.label]
        reason = None
        if item.label == "engine receipts":
            reason = "coverage verifier runs over the reconciled receipt set"
        elif identity.get("fresh"):
            reason = identity["fresh"]
        elif old.get("state") != "passed" or old.get("exit") != 0 or old.get("invalid"):
            reason = "no validated passing execution"
        elif exact and plan["source"] != record.source:
            reason = "source identity changed"
        elif (old.get("source") or {}).get("commit") != plan["source"]["commit"]:
            # Only HEAD: a pass saved after an unrelated edit is valid for its own declared
            # inputs (Record says why), and the input identity below is what decides it.
            reason = "the checkout's commit changed during the original execution"
        elif old.get("input") != identity:
            reason = "declared execution inputs changed"
        else:
            try:
                if file_digest(old["log"]) != old["log_sha256"]:
                    raise ValueError("result log changed")
                target = receipt_path(item)
                if target:
                    original = old["receipt"]
                    if file_digest(original["path"]) != original["sha256"]:
                        raise ValueError("receipt changed")
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(original["path"], target)
                    item.receipt_sha = old.get("receipt_sha", old["source"]["commit"])
                    completed_receipt(item, item.receipt_sha)
                copied = record.logdir / "reused" / (digest(item.label) + ".log")
                copied.parent.mkdir(exist_ok=True)
                shutil.copyfile(old["log"], copied)
                item.state, item.rc, item.log = "passed", 0, str(copied)
                item.provenance = {"run": str(previous), "source": old["source"],
                                   "receipt": old.get("receipt"), "log_sha256": old["log_sha256"],
                                   "input": old["input"], "earlier": old.get("provenance")}
                item.reused_from = str(previous)
                item.notes.append("validated evidence reused from " + str(previous))
                record.save(item, record.source)
            except (OSError, ValueError, KeyError, TypeError) as exc:
                reason = "saved evidence failed validation: " + str(exc)
                target = receipt_path(item)
                item.state, item.rc, item.log = "waiting", None, None
                for name in ("provenance", "reused_from"):
                    if hasattr(item, name):
                        delattr(item, name)
                if hasattr(item, "receipt_sha"):
                    del item.receipt_sha
                if target and target.exists():
                    target.unlink()
        if reason:
            item.notes.append("execute: " + reason)


def verify_target_receipts(directory, rows, root):
    """Independently recheck the current inputs and exact artifacts for coverage.

    Called by ci-receipts, the sole coverage authority. Historical receipt SHAs
    remain immutable. A target outcome alone cannot excuse a missing, changed,
    noncompleted or unqualified historical receipt.
    """
    import importlib.util
    from types import SimpleNamespace
    directory, root = Path(directory).resolve(), Path(root).resolve()
    plan = read_plan(directory)
    if Path(plan["root"]).resolve() != root:
        raise ValueError("evidence belongs to a different target checkout")
    spec = importlib.util.spec_from_file_location("verification_target_runner",
        root / "richos/app/scripts/proof-run.py")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.ROOT = str(root)
    # HEAD for every unit; the whole source only for a unit that declares no inputs (Record says
    # why): an unrelated edit must not fail the coverage of units it cannot reach.
    current = runner.source_identity()
    if current["commit"] != plan["source"]["commit"]:
        raise ValueError("target commit changed since the plan was recorded")
    undeclared = False
    outcomes = json.loads((directory / "outcomes.json").read_text())
    items = [decode_item(row, runner.Item, root, directory) for row in plan["items"]]
    runner.supply_runtime(items)
    for item in items:
        prepare_environment(item, root, directory, runner.execution_environment(item), create=False)
    units = {item.argv[item.argv.index("--only-units") + 1]: item
             for item in items if item.engine_unit}
    if len(units) != sum(item.engine_unit for item in items):
        raise ValueError("target execution plan contains duplicate engine obligations")
    snapshot = InputSnapshot()
    for row in rows:
        unit = row["unit"]
        if unit not in units:
            raise ValueError("receipt absent from target execution plan: " + unit)
        item = units[unit]
        outcome = outcomes[item.label]
        identity = plan["identities"][item.label]
        if (outcome.get("state") != "passed" or outcome.get("exit") != 0 or outcome.get("invalid")
                or (outcome.get("source") or {}).get("commit") != plan["source"]["commit"]
                or outcome.get("input") != identity):
            raise ValueError("no validated target outcome for " + unit)
        if identity.get("fresh"):
            undeclared = True
            if outcome.get("source") != plan["source"] or current != plan["source"]:
                raise ValueError("target source changed for " + unit + ", which declares no inputs")
        if identity.get("contract") == WHOLE_CHECKOUT:
            # A unit with no reviewed contract is keyed by the checkout's content (2026-10-02).
            # What this verifier can read for itself is that content and where it is; the
            # tools, environment and installs were compared by the runner, in the environment
            # the unit ran in, when it finished (Record.save) and after the run. Read here in
            # the verifier's own process, the environment holds the names the runner gives
            # every check it launches (worker tokens, slot, scope), which no unit was keyed by.
            if identity.get("checkout") != str(root) or identity.get("tree") != snapshot.checkout(root):
                raise ValueError("target checkout content changed for " + unit)
        else:
            actual = runner.input_identity(item, SimpleNamespace(**identity["settings"]), directory, snapshot)
            if actual != identity:
                raise ValueError("target execution inputs changed for " + unit)
        receipt = outcome["receipt"]
        if file_digest(receipt["path"]) != receipt["sha256"]:
            raise ValueError("target receipt artifact changed for " + unit)
        stored = [json.loads(line) for line in Path(receipt["path"]).read_text().splitlines() if line.strip()]
        if stored != [row]:
            raise ValueError("coverage receipt differs from its validated artifact for " + unit)
        if file_digest(outcome["log"]) != outcome["log_sha256"]:
            raise ValueError("completed execution log changed for " + unit)
        historical = row.get("sha") != plan["source"]["commit"]
        provenance = outcome.get("provenance")
        if historical or outcome.get("reused_from"):
            if (identity.get("fresh") or not provenance or provenance.get("input") != identity
                    or row.get("sha") != outcome.get("receipt_sha")
                    or row.get("schema") != 2 or row.get("execution_status") != "completed"
                    or row.get("verdict") != "PASS" or row.get("rc") != row.get("expected_rc")
                    or provenance.get("receipt", {}).get("sha256") != receipt["sha256"]
                    or provenance.get("log_sha256") != outcome["log_sha256"]):
                raise ValueError("unqualified historical receipt for " + unit)
        elif row.get("sha") != outcome.get("receipt_sha", plan["source"]["commit"]):
            raise ValueError("receipt execution commit differs for " + unit)
    after = runner.source_identity()
    if after["commit"] != plan["source"]["commit"] or (undeclared and after != plan["source"]):
        raise ValueError("target source changed during input validation")
    return plan["source"]["commit"]


class Lease:
    """A saved plan cannot have two simultaneous resume owners."""
    def __init__(self, directory):
        self.stream = open(Path(directory) / "resume.lock", "a")
        try:
            fcntl.flock(self.stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.stream.close()
            raise ValueError("saved plan already has an active owner") from None

    def close(self):
        self.stream.close()

    @property
    def fd(self):
        return self.stream.fileno()

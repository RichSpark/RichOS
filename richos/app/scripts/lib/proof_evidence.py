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


def identity_changed_keys(planned, now, limit=4):
    """'; identity differs at: <key paths>' for an invalidation note, or '' when nothing differs.

    Walks the two identities (planned, read when the run was planned; now, read at the
    boundary that judged it) and names the dotted key paths whose values differ, for example
    installed.playwright.installs.ffmpeg-1011.metadata. Whole-checkout checks declare no paths, so
    describe_input_change can only say "nothing under its declared paths"; this says which
    part of the identity moved. Values are never shown (digests are no help to a reader)."""
    found = []

    def walk(path, have, want):
        if have == want:
            return
        if isinstance(have, dict) and isinstance(want, dict):
            for key in sorted(set(have) | set(want), key=str):
                walk(path + [str(key)], have.get(key, _ABSENT), want.get(key, _ABSENT))
        else:
            found.append(".".join(path) or "identity")

    try:
        walk([], planned, now)
    except Exception:  # noqa: BLE001 — an explanation never breaks the verdict it explains
        return ""
    if not found:
        return ""
    more = " (+%d more)" % (len(found) - limit) if len(found) > limit else ""
    return "; identity differs at: " + ", ".join(found[:limit]) + more


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
#   * the checkout's path (what git ignores, node_modules and build products, differs between
#     worktrees; a pass made in another checkout stands here only when it still stands there,
#     on the same content and installs: seat_gap);
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
                                    "RICHOS_UI_TESTS_LEDGER",
                                    # The host shield's per-run log, its declared runs and its own
                                    # path (HOST SHIELD below).
                                    "RICHOS_HOST_SHIELD_LOG", "RICHOS_HOST_SHIELD_RUNS",
                                    "RICHOS_HOST_SHIELD_TOOL",
                                    # The tags the agent harness gives each agent's shell (engine
                                    # agent_hold.py TAG, SESSION_TAG): who started the process, a
                                    # value per agent and session, so the land, run by the lead,
                                    # never carried the author's. 2026-10-21: 18 of 24 unchanged
                                    # passes read as changed by them alone (stands_in below).
                                    "RICHOS_AGENT_OWNER", "RICHOS_AGENT_SESSION",
                                    # The land's own variables (autocheck.py LAND_ONLY): its target,
                                    # the author runs offered to it and main's copy's answer. No
                                    # check reads them (clean_env takes them from every check the
                                    # gate runs), but the preflight of main's older copy asks with
                                    # them set, and the copy that runs at a land is main's, so a fix
                                    # in autocheck.py cannot reach its own land. 2026-10-21: 19 of
                                    # 24 unchanged passes read as changed by LAND_TARGET at the land.
                                    "RICHOS_AUTOCHECK_LAND_TARGET", "RICHOS_AUTOCHECK_AUTHOR_RUNS",
                                    "RICHOS_AUTOCHECK_FROM_MAIN"))
_TOOL_DIGESTS = {}

# THE HOST SHIELD (ceo-decisions §65, engine lib/host_shield.py) starts every check with its
# stand-in directory first on PATH. A process started inside a shielded check therefore reads a
# PATH the runner that planned the check never saw: `engine receipts` (ci-shard.sh
# --verify-receipts, itself started through the shield) re-reads each unit's recorded identity,
# and a private-profile unit's PATH differed by exactly that directory, so every engine unit's
# receipt was refused as "target execution inputs changed" (2026-10-11, proof-evidence's
# cross-commit fixtures). The stand-ins only refuse host power, sleep, lock and display calls;
# they are no input of any check. An identity reads PATH without them.
HOST_SHIELD_MARK = b"# host-shield stand-in"
_SHIELD_DIRS = {}


def _is_shield_dir(directory):
    if os.path.basename(os.path.normpath(directory or ".")) != "host-shield":
        return False
    if directory not in _SHIELD_DIRS:
        found = False
        try:
            for name in os.listdir(directory):
                try:
                    with open(os.path.join(directory, name), "rb") as fh:
                        if HOST_SHIELD_MARK in fh.read(256):
                            found = True
                            break
                except OSError:
                    continue
        except OSError:
            pass
        _SHIELD_DIRS[directory] = found
    return _SHIELD_DIRS[directory]


def identity_environment(environment):
    """`environment` with every host-shield stand-in directory left out of its tool PATHs."""
    result = dict(environment)
    for name in ("PATH", "RICHOS_VERIFICATION_TOOL_PATH"):
        if name in result:
            result[name] = os.pathsep.join(d for d in result[name].split(os.pathsep) if not _is_shield_dir(d))
    return result


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


def checkout_rows(root):
    """Every (name, stage, mode, blob) git would land from the checkout at `root`, sorted.

    Repository-relative names only: the same content in two checkouts gives the same rows."""
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
    return [(name, stage, mode, blob) for name in sorted(entries)
            for stage, (mode, blob) in sorted(entries[name].items())]


def _rows_digest(rows):
    digest = hashlib.sha256()
    for name, stage, mode, blob in rows:
        digest.update(b"%s\0%s\0%s\0%s\n" % (name, stage, mode.encode(), blob.encode()))
    return digest.hexdigest()


def checkout_content(root):
    """sha256 over the content git would land from the checkout at `root` (see above)."""
    return _rows_digest(checkout_rows(root))


def rows_under(rows, names, rel):
    """The rows of `rel` and everything under it (`names` is the sorted name column of `rows`)."""
    import bisect
    prefix = os.fsencode(rel.rstrip("/"))
    start = bisect.bisect_left(names, prefix)
    found = []
    for index in range(start, len(rows)):
        name = names[index]
        if name != prefix and not name.startswith(prefix + b"/"):
            # Names sort byte-wise, so `rel.x` and `rel-x` come between `rel` and `rel/`: skip
            # them, and stop only once past every name that could still be under `rel/`.
            if name > prefix + b"/\xff":
                break
            continue
        found.append(rows[index])
    return found


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


# A DEPENDENCIES_VALIDATED MARKER IS A CACHE STAMP, NOT A BROWSER (2026-10-29). Playwright
# rewrites <install>/DEPENDENCIES_VALIDATED whenever it re-validates the host libraries for an
# install, and the postinstall of an unrelated `npm install` does it for every install on the
# machine (ffmpeg-1011 at 22:16:11Z). Its mtime and ctime were hashed into playwright_installs
# and so into every whole-checkout identity and every UI recipe: 39 passing checks in three
# running proof runs were marked invalid by a file no browser reads. Only the marker at the top
# of an install folder is left out; INSTALLATION_COMPLETE and every other file stay bound.
PLAYWRIGHT_VALIDATION_MARKER = "DEPENDENCIES_VALIDATED"


def playwright_browser_names(root, environment):
    """The install folder names (`<name with _ for ->-<revision>`, as Playwright's registry names
    them) that the browsers.json of every Playwright package harness.js may load lists, every
    revision override included; None when no candidate has a readable browsers.json.

    WHY (follow-up of 2026-10-29, fix A's own follow-up). The cache is shared by every Playwright
    on the Mac and holds installs of revisions this checkout never launches (eight stale ones that
    day); a write in any of them changed every whole-checkout identity. Over-binding costs only a
    rerun, so every present candidate counts and every override revision is kept."""
    names, read = set(), False
    for candidate, folder in playwright_package_candidates(root, environment):
        real = Path(os.path.realpath(candidate))
        if not real.is_dir():
            continue
        module_root = Path(os.path.realpath(folder)) if folder is not None else _node_module_root(real)
        for listing in (real / "browsers.json", real / "node_modules/playwright-core/browsers.json",
                        module_root / "playwright-core/browsers.json"):
            try:
                browsers = json.loads(listing.read_text())["browsers"]
                found = {"%s-%s" % (browser["name"].replace("-", "_"), revision)
                         for browser in browsers
                         for revision in [browser["revision"], *(browser.get("revisionOverrides") or {}).values()]}
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                continue
            names |= found
            read = True
    return names if read else None


def playwright_installs(environment, root=None):
    """The Playwright browser installs a check would launch, by install identity.

    Bind the recursive install metadata, including ctime, so in-place writes and signing
    changes invalidate passes even when the revision directory survives. This is input
    drift detection, not content authentication. It avoids reading gigabytes per check.
    PLAYWRIGHT_BROWSERS_PATH=0 is already covered by checkout_installs. With `root`, only
    the installs the checkout's browsers.json names are bound (playwright_browser_names)."""
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
    named = playwright_browser_names(root, environment) if root is not None else None
    installs = {}
    for entry in entries:
        if named is not None and entry.name not in named:
            continue  # an install no browsers.json of this checkout names (see playwright_browser_names)
        if PLAYWRIGHT_INSTALL.fullmatch(entry.name) and entry.is_dir(follow_symlinks=False):
            rows = []
            for directory, dirs, files in os.walk(entry.path, followlinks=False):
                for name in sorted(dirs + files):
                    path = Path(directory) / name
                    if directory == entry.path and name == PLAYWRIGHT_VALIDATION_MARKER:
                        continue  # a cache stamp, not part of the browser (see above)
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


def installed_dependencies(root, environment, browsers=None):
    """What a check executes that git does not track (WHOLE_CHECKOUT above). `browsers`: the
    playwright_installs(environment) reading this pass already took (InputSnapshot.browsers)."""
    return {"checkout": checkout_installs(root),
            "playwright": browsers if browsers is not None else playwright_installs(environment, root),
            "playwright_package": external_playwright(root, environment),
            "rust": rust_toolchain(root, environment)}


def portable_dependencies(root, environment, browsers=None):
    """installed_dependencies() bound by WHAT a UI suite loads, never by WHERE the checkout is.

    installed_dependencies() keys the Playwright package by each absolute candidate path and binds
    the checkout's own installs by name, so a worktree (which loads the main checkout's install
    through harness.js's last candidate) and the main checkout (which loads its own) can never
    agree, even running the same bytes. Here every present candidate, in the order harness.js
    tries them, is bound by the content of the module directory it would load from; absent
    candidates add nothing, so an install that appears where the harness looks first is a new
    entry and changes the identity. A checkout install that is one of those module directories is
    not bound twice; any other one still is, by name. Browser installs and the Rust toolchain are
    read exactly as installed_dependencies() reads them: neither depends on the checkout path."""
    root = Path(root).resolve()
    chain, loaded = [], []
    for candidate, folder in playwright_package_candidates(root, environment):
        real = Path(os.path.realpath(candidate))
        target = next((path for path in (Path(str(real) + suffix) for suffix in ("", ".js", ".json", ".node"))
                       if path.exists()), None)
        if target is None:
            continue
        module_root = Path(os.path.realpath(folder)) if folder is not None else _node_module_root(target)
        if module_root in loaded:
            continue
        loaded.append(module_root)
        chain.append(digest(path_identity(module_root)))
    checkout = {name: value for name, value in checkout_installs(root).items()
                if Path(os.path.realpath(root / name)) not in loaded}
    return {"checkout": checkout,
            "playwright": browsers if browsers is not None else playwright_installs(environment, root),
            "playwright_package": chain, "rust": rust_toolchain(root, environment)}


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
    environment = identity_environment(environment)
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
        self.rows = {}
        self.contents = {}
        self.portables = {}
        self.browser_installs = {}

    def checkout(self, root):
        key = str(root)
        if key not in self.checkouts:
            if key in self.rows:
                self.checkouts[key] = _rows_digest(self.rows[key][0])
            else:
                self.checkouts[key] = checkout_content(root)
        return self.checkouts[key]

    def content(self, root, rel):
        """The content git would land under `rel` (one file or a directory), without the
        checkout's path: one listing per checkout per pass, shared by every path."""
        root = str(root)
        if root not in self.rows:
            rows = checkout_rows(root)
            self.rows[root] = (rows, [row[0] for row in rows])
        key = (root, rel)
        if key not in self.contents:
            rows, names = self.rows[root]
            found = rows_under(rows, names, rel)
            self.contents[key] = _rows_digest(found) if found else {"absent": True}
        return self.contents[key]

    def installed(self, root, environment):
        key = (str(root), *(environment.get(name) for name in INSTALL_ENVIRONMENT_NAMES))
        if key not in self.installs:
            self.installs[key] = installed_dependencies(root, environment, self.browsers(environment, root))
        return self.installs[key]

    def portable(self, root, environment):
        key = (str(root), *(environment.get(name) for name in INSTALL_ENVIRONMENT_NAMES))
        if key not in self.portables:
            self.portables[key] = portable_dependencies(root, environment, self.browsers(environment, root))
        return self.portables[key]

    def browsers(self, environment, root=None):
        """playwright_installs once per pass for what it reads (the browsers path and HOME, and
        the checkout and package lookup that name the installs bound): a whole-checkout identity
        and a UI family identity each walked every browser install, so a pass holding both kinds
        walked them twice (03-E, PRD line 139's no-execution budget)."""
        key = (str(root), *(environment.get(name) for name in INSTALL_ENVIRONMENT_NAMES))
        if key not in self.browser_installs:
            self.browser_installs[key] = playwright_installs(environment, root)
        return self.browser_installs[key]

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
    # Only a declared contract names an isolation profile; a UI family recipe (contract_for)
    # never does, so its whole derivation is not repeated here just to learn that (03-E).
    recipe = declared_contract(root, item.label)
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


SHARED_FLOOR_KEYS = ("requires", "isolation", "subset_requires", "external_paths", "git_inputs")


def shared_floor(contract, extra):
    """The common requirements a recipe of this unit is held to: the unit's own `common` when it
    declares one (a family unit, qualify_recipe), otherwise the qualification's shared floor."""
    if isinstance(extra, dict) and "common" in extra:
        return extra["common"] if isinstance(extra["common"], dict) else {}
    return {key: contract[key] for key in SHARED_FLOOR_KEYS if key in contract}


def qualification_slice(root, recipe):
    """What of the shared qualification JSON a recipe is qualified by: its schema and review
    pointer, the floor it is held to (shared_floor) and its own unit, pins included.

    The whole file used to be one of every recipe's input paths, so renewing ANY unit's pin (a
    reader of an unrelated engine test) changed every qualified identity and threw away every
    reusable pass (PRD 2026-10-20: another unit's pin renewal must not). The prose review stays
    a whole-file input (recipe_identity)."""
    contract = json.loads((Path(root) / recipe["qualification"]).read_text())
    unit = recipe.get("qualification_unit")
    extra = contract.get("units", {}).get(unit) if unit is not None else None
    return digest({"schema": contract.get("schema"), "review": contract.get("review"),
                   "floor": shared_floor(contract, extra), "unit": unit, "qualified": extra})


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
    unit = recipe.get("qualification_unit")
    extra = contract.get("units", {}).get(unit) if unit is not None else None
    # A FAMILY UNIT CARRIES ITS OWN COMMON FLOOR (2026-10-20). The shared floor is the engine
    # tier's: the private profile and the whole engine and runner scripts as inputs. A browser
    # suite neither runs in that profile (it would lose the nightly's conditions and its browser
    # installs) nor reads the engine. So a reviewed unit may replace the floor with its own
    # `common`, and only a recipe of that unit's own family (contract_for derives those, from the
    # authoritative declarations) may use it: a named recipe cannot borrow it to drop a floor.
    floor = shared_floor(contract, extra)
    if extra is not None and "common" in extra and (not extra.get("family")
                                                    or recipe.get("family") != extra["family"]):
        raise ValueError("unit qualification belongs to the %s family only: %s" % (extra.get("family"), unit))
    if (not isinstance(floor.get("requires"), dict) or set(floor["requires"]) != fields):
        raise ValueError("invalid common input floor for unit: " + str(unit))
    if floor.get("isolation") and recipe.get("isolation") != floor["isolation"]:
        raise ValueError("input qualification requires isolation: " + floor["isolation"])
    if recipe.get("isolation") and not floor.get("isolation") and extra is not None and "common" in extra:
        raise ValueError("unsupported execution profile for this family: " + recipe["isolation"])
    review = relative(contract.get("review"))
    if not (root / review).is_file():
        raise ValueError("input review is missing: " + review)
    if unit is not None:
        if (not isinstance(extra, dict) or not extra.get("review")
                or not extra.get("sources") or set(extra.get("requires", {})) != fields):
            raise ValueError("missing reviewed unit qualification: " + str(unit))
        for source, expected in extra["sources"].items():
            source = relative(source)
            if not (root / source).is_file() or file_digest(root / source) != expected:
                raise UnqualifiedReader("changed unit reader requires qualification: " + source)
    for field in sorted(fields):
        for values in (recipe[field], floor["requires"][field]):
            if not isinstance(values, list) or any(not isinstance(v, str) or not v for v in values):
                raise ValueError("invalid input list: " + field)
        required = list(floor["requires"][field])
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
    required_external = floor.get("external_paths", [])
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
    git_required = [floor.get("git_inputs", {})]
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
        common = floor.get("subset_requires")
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
            minimum = [*common[field], *reviewed["requires"][field]]
            if field == "paths":
                minimum.extend(extra["sources"])
                minimum.extend(extra["requires"]["paths"])
            missing = [p for p in minimum if not any(
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
    optional = {"isolation", "qualification_unit", "external_paths", "git_inputs", "subset", "family", "invocation",
                "exists", "external_reads"}
    if set(recipe) - optional != required or not recipe["qualification"]:
        raise ValueError("incomplete verification input contract")
    root = Path(root).resolve()
    environment = identity_environment(environment)
    snapshot = snapshot or InputSnapshot()
    try:
        qualification, review = qualify_recipe(root, recipe)
    except UnqualifiedReader as exc:
        return {"fresh": str(exc)}
    # A family recipe (UI_FAMILY below) binds its paths by the content git would land, which
    # carries no checkout path, so the same bytes in another worktree are the same input. Every
    # other recipe keeps path_identity, which also binds ignored files under its roots.
    family = recipe.get("family")
    paths = {}
    selected_paths = recipe.get("subset", {}).get("paths", recipe["paths"])
    for rel in selected_paths:
        if Path(rel).is_absolute() or ".." in Path(rel).parts:
            raise ValueError(f"input must be repository-relative: {rel}")
        paths[rel] = digest(snapshot.content(root, rel) if family else snapshot.path(root / rel))
    # The prose review is a whole-file authority; the JSON only as the slice this recipe is
    # qualified by (qualification_slice).
    paths[review] = digest(snapshot.path(root / review))
    tools = {}
    for name in recipe["tools"]:
        resolved = shutil.which(name, path=environment.get("RICHOS_VERIFICATION_TOOL_PATH", environment.get("PATH")))
        if family:
            # The interpreter itself is large (node is ~100 MB): read again only when its file
            # changes (_tool_digest, the whole-checkout identity's reader), never once per pass.
            tools[name] = {"path": resolved, "sha256": _tool_digest(resolved)} if resolved else {"absent": True}
            continue
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
    identity = {"contract": digest(recipe), "paths": paths, "tools": tools, "profile": profile,
                "qualification": qualification_slice(root, recipe),
                "inventories": {path: digest(snapshot.inventory(root / path))
                                for path in recipe.get("subset", {}).get("inventories", [])},
                "external_paths": literal_external,
                "git_inputs": git_input_identity(root, recipe.get("git_inputs"), environment),
                "environment": values, "external": external,
                "platform": [platform.system(), platform.release(), platform.machine(), platform.mac_ver()[0], os.cpu_count()]}
    if family:
        # What the suite executes that git does not track, bound by what loads rather than by
        # where this checkout is (portable_dependencies).
        identity["installed"] = snapshot.portable(root, environment)
        # Which pictures a library checks for (library_shot_names), by existence alone, as the
        # content git would land has them (the same listing every family path is read from).
        if recipe.get("exists"):
            identity["exists"] = {rel: snapshot.content(root, rel) != {"absent": True}
                                  for rel in recipe["exists"]}
        if recipe.get("external_reads"):
            reads = private_read_identity(recipe["external_reads"], environment, snapshot)
            if reads.get("fresh"):
                return {"fresh": reads["fresh"]}
            identity["external_reads"] = reads
    return identity


# ONE UI FAMILY ENTRY, RESOLVED FROM THE AUTHORITATIVE UI DECLARATIONS (2026-10-20).
#
# Every browser suite was keyed by the whole checkout, its path included, so a suite that passed
# in the engineer's worktree ran again at the land (79 of 79 in nightly 68 and in the round 32
# sidebar land, whose two stale checks the engineer never ran: verification reliability PRD,
# richos-hq docs/prds/2026-10-20-verification-reliability.md). A suite has no row of its own in
# `checks`; proof-inputs.json carries ONE `families.ui-suite` entry, and contract_for() derives a
# suite's recipe from what already declares it:
#   * ALL shipped UI, through ui-sources.js manifest().order (and `@allui` exactly as
#     proof-for.sh computes it), plus the family's shipped resources: product code is never
#     sliced feature by feature, so any shipped change, a startup source included, reruns it;
#   * its own row in proof-for.ui-inputs, aliases expanded, `<dir>/` as the whole directory;
#   * the suite file and every file it reaches by a relative `require` (the libraries, and the
#     suites that are also modules of another), proof-for.sh's library rule;
#   * every `shots-*` folder the suite file names, proof-for.sh's reference rule;
#   * the family's common files and the tools, environment names and installs its review names.
# So the identity binds at least every path whose change would SELECT the suite: a reuse is never
# less safe than the selection it stands in for, and another suite's row, file or pictures are not
# an input. A label with no row, a suite the family leaves unqualified (with its reason) and any
# invocation other than `cd <suites> && node <suite>` (proof-run.py checks that, holding argv and
# cwd) stay keyed by the whole checkout.
UI_FAMILY = "ui-suite"
_REQUIRE = re.compile(r"""require\(\s*(["'])(\.{1,2}/[^"']+)\1\s*\)""")
# require(path.join(__dirname, "..", "lib", "harness")): the fixture suites' form.
_REQUIRE_JOINED = re.compile(r"""require\(\s*path\.(?:join|resolve)\(\s*__dirname((?:\s*,\s*["'][^"']+["'])+)\s*\)\s*\)""")
_UI_MANIFESTS = {}


def ui_declaration(path):
    """(rows, aliases) of proof-for.ui-inputs, read the way proof-for.sh reads it."""
    rows, aliases = {}, {}
    for line in Path(path).read_text().splitlines():
        words = line.split("#", 1)[0].split()
        if not words:
            continue
        if words[0].startswith("@"):
            aliases[words[0][1:]] = words[1:]
        elif words[0] in rows:
            raise ValueError("proof-for.ui-inputs has two rows for " + words[0])
        else:
            rows[words[0]] = words[1:]
    return rows, aliases


def _shipped_stat(directory):
    """(name, size, mtime) of every file under the shipped UI, tests and installs aside."""
    found = []
    for base, dirs, files in os.walk(directory):
        dirs[:] = sorted(d for d in dirs if d != "node_modules" and not (d == "tests" and base == str(directory)))
        for name in sorted(files):
            try:
                info = os.stat(os.path.join(base, name))
            except OSError:
                continue
            found.append((os.path.join(base, name), info.st_size, info.st_mtime_ns))
    return tuple(found)


def ui_manifest(root, family):
    """Repository-relative paths of ui-sources.js manifest().order, through the Node runtime.

    Derived once per state of the shipped tree in a process: the derivation follows the shipped
    files themselves, all of which every family identity binds by content."""
    root = Path(root).resolve()
    shipped = root / family["shipped"]
    key = (str(root), _shipped_stat(shipped))
    if key not in _UI_MANIFESTS:
        search = os.pathsep.join([os.environ.get("PATH", ""), "/opt/homebrew/bin", "/usr/local/bin"])
        node = shutil.which("node", path=search)
        if not node:
            raise ValueError("node is needed to read the shipped UI manifest")
        library = root / family["manifest"]
        result = subprocess.run(
            [node, "-e", "process.stdout.write(JSON.stringify(require(process.argv[1]).manifest().order))",
             str(library)], cwd=str(library.parent), capture_output=True, text=True, timeout=60,
            stdin=subprocess.DEVNULL)
        if result.returncode != 0:
            raise ValueError("the shipped UI manifest refused: " + result.stderr.strip()[-300:])
        order = json.loads(result.stdout)
        if not isinstance(order, list) or not order or not all(isinstance(rel, str) for rel in order):
            raise ValueError("the shipped UI manifest is not a list of files")
        _UI_MANIFESTS[key] = [family["shipped"] + "/" + rel for rel in order]
    return list(_UI_MANIFESTS[key])


def all_ui(root, family):
    """`@allui` exactly as proof-for.sh computes it: every .js, .css and .html under the shipped
    UI, tests and node_modules aside."""
    root = Path(root)
    found = []
    for path, _size, _mtime in _shipped_stat(root / family["shipped"]):
        if path.endswith((".js", ".css", ".html")):
            found.append(os.path.relpath(path, root))
    return found


def require_closure(root, *starts):
    """Every start and every file it reaches by a relative `require` (a literal `./`/`../` path or
    `path.join(__dirname, ...)` of literals), repository-relative."""
    root = Path(root).resolve()
    seen, todo = set(), [Path(os.path.normpath(start)) for start in starts]
    while todo:
        current = todo.pop()
        if current in seen or not (current == root or root in current.parents):
            continue
        seen.add(current)
        if current.suffix not in (".js", ".cjs", ".mjs"):
            continue
        try:
            targets = _file_scan(current)["requires"]
        except OSError:
            continue
        for target in targets:
            base = current.parent / target
            for candidate in (base, Path(str(base) + ".js"), Path(str(base) + ".json"), base / "index.js"):
                if candidate.is_file():
                    todo.append(Path(os.path.normpath(candidate)))
                    break
    return sorted(os.path.relpath(path, root) for path in seen)


# WHAT A SUITE'S CLOSURE LOADS MUST BE FOLLOWED TO THE BOTTOM, OR THE SUITE IS NOT QUALIFIED
# (review rv-20261020T054118Z-26a24a78-1851: thread-switch.js runs contrast.py, which imports
# qa/lib/qaimg.py, and a changed qaimg.py flipped its verdict while the identity stood still).
# require_closure follows the loads it can read exactly: a literal relative `require` and
# `require(path.join(__dirname, ...literals))`. Every other load in the closure is judged here:
#   * a child node process (process.execPath): allowed; the scripts it runs are on the suite's
#     row, and their own requires are followed like the suite's;
#   * a literal child program the family binds by content (`child_tools`): allowed;
#   * a Node builtin module: allowed;
#   * anything else (a Python, Cargo or shell child, a computed program, a computed `require`,
#     an installed package, a dynamic `import(`): the suite is NOT qualified, with the file and
#     line, unless the family entry declares that exact call in `dynamic` with its reason.
_CHILD_CALL = re.compile(r"""(?:\b(spawnSync|spawn|execFileSync|execFile|execSync|fork)|(?<![.\w$])(exec))\(\s*([^,)]*)""")
_ANY_REQUIRE = re.compile(r"""(?<![.\w$])require\(\s*([^)]*)\)""")
_DYNAMIC_IMPORT = re.compile(r"""(?<![.\w$])import\(""")
_LITERAL = re.compile(r"""^(["'])([^"']*)\1$""")
_JOINED_ARG = re.compile(r"""^path\.(?:join|resolve)\(\s*__dirname(?:\s*,\s*["'][^"']+["'])+\s*$""")
_BUILTINS = {}


def node_builtins():
    """Node's own module names, from the Node runtime, once per process."""
    search = os.pathsep.join([os.environ.get("PATH", ""), "/opt/homebrew/bin", "/usr/local/bin"])
    node = shutil.which("node", path=search)
    if not node:
        raise ValueError("node is needed to read its builtin module list")
    if node not in _BUILTINS:
        result = subprocess.run([node, "-p", "JSON.stringify(require('module').builtinModules)"],
                                capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        if result.returncode != 0:
            raise ValueError("cannot read Node's builtin modules")
        _BUILTINS[node] = frozenset(json.loads(result.stdout))
    return _BUILTINS[node]


_LOAD_SCANS = {}
_FILE_SCANS = {}


def _file_scan(path):
    """{"requires": the relative targets require_closure follows, "shots": every literal
    `shots-<folder>/<file>` outside comment lines (library_shot_names)} of one file. Read again
    whenever the file's stat identity changes, as _file_loads and _tool_digest are: every UI suite's
    closure holds the same shipped files, and reading each of them again for every suite, twice a
    pass, was most of the family identity's cost (03-E, PRD line 139's no-execution budget)."""
    info = os.stat(path)
    key = (str(path), info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_ino)
    if key not in _FILE_SCANS:
        text = Path(path).read_text(errors="replace")
        requires = [match.group(2) for match in _REQUIRE.finditer(text)]
        requires += [os.path.join(*re.findall(r"""["']([^"']+)["']""", match.group(1)))
                     for match in _REQUIRE_JOINED.finditer(text)]
        shots = []
        if "shots-" in text:
            for line in text.splitlines():
                if line.lstrip().startswith(("//", "*", "/*")):
                    continue  # a comment's example name is no read
                shots += [match.group(1) for match in _SHOT_FILE.finditer(line)]
        _FILE_SCANS[key] = {"requires": requires, "shots": shots}
    return _FILE_SCANS[key]


def _file_loads(path):
    """[(line, kind, call, arg)] for every child launch, `require` and `import(` in a file outside
    comment lines. Read again whenever the file's stat identity changes, as _tool_digest is."""
    info = os.stat(path)
    key = (str(path), info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_ino)
    if key not in _LOAD_SCANS:
        text = Path(path).read_text(errors="replace")
        found = []
        if "require(" in text or "import(" in text or "spawn" in text or "exec" in text or "fork(" in text:
            def line_of(offset):
                start = text.rfind("\n", 0, offset) + 1
                end = text.find("\n", offset)
                line = text[start:end if end >= 0 else len(text)].strip()
                return text.count("\n", 0, offset) + 1, line.startswith(("//", "*", "/*"))
            for pattern, kind in ((_CHILD_CALL, "child"), (_ANY_REQUIRE, "require"), (_DYNAMIC_IMPORT, "import")):
                for match in pattern.finditer(text):
                    number, comment = line_of(match.start())
                    if comment:
                        continue
                    if kind == "child":
                        arg = match.group(3).strip()
                        found.append((number, kind, "%s(%s" % (match.group(1) or match.group(2), arg), arg))
                    elif kind == "require":
                        arg = match.group(1).strip()
                        found.append((number, kind, "require(%s)" % arg, arg))
                    else:
                        found.append((number, kind, "import(", ""))
        _LOAD_SCANS[key] = sorted(found)
    return _LOAD_SCANS[key]


def unfollowed_loads(root, closure, family):
    """['<file>:<line> <call>'] for every load in the closure the family cannot follow (above)."""
    root = Path(root)
    declared = family.get("dynamic", {})
    tools = set(family.get("child_tools", []))
    builtins = None
    found = []
    for rel in closure:
        if not rel.endswith((".js", ".cjs", ".mjs")):
            continue
        allowed = declared.get(rel, {})
        try:
            loads = _file_loads(root / rel)
        except OSError:
            continue
        for number, kind, call, arg in loads:
            if call in allowed:
                continue
            literal = _LITERAL.match(arg)
            if kind == "child" and (arg == "process.execPath" or (literal and literal.group(2) in tools)):
                continue
            if kind == "require":
                if literal and literal.group(2).startswith(("./", "../")):
                    continue  # require_closure follows it
                if _JOINED_ARG.match(arg):
                    continue
                if literal:
                    builtins = builtins if builtins is not None else node_builtins()
                    if literal.group(2).removeprefix("node:") in builtins:
                        continue
            found.append("%s:%d %s" % (rel, number, call))
    return found


def ui_family_recipe(root, label, family):
    """The recipe of UI suite `label` (UI_FAMILY above), or {"fresh": reason}."""
    root = Path(root).resolve()
    unqualified = family.get("unqualified", {})
    if label in unqualified:
        return {"fresh": "the %s family leaves %s unqualified: %s" % (UI_FAMILY, label, unqualified[label])}
    suites = root / family["suites"]
    rows, aliases = ui_declaration(root / family["declaration"])
    if label not in rows or not (suites / label).is_file():
        return {"fresh": "input qualification not yet declared for " + label}
    paths = set(family["paths"]) | set(ui_manifest(root, family))

    def expand(tokens, depth=0):
        if depth > 8:
            raise ValueError("proof-for.ui-inputs aliases nest too deeply")
        for token in tokens:
            if token == "@allui":
                paths.update(all_ui(root, family))
            elif token.startswith("@"):
                if token[1:] not in aliases:
                    raise ValueError("proof-for.ui-inputs: no alias " + token)
                expand(aliases[token[1:]], depth + 1)
            else:
                paths.add(token.rstrip("/"))

    expand(rows[label])
    # From the suite AND every script its row names: a fixture suite or tool it runs as a child
    # process reads what that script requires, which the suite's own requires never reach
    # (review rv-20261020T044017Z-ebbb640b-fc3b, P2: a child's reads must be bound before its
    # parent's pass is reused). A folder a child lists is the row's to declare (`<dir>/`).
    scripts = [root / rel for rel in sorted(paths) if rel.endswith((".js", ".cjs", ".mjs"))]
    closure = require_closure(root, suites / label, *[path for path in scripts if path.is_file()])
    paths.update(closure)
    unfollowed = unfollowed_loads(root, closure, family)
    if unfollowed:
        return {"fresh": "the %s family cannot follow everything %s loads, so it stays unqualified: %s%s" % (
            UI_FAMILY, label, "; ".join(unfollowed[:4]),
            " (and %d more)" % (len(unfollowed) - 4) if len(unfollowed) > 4 else "")}
    text = (suites / label).read_text(errors="replace")
    for folder in sorted(suites.iterdir()):
        if folder.is_dir() and folder.name.startswith("shots-") and re.search(
                re.escape(folder.name) + r"(?![A-Za-z0-9_-])", text):
            paths.add(os.path.relpath(folder, root))
    for rel in paths:
        if Path(rel).is_absolute() or ".." in Path(rel).parts:
            raise ValueError("a UI input must be repository-relative: " + rel)
    recipe = {"family": UI_FAMILY, "invocation": {"cwd": family["suites"], "argv": ["node", label]},
              "paths": sorted(paths), "tools": list(family["tools"]),
              "environment": list(family["environment"]), "external": list(family.get("external", [])),
              "qualification": family["qualification"], "qualification_unit": family["qualification_unit"]}
    exists = library_shot_names(root, family, label, closure, recipe["paths"])
    if exists:
        recipe["exists"] = exists
    if label in family.get("private_reads", {}):
        recipe["external_reads"] = private_reads(root, family["private_reads"][label])
    return recipe


# A LIBRARY THAT CHECKS WHICH PICTURES EXIST BINDS THEIR EXISTENCE, NEVER THEIR BYTES (review
# rv-20261020T063426Z-03441b27-e3dd #1). lib/shot-stability.js proves at require time that every
# picture it declares a bound for exists (`shots-home/home-named.png`, ...), so deleting one turns
# every suite that loads it from a pass into a failure; the suite's identity bound neither the
# picture nor the folder (it names no `shots-home`), so the old pass stood. A literal
# `shots-<folder>/<file>` in any library of the suite's closure (the suites folder is the only
# place pictures live) is therefore bound by whether it exists. Its content is not: binding it
# would make every suite that loads the library run again whenever an unrelated suite's picture
# is regenerated, which is the 87-check restart the family exists to end. The suite's own named
# folders are bound by content above, so they are left out here.
_SHOT_FILE = re.compile(r"""["'`](shots-[A-Za-z0-9_.-]+/[^"'`\s]+)["'`]""")


def library_shot_names(root, family, label, closure, bound):
    suite = family["suites"] + "/" + label
    found = set()
    for rel in closure:
        if rel == suite or not rel.endswith((".js", ".cjs", ".mjs")):
            continue
        try:
            shots = _file_scan(Path(root) / rel)["shots"]
        except OSError:
            continue
        for shot in shots:
            name = family["suites"] + "/" + shot
            if not any(name == path or name.startswith(path + "/") for path in bound):
                found.add(name)
    return sorted(found)


# A PRIVATE READ IS BOUND BY CONTENT, AND A STUDY THAT REACHES OUTSIDE ITS FOLDER IS NOT
# QUALIFIED (PRD 2026-10-20, "private reads such as splash's studies"). splash.js check 18 opens,
# beside each splash-library.js entry, the round study its `source` names in the private
# richos-hq repository ($RICHOS_HQ, else ~/ab/richos-hq), and only when that repository exists.
# The family entry declares where the names are (a shipped file, bound by content already) and
# how the root is found; the recipe names each study's folder, and its identity binds whether the
# root exists and every file in each folder. A study page loads its fonts and styles from its
# own folder (round folders are self-contained and frozen); recipe_identity refuses one that
# names a file outside it, so a reach the declaration does not bind leaves the suite unqualified.
def private_reads(root, declaration):
    text = (Path(root) / declaration["named_in"]).read_text(errors="replace")
    folders = sorted({os.path.dirname(name) for name in re.findall(declaration["pattern"], text)})
    for folder in folders:
        if not folder or Path(folder).is_absolute() or ".." in Path(folder).parts:
            raise ValueError("a private read must be relative to its root: " + folder)
    return {"env": declaration["env"], "home": declaration["home"], "paths": folders}


_ESCAPING_REFERENCE = re.compile(r"""(?:src|href)\s*=\s*["'](?!data:|https?:|#|mailto:)([^"']+)["']|url\(\s*["']?(?!data:|https?:|#)([^"')]+)""")


def private_read_identity(declaration, environment, snapshot):
    """{"fresh": why} or what the declared private reads hold (private_reads above)."""
    home = environment.get("HOME") or os.path.expanduser("~")
    base = Path(environment.get(declaration["env"]) or os.path.join(home, declaration["home"]))
    if not base.is_dir():
        return {"root": {"absent": True}}
    folders = {}
    for rel in declaration["paths"]:
        folder = (base / rel).resolve()
        for page in sorted(folder.rglob("*")) if folder.is_dir() else []:
            if page.suffix not in (".html", ".css", ".js"):
                continue
            for match in _ESCAPING_REFERENCE.finditer(page.read_text(errors="replace")):
                target = (page.parent / (match.group(1) or match.group(2)).split("?")[0].split("#")[0]).resolve()
                if target != folder and folder not in target.parents:
                    return {"fresh": "%s names %s, outside the study folder its declaration binds" % (
                        os.path.relpath(page, base), match.group(1) or match.group(2))}
        folders[rel] = digest(snapshot.path(folder))
    return {"root": {"present": True}, "paths": folders}


def family_invocation(root, recipe, cwd, argv):
    """`recipe` when the runner item runs exactly the invocation its family qualifies, else fresh.

    Asked by the callers that hold the item's argv and cwd (proof-run.py input_identity): a known
    label is not enough, so a new or different command never inherits the family's review."""
    expected = recipe.get("invocation") or {}
    if (os.path.realpath(cwd) == os.path.realpath(Path(root) / expected.get("cwd", "\0"))
            and list(argv) == expected.get("argv")):
        return recipe
    return {"fresh": "the %s family qualifies only `cd %s && %s`, not this invocation" % (
        recipe.get("family"), expected.get("cwd"), " ".join(expected.get("argv") or []))}


def declared_contract(root, label):
    """The contract proof-inputs.json declares for `label` by name, or {} (none, or a family's)."""
    try:
        data = json.loads((Path(root) / "richos/app/scripts/proof-inputs.json").read_text())
    except FileNotFoundError:
        return {}
    if data.get("schema") != 1 or not isinstance(data.get("checks"), dict):
        raise ValueError("invalid proof-inputs.json")
    return data["checks"].get(label) or {}


def contract_for(root, label):
    path = Path(root) / "richos/app/scripts/proof-inputs.json"
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        return {"fresh": "no committed input contract"}
    if data.get("schema") != 1 or not isinstance(data.get("checks"), dict):
        raise ValueError("invalid proof-inputs.json")
    if label in data["checks"]:
        return data["checks"][label]
    family = (data.get("families") or {}).get(UI_FAMILY)
    if family and label.endswith(".js") and "/" not in label and " " not in label:
        try:
            return ui_family_recipe(root, label, family)
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
            return {"fresh": "the %s family could not resolve %s: %s" % (UI_FAMILY, label, exc)}
    return {"fresh": "input qualification not yet declared for " + label}


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
    receipt_verdict(path, item.argv[item.argv.index("--only-units") + 1], sha, allow_known_red)
    return {"path": str(path), "sha256": file_digest(path)}


def receipt_verdict(path, unit, sha, allow_known_red=False):
    """Refuse a receipt that is not one completed PASS of `unit` at `sha` (a KNOWN-RED one only
    when allowed): the one receipt check reuse() and coverage() share."""
    rows = [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]
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
                 current_commit=None, selection=None, seat=None, environment=None, as_recorded=None):
        refuse_duplicates(items)  # before anything is written: a saved plan never holds one
        self.root, self.logdir = str(root), Path(logdir)
        # seat(item, snapshot): the read_seat (seat_reader) for one check, so reuse() can accept
        # a pass made in another checkout that still stands there (seat_gap). None: never.
        self.seat = seat
        # as_recorded(item, environment, root, snapshot): (identity, read_seat) of one check here,
        # read under the caller environment a saved run in checkout `root` recorded (RECORDED
        # ENVIRONMENT below). None: a saved run is read only under this run's own environment.
        self.as_recorded = as_recorded
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
        if selection is not None:
            # What the plan was selected from (proof-run.py selected_by): not an input of any
            # check, so exact resume (reuse) compares items, never this.
            self.plan["selection"] = selection
        if environment is not None:
            # The caller environment every identity of this plan was read from (RECORDED
            # ENVIRONMENT below): not an input itself, the values its digests were taken of.
            self.plan["environment"] = environment
        atomic(self.logdir / "plan.json", self.plan)
        self.results = {}
        for item in items:
            item.evidence = self
        self.checkpoint(items)

    def launched(self, item):
        """Save a check as it starts. Its row records only that it is running: no pass is judged
        on it (the completion's checkpoint reads the source for that), so it needs HEAD alone,
        which is cheap, and never the whole source. Reading the whole source here (`git diff
        --binary HEAD` and every untracked file, 5.5 s each on step 2's 124-path land replay) cost
        one reading per started check, one after another, before each check could start."""
        commit = self.current_commit()
        self.save(item, self.source if commit == self.source["commit"] else dict(self.source, commit=commit))

    def checkpoint(self, items):
        # ONE SOURCE READING PER PASS (verification reliability PRD step 3, T14). The source is
        # the whole checkout (`git diff --binary HEAD` and every untracked file), and it was read
        # once per changed check: on the round 32 land replay one reading took 5.5 s, and the
        # first pass saves every planned check, so 88 checks waited about 8 minutes before the
        # first one started, in every round. Every check this pass saves is saved at one instant;
        # the reading is taken only when a pass has something to save.
        source = None
        for item in items:
            if item.label not in self.identities:
                continue  # Synthetic run findings never become reusable obligations.
            old = self.results.get(item.label, {})
            if old.get("state") == item.state and old.get("exit") == item.rc:
                continue
            if source is None:
                source = self.current_source()
            self.save(item, source)

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
                    now = self.current_identity(item)
                    if now != self.identities[item.label]:
                        raise ValueError("execution inputs changed since the run was planned"
                                         + self.explain(item) + identity_changed_keys(self.identities[item.label], now))
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
                    raise ValueError("inputs changed while they were being read" + self.explain(item)
                                     + identity_changed_keys(identities[item.label], again[item.label]))
                if identities[item.label] != row["input"]:
                    raise ValueError("inputs changed since the run was planned (found when the run finished)"
                                     + self.explain(item) + identity_changed_keys(row["input"], identities[item.label]))
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
                owner, description = {}, "owner is publishing its identity"
            # The owner queued for the engine slot (mark_slot_wait): the joiner waits for the slot too.
            item.owner_slot_wait = owner.get("engine_slot_wait") is True
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

    def mark_slot_wait(self, labels):
        """Publish, in each held row, whether this run holds that check back for the engine slot.
        A run joining the check reads it (claim) and counts that time as slot wait, never against
        its run cap or its admission wait. Written only when it changes."""
        for label, (_fd, path, row) in self.held.items():
            waiting = label in labels
            if row.get("engine_slot_wait", False) != waiting:
                row["engine_slot_wait"] = waiting
                atomic(path, row)

    def finish(self, item):
        held = self.held.pop(item.label, None)
        if held is None:
            return
        fd, path, row = held
        try:
            row["state"] = item.state
            row.pop("engine_slot_wait", None)
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


# A PASS WITH THE MUTATION PASSES ON STANDS IN FOR THE SAME CHECK WITH THEM OFF (decided by the
# lead, esc-20261015T175325Z-3b86bfa5; ported 2026-10-20 from held cc/zach-opus-reuse58c). The
# merge gate turns them off (autocheck MUTATION_SWITCH: both names "0"); the author's handover run
# leaves them as its shell has them, unset. Every consumer skips its mutation pass only when the
# value is "0" (`= 0` prints NOT RUN) or runs it only when it is "1", so a run with any value but
# "0" ran every assertion the "0" run does, and more. The qualified recipes that bind these
# names (the engine hook suites in proof-inputs.json) were therefore never reused from the
# author's offered run at the land. A saved identity that differs from the wanted one only in
# these names, each that differs being "0" in the wanted one and not "0" in the saved one,
# stands in for it. Never the other way round: a pass with them off proves nothing about them.
MUTATION_SWITCHES = ("RICHOS_MUTATION_PASSES", "RICHOS_FOURTEEN_MUTANTS")
_MUTATION_OFF = digest("0")


# THE CARGO CACHE ROOT, NAMED OR NOT, IS THE ONE THE MACHINE'S SETTINGS GIVE (2026-10-21). The
# land of cc/echo-opus-lease1x refused 55 of the author's unchanged passes as "declared execution
# inputs changed (environment.CARGO_TARGET_DIR: saved 98224b0764c9..., now absent)": the author
# exported the documented value (what `machine_settings.py cache cargo-target --key
# CARGO_TARGET_DIR` prints), the land names none. With neither name set, the Cargo shim
# (cargo_identity.target_dir), the shared build cache and the nightly (gate_environment) all take
# exactly that root from the machine's settings; RICHOS_CARGO_CACHE_ROOT is the same setting's
# other spelling (machine_settings.storage). So an environment whose every Cargo root name names
# that root reads as one that names none. Any other root is a real difference and still counts.
CARGO_ROOT_NAMES = ("CARGO_TARGET_DIR", "RICHOS_CARGO_CACHE_ROOT")


def documented_cargo_root(environment=None):
    """The Cargo cache root the machine's settings give `environment` (this process's by
    default) when it sets no Cargo root name: what `machine_settings.py cache cargo-target --key
    CARGO_TARGET_DIR` prints then. None when the settings cannot say (an unmounted volume)."""
    env = {k: v for k, v in (os.environ if environment is None else environment).items()
           if k not in CARGO_ROOT_NAMES}
    try:
        import machine_storage
        return str(machine_storage.cache("cargo-target", key="CARGO_TARGET_DIR", environment=env))
    except (ImportError, ValueError, OSError):
        return None


def cargo_root_as_documented(environment):
    """An identity's `environment` (name: digest) without its Cargo root names when every one of
    them names the documented root (above); otherwise unchanged."""
    named = [environment[name] for name in CARGO_ROOT_NAMES if name in environment]
    root = documented_cargo_root() if named else None
    if root is None or any(value != digest(root) for value in named):
        return environment
    return {k: v for k, v in environment.items() if k not in CARGO_ROOT_NAMES}


def stands_in(saved, wanted):
    """Does a pass saved under identity `saved` stand for one under `wanted` (above)?

    A whole-checkout identity never binds a RUN_SCOPED_ENVIRONMENT name (checkout_identity), so
    one saved before a name joined that set is read without it: the 2026-10-21 handover run bound
    RICHOS_AGENT_OWNER and RICHOS_AGENT_SESSION, and its passes are as valid as they were. A
    reviewed recipe binds exactly the names it declares, so its identity is compared whole."""
    if saved == wanted:
        return True
    if not isinstance(saved, dict) or not isinstance(wanted, dict):
        return False
    have, want = saved.get("environment"), wanted.get("environment")
    if not isinstance(have, dict) or not isinstance(want, dict):
        return False
    if saved.get("contract") == WHOLE_CHECKOUT == wanted.get("contract"):
        have = {k: v for k, v in have.items() if k not in RUN_SCOPED_ENVIRONMENT}
        want = {k: v for k, v in want.items() if k not in RUN_SCOPED_ENVIRONMENT}
    have, want = cargo_root_as_documented(have), cargo_root_as_documented(want)
    if ({k: v for k, v in saved.items() if k != "environment"}
            != {k: v for k, v in wanted.items() if k != "environment"}):
        return False
    rest = lambda values: {k: v for k, v in values.items() if k not in MUTATION_SWITCHES}
    if rest(have) != rest(want):
        return False
    return all(have.get(name) == want.get(name) or (want.get(name) == _MUTATION_OFF and have.get(name) != _MUTATION_OFF)
               for name in MUTATION_SWITCHES)


_ABSENT = object()


def _shown(value):
    """A field's value, short enough for one line of a refusal: a digest by its first 12 characters."""
    if value is _ABSENT:
        return "absent"
    text = value if isinstance(value, str) else json.dumps(value, sort_keys=True)
    return text[:12] + "..." if re.fullmatch(r"[0-9a-f]{40,}", text) else (text[:72] + "..." if len(text) > 75 else text)


def identity_difference(saved, wanted, limit=6):
    """Which identity fields keep a pass saved under `saved` from standing for one under `wanted`
    (stands_in), each as `field: saved X, now Y`, or "" when they are not two identities.

    THE MISSING DIAGNOSTIC (2026-10-21). "declared execution inputs changed" named no field, so
    two lands in a row refused 21 unchanged passes and the reason had to be guessed at from a
    reproduction that was not the land. A difference stands_in excuses is not listed."""
    if not isinstance(saved, dict) or not isinstance(wanted, dict):
        return ""
    rows = []

    def walk(path, have, want):
        if have == want:
            return
        if isinstance(have, dict) and isinstance(want, dict):
            for key in sorted(set(have) | set(want), key=str):
                walk(path + [str(key)], have.get(key, _ABSENT), want.get(key, _ABSENT))
        else:
            rows.append((path, have, want))

    if isinstance(saved.get("environment"), dict) and isinstance(wanted.get("environment"), dict):
        saved = {**saved, "environment": cargo_root_as_documented(saved["environment"])}
        wanted = {**wanted, "environment": cargo_root_as_documented(wanted["environment"])}
    walk([], saved, wanted)
    whole = saved.get("contract") == WHOLE_CHECKOUT == wanted.get("contract")
    shown = []
    for path, have, want in rows:
        name = path[1] if len(path) == 2 and path[0] == "environment" else None
        if name and whole and name in RUN_SCOPED_ENVIRONMENT:
            continue
        if name in MUTATION_SWITCHES and want == _MUTATION_OFF and have != _MUTATION_OFF:
            continue
        shown.append("%s: saved %s, now %s" % (".".join(path), _shown(have), _shown(want)))
    if not shown:
        return ""
    more = " and %d more" % (len(shown) - limit) if len(shown) > limit else ""
    return " (" + "; ".join(shown[:limit]) + more + ")"


# A WHOLE-CHECKOUT PASS MADE IN ANOTHER CHECKOUT STANDS HERE WHEN IT STILL STANDS THERE (2026-10-21).
#
# The land of cc/zach-opus-reuse3 (richos main 06d2ae7af): the readiness preflight read the
# author's handover run 20261021T051618Z-iscu0cdi as proving the branch (asked in the author's
# own checkout, where it does), and the gate, offered that run, then executed every one of its 23
# checks again: 1238.9 s of serial work, "23 planned, 23 passed (0 of them reused)"
# (attempt-t40c12wk). Every whole-checkout identity names where its checkout is: `checkout`, the
# installs bound by place (`installed.checkout` by name, `installed.playwright_package` by
# candidate path) and, for a check whose PATH carries the checkout's own Cargo shim, its
# environment. A pass made in the author's worktree could never stand in the main checkout the
# gate runs in, however identical the bytes.
#
# The place is not dropped; it is read where the pass was made. A pass saved in another checkout
# (its seat) stands for this check here only when ALL of these hold:
#   * both are whole-checkout identities, and both checkouts hold the same content (`tree`:
#     every byte git would land, untracked files included);
#   * the same command and runner settings;
#   * the pass still stands in its seat, read now in the seat (seat_reader) with this check's
#     environment placed there: what the preflight asks, asked again by the consumer;
#   * read there and read here, the two checkouts agree on everything but their place: tools,
#     platform, browser installs, Rust toolchain, and the dependency installs each would load,
#     by content (portable_dependencies);
#   * the receipt and log it saved are intact (reuse() checks them as for any pass).
# What git ignores beyond the dependency installs (build and bytecode caches, test output, hook
# sidecars) is bound in neither checkout, as it is bound in no single checkout across runs.
SEAT_INSTALLS = ("checkout", "playwright_package")


def placed(value, seat, here):
    """`value` with the checkout path `seat` named as `here` instead, in every string (each entry
    of a PATH-like list too) and every key: the same reading of a check, made in another place."""
    if isinstance(value, dict):
        return {placed(k, seat, here): placed(v, seat, here) for k, v in value.items()}
    if isinstance(value, list):
        return [placed(v, seat, here) for v in value]
    if isinstance(value, str) and seat in value:
        return os.pathsep.join(here + entry[len(seat):] if entry == seat or entry.startswith(seat + os.sep) else entry
                               for entry in value.split(os.pathsep))
    return value


def _beside_the_place(identity):
    """`identity` without what depends on where its checkout is (see above)."""
    rest = {k: v for k, v in identity.items() if k not in ("checkout", "environment", "command", "settings")}
    if isinstance(identity.get("installed"), dict):
        rest["installed"] = {k: v for k, v in identity["installed"].items() if k not in SEAT_INSTALLS}
    return rest


def made_elsewhere(saved, wanted):
    """Are `saved` and `wanted` whole-checkout identities of two different checkouts?"""
    return (isinstance(saved, dict) and isinstance(wanted, dict)
            and saved.get("contract") == WHOLE_CHECKOUT == wanted.get("contract")
            and bool(saved.get("checkout")) and saved.get("checkout") != wanted.get("checkout"))


def seat_difference(saved, wanted):
    """None when a pass saved under `saved` in another checkout may stand for `wanted` here as far
    as the two identities alone say (above), else why not. Reads nothing; the rest is seat_gap's."""
    if not made_elsewhere(saved, wanted):
        return "not a whole-checkout pass made in another checkout"
    if saved.get("tree") != wanted.get("tree"):
        return "the two checkouts' content differs"
    if saved.get("command") != wanted.get("command") or saved.get("settings") != wanted.get("settings"):
        return "another command or runner setting"
    return None


def seat_gap(saved, wanted, read_seat):
    """None when the whole-checkout pass saved under `saved` in another checkout stands for the
    check whose identity here is `wanted` (above), else why not. `read_seat(seat)` reads that
    checkout now (seat_reader)."""
    gap = seat_difference(saved, wanted)
    if gap:
        return gap
    seat, here = saved["checkout"], wanted["checkout"]
    try:
        there, installs_there, installs_here = read_seat(seat)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        return "its checkout %s cannot be read now: %s" % (seat, exc)
    made = {k: v for k, v in saved.items() if k not in ("command", "settings")}
    if not stands_in(made, there):
        return "it no longer stands in its own checkout %s%s" % (seat, identity_difference(made, there))
    if _beside_the_place(placed(there, seat, here)) != _beside_the_place(wanted):
        return "the two checkouts differ beyond their place%s" % identity_difference(
            _beside_the_place(placed(there, seat, here)), _beside_the_place(wanted))
    if installs_there != installs_here:
        return "the two checkouts load different dependency installs"
    return None


def seat_reader(here, argv, environment, snapshot):
    """read_seat for seat_gap: the whole-checkout identity of the check run as `argv` with this
    checkout's `environment` placed in the seat, and the dependency installs, by content
    (portable_dependencies), that the seat and `here` would each load."""
    here = str(Path(here).resolve())

    def read(seat):
        seat = str(Path(seat).resolve())
        there = placed(environment, here, seat)
        return (checkout_identity(seat, argv, there, snapshot),
                snapshot.portable(seat, there), snapshot.portable(here, environment))
    return read


def evidence_gap(label, old, identity, plan, exact=False, source=None, read_seat=None):
    """Why the saved outcome `old` cannot stand for check `label` whose inputs read `identity`
    now, or None. The one predicate reuse() and coverage() share. `read_seat`: a pass saved in
    another checkout may stand when it still stands there (seat_gap)."""
    if label == "engine receipts":
        return "coverage verifier runs over the reconciled receipt set"
    if identity.get("fresh"):
        return identity["fresh"]
    if old.get("state") != "passed" or old.get("exit") != 0 or old.get("invalid"):
        return "no validated passing execution"
    if exact and plan["source"] != source:
        return "source identity changed"
    if (old.get("source") or {}).get("commit") != plan["source"]["commit"]:
        # Only HEAD: a pass saved after an unrelated edit is valid for its own declared
        # inputs (Record says why), and the input identity is what decides it.
        return "the checkout's commit changed during the original execution"
    if not stands_in(old.get("input"), identity):
        changed = "declared execution inputs changed" + identity_difference(old.get("input"), identity)
        if read_seat is None or not made_elsewhere(old.get("input"), identity):
            return changed
        moved = seat_gap(old.get("input"), identity, read_seat)
        return None if moved is None else "%s; made in another checkout, where %s" % (changed, moved)
    return None


def _saved_artifacts_hold(old):
    """None when a saved pass's log and receipt are the bytes it recorded, else why not."""
    try:
        if file_digest(old["log"]) != old["log_sha256"]:
            return "result log changed"
        if old.get("receipt") and file_digest(old["receipt"]["path"]) != old["receipt"]["sha256"]:
            return "receipt changed"
    except (OSError, KeyError, TypeError) as exc:
        return "saved evidence failed validation: " + str(exc)
    return None


KNOWN_RED_GAP = "known-red:"
# A check reuse() never accepts (a `fresh` one, and `engine receipts`) executes wherever it is
# required; no saved run proves it, whatever source it ran on, so coverage names it apart and
# never "proven" (review rv-20261020T083937Z-2f73e49a-734e: verification-pins validates
# external readers no identity binds, and a source-only match called it proven).
FRESH_GAP = "fresh at every seat:"


def _receipt_proves(item, old):
    """None when an engine unit's saved receipt is a completed PASS, exactly as reuse() demands
    (receipt_verdict); a KNOWN-RED one is the declared exception, named apart, never proof
    (review rv-20261020T073921Z-1e0461cc-0956 #2)."""
    if not getattr(item, "engine_unit", False) or not old.get("receipt"):
        return None
    unit = item.argv[item.argv.index("--only-units") + 1]
    sha = old.get("receipt_sha", (old.get("source") or {}).get("commit"))
    try:
        receipt_verdict(old["receipt"]["path"], unit, sha)
        return None
    except (OSError, ValueError, KeyError, TypeError) as exc:
        try:
            receipt_verdict(old["receipt"]["path"], unit, sha, allow_known_red=True)
        except (OSError, ValueError, KeyError, TypeError):
            return "saved evidence failed validation: " + str(exc)
        return KNOWN_RED_GAP + " a declared exception the run let through, not proof"


def coverage(previous, items, identities, source):
    """{check: None when the saved run at `previous` proves it for the inputs read NOW, else why
    not}, for every planned item. Reads only: nothing is copied, run or written.

    THE READINESS PREFLIGHT'S QUESTION (verification reliability PRD, R3/A8): does the author's
    saved run cover its candidate's whole selected plan? Asked in the author's own checkout, so
    a check keyed by that checkout (cargo, packaging) is judged on the bytes it ran on, exactly as
    reuse() would judge it there. It never says proven where reuse() would refuse: a `fresh`
    check and `engine receipts` are never reused, so one the run PASSED is named apart
    (FRESH_GAP), neither proven nor missing: it executes at every seat. One the run failed, timed
    out or never finished is unresolved like any other check, so it stays missing and goes into
    the recovery job (review rv-20261020T163601Z-4b96c956-ed75 #1: a failed one was hidden)."""
    previous = Path(previous)
    plan = read_plan(previous)
    try:
        outcomes = json.loads((previous / "outcomes.json").read_text())
    except FileNotFoundError:
        outcomes = {}
    found = {}
    for item in items:
        old = outcomes.get(item.label, {})
        identity = identities[item.label]
        if item.label == "engine receipts" or identity.get("fresh"):
            if old.get("state") == "passed" and old.get("exit") == 0 and not old.get("invalid"):
                found[item.label] = "%s %s" % (FRESH_GAP, evidence_gap(item.label, old, identity, plan))
            else:
                found[item.label] = "no validated passing execution"
            continue
        found[item.label] = (evidence_gap(item.label, old, identity, plan) or _saved_artifacts_hold(old)
                             or _receipt_proves(item, old))
    return found


# THE RECORDED ENVIRONMENT (2026-10-22). The land of codex/first-run-reliability-20261022 at
# 4494536289 refused the author's reconciled run five times, every time for values the two
# shells held and the code did not ("declared execution inputs changed (environment.LANG,
# LC_ALL, LC_CTYPE, CARGO_TARGET_DIR, RUST_LOG, SCCACHE_DIR, RICHOS_PRIVATE_FIXTURES, ...,
# PATH, SDKROOT)"), and the lead copied the author's values into his own shell by hand between
# attempts. One name at a time had been excused before (RUN_SCOPED_ENVIRONMENT, the Cargo root,
# the mutation switches); the next shell always held another. So a run saves, in its plan, the
# caller environment its identities were read from (proof-run.py recorded_environment), and a
# pass that does not stand under this run's environment is judged again here under that one:
# every file, tool, install and Git input is read now, so a real change still executes; only
# what the shell held is taken as the run that made the pass held it. A run saved before this
# records no environment in its plan; its rerun files (proof-run.py write_rerun) hold the values
# each check it executed was started with, and run_environment takes the one that reproduces the
# most of its saved passes' digests. A value withheld as a credential is read under this run's.
_RERUN_VALUE = re.compile(r"^\s*(\S.*?)\s+\\$")
_RERUN_WITHHELD = re.compile(r'^\s*\$\{([A-Za-z_][A-Za-z0-9_]*)\+"')


def rerun_environment(path):
    """{"values", "withheld", "argv"} of one rerun file: the `env -i` assignments it starts its
    check with, and the check's command (after the host shield's `--`)."""
    values, withheld, inside, argv = {}, [], False, []
    for line in Path(path).read_text(errors="replace").splitlines():
        if not inside:
            inside = line.rstrip().endswith("env -i \\")
            continue
        found = _RERUN_VALUE.match(line)
        if not found:
            try:
                words = shlex.split(line)
            except ValueError:
                words = []
            argv = words[words.index("--") + 1:] if "--" in words else []
            break
        named = _RERUN_WITHHELD.match(line)
        if named:
            withheld.append(named.group(1))
            continue
        words = shlex.split(found.group(1))
        if len(words) != 1 or "=" not in words[0]:
            break
        name, value = words[0].split("=", 1)
        values[name] = value
    return {"values": values, "withheld": withheld, "argv": argv} if inside else None


def _under_nightly_conditions(argv):
    """May this command run under a nightly gate's conditions, which replace caller values such
    as PATH (proof-run.py nightly_gate_of: a UI suite, a run-tests.sh --only suite)?"""
    return ((len(argv) == 2 and argv[0] == "node" and argv[1].endswith(".js"))
            or (argv[:1] == ["scripts/run-tests.sh"] and "--only" in argv))


def _bound(name):
    return name not in RUN_SCOPED_ENVIRONMENT and (name in CHECKOUT_ENVIRONMENT_NAMES
                                                   or name.startswith(CHECKOUT_ENVIRONMENT_PREFIXES))


def run_environment(run):
    """The caller environment the saved `run` read its identities from: its plan's own record
    (proof-run.py recorded_environment), or for a plan saved before it recorded one, rebuilt from
    the rerun files of that run and of the runs it reused from. None when neither says anything.

    A rerun file holds one check's whole environment: the caller's, plus what the runner set
    for that check (the Cargo settings; for a suite a nightly gate runs, that gate's values,
    which replace the caller's PATH among others). So of the names a whole-checkout identity
    binds, the caller held exactly those every whole-checkout pass holds (a name some pass lacks
    was set for other checks), each with its most common value among the files of checks run
    outside the nightly's conditions. 20261022T131701Z-vjaoaa1d: 23 of its 33 whole-checkout
    passes ran under them, with the build's PATH; the caller's PATH is the one the other 10 hold.
    Names no such identity binds come from the run's own file. Never a verdict: every input is
    still read under it, and a value that is not the run's only makes its passes read changed."""
    run = Path(run)
    plan = read_plan(run)
    if isinstance(plan.get("environment"), dict):
        return plan["environment"]
    try:
        outcomes = json.loads((run / "outcomes.json").read_text())
    except (OSError, ValueError):
        return None
    passes = [row["input"]["environment"] for row in outcomes.values()
              if isinstance(row, dict) and row.get("state") == "passed" and isinstance(row.get("input"), dict)
              and row["input"].get("contract") == WHOLE_CHECKOUT and isinstance(row["input"].get("environment"), dict)]
    runs = [run]
    for row in outcomes.values():
        provenance = row.get("provenance") if isinstance(row, dict) else None
        while isinstance(provenance, dict):
            if provenance.get("run") and Path(provenance["run"]) not in runs:
                runs.append(Path(provenance["run"]))
            provenance = provenance.get("earlier")
    found = []
    for directory in runs:
        for path in sorted((directory / "rerun").glob("*.sh")):
            try:
                one = rerun_environment(path)
            except OSError:
                continue
            if one:
                found.append(one)
    if not found or not passes:
        return None
    outside = [one for one in found if not _under_nightly_conditions(one["argv"])] or found
    values = {name: value for name, value in found[0]["values"].items() if not _bound(name)}
    for name in set.intersection(*(set(environment) for environment in passes)):
        held = [one["values"][name] for one in outside if name in one["values"]]
        if _bound(name) and held:
            values[name] = max(dict.fromkeys(held), key=held.count)
    withheld = sorted({name for one in found for name in one["withheld"]})
    return {"values": values, "withheld": withheld}


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
    # One reading of another checkout per call (InputSnapshot: never shared across passes).
    snapshot = InputSnapshot()
    recorded = _ABSENT   # run_environment(previous), read once and only when a pass needs it
    for item in items:
        if item.state == "passed":
            continue
        old = outcomes.get(item.label, {})
        item.retry_first = old.get("state") != "passed"
        identity = record.identities[item.label]
        seat = getattr(record, "seat", None)
        read_seat = seat(item, snapshot) if seat is not None and not exact else None
        as_recorded, by_record = getattr(record, "as_recorded", None), False
        # A pass once accepted under a recorded environment is judged ever after by the identity
        # and environment that produced it, which its provenance carries, never by the identity
        # the accepting run saved (review rv-20261022T143750Z-fba13982-f9d7 #1: reused through a
        # land, a pass whose author's tool had since changed was taken without reading it again).
        produced = old.get("provenance") if isinstance(old.get("provenance"), dict) else None
        produced = produced if produced and isinstance(produced.get("recorded"), dict) else None
        judged = dict(old, input=produced["input"]) if produced else old
        if produced:
            reason = "made under the environment of the run that produced it"
        else:
            reason = evidence_gap(item.label, old, identity, plan, exact, record.source, read_seat)
        if reason and as_recorded is not None and not identity.get("fresh") and old.get("state") == "passed":
            if produced:
                environment, root = produced["recorded"], produced.get("root")
            else:
                if recorded is _ABSENT:
                    recorded = run_environment(previous)
                environment, root = recorded, plan["root"]
            if environment and root:
                # RECORDED ENVIRONMENT: the pass is judged as the run that made it started its checks.
                try:
                    identity, read_seat = as_recorded(item, environment, root, snapshot)
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
                    reason += "; unreadable under the environment its run recorded: %s" % exc
                else:
                    again = evidence_gap(item.label, judged, identity, plan, exact, record.source,
                                         read_seat if not exact else None)
                    reason, by_record = again and again + " (read under the environment its run recorded)", not again
        elsewhere = reason is None and made_elsewhere(judged.get("input"), identity)
        if reason is None:
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
                                   "input": judged["input"], "earlier": old.get("provenance")}
                if elsewhere:
                    item.provenance["seat"] = judged["input"]["checkout"]
                if by_record:
                    # The producing identity (`input`), environment and checkout, carried on to
                    # every later reuse and read again by verify_target_receipts.
                    item.provenance.update(recorded=environment, root=root)
                item.reused_from = str(previous)
                item.notes.append("validated evidence reused from " + str(previous) + (
                    " (made in %s, where it still stands, on the same content and installs)" % judged["input"]["checkout"]
                    if elsewhere else "") + (" (read under the environment its run recorded)" if by_record else ""))
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
            # A pass made in another checkout: the runner read that checkout when it reused it
            # (seat_gap); what this verifier can read for itself is that the two identities say
            # the same content, command and settings, and that the seat is the one it names.
            elsewhere = (provenance and provenance.get("seat")
                         and provenance.get("seat") == (provenance.get("input") or {}).get("checkout")
                         and seat_difference(provenance.get("input"), identity) is None)
            # A pass reused under the environment its run recorded (THE RECORDED ENVIRONMENT):
            # read again here, under that environment, exactly as reuse() read it.
            recorded = False
            if (provenance and isinstance(provenance.get("recorded"), dict) and not elsewhere
                    and hasattr(runner, "identity_as_recorded")):
                try:
                    again, _seat = runner.identity_as_recorded(item, provenance["recorded"], provenance["root"],
                        snapshot, SimpleNamespace(**identity["settings"]), directory)
                    recorded = stands_in(provenance.get("input"), again)
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                    recorded = False
            if (identity.get("fresh") or not provenance
                    or not (stands_in(provenance.get("input"), identity) or elsewhere or recorded)
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

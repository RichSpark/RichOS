"""build_caches.py: the build output RichOS keeps on the external drive, and when it is garbage.

WHY THIS EXISTS (2026-10-08). /Volumes/E1TB was 98% full, three weeks after it was added:
930 of 954 GB used, 824 GB of it under caches/. Measured that morning:

  cargo-target/workspaces              183 GB  one folder per checkout that ever ran Cargo
  cargo-target/shared-build-v1         121 GB  dependency units every checkout shares
  cargo-target/{richos-app,...}/       175 GB  three more Cargo trees, each with both of the above
  cargo-target/{debug,release}          44 GB  a Cargo tree from builds without the wrapper
  richos-native-ios{,-proof,-ui}       142 GB  one folder per checkout that ran an iOS build
  richos-native-android                 30 GB  one folder per checkout that ran an Android build
  richos-claude-quota                   12 GB  a Cargo tree last built on 2026-09-25

NOTHING EVER REMOVED ANY OF IT. Each tool keys its folder by a hash of the checkout's path
(lib/cargo_identity.py, bin/rios, bin/randroid, the native test scripts), so a checkout that is
landed and deleted leaves its folder behind with nothing pointing at it, forever. And Cargo
never deletes a dependency unit it no longer uses: every version bump, feature change and
toolchain update adds a unit and keeps the old one. The scratch reaper covered temp folders,
never these caches.

TWO RULES, BOTH TAKEN BY THE SCHEDULED SWEEP (scratch-reaper.py scan_build_caches):

  1. A FOLDER KEYED TO A CHECKOUT THAT NO LONGER EXISTS is deleted. The key is recomputed
     here from every live checkout exactly as each tool computes it; a folder whose key no
     live checkout produces is an orphan. It is kept while anything inside it was written or
     read within SCRATCH_BUILD_CACHE_ORPHAN_HOURS (a copy of a checkout outside git, a test
     fixture building right now) or while a Cargo build holds its lock.

  2. A CARGO UNIT NO BUILD HAS READ FOR SCRATCH_BUILD_CACHE_IDLE_HOURS is deleted. Cargo reads
     every unit's fingerprint at every build that depends on it, so a unit's last access time
     is the last time any build needed it (measured on this drive: atime is updated). A unit
     that is deleted and needed again is rebuilt by Cargo, which checks that its outputs
     exist. The profile directory's Cargo lock is held for the deletion, so no build runs
     while its units are being removed, and a build that holds it keeps all of them.

THE LANDER TAKES A WORKSPACE'S OWN FOLDERS AT THE MOMENT IT IS DELETED (owned_paths, used by
mega-lander/workspaces.py), so rule 1 only catches what that missed: a crashed agent, a
workspace removed some other way, and everything from before 2026-10-08.

NO SIZE CAP ANYWHERE. What stays is what is in use or was used recently.
"""
import fcntl
import hashlib
import os
import re
import shutil
import stat
import subprocess
import time
import machine_settings as M

HOUR = 3600

# --- where each tool puts a checkout's folder, and how it names it ------------------------
#
# (family, directory relative to the cache root, name pattern). The pattern's `key` group is
# the hash; the rest of the name is the tool's own suffix. Folders in these directories whose
# names do not match (gradle/, physical-store/, phone-sessions.jsonl) are never touched.
KEYED_FAMILIES = (
    # bin/rios: sha256(<checkout>/richos/mobile/native-ios)[:10]
    # native-ios-lab-phone.test.sh: lab-phone-<sha1(<checkout>)[:10]>
    ("ios", "richos-native-ios", re.compile(r"^(?:lab-phone-)?(?P<key>[0-9a-f]{10})$")),
    # native-ios-{core,app,share}.test.sh: sha256(<checkout>)[:10], -app, -share
    ("ios-proof", "richos-native-ios-proof", re.compile(r"^(?P<key>[0-9a-f]{10})(?:-app|-share)?$")),
    # native-ios-ui.test.sh: sha256(<checkout>/richos/mobile/native-ios)[:10]
    ("ios-ui", "richos-native-ios-ui", re.compile(r"^(?P<key>[0-9a-f]{10})$")),
    # bin/randroid: sha1(<checkout>/richos/mobile/native-android)[:10]
    ("android", "richos-native-android", re.compile(r"^(?P<key>[0-9a-f]{10})$")),
    # native-android-ui.test.sh: ui-shots/sha1(<checkout>/richos/mobile/native-android)[:10]
    ("android-shots", "richos-native-android/ui-shots", re.compile(r"^(?P<key>[0-9a-f]{10})$")),
)
# lib/cargo_identity.py: <cache root>/workspaces/sha256(<Cargo workspace root>)[:24]. A
# directory named `workspaces` anywhere in a declared Cargo tree, because a build given
# --target-dir <tree>/codex makes <tree>/codex/workspaces/<key>.
CARGO_KEY = re.compile(r"^(?P<key>[0-9a-f]{24})$")
CARGO_SEARCH_DEPTH = 4

# Inside a profile directory: the unit directories and the unit files.
UNIT_DIRS = (".fingerprint", "build", "incremental")
UNIT_FILE_DIRS = ("deps", "examples")
# <name>-<16 hex>, optionally lib-prefixed, optionally followed by .<anything>. Cargo writes
# a unit's metadata hash into every file and directory the unit owns.
UNIT_NAME = re.compile(r"^(?:lib)?(?P<name>[A-Za-z0-9_-]+?)-(?P<hash>[0-9a-f]{16})(?:\..*)?$")
PROFILE_SKIP = set(UNIT_DIRS + UNIT_FILE_DIRS)


def anchors(checkout):
    """Every path a tool may hash for this checkout: the checkout itself, its native app
    folders and every directory holding a Cargo.toml, each as written and resolved."""
    out = [checkout]
    for rel in ("richos/mobile/native-ios", "richos/mobile/native-android",
                "mobile/native-ios", "mobile/native-android"):
        out.append(os.path.join(checkout, rel))
    try:
        r = subprocess.run(["git", "-C", checkout, "ls-files", "-z", "--", "*Cargo.toml"],
                           capture_output=True, timeout=60)
        if r.returncode == 0:
            for rel in r.stdout.decode("utf-8", "replace").split("\0"):
                if rel:
                    out.append(os.path.dirname(os.path.join(checkout, rel)))
    except (OSError, subprocess.SubprocessError):
        pass
    both = []
    for p in out:
        for q in (p.rstrip("/"), os.path.realpath(p)):
            if q not in both:
                both.append(q)
    return both


def keys_for(paths):
    """Every key any tool derives from any of these paths."""
    keys = set()
    for p in paths:
        raw = os.fsencode(p)
        s256 = hashlib.sha256(raw).hexdigest()
        keys.add(s256[:24])
        keys.add(s256[:10])
        keys.add(hashlib.sha1(raw).hexdigest()[:10])
    return keys


def cargo_workspace_dirs(cargo_roots):
    """Every `workspaces` directory in the declared Cargo trees."""
    found = []
    for root in cargo_roots:
        stack = [(root, 0)]
        while stack:
            cur, depth = stack.pop()
            try:
                entries = list(os.scandir(cur))
            except OSError:
                continue
            for e in entries:
                if not e.is_dir(follow_symlinks=False):
                    continue
                if e.name == "workspaces":
                    found.append(e.path)
                elif depth + 1 < CARGO_SEARCH_DEPTH and e.name not in PROFILE_SKIP \
                        and not CARGO_KEY.match(e.name):
                    stack.append((e.path, depth + 1))
    return sorted(found)


def keyed_folders(cache_root, cargo_roots):
    """[(family, path, key)] for every folder a tool keyed to a checkout."""
    out = []
    for family, rel, pattern in KEYED_FAMILIES:
        d = os.path.join(cache_root, rel)
        try:
            names = sorted(os.listdir(d))
        except OSError:
            continue
        for name in names:
            m = pattern.match(name)
            path = os.path.join(d, name)
            if m and os.path.isdir(path) and not os.path.islink(path):
                out.append((family, path, m.group("key")))
    for d in cargo_workspace_dirs(cargo_roots):
        try:
            names = sorted(os.listdir(d))
        except OSError:
            continue
        for name in names:
            m = CARGO_KEY.match(name)
            path = os.path.join(d, name)
            if m and os.path.isdir(path) and not os.path.islink(path):
                out.append(("cargo", path, m.group("key")))
    return out


def owned_paths(checkout, cache_root, cargo_roots, checkout_anchors=None):
    """The folders keyed to this one checkout. The lander calls it BEFORE the checkout is
    deleted (its Cargo.toml files are read from the tree) and removes them after."""
    keys = keys_for(checkout_anchors if checkout_anchors is not None else anchors(checkout))
    return [(f, p) for f, p, k in keyed_folders(cache_root, cargo_roots) if k in keys]


def live_checkouts(parents, extra=()):
    """(checkouts, unreadable): every git checkout under the declared parents, with all of
    its worktrees, plus `extra`. A repository whose worktree list cannot be read is named in
    `unreadable`, and the caller deletes no orphan while any is: a checkout it could not see
    would look exactly like a deleted one."""
    found, unreadable = set(), []
    if not parents:
        # Discover sibling repositories from actual registered Git roots and
        # this engine's primary checkout, never a fixed home/checkout layout.
        parents = set()
        for candidate in [os.path.dirname(__file__), *extra]:
            if not candidate or not os.path.isdir(candidate):
                continue
            try:
                result = subprocess.run(["git", "-C", candidate, "rev-parse", "--path-format=absolute", "--git-common-dir"],
                                        capture_output=True, text=True, timeout=30, env={**os.environ, "LC_ALL": "C"})
            except (OSError, subprocess.TimeoutExpired) as exc:
                unreadable.append("%s: %s" % (candidate, exc))
                continue
            if candidate == os.path.dirname(__file__) and result.returncode and "not a git repository" in result.stderr:
                continue  # extracted engine code has no project Git ownership
            common = result.stdout.strip()
            if result.returncode == 0 and os.path.basename(common) == ".git":
                parents.add(os.path.dirname(os.path.dirname(common)))
            else:
                unreadable.append("%s: cannot discover primary Git checkout" % candidate)
    for parent in parents:
        try:
            names = sorted(os.listdir(parent))
        except OSError as exc:
            unreadable.append("%s: %s" % (parent, exc))
            continue
        for name in names:
            repo = os.path.join(parent, name)
            if not os.path.isdir(repo) or os.path.islink(repo):
                continue
            found.add(repo)
            if not os.path.isdir(os.path.join(repo, ".git")):
                continue
            try:
                r = subprocess.run(["git", "-C", repo, "worktree", "list", "--porcelain"],
                                   capture_output=True, text=True, timeout=60)
            except (OSError, subprocess.SubprocessError) as exc:
                unreadable.append("%s: %s" % (repo, exc))
                continue
            if r.returncode != 0:
                unreadable.append("%s: %s" % (repo, (r.stderr or "").strip()[:200]))
                continue
            for line in r.stdout.splitlines():
                if line.startswith("worktree "):
                    found.add(line[len("worktree "):])
    # A registered path that no longer exists is history, not a checkout: the worktree
    # ledger keeps every workspace that ever was.
    for p in extra:
        if p and os.path.isdir(p):
            found.add(p)
    return sorted(found), unreadable


def _blocks(st, seen):
    """Bytes a file holds, counted once per inode: Cargo hard-links each binary from deps/
    to the profile directory, and counting both links reported 72 GB for a 43 GB tree."""
    if seen is not None and st.st_nlink > 1:
        key = (st.st_dev, st.st_ino)
        if key in seen:
            return 0
        seen.add(key)
    return st.st_blocks * 512


def walk(path, seen=None):
    """(bytes_on_disk, newest of mtime and atime, contains_git) for a tree. Reading a
    directory entry's metadata does not change its access time."""
    total, newest, has_git = 0, 0.0, False
    seen = set() if seen is None else seen
    try:
        st = os.lstat(path)
    except OSError:
        return 0, 0.0, False
    total = _blocks(st, seen)
    newest = max(st.st_mtime, st.st_atime) if not stat.S_ISDIR(st.st_mode) else st.st_mtime
    if not stat.S_ISDIR(st.st_mode):
        return total, newest, False
    stack = [path]
    while stack:
        cur = stack.pop()
        try:
            entries = list(os.scandir(cur))
        except OSError:
            continue
        for e in entries:
            if e.name == ".git":
                has_git = True
            try:
                st = e.stat(follow_symlinks=False)
            except OSError:
                continue
            total += _blocks(st, seen)
            if stat.S_ISDIR(st.st_mode):
                # A directory's atime moves when anything lists it, including this walk's
                # own scandir, so only its mtime counts.
                newest = max(newest, st.st_mtime)
                stack.append(e.path)
            else:
                newest = max(newest, st.st_mtime, st.st_atime)
    return total, newest, has_git


# --- Cargo's lock --------------------------------------------------------------------------

class CargoLock(object):
    """Cargo's own lock on a profile directory (<profile>/.cargo-lock, flock(2)), taken
    without waiting. `held` is True when this process has it; False when a build has it."""

    def __init__(self, profile):
        self.path = os.path.join(profile, ".cargo-lock")
        self.fd = None
        self.held = False

    def __enter__(self):
        try:
            # Created when missing, exactly as Cargo creates it, so a build that starts while
            # units are being removed waits for the lock instead of racing the removal.
            self.fd = os.open(self.path, os.O_RDONLY | os.O_CREAT, 0o644)
        except OSError:
            self.held = False
            return self
        try:
            fcntl.flock(self.fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.held = True
        except OSError:
            self.held = False
        return self

    def __exit__(self, *_exc):
        if self.fd is not None:
            try:
                if self.held:
                    fcntl.flock(self.fd, fcntl.LOCK_UN)
            finally:
                os.close(self.fd)
        return False


def busy_profiles(tree):
    """Profile directories inside `tree` whose Cargo lock a running build holds."""
    busy = []
    for profile in profile_dirs([tree], depth=4):
        with CargoLock(profile) as lock:
            if not lock.held:
                busy.append(profile)
    return busy


# --- rule 2: units no build has read -----------------------------------------------------

def profile_dirs(roots, depth=6, skip=()):
    """Every Cargo profile directory (one holding .fingerprint) under the roots, not inside
    any path in `skip`."""
    skip = tuple(os.path.join(s, "") for s in skip)
    found = []
    stack = [(r, 0) for r in roots]
    while stack:
        cur, d = stack.pop()
        if skip and os.path.join(cur, "").startswith(skip):
            continue
        if os.path.isdir(os.path.join(cur, ".fingerprint")):
            found.append(cur)
            continue
        if d >= depth:
            continue
        try:
            entries = list(os.scandir(cur))
        except OSError:
            continue
        for e in entries:
            if e.is_dir(follow_symlinks=False) and e.name not in PROFILE_SKIP:
                stack.append((e.path, d + 1))
    return sorted(found)


def _unit_key(name):
    # THE HASH ALONE. A test binary is named for its target (action_ledger_tests-<h>) and its
    # fingerprint for its package (richos-core-<h>); the metadata hash is the unit's own.
    m = UNIT_NAME.match(name)
    return ("unit", m.group("hash")) if m else None


def units(profile):
    """{key: {"paths": [...], "bytes": n, "newest": t}} for one profile directory.
    Each unit's newest is the latest write or read of any file it owns."""
    out = {}
    seen = set()

    def add(key, path, size, newest):
        u = out.setdefault(key, {"paths": [], "bytes": 0, "newest": 0.0})
        u["paths"].append(path)
        u["bytes"] += size
        u["newest"] = max(u["newest"], newest)

    for sub in UNIT_DIRS:
        base = os.path.join(profile, sub)
        try:
            entries = list(os.scandir(base))
        except OSError:
            continue
        for e in entries:
            if not e.is_dir(follow_symlinks=False):
                continue
            # An incremental directory is named for the crate and rustc's own crate hash,
            # never the unit's, so it is a unit of its own.
            key = ("incremental", e.name) if sub == "incremental" else _unit_key(e.name)
            if key is None:
                continue
            size, newest, _git = walk(e.path, seen)
            add(key, e.path, size, newest)
    for sub in UNIT_FILE_DIRS:
        base = os.path.join(profile, sub)
        try:
            entries = list(os.scandir(base))
        except OSError:
            continue
        for e in entries:
            key = _unit_key(e.name)
            if key is None:
                continue
            if e.is_dir(follow_symlinks=False):
                size, newest, _git = walk(e.path, seen)
            else:
                try:
                    st = e.stat(follow_symlinks=False)
                except OSError:
                    continue
                size, newest = _blocks(st, seen), max(st.st_mtime, st.st_atime)
            add(key, e.path, size, newest)
    return out


def idle_units(profile, idle_seconds, now=None):
    """[(key, unit)] no build has written or read within idle_seconds."""
    now = time.time() if now is None else now
    return sorted(((k, u) for k, u in units(profile).items()
                   if now - u["newest"] >= idle_seconds), key=lambda kv: kv[0])


def delete_units(profile, keys, idle_seconds, now=None):
    """Delete the named units of one profile directory under Cargo's lock, each re-measured
    first so a unit a build read since the plan is kept. Returns (deleted, freed, failures,
    why_not): why_not is set, and nothing is deleted, when a build holds the lock."""
    now = time.time() if now is None else now
    deleted, freed, failures = 0, 0, []
    with CargoLock(profile) as lock:
        if not lock.held:
            return 0, 0, [], "a running Cargo build holds %s" % lock.path
        current = units(profile)
        for key in keys:
            u = current.get(key)
            if u is None or now - u["newest"] < idle_seconds:
                continue
            ok = True
            for p in u["paths"]:
                try:
                    if os.path.isdir(p) and not os.path.islink(p):
                        shutil.rmtree(p)
                    else:
                        os.unlink(p)
                except FileNotFoundError:
                    continue
                except OSError as exc:
                    ok = False
                    failures.append("%s: %s" % (p, exc))
            if ok:
                deleted += 1
                freed += u["bytes"]
    return deleted, freed, failures, ""


# --- the lander's half -----------------------------------------------------------------------

def roots():
    values = M.load()
    base = str(M.cache())
    cargo = str(M.cache("cargo-target", "CARGO_TARGET_DIR"))
    return base, [cargo]


def checkout_caches(checkout):
    """The cache folders keyed to one checkout, from the declared roots. Read BEFORE the
    checkout is deleted: its Cargo.toml files are listed from the tree."""
    root, cargo_roots = roots()
    if not checkout or not os.path.isdir(checkout):
        return []
    cargo_roots = [r for r in cargo_roots if os.path.isdir(r)]
    return [p for _f, p in owned_paths(checkout, root, cargo_roots)]


def trash_dir(cache_root):
    return os.path.join(cache_root, ".richos-trash")


def discard(paths, cache_root):
    """Move each folder into the cache root's trash (one rename, instant, same volume) and
    start one detached deletion of what was moved. Returns (moved, failures). The scheduled
    sweep deletes whatever the detached deletion did not finish."""
    trash = trash_dir(cache_root)
    moved, failures = [], []
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    for i, p in enumerate(paths):
        try:
            os.makedirs(trash, exist_ok=True)
            dest = os.path.join(trash, "%s-%d-%d-%s" % (stamp, os.getpid(), i, os.path.basename(p)))
            os.rename(p, dest)
            moved.append(dest)
        except FileNotFoundError:
            continue
        except OSError as exc:
            if getattr(exc, "errno", None) == 18:      # EXDEV: another volume, delete in place
                shutil.rmtree(p, ignore_errors=True)
                if not os.path.lexists(p):
                    continue
            failures.append("%s: %s" % (p, exc))
    if moved:
        try:
            subprocess.Popen(["/bin/rm", "-rf", "--"] + moved, stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             start_new_session=True, cwd="/")
        except OSError as exc:
            failures.append("detached deletion could not start: %s" % exc)
    return moved, failures

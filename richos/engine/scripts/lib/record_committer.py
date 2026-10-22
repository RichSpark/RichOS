#!/usr/bin/env python3
"""record_committer.py: commit the loro writes that land in a record's working tree.

Daily-driver plan step 8; two-installs spec point 28. The loro writer never
calls git (`git grep -n "git " -- richos/engine/loro` finds only prose), so
after cut-over the app's memory writes land in the record's working tree and
nothing versions them. This is the smallest committer that closes that: a
scheduled job (launchd, `install`) that, on each tick, commits exactly the
paths the loro writer writes and nothing else.

WHAT IT COMMITS, AND ONLY THAT. The loro writer's own write set in a repository
root (loro/writer/writer.js, storage.js, coverage-write.js; lib/layout.js):
records under loro/records, ceo/records, ceo/unfiled and companies/*/records;
supersessions in loro/memory; company manifests; the coverage baseline. Never
wiki/ and never anything else: a hand edit in the record is a person's change,
and sweeping it into a machine commit would carry it past the landing guards.
The writer's lock file and its in-flight temporaries are excluded.

HOW IT AVOIDS FIGHTING A LANDER. Two landers can move one checkout, and only the
app's takes a lock (two-installs review §C row 28). So each tick, before it
touches anything:

  1. takes the app lander's OWN per-repository land lock (mega-lander
     workspaces.land_lock_path, the same file app.py's land_lock flocks),
     WITHOUT WAITING: busy means a land is running, and the tick is skipped;
  2. under that lock, skips while an operator land lease is live on the
     repository (a lease is taken and released under the same flock, so none
     can appear while the committer holds it; app.py's own rule);
  3. skips unless the main checkout is at rest: HEAD attached to the declared
     branch, no merge, cherry-pick, revert or rebase in progress (a concurrent
     git command's index.lock makes git's own `add` refuse, which skips too);
  4. takes the loro writer's own SQLite lock (BEGIN IMMEDIATE on
     loro/writer-lock.sqlite) WITHOUT WAITING, so it never commits half of a
     two-file supersession, and a writer that arrives meanwhile waits the
     writer's own five seconds rather than being refused;
  5. commits ONLY those paths (`git commit --only -- <paths>`): anything a
     person has staged stays staged and is not committed; git's own index and
     ref locks turn a collision with an unlocked terminal `git merge` into one
     clean failure (recorded, retried next tick), never a mixed commit.

A skipped tick is not an error: the next one commits what this one left.

WITH THE OPERATOR FENCE ON, the committer runs Git with core.hooksPath=/dev/null:
step 2 (the same live-lease check as app.py's _live_fence_lease) is then the
only thing that serializes it against a lease holder, and it is race-free for
the reason given there. With the fence off, Git's hooks run as usual.

  record_committer.py run       --repo <main checkout> [--branch main]
  record_committer.py install   --repo <main checkout> [--interval 300] [--python P]
                                [--agents-dir D] [--no-load] [--branch main]
  record_committer.py uninstall --repo <main checkout> [--agents-dir D] [--no-load]
  record_committer.py status    --repo <main checkout> [--agents-dir D] [--no-load] [--quiet]

`status` exits 0 when the job is installed for that repository, 3 when not.
"""
import errno
import fcntl
import json
import os
import plistlib
import sqlite3
import subprocess
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
ENGINE = os.path.realpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
import operator_fences as OF  # noqa: E402  (git, the launcher, the lease, at-rest files)

LABEL_PREFIX = "com.richos.record-committer."
DEFAULT_INTERVAL = 300
DEFAULT_PYTHON = "/usr/bin/python3"
LIST_LIMIT = 20

# The loro writer's write set in a repository root, and nothing else.
LORO_PATHSPECS = (
    "loro/records",
    "loro/memory",
    "loro/coverage-baseline.json",
    "ceo/records",
    "ceo/unfiled",
    ":(glob)companies/*/company.yaml",
    ":(glob)companies/*/records/**",
)
EXCLUDES = (
    ":(exclude,glob)**/.loro-*.incoming",
    ":(exclude,glob)**/writer-lock.sqlite*",
)


def workspaces():
    sys.path.insert(0, os.path.join(ENGINE, "mega-lander"))
    try:
        import workspaces as W   # noqa: E402  (the land lock's one keying rule)
    finally:
        sys.path.pop(0)
    return W


class Skip(Exception):
    """This tick does nothing; the next one tries again."""


class Refused(Exception):
    """The request itself is wrong; nothing was done."""


# ---------------------------------------------------------------------------
# the repository
# ---------------------------------------------------------------------------

def main_checkout(repo):
    paths = OF.repo_paths(os.path.realpath(os.path.expanduser(repo)))
    if not paths:
        raise Refused("%s is not a git repository" % repo)
    if not paths["main"] or paths["gitdir"] != paths["common"] or paths["top"] != paths["main"]:
        raise Refused("%s is not the main checkout of its repository (a linked worktree is a proposal; "
                      "the committer only commits into the checkout the record is read from)" % repo)
    return paths


def pending(top):
    """The loro-written paths with uncommitted changes, repository-relative."""
    rc, out, err = OF.git(top, "status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames",
                          "--", *(LORO_PATHSPECS + EXCLUDES))
    if rc != 0:
        raise Skip("git status failed: %s" % (err.strip() or rc))
    files = []
    for entry in out.split("\0"):
        if len(entry) > 3:
            files.append(entry[3:])
    return sorted(set(files))


def state_file(key):
    return os.path.join(os.path.dirname(OF.sessions_dir()), "state", "record-committer", key + ".json")


def repo_key(top):
    base = os.path.basename(workspaces().land_lock_path(top))
    return base[:-len(".lock")] if base.endswith(".lock") else base


def record(key, top, outcome, detail, commit="", files=()):
    OF.write_json_atomic(state_file(key), {
        "schema": 1, "repository": top, "at": OF.iso(), "outcome": outcome, "detail": detail,
        "commit": commit, "files": list(files)[:LIST_LIMIT], "count": len(files)})


# ---------------------------------------------------------------------------
# the locks
# ---------------------------------------------------------------------------

class LandLock(object):
    """The app lander's per-repository land lock, taken without waiting."""

    def __init__(self, top):
        self.path = workspaces().land_lock_path(top)
        self.fh = None

    def __enter__(self):
        self.fh = os.fdopen(os.open(self.path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600), "r+")
        try:
            fcntl.flock(self.fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            self.fh.close()
            self.fh = None
            if error.errno in (errno.EAGAIN, errno.EACCES):
                raise Skip("the land lock is held: a land of this repository is running")
            raise
        return self

    def __exit__(self, *_exc):
        if self.fh is not None:
            try:
                fcntl.flock(self.fh, fcntl.LOCK_UN)
            finally:
                self.fh.close()
                self.fh = None
        return False


class WriterLock(object):
    """The loro writer's own lock (loro/writer/storage.js withWriteLock): an
    exclusive SQLite transaction on <root>/loro/writer-lock.sqlite. Opened only
    when the file exists; it is created by the writer before its first write,
    so a missing file means no loro writer has ever run on this root."""

    def __init__(self, top):
        self.path = os.path.join(top, "loro", "writer-lock.sqlite")
        self.db = None

    def __enter__(self):
        if not os.path.isfile(self.path):
            return self
        self.db = sqlite3.connect(self.path, timeout=0, isolation_level=None)
        try:
            self.db.execute("BEGIN IMMEDIATE")
        except sqlite3.OperationalError as error:
            self.db.close()
            self.db = None
            if "locked" in str(error) or "busy" in str(error):
                raise Skip("a loro write is in progress")
            raise
        return self

    def __exit__(self, *_exc):
        if self.db is not None:
            try:
                self.db.execute("ROLLBACK")
            finally:
                self.db.close()
                self.db = None
        return False


def fence_state(paths):
    """(fenced, live lease holder or None) from the repository's own launcher."""
    conf = OF.read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction"))
    if not OF.fenced(conf):
        return False, None
    lease = OF.read_lease(OF.Files(conf))
    if OF.lease_state(lease) in ("live", "expired"):
        return True, lease.get("holder") or {}
    return True, None


def at_rest(paths, branch):
    rc, out, _err = OF.git(paths["top"], "symbolic-ref", "-q", "HEAD")
    if rc != 0:
        raise Skip("HEAD is detached")
    if out.strip() != "refs/heads/" + branch:
        raise Skip("HEAD is on %s, not %s" % (out.strip(), branch))
    busy = OF.in_progress_files(paths["gitdir"])
    if busy:
        raise Skip("a %s" % busy[0][1])
    # No index.lock check here, deliberately: git's own `add` refuses on it and
    # the tick is skipped naming it (record-committer.test.sh K10). A second,
    # earlier check was measured to change nothing and was removed.


# ---------------------------------------------------------------------------
# run
# ---------------------------------------------------------------------------

def message(files):
    shown = files[:LIST_LIMIT]
    lines = ["record: commit %d loro write%s left in the working tree" % (len(files), "" if len(files) == 1 else "s"),
             ""]
    lines += ["  %s" % f for f in shown]
    if len(files) > len(shown):
        lines.append("  ... and %d more" % (len(files) - len(shown)))
    lines += ["", "Committed-By: record-committer (RichOS engine)"]
    return "\n".join(lines) + "\n"


def run(repo, branch):
    paths = main_checkout(repo)
    top = paths["top"]
    key = repo_key(top)
    try:
        if not pending(top):
            record(key, top, "clean", "no uncommitted loro writes")
            return "clean", "no uncommitted loro writes", ""
        with LandLock(top):
            fenced, holder = fence_state(paths)
            if holder is not None:
                raise Skip("an operator land lease is held (%s)" % OF.holder_label(holder))
            at_rest(paths, branch)
            with WriterLock(top):
                files = pending(top)
                if not files:
                    record(key, top, "clean", "no uncommitted loro writes")
                    return "clean", "no uncommitted loro writes", ""
                pre = ("-c", "core.hooksPath=/dev/null") if fenced else ()
                rc, _out, err = OF.git(top, *pre, "add", "-A", "--", *files)
                if rc != 0:
                    raise Skip("git add failed: %s" % (err.strip() or rc))
                rc, _out, err = OF.git(top, *pre, "commit", "-q", "--only", "-m", message(files), "--", *files)
                if rc != 0:
                    raise Skip("git commit failed: %s" % (err.strip() or rc))
                rc, out, _err = OF.git(top, "rev-parse", "HEAD")
                sha = out.strip() if rc == 0 else ""
                detail = "committed %d loro write(s)" % len(files)
                record(key, top, "committed", detail, sha, files)
                return "committed", detail, sha
    except Skip as why:
        record(key, top, "skipped", str(why))
        return "skipped", str(why), ""


# ---------------------------------------------------------------------------
# the scheduled job
# ---------------------------------------------------------------------------

def agents_dir(opts):
    return os.path.realpath(os.path.expanduser(opts.get("--agents-dir") or "~/Library/LaunchAgents"))


def label_for(key):
    return LABEL_PREFIX + key


def plist_path(opts, key):
    return os.path.join(agents_dir(opts), label_for(key) + ".plist")


def launchctl(*args):
    try:
        r = subprocess.run(["launchctl"] + list(args), capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as error:
        return 127, str(error)
    return r.returncode, (r.stdout + r.stderr).strip()


def loaded(label):
    rc, _out = launchctl("print", "gui/%d/%s" % (os.getuid(), label))
    return rc == 0


def build_plist(top, key, interval, python, branch):
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
           "HOME": os.path.expanduser("~")}
    if (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip():
        env["CLAUDE_CONFIG_DIR"] = os.environ["CLAUDE_CONFIG_DIR"].strip()
    log = state_file(key)[:-len(".json")] + ".launchd.log"
    return {
        "Label": label_for(key),
        "ProgramArguments": [python, os.path.join(ENGINE, "scripts", "lib", "record_committer.py"),
                             "run", "--repo", top, "--branch", branch],
        "StartInterval": interval,
        "RunAtLoad": True,
        "ProcessType": "Background",
        "LowPriorityIO": True,
        "Nice": 10,
        "EnvironmentVariables": env,
        "StandardOutPath": log,
        "StandardErrorPath": log,
    }


def cmd_install(opts):
    paths = main_checkout(opts["--repo"])
    top = paths["top"]
    key = repo_key(top)
    try:
        interval = int(opts.get("--interval") or DEFAULT_INTERVAL)
    except ValueError:
        raise Refused("--interval must be a whole number of seconds")
    if not 60 <= interval <= 3600:
        raise Refused("--interval must be between 60 and 3600 seconds")
    python = opts.get("--python") or DEFAULT_PYTHON
    if not os.path.isabs(python) or not os.access(python, os.X_OK):
        raise Refused("--python must be an absolute path to an executable (%s is not)" % python)
    branch = opts.get("--branch") or "main"
    load = "--no-load" not in opts
    if load:
        engine_paths = OF.repo_paths(ENGINE)
        if engine_paths and engine_paths["gitdir"] != engine_paths["common"]:
            raise Refused("this engine is in a linked worktree (%s); a scheduled job must not point at a "
                          "checkout that will be removed. Install from the engine his hooks load." % ENGINE)
    body = build_plist(top, key, interval, python, branch)
    path = plist_path(opts, key)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    os.makedirs(os.path.dirname(state_file(key)), mode=0o700, exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "wb") as fh:
        plistlib.dump(body, fh)
    os.replace(tmp, path)
    print("record-committer: INSTALLED %s" % path)
    print("  commits loro writes in %s every %d s" % (top, interval))
    if load:
        label = label_for(key)
        if loaded(label):
            launchctl("bootout", "gui/%d/%s" % (os.getuid(), label))
        rc, out = launchctl("bootstrap", "gui/%d" % os.getuid(), path)
        if rc != 0:
            raise Refused("launchctl bootstrap failed (%s); the plist is written at %s" % (out or rc, path))
        print("  loaded into launchd as %s" % label)
    else:
        print("  NOT loaded (--no-load)")
    print("  way out: record-committer.sh uninstall --repo %s" % top)
    return 0


def cmd_uninstall(opts):
    paths = main_checkout(opts["--repo"])
    key = repo_key(paths["top"])
    path = plist_path(opts, key)
    label = label_for(key)
    if "--no-load" not in opts and loaded(label):
        rc, out = launchctl("bootout", "gui/%d/%s" % (os.getuid(), label))
        if rc != 0:
            raise Refused("launchctl bootout failed (%s)" % (out or rc))
        print("record-committer: unloaded %s" % label)
    if os.path.exists(path):
        os.unlink(path)
        print("record-committer: REMOVED %s" % path)
    else:
        print("record-committer: nothing installed at %s" % path)
    return 0


def installed(opts, top):
    key = repo_key(top)
    path = plist_path(opts, key)
    try:
        with open(path, "rb") as fh:
            body = plistlib.load(fh)
    except (OSError, ValueError, plistlib.InvalidFileException):
        return False, path, None
    args = body.get("ProgramArguments") or []
    return ("--repo" in args and args[args.index("--repo") + 1:args.index("--repo") + 2] == [top]), path, body


def cmd_status(opts):
    paths = main_checkout(opts["--repo"])
    top = paths["top"]
    key = repo_key(top)
    ok, path, body = installed(opts, top)
    if "--quiet" in opts:
        return 0 if ok else 3
    print("record-committer for %s" % top)
    print("  installed : %s" % (("yes, " + path) if ok else "no (%s)" % path))
    if ok:
        print("  interval  : %s s" % body.get("StartInterval"))
        if "--no-load" in opts:
            print("  loaded    : not asked (--no-load)")
        else:
            print("  loaded    : %s" % ("yes" if loaded(label_for(key)) else "NO: the plist exists and launchd is not running it"))
    last = OF.read_json(state_file(key))
    if last:
        print("  last tick : %s  %s  %s%s" % (last.get("at"), last.get("outcome"), last.get("detail"),
                                             ("  " + last["commit"][:12]) if last.get("commit") else ""))
    else:
        print("  last tick : none recorded")
    try:
        waiting = pending(top)
    except Skip as why:
        print("  waiting   : unknown (%s)" % why)
    else:
        print("  waiting   : %d uncommitted loro write(s)" % len(waiting))
    return 0 if ok else 3


def parse(argv):
    flags = ("--no-load", "--quiet")
    valued = ("--repo", "--interval", "--python", "--agents-dir", "--branch")
    opts, i = {}, 0
    while i < len(argv):
        a = argv[i]
        if a in flags:
            opts[a] = True
            i += 1
        elif a in valued and i + 1 < len(argv):
            opts[a] = argv[i + 1]
            i += 2
        else:
            raise Refused("unknown or incomplete argument %r" % a)
    if not opts.get("--repo"):
        raise Refused("--repo <main checkout> is required")
    return opts


def main(argv):
    verbs = ("run", "install", "uninstall", "status")
    if not argv or argv[0] not in verbs:
        sys.stderr.write("usage: record_committer.py run|install|uninstall|status --repo <main checkout> [...]\n")
        return 2
    try:
        opts = parse(argv[1:])
        if argv[0] == "run":
            outcome, detail, sha = run(opts["--repo"], opts.get("--branch") or "main")
            print("record-committer: %s: %s%s" % (outcome, detail, (" " + sha[:12]) if sha else ""))
            return 0
        return {"install": cmd_install, "uninstall": cmd_uninstall, "status": cmd_status}[argv[0]](opts)
    except Refused as why:
        sys.stderr.write("record-committer: REFUSED. %s\n" % why)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

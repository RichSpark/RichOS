#!/usr/bin/env python3
"""autocheck.py — the basic checks run by git itself, at commit and at merge, for everyone.

WHY THIS EXISTS (2026-09-29, CEO ruling §97). Nightly attempt 20260929T063824Z-b27b6279
failed on `lint growth: clippy::let_underscore_must_use: 782 > 777`: a branch added five
discarded send results, nobody ran the lint before it reached main, and the build found it.
His words: *"WHY THE FUCK DOES ANYONE NEED TO BE "TOLD" ANYTHING WHEN IT COMES TO PREVENTING
BASIC-LEVEL SHIT-FUCKERY OF ANY KIND????"* So nobody is told. Git runs this through the
repository's hooks (installed by `install.sh` beside this file) for every committer: an
engineer in a worktree, Codex, Rich in the main checkout.

  COMMIT, on any branch but main (pre-commit): `lint.sh --changed --strict` over what
  differs from HEAD, i.e. the lint ratchets (static, load rules, and Clippy for a Rust set
  whose inputs changed), and no count may grow whatever room a ceiling has. Then the land's
  own selector (`proof-for.sh`) over the BRANCH'S WHOLE CHANGE: whatever it refuses
  (UNCOVERED, a map that does not reconcile) refuses the commit, and the selected suites that
  measure under a second run here (see branch_selection). A merge into a branch
  (pre-merge-commit) runs `--changed` against the ceilings only. Both run on exactly the
  content being committed: unstaged edits and untracked files are set aside for the length
  of the check and put back after it (StagedOnly). The repository enforces no formatter, so
  none is run. A failure refuses the commit with the reason.

  LAND, anything that moves main (pre-merge-commit on main, pre-commit on main, and
  pre-push of main as the backstop): the suites that own the changed files
  (`proof-for.sh --gate`), run by `proof-run.py`, plus `lint.sh --changed` when the land
  changes something under richos/app and the selection does not already include
  `lint.test.sh` (the push backstop: `--all`). Every check is stopped at 600 s, and the gate
  runs in rounds of at most 900 s that keep every pass, at most six (THE MERGE GATE'S LIMITS,
  A LARGE LAND RUNS IN ROUNDS). Nobody chooses the suites. A failing check refuses the
  merge before it exists, so it cannot be pushed; that is all that refuses it (land_verdict).
  A check that did not reach a verdict is NOT RUN, which is never a pass: it is named in the
  verdict and in the receipt, and the nightly runs it (README.md, "NOT RUN").

  THE ONE ESCAPE is git's own `--no-verify`. It cannot be prevented; it is recorded. After
  the fact (post-commit, post-merge) a commit whose tree this check never passed, or a main
  that moved to a tree without a land receipt, raises an escalation in the ledger the lead's
  session reads at every session start and turn end (`escalate.sh`), and is appended to
  `<git-common-dir>/richos-autocheck/bypass.log`.

Which copy of this file runs is chosen by the hook shim: the one committed at HEAD, else the
one on main, else (the land that introduces it) the one being merged. A branch's commits are
checked by its own version; a land into main is checked by main's.
"""
import json
import hashlib
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time

LAND_BRANCH = "main"
LINT = "richos/app/scripts/lint.sh"
LINT_DRIVER = "richos/app/scripts/lint/driver.py"
PROOF_FOR = "richos/app/scripts/proof-for.sh"
PROOF_RUN = "richos/app/scripts/proof-run.py"
BATTERY_CHECK = "richos/app/scripts/battery-check.py"
ENGINE_HOOKS = "richos/engine/scripts/hooks/"
ENGINE_HOOKS_INSTALL = ENGINE_HOOKS + "install.sh"
PHYSICAL_CHECK = "richos/mobile/physical.py"
MOBILE = "richos/mobile/"
ENGINE_ESCALATE = "richos/engine/scripts/escalate.sh"
ACTIVE = "RICHOS_AUTOCHECK_ACTIVE"
ZERO = "0" * 40


TERM_GRACE_SECONDS = 5    # a check that ran out of time gets SIGTERM, then this long, before SIGKILL


def _alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _descendants(root):
    """Every live descendant pid of `root`, whatever its process group, or None when the
    process table could not be read (never read as "no descendants")."""
    try:
        done = subprocess.run(["ps", "-axo", "pid=,ppid="], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.SubprocessError):
        return None
    if done.returncode:
        return None
    table = done.stdout
    children = {}
    for line in table.splitlines():
        parts = line.split()
        if len(parts) == 2 and all(part.isdigit() for part in parts):
            children.setdefault(int(parts[1]), []).append(int(parts[0]))
    found, todo = set(), [root]
    while todo:
        for child in children.get(todo.pop(), []):
            if child not in found:
                found.add(child)
                todo.append(child)
    return found


def _signal_all(pids, sig, group):
    """Signal the group once, then each other pid that is NOT in that group (a process in the
    group already got the signal; a second SIGTERM makes Playwright force-kill at once)."""
    try:
        os.killpg(group, sig)
    except ProcessLookupError:
        pass
    for pid in pids:
        try:
            if os.getpgid(pid) == group:
                continue
            os.kill(pid, sig)
        except ProcessLookupError:
            pass


def run_bounded(argv, timeout, grace=None, **kwargs):
    """subprocess.run(capture_output, timeout) that ends a late check gracefully.

    subprocess.run kills only the DIRECT child, and with SIGKILL. A UI suite ended that way
    leaves its test browser (Playwright.app, a GUI app) orphaned; if the suite was still
    launching it, the browser aborts and macOS shows "Playwright quit unexpectedly" on the
    user's screen (measured 2026-10-09: crash reports 15:28:36 and 16:16:48, both "parentProc:
    Exited process", the browser dead 67-80 ms after launch). So the check runs in its own
    session; on the time limit its whole group gets SIGTERM (Playwright closes its browser on
    it), then SIGKILL after `grace`, and nothing of the group is left. Raises TimeoutExpired
    like subprocess.run."""
    grace = TERM_GRACE_SECONDS if grace is None else grace
    kwargs.pop("capture_output", None)
    proc = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True, **kwargs)
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as expired:
        # Every descendant, in any process group (Playwright's browser has its own), is recorded
        # before the first signal; the wait is for ALL of them, not just the direct child, since a
        # shell wrapper dies on SIGTERM at once and would cut the grace short.
        found = _descendants(proc.pid)
        if found is None:
            say(f"autocheck: could not list the check's processes (ps failed); signalling its process group only, "
                f"so a browser in another group may be left running (pid {proc.pid})")
            found = set()
        victims = {proc.pid, *found}
        _signal_all(victims, signal.SIGTERM, group=proc.pid)
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            if proc.poll() is not None and not any(_alive(pid) for pid in victims - {proc.pid}):
                break
            time.sleep(0.1)
        _signal_all(victims, signal.SIGKILL, group=proc.pid)
        proc.communicate()
        raise expired
    except BaseException:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        raise
    return subprocess.CompletedProcess(proc.args, proc.returncode, out, err)


def run_admitted(repo, argv, **kwargs):
    """A fast check's hang guard starts under its worker token, as the nightly's does."""
    snapshot = Path(__file__).parent
    library = snapshot if (snapshot / "owned_command.py").is_file() else repo.top / "richos/app/scripts/lib"
    sys.path.insert(0, str(library))
    from owned_command import finish_group, owned_run
    # Signal the owned supervisor group once. proc_tree owns detached browsers and gets
    # nightly's full cleanup grace (its EXIT traps can take eight seconds). Walking and
    # signalling those children here as well can turn graceful browser shutdown into abort.
    return owned_run(argv, library=repo.top / "richos/engine/scripts/lib", finish=finish_group,
                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)


def say(text=""):
    print(text, file=sys.stderr, flush=True)


def banner(title, lines):
    say("")
    say(f"=== {title} ===")
    for line in lines:
        say("  " + line)
    say("")


# ---------------------------------------------------------------------------------------
# Git, with and without the hook's environment
# ---------------------------------------------------------------------------------------
# A hook runs with GIT_DIR, GIT_INDEX_FILE and friends exported. The questions about THIS
# commit (what is staged, which tree is being written) must be asked in that environment,
# because for `git commit -a` or `git commit <path>` the index being committed is a
# temporary file only that environment names. The CHECKS must not inherit it: a test suite
# that builds its own fixture repository would otherwise write into this one.

def git(*args, env=None, check=True, cwd=None):
    result = subprocess.run(["git", *args], cwd=cwd, env=env, capture_output=True, text=True,
                            stdin=subprocess.DEVNULL)
    if check and result.returncode:
        raise RuntimeError(f"git {' '.join(args)} failed ({result.returncode}): {result.stderr.strip()}")
    return result.stdout.strip() if check else result


def clean_env(common):
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    # A land runs every suite it selected: a caller's opt-in to skip unchanged suites would
    # turn them into NOT RUN, which the land refuses (accepted_not_run).
    env.pop("RUN_TESTS_SKIP_UNCHANGED", None)
    cargo = str(Path.home() / ".cargo/bin")
    if cargo not in env.get("PATH", "").split(":"):
        env["PATH"] = env.get("PATH", "") + ":" + cargo
    env[ACTIVE] = f"{common}:{os.getpid()}"
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def ancestors():
    pids, pid = set(), os.getppid()
    for _ in range(64):
        if pid <= 1 or pid in pids:
            break
        pids.add(pid)
        out = subprocess.run(["ps", "-o", "ppid=", "-p", str(pid)], capture_output=True, text=True)
        try:
            pid = int(out.stdout.strip())
        except ValueError:
            break
    return pids


def inside_own_check(common):
    """True only when an autocheck of THIS repository is an ancestor of this process: a check
    whose suites commit into the repository under check. A copied or invented value names a
    process that is not our ancestor and exempts nothing."""
    value = os.environ.get(ACTIVE, "")
    where, _, pid = value.rpartition(":")
    return where == common and pid.isdigit() and int(pid) in ancestors()


class Repo:
    def __init__(self):
        self.top = Path(git("rev-parse", "--show-toplevel"))
        self.git_dir = Path(git("rev-parse", "--absolute-git-dir"))
        common = Path(git("rev-parse", "--git-common-dir"))
        self.common = (common if common.is_absolute() else (Path.cwd() / common)).resolve()
        self.branch = git("symbolic-ref", "-q", "--short", "HEAD", check=False).stdout.strip()
        self.state = self.common / "richos-autocheck"
        self.env = clean_env(str(self.common))

    def run(self, argv, env=None, **kw):
        say("+ " + " ".join(shlex.quote(str(a)) for a in argv))
        return subprocess.run([str(a) for a in argv], cwd=self.top, env=env or self.env, stdin=subprocess.DEVNULL, **kw)

    def index_tree(self):
        return git("write-tree", cwd=self.top)

    def head_tree(self, rev="HEAD"):
        return git("rev-parse", f"{rev}^{{tree}}", cwd=self.top)

    # Receipts: a commit check leaves the tree it passed in this worktree's git dir, for the
    # post-commit hook to compare; a land check leaves one per tree, shared by every worktree.
    def marker(self):
        return self.git_dir / "richos-autocheck-verified"

    def land_receipt(self, tree):
        return self.state / "land" / tree

    def write_land_receipt(self, tree, detail):
        path = self.land_receipt(tree)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(dict(detail, tree=tree, at=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())),
                                   sort_keys=True) + "\n")


# ---------------------------------------------------------------------------------------
# The commit check
# ---------------------------------------------------------------------------------------

def commit_check(repo, what):
    started = time.monotonic()
    # What the commit holds: the index the hook was given (for `git commit <path>` or `-a`, the
    # temporary one git names in GIT_INDEX_FILE). Unstaged edits are not in it, so they neither
    # call for a lint nor answer for one (see StagedOnly).
    staged = git("diff", "--cached", "--name-only", "--no-renames").splitlines()
    app = sorted({p for p in staged if p.startswith("richos/app/")})
    if what == "commit" and (stale_pins(repo) or subprocess.run("{ git show HEAD:richos/app/scripts/autocheck/dependency-pins.py || git show refs/heads/main:richos/app/scripts/autocheck/dependency-pins.py; } 2>/dev/null | python3 -", shell=True, cwd=repo.top, stdin=subprocess.DEVNULL).returncode):
        return 1
    if not app:
        say(f"autocheck: {what}: nothing under richos/app changed, so no lint applies "
            f"({time.monotonic() - started:.1f}s)")
        if what != "commit":
            return 0
        # Everything below reads files (the policy suite's header and inputs, proof-for.sh and
        # its tables), so it runs with only what the commit holds in the tree: an unstaged edit
        # to any of them must neither call for a check nor excuse one (recheck R10 v2, the same
        # mistake as the app path's R10 at a new site).
        with StagedOnly(repo) as aside:
            if aside.summary:
                say(f"autocheck: {what}: {aside.summary} set aside for the check, so it sees only what is committed")
            load_check = repo.top / "richos/app/scripts/lint/load_rules.py"
            if any(p.startswith("richos/engine/") for p in staged) and load_check.is_file():
                result = run_bounded([sys.executable, str(load_check), "--root", str(repo.top)],
                                     timeout=FAST_HANG_SECONDS, env=repo.env, text=True)
                if result.returncode:
                    banner(f"{what.upper()} REFUSED: engine test load-rule growth", [result.stderr])
                    return 1
            if policy_applies(repo, staged) and release_policy(repo, what):
                return 1
            if physical_check(repo, what):
                return 1
            # The land's coverage rule applies to every code path, not only richos/app (2026-10-01:
            # a Swift file under richos/mobile and step lists under docs/verification passed here
            # and were refused at the merge). Lookup only: no suite runs for a change outside the app.
            if branch_selection(repo, what):
                return 1
        return 0
    with StagedOnly(repo) as aside:
        if aside.summary:
            say(f"autocheck: {what}: {aside.summary} set aside for the check, so it sees only what is committed")
        rc = lint_and_select(repo, what, app)
        if rc == 0 and what == "commit" and policy_applies(repo, staged):
            rc = release_policy(repo, what)
        if rc == 0:
            rc = physical_check(repo, what)
    if rc == 0:
        say(f"autocheck: {what}: passed in {time.monotonic() - started:.1f}s")
    return rc


# THE PHONE APPS' RELEASE POLICY RUNS AT THE COMMIT (2026-10-01). A `Logger` added to
# richos/mobile/native-ios/App/Platform/BackgroundSendTime.swift (84f1ec3af) broke rule L1
# ("no log call in production code") of native-release-policy.test.sh, and two agents built on
# it for hours: the suite is a script suite, and the commit check ran only the weight-0 UI
# suites (quick_suites, which keeps `cd richos/app/ui/tests && node ...` lines), so it never ran.
# The suite reads text files only and takes about 3 s, so a commit touching one of its own
# `# run-tests: inputs` runs it, alone, and a failure refuses the commit. The inputs are read
# from the suite's header, so the commit and the land never disagree about which files it owns.

RELEASE_POLICY = "richos/app/scripts/native-release-policy.test.sh"
INPUTS_LINE = re.compile(r"^# run-tests: inputs[ \t]+(.+)$", re.M)


def policy_inputs(repo):
    try:
        text = (repo.top / RELEASE_POLICY).read_text(errors="replace")
    except OSError:
        return []
    return [p.rstrip("/") for line in INPUTS_LINE.findall(text) for p in line.split()]


def policy_applies(repo, staged):
    inputs = policy_inputs(repo)
    return any(path == i or path.startswith(i + "/") for path in staged for i in inputs)


def release_policy(repo, what):
    say(f"+ bash {RELEASE_POLICY}")
    try:
        result = run_bounded(["bash", RELEASE_POLICY], cwd=repo.top, env=repo.env,
                                stdin=subprocess.DEVNULL, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        banner(f"{what.upper()} REFUSED: native-release-policy did not finish in 120 s",
               ["It measures about 3 s; a hang is a failure, not a pass."])
        return 1
    if result.returncode:
        sys.stderr.write((result.stdout + result.stderr)[-4000:])
        banner(f"{what.upper()} REFUSED: this change breaks the phone apps' release policy", [
            "The rule and the file are printed just above. The merge runs the same suite and would refuse it.",
            "Fix the change and commit again.",
        ])
        return 1
    return 0


# ONLY THE RELEASE BUILD ON A PHYSICAL PHONE, AND ONLY THROUGH THE COMMAND LINES (CEO 2026-10-02).
# A debuggable build ran on the wired Android phone for eight hours and he judged the app by it; it got
# there by a path outside `randroid device` / `rios device`. physical.py scan reads the whole tree
# (about a second) and refuses an install on, uninstall from or data clear of a phone anywhere outside
# those two command lines, a Gradle task that installs on every attached device, and a Debug build
# for a physical iPhone. A tree without the scanner (older than this rule) has nothing to run.

def physical_check(repo, what):
    if not (repo.top / PHYSICAL_CHECK).is_file():
        return 0
    say(f"+ python3 {PHYSICAL_CHECK} scan")
    try:
        result = run_bounded([sys.executable, PHYSICAL_CHECK, "scan", "--root", str(repo.top)], cwd=repo.top,
                                env=repo.env, stdin=subprocess.DEVNULL, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        banner(f"{what.upper()} REFUSED: the physical-phone check did not finish in 120 s",
               ["It measures about a second; a hang is a failure, not a pass."])
        return 1
    if result.returncode:
        sys.stderr.write((result.stdout + result.stderr)[-4000:])
        banner(f"{what.upper()} REFUSED: this change touches a physical phone outside randroid device / rios device", [
            "The file, the line and the rule are printed just above. Only the release build goes on a physical",
            "phone, only through the two command lines, and nothing uninstalls the app or clears its data.",
            "The merge runs the same check and would refuse it.",
        ])
        return 1
    return 0


def lint_and_select(repo, what, app):
    if not (repo.top / LINT).is_file():
        banner(f"{what.upper()} REFUSED: the lint is missing", [f"{LINT} is not in this tree; nothing can be checked."])
        return 1
    driver = repo.top / LINT_DRIVER
    known = driver.read_text(errors="replace") if driver.is_file() else ""
    if "--changed" in known:
        # A commit may not grow any count (--strict). A merge into a branch brings main's
        # landed changes, which were held to the ceilings at their land, so it is not.
        mode = ["--changed", "--strict"] if what == "commit" and "--strict" in known else ["--changed"]
    else:
        # A branch older than the commit mode: the full static pass, and the fast set when
        # Rust changed. Slower, never looser.
        rust = any(p.endswith(".rs") or Path(p).name in ("Cargo.toml", "Cargo.lock") for p in app)
        mode = ["--fast"] if rust else ["--static"]
    say(f"autocheck: {what}: {len(app)} changed path(s) under richos/app; lint {' '.join(mode)} "
        "(the content being committed)")
    result = repo.run(["bash", LINT, *mode])
    if result.returncode:
        banner(f"{what.upper()} REFUSED: the lint failed for this change", [
            "The reason is printed just above (\"Lint refused: ...\").",
            "Fix it and commit again. Nothing needs to be run by hand; this check runs itself.",
            "git's --no-verify skips it, and every skip is recorded in the lead's escalation ledger.",
        ])
        return 1
    if what == "commit" and branch_selection(repo, what):
        return 1
    return 0


# ---------------------------------------------------------------------------------------
# The commit check sees exactly the bytes being committed
# ---------------------------------------------------------------------------------------
# Hunt part 2, finding 10 (2026-09-30). The commit check ran the lint on the WORKING TREE, so
# that changed-path lint saw unstaged edits too, and then approved the INDEX. A bad staged
# edit whose unstaged replacement passed was committed with no record, and a clean staged
# edit was refused over unstaged work the commit does not hold. The land refuses a working
# tree that differs from what it commits (land_from_index); a branch commit cannot, because
# committing part of the work in progress is how atomic commits are made.
#
# So, for the length of the check, the working tree IS what is being committed. The design is
# pre-commit's `staged_files_only` (pre-commit.com, MIT), taken as design: the unstaged
# difference is saved as a binary patch and untracked files are moved into this worktree's
# git directory; `git checkout -- .` makes the working tree equal the index the commit is
# written from; the checks run; the patch is applied back and the files moved back. Ignored
# files stay where they are: they are never committed, and a check needs its build caches and
# installed dependencies. The lint still reads the working tree, as it always has; that tree
# now holds the committed bytes and nothing else.
#
# If a check is killed while edits are set aside, or they cannot be put back, they stay in
# ASIDE, and every later commit check refuses, naming the directory and how to restore it,
# until it is gone. Nothing is ever set aside on top of them.
#
# The restore never overwrites bytes written after the work was set aside (recheck N01,
# 2026-09-30). The checks can take minutes, and the engineer keeps editing: a tracked file
# that differs from the index when the restore starts was written during the check, by the
# check or by an editor, and nothing can tell which. The old fallback, when the patch did not
# apply, ran `git checkout -- .` over the whole tree and so reset every such file, including
# ones the patch never names. Now a file the patch does not name is left exactly as found. A
# file the patch names that changed during the check is where the engineer's edit and the
# newer bytes collide: the newer bytes are moved into CHANGED under ASIDE first, then
# `git checkout-index` (which refuses to write over a file that exists) and the patch put the
# engineer's edit back, and the commit is refused with both named. The only window left is the
# one inside every git write: bytes landing between git reading a file and git writing it.

ASIDE = "richos-autocheck-aside"
CHANGED = "changed-during-check"


DELETED_SUFFIX = ".deleted-during-check"


class StagedOnly:
    def __init__(self, repo):
        self.repo = repo
        self.dir = repo.git_dir / ASIDE
        self.patch = self.dir / "unstaged.patch"
        self.held = self.dir / "untracked"
        self.changed = self.dir / CHANGED
        self.moved = []
        self.patched = False
        self.patch_paths = set()
        self.summary = ""
        self.deleted = []

    def how_to_restore(self):
        lines = [f"Your unstaged edits and untracked files are in {self.dir}.",
                 f"To put them back: cd {self.repo.top} && git apply --binary {self.patch}"
                 " (when that file exists),",
                 f"then move everything under {self.held} back to the same path here,"]
        if self.changed.exists():
            lines.append(f"compare each file under {self.changed} (bytes written while the check ran) with the "
                         "file at the same path here, and keep what you want,")
        return lines + [f"then delete {self.dir} and commit again."]

    def hook_git(self, *args):
        # The hook's own environment: its GIT_INDEX_FILE is the index being committed.
        return subprocess.run(["git", *args], cwd=self.repo.top, capture_output=True, stdin=subprocess.DEVNULL)

    def __enter__(self):
        if self.dir.exists():
            raise RuntimeError("\n  ".join([
                "an earlier commit check set work aside and did not put it back "
                "(it was interrupted, or the restore failed); nothing was set aside this time.",
                *self.how_to_restore()]))
        diff = self.hook_git("diff", "--binary", "--no-color", "--no-ext-diff", "--ignore-submodules",
                             "--no-renames")
        added = self.hook_git("diff", "--name-only", "--diff-filter=A", "-z")
        untracked = self.hook_git("ls-files", "--others", "--exclude-standard", "--directory", "-z")
        for done in (diff, added, untracked):
            if done.returncode:
                raise RuntimeError("cannot read what differs from the commit: "
                                   + done.stderr.decode(errors="replace").strip())
        if added.stdout.strip(b"\0"):
            raise RuntimeError("a file added with `git add -N` is not in the commit and cannot be set aside "
                               "safely; stage it or remove it from the index (git rm --cached) and commit again")
        names = [os.fsdecode(n).rstrip("/") for n in untracked.stdout.split(b"\0") if n]
        if not diff.stdout and not names:
            return self
        if diff.stdout:
            self.patch_paths = set(self.differing())
        self.dir.mkdir(parents=True)
        try:
            (self.dir / "README.txt").write_text("\n".join(self.how_to_restore()) + "\n")
            if diff.stdout:
                self.patch.write_bytes(diff.stdout)
                self.patched = True
            for name in names:
                target = self.held / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(self.repo.top / name), str(target))
                self.moved.append(name)
            if self.patched:
                done = self.hook_git("-c", "submodule.recurse=0", "checkout", "--", ".")
                if done.returncode:
                    raise RuntimeError("cannot make the working tree equal the commit: "
                                       + done.stderr.decode(errors="replace").strip())
            if self.hook_git("diff", "--quiet", "--ignore-submodules").returncode:
                raise RuntimeError("the working tree still differs from the commit after setting work aside")
        except BaseException:
            self.restore()
            raise
        parts = []
        if self.patched:
            parts.append("unstaged edits")
        if self.moved:
            parts.append(f"{len(self.moved)} untracked path(s)")
        self.summary = " and ".join(parts)
        return self

    def __exit__(self, *exc):
        self.restore()
        return False

    def differing(self):
        """Tracked paths whose working-tree bytes differ from the index being committed."""
        done = self.hook_git("diff", "--name-only", "-z", "--no-renames", "--ignore-submodules")
        if done.returncode:
            raise RuntimeError("cannot read what differs from the commit: "
                               + done.stderr.decode(errors="replace").strip())
        return [os.fsdecode(n) for n in done.stdout.split(b"\0") if n]

    def put_patch_back(self, problems):
        """Apply the saved unstaged patch without writing over any byte written after it was
        saved. Returns the paths whose newer bytes were moved into CHANGED."""
        # git apply is all or nothing: when it fails, it has written nothing.
        applied = self.hook_git("apply", "--whitespace=nowarn", "--binary", str(self.patch))
        kept = []
        if applied.returncode:
            # A file the patch edits was written during the check, by the check or by an editor.
            # The engineer's edit from before the check is put back, and the newer bytes are
            # moved aside first, never overwritten. Files the patch does not edit are not touched.
            try:
                collided = sorted(set(self.differing()) & self.patch_paths)
            except RuntimeError as exc:
                problems.append(str(exc))
                return kept
            if not collided:
                problems.append("git apply failed: " + applied.stderr.decode(errors="replace").strip())
                return kept
            for name in collided:
                source = self.repo.top / name
                if not os.path.lexists(source):
                    # Deleted during the check. The deletion is newer work than the saved patch
                    # and has no bytes, so it is recorded as a named marker the restore reports
                    # and keeps; it is never treated as nothing to preserve (recheck N01 v2).
                    marker = self.changed / (name + DELETED_SUFFIX)
                    marker.parent.mkdir(parents=True, exist_ok=True)
                    marker.write_text("deleted while the commit check ran; the edit from before the check "
                                      "was put back over it\n")
                    self.deleted.append(name)
                    continue
                target = self.changed / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(source), str(target))
                kept.append(name)
            # checkout-index without -f refuses a path that exists, so bytes written again
            # since the move stay where they are and the patch is not applied over them.
            done = self.hook_git("checkout-index", "--", *collided)
            if done.returncode:
                problems.append("could not write the committed copy back before your edit: "
                                + done.stderr.decode(errors="replace").strip())
                return kept
            applied = self.hook_git("apply", "--whitespace=nowarn", "--binary", str(self.patch))
            if applied.returncode:
                problems.append("git apply failed: " + applied.stderr.decode(errors="replace").strip())
                return kept
        self.patched = False
        self.patch.unlink()
        return kept

    def settle_changed(self, kept, problems):
        for name in self.deleted:
            back = ("your edit from before the check is back in place" if not self.patched
                    else f"your edit from before the check is still in {self.patch}")
            problems.append(f"{name} was deleted while the check ran; {back}, and the deletion is "
                            f"recorded at {self.changed / (name + DELETED_SUFFIX)}")
        """Drop a moved-aside copy that is byte-identical to what is now back in place (a
        restore after a failed set-aside, or a write of the same bytes); name the rest."""
        for name in kept:
            target, here = self.changed / name, self.repo.top / name
            if same_bytes(target, here):
                if target.is_dir() and not target.is_symlink():
                    shutil.rmtree(target)
                else:
                    target.unlink()
                continue
            back = ("your edit from before the check is back in place" if not self.patched
                    else f"your edit from before the check is still in {self.patch}")
            problems.append(f"{name} changed while the check ran; {back}, and the bytes found there "
                            f"are kept at {target}")
        if self.changed.exists():
            for folder in sorted((p for p in self.changed.rglob("*") if p.is_dir() and not p.is_symlink()),
                                 key=lambda p: len(p.parts), reverse=True):
                if not any(folder.iterdir()):
                    folder.rmdir()
            if not any(self.changed.iterdir()):
                self.changed.rmdir()

    def restore(self):
        if not self.dir.exists():
            return
        problems = []
        # Recorded before anything is written back, so the report names only what the check
        # or an editor wrote, not what the restore itself puts back.
        try:
            during = self.differing()
        except RuntimeError as exc:
            during = []
            problems.append(str(exc))
        kept = self.put_patch_back(problems) if self.patched else []
        self.settle_changed(kept, problems)
        left = sorted(set(during) - self.patch_paths)
        if left:
            say("autocheck: tracked file(s) written while the check ran, left exactly as found: "
                + ", ".join(left))
        for name in list(self.moved):
            destination = self.repo.top / name
            if os.path.lexists(destination):
                problems.append(f"{name} exists again, so the set-aside copy was not moved over it")
                continue
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(self.held / name), str(destination))
            self.moved.remove(name)
        if problems:
            how = self.how_to_restore()
            try:
                (self.dir / "README.txt").write_text("\n".join(["Left here because: " + "; ".join(problems),
                                                                *how]) + "\n")
            except OSError:
                pass  # the same words are in the refusal printed below
            raise RuntimeError("\n  ".join(["the work set aside for the check could not all be put back: "
                                            + "; ".join(problems), *how]))
        shutil.rmtree(self.dir)


def same_bytes(a, b):
    """True when two paths hold the same thing: both absent, the same symlink target, or
    regular files with identical bytes. A directory is never the same as anything."""
    if not os.path.lexists(a) or not os.path.lexists(b):
        return not os.path.lexists(a) and not os.path.lexists(b)
    if os.path.islink(a) or os.path.islink(b):
        return os.path.islink(a) and os.path.islink(b) and os.readlink(a) == os.readlink(b)
    if not (os.path.isfile(a) and os.path.isfile(b)):
        return False
    return Path(a).read_bytes() == Path(b).read_bytes()


# ---------------------------------------------------------------------------------------
# A changed source that a reviewed check pins
# ---------------------------------------------------------------------------------------
# 2026-09-29: three finished branches (make-release.sh, a stable-release script, run-tests.sh)
# passed every commit here and were refused at the merge, because
# proof_evidence.qualify_recipe raises UnqualifiedReader when a source a reviewed unit pins by
# SHA-256 no longer matches its pin, and only proof-run.test.sh ran that. Each cost a full merge
# run plus another engineer pass. The same comparison runs here, over the branch's changed
# paths only, and reads the qualification file only when one of them is named in it.

QUALIFICATIONS = "docs/development/verification-input-qualifications.json"


def hook_env(repo):
    """The check environment plus the index the hook was given: `commit -a` or `commit <path>`
    names a temporary one in GIT_INDEX_FILE, which clean_env drops."""
    env = dict(repo.env)
    if os.environ.get("GIT_INDEX_FILE"):
        env["GIT_INDEX_FILE"] = os.environ["GIT_INDEX_FILE"]
    return env


def stale_pins(repo):
    """Refuse (return 1) when a path this branch changed is a pinned source of a reviewed unit
    and the pin no longer matches the staged content."""
    import hashlib
    paths = branch_paths(repo)
    if not paths:
        return 0
    got = subprocess.run(["git", "show", f":{QUALIFICATIONS}"], cwd=repo.top, env=hook_env(repo),
                         capture_output=True, text=True, stdin=subprocess.DEVNULL)
    if got.returncode:
        return 0  # this tree has no qualification file, so nothing is pinned
    if not any(p in got.stdout for p in paths):
        return 0  # the common case: nothing changed is named there; no parse
    try:
        units = json.loads(got.stdout).get("units", {})
    except ValueError:
        return 0  # the merge-time test reports a malformed file; not this check's job
    changed = set(paths)
    stale = []
    for unit, body in sorted(units.items()):
        for source, pinned in sorted((body.get("sources") or {}).items()):
            if source not in changed:
                continue
            blob = subprocess.run(["git", "show", f":{source}"], cwd=repo.top, env=hook_env(repo),
                                  capture_output=True, stdin=subprocess.DEVNULL)
            now = hashlib.sha256(blob.stdout).hexdigest() if blob.returncode == 0 else "(file removed)"
            if now != pinned:
                stale.append((source, unit, pinned, now))
    if not stale:
        return 0
    lines = []
    for source, unit, pinned, now in stale:
        lines.append(f"{source} (unit \"{unit}\"): pinned {pinned[:12]}, now {now[:12]}")
    lines += [
        "",
        f"A reviewed check reads these files, and {QUALIFICATIONS} pins each by SHA-256.",
        "The merge would refuse this branch with UnqualifiedReader (proof_evidence.qualify_recipe).",
        "Fix: run  python3 richos/app/scripts/autocheck/qualification-pins.py --renew <file>..",
        "(no <file>: every stale pin) instead of editing the JSON by hand; it sets each pin to the",
        "file's SHA-256 and touches nothing else. Then re-check that the unit's \"review\" text",
        "still describes what the changed file reads.",
    ]
    banner("COMMIT REFUSED: a changed file is pinned by a reviewed check", lines)
    return 1


# ---------------------------------------------------------------------------------------
# What the land would refuse, refused at the commit
# ---------------------------------------------------------------------------------------
# 2026-09-29: two finished branches (isaac-opus-speckle1, andy-opus-speckle1) were refused at
# Rich's merge for UNCOVERED paths, four each, although every one of their commits had passed
# this hook: the commit ran the lint and nothing else, so the first thing that could see the
# refusal was the land. The same day a Rust test file made app/README.md's counts false and
# nothing short of the nightly ran docs-claims.js. So a commit now asks the land's own
# selector about the BRANCH'S WHOLE CHANGE (the merge-base with main to the tree being
# committed, which is what the land will diff), refuses what it refuses, and runs the selected
# suites that measure under a second in ui/tests/suite-weights.tsv. The heavier suites (Rust,
# the browser suites, the build suites) still run only at the land: minutes per commit would
# be the wrong trade for the one engineer who is waiting on it.

QUICK_WEIGHTS = "richos/app/ui/tests/suite-weights.tsv"
UI_SUITE_LINE = "cd richos/app/ui/tests && node "


def branch_paths(repo):
    """The branch's change as the land will see it: merge-base(main, HEAD) against the index
    being committed. With no main to compare to, the staged change alone."""
    base = git("merge-base", f"refs/heads/{LAND_BRANCH}", "HEAD", check=False, cwd=repo.top)
    if base.returncode == 0 and base.stdout.strip():
        return [p for p in git("diff", "--cached", "--name-only", "--no-renames", base.stdout.strip()).splitlines() if p]
    return [p for p in git("diff", "--cached", "--name-only", "--no-renames").splitlines() if p]


def branch_range(repo, paths):
    """The selection the merge gate makes, asked the way the merge gate asks it: a `base..head`
    range, with the commit's index written as a commit object nothing refers to. proof-for.sh
    with a path list and with a range reach the engine's selector by different roads (a path
    list has no before/after, so a changed hooks.json takes the fallback and never visits the
    hook scripts it registers). 2026-10-01: a hook script passed every commit here by path list
    and was refused at the merge by range as "named by NO suite". None when no range can be
    built (no main, no HEAD, no identity): the caller then asks by path list."""
    base = git("merge-base", f"refs/heads/{LAND_BRANCH}", "HEAD", check=False, cwd=repo.top)
    if base.returncode or not base.stdout.strip():
        return None
    tree = git("write-tree", check=False, cwd=repo.top)
    if tree.returncode or not tree.stdout.strip():
        return None
    head = git("-c", "user.name=autocheck", "-c", "user.email=autocheck@invalid", "commit-tree", tree.stdout.strip(),
               "-p", "HEAD", "-m", "autocheck: the commit being checked", check=False, cwd=repo.top)
    if head.returncode or not head.stdout.strip():
        return None
    return [f"{base.stdout.strip()}..{head.stdout.strip()}"]


def quick_suites(repo, commands):
    """The selected UI suites whose measured weight is 0, i.e. under a second (the file's
    own convention). A suite with no row is not assumed quick; it runs at the land."""
    weights = repo.top / QUICK_WEIGHTS
    if not weights.is_file():
        return []
    quick = set()
    for line in weights.read_text(errors="replace").splitlines():
        parts = line.split("\t")
        if len(parts) == 2 and not line.startswith("#") and parts[1].strip() == "0":
            quick.add(parts[0].strip())
    return [c[len(UI_SUITE_LINE):].strip() for c in commands
            if c.startswith(UI_SUITE_LINE) and c[len(UI_SUITE_LINE):].strip() in quick]


ANCHORS_LINE = "cd richos/engine && python3 scripts/mutation-anchors.py --quiet"


def anchors_command(repo, paths):
    """The mutant-anchor check as a one-element list when the change touches the engine and the
    script exists, else []. The land adds it to its selection; the commit adds it to the fast
    checks, so a mutant pointed at a line this branch changed is refused where it is made."""
    if any(p.startswith("richos/engine/") for p in paths) and (repo.top / "richos/engine/scripts/mutation-anchors.py").is_file():
        return [ANCHORS_LINE]
    return []


# THE FAST CHECKS RUN AT THE COMMIT TOO (CEO section 97, 2026-10-09). A merge into richos main
# was refused at the end of a 25-minute gate by two checks that take seconds: mutation-anchors
# .test.sh (about 4 s; a mutant pointed at a line the branch had changed) and
# spawn-guard-audience.test.sh (about 3 s; a new hook not classified). Every commit had passed
# here, because only the UI suites that weigh 0 ran. Now every selected check whose recorded
# duration is short runs as well. The durations are the merge gate's own: proof-run.py keeps the
# median of each check's last executions in <proof-runs>/<sha256(main checkout)[:12]>/weights.tsv
# (record_weights); this reads that file, and the worktree's own, and keeps no list of its own.
# A check with no recorded duration is not assumed fast; it runs at the land.
FAST_SECONDS = 10.0        # a check whose recorded median is under this runs at the commit
FAST_TOTAL_SECONDS = 40.0  # the most recorded time one commit spends on them; the rest wait for the land
FAST_HANG_SECONDS = 120    # a fast check still running at this is a failure, as for the quick suites


def recorded_durations(repo):
    """{label: median seconds} from the merge gate's weights.tsv (proof-run.py history_samples:
    `label <TAB> median [<TAB> samples]`). The main checkout's file wins over the worktree's."""
    base = os.environ.get("RICHOS_PROOF_RUN_DIR") or (
        "/Volumes/E1TB/state/richos/proof-runs" if sys.platform == "darwin"
        else str(Path.home() / ".richos-nightly/proof-runs"))
    found = {}
    for root in (repo.top, repo.common.parent):   # later wins: the main checkout's
        try:
            wid = hashlib.sha256(str(root).encode()).hexdigest()[:12]
            text = (Path(base) / wid / "weights.tsv").read_text(errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            parts = line.split("\t")
            if len(parts) >= 2:
                try:
                    found[parts[0]] = float(parts[1])
                except ValueError:
                    pass
    return found


def fast_candidates(repo, commands, durations):
    """[(seconds, label, cwd, argv)] for the selected commands that proof-run.py would label and
    whose recorded duration is under FAST_SECONDS. Labels follow proof-run.py plan(): an engine
    unit is `engine <unit>`, a run-tests.sh suite is its name without `.test.sh`, a script is its
    basename without `.test.sh`/`.sh`. A node suite is labeled by its file name (`heavy.js`), `node --test` is `web-app`.
    Any other shape (cargo, unreadable) is not a candidate."""
    out = []
    for line in commands:
        found = re.match(r"^cd (\S+) && (.+)$", line.strip())
        if not found:
            continue
        try:
            argv = shlex.split(found.group(2))
        except ValueError:
            continue
        cwd = found.group(1)
        if not argv or argv[0] == "cargo":
            continue
        if argv[:2] == ["bash", "scripts/ci-shard.sh"] and "--only-units" in argv[:-1]:
            units = [u for u in argv[argv.index("--only-units") + 1].split(",") if u]
            for unit in units:
                out.append(("engine " + unit, cwd, argv[:argv.index("--only-units") + 1] + [unit]))
        elif argv[0] == "scripts/run-tests.sh" and "--only" in argv:
            flags = [a for i, a in enumerate(argv[1:], 1) if a != "--only" and argv[i - 1] != "--only"]
            for i, a in enumerate(argv[:-1]):
                if a == "--only":
                    suite = argv[i + 1]
                    label = suite[:-len(".test.sh")] if suite.endswith(".test.sh") else suite
                    out.append((label, cwd, ["scripts/run-tests.sh", *flags, "--only", suite]))
        elif argv[:2] == ["node", "--test"]:
            out.append(("web-app", cwd, argv))     # proof-run.py plan(): the web app's suite
        elif argv[0] in ("bash", "python3", "node") and len(argv) >= 2 and not argv[1].startswith("-"):
            label = re.sub(r"\.test\.sh$|\.sh$", "", os.path.basename(argv[1]))
            out.append((label, cwd, argv))
    seen, chosen = set(), []
    for label, cwd, argv in out:
        key = (cwd, tuple(argv))
        if key in seen or label not in durations or durations[label] >= FAST_SECONDS:
            continue
        seen.add(key)
        chosen.append((durations[label], label, cwd, argv))
    return sorted(chosen, key=lambda row: (row[0], row[1]))


def run_fast_checks(repo, what, commands):
    """Run the selected checks that are fast by their recorded duration, fastest first, until
    FAST_TOTAL_SECONDS of recorded time is spent. Returns (ran labels, 1 on a refusal else 0)."""
    candidates = fast_candidates(repo, commands, recorded_durations(repo))
    ran, spent = [], 0.0
    for seconds, label, cwd, argv in candidates:
        if spent + seconds > FAST_TOTAL_SECONDS:
            continue
        spent += seconds
        say("+ (cd " + cwd + " && " + " ".join(shlex.quote(a) for a in argv) + ")")
        try:
            result = run_admitted(repo, argv, cwd=repo.top / cwd, env={**repo.env, **MUTATION_SWITCH},
                                    stdin=subprocess.DEVNULL, text=True,
                                    timeout=FAST_HANG_SECONDS)
        except subprocess.TimeoutExpired:
            banner(f"{what.upper()} REFUSED: {label} did not finish in {FAST_HANG_SECONDS} s of execution",
                   [f"Its recorded duration is {seconds:g} s; a hang is a failure, not a pass."])
            return ran, 1
        if result.returncode == 75 and not getattr(result, "admitted", True):
            banner(f"{what.upper()} REFUSED: {label} was not admitted", [
                "No test ran. The worker admission limit expired; this is an infrastructure refusal.",
                (result.stdout + result.stderr)[-4000:],
            ])
            return ran, 1
        if getattr(result, "admission_seconds", 0) >= 1:
            say(f"  {label}: admission {result.admission_seconds:.1f}s, execution {result.execution_seconds:.1f}s")
        if result.returncode:
            sys.stderr.write((result.stdout + result.stderr)[-4000:])
            banner(f"{what.upper()} REFUSED: {label} failed for this branch", [
                f"proof-for.sh selects it for this change and it takes {seconds:g} s, so it runs here; the land",
                "would run it too and refuse. Its output is just above. Fix the branch and commit again.",
            ])
            return ran, 1
        ran.append(label)
    return ran, 0


def branch_selection(repo, what, run_quick=True):
    started = time.monotonic()
    if not (repo.top / PROOF_FOR).is_file():
        banner(f"{what.upper()} REFUSED: cannot select the checks", [f"{PROOF_FOR} is not in this tree."])
        return 1
    paths = branch_paths(repo)
    if not paths:
        return 0
    # The range's head is the index written as a commit, a stand-in with a fixed message. The
    # battery answer (CEO ruling §81) is in the real message, which does not exist until the
    # commit-msg hook reads it, so proof-for.sh is told the head is pending and asks every
    # commit before it. 2026-10-01: without this, every commit touching richos/mobile/ was
    # refused here from 50a5bd1bd on, whatever its message carried.
    rng = branch_range(repo, paths)
    pending = ["--commit-pending"] if rng and knows(repo, PROOF_FOR, "--commit-pending)") else []
    commands, rc = select(repo, pending + rng if rng else ["--paths", ",".join(paths)], gate=True)
    if commands is None:
        refuse_selection(what, rc)
        say("  The land runs this same selection over the same change and would refuse it there.")
        return 1
    quick = quick_suites(repo, commands) if run_quick else []
    for suite in quick:
        say(f"+ (cd richos/app/ui/tests && node {suite})")
        try:
            result = run_admitted(repo, ["node", suite], cwd=repo.top / "richos/app/ui/tests", env=repo.env,
                                    stdin=subprocess.DEVNULL, text=True, timeout=120)
        except subprocess.TimeoutExpired:
            banner(f"{what.upper()} REFUSED: {suite} did not finish in 120 s of execution",
                   [f"It measures under a second ({QUICK_WEIGHTS}); a hang is a failure, not a pass."])
            return 1
        if result.returncode == 75 and not getattr(result, "admitted", True):
            banner(f"{what.upper()} REFUSED: {suite} was not admitted", [
                "No test ran. The worker admission limit expired; this is an infrastructure refusal.",
                (result.stdout + result.stderr)[-4000:],
            ])
            return 1
        if getattr(result, "admission_seconds", 0) >= 1:
            say(f"  {suite}: admission {result.admission_seconds:.1f}s, execution {result.execution_seconds:.1f}s")
        if result.returncode:
            sys.stderr.write((result.stdout + result.stderr)[-4000:])
            banner(f"{what.upper()} REFUSED: {suite} failed for this branch", [
                f"proof-for.sh selects {suite} for this change, so the land would run it and refuse.",
                "Its output is just above. Fix the branch and commit again.",
            ])
            return 1
    fast = []
    if run_quick:
        # What the land would run, minus what it leaves to the nightly (mutation passes, devices).
        kept, _moved = for_the_nightly(repo, commands)
        kept = [c for c in kept if not (c.startswith(UI_SUITE_LINE) and c[len(UI_SUITE_LINE):].strip() in quick)]
        fast, rc = run_fast_checks(repo, what, kept + anchors_command(repo, paths))
        if rc:
            return 1
    later = len(commands) - len(quick) - len(fast)
    ran = quick + fast
    say(f"autocheck: {what}: the land's selection over the branch's {len(paths)} changed path(s) maps cleanly; "
        f"ran {', '.join(ran) or 'no quick suite'}; {max(later, 0)} heavier check command(s) run at the land "
        f"({time.monotonic() - started:.1f}s)")
    return 0


# ---------------------------------------------------------------------------------------
# The land check
# ---------------------------------------------------------------------------------------

def git_path(repo, name):
    path = Path(git("rev-parse", "--git-path", name, cwd=repo.top))
    return path if path.is_absolute() else repo.top / path


def merge_heads(repo):
    """What is being merged. A conflicted merge concluded by `git commit` has MERGE_HEAD; an
    automatic `git merge` runs pre-merge-commit BEFORE writing it (measured, git 2.5x), and
    names its heads only in GIT_REFLOG_ACTION ("merge <name> ...")."""
    try:
        heads = [line.split()[0] for line in git_path(repo, "MERGE_HEAD").read_text().splitlines() if line.strip()]
        if heads:
            return heads
    except OSError:
        pass
    action = os.environ.get("GIT_REFLOG_ACTION", "")
    if not action.startswith("merge "):
        return []
    heads = []
    for name in action.split()[1:]:
        found = git("rev-parse", "--verify", "-q", f"{name}^{{commit}}", check=False, cwd=repo.top)
        if found.returncode:
            return []
        heads.append(found.stdout.strip())
    return heads


def knows(repo, path, flag):
    """Does this tree's copy of a tool take `flag`? A land into a main older than the flag
    runs that main's tool without it: slower, never looser."""
    try:
        return flag in (repo.top / path).read_text(errors="replace")
    except OSError:
        return False


def select(repo, argv, gate=False):
    """proof-for.sh's commands for one selection, or (None, rc) when it refuses. `gate`: the
    merge gate's selection (proof-for.sh --gate), where a directory input selects its suite only
    for a product path."""
    flags = ["--gate"] if gate and knows(repo, PROOF_FOR, "--gate)") else []
    result = subprocess.run(["bash", PROOF_FOR, "--quiet", *flags, *argv], cwd=repo.top, env=repo.env,
                            stdin=subprocess.DEVNULL, capture_output=True, text=True)
    if result.returncode:
        sys.stderr.write(result.stdout + result.stderr)
        return None, result.returncode
    return [line.strip() for line in result.stdout.splitlines() if line.strip()], 0


# THE MERGE GATE'S LIMITS (2026-09-30). The CEO, on a day of 25-71 minute merge checks that
# failed on everything but the fixes: "That whole CI shitshow had the exact same end effect as
# what has been happening today". So a land runs the lint on what changed and the suites that
# own the changed files (proof-for.sh --gate), each check stopped at CHECK_CAP_SECONDS and the
# whole gate at GATE_CAP_SECONDS. What did not reach a verdict inside them is NOT RUN, named in
# the receipt, and never blocks; the nightly (nightly-local.py) still runs everything and stays
# the release gate. A failing check blocks; that is all (land_verdict). Adoption ledger §2.4,
# T3 Code's release gating: affected checks only, every job capped at 10 minutes.
CHECK_CAP_SECONDS = 600
GATE_CAP_SECONDS = 900

# A LARGE LAND RUNS IN ROUNDS (2026-10-02). The combined land of 13 finished fixes
# (cc/zach-opus-e2fix1, 214 checks) was refused four times with no failing check: each attempt
# ended at the 900 s cap and its one retry did too. Measured from those attempts, the checks
# that reached a verdict took 6608 s between them (native-android-app 471 s, contract-integrity
# section Q 261 s, proof-run 260 s, ...; 36 never reached one), and a run kept only about three
# running at once: every start waits for total CPU under CEO ruling §77's 80% line, which the
# gate's own checks hold the Mac above. So the plan needed about 2200 s however quiet the Mac
# was when it started, and no single cap fits every selection: a cap scaled from planned weights
# trusts dated data that is wrong in both directions (README.md, "A check planned past its cap").
# Instead the gate runs ROUNDS of at most GATE_CAP_SECONDS each. A round keeps every pass (the
# runner reuses a validated pass on the same tree and inputs) and the next round runs only the
# checks that still have no verdict. It stops when every check has one, when a check times out
# at its own 600 s cap a second time (a hang is a hang), when a round decides nothing new (the
# next one would not either), or at GATE_MAX_ROUNDS: the total bound is
# GATE_MAX_ROUNDS x GATE_CAP_SECONDS = 5400 s, about twice what this land needed. Every round
# waits for admission and a proof-run slot at most GATE_CAP_SECONDS, the same number each time:
# until today the first run passed what was left of the gate (893 s) and its retry 900 s, the
# waits were part of every check's identity, and the retry reused nothing.
GATE_MAX_ROUNDS = 6

# WHAT THE MERGE NEVER RUNS: a suite that needs a device this Mac has one of. On 2026-09-30 the
# iPhone suites fought over the one simulator in merge after merge and refused finished fixes
# that never touched the phone. The nightly (nightly-local.py) runs these, as it runs
# everything; the merge names each one in its verdict and its receipt as NOT RUN. The iPhone
# simulator suites are named here (proof-run.py queues the first three on one simulator lane
# for the same reason); a suite that needs a screen, the host's or the test VM's guest under
# RICHOS_GUI_HOST, is read off the suite exactly as proof-run.py's is_host_screen reads it.
SHARED_DEVICE_SUITES = {
    "native-ios-app.test.sh": "the iPhone simulator",
    "native-ios-share.test.sh": "the iPhone simulator",
    "native-ios-ui.test.sh": "the iPhone simulator",
    "mobile-ios.test.sh": "the iPhone simulator",
}
HOST_SCREEN = (re.compile(r"^[ \t]*(\.|source)[ \t]+\S*lib/gui-launch\.sh", re.M),
               re.compile(r"^# run-tests: host-screen", re.M))


def nightly_only(repo, suite):
    """Why the merge leaves this script suite to the nightly, or None."""
    if suite in SHARED_DEVICE_SUITES:
        return f"needs {SHARED_DEVICE_SUITES[suite]}; the nightly runs it"
    try:
        text = (repo.top / "richos/app/scripts" / suite).read_text(errors="replace")
    except OSError:
        return None
    if any(pattern.search(text) for pattern in HOST_SCREEN):
        return "needs a screen (the host's or the test VM's); the nightly runs it"
    return None


# A MUTATION PASS NEVER RUNS IN THE MERGE (2026-09-30). The merge of cc/zach-opus-q1 (4e73fd89)
# selected `scripts/operator-fences-mutation.test.sh`, a unit whose whole job is the fence
# suite's mutation pass (planned 1408 s): a comment in it names `ci-unit-weights.tsv`, the land
# changed that file, and the engine's selector maps a file to every unit that mentions its name
# (ci-affected-units.sh, "basename dependency (conservative)"). It ran into its 600 s cap while
# the Mac stayed too busy to start anything else, and the gate passed with 60 of 63 checks NOT
# RUN. The selector is right to be conservative; the merge is the wrong place for the pass. So
# an engine unit that IS a mutation pass is never started here, whatever selected it: one named
# `<suite>-mutation.test.sh`, or one whose header says `# merge-gate: mutation-pass`. It is
# named NOT RUN and the nightly engine run (nightly-engine.py) runs it with every other unit.
#
# The passes suites run at their own end (about seventy engine suites and three app suites
# invoke a `*.mutation.sh` or `*.mutation.py` harness) are switched off here the other way: the
# gate runs every check with MUTATION_SWITCH, each harness's first line reads it, prints NOT RUN
# and exits 0, and the suite's own checks still run and still decide. The nightly engine run
# sets it to 1 (every pass on), and so does the app nightly's workspace gate; unset, as an
# engineer runs a suite by hand, the engine harnesses run as they always did. The lead's
# decision on esc-20260930T223507Z-b12f0d6a, option B.
MUTATION_SWITCH = {"RICHOS_MUTATION_PASSES": "0", "RICHOS_FOURTEEN_MUTANTS": "0"}
MUTATION_UNIT_NAME = re.compile(r"-mutation\.test\.sh$")
MUTATION_UNIT_MARK = re.compile(r"^# merge-gate: mutation-pass\b", re.M)
MUTATION_WHY = "a mutation pass; the nightly engine run (nightly-engine.py) runs it"


def mutation_unit(repo, unit):
    """True when an engine unit (`path` or `path:section`) is a mutation pass by its name or
    its declaration. An unreadable file is judged by its name alone."""
    path = unit.split(":", 1)[0]
    if MUTATION_UNIT_NAME.search(path):
        return True
    try:
        with open(repo.top / "richos/engine" / path, errors="replace") as stream:
            head = stream.read(8192)
    except OSError:
        return False
    return bool(MUTATION_UNIT_MARK.search(head))


def for_the_nightly(repo, commands):
    """(the commands the merge runs, [{check, why}] it leaves to the nightly). A run-tests.sh
    line keeps its other suites; a suite run on its own line is dropped whole; an engine line
    keeps its units that are not mutation passes."""
    kept, moved = [], []
    for line in commands:
        found = re.match(r"^cd (\S+) && (.+)$", line.strip())
        if found and found.group(1) == "richos/engine":
            argv = shlex.split(found.group(2))
            if argv[:2] == ["bash", "scripts/ci-shard.sh"] and "--only-units" in argv[:-1]:
                at = argv.index("--only-units") + 1
                units = [u for u in argv[at].split(",") if u]
                passes = [u for u in units if mutation_unit(repo, u)]
                moved += [{"check": "engine " + u, "why": MUTATION_WHY, "suites": []} for u in passes]
                left = [u for u in units if u not in passes]
                if left:
                    kept.append("cd richos/engine && " + " ".join(
                        shlex.quote(a) for a in argv[:at] + [",".join(left)] + argv[at + 1:]))
                continue
        argv = shlex.split(found.group(2)) if found and found.group(1) == "richos/app" else []
        if argv[:1] == ["scripts/run-tests.sh"] and "--only" in argv:
            suites = [argv[i + 1] for i, arg in enumerate(argv[:-1]) if arg == "--only"]
            flags = [arg for i, arg in enumerate(argv[1:], 1) if arg != "--only" and argv[i - 1] != "--only"]
            left = []
            for suite in suites:
                why = nightly_only(repo, suite)
                if why:
                    moved.append({"check": suite, "why": why, "suites": []})
                else:
                    left.append(suite)
            if left:
                kept.append("cd richos/app && " + " ".join(
                    ["scripts/run-tests.sh", *flags, *[part for suite in left for part in ("--only", suite)]]))
            continue
        if argv[:1] == ["bash"] and len(argv) > 1 and re.fullmatch(r"scripts/[^/]+\.test\.sh", argv[1]):
            why = nightly_only(repo, argv[1][len("scripts/"):])
            if why:
                moved.append({"check": argv[1][len("scripts/"):], "why": why, "suites": []})
                continue
        kept.append(line)
    return kept, moved


DEVICE_IDENTIFIERS = "richos/engine/scripts/lib/device-identifiers.py"


def device_identifier_gate(repo, what):
    """0, or 1 after printing a refusal: the tree being landed holds an identifier of one of the
    owner's test devices (the private list, outside every repository). 2026-10-02: a CoreDevice
    id reached a branch in a commit made before the commit-time scan existed, and a merge
    carries what a branch already holds without a commit to scan, so the id was one push away
    from public. This reads the WHOLE index (the tree main will point at), not the commit's
    additions. It prints file:line and a masked preview, never the identifier. No list on this
    machine (a public clone) is announced, not refused; a list that cannot be trusted refuses."""
    script = repo.top / DEVICE_IDENTIFIERS
    if not script.is_file():
        say(f"autocheck: {what}: {DEVICE_IDENTIFIERS} is not in this tree; device identifiers NOT checked")
        return 0
    try:
        out = run_bounded([sys.executable, str(script), "--scan-tree", str(repo.top)], text=True, timeout=600, env=repo.env)
        rows = out.stdout.splitlines()
    except (OSError, subprocess.SubprocessError):
        rows = []
    verdict = rows[0].split("\t")[0] if rows else "BROKEN"
    if verdict == "CLEAN":
        return 0
    if verdict == "ABSENT":
        say(f"autocheck: {what}: DEVICE-IDENTIFIER LIST ABSENT ({rows[0].partition(chr(9))[2]}); the tree was NOT checked for test-device identifiers")
        return 0
    if verdict == "FOUND":
        banner(f"{what.upper()} REFUSED: the tree holds an identifier of a known test device", [
            *[f"  {label} (matches {preview})" for label, _, preview in (r.partition("\t") for r in rows[1:]) if label],
            "",
            "Replace each with a neutral made-up value of the same shape and commit again. The identifier is",
            "never printed; the list is private and outside every repository: edit that, never this tree.",
        ])
        return 1
    banner(f"{what.upper()} REFUSED: the device-identifier check could not be trusted",
           [rows[0].partition("\t")[2] if rows else "the scanner printed nothing"])
    return 1


def land_check(repo, what, staged, range_argv, changed_lint=True, receipt=True):
    """Run the suites proof-for.sh assigns plus the lint on the working tree, which the
    caller has established IS the tree being landed. Returns 0 and writes the receipt when
    nothing it ran failed. `changed_lint`: HEAD is the main being landed onto, so the lint
    measures the change against it (`lint.sh --changed`); the push backstop, where HEAD is
    already the land, measures the whole tree."""
    started = time.monotonic()
    for need in (PROOF_FOR, PROOF_RUN, LINT):
        if not (repo.top / need).is_file():
            banner(f"{what.upper()} REFUSED: cannot select the checks", [f"{need} is not in this tree."])
            return 1
    if (rc := device_identifier_gate(repo, what)):
        return rc
    commands = []
    covered = set()
    if range_argv:
        found, rc = select(repo, range_argv, gate=True)
        if found is None:
            return refuse_selection(what, rc)
        commands += found
        covered = set(git("diff", "--name-only", "--no-renames", *range_argv, env=repo.env, cwd=repo.top).splitlines())
        extra = [p for p in staged if p not in covered]
    else:
        extra = list(staged)
    if extra:
        found, rc = select(repo, ["--paths", ",".join(extra)], gate=True)
        if found is None:
            return refuse_selection(what, rc)
        commands += [c for c in found if c not in commands]
    commands, nightly = for_the_nightly(repo, commands)
    # The physical-phone check reads the whole tree, about a second; every land runs it (physical_check).
    # Written in the selection's own form, `cd <dir> && <command>`, the only line proof-run.py reads:
    # the bare `python3 richos/mobile/physical.py scan` refused every land that selected it (2026-10-02).
    scanner = Path(PHYSICAL_CHECK)
    commands += [f"cd {scanner.parent} && python3 {scanner.name} scan"] if (repo.top / PHYSICAL_CHECK).is_file() else []
    commands += anchors_command(repo, covered | set(staged))  # every mutant's target text still exists (the passes run only in the nightly; ~1 s)
    if nightly:
        say(f"autocheck: {what}: left to the nightly: " + ", ".join(f"{row['check']} ({row['why']})" for row in nightly))
    # The lint checks the application (it lives under richos/app). A land that changes nothing
    # under richos/app has nothing for it to check, as in commit_check, so it does not pay the
    # compiler work (hunt part 2, finding 13). Any application path, a deletion included, runs it.
    # On the changed files only: `--changed` is the lint's own rule, never looser than `--all`
    # (a count that grows is decided by the full pass, lint/driver.py).
    engine_paths = sorted(p for p in covered | set(staged) if p.startswith("richos/engine/"))
    load_check = "richos/app/scripts/lint/load_rules.py"
    if engine_paths and (repo.top / load_check).is_file():
        trusted = range_argv[0].split("..", 1)[0] if what == "push" and range_argv else "refs/heads/main"
        argv = ["python3", load_check, "--root", ".", "--trusted-ref", trusted, "--paths", *engine_paths]
        commands.append("cd . && " + " ".join(shlex.quote(a) for a in argv))
    touches_app = any(p.startswith("richos/app/") for p in covered | set(staged))
    if touches_app and not any("lint.test.sh" in c for c in commands):
        mode = "--changed" if changed_lint and knows(repo, LINT_DRIVER, "--changed") else "--all"
        commands.append(f"cd richos/app && bash scripts/lint.sh {mode}")
    say(f"autocheck: {what}: {len(commands)} check command(s) selected by proof-for.sh, run by proof-run.py:")
    for c in commands:
        say("    " + c)
    with tempfile.NamedTemporaryFile("w", prefix="autocheck-land-", suffix=".txt", delete=False) as f:
        f.write("\n".join(commands) + "\n")
        plan = f.name
    # Keep each attempt: the runner validates the frozen plan, source and evidence.
    # Exact retries resume; changed trees use the freshly selected plan and let
    # the existing runner validate which individual results remain applicable.
    identity = [str(repo.top.resolve()), git("rev-parse", "HEAD", cwd=repo.top), repo.index_tree(), commands]
    key = hashlib.sha256(str(repo.top.resolve()).encode()).hexdigest()
    root = Path(os.environ.get("RICHOS_AUTOCHECK_PROOF_ROOT", "/Volumes/E1TB/state/richos/proof-runs/autocheck"))
    if not Path("/Volumes/E1TB").is_mount() or not root.resolve().is_relative_to("/Volumes/E1TB"):
        os.unlink(plan)
        banner(f"{what.upper()} REFUSED: proof storage unavailable", ["Proof attempts must stay on the mounted external SSD."])
        return 1
    root = root / key
    root.mkdir(parents=True, exist_ok=True)
    prior = root / "last-attempt.json"
    previous = None
    saved_identity = None
    if prior.exists():
        try:
            saved = json.loads(prior.read_text())
            previous = Path(saved["directory"])
            saved_identity = saved["identity"]
            if not (previous / "plan.json").is_file():
                raise ValueError("saved proof plan is missing")
        except (OSError, ValueError, KeyError, TypeError) as exc:
            banner(f"{what.upper()} REFUSED: retry evidence is unreadable", [str(exc)])
            os.unlink(plan)
            return 1
    directory = Path(tempfile.mkdtemp(prefix="attempt-", dir=root))
    summary_path = str(root / (directory.name + "-summary.json"))
    argv = (["--resume", str(previous)] if previous and saved_identity == identity
            else ["--commands", plan, *(["--reuse", str(previous)] if previous else [])])
    reason = os.environ.get("RICHOS_AUTOCHECK_RETRY_REASON")
    if reason:
        argv += ["--retry-reason", reason]
    # The caps (THE MERGE GATE'S LIMITS, A LARGE LAND RUNS IN ROUNDS). The first round gets
    # what is left of the gate's time; every round waits for admission and for a proof-run slot
    # at most GATE_CAP_SECONDS, the same in every round and every attempt, so nothing about the
    # wait makes a pass look like a different check's.
    capped = knows(repo, PROOF_RUN, "--run-cap")
    def caps(run_cap):
        return (["--cap", str(CHECK_CAP_SECONDS), "--run-cap", str(run_cap), "--admission-wait",
                 str(GATE_CAP_SECONDS), "--slot-wait", str(GATE_CAP_SECONDS)] if capped else [])
    rounds = 1
    alone = {}   # each check retried alone this gate run (retry_failed_alone)
    try:
        say(f"autocheck: {what}: mutation passes are off in the merge ("
            + " ".join(f"{k}={v}" for k, v in MUTATION_SWITCH.items()) + "); the nightlies run them")
        result = repo.run(["python3", PROOF_RUN, *argv, *caps(max(60, int(GATE_CAP_SECONDS - (time.monotonic() - started)))),
                           "--log-dir", str(directory), "--summary-out", summary_path],
                          env={**repo.env, **MUTATION_SWITCH})
        if (directory / "plan.json").is_file():
            pending = root / "last-attempt.pending"
            pending.write_text(json.dumps({"directory": str(directory), "identity": identity}))
            os.replace(pending, prior)
        blocking, not_run, why_not = land_verdict(result.returncode, summary_path, directory)
        blocking, not_run, current = retry_failed_alone(repo, what, root, prior, identity, directory, caps,
                                                        blocking, not_run, alone)
        blocking, not_run, why_not, rounds = more_rounds(repo, what, root, prior, identity, current, caps,
                                                         blocking, not_run, why_not, alone)
        not_run = nightly + not_run
    finally:
        os.unlink(plan)
    seconds = time.monotonic() - started
    if blocking is None or blocking:
        failed = blocking is None or any(not row.get("message") for row in blocking)
        banner(f"{what.upper()} REFUSED: a check it owns " + ("failed" if failed else "has no verdict"), [
            "proof-run.py's summary above names the check, its state and its log.",
            *([why_not] if why_not else []),
            *(line for row in blocking or [] for line in
              [row.get("message") or f"FAILED: {row['check']} ({row['result']})", *alone_lines(alone.get(row["check"]))]),
            # The gate's environment is not a shell's (LC_ALL=C from the merge, TZ, the nightly's
            # conditions): this file reruns the check exactly as it ran here (proof-run.py write_rerun).
            *(f"  rerun {row['check']} as the gate ran it: sh {shlex.quote(row['rerun'])}"
              for row in blocking or [] if row.get("rerun")),
            "Nothing was committed: fix the branch and land it again. What passed on this tree is kept:",
            "the next attempt on the same tree runs only what has no verdict.",
            "git's --no-verify skips this, and every skip is recorded in the lead's escalation ledger.",
        ])
        return 1
    tree = repo.index_tree()
    flaky = [record for record in alone.values() if record["alone"]["result"] == "passed"]
    if receipt:
        repo.write_land_receipt(tree, dict(what=what, commands=commands, seconds=round(seconds, 1), not_run=not_run,
                                           rounds=rounds, failed_under_load_passed_alone=flaky))
    else:
        tree = "(none: a measurement is not a land)"
    if flaky:
        banner(f"{what.upper()}: {len(flaky)} CHECK(S) FAILED UNDER THE GATE'S LOAD AND PASSED ALONE", [
            *(line for record in flaky for line in [f"FAILED UNDER LOAD, PASSED ALONE: {record['check']}",
                                                    *alone_lines(record)]),
            "Each ran again alone, once, after the rest had finished; it passed, so it does not refuse",
            "the merge. Both logs are kept, and the receipt names them.",
        ])
    if not_run:
        names = ", ".join(f"{row['check']} ({row['why']})" for row in not_run)
        banner(f"{what.upper()} ALLOWED WITH {len(not_run)} CHECK(S) NOT RUN, WHICH IS NOT A PASS", [
            f"NOT RUN: {names}.",
            "Nothing that ran failed. These need a device the merge never uses, are mutation passes,",
            "or did not reach a verdict inside the gate's limits (600 s a check, rounds of 900 s); the",
            "receipt records them as NOT RUN, never as passed, and the nightlies run them",
            "(nightly-local.py for the app, nightly-engine.py for every engine unit).",
            "See autocheck/README.md.",
        ])
        say(f"autocheck: {what}: passed with {len(not_run)} NOT RUN: {names}; "
            f"{seconds:.1f}s; receipt for tree {tree[:12]}{passed_alone_note(flaky)}")
        return 0
    say(f"autocheck: {what}: every selected check passed in {seconds:.1f}s; receipt for tree {tree[:12]}"
        f"{passed_alone_note(flaky)}")
    return 0


# Why a check that did not pass is NOT RUN rather than a failure (land_verdict).
NOT_RUN_WHY = {"timed-out": "over its 600 s cap", "not-admitted": "not admitted",
               "contained": "stopped by the verification controller",
               "resource-envelope-exceeded": "stopped by the verification controller",
               "infrastructure-failed": "the runner could not supervise it",
               "cleanup-failed": "the runner could not clean it up",
               "scheduler-starvation": "not admitted", "resource-recovery-exhausted": "not admitted"}
ENDED = "cancelled"  # dialect-exempt: proof-run.py's state value for a check its run ended (CANCELED there)
NOT_RUN_WHY[ENDED] = "ended at the gate's 900 s cap"
# A pass invalidated only because what it read changed while the run went (proof_evidence.py,
# Record.save and finalize): no verdict either way, which is not a failure of the change.
# The two states that mean "the host ran out of time, not the change": the next round runs them
# (more_rounds). A check that times out at its own cap twice refuses the merge.
RETRY_STATES = ("timed-out", ENDED)
INPUTS_MOVED = ("inputs changed", "source changed during execution")
# proof-run.py --only-check marks a check it left out of a round this way (leave_unselected).
UNSELECTED = "retry-unselected"


def left_out(row):
    """Did the runner leave this check out of a round because the round did not name it?"""
    return (row.get("state") == "not-run"
            and (any(s.get("state") == UNSELECTED for s in row.get("suites") or [])
                 or str(row.get("why", "")).startswith("not selected for this retry")))


# A CHECK THAT FAILS UNDER THE GATE'S OWN LOAD RUNS AGAIN ALONE, ONCE (2026-10-08). Twice that day
# the gate refused a merge on one timing check while its own parallel checks held the Mac at 97-99%
# CPU, and the same check passed when Rich reran it alone on the same tree: setup.js case 24
# ("page.waitForFunction: Timeout 30000ms exceeded", attempt-alpn3mjn/28-setup.js.log) and
# splash.js case 10b (the shutter's window moved 236 ms to 358 ms, attempt-pk8d8y3c/45-splash.js.log).
# His manual step was the first step of docs/development/verification-retries.md: retry the failed
# unit alone, with normal parallelism and admission. The gate now does it: after a round has
# finished, each check that FAILED runs again in a run of its own (--resume, --only-check: every
# other check is NOT RUN there and every validated pass is carried), under the same caps and
# admission as any round. Passed alone: it no longer refuses, and the banner, the final line and the
# receipt (`failed_under_load_passed_alone`) say it failed under load and passed alone, with both
# logs. Failed alone, or no verdict alone: the gate refuses as before, naming both logs. Once per
# check per gate run, never a loop; the first check that fails alone ends the retries, because the
# gate refuses whatever the others would do. Only `failed`: `blocked` is a failure the runner
# refuses to repeat, `invalid` is a check that could not show it ran, and the no-verdict states
# have their own rounds (more_rounds).
ALONE_REASON = "merge gate: failed while the gate ran its checks together; one retry alone before the gate decides"


def alone_lines(record):
    """The two logs of a check retried alone, for a banner; [] for a check that was not."""
    if not record:
        return []
    return [f"  under the gate's load: {record['under_load']['result']}, log {record['under_load']['log'] or '(none)'}",
            f"  alone: {record['alone']['result']}, log {record['alone']['log'] or '(none)'}"]


def passed_alone_note(flaky):
    if not flaky:
        return ""
    return "; failed under load and passed alone: " + ", ".join(record["check"] for record in flaky)


def summary_rows(path):
    try:
        with open(path) as stream:
            return {row.get("check"): row for row in json.load(stream)["checks"]}
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        return {}


def retry_failed_alone(repo, what, root, prior, identity, current, caps, blocking, not_run, alone):
    """Each failed check not yet retried runs again alone, once (see ALONE_REASON above).
    Returns (blocking, not_run, the run directory the next round resumes)."""
    if not blocking or not knows(repo, PROOF_RUN, "--only-check"):
        return blocking, not_run, current
    for row in list(blocking):
        name = row["check"]
        if row.get("result") != "failed" or row.get("message") or name in alone:
            continue
        say(f"autocheck: {what}: {name} failed while the gate ran its checks together; "
            "it runs again alone, once, before the gate decides")
        again = Path(tempfile.mkdtemp(prefix="attempt-", dir=root))
        again_summary = str(root / (again.name + "-summary.json"))
        result = repo.run(["python3", PROOF_RUN, "--resume", str(current), "--only-check", name,
                           "--retry-reason", ALONE_REASON, *caps(GATE_CAP_SECONDS), "--log-dir", str(again),
                           "--summary-out", again_summary], env={**repo.env, **MUTATION_SWITCH})
        if (again / "plan.json").is_file():
            pending = root / "last-attempt.pending"
            pending.write_text(json.dumps({"directory": str(again), "identity": identity}))
            os.replace(pending, prior)
            current = again
        rows = summary_rows(again_summary)
        mine = rows.get(name) or {}
        alone[name] = {"check": name, "under_load": {"result": row["result"], "log": row.get("log")},
                       "alone": {"result": mine.get("result") or "no verdict", "log": mine.get("log")}}
        if mine.get("result") != "passed":
            say(f"autocheck: {what}: {name} did not pass alone either ({alone[name]['alone']['result']}); "
                "the gate refuses")
            break
        say(f"autocheck: {what}: {name} FAILED UNDER LOAD AND PASSED ALONE; "
            f"under load: {row.get('log')}; alone: {mine.get('log')}")
        blocking.remove(row)
        # What else ran in that run (the engine receipts proof, after a unit) takes its new verdict;
        # a check that run left out keeps the row it had, and one that had passed and whose pass
        # could not be carried has no verdict, so the next round runs it (as in more_rounds).
        latest_blocking, latest_not_run, _ = land_verdict(result.returncode, again_summary, again)
        known = {r["check"] for r in blocking} | {r["check"] for r in not_run}
        left = {check for check, other in rows.items() if left_out(
            {"state": other.get("result"), "why": (other.get("not_run") or {}).get("why", ""),
             "suites": (other.get("not_run") or {}).get("suites")})}
        reran = set(rows) - left - {name}
        blocking = [r for r in blocking if r["check"] not in reran] + [
            r for r in latest_blocking or [] if r["check"] in reran]
        not_run = [r for r in not_run if r["check"] not in reran] + [
            r for r in latest_not_run if r["check"] in reran] + [
            {"check": check, "why": "its earlier pass could not be carried over", "state": ENDED, "suites": []}
            for check in sorted(left - known)]
    return blocking, not_run, current


def more_rounds(repo, what, root, prior, identity, directory, caps, blocking, not_run, why_not, alone=None):
    """A LARGE LAND RUNS IN ROUNDS (see GATE_MAX_ROUNDS). After the first round: while nothing
    failed and some check has no verdict (timed out at its cap, or ended at the round's cap), run
    another round that resumes the last one (every validated pass kept, its receipt carried) and
    names only those checks. Returns (blocking, not_run, why_not, rounds).

    A check the runner left out of a round keeps the row it had: a NOT RUN keeps its reason. A
    check that had PASSED and was left out was not carried over (its pass no longer validated),
    so it has no verdict and the next round runs it. The gate refuses, naming each check with no
    verdict, when one times out at its own cap a second time, when a round decides nothing that
    the round before had not, or when GATE_MAX_ROUNDS rounds have run."""
    rounds, current, timed_out, before = 1, directory, set(), None
    only = knows(repo, PROOF_RUN, "--only-check")
    while blocking == [] and (current / "plan.json").is_file():
        retry = [row for row in not_run if row.get("state") in RETRY_STATES]
        if not retry:
            break
        names = [row["check"] for row in retry]
        twice = [row for row in retry if row["state"] == "timed-out" and row["check"] in timed_out]
        timed_out |= {row["check"] for row in retry if row["state"] == "timed-out"}
        stalled = before is not None and not (before - set(names))
        if twice or stalled or rounds >= GATE_MAX_ROUNDS:
            if twice:
                blocking = [{"check": row["check"], "result": row["state"],
                             "message": f"NO VERDICT AFTER ONE RETRY: {row['check']} ({row['state']}); re-run it alone."}
                            for row in twice]
                why_not = "A check that timed out at its own 600 s cap did it again on its retry: a hang is not a pass."
            else:
                blocking = [{"check": row["check"], "result": row["state"],
                             "message": f"NO VERDICT AFTER {rounds} ROUNDS: {row['check']} ({row['state']}); re-run it alone."}
                            for row in retry]
                why_not = (f"Round {rounds} decided nothing that round {rounds - 1} had not; another would not either."
                           if stalled else
                           f"The gate's {GATE_MAX_ROUNDS} rounds of at most {GATE_CAP_SECONDS} s each "
                           f"(its total bound, {GATE_MAX_ROUNDS * GATE_CAP_SECONDS} s) are spent.")
            not_run = [row for row in not_run if row.get("state") not in RETRY_STATES]
            break
        rounds += 1
        say(f"autocheck: {what}: round {rounds} of at most {GATE_MAX_ROUNDS}: no verdict yet for {len(names)} "
            f"check(s) ({', '.join(names)}); every pass so far is kept and only these run")
        again = Path(tempfile.mkdtemp(prefix="attempt-", dir=root))
        again_summary = str(root / (again.name + "-summary.json"))
        argv = ["--resume", str(current), "--retry-reason",
                "merge gate: an owning check ended with no verdict at a time cap; the next round runs it"]
        if only:
            # Only these checks (hunt v2 V02): a bare --resume puts every unfinished check back
            # in the same scheduler. A runner without --only-check resumes the whole saved plan.
            for name in names:
                argv += ["--only-check", name]
        result = repo.run(["python3", PROOF_RUN, *argv, *caps(GATE_CAP_SECONDS), "--log-dir", str(again),
                           "--summary-out", again_summary], env={**repo.env, **MUTATION_SWITCH})
        if (again / "plan.json").is_file():
            pending = root / "last-attempt.pending"
            pending.write_text(json.dumps({"directory": str(again), "identity": identity}))
            os.replace(pending, prior)
        earlier = {row["check"]: row for row in not_run}
        blocking, latest, why_not = land_verdict(result.returncode, again_summary, again)
        not_run = []
        for row in latest:
            if not left_out(row):
                not_run.append(row)
            elif row["check"] in earlier:
                not_run.append(earlier[row["check"]])  # a NOT RUN keeps its own reason
            else:
                # It passed earlier and its pass was not carried into this round: no verdict.
                not_run.append(dict(row, state=ENDED, why="its earlier pass could not be carried over"))
        before, current = set(names), again
        if alone is not None:
            # A check that failed in this round, under its load, runs again alone first (ALONE_REASON).
            blocking, not_run, current = retry_failed_alone(repo, what, root, prior, identity, current, caps,
                                                            blocking, not_run, alone)
    return blocking, not_run, why_not, rounds


def land_verdict(rc, summary_path, directory):
    """(blocking rows, not_run rows, reason) from proof-run's summary. A failing check blocks:
    `failed`, `blocked` (an unchanged failure the runner refuses to run again), and `invalid`
    for any reason but inputs that moved during the run. Everything else that did not pass did
    not reach a verdict: NOT RUN, named, never blocking. The coverage verifier `engine receipts`
    fails whenever one of its units did not pass, so it blocks only when every unit passed.
    (None, [], reason) when there is no verdict to read."""
    if rc == 0:
        return [], [], None
    if rc not in (1, 3):
        return None, [], f"proof-run.py exited {rc}, which is not a verdict."
    try:
        with open(summary_path) as stream:
            checks = json.load(stream)["checks"]
    except (OSError, ValueError, KeyError, TypeError) as exc:
        return None, [], f"proof-run.py exited {rc} and its summary is unreadable ({exc})."
    try:
        outcomes = json.loads((Path(directory) / "outcomes.json").read_text())
        outcomes = outcomes if isinstance(outcomes, dict) else {}
    except (OSError, ValueError):
        outcomes = {}
    units = [row for row in checks if row.get("check", "").startswith("engine ") and row["check"] != "engine receipts"]
    units_passed = all(row.get("result") == "passed" for row in units)
    blocking, not_run = [], []
    for row in checks:
        state, name = row.get("result"), row.get("check")
        if state == "passed":
            continue
        why = None
        if state in ("failed", "blocked"):
            if name == "engine receipts" and not units_passed:
                why = "its engine units did not all reach a verdict"
        elif state == "invalid":
            invalid = str((outcomes.get(name) or {}).get("invalid", ""))
            if any(phrase in invalid for phrase in INPUTS_MOVED):
                why = "what it read changed while the gate ran"
        elif state == "not-run":
            why = (row.get("not_run") or {}).get("why") or "not run"
        else:
            why = NOT_RUN_WHY.get(state, state)
        if why is None:
            blocking.append({"check": name, "result": state, "rerun": row.get("rerun"), "log": row.get("log")})
        else:
            not_run.append({"check": name, "why": why, "state": state,
                            "suites": (row.get("not_run") or {}).get("suites", [])})
    return blocking, not_run, None


def refuse_selection(what, rc):
    reason = {1: "a changed code path is covered by no suite (UNCOVERED above)",
              3: "a commit touching richos/mobile/ has not answered the battery question (CEO ruling §81)"}.get(
        rc, f"proof-for.sh could not map the change (exit {rc})")
    banner(f"{what.upper()} REFUSED: {reason}", ["proof-for.sh's own explanation is printed above."])
    return 1


def land_from_index(repo, what, receipt=True):
    """pre-commit or pre-merge-commit on main: the index is the tree being landed. `measure`
    runs the same check, written nowhere (receipt=False)."""
    dirty = git("diff", "--quiet", check=False)
    if dirty.returncode:
        banner(f"{what.upper()} REFUSED: the working tree differs from what is being committed", [
            "The land checks run the suites on the working tree, so it must be exactly the tree",
            "that main will point at. Stage or discard the difference (git diff) and try again.",
        ])
        return 1
    staged = [p for p in git("diff", "--cached", "--name-only", "--no-renames").splitlines() if p]
    return land_check(repo, what, staged, land_range(repo, merge_heads(repo)), receipt=receipt)


def land_range(repo, heads):
    """The range the merge gate asks about: main's tip to the tree being landed, that tree
    written as a merge commit nothing refers to (parents HEAD and the merged head), as
    branch_range writes the commit's index. Its diff is what the merge changes on main, which
    is what the branch brings; its head is the tree main will point at, so the selectors read
    the suites and files that land; its commits are the branch's (battery-check.py skips merge
    commits). [] when this is not a merge of one head: the staged paths are then asked alone.

    2026-10-01: the range was `HEAD..<branch tip>`. `git diff A..B` compares the two TREES, so
    for a branch cut before main's last land it listed every file main had changed since the
    branch's base, and the engine's selector read those files and the suites from the branch
    tip, which predates them: cc/echo-opus-voiceecho1 (no engine path) was refused at main
    bb112ab68 with seven of main's own engine files "named by NO suite"."""
    if len(heads) != 1:
        return []
    tree = git("write-tree", cwd=repo.top)
    landed = git("-c", "user.name=autocheck", "-c", "user.email=autocheck@invalid", "commit-tree", tree,
                 "-p", "HEAD", "-p", heads[0], "-m", "autocheck: the merge being landed", cwd=repo.top)
    return [f"HEAD..{landed}"]


def pre_push(repo, stdin_text):
    for line in stdin_text.splitlines():
        parts = line.split()
        if len(parts) != 4:
            continue
        _local_ref, local_sha, remote_ref, remote_sha = parts
        if remote_ref != f"refs/heads/{LAND_BRANCH}" or local_sha == ZERO:
            continue
        tree = repo.head_tree(local_sha)
        if repo.land_receipt(tree).exists():
            continue
        say(f"autocheck: push: {LAND_BRANCH} at {local_sha[:12]} has no land receipt "
            "(a fast-forward, a cherry-pick or a --no-verify merge); running the land checks now")
        head = git("rev-parse", "HEAD", cwd=repo.top)
        status = git("status", "--porcelain", "--untracked-files=no", env=repo.env, cwd=repo.top)
        if repo.branch != LAND_BRANCH or head != local_sha or status:
            banner("PUSH REFUSED: main's tip was never checked", [
                f"{local_sha[:12]} has no land receipt, and this checkout is not at that commit",
                "with a clean tree, so the checks cannot run here. Push from the main checkout.",
            ])
            return 1
        base = remote_sha if remote_sha != ZERO else git("rev-list", "--max-parents=0", local_sha, cwd=repo.top).split()[0]
        if land_check(repo, "push", [], [f"{base}..{local_sha}"], changed_lint=False):
            return 1
    return 0


# ---------------------------------------------------------------------------------------
# After the fact: what --no-verify skipped is recorded where the lead sees it
# ---------------------------------------------------------------------------------------

def record_skip(repo, title, question, tried=None, meanwhile=None):
    repo.state.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(repo.state / "bypass.log", "a") as log:
        log.write(f"{stamp}\t{repo.top}\t{repo.branch or '(detached)'}\t{title}\n")
    local = repo.top / ENGINE_ESCALATE
    tool = local if local.is_file() else Path.home() / ".claude/richos-engine/scripts/escalate.sh"
    fields = dict(title=title, state="work-complete", question=question,
                  tried=tried or "The automatic commit and land checks (richos/app/scripts/autocheck) did not run for this change.",
                  meanwhile=meanwhile or "The commit exists; the land check will still run if it is merged into main.")
    with tempfile.NamedTemporaryFile("w", prefix="autocheck-skip-", suffix=".json", delete=False) as f:
        json.dump(fields, f)
        path = f.name
    try:
        result = subprocess.run(["bash", str(tool), "raise", "--fields", path, "--worktree", str(repo.top), "--no-record"],
                                cwd=repo.top, env=repo.env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    finally:
        os.unlink(path)
    if result.returncode:
        banner("AUTOCHECK SKIP NOT DELIVERED to the escalation ledger", [
            title, f"{tool} exited {result.returncode}:", *(result.stdout + result.stderr).strip().splitlines()[-6:],
            f"It is in {repo.state / 'bypass.log'}.",
        ])
    else:
        banner("AUTOCHECK SKIPPED, RECORDED", [title, "Raised in the lead's escalation ledger."])


SELF = "richos/app/scripts/autocheck/autocheck.py"


def existed_before(repo, revs):
    """Did the check exist when git would have run it? The commit that introduces it (or a
    branch that has never seen it) skipped nothing and is not recorded."""
    if repo.branch != LAND_BRANCH:
        revs = [*revs, f"refs/heads/{LAND_BRANCH}"]
    return any(git("cat-file", "-e", f"{rev}:{SELF}", check=False, cwd=repo.top).returncode == 0 for rev in revs)


PHONE_WATCH = "richos/mobile/perf/watch.py"


def phone_watch(repo, old, new):
    """Main moved from `old` to `new`: hand it to the phone speed watch (CEO 2026-10-01: the mobile
    apps' start times are checked by the system after every mobile change, never by him). watch.py
    decides everything: it acts only in the operator's main checkout, reports missed runs, and starts
    a detached run on the phones only when old..new touches richos/mobile/. It returns at once; the
    land never waits for a phone. A trigger that cannot run is recorded for the lead."""
    script = repo.top / PHONE_WATCH
    if not old or not script.is_file():
        return
    env = {k: v for k, v in repo.env.items() if not k.startswith("GIT_")}
    env.pop(ACTIVE, None)
    try:
        result = run_bounded([sys.executable, str(script), "trigger", "--repo", str(repo.top), "--from", old,
                                 "--to", new], cwd=repo.top, env=env, stdin=subprocess.DEVNULL, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        result = None
        detail = str(exc)
    else:
        detail = (result.stdout + result.stderr).strip()
        for line in detail.splitlines()[-4:]:
            say(line)
    if result is None or result.returncode:
        record_skip(repo, f"the phone speed watch did not run for main {new[:12]}",
                    f"{PHONE_WATCH} trigger failed ({detail[-300:]}). Was the phone speed check of this land run?",
                    tried=f"autocheck's hook on main ran {PHONE_WATCH} trigger.",
                    meanwhile="The land is complete; its phone speed check did not start.")


def post_commit(repo):
    head = git("rev-parse", "HEAD", cwd=repo.top)
    tree = repo.head_tree()
    action = os.environ.get("GIT_REFLOG_ACTION", "")
    if not existed_before(repo, ["HEAD^1"]):
        return 0
    if repo.branch == LAND_BRANCH:
        parent = git("rev-parse", "-q", "--verify", "HEAD^1", check=False, cwd=repo.top).stdout.strip()
        phone_watch(repo, parent, head)
        if not repo.land_receipt(tree).exists():
            how = action or "git commit --no-verify"
            record_skip(repo, f"main moved to {head[:12]} without the land checks ({how})",
                        f"Was landing {head[:12]} on main without its suites deliberate? The next push of main runs them.")
        return 0
    marker = repo.marker()
    verified = marker.read_text().strip() if marker.exists() else ""
    if marker.exists():
        marker.unlink()
    if verified == tree:
        return 0
    replaying = any(git_path(repo, name).exists() for name in
                    ("CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply"))
    if replaying or (action and not action.startswith("commit")):
        return 0  # a rebase, cherry-pick, revert or am replays commits; the land check covers them
    record_skip(repo, f"commit {head[:12]} on {repo.branch or 'a detached HEAD'} skipped the automatic checks (--no-verify)",
                f"Was skipping the lint on {head[:12]} deliberate? Its land will run the full checks.")
    return 0


def refresh_hook_sidecars(repo, old, new):
    """Main moved from `old` to `new`: if that touched a guard script under richos/engine/scripts/hooks/,
    re-mint the engine's .sha256 sidecars with the engine's own install.sh. The sidecars are gitignored,
    so a merge brings new script bytes and leaves the old hashes, which turns the integrity probe's BR4
    red and makes install-fresh refuse to run (2026-10-05, 112 guards, lands that missed the manual
    step). A merge that touches no hook script runs nothing. A failed refresh is recorded for the lead."""
    install = repo.top / ENGINE_HOOKS_INSTALL
    if not old or not install.is_file():
        return
    touched = git("diff", "--name-only", old, new, "--", ENGINE_HOOKS, check=False, cwd=repo.top).stdout.split()
    if not touched:
        return
    env = {k: v for k, v in repo.env.items() if not k.startswith("GIT_")}
    env.pop(ACTIVE, None)
    try:
        result = run_bounded(["bash", str(install)], cwd=repo.top, env=env, stdin=subprocess.DEVNULL, text=True, timeout=300)
    except (OSError, subprocess.TimeoutExpired) as exc:
        result = None
        detail = str(exc)
    else:
        detail = (result.stdout + result.stderr).strip()
    if result is None or result.returncode:
        record_skip(repo, f"the hook sidecars were not refreshed after main moved to {new[:12]}",
                    f"{ENGINE_HOOKS_INSTALL} failed ({detail[-300:]}). Run it from the main checkout: until then BR4 is red.",
                    tried=f"autocheck's hook on main ran {ENGINE_HOOKS_INSTALL} because {touched[0]} changed.",
                    meanwhile="The land is complete; the guard scripts' sidecars still hash the old bytes.")
        return
    say(f"autocheck: {len(touched)} hook script(s) changed; engine sidecars refreshed ({ENGINE_HOOKS_INSTALL})")


def made_a_merge_commit(repo, parents):
    """Did THIS merge write a new merge commit, or move HEAD to a commit that already existed?

    Two parents on the new HEAD is not the answer. On 2026-09-29 a branch fast-forwarded onto
    main's tip 5ddcce1c, which is itself a land merge with two parents; no pre-merge-commit
    ran (a fast-forward writes no commit, so there is nothing to check), and this hook
    reported that as a `--no-verify` skip that never happened. A merge commit written here
    always has the pre-merge HEAD as its first parent, and git's own reflog says
    `Fast-forward` when it moved HEAD without writing one: both must say "a new commit"."""
    if len(parents) < 2:
        return False
    orig = git("rev-parse", "-q", "--verify", "ORIG_HEAD", check=False, cwd=repo.top).stdout.strip()
    if orig and parents[0] != orig:
        return False
    subject = git("reflog", "-1", "--format=%gs", "HEAD", check=False, cwd=repo.top).stdout.strip()
    return not subject.endswith("Fast-forward")


def post_merge(repo, squash):
    if squash == "1":
        return 0
    head = git("rev-parse", "HEAD", cwd=repo.top)
    tree = repo.head_tree()
    parents = git("rev-list", "--parents", "-n", "1", "HEAD", cwd=repo.top).split()[1:]
    if not existed_before(repo, ["ORIG_HEAD", *parents]):
        return 0
    merged = made_a_merge_commit(repo, parents)
    if repo.branch == LAND_BRANCH:
        orig = git("rev-parse", "-q", "--verify", "ORIG_HEAD", check=False, cwd=repo.top).stdout.strip()
        phone_watch(repo, orig, head)
        refresh_hook_sidecars(repo, orig, head)
        if not repo.land_receipt(tree).exists():
            how = "git merge --no-verify" if merged else "a fast-forward"
            record_skip(repo, f"main moved to {head[:12]} without the land checks ({how})",
                        f"Was landing {head[:12]} on main without its suites deliberate? The next push of main runs them.")
        return 0
    marker = repo.marker()
    verified = marker.read_text().strip() if marker.exists() else ""
    if marker.exists():
        marker.unlink()
    if merged and verified != tree:
        record_skip(repo, f"merge {head[:12]} on {repo.branch or 'a detached HEAD'} skipped the automatic checks (--no-verify)",
                    f"Was skipping the lint on {head[:12]} deliberate? Its land will run the full checks.")
    return 0


def commit_msg(repo, message_file):
    """commit-msg: a commit touching richos/mobile/ must carry a Battery-check trailer that GIT
    parses (CEO ruling §81). 2026-10-01: a trailer wrapped over three unindented lines was no
    trailer to git, and nothing refused it until the merge, because the land was the only place
    the rule ran. The message is read by battery-check.py --message, the same verdict the land
    gives, so the two cannot disagree. A merge, a rebase, a cherry-pick, a revert and an am
    replay commits already written; the land check covers them."""
    if any(git_path(repo, name).exists() for name in
           ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply")):
        return 0
    # git exports no amend marker to commit-msg (measured 2026-10-01: GIT_REFLOG_ACTION is not
    # set for the hook), so the `git commit` that called us is asked for its own arguments.
    amend = os.environ.get("GIT_REFLOG_ACTION", "").startswith("commit (amend)") or any(
        "--amend" in subprocess.run(["ps", "-o", "args=", "-p", str(pid)], capture_output=True, text=True).stdout.split()
        for pid in ancestors())
    against = ["HEAD^"] if amend and git("rev-parse", "-q", "--verify", "HEAD^", check=False,
                                          cwd=repo.top).returncode == 0 else []
    paths = git("diff", "--cached", "--name-only", "--no-renames", *against, cwd=repo.top).splitlines()
    if not any(p.startswith(MOBILE) for p in paths):
        return 0
    script = repo.top / BATTERY_CHECK
    if not script.exists() or "--message" not in script.read_text():
        say(f"autocheck: commit-msg: this tree's {BATTERY_CHECK} cannot read a message; the land check will")
        return 0
    result = subprocess.run([sys.executable, str(script), "--message", message_file], cwd=repo.top,
                            env=repo.env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    if result.returncode == 0:
        return 0
    banner("COMMIT REFUSED: the Battery-check trailer is missing or does not parse (CEO ruling §81)",
           (result.stderr or result.stdout).rstrip().splitlines())
    return 1


# ---------------------------------------------------------------------------------------

def main(argv):
    hook = argv[0] if argv else ""
    if hook == "measure":
        # `autocheck.py measure`, run by hand in a checkout: the land check of what is staged
        # there against HEAD, exactly as a commit onto main runs it, with no land receipt
        # written. How a merge is replayed through the gate to measure it (README.md).
        try:
            return land_from_index(Repo(), "measure", receipt=False)
        except (RuntimeError, OSError) as exc:
            banner("MEASURE REFUSED: the check could not run", [str(exc)])
            return 1
    if hook not in ("pre-commit", "pre-merge-commit", "commit-msg", "post-commit", "post-merge", "pre-push"):
        return 0
    stdin_text = sys.stdin.read() if hook == "pre-push" else ""
    repo = Repo()
    if inside_own_check(str(repo.common)):
        say(f"autocheck: {hook}: inside an automatic check of this repository (a suite's own commit); not re-entered")
        return 0
    if hook in ("pre-commit", "pre-merge-commit", "commit-msg"):
        # A terminated check unwinds like an interrupted one, so StagedOnly puts set-aside work
        # back (a KeyboardInterrupt already does). Only SIGKILL leaves it in ASIDE, and the next
        # commit check then refuses and says where it is.
        for number in (signal.SIGTERM, signal.SIGHUP):
            signal.signal(number, lambda signum, _frame: sys.exit(128 + signum))
    try:
        if hook == "commit-msg":
            return commit_msg(repo, argv[1]) if len(argv) > 1 else 0
        if hook == "pre-commit":
            if repo.branch == LAND_BRANCH:
                return land_from_index(repo, "commit to main")
            rc = commit_check(repo, "commit")
        elif hook == "pre-merge-commit":
            if repo.branch == LAND_BRANCH:
                return land_from_index(repo, "merge into main")
            rc = commit_check(repo, "merge")
        elif hook == "post-commit":
            return post_commit(repo)
        elif hook == "post-merge":
            return post_merge(repo, argv[1] if len(argv) > 1 else "0")
        else:
            return pre_push(repo, stdin_text)
        if rc == 0:
            repo.marker().write_text(repo.index_tree() + "\n")
        return rc
    except (RuntimeError, OSError) as exc:
        if hook.startswith("post-"):
            say(f"autocheck: {hook} could not complete its record: {exc}")
            return 0
        banner(f"{hook.upper()} REFUSED: the automatic check could not run", [str(exc)])
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

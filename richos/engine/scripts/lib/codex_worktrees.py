"""Codex's own finished worktrees: which may be closed, and closing them.

Cleanup PRD 2026-10-23, section 3 and A6. The workspace engine never touches a
codex/ workspace (its point 2), so Codex's finished worktrees had no collector:
measured 2026-10-23, 45 codex/ worktrees registered in the RichOS repository and
59 parents under the Codex app's worktree root, about 67 GB. CEO decision §47
authorizes deleting a plain codex/ worktree whose commits are all in main.

WHERE CANDIDATES COME FROM: only the declared Codex worktree roots, a checkout one
or two levels below each (<root>/<checkout> or <root>/<task>/<checkout>). Never a
repository's whole worktree list: a sweep that started from the repository would
reach every agent's and person's worktree.

WHAT CLOSES (all of it, rechecked at deletion time):
  * git registers the checkout as a linked worktree of its repository, it is not
    that repository's main checkout and it is not locked;
  * its branch is codex/<name> (or it is a detached checkout of the app's root),
    and its tip is an ancestor of the repository's main;
  * it has no uncommitted or untracked entry and no ignored entry the main
    checkout does not have, other than what the repository declares regenerable
    (`#regenerable:` in its root .gitignore), by the workspace engine's own
    `uncommitted()` and pattern rule;
  * no process has its working directory inside it, and the open-file table
    shows nothing inside it in use: no open file, mapped library or running
    executable (a reader whose working directory is elsewhere still keeps it);
  * for a checkout under the app's root: the app's own attachment records
    (read-only, from a consistent snapshot) show no LIVE attachment for it. A
    live attachment is closed only by its owning chat's archive_worktree, which
    needs the CEO's standing authorization (PRD section 3); until then it is an
    unresolved entry, never deleted here.
Closing is git's own `worktree remove` without --force (git refuses a checkout
that changed since) and `branch -d` (git refuses an unmerged branch). Anything
that cannot be read is undecided, never "nobody owns it".
"""
import os
import re
import shutil
import sqlite3
import subprocess
import tempfile

KEEP, DELETE, UNDECIDED = "KEEP", "DELETE", "INDETERMINATE"
CODEX_PREFIX = "codex/"
MARKER = ".codex-worktree-name"


def _git(repo, *args, timeout=120):
    try:
        r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True,
                           timeout=timeout, env=dict(os.environ, LC_ALL="C"))
        return r.returncode, r.stdout, r.stderr
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 124, "", str(exc)


def checkouts(roots):
    """([checkout paths], [parent dirs holding nothing or only the app's marker])."""
    found, husks = [], []
    for root in roots:
        try:
            names = sorted(os.listdir(root))
        except OSError:
            continue
        for name in names:
            top = os.path.join(root, name)
            if os.path.islink(top) or not os.path.isdir(top):
                continue
            if os.path.isfile(os.path.join(top, ".git")):
                found.append(top)
                continue
            try:
                inner = sorted(os.listdir(top))
            except OSError:
                continue
            if set(inner) <= {MARKER}:
                husks.append(top)
                continue
            for sub in inner:
                p = os.path.join(top, sub)
                if not os.path.islink(p) and os.path.isfile(os.path.join(p, ".git")):
                    found.append(p)
    return found, husks


def repository_of(checkout):
    """The main checkout of the repository a linked checkout belongs to, or ''."""
    try:
        with open(os.path.join(checkout, ".git"), encoding="utf-8", errors="replace") as fh:
            text = fh.read(4096).strip()
    except OSError:
        return ""
    if not text.startswith("gitdir:"):
        return ""
    gitdir = os.path.realpath(os.path.join(checkout, text[len("gitdir:"):].strip()))
    parent = os.path.dirname(gitdir)
    if os.path.basename(parent) != "worktrees":
        return ""
    common = os.path.dirname(parent)
    return os.path.dirname(common) if os.path.basename(common) == ".git" else ""


def attachments(codex_home):
    """{'live': {realpath}, 'archived': {realpath}} from the app's own records, or
    (None, why) when they cannot be read or are not in the shape this reads.

    Read-only: the database is opened with mode=ro and copied with SQLite's own
    backup API into a private snapshot, so the app's records are never written
    and a write in progress is never half-read. The schema is recognized by the
    fields this needs, never by a file name or version alone."""
    db = os.path.join(codex_home, "state_5.sqlite")
    if not os.path.exists(db):
        return {"live": set(), "archived": set()}, ""
    snap = tempfile.mkdtemp(prefix="codex-attachments.")
    try:
        try:
            src = sqlite3.connect("file:%s?mode=ro" % db, uri=True, timeout=10)
            dst = sqlite3.connect(os.path.join(snap, "snapshot.sqlite"))
            src.backup(dst)
            src.close()
            cols = {r[1] for r in dst.execute("pragma table_info(thread_attachments)")}
            if not {"thread_id", "attachment_type", "identity_key", "payload"} <= cols:
                return None, "%s has no thread_attachments table in the shape this reads" % db
            rows = dst.execute("select attachment_type, identity_key from thread_attachments "
                               "where attachment_type in ('worktree', 'archived_worktree')").fetchall()
            dst.close()
        except sqlite3.Error as exc:
            return None, "%s could not be read: %s" % (db, exc)
        out = {"live": set(), "archived": set()}
        for kind, key in rows:
            path = os.path.realpath(os.path.expanduser(key or ""))
            out["live" if kind == "worktree" else "archived"].add(path)
        return out, ""
    finally:
        shutil.rmtree(snap, ignore_errors=True)


def open_paths(env=None, timeout=120):
    """Every path the whole machine's open-file table shows in use, or None.

    `lsof -F n` names each open file, each mapped library, each process's running
    executable (its txt entry) and each working directory, so one read answers
    "is anything in this checkout in use" whatever the process's own cwd is. Exit 1
    is lsof's normal answer when some processes cannot be listed; what it could
    list still answers. A table naming nothing is not a table (lsof always lists
    itself), so it is None, never "nothing is open". `env` is the caller's PATH
    for lsof, which lives in /usr/sbin, outside launchd's default PATH."""
    try:
        r = subprocess.run(["lsof", "-n", "-P", "-F", "n"], capture_output=True, text=True,
                           timeout=timeout, env=env)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if r.returncode not in (0, 1):
        return None
    paths = {line[1:] for line in r.stdout.splitlines() if line.startswith("n/")}
    return paths or None


def regenerable(repo, ws):
    """The repository's own `#regenerable:` patterns, read from main's .gitignore."""
    rc, out, _err = _git(repo, "show", "main:.gitignore")
    if rc != 0:
        return []
    return [m.group(1) for m in (ws._REGENERABLE_RE.match(l.strip()) for l in out.splitlines()) if m]


def decide(path, ws, app_root, attached, cwds, opened=None):
    """(action, why, info) for one checkout. info carries what closing needs.

    `opened` is open_paths() (or the caller's own read of the same table); None,
    including not passed, means it could not be read, and the checkout stays
    undecided."""
    real = os.path.realpath(path)
    repo = repository_of(real)
    if not repo:
        return UNDECIDED, "its .git does not name a repository's linked worktree", {}
    wl = ws.worktree_list(repo)
    if wl is None:
        return UNDECIDED, "git cannot list the worktrees of %s" % repo, {}
    entry = next((e for e in wl if e["path"] == real), None)
    if entry is None:
        return UNDECIDED, "%s does not register it as a worktree" % repo, {}
    if entry is wl[0]:
        return KEEP, "it is the main checkout of %s" % repo, {}
    if entry["locked"]:
        return KEEP, "git has it locked", {}
    app = os.path.realpath(app_root) if app_root else ""
    in_app = bool(app) and (real + "/").startswith(app + "/")
    branch = entry["branch"]
    if not branch.startswith(CODEX_PREFIX) and not (in_app and entry["detached"]):
        return KEEP, "its branch %r is not a codex/ branch" % (branch or "(detached)"), {}
    tip = entry["head"]
    main_tip, unread = ws.branch_tip_read(repo, "main")
    if unread or not main_tip:
        return UNDECIDED, "main of %s cannot be read%s" % (repo, ": " + unread if unread else ""), {}
    if not tip or not ws.is_ancestor(repo, tip, main_tip):
        return KEEP, "its commits are not all in main (an unmerged handoff stays until it is " \
                     "integrated or its owner discards it)", {}
    try:
        dirty, ignored = ws.uncommitted(real)
    except Exception as exc:  # noqa: BLE001  unreadable is undecided, never clean
        return UNDECIDED, "its status cannot be read: %s" % str(exc)[:160], {}
    if dirty:
        return KEEP, "it has %d uncommitted or untracked entr%s (%s)" % (
            len(dirty), "y" if len(dirty) == 1 else "ies", ", ".join(dirty[:3])), {}
    pats = regenerable(repo, ws)
    needed = [rel for rel in ignored if not ws._is_regenerable(rel, pats)]
    if needed:
        return KEEP, "it has ignored material the main checkout does not have and the " \
                     "repository does not declare regenerable (%s)" % ", ".join(needed[:3]), {}
    if cwds is None:
        return UNDECIDED, "the process table could not be read, so work running in it " \
                          "cannot be ruled out", {}
    busy = sorted(pid for pid, cwd in cwds.items()
                  if cwd and (os.path.realpath(cwd) + "/").startswith(real + "/"))
    if busy:
        return KEEP, "process %s works in it" % ", ".join(str(p) for p in busy[:5]), {}
    if opened is None:
        return UNDECIDED, "the open-file table could not be read, so a process using a file " \
                          "or running a program in it cannot be ruled out", {}
    held = sorted(p for p in opened if p == real or p.startswith(real + "/"))
    if held:
        return KEEP, "a process has %d path(s) in it in use (%s)" % (
            len(held), ", ".join(os.path.relpath(p, real) for p in held[:3])), {}
    if in_app:
        if attached is None:
            return UNDECIDED, "the Codex app's attachment records cannot be read, so whether " \
                              "a chat owns it is unknown", {}
        if real in attached["live"]:
            return UNDECIDED, "finished, but a Codex chat still has it attached: only that " \
                              "chat's archive_worktree may close it, and dispatching that " \
                              "needs the CEO's standing authorization (cleanup PRD section 3)", {}
    return DELETE, "%s worktree on %s: all its commits are in main, nothing uncommitted or " \
                   "unpreserved, nothing in it in use%s" % (
                       "an app-managed" if in_app else "a plain Codex",
                       branch or "a detached HEAD",
                       ", and no live app attachment" if in_app else ""), \
        {"repo": repo, "branch": branch if branch.startswith(CODEX_PREFIX) else ""}


def close(path, info, ws, app_root, attached_fn, cwds_fn, opened_fn=None):
    """('', '') closed; (why_kept, '') a recheck now keeps it; ('', error) failed.

    Every check is read fresh here, the open-file table included; without an
    `opened_fn` the table counts as unread and nothing closes."""
    action, why, now_info = decide(path, ws, app_root, attached_fn(), cwds_fn(),
                                   opened_fn() if opened_fn else None)
    if action != DELETE:
        return "at deletion time: " + why, ""
    repo = now_info["repo"]
    rc, _out, err = _git(repo, "worktree", "remove", os.path.realpath(path))
    if rc != 0:
        return "", "git worktree remove refused: %s" % err.strip()[:200]
    if now_info.get("branch"):
        rc, _out, err = _git(repo, "branch", "-d", now_info["branch"])
        if rc != 0:
            return "", "the checkout is closed, but git branch -d %s refused: %s" % (
                now_info["branch"], err.strip()[:200])
    parent = os.path.dirname(os.path.realpath(path))
    if app_root and os.path.dirname(parent) == os.path.realpath(app_root):
        try:
            if set(os.listdir(parent)) <= {MARKER}:
                shutil.rmtree(parent)
        except OSError:
            pass
    return "", ""

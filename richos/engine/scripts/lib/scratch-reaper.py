#!/usr/bin/env python3
"""scratch-reaper.py — the scan, the verdict and the deletion. No schedule here.

===========================================================================
WHAT THIS IS FOR
===========================================================================
On 2026-09-17 macOS said "Your disk is almost full" — 1.8 GB free of 460 GB.
19 GB of it was scratch that nothing was using, and it was reclaimed because a
person read the warning and deleted it by hand. That is the failure: not the
garbage, THE PERSON. The CEO's words the same morning: "There needs to be a
mechanism to make sure that scratch directories get deleted on a regular
basis. I don't want piles of garbage to pile up endlessly."

===========================================================================
THE ONE RULE THAT DECIDES EVERYTHING HERE
===========================================================================
A scratch directory is deleted because SOMETHING PROVED NOBODY OWNS IT, never
because it looks old. Age is a floor applied AFTER the proof, and it exists
only to cover the seconds between a session starting and recording itself.

The proof has three possible answers and THREE is not folded into two:

  RUNNING        a session file names this session and its process, and that
                 process is still the same process. Kept, always.
  ENDED          no running `claude` process on this machine is this session,
                 established by EXHAUSTION: every running claude process is
                 named by a session file, and none of them names this one.
  INDETERMINATE  a claude process is running that NO session file names, and
                 it started early enough that it could be the owner. Kept,
                 counted, and named in the verdict line.

INDETERMINATE is never collapsed into ENDED. The whole reason this engine
carries an agent-liveness module at all is the 2026-08-31 incident in which a
confident wrong answer beat an honest "I cannot tell" — absence prompts a
check, a false positive does not.

The liveness primitive is NOT reimplemented here. process_start() comes from
mega-lander/workspaces.py, which is where "ended is read from the operating
system, never guessed" already lives (spec point 12). If that import fails
this module FAILS LOUD rather than falling back to a weaker test: a reaper
that silently downgrades its own liveness check is the one shape of this
program nobody could ever notice was broken.

===========================================================================
WHAT IT WILL DELETE, AND THE FOUR WALLS IT GOES THROUGH FIRST
===========================================================================
Classes (each named in the log line, so every deletion says why):

  claude-session   <root>/<slug>/<session-uuid> whose session is ENDED
  claude-husk      a <root>/<slug> directory left empty by the above
  claude-orphan    a file or non-session directory sitting directly in a
                   scratch root whose mtime PREDATES THE START OF EVERY
                   RUNNING claude PROCESS — so no running session can have
                   written it, and none can write it again
  tmp-workspace    a $TMPDIR directory matching a declared pattern that no
                   process holds open
  nightly-release  / nightly-log  beyond the declared retention
  land-kept        a <workspaces state>/kept/<key>-<time> directory an
                   automatic land moved ignored files into, older than
                   SCRATCH_KEPT_RETENTION_DAYS

Walls, applied to every candidate, in order, after the verdict and before the
unlink. Any wall that trips makes the entry INDETERMINATE — never a silent
skip, because a candidate that cannot be deleted for a structural reason is
exactly the thing somebody needs to read:

  1. CONTAINMENT   the real path must sit strictly inside a declared root,
                   and at least one component below it. A symlink is never
                   followed; the link itself is what would be removed.
  2. NO REPOSITORY the tree must contain no .git anywhere. A scratch
                   directory holding a checkout is somebody's work.
  3. NOT REGISTERED no registered workspace path may be the candidate, sit
                   inside it, or contain it. Read from the workspace registry
                   and the worktree ledger — the same two files the worktree
                   reaper reads.
  4. NEVER-TOUCH   $HOME, ~/.claude, ~/RichOS, ~/.richos-signing, the nightly
                   source worktree and the nightly runtime, by name.

===========================================================================
WHAT IT WILL NOT DELETE, MEASURED RATHER THAN PROMISED (2026-09-17)
===========================================================================
  * The live session's own scratchpad — 4,270 MB of the 5.2 GB under
    /private/tmp/claude-501 on the morning this was written. The reaper that
    would have prevented that morning's disk-full warning does not exist,
    because the biggest pile belonged to a session that was RUNNING.
  * ~/.richos-nightly/source — 5.89 GB, and A REGISTERED GIT WORKTREE. Walls
    2 and 3 both refuse it and the config never nominates it.
  * The 244 loose *.out files in the scratch root that morning: every one had
    an mtime AFTER the live session's process started, so the claude-orphan
    rule kept all of them. A rule that deletes nothing on the day it ships is
    a rule whose safety is measurable.
"""

import argparse
import calendar
import fnmatch
import glob
import importlib.util
import json
import os
import re
import stat
import subprocess
import sys
import time
import machine_settings

RUNNING = "RUNNING"
ENDED = "ENDED"
INDETERMINATE = "INDETERMINATE"

DELETE = "DELETE"
KEEP = "KEEP"

# A sentinel distinct from None, so "not read yet" and "read and unusable" are
# never the same value.
_FAILED = object()

UUID_RE = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-"
                     r"[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")


class Fatal(Exception):
    """Something this program refuses to work around."""


class Deadline(Exception):
    """The scan ran out of the budget its caller gave it.

    Only --notice sets one. A SessionStart notice that adds seconds to every
    session start is a notice somebody removes, and the honest failure is to
    say the scan could not finish rather than to hold the session open."""


_DEADLINE = [None]


def check_deadline():
    if _DEADLINE[0] is not None and time.time() > _DEADLINE[0]:
        raise Deadline("the scan exceeded its budget")


# ---------------------------------------------------------------------------
# the liveness primitive, borrowed rather than rewritten
# ---------------------------------------------------------------------------

def load_workspaces():
    """mega-lander/workspaces.py, or Fatal. ONE PREDICATE, MANY CALLERS."""
    lib = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                       "..", "..", "mega-lander", "workspaces.py")
    lib = os.path.normpath(lib)
    if not os.path.exists(lib):
        raise Fatal("the liveness primitive is missing: %s. Nothing here may "
                    "guess at whether a session is alive." % lib)
    try:
        spec = importlib.util.spec_from_file_location("sr_workspaces", lib)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    except Exception as exc:                                  # pragma: no cover
        raise Fatal("the liveness primitive could not be loaded from %s: %s"
                    % (lib, exc))
    for fn in ("process_start", "session_state"):
        if not hasattr(mod, fn):
            raise Fatal("%s carries no %s(); the liveness contract this reaper "
                        "depends on has moved." % (lib, fn))
    return mod


def sessions_dir():
    """The platform's session registry — the same resolution workspaces.py
    uses, spelled out here because this module may run with no session at
    all (launchd), where CLAUDE_CONFIG_DIR is not exported."""
    d = (os.environ.get("RICHOS_SESSIONS_DIR") or "").strip()
    if d:
        return d
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "sessions")


# The system tool directories, appended to PATH when missing. lsof lives in
# /usr/sbin on macOS, and the scheduled job's PATH (launchd's, set by the plist
# install writes) does not carry /usr/sbin. Measured 2026-10-23 under a launchd
# job with that PATH and ProcessType Background: `lsof` raised FileNotFoundError,
# while /usr/sbin/lsof answered with exit 0 in 0.51 s. Every scheduled run since
# reported "THE OPEN-FILE TABLE COULD NOT BE READ" and decided nothing under
# $TMPDIR or /private/tmp. Appended, never prepended, so a caller's own PATH
# still wins.
_SYSTEM_TOOL_DIRS = ("/usr/sbin", "/sbin")


def _ps_env():
    env = dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")
    path = [d for d in (env.get("PATH") or "").split(os.pathsep) if d]
    path += [d for d in _SYSTEM_TOOL_DIRS if d not in path]
    env["PATH"] = os.pathsep.join(path)
    return env


def session_process_names():
    v = (os.environ.get("SCRATCH_SESSION_PROCESS_NAMES") or "").strip()
    if not v:
        raise Fatal(
            "SCRATCH_SESSION_PROCESS_NAMES is not declared. The reaper decides "
            "that a session is gone by accounting for every session process on "
            "the machine; without the list of names that ARE sessions it would "
            "be accounting for nothing and calling it proof.")
    return set(v.split())


def claude_processes():
    """[(pid, lstart)] for every session process of THIS user.

    TZ=UTC0 is not cosmetic: the session files record `ps -o lstart=` under
    that environment, and a start time read in local time never compares equal
    to the one on disk. Measured 2026-09-17: the same process reads
    "Thu 17 Sep 00:31:27 2026" locally and "Wed Sep 16 23:31:27 2026" under
    TZ=UTC0, which is what the registration holds."""
    try:
        r = subprocess.run(["ps", "-Ao", "pid=,uid=,lstart=,comm="],
                           capture_output=True, text=True, timeout=20,
                           env=_ps_env())
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise Fatal("the process table could not be read (%s), so nothing can "
                    "be shown to be dead." % exc)
    if r.returncode != 0:
        raise Fatal("ps exited %d reading the process table: %s"
                    % (r.returncode, r.stderr.strip()[:200]))
    uid = str(os.getuid())
    names = session_process_names()
    out = []
    for line in r.stdout.splitlines():
        parts = line.split()
        # pid uid <lstart: 5 fields> comm...
        if len(parts) < 8 or parts[1] != uid:
            continue
        comm = " ".join(parts[7:])
        if os.path.basename(comm) not in names:
            continue
        try:
            pid = int(parts[0])
        except ValueError:
            continue
        out.append((pid, " ".join(parts[2:7])))
    return out


def lstart_epoch(text):
    """`ps -o lstart=` under TZ=UTC0 -> epoch seconds, or None.

    calendar.timegm, NOT time.mktime MINUS AN OFFSET, AND THE DIFFERENCE IS AN
    HOUR OF WRONG ANSWERS FOR HALF THE YEAR.

    The old form was `mktime(strptime(text)) - time.timezone`. mktime interprets
    the struct as LOCAL time, and `time.timezone` is the zone's STANDARD offset —
    it does not move for daylight saving. So in any zone observing DST, in the
    months it is observed, this returned a start time one hour EARLY. Measured on
    this machine, 2026-09-18, on a process started at that instant:

        timezone 0   altzone -3600   daylight 1   tm_isdst 1
        ps lstart (TZ=UTC0): Fri Sep 18 10:13:15 2026
        shipped lstart_epoch -> 1789722795
        correct  (timegm)    -> 1789726395   (== time.time() to the second)
        error                -> -3600

    WHY IT SURVIVED: it fails in the SAFE direction. Every caller asks "is this
    older than the earliest running session", and a start time an hour early
    makes the answer no more often, so the reaper KEPT things it could have
    deleted rather than deleting things it should have kept. An hour of lost
    coverage every run, reported by nothing, in the one primitive the claude-
    orphan rule and the deny-by-default temp arm both rest on.

    It was found by a test that asserted WHICH REASON a KEEP carried, not merely
    that the tree survived — S21o. A case that had only checked the directory was
    still there would have passed against the wrong answer for ever.

    `ps` is run under TZ=UTC0 by _ps_env(), so the string is UTC and timegm is
    the only conversion that is right in every zone and every season.
    """
    try:
        return calendar.timegm(time.strptime(text, "%a %b %d %H:%M:%S %Y"))
    except (ValueError, OverflowError):
        return None


class Liveness(object):
    """Who is running, and what that lets us say about who is not."""

    def __init__(self, ws):
        self.ws = ws
        self.registered = {}     # pid -> sessionId, from the session files
        self.running = {}        # sessionId -> (pid, why)
        self.unattributed = []   # [(pid, lstart, epoch)]
        self.notes = []
        self._processes = claude_processes()
        self._oldest = False     # False = not read yet; None = unreadable
        self._read()

    def _read(self):
        sd = sessions_dir()
        for path in sorted(glob.glob(os.path.join(sd, "*.json"))):
            try:
                with open(path, encoding="utf-8") as fh:
                    rec = json.load(fh)
            except (OSError, ValueError):
                continue
            pid, sid = rec.get("pid"), str(rec.get("sessionId") or "")
            if not pid or not sid:
                continue
            state, text = self.ws.process_start(pid)
            if state == "ok" and text == str(rec.get("procStart") or ""):
                self.registered[int(pid)] = sid
                self.running[sid] = (int(pid), "process %d of session %s runs"
                                     % (int(pid), sid[:8]))
            elif state == "ok":
                # The pid was reused. The session is gone; the file is stale. The
                # process now holding that pid is NOT attributed by this file, so it
                # stays in the unattributed set and protects what it could own.
                self.notes.append(
                    "session file %s names pid %s, which is now a different "
                    "process — the registration is stale, not live"
                    % (os.path.basename(path), pid))

        for pid, lstart in self._processes:
            if pid in self.registered:
                continue
            self.unattributed.append((pid, lstart, lstart_epoch(lstart)))

        if self.unattributed:
            self.notes.append(
                "%d running claude process(es) are named by NO session file "
                "(%s) — anything they could own is INDETERMINATE"
                % (len(self.unattributed),
                   ", ".join(str(p) for p, _, _ in self.unattributed)))

    def oldest_unattributed_start(self):
        eps = [e for _, _, e in self.unattributed if e is not None]
        if len(eps) < len(self.unattributed):
            return 0          # a start time we could not parse: assume ancient
        return min(eps) if eps else None

    def oldest_start(self):
        """The start of the EARLIEST running claude process, attributed or
        not. Nothing written before this can have been written by anything
        that is still running. Read ONCE: the process table is a snapshot, and
        two readings of it inside one pass could disagree with each other."""
        if self._oldest is not False:
            return self._oldest
        eps = []
        for pid, lstart in self._processes:
            e = lstart_epoch(lstart)
            if e is None:
                self._oldest = None
                return None
            eps.append(e)
        self._oldest = min(eps) if eps else None
        return self._oldest

    def ended_epoch(self, session_id):
        """When this session RECORDED ITS END, as epoch seconds, or None.

        THE AGE FLOOR EXISTS TO COVER A RACE THIS SESSION IS NOT IN. Its own
        declaration says so: "Age is a floor applied AFTER the proof, and it
        exists only to cover the seconds between a session starting and
        recording itself." A session that has written `ended_at` into its
        record is not starting; it has finished, and it said when.

        WHY IT MATTERS, MEASURED 2026-09-19. 11 GB of a finished session's
        scratchpad sat on this disk and was reported KEEP by every six-hourly
        pass with the reason:

          session 5645c662 recorded its end at 2026-09-18T15:38:15Z, and no
          running claude process is this session, but it was written 0 min ago
          and the floor is 60 min

        — twenty-two hours after it ended. The age comes from the newest mtime
        in the tree, and an orphaned background child of the dead session (or
        anything else holding the path) keeps touching it, so the tree is for
        ever "0 min old" and the floor never expires. Rich deleted it by hand;
        free space went 198 -> 214 GB. That is the exact failure §54 was
        written about: reclaiming it took a person.

        IT IS THE SAME DEFECT AS scan_standing_failures', in the same file, for
        the same reason — a clock read off an mtime that something else moves.
        There the answer was to reconsider regardless of age; here it is to take
        the age from the fact that is actually about the session.
        """
        # hasattr, not a hard requirement in load_workspaces(): process_start
        # and session_state are the LIVENESS contract and their absence is
        # Fatal. This one is an optimization of the floor, and an engine whose
        # workspaces module predates it must lose the improvement rather than
        # refuse to sweep at all — the same asymmetry config_from_env's opt()
        # is built on.
        if not hasattr(self.ws, "load_session"):
            return None
        try:
            rec = self.ws.load_session(session_id)
        except Exception:                                     # pragma: no cover
            return None
        when = (rec or {}).get("ended_at")
        if not isinstance(when, str) or not when:
            return None
        try:
            return calendar.timegm(time.strptime(when, "%Y-%m-%dT%H:%M:%SZ"))
        except (ValueError, OverflowError):
            return None

    def verdict(self, session_id, newest_mtime):
        """(RUNNING|ENDED|INDETERMINATE, why) for one session's scratch."""
        if session_id in self.running:
            return RUNNING, self.running[session_id][1]
        state, why = self.ws.session_state(session_id)
        if state == "running":
            return RUNNING, "the workspace registry says " + why
        cutoff = self.oldest_unattributed_start()
        if cutoff is not None and newest_mtime >= cutoff:
            return (INDETERMINATE,
                    "a running claude process is named by no session file and "
                    "started before this scratch was last written, so it could "
                    "own it")
        if state == "ended":
            return ENDED, why + ", and no running claude process is this session"
        return (ENDED,
                "no running claude process is session %s: every claude process "
                "on this machine is named by a session file and none names it"
                % session_id[:8])


# ---------------------------------------------------------------------------
# the walls
# ---------------------------------------------------------------------------

def never_touch():
    home = os.path.expanduser("~")
    out = [home,
           os.path.join(home, ".claude"),
           os.path.join(home, "RichOS"),
           os.path.join(home, ".richos-signing"),
           os.path.join(home, ".richos-nightly"),
           os.path.join(home, ".richos-nightly", "source"),
           os.path.join(home, ".richos-nightly", "runtime"),
           "/", "/tmp", "/private/tmp", "/var", "/private/var"]
    return set(os.path.realpath(p) for p in out)


def registered_workspaces():
    """Every path the workspace registry or the worktree ledger calls a
    workspace. The reaper reads the SAME two files the worktree reaper does,
    so the two can never disagree about what is registered."""
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    paths = set()

    def harvest(obj):
        if isinstance(obj, dict):
            for key, val in obj.items():
                if key in ("worktree", "workspaces", "path", "dir") and val:
                    if isinstance(val, str):
                        paths.add(val)
                    elif isinstance(val, list):
                        # The registry stores `workspaces: [{"path": ...}]`; a string
                        # item is a path, an object or list is read for its own paths.
                        for v in val:
                            if isinstance(v, str):
                                paths.add(v)
                            elif isinstance(v, (dict, list)):
                                harvest(v)
                    elif isinstance(val, dict):
                        harvest(val)
                elif isinstance(val, (dict, list)):
                    harvest(val)
        elif isinstance(obj, list):
            for item in obj:
                harvest(item)

    for p in glob.glob(os.path.join(base, "state", "workspaces", "agents", "*.json")):
        try:
            with open(p, encoding="utf-8") as fh:
                harvest(json.load(fh))
        except (OSError, ValueError):
            continue
    ledger = os.path.join(base, "state", "worktree-ledger.jsonl")
    try:
        with open(ledger, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    harvest(json.loads(line))
                except ValueError:
                    continue
    except OSError:
        pass
    return set(os.path.realpath(p) for p in paths if os.path.isabs(p))


def inside(child, parent):
    child, parent = child.rstrip("/"), parent.rstrip("/")
    return child == parent or child.startswith(parent + "/")


def _gitdir_target(pointer_file):
    """The realpath a `gitdir:` pointer file names, or '' if it names none."""
    try:
        with open(pointer_file, encoding="utf-8", errors="replace") as fh:
            text = fh.read(4096).strip()
    except OSError:
        return ""
    if not text.startswith("gitdir:"):
        return ""
    target = text[len("gitdir:"):].strip()
    if not os.path.isabs(target):
        target = os.path.join(os.path.dirname(pointer_file), target)
    return os.path.realpath(target)


def repositories_reach_outside(tree):
    """'' when every .git under `tree` is a repository wholly inside it, else why not.

    A test suite's fixture is a repository it made inside its own allocation. A
    `.git` FILE pointing elsewhere is a checkout of somebody's repository, and a
    repository whose linked worktrees live outside `tree` would leave those
    checkouts without an object store if it were deleted. Either is refused. A
    tree that cannot be read is refused too: unreadable is not proof. A pointer to
    a path that does not exist on this machine (an extracted copy of another
    machine's checkout) belongs to no repository here and is not refused.

    Measured 2026-10-23 in Codex's temp root: two directories held linked
    checkouts of the main RichOS repository, which wall 3 cannot see because git's
    own worktree list is not the workspace registry.
    """
    real = os.path.realpath(tree)

    def elsewhere(target):
        return not target or (not inside(target, real) and os.path.lexists(target))

    stack = [real]
    while stack:
        cur = stack.pop()
        try:
            entries = list(os.scandir(cur))
        except OSError as exc:
            return "%s could not be read (%s)" % (cur, exc)
        for e in entries:
            if e.name == ".git":
                if e.is_file(follow_symlinks=False):
                    target = _gitdir_target(e.path)
                    if elsewhere(target):
                        return ("%s is a linked checkout of a repository outside this "
                                "tree (%s)" % (cur, target or "unreadable pointer"))
                elif e.is_dir(follow_symlinks=False):
                    for pointer in glob.glob(os.path.join(e.path, "worktrees", "*", "gitdir")):
                        target = _gitdir_target_line(pointer)
                        if elsewhere(target):
                            return ("%s has a linked worktree outside this tree (%s)"
                                    % (e.path, target or "unreadable"))
                else:
                    return "%s is a .git that is neither a file nor a directory" % e.path
                continue
            if e.is_dir(follow_symlinks=False):
                stack.append(e.path)
    return ""


def _gitdir_target_line(pointer):
    """A repository's worktrees/<name>/gitdir holds one absolute path, no prefix."""
    try:
        with open(pointer, encoding="utf-8", errors="replace") as fh:
            target = fh.read(4096).strip()
    except OSError:
        return ""
    return os.path.realpath(target) if target else ""


# ---------------------------------------------------------------------------
# the allocator's ledger — added 2026-09-18
# ---------------------------------------------------------------------------
# scripts/lib/scratch.sh writes one JSON object per line as it allocates and
# releases. This reads it into {path: row} for the LAST event about each path.
#
# A MISSING OR CORRUPT LEDGER IS NOT AN ERROR HERE, and that is the design
# rather than a tolerance. The scratch root is swept deny-by-default: a child
# with no ledger row is a candidate, not an exemption. So losing the ledger
# makes the sweep MORE willing to delete, never less — which is the right
# direction for a file whose whole purpose is reclaiming space, and it means
# an attacker (or a bug) cannot protect garbage by damaging the ledger.
# Attribution gets worse without it; safety does not, because the pid in the
# directory NAME is the second record and the age floor is the third.

def ledger_path():
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.environ.get("SCRATCH_LEDGER") \
        or os.path.join(base, "state", "scratch-ledger.jsonl")


def read_ledger(path):
    """{realpath: {label, pid, created_epoch, ttl_minutes, released}}."""
    rows = {}
    try:
        fh = open(path, encoding="utf-8", errors="replace")
    except OSError:
        return rows
    with fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except ValueError:
                continue            # one bad line never costs the other rows
            p = obj.get("path")
            if not isinstance(p, str) or not p:
                continue
            cur = rows.setdefault(p, {})
            if obj.get("event") == "release":
                cur["released"] = True
                continue
            cur["released"] = False
            cur["label"] = obj.get("label") or "?"
            try:
                cur["pid"] = int(obj.get("pid") or 0)
            except (TypeError, ValueError):
                cur["pid"] = 0
            try:
                cur["ttl_minutes"] = int(obj.get("ttl_minutes") or 0)
            except (TypeError, ValueError):
                cur["ttl_minutes"] = 0
            cur["created"] = obj.get("created") or ""
            # Where the allocation was made from (scratch.sh, 2026-10-01). Only a
            # land's scoped sweep reads it, to attribute the allocation to the
            # agent whose workspace it was made in. Rows written before it existed
            # have none and are attributed to nobody.
            cur["cwd"] = obj.get("cwd") if isinstance(obj.get("cwd"), str) else ""
    return rows


def pid_alive(pid):
    """True / False / None ('cannot tell')."""
    if not pid or pid <= 0:
        return False
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        # It exists and belongs to somebody else. Alive, and NOT ours to
        # reason about further.
        return True
    except OSError:
        return None


# ---------------------------------------------------------------------------
# WHAT THE OPERATING SYSTEM HAS MARKED AS ITS OWN — added 2026-09-19
# ---------------------------------------------------------------------------
# THE ALERT THIS CLOSES, IN THE CEO'S WORDS (2026-09-19, reading the status
# line): "And what is this all about: MASSIVE ALERT — DISK: 218 path(s) could
# not be deleted (delete BY HAND)".
#
# Eighteen of those 218 were directories macOS creates under the per-user temp
# root for its OWN agents — AudioComponentRegistrar, SandboxHelper, talagent,
# studentd, StatusKitAgent, ${DaemonNameOrIdentifierHere} and twelve more. Every
# one is 64 bytes, owned by this uid, mode 0700, and has failed with
# `[Errno 1] Operation not permitted` on all 52 attempts since
# 2026-09-18T12:15:05Z. Nobody can delete them and nobody should: they are not
# garbage, they are the operating system's, and the alert had been screaming
# about them for a day. §54's "the clean-up failed" branch asks a PERSON to go
# and delete something. There is nothing here for a person to do.
#
# THE TEST IS THE OPERATING SYSTEM'S OWN MARK, NOT A NAME AND NOT A HEURISTIC.
# Measured on this machine, all 18 of 18:
#
#   $ ls -ldO $TMPDIR/talagent
#   drwx------@ 2 alex staff sunlnk 64 7 Sep 19:57 .../talagent
#   $ xattr -l $TMPDIR/talagent
#   com.apple.rootless: folders
#
# `sunlnk` is SF_NOUNLINK, a SUPER-USER file flag: it is exactly why unlink
# returns EPERM, it cannot be set by an unprivileged process, and `chflags` at
# this privilege cannot clear it — which is what makes this a classification
# rather than another thing to retry. st_flags is one lstat and needs no
# subprocess and no third-party module (os.getxattr does not exist on macOS
# CPython), so the flags are what this reads.
#
# WHY A STRUCTURAL TEST AND NOT A LIST OF THE EIGHTEEN NAMES: the same argument
# scan_unknown's header makes about allowlists. macOS adds agents with every
# point release, the names are nowhere in this tree, and a list somebody has to
# maintain converges on the day it stops being maintained. A flag the kernel
# sets covers the daemon that ships next year.
#
# NOTHING IS WEAKENED BY THIS. A path this refuses is a path no process at this
# privilege could ever have removed; the only thing that changes is that its
# removal is no longer ATTEMPTED and no longer REPORTED as a failure a person
# must act on.
_SF_NOUNLINK = getattr(stat, "SF_NOUNLINK", 0x00100000)
_SF_IMMUTABLE = getattr(stat, "SF_IMMUTABLE", 0x00020000)
_SF_APPEND = getattr(stat, "SF_APPEND", 0x00040000)
_SYSTEM_FLAGS = _SF_NOUNLINK | _SF_IMMUTABLE | _SF_APPEND

# Every flag bit `chflags` can name, so the message says WHICH one was found
# rather than repeating the mask back at the reader. Ordered most-specific
# first; only bits inside the declared mask are ever consulted.
_FLAG_NAMES = ((_SF_NOUNLINK, "sunlnk"),
               (_SF_IMMUTABLE, "schg"),
               (_SF_APPEND, "sappnd"),
               (getattr(stat, "SF_ARCHIVED", 0x00010000), "arch"),
               (getattr(stat, "UF_IMMUTABLE", 0x00000002), "uchg"),
               (getattr(stat, "UF_APPEND", 0x00000004), "uappnd"),
               (getattr(stat, "UF_NODUMP", 0x00000001), "nodump"),
               (getattr(stat, "UF_HIDDEN", 0x00008000), "hidden"))

_SYSTEM_MASK = [None]


def system_flags_mask():
    """The declared mask, read once.

    DECLARED, LIKE EVERY OTHER THRESHOLD IN THIS PROGRAM, and for one more
    reason besides: THE SUPER-USER FLAGS CANNOT BE SET WITHOUT ROOT, so a
    hermetic test cannot build a fixture carrying one. `chflags sunlnk` from an
    ordinary account is "Operation not permitted" — which is the same fact that
    makes this classification correct in production and would have made it
    untestable. A test world declares a mask of a flag it CAN set (`nodump`)
    and exercises the shipped code path byte for byte: a real chflags, a real
    st_flags, a real classification. The production default is the SF_* set and
    is what orchestration.config declares.

    UF_IMMUTABLE (`uchg`) IS DELIBERATELY NOT IN THE DEFAULT. apply()'s retry
    clears exactly that flag and then succeeds — a `uchg` path is ours, and an
    obstacle we can remove is not an owner.
    """
    if _SYSTEM_MASK[0] is not None:
        return _SYSTEM_MASK[0]
    v = (os.environ.get("SCRATCH_SYSTEM_FLAGS") or "").strip()
    mask = _SYSTEM_FLAGS
    if v:
        try:
            mask = int(v, 0)
        except ValueError:
            # A mask nobody can read is not a reason to sweep differently. Say
            # so and use the declared default: this decides only whether a path
            # is ATTEMPTED, and the conservative direction is to attempt it.
            sys.stderr.write("scratch-reaper: SCRATCH_SYSTEM_FLAGS is %r, which "
                             "is not a number; using the built-in mask.\n" % v)
    _SYSTEM_MASK[0] = mask
    return mask


def system_protected(path):
    """'' if this path is ours to remove, else WHY THE KERNEL SAYS IT IS NOT.

    A one-lstat test, safe on anything, and false for every ordinary file. It is
    consulted BEFORE a deletion is planned, so an OS-owned path never enters the
    failure ledger at all, and again when an old ledger row is reconsidered, so
    the eighteen already in there leave it.
    """
    mask = system_flags_mask()
    if not mask:
        return ""
    try:
        flags = getattr(os.lstat(path), "st_flags", 0)
    except OSError:
        return ""
    if not (flags & mask):
        return ""
    hit = [name for bit, name in _FLAG_NAMES if bit & mask and flags & bit]
    return ("the operating system set the file flag %s on it — a flag this "
            "program did not set and cannot clear at this privilege. macOS "
            "marks the per-user temp directories it makes for its own agents "
            "this way; it is not our garbage, nobody here can delete it, and "
            "there is nothing in it for a person to do"
            % ("/".join(hit) or "0x%x" % (flags & mask)))


def failures_path():
    """Where deletions that FAILED are remembered between runs.

    A separate small file rather than a scan of the log, because the log is
    append-only and grows without bound: answering "is this still failing" from
    it would mean re-reading every failure that ever happened and working out
    which were later resolved. This file holds only what is STILL failing.
    """
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.environ.get("SCRATCH_FAILURES_STATE") \
        or os.path.join(base, "state", "scratch-failures.json")


def read_failures(path):
    try:
        with open(path, encoding="utf-8") as fh:
            obj = json.load(fh)
    except (OSError, ValueError):
        return {}
    return obj if isinstance(obj, dict) else {}


def write_failures(path, rows):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(rows, fh, indent=1, sort_keys=True)
        os.replace(tmp, path)
    except OSError as exc:                                    # pragma: no cover
        sys.stderr.write("scratch-reaper: could not write %s: %s\n"
                         % (path, exc))


def process_table():
    """{pid: (uid, start_epoch)} for EVERY process on the machine, or None.

    ONE ps, read once. Separate from Liveness.claude_processes() because that one
    filters to session process names by design and this one must see pid 1.
    """
    try:
        r = subprocess.run(["ps", "-Ao", "pid=,uid=,lstart="],
                           capture_output=True, text=True, timeout=20,
                           env=_ps_env())
    except (OSError, subprocess.TimeoutExpired):
        return None
    if r.returncode != 0:
        return None
    out = {}
    for line in r.stdout.splitlines():
        parts = line.split()
        if len(parts) < 7:
            continue
        try:
            pid, uid = int(parts[0]), int(parts[1])
        except ValueError:
            continue
        out[pid] = (uid, lstart_epoch(" ".join(parts[2:7])))
    return out


def birth_time(path):
    """When the directory came into being, or None.

    st_birthtime on macOS, which is where this runs. NOT st_mtime: a tree that is
    still being written has a recent mtime and tells you nothing about when it was
    created, and creation is the only time that can be compared against a
    process's start.
    """
    try:
        st = os.lstat(path)
    except OSError:
        return None
    bt = getattr(st, "st_birthtime", None)
    if bt:
        return bt
    # Linux has no birth time through os.stat; ctime is the closest available and
    # is an inode-change time, so it is only ever LATER than the true creation.
    # That makes the comparison below more willing to call a pid unattributed,
    # which is the direction that loses coverage rather than work.
    return getattr(st, "st_ctime", None)


def pid_from_name(name):
    """The owning pid out of a `<pid>-<label>-<random>` directory name.

    The second record of the same fact, on the thing itself. A sweeper that
    has lost the ledger entirely can still attribute a directory from this.
    """
    head = name.split("-", 1)[0]
    if head.isdigit():
        try:
            return int(head)
        except ValueError:
            return 0
    return 0


class Walls(object):
    def __init__(self, roots):
        self.roots = [os.path.realpath(r) for r in roots]
        self.never = never_touch()
        self.registered = registered_workspaces()

    def check(self, path, has_git, git_is_fixture=False):
        """'' if the path may be deleted, else why it may not.

        `git_is_fixture` narrows WALL 2 and nothing else. It is passed ONLY by
        the legacy-family arm, and only ever after that arm has established
        every one of the other conditions. See the note on wall 2 below.
        """
        real = os.path.realpath(path)
        if real in self.never:
            return "wall 4: %s is on the never-touch list" % real
        if not any(inside(real, r) and real != r for r in self.roots):
            return ("wall 1: %s is not inside any declared scratch root (%s)"
                    % (real, ", ".join(self.roots) or "none"))
        if has_git and not git_is_fixture:
            return ("wall 2: this tree contains a .git — it is a checkout, not "
                    "scratch. If it is a test fixture, it is yours to delete by "
                    "hand; this program will not.")
        # WALL 2, NARROWED 2026-09-18, AND THE MEASUREMENT THAT FORCED IT.
        #
        # The first run of the widened legacy sweep returned 2,800
        # INDETERMINATE entries and ALL 2,800 were this wall. Every one was a
        # harness fixture that makes a throwaway repository under $TMPDIR:
        # richos-provision-git/-fresh/-ignore/-valid/-install-home and nine
        # more siblings at ~249 each, ws-spec-* at 52, byref.* — 0.61 GB and
        # 2,800 directory entries of test scaffolding.
        #
        # LEAVING THEM INDETERMINATE IS NOT THE SAFE CHOICE, IT IS THE ONE THAT
        # BREAKS THE MECHANISM. Undecidable makes the run exit 3, and exit 3 is
        # what the new watchdog turns into a MASSIVE ALERT. A permanent alert
        # that nobody can clear is noise, and noise is precisely what makes a
        # real signal get ignored — which is the failure this whole rule exists
        # to prevent. An alert that fires on 2,800 test fixtures every six
        # hours would be switched off within a day, and then the next 105 GB
        # arrives unannounced.
        #
        # So wall 2 is narrowed rather than removed, and ONLY where all six of
        # these already hold — the caller establishes 2-6 before asking:
        #   1. wall 1: a direct child of $TMPDIR
        #   2. the name matches a DECLARED legacy harness family
        #   3. nothing has touched the tree for the declared legacy age
        #   4. no process holds any file inside it open (machine-wide lsof)
        #   5. wall 3 below: not a registered workspace, and does not contain
        #      or sit inside one — which is what actually protects a real
        #      worktree, and it is untouched
        #   6. wall 4: not on the never-touch list
        #
        # A person does not clone their work into
        # $TMPDIR/richos-provision-git.XXXXXX.<random> and leave it untouched
        # for two hours with no open handle. A harness does exactly that, 2,800
        # times. Wall 3 is the wall that was ever protecting a real checkout
        # here; wall 2 was protecting test scaffolding from itself.
        #
        # This narrowing applies to NEITHER the allocator root NOR the claude
        # scratch roots NOR the nightly. Those arms pass git_is_fixture=False
        # and wall 2 stands for them exactly as before.
        for reg in self.registered:
            if inside(reg, real) or inside(real, reg):
                return ("wall 3: %s is a REGISTERED workspace" % reg)
        return ""


# ---------------------------------------------------------------------------
# measuring
# ---------------------------------------------------------------------------

def measure(path):
    """(bytes_on_disk, newest_mtime, contains_git). Errors are skipped, never
    guessed at: a file that vanished mid-walk was never going to be freed."""
    total, newest, has_git = 0, 0.0, False
    try:
        st = os.lstat(path)
    except OSError:
        return 0, 0.0, False
    newest = st.st_mtime
    total = getattr(st, "st_blocks", 0) * 512 or st.st_size
    if not os.path.isdir(path) or os.path.islink(path):
        return total, newest, False
    stack = [path]
    while stack:
        check_deadline()
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
            total += getattr(st, "st_blocks", 0) * 512 or st.st_size
            newest = max(newest, st.st_mtime)
            if e.is_dir(follow_symlinks=False):
                stack.append(e.path)
    return total, newest, has_git


_BUILD_CACHES = []


def build_caches():
    """lib/build_caches.py, loaded by path: the lander loads the same file, so the two can
    never disagree about which folder belongs to which checkout."""
    if not _BUILD_CACHES:
        spec = importlib.util.spec_from_file_location(
            "richos_build_caches",
            os.path.join(os.path.dirname(os.path.abspath(__file__)), "build_caches.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _BUILD_CACHES.append(mod)
    return _BUILD_CACHES[0]


_CODEX_WORKTREES = []


def codex_worktrees():
    """lib/codex_worktrees.py (Codex's finished worktrees, cleanup PRD A6), by path."""
    if not _CODEX_WORKTREES:
        spec = importlib.util.spec_from_file_location(
            "richos_codex_worktrees",
            os.path.join(os.path.dirname(os.path.abspath(__file__)), "codex_worktrees.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _CODEX_WORKTREES.append(mod)
    return _CODEX_WORKTREES[0]


def human(n):
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if n < 1024 or unit == "TB":
            return ("%d %s" % (n, unit)) if unit == "B" else ("%.1f %s" % (n, unit))
        n /= 1024.0
    return str(n)


# ---------------------------------------------------------------------------
# the plan
# ---------------------------------------------------------------------------

class Entry(object):
    def __init__(self, path, klass, size, action, why, standing=False):
        self.path, self.klass, self.size = path, klass, size
        self.action, self.why = action, why
        # A STANDING CONDITION: kept, and a PERSON has to do something about it.
        #
        # There are three kinds of KEEP and only two of them were distinguished.
        # Most are "nothing to do" — alive, young, held open — and report() hides
        # them unless asked. A campaign root past its declared retention is the
        # third kind: the program will never take it and §54 says Rich removes it
        # by hand. Hiding that behind --verbose is the same as not reporting it.
        #
        # A FLAG AND NOT A SUBSTRING MATCH. The first cut of this asked
        # `"PAST ITS RETENTION" not in e.why` in two different places, which makes
        # the report's prose load-bearing — reword the sentence and the accounting
        # silently changes.
        self.standing = standing

    def key(self):
        return (self.klass, self.path)


class Reaper(object):
    def __init__(self, cfg, ws):
        self.cfg = cfg
        self.ws = ws
        self.live = Liveness(ws)
        self.entries = []
        self.roots = []
        self.floor = cfg["age_floor_minutes"] * 60
        self.now = time.time()
        # The cached whole-machine open-file snapshot. None = not read yet,
        # _FAILED = read and could not be trusted. Three states, not two, for
        # the same reason the liveness verdict has three.
        self._open_tmp = None
        # The allocator-root reduction of the same snapshot. Three states like
        # its sibling: None = not read, _FAILED = read and untrustworthy.
        self._open_alloc = None
        # {declared shared temp root: set of children something holds open}.
        # Same three states again.
        self._open_shared = None
        self.docker_actions = []
        # Paths scan_standing_failures has already decided. The other arms skip
        # these, so no path is ever counted twice in the verdict.
        self._standing = set()
        # Ledger rows this pass has established were NEVER OURS TO DELETE. They
        # are removed from the failure file rather than retried for ever: a
        # failure record is a claim that a person must act, and a claim nobody
        # can act on is the kind of permanent alert that kills every other one.
        self._unfailable = set()
        # [(path, root)] the deny-by-default arm will decide LAST. Deferred so a
        # --notice run that runs out of budget loses this arm's numbers and not
        # the whole pass. See scan_deferred_unknown.
        self._unknown = []
        self.deferred_skipped = 0
        # Set by --notice: the expensive arm is NOT ATTEMPTED, and the garbage
        # numbers come from the last full pass's state file instead. See
        # scan_deferred_unknown.
        self.skip_unknown_arm = False
        self.unknown_not_scanned = 0
        # Set by --notice too: the build-cache arm walks the external drive's caches,
        # which a session-start banner must not pay for.
        self.skip_build_caches = False
        # And the Codex worktree arm, which runs git status in every Codex checkout.
        self.skip_codex = False
        # And the named-agent scratch report, which measures every leftover entry.
        self.skip_named = False
        # A LAND'S SCOPED SWEEP (--agent). None for every other run. When set,
        # scan_agent() replaces scan(), and apply() collects nothing it did not
        # plan: no test devices, no Docker. See scan_agent.
        self.scope = None
        self.unattributed = 0

    def named_scratch(self, path):
        """Scheduled collection never owns named scratch or a containing tree."""
        resolved = os.path.realpath(path)
        for root in self.cfg.get("agent_roots", []):
            if not root:
                continue
            root = os.path.realpath(root)
            if resolved == root or resolved.startswith(root + "/") or root.startswith(resolved + "/"):
                return True
        return False

    def add(self, path, klass, size, action, why, standing=False):
        self.entries.append(Entry(path, klass, size, action, why, standing))

    @staticmethod
    def reaches_outside(path, has_git, narrowed):
        """'' unless wall 2 is narrowed for this tree and a repository in it is a
        checkout of, or has worktrees in, a repository elsewhere (cleanup PRD R1)."""
        if not (has_git and narrowed):
            return ""
        why = repositories_reach_outside(path)
        return ("wall 2: %s; a checkout of another repository is closed by its "
                "owner, never deleted as a fixture" % why) if why else ""

    # --- classes ---------------------------------------------------------

    def scan_claude_roots(self, walls):
        for root in self.cfg["claude_roots"]:
            if not os.path.isdir(root):
                continue
            self.roots.append(root)
            oldest = self.live.oldest_start()
            for name in sorted(os.listdir(root)):
                entry = os.path.join(root, name)
                if os.path.isdir(entry) and not os.path.islink(entry):
                    self.scan_slug(entry, walls)
                else:
                    self.scan_orphan(entry, walls, oldest)

    def scan_slug(self, slug, walls):
        try:
            children = sorted(os.listdir(slug))
        except OSError:
            return
        sessions = [c for c in children if UUID_RE.match(c)]
        if not sessions:
            self.scan_orphan(slug, walls, self.live.oldest_start())
            return
        dead = 0
        for name in sessions:
            path = os.path.join(slug, name)
            # THE LIVE CHECK COMES BEFORE THE WALK, and not for tidiness: the
            # live session's scratchpad was 4.27 GB on the morning this was
            # written, and walking it is most of what a pass costs. A session
            # that is running is not a candidate, so its size is never needed.
            if name in self.live.running:
                self.add(path, "claude-session", 0, KEEP,
                         self.live.running[name][1] + " (not measured: a live "
                         "session is never a candidate)")
                continue
            size, newest, has_git = measure(path)
            state, why = self.live.verdict(name, newest)
            if state == RUNNING:
                self.add(path, "claude-session", size, KEEP, why)
                continue
            if state == INDETERMINATE:
                self.add(path, "claude-session", size, INDETERMINATE, why)
                continue
            # THE CLOCK COMES OFF THE SESSION'S OWN RECORD WHEN IT HAS ONE, and
            # off the newest mtime only when it does not. See ended_epoch: an
            # orphaned writer kept an 11 GB finished scratchpad permanently "0
            # min old" and the floor never expired for twenty-two hours.
            ended = self.live.ended_epoch(name)
            if ended is not None:
                age = self.now - ended
                clock = ("it recorded its end %d min ago and the floor is "
                         "%d min" % (age // 60, self.cfg["age_floor_minutes"]))
            else:
                age = self.now - newest
                clock = ("it was written %d min ago and the floor is %d min"
                         % (age // 60, self.cfg["age_floor_minutes"]))
            if age < self.floor:
                self.add(path, "claude-session", size, KEEP,
                         "%s, but %s" % (why, clock))
                continue
            # A REPOSITORY A LIVE APP INSTANCE HAS REGISTERED IS NOT GARBAGE,
            # whatever the session that made it did. See live_app_repository.
            held = self.live_app_repository(path)
            if held:
                self.add(path, "claude-session", size, KEEP, held)
                continue
            refused = walls.check(path, has_git)
            if refused:
                self.add(path, "claude-session", size, INDETERMINATE, refused)
                continue
            self.add(path, "claude-session", size, DELETE, why)
            dead += 1
        if dead == len(children):
            size, _, has_git = measure(slug)
            refused = walls.check(slug, has_git)
            if not refused:
                self.add(slug, "claude-husk", 0, DELETE,
                         "every session directory in this scratch root is "
                         "being deleted, so the root is left empty")

    def scan_orphan(self, path, walls, oldest_start):
        size, newest, has_git = measure(path)
        if oldest_start is None:
            self.add(path, "claude-orphan", size, INDETERMINATE,
                     "no process start time could be read, so it cannot be "
                     "shown that no running session wrote this")
            return
        if newest >= oldest_start:
            self.add(path, "claude-orphan", size, KEEP,
                     "written after the earliest running claude process "
                     "started, so a running session may own it")
            return
        if (self.now - newest) < self.floor:
            self.add(path, "claude-orphan", size, KEEP,
                     "younger than the %d min floor" % self.cfg["age_floor_minutes"])
            return
        refused = walls.check(path, has_git)
        if refused:
            self.add(path, "claude-orphan", size, INDETERMINATE, refused)
            return
        self.add(path, "claude-orphan", size, DELETE,
                 "no claude process running on this machine existed when this "
                 "was last written, so none of them wrote it and none can "
                 "write it again")

    def scan_standing_failures(self, walls):
        """PATHS A PREVIOUS RUN FAILED TO DELETE, AND STILL CANNOT.

        ===================================================================
        THE DEFECT THIS EXISTS FOR — A FAILED DELETION HID ITSELF
        ===================================================================
        Measured on the first real run of the widened sweep, 2026-09-18. Five of
        37,146 deletions failed with EPERM/EACCES on git-worktree fixtures. The
        NEXT run reported `ok:true, failures:0` — with all five directories
        still on disk.

        The reason is that `shutil.rmtree` had already removed some children
        before it hit the unremovable one, and THAT UPDATED THE DIRECTORY'S
        MTIME. The age is taken from the newest mtime, so the tree came back as
        "touched 1 min ago, inside the 2 h legacy floor" and was KEPT — not
        failed, not undecidable, KEPT, silently, for two hours.

        THE REAPER'S OWN FAILURE MADE ITS NEXT ATTEMPT IMPOSSIBLE AND ERASED THE
        EVIDENCE. Under the CEO's rule — "if the clean-up fails or impossible for
        some reason, then Rich must get a MASSIVE ALERT about it" — that is the
        worst available outcome: garbage that cannot be removed produces no
        alert, because the act of failing to remove it resets the clock and the
        next report says everything is fine.

        So a failure is now DURABLE. It is written to a state file, and every
        subsequent run reconsiders it REGARDLESS OF ITS AGE, retries it, and
        keeps reporting it until it is actually gone. The alert therefore
        persists until a person clears it, which is what the rule asks for.

        THE AGE FLOOR IS THE ONLY THING BYPASSED. Every wall still applies, and
        so does the open-handle check: the floor exists to cover the race
        between a thing starting and registering itself, and a path this program
        has already adjudicated and tried to delete is not in that race.
        """
        rows = read_failures(failures_path())
        if not rows:
            return
        for path in sorted(rows):
            check_deadline()
            if not os.path.exists(path):
                continue            # gone; apply() will drop it from the file
            self._standing.add(os.path.realpath(path))
            self._standing.add(path)
            size, _newest, has_git = measure(path)
            row = rows.get(path) or {}
            first = row.get("first") or "?"
            err = row.get("error") or "?"

            # A PATH THE KERNEL OWNS LEAVES THE LEDGER, and this is the branch
            # that clears the eighteen already in it. It is not a retry that
            # keeps failing quietly: the row is DELETED from the failure file by
            # _record_failures, the path is reported once under its own class
            # beside the other programs' temp, and no run ever tries again.
            owned = system_protected(path)
            if owned:
                self._unfailable.add(path)
                self.add(path, "tmp-system", size, KEEP,
                         "%s. It was recorded as a failed deletion %s and "
                         "retried %s time(s); that record is wrong and is "
                         "dropped — nothing here ever needed a person"
                         % (owned, first, row.get("attempts", "?")))
                continue

            snapshot = self.open_under_tmp()
            in_tmp = os.path.dirname(path) == \
                os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
            if snapshot is None and in_tmp:
                # The normal arms refuse when they cannot tell whether a tree is in
                # use (hunt part 5, P5-05); a retry has no more right to guess.
                self.add(path, "standing-failure", size, INDETERMINATE,
                         "a previous run failed to delete it, but the open-file table "
                         "could not be read, and a tree something is reading must never "
                         "be deleted from under it")
                continue
            if snapshot is not None and in_tmp and \
                    self.held_in_snapshot(path, snapshot):
                self.add(path, "standing-failure", size, KEEP,
                         "a process has opened a file inside it since the "
                         "failed deletion, so it is no longer abandoned")
                continue
            linked = self.reaches_outside(path, has_git, self.cfg["legacy_git_is_fixture"])
            if linked:
                self.add(path, "standing-failure", size, INDETERMINATE, linked)
                continue
            refused = walls.check(path, has_git,
                                  git_is_fixture=self.cfg["legacy_git_is_fixture"])
            if refused:
                self.add(path, "standing-failure", size, INDETERMINATE, refused)
                continue
            self.add(path, "standing-failure", size, DELETE,
                     "A PREVIOUS RUN FAILED TO DELETE THIS (first seen %s, "
                     "error: %s) and it is still here. Retried regardless of "
                     "age, because a failed rmtree updates the directory's "
                     "mtime and would otherwise hide this behind the age floor "
                     "for hours." % (first, err))

    def scan_scratch_root(self, walls):
        """THE ALLOCATOR'S ROOT, SWEPT DENY-BY-DEFAULT.

        Every child is a candidate unless a LIVE OWNER is proven. This is the
        arm that would have caught the 105 GB, and the reason it would is that
        it never asks what the directory is called.

        The verdict for one child, in order:

          released in the ledger        -> DELETE (its maker said it was done)
          owning pid ALIVE              -> KEEP, always, whatever the TTL says
          owning pid unknowable         -> INDETERMINATE
          owning pid dead, young        -> KEEP (the age floor)
          owning pid dead, old enough   -> DELETE
          no ledger row at all          -> the pid comes off the NAME and the
                                           same ladder applies; if the name
                                           carries no pid either, it is
                                           garbage by construction, because
                                           the only way to get a directory
                                           under this root is to ask the
                                           allocator for one.

        A LIVE PID IS ALWAYS KEEP, AND TTL NEVER OVERRIDES IT. An expired TTL
        means the caller underestimated its own run, which is a bad estimate
        and not permission to delete a directory a running process is writing
        into.

        THIS COMMENT USED TO CLAIM TTL COVERED PID REUSE AND IT DID NOT.
        It read: "TTL earns its place on the other side: it is what lets a row
        whose pid has been REUSED by an unrelated process still age out, because
        the age floor and the TTL both have to pass." A live pid returned KEEP
        two lines above TTL was ever consulted, so a reused pid was immortal and
        the comment described a safety the program did not have. What actually
        answers pid reuse is owner_state(): a process that started after the
        directory was created cannot have created it. TTL is what it always
        was — a declaration of intent, never an authority to delete.

        EVERY ALLOCATOR ROOT, NOT ONLY THIS JOB'S. scratch.sh allocates under
        whatever TMPDIR its caller has, and the nightly gates and Codex run with
        their own (the declared shared and producer temp roots). launchd's TMPDIR
        held none of their allocations, so a killed gate's sandbox there was never
        looked at (cleanup PRD 2026-10-23, coverage).
        """
        for root in self.allocator_roots():
            self.scan_allocator_root(root, walls)

    def allocator_roots(self):
        """This job's allocator root first, then one under each other temp root."""
        tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
        out = []
        for base in [tmp] + list(self.cfg.get("shared_tmp_roots") or []):
            root = os.path.join(os.path.realpath(base), self.cfg["scratch_root_name"])
            if root not in out:
                out.append(root)
        return out

    def scan_allocator_root(self, root, walls):
        if not os.path.isdir(root):
            return
        self.roots.append(root)
        rows = read_ledger(ledger_path())
        ttl_default = self.cfg["default_ttl_minutes"]

        for name in sorted(os.listdir(root)):
            check_deadline()
            path = os.path.join(root, name)
            if path in self._standing or os.path.realpath(path) in self._standing:
                continue    # scan_standing_failures already decided this one
            if os.path.islink(path):
                # A symlink in the root is not an allocation. Unlink it: it
                # cannot be big, and leaving it would let a symlink to
                # somewhere valuable sit inside a root that gets deleted from.
                self.add(path, "scratch-alloc", 0, DELETE,
                         "a symlink in the allocator root, which the allocator "
                         "never creates")
                continue
            size, newest, has_git = measure(path)
            row = rows.get(os.path.realpath(path)) or rows.get(path) or {}
            label = row.get("label") or "unrecorded"
            pid = row.get("pid") or pid_from_name(name)
            ttl = row.get("ttl_minutes") or ttl_default
            src = "ledger" if row else "the directory name"

            if row.get("released"):
                # Its maker said it was finished and the tree is still here,
                # so the rm it ran did not complete. Nothing owns it.
                self.add(path, "scratch-alloc", size, DELETE,
                         "the ledger records this as RELEASED by pid %d (%s), "
                         "so its maker is done with it" % (pid, label))
                continue

            alive, why_pid = self.owner_state(pid, bool(row), path)
            if alive is None:
                self.add(path, "scratch-alloc", size, INDETERMINATE,
                         "cannot tell whether pid %d is alive, and a live "
                         "owner is the one thing that must never be deleted "
                         "from under" % pid)
                continue
            if alive:
                self.add(path, "scratch-alloc", size, KEEP,
                         "pid %d is ALIVE (owner of '%s', from %s) — a live "
                         "owner is kept whatever its TTL says"
                         % (pid, label, src))
                continue

            age = self.now - newest
            if age < self.floor:
                self.add(path, "scratch-alloc", size, KEEP,
                         "younger than the %d min floor"
                         % self.cfg["age_floor_minutes"])
                continue

            # THE OPEN-FILE WALL (D5). After the age floor and before the
            # decision: the owning pid being dead means the ALLOCATION is over,
            # never that nothing is reading the tree. A live holder here is the
            # difference between collecting garbage and destroying work, and
            # this arm is the one the app's own test instances live under.
            #
            # Placed after the age floor deliberately, so the common case — a
            # young directory — never pays for a whole-machine lsof.
            snapshot = self.open_under_alloc(root)
            if snapshot is None:
                self.add(path, "scratch-alloc", size, INDETERMINATE,
                         "pid %d is ended, but the open-file table could not "
                         "be read, and a tree something is reading must never "
                         "be deleted from under it" % (pid or 0))
                continue
            if name in snapshot:
                # §54 addendum 4: a holder that is POSITIVELY a collectable test
                # instance is itself garbage and is quit at apply() time. Any
                # other holder, or a mixed set, keeps the tree.
                held_by, pids = self.test_instance_holders(path)
                if held_by == "test-instances":
                    self.add(path, "scratch-alloc", size, DELETE,
                             "pid %d is ENDED (owner of '%s', from %s) and the "
                             "only thing holding this open is test instance(s) "
                             "of the app (pid %s), which are themselves garbage "
                             "under §54 addendum 4 and are quit before this is "
                             "removed"
                             % (pid or 0, label, src,
                                ", ".join(str(p) for p in pids)))
                    continue
                self.add(path, "scratch-alloc", size, KEEP,
                         "pid %d is ended, but a LIVE process holds a file open "
                         "inside this tree%s — the allocation being over does "
                         "not make a tree something is reading garbage"
                         % (pid or 0,
                            "" if held_by != "mixed" else
                            " (one holder is a test instance of the app, but "
                            "not all of them are, so nothing here is quit)"))
                continue

            # A REPOSITORY INSIDE AN ENDED ALLOCATION IS A TEST FIXTURE ONLY ON
            # POSITIVE EVIDENCE (cleanup PRD R1): the allocator's own ledger row says
            # this directory was allocated (a name is never enough), its label is a
            # producer DECLARED to make only fixture repositories
            # (SCRATCH_FIXTURE_REPO_LABELS), and every repository in it is wholly
            # inside it. A row and containment prove ownership, not that nothing in
            # it is somebody's unpushed work: the mid-job review's fixture (a
            # 'prototype' allocation holding an unpushed commit, a tracked edit and
            # notes) was deleted on exactly that (rv-20261023T212629Z-3dc330ca-a2b9).
            # A declared suite killed by SIGKILL cannot run its trap, and wall 2 used
            # to keep its fixture repositories until a person deleted them.
            fixture = False
            if has_git and row and row.get("label") in (self.cfg.get("fixture_repo_labels") or []):
                outside = repositories_reach_outside(path)
                if outside:
                    self.add(path, "scratch-alloc", size, INDETERMINATE,
                             "wall 2: pid %d is ENDED and the ledger records this "
                             "allocation, but %s" % (pid or 0, outside))
                    continue
                fixture = True
            refused = walls.check(path, has_git, git_is_fixture=fixture)
            if refused:
                self.add(path, "scratch-alloc", size, INDETERMINATE, refused)
                continue

            if pid and why_pid:
                why = ("pid %d is not this allocation's owner (%s) and nothing "
                       "has touched this for %d min; its TTL was %d min"
                       % (pid, why_pid, age // 60, ttl))
            elif pid:
                why = ("pid %d is ENDED (owner of '%s', from %s) and nothing "
                       "has touched this for %d min; its TTL was %d min"
                       % (pid, label, src, age // 60, ttl))
            else:
                why = ("no ledger row and no owner pid in the name — the only "
                       "way to get a directory under this root is to ask the "
                       "allocator for one, so an unrecorded child is garbage "
                       "by construction")
            self.add(path, "scratch-alloc", size, DELETE, why)

    def owner_state(self, pid, from_ledger, path):
        """(True|False|None, '' or why-it-is-not-the-owner) for an allocation.

        =================================================================
        D11 — A NAME-DERIVED PID IS ATTRIBUTION, NOT LIVENESS
        =================================================================
        `pid_from_name()` takes the digits before the first `-`, and `pid_alive()`
        returns True on PermissionError. Frank put a directory called
        `1-frank-immortal-b` in the allocator root and the reaper said:

            KEEP  2.0 MB  .../richos-scratch/1-frank-immortal-b
                  why: pid 1 is ALIVE (owner of 'unrecorded', from the directory
                       name) — a live owner is kept whatever its TTL says

        Forever, at any size, with no alert and no TTL escape — and SILENTLY BY
        CONSTRUCTION, because a KEEP is the reaper working as designed. macOS
        recycles pids at 99998, so a week-old directory whose name begins with a
        number that is now a live pid is not exotic.

        THE LEDGER ROW IS A DIFFERENT KIND OF FACT AND IS STILL TRUSTED. It was
        written BY the allocator AT allocation time; the digits in a name are a
        second copy of it for the case where the ledger is lost, and a copy that
        can be forged by naming a directory is evidence of attribution and not of
        life. So a name-derived pid has to pass three tests that a ledger row does
        not, and each of them can only ever take a KEEP away:

          1. IT MUST BE OURS. A PermissionError means the process belongs to
             another user, and the allocator runs as us — so it cannot be the
             caller. This one test alone retires `1-frank-immortal-b`.
          2. IT MUST BE ABOVE A DECLARED FLOOR. Measured on this machine: the
             lowest pid owned by uid 501 is 160, while root's boot daemons hold
             1, 88, 90, 92, 93. A single- or double-digit pid is not a harness.
          3. IT MUST HAVE STARTED NO LATER THAN THE DIRECTORY WAS CREATED. A
             process that began after the directory existed cannot have made it,
             which is exactly what pid reuse looks like.

        THE BRIEF ASKED FOR TEST 3 THE OTHER WAY ROUND — "a pid whose process
        start time precedes the directory's creation is unattributed" — and that
        is inverted. A creator necessarily exists BEFORE it creates: a session
        process started this morning and allocating scratch this afternoon is the
        normal case, and refusing it would make every long-running owner
        unattributed. What proves reuse is starting AFTERWARDS.

        An unattributed pid does NOT mean delete. It means the allocation has no
        proven live owner, so the ordinary ladder applies underneath it — the age
        floor, the open-file wall, and all four walls.

        =================================================================
        THE REUSE TEST APPLIES TO A LEDGER ROW TOO, AND THAT IS A SECOND
        HOLE IN THE SAME FAMILY — FOUND BY THE MUTATION HARNESS
        =================================================================
        Mutant M37 was written to prove the ledger branch was load-bearing and it
        came back "the suite still PASSED without this property", which sent me
        back to the code with a better question. `scan_scratch_root`'s own comment
        claimed: "TTL earns its place on the other side: it is what lets a row
        whose pid has been REUSED by an unrelated process still age out, because
        the age floor and the TTL both have to pass."
        **THAT WAS NOT TRUE OF THE CODE.** A live pid returned KEEP before TTL was
        ever consulted — "a live owner is kept whatever its TTL says" — so a ledger
        row from three days ago whose pid now belongs to an unrelated live process
        was immortal in exactly the way `1-frank-immortal-b` was. The comment
        described a safety the program did not have.
        So the reuse test is applied to BOTH sources: it is a fact about the
        filesystem and the process table rather than a question of how much a
        record is trusted. What the ledger row earns is exemption from the two
        NAME-shape tests — the uid test and the pid floor — because it was written
        by our own process at allocation time and its uid is therefore implied.
        """
        if not pid or pid <= 0:
            return False, ""
        table = self.proc_table()
        if table is None:
            # The process table could not be read. pid_alive alone is what is
            # left, and it is the OLD behavior — which is a loss of this check
            # rather than a loss of safety, so it is taken rather than refused.
            return pid_alive(pid), ""
        if pid not in table:
            return False, ""
        uid, start = table[pid]
        born = birth_time(path)
        if born is not None and start is not None and start > born + 1:
            return False, ("it started %d s after this directory was created, so "
                           "it cannot have created it — this is pid reuse"
                           % int(start - born))
        if from_ledger:
            return True, ""
        if pid < self.cfg["min_owner_pid"]:
            return False, ("it is below the declared floor of %d, and the lowest "
                           "pid this user owns on this machine is 160 — a pid "
                           "that low is a boot daemon, not an allocator's caller"
                           % self.cfg["min_owner_pid"])
        if uid != os.getuid():
            return False, ("it belongs to uid %d and the allocator runs as %d, "
                           "so it cannot be the process that asked for this "
                           "directory — the name's digits collided with a live "
                           "pid" % (uid, os.getuid()))
        return True, ""

    def proc_table(self):
        """The whole-machine {pid: (uid, start)} table, read once per run."""
        if "_ptable" not in self.__dict__:
            self.__dict__["_ptable"] = process_table()
        return self.__dict__["_ptable"]

    def scan_tmp(self, walls):
        tmp = os.environ.get("TMPDIR") or "/tmp"
        tmp = os.path.realpath(tmp)
        if not os.path.isdir(tmp):
            return
        self.roots.append(tmp)
        # The LEGACY families get their own, tighter, age rule — declared in
        # hours rather than minutes because these are directories whose makers
        # have not been migrated to the allocator yet and which have already
        # been measured filling a disk inside a single run.
        legacy = self.cfg["legacy_tmp_patterns"]
        legacy_floor = self.cfg["legacy_age_hours"] * 3600
        scratch_root_name = self.cfg["scratch_root_name"]
        for name in sorted(os.listdir(tmp)):
            check_deadline()
            # Never twice. scan_scratch_root owns this one.
            if name == scratch_root_name or self.named_scratch(os.path.join(tmp, name)):
                continue
            is_legacy = any(fnmatch.fnmatch(name, pat) for pat in legacy)
            if is_legacy:
                self.scan_legacy_tmp(os.path.join(tmp, name), walls,
                                     legacy_floor)
                continue
            path = os.path.join(tmp, name)
            if not any(fnmatch.fnmatch(name, pat)
                       for pat in self.cfg["tmp_patterns"]):
                # THE LINE THAT WAS `continue`, AND THAT `continue` IS D1. Every
                # name outside both declared lists was not kept and not deleted —
                # it was never looked at, so no number in the verdict line could
                # be used to notice it. 56,770 of 60,942 entries on this machine.
                #
                # DEFERRED RATHER THAN DECIDED HERE, and the deferral is the
                # point: this arm is the expensive one (it walks every entry
                # under the root rather than the few that match a glob), so it
                # runs LAST, after every cheap arm has finished, and a --notice
                # run that exhausts its budget loses only this arm's numbers
                # instead of the whole pass. See scan().
                if self.cfg["deny_by_default"]:
                    self._unknown.append((path, tmp))
                continue
            if not os.path.isdir(path) or os.path.islink(path):
                continue
            size, newest, has_git = measure(path)
            if (self.now - newest) < self.floor:
                self.add(path, "tmp-workspace", size, KEEP,
                         "younger than the %d min floor"
                         % self.cfg["age_floor_minutes"])
                continue
            holder = self.holder(path)
            if holder is None:
                self.add(path, "tmp-workspace", size, INDETERMINATE,
                         "lsof could not answer whether a process holds this "
                         "open, and a temp workspace is only dead when its "
                         "creator is")
                continue
            if holder:
                self.add(path, "tmp-workspace", size, KEEP,
                         "held open by pid %s" % holder)
                continue
            refused = walls.check(path, has_git)
            if refused:
                self.add(path, "tmp-workspace", size, INDETERMINATE, refused)
                continue
            self.add(path, "tmp-workspace", size, DELETE,
                     "matches a declared harness pattern and no process holds "
                     "it open, so its creator is gone")

    def scan_legacy_tmp(self, path, walls, floor):
        if path in self._standing or os.path.realpath(path) in self._standing:
            return      # scan_standing_failures already decided this one
        """A LEGACY NAME FAMILY — the migration ramp, and it is meant to empty.

        These are the directories that 171 engine files still create with a
        bare mktemp. The families are declared in SCRATCH_LEGACY_TMP_PATTERNS
        and that list is expected to SHRINK to nothing as callers move to
        scripts/lib/scratch.sh; a permanent list of names would be the
        allowlist that let 105 GB through.

        The age rule is the tighter one and it is measured from the NEWEST
        mtime in the tree, so a harness that is still writing never ages into
        candidacy however long it runs. lsof sits behind that as the second
        answer.
        """
        name = os.path.basename(path)
        if not os.path.isdir(path) or os.path.islink(path):
            # A loose FILE from a legacy family is swept too — mktemp without
            # -d leaves files, and 1,945 of them were counted under one family.
            # They are small individually and they are why the entry count hit
            # 88,829, which is its own kind of unusable.
            try:
                st = os.lstat(path)
            except OSError:
                return
            if (self.now - st.st_mtime) < floor:
                return
            size = getattr(st, "st_blocks", 0) * 512 or st.st_size
            refused = walls.check(path, False)
            if refused:
                self.add(path, "tmp-legacy", size, INDETERMINATE, refused)
                return
            self.add(path, "tmp-legacy", size, DELETE,
                     "a loose file of the legacy family '%s', untouched for "
                     "more than the declared %d h" % (name, floor // 3600))
            return

        size, newest, has_git = measure(path)
        age = self.now - newest
        if age < floor:
            self.add(path, "tmp-legacy", size, KEEP,
                     "touched %d min ago, inside the %d h legacy floor — a "
                     "harness that is still writing never ages out"
                     % (age // 60, floor // 3600))
            return
        snapshot = self.open_under_tmp()
        if snapshot is None:
            self.add(path, "tmp-legacy", size, INDETERMINATE,
                     "the open-file table could not be read, and a temp "
                     "workspace is only dead when its creator is")
            return
        if self.held_in_snapshot(path, snapshot):
            # §54 ADDENDUM 4: A TEST INSTANCE HOLDING IT OPEN IS NOT A REASON TO
            # KEEP IT -- the instance IS the garbage, and it pins the rest.
            #
            # Before this, a stray app instance made its own scratch directory
            # immortal: held -> KEEP, on every run, for ever. Observed for real
            # while this was being written. One fake app left behind by a killed
            # test harness held its sandbox open, and the sweeper's verdict on
            # that directory was KEEP with "a process holds a file open inside
            # it" -- correct by the old rule, and the old rule guaranteed the
            # garbage stayed.
            #
            # The holder is only overridden when it is POSITIVELY a collectable
            # test instance. Any other holder -- an editor, a build, a shell, a
            # process this cannot place -- still means KEEP, exactly as before.
            held_by, pids = self.test_instance_holders(path)
            if held_by == "test-instances":
                self.add(path, "tmp-legacy", size, DELETE,
                         "held open only by test instance(s) of the app (pid "
                         "%s), which are themselves garbage under §54 addendum "
                         "4 and are quit before this is removed; nothing has "
                         "touched it for %d h"
                         % (", ".join(str(p) for p in pids), age // 3600))
                return
            self.add(path, "tmp-legacy", size, KEEP,
                     "a process holds a file open inside it"
                     + ("" if held_by != "mixed" else
                        " (one of them is a test instance of the app, but not "
                        "all of them are, so nothing here is quit)"))
            return
        # git_is_fixture is passed HERE and only here, and only now — after the
        # family match, the age floor and the open-handle check have all been
        # established above. Wall 3 (registered workspace) is what protects a
        # real worktree and it still applies in full.
        linked = self.reaches_outside(path, has_git, self.cfg["legacy_git_is_fixture"])
        if linked:
            self.add(path, "tmp-legacy", size, INDETERMINATE, linked)
            return
        refused = walls.check(path, has_git,
                              git_is_fixture=self.cfg["legacy_git_is_fixture"])
        if refused:
            self.add(path, "tmp-legacy", size, INDETERMINATE, refused)
            return
        fixture = " (its .git is harness scaffolding, not a checkout)" if has_git else ""
        self.add(path, "tmp-legacy", size, DELETE,
                 "a legacy harness family ('%s') left behind: nothing has "
                 "touched it for %d h and no process holds it open, so the "
                 "run that made it is over%s" % (name, age // 3600, fixture))

    # -----------------------------------------------------------------------
    # THE DENY-BY-DEFAULT TEMP ARM — added 2026-09-18, and it is the answer to
    # the shape of the defect rather than to its instances.
    # -----------------------------------------------------------------------
    # FRANK'S D1/D2, MEASURED ON THIS MACHINE AND RE-DERIVED HERE BEFORE THIS
    # WAS WRITTEN: `scan_tmp` skipped past any $TMPDIR entry matching neither
    # declared pattern list, and /private/tmp outside the two claude roots was
    # read by no arm at all.
    #
    #   direct children of $TMPDIR                   60,942
    #   covered by a declared pattern                 4,172   (0.08 GB)
    #   INVISIBLE to every arm                       56,770   (1.95 GB)
    #
    # THE ARGUMENT IS THE ASYMMETRY, NOT THE BYTE COUNT. An allowlist of names
    # can only ever contain names somebody read off a creator, and the biggest
    # invisible family on this disk — `richos-owned-wake-native-*`, 42 entries,
    # 198 MB, 9 days old — HAS NO CREATOR IN THE TREE ANY MORE:
    #
    #   $ grep -rln 'owned-wake-native' richos/     -> (no matches)
    #
    # A name nobody can read cannot be enumerated, so the enumeration cannot
    # converge. A keep-list of FOREIGN owners runs the other way: it names other
    # people's software, it is short, and it stops growing.
    #
    # ===================================================================
    # WHAT MAKES THIS SAFE, AND IT IS NOT THE AGE
    # ===================================================================
    # The primary proof is the one scan_orphan already uses on loose files in a
    # claude scratch root, and it is a proof rather than a guess: THE NEWEST
    # MTIME IN THE TREE PREDATES THE START OF EVERY RUNNING SESSION PROCESS, so
    # no running session can have written it and none can write it again. On this
    # machine at the time of writing there is one running session, started
    # "Wed Sep 16 23:31:27 2026" (TZ=UTC0) — so nothing touched in the last day
    # and a half is a candidate, whatever it is called.
    #
    # Four more conditions sit on top, and every one of them can only make the
    # arm refuse:
    #
    #   * OWNED BY THIS UID. /private/tmp is world-writable and shared: 97 of its
    #     753 entries are root's. Another user's temp directory is never ours to
    #     collect, and the check is one lstat.
    #   * PAST A DECLARED AGE FLOOR of its own, far longer than the legacy arm's
    #     two hours, because the legacy families are named-and-known and these
    #     are not.
    #   * NOTHING HOLDS A FILE OPEN INSIDE IT, from the same cached whole-machine
    #     lsof both older arms use.
    #   * ALL FOUR WALLS, with wall 2 (no .git) standing in full unless
    #     SCRATCH_UNKNOWN_GIT_IS_FIXTURE says otherwise.
    #
    # THE LEGACY PATTERN LIST IS NOT MADE REDUNDANT BY THIS, and that is a
    # finding against the brief that ordered the change. Frank's Fix 1 says the
    # families "become redundant under deny-by-default and can be deleted from
    # the config". They cannot: they are the FAST LANE. A legacy family is swept
    # at two hours because the 105 GB directory reached that size inside one run,
    # and this arm's floor is measured in days. Deleting the list would trade a
    # two-hour reclaim for a three-day one on exactly the family that filled the
    # disk. The list stays, and what changes is its JOB: it is no longer the
    # coverage, it is a fast lane on top of the coverage.
    def scan_unknown(self, path, walls, root, held):
        """One child of a temp root that no other arm claims. Deny-by-default.

        `held` is the cached open-file reduction for `root` — a set, or None
        when the open-file table could not be read.
        """
        name = os.path.basename(path.rstrip("/"))
        # NAMED APART FROM scan_legacy_tmp's `floor` ON PURPOSE. The mutation
        # harness replaces source text literally, and two arms sharing the
        # spelling `if age < floor:` would make a mutant aimed at one of them
        # silently rewrite both.
        unknown_floor = max(self.cfg["unknown_age_hours"] * 3600, self.floor)

        if os.path.islink(path):
            # The link, not its target. Wall 1 resolves realpath, which for a
            # link is wherever it points — so a link is reported rather than
            # reasoned about. There is one on this machine.
            self.add(path, "tmp-foreign", 0, KEEP,
                     "a symlink in a temp root: this arm unlinks nothing it "
                     "cannot contain, and where a link points is not this "
                     "program's to decide")
            return

        # THE KERNEL'S OWN MARK IS READ BEFORE THE DECLARED KEEP-LIST, because
        # it is the stronger statement of the same fact and it needs no name.
        # This is what stops the eighteen macOS agent directories being planned
        # for deletion at all — and the only reason the keep-list above did not
        # already cover them is that nobody had read their names off a creator,
        # which is the argument this arm's own header makes about allowlists.
        owned = system_protected(path)
        if owned:
            size, _newest, _git = measure(path)
            self.add(path, "tmp-system", size, KEEP, owned)
            return

        if any(fnmatch.fnmatch(name, pat) for pat in self.cfg["foreign_patterns"]):
            size, _newest, _git = measure(path)
            self.add(path, "tmp-foreign", size, KEEP,
                     "matches a declared FOREIGN owner — another program's "
                     "temp directory, counted so it is visible and never "
                     "deleted by us")
            return

        try:
            st = os.lstat(path)
        except OSError:
            return          # it went away mid-scan; it was never going to be freed
        if st.st_uid != os.getuid():
            self.add(path, "tmp-foreign", 0, KEEP,
                     "owned by uid %d, not this user — %s is shared and another "
                     "user's temp directory is never ours to collect"
                     % (st.st_uid, root))
            return

        size, newest, has_git = measure(path)

        oldest = self.live.oldest_start()
        if oldest is None:
            self.add(path, "tmp-unknown", size, INDETERMINATE,
                     "no session process start time could be read, so it "
                     "cannot be shown that no running session wrote this")
            return
        if newest >= oldest:
            self.add(path, "tmp-unknown", size, KEEP,
                     "touched after the earliest running session process "
                     "started, so a running session may own it")
            return

        age = self.now - newest
        if age < unknown_floor:
            self.add(path, "tmp-unknown", size, KEEP,
                     "nothing running can own it, but it was touched %d h ago "
                     "and the floor for an undeclared temp family is %d h"
                     % (age // 3600, unknown_floor // 3600))
            return

        if held is None:
            self.add(path, "tmp-unknown", size, INDETERMINATE,
                     "the open-file table could not be read, and a tree "
                     "something is reading must never be deleted from under it")
            return
        if name in held:
            # §54 addendum 4, exactly as the legacy arm applies it: a holder that
            # is POSITIVELY a collectable test instance is itself the garbage and
            # pins the rest. Any other holder, or a mixed set, keeps the tree.
            held_by, pids = self.test_instance_holders(path)
            if held_by == "test-instances":
                self.add(path, "tmp-unknown", size, DELETE,
                         "held open only by test instance(s) of the app (pid "
                         "%s), which are themselves garbage under §54 addendum "
                         "4 and are quit before this is removed; nothing "
                         "running can own it and nothing has touched it for %d h"
                         % (", ".join(str(p) for p in pids), age // 3600))
                return
            self.add(path, "tmp-unknown", size, KEEP,
                     "a live process holds a file open inside it"
                     + ("" if held_by != "mixed" else
                        " (one of them is a test instance of the app, but not "
                        "all of them are, so nothing here is quit)"))
            return

        # IN A PRODUCER'S OWN TEMP ROOT (SCRATCH_BULK_TMP_ROOTS) a repository is a
        # fixture only in a DECLARED harness family (SCRATCH_LEGACY_TMP_PATTERNS):
        # Codex works in its TMPDIR, and an unfinished clone or prototype there is
        # not shown disposable by being old and unopened (cleanup PRD R1; mid-job
        # review rv-20261023T212629Z-3dc330ca-a2b9). It stays undecided, reported.
        narrowed = self.cfg["unknown_git_is_fixture"] and (
            os.path.realpath(root) not in (self.cfg.get("producer_tmp_roots") or [])
            or any(fnmatch.fnmatch(name, pat) for pat in self.cfg.get("legacy_tmp_patterns") or []))
        linked = self.reaches_outside(path, has_git, narrowed)
        if linked:
            self.add(path, "tmp-unknown", size, INDETERMINATE, linked)
            return
        refused = walls.check(path, has_git, git_is_fixture=narrowed)
        if refused:
            self.add(path, "tmp-unknown", size, INDETERMINATE, refused)
            return

        self.add(path, "tmp-unknown", size, DELETE,
                 "no arm declares this name, and nothing needs to: no running "
                 "session process existed when it was last written, nothing "
                 "has touched it for %d h, no process holds a file open inside "
                 "it, and it is not a registered workspace" % (age // 3600))

    def scan_shared_tmp(self, walls):
        """The DECLARED SHARED TEMP ROOTS — /private/tmp on this machine.

        A SEPARATE ROOT AND NOT A PATTERN, because it is not $TMPDIR and nothing
        reaches it through $TMPDIR. Frank measured 25.77 GiB here that no arm of
        the mechanism had an opinion about, including a single 19 GiB directory of
        two-day-old evidence named `richos-*` on the volume the watchdog guards.

        The two claude roots live inside it and are swept by their own arm, so
        they are skipped by name here — never decided twice.
        """
        if not self.cfg["deny_by_default"]:
            return
        skip = set(os.path.realpath(r) for r in self.cfg["claude_roots"])
        skip.update(self.allocator_roots())     # decided by owner, in scan_scratch_root
        for root in self.cfg["shared_tmp_roots"]:
            if not os.path.isdir(root):
                continue
            self.roots.append(root)
            try:
                names = sorted(os.listdir(root))
            except OSError:
                continue
            for name in names:
                path = os.path.join(root, name)
                if os.path.realpath(path) in skip or self.named_scratch(path):
                    continue            # separate scoped/Claude-root collection owns it
                self._unknown.append((path, root))

    def live_agent_names(self):
        """Names of registered agents whose run has not finished (paused counts as
        not finished), or None when the agent registry cannot be read."""
        try:
            if self.ws.damaged_records(include_done=False):
                return None
            cache = {}
            return {r.get("name") for r in self.ws.all_agents()
                    if not self.ws.finished_state(r, cache)[0]}
        except Exception:
            return None

    def scan_named_scratch(self, walls):
        """NAMED-AGENT SCRATCH LEFT BEHIND, REPORTED AND NEVER DELETED HERE.

        <agent root>/<name> is collected only by a land's scoped sweep (scan_agent),
        so the scratch of an agent that was stopped or shelved and never landed was
        neither collected nor reported (review rv-20261023T232303Z-ba744b07-d444 #2).
        Each child older than the age floor whose name no unfinished registered agent
        carries is an undecided entry: the published result stays unresolved for
        this root, and the lead is told, until it is gone."""
        roots = [r for r in self.cfg.get("agent_roots") or [] if r and os.path.isdir(r)]
        if not roots or self.skip_named:
            return
        live = self.live_agent_names()
        for root in roots:
            self.roots.append(root)
            try:
                names = sorted(os.listdir(root))
            except OSError as exc:
                self.add(root, "agent-named", 0, INDETERMINATE,
                         "the named-agent scratch root could not be listed (%s)" % exc)
                continue
            for name in names:
                check_deadline()
                path = os.path.join(root, name)
                if live is not None and name in live:
                    self.add(path, "agent-named", 0, KEEP,
                             "named for %s, whose run has not finished" % name)
                    continue
                size, newest, _has_git = measure(path)
                if self.now - newest < self.floor:
                    self.add(path, "agent-named", size, KEEP,
                             "named scratch written %d min ago, inside the %d min floor"
                             % ((self.now - newest) // 60, self.cfg["age_floor_minutes"]))
                    continue
                if live is None:
                    self.add(path, "agent-named", size, INDETERMINATE,
                             "named-agent scratch, and the agent registry cannot be read "
                             "to tell whether %s is still running" % name)
                    continue
                self.add(path, "agent-named", size, INDETERMINATE,
                         "named-agent scratch no running agent carries (%s was stopped, "
                         "shelved or never landed). Only a land's sweep deletes named "
                         "scratch, so remove it by hand once nothing needs it" % name)

    def scan_deferred_unknown(self, walls):
        """THE DEFERRED DENY-BY-DEFAULT PASS, and the ONE arm allowed to be cut
        short without failing the run.

        WHY IT IS SEPARATE. Every other arm walks a handful of paths a glob
        picked out; this one walks every direct child of two whole temp roots —
        measured at 9.0 s warm for $TMPDIR's 60,942 children on this machine,
        against the 1.3 s the SessionStart notice was written around. A notice
        that blew its budget would print "the size is UNKNOWN" at every session
        start and be deleted within a week, and then nothing tells anybody
        anything, which is the failure the whole mechanism exists to end.

        So a Deadline here is NOT an error: what has been decided stays decided,
        the rest is counted, and the count is reported. `scan_deferred_skipped`
        is the number of entries this arm never reached, and Fix 2 puts it on the
        verdict line beside the undecidable count — an honest "N entries were not
        measured within the budget" is a usable sentence, and "UNKNOWN" is not.
        """
        # --notice DOES NOT ATTEMPT THIS ARM AT ALL, and that is a correction to
        # the first version of this change rather than a shortcut.
        #
        # With a budget, the arm reached part of the way and the banner said
        # "44,851 temp entries were NOT MEASURED within the 5 s budget" — AT
        # EVERY SESSION START, FOREVER, because 60,942 temp entries is what this
        # machine simply has. That line describes the budget, not the machine's
        # health, and a line that is always true is wallpaper. Wallpaper is how a
        # real signal comes to be skipped, which is the failure this whole
        # mechanism exists to prevent.
        #
        # So the banner's garbage numbers come from the LAST FULL PASS, which the
        # scheduled job publishes to scratch-reaper-state.json every six hours.
        # A pile of garbage nobody is coming for does not change in six hours —
        # that is what makes it that pile — and leg 3 of the notice already
        # shouts if the scheduled job has not completed a pass in fourteen.
        if self.skip_unknown_arm:
            self.unknown_not_scanned = len(self._unknown)
            return
        # THE OPEN-FILE TABLE DOES FAIL UNDER LAUNCHD ON THIS MACHINE, AND THAT
        # CHANGES WHAT AN UNREADABLE TABLE MAY COST.
        #
        # Read off the operator's own launchd log on 2026-09-18:
        #   $ grep -c 'verdict: undecided' ~/.claude/state/scratch-reaper-launchd.log  -> 1
        #   $ grep -c 'verdict: decided'   ~/.claude/state/scratch-reaper-launchd.log  -> 4
        # and the undecided one's reason is exactly this: "the open-file table could
        # not be read, and a temp workspace is only dead when its creator is".
        #
        # WHAT THAT ESTABLISHES IS THAT IT HAPPENS, NOT HOW OFTEN. Five verdicts is
        # not a frequency and no rate is claimed from it. Happening at all is enough,
        # because of what it would now cost: it cost ONE entry in that run, since the
        # old $TMPDIR arm had almost no candidates.
        #
        # THIS ARM HAS TENS OF THOUSANDS. Deciding each one individually would turn
        # a single lsof timeout into ~40,000 identical INDETERMINATE entries, an
        # enormous undecidable pile in the garbage alarm, and exit 3 — on every run
        # where it happens. Thousands of copies of one fact is the noise that makes a
        # real signal get skipped, which is the failure this mechanism exists to
        # prevent.
        #
        # So an unreadable table is reported ONCE PER ROOT, with the count. The
        # safety is identical — nothing under that root is deleted — and the report
        # is one line a person can read instead of a wall they will learn to page
        # past.
        held = {}
        done = 0
        blind = {}
        for path, root in self._unknown:
            if root not in held:
                tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
                held[root] = (self.open_under_tmp() if root == tmp
                              else self.open_under_shared(root))
            if held[root] is None:
                blind[root] = blind.get(root, 0) + 1
        for root, n in sorted(blind.items()):
            self.add(root, "tmp-unknown", 0, INDETERMINATE,
                     "THE OPEN-FILE TABLE COULD NOT BE READ, so the %d undeclared "
                     "entr(ies) directly under this root were not decided. A tree "
                     "something is reading must never be deleted from under it. "
                     "Reported once rather than %d times: one fact repeated %d "
                     "times is noise, and noise is what makes a real signal get "
                     "skipped." % (n, n, n))
        try:
            for path, root in self._unknown:
                check_deadline()
                if held.get(root) is None:
                    done += 1
                    continue        # already reported once, for this whole root
                if path in self._standing or \
                        os.path.realpath(path) in self._standing:
                    done += 1
                    continue        # scan_standing_failures already decided it
                self.scan_unknown(path, walls, root, held[root])
                done += 1
        except Deadline:
            pass
        self.deferred_skipped = len(self._unknown) - done

    def open_under_tmp(self):
        """One lsof for the WHOLE MACHINE, cached: the set of open paths under
        $TMPDIR. None if lsof could not be trusted to answer.

        WHY A SNAPSHOT AND NOT A PROBE PER DIRECTORY. The original tmp arm
        matched two globs and in practice zero directories, so one `lsof -t --
        <path>` per candidate was free. Widening the coverage to the legacy
        families made it ruinous: measured 2026-09-18 there are 87,330 entries
        under $TMPDIR and thousands match, and the first dry run of this change
        produced NO output at all before it was killed at 120 s — with exit
        144, which is the same signal that killed the harness that left the
        105 GB in the first place.

        A reaper too slow to finish is a reaper that gets removed from the
        session-start path, and then nothing sweeps anything. So the open-file
        table is read ONCE and asked about many paths, instead of being
        re-derived per path. `+D` is not used for the same reason: it walks the
        tree it is given, and the tree here is the 87,330-entry directory whose
        size is the problem.

        -n and -P suppress DNS and port-name lookups, which are the two things
        that make a full lsof hang on a machine with a network mount.
        """
        if self._open_tmp is not None:
            return self._open_tmp if self._open_tmp is not _FAILED else None
        tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
        try:
            r = subprocess.run(["lsof", "-n", "-P", "-F", "n"],
                               capture_output=True, text=True, timeout=120,
                               env=_ps_env())
        except (OSError, subprocess.TimeoutExpired):
            self._open_tmp = _FAILED
            self._open_alloc = _FAILED
            self._open_shared = _FAILED
            return None
        # lsof exits 1 when some of what it was asked about could not be
        # listed, which on a whole-machine scan is normal (other users'
        # processes). Its stdout is still the answer for everything it COULD
        # read, and treating a partial answer as no answer would make every
        # legacy candidate permanently INDETERMINATE.
        if r.returncode not in (0, 1):
            self._open_tmp = _FAILED
            self._open_alloc = _FAILED
            self._open_shared = _FAILED
            return None
        # Reduced to the set of FIRST PATH COMPONENTS under $TMPDIR, because
        # every candidate this answers about is a direct child of $TMPDIR. That
        # turns "is anything open inside this directory" from a scan of the
        # whole snapshot per candidate — thousands times thousands — into one
        # set lookup.
        #
        # A SECOND REDUCTION IS BUILT IN THE SAME PASS, keyed to children of the
        # ALLOCATOR ROOT. It cannot share the first one: the allocator's
        # candidates are children of $TMPDIR/<root>, so their first component
        # under $TMPDIR is the root's own name and every one of them would
        # collapse to the same key. Deriving it here rather than in a second
        # lsof keeps the cost at ONE whole-machine read, which is the property
        # that made this method worth having.
        # A THIRD FAMILY OF REDUCTIONS IS BUILT IN THE SAME PASS, one per
        # DECLARED SHARED TEMP ROOT (/private/tmp). Same argument as the
        # allocator reduction above: those candidates are children of a
        # different directory, so they cannot share $TMPDIR's key, and deriving
        # them here keeps the cost at ONE whole-machine read. On this machine
        # /private/tmp holds 753 direct children, so a per-candidate `lsof +D`
        # would be 753 tree walks against one read.
        pref = tmp + "/"
        # One reduction per allocator root (this job's and each other temp root's).
        allocs = [(a, a + "/") for a in self.allocator_roots()]
        shared = [(r, r.rstrip("/") + "/") for r in self.cfg["shared_tmp_roots"]]
        found = set()
        alloc_sets = dict((a, set()) for a, _ in allocs)
        shared_sets = dict((r, set()) for r, _ in shared)
        for line in r.stdout.splitlines():
            if not line.startswith("n"):
                continue
            p = line[1:]
            for root, spref in shared:
                if p.startswith(spref):
                    rest = p[len(spref):]
                    if rest:
                        shared_sets[root].add(rest.split("/", 1)[0])
            for aroot, apref in allocs:
                if p.startswith(apref):
                    rest = p[len(apref):]
                    if rest:
                        alloc_sets[aroot].add(rest.split("/", 1)[0])
            if not p.startswith(pref):
                continue
            rest = p[len(pref):]
            if not rest:
                continue
            found.add(rest.split("/", 1)[0])
        self._open_tmp = found
        self._open_alloc = alloc_sets
        self._open_shared = shared_sets
        return found

    def open_under_shared(self, root):
        """The set of children of a declared SHARED temp root that something
        holds open, or None if the open-file table could not be read.

        Same three states as its two siblings, and for the same reason: None
        keeps the tree, an empty set is permission to delete it.
        """
        if self._open_shared is None:
            self.open_under_tmp()      # populates every reduction in one read
        if self._open_shared is None or self._open_shared is _FAILED:
            return None
        return self._open_shared.get(os.path.realpath(root))

    def open_under_alloc(self, root=None):
        """The set of children of allocator root `root` (this job's when None)
        something holds open, or None.

        THE ALLOCATOR ARM WAS THE ONLY ONE THAT TRUSTED A PID ALONE, and that
        made it the one arm that could destroy live work rather than garbage.
        Both $TMPDIR arms already consulted the open-file table; this one went
        straight from "the owning pid is dead" to DELETE.

        REPRODUCED BEFORE FIXING, on this machine: a directory under the
        allocator root named for a dead pid (99999997), with a live `tail -f`
        holding a file open inside it, was planned for deletion —

          DELETE  2.0 MB  .../richos-scratch/99999997-zach-d5-repro
            why: pid 99999997 is ENDED (owner of 'unrecorded', from the
                 directory name) and nothing has touched this for 374961 min

        — and the reason given never mentions the holder, because nothing had
        looked. The owning pid being dead says the ALLOCATION is over; it says
        nothing about whether another process is reading the tree right now.
        """
        if self._open_alloc is None:
            self.open_under_tmp()      # populates both reductions in one read
        if self._open_alloc is None or self._open_alloc is _FAILED:
            return None
        if root is None:
            root = self.allocator_roots()[0]
        return self._open_alloc.get(root)    # a root never reduced is unknown, not empty

    def held_in_snapshot(self, path, snapshot):
        """True if any open file sits at or inside `path`.

        `path` is a direct child of $TMPDIR, so its basename is the key.
        """
        return os.path.basename(path.rstrip("/")) in snapshot

    def appinstances(self):
        """The shared test-instance definition, or None if it cannot be loaded.

        LAZY, AND A FAILURE IS NEVER FATAL. If this module cannot be imported the
        reaper behaves exactly as it did before addendum 4: a held directory is
        KEPT. Losing the new coverage is a regression; refusing to sweep at all
        because of it would be an outage.
        """
        if "appinst" in self.__dict__:
            return self.__dict__["appinst"]
        mod = None
        try:
            here = os.path.dirname(os.path.abspath(__file__))
            if here not in sys.path:
                sys.path.insert(0, here)
            import appinstances
            mod = appinstances
        except Exception:
            mod = None
        self.__dict__["appinst"] = mod
        return mod

    def app_repositories(self):
        """Every repository a LIVE instance of the app has registered.

        ===================================================================
        THE ONE THING IN A DEAD SESSION'S SCRATCHPAD THAT IS NOT GARBAGE
        ===================================================================
        A finished session's scratchpad is ours and is collected (scan_slug).
        But a QA fixture repository is built INSIDE one, and its absolute path
        is then written into the app's entity registry — so an app instance
        that is still running is pointed at a directory this program is about
        to remove. Deleting it is not reclaiming garbage; it is pulling a
        repository out from under a running program, and the running program
        is the only thing that knows it mattered.

        WHOSE REGISTRY IS READ, AND WHY BOTH. The operator's own
        ~/Library/Application Support/com.richos.app/entities.json is read
        ALWAYS — it is one small file, it names the repositories a person
        connected on purpose, and in practice none of them is ever in scratch,
        so it costs nothing and can only ever keep more. A TEST instance is
        launched with HOME set into scratch and keeps its registry under THAT
        home, so its path is discovered from the instance's own OPEN FILES
        rather than guessed: appinstances already reads every app process's open
        paths, for exactly the reason its header gives — macOS will not show
        another process's environment, but it will show what that process has
        open, and the registry is a file the app holds.

        SO THE SECOND SOURCE IS LIVE INSTANCES ONLY. A scratch registry
        belonging to no running process is a dead instance's opinion about a
        dead session's directory, and it protects nothing. The set is computed
        once per pass.

        Returns a set of realpaths. Never raises: if none of this can be read
        the set is empty and the reaper behaves exactly as it did before.
        """
        if "app_repos" in self.__dict__:
            return self.__dict__["app_repos"]
        out = set()
        rel = self.cfg["app_registry_relpath"]
        files = []
        home = os.path.expanduser("~")
        if home and rel:
            files.append(os.path.join(home, rel))
        mod = self.appinstances()
        if mod is not None and rel:
            try:
                found = mod.find() or []
            except Exception:                                 # pragma: no cover
                found = []
            tail = os.sep + rel
            # EVERY live app process, whatever appinstances decided about it.
            # The verdict answers "may this be quit"; this asks "what has it
            # got open", and a registry held by the operator's own instance
            # protects a repository just as much as a test instance's does.
            for inst in found:
                for p in (getattr(inst, "evidence", None) or []):
                    if p.endswith(tail):
                        files.append(p)
        for path in files:
            try:
                with open(path, encoding="utf-8") as fh:
                    obj = json.load(fh)
            except (OSError, ValueError):
                continue
            for ent in (obj.get("entities") or []) if isinstance(obj, dict) else []:
                if not isinstance(ent, dict):
                    continue
                for key in ("roots", "connected_repositories"):
                    for r in (ent.get(key) or []):
                        if isinstance(r, str) and r:
                            out.add(os.path.realpath(r))
        self.__dict__["app_repos"] = out
        return out

    def live_app_repository(self, path):
        """'' if nothing live has this registered, else why it is kept."""
        repos = self.app_repositories()
        if not repos:
            return ""
        real = os.path.realpath(path)
        for r in repos:
            if real == r or inside(r, real) or inside(real, r):
                return ("an instance of the app has %s registered as a "
                        "repository, and this %s it — a registered repository "
                        "is somebody's work, not scratch"
                        % (r, "is" if real == r else
                           "contains" if inside(r, real) else "sits inside"))
        return ""

    def test_instance_holders(self, path):
        """Who holds `path` open: ('test-instances', pids) | ('mixed', pids)
        | ('other', pids) | ('unknown', []).

        'test-instances' is returned ONLY when EVERY holder is a collectable
        test instance. That is the whole safety property: one unplaceable holder
        and the directory is kept, because a directory holding somebody's real
        work open is not garbage no matter what else is in it.
        """
        mod = self.appinstances()
        if mod is None:
            return "unknown", []
        pids_text = self.holder(path)
        if pids_text is None or not pids_text.strip():
            # None = lsof could not answer; '' = nobody. Neither is a set of
            # holders we can override, and the caller has already established
            # from the cached snapshot that something holds it.
            return "unknown", []
        try:
            pids = sorted({int(p) for p in pids_text.split()})
        except ValueError:
            return "unknown", []
        try:
            found = mod.find(roots=mod.RootSet(extra=[path]))
        except Exception:
            return "unknown", []
        if found is None:
            return "unknown", []
        collectable = {i.pid for i in found if i.verdict == mod.COLLECT}
        if not collectable:
            return "other", pids
        # Every holder must be accounted for. A pid holding the tree that is not
        # a collectable test instance is a reason to keep the tree.
        if set(pids) <= collectable:
            return "test-instances", pids
        return "mixed", pids

    def holder(self, path):
        """'' nobody, '<pids>' somebody, None cannot tell.

        `+D` FOR A DIRECTORY, AND THAT IS A BUG FIX, NOT A REFINEMENT. This used
        `lsof -t -- <path>` for everything, which asks "who has THIS NODE open".
        For a directory that is almost never the question: a process writing
        into a scratch tree holds a FILE INSIDE it, not the directory itself.
        Measured on this machine:

            $ lsof -t -- .../holdertest              -> (nothing)
            $ lsof +D .../holdertest -t               -> 94085
            $ lsof -t -- .../holdertest/f.lock        -> 94085

        So the old form answered "nobody holds it" about a directory a live
        `tail -f` was reading, and BOTH callers believed it. That is the same
        defect as D5 one level down, and it was in the arm that exists to
        protect a live agent workspace: a `richos-*-workspace` with a process
        writing inside it read as unheld.

        THE COST, and why it is acceptable here: `+D` walks the tree, which is
        why the $TMPDIR legacy arm uses the cached whole-machine snapshot
        instead. This function is only ever reached for ONE candidate that has
        already passed the age floor, so the walk is of a single stale scratch
        directory rather than of $TMPDIR's 87,000 entries.

        A TIMEOUT RETURNS None, NEVER ''. The three states are load-bearing:
        None becomes INDETERMINATE and keeps the tree, whereas '' would mean
        "proven unheld" and permission to delete. A slow answer must never be
        allowed to read as an absence of holders.
        """
        try:
            if os.path.isdir(path) and not os.path.islink(path):
                args = ["lsof", "+D", path, "-t", "-n", "-P", "-w"]
            else:
                args = ["lsof", "-t", "-n", "-P", "-w", "--", path]
            r = subprocess.run(args, capture_output=True,
                               text=True, timeout=60, env=_ps_env())
        except (OSError, subprocess.TimeoutExpired):
            return None
        # lsof exits 1 when it has nothing to report AND when some of what it
        # was asked about could not be listed; both are normal here and its
        # stdout is the answer either way.
        if r.returncode not in (0, 1):
            return None
        pids = [t for t in r.stdout.split() if t.isdigit()]
        # This process and its parent walk the tree to measure it, so they can
        # appear as holders of their own candidate. A reaper that reported itself
        # as the reason not to delete would keep everything for ever.
        mine = {str(os.getpid()), str(os.getppid())}
        return " ".join(p for p in pids if p not in mine)

    def scan_campaign_roots(self, walls):
        """DECLARED CAMPAIGN ROOTS — MEASURED AND REPORTED, NEVER DELETED.

        =================================================================
        FRANK'S D3, AND A MEASUREMENT THAT CHANGED WHAT THE FIX CAN BE
        =================================================================
        His finding: `richos-rechecks` (17.01 GiB) and
        `richos-password-free-workspaces` (8.41 GiB) sit under ~/ab in NO ledger
        row and under NO declared root, so the worktree reaper will never see them
        and the scratch reaper will never see them. True, and re-measured here at
        18.90 GB and 13.75 GB.

        HIS PROPOSED SIGNAL DOES NOT SURVIVE THE CENSUS. "Not in the ledger" was
        the natural rule, and running it over the whole of ~/ab on 2026-09-18 gives
        24 of 29 directories unnamed — including `fitapp` (the legacy product),
        `prospects` (the outreach data layer), `li-profile-da""ta-grabber` (the
        capture extension), `deeply`, `saferecord`, `autocoder`, `wsp`, `ai-book`
        and `press-and-publicity`. EVERY ONE OF THOSE IS THE OPERATOR'S OWN
        PROJECT. Under ~/ab the default is "this is somebody's work", which is the
        exact opposite of the default under $TMPDIR, so the deny-by-default shape
        that is right there would be catastrophic here.

        SO THE ASYMMETRY RUNS THE OTHER WAY AND THE SHAPE FOLLOWS IT. An
        enumeration is normally the wrong answer — it is the D1 lesson — but the
        failure modes are not comparable: a missed campaign root costs disk space
        that the watchdog's ~/ab consumer line still reports, while a
        deny-by-default miss costs the product tree. An enumeration that can only
        FAIL TO NOMINATE is a safe enumeration.

        AND NOTHING HERE IS EVER DELETED AUTOMATICALLY. Every one of these trees
        contains a `.git`, which is wall 2 — "a scratch directory holding a
        checkout is somebody's work" — and the reaper refuses those everywhere
        else. §54 has a second branch for exactly this case: *"if the clean-up
        fails or impossible for some reason, then Rich must get a MASSIVE ALERT
        about it and get on with manually deleting the garbage"*. So a campaign
        root past its declared retention is counted into `skipped`, which is what
        the garbage alarm reads, and the reason carries the command.
        """
        parent = os.path.expanduser(self.cfg["campaign_parent"] or "")
        names = self.cfg["campaign_roots"]
        if not parent or not names or not os.path.isdir(parent):
            return
        retention = self.cfg["campaign_retention_days"] * 86400
        for name in names:
            for path in sorted(glob.glob(os.path.join(parent, name))):
                check_deadline()
                if not os.path.isdir(path) or os.path.islink(path):
                    continue
                size, newest, _has_git = measure(path)
                age = self.now - newest
                if age < retention:
                    self.add(path, "campaign-root", size, KEEP,
                             "a declared campaign root, touched %d d ago, and "
                             "the declared retention is %d d"
                             % (age // 86400, retention // 86400))
                    continue
                self.add(path, "campaign-root", size, KEEP,
                         "A DECLARED CAMPAIGN ROOT PAST ITS RETENTION: nothing "
                         "has touched it for %d d and the declared retention is "
                         "%d d. IT IS NOT DELETED AUTOMATICALLY — it holds a "
                         "checkout, which is wall 2 everywhere else in this "
                         "program — so under §54 it is reported and a person "
                         "removes it:  rm -rf %s"
                         % (age // 86400, retention // 86400, path),
                         standing=True)

    def scan_nightly(self, walls):
        base = self.cfg["nightly_dir"]
        keep = self.cfg["nightly_keep"]
        for sub, klass, pattern in (("releases", "nightly-release", "*"),
                                    ("logs", "nightly-log", "*.log")):
            d = os.path.join(base, sub)
            if not os.path.isdir(d):
                continue
            self.roots.append(d)
            items = []
            for name in os.listdir(d):
                if not fnmatch.fnmatch(name, pattern):
                    continue
                path = os.path.join(d, name)
                size, newest, has_git = measure(path)
                items.append((newest, path, size, has_git))
            items.sort(reverse=True)
            for i, (newest, path, size, has_git) in enumerate(items):
                if i < keep:
                    self.add(path, klass, size, KEEP,
                             "one of the %d newest kept by declaration" % keep)
                    continue
                if (self.now - newest) < self.floor:
                    self.add(path, klass, size, KEEP,
                             "younger than the %d min floor"
                             % self.cfg["age_floor_minutes"])
                    continue
                refused = walls.check(path, has_git)
                if refused:
                    self.add(path, klass, size, INDETERMINATE, refused)
                    continue
                self.add(path, klass, size, DELETE,
                         "#%d newest of %d, and the declared retention is %d"
                         % (i + 1, len(items), keep))

    def scan_kept(self, walls):
        """WHAT AN AUTOMATIC LAND KEPT, DELETED AFTER THE DECLARED RETENTION
        (2026-10-04, §54). The land moves a merged, ended workspace's ignored
        files into <workspaces state>/kept/<key>-<time>/ instead of waiting for
        a waiver; nothing else ever removed them. Each such directory is aged
        from its own mtime, which is when the land made it: the files inside
        keep the mtimes they had in the workspace."""
        base = self.cfg.get("kept_dir") or ""
        if not base or not os.path.isdir(base):
            return
        self.roots.append(base)
        retention = self.cfg["kept_retention_days"] * 86400
        for name in sorted(os.listdir(base)):
            check_deadline()
            path = os.path.join(base, name)
            try:
                age = self.now - os.lstat(path).st_mtime
            except OSError:
                continue
            size, _newest, has_git = measure(path)
            if age < retention:
                self.add(path, "land-kept", size, KEEP,
                         "kept by a land %d d ago; the declared retention is %d d"
                         % (age // 86400, retention // 86400))
                continue
            refused = walls.check(path, has_git)
            if refused:
                self.add(path, "land-kept", size, INDETERMINATE, refused)
                continue
            self.add(path, "land-kept", size, DELETE,
                     "kept by a land %d d ago, past the declared retention of %d d"
                     % (age // 86400, retention // 86400))

    def scan_build_caches(self, walls):
        """BUILD OUTPUT ON THE EXTERNAL DRIVE (2026-10-08, §54). The drive was 98% full
        three weeks after it was added, 824 GB of it build caches nothing ever removed:
        every checkout's own Cargo and native-app folders after the checkout was gone,
        and every Cargo dependency unit after no build used it any more. The rules and
        their measurements are in lib/build_caches.py; this arm only plans them.

          build-cache        a folder keyed to a checkout that no longer exists
          build-cache-trash  what the lander moved aside and its own deletion left
          cargo-units        one row per Cargo profile directory: its units no build
                             has read for SCRATCH_BUILD_CACHE_IDLE_HOURS, deleted
                             under Cargo's lock in apply()
        """
        root = self.cfg.get("build_cache_root") or ""
        if self.skip_build_caches:
            return
        bc = build_caches()
        self.roots.append(root)
        cargo_roots = [r for r in self.cfg["cargo_target_roots"] if os.path.isdir(r)]
        floor_h = self.cfg["age_floor_minutes"] / 60.0
        orphan_s = self.cfg["build_cache_orphan_hours"] * 3600
        idle_s = self.cfg["build_cache_idle_hours"] * 3600

        trash = bc.trash_dir(root)
        planned = [trash]
        for name in (sorted(os.listdir(trash)) if os.path.isdir(trash) else []):
            path = os.path.join(trash, name)
            size, _newest, has_git = bc.walk(path)
            try:
                moved = calendar.timegm(time.strptime(name[:16], "%Y%m%dT%H%M%SZ"))
            except ValueError:
                moved = 0
            if self.now - moved < self.floor:
                self.add(path, "build-cache-trash", size, KEEP,
                         "moved aside by a land under %.0f h ago; its own deletion may "
                         "still be running" % floor_h)
                continue
            refused = walls.check(path, has_git)
            if refused:
                self.add(path, "build-cache-trash", size, INDETERMINATE, refused)
                continue
            self.add(path, "build-cache-trash", size, DELETE,
                     "moved aside by a land; its detached deletion did not finish")

        checkouts, unreadable = bc.live_checkouts(self.cfg["checkout_parents"],
                                                  extra=walls.registered)
        live = set()
        for c in checkouts:
            check_deadline()
            live |= bc.keys_for(bc.anchors(c))
        for family, path, key in bc.keyed_folders(root, cargo_roots):
            check_deadline()
            if key in live:
                self.add(path, "build-cache", 0, KEEP,
                         "%s cache of a checkout that exists" % family)
                continue
            if unreadable:
                self.add(path, "build-cache", 0, INDETERMINATE,
                         "no checkout I could read has key %s, but the worktree list of "
                         "%s could not be read" % (key, unreadable[0]))
                continue
            size, newest, has_git = bc.walk(path)
            if self.now - newest < orphan_s:
                self.add(path, "build-cache", size, KEEP,
                         "no live checkout has key %s, but something wrote or read it "
                         "%d min ago (declared idle time %d h)"
                         % (key, (self.now - newest) // 60,
                            self.cfg["build_cache_orphan_hours"]))
                continue
            busy = bc.busy_profiles(path)
            if busy:
                self.add(path, "build-cache", size, KEEP,
                         "a running Cargo build holds %s/.cargo-lock" % busy[0])
                continue
            refused = walls.check(path, has_git)
            if refused:
                self.add(path, "build-cache", size, INDETERMINATE, refused)
                continue
            self.add(path, "build-cache", size, DELETE,
                     "%s cache whose checkout is gone: no live checkout has key %s, and "
                     "nothing has written or read it for %d h"
                     % (family, key, (self.now - newest) // 3600))
            planned.append(path)

        for profile in bc.profile_dirs(cargo_roots, skip=planned):
            check_deadline()
            refused = walls.check(profile, False)
            if refused:
                self.add(profile, "cargo-units", 0, INDETERMINATE, refused)
                continue
            with bc.CargoLock(profile) as lock:
                if not lock.held:
                    self.add(profile, "cargo-units", 0, KEEP,
                             "a running Cargo build holds its lock; its units are "
                             "decided by the next pass")
                    continue
            idle = bc.idle_units(profile, idle_s, now=self.now)
            if not idle:
                continue
            e = Entry(profile, "cargo-units", sum(u["bytes"] for _k, u in idle), DELETE,
                      "%d Cargo unit(s) no build has read for %d h; deleted under "
                      "Cargo's lock, and Cargo rebuilds any that is needed again"
                      % (len(idle), self.cfg["build_cache_idle_hours"]))
            e.unit_keys = [k for k, _u in idle]
            self.entries.append(e)

    def codex_app_root(self):
        home = self.cfg.get("codex_home") or ""
        return os.path.realpath(os.path.join(home, "worktrees")) if home else ""

    def codex_attachments(self):
        home = self.cfg.get("codex_home") or ""
        if not home:
            return {"live": set(), "archived": set()}, ""
        return codex_worktrees().attachments(home)

    def scan_codex_worktrees(self, walls):
        """CODEX'S FINISHED WORKTREES (cleanup PRD section 3, A6), from the declared
        Codex worktree roots only; the decision and the closing are lib/codex_worktrees.py.

        A worktree that closes is planned DELETE and closed in apply() by git itself
        after every check is repeated. A finished worktree a Codex chat still has
        attached is UNDECIDED: only that chat's archive_worktree may close it. Kept
        work (unmerged, uncommitted, active, ignored material) is KEEP with its reason.
        """
        roots = [r for r in self.cfg.get("codex_worktree_roots") or [] if os.path.isdir(r)]
        if not roots or self.skip_codex:
            return
        # Never take git's optional index lock in someone else's checkout.
        os.environ["GIT_OPTIONAL_LOCKS"] = "0"
        cw = codex_worktrees()
        app_root = self.codex_app_root()
        attached, why_not = self.codex_attachments()
        cwds = self.ws._process_cwds(strict=True)
        # What is in use inside a checkout whatever the user's cwd: open files,
        # mapped libraries, running executables. One read, kept to these roots.
        opened = cw.open_paths(env=_ps_env())
        if opened is not None:
            prefixes = tuple(os.path.realpath(r).rstrip("/") + "/" for r in roots)
            opened = {p for p in opened if p.startswith(prefixes)}
        found, husks = cw.checkouts(roots)
        self.roots.extend(roots)
        for path in found:
            check_deadline()
            action, why, info = cw.decide(path, self.ws, app_root, attached, cwds, opened)
            if attached is None and why_not and action == cw.UNDECIDED:
                why = "%s (%s)" % (why, why_not)
            e = Entry(path, "codex-worktree", measure(path)[0] if action == DELETE else 0,
                      action, why)
            e.close_info = info
            self.entries.append(e)
        for husk in husks:
            real = os.path.realpath(husk)
            if attached is None:
                self.add(husk, "codex-husk", 0, INDETERMINATE,
                         "an emptied worktree parent, but the Codex app's attachment "
                         "records cannot be read (%s)" % why_not)
            elif any((p + "/").startswith(real + "/") for p in attached["live"]):
                self.add(husk, "codex-husk", 0, INDETERMINATE,
                         "an emptied worktree parent that a live Codex attachment still "
                         "names; the app has lost the checkout it shows that chat")
            else:
                refused = walls.check(husk, False)
                if refused:
                    self.add(husk, "codex-husk", 0, INDETERMINATE, refused)
                else:
                    self.add(husk, "codex-husk", 0, DELETE,
                             "a worktree parent holding nothing but the Codex app's "
                             "marker, and no live attachment names anything in it")

    # --- running it ------------------------------------------------------

    def scan(self):
        roots = [r for r in self.cfg["claude_roots"] if os.path.isdir(r)]
        tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
        nightly = [os.path.join(self.cfg["nightly_dir"], s)
                   for s in ("releases", "logs")]
        shared = [r for r in self.cfg["shared_tmp_roots"] if os.path.isdir(r)]
        kept = [self.cfg["kept_dir"]] if self.cfg.get("kept_dir") else []
        caches = [self.cfg["build_cache_root"]] if self.cfg.get("build_cache_root") else []
        cargo = [r for r in self.cfg.get("cargo_target_roots", []) if os.path.isdir(r)]
        codex = [r for r in self.cfg.get("codex_worktree_roots", []) if os.path.isdir(r)]
        walls = Walls(roots + [tmp] + nightly + shared + kept + caches + cargo + codex)
        # FIRST, so the other arms can skip what it has already claimed. A path
        # decided twice would be counted twice in the verdict, and a standing
        # failure is precisely a path another arm would otherwise report as KEEP.
        self.scan_standing_failures(walls)
        self.scan_claude_roots(walls)
        # BEFORE scan_tmp, which skips the allocator root by name so the two
        # never decide the same path twice.
        self.scan_scratch_root(walls)
        self.scan_tmp(walls)
        self.scan_shared_tmp(walls)
        self.scan_named_scratch(walls)
        self.scan_nightly(walls)
        self.scan_kept(walls)
        self.scan_build_caches(walls)
        self.scan_codex_worktrees(walls)
        self.scan_campaign_roots(walls)
        self.scan_docker_containers(walls)
        # LAST, ALWAYS. It is the only expensive arm and the only one that may be
        # cut short by a budget without failing the run; everything above has
        # already been decided by the time it starts.
        self.scan_deferred_unknown(walls)
        self.entries.sort(key=Entry.key)
        return self.entries

    def scan_agent(self, scope):
        """A LAND'S SWEEP: ONLY WHAT PROVABLY BELONGS TO THE AGENT BEING LANDED.

        WHAT IT REPLACED, AND WHY. A land used to run the whole machine's sweep
        (this program's scan()), so landing one agent deleted whatever any other
        agent's or session's scratch had become eligible for: on 2026-10-01 two
        lands deleted 141 entries, none of them the landed agent's (60 harness
        families and 79 unclaimed entries under $TMPDIR, two /private/tmp IPC
        files). Every one had passed the walls and the liveness proof, and none
        was the land's business. A land is the moment ONE agent's scratch becomes
        garbage, so that is all it may take. The whole machine stays the
        scheduled job's, which runs every six hours with the full proof.

        `scope` is {"name", "session", "workspaces": [paths], "hold_named": bool}.
        Two things are the agent's, and nothing else:

          1. AN ALLOCATION MADE FROM ITS WORKSPACE. A child of the allocator
             root whose ledger row's `cwd` lies inside one of the agent's
             workspaces. Then the allocator ladder: released -> DELETE; owning
             pid alive -> KEEP; unknowable -> INDETERMINATE; something holds a
             file open inside it -> KEEP; a wall -> INDETERMINATE.
          2. A DIRECTORY NAMED FOR IT. <root>/<name> for each declared agent
             root (SCRATCH_AGENT_ROOTS), and <session scratchpad>/<name> in the
             agent's own session's directory under each Claude scratch root.
             Kept if another live agent carries the same name (hold_named), or
             something holds a file open inside it; a wall makes it
             INDETERMINATE.

        Everything else it looks at in those places is UNATTRIBUTED: left alone
        and counted (`unattributed=` on the verdict line). The session
        scratchpad itself is never a candidate, nor is anything in it but the
        agent's named directory: the lead and every in-process teammate share it.
        """
        self.scope = scope
        name = scope.get("name") or ""
        ws = [os.path.realpath(p) for p in scope.get("workspaces") or [] if p]
        tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
        alloc_root = os.path.join(tmp, self.cfg["scratch_root_name"])
        claude_roots = [r for r in self.cfg["claude_roots"] if os.path.isdir(r)]
        agent_roots = [r for r in self.cfg["agent_roots"] if os.path.isdir(r)]
        walls = Walls(claude_roots + [tmp] + agent_roots)

        def mine(row):
            cwd = row.get("cwd") or ""
            if not cwd:
                return False
            real = os.path.realpath(cwd)
            return any(inside(real, w) for w in ws)

        def held(path, size, klass, owner):
            """True if the path was decided here as kept or undecidable."""
            h = self.holder(path)
            if h is None:
                self.add(path, klass, size, INDETERMINATE,
                         "%s, but the open-file table could not be read, and a "
                         "tree something is reading must never be deleted from "
                         "under it" % owner)
                return True
            if h.strip():
                self.add(path, klass, size, KEEP,
                         "%s, but a live process (pid %s) holds a file open "
                         "inside it" % (owner, h.strip()))
                return True
            return False

        # 1. allocations made from the agent's workspaces
        if os.path.isdir(alloc_root):
            self.roots.append(alloc_root)
            rows = read_ledger(ledger_path())
            for child in sorted(os.listdir(alloc_root)):
                check_deadline()
                path = os.path.join(alloc_root, child)
                row = rows.get(os.path.realpath(path)) or rows.get(path) or {}
                if os.path.islink(path) or not mine(row):
                    self.unattributed += 1
                    continue
                size, _newest, has_git = measure(path)
                owner = ("allocated by pid %d ('%s') from %s, inside %s's workspace"
                         % (row.get("pid") or 0, row.get("label") or "?",
                            row.get("cwd"), name))
                if not row.get("released"):
                    alive, why_pid = self.owner_state(row.get("pid") or 0, True, path)
                    if alive is None:
                        self.add(path, "agent-alloc", size, INDETERMINATE,
                                 "%s; cannot tell whether that pid is alive" % owner)
                        continue
                    if alive:
                        self.add(path, "agent-alloc", size, KEEP,
                                 "%s; that pid is ALIVE" % owner)
                        continue
                if held(path, size, "agent-alloc", owner):
                    continue
                refused = walls.check(path, has_git)
                if refused:
                    self.add(path, "agent-alloc", size, INDETERMINATE, refused)
                    continue
                self.add(path, "agent-alloc", size, DELETE,
                         "%s, which has landed; its maker %s and nothing holds "
                         "it open" % (owner, "released it" if row.get("released")
                                      else "has ended"))

        # 2. directories named for the agent
        places = list(agent_roots)
        session = scope.get("session") or ""
        if session:
            for root in claude_roots:
                for pad in sorted(glob.glob(os.path.join(root, "*", session, "scratchpad"))):
                    if os.path.isdir(pad) and not os.path.islink(pad):
                        places.append(pad)
        for place in places:
            try:
                children = sorted(os.listdir(place))
            except OSError:
                continue
            for child in children:
                if not name or child != name:
                    self.unattributed += 1
                    continue
                path = os.path.join(place, child)
                if os.path.islink(path) or not os.path.isdir(path):
                    self.unattributed += 1
                    continue
                size, _newest, has_git = measure(path)
                owner = "named for %s in %s" % (name, place)
                if scope.get("hold_named"):
                    self.add(path, "agent-named", size, KEEP,
                             "%s, but another LIVE agent carries the same name, so "
                             "this may be its directory" % owner)
                    continue
                if held(path, size, "agent-named", owner):
                    continue
                refused = walls.check(path, has_git)
                if refused:
                    self.add(path, "agent-named", size, INDETERMINATE, refused)
                    continue
                self.add(path, "agent-named", size, DELETE,
                         "%s, which has landed, and nothing holds it open" % owner)
        self.entries.sort(key=Entry.key)
        return self.entries

    def report(self, verbose=False):
        """The plan, with NO timestamps and no elapsed anything in it, so that
        --dry-run and --apply can be compared byte for byte."""
        out = []
        for line in self.live.notes:
            out.append("note: " + line)
        live = sorted(self.live.running.items())
        out.append("live sessions: %s"
                   % (", ".join("%s (pid %d)" % (s[:8], p)
                                for s, (p, _) in live) or "none"))
        for e in self.entries:
            # A STANDING KEEP IS ALWAYS PRINTED. Kept, and a person has to act on
            # it — hiding that behind --verbose is the same as not reporting it,
            # which is the half of §54 this whole pass exists to add.
            if e.action == KEEP and not verbose and not e.standing:
                continue
            out.append("%-13s %10s  %s" % (e.action, human(e.size), e.path))
            out.append("%13s %10s  why: %s" % ("", "", e.why))
        out.append(self.verdict_line())
        return "\n".join(out)

    def counts(self):
        d = sum(1 for e in self.entries if e.action == DELETE)
        i = sum(1 for e in self.entries if e.action == INDETERMINATE)
        k = sum(1 for e in self.entries if e.action == KEEP)
        b = sum(e.size for e in self.entries if e.action == DELETE)
        return d, i, k, b

    def undecidable_bytes(self):
        return sum(e.size for e in self.entries if e.action == INDETERMINATE)

    def skipped(self):
        """(count, bytes) OF GARBAGE NO RUN OF THIS PROGRAM WILL EVER TAKE.

        FRANK'S FIX 2, AND IT IS THE HALF OF THE CEO'S RULE THE MECHANISM DID NOT
        IMPLEMENT. His verdict: "Its alarm is a disk-space alarm and a
        delete-failure alarm. It has no garbage alarm." Garbage no arm considered
        produced neither a cleanup nor a word, and `kept=1611` out of 53,593 meant
        no number in the verdict line could be used to notice.

        Under deny-by-default nothing under the temp roots is unconsidered any
        more, so the honest number changed shape. What this counts is what the
        program has LOOKED AT AND WILL NEVER COLLECT:

          * another program's temp directory (the declared foreign keep-list)
          * another user's (the uid wall)
          * a symlink, whose target is not this program's to reason about
          * entries a BUDGETED run did not reach — counted, never guessed at

        It is deliberately NOT the same number as `kept`. A kept entry is usually
        alive or young and will be collected in due course; a skipped one will sit
        there for ever unless a person removes it. 1.13 GB of it on this machine
        on 2026-09-18, of which 942 MB is one VS Code installer directory.

        ENTRIES A BUDGET DID NOT REACH ARE **NOT** COUNTED HERE, and the first
        version of this method counted them. It printed "1.0 GB in 47,681
        place(s)" — a byte count from 2,830 measured entries wearing a place count
        inflated by 44,851 unmeasured ones. Two different facts in one sentence
        is a sentence nobody can act on. `not_measured` is its own field.
        """
        # TWO CLASSES, AND A CAMPAIGN ROOT BELONGS HERE FOR THE SAME REASON A
        # FOREIGN DIRECTORY DOES: this program has looked at it and will never
        # take it. A campaign root holds a checkout, which is wall 2 everywhere
        # else, so §54's second branch applies — Rich is told and removes it.
        # Only ones PAST their declared retention count; a young one is not
        # garbage yet and would make this number permanent wallpaper.
        rows = [e for e in self.entries
                if e.klass in ("tmp-foreign", "tmp-system") or e.standing]
        return len(rows), sum(e.size for e in rows)

    def skipped_classes(self):
        """The same pile, SPLIT BY WHO IT BELONGS TO AND WHO CAN ACT ON IT.

        ===================================================================
        WHY ONE NUMBER WAS NOT ENOUGH — THE CEO'S QUESTION, 2026-09-19
        ===================================================================
        He read the status line and asked: "And what is this all about: MASSIVE
        ALERT — DISK: ... 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT".
        The honest answer, measured the same hour on the same machine, is that
        the 13.9 GB was two completely different things added together:

          12.8 GiB   TWO DECLARED CAMPAIGN ROOTS past their retention —
                     richos-password-free-workspaces and richos-recovery-codex.
                     Ours, and the report already prints the exact `rm -rf` for
                     each. A person removing them reclaims 12.8 GB.
           1.15 GiB  4,463 entries of OTHER PROGRAMS' temp — VS Code's updater
                     at 898 MB, Codex's dot-directories, WebKit's blob registry.
                     Nobody should ever remove these and nothing about them is
                     an alert.

        Summing those under one heading made the actionable 12.8 GB invisible
        inside a number that is mostly not actionable, and made the whole line
        read as an alarm about other people's files. A count is only usable if
        everything inside it calls for the same response.

        Returns an ordered list of (key, label, count, bytes). The key is what
        the watchdog switches on; the label is the sentence a person reads.
        """
        by_hand = [e for e in self.entries if e.standing]
        foreign = [e for e in self.entries
                   if not e.standing and e.klass in ("tmp-foreign", "tmp-system")]
        return [
            ("by_hand",
             "OURS, AND NEVER DELETED AUTOMATICALLY — a person removes it",
             len(by_hand), sum(e.size for e in by_hand)),
            ("foreign",
             "another program's or the operating system's temporary files",
             len(foreign), sum(e.size for e in foreign)),
        ]

    def verdict_line(self):
        d, i, k, b = self.counts()
        sn, sb = self.skipped()
        line = ("verdict: %s deletable=%d reclaimable=%s kept=%d "
                "undecidable=%d skipped=%d skipped_bytes=%s"
                % ("undecided" if i else "decided", d, human(b), k, i,
                   sn, human(sb)))
        # THE SPLIT, ON THE SAME LINE AS THE TOTAL IT SPLITS. `skipped=` on its
        # own was the number the CEO asked about, and the answer was that most
        # of it is a person's to remove and the rest is nobody's business. Both
        # halves are printed so the total can never again stand in for either.
        for key, _label, n, b2 in self.skipped_classes():
            line += " %s=%d %s_bytes=%s" % (key, n, key, human(b2))
        # ONLY WHEN IT IS TRUE, so the line a person is used to reading does not
        # grow a field that is always zero. A budget that truncated the scan is
        # an exceptional condition and reads as one.
        if self.deferred_skipped:
            line += " not_measured=%d" % self.deferred_skipped
        if self.unknown_not_scanned:
            line += " not_scanned=%d" % self.unknown_not_scanned
        if self.scope is not None:
            line += " scope=agent:%s unattributed=%d" % (
                self.scope.get("name") or "?", self.unattributed)
        return line

    def prune_docker(self):
        """Docker's own caches, which no walk of the filesystem can reclaim.

        CEO, 2026-09-18 (§54 addendum 3), on being shown `docker system df`:
        19.46 GB of unused images and 8.5 GB of reclaimable build cache
        accumulating with nothing removing them.

        THIS DOES NOT DELETE FILES, IT ASKS DOCKER TO. Docker's data root on
        this machine is /Volumes/E1TB/vm/docker/DockerDesktop — the external
        SSD, a different volume from the one the primary threshold guards, and
        a directory no wall in this program would ever be allowed inside. The
        only correct way to reclaim it is the daemon's own API.

        A STOPPED DAEMON IS SKIPPED, SILENTLY, AND IS NEVER A FAILURE. Docker
        Desktop is not running most of the time on a laptop. A scheduled job
        that reported a failure every six hours because an optional tool was
        not started would train its reader to ignore the log, which is the same
        way the 2,800-fixture alert would have died.

        Returns a list of log lines. Never raises.
        """
        out = []
        if not self.cfg["docker_prune"]:
            return out
        # ONE RESOLVER, shared with scan_docker_containers. See _docker_path.
        docker = self._docker_path()
        if docker is None:
            # Not installed. Not a failure, and not worth a line every run.
            return out

        # IS THE DAEMON ANSWERING? `docker info` talks to the daemon, unlike
        # `docker --version` which answers from the client alone and would say
        # yes with Docker Desktop shut down.
        try:
            probe = subprocess.run([docker, "info", "--format", "{{.ServerVersion}}"],
                                   capture_output=True, text=True, timeout=20)
        except (OSError, subprocess.TimeoutExpired):
            return out
        if probe.returncode != 0 or not probe.stdout.strip():
            return out

        stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        # THREE STAGES, AND THE ORDER IS THE WHOLE POINT OF THE FIRST ONE.
        #
        # CEO, 2026-09-18: "Go for stricter standing rule i.e. automatically
        # dropping unused named images after 30 days" — given after being told
        # the caveat that STOPPED CONTAINERS PIN IMAGES. So containers are
        # pruned BEFORE images: run it the other way round and every image held
        # by a months-old exited container survives stage 2 for a reason
        # nothing in the log would explain.
        #
        # `container prune` only ever considers STOPPED containers. A running
        # one is not a candidate at any age, which is why no extra guard is
        # needed to protect it.
        #
        # `image prune -a` with `until=720h` is the stricter rule he chose, and
        # it is what finally reclaims the ~17.8 GB of unused-but-TAGGED images
        # measured on 2026-09-18 that dangling-only prune could not touch. The
        # age filter is what makes -a safe here: an image pulled or built this
        # month is never a candidate, however unreferenced it is today.
        until = self.cfg["docker_until"]
        jobs = [
            (["container", "prune", "-f", "--filter", "until=" + until],
             "docker-container-prune"),
            (["image", "prune", "-a", "-f", "--filter", "until=" + until],
             "docker-image-prune"),
            (["builder", "prune", "-f", "--keep-storage",
              self.cfg["docker_keep_storage"]], "docker-builder-prune"),
        ]
        for args, klass in jobs:
            try:
                r = subprocess.run([docker] + args, capture_output=True,
                                   text=True, timeout=600)
            except (OSError, subprocess.TimeoutExpired) as exc:
                out.append("%s FAILED docker bytes=0 class=%s why=%s error=%s"
                           % (stamp, klass, " ".join(args), exc))
                self.docker_actions.append((klass, 0, str(exc)))
                continue
            if r.returncode != 0:
                err = (r.stderr or "").strip().splitlines()
                out.append("%s FAILED docker bytes=0 class=%s why=%s error=%s"
                           % (stamp, klass, " ".join(args),
                              err[-1] if err else "exit %d" % r.returncode))
                self.docker_actions.append((klass, 0, "exit %d" % r.returncode))
                continue
            freed = _docker_reclaimed(r.stdout)
            # EVERY REMOVAL NAMED, not just the total — the CEO asked for name,
            # size and age. Docker names each item it removed but reports only
            # one total, so the size sits on the summary line and the names on
            # their own. The AGE is the filter: every item on this list was
            # older than it, which is the only age statement that is true of all
            # of them rather than guessed per item.
            for item in _docker_removed_names(r.stdout):
                out.append("%s DELETED docker bytes=0 class=%s-item why=%s"
                           % (stamp, klass,
                              "%s, older than the declared %s"
                              % (item, until if "builder" not in klass
                                 else "keep-storage floor")))
            out.append("%s DELETED docker bytes=%d class=%s why=%s"
                       % (stamp, freed, klass,
                          "the daemon reclaimed it (%s)" % " ".join(args)))
            self.docker_actions.append((klass, freed, ""))
        out.extend(self.prune_docker_volumes(docker, stamp))
        return out

    def prune_docker_volumes(self, docker, stamp):
        """THE FOURTH STAGE — unused ANONYMOUS volumes past the declared age.

        =================================================================
        THE BRIEF PRESCRIBED `docker volume prune --filter until=...`
        AND THAT FILTER DOES NOT EXIST
        =================================================================
        Checked before it was built, against this machine and against the vendor's
        own reference:

            $ docker volume prune --help
              --filter filter   Provide filter values (e.g. "label=<label>")
            $ docker version --format '{{.Server.Version}}'   ->  29.2.0

        and docs.docker.com/reference/cli/docker/volume/prune: *"The currently
        supported filters are: label"*. There is no `until` for volumes — unlike
        container, image and builder prune, which all take one. So the prescription
        could not have worked, and copying it would have shipped either a command
        that errors on every run or, worse, one whose unknown filter is ignored and
        which therefore prunes with NO AGE LIMIT AT ALL.

        THE AGE IS NOT OPTIONAL HERE, which is why this is not simply
        `docker volume prune -f`. Every other stage of this sweep is safe because
        of its age filter: "an image pulled or built this month is never a
        candidate, however unreferenced it is today". A volume detached from a
        container ten minutes ago is exactly the thing somebody is about to
        reattach. So the window is applied HERE, from
        `docker volume inspect --format '{{.CreatedAt}}'`, which is documented and
        which this machine answers with an RFC3339 timestamp.

        ANONYMOUS VOLUMES ONLY, and that is Docker's own default rather than a
        choice of mine: `docker volume prune` needs `-a` before it will touch a
        NAMED volume, because a name is somebody having meant it. Measured here:
        21 dangling volumes, 402.2 MB, every one carrying
        `com.docker.volume.anonymous`.
        """
        out = []
        hours = _hours_from_until(self.cfg["docker_until"])
        if hours is None:
            out.append("%s FAILED docker bytes=0 class=docker-volume-prune "
                       "why=unreadable-age error=%r is not a duration this "
                       "understands (expected e.g. 720h)"
                       % (stamp, self.cfg["docker_until"]))
            return out
        try:
            r = subprocess.run([docker, "volume", "ls", "-q", "-f",
                                "dangling=true"],
                               capture_output=True, text=True, timeout=120)
        except (OSError, subprocess.TimeoutExpired) as exc:
            out.append("%s FAILED docker bytes=0 class=docker-volume-prune "
                       "why=list error=%s" % (stamp, exc))
            return out
        if r.returncode != 0:
            return out
        cutoff = self.now - hours * 3600
        removed = 0
        for name in r.stdout.split():
            try:
                i = subprocess.run(
                    [docker, "volume", "inspect", name, "--format",
                     "{{.CreatedAt}}|{{index .Labels \"com.docker.volume.anonymous\"}}"],
                    capture_output=True, text=True, timeout=60)
            except (OSError, subprocess.TimeoutExpired):
                continue
            if i.returncode != 0:
                continue
            created, _, anon = (i.stdout or "").strip().partition("|")
            # `index` on a missing key prints "<no value>"; an anonymous volume
            # carries the label with an EMPTY value, which prints as "".
            if anon.strip() == "<no value>":
                continue            # a NAMED volume: somebody meant it
            when = _rfc3339_epoch(created)
            if when is None or when > cutoff:
                continue
            try:
                d = subprocess.run([docker, "volume", "rm", name],
                                   capture_output=True, text=True, timeout=120)
            except (OSError, subprocess.TimeoutExpired) as exc:
                out.append("%s FAILED docker bytes=0 class=docker-volume-prune "
                           "why=%s error=%s" % (stamp, name, exc))
                continue
            if d.returncode != 0:
                err = (d.stderr or "").strip().splitlines()
                out.append("%s FAILED docker bytes=0 class=docker-volume-prune "
                           "why=%s error=%s"
                           % (stamp, name, err[-1] if err else
                              "exit %d" % d.returncode))
                continue
            removed += 1
            out.append("%s DELETED docker bytes=0 class=docker-volume-prune-item "
                       "why=%s, anonymous and unused, created %s (older than the "
                       "declared %s)" % (stamp, name, created,
                                         self.cfg["docker_until"]))
        if removed:
            out.append("%s DELETED docker bytes=0 class=docker-volume-prune "
                       "why=%d anonymous unused volume(s) older than %s"
                       % (stamp, removed, self.cfg["docker_until"]))
            self.docker_actions.append(("docker-volume-prune", 0, ""))
        return out

    def scan_docker_containers(self, walls):
        """A RUNNING CONTAINER IS IMMORTAL BY DESIGN — so it gets reported.

        Frank's D14. `container prune` only ever considers STOPPED containers, and
        the shipped code says so as a virtue: "a running one is not a candidate at
        any age". That is correct as a deletion rule and it leaves a hole:

            $ docker ps --format '{{.Names}} | {{.Image}} | up {{.RunningFor}} | {{.Command}}'
            rl55 | richos-linux:git-latest | up 8 days ago | "sleep infinity"
            rlx  | richos-linux:24.04      | up 8 days ago | "sleep infinity"

        Two `sleep infinity` dev shells, eight days old, pinning 514 MB and 349 MB
        of image so `image prune -a` can never reclaim them. Nothing alerts, and
        that is how dev containers are actually used, so it recurs.

        NOTHING IS EVER STOPPED HERE. A running container may be a service — four
        of the six on this machine are the Buzz production stack — and stopping one
        automatically is a category of action this program does not take. It is
        REPORTED, as a standing entry, so it reaches the garbage alarm and a person
        decides. Same shape as a campaign root, for the same reason.

        Narrow by construction: only images matching a DECLARED throwaway pattern,
        and only past a declared age.
        """
        pats = self.cfg["docker_throwaway_images"]
        if not self.cfg["docker_prune"] or not pats or self.skip_unknown_arm:
            return
        docker = self._docker_path()
        if docker is None or not self._docker_alive(docker):
            return
        try:
            r = subprocess.run(
                [docker, "ps", "--format",
                 "{{.Names}}\t{{.Image}}\t{{.CreatedAt}}\t{{.Command}}"],
                capture_output=True, text=True, timeout=60)
        except (OSError, subprocess.TimeoutExpired):
            return
        if r.returncode != 0:
            return
        max_age = self.cfg["docker_container_alert_days"] * 86400
        for line in r.stdout.splitlines():
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            name, image, created = parts[0], parts[1], parts[2]
            if not any(fnmatch.fnmatch(image, p) for p in pats):
                continue
            when = _docker_ps_epoch(created)
            if when is None:
                continue
            age = self.now - when
            if age < max_age:
                continue
            self.add("docker://container/" + name, "docker-container", 0, KEEP,
                     "A RUNNING CONTAINER ON A DECLARED THROWAWAY IMAGE (%s) has "
                     "been up %d d, and the declared limit is %d d. It is never "
                     "stopped automatically — a running container may be a service "
                     "— and `container prune` considers stopped ones only, so it "
                     "pins its image for ever. Under §54 it is reported and a "
                     "person ends it:  docker rm -f %s"
                     % (image, age // 86400,
                        self.cfg["docker_container_alert_days"], name),
                     standing=True)

    def _docker_path(self):
        """Where docker is, resolved ONCE and the same way for every caller.

        THIS WAS TWO RESOLVERS AND THEY DISAGREED, which the suite caught: the
        prune arm resolved through PATH and this one only checked three absolute
        locations, so a test's stubbed `docker` was honored by one arm and ignored
        by the other. Two answers to "where is docker" is one more than there can
        usefully be.

        shutil.which, because it honors PATH and is the documented way. The prune
        arm used to shell out to `command -v`, which works here only because macOS
        ships /usr/bin/command as a real binary — checked, it does — and would fail
        on a host that does not.
        """
        if "_dockerbin" in self.__dict__:
            return self.__dict__["_dockerbin"]
        import shutil
        found = shutil.which("docker")
        if not found:
            for cand in ("/opt/homebrew/bin/docker", "/usr/local/bin/docker",
                         "/usr/bin/docker"):
                if os.access(cand, os.X_OK):
                    found = cand
                    break
        self.__dict__["_dockerbin"] = found
        return found

    def _docker_alive(self, docker):
        if "_dockerup" in self.__dict__:
            return self.__dict__["_dockerup"]
        try:
            p = subprocess.run([docker, "info", "--format", "{{.ServerVersion}}"],
                               capture_output=True, text=True, timeout=20)
            up = p.returncode == 0 and bool(p.stdout.strip())
        except (OSError, subprocess.TimeoutExpired):
            up = False
        self.__dict__["_dockerup"] = up
        return up

    def apply(self, log_path):
        """Delete, one line per deletion, freed bytes counted from what the
        walk measured. Returns (deleted, freed, failures)."""
        import shutil
        deleted, freed, failures = 0, 0, []
        lines = []
        stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        # DEEPEST FIRST. A husk and the sessions inside it are both on the
        # plan; removing the husk first would leave every child reported as a
        # FAILED deletion that had in fact just succeeded, which is the one
        # kind of log line nobody can act on.
        if self.scope is None:
            for entry in self.entries:
                if entry.action == DELETE and self.named_scratch(entry.path):
                    entry.action = KEEP
                    entry.why = "named scratch is collected only by its successful landing"
        order = sorted((e for e in self.entries if e.action == DELETE),
                       key=lambda e: (-e.path.count(os.sep), e.path))
        # §54 ADDENDUM 4, AND IT RUNS BEFORE THE FIRST rmtree. Any test instance
        # of the app rooted in something about to be deleted is quit first.
        #
        # THE ORDER IS THE POINT. Without it the sweeper deletes a dead session's
        # scratchpad while an app instance is still running on it -- the claude-
        # roots arm has no open-handle check at all, by design, because a dead
        # session's scratch is garbage whoever is holding it. The result was an
        # orphan window running against a HOME that no longer exists: garbage
        # that has been made HARDER to account for, not collected.
        #
        # Scoped to the planned paths with include_declared=False, so this can
        # only ever quit an instance living inside something this run has already
        # decided to delete. It never reaches a live peer's instance.
        # A cargo-units row names a profile directory that is NOT deleted, only some of its
        # units, so an app running from it is not inside anything being removed.
        # A Codex worktree is closed by git after a recheck that keeps any with a
        # process in it, so no app instance there is quit on the plan's word.
        for line in self.collect_test_instances([e.path for e in order
                                                 if e.klass not in ("cargo-units",
                                                                    "codex-worktree")], stamp):
            lines.append(line)
            if " FAILED test-instance " in line:
                failures.append(line.split("error=", 1)[-1])
        # §54 FOR SIMULATORS AND EMULATORS. A device a killed test booted is
        # removed once its owner is proven gone (scripts/lib/testdevices.py);
        # one that will not close, or whose owner cannot be proven while it
        # runs, is recorded in its own failure file for the MASSIVE ALERT. It
        # is not added to `failures` here: that record is keyed by path and
        # would resolve a device row on its next read.
        #
        # NOT IN A LAND'S SCOPED SWEEP, nor the Docker arm below: neither is
        # attributable to the agent being landed, and a land takes only what is
        # (scan_agent). The scheduled sweep still runs both.
        if self.scope is None:
            lines.extend(self.collect_test_devices(stamp))
        for e in order:
            if e.klass == "codex-worktree":
                # CLOSED BY GIT, NOT rmtree, after every check is repeated now
                # (lib/codex_worktrees.py close): a checkout that changed since the
                # plan is kept, and git itself refuses a dirty one or an unmerged branch.
                kept, err = codex_worktrees().close(
                    e.path, getattr(e, "close_info", {}), self.ws, self.codex_app_root(),
                    lambda: self.codex_attachments()[0],
                    lambda: self.ws._process_cwds(strict=True),
                    lambda: codex_worktrees().open_paths(env=_ps_env()))
                if kept:
                    lines.append("%s KEPT %s class=%s why=%s" % (stamp, e.path, e.klass, kept))
                    continue
                if err:
                    failures.append("%s: %s" % (e.path, err))
                    lines.append("%s FAILED %s class=%s error=%s" % (stamp, e.path, e.klass, err))
                    continue
                deleted += 1
                freed += e.size
                lines.append("%s DELETED %s bytes=%d class=%s why=%s"
                             % (stamp, e.path, e.size, e.klass, e.why))
                continue
            if e.klass == "cargo-units":
                # UNITS, NOT THE DIRECTORY. Re-measured and deleted under Cargo's own
                # lock (lib/build_caches.py), so a build that started since the plan
                # keeps every unit, and a unit it read since the plan is kept.
                n, got, bad, why_not = build_caches().delete_units(
                    e.path, e.unit_keys, self.cfg["build_cache_idle_hours"] * 3600)
                if why_not:
                    lines.append("%s KEPT %s class=%s why=%s" % (stamp, e.path, e.klass, why_not))
                    continue
                if n:
                    deleted += 1
                    freed += got
                    lines.append("%s DELETED %s bytes=%d class=%s units=%d why=%s"
                                 % (stamp, e.path, got, e.klass, n, e.why))
                for f in bad:
                    failures.append(f)
                    lines.append("%s FAILED %s class=%s error=%s" % (stamp, e.path, e.klass, f))
                continue
            try:
                # A DIRECTORY IS THE ONLY THING rmtree CAN TAKE. THE TEST USED TO
                # BE ITS COMPLEMENT AND THAT IS 200 OF THE 218 PERMANENT ALERTS.
                #
                # It was `islink or isfile -> unlink, ELSE rmtree`, so every path
                # that was neither a link nor a REGULAR file went to rmtree — and
                # a unix socket and a FIFO are neither. Measured in the standing
                # failure ledger on 2026-09-19, 218 rows:
                #
                #   198  [Errno 102] Operation not supported on socket
                #          (197 of them srt-mux-*.sock, 0 bytes, 10 days old)
                #     2  [Errno 20] Not a directory        (clr-debug-pipe-*)
                #    18  [Errno 1]  Operation not permitted (the OS's own, above)
                #
                # Every one of the 200 had ALREADY PASSED every wall, the age
                # floor and the open-handle check: the reaper had correctly
                # decided they were abandoned garbage, and then could not carry
                # the decision out because of the shape of the branch. One
                # missing `os.unlink` produced two hundred MASSIVE ALERT lines
                # that no person could ever clear, every six hours, for a day.
                #
                # ASKING WHAT IT IS, RATHER THAN WHAT IT IS NOT: a directory that
                # is not a symlink gets rmtree, and ANYTHING else — regular file,
                # symlink, socket, FIFO, device node — gets unlink, which is the
                # call that removes a name from a directory whatever kind of
                # thing the name refers to.
                if os.path.isdir(e.path) and not os.path.islink(e.path):
                    shutil.rmtree(e.path)
                else:
                    os.unlink(e.path)
            except OSError as exc:
                # ONE RETRY, AFTER MAKING THE TREE WRITABLE. Measured on the
                # first real run of the widened sweep: 5 of 37,146 deletions
                # failed with EPERM/EACCES, every one of them a git-worktree
                # fixture whose directories a harness had left at a mode that
                # forbids unlinking their contents — e.g.
                # ws-fourteen.*/entity/.claude/worktrees/agent-*.
                #
                # A MODE BIT IS NOT A SAFETY BOUNDARY HERE, and that is the
                # whole justification. By the time a path reaches this loop it
                # has already passed all four walls: it is inside a declared
                # root, it is not a registered workspace, it is not on the
                # never-touch list, and either it is not a checkout or it is a
                # declared legacy fixture. The decision to delete is already
                # made; a directory that forbids its own removal is an obstacle
                # to carrying it out, not a reason to reconsider.
                #
                # WHY IT MATTERS ENOUGH TO DO AT ALL: under the CEO's rule every
                # failure here becomes a MASSIVE ALERT. Five unfixable alerts
                # arriving every six hours forever would train their reader to
                # skip them, and then the alert that means something arrives
                # into a habit of not reading. Removing avoidable failures is
                # how the alert stays worth reading.
                #
                # A SECOND FAILURE IS A REAL FAILURE and is reported exactly as
                # before. Nothing here swallows anything: the retry either works
                # or the original error is logged with the retry's own error
                # beside it.
                retry_exc = None
                if getattr(exc, "errno", None) in (1, 13) \
                        and e.klass in _UNSTICKABLE_CLASSES:
                    try:
                        cleared = _make_writable(e.path)
                        # The same is-it-a-directory test as the first attempt.
                        # Two spellings of one question is how a fix reaches one
                        # of them and not the other.
                        if os.path.isdir(e.path) and not os.path.islink(e.path):
                            shutil.rmtree(e.path)
                        else:
                            os.unlink(e.path)
                        deleted += 1
                        freed += e.size
                        lines.append("%s DELETED %s bytes=%d class=%s why=%s "
                                     "note=%s"
                                     % (stamp, e.path, e.size, e.klass, e.why,
                                        "removed on the second attempt after "
                                        "making the tree writable and clearing "
                                        "%d uchg flag(s); first attempt: %s"
                                        % (cleared, exc)))
                        continue
                    except OSError as exc2:
                        retry_exc = exc2
                failures.append("%s: %s" % (e.path, retry_exc or exc))
                lines.append("%s FAILED %s bytes=%d class=%s why=%s error=%s%s"
                             % (stamp, e.path, e.size, e.klass, e.why, exc,
                                "" if retry_exc is None
                                else " retry_error=%s" % retry_exc))
                continue
            deleted += 1
            freed += e.size
            lines.append("%s DELETED %s bytes=%d class=%s why=%s"
                         % (stamp, e.path, e.size, e.klass, e.why))
        # THE DOCKER ARM RUNS AFTER THE FILESYSTEM SWEEP, NEVER BEFORE. The
        # sweep's plan was measured before any deletion so that --dry-run and
        # --apply print byte-identical plans; asking a daemon to prune first
        # would change the disk underneath that guarantee.
        for line in (self.prune_docker() if self.scope is None else []):
            lines.append(line)
            if " FAILED docker " in line:
                failures.append(line.split("error=", 1)[-1])
            else:
                # Counted into `freed` so the verdict line's arithmetic is the
                # whole truth about what the run reclaimed, on every volume.
                try:
                    freed += int(line.split("bytes=", 1)[1].split()[0])
                    deleted += 1
                except (IndexError, ValueError):
                    pass
        lines.append("%s verdict: deleted=%d freed=%d freed_human=%s "
                     "undecidable=%d failures=%d%s"
                     % (stamp, deleted, freed, human(freed),
                        self.counts()[1], len(failures),
                        "" if self.scope is None else
                        " scope=agent:%s unattributed=%d"
                        % (self.scope.get("name") or "?", self.unattributed)))
        write_log(log_path, lines)
        self._record_failures(failures, stamp)
        return deleted, freed, failures

    def collect_test_devices(self, stamp):
        """Log lines for one pass of testdevices.collect(apply=True). Never
        raises: an unloadable collector persists an alert and the sweep goes on."""
        try:
            here = os.path.dirname(os.path.abspath(__file__))
            if here not in sys.path:
                sys.path.insert(0, here)
            import testdevices
            res = testdevices.collect(apply=True)
        except Exception as exc:
            try:
                from testdevice_alerts import record_failure
                record_failure("collector unavailable: %s" % exc)
            except Exception as alert_error:
                return ["%s FAILED test-device collector and its alert writer: %s; %s" % (stamp, exc, alert_error)]
            return ["%s FAILED test-device collector unavailable: %s" % (stamp, str(exc)[:160])]
        out = ["%s NOTE test-devices %s" % (stamp, n) for n in res.get("notes") or []]
        for key, word in (("collected", "REMOVED"), ("survivors", "FAILED"),
                          ("undecided", "KEPT"), ("deferred", "DEFERRED")):
            for d in res.get(key) or []:
                out.append("%s %s test-device %s %s name=%r why=%s"
                           % (stamp, word, d["kind"], d["id"], d["name"], d.get("how") or d.get("why")))
        return out

    def collect_test_instances(self, paths, stamp):
        """Quit every test instance rooted in `paths`. Returns log lines.

        A SURVIVOR IS A FAILED COLLECTION and is logged as `FAILED
        test-instance`, which apply() turns into a failure and therefore into the
        MASSIVE ALERT. A window that will not close is precisely the "clean-up
        failed" branch of §54: Rich is told, and he ends it by hand.

        Never raises. If the collector is unavailable this returns one note and
        the sweep proceeds -- the same trade stop_containers makes in the lander.
        """
        mod = self.appinstances()
        if mod is None or not paths:
            return []
        try:
            res = mod.collect_and_record(
                roots=mod.RootSet(extra=paths, include_declared=False))
        except Exception as exc:
            return ["%s NOTE test-instance collector unavailable: %s"
                    % (stamp, str(exc)[:160])]
        out = []
        for d in res.get("collected") or []:
            out.append("%s QUIT test-instance pid=%d root=%s note=%s"
                       % (stamp, d["pid"], d.get("root"), d.get("how")))
        for d in res.get("undecided") or []:
            # Not a failure: an instance this cannot place is LEFT RUNNING on
            # purpose, and saying so in the log is how that stays visible.
            out.append("%s KEPT test-instance pid=%d why=%s"
                       % (stamp, d["pid"], d.get("why")))
        for d in res.get("survivors") or []:
            out.append("%s FAILED test-instance pid=%d root=%s error=%s"
                       % (stamp, d["pid"], d.get("root"),
                          "a test instance of the app (pid %d) could not be "
                          "quit: %s" % (d["pid"], d.get("how"))))
        return out

    def _record_failures(self, failures, stamp):
        """Carry this run's failures forward, and drop what is now gone.

        THE DROP IS AS IMPORTANT AS THE RECORD. A failure file that only ever
        grew would turn into a permanent alert about paths that were cleaned up
        weeks ago, and a permanent alert is one nobody reads — which is how the
        real one gets missed. Anything that no longer exists on disk leaves the
        file on the next run, whether this program removed it or a person did.
        """
        state = failures_path()
        rows = read_failures(state)
        # Everything previously recorded that has since vanished is resolved.
        for path in list(rows):
            if not os.path.exists(path):
                del rows[path]
        # ...AND SO IS EVERYTHING THIS PASS PROVED WAS NEVER OURS. A row here is
        # a standing instruction to a person: "the clean-up failed, go and delete
        # this by hand." Eighteen of them named directories the kernel forbids
        # anybody at this privilege from removing, so the instruction could not
        # be carried out, and it was re-issued every six hours for a day.
        # Resolving them is not hiding a failure; it is withdrawing a claim that
        # was wrong.
        for path in self._unfailable:
            rows.pop(path, None)
        for entry in failures:
            path, _, err = entry.partition(": ")
            if not path:
                continue
            if system_protected(path):
                # Belt and braces: nothing the kernel owns should have reached
                # the delete loop at all, and if it ever does it must not be
                # written into a file that asks a person to remove it.
                continue
            prev = rows.get(path) or {}
            rows[path] = {
                # FIRST SEEN IS PRESERVED across runs. How long something has
                # been unremovable is the single most useful fact in the alert:
                # a failure five minutes old may be a running process, and one
                # five days old is something a person has to look at.
                "first": prev.get("first") or stamp,
                "last": stamp,
                "error": err or prev.get("error") or "?",
                "attempts": int(prev.get("attempts") or 0) + 1,
            }
        if rows:
            write_failures(state, rows)
        elif os.path.exists(state):
            # No standing failures: remove the file rather than leave an empty
            # object behind, so its mere existence answers "is anything stuck".
            try:
                os.unlink(state)
            except OSError:
                write_failures(state, {})


def _hours_from_until(text):
    """'720h' -> 720. None if it is not a form this understands.

    Deliberately narrow: `until` accepts several spellings from Docker, and this
    only has to read the one THIS config declares. A duration it cannot read is
    reported as a failure rather than defaulted, because a default here would be
    an age limit nobody chose on a command that deletes.
    """
    t = (text or "").strip().lower()
    if t.endswith("h"):
        t = t[:-1]
    try:
        return float(t)
    except ValueError:
        return None


def _rfc3339_epoch(text):
    """'2026-09-10T11:25:40Z' -> epoch seconds, or None.

    `docker volume inspect --format '{{.CreatedAt}}'` answers in this form on this
    machine, measured. Parsed with calendar.timegm and never with mktime, for the
    reason lstart_epoch carries at length: the string is UTC and mktime reads local.
    """
    t = (text or "").strip()
    for fmt in ("%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%dT%H:%M:%S"):
        try:
            return calendar.timegm(time.strptime(t[:19], fmt))
        except ValueError:
            continue
    return None


def _docker_ps_epoch(text):
    """`docker ps --format '{{.CreatedAt}}'` -> epoch seconds, or None.

    Its shape is '2026-09-10 11:25:40 +0100 BST' — LOCAL time with an explicit
    numeric offset, which is a different format from the volume inspect one above
    and is why this is a second function rather than a second regex in the first.
    The numeric offset is what makes it unambiguous; the trailing zone NAME is
    ignored because names are not unique across the world.
    """
    parts = (text or "").strip().split()
    if len(parts) < 3:
        return None
    try:
        base = calendar.timegm(time.strptime(" ".join(parts[:2]),
                                             "%Y-%m-%d %H:%M:%S"))
    except ValueError:
        return None
    off = parts[2]
    if len(off) != 5 or off[0] not in "+-":
        return None
    try:
        mins = int(off[1:3]) * 60 + int(off[3:5])
    except ValueError:
        return None
    return base - mins * 60 if off[0] == "+" else base + mins * 60


_RECLAIM_RE = re.compile(
    r"Total reclaimed space:\s*([0-9.]+)\s*([KMGT]?i?B)", re.IGNORECASE)

_UNITS = {"b": 1, "kb": 1000, "mb": 1000 ** 2, "gb": 1000 ** 3,
          "tb": 1000 ** 4, "kib": 1024, "mib": 1024 ** 2,
          "gib": 1024 ** 3, "tib": 1024 ** 4}


def _docker_reclaimed(text):
    """Bytes out of docker's `Total reclaimed space: 1.7GB` trailer.

    0 when it cannot be read, NEVER a guess: this number goes into a log line
    that says how much space came back, and an invented one would make the
    log's arithmetic a fiction. Docker prints decimal units (GB = 10^9), which
    is why the table is not powers of two.
    """
    m = _RECLAIM_RE.search(text or "")
    if not m:
        return 0
    try:
        val = float(m.group(1))
    except ValueError:
        return 0
    return int(val * _UNITS.get(m.group(2).lower(), 1))


def _docker_removed_names(text):
    """The items docker says it removed, one string each.

    Docker's prune output is a `Deleted Images:` / `Deleted Containers:` /
    `Deleted build cache objects:` header followed by one item per line and a
    `Total reclaimed space:` trailer. Only the `untagged:` and `deleted:` lines
    carry a name worth logging; bare digests under a build-cache prune are
    hundreds of lines of noise, so they are counted rather than listed.
    """
    names, digests = [], 0
    for line in (text or "").splitlines():
        line = line.strip()
        if not line or line.endswith(":") or line.startswith("Total reclaimed"):
            continue
        low = line.lower()
        if low.startswith("untagged:") or low.startswith("deleted:"):
            val = line.split(":", 1)[1].strip()
            if val.startswith("sha256:"):
                digests += 1
            else:
                names.append(val)
        elif len(line) >= 12 and " " not in line:
            digests += 1
    if digests:
        names.append("%d untagged layer/cache object(s)" % digests)
    return names


# The classes where an obstacle to removal may be CLEARED rather than reported.
#
# Narrow on purpose, and the omissions are the point: a claude session's
# scratchpad, a husk, an orphan file and the nightly's releases are all things a
# PERSON may have deliberately protected, so an obstacle there is reported and a
# human decides. The four below are machine-made scratch whose maker is provably
# gone — the allocator root, the declared harness families, and a path this
# program already tried to delete once.
_UNSTICKABLE_CLASSES = frozenset((
    "scratch-alloc", "tmp-legacy", "tmp-workspace", "standing-failure",
    # tmp-unknown joins them 2026-09-18: by the time an entry of that class is on
    # the delete list, NO RUNNING SESSION PROCESS EXISTED WHEN IT WAS LAST
    # WRITTEN, nothing holds it open, and it has sat untouched for days. A mode
    # bit on a tree that old, in a temp root, is a harness's leftover and not a
    # person's protection — and every failure here becomes a MASSIVE ALERT, so an
    # avoidable one costs the credibility of the unavoidable ones.
    "tmp-unknown",
    # Build caches join them 2026-10-08: Swift Package Manager leaves its checkouts
    # read-only inside an iOS build folder, and the folder is already proven orphaned.
    "build-cache", "build-cache-trash"))

UF_IMMUTABLE = 0x00000002       # uchg. stat.UF_IMMUTABLE, named here so the
                                # constant is readable beside its use.


def _make_writable(path):
    """Clear what stops a removal: directory modes, and the uchg flag.

    MODES: directories only, owner bit only, u+rwx — never a mode handed to
    anybody else. A read-only FILE is removable already; what stops an unlink is
    the mode of the DIRECTORY holding it, so widening file modes buys nothing.

    THE uchg FLAG, and why clearing it is right here. Measured 2026-09-18: two
    trees resisted both `shutil.rmtree` AND `/bin/rm -rf` with EPERM, and the
    cause was a single file — `.claude/worktrees/.parked-agent-*/pinned.txt`,
    `chflags uchg`, flag 0x2 — left behind by a harness that tests what happens
    to a pinned file. Its mode was 644 and its owner was the invoking user; only
    the flag stood in the way.

    A uchg FLAG IS NOT A SAFETY BOUNDARY IN A DECLARED HARNESS SANDBOX. By the
    time a path reaches here it has passed all four walls, its owner is provably
    gone, and it has sat untouched for hours (118 in the measured case). The
    alternative is a permanent recurring alert about a test fixture's leftover
    flag — and under the CEO's rule every failure becomes a MASSIVE ALERT, so an
    avoidable one costs the credibility of the unavoidable ones.

    IT IS STILL BOUNDED: only for the classes in _UNSTICKABLE_CLASSES, never for
    a session's scratchpad or the nightly, where a person may have protected
    something deliberately and gets asked instead. And every clearing is logged.

    Symlinks are never followed: chmod or chflags through a symlink would reach
    outside the tree, which is the thing every wall exists to prevent.
    """
    cleared = 0
    def _unstick(p, is_dir):
        nonlocal cleared
        if os.path.islink(p):
            return
        try:
            st = os.lstat(p)
        except OSError:
            return
        if is_dir:
            try:
                os.chmod(p, st.st_mode | 0o700)
            except OSError:
                pass
        flags = getattr(st, "st_flags", 0)
        if flags & UF_IMMUTABLE:
            try:
                os.chflags(p, flags & ~UF_IMMUTABLE)
                cleared += 1
            except (OSError, AttributeError):
                pass

    _unstick(path, os.path.isdir(path) and not os.path.islink(path))
    for root, dirs, files in os.walk(path, topdown=True, followlinks=False):
        for d in dirs:
            _unstick(os.path.join(root, d), True)
        for f in files:
            _unstick(os.path.join(root, f), False)
    return cleared


# The deletion log keeps one previous file, as quota_watch.py's poll log does, so
# the cleanup's own record cannot become a pile (cleanup PRD R4). Measured
# 2026-10-23: 63 MB, never trimmed.
LOG_MAX_BYTES = 16 * 1024 * 1024


def rotate_log(path, limit=LOG_MAX_BYTES):
    try:
        if os.path.getsize(path) > limit:
            os.replace(path, path + ".1")
    except OSError:
        pass


def write_log(path, lines):
    rotate_log(path)
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "a", encoding="utf-8") as fh:
            for line in lines:
                fh.write(line + "\n")
    except OSError as exc:                                    # pragma: no cover
        sys.stderr.write("scratch-reaper: could not write the log at %s: %s\n"
                         % (path, exc))


# ---------------------------------------------------------------------------
# configuration — declared, never defaulted
# ---------------------------------------------------------------------------

def config_from_env(process_only=False):
    try:
        if not process_only:
            machine_settings.apply()
        named_root = str(machine_settings.agent_root()) if not process_only else ""
        cache_root, cargo_roots = build_caches().roots() if not process_only else ("", [])
        bulk = machine_settings.load().get("RICHOS_BULK_ROOT") if not process_only else ""
        bulk_root = str(machine_settings.validate(bulk, require_directory=True)) if bulk else ""
    except (ValueError, OSError) as exc:
        raise Fatal(str(exc)) from exc
    # THE PRODUCERS' OWN TEMP ROOTS on the bulk drive: the nightly gates' TMPDIR
    # (nightly-local.py gate_tmpdir), Codex's TMPDIR, and the autocheck suite's
    # fixture parent. Declared relative to RICHOS_BULK_ROOT and swept exactly like
    # a shared temp root. Measured 2026-10-23: 13 GB of test fixtures sat in them
    # and the scheduled job, whose TMPDIR is launchd's, never looked.
    producer_roots = []
    for rel in (os.environ.get("SCRATCH_BULK_TMP_ROOTS") or "").split():
        if not bulk_root:
            break
        real = os.path.realpath(os.path.join(bulk_root, rel))
        if os.path.isabs(rel) or not inside(real, bulk_root) or real == bulk_root:
            raise Fatal("SCRATCH_BULK_TMP_ROOTS names %r, which is not a directory "
                        "below RICHOS_BULK_ROOT (%s)." % (rel, bulk_root))
        producer_roots.append(real)
    # CODEX'S OWN WORKTREE ROOTS (cleanup PRD A6): each absolute (the Codex app's
    # root under its home) or relative to RICHOS_BULK_ROOT. Empty fallback: an
    # engine whose config predates the keys closes no Codex worktree.
    codex_roots = []
    for entry in (os.environ.get("SCRATCH_CODEX_WORKTREE_ROOTS") or "").split():
        if os.path.isabs(entry):
            codex_roots.append(os.path.realpath(entry))
            continue
        if not bulk_root:
            continue
        real = os.path.realpath(os.path.join(bulk_root, entry))
        if not inside(real, bulk_root) or real == bulk_root:
            raise Fatal("SCRATCH_CODEX_WORKTREE_ROOTS names %r, which is not a directory "
                        "below RICHOS_BULK_ROOT (%s)." % (entry, bulk_root))
        codex_roots.append(real)
    codex_home = (os.environ.get("SCRATCH_CODEX_HOME") or "").strip()
    def need(name):
        v = (os.environ.get(name) or "").strip()
        if not v:
            raise Fatal(
                "%s is not declared. Every threshold this program uses is "
                "declared in orchestration.config with the reason beside it; "
                "a value taken from a tool's default is a value nobody chose."
                % name)
        return v

    def number(name):
        v = need(name)
        try:
            return int(v)
        except ValueError:
            raise Fatal("%s is declared as %r, which is not a number." % (name, v))

    uid = str(os.getuid())
    roots = [r.replace("%u", uid) for r in need("SCRATCH_CLAUDE_ROOTS").split()]
    seen, ordered = set(), []
    for r in roots:
        real = os.path.realpath(r)
        if real in seen:
            continue
        seen.add(real)
        ordered.append(real)
    # The keys added on 2026-09-18 are read with a DECLARED FALLBACK rather
    # than need(), and the asymmetry is deliberate rather than laziness.
    #
    # need() is right for a THRESHOLD: a number nobody chose is a number nobody
    # can defend, so refusing to run is better than inventing one. It is wrong
    # for COVERAGE. An engine whose config predates this change would, under
    # need(), refuse to sweep at all — and a reaper that exits 2 because a new
    # key is missing reclaims nothing, which is strictly worse than the
    # partial coverage it had yesterday. The failure modes are not symmetric:
    # a missing threshold risks deleting to the wrong number, a missing
    # coverage key risks deleting less than it could.
    #
    # Each fallback below is therefore the CONSERVATIVE value — the one that
    # sweeps nothing new — except SCRATCH_ROOT_NAME, whose fallback is the
    # allocator's own fixed root. That one is safe to assume because
    # scripts/lib/scratch.sh hard-codes the same name: the two would have to
    # disagree for it to be wrong, and they are edited together.
    def opt(name, default):
        v = (os.environ.get(name) or "").strip()
        return v if v else default

    def opt_number(name, default):
        v = opt(name, "")
        if not v:
            return default
        try:
            return int(v)
        except ValueError:
            raise Fatal("%s is declared as %r, which is not a number."
                        % (name, v))

    return {
        "claude_roots": ordered,
        "tmp_patterns": need("SCRATCH_TMP_PATTERNS").split(),
        "age_floor_minutes": number("SCRATCH_AGE_FLOOR_MINUTES"),
        "nightly_dir": os.path.expanduser(need("SCRATCH_NIGHTLY_DIR")),
        "nightly_keep": number("SCRATCH_NIGHTLY_KEEP"),
        "notice_bytes": number("SCRATCH_NOTICE_BYTES"),
        "session_process_names": sorted(session_process_names()),
        "scratch_root_name": opt("SCRATCH_ROOT_NAME", "richos-scratch"),
        "default_ttl_minutes": opt_number("SCRATCH_DEFAULT_TTL_MINUTES", 360),
        "legacy_tmp_patterns": opt("SCRATCH_LEGACY_TMP_PATTERNS", "").split(),
        "legacy_age_hours": opt_number("SCRATCH_LEGACY_AGE_HOURS", 2),
        "legacy_git_is_fixture":
            opt("SCRATCH_LEGACY_GIT_IS_FIXTURE", "0") == "1",
        "docker_prune": opt("SCRATCH_DOCKER_PRUNE", "0") == "1",
        "docker_keep_storage": opt("SCRATCH_DOCKER_KEEP_STORAGE", "20GB"),
        "docker_until": opt("SCRATCH_DOCKER_UNTIL", "720h"),
        # --- the deny-by-default temp arm, added 2026-09-18 (Frank D1/D2) ----
        # OFF is the conservative fallback, for the reason opt() exists at all:
        # an engine whose config predates this key sweeps exactly what it swept
        # yesterday rather than suddenly sweeping every name under $TMPDIR.
        "deny_by_default": opt("SCRATCH_TMP_DENY_BY_DEFAULT", "0") == "1",
        "shared_tmp_roots": [os.path.realpath(p) for p in
                             opt("SCRATCH_SHARED_TMP_ROOTS", "").split()]
                            + [p for p in producer_roots
                               if p not in [os.path.realpath(q) for q in
                                            opt("SCRATCH_SHARED_TMP_ROOTS", "").split()]],
        "producer_tmp_roots": producer_roots,
        "codex_worktree_roots": codex_roots,
        "codex_home": os.path.realpath(os.path.expanduser(codex_home)) if codex_home else "",
        # Allocator labels whose producers make only fixture repositories (PRD R1).
        # Empty fallback: no repository in an allocation is ever taken as a fixture.
        "fixture_repo_labels": opt("SCRATCH_FIXTURE_REPO_LABELS", "").split(),
        "foreign_patterns": opt("SCRATCH_FOREIGN_PATTERNS", "").split(),
        "unknown_age_hours": opt_number("SCRATCH_UNKNOWN_AGE_HOURS", 72),
        "unknown_git_is_fixture":
            opt("SCRATCH_UNKNOWN_GIT_IS_FIXTURE", "0") == "1",
        # The two notice thresholds the garbage alarm uses. Their fallbacks are
        # the CONSERVATIVE direction for a notice, which is to say the LOUD one:
        # a machine whose config predates these keys is told about garbage it
        # cannot collect rather than kept quiet about it.
        "skipped_notice_bytes":
            opt_number("SCRATCH_SKIPPED_NOTICE_BYTES", 1024 ** 3),
        "undecidable_notice_bytes":
            opt_number("SCRATCH_UNDECIDABLE_NOTICE_BYTES", 64 * 1024 ** 2),
        # D11. The fallback is 0, which DISABLES the floor test and leaves the
        # other two — an engine whose config predates this key keeps exactly the
        # behavior it had rather than acquiring a number nobody declared.
        "min_owner_pid": opt_number("SCRATCH_MIN_OWNER_PID", 0),
        # D3. Empty fallbacks: a campaign root is nominated BY DECLARATION and
        # never by inference, so an engine whose config predates these keys
        # nominates nothing.
        "campaign_parent": os.environ.get("RICHOS_CAMPAIGN_PARENT") or opt("SCRATCH_CAMPAIGN_PARENT", ""),
        "campaign_roots": opt("SCRATCH_CAMPAIGN_ROOTS", "").split(),
        "campaign_retention_days":
            opt_number("SCRATCH_CAMPAIGN_RETENTION_DAYS", 7),
        # D14. Empty fallback: no image is a throwaway unless somebody says so.
        "docker_throwaway_images":
            opt("SCRATCH_DOCKER_THROWAWAY_IMAGES", "").split(),
        "docker_container_alert_days":
            opt_number("SCRATCH_DOCKER_CONTAINER_ALERT_DAYS", 7),
        # Where the app keeps its entity registry, RELATIVE TO A HOME. Declared
        # rather than hard-coded because it is the app's shape and not this
        # program's; the fallback is the shipped one, so an engine whose config
        # predates the key still protects a registered repository.
        "app_registry_relpath":
            opt("SCRATCH_APP_REGISTRY_RELPATH",
                "Library/Application Support/com.richos.app/entities.json"),
        # Where each agent keeps a directory named for itself (2026-10-01). Read
        # ONLY by a land's scoped sweep (--agent), which deletes <root>/<name>
        # for the agent being landed. Empty fallback: an engine whose config
        # predates the key deletes no named directory there.
        "agent_roots": [named_root],
        # What an automatic land kept (2026-10-04). Empty fallback: an engine
        # whose config predates the key deletes nothing there.
        "kept_dir": os.path.realpath(os.path.expanduser(opt("SCRATCH_KEPT_DIR", "")))
                    if opt("SCRATCH_KEPT_DIR", "") else "",
        "kept_retention_days": opt_number("SCRATCH_KEPT_RETENTION_DAYS", 7),
        # Build caches on the external drive (2026-10-08, lib/build_caches.py). Empty
        # fallbacks: an engine whose config predates these keys deletes no build cache.
        "build_cache_root": cache_root,
        "cargo_target_roots": cargo_roots,
        "checkout_parents": [],  # build_caches discovers actual registered Git parents
        "build_cache_orphan_hours": opt_number("SCRATCH_BUILD_CACHE_ORPHAN_HOURS", 6),
        "build_cache_idle_hours": opt_number("SCRATCH_BUILD_CACHE_IDLE_HOURS", 72),
    }


def state_base():
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state")


def default_log():
    return os.environ.get("SCRATCH_REAPER_LOG") \
        or os.path.join(state_base(), "scratch-reaper.log")


def default_state():
    """What the LAST --apply did. The SessionStart notice reads it to answer
    the question the reclaimable byte count cannot: is the scheduled job still
    running at all? A scheduled deleter that quietly stopped looks exactly
    like a machine with no garbage on it."""
    return os.environ.get("SCRATCH_REAPER_STATE") \
        or os.path.join(state_base(), "scratch-reaper-state.json")


# THE ONE CONTEXT THAT PUBLISHES CLEANUP HEALTH (cleanup PRD R3/R4): the installed
# launchd job, whether it fired at a calendar slot or by `launchctl kickstart`.
# scratch-reaper.sh reads launchd's XPC_SERVICE_NAME and hands the fact on as
# SCRATCH_REAPER_SCHEDULED (why the shell and not this program: see there). An
# interactive --apply prints its result and records failed deletions, but never
# writes or clears the unattended result: on 2026-10-23 a manual run at 16:42Z
# overwrote the state the failing scheduled runs had published, and the record
# then described a run the scheduler never made.
def unattended():
    return os.environ.get("SCRATCH_REAPER_SCHEDULED") == "1"


def unresolved_rows(reaper, failures):
    """What kept this run from being a completed cleanup, grouped by the inspected
    root and class it happened in, each with one example and its reason. An entry
    a verified rule KEEPs is not here; an undecided one, a failed deletion and an
    entry the run never reached are."""
    roots = sorted(set(os.path.realpath(r) for r in reaper.roots), key=len, reverse=True)

    def scope_of(path):
        real = os.path.realpath(path)
        return next((r for r in roots if inside(real, r)), real)

    groups = {}
    for e in reaper.entries:
        if e.action != INDETERMINATE:
            continue
        key = (scope_of(e.path), e.klass)
        g = groups.setdefault(key, {"scope": key[0], "class": e.klass, "count": 0,
                                    "bytes": 0, "example": e.path, "why": e.why})
        g["count"] += 1
        g["bytes"] += e.size
    out = []
    for g in groups.values():
        g["cause"] = ("%d undecided %s entr%s (%s), e.g. %s: %s"
                      % (g["count"], g["class"], "y" if g["count"] == 1 else "ies",
                         human(g["bytes"]), g.pop("example"), g.pop("why")[:200]))
        out.append(g)
    # A FAILED DELETION IS NAMED BY THE ROOT IT HAPPENED IN, like everything above:
    # the watchdog keeps an action open until a healthy run has inspected every
    # root that failed, and a label that is not a path could never be inspected.
    # Each failure reads "<path>: <error>" (apply()).
    failed = {}
    for f in failures:
        scope = scope_of(str(f).split(": ", 1)[0])
        failed.setdefault(scope, []).append(str(f))
    for scope, rows in failed.items():
        out.append({"scope": scope, "class": "deletion", "count": len(rows),
                    "cause": "%d deletion(s) failed, e.g. %s" % (len(rows), rows[0][:200])})
    if reaper.deferred_skipped:
        out.append({"scope": "deny-by-default arm", "count": reaper.deferred_skipped,
                    "cause": "%d entries were not reached before the run's budget ran out"
                             % reaper.deferred_skipped})
    return sorted(out, key=lambda r: (r["scope"], r.get("class", "")))


def publish_refusal(cause):
    """A scheduled run that refused to run publishes that, keeping the time of the
    last run the scheduled job completed (a manual run's time is never kept)."""
    prev = read_failures(default_state())
    payload = {k: prev[k] for k in ("last_apply", "last_apply_epoch")
               if prev.get("source") == "launchd" and k in prev}
    payload.update({"source": "launchd", "outcome": "unresolved",
                    "result_epoch": int(time.time()), "roots": [],
                    "unresolved": [{"scope": "run", "cause": str(cause)[:300]}]})
    write_state(default_state(), payload)


def write_state(path, payload):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, indent=1, sort_keys=True)
        os.replace(tmp, path)
    except OSError as exc:                                    # pragma: no cover
        sys.stderr.write("scratch-reaper: could not write %s: %s\n" % (path, exc))


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True, description=__doc__)
    ap.add_argument("--apply", action="store_true",
                    help="delete. Without it nothing is deleted.")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--verbose", action="store_true",
                    help="show the KEEP rows too — the whole reasoning.")
    ap.add_argument("--notice", action="store_true",
                    help="print ONE line if more than the declared threshold "
                         "is reclaimable, and nothing otherwise.")
    ap.add_argument("--log", default=None)
    ap.add_argument("--deadline", type=float, default=None,
                    help="seconds the scan may take before it gives up and "
                         "says so. For --notice only.")
    # A LAND'S SCOPED SWEEP (scan_agent). With --agent, nothing but what
    # provably belongs to that agent is planned.
    ap.add_argument("--process-only", action="store_true",
                    help="sweep only PID-owned allocations under this job's TMPDIR")
    ap.add_argument("--agent", default=None,
                    help="sweep ONLY the scratch of this landed agent")
    ap.add_argument("--session", default="",
                    help="with --agent: the session the agent ran in")
    ap.add_argument("--workspace", action="append", default=[],
                    help="with --agent: one of its workspaces (repeatable)")
    ap.add_argument("--hold-named", action="store_true",
                    help="with --agent: another live agent has the same name, "
                         "so its named directories are kept")
    args = ap.parse_args(argv)

    if args.deadline:
        _DEADLINE[0] = time.time() + args.deadline
    try:
        cfg = config_from_env(args.process_only)
        ws = load_workspaces()
        reaper = Reaper(cfg, ws)
        # The banner never pays for the expensive arm. Its garbage numbers come
        # from the last full pass, which the scheduled job publishes.
        reaper.skip_unknown_arm = bool(args.notice)
        reaper.skip_build_caches = bool(args.notice)
        reaper.skip_codex = bool(args.notice)
        reaper.skip_named = bool(args.notice)
        if args.process_only:
            reaper.scope = {"kind": "process", "name": "process"}
            reaper.scan_scratch_root(Walls([os.path.realpath(os.environ.get("TMPDIR") or "/tmp")]))
        elif args.agent is not None:
            reaper.scan_agent({"name": args.agent, "session": args.session,
                               "workspaces": args.workspace,
                               "hold_named": bool(args.hold_named)})
        else:
            reaper.scan()
    except Fatal as exc:
        sys.stderr.write("scratch-reaper: %s\n" % exc)
        if args.apply and not args.process_only and args.agent is None and unattended():
            publish_refusal("the scheduled run refused to start: %s" % exc)
        return 2
    except Deadline:
        if args.notice:
            print("SCRATCH: the reclaimable-scratch scan did not finish within "
                  "%.0fs, so how much dead scratch is on this disk is UNKNOWN. "
                  "Read it by hand: %s --verbose"
                  % (args.deadline, os.path.join(
                      os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                      "scratch-reaper.sh")))
            return 3
        sys.stderr.write("scratch-reaper: the scan exceeded its %.0fs budget.\n"
                         % args.deadline)
        return 3

    deleted, freed, undecidable = 0, 0, reaper.counts()[1]

    if args.notice:
        d, i, k, b = reaper.counts()
        cmd = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "scratch-reaper.sh")
        if b >= cfg["notice_bytes"]:
            print("SCRATCH: %s of dead scratch in %d place(s) is reclaimable "
                  "now — nothing running owns any of it. Free it: %s --apply"
                  % (human(b), d, cmd))
        # The undecidable pile gets its own line and ITS OWN, LOWER THRESHOLD.
        # It is the one thing the scheduled job will NEVER clear on its own, so
        # if nobody is told, it grows forever — which is the exact failure the
        # whole mechanism was ordered to end. Sharing SCRATCH_NOTICE_BYTES hid up
        # to a GiB of it (Frank's D12); the two piles are different in kind and
        # get different numbers.
        if reaper.undecidable_bytes() >= cfg["undecidable_notice_bytes"]:
            print("SCRATCH: %s in %d place(s) could not be decided and will "
                  "never be reclaimed by the scheduled job. Read why: %s "
                  "--verbose" % (human(reaper.undecidable_bytes()), i, cmd))
        # THE GARBAGE ALARM, AND IT IS READ FROM THE LAST FULL PASS.
        #
        # The banner does not run the expensive deny-by-default arm (see
        # scan_deferred_unknown), so these two numbers come from the state file
        # the scheduled --apply publishes. THE NUMBER IS DATED IN THE SENTENCE —
        # a figure presented as live that is six hours old is the stale-artifact
        # failure, and the honest form costs one clause.
        st = read_failures(default_state())      # same tolerant JSON reader
        sb = int(st.get("skipped_bytes") or 0)
        sn = int(st.get("skipped") or 0)
        ub = int(st.get("undecidable_bytes") or 0)
        un = int(st.get("undecidable") or 0)
        when = st.get("last_apply") or "?"
        if sb >= cfg["skipped_notice_bytes"]:
            print("SCRATCH: %s in %d place(s) is garbage NOTHING WILL EVER "
                  "COLLECT — another program's temp directory, another user's, "
                  "or a symlink. Nobody is coming for it; it goes when a person "
                  "removes it. Measured by the scheduled pass at %s. See which: "
                  "%s --verbose" % (human(sb), sn, when, cmd))
        if ub >= cfg["undecidable_notice_bytes"] \
                and reaper.undecidable_bytes() < cfg["undecidable_notice_bytes"]:
            # Only when the LIVE scan above did not already say it: the live
            # arms and the full pass see different piles, and saying it twice in
            # two shapes is worse than saying it once.
            print("SCRATCH: %s in %d place(s) could not be decided by the "
                  "scheduled pass at %s and no scheduled run will ever clear "
                  "it. Read why: %s --verbose" % (human(ub), un, when, cmd))
        return 3 if undecidable else 0

    n_failures = 0
    skipped_n, skipped_b = reaper.skipped()
    # THE SPLIT IS PUBLISHED, NOT JUST PRINTED. The watchdog's job is to say
    # which of these a person must act on, and it cannot do that from a total.
    split = {k: {"count": n, "bytes": b2, "label": label}
             for k, label, n, b2 in reaper.skipped_classes()}
    # PATHS, NOT JUST A COUNT, FOR THE ONE CLASS A PERSON ACTS ON. The alert's
    # whole value is being able to say WHICH 12.8 GB, and re-deriving that costs
    # a full nine-second pass. Capped, largest first, and the `why` already
    # carries the exact command.
    by_hand_rows = sorted((e for e in reaper.entries if e.standing),
                          key=lambda e: -e.size)[:5]
    split["by_hand"]["paths"] = [{"path": e.path, "bytes": e.size,
                                  "why": e.why} for e in by_hand_rows]
    failures = []
    if args.apply:
        deleted, freed, failures = reaper.apply(args.log or default_log())
        n_failures = len(failures)
    # THE STATE FILE DESCRIBES THE LAST FULL PASS, and the watchdog reads it as
    # such: a land's scoped sweep looked at one agent's scratch, and publishing
    # its numbers would tell the watchdog the scheduled job had just run. And
    # ONLY THE INSTALLED JOB PUBLISHES IT (cleanup PRD R3/R4): a manual --apply
    # prints its result and never replaces or clears the unattended one.
    if args.apply and reaper.scope is None and unattended():
        unresolved = unresolved_rows(reaper, failures)
        write_state(default_state(), {
            # source/outcome/unresolved/roots: the cleanup health the watchdog
            # turns into one owner action. `healthy` means every candidate was
            # decided and nothing failed; a deliberate KEEP is not a failure.
            "source": "launchd",
            "outcome": "unresolved" if unresolved else "healthy",
            "unresolved": unresolved,
            "roots": sorted(set(reaper.roots)),
            "result_epoch": int(time.time()),
            "last_apply": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "last_apply_epoch": int(time.time()),
            "deleted": deleted, "freed": freed,
            "undecidable": undecidable,
            "undecidable_bytes": reaper.undecidable_bytes(),
            # PUBLISHED SO THE WATCHDOG CAN SEE IT WITHOUT RUNNING THIS PROGRAM.
            # The garbage alarm lives in the turn-end and session-start notices,
            # which read the watchdog, which reads this file. Frank's Fix 2.
            "skipped": skipped_n, "skipped_bytes": skipped_b,
            "skipped_split": split,
            "failures": n_failures,
            "verdict": "undecided" if undecidable else "decided",
        })
    if args.json:
        print(json.dumps({
            "verdict": "undecided" if undecidable else "decided",
            "applied": bool(args.apply),
            "deleted": deleted, "freed": freed,
            "undecidable": undecidable,
            "undecidable_bytes": reaper.undecidable_bytes(),
            "unattributed": reaper.unattributed,
            "skipped": skipped_n, "skipped_bytes": skipped_b,
            "skipped_split": split,
            "failures": n_failures,
            "entries": [{"path": e.path, "class": e.klass, "bytes": e.size,
                         "action": e.action, "why": e.why}
                        for e in reaper.entries],
        }, indent=1, sort_keys=True))
    else:
        print(reaper.report(verbose=args.verbose))
        if args.apply:
            # `failures=` IS ON THIS LINE AND NOT ONLY IN THE LOG. Frank's D10:
            # a run in which every deletion failed printed
            # `applied: deleted=0 freed=0 B` — byte-identical in shape to a run
            # with nothing to do — and exited 0. The word FAILED existed only
            # inside a log file nobody reads.
            print("applied: deleted=%d freed=%s failures=%d log=%s"
                  % (deleted, human(freed), n_failures,
                     args.log or default_log()))
    # EXIT CODES, AND A FAILED DELETION IS NOW IN ONE (D10). It used to be
    # `3 if undecidable else 0`, with `failures` never consulted — so launchd saw
    # green on a run that could not delete a thing. 4 outranks 3 because a
    # deletion that FAILED is the branch of §54 that says Rich deletes it by
    # hand, and an undecidable entry is the branch that says somebody reads it.
    if n_failures:
        return 4
    return 3 if undecidable else 0


if __name__ == "__main__":
    sys.exit(main())

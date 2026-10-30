#!/usr/bin/env python3
"""review_watch.py: THE SECOND REVIEW STARTS BY ITSELF, WITH NOBODY ASKING.

===========================================================================
WHAT THIS EXISTS FOR
===========================================================================
On 2026-10-08 the CEO fetched a Codex review of an agent's work by hand four
times, and each one found defects the author's own tests had passed. His words
(ruling §113): "A regular RichOS user can never be expected anything even
remotely close to that. So, this all must be completely automated."

scripts/second-review.sh (slice 1) does one review. This is slice 2 of richos-hq
docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md (§2.1, §2.4,
§2.5), with Sage's check beside it (...-sage-check.md §2, §4 row 2): the
watcher that starts that command by itself. It is the engine's plugin monitor
`review-watch` (monitors/monitors.json), started by Claude Code with every
interactive session as stall-watch and codex-watch are.

===========================================================================
WHEN A REVIEW STARTS (plan §2.1). Level-triggered: every look compares what
exists (the workspace registry, git, Codex's channel) with what has been
reviewed (the review ledger and the locks). Nothing waits for an event.
===========================================================================
  1. HANDOVER. A teammate's run has ended (the registry's finished_state: its
     SubagentStop, a stop, or its session gone) with commits ahead of its
     base, and no handover verdict exists for its newest commit. A worker that
     dies silently is read from the registry, not from its report, so its
     commits are reviewed anyway. A mid-job verdict does not stand in for the
     handover one: a mid-job reviewer is told to list what is not yet claimed
     instead of reporting it, so it never judged the work as handed over.
     Work stopped before it finished, whose recorded wait (workspaces.sh wait)
     names the `continues:` job that finishes it, is not handed over: it is
     reviewed when that continuation hands over, whose range holds its commits
     (CEO 2026-10-20: "why does the handover of the unfinished work from the
     previous session needs a "review" here in this session???").
  2. A LONG JOB. A teammate still running has commits with no verdict, and 60
     minutes (LONG_JOB_MINUTES, an estimate taken from echo-fable-dict5's run)
     have passed since its clock started. THE CLOCK (Sage §1.2): the
     registry's start for the teammate, then the start of each review of its
     work; where the registry has no start, the moment this watcher first saw
     a commit of it. Never git's commit dates, which an author sets: a
     teammate branched from a day-old main commit is not "long" five minutes
     in.
  3. GONE QUIET. stall-watch's silent-teammate signal (no commit and no
     transcript write for 20 minutes, STALL_WATCH_SILENT_MINUTES) for a
     running teammate with unreviewed commits.
  4. CODEX'S HANDOVER (Sage §2 catch 1). codex/ work never enters the
     registry, so a READY entry in ~/.richos-coordination/rich-codex/to-rich.md
     naming a codex/ branch starts its review (Claude reviews Codex's work;
     second-review decides that from the branch). The original words are the
     to-codex.md entries naming the branch, else the one written just before
     Codex first named it; the READY entry is what the author claims.

THE RANGE IS THE AGENT'S OWN COMMITS: the base is the merge base of the tip and
the integration branch the registry records for the work (workspaces.py
integration_target), passed to second-review explicitly. On 2026-10-09 a run
over richos 2d5aaf844 with a wider range reviewed unrelated main commits.

Only repositories listed in SECOND_REVIEW_REPOS (the governed repository's
orchestration.config) are reviewed; record-only repositories are not (plan §6).
Work whose changed files are all release notes (operator_fences.RELEASE_NOTES)
is never reviewed: the land asks no review of it either (CEO 2026-10-23).

===========================================================================
ONE TIP IS NEVER REVIEWED TWICE AT ONCE (Sage §1.3)
===========================================================================
A file per repository and tip under <state>/locks/, created exclusively, holds
the review's process id and its start time. A watcher's memory is not the
guard, so a second watcher (a twin session, or the lead's own monitor beside a
host child) is harmless. When a handover arrives while a mid-job review of the
same tip runs, that review is stopped by its recorded process id and replaced
by the handover review. Qualified declared recovery may additionally stop exact affected workers through its separate control supervisor. No cap on how many
reviews run at once: second-review admits each by the engine's CPU rule.

A REVIEW THAT ENDS WITHOUT A VERDICT (crashed, refused, past its own 60-minute
limit, or running past OVERRUN_MINUTES, when it is stopped by its recorded
process id) is started ONCE more; a second loss is told to the lead, once.

===========================================================================
WHAT IS TOLD (plan §2.4, §2.5). Each printed block wakes the lead.
===========================================================================
  * every verdict, once, from the review ledger (second-review's
    reviews.jsonl): what was reviewed, by which model, its findings and what
    the lead can do;
  * a changes-requested handover nobody has acted on (no continuation spawned
    with `continues:`, the work not landed or discarded), again every 30
    minutes (REPEAT_MINUTES);
  * a finding still open through two rechecks in a row ("not converging"),
    once: the lead decides; the CEO is not paged;
  * a second lost review of one commit, once;
  * a Codex handover it could not turn into a review, once.
Starting a review is not told: the verdict is.

It never edits, lands, merges or pauses work. Its ordinary review cleanup stops
only reviews it (or another watcher) started and recorded: every
process of this user that carries the review's own mark (MARK_ENV, a random
value set at its start, which everything the review starts inherits however it
is parented or grouped), and every process in the session the review leads.
Declared recovery extends that contract through recovery_runner: an independent
supervisor disables tools for the exact affected registered worker, ends only
its birth-checked owned commands and observes native cancellation separately.
Codex file-based handovers remain review and landing scope, not native dispatch
or interruption coverage. Observation mode never applies recovery controls.

===========================================================================
COMMANDS (review-watch.sh passes --engine-root and --config)
===========================================================================
  --monitor  the plugin monitor's body: one per session, a look every 60 s,
             ends with its session; each look is a fresh --tick child, so a
             landed fix reaches the next look (EVERY LOOK IS FRESH CODE)
  --tick     one look, printing what the monitor would print
  --status   whether this session is watched (its lock held and a recent
             successful look), then one line per running review

Test seams (review-watch.test.sh only): REVIEW_WATCH_STATE_DIR,
REVIEW_WATCH_NOW, REVIEW_WATCH_POLL_SECONDS, REVIEW_WATCH_SECOND_REVIEW,
REVIEW_WATCH_TO_RICH, REVIEW_WATCH_TO_CODEX; second-review's own
SECOND_REVIEW_STATE_DIR places the ledger.
"""

import argparse
import contextlib
import datetime as _dt
import fcntl
import glob
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import stall_watch  # noqa: E402  (sibling: session, lock, JSON and silent-teammate helpers)
from app_review import app_paths  # noqa: E402  (sibling: the app's review paths, shared with its hook)
import operator_fences as F  # noqa: E402  (sibling: review_repos, the one reader of SECOND_REVIEW_REPOS)

POLL_SECONDS = 60
LONG_JOB_MINUTES = 60
REPEAT_MINUTES = 30
# second-review's own worst case: a 30-minute admission wait, a 60-minute
# Codex run and a 60-minute Claude fallback, with room to spare.
OVERRUN_MINUTES = 160
START_GRACE_SECONDS = 120
# Stopping one review in a look (superseded, or past OVERRUN_MINUTES): SIGTERM, then SIGKILL.
STOP_TERM_SECONDS = 10
STOP_KILL_SECONDS = 5
# Stopping every review at the app's quit, all at once: at most 0.5 + 1 + 0.5 + 1 = 3 s (two freezes,
# the SIGTERM grace, the SIGKILL grace), inside the host's five-second STOP_BOUND (richos-core
# review_watch.rs) with two seconds to settle and exit.
QUIT_TERM_SECONDS = 1.0
QUIT_KILL_SECONDS = 1.0
# How long a stop waits for every process of a review to be frozen (SIGSTOP) before it ends them.
FREEZE_SECONDS = 0.5
# THE REVIEW'S OWNERSHIP MARK (the real second review of 0d83e456d, findings 1 and 2): a random value
# spawn puts in this variable of the review's environment, which every process the review starts
# inherits, whatever its parent (a tool's background task is reparented to PID 1 once its shell
# returns) and whatever its process group or session. A stop ends every process of this user whose
# environment carries that exact value; the lock goes only once none is left. Not "...TOKEN": Codex's
# documented default shell_environment_policy drops variables whose names contain KEY, SECRET or
# TOKEN from its commands' environment.
# MEASURED 2026-10-09 (macOS 15.6, SIP on): the kernel withholds the environment of Apple's own
# platform binaries (/bin/bash, /bin/zsh, /bin/sleep, /usr/bin/perl, /usr/bin/tail, sandbox-exec)
# even from their own user, while python, node and the Command Line Tools' python3 show it; so the
# review's own session (it leads one from spawn) counts too, which those keep unless they setsid.
MARK_ENV = "RICHOS_REVIEW_OWNER"
MAX_LOSSES = 2
# A review that ended because the quota hold was in force (second_review.py's quota_held row) is
# "held", not lost: it spends none of MAX_LOSSES and is started again no sooner than this, the
# quota poll's own five minutes, so a hold of any length neither ends the commit's automatic
# reviews nor starts one every look (the second review of 3331a349416c, finding 1).
HOLD_RETRY_SECONDS = 300
KEEP_SECONDS = 7 * 86400
SESSION_KEEP_SECONDS = 2 * 86400
BLOCK_CHARS = 2000
HEAD_CHARS = 140                                    # room for a block's head line, which names the count
GIT_SECONDS = 10
CONFIG_KEY = "SECOND_REVIEW_REPOS"
MID_JOB = ("long-job", "quiet")
HANDOVER_KINDS = ("handover", "manual")
CONTINUES_RE = re.compile(r"(?<![\w-])continues:")   # a recorded wait naming the job that continues the work
DEFAULT_TO_RICH = "~/.richos-coordination/rich-codex/to-rich.md"
DEFAULT_TO_CODEX = "~/.richos-coordination/rich-codex/to-codex.md"
SHA_RE = re.compile(r"\b[0-9a-f]{40}\b")
CODEX_BRANCH_RE = re.compile(r"\bcodex/[A-Za-z0-9._/-]*[A-Za-z0-9_]")


# ---------------------------------------------------------------------------
# small utilities
# ---------------------------------------------------------------------------

def clock():
    raw = (os.environ.get("REVIEW_WATCH_NOW") or "").strip()
    try:
        return float(raw) if raw else time.time()
    except ValueError:
        return time.time()


def hhmm(t):
    return stall_watch.hhmm(t)


def parse_iso(text):
    if not text:
        return None
    try:
        t = str(text)
        if t.endswith("Z"):
            t = t[:-1] + "+00:00"
        return _dt.datetime.fromisoformat(t).timestamp()
    except (TypeError, ValueError):
        return None


def iso(t):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t))


def state_root():
    d = (os.environ.get("REVIEW_WATCH_STATE_DIR") or "").strip()
    if d:
        return d
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state", "review-watch")


def review_ledger():
    """second-review's ledger: the same path its own state_root() gives."""
    d = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip()
    if not d:
        base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
        d = os.path.join(base, "state")
    return os.path.join(d, "reviews.jsonl")


def _p(*parts):
    return os.path.join(state_root(), *parts)


def read_jsonl(path):
    rows = []
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if isinstance(r, dict):
                    rows.append(r)
    except OSError:
        pass
    return rows


def append_jsonl(path, row):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        os.write(fd, (json.dumps(row, sort_keys=True) + "\n").encode("utf-8"))
    finally:
        os.close(fd)


def git(repo, *args):
    """stdout stripped, or None when git fails."""
    try:
        p = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True, timeout=GIT_SECONDS)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return p.stdout.strip() if p.returncode == 0 else None


def group_gone(pgid):
    """Is no process of this group left running? ESRCH: none at all. EPERM: on macOS killpg
    refuses a group whose members are all zombies (measured 2026-10-09, Darwin 24.6: a SIGKILLed,
    unreaped `sleep` group answers EPERM, a reaped one ESRCH); a review and everything under it
    run as this user, so EPERM is never a live process of a review that could still be signaled."""
    try:
        os.killpg(int(pgid), 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        return True
    except (OSError, ValueError):
        return False
    return False


def leads_session(pid):
    """Does a live process with this pid lead a session (its session id is its own pid)?"""
    try:
        return os.getsid(int(pid)) == int(pid)
    except (OSError, ValueError):
        return False


_PROCARGS = {}


def environment(pid):
    """[b"NAME=value"] of a process's environment as it was started, or [] when it cannot be read
    (another user's process, one that has exited, or one of Apple's platform binaries: MARK_ENV)."""
    if sys.platform.startswith("linux"):
        try:
            with open("/proc/%d/environ" % pid, "rb") as f:
                return f.read().split(b"\0")
        except OSError:
            return []
    if "libc" not in _PROCARGS:
        import ctypes
        import ctypes.util
        _PROCARGS["ctypes"] = ctypes
        _PROCARGS["libc"] = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
        _PROCARGS["buf"] = ctypes.create_string_buffer(1 << 20)        # kern.argmax on macOS
    ctypes, buf = _PROCARGS["ctypes"], _PROCARGS["buf"]
    size = ctypes.c_size_t(len(buf))
    mib = (ctypes.c_int * 3)(1, 49, pid)            # CTL_KERN, KERN_PROCARGS2
    if _PROCARGS["libc"].sysctl(mib, 3, buf, ctypes.byref(size), None, 0) != 0 or size.value < 4:
        return []
    raw = buf.raw[:size.value]
    # argc, the executable's path, NUL padding, argc arguments, then the environment.
    argc, parts = int.from_bytes(raw[:4], sys.byteorder), raw[4:].split(b"\0")
    i = 1
    while i < len(parts) and not parts[i]:
        i += 1
    return parts[i + argc:]


def owned_processes(reviews):
    """[{pid: (pgid, stat, sid)}], one per (leader pid, mark, session owned) in `reviews`: the live
    processes of this user that carry that review's mark in their environment, from one read of the
    process table; None when the table cannot be read (a ps timeout or error), which is never an
    empty session (the real second review of 16c154f5a, finding 1; Watcher.stop_all). Ownership
    captured at the review's start, never a name, a path or an ancestry: the exact mark and the
    user id.
    AND THE SESSION ITS LEADER LEADS (spawn starts each review in a session of its own), for what
    hides its environment (MARK_ENV): its members count while the session is owned (the caller's
    word: its leader verified alive, or recorded so and the session not seen empty since; Watcher.
    stop_all), or when one of them carries the mark. A session id is never reused while a member is
    left; once none is, the leader's pid can lead a new session, so an empty session is never owned
    again."""
    out = [{} for _ in reviews]
    try:
        r = subprocess.run(["ps", "-A", "-o", "pid=,uid=,pgid=,stat="], capture_output=True, text=True,
                           timeout=1, env=dict(os.environ, LC_ALL="C"))
    except (OSError, subprocess.TimeoutExpired):
        return None
    if r.returncode != 0:
        return None
    uid, me = os.getuid(), os.getpid()
    marks = [("%s=%s" % (MARK_ENV, mark)).encode() if mark else None for _leader, mark, _owned in reviews]
    session = [{} for _ in reviews]
    proven = [owned for _leader, _mark, owned in reviews]
    for line in r.stdout.splitlines():
        parts = line.split()
        try:
            pid, puid, pgid, stat = int(parts[0]), int(parts[1]), int(parts[2]), parts[3]
            if puid != uid or pid == me or stat.startswith("Z"):
                continue
            sid = os.getsid(pid)
        except (IndexError, ValueError, OSError):
            continue
        env = environment(pid) if any(marks) else []
        for n, (leader, _mark, _owned) in enumerate(reviews):
            marked = bool(marks[n]) and marks[n] in env
            if marked:
                out[n][pid] = (pgid, stat, sid)
            if sid == leader:
                session[n][pid] = (pgid, stat, sid)
                proven[n] = proven[n] or marked
    for n in range(len(reviews)):
        if proven[n]:
            out[n].update(session[n])
    return out


def repo_tag(repo):
    return hashlib.sha1(os.path.realpath(repo).encode("utf-8")).hexdigest()[:12]


def configured_repos(config):
    """([repository name], "") from SECOND_REVIEW_REPOS, read by
    F.review_repos, the one reader install and the land check use too, which
    sources the config in a clean bash; (None, "") when the key is absent or
    blank; (None, why) when the config cannot be read or sourced, or its value
    is not names."""
    if not config or not os.path.isfile(config):
        return None, ""
    try:
        with open(config, encoding="utf-8") as f:
            f.read()
    except (OSError, ValueError) as exc:
        return None, "it cannot be read (%s)" % (getattr(exc, "strerror", None) or exc)
    names, why = F.review_repos(config)
    return (names or None), why


# ---------------------------------------------------------------------------
# what could be reviewed
# ---------------------------------------------------------------------------

class Item(object):
    """One piece of work at one tip."""

    def __init__(self, work, name, repo, tip, base, state, started=None, branch="", ref="", quiet=False,
                 continued=False, source="registry", words=(), claims=(), title=""):
        self.work, self.name, self.repo, self.tip, self.base = work, name, repo, tip, base
        self.state, self.started, self.branch, self.ref = state, started, branch, ref
        self.quiet, self.continued, self.source = quiet, continued, source
        self.words, self.claims, self.title = list(words), list(claims), title

    @property
    def key(self):
        return (self.repo, self.tip)

    env = None          # app mode: the partition this item's registry lives in (RICHOS_WORKSPACES_DIR)
    session = ""        # the lead session that registered the work (the operator host delivers to it)
    stopped_early = False   # ended unfinished, its recorded wait naming a continues: job
    claude = ""         # app mode: the Claude CLI the app ships
    accounts = ""       # app mode: the app's account list (claude-accounts.json), read when the reviewer starts
    codex_reviews = ""  # app mode: the app's Codex review switch (codex-reviews.json), read when the review starts

    def args(self, trigger):
        """second-review's arguments for this item."""
        a = ["--repo", self.repo, "--tip", self.tip, "--base", self.base, "--trigger", trigger, "--work", self.work]
        if getattr(self, "recheck", ""):
            a += ["--recheck", self.recheck]
        if getattr(self, "reconciliation_claim", ""):
            a += ["--claims-file", self.reconciliation_claim]
        if self.source == "app":
            # A regular RichOS user: Claude reviews (plan §2.3, "RichOS is Claude", §16), on the
            # CLI the app ships, with the user's own turn as the original words beside the
            # brief the worker received. Unless the user turned on "Let Codex review your team's
            # work" in Settings (round 20.2, ruling §114): then Codex, whenever it can (app_reviewer).
            a = ["--name", self.ref] + a + ["--reviewer", app_reviewer(self.codex_reviews)]
            for w in self.words:
                a += ["--words-file", w]
            if self.claude:
                a += ["--claude", self.claude]
            if self.accounts:
                a += ["--accounts", self.accounts]
            return a
        if self.source == "codex":
            a += ["--branch", self.branch, "--author", "codex"]
            for w in self.words:
                a += ["--words-file", w]
            for c in self.claims:
                a += ["--claims-file", c]
        else:
            a = ["--name", self.ref] + a
        return a


def app_reviewer(setting):
    """second-review's --reviewer for an app review: "auto" when the user turned on the Codex
    review switch in Settings (round 20.2; his words, ruling §114: "we should give the user a
    toggle/switch to manually enable that"), "claude" otherwise. Read when the review starts, so
    a flip counts from the next review. Off on first run: no file, an unreadable one, or anything
    but {"on": true} is off. "auto" is Codex when second-review finds it and Codex itself says it
    is signed in, with CODEX_ISOLATION (no session in the user's Codex or ChatGPT app), and Claude
    otherwise, which is the row's "Claude is reviewing in the meantime"."""
    if not setting:
        return "claude"
    try:
        with open(setting, encoding="utf-8") as f:
            on = json.load(f).get("on") is True
    except (OSError, ValueError, AttributeError):
        on = False
    return "auto" if on else "claude"


class World(object):
    """The real sources: the workspace registry, git and Codex's channel."""

    def __init__(self, engine_root, config):
        self.engine_root = engine_root
        self.config = config
        self.repos, self.repos_error = configured_repos(config)
        self.ws = stall_watch._load("review_watch_workspaces",
                                    os.path.join(engine_root, "mega-lander", "workspaces.py"))
        self.src = stall_watch.Sources(engine_root)
        self._merge_bases = {}

    # -- which repositories ---------------------------------------------------
    def wanted(self, repo):
        return F.review_listed(self.repos, repo)

    def wants_space(self, w):
        return self.wanted(w["repo"])

    def repo_named(self, name):
        try:
            known = self.ws.known_repos() if self.ws else []
        except Exception:  # noqa: BLE001
            known = []
        for k in known:
            if os.path.basename(os.path.realpath(k)) == name:
                return os.path.realpath(k)
        return ""

    # -- git --------------------------------------------------------------------
    def merge_base(self, repo, a, b):
        k = (repo, a, b)
        if k not in self._merge_bases:
            self._merge_bases[k] = git(repo, "merge-base", a, b)
        return self._merge_bases[k]

    def integration_tip(self, chain, repo):
        """(tip, problem): the branch the registry records for this work."""
        ws = self.ws
        try:
            branch, tip, why = ws.integration_target(chain, repo)
            if not tip:
                branch, tip, why2 = ws.integration_for(repo)
                why = why2 or why
        except Exception as exc:  # noqa: BLE001
            tip, why = "", "the integration branch could not be read (%s)" % exc.__class__.__name__
        if tip:
            return tip, ""
        main = git(repo, "rev-parse", "--verify", "-q", "refs/heads/main^{commit}")
        if main:
            return main, ""
        return "", why or "no integration branch is recorded for %s" % repo

    # -- the registry -----------------------------------------------------------
    def registry_items(self, now, seen):
        """(items, problems). seen: {work: first epoch a commit was seen}."""
        ws = self.ws
        if ws is None:
            return [], ["the workspace registry library (mega-lander/workspaces.py) could not be loaded"]
        try:
            recs = ws.all_agents()
        except Exception as exc:  # noqa: BLE001
            return [], ["the workspace registry could not be read (%s)" % exc.__class__.__name__]
        continued = set()
        for r in recs:
            continued.update((r.get("recovery_assignment") or {}).get("correction_of") or r.get("continues") or [])
        items, problems, cache = [], [], {}
        th_silent = stall_watch.thresholds()["silent"]
        for rec in recs:
            if rec.get("disposition"):
                continue
            spaces = [w for w in rec.get("workspaces") or []
                      if w.get("branch") and not w.get("deleted_at") and not w.get("branch_deleted_at")
                      and w.get("repo") and self.wants_space(w)]
            if not spaces:
                continue
            try:
                fin, paused, _why = ws.finished_state(rec, cache)
                chain = ws.review_chain(rec)
            except Exception:  # noqa: BLE001
                continue
            state = "ended" if fin else ("paused" if paused else "running")
            work = ws.recovery_work_key(rec)
            started = None
            for k in ("started_at", "spawned_at", "registered_at"):
                started = parse_iso(rec.get(k))
                if started is not None:
                    break
            name = str(rec.get("name") or rec["key"])
            for w in spaces:
                repo = os.path.realpath(w["repo"])
                tip = git(repo, "rev-parse", "--verify", "-q", "refs/heads/%s^{commit}" % w["branch"])
                if not tip:
                    continue
                itip, why = self.integration_tip(chain, repo)
                if not itip:
                    problems.append("%s: %s" % (name, why))
                    continue
                base = self.merge_base(repo, itip, tip)
                if not base or base == tip:
                    continue                       # nothing of its own ahead of the integration branch
                seen.setdefault(work, now)
                quiet = False
                if state == "running":
                    marks = [started or seen[work]]
                    c = stall_watch._git_last_commit(w.get("path"))
                    if c and c[0] > marks[0]:
                        marks.append(c[0])
                    t = stall_watch._transcript(self.src, rec)
                    if t:
                        try:
                            marks.append(os.path.getmtime(t))
                        except OSError:
                            pass
                    quiet = now - max(marks) >= th_silent
                item = Item(work, name, repo, tip, base, state, started=started, branch=w["branch"],
                            ref=rec["key"], quiet=quiet, continued=rec["key"] in continued)
                item.session = str(rec.get("session_id") or "")
                item.stopped_early = state == "ended" and bool(
                    CONTINUES_RE.search(str((rec.get("waiting") or {}).get("on") or "")))
                items.append(item)
        return items, problems

    # -- Codex's handovers --------------------------------------------------------
    def codex_items(self, now, codex_state):
        """(items, problems, notes). codex_state is the shared reading state
        (offset in to-rich.md and the pending handovers), updated in place."""
        to_rich = os.path.expanduser(os.environ.get("REVIEW_WATCH_TO_RICH") or DEFAULT_TO_RICH)
        to_codex = os.path.expanduser(os.environ.get("REVIEW_WATCH_TO_CODEX") or DEFAULT_TO_CODEX)
        problems = []
        pending = codex_state.setdefault("pending", [])
        try:
            st = os.stat(to_rich)
        except OSError:
            st = None
        if st is not None:
            off = codex_state.get("offset")
            if codex_state.get("inode") != st.st_ino or not isinstance(off, int) or off > st.st_size:
                # The first look at this file (or a new file): the backlog is
                # not replayed, as codex-watch does.
                codex_state.update({"inode": st.st_ino, "offset": st.st_size, "seen": st.st_size})
            elif st.st_size > off and st.st_size == codex_state.get("seen"):
                with open(to_rich, "rb") as fh:
                    fh.seek(off)
                    data = fh.read(st.st_size - off)
                end = data.rfind(b"\n") + 1 or len(data)
                text = data[:end].decode("utf-8", "replace")
                codex_state["offset"] = off + end
                for header, body in split_entries(text):
                    got = self.codex_handover(header, body, to_rich, to_codex, now)
                    if isinstance(got, str):
                        problems.append(got)
                    elif got:
                        pending[:] = [p for p in pending if p.get("work") != got["work"] or p.get("tip") == got["tip"]]
                        if not any(p.get("tip") == got["tip"] and p.get("repo") == got["repo"] for p in pending):
                            pending.append(got)
            codex_state["seen"] = st.st_size
        items = []
        keep = []
        for p in pending:
            if now - float(p.get("at") or now) > KEEP_SECONDS:
                continue
            keep.append(p)
            base = self.merge_base_for(p["repo"], p["tip"])
            if not base or base == p["tip"]:
                continue
            items.append(Item(p["work"], "codex", p["repo"], p["tip"], base, "ended", branch=p["branch"],
                              source="codex", words=p.get("words") or [], claims=p.get("claims") or [],
                              title=p.get("title") or ""))
        pending[:] = keep
        return items, problems

    def merge_base_for(self, repo, tip):
        itip, _why = self.integration_tip([], repo)
        return self.merge_base(repo, itip, tip) if itip else None

    def codex_handover(self, header, body, to_rich, to_codex, now):
        """A pending review for a READY entry naming a codex/ branch; a string
        saying why one cannot start; or None for any other entry."""
        if not header or "READY" not in header.upper():
            return None
        text = header + "\n" + body
        m = CODEX_BRANCH_RE.search(header) or CODEX_BRANCH_RE.search(body)
        if not m:
            return None
        branch = m.group(0)
        # Codex writes "Repository: richos only." and "Repository richos-hq only.", with or
        # without the colon.
        listed = [n.strip(".") for line in re.findall(r"Repositor(?:y|ies)\b([^\n]*)", text)
                  for n in re.findall(r"[A-Za-z0-9._-]+", line)]
        names = [n for n in listed if n in (self.repos or [])]
        if not names and any(self.repo_named(n) for n in listed):
            # It names only repositories that get no second review: not a review case.
            return None
        if not names and self.repos and len(self.repos) == 1:
            names = [self.repos[0]]
        repo = self.repo_named(names[0]) if names else ""
        if not repo or not self.wanted(repo):
            return None
        cm = re.search(r"Commit:\s*`?([0-9a-f]{40})", text) or SHA_RE.search(text)
        tip = cm.group(1) if cm and cm.groups() else (cm.group(0) if cm else "")
        if not tip:
            tip = git(repo, "rev-parse", "--verify", "-q", "refs/heads/%s^{commit}" % branch) or ""
        if not tip or git(repo, "cat-file", "-e", tip + "^{commit}") is None:
            return ("Codex's handover %r names %s, whose commit %s is not in %s, so no review of it started"
                    % (header[:90], branch, tip[:12] or "(none)", repo))
        words = codex_words(to_codex, to_rich, branch)
        if not words:
            return ("Codex's handover %r: no entry in %s names %s or comes before Codex first named it, so there "
                    "are no original words and no review of it started" % (header[:90], to_codex, branch))
        wdir = _p("codex-words")
        os.makedirs(wdir, exist_ok=True)
        stem = "%s-%s" % (re.sub(r"[^A-Za-z0-9._-]", "_", branch), tip[:12])
        wpath, cpath = os.path.join(wdir, stem + ".words.md"), os.path.join(wdir, stem + ".claims.md")
        with open(wpath, "w", encoding="utf-8") as f:
            f.write("".join("%s\n%s\n" % (h, b) for h, b in words))
        with open(cpath, "w", encoding="utf-8") as f:
            f.write("%s\n%s\n" % (header, body))
        return {"work": "branch:%s:%s" % (repo, branch), "repo": repo, "branch": branch, "tip": tip,
                "words": [wpath], "claims": [cpath], "at": now, "title": header[3:].strip()[:120]}


class AppWorld(World):
    """THE APP'S OWN WATCHER (second review, slice 4): the same looks over the app's registry.

    A regular RichOS user's app runs this as the host's own child (richos-core review_watch.rs),
    because a product lease loads no settings and no monitors (`--setting-sources ''`, Sage's
    check §1.3). What differs from his team's watcher, and why:
      * The registry is partitioned, one per company and conversation
        (<app state>/workspaces/<sha256>), so each look reads every partition.
      * Every connected repository is reviewed: the user connected it for this work; there is no
        orchestration.config to list it in.
      * WORKERS ONLY: an item is a registry workspace whose receipt says request.role worker; a
        handover reviewer's cc/ workspace is never itself picked for a review.
      * MID-JOB ONLY (long-job, gone quiet). A handover in the app is already reviewed, and
        `integrate` refuses without that review's pass (DESKTOP.md step 5, app.py integrate);
        a second handover review would only spend the user's subscription twice.
      * Claude reviews, on the CLI the app ships (--claude), with the user's own turn, kept
        beside the worker's receipt by app.py prepare, as the original words. Each review runs on
        the Claude account the app's work runs on when its reviewer starts: second-review reads
        the app's account list (--accounts) then, as a work lease is given the account in use.
      * Codex reviews instead when the user turned on the Settings switch "Let Codex review
        your team's work" (round 20.2, ruling §114; --codex-reviews, read when each review
        starts, app_reviewer), whenever Codex is installed and signed in; Claude otherwise.
      * No Codex channel: that is his team's.
    """

    codex_reviews = ""  # the Codex switch's file; none (a world built without it) is the switch off

    def __init__(self, engine_root, app_state, claude="", accounts="", codex_reviews=""):
        self.engine_root = engine_root
        self.config = ""
        self.app_state = os.path.realpath(app_state)
        self.claude = claude
        self.accounts = accounts
        self.codex_reviews = codex_reviews
        self.repos, self.repos_error = ["(every connected repository)"], ""
        self.ws = stall_watch._load("review_watch_workspaces",
                                    os.path.join(engine_root, "mega-lander", "workspaces.py"))
        self.src = stall_watch.Sources(engine_root)
        self._merge_bases = {}

    def wanted(self, repo):
        return True

    def wants_space(self, w):
        # The implementation workspace in a connected repository (cc/), never the provider's
        # native coordination worktree, which is not the work (app.py target_workspaces).
        return w.get("kind") == "cc"

    def partitions(self):
        return sorted(p for p in glob.glob(os.path.join(self.app_state, "workspaces", "*"))
                      if os.path.isdir(p) and not os.path.islink(p))

    def receipt_for(self, partition, name):
        """(receipt, path) app.py prepare wrote for `name` in this partition; (None, "") if none."""
        receipts = os.path.join(self.app_state, "work-receipts", os.path.basename(partition))
        for path in sorted(glob.glob(os.path.join(receipts, "*.json"))):
            try:
                with open(path, encoding="utf-8") as f:
                    receipt = json.load(f)
                if receipt.get("name") != name:
                    continue
            except (OSError, ValueError, AttributeError):
                continue
            return receipt, path
        return None, ""

    def words_for(self, partition, name):
        """The user's turn app.py prepare kept beside the receipt that started `name`."""
        _receipt, path = self.receipt_for(partition, name)
        words = path[:-len(".json")] + ".words" if path else ""
        return [words] if words and os.path.isfile(words) else []

    @staticmethod
    def is_worker(receipt):
        """Only implementation work gets a mid-job review: a receipt whose request.role is worker.
        A handover reviewer also gets a cc/ workspace, at the worker's commit (app.py prepare), and
        a quiet or long review would otherwise be reviewed itself, with a verdict that names the
        reviewer's branch and so never reaches the worker (app.py worker_spaces: role worker)."""
        try:
            return receipt.get("request", {}).get("role") == "worker"
        except AttributeError:
            return False

    def registry_items(self, now, seen):
        items, problems = [], []
        before = os.environ.get("RICHOS_WORKSPACES_DIR")
        try:
            for part in self.partitions():
                os.environ["RICHOS_WORKSPACES_DIR"] = part
                got, probs = World.registry_items(self, now, seen)
                for it in got:
                    receipt, _path = self.receipt_for(part, it.name)
                    if not self.is_worker(receipt):
                        continue
                    it.source, it.env, it.claude = "app", {"RICHOS_WORKSPACES_DIR": part}, self.claude
                    it.accounts, it.codex_reviews = self.accounts, self.codex_reviews
                    it.words = self.words_for(part, it.name)
                    items.append(it)
                problems += probs
        finally:
            if before is None:
                os.environ.pop("RICHOS_WORKSPACES_DIR", None)
            else:
                os.environ["RICHOS_WORKSPACES_DIR"] = before
        return items, problems

    def codex_items(self, now, codex_state):
        return [], []


def split_entries(text):
    """[(header line or '', body)] split at `## ` headers."""
    out = []
    for ln in text.split("\n"):
        if ln.startswith("## ") or not out:
            out.append([ln, []] if ln.startswith("## ") else ["", [ln]])
        else:
            out[-1][1].append(ln)
    return [(h, "\n".join(b).strip("\n")) for h, b in out]


def codex_words(to_codex, to_rich, branch):
    """What Rich asked Codex for: every to-codex.md entry naming the branch;
    else the last one written before Codex first named the branch."""
    try:
        with open(to_codex, encoding="utf-8", errors="replace") as f:
            asks = split_entries(f.read())
    except OSError:
        return []
    named = [(h, b) for h, b in asks if h and branch in h + "\n" + b]
    if named:
        return named
    first = None
    try:
        with open(to_rich, encoding="utf-8", errors="replace") as f:
            for h, b in split_entries(f.read()):
                if h and branch in h + "\n" + b:
                    first = entry_time(h)
                    break
    except OSError:
        return []
    if first is None:
        return []
    before = [(h, b) for h, b in asks if h and entry_time(h) is not None and entry_time(h) <= first]
    return before[-1:]


def entry_time(header):
    m = re.match(r"##\s+(\d{4}-\d\d-\d\dT\d\d:\d\d(?::\d\d)?Z)", header or "")
    if not m:
        return None
    t = m.group(1)
    if len(t) == 17:
        t = t[:-1] + ":00Z"
    return parse_iso(t)


# ---------------------------------------------------------------------------
# what has been reviewed: the ledger, the locks and the attempts
# ---------------------------------------------------------------------------

def lock_path(repo, tip):
    return _p("locks", "%s-%s.lock" % (repo_tag(repo), tip))


class Book(object):
    """Everything known about reviews, read once per look."""

    def __init__(self, rows, running, attempts):
        self.rows = rows
        self.running = running                      # {(repo, tip): lock info}
        self.verdicts = {}                          # {(repo, tip): [rows with a verdict]}
        self.by_work = {}
        for r in rows:
            if r.get("verdict"):
                self.verdicts.setdefault((os.path.realpath(r.get("repo") or ""), r.get("tip")), []).append(r)
                self.by_work.setdefault(r.get("work"), []).append(r)
        self.losses = {}
        self.held = {}                              # {(repo, tip): the newest held attempt still owed a retry}
        for a in attempts:
            k = (a.get("repo"), a.get("tip"))
            if a.get("outcome") == "lost":
                self.losses.setdefault(k, []).append(a)
                self.held.pop(k, None)
            elif a.get("outcome") == "held":
                self.held[k] = a
            elif a.get("outcome") == "verdict":
                self.losses.pop(k, None)
                self.held.pop(k, None)

    def handover_verdict(self, key):
        return [r for r in self.verdicts.get(key, []) if r.get("trigger") in HANDOVER_KINDS]

    def any_verdict(self, key):
        return self.verdicts.get(key, [])

    def last_review_at(self, work):
        """The start of the newest review of this work, finished or running."""
        ts = [parse_iso(r.get("at")) for r in self.by_work.get(work, [])]
        ts += [float(i.get("started_at") or 0) for i in self.running.values() if i.get("work") == work]
        ts = [t for t in ts if t]
        return max(ts) if ts else None


_NOTES_ONLY = {}


def notes_only(item):
    """Is everything this work changes release notes (F.RELEASE_NOTES)? Then no
    second review starts for it, and the land asks for none. Commits never
    change, so each (repo, base, tip) is asked of git once."""
    k = (item.repo, item.base, item.tip)
    if k not in _NOTES_ONLY:
        _NOTES_ONLY[k] = F.notes_only(item.repo, item.base, item.tip)
    return _NOTES_ONLY[k]


def due(items, book, now, seen, long_seconds, handover=True):
    """[(item, trigger, supersede lock info or None)]: the reviews to start now.
    Pure: everything it needs is in its arguments. handover=False is the app's watcher
    (AppWorld): its handovers are reviewed by the app's own reviewer, so it starts mid-job
    reviews only."""
    out = []
    for it in items:
        if it.tip == it.base:
            continue
        if it.state == "ended" and not handover:
            continue
        key = it.key
        lost = book.losses.get(key) or []
        if len(lost) >= MAX_LOSSES:
            continue                                # told once; nothing more starts by itself
        running = book.running.get(key)
        held = book.held.get(key)
        if held and not running and now - float(held.get("ended_at") or 0) < HOLD_RETRY_SECONDS:
            continue                                # held by the quota: asked again at the next poll

        if it.state == "ended":
            if book.handover_verdict(key) or it.stopped_early:
                continue                            # stopped early: its continuation's handover covers these commits
            if running and running.get("trigger") in MID_JOB:
                out.append((it, "handover", running))  # the handover replaces a mid-job review of this tip
                continue
            if running:
                continue
            out.append((it, "handover", None))     # a lost one is started once more, as what it now is
            continue
        if running or book.any_verdict(key):
            continue
        if lost:
            out.append((it, lost[-1].get("trigger") or "long-job", None))   # a lost review, started once more
            continue
        if held:
            out.append((it, held.get("trigger") or "long-job", None))       # a held review, asked again
            continue
        anchor = book.last_review_at(it.work)
        start = it.started if it.started is not None else seen.get(it.work, now)
        anchor = max(anchor or start, start)
        if now - anchor >= long_seconds:
            out.append((it, "long-job", None))
        elif it.quiet and it.state == "running":
            out.append((it, "quiet", None))
    return out


# ---------------------------------------------------------------------------
# the watcher
# ---------------------------------------------------------------------------

class Watcher(object):
    # A quit that arrives while spawn has a child forked and not yet in self.children is held
    # (hold_quit > 0, the signal in held_quit) and taken once it is registered; _starting is the
    # (lock path, info) start() is filling in, until the child's pid is in that lock.
    hold_quit = 0
    held_quit = 0
    _starting = None

    def __init__(self, engine_root, config, world=None):
        self.engine_root = engine_root
        self.config = config
        self.world = world or World(engine_root, config)
        self.children = {}                          # pid -> Popen, until its lock is settled
        self.marks = {}                             # pid -> the MARK_ENV value its review carries

    # -- processes (overridden by the replay test) ---------------------------------
    def second_review(self):
        return (os.environ.get("REVIEW_WATCH_SECOND_REVIEW") or "").strip() or os.path.join(
            self.engine_root, "scripts", "second-review.sh")

    def spawn(self, item, trigger, log_path):
        """(pid, start identity) of a started review, which leads a session of its own and carries a
        mark of its own (MARK_ENV) in its environment."""
        argv = ["bash", self.second_review()] + item.args(trigger)
        mark = os.urandom(16).hex()
        env = dict(os.environ, **(item.env or {}))
        env[MARK_ENV] = mark
        # Direct app callers also use spawn without the durable start/lock path.
        # Give those invocations a receipt identity without weakening joined-verdict checks.
        if not getattr(item, "review_invocation", ""):
            item.review_invocation = os.urandom(16).hex()
        env["SECOND_REVIEW_INVOCATION"] = item.review_invocation
        # PARTLY REGISTERED (second review of f14155545, finding 2): a review leads its own
        # session, so a quit that lands inside Popen (its fork done, Popen not yet returned) would
        # lose the only handle on it. The quit is held until the child is in self.children, then
        # taken, and stop_own stops every child there, whether or not its pid reached its lock.
        self.hold_quit += 1
        try:
            with open(log_path, "ab") as log:
                p = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                     start_new_session=True, cwd=state_root(), env=env)
            self.children[p.pid] = p
            self.marks[p.pid] = mark
        finally:
            self.hold_quit -= 1
            if self.held_quit and not self.hold_quit:
                signum, self.held_quit = self.held_quit, 0
                raise SystemExit(128 + signum)
        return p.pid, self.process_start(p.pid)

    @staticmethod
    def process_start(pid):
        try:
            r = subprocess.run(["ps", "-o", "lstart=", "-p", str(int(pid))], capture_output=True, text=True,
                               timeout=10, env=dict(os.environ, LC_ALL="C", TZ="UTC0"))
        except (OSError, ValueError, subprocess.TimeoutExpired):
            return ""
        return " ".join(r.stdout.split()) if r.returncode == 0 else ""

    def alive(self, info):
        """Is the review this lock records still running? Its pid, and its start
        time (a reused pid is not it). A child of this watcher is reaped first."""
        pid = info.get("pid")
        if not pid:
            return None
        child = self.children.get(pid)
        if child is not None:
            return child.poll() is None
        try:
            os.kill(int(pid), 0)
        except PermissionError:
            pass
        except (OSError, ValueError, TypeError):
            return False
        start = self.process_start(pid)
        return bool(start) and (not info.get("pid_start") or start == info["pid_start"])

    def stop(self, info):
        """Stop one review (stop_all); [] once it is gone, else [info]."""
        return self.stop_all([info], STOP_TERM_SECONDS, STOP_KILL_SECONDS)

    def stop_all(self, infos, term_seconds, kill_seconds):
        """Stop every review in `infos` AT ONCE, bounded by 2 * FREEZE_SECONDS + term_seconds +
        kill_seconds however many reviews there are (never their sum).

        A REVIEW IS EVERY PROCESS THAT CARRIES ITS MARK (the real second review of 0d83e456d,
        findings 1 and 2; owned_processes), never what an ancestry or a process group says: a
        tool's background task reparented to PID 1, a command in a group or session of its own and
        a reviewer whose launcher has exited are all still the review's. While its recorded leader
        is verified alive, the group it leads (second-review and its reviewer) is signaled as a
        group too, so nothing forked between two reads escapes. FROZEN FIRST: SIGSTOP to all of it,
        read again, SIGSTOP whatever was not stopped yet, until a read finds every process stopped
        (a stopped process starts nothing) or FREEZE_SECONDS pass. Then every process outside the
        leader's group is SIGKILLed, and the group gets SIGTERM and SIGCONT, so second-review exits
        and releases its scratch. Whatever is left after term_seconds is frozen again and
        SIGKILLed, again on every read, until none is left. Returns the infos with a process NOT
        seen gone inside the bound (normally none), so a caller never settles a running review.

        ITS SESSION STAYS ITS OWN AFTER ITS LAUNCHER EXITS (the second review of 283b4379d,
        finding 1): an Apple tool hides the mark, so it is found only as a member of the session
        the leader led. That session is owned while the leader is verified alive, and, once the
        leader has exited, while the review was recorded as leading it (own_session in its lock,
        start; or this watcher's own child) and no read has found it empty, unless its pid now
        leads a session again, which only a new process can (owned_processes).

        A READ THAT FAILS IS NOT AN EMPTY SESSION (the real second review of 16c154f5a, finding 1):
        a process table that cannot be read leaves each review unknown, never gone: it keeps its
        session and is kept as left, so a stop that never reads the table again returns it and
        its lock stays unsettled for the next look."""
        live = [i for i in infos if i.get("pid")]
        for i in live:
            i["_verified"] = bool(self.alive(i))
            pid = int(i["pid"])
            i["_session"] = i["_verified"] or (bool(i.get("own_session") or pid in self.children)
                                               and not leads_session(pid))

        def scan(infos):
            found = owned_processes([(int(i["pid"]), i.get("mark") or self.marks.get(i["pid"], ""), i["_session"])
                                     for i in infos])
            if found is None:
                return [None] * len(infos)          # unknown: none gone, every session still owned
            for i, procs in zip(infos, found):
                i["_session"] = i["_session"] and any(s == int(i["pid"]) for _g, _st, s in procs.values())
            return found

        def kill(pids, sig):
            for p in pids:
                try:
                    os.kill(p, sig)
                except OSError:
                    pass                            # gone already

        def killpg(infos, sig):
            for i in infos:
                if i["_verified"]:
                    try:
                        os.killpg(int(i["pid"]), sig)
                    except OSError:
                        pass

        def left(infos, found):
            """The reviews with a process left, and what was found of each; this watcher's own
            leaders are reaped first."""
            keep = []
            for i, procs in zip(infos, found):
                child = self.children.get(i["pid"])
                if child is not None:
                    child.poll()
                if procs is None or procs or (i["_verified"] and not group_gone(i["pid"])):
                    keep.append((i, procs))
            return [i for i, _p in keep], [p for _i, p in keep]

        def freeze(infos):
            end = time.monotonic() + FREEZE_SECONDS
            while True:
                killpg(infos, signal.SIGSTOP)
                found = scan(infos)
                running = [p for procs in found for p, (_g, stat, _s) in (procs or {}).items() if stat[:1] != "T"]
                if (not running and None not in found) or time.monotonic() >= end:
                    return found
                kill(running, signal.SIGSTOP)
                time.sleep(0.02)

        live, found = left(live, scan(live))
        if not live:
            return []
        found = freeze(live)
        kill([p for i, procs in zip(live, found) for p, (g, _st, _s) in (procs or {}).items()
              if not (i["_verified"] and g == int(i["pid"]))], signal.SIGKILL)
        killpg(live, signal.SIGTERM)
        killpg(live, signal.SIGCONT)
        for grace, last in ((term_seconds, False), (kill_seconds, True)):
            if last:
                found = freeze(live)
            end = time.monotonic() + grace
            while True:
                if last:
                    killpg(live, signal.SIGKILL)
                    kill([p for procs in found for p in procs or ()], signal.SIGKILL)
                live, found = left(live, scan(live))
                if not live or time.monotonic() >= end:
                    break
                time.sleep(0.05)
            if not live:
                return []
        return live

    # -- locks -------------------------------------------------------------------
    def take_lock(self, item, trigger, now, attempt):
        path = lock_path(item.repo, item.tip)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            return None
        # The lead session that owns the work is kept in the review's own record (this lock, then
        # its attempt row), so its verdict reaches that lead even when the worker commits again
        # while the review runs and no item carries the reviewed tip any more.
        info = {"repo": item.repo, "tip": item.tip, "work": item.work, "name": item.name, "trigger": trigger,
                "started_at": now, "attempt": attempt, "watcher": os.getpid(), "pid": None, "pid_start": "",
                "session": getattr(item, "session", "") or ""}
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(info, f)
        return path, info

    def rewrite_lock(self, path, info):
        tmp = "%s.%d.tmp" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(info, f)
        os.replace(tmp, path)

    @staticmethod
    def joined_verdict(info, rows):
        """Accept an older reused verdict only through the child's exact host receipt."""
        try:
            with open(info.get('log') or '', 'rb') as source:
                source.seek(0,os.SEEK_END)
                size=source.tell();source.seek(max(0,size-65536))
                lines=source.read().decode('utf-8',errors='replace').splitlines()
            prefix='SECOND-REVIEW RECEIPT '
            line=next((line for line in reversed(lines) if line.startswith(prefix)),None)
            receipt=json.loads(line[len(prefix):]) if line else None
        except (OSError,ValueError,TypeError):
            return None
        if not isinstance(receipt,dict) or not info.get('review_invocation') or receipt.get('invocation')!=info['review_invocation']:return None
        if (receipt.get('recheck_of') or '')!=(info.get('recheck_of') or ''):return None
        purpose='handover' if info.get('trigger') in ('handover','manual') else 'progress'
        for row in rows:
            if (row.get('verdict') and row.get('input_identity')
                and all(receipt.get(k)==row.get(k) for k in ('id','repo_id','tip','work','input_identity'))
                and receipt.get('tip')==info.get('tip') and receipt.get('work')==info.get('work')
                and F.review_row_belongs(row,F.review_repo_identity(info.get('repo')))
                and (receipt.get('recheck_of') or '')==(row.get('recheck_of') or '')
                and receipt.get('purpose')==purpose
                and ('handover' if row.get('trigger') in ('handover','manual') else 'progress')==purpose):
                return row
        return None

    def settle(self, path, info, now, rows, outcome=None, why=""):
        """The lock's review has ended: one attempt row, then the lock goes.
        Renamed first, so of two watchers exactly one settles it."""
        claimed = "%s.settled.%d" % (path, os.getpid())
        try:
            os.rename(path, claimed)
        except OSError:
            return None
        if outcome is None:
            started = float(info.get("started_at") or 0)
            mine = [r for r in rows if os.path.realpath(r.get("repo") or "") == info.get("repo")
                    and r.get("tip") == info.get("tip") and r.get("id") not in info.get("prior_reviews", [])
                    and (parse_iso(r.get("at")) or 0) >= started - 5]
            got = [r for r in mine if r.get("verdict")]
            if got or self.joined_verdict(info,rows):
                outcome = "verdict"
            elif mine and mine[-1].get("quota_held"):
                outcome = "held"
                why = why or mine[-1].get("why") or "the quota hold is in force"
            else:
                outcome = "lost"
                why = why or (mine[-1].get("why") if mine else "") or self.log_tail(info) or \
                    "the review ended without writing a row to the ledger"
        a = {"repo": info.get("repo"), "tip": info.get("tip"), "work": info.get("work"), "name": info.get("name"),
             "trigger": info.get("trigger"), "started_at": info.get("started_at"), "ended_at": now,
             "outcome": outcome, "why": why, "attempt": info.get("attempt"), "session": info.get("session") or ""}
        append_jsonl(_p("attempts.jsonl"), a)
        try:
            os.unlink(claimed)
        except OSError:
            pass
        self.children.pop(info.get("pid"), None)
        self.marks.pop(info.get("pid"), None)
        return a

    @staticmethod
    def log_tail(info):
        try:
            with open(info.get("log") or "", encoding="utf-8", errors="replace") as f:
                lines = [ln.strip() for ln in f.read().splitlines() if ln.strip()]
        except OSError:
            return ""
        return " | ".join(lines[-2:])[:300]

    def reconcile(self, now, rows):
        """{(repo, tip): info} of reviews still running; ended ones settled."""
        running = {}
        for path in sorted(glob.glob(_p("locks", "*.lock"))):
            try:
                with open(path, encoding="utf-8") as f:
                    info = json.load(f)
            except (OSError, ValueError):
                info = None
            if not isinstance(info, dict):
                try:
                    if now - os.path.getmtime(path) > START_GRACE_SECONDS:
                        os.unlink(path)
                except OSError:
                    pass
                continue
            info["path"] = path
            alive = self.alive(info)
            age = now - float(info.get("started_at") or now)
            if alive is None and age < START_GRACE_SECONDS:
                running[(info.get("repo"), info.get("tip"))] = info
                continue
            if alive and age > OVERRUN_MINUTES * 60 and not self.stop(info):
                self.settle(path, info, now, rows, "lost",
                            "it ran %d minutes, past the %d-minute bound, and was stopped by its recorded process id"
                            % (age / 60, OVERRUN_MINUTES))
                continue
            # ITS LAUNCHER HAS EXITED; WHAT IT STARTED MAY NOT HAVE (the real second review of 0d83e456d,
            # finding 2): a reviewer past run_bounded's limit leaves its tools behind. They are
            # ended by the review's mark, and the lock is settled only once none is left.
            if alive or self.stop(info):
                running[(info.get("repo"), info.get("tip"))] = info
                continue
            self.settle(path, info, now, rows)
        return running

    def start(self, item, trigger, now, attempt, supersede=None, rows=()):
        if supersede:
            if self.stop(supersede):
                return None                         # still running: the next look tries again
            self.settle(supersede["path"], supersede, now, rows, "superseded",
                        "replaced by the handover review of the same commit")
        got = self.take_lock(item, trigger, now, attempt)
        if got is None:
            return None
        path, info = got
        os.makedirs(_p("logs"), exist_ok=True)
        info["review_invocation"] = os.urandom(16).hex()
        item.review_invocation = info["review_invocation"]
        info["recheck_of"] = getattr(item,"recheck","")
        ident = F.review_repo_identity(item.repo)
        info["prior_reviews"] = [r.get("id") for r in rows if r.get("tip")==item.tip and F.review_row_belongs(r,ident)]
        info["log"] = _p("logs", "%s-%s-%d-%s.log" % (repo_tag(item.repo), item.tip[:12], attempt,info["review_invocation"]))
        self._starting = (path, info)               # stop_own's handle until the pid is in the lock
        try:
            pid, pstart = self.spawn(item, trigger, info["log"])
        except OSError as exc:
            self.rewrite_lock(path, info)
            self.settle(path, info, now, rows, "lost", "second-review could not be started: %s" % exc)
            self._starting = None
            return None
        info["pid"], info["pid_start"], info["mark"] = pid, pstart, self.marks.get(pid, "")
        # It leads a session of its own (spawn), recorded while it is verified alive, so the session
        # stays the review's after it exits, for the members that hide the mark (stop_all).
        info["own_session"] = bool(self.alive(info))
        self.rewrite_lock(path, info)
        self._starting = None
        return info

    def stop_own(self, now):
        """The app quit (or its host died): every review THIS watcher started is stopped by its
        recorded process id and settled as stopped, never as lost, so a quit does not count
        toward the two losses after which nothing starts by itself. It starts again at the
        first look after the app opens, if it is still due. Reviews another watcher started are
        never touched.

        THE HOST'S BOUND: richos-core review_watch.rs STOP_BOUND gives this process five seconds
        after SIGTERM before it SIGKILLs this watcher's group, and each review leads its own
        session, out of that group's reach. So every review is stopped at once and escalated to
        SIGKILL inside 2 * FREEZE_SECONDS + QUIT_TERM_SECONDS + QUIT_KILL_SECONDS (3 s, whatever
        the count), leaving two seconds of the five for settling and exit."""
        rows = read_jsonl(review_ledger())
        own = []
        for path in sorted(glob.glob(_p("locks", "*.lock"))):
            info = stall_watch._read_json(path)
            if not info or info.get("pid") not in self.children:
                continue
            info["path"] = path
            own.append(info)
        # EVERY OWNED CHILD (second review of f14155545, finding 2): a quit between spawn's Popen
        # and the lock write leaves a child whose pid its lock does not hold yet. It is stopped
        # too, and the lock start() was filling in for it is settled with it; a lock taken with
        # no child started under it is settled as stopped, never left to count as a loss.
        starting = self._starting
        if starting and any(i["path"] == starting[0] for i in own):
            starting = None
        recorded = set(i.get("pid") for i in own)
        for pid in sorted(self.children):
            if pid in recorded:
                continue
            if starting:
                own.append(dict(starting[1], pid=pid, path=starting[0]))
                starting = None
            else:
                own.append({"pid": pid})
        if starting:
            own.append(dict(starting[1], path=starting[0]))
        self._starting = None
        left = set(id(i) for i in self.stop_all(own, QUIT_TERM_SECONDS, QUIT_KILL_SECONDS))
        for info in own:
            if id(info) in left:
                # Still running after SIGKILL, or not seen gone (the process table could not be
                # read): never recorded as stopped. Its lock stays, and the next look settles it
                # by what it finds then.
                sys.stderr.write("review-watch: the review with pid %s was not seen gone %.0f s after the quit "
                                 "began; it was not recorded as stopped\n"
                                 % (info.get("pid"), 2 * FREEZE_SECONDS + QUIT_TERM_SECONDS + QUIT_KILL_SECONDS))
                continue
            if info.get("path"):
                self.settle(info["path"], info, now, rows, "stopped",
                            "the app quit while it ran; it starts again at a look after the app opens, if still due")
            self.children.pop(info.get("pid"), None)
            self.marks.pop(info.get("pid"), None)

    # -- one look --------------------------------------------------------------------
    def look(self, now, session_state):
        """Lines to print. Starts what is due; settles what ended."""
        if self.world.repos_error:
            if session_state.get("told_unreadable_repos") == self.world.repos_error:
                return []
            session_state["told_unreadable_repos"] = self.world.repos_error
            return ["REVIEW-WATCH %s: %s in %s cannot be read: %s. No second review starts by itself until it "
                    "reads %s=\"name name ...\"." % (hhmm(now), CONFIG_KEY, self.config, self.world.repos_error,
                                                     CONFIG_KEY)]
        if not self.world.repos:
            if session_state.get("told_no_repos"):
                return []
            session_state["told_no_repos"] = now
            return ["REVIEW-WATCH %s: no repository is listed in %s (%s), so no second review starts by itself. "
                    "Add one, e.g. %s=\"richos\"." % (hhmm(now), CONFIG_KEY, self.config or "no orchestration.config",
                                                      CONFIG_KEY)]
        shared = stall_watch._read_json(_p("shared.json"))
        seen = shared.setdefault("seen", {})
        codex_state = shared.setdefault("codex", {})
        rows = read_jsonl(review_ledger())
        running = self.reconcile(now, rows)
        items, problems = self.world.registry_items(now, seen)
        citems, cproblems = self.world.codex_items(now, codex_state)
        items += citems
        problems += cproblems
        items = [it for it in items if not notes_only(it)]
        import recovery_runner
        entity = os.path.dirname(self.config) if self.config else os.environ.get("RICHOS_ENTITY_ROOT", "")
        recovery_requests = recovery_runner.tick(items, self.engine_root, entity,
                            getattr(self.world, "ws", None)) if entity else []
        if entity and not isinstance(self.world, AppWorld) and getattr(self.world, "ws", None) is not None:
            # Axel/Wes event triage drains recorded dispatches, lead sends and
            # results in its own process, only when one of those records
            # changed. It never waits here and never depends on a commit.
            try:
                import recovery_events
                started = recovery_events.launch(self.engine_root, entity, self.world.ws)
                if started and started.get("gap"):
                    sys.stderr.write("review-watch: %s\n" % started["gap"])
            except (OSError, ValueError) as error:
                sys.stderr.write("review-watch: recovery event drain unavailable: %s\n" % error)
        attempts = read_jsonl(_p("attempts.jsonl"))
        book = Book(rows, running, attempts)
        import recovery
        conflicting={}
        for row in rows:
            key=(row.get('work'),row.get('tip'))
            conflicting.setdefault(key,set()).add(row.get('verdict'))
        for item in items:
            if item.state!='ended' or not {'passed','changes-requested'}.issubset(conflicting.get((item.work,item.tip),set())):continue
            saved={k:os.environ.get(k) for k in (item.env or {})}
            os.environ.update(item.env or {})
            try:
                task=recovery.conflict_task(item.repo,item.work,item.tip)
                if task:
                    item.reconciliation_claim=str(recovery.root()/'reconciliations'/(task['id']+'.json'))
                    recovery_requests=list(recovery_requests or [])+[(item,task['prior'])]
            finally:
                for k,v in saved.items():
                    if v is None:os.environ.pop(k,None)
                    else:os.environ[k]=v
        for item, prior in recovery_requests or []:
            if item.key not in book.running and len(book.losses.get(item.key) or [])<MAX_LOSSES:
                item.recheck = prior
                info = self.start(item, "handover", now, 1, None, rows)
                if info:
                    book.running[item.key] = info
        handover = not isinstance(self.world, AppWorld)
        for it, trigger, supersede in due(items, book, now, seen, LONG_JOB_MINUTES * 60, handover):
            n = len(book.losses.get(it.key) or []) + 1
            info = self.start(it, trigger, now, n, supersede, rows)
            if info:
                book.running[it.key] = info
        for w in list(seen):
            if now - float(seen[w]) > KEEP_SECONDS:
                del seen[w]
        stall_watch._write_json(_p("shared.json"), shared)
        attempts = read_jsonl(_p("attempts.jsonl"))
        return tell(now, session_state, rows, Book(rows, book.running, attempts), items, problems, attempts)


# ---------------------------------------------------------------------------
# what is told
# ---------------------------------------------------------------------------

def _verdict_file(row):
    try:
        with open(os.path.join(row.get("record") or "", "verdict.json"), encoding="utf-8") as f:
            v = json.load(f)
        return v if isinstance(v, dict) else {}
    except (OSError, ValueError):
        return {}


def _who(row, items_by_key):
    """(name, item) of the work a ledger row reviewed: the item at its tip only when it is the
    same work, so another work holding that commit never lends its name or its `continues:` key."""
    it = items_by_key.get((os.path.realpath(row.get("repo") or ""), row.get("tip"), row.get("work") or ""))
    if it is None and not row.get("work"):
        it = items_by_key.get((os.path.realpath(row.get("repo") or ""), row.get("tip")))
    return (it.name if it else row.get("author") or "?"), it


def owner_session(row, items, book, attempts):
    """The lead session a verdict goes to (--host-json), by the work's STABLE identity, never by
    the reviewed tip alone: a worker that commits while its review runs no longer has an item at
    that tip, and a verdict sent to an empty session is refused by the host and lost.
    THE RECORDED OWNER COMES FIRST (second review of f14155545, finding 3): another work can hold
    the reviewed commit (a handover reviewer's cc/ workspace is made at the worker's commit), so
    a match on repository and tip alone can name another lead. In order: the review's own record
    (its lock while it runs, then its newest attempt row once settled), which kept the session it
    was started for; then the registry, for the SAME work only (`teammate:<root key>`, which a
    continuation shares), its item at the exact tip before any other item of it. A record or an
    item of another work never names the lead. "" when none names one (Codex's work, a teammate
    started from his terminal, a review run by hand)."""
    key = (os.path.realpath(row.get("repo") or ""), row.get("tip"))
    work = row.get("work") or ""

    def same_work(other):
        return not work or not other or other == work
    info = book.running.get(key) or {}
    if info.get("session") and same_work(info.get("work")):
        return info["session"]
    for a in reversed(attempts or []):
        if (os.path.realpath(a.get("repo") or ""), a.get("tip")) == key and a.get("session") \
                and same_work(a.get("work")):
            return a["session"]
    if work:
        mine = [it for it in items if it.work == work and it.session]
        for it in sorted(mine, key=lambda i: i.key != key):
            return it.session
    return ""


def _cr_key(repo, tip, work):
    """The told-state key of a changes-requested handover: one per work at a commit, so a notice
    to one work's lead never advances or suppresses another work's at the same commit."""
    return "cr:%s:%s:%s" % (os.path.realpath(repo or ""), tip, work or "")


def render_verdict(row, items_by_key, again=None):
    who, it = _who(row, items_by_key)
    trig = row.get("trigger") or "?"
    mid = trig in MID_JOB
    head = "  [%s] %s, %s review, %s@%s by %s (%s): %d finding%s, %d P1%s" % (
        str(row.get("verdict")).upper(), who, "mid-job (%s)" % trig if mid else trig,
        os.path.basename(row.get("repo") or ""), str(row.get("tip"))[:12], row.get("reviewer") or "?",
        row.get("reviewer_model") or "?", int(row.get("findings") or 0), "" if row.get("findings") == 1 else "s",
        int(row.get("p1") or 0), " (forced by its findings)" if row.get("forced") else "")
    if again:
        head += "  (told again, notice %d; first told %s)" % again
    out = [head]
    answer = _verdict_file(row).get("answer") or {}
    for n, f in enumerate((answer.get("findings") or [])[:4], 1):
        out.append("      P%s %s (%s)%s" % (f.get("priority"), " ".join(str(f.get("title") or "").split())[:110],
                                           ", ".join(f.get("files") or [])[:80],
                                           " [%s#%d]" % (row.get("id"), n) if f.get("blocks") else ""))
    if len(answer.get("findings") or []) > 4:
        out.append("      ... and %d more" % (len(answer["findings"]) - 4))
    out.append("      record: %s" % os.path.join(row.get("record") or "?", "verdict.json"))
    if row.get("verdict") == "passed":
        out.append("      You can: land it%s." % (" as usual" if not mid else " once it hands over and that review passes"))
    elif mid:
        out.append("      You can: nothing was stopped; tell %s now if it should change course (SendMessage). "
                   "Its handover is reviewed anyway." % who)
    else:
        out.append("      You can: start a fresh continuation with these findings as its input (`continues: %s`); a "
                   "finished agent is never resumed. Told again every %d min until a continuation exists or the "
                   "work is landed or discarded." % (it.ref.split("--")[-1] if it and it.ref else who, REPEAT_MINUTES))
        out.append("      Or, when no user hits a blocking finding in normal use of this work (ruling §116), file it "
                   "as a follow-up and land: %s --follow-up <finding id> --why '<who rules, and why>'."
                   % os.path.join(os.path.dirname(HERE), "second-review.sh"))
    return out


def _same_repo(a, b):
    """Whether two verdict rows are of one repository: by the identity both
    recorded (`repo_id`) when both have one, otherwise by the real path."""
    if a.get("repo_id") and b.get("repo_id"):
        return a["repo_id"] == b["repo_id"]
    return os.path.realpath(a.get("repo") or "") == os.path.realpath(b.get("repo") or "")


def not_converging(row, rows):
    """[(finding id, title)] still open AND blocking in this recheck and in the
    recheck of the same repository before it. One teammate's repositories share
    a work key, and each repository's review carries only its own findings, so
    rechecks are compared repository by repository. A finding that does not
    block was filed as a follow-up when its review passed, so it is never told
    here (review rv-20261009T104053Z-8662354d-47b2). A verdict written before
    findings said whether they block counts its still-open findings as blocking,
    as they were then."""
    work = row.get("work")
    same = [r for r in rows if r.get("work") == work and r.get("verdict") and _same_repo(r, row)]
    try:
        i = [r.get("id") for r in same].index(row.get("id"))
    except ValueError:
        return []
    if i < 1:
        return []
    now_v, prev_v = _verdict_file(row), _verdict_file(same[i - 1])

    def still(v):
        return set(e.get("id") for e in (v.get("answer") or {}).get("earlier_findings") or []
                   if e.get("status") == "still-open" and e.get("blocks") is not False)
    both = still(now_v) & still(prev_v)
    titles = dict((e.get("id"), e.get("title")) for e in now_v.get("earlier_findings_in") or [])
    return [(fid, titles.get(fid) or "") for fid in sorted(both)]


# --host-json (the operator host's child, richos-core review_watch.rs): every look's notices as
# JSON lines, one per lead session, which the host sends to that lead as a message of its own.
HOST_JSON = {"on": False}
# --monitor inside a lead session (run_loop): that session's id. Its notices are that lead's
# delivery, so a block owned by another lead is left to that lead's own monitor (below).
MONITOR = {"session": ""}


def for_this_monitor(owner):
    """THE PLAIN MONITOR DELIVERS THROUGH THE OWNER TOO (second review of 783a8dba1, finding 2):
    a block goes to this lead session's monitor when this session owns it, when no session does,
    or when its owner has no live monitor of its own to print it (that session ended), so a
    verdict reaches its lead and is never dropped. Without a known session, every block.
    A live owner's monitor prints its verdicts itself, on its first look too: tell() delivers a
    session's own undelivered verdicts there, wherever the shared cursor is (second review of
    b5ff41f02, finding 2)."""
    me = MONITOR["session"]
    if not me or not owner or owner == me:
        return True
    if os.sep in owner or owner.startswith("."):
        return True                                 # not a session id this watcher could have written
    return not stall_watch.lock_held(os.path.join(_p("sessions", owner), "monitor.lock"))


def _row_key(row):
    """A verdict's key in last-told.json's `delivered`: its review id (every second-review row has one)."""
    return str(row.get("id") or "%s:%s:%s:%s" % (row.get("repo"), row.get("tip"), row.get("work"), row.get("at")))


def host_json_lines(now, body, owners, keys):
    """[JSON line]: one {"session", "text", "keys"} per lead session, its blocks under the usual
    head, with the review ids of the verdicts in it for the host to acknowledge (host_acks)."""
    out, ids, order = {}, {}, []
    for block, session, key in zip(body, owners, keys):
        if session not in out:
            out[session], ids[session] = [], []
            order.append(session)
        out[session].append(block)
        if key and key not in ids[session]:
            ids[session].append(key)
    lines = []
    for session in order:
        blocks = out[session]
        head = ("REVIEW-WATCH %s: %d second-review notice%s (it only starts reviews and reports: nothing was "
                "paused, stopped or killed)" % (hhmm(now), len(blocks), "" if len(blocks) == 1 else "s"))
        text = "\n".join([head] + [line for b in blocks for line in b])
        lines.append(json.dumps({"session": session, "text": text, "keys": ids[session]}, sort_keys=True))
    return lines


# DELIVERED ONLY ONCE THE HOST ACCEPTS IT (the real second review of 0d83e456d, finding 3): tell()
# records nothing delivered. It leaves the review ids of the owned verdicts it printed in
# PRINTED, and tick() marks them delivered in last-told.json only once its output is accepted:
# written and flushed, or, for an operator host that says it acknowledges (ACKS_ENV, set by
# richos-core review_watch.rs), acknowledged on this process's stdin ({"ack": [ids]}) once the host
# has told the lead. A pipe that fails, or a notice the host's desk refuses, leaves the verdict to
# be told again at the next look.
# ONLY WHAT WAS EMITTED (the real second review of 0be50ade1, finding 1): PRINTED is built from the
# blocks the output really carries, after the monitor's cap, never from every block tell() made.
# Its "told" entries, the notices with no delivery key (a NOT STARTED problem, a handover reminder,
# a notice for another lead), are written to the session's told.json only once the output is
# written and flushed; a block the cap left out, or a pipe that failed, is told at a later look.
ACKS_ENV = "RICHOS_REVIEW_WATCH_ACKS"
PRINTED = {"keys": [], "told": {}}
ACKS = {"fd": 0, "buf": b""}


def host_acks():
    return HOST_JSON["on"] and os.environ.get(ACKS_ENV) == "1"


def read_acks():
    """The review ids the host has acknowledged since the last read, without waiting."""
    try:
        os.set_blocking(ACKS["fd"], False)
        while True:
            chunk = os.read(ACKS["fd"], 65536)
            if not chunk:
                break
            ACKS["buf"] += chunk
    except OSError:
        pass                                        # nothing more yet (BlockingIOError), or no stdin
    lines = ACKS["buf"].split(b"\n")
    ACKS["buf"] = lines.pop()
    keys = []
    for line in lines:
        try:
            keys += [str(k) for k in json.loads(line).get("ack") or []]
        except (ValueError, AttributeError, TypeError):
            continue
    return keys


def deliver(keys, now):
    """Record these verdicts delivered. Called under the look lock."""
    if not keys:
        return
    path = _p("last-told.json")
    shared = stall_watch._read_json(path)
    delivered = shared.get("delivered") if isinstance(shared.get("delivered"), dict) else {}
    for k in keys:
        delivered.setdefault(k, now)
    shared["delivered"] = delivered
    for key in keys:
        notice = (shared.get("recovery_refs") or {}).get(key)
        if notice:
            try:
                import recovery
                recovery.owner_delivered(notice["record"], key, notice["session"])
            except (OSError, ValueError) as error:
                sys.stderr.write("review-watch: recovery delivery audit unavailable: %s\n" % error)
    stall_watch._write_json(path, shared)


def tell(now, sstate, rows, book, items, problems, attempts):
    # By (repo, tip) for a row that names no work, and by (repo, tip, work) for one that does (_who).
    items_by_key = dict((it.key, it) for it in items)
    items_by_key.update(((it.repo, it.tip, it.work), it) for it in items)
    told = sstate.setdefault("told", {})
    body, owners, keys, marks, pending = [], [], [], [], {}
    PRINTED["keys"], PRINTED["told"] = [], {}

    def add(block, it=None, session=None, key=None, mark=None):
        # Each block keeps the session of the lead whose teammate it is about, so the operator
        # host can deliver it to that lead (--host-json); "" when no lead started the work. An
        # owned notice's block keeps its delivery key too, recorded delivered only once accepted;
        # any other keeps its told mark, (told key, value), written only once emitted (PRINTED).
        body.append(block)
        owners.append(session if session is not None else (getattr(it, "session", "") or ""))
        keys.append(key)
        marks.append(mark)
        if mark:
            pending[mark[0]] = mark[1]
    # -- new verdicts, from where this session last read the ledger ---------------
    # ONLY A VERDICT'S RECORDED OWNER CONSUMES IT (second review of b5ff41f02, finding 2).
    # last-told.json's `rows` is where a monitor's FIRST look starts reading, so a verdict written
    # while nobody watched is still told; it is nobody's record of an owned verdict. That record is
    # `delivered` beside it ({review id: when}), written only once a verdict printed for its owner
    # was accepted (PRINTED, tick): by the owner's own monitor, or by the operator host
    # (--host-json), which sends every block to its owner. A verdict whose owner this watcher
    # delivers to is told whenever it is not in `delivered`, wherever any cursor is and however old
    # it is: another lead's monitor may have shown it (its owner had no live monitor then) or left
    # it (it had one), and neither consumes it. Passed and mid-job verdicts have no reminder; this
    # is their only delivery. `delivered` keeps every id still in the ledger, so a told verdict
    # never comes back. Nothing is inferred
    # from the old cursor-only state: on the first look under this rule an owner may be told its
    # earlier verdicts once more, a repeat and never a loss (that review's recheck).
    # AND ONLY WHAT IS PRINTED (the same review's recheck): a monitor's block is capped at
    # BLOCK_CHARS, so verdicts stop where the cap is reached; those left are neither marked nor
    # passed by this session's cursor, and the next look tells them.
    me = MONITOR["session"]
    shared = stall_watch._read_json(_p("last-told.json"))
    seen = shared.get("rows") if isinstance(shared.get("rows"), int) and shared["rows"] <= len(rows) else None
    delivered = shared.get("delivered") if isinstance(shared.get("delivered"), dict) else {}
    # Which earlier rows could be this watcher's to tell (an owner it delivers to), so the owner
    # lookup runs only for them: the review records and registry items that name such a session.
    ours = lambda sid: bool(sid) and (sid == me or HOST_JSON["on"])
    our_tips = set((os.path.realpath(r.get("repo") or ""), r.get("tip"))
                   for r in list(attempts or []) + list(book.running.values()) if ours(r.get("session")))
    our_works = set(it.work for it in items if ours(it.session))
    start = sstate.get("rows")
    if not isinstance(start, int) or start > len(rows):
        start = seen if seen is not None else len(rows)
    budget = None if HOST_JSON["on"] else BLOCK_CHARS - HEAD_CHARS
    used, stop = 0, len(rows)
    for i, row in enumerate(rows):
        if not row.get("verdict"):
            continue
        key = _row_key(row)
        if i < start and (key in delivered or (row.get("work") not in our_works and (
                os.path.realpath(row.get("repo") or ""), row.get("tip")) not in our_tips)):
            continue
        session = owner_session(row, items, book, attempts)
        owned = ours(session)
        if (owned and key in delivered) or (i < start and not owned):
            continue                                # told to its owner already, or not this watcher's to tell
        blocks = [render_verdict(row, items_by_key)]
        # NOT CONVERGING is kept until accepted, like its verdict (the real second review of
        # 0be50ade1, finding 1): an owned one carries its own delivery key, never a told mark.
        nc = [(k, fid, title) for k, fid, title in (("nc:%s:%s" % (row.get("work"), fid), fid, title)
                                                    for fid, title in not_converging(row, rows))
              if k not in told and k not in delivered and k not in pending and k not in keys]
        who, _it = _who(row, items_by_key)
        blocks += [["  [NOT CONVERGING] %s: finding %s (%s) is still open after two rechecks in a row." % (
                        who, fid, " ".join(title.split())[:100]),
                    "      You decide: another engineer, another model or a smaller slice. The CEO is not paged."]
                   for _k, fid, title in nc]
        cost = sum(len(x) + 1 for b in blocks for x in b) if budget is not None and for_this_monitor(session) else 0
        if cost and used and used + cost > budget:
            stop = i                                # the cap: this row and every later one wait for the next look
            break
        used += cost
        cr = row.get("verdict") == "changes-requested" and row.get("trigger") not in MID_JOB
        add(blocks[0], session=session, key=key if owned else None, mark=(
            _cr_key(row.get("repo"), row.get("tip"), row.get("work")), {"first": now, "last": now, "count": 1})
            if cr else None)
        for b, (k, _fid, _title) in zip(blocks[1:], nc):
            add(b, session=session, key=k if owned else None, mark=None if owned else (k, {"first": now}))
    sstate["rows"] = max(start, stop)
    shared["rows"] = max(stop, seen or 0)
    lost_twice = dict(("lost:%s:%s" % key, key) for key, lost in book.losses.items() if len(lost) >= MAX_LOSSES)
    ids = set(_row_key(r) for r in rows if r.get("verdict")) | set(lost_twice)
    shared["delivered"] = dict((k, t) for k, t in delivered.items() if k in ids or (
        (k.startswith("nc:") or k.startswith("recovery:")) and now - float(t or 0) <= KEEP_SECONDS))
    stall_watch._write_json(_p("last-told.json"), shared)
    import recovery_runner
    for notice in recovery_runner.NOTICES:
        shared.setdefault("recovery_refs", {})[notice["key"]] = dict(record=notice["record"], session=notice["session"])
        k, session = notice["key"], notice["session"]
        if k not in delivered and ours(session):
            add(notice["lines"], session=session, key=k)
    stall_watch._write_json(_p("last-told.json"), shared)
    verdict_blocks = len(body)
    # -- an unhandled changes-requested handover, again every 30 minutes ------------
    # THE SAME OWNER AS THE FIRST NOTICE (second review of 783a8dba1, finding 2): another work can
    # hold the reviewed commit (a handover reviewer's cc/ workspace is made at the worker's
    # commit), so the verdict is looked up for THIS item's work only, its clock is that work's, and
    # it goes through owner_session like the first notice, never through whichever item is here.
    for it in items:
        if it.source != "registry" or it.state != "ended" or it.continued:
            continue
        hv = [r for r in book.handover_verdict(it.key) if r.get("work") == it.work]
        if not hv or hv[-1].get("verdict") != "changes-requested":
            continue
        k = _cr_key(it.repo, it.tip, it.work)
        t = pending.get(k) or told.get(k)
        if t is None:
            add(render_verdict(hv[-1], items_by_key), session=owner_session(hv[-1], items, book, attempts),
                mark=(k, {"first": now, "last": now, "count": 1}))
        elif now - float(t.get("last") or now) >= REPEAT_MINUTES * 60:
            again = dict(t, last=now, count=int(t.get("count") or 1) + 1)
            add(render_verdict(hv[-1], items_by_key, (again["count"], hhmm(float(again["first"])))),
                session=owner_session(hv[-1], items, book, attempts), mark=(k, again))
    # -- a commit whose review was lost twice --------------------------------------
    # KEPT UNTIL ACCEPTED, LIKE A VERDICT (the real second review of 802194f0e, finding 2): told once,
    # this is the only word that automatic reviews of the commit have stopped. For an owner this
    # watcher delivers to it carries its key and is recorded in `delivered` only once accepted
    # (PRINTED, tick); a refused or failed delivery tells it again at the next look. `told` still
    # holds one told under the earlier rule, and one shown to a lead that is not this watcher's.
    # NOT FOR LANDED WORK (2026-10-24 13:32Z: four notices of a session ended 2026-10-21, for commits
    # already on main, told again by a new session's watcher): a commit already on its repository's
    # main is not told at all: the work landed.
    for k, key in lost_twice.items():
        if k in told or k in delivered:
            continue
        if key[0] and git(key[0], "merge-base", "--is-ancestor", str(key[1]), "refs/heads/main") is not None:
            continue
        a = book.losses[key][-1]
        session = a.get("session") or getattr(items_by_key.get(key + (a.get("work") or "",)), "session", "") or ""
        add(["  [NO VERDICT TWICE] %s, %s@%s: %s" % (a.get("name"), os.path.basename(key[0] or ""),
                                                    str(key[1])[:12], " ".join(str(a.get("why")).split())[:300]),
             "      Nothing more starts for this commit by itself. You can: fix the cause, then run "
             "%s by hand." % "second-review.sh"], session=session, key=k if ours(session) else None,
            mark=None if ours(session) else (k, {"first": now}))
    # -- what could not be read or started ------------------------------------------
    for p in problems:
        k = "problem:" + p[:120]
        if k in told or k in pending:
            continue
        add(["  [NOT STARTED] " + p[:400]], mark=(k, {"first": now}))
    for k in list(told):
        if now - float(told[k].get("first") or now) > KEEP_SECONDS:
            del told[k]
    if HOST_JSON["on"]:
        shown = list(range(len(body)))              # the host is given every block
        lines = host_json_lines(now, body, owners, keys) if body else []
    else:
        shown, lines = [n for n, s in enumerate(owners) if for_this_monitor(s)], []
    if shown and not HOST_JSON["on"]:
        # Only an enforce notice can stop work; questions-only notices are questions.
        recovery_on = any(n.get("mode") == "enforce" for n in recovery_runner.NOTICES)
        contract = "declared recovery control may stop exact affected workers" if recovery_on else "it only starts reviews and reports: nothing was paused, stopped or killed"
        lines = ["REVIEW-WATCH %s: %d second-review notice%s (%s)" % (hhmm(now), len(shown), "" if len(shown)==1 else "s", contract)]
        used = len(lines[0])
        for i, n in enumerate(shown):
            cost = sum(len(x) + 1 for x in body[n])
            # Verdict blocks were already fitted to the cap above: never cut here.
            if n >= verdict_blocks and used + cost > BLOCK_CHARS and len(lines) > 1:
                lines.append("  ... more in the next look's block, or read %s" % review_ledger())
                shown = shown[:i]
                break
            lines += body[n]
            used += cost
    PRINTED["keys"] = list(dict.fromkeys(keys[n] for n in shown if keys[n]))
    PRINTED["told"] = dict(marks[n] for n in shown if marks[n])
    return lines


# ---------------------------------------------------------------------------
# the session, the loop and the commands
# ---------------------------------------------------------------------------

def session_dir(sid):
    d = _p("sessions", sid or "no-session")
    os.makedirs(d, exist_ok=True)
    return d


def prune(keep):
    root = _p("sessions")
    now = time.time()
    try:
        names = os.listdir(root)
    except OSError:
        names = []
    for n in names:
        d = os.path.join(root, n)
        if d == keep or not os.path.isdir(d) or stall_watch.lock_held(os.path.join(d, "monitor.lock")):
            continue
        try:
            newest = max([os.path.getmtime(os.path.join(d, f)) for f in os.listdir(d)] + [os.path.getmtime(d)])
        except OSError:
            continue
        if now - newest > SESSION_KEEP_SECONDS:
            for f in os.listdir(d):
                try:
                    os.unlink(os.path.join(d, f))
                except OSError:
                    pass
            try:
                os.rmdir(d)
            except OSError:
                pass
    for sub in ("logs", "codex-words"):
        for f in glob.glob(_p(sub, "*")):
            try:
                if now - os.path.getmtime(f) > KEEP_SECONDS:
                    os.unlink(f)
            except OSError:
                pass


@contextlib.contextmanager
def look_lock():
    """The machine-wide look lock (two watchers take turns)."""
    os.makedirs(state_root(), exist_ok=True)
    fd = os.open(_p("look.lock"), os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)


def tick(watcher, sd, now=None, out=None, tell_failure=True):
    """One look under the look lock; the verdicts it printed are delivered once accepted (PRINTED).
    Returns what the look raised, or "" for a look that finished. With tell_failure off (the
    monitor's own look, --monitor-look) a raised look is not printed here: the monitor tells it,
    once per failing streak (run_loop)."""
    out = out or sys.stdout
    now = clock() if now is None else now
    with look_lock():
        if host_acks():
            deliver(read_acks(), now)
        path = os.path.join(sd, "told.json")
        sstate = stall_watch._read_json(path)
        failed = ""
        try:
            lines = watcher.look(now, sstate)
        except Exception as exc:  # noqa: BLE001: one failed look is said, never the end of watching
            failed = "%s: %s" % (exc.__class__.__name__, str(exc)[:200])
            k = "failed:%s" % exc.__class__.__name__
            lines = [] if not tell_failure or k in sstate.get("told", {}) else [
                "REVIEW-WATCH %s: a look failed (%s); the next look tries again" % (hhmm(now), failed)]
            PRINTED["keys"], PRINTED["told"] = [], {k: {"first": now}}
        printed, marked = (PRINTED["keys"], PRINTED["told"]) if lines else ([], {})
        PRINTED["keys"], PRINTED["told"] = [], {}
        sstate["last_look"] = now
        stall_watch._write_json(path, sstate)
    if lines:
        out.write("\n".join(lines) + "\n")
        out.flush()
    if (printed and not host_acks()) or marked:
        with look_lock():
            if not host_acks():
                deliver(printed, now)
            if marked:
                sstate = stall_watch._read_json(path)
                sstate.setdefault("told", {}).update(marked)
                stall_watch._write_json(path, sstate)
    return failed


def current_session(engine_root):
    ws = stall_watch._load("review_watch_session_ws", os.path.join(engine_root, "mega-lander", "workspaces.py"))
    if ws is None:
        return "", None
    try:
        sid = ws.current_session() or ""
        pid = ws.session_pid(sid) if sid else ws.session_pid()
    except Exception:  # noqa: BLE001
        return "", None
    return sid, pid


# ---------------------------------------------------------------------------
# EVERY LOOK IS FRESH CODE (stall_watch.py's design, R5 there)
# ---------------------------------------------------------------------------
# 2026-10-24: the fix for a READY that names only richos-hq landed at 19:49Z, but this
# session's monitor (PID 90830, started 13:32Z) had loaded review_watch.py once, and at
# 20:44Z it told [NOT STARTED] for exactly that READY again. stall-watch met the same thing
# on 2026-10-22. So the plain monitor keeps only its lock, its session's liveness and its
# output; every look is `--tick --monitor-look` in a fresh child, which loads the engine from
# disk and records the source identity it evaluated (evaluated.json). A child that fails, whose
# look raised (it exits 1 and records no evaluation), or that outlives LOOK_SECONDS is a watcher
# failure, told once per failing streak and reason.
# The app's and the operator host's watchers (run_host_loop) end with their host and keep
# their reviews in memory for the quit, so they still look in-process.
EVALUATOR_SOURCES = ("scripts/lib/review_watch.py", "scripts/lib/stall_watch.py", "scripts/lib/app_review.py",
                     "scripts/lib/operator_fences.py", "scripts/lib/recovery.py", "scripts/lib/recovery_runner.py",
                     "scripts/lib/recovery_events.py", "mega-lander/workspaces.py")
# An estimate, not a measurement: a look is git calls bounded at GIT_SECONDS each, and stops
# of overrunning reviews bounded at STOP_TERM_SECONDS + STOP_KILL_SECONDS each.
LOOK_SECONDS = 300


def source_identity(engine_root):
    """The sha256 (16 hex) of the evaluator's source files as they are on disk."""
    h = hashlib.sha256()
    for rel in EVALUATOR_SOURCES:
        try:
            with open(os.path.join(engine_root, rel), "rb") as fh:
                h.update(rel.encode() + b"\0" + fh.read() + b"\0")
        except OSError:
            h.update(rel.encode() + b"\0missing\0")
    return h.hexdigest()[:16]


def _child_look(a, engine_root, sid, sd):
    """(ok, why). One look in a fresh child of this same script; what it prints is passed on."""
    timeout = stall_watch._env_float("REVIEW_WATCH_LOOK_SECONDS", LOOK_SECONDS)
    cmd = [sys.executable, "-B", os.path.abspath(__file__), "--tick", "--monitor-look",
           "--config", a.config, "--engine-root", engine_root]
    started = time.time()
    # The source when the child STARTS; the child hashes it again after its look, and a
    # difference means which version it ran is unknown (recorded as no version).
    before = source_identity(engine_root)
    try:
        r = subprocess.run(cmd, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=timeout,
                           env=dict(os.environ, RICHOS_SESSION_ID=sid or ""))
    except subprocess.TimeoutExpired as exc:
        # What it printed before it was ended is passed on: a verdict it printed may already be
        # recorded delivered. (Python gives this partial output as bytes, whatever text= says.)
        part = exc.stdout or ""
        part = part.decode("utf-8", "replace") if isinstance(part, bytes) else part
        if part:
            sys.stdout.write(part if part.endswith("\n") else part + "\n")
            sys.stdout.flush()
        return False, "the look did not finish within %.0f s" % timeout
    except OSError as exc:
        return False, "the look could not start (%s)" % exc
    if r.stdout:
        sys.stdout.write(r.stdout)
        sys.stdout.flush()
    if r.stderr and r.returncode == 0:
        sys.stderr.write(r.stderr)                  # a look's own notes, as before; a failure's tail is told below
        sys.stderr.flush()
    if r.returncode != 0:
        tail = " ".join((r.stderr or "").strip().splitlines()[-2:])[:200]
        return False, "the look exited %d%s" % (r.returncode, (": " + tail) if tail else "")
    path = os.path.join(sd, "evaluated.json")
    ev = stall_watch._read_json(path)
    if not isinstance(ev.get("at"), (int, float)) or ev["at"] < started:
        return False, "the look recorded no evaluated source"
    if ev.get("identity") != before:
        stall_watch._write_json(path, dict(ev, identity="changed-during-the-look"))
    return True, ""


def run_loop(a, engine_root):
    sid, spid = current_session(engine_root)
    sd = session_dir(sid)
    fd = stall_watch._try_lock(os.path.join(sd, "monitor.lock"))
    if fd is None:
        return 0                                    # this session is already watched
    prune(sd)
    alive = stall_watch.session_alive_check(spid) if spid else None
    poll = stall_watch._env_float("REVIEW_WATCH_POLL_SECONDS", POLL_SECONDS)
    failing = ""
    try:
        while alive is None or alive():
            ok, why = _child_look(a, engine_root, sid, sd)
            if not ok:
                # Beside the last successful look, so --status reads a failing child as failing.
                path = os.path.join(sd, "evaluated.json")
                stall_watch._write_json(path, dict(stall_watch._read_json(path), failed={"at": time.time(), "why": why}))
                if why != failing:                  # once per failing streak and reason
                    # What the failed look did before it ended (a review started or settled, a
                    # notice printed) is not known, so nothing here claims it did nothing.
                    sys.stdout.write("REVIEW-WATCH %s: a look failed (%s); what it did before it ended is not "
                                     "known. The next look tries again. Check: review-watch.sh --status\n"
                                     % (hhmm(clock()), why))
                    sys.stdout.flush()
            failing = "" if ok else why
            stall_watch._nap(poll, alive)
    finally:
        os.close(fd)
    return 0


# The host's own process id, which richos-core review_watch.rs sets in its child's environment.
HOST_ENV = "RICHOS_REVIEW_WATCH_HOST"


def run_host_loop(watcher, name):
    """A watcher that is the app host's own child (richos-core review_watch.rs): the app's
    (--app-state, name "app") or the operator install's (--host-json, name "operator-host").

    It ends with the app, three ways, and each one stops the reviews it started first:
    SIGTERM (the host's quit path), SIGHUP, and its parent going away (an app that crashed or
    was killed: the parent process id changes when the host dies, so a review never runs on for
    an app that is gone).

    THE HOST IS KNOWN BEFORE THE FIRST LOOK (the real second review of 16c154f5a, finding 2): the
    host passes its own pid at spawn (HOST_ENV), so a host that died while this process started,
    which leaves it reparented already, is seen gone here and nothing is looked at or started.
    Without it (a direct run), the parent at this point is the host."""
    host = os.environ.pop(HOST_ENV, "").strip()     # never inherited by the reviews
    parent = int(host) if host.isdigit() else os.getppid()

    def alive():
        return os.getppid() == parent

    if not alive():
        return 0                                    # its host is gone already
    sd = session_dir(name)
    fd = stall_watch._try_lock(os.path.join(sd, "monitor.lock"))
    if fd is None:
        return 0                                    # another copy of the app already watches

    def ended(signum, _frame):
        if getattr(watcher, "hold_quit", 0):
            watcher.held_quit = signum              # spawn takes it once its child is registered
            return
        raise SystemExit(128 + signum)

    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, ended)
    prune(sd)
    poll = stall_watch._env_float("REVIEW_WATCH_POLL_SECONDS", POLL_SECONDS)
    try:
        while alive():
            tick(watcher, sd)
            stall_watch._nap(poll, alive)
    finally:
        for sig in (signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, signal.SIG_IGN)
        watcher.stop_own(clock())
        os.close(fd)
    return 0


def app_mode(a, engine_root):
    """--app-state: the app's watcher. Its state and its ledger live under the app's own
    engine state (app_paths); the transcripts its reviews read are the app's platform record."""
    paths = app_paths(a.app_state)
    os.environ["REVIEW_WATCH_STATE_DIR"] = paths["watch"]
    os.environ["SECOND_REVIEW_STATE_DIR"] = paths["reviews"]
    if not (os.environ.get("RICHOS_PROJECTS_DIR") or "").strip():
        os.environ["RICHOS_PROJECTS_DIR"] = os.path.join(os.path.realpath(a.app_state), "platform-projects")
    watcher = Watcher(engine_root, "", AppWorld(engine_root, a.app_state, a.claude, a.accounts, a.codex_reviews))
    if a.status:
        for path in sorted(glob.glob(_p("locks", "*.lock"))):
            info = stall_watch._read_json(path)
            print("  running: %s %s@%s (%s) since %s, pid %s" % (
                info.get("name"), os.path.basename(info.get("repo") or ""), str(info.get("tip"))[:12],
                info.get("trigger"), iso(float(info.get("started_at") or 0)), info.get("pid")))
        return 0
    if a.tick:
        tick(watcher, session_dir("app"))
        return 0
    return run_host_loop(watcher, "app")


def mode_status(engine_root):
    sid, _pid = current_session(engine_root)
    sd = _p("sessions", sid or "no-session")
    held = stall_watch.lock_held(os.path.join(sd, "monitor.lock"))
    # Watching is a held lock AND a recent successful look, as stall-watch's --alive reads it:
    # the lock alone is also held by a monitor whose every look fails.
    ev = stall_watch._read_json(os.path.join(sd, "evaluated.json"))
    last = ev.get("at") if isinstance(ev.get("at"), (int, float)) else None
    failed = ev.get("failed") or {}
    poll = float(ev.get("poll") or POLL_SECONDS)
    watching = False
    if not held:
        line = "NOT WATCHING this session"
    elif failed and float(failed.get("at") or 0) >= float(last or 0):
        line = "LOOKS FAILING since %s: %s" % (iso(float(failed.get("at") or 0)), failed.get("why"))
    elif not last:
        told = stall_watch._read_json(os.path.join(sd, "told.json")).get("last_look")
        line = ("OLD MONITOR: it looks in-process with the code it started with (last look %s); it is replaced "
                "when its session restarts" % iso(told) if told else "the monitor started; no look finished yet")
    elif time.time() - last > 3 * poll + 30:
        line = "STOPPED LOOKING: the last successful look was %s (it looks every %.0f s)" % (iso(last), poll)
    else:
        watching = True
        line = "watching this session; last look %s, source %s" % (iso(last), ev.get("identity"))
    print("review-watch: %s" % line)
    for path in sorted(glob.glob(_p("locks", "*.lock"))):
        info = stall_watch._read_json(path)
        print("  running: %s %s@%s (%s, attempt %s) since %s, pid %s" % (
            info.get("name"), os.path.basename(info.get("repo") or ""), str(info.get("tip"))[:12],
            info.get("trigger"), info.get("attempt"), iso(float(info.get("started_at") or 0)), info.get("pid")))
    return 0 if watching else 1


def main(argv):
    ap = argparse.ArgumentParser(prog="review-watch.sh")
    g = ap.add_mutually_exclusive_group(required=True)
    for flag in ("--monitor", "--tick", "--status"):
        g.add_argument(flag, action="store_true")
    ap.add_argument("--config", default="")
    ap.add_argument("--engine-root", default="")
    ap.add_argument("--app-state", default="", help="the app's engine state: the app's own watcher (AppWorld)")
    ap.add_argument("--claude", default="", help="app mode: the Claude CLI the app ships")
    ap.add_argument("--accounts", default="",
                    help="app mode: the app's Claude account list; each review runs on the account in use")
    ap.add_argument("--codex-reviews", default="",
                    help="app mode: the app's Codex review switch (codex-reviews.json); on, Codex reviews when it can")
    ap.add_argument("--host-json", action="store_true",
                    help="the operator host's child: notices as JSON lines, one per lead session")
    ap.add_argument("--monitor-look", action="store_true", help=argparse.SUPPRESS)  # --tick: the monitor's own look
    a = ap.parse_args(argv)
    engine_root = a.engine_root or os.path.dirname(os.path.dirname(HERE))
    if a.app_state:
        return app_mode(a, engine_root)
    if a.status:
        return mode_status(engine_root)
    if not a.config:
        return 0                                    # a repository that never adopted the engine: nothing to watch
    watcher = Watcher(engine_root, a.config)
    if a.host_json:
        # The operator install's host runs this as its child and sends each line's text to the
        # lead of that session as a message of its own (Sage's check §1.3): a monitor inside a
        # print-mode lead would start a turn the host never asked for and cannot attribute.
        HOST_JSON["on"] = True
        if a.monitor:
            return run_host_loop(watcher, "operator-host")
    if a.tick:
        sid, _pid = current_session(engine_root)
        sd = session_dir(sid)
        if a.monitor_look:
            MONITOR["session"] = sid
        failed = tick(watcher, sd, tell_failure=not a.monitor_look)
        if a.monitor_look:
            if failed:
                # A look whose evaluation raised is a failed look (the second review of 5331505bd,
                # finding 1): no evaluation is recorded, and the monitor tells it.
                sys.stderr.write("the evaluation raised %s\n" % failed)
                return 1
            # The source on disk AFTER the look; the monitor compares it with the source when
            # this child started (_child_look).
            stall_watch._write_json(os.path.join(sd, "evaluated.json"), {
                "identity": source_identity(engine_root), "at": time.time(), "pid": os.getpid(),
                "poll": stall_watch._env_float("REVIEW_WATCH_POLL_SECONDS", POLL_SECONDS)})
        return 0
    return run_loop(a, engine_root)


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)

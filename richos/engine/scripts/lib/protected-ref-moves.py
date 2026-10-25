#!/usr/bin/env python3
"""protected-ref-moves.py — THE READER FOR A CHECK THAT STOPPED WRITING.

===========================================================================
WHY THIS FILE EXISTS
===========================================================================
On 2026-09-14 `_restore_protected_refs` in mega-lander/workspaces.py stopped
moving a protected ref back. It had moved `refs/heads/main` in /Users/alex/ab/
richos three times in one night — the second and third twelve seconds apart, in
opposite directions, while the lead was landing (docs/verification/
ref-write-forensics-2026-09-14.md; re-derived from the store's own event log:
three `protected-ref-restored` rows, at 22:41:29Z, 00:09:14Z and 00:09:26Z).
The check cannot tell the lead's ordinary land from the abuse it hunts, so it
now REPORTS a move and leaves the ref alone.

That trade was raised by the engineer who made it, in his own words: *"who
reads that notice is not mine to decide."* It is the right trade and it opened
a real gap, because of where the report went. Measured, not assumed:

  1. a row in the store's event log      nothing reads it   (no hook, no
     (`protected-ref-moved`)             script, no gate — grepped)
  2. a row on the agent's own record     nothing surfaces it
  3. a `=== PROTECTED REF MOVED ... ===` stderr of a PostToolUse hook that
     line from observe-created-refs.sh   exits 0 — the transcript, never the
                                         operator's scroll (the measured
                                         channel table in
                                         scripts/lib/stop-hook-notice.sh),
                                         and it is printed into the session of
                                         the AGENT THAT MADE THE MOVE, which
                                         is the one party that cannot decide
                                         whether the move was allowed.

So the automatic action was replaced by a notice with no reader. This file is
the reader. It is deliberately NOT a guard: it refuses nothing, blocks nothing
and writes to no repository. The check stopped writing because its inference
was wrong; a blocking version of the same inference would be the same defect
wearing a heavier coat.

===========================================================================
THE PREDICATE — ONE MECHANICAL QUESTION, ASKED LATER
===========================================================================
    IS THE TIP THE BRANCH HELD WHEN THE AGENT'S CALL STARTED STILL ON THE
    BRANCH?

That is the whole rule. It has no threshold, no heuristic and no opinion about
who moved anything. It answers whether commits are OFF THE BRANCH, never whether
they were lost: a relocation or a rewrite can keep the work elsewhere, so the
notice states the tip, the count off the branch and the age, and nothing more.

  * `git merge-base --is-ancestor <snapshot tip> <branch now>` succeeds
    -> HEALED. Everything that was on the branch is still on it. This is the
    lead's ordinary land, the agent's doorway merge that was then landed, a
    deletion that was re-created — every benign shape collapses here and is
    never mentioned. It is also what the three real 2026-09-13/14 incidents
    look like today, because a human recovered them.
  * it fails -> OPEN. Commits that were on a protected branch are not on it
    any more. Nothing about that is ambiguous and nothing about it self-heals.
  * it cannot be asked (the repository, the branch or the object is gone)
    -> UNDECIDABLE, and an undecidable finding is announced exactly as loudly
    as an open one. Absence is never evidence, and INDETERMINATE is never
    collapsed into either answer.

WHY THE CHECK ITSELF CANNOT ASK THIS. `_restore_protected_refs` runs at a
PostToolUse, in the middle of a land, with one agent's minutes-stale snapshot
and no idea what the next ten seconds hold. At that instant "main moved" and
"commits were lost" are indistinguishable — the reproduction's two modes have
the identical shape and opposite consequences. At REST, after the turn, they
are two different facts and one `is-ancestor` separates them. The split is the
point: the check observes, the reader decides, and neither guesses.

===========================================================================
WHAT CLOSES A FINDING
===========================================================================
Two things, and the first is why this does not become wallpaper:

  * THE TREE HEALS ITSELF. The commits come back — by a merge, a reset, a
    cherry-pick, anything — and the finding is silent from the next turn on,
    with nobody told to close it. Most findings will end this way.
  * THE PERSON SETTLES IT. Rewinding a recorded branch to drop a bad land is a
    legitimate act and it loses commits on purpose, so it must be closable by
    saying so:

        protected-ref-moves.py review --repo <r> --branch <b> --tip <sha>
                                      --why "<why this loss was intended>"

    which appends one `protected-ref-move-reviewed` row to the SAME event log,
    through workspaces.py's own `event()`. One writer, one substrate, no
    sidecar ledger: two ledgers written by one writer and read by nobody is how
    the waiver count on this machine reached 251.

`--why` is required and is not allowed to be blank. A settlement with no reason
is a mute button, and a mute button on the last surviving notice over a ref
that only the lead may move is how this mechanism dies quietly — which is the
one outcome the CEO named when he asked for an owner.

===========================================================================
WHAT IT NEVER DOES
===========================================================================
  * It never writes a ref, never moves one back, and never runs a git command
    that mutates anything. Every git call here is a read.
  * It never edits, closes or re-stamps a row it did not write this run.
  * It never decides that a loss was acceptable. It decides whether commits
    are missing, which is a fact, and says so until a person answers it.

===========================================================================
COMMANDS
===========================================================================
    protected-ref-moves.py hook-summary   three lines for the Stop hook:
                                          state key / count / one sentence
    protected-ref-moves.py list           one readable block per finding
    protected-ref-moves.py review ...     settle one finding, with a reason

Exit codes: 0 clear, 1 findings outstanding, 2 the predicate could not run.
A caller that cannot tell 2 from 0 is a caller that reads an absent reader as a
clean report, so 2 is never returned quietly.
"""

import argparse
import calendar
import hashlib
import json
import os
import subprocess
import sys
import time

# THE STORE IS workspaces.py's STORE, RESOLVED BY workspaces.py. Importing it
# costs ~40 ms including interpreter start (measured 2026-09-14) and removes a
# drift surface that would otherwise be a copy of `state_dir()` going stale in
# a second file. `event()` comes from the same import, so this file adds no
# second writer to the log it reads.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "mega-lander"))
try:
    import workspaces  # noqa: E402
except Exception as _e:  # reported, never guessed around
    sys.stderr.write("protected-ref-moves: mega-lander/workspaces.py could not be "
                     "imported (%s), so the store that holds every protected-ref "
                     "finding cannot be located.\n" % _e)
    sys.exit(2)

# Every event class that carries a protected ref's SNAPSHOT TIP. All three are
# swept by the one rule, which is why there is no per-class branch below:
#   protected-ref-moved           the ref moved and was left alone (since 2026-09-14)
#   protected-ref-restored        the ref was deleted and put back (still automatic)
#   protected-ref-restore-failed  the ref was deleted and could NOT be put back
CLASSES = ("protected-ref-moved", "protected-ref-restored", "protected-ref-restore-failed")
SETTLED = "protected-ref-move-reviewed"

# A bound on the read, not on the truth: the newest 20000 PROTECTED-REF rows are
# read, however much unrelated traffic follows them. That is about ninety times
# the whole log on this machine on the day this was written, and it keeps what
# is held in memory flat as the log grows.
MAX_LINES = 20000

# The age rungs the sentence names, in seconds. The same three the escalation
# ladder uses, for the same reason: a condition that never changes is announced
# once and then never again, and crossing a rung is a state change.
RUNGS = (3600, 86400, 259200)


def _rows():
    """Every protected-ref row in the store's event log, oldest first.

    Raises OSError if the log cannot be read. A log that is ABSENT is not an
    error — an engine that has never observed a protected ref has none to
    report — and is told apart from an unreadable one by that distinction."""
    path = os.path.join(workspaces.state_dir(), "events.jsonl")
    if not os.path.exists(path):
        return []
    # THE BOUND IS ON PROTECTED-REF ROWS, NOT ON THE FILE'S LAST LINES. The log
    # is shared with every workspace event; cutting it at its last N lines
    # before filtering let ordinary traffic push an unanswered protected-ref row
    # off the end, and the Stop warning vanished while commits were still lost.
    # The file is streamed and only matching lines are kept, so the cost stays
    # flat in memory and the newest MAX_LINES protected-ref rows survive.
    from collections import deque
    kept = deque(maxlen=MAX_LINES)
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if "protected-ref" in line:
                kept.append(line)
    out = []
    for line in kept:
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if isinstance(d, dict) and d.get("event") in CLASSES + (SETTLED,):
            out.append(d)
    return out


def _git(repo, *args):
    """A READ of a repository, and the only kind of git call in this file."""
    env = dict(os.environ)
    for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
        env.pop(k, None)
    env["LC_ALL"] = "C"
    try:
        return subprocess.run(["git", "-C", repo] + list(args),
                              capture_output=True, text=True, timeout=20, env=env)
    except (OSError, subprocess.SubprocessError):
        return None


def _ident(d):
    """(repo, branch, tip) — what a finding IS, and what a settlement names.

    The timestamp is deliberately not part of it: the same ref losing the same
    tip twice is one fact about the tree, and a settlement of it settles it."""
    return (str(d.get("repo") or ""), str(d.get("branch") or ""), str(d.get("tip") or ""))


def _epoch(ts):
    """The row's timestamp as an epoch, or None.

    `calendar.timegm`, NEVER `time.mktime(...) - time.timezone`. The log writes
    UTC with a trailing Z (workspaces.py `iso()`), and the subtraction form gets
    the STANDARD offset rather than the one in force, so on a machine observing
    summer time it reports every finding an hour older than it is. Measured on
    2026-09-14 in Europe/London: a row written seconds earlier was announced as
    "1 h ago", which is the age rung the notice escalates on."""
    try:
        return calendar.timegm(time.strptime(str(ts), "%Y-%m-%dT%H:%M:%SZ"))
    except (ValueError, TypeError, OverflowError):
        return None


def _age_bucket(ts):
    """0 under an hour, 1 over an hour, 2 over a day, 3 over three days."""
    t = _epoch(ts)
    if t is None:
        return 0
    age = max(0.0, time.time() - t)
    return sum(1 for r in RUNGS if age >= r)


def _age_words(ts):
    t = _epoch(ts)
    if t is None:
        return "at an unrecorded time"
    age = max(0, int(time.time() - t))
    if age < 3600:
        return "%d min ago" % (age // 60)
    if age < 86400:
        return "%d h ago" % (age // 3600)
    return "%d days ago" % (age // 86400)


def classify():
    """[finding] for every protected-ref row that is neither settled nor healed.

    A finding is a dict: state (OPEN | UNDECIDABLE), repo, branch, tip, ts, why,
    agent, lost (commit count, or None), detail (for UNDECIDABLE)."""
    rows = _rows()
    settled = set(_ident(d) for d in rows if d.get("event") == SETTLED)

    # LAST ROW WINS PER IDENTITY. A branch observed twice with the same snapshot
    # tip is one fact, and the newest row carries the newest `found`.
    latest = {}
    for d in rows:
        if d.get("event") not in CLASSES:
            continue
        ident = _ident(d)
        if not ident[0] or not ident[1] or not ident[2]:
            continue
        if ident in settled:
            continue
        latest[ident] = d

    findings = []
    for ident, d in sorted(latest.items(), key=lambda kv: str(kv[1].get("ts") or "")):
        repo, branch, tip = ident
        base = {"repo": repo, "branch": branch, "tip": tip,
                "ts": str(d.get("ts") or ""), "why": str(d.get("why") or ""),
                "event": str(d.get("event") or ""),
                "agent": str(d.get("key") or "").split("--")[-1],
                "lost": None, "detail": "", "now": ""}

        if not os.path.isdir(repo):
            base.update(state="UNDECIDABLE", detail="the repository is not on disk at %s" % repo)
            findings.append(base)
            continue
        r = _git(repo, "rev-parse", "--verify", "--quiet", "refs/heads/%s" % branch)
        if r is None:
            base.update(state="UNDECIDABLE", detail="git could not be run in %s" % repo)
            findings.append(base)
            continue
        if r.returncode != 0 or not r.stdout.strip():
            base.update(state="UNDECIDABLE",
                        detail="the branch %s does not exist in %s any more, so whether its "
                               "commits survive cannot be asked of it" % (branch, repo))
            findings.append(base)
            continue
        now_tip = r.stdout.strip()
        base["now"] = now_tip
        e = _git(repo, "cat-file", "-e", "%s^{commit}" % tip)
        if e is None or e.returncode != 0:
            base.update(state="UNDECIDABLE",
                        detail="the object %s is no longer in %s's object store, so the commits "
                               "it named are gone rather than merely off the branch"
                               % (tip[:12], repo))
            findings.append(base)
            continue
        a = _git(repo, "merge-base", "--is-ancestor", tip, now_tip)
        if a is None:
            base.update(state="UNDECIDABLE", detail="git could not be run in %s" % repo)
            findings.append(base)
            continue
        if a.returncode == 0:
            continue                      # HEALED: what was on the branch is still on it
        c = _git(repo, "rev-list", "--count", "%s..%s" % (now_tip, tip))
        if c is not None and c.returncode == 0 and c.stdout.strip().isdigit():
            base["lost"] = int(c.stdout.strip())
        base.update(state="OPEN")
        findings.append(base)
    return findings


def state_key(findings):
    """`clear`, or a key that changes when the SET changes or any member ages
    across a rung. The digest is over the identities and their buckets, so a
    finding that heals and a finding that arrives both change it."""
    if not findings:
        return "clear"
    parts = sorted("%s|%s|%s|%s|%d" % (f["state"], f["repo"], f["branch"], f["tip"],
                                       _age_bucket(f["ts"])) for f in findings)
    digest = hashlib.sha1("\n".join(parts).encode("utf-8")).hexdigest()[:8]
    o = sum(1 for f in findings if f["state"] == "OPEN")
    u = len(findings) - o
    return "prm-o%d-u%d-%s" % (o, u, digest)


def _one_line(f):
    who = ("%s's tool call" % f["agent"]) if f["agent"] else "an agent's tool call"
    head = "%s in %s" % (f["branch"], f["repo"])
    if f["state"] == "UNDECIDABLE":
        return ("%s CANNOT BE CHECKED: it was reported moved during %s %s and %s"
                % (head, who, _age_words(f["ts"]), f["detail"]))
    if f["lost"] == 1:
        lost = "1 commit is off the branch"
    elif f["lost"]:
        lost = "%d commits are off the branch" % f["lost"]
    else:
        lost = "commits are off the branch"
    return ("%s no longer contains %s, the tip it held when %s started %s — %s"
            % (head, f["tip"][:12], who, _age_words(f["ts"]), lost))


def sentence(findings):
    """ONE LINE. The host prefixes every line with `Stop says: `, so a multi-line
    message is a message that renders badly and gets skipped."""
    if not findings:
        return "PROTECTED REF WATCH: clear again — every protected ref holds the commits it held."
    first = findings[0]
    more = ""
    if len(findings) > 1:
        more = " (and %d more — scripts/lib/protected-ref-moves.py list)" % (len(findings) - 1)
    settle = ("scripts/lib/protected-ref-moves.py review --repo %s --branch %s --tip %s --why '<reason>'"
              % (first["repo"], first["branch"], first["tip"][:12]))
    # ONLY WHAT IS ESTABLISHED. Non-ancestry says the tip is off the branch; it
    # does not say the work was lost (a relocation or a rewrite keeps it
    # elsewhere) nor that nobody acted, so the sentence claims neither
    # (2026-10-24: main's 0899f054bf3d moved a ledger to the private record on
    # purpose, and this line called it lost and ignored for 35 days).
    return ("PROTECTED REF WATCH: %s%s. No settlement is recorded. The engine reports this and "
            "never moves a ref back (docs/verification/ref-write-forensics-2026-09-14.md). "
            "Look: git -C %s reflog show %s --date=iso. If those commits belong on the branch, "
            "restore them; if the move was intended, settle it with the reason: %s"
            % (_one_line(first), more, first["repo"], first["branch"], settle))


def cmd_hook_summary():
    findings = classify()
    print(state_key(findings))
    print(len(findings))
    print(sentence(findings))
    return 1 if findings else 0


def cmd_list():
    findings = classify()
    if not findings:
        print("clear: no protected ref is missing a tip it held when an agent's call started.")
        return 0
    for f in findings:
        print("%-12s %s" % (f["state"], _one_line(f)))
        print("             reported %s by %s as: %s"
              % (f["ts"], f["event"], f["why"] or "(no reason recorded)"))
        if f.get("now"):
            print("             the branch is at %s now; it was at %s then"
                  % (f["now"][:12], f["tip"][:12]))
        print("             settle: scripts/lib/protected-ref-moves.py review --repo %s --branch %s "
              "--tip %s --why '<reason>'" % (f["repo"], f["branch"], f["tip"][:12]))
    return 1


def cmd_review(args):
    why = (args.why or "").strip()
    if not why:
        sys.stderr.write("review needs --why: a settlement with no reason is a mute button, and a "
                         "mute button on the only notice over a ref that only the lead may move is "
                         "how this mechanism dies quietly.\n")
        return 2
    # The tip is matched against the FULL sha recorded in the log, so an
    # abbreviated --tip settles the finding it names rather than nothing.
    want = (os.path.realpath(os.path.expanduser(args.repo)), args.branch)
    target = None
    for f in classify():
        if (os.path.realpath(f["repo"]), f["branch"]) == want and f["tip"].startswith(args.tip):
            target = f
            break
    if target is None:
        sys.stderr.write("nothing outstanding matches repo=%s branch=%s tip=%s. "
                         "`list` shows what is.\n" % (args.repo, args.branch, args.tip))
        return 2
    workspaces.event(SETTLED, repo=target["repo"], branch=target["branch"], tip=target["tip"],
                     of=target["event"], state=target["state"], why=why,
                     by=(os.environ.get("USER") or "unknown"))
    print("settled: %s in %s, tip %s. The reason is recorded in the store's event log."
          % (target["branch"], target["repo"], target["tip"][:12]))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("hook-summary")
    sub.add_parser("list")
    r = sub.add_parser("review")
    r.add_argument("--repo", required=True)
    r.add_argument("--branch", required=True)
    r.add_argument("--tip", required=True)
    r.add_argument("--why", required=True)
    a = ap.parse_args(argv)
    try:
        if a.cmd == "hook-summary":
            return cmd_hook_summary()
        if a.cmd == "list":
            return cmd_list()
        if a.cmd == "review":
            return cmd_review(a)
    except OSError as e:
        sys.stderr.write("protected-ref-moves: the store could not be read (%s). This is not a "
                         "clean report.\n" % e)
        return 2
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

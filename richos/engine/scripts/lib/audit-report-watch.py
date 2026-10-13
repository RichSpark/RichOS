#!/usr/bin/env python3
"""audit-report-watch.py - AN AUDIT REPORT THAT CHANGES AFTER ITS FIXES WERE
BRIEFED REACHES THE LEAD, BY ID.

THE FAILURE (2026-09-30). Codex rewrote two hunt reports after the fix briefs
had been written from the earlier versions: part 1 in richos-hq bf59c54b and
part 5 in 800a9969. The rewrites added findings (part 1: 40, 44, 46; part 5:
P5-36, P5-60, P5-61). Nothing told the lead, so six severity 1 findings sat
unfixed until a count found them. The report was updated; the people fixing
from it were not.

THE MECHANISM. A sweep, not a commit hook, because the rewrite came from a
different agent in a different repository, and a hook in one repository's
.git is invisible to the engine and to every other committer. The sweep keeps,
per report, the finding IDs and a hash of each finding's text as last seen.
Each run compares the report at the record repository's HEAD with that memory.
A finding that is new, or whose text changed, raises ONE escalation through
escalate.sh naming the report, the commit that last touched it and the IDs.
The escalation then reaches the lead through the path every escalation takes
(scripts/hooks/notice-escalations.sh, at every turn end, in the lead's own
context). notice-escalations.sh runs this sweep just before it reads the ledger.

ORDER MATTERS. The escalation is raised first and the memory is written
second, so a raise that fails is retried at the next turn end instead of being
lost. A crash between the two costs one duplicate escalation, never a silent one.

THE FIRST SIGHTING OF A REPORT IS A BASELINE AND SAYS NOTHING. Announcing
every finding of every existing report the day this ships would be noise
that teaches the lead to skip the line. A report first seen later than the
baseline run, with findings, is announced whole: that is new work to fix.

A FINDING is a markdown heading of level 2-4 that begins with an ID: digits
("## 40. Title") or a prefixed ID ("### P5-36: Title"). Its text is everything
up to the next heading of the same or a higher level.

Exit 0: swept (raised or quiet). Exit 2: could not sweep; the reason is on
stderr, and the caller says so audibly.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

HEADING = re.compile(r"^(#{2,4})\s+((?:[A-Z]+\d*-)?\d+)\s*[.:)]\s*(.*)$")
ENGINE_SCRIPTS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True,
                          text=True, check=True).stdout


def parse_findings(text):
    """Return {id: (title, sha1 of the finding's text)}."""
    found, cur, level, buf = {}, None, 0, []

    def close():
        if cur:
            digest = hashlib.sha1("\n".join(buf).strip().encode()).hexdigest()
            found[cur[0]] = (cur[1], digest)

    fence = None  # (char, length) of the open code fence: its lines are never headings (P5-75)
    for line in text.splitlines():
        fm = re.match(r"^ {0,3}(`{3,}|~{3,})", line)
        if fence is not None:
            if fm and fm.group(1)[0] == fence[0] and len(fm.group(1)) >= fence[1] \
                    and not line.strip()[len(fm.group(1)):].strip():
                fence = None
            if cur is not None:
                buf.append(line)
            continue
        if fm:
            fence = (fm.group(1)[0], len(fm.group(1)))
            if cur is not None:
                buf.append(line)
            continue
        m = re.match(r"^(#{1,6})\s", line)
        if m and (cur is None or len(m.group(1)) <= level):
            close()
            cur = None
            h = HEADING.match(line)
            if h:
                cur, level, buf = (h.group(2), h.group(3).strip()), len(h.group(1)), []
                continue
        if cur is not None:
            buf.append(line)
    close()
    return found


def reports(repo, subdir):
    out = git(repo, "ls-tree", "-r", "HEAD", "--", subdir + "/")
    for row in out.splitlines():
        meta, path = row.split("\t", 1)
        if path.endswith(".md"):
            yield path, meta.split()[2]


def load(state_file):
    try:
        with open(state_file) as f:
            return json.load(f)
    except FileNotFoundError:
        return None


def raise_escalation(repo, path, commit, new, changed, scratch):
    ids = ", ".join(new + changed)
    parts = []
    if new:
        parts.append("new: " + ", ".join(new))
    if changed:
        parts.append("changed: " + ", ".join(changed))
    fields = {
        "title": "Audit report %s changed in %s: %s" % (path, commit, "; ".join(parts)),
        "state": "work-complete",
        "question": (
            "Commit %s changed %s in %s (%s). Fix briefs written from the earlier "
            "version do not cover these findings. Check whether each is fixed or "
            "briefed, and brief the ones that are not." % (commit, path, repo, "; ".join(parts))),
        "for": "lead",
        "tried": "Nothing; this notice is raised by audit-report-watch.py, not by a teammate.",
        "meanwhile": "No action is taken by the watcher. Finding IDs: " + ids,
    }
    os.makedirs(scratch, exist_ok=True)
    fpath = os.path.join(scratch, "esc-%d.json" % os.getpid())
    with open(fpath, "w") as f:
        json.dump(fields, f)
    try:
        subprocess.run([os.path.join(ENGINE_SCRIPTS, "escalate.sh"), "raise",
                        "--fields", fpath, "--worktree", repo,
                        "--teammate", "audit-report-watch", "--no-record"],
                       capture_output=True, text=True, check=True)
    except subprocess.CalledProcessError as e:
        raise RuntimeError("escalate.sh raise failed (exit %d): %s" % (e.returncode, e.stderr.strip()))
    finally:
        try:
            os.unlink(fpath)
        except OSError:
            pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=os.environ.get("RICHOS_AUDIT_RECORD_REPO"))
    ap.add_argument("--subdir", default="docs/audits")
    ap.add_argument("--state-dir", default=os.environ.get("RICHOS_AUDIT_WATCH_STATE")
                    or os.path.expanduser("~/.claude/state/audit-report-watch"))
    a = ap.parse_args()

    if a.repo is None:
        source = Path(ENGINE_SCRIPTS)
        if not any(os.path.lexists(parent / ".git") for parent in (source, *source.parents)):
            return 0  # extracted engines have no implicit record repository
        try:
            common = subprocess.check_output(
                ["git", "-C", ENGINE_SCRIPTS, "rev-parse", "--path-format=absolute", "--git-common-dir"],
                text=True, stderr=subprocess.PIPE, timeout=30).strip()
        except (OSError, subprocess.SubprocessError) as e:
            print("audit-report-watch: cannot resolve record repository: %s" % e, file=sys.stderr)
            return 2
        a.repo = str(Path(common).parent.parent / "richos-hq")

    if not os.path.isdir(os.path.join(a.repo, ".git")) and not os.path.isfile(os.path.join(a.repo, ".git")):
        return 0  # no record repository on this machine: nothing to watch
    os.makedirs(a.state_dir, exist_ok=True)
    state_file = os.path.join(a.state_dir, "seen.json")
    seen = load(state_file)
    baseline = seen is None
    seen = seen or {}
    raised = 0
    try:
        for path, blob in reports(a.repo, a.subdir):
            prev = seen.get(path)
            if prev and prev["blob"] == blob:
                continue
            text = git(a.repo, "show", "HEAD:" + path)
            cur = parse_findings(text)
            if not cur:
                continue
            entry = {"blob": blob, "findings": {k: v[1] for k, v in cur.items()}}
            if baseline:
                seen[path] = entry
                continue
            old = prev["findings"] if prev else {}
            new = sorted(k for k in cur if k not in old)
            changed = sorted(k for k in cur if k in old and old[k] != cur[k][1])
            if new or changed:
                commit = git(a.repo, "log", "-1", "--format=%h", "--", path).strip() or "HEAD"
                raise_escalation(a.repo, path, commit, new, changed, os.path.join(a.state_dir, "tmp"))
                raised += 1
            seen[path] = entry
    except (RuntimeError, subprocess.CalledProcessError, OSError) as e:
        print("audit-report-watch: %s" % e, file=sys.stderr)
        return 2
    finally:
        tmp = state_file + ".tmp"
        with open(tmp, "w") as f:
            json.dump(seen, f, indent=1, sort_keys=True)
        os.replace(tmp, state_file)
    return 0


if __name__ == "__main__":
    sys.exit(main())

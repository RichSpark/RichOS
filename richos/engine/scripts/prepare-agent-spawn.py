#!/usr/bin/env python3
"""Build an isolated Agent tool input with the mandatory merge-at-handover line
(main, or the branch a teammate bound to a release branch lands on).

Read task-specific Agent input JSON from --file (or stdin), print prepared JSON.
This creates no worktree, dispatches nobody and grants no guard exemptions.
Cross-repo work still needs create-teammate-worktree.sh and its registered path
on a cross-repo-worktree: prompt line. Live guards remain authoritative.
"""
import argparse
import json
from pathlib import Path
import re
import sys


def last_step(targets=None):
    """The handover's last step: merge the branch the work lands on into the
    teammate's own worktree. `targets`: [(repository, branch)] for every
    repository it works in. With none, or all of them on main, it is the
    merge-main line, word for word. A teammate bound to a release branch merges
    THAT branch (2026-10-14: the lead cut the merge-main line out of five v1.3.0
    briefs by hand, because it would have pulled main into the release)."""
    branches = []
    for _repo, branch in targets or []:
        if branch and branch not in branches:
            branches.append(branch)
    if not branches or branches == ["main"]:
        return ("Your last step before you hand over, and only if you committed anything: in each repository you committed to, "
                "run `git merge main` inside your own worktree. `main` here is the local branch named `main`, not `origin/main`. "
                "Never rebase. If git reports a conflict, resolve it and commit the merge. If the conflict is in a screenshot "
                "baseline picture (a `.png` under `ui/tests/shots-*`), regenerate that picture from the merged tree and look at it: "
                "if it shows anything besides your change and main's change together, do not commit it; say what it shows in your "
                "report. Merge only once; if main moves again after your merge, do not merge again. Then hand over.")
    if len(branches) == 1:
        b = branches[0]
        which = ("run `git merge %s` inside your own worktree. `%s` here is the local branch named `%s`, not "
                 "`origin/%s`; it is the branch your work lands on, and no other branch is merged into your work."
                 % (b, b, b, b))
        theirs, moves = "%s's change" % b, "%s moves" % b
    else:
        each = "; ".join("`git merge %s` in %s" % (b, r) for r, b in targets)
        which = ("merge the branch your work lands on there, inside your own worktree: %s. Each is the local branch "
                 "of that name, not its `origin/` copy, and no other branch is merged into your work." % each)
        theirs, moves = "that branch's change", "that branch moves"
    return ("Your last step before you hand over, and only if you committed anything: in each repository you "
            "committed to, %s Never rebase. If git reports a conflict, resolve it and commit the merge. If the "
            "conflict is in a screenshot baseline picture (a `.png` under `ui/tests/shots-*`), regenerate that "
            "picture from the merged tree and look at it: if it shows anything besides your change and %s "
            "together, do not commit it; say what it shows in your report. Merge only once; if %s again after "
            "your merge, do not merge again. Then hand over." % (which, theirs, moves))


def prepare(value, targets=None):
    """`targets`: [(repository, the branch its work lands on)]; see last_step."""
    if not isinstance(value, dict):
        raise ValueError("input must be an Agent tool-input object")
    problems = [f"missing {key}" for key in ("name", "subagent_type", "prompt")
                if not isinstance(value.get(key), str) or not value[key].strip()]
    if value.get("resume"):
        problems.append("this helper prepares new spawns; use the resume contract for an existing agent")
    if value.get("cwd"):
        problems.append("cwd cannot carry a native lifecycle witness; use a registered cross-repo-worktree: prompt line")
    if value.get("isolation") not in (None, "worktree"):
        problems.append("this helper prepares isolation=worktree only; it will not override another isolation mode")
    if problems:
        raise ValueError("; ".join(problems))
    result = dict(value, isolation="worktree")
    result.pop("cwd", None)
    named = [b for _r, b in targets or [] if b] or ["main"]
    if not re.search(r"inflight-ack\.sh|inflight-acks/", result["prompt"]) and \
            not all("git merge %s" % b in result["prompt"] for b in named):
        result["prompt"] = result["prompt"].rstrip() + "\n\n" + last_step(targets)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", default="-", help="task-specific Agent tool input JSON, or - for stdin")
    args = parser.parse_args()
    try:
        raw = sys.stdin.read() if args.file == "-" else Path(args.file).read_text()
        print(json.dumps(prepare(json.loads(raw)), indent=2))
    except (ValueError, OSError) as exc:
        print(f"prepare-agent-spawn: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

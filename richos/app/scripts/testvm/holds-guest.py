#!/usr/bin/env python3
"""holds-guest.py — inside a test VM guest: held work on file in one conversation's registry.

Run by holds-walk.py over ssh, with the guest's /usr/bin/python3, never on a host:

  holds-guest.py file ENGINE WORKSPACES REPO MINUTES
      In WORKSPACES (the conversation's partition under <app data>/engine-state/workspaces),
      registered as spawn.sh registers teammates (workspaces.new_record), MINUTES old:
        - spelling-mate: finished, with an unresolved wait whose only end is free text
          ("code done, real-app checks left"), the 2026-09-27 kind of hold;
        - fix-mate: finished on branch cc/fix-mate of REPO (one commit, a minute older than
          the correction), with a correction sent after that commit that its run never acted
          on (2026-10-21, echo-opus-andreas2).
      Prints one JSON line: the record paths, the branch tip and the engine's own turn-end lines.
"""
import json
import os
import subprocess
import sys
import time


def run(*args, env=None):
    return subprocess.run(list(args), check=True, capture_output=True, text=True,
                          env=dict(os.environ, **(env or {}))).stdout.strip()


def file(engine, workspaces, repo, minutes):
    sys.path.insert(0, engine + "/mega-lander")
    os.environ["RICHOS_WORKSPACES_DIR"] = workspaces
    import workspaces as w
    w._ensure_dirs()
    then = int(time.time() - float(minutes) * 60)
    if not os.path.isdir(os.path.join(repo, ".git")):
        raise SystemExit("no repository at %s" % repo)
    # fix-mate's own commit, a minute before the correction, without touching the working tree.
    dated = {"GIT_AUTHOR_DATE": "@%d +0000" % (then - 60), "GIT_COMMITTER_DATE": "@%d +0000" % (then - 60)}
    # Only the commit's DATE is the fixture's; the identity is the machine's own, with a
    # placeholder only where none is configured at all (the guest's fixture home has none).
    if not subprocess.run(["git", "-C", repo, "config", "--get", "user.email"],
                          capture_output=True, text=True).stdout.strip():
        dated.update({"GIT_AUTHOR_NAME": "QA", "GIT_AUTHOR_EMAIL": "qa@example.invalid",
                      "GIT_COMMITTER_NAME": "QA", "GIT_COMMITTER_EMAIL": "qa@example.invalid"})
    tip = run("git", "-C", repo, "commit-tree", "HEAD^{tree}", "-p", "HEAD", "-m", "fix-mate: work", env=dated)
    branch = "cc/fix-mate"
    run("git", "-C", repo, "branch", "-f", branch, tip)
    made = []
    for name, extra in (
            ("spelling-mate", {"waiting": {"kind": "started", "at": then,
                                           "on": "code done, real-app checks left"}}),
            ("fix-mate", {"correction": {"at": then, "text": "Fix review finding #1: the sign-in status "
                                                             "must survive a failed usage read."},
                          "workspaces": [{"kind": "cc", "repo": repo, "path": repo, "branch": branch,
                                          "deleted_at": None}]})):
        key = "holds-walk--" + name
        rec = w.new_record(key, name=name, session_id="holds-walk-ended",
                           end={"at": then, "signal": "SubagentStop"}, **extra)
        w.write_json(w.agent_path(key), rec)
        made.append(w.agent_path(key))
    print(json.dumps({"records": made, "tip": tip, "lines": w.hold_lines("")}))


if __name__ == "__main__":
    {"file": file}[sys.argv[1]](*sys.argv[2:])

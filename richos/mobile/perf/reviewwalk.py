#!/usr/bin/env python3
"""reviewwalk.py — no RichConnect build goes to a store review before an automated walk of exactly what
the store's reviewer does has passed on that exact build.

THE CEO, 2026-10-04 (richos-hq/wiki/ceo-decisions.md §107): the CEO is never the first tester. No build
goes to a store review until an automated walk of exactly what the reviewer does has passed on that
exact build, and the command checks it itself.

One record per walk, shared by both platforms' walks and both platforms' gates:

  resolved review-walk state/<platform>/<full commit sha>.json
  {"platform": "ios" | "android", "commit": "<full sha>", "passed": true | false,
   "at": "<ISO UTC>", "host": "<review host name>",
   "steps": [{"name": "...", "ok": true | false, "detail": "..."}, ...]}

A walk writes it (`write`, or `reviewwalk.py write` with the record on stdin); the latest walk of a
commit replaces the earlier one, so a later failure withdraws an earlier pass. A gate reads it
(`check`, or `reviewwalk.py check`). The gate passes only when the file for that platform and that
commit exists, is readable, names that platform and that commit, says passed: true, and lists at least
one step with every step ok. Anything else refuses, naming the platform, the commit and what is
missing or failing. It fails closed and has no skip flag; the records are read from the fixed state
directory only (a test passes its own directory to the functions, never through the command line).

  reviewwalk.py check --platform ios|android --commit SHA
      exit 0 with one JSON object on stdout ({ok, platform, commit, record, line}),
      or exit 1 with one REFUSED line on stderr.
  reviewwalk.py write < record.json
      validates the record's shape and writes it atomically; prints the path.
"""
import argparse
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import machine_storage as M
import re
import sys

ROOT = str(M.state("review-walk"))
LABEL = {"android": "Android", "ios": "iPhone"}
WALK = {"ios": "richos/mobile/native-ios/bin/rios review-walk --commit {sha}",
        "android": "richos/mobile/native-android/bin/randroid review-walk --commit {sha}"}
SHA = re.compile(r"^[0-9a-f]{40}$")


class Refused(Exception):
    """The build may not go to a store review; the sentence says why."""


def path_for(platform, commit, root=ROOT):
    return Path(root) / platform / f"{commit}.json"


def _shape(record):
    """Why `record` is not a well-formed walk record, or "" when it is."""
    if not isinstance(record, dict):
        return "it is not a JSON object"
    if record.get("platform") not in LABEL:
        return f"its platform is {record.get('platform')!r}"
    if not isinstance(record.get("commit"), str) or not SHA.match(record["commit"]):
        return f"its commit is {record.get('commit')!r}, not a full commit"
    if not isinstance(record.get("passed"), bool):
        return "it does not say passed true or false"
    if not isinstance(record.get("at"), str) or not record["at"]:
        return "it has no time"
    if not isinstance(record.get("host"), str) or not record["host"]:
        return "it names no review host"
    steps = record.get("steps")
    if not isinstance(steps, list) or not steps:
        return "it lists no steps"
    for i, step in enumerate(steps):
        if not isinstance(step, dict) or not isinstance(step.get("name"), str) or not isinstance(step.get("ok"), bool):
            return f"its step {i} is not {{name, ok, detail}}"
    return ""


def check(platform, commit, root=ROOT):
    """The PASS result (its "line" is the sentence), or raise Refused."""
    if platform not in LABEL:
        raise Refused(f"unknown platform {platform!r}")
    what = f"{LABEL[platform]} build of {str(commit)[:12]}"
    if not isinstance(commit, str) or not SHA.match(commit):
        raise Refused(f"{LABEL[platform]} build: {commit!r} is not a full commit")
    path = path_for(platform, commit, root)
    walk = WALK[platform].format(sha=commit)
    try:
        try:
            record = json.loads(path.read_text())
        except FileNotFoundError:
            raise Refused(f"no review walk has run on this commit ({path} does not exist); run {walk}") from None
        except (OSError, ValueError) as exc:
            raise Refused(f"the review walk record {path} is unreadable ({exc}); run {walk}") from None
        bad = _shape(record)
        if bad:
            raise Refused(f"the review walk record {path} is malformed: {bad}; run {walk}")
        if record["platform"] != platform or record["commit"] != commit:
            raise Refused(f"the review walk record {path} is for {LABEL.get(record['platform'], '?')} "
                          f"{record['commit'][:12]}, not this build; run {walk}")
        failed = [s for s in record["steps"] if not s["ok"]]
        if not record["passed"] or failed:
            step = failed[0] if failed else {"name": "?", "detail": "the record says passed: false"}
            raise Refused(f"the last review walk ({record['at']}) FAILED at step {step['name']!r}: "
                          f"{step.get('detail') or 'no detail'}; fix it and run {walk}")
    except Refused as exc:
        raise Refused(f"{what}: {exc}") from None
    return {"ok": True, "platform": platform, "commit": commit, "record": str(path),
            "line": (f"{what}: the review walk PASSED on {record['host']} at {record['at']} "
                     f"({len(record['steps'])} steps)")}


def write(record, root=ROOT):
    """Write a walk record atomically; returns its path. Raises ValueError on a malformed record."""
    bad = _shape(record)
    if bad:
        raise ValueError(f"not a review walk record: {bad}")
    if record["passed"] != all(s["ok"] for s in record["steps"]):
        raise ValueError("not a review walk record: passed disagrees with its steps")
    path = path_for(record["platform"], record["commit"], root)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(f".json.{os.getpid()}.tmp")
    tmp.write_text(json.dumps(record, indent=1) + "\n")
    os.replace(tmp, path)
    return path


def main(argv=None):
    ap = argparse.ArgumentParser(prog="reviewwalk.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--platform", required=True, choices=sorted(LABEL))
    c.add_argument("--commit", required=True)
    sub.add_parser("write")
    args = ap.parse_args(argv)
    if args.cmd == "write":
        try:
            print(write(json.loads(sys.stdin.read())))
        except (OSError, ValueError) as exc:
            print(f"reviewwalk.py write: {exc}", file=sys.stderr)
            return 1
        return 0
    try:
        result = check(args.platform, args.commit)
    except Refused as exc:
        print(f"REFUSED by the review-walk gate (CEO §107): {exc}", file=sys.stderr)
        return 1
    except Exception as exc:  # noqa: BLE001 — anything unexpected refuses, never allows
        print(f"REFUSED by the review-walk gate (CEO §107): {args.platform}: the gate itself failed: "
              f"{type(exc).__name__}: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())

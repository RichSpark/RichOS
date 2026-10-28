#!/usr/bin/env python3
"""ci-run-records.py — match pushes against workflow runs, and name what is missing.

The comparison half of `ci-run-record-check.sh`, separated so it can be tested
against captured payloads with no network and no token. The reasoning it
encodes, once:

  A PUSH IS THE UNIT, NOT A COMMIT. `on: push` fires once per push, against the
  pushed head. Five commits in one push correctly produce one run, so a check
  built on commits reports four false gaps per batch and gets switched off.

  A MISSING RUN IS NOT A FAILED RUN. A run that exists and failed is somebody's
  bug and is visible. A run that never existed is invisible, and every tool that
  reads "the latest run" reports the PREVIOUS commit's verdict in its place.
  That is the case here.

  A PUSH INSIDE THE GRACE WINDOW IS PENDING, NOT MISSING. Its run may not have
  reached the API yet. Named on every run so the tolerance is never invisible.

  NO ANSWER IS NOT A PASS. If the payloads cannot be read, this exits 2 with the
  reason. A run-record check that goes quiet when its sources fail is the defect
  it exists to catch, one level out.

  A MISSING VERDICT NEEDS PROOF OF COVERAGE, AND ABSENCE FROM A PAGE IS NOT
  PROOF. Added 2026-09-14, when this file's first real finding turned out to be
  its own pagination: the caller read one page of runs, the oldest of which was
  2026-09-10T07:09:05Z, and reported two pushes from the half hour before it as
  having no run at all. Both had runs, on page 2 — 0fb68b7c run #101 (success)
  and de6ca1f8 run #107 (FAILURE). "Missing" is a claim about the world; what
  was actually known was the edge of a window.

  So a push is only MISSING where the run list provably covers it: the list was
  exhausted, or the push is newer than the oldest run fetched. Anything else is
  UNCOVERED — named, not counted as a finding, exit 2. This matters beyond
  tidiness because the remedy printed below tells the operator to move --since
  past a permanent gap, and taking that advice here would have buried a real
  failing run behind a note saying nothing was at risk.
"""
import argparse
import datetime
import json
import sys


def page_info(path):
    """'<items> <total_count>' for one run-list page, for the paging loop."""
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
    items = data.get("workflow_runs") if isinstance(data, dict) else data
    items = items if isinstance(items, list) else []
    total = data.get("total_count", -1) if isinstance(data, dict) else -1
    if not isinstance(total, int):
        total = -1
    return "%d %d" % (len(items), total)


def merge_pages(list_path, out_path):
    """Concatenate the run-list pages named in `list_path` into one payload."""
    merged = []
    with open(list_path, encoding="utf-8") as fh:
        paths = [ln.strip() for ln in fh if ln.strip()]
    for path in paths:
        with open(path, encoding="utf-8") as pfh:
            data = json.load(pfh)
        items = data.get("workflow_runs") if isinstance(data, dict) else data
        if isinstance(items, list):
            merged.extend(items)
    with open(out_path, "w", encoding="utf-8") as out:
        json.dump({"workflow_runs": merged}, out)


def load(path, what):
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError) as exc:
        sys.stderr.write("ci-run-records.py: cannot read %s (%s): %s\n" % (what, path, exc))
        raise SystemExit(2)


def parse_ts(value):
    if not value:
        return None
    try:
        return datetime.datetime.fromisoformat(str(value).replace("Z", "+00:00"))
    except ValueError:
        return None


def pushes_from_events(events, branch):
    """(sha, created_at) for every PushEvent onto `branch`, newest first."""
    out = []
    if not isinstance(events, list):
        return out
    want = "refs/heads/%s" % branch
    for ev in events:
        if not isinstance(ev, dict) or ev.get("type") != "PushEvent":
            continue
        payload = ev.get("payload") or {}
        if payload.get("ref") != want:
            continue
        head = payload.get("head")
        if not head:
            continue
        out.append((head, parse_ts(ev.get("created_at"))))
    return out


def runs_floor(runs):
    """The created_at of the OLDEST run fetched — the edge of what is known.

    A push older than this was never inside the window, so its absence from the
    index says nothing about whether a run exists for it.
    """
    items = (runs or {}).get("workflow_runs") if isinstance(runs, dict) else runs
    if not isinstance(items, list):
        return None
    stamps = [parse_ts(r.get("created_at")) for r in items if isinstance(r, dict)]
    stamps = [t for t in stamps if t is not None]
    return min(stamps) if stamps else None


def runs_index(runs):
    """head_sha -> the newest run for it."""
    index = {}
    items = (runs or {}).get("workflow_runs") if isinstance(runs, dict) else runs
    if not isinstance(items, list):
        return index
    for run in items:
        if not isinstance(run, dict):
            continue
        sha = run.get("head_sha")
        if not sha:
            continue
        prev = index.get(sha)
        if prev is None or str(run.get("created_at") or "") > str(prev.get("created_at") or ""):
            index[sha] = run
    return index


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    # Two tiny side entry points, here rather than inlined into the shell, so
    # every line that parses this API's JSON lives in one file and one language.
    if argv and argv[0] == "--page-info":
        if len(argv) != 2:
            sys.stderr.write("ci-run-records.py: --page-info needs one path\n")
            return 2
        try:
            print(page_info(argv[1]))
        except (OSError, ValueError) as exc:
            sys.stderr.write("ci-run-records.py: --page-info %s: %s\n" % (argv[1], exc))
            return 2
        return 0
    if argv and argv[0] == "--merge-pages":
        if len(argv) != 3:
            sys.stderr.write("ci-run-records.py: --merge-pages needs <list> <out>\n")
            return 2
        try:
            merge_pages(argv[1], argv[2])
        except (OSError, ValueError) as exc:
            sys.stderr.write("ci-run-records.py: --merge-pages: %s\n" % exc)
            return 2
        return 0

    ap = argparse.ArgumentParser()
    ap.add_argument("--events", required=True)
    ap.add_argument("--runs", required=True)
    ap.add_argument("--branch", default="main")
    ap.add_argument("--grace-minutes", type=float, default=20.0)
    ap.add_argument("--source", default="events", choices=("events", "tip"))
    # THE WINDOW HAS A START DATE, AND IT IS DECLARED RATHER THAN INFERRED.
    # This repository has 95 pushes to main with no run, for the entirely
    # correct reason that `push:` was not a trigger when they were made. A
    # check that reported those as dropped runs would be red on its first
    # execution and every one after it, which is how a check gets ignored. So
    # the caller declares the day the trigger was restored; pushes before it
    # are outside the promise and are counted, named in one line, and not
    # failed on. Deriving it from the run history instead would be circular:
    # if every run were dropped there would be no history to derive from.
    ap.add_argument("--since", default="",
                    help="ISO date; pushes before it are outside the watched window")
    # WITHOUT THIS THE DEFAULT IS "I ONLY SAW A WINDOW", which is the honest
    # default: the caller sets it when its paging loop reached the end of the
    # run list, and only then may absence from the index mean absence.
    ap.add_argument("--runs-complete", action="store_true",
                    help="the runs payload is the WHOLE run list, not a page of it")
    ap.add_argument("--tip", default="")
    ap.add_argument("--workflow", default="the workflow")
    ap.add_argument("--repo", default="<repo>")
    args = ap.parse_args(argv)

    runs_payload = load(args.runs, "the run list")
    runs = runs_index(runs_payload)
    floor = None if args.runs_complete else runs_floor(runs_payload)
    now = datetime.datetime.now(datetime.timezone.utc)
    cutoff = now - datetime.timedelta(minutes=args.grace_minutes)

    if args.source == "tip":
        if not args.tip:
            sys.stderr.write("ci-run-records.py: --source tip needs --tip <sha>\n")
            return 2
        checked = [(args.tip, None)]
        predicate = ("the branch tip has a run record (the event feed was unreadable, so "
                     "earlier pushes were NOT checked)")
    else:
        events = load(args.events, "the event feed")
        checked = pushes_from_events(events, args.branch)
        predicate = "every push to %s in the event feed has a run record" % args.branch
        if not checked:
            # Zero pushes is not zero problems: it means the feed carried none
            # for this branch, which on an active branch is itself suspicious.
            # Reported, not silently green.
            print("ci-run-record-check: NO pushes to %s were found in the event feed for %s."
                  % (args.branch, args.repo))
            print("  Nothing could be checked. That is not the same as nothing being wrong: on a branch")
            print("  that receives pushes, an empty feed means the feed is the thing that is broken.")
            return 2

    since = parse_ts(args.since + "T00:00:00+00:00") if args.since and "T" not in args.since \
        else parse_ts(args.since)
    if args.since and since is None:
        sys.stderr.write("ci-run-records.py: --since %r is not an ISO date\n" % args.since)
        return 2

    missing = []
    pending = []
    found = []
    before_window = []
    uncovered = []
    seen = set()
    for sha, when in checked:
        if sha in seen:
            continue
        seen.add(sha)
        if since is not None and when is not None and when < since:
            before_window.append((sha, when))
            continue
        run = runs.get(sha)
        if run is not None:
            found.append((sha, run))
        elif when is not None and when > cutoff:
            pending.append((sha, when))
        elif floor is not None and when is not None and when < floor:
            # OLDER THAN THE OLDEST RUN FETCHED. Had a run existed for this push
            # it need not have been inside the window, so nothing can be said.
            # Calling it missing here is the 2026-09-14 false positive.
            uncovered.append((sha, when))
        else:
            missing.append((sha, when))

    print("ci-run-record-check: %s / %s" % (args.repo, args.workflow))
    print("  predicate: %s" % predicate)
    print("  grace: %g minute(s); %d push(es) examined, %d with a run, %d pending, %d MISSING, %d uncovered"
          % (args.grace_minutes, len(seen), len(found), len(pending), len(missing), len(uncovered)))
    # PRINTED ON EVERY RUN, like the grace window, because the reach of the run
    # list is the thing that silently decided the verdict for a year's worth of
    # nobody looking at it.
    if args.runs_complete:
        print("  run coverage: the WHOLE run list was fetched — absence from it is absence.")
    elif floor is not None:
        print("  run coverage: back to %s only (the run list was not exhausted); a push older"
              % floor)
        print("     than that is UNCOVERED, never MISSING — absence from a page is not absence.")
    else:
        print("  run coverage: the runs payload carries no timestamps, so nothing bounds it.")
    if before_window:
        print("  %d push(es) predate %s and are outside the watched window — the trigger was not on"
              % (len(before_window), args.since))
        print("     for them, so their absence of a run is correct and is not a finding.")

    for sha, when in pending:
        print("  PENDING  %s (pushed %s — inside the grace window)" % (sha[:12], when))

    if uncovered:
        sys.stderr.write(
            "\n? %d push(es) to %s are OUTSIDE the run list that was fetched, so whether they\n"
            "  have a run is UNKNOWN. They are not reported as missing, because absence from a\n"
            "  page is not absence:\n\n" % (len(uncovered), args.branch))
        for sha, when in uncovered:
            sys.stderr.write("    %s   pushed %s (oldest run fetched: %s)\n"
                             % (sha, when if when else "(time unknown)", floor))
        sys.stderr.write(
            "\n  Raise --max-pages (or --limit) until the run list is exhausted, or move --since\n"
            "  forward to a point the fetched runs cover. Do NOT read this as a clean bill of\n"
            "  health and do NOT read it as a dropped run: it is a question that was not asked.\n\n")

    if not missing:
        if uncovered:
            return 2
        for sha, run in found[:5]:
            print("  ok       %s -> run %s (%s)"
                  % (sha[:12], run.get("id"), run.get("conclusion") or run.get("status")))
        return 0

    sys.stderr.write("\n✗ %d push(es) to %s have NO run of %s at all.\n\n"
                     % (len(missing), args.branch, args.workflow))
    for sha, when in missing:
        sys.stderr.write("    %s   pushed %s\n" % (sha, when if when else "(time unknown)"))
    sys.stderr.write(
        "\n  A MISSING run is not a failing run. Nothing shows a cross for these commits, and any\n"
        "  tool that reads 'the latest run' is reporting an EARLIER commit's verdict for them.\n"
        "  The usual cause is GitHub's account-wide 20-job concurrency limit, which DROPS runs\n"
        "  rather than queueing them.\n\n"
        "  1. CONFIRM THE ABSENCE BEFORE ACTING ON IT. This step is FIRST because on 2026-09-14\n"
        "     this check named two SHAs here and BOTH had runs; it was reporting the edge of its\n"
        "     own pagination. One API call settles it, and it is not bounded by any window:\n\n"
        "         gh api 'repos/%s/actions/workflows/%s/runs?head_sha=<sha>' \\\n"
        "             --jq '.total_count, (.workflow_runs[]|\"\\(.run_number) \\(.event) \\(.conclusion)\")'\n\n"
        "     total_count 0 means it really is missing. Anything else means this check is wrong\n"
        "     and the bug is HERE, not in GitHub. Fix it here.\n\n"
        "  2. If it is genuinely absent, find out why, then give it a run. A workflow_dispatch ref\n"
        "     must be a BRANCH OR TAG, never a bare SHA, so a superseded push cannot be\n"
        "     re-dispatched in place:\n\n"
        "         git push origin <sha>:refs/heads/rerun/<sha8>\n"
        "         gh workflow run %s --repo %s --ref rerun/<sha8>\n\n"
        "     THAT FIRST LINE IS REFUSED IN SOME REPOSITORIES, INCLUDING THIS ONE'S USUAL HOME.\n"
        "     RichSpark/RichOS carries an active ruleset named `main-only` with an EMPTY\n"
        "     bypass list, excluding only refs/heads/main from `creation` and `update`\n"
        "     (gh api repos/RichSpark/RichOS/rulesets/23194738). The owner is bound by it\n"
        "     too. Where that is so, there is no way to give a superseded SHA a run: the API\n"
        "     takes a ref, and every ref you could make is refused. Say that out loud rather\n"
        "     than making somebody discover it at the push.\n\n"
        "  3. ONLY IF STEP 1 CONFIRMED THE ABSENCE and the gap is permanent, move --since past it\n"
        "     AND SAY WHY in the workflow file; a check red over history nobody can change is a\n"
        "     check nobody reads. Taking this step on an UNCONFIRMED absence is how a real red\n"
        "     gets buried: the second SHA this check wrongly named had run #107, and it FAILED.\n\n"
        % (args.repo, args.workflow, args.workflow, args.repo))
    return 1


if __name__ == "__main__":
    sys.exit(main())

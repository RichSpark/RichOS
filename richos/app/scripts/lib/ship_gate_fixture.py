#!/usr/bin/env python3
"""ship_gate_fixture.py — a STAND-IN passing speed verdict, for a test that must run a gated build.

Every Android bundle asks the speed gate first (richos/mobile/perf/shipgate.py, CEO §106), whatever key
signs it. The Android suite's throwaway-key bundle (native-android-app.test.sh) proves the signing path
and needs a pass, so it writes this fixture into its own scratch and names it with
RICHOS_SHIP_GATE_VERDICTS. The gate judges the fixture by every rule it applies to the speed watch's own
verdicts (a "good" verdict, a record on disk for committed code with the same app code, cold and warm
under the current §104 limits), and its pass line says the verdicts were a stand-in.

  ship_gate_fixture.py --dir DIR --platform android|ios --commit SHA [--walks WALKS]

Writes DIR/measured.json and DIR/<platform>.json and prints DIR. DIR is the caller's scratch, never the
speed watch's state directory.

The bundle also needs a PASSING review walk of its commit (CEO §107, richos/mobile/perf/reviewwalk.py).
With --walks, a STAND-IN passing walk record for SHA is written into WALKS, the caller's scratch named by
RICHOS_REVIEW_WALK_RECORDS (never the walks' own state directory); `randroid bundle` judges it by every
rule of a real record and its pass line says the walk records were a stand-in.
"""
import argparse
import json
from pathlib import Path
import sys

# Start series that pass §104 on each platform with room to spare (the iPhone's record has no start 1).
PASSING = {"android": {"cold": [900] + [500] * 19, "warm": [100] * 20},
           "ios": {"cold": [400] * 19, "warm": [None] + [400] * 19}}
import machine_storage as M
WATCH_STATE = str(M.state("phone-speed-watch", "RICHOS_PHONE_WATCH_HOME"))
WALK_STATE = str(M.state("review-walk"))


def write(directory, platform, commit, repo=None):
    directory = Path(directory).resolve()
    if directory == Path(WATCH_STATE).resolve():
        raise SystemExit(f"refused: {directory} is the speed watch's own state; a stand-in goes in scratch")
    directory.mkdir(parents=True, exist_ok=True)
    record = directory / f"{platform}.json"
    record.write_text(json.dumps({
        "standIn": "test fixture (richos/app/scripts/lib/ship_gate_fixture.py), not a phone measurement",
        "build": {"commit": commit, "dirty": False},
        "metrics": {"coldLaunch": {"samplesMs": PASSING[platform]["cold"]},
                    "warmStandard": {"verdict": "PASS", "startsMs": PASSING[platform]["warm"]}}}))
    measured = directory / "measured.json"
    entries = json.loads(measured.read_text()) if measured.exists() else {}
    entries[platform] = {"commit": commit, "record": str(record), "verdict": "good"}
    if repo:  # cover exactly the uncommitted app code in REPO now (a merge gate bundles its merge tree)
        sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "mobile/perf"))
        import shipgate  # noqa: E402
        entries[platform]["coversTree"] = shipgate.tree_fingerprint(platform, repo)
    measured.write_text(json.dumps(entries))
    return directory


def walk(directory, platform, commit):
    """A stand-in PASSING review walk record for `commit`, in the caller's scratch `directory`."""
    directory = Path(directory).resolve()
    if directory == Path(WALK_STATE).resolve():
        raise SystemExit(f"refused: {directory} is the review walks' own state; a stand-in goes in scratch")
    sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "mobile/perf"))
    import reviewwalk  # noqa: E402
    return reviewwalk.write({
        "platform": platform, "commit": commit, "passed": True, "at": "2026-10-04T00:00:00Z",
        "host": "stand-in.invalid", "standIn": "test fixture (richos/app/scripts/lib/ship_gate_fixture.py), not a walk",
        "steps": [{"name": "stand-in", "ok": True, "detail": "test fixture"}]}, root=directory)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="ship_gate_fixture.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--dir", required=True)
    ap.add_argument("--platform", required=True, choices=sorted(PASSING))
    ap.add_argument("--commit", required=True)
    ap.add_argument("--repo", help="also cover the uncommitted app code of this checkout, as it is now")
    ap.add_argument("--walks", help="also write a stand-in passing review walk record for --commit here")
    args = ap.parse_args(argv)
    print(write(args.dir, args.platform, args.commit, args.repo))
    if args.walks:
        print(walk(args.walks, args.platform, args.commit))
    return 0


if __name__ == "__main__":
    sys.exit(main())

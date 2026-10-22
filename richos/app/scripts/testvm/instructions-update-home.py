#!/usr/bin/env python3
"""instructions-update-home.py: a fixture home for the improved-instructions walk (CEO, 2026-10-22).

  instructions-update-home.py FIXTURE_HOME [--earlier N]
  run-walk.py --bundle APP --home FIXTURE_HOME --engine ENGINE --report REPORT --wait 600 -- \\
      steps-walk.py --steps docs/verification/<record>/steps/walk.json --out DIR

It writes the user's folder as a user who installed an older RichOS and changed Rich's file
would have it: ~/myrichos/rich.md is the engine's earlier shipped version N (default 5, the one
before the current), byte for byte, with one line of the user's own added as a paragraph at
the end (USER_LINE); stu.md is the current shipped one, unchanged; instructions.md is empty;
there is no record (`.richos/`), as on every install before the record existed. A new build
then asks about rich.md alone, and OK must keep USER_LINE and add the new shipped lines.

Test data only: it refuses a home that already has a myrichos folder, so it never writes over
anybody's real files.
"""
import argparse
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ENGINE_APP = HERE.parents[2] / "engine" / "app"
USER_LINE = "Fixture user rule: sign every reply with the word heron."


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("home", type=Path)
    p.add_argument("--earlier", type=int, default=5, help="which earlier/rich/<N>.txt the copy was made from")
    args = p.parse_args()
    folder = args.home / "myrichos"
    if folder.exists():
        print(f"refusing: {folder} already exists; this writes fixture files only into a new home", file=sys.stderr)
        return 2
    older = (ENGINE_APP / "earlier" / "rich" / f"{args.earlier}.txt").read_bytes()
    folder.mkdir(parents=True)
    (folder / "rich.md").write_bytes(older + b"\n" + USER_LINE.encode() + b"\n")
    (folder / "stu.md").write_bytes((ENGINE_APP / "agents" / "stu.md").read_bytes())
    (folder / "instructions.md").write_bytes(b"")
    print(f"seeded {folder}: rich.md = earlier/rich/{args.earlier}.txt + one user line; stu.md as shipped; no record")
    return 0


if __name__ == "__main__":
    sys.exit(main())

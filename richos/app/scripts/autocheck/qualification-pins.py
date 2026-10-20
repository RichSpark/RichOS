#!/usr/bin/env python3
"""qualification-pins.py — renew the SHA-256 pins in docs/development/verification-input-qualifications.json.

WHY (2026-10-01): every handoff that changes a file a reviewed check reads has to renew that
file's pin in the qualification record. The commit hook refuses the commit and says how; a
night of thirteen handoffs hand-edited those pins with sed, and one agent wrote a throwaway
helper. This is that helper, committed (a helper you needed and did not find is added to the
toolkit). It is the qualification-record twin of dependency-pins.py --renew.

  qualification-pins.py --renew [--note TEXT] [PATH..]
                                          set every "sources" pin of each repository-relative
                                          PATH to the working-tree bytes. With no PATH: every
                                          pinned file that differs from its pin (so it also
                                          covers a file changed in an earlier commit). Prints
                                          each pin it moved. A pinned file that no longer
                                          exists is refused, never silently dropped. --note
                                          appends why the review still holds to the "review"
                                          text of each unit whose pin moved, and only there.
  qualification-pins.py --all             list every stale pin; exit 1 when there is one
                                          (read-only)

Only the 64-hex digest after the path is rewritten, in place, so the file's formatting, key
order and every "review" text stay byte-identical. A changed file's unit "review" text still
has to be re-read by a human: renewing the pin does not say the review is still true.

WHERE A RENEWAL NOTE GOES (2026-10-20, verification reliability PRD step 2, finding 2): into the
renewed unit's own "review" text (--note), never into the prose review
(verification-input-qualifications.md). The prose is a whole-file input of every qualified
identity, so one note there discarded every reusable pass, UI suites included; a unit's review
is in its own slice only (proof_evidence.qualification_slice), which its pin renewal changes
anyway.

The hash is `shasum -a 256 <file>` over the raw bytes, which is what the commit hook compares.
The repository root is the current directory's Git top (override: QUALIFICATION_PINS_TOP).
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import time

RECORD = "docs/development/verification-input-qualifications.json"


def top():
    if os.environ.get("QUALIFICATION_PINS_TOP"):
        return os.path.abspath(os.environ["QUALIFICATION_PINS_TOP"])
    out = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True,
                         stdin=subprocess.DEVNULL)
    if out.returncode:
        raise SystemExit("qualification-pins: not inside a Git repository")
    return out.stdout.strip()


def sha(root, path):
    full = os.path.join(root, path)
    if not os.path.isfile(full):
        return None
    with open(full, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def pins(data):
    """[(unit, path, pinned digest)]."""
    return [(unit, path, pin)
            for unit, body in sorted((data.get("units") or {}).items())
            for path, pin in sorted((body.get("sources") or {}).items())]


def stale(root, data):
    return [(u, p, pin, sha(root, p)) for u, p, pin in pins(data) if sha(root, p) != pin]


def note_review(text, data, unit, note):
    """`text` with `note` appended to unit `unit`'s own "review" string, everything else as it was."""
    review = data["units"][unit]["review"]
    key = text.index(json.dumps(unit) + ":")
    at = text.index('"review"', key)
    start = text.index('"', text.index(":", at))
    value, end = json.JSONDecoder().raw_decode(text, start)
    if value != review:
        raise SystemExit(f"qualification-pins: cannot find unit \"{unit}\"'s own review text to note")
    return text[:start] + json.dumps(review.rstrip() + " " + note.strip(), ensure_ascii=False) + text[end:]


def renew(root, wanted, note=None):
    record = os.path.join(root, RECORD)
    with open(record, encoding="utf-8", newline="") as handle:
        text = handle.read()
    data = json.loads(text)
    targets = sorted(set(wanted) if wanted else {p for _, p, _, _ in stale(root, data)})
    known = {p for _, p, _ in pins(data)}
    for path in targets:
        if path not in known:
            print(f"qualification-pins: {path} is not pinned in {RECORD}; nothing to renew")
    moved = 0
    noted = set()
    for path in sorted(set(targets) & known):
        now = sha(root, path)
        if now is None:
            raise SystemExit(f"qualification-pins: pinned file {path} no longer exists; "
                             f"remove its pin by hand after reading the unit's review text")
        pattern = re.compile(r'("' + re.escape(path) + r'"\s*:\s*")([0-9a-f]{64})(")')
        def swap(match):
            nonlocal moved
            if match.group(2) != now:
                moved += 1
                print(f"renewed {path}: {match.group(2)[:12]} -> {now[:12]}")
            return match.group(1) + now + match.group(3)
        if note:
            noted |= {(unit, path, pin) for unit, p, pin in pins(data) if p == path and pin != now}
        text = pattern.sub(swap, text)
    for unit in sorted({unit for unit, _p, _pin in noted}):
        moved_here = sorted((p, pin) for u, p, pin in noted if u == unit)
        stamp = time.strftime("%Y-%m-%d", time.gmtime())
        text = note_review(text, json.loads(text), unit, "Renewed %s: %s. %s" % (
            stamp, ", ".join("%s %s to %s" % (p, pin[:8], (sha(root, p) or "")[:8]) for p, pin in moved_here),
            note))
        print(f"noted the renewal in unit \"{unit}\"'s review")
    with open(record, "w", encoding="utf-8", newline="") as handle:
        handle.write(text)
    json.loads(text)
    left = stale(root, json.loads(text))
    print(f"qualification-pins: {moved} pin(s) renewed; stale now: {len(left)}")
    for unit, path, _, _ in left:
        print(f"  still stale: {path} (unit \"{unit}\")")
    return 0


def main(argv):
    root = top()
    if argv[:1] == ["--all"] and len(argv) == 1:
        with open(os.path.join(root, RECORD), encoding="utf-8") as handle:
            found = stale(root, json.load(handle))
        for unit, path, pin, now in found:
            print(f"{path} (unit \"{unit}\"): pinned {pin[:12]}, now {(now or '(removed)')[:12]}")
        return 1 if found else 0
    if argv[:1] == ["--renew"]:
        rest, note = argv[1:], None
        if rest[:1] == ["--note"]:
            if len(rest) < 2 or not rest[1].strip():
                print("qualification-pins: --note needs the reason the review still holds", file=sys.stderr)
                return 2
            note, rest = rest[1], rest[2:]
        return renew(root, rest, note)
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

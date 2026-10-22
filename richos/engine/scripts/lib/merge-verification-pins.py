#!/usr/bin/env python3
"""merge-verification-pins.py — git's merge driver for the two verification maps.

    merge-verification-pins.py %O %A %B %P      (registered by richos/app/scripts/autocheck/install.sh)

The maps (richos/engine/scripts/lib/verification-dependencies.json and
docs/development/verification-input-qualifications.json) pin readers by sha256, and every branch
that changes a pinned reader renews its pin. So any two branches built in parallel conflicted
there: on 2026-10-05 alone four merges stopped on them, and each time an agent took one side's
lines and re-ran renew-verification-pins.py by hand (a64668554, ccc3ff319). A pin is not text to
combine; it is the hash of the merged file.

So this driver merges the two documents as JSON, three ways, and then regenerates the pins:

  1. Every entry from both sides is kept. A key one side added is added; a value one side changed
     and the other did not takes the change; a key one side removed and the other left as it was
     is removed (a key one side removed and the other changed is kept, changed). Lists are merged
     as sets in order: what either side added is kept, what one side removed is removed.
  2. A pin both sides changed to different values is taken from the merged FILE: the pinned
     file's three versions (merge base, ours, theirs) are merged with `git merge-file`, and the
     pin becomes the sha256 of that result, which is what renew-verification-pins.py would write
     once the merge is on disk. A pin is a `sha256` beside a `path`/`source`, or a value in a
     `sources` map.
  3. A review note (`evidence`, `review`) both sides extended is kept with both additions, ours
     first, when each side only appended to the base note: a renewal appends its sentence, so two
     parallel renewals of one entry did exactly that (2026-10-22, cc/zach-opus-folderfiles2 and
     cc/zach-opus-landgate4 on nodes/mega-lander/app.py#land-lock-fixtures). A note either side
     rewrote is a real conflict.
  4. Anything else both sides changed differently is a real conflict: the driver leaves git's own
     conflict markers in the file (`git merge-file`) and exits 1, exactly as without it.

The other side's commit is read from the merge itself: MERGE_HEAD when git has written it, else
the GITHEAD_<sha> variable git sets for a merge's other side, else GIT_REFLOG_ACTION
("merge <name>"). When it cannot be found (a rebase, a cherry-pick) a pin both sides changed is
left at ours and the commit check names it (autocheck/dependency-pins.py), with the renewal
command, before anything lands.

The output is written as renew-verification-pins.py writes it (json.dumps, indent 2, newline), so
a merged map is byte-identical to one renewed by hand.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile

PIN = re.compile(r"[0-9a-f]{64}")
ENGINE = "richos/engine/"


def git(*args, data=None):
    env = {k: v for k, v in os.environ.items() if k not in ("GIT_INDEX_FILE",)}
    result = subprocess.run(["git", *args], input=data, capture_output=True, env=env)
    return result.returncode, result.stdout


def other_side():
    """The commit being merged in, or None."""
    code, out = git("rev-parse", "-q", "--verify", "MERGE_HEAD")
    if code == 0 and out.strip():
        return out.decode().strip()
    heads = [key[len("GITHEAD_"):] for key in os.environ if re.fullmatch(r"GITHEAD_[0-9a-f]{40,64}", key)]
    if len(heads) == 1:
        return heads[0]
    action = os.environ.get("GIT_REFLOG_ACTION", "")
    if action.startswith("merge "):
        names = action[len("merge "):].split()
        if len(names) == 1:
            code, out = git("rev-parse", "-q", "--verify", names[0] + "^{commit}")
            if code == 0:
                return out.decode().strip()
    return None


def blob(rev, path):
    code, out = git("show", "%s:%s" % (rev, path))
    return out if code == 0 else None


def merged_file_digest(pin_path, map_path, revs):
    """sha256 of the pinned file as this merge will leave it, or None when that cannot be known."""
    ours, base, theirs = revs
    # A pin names a repository-relative path (richos/..., docs/...) or an engine-relative one
    # (scripts/...), exactly as renew-verification-pins.py reads it.
    candidates = [pin_path] if pin_path.startswith(("richos/", "docs/")) else [ENGINE + pin_path, pin_path]
    for path in candidates:
        versions = [blob(rev, path) for rev in (base, ours, theirs)]
        if versions[1] is None and versions[2] is None:
            continue
        b, o, t = versions
        if o == t:
            data = o
        elif b == o:
            data = t
        elif b == t:
            data = o
        elif None in versions:
            return None
        else:
            with tempfile.TemporaryDirectory() as tmp:
                files = []
                for name, content in (("ours", o), ("base", b), ("theirs", t)):
                    files.append(os.path.join(tmp, name))
                    with open(files[-1], "wb") as stream:
                        stream.write(content)
                code, data = git("merge-file", "-p", *files)
            if code != 0:
                return None
        return hashlib.sha256(data).hexdigest() if data is not None else None
    return None


class Conflict(Exception):
    pass


MISSING = object()


def merge(base, ours, theirs, where, pins):
    if ours == theirs:
        return ours
    if base == ours:
        return theirs
    if base == theirs:
        return ours
    if isinstance(ours, dict) and isinstance(theirs, dict):
        base = base if isinstance(base, dict) else {}
        result = {}
        order = list(ours) + [k for k in theirs if k not in ours]
        for key in order:
            b, o, t = base.get(key, MISSING), ours.get(key, MISSING), theirs.get(key, MISSING)
            if o is MISSING and t is MISSING:
                continue
            if o is MISSING:
                if t == b:
                    continue            # ours removed it, theirs left it alone
                value = t               # ours removed it, theirs changed or added it: kept
            elif t is MISSING:
                if o == b:
                    continue
                value = o
            else:
                value = merge(None if b is MISSING else b, o, t, where + [key], pins)
            result[key] = value
        return result
    if isinstance(ours, list) and isinstance(theirs, list):
        base = base if isinstance(base, list) else []
        key = lambda v: json.dumps(v, sort_keys=True)
        removed = {key(v) for v in base} - {key(v) for v in theirs}
        removed |= {key(v) for v in base} - {key(v) for v in ours}
        seen, result = set(), []
        for value in ours + theirs:
            k = key(value)
            if k in seen or k in removed:
                continue
            seen.add(k)
            result.append(value)
        return result
    if isinstance(ours, str) and isinstance(theirs, str) and PIN.fullmatch(ours) and PIN.fullmatch(theirs):
        pins.append(where)
        return ours
    notes = appended_notes(base, ours, theirs) if where and where[-1] in NOTES else None
    if notes is not None:
        return notes
    raise Conflict("/".join(map(str, where)))


# A review note (`evidence` in the dependency map, `review` in the qualifications map) is a running
# record: every renewal appends its own sentence to what was there. Two branches renewing one entry
# in parallel therefore each extend the same base note; both additions are kept, ours first.
NOTES = ("evidence", "review")


def appended_notes(base, ours, theirs):
    """Both notes kept when each side only appended to the base note, else None (a real conflict)."""
    if not all(isinstance(v, str) for v in (base, ours, theirs)):
        return None
    if theirs.startswith(ours):
        return theirs
    if ours.startswith(theirs):
        return ours
    if ours.startswith(base) and theirs.startswith(base):
        added = theirs[len(base):]
        if base and not added[:1].isspace() and not ours[-1:].isspace():
            added = " " + added
        return ours + added
    return None


def pin_path_of(document, where):
    """The file a pin at `where` names: the sibling path/source of a sha256, or a sources key."""
    parent = document
    for part in where[:-1]:
        parent = parent[part]
    if where[-1] == "sha256":
        for name in ("path", "source"):
            if isinstance(parent.get(name), str):
                return parent[name]
        return None
    if len(where) >= 2 and where[-2] == "sources":
        return where[-1]
    return None


def main(argv):
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    base_file, ours_file, theirs_file = argv[:3]
    label = argv[3] if len(argv) > 3 else ours_file
    try:
        documents = [json.load(open(path)) for path in (base_file, ours_file, theirs_file)]
        pins = []
        result = merge(*documents, [], pins)
    except (OSError, ValueError, Conflict) as exc:
        print("merge-verification-pins: %s: not merged as JSON (%s); leaving git's conflict markers"
              % (label, exc), file=sys.stderr)
        code, _ = git("merge-file", "-L", "ours", "-L", "base", "-L", "theirs", ours_file, base_file, theirs_file)
        return 1 if code else 0
    revs = None
    theirs_rev = other_side()
    if pins and theirs_rev:
        code, ours_rev = git("rev-parse", "HEAD")
        code2, base_rev = git("merge-base", "HEAD", theirs_rev)
        if code == 0 and code2 == 0:
            revs = (ours_rev.decode().strip(), base_rev.decode().split()[0], theirs_rev)
    unresolved = []
    for where in pins:
        path = pin_path_of(result, where)
        digest = merged_file_digest(path, label, revs) if (path and revs) else None
        parent = result
        for part in where[:-1]:
            parent = parent[part]
        if digest:
            parent[where[-1]] = digest
        else:
            unresolved.append(path or "/".join(map(str, where)))
    with open(ours_file, "w") as stream:
        stream.write(json.dumps(result, indent=2) + "\n")
    if unresolved:
        print("merge-verification-pins: %s: %d pin(s) both sides changed are left at ours because the "
              "merged file could not be read here: %s. Renew them after the merge: python3 "
              "richos/engine/scripts/lib/renew-verification-pins.py <file> ..."
              % (label, len(unresolved), ", ".join(sorted(set(unresolved)))), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

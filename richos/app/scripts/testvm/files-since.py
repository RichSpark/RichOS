#!/usr/bin/env python3
"""What was written, and where, between two moments: a file-date search with a positive control.

Runs INSIDE a test guest (push it with guest.sh --push). Standard library only.

  files-since.py mark <file>
      Create or touch <file>; its modification time is the moment.

  files-since.py scan --since <marker> [--until <marker>] --root <dir> [--root <dir>...]
                      [--inside <dir>...] [--declared <dir>=<reason>...] [--harness <dir>...]
                      [--baseline <scan.json>] [--control <file>...] [--named <word>...]
                      [--tolerate-unreadable <dir>...]
      Every file, symlink or newly born directory under the roots whose modification,
      change or birth time is after --since (and not after --until), classified. JSON on
      stdout. Exit 0 when every control was found and nothing NAMED sits outside --inside;
      1 when something named does; 2 when a control was not found, a --root could not
      be listed, or a directory below a root could not be listed and lies outside every
      --tolerate-unreadable directory (the search itself is not working, so its silence
      proves nothing about the subtree it never read).
      A guest's system folders always hold a few unreadable directories, so the caller
      names those folders with --tolerate-unreadable: what is unreadable under them is
      counted (tolerated_unreadable_count) and does not fail the search. An unreadable
      directory anywhere else, such as the home the app under test runs in, does.

WHY A POSITIVE CONTROL. "The search found nothing outside the folder" is a claim about
the search as much as about the app. A root that was not readable, a clock compared the
wrong way round, or a path filter that drops everything all produce the same empty
answer as a clean app. A control is a file written deliberately inside the window, in a
place the search must cover, whose name carries a --named word: if the scan does not
report it, and would not have flagged it as named had it not been declared a control,
the scan is broken and exits 2.

CLASSES, in the order they are decided:
  control   a --control file
  inside    under an --inside directory (where the thing under test is allowed to write)
  declared  under a --declared directory, with its reason (written by design, e.g. a link)
  harness   under a --harness directory (the walk's own payload and tools)
  named     outside all of the above, and its path contains a --named word
            (case-insensitive) — the finding this search exists for
  baseline  the same path also changed in the --baseline window, when nothing ran
  other     none of the above: reported for a person to read, never silently dropped

Symlinks are reported, never followed, so a linked directory is counted once, at the
real path it points to, under whatever class that real path has.
"""
import argparse
import json
import os
import sys
import time


def moment(path):
    return os.stat(path).st_mtime


def changed_at(st):
    born = getattr(st, 'st_birthtime', 0)
    return max(st.st_mtime, st.st_ctime), born


def under(path, prefixes):
    for p in prefixes:
        if path == p or path.startswith(p.rstrip('/') + '/'):
            return p
    return None


def scan(args):
    since = moment(args.since)
    until = moment(args.until) if args.until else None
    inside = [os.path.realpath(p) for p in args.inside]
    harness = [os.path.realpath(p) for p in args.harness]
    declared = {}
    for d in args.declared:
        path, _, reason = d.partition('=')
        declared[os.path.realpath(path)] = reason or 'declared'
    controls = {os.path.realpath(c) for c in args.control}
    named = [w.lower() for w in args.named]
    baseline = set()
    if args.baseline:
        with open(args.baseline) as f:
            baseline = {row['path'] for row in json.load(f)['files']}
    markers = {os.path.realpath(args.since)} | ({os.path.realpath(args.until)} if args.until else set())

    tolerate = [os.path.realpath(p) for p in args.tolerate_unreadable]
    rows, unreadable, unreadable_roots, unreadable_dirs, tolerated = [], [], [], [], []

    def cannot_list(e):
        path = os.path.realpath(str(e.filename))
        unreadable.append(str(e.filename))
        (tolerated if under(path, tolerate) else unreadable_dirs).append(path)
    for root in args.root:
        root = os.path.realpath(root)
        # A root that cannot be listed at all is a place the search was told to cover and
        # could not: a control found elsewhere proves nothing about it.
        try:
            with os.scandir(root):
                pass
        except OSError:
            unreadable_roots.append(root)
            continue
        for dirpath, dirnames, filenames in os.walk(root, topdown=True, onerror=cannot_list):
            for name in list(dirnames) + filenames:
                full = os.path.join(dirpath, name)
                try:
                    st = os.lstat(full)
                except OSError as e:
                    unreadable.append(full)
                    continue
                t, born = changed_at(st)
                is_dir = os.path.isdir(full) and not os.path.islink(full)
                # A directory's own times move whenever an entry inside it changes, so a
                # directory is reported only when it was BORN inside the window; its
                # contents are reported on their own.
                when = born if is_dir else t
                if when <= since or (until is not None and when > until):
                    continue
                if full in markers:
                    continue
                # A file's own size, so a listing says how much each leftover takes; a
                # directory or a link is 0 here (its contents are reported on their own).
                kind = 'dir' if is_dir else ('link' if os.path.islink(full) else 'file')
                rows.append({'path': full, 'kind': kind, 'at': round(when, 3),
                             'bytes': st.st_size if kind == 'file' else 0})
            # Never descend through a symlink; os.walk already does not follow them.

    counts = {}
    for row in rows:
        p = row['path']
        would_be_named = any(w in p.lower() for w in named)
        if p in controls:
            cls = 'control'
            row['would_be'] = 'named' if would_be_named else 'other'
        elif under(p, inside):
            cls = 'inside'
        elif under(p, list(declared)):
            cls = 'declared'
            row['reason'] = declared[under(p, list(declared))]
        elif under(p, harness):
            cls = 'harness'
        elif would_be_named:
            cls = 'named'
        elif p in baseline:
            cls = 'baseline'
        else:
            cls = 'other'
        row['class'] = cls
        counts[cls] = counts.get(cls, 0) + 1

    found = {r['path'] for r in rows if r['class'] == 'control'}
    missing = sorted(controls - found)
    unflagged = sorted(r['path'] for r in rows if r['class'] == 'control' and r.get('would_be') != 'named' and named)
    verdict = 'pass'
    if missing or unflagged or unreadable_roots or unreadable_dirs:
        verdict = 'search-broken'
    elif counts.get('named'):
        verdict = 'named-outside'
    out = {
        'since': since, 'until': until, 'scanned_at': time.time(),
        'roots': [os.path.realpath(r) for r in args.root],
        'inside': inside, 'declared': declared, 'harness': harness, 'named_words': named,
        'unreadable_roots': unreadable_roots, 'unreadable_dirs': sorted(unreadable_dirs),
        'tolerated_unreadable_count': len(tolerated),
        'controls_missing': missing, 'controls_not_flagged_named': unflagged,
        'unreadable_count': len(unreadable), 'unreadable_sample': unreadable[:20],
        'counts': counts, 'verdict': verdict,
        'files': sorted(rows, key=lambda r: r['path']),
    }
    json.dump(out, sys.stdout, indent=1)
    sys.stdout.write('\n')
    return {'pass': 0, 'named-outside': 1, 'search-broken': 2}[verdict]


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='cmd', required=True)
    m = sub.add_parser('mark'); m.add_argument('file')
    s = sub.add_parser('scan')
    s.add_argument('--since', required=True); s.add_argument('--until')
    s.add_argument('--root', action='append', required=True)
    s.add_argument('--inside', action='append', default=[])
    s.add_argument('--declared', action='append', default=[])
    s.add_argument('--harness', action='append', default=[])
    s.add_argument('--baseline'); s.add_argument('--control', action='append', default=[])
    s.add_argument('--named', action='append', default=[])
    s.add_argument('--tolerate-unreadable', action='append', default=[])
    a = p.parse_args()
    if a.cmd == 'mark':
        with open(a.file, 'a'):
            os.utime(a.file, None)
        return 0
    return scan(a)


if __name__ == '__main__':
    sys.exit(main())

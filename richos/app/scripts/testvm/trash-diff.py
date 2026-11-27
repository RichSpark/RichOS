#!/usr/bin/env python3
"""trash-diff.py - run IN THE GUEST: which Desktop items went to the Trash, told by inode.

  trash-diff.py mark LABEL
  trash-diff.py check LABEL (--moved NAME... | --moved-count N | --none) [--wait SECONDS]
  ... [--desktop DIR] [--trash DIR] [--state DIR]

`mark` records every top-level entry of the Desktop and of the Trash (name and inode, the link
itself for a symbolic link, never its target). `check` reads them again and answers, as one JSON
line, what left the Desktop and where it went:

  removed   names that were on the Desktop at the mark and whose inode is no longer there
  moved     those of them matched to an added Trash entry by inode (the Trash name may differ:
            Finder renames "untitled folder" to "untitled folder 2" when the Trash has one)
  trashed   every entry new in the Trash since the mark, by its name there
  kept      names still on the Desktop under the same inode

Told by inode because a name proves nothing here: several New Folders in a row are all
"untitled folder", one after another, and only the inode says which one moved.

The expectation decides the exit:
  --moved NAME...   exactly these Desktop items went to the Trash, nothing else left the Desktop
                    and nothing else is new in the Trash
  --moved-count N   exactly N Desktop items went to the Trash, nothing else left the Desktop
                    and nothing else is new in the Trash (a New Folder whose name the walk
                    does not choose)
  --none            nothing left the Desktop and nothing new is in the Trash
--wait SECONDS asks again until the expectation holds or the time is up (Finder moves an item a
moment after the key), and reports the last reading either way. With --none the wait is a window
watched to its end: the first move seen in it fails at once, and only a window with no move holds
(a no-move check that stopped at its first clean reading would miss a move that lands later).

Exit 0 when the expectation holds, 1 when it does not (with a sentence), 2 when it cannot answer
(no mark under that label, a folder it cannot read). Default folders are ~/Desktop and ~/.Trash,
the state under ~/qa-trash-diff. Added with the Desktop Forward Delete walk.
"""
import argparse
from collections import Counter, defaultdict, deque
import json
import os
from pathlib import Path
import sys
import time


def entries(folder):
    """{name: inode} of the folder's top-level entries; a symbolic link is the link itself."""
    out = {}
    with os.scandir(folder) as it:
        for e in it:
            if e.name in ('.DS_Store', '.localized'):
                continue
            out[e.name] = e.inode()
    return out


def reading(before, desktop, trash):
    d_now, t_now = entries(desktop), entries(trash)
    d_inodes = set(d_now.values())
    t_before = before['trash']
    new_links = Counter(t_now.values()) - Counter(t_before.values())
    # An unchanged name or a rename alone cannot prove that a link arrived.
    trashed = sorted(name for name, ino in t_now.items() if new_links[ino] and t_before.get(name) != ino)
    t_by_inode = defaultdict(deque)
    for name in trashed:
        t_by_inode[t_now[name]].append(name)
    removed = sorted(n for n, ino in before['desktop'].items() if ino not in d_inodes)
    moved_as = {}
    for name in removed:
        ino = before['desktop'][name]
        if new_links[ino] and t_by_inode[ino]:
            moved_as[name] = t_by_inode[ino].popleft()
            new_links[ino] -= 1
    return {
        'removed': removed,
        'moved': list(moved_as),
        'moved_as': moved_as,
        'trashed': trashed,
        'kept': sorted(n for n, ino in before['desktop'].items() if ino in d_inodes),
    }


def verdict(r, args):
    if args.none:
        if r['removed'] or r['trashed']:
            return 'expected nothing to move, but %s left the Desktop and %s is new in the Trash' % (
                r['removed'] or 'nothing', r['trashed'] or 'nothing')
        return None
    if r['removed'] != r['moved']:
        return 'left the Desktop without reaching the Trash: %s' % sorted(set(r['removed']) - set(r['moved']))
    extra = sorted(set(r['trashed']) - set(r['moved_as'].values()))
    if extra:
        return 'also new in the Trash, not one of the moved Desktop items: %s' % extra
    if args.moved is not None and sorted(args.moved) != r['moved']:
        return 'expected exactly %s in the Trash, moved %s' % (sorted(args.moved), r['moved'])
    if args.moved_count is not None and len(r['moved']) != args.moved_count:
        return 'expected %d Desktop item(s) in the Trash, moved %d: %s' % (args.moved_count, len(r['moved']), r['moved'])
    return None


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('action', choices=['mark', 'check'])
    p.add_argument('label')
    p.add_argument('--desktop', default=str(Path.home() / 'Desktop'))
    p.add_argument('--trash', default=str(Path.home() / '.Trash'))
    p.add_argument('--state', default=str(Path.home() / 'qa-trash-diff'))
    want = p.add_mutually_exclusive_group()
    want.add_argument('--moved', nargs='+', metavar='NAME')
    want.add_argument('--moved-count', type=int, metavar='N')
    want.add_argument('--none', action='store_true')
    p.add_argument('--wait', type=float, default=0.0, metavar='SECONDS')
    args = p.parse_args(argv)
    if '/' in args.label or args.label.startswith('.'):
        print('trash-diff: a label is a plain word, not a path', file=sys.stderr)
        return 2
    mark = Path(args.state) / (args.label + '.json')
    try:
        if args.action == 'mark':
            Path(args.state).mkdir(parents=True, exist_ok=True)
            data = {'desktop': entries(args.desktop), 'trash': entries(args.trash)}
            mark.write_text(json.dumps(data))
            print(json.dumps({'label': args.label, 'marked': True, 'desktop': sorted(data['desktop']),
                              'trash_count': len(data['trash'])}))
            return 0
        if args.moved is None and args.moved_count is None and not args.none:
            print('trash-diff: check needs an expectation: --moved NAME..., --moved-count N or --none', file=sys.stderr)
            return 2
        if not mark.is_file():
            print('trash-diff: no mark named %r under %s; run "mark %s" first' % (args.label, args.state, args.label),
                  file=sys.stderr)
            return 2
        before = json.loads(mark.read_text())
        deadline = time.monotonic() + max(0.0, args.wait)
        while True:
            r = reading(before, args.desktop, args.trash)
            why = verdict(r, args)
            if time.monotonic() >= deadline:
                break
            if args.none:
                if why is not None:
                    break  # a move appeared: fail now
            elif why is None:
                break
            time.sleep(0.2)
    except OSError as e:
        print('trash-diff: cannot read the folders: %s' % e, file=sys.stderr)
        return 2
    r.update(label=args.label, holds=why is None)
    if why:
        r['why'] = why
    print(json.dumps(r))
    return 0 if why is None else 1


if __name__ == '__main__':
    sys.exit(main())

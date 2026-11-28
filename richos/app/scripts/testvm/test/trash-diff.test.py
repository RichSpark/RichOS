#!/usr/bin/env python3
"""trash-diff.py against a temporary Desktop and Trash: each answer, each refusal."""
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'trash-diff.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    desk, trash, state = root / 'Desktop', root / '.Trash', root / 'state'
    desk.mkdir()
    trash.mkdir()
    target = root / 'target'
    target.mkdir()
    (target / 'inside.txt').write_text('kept')
    (desk / 'untitled folder').mkdir()
    (desk / 'Notes.txt').write_text('n')
    (desk / 'Keep.txt').write_text('k')
    os.symlink(target, desk / 'Folder link')
    (trash / 'untitled folder').mkdir()  # an earlier one: the next is renamed in the Trash

    def tool(*args):
        r = subprocess.run([sys.executable, str(TOOL), *args, '--desktop', str(desk), '--trash', str(trash),
                            '--state', str(state)], capture_output=True, text=True)
        return r.returncode, (json.loads(r.stdout) if r.stdout.strip().startswith('{') else {}), r.stderr

    code, out, err = tool('check', 'a', '--none')
    check('check with no mark cannot answer: exit 2 and a sentence', code == 2 and 'no mark' in err, (code, err))
    code, out, err = tool('check', 'a')
    check('check with no expectation cannot answer: exit 2', code == 2 and 'expectation' in err, (code, err))
    code, out, err = tool('mark', '../a')
    check('a label that is a path is refused: exit 2', code == 2, (code, err))

    code, out, _ = tool('mark', 'a')
    check('mark records the Desktop by name', code == 0 and out['desktop'] == ['Folder link', 'Keep.txt', 'Notes.txt', 'untitled folder'], out)
    code, out, _ = tool('check', 'a', '--none')
    check('nothing moved: --none holds', code == 0 and out['holds'] and out['kept'] == ['Folder link', 'Keep.txt', 'Notes.txt', 'untitled folder'], out)

    # Three move to the Trash the way Finder does it: renamed there, the link itself and not its target.
    os.rename(desk / 'untitled folder', trash / 'untitled folder 2')
    os.rename(desk / 'Notes.txt', trash / 'Notes.txt')
    os.rename(desk / 'Folder link', trash / 'Folder link')
    code, out, _ = tool('check', 'a', '--moved', 'untitled folder', 'Notes.txt', 'Folder link')
    check('exactly the three, told by inode, with the renamed Trash name', code == 0
          and out['moved'] == ['Folder link', 'Notes.txt', 'untitled folder']
          and out['moved_as']['untitled folder'] == 'untitled folder 2' and out['kept'] == ['Keep.txt'], out)
    check('the link target is untouched', (target / 'inside.txt').read_text() == 'kept')
    code, out, err = tool('check', 'a', '--moved', 'Notes.txt')
    check('a wrong list fails with a sentence: exit 1', code == 1 and not out['holds'] and 'expected exactly' in out['why'], out)
    code, out, _ = tool('check', 'a', '--moved-count', '3')
    check('--moved-count 3 holds', code == 0, out)
    code, out, _ = tool('check', 'a', '--none')
    check('--none fails when something moved', code == 1 and 'expected nothing' in out['why'], out)

    # A positive check fails when something besides the moved items is also new in the Trash
    # (review of the Desktop Forward Delete walk tools: an extra Trash entry passed before).
    (desk / 'Solo.txt').write_text('s')
    code, out, _ = tool('mark', 'extra')
    os.rename(desk / 'Solo.txt', trash / 'Solo.txt')
    (trash / 'Stray from elsewhere').mkdir()
    code, out, _ = tool('check', 'extra', '--moved', 'Solo.txt')
    check('--moved fails when another entry is also new in the Trash', code == 1 and out['moved'] == ['Solo.txt']
          and 'also new in the Trash' in out['why'] and 'Stray from elsewhere' in out['why'], out)
    code, out, _ = tool('check', 'extra', '--moved-count', '1')
    check('--moved-count fails the same way', code == 1 and 'also new in the Trash' in out['why'], out)
    os.rmdir(trash / 'Stray from elsewhere')
    code, out, _ = tool('check', 'extra', '--moved', 'Solo.txt')
    check('without the stray entry the same check holds', code == 0 and out['trashed'] == ['Solo.txt'], out)

    # An item that left the Desktop for somewhere other than the Trash is never "moved".
    code, out, _ = tool('mark', 'b')
    os.rename(desk / 'Keep.txt', root / 'Keep.txt')
    code, out, _ = tool('check', 'b', '--moved-count', '0', '--wait', '0.4')
    check('gone but not in the Trash fails, after waiting', code == 1 and out['removed'] == ['Keep.txt'] and out['moved'] == []
          and 'without reaching the Trash' in out['why'], out)

    # A pre-existing link shares the Desktop inode even when the Desktop file goes elsewhere.
    (desk / 'Linked.txt').write_text('linked')
    os.link(desk / 'Linked.txt', trash / 'Old link.txt')
    code, _, _ = tool('mark', 'hard-link')
    check('mark captures both hard links', code == 0)
    os.rename(desk / 'Linked.txt', root / 'Linked.txt')
    code, out, _ = tool('check', 'hard-link', '--moved', 'Linked.txt')
    check('an existing Trash hard link cannot prove a new move',
          code == 1 and out['removed'] == ['Linked.txt'] and out['moved'] == [] and out['trashed'] == []
          and 'without reaching the Trash' in out['why'], (code, out))
    os.rename(trash / 'Old link.txt', trash / 'Renamed link.txt')
    code, out, _ = tool('check', 'hard-link', '--moved', 'Linked.txt')
    check('renaming the old Trash link cannot prove a new move either',
          code == 1 and out['moved'] == [] and out['trashed'] == [], (code, out))

    os.rename(root / 'Linked.txt', desk / 'Linked.txt')
    code, _, _ = tool('mark', 'fresh-link')
    check('mark captures the Desktop file and renamed Trash link', code == 0)
    os.rename(desk / 'Linked.txt', trash / 'Moved link.txt')
    code, out, _ = tool('check', 'fresh-link', '--moved', 'Linked.txt')
    check('a genuinely added link proves the move even when its inode was already in the Trash',
          code == 0 and out['moved_as'] == {'Linked.txt': 'Moved link.txt'} and out['trashed'] == ['Moved link.txt'],
          (code, out))

    # --none watches its whole window: a move that lands after the first clean reading still fails.
    import threading
    (desk / 'Keep folder').mkdir()
    code, out, _ = tool('mark', 'late')
    threading.Timer(0.6, lambda: os.rename(desk / 'Keep folder', trash / 'Keep folder')).start()
    t0 = time.monotonic()
    code, out, _ = tool('check', 'late', '--none', '--wait', '2')
    took = time.monotonic() - t0
    check('--none fails on a move 0.6 s into a 2 s window, as soon as it lands', code == 1 and out['moved'] == ['Keep folder']
          and took < 1.8, (code, out, took))
    code, out, _ = tool('mark', 'quiet')
    t0 = time.monotonic()
    code, out, _ = tool('check', 'quiet', '--none', '--wait', '1')
    check('--none with no move holds only after watching the whole window', code == 0 and time.monotonic() - t0 >= 1.0, (code, out))

    os.chmod(desk, 0)
    code, out, err = tool('mark', 'c')
    os.chmod(desk, 0o755)
    check('an unreadable Desktop cannot answer: exit 2', code == 2 and 'cannot read' in err, (code, err))

print('trash-diff.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)

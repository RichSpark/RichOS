#!/usr/bin/env python3
"""trash-diff.py against a temporary Desktop and Trash: each answer, each refusal."""
import json
import os
import subprocess
import sys
import tempfile
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

    # An item that left the Desktop for somewhere other than the Trash is never "moved".
    code, out, _ = tool('mark', 'b')
    os.rename(desk / 'Keep.txt', root / 'Keep.txt')
    code, out, _ = tool('check', 'b', '--moved-count', '0', '--wait', '0.4')
    check('gone but not in the Trash fails, after waiting', code == 1 and out['removed'] == ['Keep.txt'] and out['moved'] == []
          and 'without reaching the Trash' in out['why'], out)

    os.chmod(desk, 0)
    code, out, err = tool('mark', 'c')
    os.chmod(desk, 0o755)
    check('an unreadable Desktop cannot answer: exit 2', code == 2 and 'cannot read' in err, (code, err))

print('trash-diff.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)

#!/usr/bin/env python3
"""Quit and reopen the packaged app while Codex entries wait in a legacy channel: each entry is
kept once. Guest fixture data only; never the operator's ~/.richos-coordination.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes
the clone:

  reserve.py -- run-walk.py --bundle ZIP --home FIXTURE --engine ENGINE --report REPORT -- \\
      codex-legacy-restart-walk.py --out DIR

FIXTURE is a home whose `.richos-coordination/rich-codex/to-rich.md` exists (the conventional
channel the app watches when no CODEX_CHANNELS is set, codex_watch.rs DEFAULT_CHANNELS).

WHAT IT CHECKS, in the guest:
  1. the app's own Codex watcher (engine scripts/lib/codex_watch.py --monitor --host-json)
     starts at launch and looks at the channel (its app-host offsets file appears);
  2. two entries appended to the channel are kept in the pending spool, two lines;
  3. the app's own Quit (the menu bar item, main.rs MENU_QUIT): the app is gone and so is the
     watcher pid its host-child.json recorded (the app's logs, saved beside the report as
     app-logs.txt, carry the Exit arm's "[richos] quit: the Codex watcher <pid> ended.");
  4. the app opened again: once its new watcher has looked three times, the spool is
     byte-identical, each entry once (nightly gate 2026-11-01: a restart told them again);
  5. the reopened app is quit by its own Quit and verified gone (CEO §54 addendum 4).

WHAT IT DOES NOT FORCE: the quit landing inside a look. That timing is forced, every run, by
engine/scripts/codex-watch.test.sh C11; this walk is the same path on the packaged app.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

ENTRIES = '## 2026-11-01 09:00Z — FIRST WAITING ENTRY\nfirst-waiting-line\n' \
          '## 2026-11-01 09:01Z — SECOND WAITING ENTRY\nsecond-waiting-line\n'


class Failed(Exception):
    pass


def until(seconds, done):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = done()
        if value:
            return value
        time.sleep(0.5)
    return done()


class Walk:
    def __init__(self, vm, out):
        self.vm, self.out = vm, out
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / vm
        self.state = state
        self.payload = (state / 'payload').read_text().strip()
        self.home = self.payload + '/home'
        self.watch = self.home + '/Library/Application Support/com.richos.app/engine-state/codex-watch'
        self.channel = self.home + '/.richos-coordination/rich-codex/to-rich.md'
        self.facts = {}

    def sh(self, command, timeout=60):
        return guest(self.vm, command, timeout)

    def pid(self):
        return int((self.state / 'app.pid').read_text().strip())

    def alive(self, pid):
        return self.sh('kill -0 %d 2>/dev/null && echo alive || true' % pid) == 'alive'

    def offsets_inode(self):
        return self.sh('stat -f %i ' + shlex.quote(self.watch + '/watch/app-host/offsets.json') + ' 2>/dev/null || true')

    def pending(self):
        return self.sh('cat ' + shlex.quote(self.watch + '/pending.jsonl') + ' 2>/dev/null || true')

    def logs(self):
        return self.sh('cat ' + shlex.quote(self.payload) + '/app.log ' + shlex.quote(self.payload)
                       + '/relaunch-*.log 2>/dev/null || true', 60)

    def looks(self, n, seconds=120):
        """The watcher has saved its state n more times (each save replaces the file)."""
        seen, last = [0], [self.offsets_inode()]

        def more():
            now = self.offsets_inode()
            if now and now != last[0]:
                seen[0], last[0] = seen[0] + 1, now
            return seen[0] >= n
        return until(seconds, more)

    def quit(self, label):
        pid = self.pid()
        r = subprocess.run([str(HERE / 'ax.sh'), self.vm, 'click', '--title', 'Quit RichOS', '--role', 'AXMenuItem',
                            '--in', 'menubar', '--first'], capture_output=True, text=True, timeout=60)
        if r.returncode:
            raise Failed('%s: the Quit menu item could not be pressed: %s' % (label, (r.stderr or r.stdout)[-400:]))
        gone = until(30, lambda: not self.alive(pid))
        self.facts[label] = {'pid': pid, 'gone': bool(gone)}
        if not gone:
            raise Failed('%s: the app (pid %d) was still running 30 s after its Quit' % (label, pid))
        return pid

    def run(self):
        if not until(120, self.offsets_inode):
            raise Failed('the Codex watcher never looked at the channel; app log tail: %r' % self.logs()[-800:])
        if not self.looks(2):
            raise Failed('the Codex watcher stopped looking')
        self.sh('printf %s ' + shlex.quote(ENTRIES) + ' >> ' + shlex.quote(self.channel))
        if not until(120, lambda: len(self.pending().splitlines()) == 2):
            raise Failed('the two waiting entries were not kept; spool: %r' % self.pending())
        before = self.pending()
        watcher = json.loads(self.sh('cat ' + shlex.quote(self.watch + '/host-child.json')))['pid']
        self.quit('first-quit')
        self.facts['watcher'] = {'pid': watcher}
        if not until(15, lambda: not self.alive(watcher)):
            raise Failed('the Codex watcher (pid %d) outlived the app\'s Quit' % watcher)
        launched = relaunch(self.vm)
        self.facts['reopened'] = launched
        if not self.looks(3):
            raise Failed('the reopened app\'s Codex watcher did not look three times; log: %r' % launched.get('log'))
        after = self.pending()
        texts = [json.loads(l) for l in after.splitlines() if l.strip()]
        self.facts['spool'] = {'lines_before': len(before.splitlines()), 'lines_after': len(texts),
                               'identical': before == after,
                               'first': sum('first-waiting-line' in t for t in texts),
                               'second': sum('second-waiting-line' in t for t in texts)}
        self.quit('final-quit')
        s = self.facts['spool']
        if not (s['identical'] and s['first'] == 1 and s['second'] == 1):
            raise Failed('after the reopen the spool holds first %d time(s), second %d time(s), identical=%s'
                         % (s['first'], s['second'], s['identical']))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out)
    verdict, why = 'PASS', None
    try:
        walk.run()
    except (Failed, RuntimeError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as exc:
        verdict, why = 'FAIL', str(exc)
    try:
        (a.out / 'app-logs.txt').write_text(walk.logs())
    except (RuntimeError, OSError, subprocess.TimeoutExpired) as exc:
        walk.facts['app-logs'] = 'not saved: %s' % exc
    report = {'verdict': verdict, 'why': why, **walk.facts}
    (a.out / 'codex-legacy-restart.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if verdict == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())

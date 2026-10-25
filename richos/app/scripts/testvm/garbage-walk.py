#!/usr/bin/env python3
"""garbage-walk.py — what the regular app leaves behind after ordinary use, a quit and a forced kill.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      garbage-walk.py --out DIR --expect-sha SHA [--gone PATTERN ...]

THE QUESTION (CEO §54 addendum 2, 2026-09-18): "The RichOS app must always clean up its garbage
automatically, no matter what." Which files and folders does the app create and not remove, and
does a later start, a settled job or a quit remove them; a crash or a force-quit at the next start.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). stu-reminder-walk.py's
identity, first-run, job (a job for Stu that needs a teammate, SETTLED) and idle steps, unchanged,
then the app's own Quit, a restart, a SIGKILL of the app by its recorded pid, and a restart again:

  mark          a marker dated two seconds before the running app's own start: everything the
                listings report was written after the app started
  list-*        files-since.py over the app's HOME, /private/tmp, the guest user's own
                /private/var/folders directory, the guest's real ~/Library and /Users/Shared:
                every file, link and newly born directory written since the mark that still exists,
                with its size, as listing-<name>.json, grouped by place in the step's evidence; and
                the processes still running from this payload or named claude / provider-supervisor
  quit          the app's Quit (the menu bar item, main.rs MENU_QUIT); "Quit and stop the work" if
                asked; the recorded pid gone within --quit-within seconds
  restart       relaunch.py; the composer back; then --settle seconds for start-up work
  kill          SIGKILL to the recorded pid, verified to be this payload's app first
  --gone        a regular expression for a kind of path the app must clean up. list-quit FAILS
                naming each matching path the running app had that is still there after its Quit;
                list-final each matching path the killed run left that is still there after the
                next start. What the run now running made is its own. Without --gone the walk only
                reports (the discovery run).

A positive control (a file named garbage-walk-control, written in the app's HOME after the mark) must
be found by every listing, or the listing FAILS: a search that cannot see its own control proves
nothing about what it did not report.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
from collections import defaultdict
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
srw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(srw)
from relaunch import guest  # noqa: E402

StepFailed = srw.StepFailed
CONTROL = 'garbage-walk-control'
STEPS = ['identity', 'mark', 'first-run', 'job', 'idle', 'list-running', 'quit', 'list-quit', 'restart',
         'list-restart', 'kill', 'list-kill', 'restart-after-kill', 'list-final']
# Which earlier listing a listing judges --gone against: after the Quit, what the running app had;
# after the restart that follows the kill, what the killed run left.
BEFORE = {'quit': 'running', 'final': 'kill'}
# The processes still running that this walk reports: this payload's, and any claude or supervisor.
# The probe leaves out itself and the shell that ran it, whose command lines carry the same words.
PROCS = ('import json,os,subprocess,sys\n'
         'me={os.getpid(),os.getppid()}\n'
         'out=subprocess.run(["ps","-axo","pid=,ppid=,command="],capture_output=True,text=True).stdout\n'
         'rows=[]\n'
         'for l in out.splitlines():\n'
         '    p=l.split(None,2)\n'
         '    if len(p)<3 or int(p[0]) in me: continue\n'
         '    c=p[2]\n'
         '    if sys.argv[1] in c or "provider-supervisor" in c or "/claude" in c.split(" ")[0]:\n'
         '        rows.append({"pid":int(p[0]),"ppid":int(p[1]),"command":c[:300]})\n'
         'print(json.dumps(rows))\n')


def place(path, roots):
    """The leftover's place: its root and up to three names below it."""
    for root in sorted(roots, key=len, reverse=True):
        if path == root or path.startswith(root.rstrip('/') + '/'):
            rel = path[len(root):].strip('/').split('/')
            return root.rstrip('/') + '/' + '/'.join(rel[:3])
    return path


class GarbageWalk(srw.StuReminderWalk):
    def tool(self):
        return self.payload + '/garbage-walk/files-since.py'

    def recorded_pid(self):
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = (state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('the recorded app pid is not a number: %r' % pid)
        return int(pid)

    def mark(self):
        pid = self.recorded_pid()
        made = self.payload + '/garbage-walk'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        srw.command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'files-since.py', made + '/'], 300)
        # The app's own start, read from the process: the mark is two seconds before it.
        script = ('import os,subprocess,sys,time\n'
                  'pid=sys.argv[1]\n'
                  'lst=subprocess.run(["ps","-o","lstart=","-p",pid],capture_output=True,text=True,'
                  'env=dict(os.environ,LC_ALL="C")).stdout.strip()\n'
                  'start=time.mktime(time.strptime(lst,"%a %b %d %H:%M:%S %Y"))\n'
                  'm=sys.argv[2]\n'
                  'open(m,"a").close(); os.utime(m,(start-2,start-2))\n'
                  'print(start)\n')
        started = float(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), pid, shlex.quote(made + '/mark')), 60))
        uid = guest(self.vm, 'id -u', 30).strip()
        user_tmp = guest(self.vm, 'getconf DARWIN_USER_TEMP_DIR', 30).strip().rstrip('/')
        # /private/var/folders/xx/yyyy/T -> the user's own folder, holding T, C and 0.
        user_folders = os.path.dirname(os.path.realpath(user_tmp)) if user_tmp else ''
        if not user_folders.startswith('/var/folders/') and not user_folders.startswith('/private/var/folders/'):
            raise StepFailed('the guest user\'s temporary folder is not under /var/folders: %r' % user_tmp)
        if not user_folders.startswith('/private'):
            user_folders = '/private' + user_folders
        real_home = guest(self.vm, 'echo "$HOME"', 30).strip()
        self.facts['garbage'] = {'mark': made + '/mark', 'app_started': started, 'app_pid': pid, 'uid': uid,
                                 'roots': [self.home, '/private/tmp', user_folders, real_home + '/Library',
                                           '/Users/Shared'],
                                 'harness': [self.payload + '/garbage-walk', self.home + '/Library/Keychains']}
        self.save()
        return self.facts['garbage']

    def listing(self, name):
        g = self.facts.get('garbage')
        if not g:
            raise StepFailed('mark must have run (no marker on record)')
        control = self.home + '/' + CONTROL + '-' + name
        guest(self.vm, 'echo control > ' + shlex.quote(control), 30)
        argv = ['/usr/bin/python3', self.tool(), 'scan', '--since', g['mark'], '--control', control,
                '--named', CONTROL]
        for r in g['roots']:
            argv += ['--root', r]
        for h in g['harness']:
            argv += ['--harness', h]
        # The system folders always hold a few directories this user cannot read; the app's
        # HOME is not one of them, so an unreadable directory there still breaks the search.
        for r in g['roots'][1:]:
            argv += ['--tolerate-unreadable', r]
        raw = guest(self.vm, shlex.join(argv) + '; echo "exit=$?" >&2', 600)
        guest(self.vm, 'rm -f ' + shlex.quote(control), 30)
        out = json.loads(raw)
        (self.out / ('listing-%s.json' % name)).write_text(json.dumps(out, indent=1) + '\n')
        if out.get('controls_missing') or out.get('unreadable_roots') or out.get('unreadable_dirs'):
            raise StepFailed('the search is not working: control missing %s, unreadable roots %s, unreadable '
                             'directories %s' % (out.get('controls_missing'), out.get('unreadable_roots'),
                                                 out.get('unreadable_dirs', [])[:5]))
        rows = [r for r in out['files'] if r['class'] not in ('control', 'harness')]
        places = defaultdict(lambda: {'files': 0, 'dirs': 0, 'bytes': 0})
        for r in rows:
            p = places[place(r['path'], g['roots'])]
            p['files' if r['kind'] != 'dir' else 'dirs'] += 1
            p['bytes'] += r.get('bytes', 0)
        procs = json.loads(guest(self.vm, 'python3 -c %s %s' % (shlex.quote(PROCS), shlex.quote(self.payload)), 60) or '[]')
        summary = {'entries': len(rows), 'bytes': sum(r.get('bytes', 0) for r in rows),
                   'places': dict(sorted(places.items())), 'processes': procs}
        (self.out / ('summary-%s.json' % name)).write_text(json.dumps(summary, indent=1) + '\n')
        before = BEFORE.get(name)
        if self.a.gone and before:
            # A LEFTOVER is a path of a kind the app must clean up that was there in the listing
            # before (the running app's, before its Quit; the killed run's, before the restart) and
            # is still there now. What the run now running made is its own, not a leftover.
            try:
                earlier = {r['path'] for r in json.loads((self.out / ('listing-%s.json' % before)).read_text())['files']}
            except (OSError, ValueError, KeyError):
                raise StepFailed('listing-%s.json is needed to judge what was left since it' % before)
            kinds = [re.compile(p) for p in self.a.gone]
            judged = sorted(p for p in earlier if any(k.search(p) for k in kinds))
            left = sorted(r['path'] for r in rows if r['path'] in earlier and any(k.search(r['path']) for k in kinds))
            summary['judged_since'] = before
            summary['were_there'] = judged
            summary['still_there'] = left
            (self.out / ('summary-%s.json' % name)).write_text(json.dumps(summary, indent=1) + '\n')
            if left:
                raise StepFailed('%d of %d leftover(s) from the %s listing are still there, first: %s'
                                 % (len(left), len(judged), before, left[:10]))
        return summary

    def list_running(self):
        return self.listing('running')

    def list_quit(self):
        return self.listing('quit')

    def list_restart(self):
        return self.listing('restart')

    def list_kill(self):
        return self.listing('kill')

    def list_final(self):
        return self.listing('final')

    def gone(self, pid, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if guest(self.vm, 'kill -0 %d 2>/dev/null && echo alive || true' % pid, 30) != 'alive':
                return True
            time.sleep(0.5)
        return False

    def quit(self):
        pid = self.recorded_pid()
        began = time.monotonic()
        # The app's own Quit item (main.rs, MENU_QUIT) is in the menu bar, which is in no window.
        self.ax('click', '--title', 'Quit RichOS', '--role', 'AXMenuItem', '--in', 'menubar', '--first')
        asked = False
        end = time.monotonic() + 15
        while time.monotonic() < end:
            if guest(self.vm, 'kill -0 %d 2>/dev/null && echo alive || true' % pid, 30) != 'alive':
                break
            try:
                if self.present('Quit and stop the work'):
                    self.press('Quit and stop the work')
                    asked = True
                    break
            except StepFailed as exc:
                # With nothing registered the app quits at once, so the app can be gone between
                # the check above and this read (garbage walk 1, walk-823596393f20).
                if 'noprocess' not in str(exc):
                    raise
                break
            time.sleep(1)
        if not self.gone(pid, self.a.quit_within):
            raise StepFailed('the app (pid %d) was still running %.0f s after its Quit' % (pid, self.a.quit_within))
        return {'pid': pid, 'asked_to_stop_work': asked, 'gone_after_s': round(time.monotonic() - began, 1)}

    def restart(self):
        evidence = self.relaunch()
        time.sleep(self.a.settle)
        return dict(evidence, settled_s=self.a.settle)

    def restart_after_kill(self):
        return self.restart()

    def kill(self):
        pid = self.recorded_pid()
        rows = {r['pid'] for r in self.app_rows()}
        if pid not in rows:
            raise StepFailed('the recorded pid %d is not this payload\'s app (running: %s)' % (pid, sorted(rows)))
        guest(self.vm, 'kill -KILL %d' % pid, 30)
        if not self.gone(pid, 10):
            raise StepFailed('the app (pid %d) survived SIGKILL for 10 s' % pid)
        return {'pid': pid, 'signal': 'KILL'}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=srw.JOB)
    p.add_argument('--within', type=float, default=300)
    p.add_argument('--job-within', type=float, default=1200)
    p.add_argument('--idle-within', type=float, default=900)
    p.add_argument('--approvals', type=int, default=3)
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--quit-within', type=float, default=30, help='seconds for the app to be gone after its Quit')
    p.add_argument('--settle', type=float, default=60, help='seconds after a restart before the listing')
    p.add_argument('--gone', action='append', default=[],
                   help='regular expression for a kind of guest path the app must clean up: after the Quit '
                        'and after the restart that follows the kill, none that was there before may remain')
    p.add_argument('--gone-file', type=Path,
                   help='a file of --gone expressions, one per line (blank lines and # lines skipped)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    if a.gone_file:
        a.gone += [line.strip() for line in a.gone_file.read_text().splitlines()
                   if line.strip() and not line.lstrip().startswith('#')]
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = GarbageWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'gone': a.gone, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

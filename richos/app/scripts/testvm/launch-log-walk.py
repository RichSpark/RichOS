#!/usr/bin/env python3
"""launch-log-walk.py: after a launch with no terminal, is its timing line in the launch log?

A launch from the Dock or Finder has no terminal, so what the app writes to its error output is
kept nowhere. The app keeps each launch's timing line in ~/Library/Logs/RichOS/launch.log
(src-tauri/src/launch_timing.rs). This walk installs a build in a guest, starts it as a Dock
click starts it (`open -a`, through LaunchServices, with NO --stdout or --stderr, so the file is
the only place the line can be), waits for that launch's entry, quits the app by the pid captured
at spawn, checks it is gone, and prints the file.

Run by run-walk.py with --no-app (the walk installs what it launches); never on the host's screen:

  run-walk.py --no-app --home EMPTY_DIR --engine none --report R -- \\
      launch-log-walk.py --candidate CANDIDATE.zip --out DIR [--launches 2] [--splash SPLASH_DIR]

Exit 0 when every launch has its own entry (its pid, "launch timing:", a first frame) and every
instance is gone; 1 when an entry is missing; 2 on a harness failure or an instance left running.
"""
import argparse
import importlib.util
import json
import os
import re
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
INSTALLED = '/Applications/RichOS.app'
LOG = 'Library/Logs/RichOS/launch.log'


def entry_for(text, pid):
    """This pid's launch line in the launch log, or None."""
    for line in text.splitlines():
        if ' pid %d ' % pid in line and 'launch timing:' in line:
            return line
    return None


def figures(entry):
    """The milestones of one entry, in milliseconds after the process started."""
    names = (('update step', r'update step (\d+) ms'), ('window', r'window (\d+) ms'),
             ('first frame', r'first frame (\d+) ms'), ('splash started playing', r'splash started playing (\d+) ms'),
             ('start-up finished', r'start-up finished (\d+) ms'), ('page took over', r'page took over (\d+) ms'))
    return {name: int(m.group(1)) for name, pattern in names if (m := re.search(pattern, entry))}


def in_guest(a):
    """Inside the guest: the launches, one after another, each quit before the next."""
    spec = importlib.util.spec_from_file_location('probe', os.path.join(os.path.dirname(a.guest), 'launch-speed-guest.py'))
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    plan = json.load(open(a.guest))
    log = os.path.join(plan['env']['HOME'], LOG)
    rows = []
    for n in range(plan['launches']):
        before = {r[0] for r in probe.ps_rows()}
        cmd = ['open', '-n', '-a', INSTALLED]
        for k, v in plan['env'].items():
            cmd += ['--env', '%s=%s' % (k, v)]
        subprocess.run(cmd, capture_output=True)
        pid, entry, end = None, None, time.time() + plan['settle']
        while time.time() < end and entry is None:
            if pid is None:
                pid = next((r[0] for r in probe.ps_rows() if probe.owned(r, before, [INSTALLED])), None)
            elif os.path.exists(log):
                entry = entry_for(open(log, errors='replace').read(), pid)
            time.sleep(0.1)
        time.sleep(1.5)
        how = probe.quit_pid(pid, 'Quit RichOS') if pid is not None else 'no process found'
        rows.append({'launch': n + 1, 'pid': pid, 'entry': entry, 'quit': how,
                     'gone': pid is not None and not probe.alive(pid)})
    print(json.dumps({'rows': rows, 'log': open(log, errors='replace').read() if os.path.exists(log) else None}))
    return 0


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--candidate', type=Path, help='the build under test, a RichOS zip')
    p.add_argument('--out', type=Path)
    p.add_argument('--launches', type=int, default=2)
    p.add_argument('--splash', type=Path,
                   help="a packaged build's Contents/Resources/Splash folder, put into the installed app so "
                        'the native splash plays (a branch bundle carries none)')
    p.add_argument('--guest', help='(inside the guest) the plan file')
    a = p.parse_args()
    if a.guest:
        return in_guest(a)
    if not (a.vm and a.candidate and a.out):
        p.error('the VM name (run-walk.py passes it), --candidate and --out are required')
    sys.path.insert(0, str(HERE))
    from relaunch import guest  # the committed helper

    a.out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
    payload = (state / 'payload').read_text().strip()
    if not payload.startswith('/Users/') or '/testvm/' not in payload:
        print('not a guest payload: ' + payload, file=sys.stderr)
        return 2
    q = shlex.quote

    def push(src, dest):
        subprocess.run([str(HERE / 'guest.sh'), a.vm, '--push', str(src), dest], check=True, timeout=900)

    work = payload + '/launch-log'
    guest(a.vm, 'mkdir -p ' + q(work + '/candidate'))
    for tool in ('launch-speed-guest.py', 'launch-log-walk.py'):
        push(HERE / tool, work + '/' + tool)
    push(a.candidate, work + '/candidate/' + a.candidate.name)
    guest(a.vm, 'cd ' + q(work + '/candidate') + ' && ditto -x -k ' + q(a.candidate.name) + ' . && rm -f ' + q(a.candidate.name), 600)
    staged = guest(a.vm, 'ls -d ' + q(work + '/candidate') + '/*.app').splitlines()
    if len(staged) != 1:
        print('the candidate zip must hold exactly one app', file=sys.stderr)
        return 2
    guest(a.vm, 'sudo -n rm -rf ' + INSTALLED + ' && sudo -n ditto ' + q(staged[0]) + ' ' + INSTALLED
          + ' && sudo -n chown -R "$(id -un)" ' + INSTALLED
          + ' && (xattr -dr com.apple.quarantine ' + INSTALLED + ' 2>/dev/null || true)', 600)
    if a.splash:
        # A branch bundle has no native splash packs; a packaged build's are put where it has them.
        subprocess.run(['tar', 'czf', str(a.out / 'splash.tgz'), '--exclude', '._*', '-C', str(a.splash.parent), a.splash.name],
                       check=True, env={**os.environ, 'COPYFILE_DISABLE': '1'})
        push(a.out / 'splash.tgz', work + '/splash.tgz')
        resources = INSTALLED + '/Contents/Resources'
        guest(a.vm, 'mkdir -p ' + resources + ' && tar xzf ' + q(work + '/splash.tgz') + ' -C ' + resources
              + (' && mv ' + q(resources + '/' + a.splash.name) + ' ' + resources + '/Splash' if a.splash.name != 'Splash' else '')
              + ' && codesign --force -s - ' + INSTALLED, 300)
    home = payload + '/home'
    plan ={'launches': a.launches, 'settle': 120,
            'env': {'HOME': home, 'CLAUDE_CONFIG_DIR': home + '/.claude', 'RICHOS_ACTIVATION': 'regular',
                    'RICHOS_CLAUDE_BIN': '/Users/admin/.local/bin/claude', 'DISABLE_AUTOUPDATER': '1',
                    'RICHOS_ENGINE_DIR': payload + '/engine'}}
    (a.out / 'plan.json').write_text(json.dumps(plan, indent=1))
    push(a.out / 'plan.json', work + '/plan.json')
    said = guest(a.vm, 'python3 ' + q(work + '/launch-log-walk.py') + ' --guest ' + q(work + '/plan.json'), 900)
    result = json.loads(said.splitlines()[-1])
    (a.out / 'launch.log').write_text(result['log'] or '')
    left = guest(a.vm, 'pgrep -fl richos-tauri || true')
    for row in result['rows']:
        row['figures'] = figures(row['entry']) if row['entry'] else None
    (a.out / 'result.json').write_text(json.dumps({'rows': result['rows'], 'left_running': left}, indent=1) + '\n')
    print('the launch log, %s in the guest:' % (home + '/' + LOG))
    print(result['log'] or '(no file)')
    for row in result['rows']:
        print('launch %d, pid %s: %s; %s; %s' % (
            row['launch'], row['pid'],
            ', '.join('%s %d ms' % kv for kv in row['figures'].items()) if row['figures'] else 'NO ENTRY',
            row['quit'], 'gone' if row['gone'] else 'STILL RUNNING'))
    print('richos-tauri processes left in the guest: ' + (left or 'none'))
    if left or not all(row['gone'] for row in result['rows']):
        return 2
    return 0 if all(row['figures'] and 'first frame' in row['figures'] for row in result['rows']) else 1


if __name__ == '__main__':
    sys.exit(main())

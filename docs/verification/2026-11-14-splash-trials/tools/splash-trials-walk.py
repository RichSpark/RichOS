#!/usr/bin/env python3
"""splash-trials-walk.py: the three trials of Sage's plan section 7, in the test guest.

Run by run-walk.py (richos/app/scripts/testvm), which holds one guest slot for this run only,
boots the guest, launches the bundle once and removes the clone afterwards:

  run-walk.py --wait 1800 --bundle ZIP --home FIXTURE --engine ENGINE --report REPORT -- \\
      splash-trials-walk.py --testvm <richos>/richos/app/scripts/testvm --frames <screen-frames> --out DIR \\
      [--phases t2,t1,t1nopush,t3png,t3jpg,dictionary]

run-walk.py passes the owned VM name as the first argument. Never on the host's screen.

  1. quits the instance run.sh started (by its recorded pid);
  2. pushes screen-frames and splash-trials-guest.py and runs the phases in the guest;
  3. pulls everything into --out, including what was left running (must be nothing).
"""
import argparse
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    p = argparse.ArgumentParser()
    p.add_argument('vm')
    p.add_argument('--testvm', type=Path, required=True)
    p.add_argument('--frames', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--phases', default='t2,t1,t1nopush,t3png,t3jpg,t3png2x,t3jpg2x,dictionary')
    p.add_argument('--opening2x', type=Path, help='a directory holding the 2x pixel-count opening set')
    p.add_argument('--t1-runs', type=int, default=20)
    p.add_argument('--dictionary-runs', type=int, default=10)
    a = p.parse_args()
    sys.path.insert(0, str(a.testvm))
    from relaunch import guest, quit_app  # the committed helpers

    def g(command, timeout=120):
        return guest(a.vm, command, timeout)

    def push(src, dest):
        subprocess.run([str(a.testvm / 'guest.sh'), a.vm, '--push', str(src), dest], check=True, timeout=300)

    a.out.mkdir(parents=True, exist_ok=True)
    payload = quit_app(a.vm)  # run.sh's instance, by its recorded pid
    time.sleep(2)
    app = g('find ' + shlex.quote(payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
    home = payload + '/home'
    work = payload + '/splash-trials'
    g('mkdir -p ' + shlex.quote(work))
    push(HERE / 'splash-trials-guest.py', work + '/splash-trials-guest.py')
    push(a.frames, work + '/screen-frames')
    g('chmod +x ' + shlex.quote(work + '/screen-frames'))
    opening2x = ''
    if a.opening2x:
        bundle = a.out / 'opening2x.tgz'
        subprocess.run(['tar', 'czf', str(bundle), '-C', str(a.opening2x.parent), a.opening2x.name], check=True)
        push(bundle, work + '/opening2x.tgz')
        g('cd ' + shlex.quote(work) + ' && tar xzf opening2x.tgz && rm opening2x.tgz', 300)
        bundle.unlink()
        opening2x = work + '/' + a.opening2x.name
    facts = {
        'vm': a.vm, 'app': app,
        'guest_os': g('sw_vers -productVersion'),
        'guest_cpu': g('sysctl -n hw.ncpu') + ' cpus, ' + g('sysctl -n hw.memsize') + ' bytes',
        'display': g('system_profiler SPDisplaysDataType 2>/dev/null | grep -i -e resolution -e "UI Looks" || true'),
        'load_before': g('sysctl -n vm.loadavg'),
    }
    env = {'HOME': home, 'CLAUDE_CONFIG_DIR': home + '/.claude', 'RICHOS_ACTIVATION': 'regular',
           'RICHOS_CLAUDE_BIN': '/Users/admin/.local/bin/claude', 'DISABLE_AUTOUPDATER': '1'}
    cmd = ('cd ' + shlex.quote(work) + ' && python3 splash-trials-guest.py --app ' + shlex.quote(app)
           + ' --out ' + shlex.quote(work + '/out') + ' --frames ' + shlex.quote(work + '/screen-frames')
           + ' --phases ' + shlex.quote(a.phases) + ' --t1-runs %d --dictionary-runs %d ' % (a.t1_runs, a.dictionary_runs)
           + (' --opening2x ' + shlex.quote(opening2x) + ' ' if opening2x else '')
           + ' '.join('--env ' + shlex.quote(k + '=' + v) for k, v in env.items()) + ' 2>&1 || true')
    facts['guest_run'] = g(cmd, 3000)
    facts['load_after'] = g('sysctl -n vm.loadavg')
    facts['left_running'] = g('ps -axo pid=,ppid=,comm= | grep -e "/richos-tauri$" -e "/Dictionary$" | grep -v grep || true')
    g('cd ' + shlex.quote(work) + ' && tar czf out.tgz out', 900)
    subprocess.run([str(a.testvm / 'guest.sh'), a.vm, '--pull', work + '/out.tgz', str(a.out / 'out.tgz')],
                   check=True, timeout=1800)
    (a.out / 'facts.json').write_text(json.dumps(facts, indent=1))
    print(json.dumps(facts, indent=1))
    return 0 if not facts['left_running'] and 'not gone 0' in facts['guest_run'] else 1


if __name__ == '__main__':
    sys.exit(main())

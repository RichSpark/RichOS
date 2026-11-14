#!/usr/bin/env python3
"""splash-trials-guest.py: runs INSIDE the test guest. The three trials of Sage's plan
(richos-hq/docs/plans/2026-11-14-existing-splash-under-half-a-second.md, section 7), and the
CEO's benchmark (Dictionary) for how fast this guest is that day, one launch at a time.

  splash-trials-guest.py --app RichOS.app --out DIR --frames ./screen-frames --env K=V ... \
      [--phases t2,t1,t1nopush,t3png,t3jpg,dictionary] [--t1-runs 10] [--dictionary-runs 10]

Every launch goes through LaunchServices (`open -n -a`, the route a Dock click takes), with its
request instant on the guest's clock. Launches that are timed are recorded by screen-frames
(ScreenCaptureKit, each new frame stamped with the instant macOS displayed it). Every instance is
quit by the pid captured when it appeared (new, parent 1, the app's own executable), TERM then
KILL after 8 s, and checked gone with every process descended from it (captured before the quit,
so a grandchild that outlives its parent is still checked).
"""
import argparse
import json
import os
import subprocess
import sys
import threading
import time


def ps_rows():
    out = subprocess.run(['ps', '-axo', 'pid=,ppid=,comm='], capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3:
            rows.append((int(parts[0]), int(parts[1]), parts[2]))
    return rows


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def quit_pid(pid):
    if not alive(pid):
        return 'already gone'
    os.kill(pid, 15)
    end = time.time() + 8
    while time.time() < end:
        if not alive(pid):
            return 'quit on TERM'
        time.sleep(0.05)
    try:
        os.kill(pid, 9)
    except OSError:
        pass
    time.sleep(0.3)
    return 'killed after 8 s' if not alive(pid) else 'STILL ALIVE'


def follow(path, stop, lines):
    pos, buf = 0, b''
    while not stop.is_set():
        try:
            with open(path, 'rb') as f:
                f.seek(pos)
                chunk = f.read()
        except OSError:
            chunk = b''
        if chunk:
            now = time.time()
            pos += len(chunk)
            buf += chunk
            while b'\n' in buf:
                line, buf = buf.split(b'\n', 1)
                lines.append((now, line.decode('utf-8', 'replace')))
        time.sleep(0.002)


def descendants(pid, rows=None):
    """Every process descended from `pid` (children, grandchildren, ...), from one ps snapshot."""
    rows = ps_rows() if rows is None else rows
    found, frontier = [], [pid]
    while frontier:
        parent = frontier.pop()
        for row in rows:
            if row[1] == parent and row[0] not in found:
                found.append(row[0])
                frontier.append(row[0])
    return found


def rss_kb(pid):
    out = subprocess.run(['ps', '-o', 'rss=', '-p', str(pid)], capture_output=True, text=True).stdout.strip()
    return int(out) if out.isdigit() else None


class Launch:
    """One launch: optional recording, the app's log followed, the pid found, then quit."""

    def __init__(self, a, name, app, exe, env, record_s, until=None, settle=10.0, serve=None):
        self.dir = os.path.join(a.out, name)
        os.makedirs(self.dir, exist_ok=True)
        log = os.path.join(self.dir, 'app.log')
        before = {r[0] for r in ps_rows()}
        load = os.getloadavg()
        rec = None
        if record_s:
            rec = subprocess.Popen([a.frames, os.path.join(self.dir, 'frames'), str(record_s)],
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            first = rec.stdout.readline().strip()
            if not first.startswith('STARTED'):
                raise SystemExit('screen-frames did not start: ' + first + rec.stdout.read())
            time.sleep(0.7)
        stop = threading.Event()
        lines = []
        t = threading.Thread(target=follow, args=(log, stop, lines))
        t.start()
        cmd = ['open', '-n', '-a', app]
        for kv in env:
            cmd += ['--env', kv]
        cmd += ['--stdout', log, '--stderr', log]
        t_open = time.time()
        subprocess.run(cmd, capture_output=True)
        t_open_done = time.time()
        exe_path = os.path.join(app, 'Contents/MacOS', exe)
        pid = t_pid = None
        end = t_open + settle
        rss = []
        done_at = None
        while time.time() < end:
            if pid is None:
                for row in ps_rows():
                    if row[0] not in before and row[1] == 1 and row[2] == exe_path:
                        pid, t_pid = row[0], time.time()
                        break
            if pid is not None and len(rss) < 400:
                rss.append((time.time(), rss_kb(pid)))
            if serve is not None:
                serve(self.dir)
            if until and done_at is None and until(self.dir, lines):
                done_at = time.time()
                end = min(end, done_at + 1.5)
            time.sleep(0.02)
        if rec is not None:
            rest = rec.communicate(timeout=60)[0]
        else:
            rest = ''
        stop.set()
        t.join()
        children = descendants(pid) if pid is not None else []
        how = quit_pid(pid) if pid is not None else 'no pid found'
        time.sleep(0.5)
        for c in children:
            if alive(c):
                quit_pid(c)
        gone = pid is not None and not alive(pid) and not any(alive(c) for c in children)
        self.result = {
            'name': name, 'load_before': load, 't_open': t_open, 't_open_done': t_open_done, 'pid': pid, 't_pid_seen': t_pid,
            'children': children, 'quit': how, 'all_gone': gone, 'recorder': rest.strip(),
            'lines': [{'t': tt, 'text': x} for tt, x in lines], 'rss_kb': rss,
        }
        with open(os.path.join(self.dir, 'launch.json'), 'w') as f:
            json.dump(self.result, f)
        print('%s: pid %s %s, all gone %s, %d log lines; %s' % (name, pid, how, gone, len(lines), rest.strip()), flush=True)
        time.sleep(1.5)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', required=True)
    p.add_argument('--out', required=True)
    p.add_argument('--frames', required=True)
    p.add_argument('--env', action='append', default=[])
    p.add_argument('--phases', default='t2,t1,t1nopush,t3png,t3jpg,dictionary')
    p.add_argument('--t1-runs', type=int, default=20)
    p.add_argument('--t3-runs', type=int, default=3)
    p.add_argument('--opening2x', default='', help='a directory with the 2x pixel-count opening set')
    p.add_argument('--settle-load', type=float, default=3.0,
                   help='before the first launch, wait (at most 300 s) until the 1-minute load is under this')
    p.add_argument('--dictionary-runs', type=int, default=10)
    a = p.parse_args()
    os.makedirs(a.out, exist_ok=True)
    t2 = os.path.join(a.out, 't2-files')
    opening = os.path.join(t2, 'opening')
    phases = a.phases.split(',')
    results = []
    settle_start = time.time()
    while os.getloadavg()[0] >= a.settle_load and time.time() - settle_start < 300:
        time.sleep(5)
    print('settled %.0f s, load %s' % (time.time() - settle_start, os.getloadavg()), flush=True)

    def busy_over(_d, lines):
        return any('[trial]' in x and 'removed' in x for _, x in lines)

    if 't2' in phases:
        os.makedirs(t2, exist_ok=True)
        served = set()

        def serve(_d):
            for k in range(3):
                req = os.path.join(t2, 'req-%d.json' % k)
                if k in served or not os.path.exists(req):
                    continue
                try:
                    r = json.load(open(req))
                except ValueError:
                    continue
                x, y, w, h = r['content']
                stamp = time.time()
                subprocess.run(['screencapture', '-x', '-R%d,%d,%d,%d' % (round(x), round(y), round(w), round(h)),
                                os.path.join(t2, 'screen-%d.png' % k)], capture_output=True)
                subprocess.run(['screencapture', '-x', '-o', '-l%d' % r['window'],
                                os.path.join(t2, 'screen-window-%d.png' % k)], capture_output=True)
                open(os.path.join(t2, 'ack-%d' % k), 'w').write('%.3f %.3f' % (stamp, time.time()))
                served.add(k)

        def t2_done(_d, _lines):
            return os.path.exists(os.path.join(t2, 'trial2-done'))

        env = a.env + ['RICHOS_TRIAL=2', 'RICHOS_TRIAL_DIR=' + t2]
        results.append(Launch(a, 't2', a.app, 'richos-tauri', env, 0, until=t2_done, settle=150, serve=serve).result)

    def run(name, extra, runs, record=7, opening_dir=None):
        for i in range(runs):
            env = a.env + extra + ['RICHOS_TRIAL_DIR=' + t2, 'RICHOS_TRIAL_OPENING=' + (opening_dir or opening)]
            results.append(Launch(a, '%s-%02d' % (name, i + 1), a.app, 'richos-tauri', env, record, until=busy_over, settle=12).result)

    if 't1' in phases:
        run('t1', ['RICHOS_TRIAL=1', 'RICHOS_TRIAL_PICTURE=' + os.path.join(opening, 'full-000.png')], a.t1_runs)
    if 't1nopush' in phases:
        run('t1nopush', ['RICHOS_TRIAL=1', 'RICHOS_TRIAL_NOPUSH=1', 'RICHOS_TRIAL_PICTURE=' + os.path.join(opening, 'full-000.png')], 2)
    if 't3png' in phases:
        run('t3png', ['RICHOS_TRIAL=3', 'RICHOS_TRIAL_EXT=png', 'RICHOS_TRIAL_THREADS=4'], a.t3_runs)
    if 't3jpg' in phases:
        run('t3jpg', ['RICHOS_TRIAL=3', 'RICHOS_TRIAL_EXT=jpg', 'RICHOS_TRIAL_THREADS=4'], a.t3_runs)
    if 't3png2x' in phases and a.opening2x:
        run('t3png2x', ['RICHOS_TRIAL=3', 'RICHOS_TRIAL_EXT=png', 'RICHOS_TRIAL_THREADS=4'], a.t3_runs, opening_dir=a.opening2x)
    if 't3jpg2x' in phases and a.opening2x:
        run('t3jpg2x', ['RICHOS_TRIAL=3', 'RICHOS_TRIAL_EXT=jpg', 'RICHOS_TRIAL_THREADS=4'], a.t3_runs, opening_dir=a.opening2x)
    if 'dictionary' in phases:
        for i in range(a.dictionary_runs):
            results.append(Launch(a, 'dictionary-%02d' % (i + 1), '/System/Applications/Dictionary.app', 'Dictionary',
                                  [], 4, settle=5).result)
    with open(os.path.join(a.out, 'results.json'), 'w') as f:
        json.dump(results, f)
    left = [r for r in results if not r['all_gone']]
    print('launches %d, not gone %d' % (len(results), len(left)))
    return 0 if not left else 1


if __name__ == '__main__':
    sys.exit(main())

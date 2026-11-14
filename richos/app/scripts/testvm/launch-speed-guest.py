#!/usr/bin/env python3
"""launch-speed-guest.py: runs INSIDE the test guest for launch-speed-walk.py. Never on a host.

Launches apps through LaunchServices (`open -n -a`, the route a Dock click takes) and records, on
the guest's one clock, the instant each `open` was started, the process it started and every line
the app wrote (stamped as it arrived). Times are read afterwards from the app's own launch line
and the guest's system log, never from screen frames (Sage's review of the launch-speed audit,
piece 4, item 1); a frame is taken only to show WHAT was on screen, untimed.

It quits only processes it captured at spawn (new, parent 1, executable inside one of --owns),
by the app's own menu Quit first (so the next launch is a fresh one, which shows the splash), and
verifies each is gone. Exit 0 when every instance it started is gone.

  launch-speed-guest.py --plan PLAN.json --out DIR

PLAN.json is a list of launches, in order:
  {"label": "...", "app": "/Applications/RichOS.app", "owns": ["/Applications/RichOS.app", ...],
   "env": {"HOME": "..."}, "menu_quit": "Quit RichOS", "done": "launch timing:", "settle": 20,
   "purge": false, "frame_at": 1.0, "quit_at": null, "alert": false}
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
        if len(parts) == 3 and parts[0].isdigit() and parts[1].isdigit():
            rows.append((int(parts[0]), int(parts[1]), parts[2]))
    return rows


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def menu_quit(pid, title):
    """The app's own Quit, through its menu in the accessibility tree (never a keystroke)."""
    script = ('tell application "System Events" to tell (first process whose unix id is %d) '
              'to click menu item "%s" of menu 1 of menu bar item 2 of menu bar 1' % (pid, title))
    return subprocess.run(['osascript', '-e', script], capture_output=True, text=True, timeout=30)


def wait_gone(pid, seconds):
    end = time.time() + seconds
    while time.time() < end:
        if not alive(pid):
            return True
        time.sleep(0.05)
    return not alive(pid)


def quit_pid(pid, title):
    if not alive(pid):
        return 'already gone'
    if title:
        menu_quit(pid, title)
        if wait_gone(pid, 15):
            return 'quit from its own menu'
    os.kill(pid, 15)
    if wait_gone(pid, 8):
        return 'quit on TERM'
    try:
        os.kill(pid, 9)
    except OSError:
        pass
    return 'killed after TERM' if wait_gone(pid, 3) else 'STILL ALIVE'


def dismiss_alert():
    """Press the start-up alert's button. The alert is shown by UserNotificationCenter."""
    script = ('tell application "System Events" to tell process "UserNotificationCenter" '
              'to click button 1 of window 1')
    return subprocess.run(['osascript', '-e', script], capture_output=True, text=True, timeout=30)


def follow(path, stop, lines):
    pos = 0
    buf = b''
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


def frame(path):
    subprocess.run(['screencapture', '-x', '-t', 'jpg', path], capture_output=True, timeout=30)


def owned(row, before, owns):
    return row[0] not in before and row[1] == 1 and any(row[2].startswith(o.rstrip('/') + '/') for o in owns)


def launch(n, step, out, seen=None):
    run_dir = os.path.join(out, 'run%02d' % n)
    os.makedirs(run_dir, exist_ok=True)
    log = os.path.join(run_dir, 'app.log')
    if step.get('purge'):
        # The disk cache flushed, as after a restart: staging the bundle had just read every file.
        subprocess.run(['sudo', '-n', 'purge'], capture_output=True, timeout=300)
        time.sleep(2)
    before = {r[0] for r in ps_rows()}
    stop = threading.Event()
    lines = []
    follower = threading.Thread(target=follow, args=(log, stop, lines), daemon=True)
    follower.start()
    cmd = ['open', '-n', '-a', step['app']]
    for k, v in step.get('env', {}).items():
        cmd += ['--env', '%s=%s' % (k, v)]
    cmd += ['--stdout', log, '--stderr', log]
    t_open = time.time()
    subprocess.run(cmd, capture_output=True)
    t_open_done = time.time()
    pid = t_pid = None
    events = []
    end = t_open + step.get('settle', 20)
    done_at = None
    # Untimed pictures: one at `frame_at`, or one at each of `frames_at` (seconds after `open`).
    shots = sorted(step.get('frames_at') or ([step['frame_at']] if step.get('frame_at') is not None else []))
    quit_at = step.get('quit_at')
    while time.time() < end:
        now = time.time()
        if pid is None:
            for row in ps_rows():
                if owned(row, before, step['owns']):
                    pid, t_pid = row[0], now
                    if seen is not None:
                        seen["pid"] = pid
                    break
        if shots and now - t_open >= shots[0]:
            shot = shots.pop(0)
            frame(os.path.join(run_dir, ('frame-%.2fs.jpg' if step.get('frames_at') else 'frame-%.1fs.jpg') % shot))
            events.append({'what': 'frame', 't': now, 'at': shot})
        if quit_at is not None and pid is not None and now - t_open >= quit_at:
            # The app's menu exists a moment after its process does: the Quit is pressed again
            # until a press lands, and the press that landed is the one recorded.
            r = menu_quit(pid, step['menu_quit'])
            pressed_until = time.time() + 20
            while r.returncode != 0 and alive(pid) and time.time() < pressed_until:
                time.sleep(0.3)
                r = menu_quit(pid, step['menu_quit'])
            events.append({'what': 'quit pressed during start-up', 't': time.time(), 'osascript': r.returncode,
                           'stderr': r.stderr.strip()[:300],
                           'start_up_finished_before': any('boot complete' in t for _, t in lines)})
            quit_at = None
            gone = wait_gone(pid, 30)
            events.append({'what': 'gone after the quit' if gone else 'STILL RUNNING 30 s after the quit',
                           't': time.time()})
            break
        if step.get('alert') and pid is not None and any('application startup:' in t for _, t in lines):
            time.sleep(1.5)
            frame(os.path.join(run_dir, 'frame-alert.jpg'))
            events.append({'what': 'alert frame', 't': time.time()})
            # A panic in start-up can raise the alert twice (the panic, then the abort that ends
            # the process from inside the window's callback); each is dismissed as it comes.
            gone = False
            for _ in range(3):
                r = dismiss_alert()
                events.append({'what': 'alert dismissed', 't': time.time(), 'osascript': r.returncode,
                               'stderr': r.stderr.strip()[:300]})
                gone = wait_gone(pid, 10)
                if gone:
                    break
            events.append({'what': 'exited after the alert' if gone else 'STILL RUNNING after the alert',
                           't': time.time()})
            break
        if step.get('done') and done_at is None and any(step['done'] in t for _, t in lines):
            done_at = now
            end = min(end, now + 1.5)
        # A build older than the launch line never prints it: its boot's end, plus time for the
        # page to load and the system log to record it, ends the watch instead of the full settle.
        if step.get('done') and done_at is None and any('boot complete' in t for _, t in lines):
            end = min(end, now + 6.0)
        time.sleep(0.02)
    stop.set()
    follower.join()
    children = [r[0] for r in ps_rows() if pid is not None and r[1] == pid]
    how = quit_pid(pid, step.get('menu_quit', '')) if pid is not None else 'no process found'
    time.sleep(0.5)
    # Children the app left behind are stopped together, not one 8-second wait after another.
    left = [c for c in children if alive(c)]
    for c in left:
        try:
            os.kill(c, 15)
        except OSError:
            pass
    deadline = time.time() + 8
    while time.time() < deadline and any(alive(c) for c in left):
        time.sleep(0.05)
    for c in left:
        if alive(c):
            try:
                os.kill(c, 9)
            except OSError:
                pass
    gone = pid is not None and not alive(pid) and not any(alive(c) for c in children)
    # What the app said while it quit (the splash picked for the next start, its opening drawn, the
    # update put in place), read after it ended: unstamped.
    try:
        with open(log, 'rb') as f:
            said = f.read().decode('utf-8', 'replace').splitlines()
    except OSError:
        said = []
    result = {'n': n, 'label': step['label'], 'app': step['app'], 't_open': t_open, 't_open_done': t_open_done,
              'pid': pid, 't_pid_seen': t_pid, 'quit': how, 'all_gone': gone, 'events': events,
              'expect_splash': step.get('expect_splash'),
              'lines': [{'t': t, 'text': text} for t, text in lines],
              'quit_lines': said[len(lines):]}
    print('launch %d %s pid %s: %s, all gone %s, %d log lines' % (n, step['label'], pid, how, gone, len(lines)),
          flush=True)
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--plan', required=True)
    p.add_argument('--out', required=True)
    a = p.parse_args()
    os.makedirs(a.out, exist_ok=True)
    with open(a.plan) as f:
        plan = json.load(f)
    started = time.time()
    results = []
    for n, step in enumerate(plan, 1):
        if step.get('before'):
            subprocess.run(['/bin/sh', '-c', step['before']], capture_output=True, timeout=600)
        seen = {}
        try:
            results.append(launch(n, step, a.out, seen))
        except Exception as error:  # one launch that could not run is a missing result, never every result
            # A process it had already started is quit by its captured pid and checked gone, as any.
            pid = seen.get('pid')
            how = quit_pid(pid, step.get('menu_quit', '')) if pid else 'no process found'
            results.append({'n': n, 'label': step['label'], 'app': step['app'], 't_open': None, 'pid': pid,
                            'quit': 'not launched: %s; %s' % (error, how), 'all_gone': not (pid and alive(pid)),
                            'events': [], 'lines': [], 'expect_splash': step.get('expect_splash'), 'quit_lines': []})
            print('launch %d %s could not run: %s' % (n, step['label'], error), flush=True)
        if step.get('after'):
            subprocess.run(['/bin/sh', '-c', step['after']], capture_output=True, timeout=600)
        time.sleep(step.get('pause', 2.0))
    with open(os.path.join(a.out, 'probe.json'), 'w') as f:
        json.dump({'started': started, 'ended': time.time(), 'results': results}, f)
    return 0 if all(r['all_gone'] or r['pid'] is None for r in results) else 1


if __name__ == '__main__':
    sys.exit(main())

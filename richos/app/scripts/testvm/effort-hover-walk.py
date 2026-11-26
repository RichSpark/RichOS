#!/usr/bin/env python3
"""effort-hover-walk.py — Rich on Opus in Settings, on the real app in a guest, dark and light:
High is chosen and named recommended, and the pointer over the effort steps, down across
"Recommended", never moves the steps.

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      effort-hover-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument, quits the app by its pid and deletes
the clone when this returns (CEO 54). Nothing is on the host's screen (CEO ruling 65), and the
guest plays no sound (run-walk.py's default: tart's null speaker).

THE QUESTION (CEO 2026-11-26): "for Rich let's change our recommendation (and default setting) to
"High" when the user selects Opus instead of Fable for Rich", and "a weird jittering issue when
moving the mouse above "Recommended"". His recording: the head over the steps had "Low ·
recommended for Opus" on a line of its own; the pointer just under the steps previewed a level
with a short name, the head lost that line, the steps jumped up out from under the pointer, the
preview ended, and back, again and again.

First the build is checked (the boot log's "this app: built from" against --expect-sha) and the
first-run sheets are closed. Then, for each theme (Settings -> Dark theme, then Light theme):
  opus      Rich's row switched to Opus (the first Opus button, the conversation's row). PASS needs
            the High step marked chosen (each step's own checked state, read in the guest) and
            "recommended for Opus" read off a picture of the screen (opus-<theme>.png).
  hover     the real pointer, no button held (pointer-drag.sh --hover), at the foot of the Low,
            Medium, High and Extra high steps and down across "Recommended" in 4 px moves; after
            each move the High step's position is read from the accessibility tree, and at the
            first point the pointer rests while it is read five more times. PASS needs every
            reading at the position read with the pointer away (within 1 px). Pictures:
            hover-<theme>.png, the pointer resting at the foot of Medium (a short name
            previewed, which is what made the steps jump before).

Evidence in --out: report.json and the pictures. Exit 0 when every step passes.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

LEVELS = ['low', 'medium', 'high', 'xhigh', 'max']


class StepFailed(Exception):
    pass


def command(argv, timeout=60):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


class Walk:
    def __init__(self, vm, out, expect_sha):
        self.vm, self.out, self.expect_sha = vm, out, expect_sha
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / vm
        self.payload = (state / 'payload').read_text().strip()

    def ax(self, *args):
        """ax.sh's matches as dictionaries (its first line is the search's own meta row)."""
        rows = []
        for line in command([HERE / 'ax.sh', self.vm, *args, '--json']).splitlines():
            line = line.strip()
            if not line.startswith('{'):
                continue
            row = json.loads(line)
            if row.get('error'):
                raise StepFailed('ax.sh %s: %s' % (' '.join(args), json.dumps(row)))
            if not row.get('meta'):
                rows.append(row)
        return rows

    def node(self, dom_id):
        found = [n for n in self.ax('find', '--id', dom_id, '--first') if n.get('role')]
        if not found:
            raise StepFailed('#%s is not on screen' % dom_id)
        return found[0]

    def until(self, what, test, seconds=60, every=2):
        end = time.monotonic() + seconds
        while True:
            try:
                value = test()
            except StepFailed:
                value = None
            if value:
                return value
            if time.monotonic() >= end:
                raise StepFailed('%s did not happen within %g s' % (what, seconds))
            time.sleep(every)

    def shot(self, name):
        command([HERE / 'shot.sh', self.vm, self.out / name], 120)
        return name

    def hover(self, x, y):
        command([HERE / 'pointer-drag.sh', self.vm, '%d,%d' % (round(x), round(y)), '--hover', '--rest', '0.4'], 60)

    def found(self, *args):
        try:
            return bool([n for n in self.ax('find', *args, '--first') if n.get('role')])
        except StepFailed:
            return False

    def ready(self):
        """In from the home screen, then the first-run sheets closed. The app opens on the home
        screen, whose live canvas starves accessibility reads, so it is recognized by OCR on a
        frame and left by Enter, its own hint for Talk to Rich, sent to the app's pid
        (asked-again-walk.py's way). Then the sheets a development bundle shows
        (stu-reminder-walk.py's decline_first_run_sheets: it carries no engine to install, so its
        setup sheet offers only `Close`, pressed by its exact title, never the window's own close
        button), until the window has been free of them for two reads in a row."""
        home = None
        began = time.monotonic()
        while time.monotonic() - began < 120:
            self.shot('home.png')
            r = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich', str(self.out / 'home.png'),
                                '--first', '--quiet'], capture_output=True, text=True, timeout=120)
            if r.returncode == 0:
                home = round(time.monotonic() - began, 1)
                break
            time.sleep(2)
        if home is None:
            raise StepFailed('the home screen ("Talk to Rich") was not on screen within 120 s (home.png)')
        command([HERE / 'ax.sh', self.vm, '--key', '36'])
        presses, clear = 0, 0
        end = time.monotonic() + 180
        while time.monotonic() < end and clear < 2:
            if self.found('--title', 'Not now', '--role', 'AXButton'):
                self.ax('click', '--title', 'Not now', '--role', 'AXButton', '--first')
            elif self.found('--title', 'Close', '--role', 'AXButton'):
                self.ax('click', '--title', 'Close', '--role', 'AXButton', '--first')
            else:
                clear = clear + 1 if self.found('--id', 'set-btn') else 0
                time.sleep(4)
                continue
            presses, clear = presses + 1, 0
            time.sleep(3)
        if clear < 2:
            self.shot('not-ready.png')
            raise StepFailed('the window never came free of first-run sheets (%d pressed; not-ready.png)' % presses)
        return {'home_on_screen_within_s': home, 'first_run_sheets_closed': presses}

    def open_settings(self):
        try:
            self.node('ef-rich-low')
            return
        except StepFailed:
            pass
        self.ax('click', '--id', 'set-btn', '--first')
        self.until('the effort control in Settings', lambda: self.node('ef-rich-low'))

    def identity(self):
        line = guest(self.vm, 'grep -m1 "this app: built from" ' + shlex.quote(self.payload + '/app.log'), 60)
        built = line.rsplit(' ', 1)[-1]
        if not built.startswith(self.expect_sha[:12]):
            raise StepFailed('the running app was built from %s, not %s' % (built, self.expect_sha))
        return {'built_from': built}

    def theme(self, theme):
        self.open_settings()
        self.ax('click', '--title', '%s theme' % theme.capitalize(), '--first')
        time.sleep(1.5)
        self.open_settings()
        return {'theme': theme}

    def opus(self, theme):
        self.open_settings()
        self.ax('click', '--title', 'Opus', '--role', 'AXRadioButton', '--nth', '0')
        values = {}

        def chosen():
            for level in LEVELS:
                values[level] = self.node('ef-rich-%s' % level).get('value', '')
            checked = [lv for lv in LEVELS if str(values[lv]).strip().lower() in ('1', 'true')]
            return checked if checked == ['high'] else None
        self.until('High chosen in Rich\'s row on Opus', chosen, 30)
        self.hover(5, 5)
        picture = self.shot('opus-%s.png' % theme)
        read = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'recommended for Opus', str(self.out / picture),
                               '--first', '--quiet'], capture_output=True, text=True, timeout=120)
        if read.returncode != 0:
            raise StepFailed('"recommended for Opus" is not on the picture of the screen (%s): %s'
                             % (picture, (read.stdout + read.stderr).strip()))
        return {'checked': values, 'recommended_for_opus_on_screen': True, 'picture': picture}

    def hover_step(self, theme):
        self.hover(5, 5)
        time.sleep(0.5)
        base = self.node('ef-rich-high')
        steps = {lv: self.node('ef-rich-%s' % lv) for lv in ('low', 'medium', 'high', 'xhigh')}
        readings, moved = [], []
        for lv, step in steps.items():
            cx = step['x'] + step['w'] / 2
            foot = step['y'] + step['h']
            for dy in range(-4, 32, 4):
                self.hover(cx, foot + dy)
                tries = 6 if dy == 4 else 1
                for _ in range(tries):
                    now = self.node('ef-rich-high')
                    readings.append({'over': lv, 'dy': dy, 'y': now['y'], 'x': now['x']})
                    if abs(now['y'] - base['y']) > 1 or abs(now['x'] - base['x']) > 1:
                        moved.append(readings[-1])
                if lv == 'medium' and dy == 4:
                    picture = self.shot('hover-%s.png' % theme)
        self.hover(5, 5)
        result = {'high_step_away': {'x': base['x'], 'y': base['y']}, 'readings': len(readings),
                  'moved': moved, 'picture': picture}
        if moved:
            raise StepFailed('the steps moved under the pointer: %s' % json.dumps(moved[:6]))
        return result


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    w = Walk(a.vm, a.out, a.expect_sha)
    report, failed = {'steps': {}}, False

    def step(name, fn, *args):
        nonlocal failed
        started = time.monotonic()
        try:
            report['steps'][name] = {'pass': True, **fn(*args)}
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as e:
            failed = True
            report['steps'][name] = {'pass': False, 'error': str(e)}
        report['steps'][name]['seconds'] = round(time.monotonic() - started, 1)
        print('[walk] %s %s' % (name, 'PASS' if report['steps'][name]['pass'] else 'FAIL: ' + report['steps'][name]['error']), flush=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        return report['steps'][name]['pass']

    if step('identity', w.identity) and step('ready', w.ready):
        for theme in ('dark', 'light'):
            if step('theme-' + theme, w.theme, theme) and step('opus-' + theme, w.opus, theme):
                step('hover-' + theme, w.hover_step, theme)
    report['pass'] = not failed
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())

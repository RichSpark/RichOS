#!/usr/bin/env python3
"""home-fps-walk.py — the home screen's frame rate in the real app, before and after the memory count
changes, measured in the guest and never on the host's screen (CEO ruling 65).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      home-fps-walk.py --out DIR [--alder 1800] [--brightwater 900] [--span 3] [--samples 3]

run-walk.py passes the owned VM name as the first argument.

WHY (round 75, richos-hq a4c16a665): in headless WebKit the loro picture ran at 17 to 26 frames a
second for the rest of a visit after any change of the memory count; the pass that colors every
thread, inside the engine's very long frame function, was sometimes left unoptimized. This walk asks
the same question of the app's own window.

HOW IT COUNTS: frame-rate.swift, compiled on the host and copied into the guest, streams the app's window with ScreenCaptureKit
and counts the frames whose content changed (`.complete`). The picture moves on every frame it draws
(twinkle, packets, drift), so that count is the frames the window presented. Nothing is injected into
the app. Each measurement is --samples spans of --span seconds, the first 4 s after the moment named.

THE STEPS, in order:
  example     a fresh user's home screen: the example, the count never changes; measured, then
              again --later seconds after that
  companies   two fictional companies' repositories and a team (home-standard-walk.py's step), and the
              app relaunched as a returning user's starts: the example comes up first
  during      the example while the repositories are read (the count has not changed yet)
  change      his records arrive while the screen is up (the count changes in the running app: the
              left column comes to read "N memories"); measured once the reading has finished, then
              again --later seconds after that
  relaunch    the app relaunched: the same records drawn at launch, the count never changes in the
              visit; the control for `change`

Evidence: --out/report.json (every sample, every step's result) and a frame of the screen per
measurement. Exit 0 when every step ran; the rates are the result, not a pass mark. It quits nothing
itself: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
"""
import argparse
import importlib.util
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
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('home_standard_walk', HERE / 'home-standard-walk.py')
standard = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(standard)
StepFailed = standard.StepFailed

STEPS = ['example', 'companies', 'during', 'change', 'relaunch']
SETTLE = 4  # seconds after the moment named before the first sample


class Walk(standard.Walk):
    def __init__(self, vm, out, alder, brightwater, span, samples, later):
        super().__init__(vm, out, alder, brightwater)
        self.span, self.samples, self.later = span, samples, later
        self.meter = self.payload + '/frame-rate'

    def pid(self):
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = (state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('the recorded app pid is not numeric')
        return pid

    def build_meter(self):
        """frame-rate.swift compiled on the host (seconds, against minutes in a fresh guest) for the
        guest's macOS and copied in; the build lands in --out, beside the evidence."""
        binary = self.out / 'frame-rate'
        code, text = standard.run(['swiftc', '-O', '-target', 'arm64-apple-macos14.0', HERE / 'frame-rate.swift', '-o', binary], 600)
        if code:
            raise StepFailed('frame-rate.swift did not compile: ' + text[-600:])
        code, text = standard.run([HERE / 'guest.sh', self.vm, '--push', binary, self.meter], 120)
        if code:
            raise StepFailed('could not copy frame-rate into the guest: ' + text[-300:])
        binary.unlink()

    def measure(self, name):
        """--samples spans of --span seconds of the app's window, and one frame of the screen."""
        time.sleep(SETTLE)
        pid = self.pid()
        wait = int(self.span * self.samples + 60)
        r = subprocess.run([str(HERE / 'guest.sh'), self.vm, shlex.quote(self.meter) + ' %s %s %d' % (pid, self.span, self.samples)],
                           capture_output=True, text=True, timeout=wait)
        if r.returncode:
            raise StepFailed('the frame rate could not be measured (exit %d): ' % r.returncode + (r.stderr + r.stdout)[-1200:])
        row = json.loads(r.stdout.strip().splitlines()[-1])
        row['fps'] = [s['fps'] for s in row['samples']]
        row['at'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
        # the guests share this Mac's GPU and cores: a second guest running beside this one is part of
        # the conditions a rate was measured under, so it is written down with it
        runs = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run'
        row['other_guests'] = sorted(p.name for p in runs.iterdir() if p.is_dir() and p.name != self.vm)
        row['host_load'] = [round(x, 1) for x in os.getloadavg()]
        row['frame'] = self.shot('fps-' + name)
        print('  %s: %s frames a second (other guests: %s)' % (name, ', '.join('%g' % f for f in row['fps']), ', '.join(row['other_guests']) or 'none'), flush=True)
        return row

    def memories(self):
        """The left column's "N memories", read from the accessibility tree, or None."""
        node = self.node('--id', 'home-brand-line')
        text = (node or {}).get('value') or (node or {}).get('title') or ''
        if 'memories' not in text:
            found = self.text(' memories')
            text = (found or {}).get('value') or (found or {}).get('title') or ''
        return text if 'memories' in text else None

    def to_home(self):
        """home-standard-walk.py's, with the sheets declined on every try: a first-run sheet that comes
        up after the first look (the memory setup's "Where should I keep what you tell me?") covers the
        rail, and a press on Home under it does nothing."""
        for _ in range(8):
            self.decline_sheets()
            if self.node('--id', 'home-enter'):
                return True
            self.ax('click', '--id', 'nr-home', '--first')
            time.sleep(3)
        raise StepFailed('the home screen did not come up')

    # ---- the steps ------------------------------------------------------------------------
    def example(self):
        self.build_meter()
        self.to_home()
        time.sleep(8)  # the curtain lifts and the picture draws itself in
        first = self.measure('example')
        time.sleep(self.later)
        return {'measured': first, 'measured_later': self.measure('example-later')}

    def companies(self):
        began = time.monotonic()
        result = super().companies()
        result['relaunched_after_s'] = round(time.monotonic() - began, 1)
        self.relaunched = time.monotonic()
        return result

    def during(self):
        if self.kept():
            return {'skipped': 'the repositories were already read before the example could be measured'}
        return {'memories_column': self.memories(), 'measured': self.measure('during')}

    def change(self):
        path = self.until(self.kept, 600, 'the repositories were not read and kept')
        read_s = round(time.monotonic() - self.relaunched, 1)
        code, text = standard.run([HERE / 'guest.sh', self.vm, '--pull', path, self.out / 'home-field-repos.json'], 300)
        if code:
            raise StepFailed('could not pull the kept picture: ' + text[-300:])
        counts = json.loads((self.out / 'home-field-repos.json').read_text())['field']['meta']['counts']
        (self.out / 'home-field-repos.json').unlink()
        memories = '{:,} memories'.format(counts['objects'])
        self.until(lambda: self.text(memories), 300, 'the left column never said "' + memories + '"')
        drawn_s = round(time.monotonic() - self.relaunched, 1)
        time.sleep(8)  # the picture draws his records in
        first = self.measure('change')
        time.sleep(self.later)
        second = self.measure('change-later')
        return {'objects': counts['objects'], 'kept_after_s': read_s, 'column_after_s': drawn_s, 'column': memories,
                'measured': first, 'measured_later': second, 'later_s': self.later}

    def relaunch(self):
        relaunch(self.vm)
        time.sleep(8)
        self.to_home()
        column = self.memories()
        time.sleep(8)
        first = self.measure('relaunch')
        time.sleep(self.later)
        second = self.measure('relaunch-later')
        return {'memories_column_at_launch': column, 'measured': first, 'measured_later': second}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--alder', type=int, default=1800, help='documents in the first company')
    p.add_argument('--brightwater', type=int, default=900, help='documents in the second company')
    p.add_argument('--span', type=float, default=3, help='seconds per sample')
    p.add_argument('--samples', type=int, default=3, help='samples per measurement')
    p.add_argument('--later', type=float, default=30, help='seconds between a measurement and its repeat')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out, a.alder, a.brightwater, a.span, a.samples, a.later)
    walk.relaunched = time.monotonic()
    report = {'vm': a.vm, 'span_s': a.span, 'samples': a.samples, 'steps': {}}
    failed = False
    for step in steps:
        began = time.monotonic()
        try:
            report['steps'][step] = {'pass': True, 'result': getattr(walk, step)()}
        except (StepFailed, subprocess.TimeoutExpired, RuntimeError, KeyError, ValueError) as exc:
            report['steps'][step] = {'pass': False, 'why': str(exc)[-1500:]}
            failed = True
            try:
                walk.shot('failed-' + step)
            except Exception:
                pass
        report['steps'][step]['seconds'] = round(time.monotonic() - began, 1)
        print(step, 'PASS' if report['steps'][step]['pass'] else 'FAIL: ' + report['steps'][step]['why'][:400], flush=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        if not report['steps'][step]['pass']:
            break
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())

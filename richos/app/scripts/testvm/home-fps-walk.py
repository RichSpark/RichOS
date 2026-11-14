#!/usr/bin/env python3
"""home-fps-walk.py — the home screen's frame rate in the real app, before and after the memory count
changes, measured in the guest and never on the host's screen (CEO ruling 65).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --no-audio --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      home-fps-walk.py --out DIR [--alder 1800] [--brightwater 900] [--span 3] [--samples 3]

run-walk.py passes the owned VM name as the first argument. It refuses a guest booted with a sound
device: without --no-audio the guest's sound reaches this Mac's speakers (CEO ruling 53), and nothing
here needs sound.

WHY (round 75, richos-hq a4c16a665): in headless WebKit the loro picture ran at 17 to 26 frames a
second for the rest of a visit after any change of the memory count; the pass that colors every
thread, inside the engine's very long frame function, was sometimes left unoptimized. This walk asks
the same question of the app's own window.

HOW IT COUNTS: frame-rate.swift, compiled on the host and copied into the guest, streams the app's window with
ScreenCaptureKit and counts the PICTURE's frames: complete frames whose middle-of-the-picture region
changed, so the home screen's own CSS animations (the bokeh's drift, the Talk to Rich dot) are not
counted as the picture. The window's rate is kept beside it. Nothing is injected into the app. Each measurement is --samples spans of --span seconds, the first 4 s after the moment named.

THE STEPS, in order:
  example     a fresh user's home screen: the example, the count never changes; measured, then
              again --later seconds after that
  companies   two fictional companies' repositories and a team (home-standard-walk.py's), the app
              relaunched as a returning user's starts and the home screen put up at once and read
  during      the example while the repositories are read (the count has not changed yet)
  change      his records arrive while the screen is up: a look at the home screen without his count,
              then one with it, the same app process throughout, or the step fails (a count already
              on the screen at the first look is not a change); measured after it, then
              again --later seconds after that, --long samples in a row
  relaunch    the app relaunched: the same records drawn at launch, his count on the home screen at the
              first look and still at the end, one app process, or the step fails; the control for
              `change` (measured the same way)

Evidence: --out/report.json (every sample, every step's result) and a frame of the screen per
measurement. Exit 0 when every step ran; the rates are the result, not a pass mark. It quits nothing
itself: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
"""
import argparse
import importlib.util
import json
import os
import re
from pathlib import Path
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import reserve  # noqa: E402  (the admission's own CPU sampler)

_spec = importlib.util.spec_from_file_location('home_standard_walk', HERE / 'home-standard-walk.py')
standard = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(standard)
StepFailed = standard.StepFailed

STEPS = ['example', 'companies', 'during', 'change', 'relaunch']
SETTLE = 4  # seconds after the moment named before the first sample


class Walk(standard.Walk):
    def __init__(self, vm, out, alder, brightwater, span, samples, later, long):
        super().__init__(vm, out, alder, brightwater)
        self.span, self.samples, self.later, self.long = span, samples, later, long
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

    def webcontent_cpu_s(self):
        """Seconds of CPU the guest's web content processes have used so far (the picture's JavaScript
        and its drawing run there), summed over all of them; None when ps names none."""
        rows = guest(self.vm, 'ps -A -o time=,comm= | grep WebKit.WebContent || true')
        total, seen = 0.0, False
        for line in rows.splitlines():
            clock = line.split()[0] if line.split() else ''
            try:
                parts = [float(p) for p in clock.split(':')]
            except ValueError:
                continue
            seconds = 0.0
            for p in parts:
                seconds = seconds * 60 + p
            total, seen = total + seconds, True
        return total if seen else None

    def measure(self, name, samples=None):
        """`samples` (default --samples) spans of --span seconds of the app's window, and one frame of the
        screen. Beside the rates: the host's total CPU while they were taken, and the web content
        processes' CPU per picture frame. A frame function left unoptimized costs more CPU per frame
        whatever the host is doing; a busy host leaves it as it was and costs frames instead."""
        samples = samples or self.samples
        time.sleep(SETTLE)
        pid = self.pid()
        wait = int(self.span * samples + 60)
        cpu_before = self.webcontent_cpu_s()
        r = subprocess.run([str(HERE / 'guest.sh'), self.vm, shlex.quote(self.meter) + ' %s %s %d' % (pid, self.span, samples)],
                           capture_output=True, text=True, timeout=wait)
        cpu_after = self.webcontent_cpu_s()
        if r.returncode:
            raise StepFailed('the frame rate could not be measured (exit %d): ' % r.returncode + (r.stderr + r.stdout)[-1200:])
        row = json.loads(r.stdout.strip().splitlines()[-1])
        row['fps'] = [s['fps'] for s in row['samples']]  # the picture's own frames (frame-rate.swift)
        row['window_fps'] = [s['window_fps'] for s in row['samples']]
        row['any_change_fps'] = [round(s['any_change'] / s['seconds'], 1) for s in row['samples'] if 'any_change' in s]
        # the CPU is divided by the PICTURE's frames: the window's other frames (its CSS animations) are
        # composited without the page drawing anything, so they are not the picture's cost
        frames = sum(s['picture'] for s in row['samples'])
        if cpu_before is not None and cpu_after is not None and frames:
            row['webcontent_cpu_ms_per_picture_frame'] = round((cpu_after - cpu_before) * 1000 / frames, 1)
        row['at'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
        # the guests share this Mac's GPU and cores: a second guest running beside this one, and how
        # busy the Mac was, are part of the conditions a rate was measured under
        runs = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run'
        row['other_guests'] = sorted(p.name for p in runs.iterdir() if p.is_dir() and p.name != self.vm)
        row['host_load'] = [round(x, 1) for x in os.getloadavg()]
        try:
            row['host_cpu_busy_percent'] = round(reserve.cpu_busy_percent(), 1)
        except (BlockingIOError, OSError) as exc:
            row['host_cpu_busy_percent'] = 'unavailable: ' + str(exc)
        row['frame'] = self.shot('fps-' + name)
        print('  %s: picture %s frames a second (window %s; region changed at all %s); %s ms of web content CPU a picture frame; host CPU %s%% busy (other guests: %s)' % (
            name, ', '.join('%g' % f for f in row['fps']), ', '.join('%g' % f for f in row['window_fps']),
            ', '.join('%g' % f for f in row['any_change_fps']), row.get('webcontent_cpu_ms_per_picture_frame', '?'),
            row['host_cpu_busy_percent'], ', '.join(row['other_guests']) or 'none'), flush=True)
        return row

    def memories(self):
        """The number in the left column's "N MEMORIES" (the style sheet draws it in capitals and the
        accessibility tree gives it as drawn), or None while the column names no memories. One look:
        under load one accessibility call takes seconds, and a slow look misses the moment."""
        found = self.node('--value', 'MEMORIES', '--contains') or self.node('--title', 'MEMORIES', '--contains')
        text = (found or {}).get('value') or (found or {}).get('title') or ''
        m = re.search(r'([0-9][0-9,]*)\s+memories', text, re.I)
        return int(m.group(1).replace(',', '')) if m else None

    def reading(self):
        """One look at the home screen: when, the app's pid, whether the home screen is up, and the
        column's count of memories."""
        home = bool(self.node('--id', 'home-enter'))
        return {'t': round(time.monotonic() - self.relaunched, 1), 'pid': self.pid(), 'home': home,
                'memories': self.memories() if home else None}

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
        """home-standard-walk.py's two companies and team, and the app relaunched as a returning user's
        starts; the home screen is put up at once and read, so that his records are seen to ARRIVE in the
        running app."""
        began = time.monotonic()
        repos = self.home + '/Companies'
        for script in ('home-standard-repos.sh', 'seed-team.sh'):
            code, text = standard.run([HERE / 'guest.sh', self.vm, '--push', HERE / script, self.payload + '/'], 120)
            if code:
                raise StepFailed('could not copy ' + script + ' into the guest: ' + text[-300:])
        made = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/home-standard-repos.sh') + ' ' + shlex.quote(repos)
                     + ' %d %d' % (self.alder, self.brightwater), 600)
        registry = {'version': 1, 'entities': [
            {'id': 'alder', 'display_name': 'Alder Works', 'roots': [repos + '/alder']},
            {'id': 'brightwater', 'display_name': 'Brightwater Labs', 'roots': [repos + '/brightwater']},
        ]}
        data = self.home + '/' + standard.DATA
        guest(self.vm, 'mkdir -p ' + shlex.quote(data) + ' && printf %s ' + shlex.quote(json.dumps(registry)) + ' > ' + shlex.quote(data + '/entities.json'))
        team = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/seed-team.sh') + ' ' + shlex.quote(data) + ' 3')
        result = {'repositories': made.splitlines(), 'teammates': team.strip()}
        relaunch(self.vm)
        self.relaunched = time.monotonic()
        # looked at from the first second: an app that opens on the home screen shows the example before
        # his records arrive, and a look that waits for anything else first can miss that moment
        self.timeline = []
        while time.monotonic() - self.relaunched < 90:
            self.timeline.append(self.reading())
            if self.timeline[-1]['memories']:
                break
            time.sleep(1)
        if not self.timeline[-1]['home']:
            self.to_home()
            self.timeline.append(self.reading())
        result['written_after_s'] = round(self.relaunched - began, 1)
        result['readings'] = list(self.timeline)
        return result

    def during(self):
        """The example on the screen before his records arrive, if they have not yet."""
        if self.timeline[-1]['memories']:
            return {'skipped': 'his records were already on the screen at the first look', 'reading': self.timeline[-1]}
        row = self.measure('during', 3)
        self.timeline.append(self.reading())
        return {'measured': row, 'reading_after': self.timeline[-1]}

    def change(self):
        """His records arrive while the screen is up: a look without his count, then one with it, the same
        app process throughout. Only then are the rates measured as after a change."""
        path = self.until(self.kept, 600, 'the repositories were not read and kept')
        code, text = standard.run([HERE / 'guest.sh', self.vm, '--pull', path, self.out / 'home-field-repos.json'], 300)
        if code:
            raise StepFailed('could not pull the kept picture: ' + text[-300:])
        objects = json.loads((self.out / 'home-field-repos.json').read_text())['field']['meta']['counts']['objects']
        (self.out / 'home-field-repos.json').unlink()
        end = time.monotonic() + 300
        while time.monotonic() < end and self.timeline[-1]['memories'] != objects:
            time.sleep(1)
            self.timeline.append(self.reading())
        before = [r for r in self.timeline if r['home'] and r['memories'] != objects]
        after = [r for r in self.timeline if r['home'] and r['memories'] == objects]
        pids = {r['pid'] for r in self.timeline}
        if not before or not after or len(pids) != 1 or before[0]['t'] > after[0]['t']:
            raise StepFailed('no change of the count was seen in this visit (a look at the home screen without '
                             '%d memories, then one with them, one app process): %s' % (objects, json.dumps(self.timeline)))
        self.objects = objects
        time.sleep(8)  # the picture draws his records in
        first = self.measure('change')
        time.sleep(self.later)
        second = self.measure('change-later', self.long)
        return {'objects': objects, 'seen_without_s': before[-1]['t'], 'seen_with_s': after[0]['t'],
                'timeline': self.timeline, 'measured': first, 'measured_later': second, 'later_s': self.later}

    def relaunch(self):
        """The control: the same records drawn at launch. His count must be on the home screen at the first
        look that finds the home screen, the same app process to the end, or this is not a visit without
        a change and the step fails."""
        objects = self.objects
        relaunch(self.vm)
        self.relaunched = time.monotonic()
        readings = []
        while time.monotonic() - self.relaunched < 90:
            readings.append(self.reading())
            if readings[-1]['home']:
                break
            time.sleep(1)
        if not readings[-1]['home']:
            self.to_home()
            readings.append(self.reading())
        if readings[-1]['memories'] != objects:
            raise StepFailed('not a visit without a change: the first look at the home screen did not show %d memories: %s'
                             % (objects, json.dumps(readings)))
        time.sleep(8)
        first = self.measure('relaunch')
        time.sleep(self.later)
        second = self.measure('relaunch-later', self.long)
        readings.append(self.reading())
        if readings[-1]['memories'] != objects or len({r['pid'] for r in readings}) != 1:
            raise StepFailed('the count or the app process changed during the control visit: ' + json.dumps(readings))
        return {'objects': objects, 'readings': readings, 'measured': first, 'measured_later': second}

def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--alder', type=int, default=1800, help='documents in the first company')
    p.add_argument('--brightwater', type=int, default=900, help='documents in the second company')
    p.add_argument('--span', type=float, default=3, help='seconds per sample')
    p.add_argument('--samples', type=int, default=3, help='samples per measurement')
    p.add_argument('--later', type=float, default=30, help='seconds between a measurement and its repeat')
    p.add_argument('--long', type=int, default=15, help='samples in the repeat after the change and after the relaunch, '
                   'long enough to tell a slowdown that stays from one that passes')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
    try:
        audio = (state / 'audio').read_text().strip()
    except OSError:
        audio = 'unknown'
    if audio != 'off':
        p.error('the guest was booted with its sound device (%s), which plays on this Mac\'s speakers: '
                'run it with run-walk.py --no-audio' % audio)
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out, a.alder, a.brightwater, a.span, a.samples, a.later, a.long)
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

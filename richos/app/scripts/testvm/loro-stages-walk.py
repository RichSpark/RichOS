#!/usr/bin/env python3
"""loro-stages-walk.py — the home screen's stages (round 75) on the real app: a new user's first
launch, the handover at his first hire, and his picture at about 30, 100, 300 and 1,000 memories.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      loro-stages-walk.py --out DIR [--sizes 30,100,300,1000] [--steps fresh,hire,grow,replay]

run-walk.py passes the owned VM name as the first argument.

THE ORDER (CEO, 2026-11-14, on richos-hq round 75): "Yes, build it with the picked defaults." The
design is `richos-hq/design/mockups/rounds/round-75`; the app's part is `ui/home-growth.js`.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  fresh   a new user's home screen, nothing written: the finished example with its sentence and,
          under it, "Move through it. Click a light." (frame home-fresh)
  hire    one fictional company's repository of about the first size is written in the guest
          (home-standard-repos.sh), registered with a team of three (seed-team.sh), and the app is
          relaunched, as the launch after a first hire is: the example in full draining to its
          outline (frames handover-1 to handover-3, two seconds apart) and the sentence "That was
          the example…" with his first lights (frame home-first-hire)
  grow    for each size: the repository is written again at that size and the app is relaunched
          until the left column says that many memories (a changed repository is read behind the
          kept picture; the home screen asks again while it is up and draws the new lights
          arriving); once the stage's moment has played out (20 s), the frame home-<size> is
          taken. Each records what the screen says: the
          brand line, the distance line, and the stage's name where one was crossed.
  replay  on the last size's picture, "Watch it grow" pressed as a hand presses it: the button
          reads "Stop", the replay's line counts memories up from the first light (read from the
          accessibility tree about every second, frames replay-early and replay-late), the last
          frame reads "<n> memories · <stage>" (frame replay-last), and the replay ends by itself
          with the button reading "Watch it grow" again (added with the nightly 100 walk)

A repository is drawn as a few more objects than it has documents (its README, its decision
register, its working days), so each size is asked for as that many documents rounded up to a
multiple of eight, which lands inside the size's stage; the count on the screen is what is recorded.

Evidence (frames, every step's result) is written to --out/report.json. Exit 0 when every step
passes. It quits nothing itself: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
"""
import argparse
import importlib.util
import json
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
StepFailed, DATA = standard.StepFailed, standard.DATA

STEPS = ['fresh', 'hire', 'grow', 'replay']


class Walk(standard.Walk):
    def __init__(self, vm, out, sizes):
        super().__init__(vm, out, 0, 0)
        self.sizes = sizes
        self.repos = self.home + '/Companies'

    def window(self):
        """Is any of the app's window on the screen: the home screen's door or the app's rail?"""
        return self.node('--id', 'home-enter') or self.node('--id', 'nr-home')

    def launch(self, what):
        """Relaunch and bring the home screen up. A relaunch can leave the app running with no
        window (seen at the sizes on 2026-11-14, three launches in a row once); the app is then
        opened again as a click on its Dock icon opens it, which gives it its window on the
        conversation, and the home screen is reached from there. Records which way it came up."""
        how = []
        for attempt in (1, 2, 3):
            relaunch(self.vm)
            try:
                self.until(self.window, 40, 'no window')
                how.append('window')
            except StepFailed:
                app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
                guest(self.vm, 'open ' + shlex.quote(app))
                how.append('reopened')
                try:
                    self.until(self.window, 40, 'no window')
                except StepFailed:
                    continue
            try:
                self.to_home()
                return how
            except StepFailed:
                continue
        log = guest(self.vm, 'tail -n 15 ' + shlex.quote(self.payload + '/app.log') + ' 2>/dev/null || true')
        raise StepFailed('the home screen did not come up ' + what + ' in three launches (' + ', '.join(how) + '). app.log: ' + log[-900:])

    def said(self, element_id):
        """What the element with this id reads, or None."""
        node = self.node('--id', element_id)
        if not node:
            return None
        return ' '.join(str(node.get(k) or '') for k in ('value', 'title')).strip() or None

    @staticmethod
    def documents(size):
        return (size + 7) // 8 * 8

    def write_repository(self, size):
        documents = self.documents(size)
        return guest(self.vm, 'rm -rf ' + shlex.quote(self.repos + '/alder') + ' && sh ' + shlex.quote(self.payload + '/home-standard-repos.sh')
                     + ' ' + shlex.quote(self.repos) + ' --repo alder %d 0' % documents, 600).strip()

    def memories(self):
        """The number the left column gives ("loro · 34 memories"), or None."""
        node = self.text(' memories')
        words = ' '.join(str(node.get(k) or '') for k in ('value', 'title')) if node else ''
        digits = ''.join(c for c in words.split(' memories')[0].split('·')[-1] if c.isdigit())
        return int(digits) if digits else None

    def fresh(self):
        result = super().fresh()
        invitation = self.text('Move through it. Click a light.')
        if not invitation:
            raise StepFailed('the invitation is not under the first-run banner')
        result['invitation'] = self.rect(invitation)
        result['frame_reads_invitation'] = self.reads(result['frame'], 'Move through it')
        return result

    def hire(self):
        standard.run([HERE / 'guest.sh', self.vm, '--push', HERE / 'home-standard-repos.sh', self.payload + '/'], 120)
        standard.run([HERE / 'guest.sh', self.vm, '--push', HERE / 'seed-team.sh', self.payload + '/'], 120)
        made = self.write_repository(self.sizes[0])
        data = self.home + '/' + DATA
        registry = {'version': 1, 'entities': [{'id': 'alder', 'display_name': 'Alder Works', 'roots': [self.repos + '/alder']}]}
        guest(self.vm, 'mkdir -p ' + shlex.quote(data) + ' && printf %s ' + shlex.quote(json.dumps(registry)) + ' > ' + shlex.quote(data + '/entities.json'))
        team = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/seed-team.sh') + ' ' + shlex.quote(data) + ' 3')
        launches = self.launch('after the hire')
        frames = []
        for n in (1, 2, 3):
            frames.append(self.shot('handover-%d' % n))
            time.sleep(2)
        sentence = self.until(lambda: self.text('That was the example.'), 60, 'the handover\'s sentence never came')
        time.sleep(6)  # the rest of his lights arrive behind the first
        frame = self.shot('home-first-hire')
        return {'repository': made, 'launches': launches, 'teammates': team.strip(), 'handover_frames': frames, 'sentence': self.rect(sentence),
                'frame': frame, 'frame_reads_sentence': self.reads(frame, 'That was the example'),
                'legend_example': bool(self.text('Example, not yours')), 'memories': self.memories(), 'distance': self.said('home-distance')}

    def grow(self):
        rows = []
        for size in self.sizes:
            made = self.write_repository(size)
            # A repository that changed is read again BEHIND the kept picture and drawn at the next
            # launch (`home_repos.rs`, `field_kept_or_built`), so the first launch after the write
            # still shows the old count and starts the read; a later one draws the new picture.
            # The home screen also asks again while it is up (`home.js`, `refreshRecords`: 3 s, 8 s
            # and 20 s after an answer with nothing new), so a launch whose first answer is the
            # kept picture still draws the new lights arriving on that visit.
            want = self.documents(size)
            launches, count = [], None
            for _ in range(4):
                launches += self.launch('at %d' % size)
                try:
                    count = self.until(lambda: (self.memories() or 0) >= want and self.memories(), 50, 'not yet')
                    break
                except StepFailed:
                    continue
            if count is None:
                log = guest(self.vm, 'tail -n 25 ' + shlex.quote(self.payload + '/app.log') + ' 2>/dev/null || true')
                raise StepFailed('the left column never reached %d memories (%s). app.log: %s' % (want, ', '.join(launches), log[-900:]))
            # the sentence of the stage just crossed, while it is up (its first words: the same in every build)
            crossed = next((words for words in ('The lights are joined', 'The first territory is in its own color',
                                                'The river is running', 'Every territory is in color') if self.text(words)), None)
            time.sleep(20)  # a crossing and its look ahead have played out; the lights have arrived
            frame = self.shot('home-%d' % size)
            rows.append({'asked': size, 'launches': launches, 'repository': made, 'memories': self.memories() or count, 'distance': self.said('home-distance'),
                         'crossed': crossed, 'caption': self.said('home-caption'), 'watch_it_grow': bool(self.node('--id', 'home-grow')),
                         'see_it_finished': bool(self.node('--id', 'home-finished')), 'frame': frame})
        return rows

    def replay_line(self):
        """What the replay's line reads: the status element's own text, or, where the tree gives
        its words on its children, the texts drawn inside its rectangle."""
        node = self.node('--id', 'home-replay')
        if not node:
            return None
        own = ' '.join(str(node.get(k) or '') for k in ('value', 'title', 'desc')).strip()
        if own:
            return own
        r = self.rect(node)
        if not r or not r.get('w'):
            return None
        inside = [n for n in self.ax('find', '--value', 'memor', '--contains')
                  if r['x'] - 2 <= (n.get('x') or -1) <= r['x'] + r['w'] and r['y'] - 2 <= (n.get('y') or -1) <= r['y'] + r['h']]
        return ' '.join(str(n.get('value')) for n in inside).strip() or None

    def replay(self):
        """"Watch it grow" on the picture the last step left: pressed, it plays the picture from
        the first light to today and ends by itself (`home-growth.js`, `growPressed`)."""
        button = next((b for b in ('home-grow', 'home-grow-solo') if self.node('--id', b)), None)
        if not button:
            raise StepFailed('no "Watch it grow" button on the home screen')
        before = self.said(button)
        if not before or 'Watch it grow' not in before:
            raise StepFailed('the button reads %r, not "Watch it grow"' % before)
        r = self.rect(self.node('--id', button))
        point = '%d,%d' % (r['x'] + r['w'] / 2, r['y'] + r['h'] / 2)
        self.pointer(point, point)  # the real mouse, the same point twice: a click
        began = time.monotonic()
        lines, frames, stop_seen = [], [], False
        while time.monotonic() - began < 40:
            label = self.said(button) or ''
            stop_seen = stop_seen or 'Stop' in label
            line = self.replay_line()
            if line and (not lines or lines[-1][1] != line):
                lines.append((round(time.monotonic() - began, 1), line))
            if len(frames) == 0 and time.monotonic() - began > 3:
                frames.append(self.shot('replay-early'))
            elif len(frames) == 1 and time.monotonic() - began > 8:
                frames.append(self.shot('replay-late'))
            if line and ' · ' in line and 'memories ·' in line and len(frames) < 3:
                frames.append(self.shot('replay-last'))
            if stop_seen and 'Watch it grow' in label and time.monotonic() - began > 5:
                break
            time.sleep(0.5)
        after = self.said(button)
        counts = []
        for _, line in lines:
            words = line.split(' memor')[0].split('·')[-1]
            digits = ''.join(c for c in words if c.isdigit())
            if digits:
                counts.append(int(digits))
        result = {'button': button, 'stop_seen': stop_seen, 'lines': lines, 'counts': counts, 'frames': frames,
                  'after': after, 'seconds': round(time.monotonic() - began, 1)}
        if not stop_seen:
            raise StepFailed('the button never read "Stop" after the press: ' + json.dumps(result)[:900])
        if len(counts) < 3 or counts != sorted(counts) or counts[0] >= counts[-1]:
            raise StepFailed('the replay\'s count did not rise from the first light: ' + json.dumps(result)[:900])
        if not any('memories ·' in line for _, line in lines):
            raise StepFailed('the replay\'s last frame "<n> memories · <stage>" never showed: ' + json.dumps(result)[:900])
        if not after or 'Watch it grow' not in after:
            raise StepFailed('the replay did not end by itself within 40 s: ' + json.dumps(result)[:900])
        return result


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--sizes', default='30,100,300,1000', help='how many memories to show his picture at, in order')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out, [int(s) for s in a.sizes.split(',')])
    report = {'vm': a.vm, 'steps': {}}
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
                code, tree = standard.run([HERE / 'ax.sh', a.vm, 'tree'], 60)
                (a.out / ('failed-' + step + '.tree')).write_text(tree + '\n')
            except Exception:
                pass
        report['steps'][step]['seconds'] = round(time.monotonic() - began, 1)
        print(step, 'PASS' if report['steps'][step]['pass'] else 'FAIL: ' + report['steps'][step]['why'][:400], flush=True)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())

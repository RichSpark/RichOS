#!/usr/bin/env python3
"""home-standard-walk.py — the round 73 home screen on the real app: two connected companies drawn from
their own repositories, SwearMeter on and off, and Space opening Talk to Rich.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      home-standard-walk.py --out DIR [--alder 1800] [--brightwater 900] [--steps a,b,c]

run-walk.py passes the owned VM name as the first argument.

THE ORDER (CEO, 2026-11-13): round 73 is the home screen standard: the loro picture of the user's own
data for every connected company, the SwearMeter signal while SwearMeter is on, the SwearMeter button
on the navigation rail, and Enter or Space opening Talk to Rich.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  companies   two fictional companies' repositories are written in the guest (home-standard-repos.sh),
              registered as two companies in the app's own registry with a team of three
              (seed-team.sh), and the app is relaunched, as a returning user's app starts
  picture     the home screen once the repositories are read: the kept picture
              (home-field-repos.json) holds both companies and at least 2,500 objects, the left
              column says how many memories that is, and no example is on the screen
  swear-on    Settings, Show SwearMeter, pressed with the real mouse: the home column's last signal
              reads "swear words" and "in the last 30 days", above Talk to Rich
  space       the Space key on the home screen: Talk to Rich opens (the home screen leaves)
  card        in the app (the home screen has no rail): the SwearMeter button is on the rail above
              Bust a bug; the pointer rests on it and the card opens, reading "All time"
  swear-off   back on the home screen, the switch pressed again: the signal is gone; in the app
              again, the rail's button is gone

Evidence (frames, the kept picture's counts, every step's result) is written to --out/report.json.
Exit 0 when every step passes. It quits nothing itself: run-walk.py quits the app by its pid and
deletes the clone (CEO 54).
"""
import argparse
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
QA = HERE.parent / 'qa'
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

STEPS = ['companies', 'picture', 'swear-on', 'space', 'card', 'swear-off']
DATA = 'Library/Application Support/com.richos.app'


class StepFailed(Exception):
    pass


def run(args, timeout=120):
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True, timeout=timeout)
    return r.returncode, (r.stdout + r.stderr).strip()


class Walk:
    def __init__(self, vm, out, alder, brightwater):
        self.vm, self.out, self.alder, self.brightwater = vm, out, alder, brightwater
        self.payload = (Path.home() / '.richos-testvm' / 'run' / vm / 'payload').read_text().strip()
        self.home = self.payload + '/home'

    # ---- the guest's screen ---------------------------------------------------------------
    def ax(self, mode, *args):
        code, text = run([HERE / 'ax.sh', self.vm, mode, *args, '--json'], 60)
        return [n for n in (json.loads(s) for s in text.splitlines() if s.startswith('{')) if n.get('role') and not n.get('meta') and not n.get('error')]

    def node(self, *args):
        nodes = self.ax('find', *args, '--first')
        return nodes[0] if nodes else None

    def text(self, words):
        """The node that reads `words`. The column's labels are drawn in capitals by the style sheet
        and the accessibility tree gives them as drawn, so both spellings are asked for."""
        for spelled in (words, words.upper()):
            for by in ('--value', '--title'):
                found = self.node(by, spelled, '--contains')
                if found:
                    return found
        return None

    def rect(self, node):
        return None if node is None else {k: node.get(k) for k in ('x', 'y', 'w', 'h')}

    def until(self, check, seconds, what):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            got = check()
            if got:
                return got
            time.sleep(1)
        raise StepFailed(what + ' within ' + str(seconds) + ' s')

    def shot(self, name):
        code, text = run([HERE / 'shot.sh', self.vm, self.out / (name + '.png')], 120)
        if code:
            raise StepFailed('no frame ' + name + ': ' + text[-300:])
        return name + '.png'

    def reads(self, frame, pattern, box=None):
        """Whether the frame's pixels read `pattern` (OCR), in `box` enlarged 3x when given: what a
        person sees there, not what the accessibility tree says is in the page."""
        target = self.out / frame
        if box:
            crop = self.out / (Path(frame).stem + '-crop.png')
            x, y, w, h = [int(v) for v in (box['x'] - 8, box['y'] - 8, box['w'] + 16, box['h'] + 16)]
            code, text = run([QA / 'frame.py', 'crop', target, crop, x, y, w, h, '--scale', '3'], 60)
            if code:
                raise StepFailed('could not crop ' + frame + ': ' + text[-300:])
            target = crop
        code, text = run([QA / 'ocr-find.sh', pattern, target, '--quiet'], 120)
        if code not in (0, 1):
            raise StepFailed('ocr-find could not read ' + str(target) + ': ' + text[-300:])
        return code == 0

    def pointer(self, *points, hover=False):
        code, text = run([HERE / 'pointer-drag.sh', self.vm, *points] + (['--hover'] if hover else []), 60)
        if code:
            raise StepFailed('the pointer did not move: ' + text[-300:])

    def key(self, code):
        guest(self.vm, 'osascript -e ' + shlex.quote('tell application "System Events" to key code %d' % code))

    def decline_sheets(self):
        """A new user's sheets ("Not now", up to three) cover the app and keep its rail out of reach."""
        for _ in range(3):
            if self.ax('click', '--title', 'Not now', '--role', 'AXButton', '--first'):
                time.sleep(3)
            else:
                break

    def to_home(self):
        """Decline the first-run sheets, then the home screen: it is up when Talk to Rich is;
        otherwise the rail's Home button is pressed."""
        self.decline_sheets()
        for _ in range(6):
            if self.node('--id', 'home-enter'):
                return True
            self.ax('click', '--id', 'nr-home', '--first')
            time.sleep(3)
        raise StepFailed('the home screen did not come up')

    def switch(self):
        """Settings open, and the Show SwearMeter row clear of the panel's fixed footer."""
        if not self.node('--id', 'set-swearmeter'):
            self.ax('click', '--id', 'set-btn', '--first')
        self.until(lambda: self.node('--id', 'set-swearmeter'), 20, 'the Settings panel did not open')
        for _ in range(8):
            r = self.rect(self.node('--id', 'set-swearmeter'))
            if r and r['y'] + r['h'] < 770:
                return r
            self.pointer('520,182', '520,182')  # the panel's own title: takes focus, presses nothing
            self.key(121)                       # Page Down, as a hand scrolls
            time.sleep(1)
        raise StepFailed('Show SwearMeter never came clear of the footer')

    def press_switch(self):
        r = self.switch()
        # the switch is at the row's right end; the press lands 20 px in from it, with the real mouse
        point = '%d,%d' % (r['x'] + r['w'] - 20, r['y'] + r['h'] / 2)
        self.pointer(point, point)

    def close_settings(self):
        self.key(53)
        time.sleep(1.5)

    def signal(self):
        return self.text('in the last 30 days')

    # ---- the steps ------------------------------------------------------------------------
    def companies(self):
        repos = self.home + '/Companies'
        run([HERE / 'guest.sh', self.vm, '--push', HERE / 'home-standard-repos.sh', self.payload + '/'], 120)
        run([HERE / 'guest.sh', self.vm, '--push', HERE / 'seed-team.sh', self.payload + '/'], 120)
        made = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/home-standard-repos.sh') + ' ' + shlex.quote(repos) + ' %d %d' % (self.alder, self.brightwater), 600)
        registry = {'version': 1, 'entities': [
            {'id': 'alder', 'display_name': 'Alder Works', 'roots': [repos + '/alder']},
            {'id': 'brightwater', 'display_name': 'Brightwater Labs', 'roots': [repos + '/brightwater']},
        ]}
        data = self.home + '/' + DATA
        guest(self.vm, 'mkdir -p ' + shlex.quote(data) + ' && printf %s ' + shlex.quote(json.dumps(registry)) + ' > ' + shlex.quote(data + '/entities.json'))
        team = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/seed-team.sh') + ' ' + shlex.quote(data) + ' 3')
        relaunch(self.vm)
        time.sleep(8)
        self.to_home()
        return {'repositories': made.splitlines(), 'teammates': team.strip()}

    def kept(self):
        row = guest(self.vm, 'f="$(find ' + shlex.quote(self.home) + ' -name home-field-repos.json 2>/dev/null | head -1)"; [ -n "$f" ] && echo "$f" || true', 60)
        return row.strip() or None

    def picture(self):
        began = time.monotonic()
        path = self.until(self.kept, 300, 'the repositories were not read and kept')
        read_s = round(time.monotonic() - began, 1)
        code, text = run([HERE / 'guest.sh', self.vm, '--pull', path, self.out / 'home-field-repos.json'], 300)
        if code:
            raise StepFailed('could not pull the kept picture: ' + text[-300:])
        field = json.loads((self.out / 'home-field-repos.json').read_text())['field']
        (self.out / 'home-field-repos.json').unlink()  # the counts are kept in the report; the file is 2 MB
        counts = field['meta']['counts']
        domains = [(d['label'], d['nodeCount']) for d in field['domains']]
        if len(domains) != 2 or counts['objects'] < 2500:
            raise StepFailed('the kept picture is not two companies of 2,500 objects or more: ' + json.dumps(domains))
        memories = '{:,} memories'.format(counts['objects'])
        # his objects arrive after the picture is first up; the column says so once they are drawn
        self.until(lambda: self.text(memories), 120,
                   'the left column never said "' + memories + '"')
        time.sleep(12)  # the picture draws itself in
        frame = self.shot('home-two-companies')
        example = bool(self.text('Example, not yours'))
        if example:
            raise StepFailed('the example is still on the screen with ' + memories)
        return {'kept_after_s': read_s, 'companies': domains, 'objects': counts['objects'], 'links': counts['links'], 'sources': counts['sources'],
                'cross_company_links': counts['crossCompanyLinks'], 'unread_folders': field['meta']['unreadFolders'], 'column': memories, 'frame': frame,
                'frame_reads_memories': self.reads(frame, 'memories')}

    def swear_on(self):
        if self.signal():
            raise StepFailed('SwearMeter is on the home screen before it was switched on')
        self.press_switch()
        began = time.monotonic()
        sig = self.until(self.signal, 20, 'the home screen shows no SwearMeter signal after switching on')
        seconds = round(time.monotonic() - began, 1)
        self.close_settings()
        frame = self.shot('swear-on')
        sig = self.rect(self.signal())
        door = self.rect(self.node('--id', 'home-enter'))
        box = {'x': sig['x'], 'y': sig['y'] - 44, 'w': max(sig['w'], 230), 'h': sig['h'] + 44}
        problems = []
        if not self.reads(frame, 'swear words?', box):
            problems.append('the frame does not read "swear words" above "in the last 30 days"')
        if not self.reads(frame, 'last 30 days', box):
            problems.append('the frame does not read "in the last 30 days"')
        if not (door and sig['y'] + sig['h'] <= door['y']):
            problems.append('the signal is not above Talk to Rich')
        result = {'seconds_to_signal': seconds, 'signal': sig, 'talk_to_rich': door, 'frame': frame}
        if problems:
            raise StepFailed('; '.join(problems) + ': ' + json.dumps(result))
        return result

    def card(self):
        self.decline_sheets()
        b = self.rect(self.until(lambda: self.node('--id', 'nr-swear'), 15, 'the rail shows no SwearMeter button while SwearMeter is on'))
        bug = self.rect(self.node('--id', 'nr-bug'))
        if b['h'] < 30 or not (bug and b['y'] + b['h'] <= bug['y'] + 1):
            raise StepFailed('the SwearMeter button is not above Bust a bug: ' + json.dumps({'button': b, 'bust_a_bug': bug}))
        self.pointer('%d,%d' % (b['x'] + b['w'] / 2, b['y'] + b['h'] / 2), hover=True)
        card = self.until(lambda: self.node('--id', 'swear-card'), 10, 'resting the pointer on the SwearMeter button opened no card')
        time.sleep(1.5)
        frame = self.shot('swear-card')
        box = self.rect(card)
        reads = {word: self.reads(frame, word, box) for word in ('SwearMeter', 'Last 30 days', 'All time')}
        self.pointer('900,300', hover=True)
        gone = self.until(lambda: not self.node('--id', 'swear-card') or None, 10, 'the card stayed after the pointer left')
        if not all(reads.values()):
            raise StepFailed('the card does not read every heading: ' + json.dumps(reads))
        return {'button': b, 'bust_a_bug': bug, 'card': box, 'reads': reads, 'closed_when_the_pointer_left': bool(gone), 'frame': frame}

    def swear_off(self):
        self.to_home()
        self.until(self.signal, 15, 'back on the home screen the SwearMeter signal is not there')
        self.press_switch()
        self.until(lambda: not self.signal() or None, 15, 'the home screen still shows the SwearMeter signal after switching off')
        self.close_settings()
        frame = self.shot('swear-off')
        reads = self.reads(frame, 'swear words')
        self.ax('click', '--id', 'home-enter', '--first')
        self.until(lambda: not self.node('--id', 'home-enter') or None, 15, 'Talk to Rich did not open')
        self.decline_sheets()
        self.until(lambda: self.node('--id', 'nr-bug'), 15, 'the rail is not on the screen')
        rail = self.shot('swear-off-rail')
        # A button the page has hidden can stay in the accessibility tree for a while as a leftover
        # with no height of its own (walk 5: 51 x 10 where the button is 37 x 37), so the button is
        # on the rail only while it has a button's size.
        def off_the_rail():
            r = self.rect(self.node('--id', 'nr-swear'))
            return not r or r['h'] < 30 or None
        self.until(off_the_rail, 20, 'the rail still shows the SwearMeter button after switching off')
        return {'signal': None, 'button': self.rect(self.node('--id', 'nr-swear')), 'frame': frame, 'frame_reads_swear': reads, 'rail_frame': rail}

    def space(self):
        self.until(lambda: self.node('--id', 'home-enter'), 15, 'the home screen is not up')
        self.pointer('900,300', '900,300')  # a click on the picture: the app has the keyboard, nothing is focused
        time.sleep(1)
        if not self.node('--id', 'home-enter'):
            raise StepFailed('a click on the picture left the home screen')
        self.key(49)
        self.until(lambda: not self.node('--id', 'home-enter') or None, 15, 'Space did not open Talk to Rich')
        time.sleep(2)
        return {'opened_talk_to_rich': True, 'frame': self.shot('space-opened')}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--alder', type=int, default=1800, help='documents in the first company')
    p.add_argument('--brightwater', type=int, default=900, help='documents in the second company')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out, a.alder, a.brightwater)
    report = {'vm': a.vm, 'steps': {}}
    failed = False
    for step in steps:
        began = time.monotonic()
        try:
            report['steps'][step] = {'pass': True, 'result': getattr(walk, step.replace('-', '_'))()}
        except (StepFailed, subprocess.TimeoutExpired, RuntimeError, KeyError, ValueError) as exc:
            report['steps'][step] = {'pass': False, 'why': str(exc)[-1500:]}
            failed = True
            try:
                walk.shot('failed-' + step)
                code, tree = run([HERE / 'ax.sh', a.vm, 'tree'], 60)
                (a.out / ('failed-' + step + '.tree')).write_text(tree + '\n')
            except Exception:
                pass
        report['steps'][step]['seconds'] = round(time.monotonic() - began, 1)
        print(step, 'PASS' if report['steps'][step]['pass'] else 'FAIL: ' + report['steps'][step]['why'][:400], flush=True)
        if failed:
            break
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())

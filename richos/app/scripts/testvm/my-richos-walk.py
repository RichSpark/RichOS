#!/usr/bin/env python3
"""my-richos-walk.py - My RichOS (round 109) on the real app in a guest: off runs nothing, on shows
token use from a real turn and every suite app's memory, the card and both details in dark and
light, and the app's CPU and memory off, on and idle, and on during a turn.

  richos/app/scripts/testvm/run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TAR \\
      --report REPORT.json --wait 600 -- \\
      richos/app/scripts/testvm/my-richos-walk.py --out OUT_DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument, quits the app by its pid and deletes
the clone when this returns (CEO 54). Nothing is on the host's screen (CEO ruling 65) and the guest
plays no sound (run-walk.py's default: tart's null speaker). The bundle is this branch's
(branch-bundle.sh); the engine is any release's (it is not what is proven here).

THE QUESTION (CEO, 2026-11-26): "I need a way to track or see at a glance 1) whether or not tokens
are being consumed now 2) the velocity with which they've been being consumed in the last say 1
minute and last 5 minutes and last 15 minutes ... 4) also see how much memory/RAM is being used now
by each app of the RichOS suite individuall and all combined", "off by default", and on the build:
"I really hope that those graphs can be drawn without using much CPU/RAM resources."

THE STEPS, in one guest:
  identity   the boot log's "this app: built from" is --expect-sha
  setup      conversation-walk.py's first run: the company and its first conversation
  off-idle   the switch is off (as it ships): no My RichOS square in the window; the app's CPU and
             memory over --idle seconds with nothing happening
  off-turn   one real turn (--prompt) with the switch off, measured from the send to the reply
  on         Settings > Show My RichOS switched on: the square is in the rail, under My Mac
  on-idle    the same idle measurement with the switch on (the 1-second timer and the 2-second
             memory reader running, the card closed)
  on-turn    the same turn with the switch on, measured the same way; then, while the rail still
             says tokens are being used: rail-dark.png, the square's own words, and the card
             opened by the pointer (card-dark.png) with its figures read from the window
  details    the token half of the card: details-tokens-dark.png; Memory: details-memory-dark.png
  idle       the rail says Idle within 25 s of the reply (ACTIVE_MS is 10 s after the last request)
  light      Settings > Light theme, then the rail, the card and both details again (*-light.png)
  off        the switch off again: the square leaves the rail
  quit       the app is quit by its recorded pid and no richos-tauri process is left in the guest

THE MEASUREMENT is the guest's own `ps`: the CPU time and resident memory of the app's process
(richos-tauri) and of every WebKit process (the web view runs the page's timer and paints the
canvases, so it is counted; the guest runs no other WebKit app). CPU is the CPU time used over the
wall time, in percent of one core.

Evidence in --out: report.json, the pictures, and what the turns left (ledger.jsonl, app.log,
native-projects/). Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

STEPS = ['identity', 'setup', 'off-idle', 'off-turn', 'on', 'on-idle', 'on-turn', 'details', 'idle', 'light', 'off', 'quit']
PROMPT = 'Write a plain paragraph of about 250 words on the history of tea. No tools, no headings.'

# The guest's `ps`, once: every richos-tauri and WebKit process with its CPU time (seconds) and
# resident memory (KB). Run in the guest by python3 -c.
SNAP = r'''
import json, subprocess, time
def secs(t):
    d = 0
    if "-" in t:
        d, t = t.split("-"); d = int(d)
    s = 0.0
    for part in t.split(":"):
        s = s * 60 + float(part)
    return d * 86400 + s
rows = {}
out = subprocess.run(["ps", "-A", "-o", "pid=,time=,rss=,command="], capture_output=True, text=True).stdout
for line in out.splitlines():
    parts = line.split(None, 3)
    if len(parts) < 4:
        continue
    exe = parts[3].split(" ")[0]
    name = exe.rsplit("/", 1)[-1]
    if name == "richos-tauri" or name.startswith("com.apple.WebKit."):
        rows[parts[0]] = [secs(parts[1]), int(parts[2]), name]
print(json.dumps({"at": time.time(), "rows": rows}))
'''


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def usage(before, after):
    """CPU (percent of one core) and resident memory (MB) between two snapshots."""
    wall = after['at'] - before['at']
    cpu = sum(t - before['rows'][p][0] for p, (t, _, _) in after['rows'].items() if p in before['rows'])
    by = {}
    for p, (t, rss, name) in after['rows'].items():
        key = 'richos-tauri' if name == 'richos-tauri' else 'webkit'
        slot = by.setdefault(key, {'cpu_s': 0.0, 'rss_mb': 0.0})
        slot['rss_mb'] += rss / 1024
        if p in before['rows']:
            slot['cpu_s'] += t - before['rows'][p][0]
    for slot in by.values():
        slot['cpu_percent'] = round(100 * slot.pop('cpu_s') / wall, 2)
        slot['rss_mb'] = round(slot['rss_mb'], 1)
    return {'seconds': round(wall, 1), 'cpu_percent_of_one_core': round(100 * cpu / wall, 2),
            'rss_mb': round(sum(r[1] for r in after['rows'].values()) / 1024, 1), 'by_process': by}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--out', type=Path)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--company', default='Acme')
    p.add_argument('--within', type=float, default=300, help='seconds for a reply (default 300)')
    p.add_argument('--idle', type=float, default=30, help='seconds of each idle measurement (default 30)')
    p.add_argument('--prompt', default=PROMPT)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    if not (a.vm and a.out):
        p.error('the VM name (run-walk.py gives it) and --out are required')
    conversation_walk = load('conversation_walk', HERE / 'conversation-walk.py')
    adopt_walk = conversation_walk.adopt_walk
    relaunch = load('relaunch', HERE / 'relaunch.py')
    StepFailed = adopt_walk.StepFailed
    guest = adopt_walk.guest

    class Walk(conversation_walk.ConversationWalk):
        def __init__(self, a):
            a.conversation, a.memory = None, False
            a.press_grace = 20
            conversation_walk.steps_of = lambda _path: []  # this walk gives its own messages
            super().__init__(a)
            self.thread = None
            self.report = {}
            self.turns = 0

        # --- reading the guest ---------------------------------------------------------------
        def snap(self):
            return json.loads(guest(self.vm, 'python3 -c ' + repr_sh(SNAP), 60))

        def measure_idle(self):
            before = self.snap()
            time.sleep(self.a.idle)
            return usage(before, self.snap())

        def node(self, dom_id):
            found = [n for n in self.ax('find', '--id', dom_id, '--first') if n.get('role')]
            if not found:
                raise StepFailed('#%s is not on screen' % dom_id)
            return found[0]

        def present_id(self, dom_id):
            try:
                return bool(self.node(dom_id))
            except StepFailed:
                return False

        def until(self, what, test, seconds, every=1):
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

        def hover(self, x, y):
            adopt_walk.command([HERE / 'pointer-drag.sh', self.vm, '%d,%d' % (round(x), round(y)), '--hover', '--rest', '0.4'], 60)

        def escape(self):
            adopt_walk.command([HERE / 'ax.sh', self.vm, '--key', '53'], 60)
            time.sleep(0.6)

        def square(self):
            """The square's own words (its accessible name): 'My RichOS: using tokens, ...'."""
            n = self.node('rom-square')
            return ' '.join(str(n.get(k) or '') for k in ('title', 'desc')).strip()

        def seen(self, name, *phrases):
            """A picture of the screen as `name`, read by OCR (qa/ocr-read.py): its text when every
            phrase is on it, else None. The card's and the details' figures sit inside buttons
            named by their own aria-label, so the accessibility tree does not carry them."""
            self.shot(name)
            r = subprocess.run([sys.executable, str(HERE.parent / 'qa' / 'ocr-read.py'), '--fresh', str(self.out / name)],
                               capture_output=True, text=True, timeout=120)
            if r.returncode:
                raise StepFailed('OCR could not read %s: %s' % (name, (r.stderr or r.stdout).strip()[-200:]))
            text = ' '.join(r.stdout.split())
            return text if all(p in text for p in phrases) else None

        def say(self, text):
            self.turns += 1
            step = {'kind': 'say', 'text': text, 'expect_reply': True}
            record = self.run_step(self.turns, step)
            failure = conversation_walk.verdict(step, record)
            if failure:
                raise StepFailed('%r got no reply: %s' % (text, failure))
            return record

        def settings_switch(self, on):
            self.ax('click', '--id', 'set-btn', '--first')
            self.until('the Show My RichOS switch', lambda: self.present_id('set-richosmon'), 30)
            value = str(self.node('set-richosmon').get('value'))
            if (value in ('1', 'True', 'true')) != on:
                self.ax('click', '--id', 'set-richosmon', '--first')
            self.escape()

        # --- steps ---------------------------------------------------------------------------
        def step_identity(self):
            line = guest(self.vm, 'grep -m1 "this app: built from" ' + repr_sh(self.payload + '/app.log'), 60)
            built = line.rsplit(' ', 1)[-1]
            if not built.startswith(self.a.expect_sha[:12]):
                raise StepFailed('the running app was built from %s, not %s' % (built, self.a.expect_sha))
            return {'built_from': built}

        def step_setup(self):
            made = self.setup()
            self.thread = made['thread']
            return made

        def step_off_idle(self):
            if self.present_id('rom-square'):
                raise StepFailed('the My RichOS square is in the window before the switch was turned on')
            return {'square_in_window': False, **self.measure_idle()}

        def turn(self):
            before = self.snap()
            record = self.say(self.a.prompt)
            after = self.snap()
            return {'reply_chars': len(record.get('reply', '')), 'turn_seconds': record.get('seconds'), **usage(before, after)}

        def step_off_turn(self):
            return self.turn()

        def set_theme(self, theme):
            """Settings > Dark theme or Light theme (effort-hover-walk.py's way), then Settings closed."""
            self.ax('click', '--id', 'set-btn', '--first')
            self.until('Settings', lambda: self.present_id('set-richosmon'), 30)
            self.ax('click', '--title', '%s theme' % theme.capitalize(), '--first')
            time.sleep(1.5)
            self.escape()
            self.hover(5, 5)

        def step_on(self):
            self.set_theme('dark')
            self.settings_switch(True)
            self.until('the My RichOS square in the rail', lambda: self.present_id('rom-square'), 15)
            mac, rom, me = self.node('res-square'), self.node('rom-square'), self.node('nr-me')
            if not (rom['y'] >= mac['y'] + mac['h'] - 1 and rom['y'] + rom['h'] <= me['y'] + 1):
                raise StepFailed('the square is not between My Mac and the name circle: %s' % json.dumps([mac, rom, me]))
            return {'square': self.square()}

        def step_on_idle(self):
            return self.measure_idle()

        def step_on_turn(self):
            fact = self.turn()
            said = self.until('the rail saying tokens are being used', lambda: 'using tokens' in self.square() and self.square(), 8)
            self.hover(5, 5)
            fact['rail_dark'] = self.shot_name('rail-dark.png')
            fact['square_said'] = said
            fact.update(self.card('dark'))
            return fact

        def shot_name(self, name):
            self.shot(name)
            return name

        def card(self, theme):
            """The card opened by the pointer resting on the square, as a hand does."""
            rom = self.node('rom-square')
            self.hover(rom['x'] + rom['w'] / 2, rom['y'] + rom['h'] / 2)
            name = 'card-%s.png' % theme
            said = self.until('the card with its three windows and memory',
                              lambda: self.seen(name, 'My RichOS', 'Last 15 minutes', 'Memory', '2-minute average'), 20, every=2)
            return {'card_' + theme: name, 'card_said_' + theme: said}

        def details(self, theme):
            # Pinned by a press on the square (it stays open), then its token half.
            self.hover(5, 5)
            self.ax('click', '--id', 'rom-square', '--first')
            self.until('the pinned card', lambda: self.present_id('rom-card'), 15)
            self.ax('click', '--title', 'Tokens, open the details', '--first')
            self.until('the details', lambda: self.present_id('rom-close'), 15)
            tokens = 'details-tokens-%s.png' % theme
            tokens_said = self.until('the Tokens details', lambda: self.seen(tokens, 'Tokens used', 'Where they went', 'By model', 'By kind'), 20, every=2)
            self.ax('click', '--title', 'Memory', '--first')
            memory = 'details-memory-%s.png' % theme
            memory_said = self.until('the Memory details', lambda: self.seen(memory, 'Each app', '2-minute average', 'RichOS'), 20, every=2)
            self.escape()
            if self.present_id('rom-close'):
                raise StepFailed('Escape left the details open')
            return {'tokens': tokens, 'tokens_said': tokens_said, 'memory': memory, 'memory_said': memory_said}

        def step_details(self):
            return self.details('dark')

        def step_idle(self):
            said = self.until('the rail saying Idle', lambda: 'idle' in self.square() and self.square(), 25)
            return {'square_said': said}

        def step_light(self):
            self.set_theme('light')
            fact ={'rail_light': self.shot_name('rail-light.png'), 'square_said': self.square()}
            fact.update(self.card('light'))
            fact.update(self.details('light'))
            return fact

        def step_off(self):
            self.settings_switch(False)
            self.until('the square leaving the rail', lambda: not self.present_id('rom-square'), 15)
            return {'square_in_window': False}

        def step_quit(self):
            relaunch.quit_app(self.vm)
            left = guest(self.vm, 'pgrep -x richos-tauri || true', 60).strip()
            if left:
                raise StepFailed('richos-tauri is still running in the guest after the quit: %s' % left)
            return {'richos_tauri_processes_left': 0}

    walk = Walk(a)
    ok = True
    steps = [s for s in a.steps.split(',') if s]
    try:
        for name in steps:
            began = time.monotonic()
            try:
                fact = getattr(walk, 'step_' + name.replace('-', '_'))()
                walk.report[name] = {'pass': True, 'seconds': round(time.monotonic() - began, 1), 'fact': fact}
                print('%s PASS %s' % (name, json.dumps(fact)[:900]), flush=True)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired, KeyError, ValueError) as exc:
                walk.report[name] = {'pass': False, 'seconds': round(time.monotonic() - began, 1), 'failure': str(exc)[:2000]}
                print('%s FAIL %s' % (name, str(exc)[:1200]), flush=True)
                ok = False
                try:
                    walk.shot('failed-%s.png' % name)
                except Exception:  # noqa: BLE001 -- the evidence is best effort, the verdict is not
                    pass
                break
            finally:
                (a.out / 'report.json').write_text(json.dumps(walk.report, indent=2) + '\n')
    finally:
        try:
            walk.pull()
        except Exception as exc:  # noqa: BLE001 -- the evidence is best effort, the verdict is not
            print('pull failed: %s' % str(exc)[:300], file=sys.stderr)
        (a.out / 'report.json').write_text(json.dumps(walk.report, indent=2) + '\n')
    return 0 if ok else 1


def repr_sh(text):
    import shlex
    return shlex.quote(text)


if __name__ == '__main__':
    sys.exit(main())

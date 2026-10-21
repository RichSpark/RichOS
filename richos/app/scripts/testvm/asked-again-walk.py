#!/usr/bin/env python3
"""asked-again-walk.py — a correction whose earlier answer cannot be read is asked again, on screen (CEO ruling §96).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      asked-again-walk.py --out DIR --expect-sha SHA [--also-sha SHA]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset, still as
one run under run-walk.py.

THE QUESTION. His answer, 2026-09-28 (§96): asked "If the file holding your past answers gets
corrupted, RichOS can't tell whether you already answered. Should it ask you again, or stay silent
about that one?", he said "Ask me again". The desk (`correction.rs`) now puts such a correction back
in front of him as an ordinary question, and the card LEADS with the desk's own sentence
(`main.js` `renderProposalCard`). This walk puts exactly that state on the real app's screen:

  identity    the running app says it was built from --expect-sha (or --also-sha)
  launch      the app is past the splash: a first-run sheet or the rail is on screen
  first-run   memory set up ("Set it up", the desk is installed), company Acme added with a folder
              that is a repository, the business questions declined
  themes      Settings -> Dark theme, then Light theme: one frame of the main window in each
  seed-desk   two proposals for this launch's entity appended to the desk log, with a TORN answer
              line between them: the first one's answer is unreadable for good, the second is new
  relaunch    the app is quit by its recorded pid and started again (relaunch.py), so the desk
              replays the log; the HOME screen must come up past the splash (read by OCR: its
              canvas starves accessibility reads), the boot line must carry the desk's health
              sentence, and Enter (the home screen's own hint) opens the conversation view
  card        Corrections opened: the first card leads with the sentence, verbatim, above its
              target line, the second card carries none; one frame per theme
  answer      "Not now" on the asked-again card: the card goes, and the desk log records a
              decline for THAT proposal and no other

Evidence (frames, accessibility reads, the desk log, the boot log) is pulled to --out. Nothing is
quit here: run-walk.py stops the app by its pid, stops the guest and deletes the clone (CEO §54).
Exit 0 when every step passes.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
QA = HERE.parent / 'qa'
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

STEPS = ['identity', 'launch', 'first-run', 'themes', 'seed-desk', 'relaunch', 'card', 'answer']
# `correction.rs` `asked_again_sentence(1)`, verbatim. The walk compares what is ON SCREEN with it.
SENTENCE = ('RichOS could not read your earlier answer to this correction, so it is asking you again. '
            'You may already have answered it.')
HEALTH_HEADLINE = 'A correction you may have already answered is being asked again.'
REF_ASKED = 'rec:ceo/records/qa-walk-asked-again'
REF_NEW = 'rec:ceo/records/qa-walk-ordinary'


class StepFailed(Exception):
    pass


def command(argv, timeout=30):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


def proposal_numbers(log_text):
    """Every `prop-<n>` number already in the desk log, so the seeded ids never collide."""
    return [int(n) for n in re.findall(r'"prop-(\d+)"', log_text)]


def entity_of(config):
    """The entity this launch is bound to: `config.json`'s `entity`, a non-empty string, or None."""
    value = config.get('entity') if isinstance(config, dict) else None
    return value if isinstance(value, str) and value.strip() else None


def desk_lines(entity, first, at_ms):
    """The three lines appended to `loro-corrections.jsonl`, in order.

    1. `prop-<first>` proposed, awaiting him — the one whose answer becomes unreadable;
    2. a TORN answer line: the start of a `declined` record for it, cut off mid-number. It is not
       JSON, so the desk classifies it as damaged, and damaged bytes are unreadable for good —
       exactly §96's case, and the one `correction.rs`'s own torn-line test uses;
    3. `prop-<first+1>` proposed AFTER the tear, so nothing below it can have been its answer: the
       ordinary card, the control that must carry no sentence.
    """
    if not entity:
        raise ValueError('no entity: the proposals would be scoped to nobody and never shown')
    if first < 1:
        raise ValueError('proposal numbers start at 1')

    def proposal(n, ref, why, at):
        return {'rec': 'proposed', 'id': f'prop-{n}', 'at': at, 'entity_id': entity, 'thread_id': '',
                'write': {'op': 'correct', 'recordRef': ref, 'title': 'QA walk title', 'kind': None,
                          'confidence': None, 'tags': None, 'narrowScopeTo': None, 'body': None},
                'why': why, 'preview': '---\ntitle: QA walk title\n---\n', 'state': 'awaiting-ceo',
                'outcome': None, 'failure': None}

    asked = json.dumps(proposal(first, REF_ASKED, 'QA walk: his earlier answer to this one is unreadable', at_ms))
    whole = json.dumps({'rec': 'declined', 'id': f'prop-{first}', 'at': at_ms + 1, 'permanent': False})
    torn = whole[:whole.index('"at"') + 8]
    ordinary = json.dumps(proposal(first + 1, REF_NEW, 'QA walk: an ordinary new correction', at_ms + 2))
    for line in (asked, ordinary):
        json.loads(line)
    try:
        json.loads(torn)
    except json.JSONDecodeError:
        pass
    else:
        raise AssertionError('the torn line parsed, so it is not damage')
    return [asked, torn, ordinary]


def declined_ids(log_text):
    """Proposal ids the desk log records a readable decline for."""
    out = []
    for line in log_text.splitlines():
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(rec, dict) and rec.get('rec') == 'declined':
            out.append(rec.get('id'))
    return out


class Walk:
    def __init__(self, a):
        self.a = a
        self.vm = a.vm
        self.out = a.out
        self.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.payload = (state / 'payload').read_text().strip()
        self.home = self.payload + '/home'
        self.data = self.home + '/Library/Application Support/com.richos.app'
        self.desk = self.data + '/loro-corrections.jsonl'
        self.company = self.home + '/Acme'
        self.boot_log = self.payload + '/app.log'
        self.facts = {}

    def save(self):
        (self.out / 'facts.json').write_text(json.dumps(self.facts, indent=2) + '\n')

    # --- the guest's screen -----------------------------------------------------------------
    def ax(self, mode, *args, timeout=40):
        argv = [HERE / 'ax.sh', self.vm, mode, *args, '--json']
        return [json.loads(s) for s in command(argv, timeout).splitlines() if s.startswith('{')]

    def nodes(self, mode, *args):
        return [n for n in self.ax(mode, *args) if not n.get('meta')]

    def find(self, *args):
        try:
            return self.nodes('find', *args)
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return []
            raise

    def press(self, title, role='AXButton', *extra):
        return self.ax('click', '--title', title, '--role', role, '--contains', '--first', *extra)

    def present(self, title, role='AXButton'):
        return bool(self.find('--title', title, '--role', role, '--contains', '--first'))

    def text_present(self, text):
        return bool(self.find('--value', text, '--contains', '--first'))

    def wait_for(self, check, what, seconds=30):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if check():
                return True
            time.sleep(1)
        self.tree(f'timeout-{re.sub(r"[^a-z0-9]+", "-", what.lower())[:40]}')
        raise StepFailed(f'{what} never happened within {seconds} s')

    def tree(self, name):
        try:
            text = command([HERE / 'ax.sh', self.vm, 'tree', '--max', '400'], 60)
        except StepFailed as exc:
            text = str(exc)
        (self.out / f'{name}.ax.txt').write_text(text)

    def shot(self, name, tree=True):
        command([HERE / 'shot.sh', self.vm, self.out / f'{name}.png'], 60)
        # The home screen's live canvas keeps an accessibility read past its 20 s deadline
        # (measured on .32), so a frame of it is taken without one.
        if tree:
            self.tree(name)
        return f'{name}.png'

    def frame_says(self, name, text):
        """True when OCR reads `text` in the frame just taken (qa/ocr-find.sh: 0 hit, 1 none)."""
        r = subprocess.run([str(QA / 'ocr-find.sh'), text, str(self.out / f'{name}.png'), '--first', '--quiet'],
                           capture_output=True, text=True, timeout=60)
        if r.returncode not in (0, 1):
            raise StepFailed('ocr-find could not read the frame: ' + r.stderr)
        return r.returncode == 0

    def pull(self, remote, name):
        command([HERE / 'guest.sh', self.vm, '--pull', remote, self.out / name], 60)

    def set_theme(self, label):
        self.press('Settings', 'AXPopUpButton')
        # The theme options carry aria-pressed, so WebKit exposes them as toggles rather than
        # AXButton (measured on .32: `--role AXButton` matched nothing). Matched by label alone.
        self.ax('click', '--title', label, '--first')
        # The menu closes on Escape, sent to the app's own pid (ax.sh refuses otherwise); if the
        # app is not frontmost, the Settings button closes it the way a person would.
        try:
            command([HERE / 'ax.sh', self.vm, '--key', '53'])
        except StepFailed:
            self.press('Settings', 'AXPopUpButton')
        time.sleep(1.5)

    # --- steps --------------------------------------------------------------------------------
    def identity(self):
        line = guest(self.vm, 'grep -m1 "this app: built from" ' + shlex.quote(self.boot_log))
        built = line.rsplit(' ', 1)[-1]
        allowed = [s for s in (self.a.expect_sha, self.a.also_sha) if s]
        if not any(built.startswith(s) for s in allowed):
            raise StepFailed(f'the running app was built from {built}, not {" or ".join(allowed)}')
        self.facts['built_from'] = built
        version = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d '
                        '-exec /usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" {}/Contents/Info.plist \\;')
        self.facts['bundle_version'] = version
        self.save()
        return {'built_from': built, 'bundle_version': version}

    def launch(self):
        began = time.monotonic()
        self.shot('01-first-frame')

        def past_splash():
            return (self.present('Set it up') or self.present('Not now') or self.present('Corrections', 'AXPopUpButton')
                    or self.present('Add this company'))
        self.wait_for(past_splash, 'a first-run sheet or the rail', 90)
        waited = round(time.monotonic() - began, 1)
        frame = self.shot('02-past-splash')
        return {'past_splash_within_s': waited, 'frame': frame}

    def first_run(self):
        # The engine sheet should not appear (run.sh supplies the pinned engine); if it does, it is
        # recorded and declined, since this walk is about the memory the desk needs.
        if self.text_present("There's one thing I need on this Mac"):
            self.facts['engine_sheet'] = True
            self.shot('03a-engine-sheet')
            self.press('Not now')
        self.wait_for(lambda: self.text_present('Where should I keep what you tell me?'), 'the memory sheet', 40)
        self.shot('03-memory-sheet')
        self.ax('click', '--title', 'Set it up', '--role', 'AXButton', '--in', 'dialog', '--first')
        # A yes that works closes the sheet with no second box (product hunt 2026-10-15,
        # finding 15), so the walk waits for the question to go, not for a box to close.
        self.wait_for(lambda: not self.text_present('Where should I keep what you tell me?'), 'the memory sheet to close', 60)
        self.shot('04-memory-done')
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        self.wait_for(lambda: self.present('Add this company'), 'the company question', 40)
        command([HERE / 'ax.sh', self.vm, 'set the clipboard to "seed"'])
        self.ax('type', 'Acme', '--role', 'AXTextField', '--title', "What's the company called?", '--contains', '--first', '--replace')
        self.ax('type', self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac', '--contains', '--first', '--replace')
        self.press('Add this company')
        time.sleep(3)
        if self.present('Start the questions'):
            self.press('Not now')
        config = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/config.json')))
        entity = entity_of(config)
        if not entity:
            (self.out / 'config.json').write_text(json.dumps(config, indent=2))
            raise StepFailed('config.json names no entity after the company was added')
        self.facts['entity'] = entity
        desk = guest(self.vm, 'test -f ' + shlex.quote(self.desk) + ' && echo yes || echo no')
        self.facts['desk_log_after_setup'] = desk
        available = guest(self.vm, 'grep -c "loro correction desk: CLOSED" ' + shlex.quote(self.boot_log) + ' || true')
        self.save()
        frame = self.shot('05-home')
        return {'entity': entity, 'desk_log_exists': desk, 'boot_said_desk_closed': available, 'frame': frame}

    def themes(self):
        # Ends on Light, so the relaunch below also shows what the home screen does under a
        # light preference (§15 clamps the home screen dark; a light home would be the defect).
        self.set_theme('Dark theme')
        dark = self.shot('06-conversation-dark')
        self.set_theme('Light theme')
        light = self.shot('07-conversation-light')
        return {'dark': dark, 'light': light}

    def seed_desk(self):
        entity = self.facts.get('entity')
        if not entity:
            raise StepFailed('first-run must have run: no entity on record')
        existing = guest(self.vm, 'cat ' + shlex.quote(self.desk) + ' 2>/dev/null || true', 30)
        first = max(proposal_numbers(existing) + [0]) + 70
        at_ms = int(float(guest(self.vm, "python3 -c 'import time; print(time.time()*1000)'")))
        lines = desk_lines(entity, first, at_ms)
        payload = ''.join(line + '\n' for line in lines)
        # Appended, never rewritten: whatever the desk wrote itself stays exactly where it was.
        r = subprocess.run([str(HERE / 'guest.sh'), self.vm, 'cat >> ' + shlex.quote(self.desk)],
                           input=payload, capture_output=True, text=True, timeout=30)
        if r.returncode:
            raise StepFailed('could not append to the desk log: ' + r.stderr)
        self.facts['asked_id'] = f'prop-{first}'
        self.facts['ordinary_id'] = f'prop-{first + 1}'
        self.save()
        self.pull(self.desk, 'desk-log-seeded.jsonl')
        return {'asked': self.facts['asked_id'], 'ordinary': self.facts['ordinary_id'], 'lines': len(lines)}

    def relaunch(self):
        started = relaunch(self.vm)
        self.facts['relaunch_log'] = started['log']
        self.facts['relaunch_pid'] = started['pid']
        self.save()
        # A launch with a company and a memory opens on the HOME screen, past the splash; its
        # live canvas starves accessibility reads, so it is recognized by OCR on the frame.
        began = time.monotonic()
        home = None
        while time.monotonic() - began < 90:
            self.shot('08-home-after-relaunch', tree=False)
            if self.frame_says('08-home-after-relaunch', 'Talk to Rich'):
                home = round(time.monotonic() - began, 1)
                break
            time.sleep(2)
        if home is None:
            raise StepFailed('the home screen ("Talk to Rich") was not on screen within 90 s of the relaunch')
        log = guest(self.vm, 'cat ' + shlex.quote(started['log']))
        (self.out / 'relaunch-boot.log').write_text(log)
        if HEALTH_HEADLINE not in log:
            raise StepFailed('the boot log does not carry the desk health headline: ' + HEALTH_HEADLINE)
        if SENTENCE not in log:
            raise StepFailed('the boot log does not carry the asked-again sentence')
        refused = 'another RichOS session is running' in log
        if refused:
            raise StepFailed('the relaunch was refused as another session')
        # "Enter" is the home screen's own hint for Talk to Rich; the key goes to the app's pid.
        command([HERE / 'ax.sh', self.vm, '--key', '36'])
        self.wait_for(lambda: self.present('Corrections', 'AXPopUpButton'), 'the conversation view after Talk to Rich', 60)
        frame = self.shot('09-conversation-after-relaunch')
        return {'pid': started['pid'], 'home_on_screen_within_s': home, 'home_frame': '08-home-after-relaunch.png',
                'health_headline_in_boot_log': True, 'frame': frame}

    def card(self):
        self.press('Corrections', 'AXPopUpButton')
        self.wait_for(lambda: self.text_present('could not read your earlier answer'), 'the asked-again sentence', 30)
        sentences = self.find('--value', 'could not read your earlier answer', '--contains')
        asked_target = self.find('--value', REF_ASKED, '--contains', '--first')
        ordinary_target = self.find('--value', REF_NEW, '--contains', '--first')
        (self.out / 'card-nodes.json').write_text(json.dumps(
            {'sentences': sentences, 'asked_target': asked_target, 'ordinary_target': ordinary_target}, indent=2))
        if len(sentences) != 1:
            raise StepFailed(f'{len(sentences)} asked-again sentences on screen, expected exactly 1')
        s = sentences[0]
        if s.get('value') != SENTENCE:
            raise StepFailed('the sentence on screen is not the desk sentence verbatim: ' + repr(s.get('value')))
        if not asked_target or not ordinary_target:
            raise StepFailed('both cards must be on screen (asked-again and ordinary)')
        a, o = asked_target[0], ordinary_target[0]
        if not s['y'] < a['y'] < o['y']:
            raise StepFailed(f"the sentence does not lead its own card: sentence y={s['y']}, its target y={a['y']}, "
                             f"the ordinary card's target y={o['y']}")
        light = self.shot('10-card-light')
        # The Corrections dialog is modal, so Settings is unreachable behind it (measured: ax.sh
        # reports `blocked`). Closed with its own control, the theme changed, and reopened.
        self.ax('click', '--title', 'Close corrections', '--in', 'dialog', '--first')
        self.set_theme('Dark theme')
        self.press('Corrections', 'AXPopUpButton')
        self.wait_for(lambda: self.text_present('could not read your earlier answer'), 'the sentence in dark', 30)
        dark_nodes = self.find('--value', 'could not read your earlier answer', '--contains')
        (self.out / 'card-nodes-dark.json').write_text(json.dumps(dark_nodes, indent=2))
        dark = self.shot('11-card-dark')
        return {'sentence': s, 'asked_target': a, 'ordinary_target': o, 'light': light, 'dark': dark}

    def answer(self):
        asked, ordinary = self.facts.get('asked_id'), self.facts.get('ordinary_id')
        if not asked:
            raise StepFailed('seed-desk must have run')
        if not self.text_present('could not read your earlier answer'):
            self.press('Corrections', 'AXPopUpButton')
            self.wait_for(lambda: self.text_present('could not read your earlier answer'), 'the sentence', 30)
        # Breadth first, the asked-again card's buttons come before the ordinary card's; the desk
        # log, read afterwards, says which proposal the press actually answered.
        pressed = self.ax('click', '--title', 'Not now', '--role', 'AXButton', '--in', 'dialog', '--first')
        self.wait_for(lambda: not self.text_present('could not read your earlier answer'), 'the asked-again card to go', 20)
        time.sleep(1)
        log = guest(self.vm, 'cat ' + shlex.quote(self.desk))
        (self.out / 'desk-log-after.jsonl').write_text(log)
        declined = declined_ids(log)
        if declined != [asked]:
            raise StepFailed(f'the desk log records declines for {declined}, expected exactly [{asked}]')
        still = bool(self.find('--value', REF_NEW, '--contains', '--first'))
        if not still:
            raise StepFailed('the ordinary card went too')
        frame = self.shot('12-after-not-now')
        return {'pressed': [n for n in pressed if n.get('clicked')], 'declined': declined,
                'ordinary_still_pending': ordinary if still else None, 'frame': frame}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the app says it was built from')
    p.add_argument('--also-sha', default='', help='a second acceptable identity (the source commit of a build commit)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, json.JSONDecodeError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
            try:
                walk.shot(f'fail-{step}')
            except (StepFailed, subprocess.TimeoutExpired):
                pass
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        walk.pull(walk.boot_log, 'boot.log')
    except (StepFailed, subprocess.TimeoutExpired):
        pass
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""folders-walk.py — every folder field opens the folder chooser, and the Connected folders sheet says
what the CEO wrote (feedback 2026-10-06 item 2 and 2026-10-06_03), plus the rail's initials (D12).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      folders-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

HIS WORDS. 2026-10-06_03: "Clicking into 'Its folder on this Mac' also needs to open a finder or
folder picker to select the folder. And change the input placeholder to 'Click and select folder'.
In general, input fields like these should always make it as easy as possible for the user to select
the folder." Item 2: "Replace 'Connected repositories' with 'Connected folders (repositories)' in the
settings menu"; "The company should be automatically pre-selected wherever possible"; "Repository
folder selection is not as seamless as it should be"; and his copy table for the popup.

WHY THIS WALK EXISTS. No walk was written for item 2 or for _03. adopt-walk.py, command-walk.py and
reap-walk.py TYPE a path into those fields through the accessibility API, which fires no click, so
they pass whether or not the chooser opens. This walk clicks the field with the guest's mouse
(System Events `click at`, ax.sh click --at) the way a person does, at its left end, its middle and
its right end, and drives the chooser that opens: Cancel leaves the field as it was; Go to folder
(Command-Shift-G), the path, Return and Open fill it.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  first-run   memory setup declined; on "Which company is this copy of Rich for?": the folder
              field's placeholder read off the frame (OCR); the field clicked at its left, middle
              and right: each click opens the chooser (an AXSheet on the app's window), the first
              two are dismissed with Cancel (the field stays empty), the third drives Go to folder
              to ~/Acme and presses Open (the field holds that path); "Acme" added; a conversation
              is created
  connected   Settings -> "Connected folders (repositories)": the menu row's text and box; the
              sheet's headline, description, guidance, Company label, empty state, folder label,
              placeholder (OCR) and both buttons against his table; Company pre-selected as Acme;
              the folder field clicked at its middle opens the chooser; ~/Acme-docs (a plain folder
              with one file) chosen; Connect folder; the folder listed; ~/Acme-docs is now a Git
              repository whose first commit holds its file; photographed in light and dark
  initials    D12: the name "Mona Wells" set in the rail footer's popover; the rail photographed in
              light and dark with the initials node's box recorded, for qa/frame.py

Every frame and the evidence (report.json, facts.json) go to --out. Exit 0 when every step passes.
Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed
command = adopt.command

STEPS = ['identity', 'first-run', 'connected', 'initials']
NAME = 'Mona Wells'

# His table (feedback 2026-10-06 item 2d), word for word. The apostrophe in "company's" and
# "you're" is the typographic one the app ships (ui/repositories.js).
SHEET_COPY = {
    'headline': 'Connected project folders (repositories)',
    'description': 'Connect a project folder as this company’s home for Rich. Connecting it does not move or change its existing files.',
    'guidance': 'One folder per company is usually enough. But in rare cases additional folders might be needed. Ask Rich if you’re unsure.',
    'company label': 'Company',
    'empty state': 'No project folders connected yet.',
    'folder label': 'Project folder location',
    'primary button': 'Connect folder',
    'secondary button': 'Close',
}
PLACEHOLDER = 'Click and select folder'


class Walk(adopt.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.docs = self.home + '/Acme-docs'

    # --- the guest's screen -----------------------------------------------------------------
    def ax(self, mode, *args, app=None, timeout=150):
        # A read (find, tree) with no answer on a loaded host is asked again, at most three times
        # in all; a click is sent once (axwait.ask_again); any other failure is raised as before.
        for attempt in range(3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 2:
                    raise
                time.sleep(2)

    def script(self, text, timeout=90):
        out = command([HERE / 'ax.sh', self.vm, text], timeout)
        return '\n'.join(l for l in out.splitlines() if not l.startswith('{')).strip()

    def nodes(self, *args):
        """The matching nodes, [] when nothing matched. A read with no answer raises (after ax()
        asked it again): most callers record what is on screen, and no answer is not absence."""
        try:
            return [n for n in self.ax('find', *args) if 'x' in n and not n.get('meta')]
        except StepFailed as exc:
            if axwait.absent(exc):
                return []
            raise

    def node(self, title, role):
        found = self.nodes('--title', title, '--role', role, '--contains')
        if not found:
            raise StepFailed(f'no {role} "{title}" on screen')
        return found[0]

    def text_on_screen(self, words):
        """A static text, a title or a description carrying `words` (exact words, contained)."""
        return bool(self.nodes('--value', words, '--contains') or self.nodes('--title', words, '--contains'))

    def sheet_open(self):
        """Whether the folder chooser is up; None when the read had no answer (axwait.py), so
        wait_sheet() asks again and never reads it as either open or closed."""
        try:
            return bool(self.nodes('--role', 'AXSheet'))
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return None
            raise

    def wait_sheet(self, want, seconds=20):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if self.sheet_open() == want:
                return True
            time.sleep(1)
        return False

    def click_at(self, x, y):
        command([HERE / 'ax.sh', self.vm, 'click', '--at', f'{int(x)},{int(y)}'], 90)

    def ocr(self, frame, words, box=None):
        """Whether `words` are read off the frame. With `box` (an ax node), only that box is read,
        cropped and scaled three times first: the placeholder's light gray "Click" is lost when
        the whole 1680x1050 frame is read (nightly 41 walk, walk-bd5c53b21c6c: "and select
        folder" only), and read whole from the field's own crop."""
        target = self.out / frame
        if box:
            target = self.out / (Path(frame).stem + '-field-x3.png')
            command([sys.executable, HERE.parent / 'qa' / 'frame.py', 'crop', self.out / frame, target,
                     int(box['x']), int(box['y']), int(box['w']), int(box['h']), '--scale', '3'], 60)
        r = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), words, str(target), '--quiet'],
                           capture_output=True, text=True, timeout=300)
        return r.returncode == 0

    def appearance(self, dark):
        self.script('tell application "System Events" to tell appearance preferences to set dark mode to '
                    + ('true' if dark else 'false'))
        time.sleep(3)  # theme-boot follows the OS appearance live; give the repaint time

    def both_themes(self, stem):
        self.appearance(False)
        self.shot(stem + '-light.png')
        self.appearance(True)
        self.shot(stem + '-dark.png')
        self.appearance(False)
        return [stem + '-light.png', stem + '-dark.png']

    # --- the chooser --------------------------------------------------------------------------
    def chooser_from(self, field, fx, label):
        """Click `field` (an ax node) at the fraction fx of its width, mid-height, as the mouse does.
        Returns whether the chooser opened, with a frame of it."""
        x = field['x'] + max(6, min(field['w'] - 6, field['w'] * fx))
        y = field['y'] + field['h'] / 2
        self.click_at(x, y)
        opened = self.wait_sheet(True, 15)
        self.shot(f'{label}.png')
        return {'clicked_at': [int(x), int(y)], 'chooser_opened': opened}

    def cancel_chooser(self):
        try:
            self.ax('click', '--title', 'Cancel', '--role', 'AXButton', '--in', 'dialog', '--first')
        except StepFailed:
            self.script('tell application "System Events" to key code 53')
        return self.wait_sheet(False, 15)

    def choose(self, path, label):
        """In the open chooser: Go to folder, the path, Return, then Open."""
        self.script('tell application "System Events" to keystroke "g" using {command down, shift down}')
        time.sleep(2)
        self.script('tell application "System Events" to keystroke ' + json.dumps(path))
        time.sleep(1)
        self.shot(f'{label}-goto.png')
        self.script('tell application "System Events" to key code 36')
        time.sleep(2.5)
        self.shot(f'{label}-in-folder.png')
        self.ax('click', '--title', 'Open', '--role', 'AXButton', '--in', 'dialog', '--first')
        closed = self.wait_sheet(False, 15)
        time.sleep(1)
        return closed

    def field_value(self, title):
        found = self.nodes('--title', title, '--role', 'AXTextField', '--contains')
        return found[0].get('value', '') if found else None

    # --- steps --------------------------------------------------------------------------------
    def first_run(self):
        if self.present('Set it up'):
            self.press('Not now')
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        self.wait_for('Add this company', seconds=60)
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        time.sleep(1)
        self.shot('1-company-sheet.png')
        field = self.node('Its folder on this Mac', 'AXTextField')
        ev = {'field': {k: field.get(k) for k in ('x', 'y', 'w', 'h', 'value')},
              'placeholder_on_frame': self.ocr('1-company-sheet.png', PLACEHOLDER, box=field),
              'old_placeholder_on_frame': self.ocr('1-company-sheet.png', '/Users/you', box=field)}
        problems = []
        if not ev['placeholder_on_frame']:
            problems.append(f'the empty folder field does not read "{PLACEHOLDER}"')
        clicks = []
        for fx, label in ((0.03, '1-click-left'), (0.5, '1-click-middle')):
            c = self.chooser_from(field, fx, label)
            if c['chooser_opened']:
                c['cancel_closed_it'] = self.cancel_chooser()
                c['field_after_cancel'] = self.field_value('Its folder on this Mac')
                if c['field_after_cancel']:
                    problems.append(f'{label}: Cancel left {c["field_after_cancel"]!r} in the field')
            else:
                problems.append(f'{label}: the chooser did not open')
            clicks.append(c)
        c = self.chooser_from(field, 0.97, '1-click-right')
        if c['chooser_opened']:
            c['open_closed_it'] = self.choose(self.company, '1-choose')
            c['field_after_open'] = self.field_value('Its folder on this Mac')
            if (c['field_after_open'] or '').rstrip('/') != self.company:
                problems.append(f'after Open the field holds {c["field_after_open"]!r}, not the chosen folder')
        else:
            problems.append('1-click-right: the chooser did not open')
        clicks.append(c)
        ev['clicks'] = clicks
        self.shot('1-folder-chosen.png')
        self.press('Add this company')
        time.sleep(3)
        if self.present('Start the questions'):
            self.press('Not now')
        end = time.monotonic() + 90
        created = []
        while time.monotonic() < end and not created:
            rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
            created = [json.loads(r) for r in rows.splitlines() if '"ThreadCreated"' in r]
            if not created:
                time.sleep(1)
        if not created:
            problems.append('no conversation was created after the company was added')
        entities = guest(self.vm, 'cat ' + shlex.quote(self.data + '/entities.json') + ' 2>/dev/null || true')
        ev['company_folder_registered'] = self.company in entities
        if not ev['company_folder_registered']:
            problems.append('entities.json does not carry the chosen folder')
        self.facts['first_run'] = ev
        self.save()
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:1500])
        return ev

    def connected(self):
        guest(self.vm, 'mkdir -p {d} && printf "Docs for Acme\\n" > {d}/brief.md'.format(d=shlex.quote(self.docs)))
        self.press('Settings', role='AXPopUpButton')
        time.sleep(2)
        rows = self.nodes('--title', 'Connected folders', '--role', 'AXButton', '--contains')
        ev = {'menu_row': [{k: r.get(k) for k in ('title', 'desc', 'x', 'y', 'w', 'h')} for r in rows[:2]]}
        self.shot('2-menu-light.png')
        problems = []
        if not rows:
            raise StepFailed('the Settings dialog has no "Connected folders" row')
        if 'Connected folders (repositories)' not in (rows[0].get('title') or '') + ' ' + (rows[0].get('desc') or ''):
            problems.append('the row does not read "Connected folders (repositories)": ' + repr(rows[0].get('title')))
        self.appearance(True)
        self.shot('2-menu-dark.png')
        self.appearance(False)
        self.open_connected_folders(settings_open=True)
        self.shot('2-sheet-light.png')
        copy = {}
        for key, words in SHEET_COPY.items():
            copy[key] = self.text_on_screen(words)
            if not copy[key]:
                problems.append(f'{key}: "{words}" is not on the sheet')
        ev['copy'] = copy
        field = self.node('Project folder location', 'AXTextField')
        ev['placeholder_on_frame'] = self.ocr('2-sheet-light.png', PLACEHOLDER, box=field)
        if not ev['placeholder_on_frame']:
            problems.append(f'the folder field does not read "{PLACEHOLDER}"')
        company = self.nodes('--title', 'Company', '--role', 'AXPopUpButton')
        ev['company_value'] = company[0].get('value') if company else None
        if ev['company_value'] != 'Acme':
            problems.append(f'Company is {ev["company_value"]!r}, not pre-selected as Acme')
        c = self.chooser_from(field, 0.5, '2-click-middle')
        if c['chooser_opened']:
            c['open_closed_it'] = self.choose(self.docs, '2-choose')
            c['field_after_open'] = self.field_value('Project folder location')
            if (c['field_after_open'] or '').rstrip('/') != self.docs:
                problems.append(f'after Open the field holds {c["field_after_open"]!r}, not the chosen folder')
        else:
            problems.append('the folder field click did not open the chooser')
        ev['click'] = c
        self.shot('2-folder-chosen.png')
        self.ax('click', '--title', 'Connect folder', '--role', 'AXButton', '--in', 'dialog', '--first')
        end = time.monotonic() + 60
        listed = False
        while time.monotonic() < end and not listed:
            entities = guest(self.vm, 'cat ' + shlex.quote(self.data + '/entities.json') + ' 2>/dev/null || true')
            listed = self.docs in entities
            if not listed:
                time.sleep(1)
        ev['connected_in_registry'] = listed
        if not listed:
            problems.append('entities.json never listed the connected folder')
        time.sleep(2)
        ev['listed_on_sheet'] = self.text_on_screen('Acme-docs')
        if not ev['listed_on_sheet']:
            problems.append('the sheet does not list the connected folder')
        ev['git'] = guest(self.vm, 'cd {d} && git log --format="%s" 2>&1 | head -3; git ls-files 2>&1 | head -5'.format(
            d=shlex.quote(self.docs)))
        if 'brief.md' not in ev['git']:
            problems.append('the connected folder is not a Git repository holding its file: ' + ev['git'][:200])
        ev['sheet_frames'] = self.both_themes('2-sheet-after')
        try:
            self.ax('click', '--title', 'Close', '--role', 'AXButton', '--in', 'dialog', '--first')
        except StepFailed:
            self.script('tell application "System Events" to key code 53')
        self.facts['connected'] = ev
        self.save()
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:1500])
        return ev

    def initials(self):
        time.sleep(1)
        offers = self.nodes('--title', 'No name is set', '--contains') or self.nodes('--title', 'Set your name', '--contains')
        if not offers:
            raise StepFailed('the rail footer offers no name to set')
        self.press(offers[0].get('title') or 'No name is set', role=offers[0]['role'])
        time.sleep(1.5)
        self.shot('3-name-popover.png')
        fields = [n for n in self.nodes('--title', 'Your name', '--contains')
                  if n.get('role') in ('AXTextField', 'AXTextArea', 'AXComboBox')]
        if not fields:
            raise StepFailed('the popover shows no "Your name" field')
        self.type_into(NAME, '--role', fields[0]['role'], '--title', 'Your name')
        self.script('tell application "System Events" to key code 36')
        time.sleep(1)
        self.script('tell application "System Events" to key code 53')
        time.sleep(1.5)
        named = self.nodes('--title', NAME, '--contains') + self.nodes('--value', NAME, '--contains')
        initials = self.nodes('--value', 'MW') + self.nodes('--title', 'MW')
        ev = {'name_nodes': [{k: n.get(k) for k in ('role', 'title', 'value', 'x', 'y', 'w', 'h')} for n in named[:3]],
              'initials_nodes': [{k: n.get(k) for k in ('role', 'title', 'value', 'x', 'y', 'w', 'h')} for n in initials[:3]],
              'frames': self.both_themes('3-rail')}
        self.facts['initials'] = ev
        self.save()
        if not named:
            raise StepFailed('the name never reached the rail | ' + json.dumps(ev)[:800])
        return ev


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
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
        except (StepFailed, RuntimeError, ValueError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        # identity is the precondition of everything else; the screen steps each carry on so one
        # finding does not hide the next.
        if not ok and step == 'identity':
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

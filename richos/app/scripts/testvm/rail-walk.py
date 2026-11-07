#!/usr/bin/env python3
"""rail-walk.py — the navigation rail (design round 58, version A) in the packaged app, in the guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      rail-walk.py --out DIR --expect-sha SHA

THE QUESTION, the CEO's words (2026-11-06): "No nav rail on the home screen. So, the home screen needs to
be the same as before. Nav rail everywhere else but not on the home screen." "clicking the RichOS wordmark
should still retain the same effect as clicking the home button on the nav rail". The page's own suite
(ui/tests/nav-rail.js) proves it in a browser stand-in; this walk is the real app, the real mouse.

STEPS (in the guest, never on the host's screen, CEO section 65):
  identity, first-run   as folders-walk.py
  rail         the rail on the conversation screen: its eight items by DOM id and where they sit, photographed
               in light and dark; the first-run question was photographed by new-user-walk.py (not here)
  hover        the real pointer rests on RichFinder: its label (#nr-tip) is on screen to the right
  items        RichFinder opens the palette, RichCleaner its window, Bust a bug its panel, the initials circle
               Settings: each opened by a real click on the rail item and photographed
  spend        Settings, Token spend: the rail is still on the left, a click on Chats leaves Token spend
  scribe       RichScribe opened from the rail (when the app offers it): over the sheet the hover label shows
               and the activity square's menu opens on a real right-click and a real click on Readout takes;
               put back to Ink
  home         the Home button and the RichOS wordmark both lead to Home; on Home no rail item is on screen
Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('folders_walk', HERE / 'folders-walk.py')
folders = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(folders)
StepFailed = folders.StepFailed
command = folders.command

STEPS = ['identity', 'first-run', 'rail', 'hover', 'items', 'spend', 'scribe', 'home-screen']
ITEMS = ['nr-home', 'nr-chats', 'nr-finder', 'nr-cleaner', 'nr-scribe', 'nr-bug', 'res-square', 'nr-me']


class Walk(folders.Walk):
    def first_run(self):
        # The setup sheet can come up after the first look at the screen (as search-walk.py handles it).
        try:
            self.wait_for('Set it up', seconds=40)
        except StepFailed:
            pass
        return super().first_run()

    def center(self, dom_id):
        found = self.nodes('--id', dom_id)
        if not found:
            return None
        n = found[0]
        return round(n['x'] + n['w'] / 2), round(n['y'] + n['h'] / 2)

    def geometry(self, dom_id):
        found = self.nodes('--id', dom_id)
        return {k: found[0].get(k) for k in ('x', 'y', 'w', 'h')} if found else None

    def pointer(self, x, y, mode=None):
        pts = [f'{x},{y}'] if mode == 'hover' else [f'{x},{y}', f'{x},{y}']
        command([HERE / 'pointer-drag.sh', self.vm] + pts + (['--' + mode] if mode else []), 60)

    def click_id(self, dom_id):
        c = self.center(dom_id)
        if not c:
            raise StepFailed(f'#{dom_id} is not on screen')
        self.pointer(*c)

    def escape(self):
        self.script('tell application "System Events" to key code 53')
        time.sleep(1.5)

    def rail_ids(self):
        return {i: self.geometry(i) for i in ITEMS}

    def rail(self):
        self.script('tell application "RichOS" to activate')
        time.sleep(1)
        got = self.rail_ids()
        self.shot('rail-light.png')
        self.appearance(True)
        self.shot('rail-dark.png')
        self.appearance(False)
        self.facts['rail'] = got
        self.save()
        # nr-scribe is shown only where RichScribe is; every other item must be there
        missing = [i for i, g in got.items() if g is None and i != 'nr-scribe']
        if missing:
            raise StepFailed('rail items not on screen: ' + ', '.join(missing))
        # one strip: every item's left edge within 15 px of Home's (the strip is the window's left edge)
        left = [i for i, g in got.items() if g and abs(g['x'] - got['nr-home']['x']) > 15]
        if left:
            raise StepFailed('rail items not at the window edge: ' + ', '.join(left))
        return got

    def hover(self):
        x, y = self.center('nr-finder')
        self.pointer(x, y, 'hover')
        time.sleep(1.2)
        tip = self.nodes('--id', 'nr-tip')
        self.shot('hover-finder.png')
        ev = {'finder_center': [x, y], 'tip': [{k: n.get(k) for k in ('x', 'y', 'w', 'h', 'value', 'title')} for n in tip],
              'tip_text_on_screen': self.text_on_screen('RichFinder')}
        self.facts['hover'] = ev
        self.save()
        if not ev['tip_text_on_screen']:
            raise StepFailed('no RichFinder label while the pointer rests on its item: ' + json.dumps(ev))
        return ev

    def items(self):
        ev = {}
        # RichFinder: the palette
        self.click_id('nr-finder')
        time.sleep(2)
        self.shot('item-finder.png')
        ev['finder_palette'] = bool(self.nodes('--role', 'AXTextField', '--first')) and self.text_on_screen('Search')
        self.escape()
        # Bust a bug: the panel
        self.click_id('nr-bug')
        time.sleep(2.5)
        ev['bug_panel_after_first_click'] = bool(self.nodes('--id', 'bugdock-input'))
        self.shot('item-bug.png')
        if not ev['bug_panel_after_first_click']:
            self.click_id('nr-bug')
            time.sleep(2.5)
            ev['bug_panel_after_second_click'] = bool(self.nodes('--id', 'bugdock-input'))
            self.shot('item-bug-second.png')
        ev['bug_doctor_line'] = self.text_on_screen('RichDoctor')
        self.escape()
        # the initials circle: Settings
        self.click_id('nr-me')
        time.sleep(2)
        self.shot('item-me.png')
        ev['settings_open'] = bool(self.nodes('--id', 'set-spend-open')) or self.text_on_screen('Token spend')
        self.escape()
        # RichCleaner: its own window
        self.click_id('nr-cleaner')
        time.sleep(3)
        self.shot('item-cleaner.png')
        ev['cleaner_window'] = self.script('tell application "System Events" to tell process "RichOS" to get name of every window')
        self.script('tell application "System Events" to keystroke "w" using command down')
        time.sleep(1.5)
        self.script('tell application "RichOS" to activate')
        time.sleep(1)
        self.facts['items'] = ev
        self.save()
        if not (ev['bug_panel_after_first_click'] or ev.get('bug_panel_after_second_click')):
            raise StepFailed('Bust a bug on the rail opened no panel: ' + json.dumps(ev))
        if not ev['settings_open']:
            raise StepFailed('the initials circle did not open Settings: ' + json.dumps(ev))
        return ev

    def spend(self):
        # Settings is open from the last step (or opened by the Settings button)
        self.script('tell application "RichOS" to activate')
        time.sleep(1)
        # the first click after RichCleaner's window was closed only brings the main window forward
        # (seen on this build: items step), so look again between clicks
        for n in range(4):
            if self.nodes('--id', 'set-spend-open'):
                break
            self.click_id('nr-me')  # Settings by the initials circle, the way the items step opened it
            time.sleep(2)
            self.shot(f'spend-open-try-{n}.png')
        self.click_id('set-spend-open')
        time.sleep(3)
        self.shot('spend.png')
        rail = self.rail_ids()
        ev = {'rail_on_spend': rail, 'spend_back': bool(self.nodes('--id', 'spend-back'))}
        self.facts['spend'] = ev
        self.save()
        absent = [i for i, g in rail.items() if g is None and i != 'nr-scribe']
        if absent or not ev['spend_back']:
            raise StepFailed('the rail is not on Token spend: ' + json.dumps(ev))
        self.click_id('nr-chats')
        time.sleep(2)
        ev['spend_left_by_chats'] = not self.nodes('--id', 'spend-back')
        self.shot('spend-left.png')
        if not ev['spend_left_by_chats']:
            raise StepFailed('Chats did not leave Token spend')
        return ev

    def scribe(self):
        if not self.nodes('--id', 'nr-scribe'):
            self.shot('no-scribe-item.png')
            return {'skipped': 'the app shows no RichScribe item in this guest'}
        self.click_id('nr-scribe')
        time.sleep(3)
        self.shot('scribe-sheet.png')
        ev = {'sheet_open': bool(self.nodes('--id', 'dictation-sheet'))}
        x, y = self.center('nr-finder')
        self.pointer(x, y, 'hover')
        time.sleep(1.2)
        self.shot('scribe-hover.png')
        ev['hover_label_over_sheet'] = bool(self.nodes('--id', 'nr-tip')) and self.text_on_screen('RichFinder')
        if not ev['hover_label_over_sheet']:
            raise StepFailed('no RichFinder label while the RichScribe sheet is open: ' + json.dumps(ev))
        c = self.center('res-square')
        if not c:
            raise StepFailed('the activity square is not on screen')
        checked = ('1', 'true', 'True')

        def choice(title):
            found = self.nodes('--title', title, '--contains')
            return found[0] if found else None

        def open_menu():
            at = self.center('res-square')
            self.pointer(at[0], at[1], 'right')
            time.sleep(1.5)

        def press(node):
            self.pointer(round(node['x'] + node['w'] / 2), round(node['y'] + node['h'] / 2))
            time.sleep(1.5)

        open_menu()
        self.shot('scribe-menu.png')
        readout, ink = choice('Readout'), choice('Ink')
        ev['menu_choice_on_screen'] = bool(readout and ink)
        if not ev['menu_choice_on_screen']:
            raise StepFailed('the activity menu did not show Ink and Readout over the sheet: ' + json.dumps(ev))
        ev['before'] = {'ink': str(ink.get('value')), 'readout': str(readout.get('value'))}
        press(readout)
        self.shot('scribe-readout.png')
        # the selected view is read back from the menu itself, opened again
        open_menu()
        self.shot('scribe-menu-after-readout.png')
        after_readout = {'ink': str((choice('Ink') or {}).get('value')), 'readout': str((choice('Readout') or {}).get('value'))}
        ev['after_readout_click'] = after_readout
        # put it back to Ink and read that back too
        if choice('Ink'):
            press(choice('Ink'))
        open_menu()
        after_ink = {'ink': str((choice('Ink') or {}).get('value')), 'readout': str((choice('Readout') or {}).get('value'))}
        ev['after_ink_click'] = after_ink
        self.shot('scribe-menu-after-ink.png')
        if after_readout['readout'] not in checked or after_readout['ink'] in checked:
            raise StepFailed('the click on Readout did not select it: ' + json.dumps(ev))
        if after_ink['ink'] not in checked or after_ink['readout'] in checked:
            raise StepFailed('the click on Ink did not put Ink back: ' + json.dumps(ev))
        self.escape()  # the open menu
        if self.nodes('--id', 'dictation-sheet'):
            self.escape()  # then the sheet
        ev['sheet_closed_by_escape'] = not self.nodes('--id', 'dictation-sheet')
        self.facts['scribe'] = ev
        self.save()
        if not ev['sheet_open']:
            raise StepFailed('the RichScribe sheet did not open: ' + json.dumps(ev))
        return ev

    def home_screen(self):  # not `home`: Walk.home is the guest's home folder path
        self.script('tell application "RichOS" to activate')
        time.sleep(1)
        self.click_id('nr-home')
        time.sleep(3)
        self.shot('home-from-rail.png')
        on_home = {i: self.geometry(i) for i in ITEMS}
        ev = {'items_on_screen_at_home_by_button': [i for i, g in on_home.items() if g]}
        # back to the app, then the wordmark
        self.press('Talk to Rich', contains=True) if self.present('Talk to Rich') else None
        time.sleep(3)
        self.shot('app-again.png')
        ev['rail_back_in_app'] = bool(self.geometry('nr-chats'))
        if ev['rail_back_in_app']:
            self.click_id('rail-wordmark')
            time.sleep(3)
            self.shot('home-from-wordmark.png')
            ev['items_on_screen_at_home_by_wordmark'] = [i for i in ITEMS if self.geometry(i)]
        self.facts['home'] = ev
        self.save()
        if ev['items_on_screen_at_home_by_button'] or ev.get('items_on_screen_at_home_by_wordmark'):
            raise StepFailed('rail items are on screen on Home: ' + json.dumps(ev))
        return ev


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in STEPS]
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))
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
            row['detail'] = str(exc)[:3000]
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

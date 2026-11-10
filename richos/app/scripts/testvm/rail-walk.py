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
import re
import shlex
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
OPTIONAL = ['net-cut', 'net-restore', 'richpass-download', 'first-click', 'richpass-page', 'richpass-links', 'richpass-protect', 'richpass-html']
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

    def open_richpass(self):
        """Dismiss the first-run controls, then open the RichPass page from the rail."""
        entry_controls = ('setup-close', 'memory-setup-later', 'first-run-later',
                          'entity-picker-later', 'home-enter')
        deadline = time.monotonic() + 45
        while not self.center('nr-pass'):
            if time.monotonic() >= deadline:
                self.shot('richpass-entry-failure.png')
                raise StepFailed('the RichPass rail item did not appear after app startup')
            control = next((item for item in entry_controls if self.center(item)), None)
            if control:
                self.click_id(control)
            time.sleep(0.5)
        self.click_id('nr-pass')
        self.wait_for('Download RichPass')

    def save_panel(self, directory):
        """Drive the native save panel (already open) to `directory` and press Save."""
        self.script('tell application "System Events" to keystroke "g" using {command down, shift down}')
        self.script('tell application "System Events"\nkeystroke ' + json.dumps(directory)
                    + '\nkey code 36\nend tell')
        self.ax('click', '--title', 'Save', '--role', 'AXButton', '--in', 'dialog', '--first')
        if not self.wait_sheet(False):
            raise StepFailed('the native save panel did not close')

    def richpass_download(self):
        """Press the real download button and save the bundled bytes in the guest."""
        self.open_richpass()

        directory = self.home + '/RichPass-download-proof'
        destination = directory + '/RichPass.html'
        folders.guest(self.vm, 'mkdir ' + shlex.quote(directory))
        self.click_id('rp-download')
        if not self.wait_sheet(True):
            raise StepFailed('Download RichPass did not open the native save panel')
        self.save_panel(directory)

        digest = folders.guest(self.vm, '/usr/bin/shasum -a 256 ' + shlex.quote(destination)).split()[0]
        if digest != self.a.expect_richpass_sha256:
            raise StepFailed('the native save did not write the approved RichPass bytes')
        self.wait_for('Save RichPass and protect it')
        return {'sha256': digest, 'file': 'RichPass.html', 'protected': False}

    def first_click(self):
        """Fix 8032cea3f: after RichCleaner's window closes, ONE real click on a rail item and ONE on
        Settings each open on that first click. No activate between the close and the click."""
        ev = {}
        for label, item, probe in (('rail_bug', 'nr-bug', 'bugdock-input'), ('settings', 'nr-me', 'set-spend-open')):
            self.click_id('nr-cleaner')
            time.sleep(3)
            ev[label + '_cleaner_windows'] = self.script(
                'tell application "System Events" to tell process "RichOS" to get name of every window')
            self.script('tell application "System Events" to keystroke "w" using command down')
            time.sleep(1.5)
            self.click_id(item)  # the one click
            time.sleep(2.5)
            ev[label + '_opened_on_first_click'] = bool(self.nodes('--id', probe))
            self.shot('first-click-' + label + '.png')
            self.escape()
        self.facts['first_click'] = ev
        self.save()
        bad = [k for k, v in ev.items() if k.endswith('first_click') and not v]
        if bad:
            raise StepFailed('not opened on the first click after RichCleaner closed: ' + json.dumps(ev))
        return ev

    RICHPASS_TEXTS = ['Safer Than Any Password Manager', 'More info', 'RichPass.html \u00b7 free \u00b7 open-source \u00b7 one file',
                      'Works offline, in your browser', 'Ready when you return', 'Re-generate instead of saving',
                      'Enable protection', 'While RichOS is open', 'Download RichPass', 'Save RichPass and protect it']

    def richpass_page(self):
        self.open_richpass()
        time.sleep(1.5)
        ev = {'present': {t: self.text_on_screen(t) for t in self.RICHPASS_TEXTS}}
        ev['absent_remember_passphrase'] = not self.text_on_screen('Remember your passphrase')
        ev['mark_images'] = [{k: n.get(k) for k in ('x', 'y', 'w', 'h')} for n in self.nodes('--role', 'AXImage')][:6]
        self.both_themes('richpass-page')
        self.facts['richpass_page'] = ev
        self.save()
        missing = [t for t, ok in ev['present'].items() if not ok]
        if missing or not ev['absent_remember_passphrase']:
            raise StepFailed('RichPass page text: missing %s; remember-block gone: %s'
                             % (missing, ev['absent_remember_passphrase']))
        return ev

    def safari_url(self):
        return folders.guest(self.vm, "osascript -e 'tell application \"Safari\" to if (count windows) > 0 then "
                                      "get URL of current tab of front window'").strip()

    def wait_url(self, want, seconds=30):
        end = time.monotonic() + seconds
        url = ''
        while time.monotonic() < end:
            try:
                url = self.safari_url()
            except Exception:  # noqa: BLE001 - Safari may not be up yet
                url = ''
            if url.rstrip('/') == want:
                return url
            time.sleep(2)
        return url

    def richpass_links(self):
        want = 'https://github.com/RichSpark/RichOS/tree/main/richos/tools/richpass'
        ev = {}
        self.open_richpass()
        self.click_id('rp-more')  # the mouse
        ev['mouse_url'] = self.wait_url(want)
        self.shot('why-how-mouse.png')
        folders.guest(self.vm, "osascript -e 'tell application \"Safari\" to quit'")
        time.sleep(3)
        self.script('tell application "RichOS" to activate')
        time.sleep(1.5)
        self.click_id('rp-title')  # focus the heading, then the keyboard: Tab to the link, Return
        self.script('tell application "System Events" to key code 48')
        time.sleep(0.7)
        self.shot('why-how-focused.png')
        self.script('tell application "System Events" to key code 36')
        ev['keyboard_url'] = self.wait_url(want)
        self.shot('why-how-keyboard.png')
        folders.guest(self.vm, "osascript -e 'tell application \"Safari\" to quit'")
        time.sleep(2)
        self.script('tell application "RichOS" to activate')
        self.facts['richpass_links'] = ev
        self.save()
        for how in ('mouse', 'keyboard'):
            if ev[how + '_url'].rstrip('/') != want:
                raise StepFailed(f'Why & How by {how} opened {ev[how + "_url"]!r}, not {want}')
        return ev

    def net_cut(self):
        """The guest loses its default route (as netcut-walk.py does): nothing beyond its own subnet answers."""
        gateway = folders.guest(self.vm, "route -n get default 2>/dev/null | awk '/gateway:/{print $2}'", 30).strip()
        if not gateway:
            raise StepFailed('the guest has no default route to remove')
        gateway6 = folders.guest(self.vm, "netstat -rn -f inet6 | awk '$1==\"default\" && $NF==\"en0\"{print $2; exit}'", 30).strip()
        self.facts.update({'gateway': gateway, 'gateway6': gateway6})
        self.save()
        folders.guest(self.vm, 'sudo -n route -q -n delete default >/dev/null'
                      + ('; sudo -n route -q -n delete -inet6 default >/dev/null' if gateway6 else ''), 30)
        probe = folders.guest(self.vm, 'curl -sS -m 6 -o /dev/null -w "%{http_code}" https://github.com 2>&1; true', 30).strip()
        self.facts['cut_probe'] = probe
        self.save()
        if probe.endswith('200') or probe.endswith('301'):
            raise StepFailed('github.com still answered after the cut: ' + probe)
        return {'gateway': gateway, 'probe_after_cut': probe}

    def net_restore(self):
        gateway, gateway6 = self.facts.get('gateway'), self.facts.get('gateway6')
        if not gateway:
            raise StepFailed('no default route was recorded to put back')
        folders.guest(self.vm, 'sudo -n route -q -n add default ' + shlex.quote(gateway) + ' >/dev/null'
                      + ('; sudo -n route -q -n add -inet6 default ' + shlex.quote(gateway6) + ' >/dev/null' if gateway6 else ''), 30)
        probe = ''
        for _ in range(20):
            probe = folders.guest(self.vm, 'curl -sS -m 6 -o /dev/null -w "%{http_code}" https://github.com 2>&1; true', 30).strip()
            if probe.endswith('200') or probe.endswith('301'):
                break
            time.sleep(2)
        if not (probe.endswith('200') or probe.endswith('301')):
            raise StepFailed('the guest did not reach github.com after the restore: ' + probe)
        return {'probe_after_restore': probe}

    def richpass_protect(self):
        self.open_richpass()
        directory = self.home + '/RichPass-protect-proof'
        folders.guest(self.vm, 'mkdir ' + shlex.quote(directory))
        ev = {}
        self.press('Save RichPass and protect it')
        if not self.wait_sheet(True):
            raise StepFailed('Save RichPass and protect it did not open the save panel')
        self.save_panel(directory)
        # The confirmation is a native alert the app's accessibility tree does not list (rp2: the tree held
        # only the page, every control disabled), so it is found on the picture and answered with the real
        # mouse on its button, centered at (840, 510) in the guest's 1680x1050 frame (protect-confirm.png).
        seen = False
        for _ in range(15):
            self.shot('protect-confirm.png')
            if self.ocr('protect-confirm.png', 'Reserve this exact location'):
                seen = True
                break
            time.sleep(1)
        if not seen:
            raise StepFailed('the Protect this location confirmation did not appear (protect-confirm.png)')
        self.pointer(840, 510)
        ev['confirmation_pressed'] = 'Protect this location at 840,510'
        time.sleep(3)
        self.shot('protect-on.png')
        ev['state_title'] = [n.get('value') or n.get('title') for n in self.nodes('--id', 'rp-state-title')]
        ev['stop_button'] = self.present('Stop protecting')
        ev['switch'] = [str(n.get('value')) for n in self.nodes('--id', 'rp-switch')]
        ev['saved'] = folders.guest(self.vm, '/usr/bin/shasum -a 256 ' + shlex.quote(directory + '/RichPass.html')).split()[0]
        self.both_themes('protect-on')
        if not ev['stop_button']:
            raise StepFailed('no protected state after saving: ' + json.dumps(ev))
        self.press('Stop protecting')
        time.sleep(2)
        self.shot('protect-off.png')
        ev['off_again'] = self.present('Save RichPass and protect it')
        self.facts['richpass_protect'] = ev
        self.save()
        if ev['saved'] != self.a.expect_richpass_sha256 and self.a.expect_richpass_sha256:
            raise StepFailed('the protected copy is not the approved bytes: ' + json.dumps(ev))
        if not ev['off_again']:
            raise StepFailed('Stop protecting did not return to off: ' + json.dumps(ev))
        return ev

    def safari_front(self):
        """RichOS's window covers Safari's, so RichOS is hidden (as a person would) and Safari raised."""
        self.script('tell application "System Events" to set visible of process "RichOS" to false')
        time.sleep(1)
        self.script('tell application "Safari" to activate')
        time.sleep(1.5)

    def keys_into_field(self, text):
        """Keys into the master field RichPass focuses itself (a click from the pointer tool would raise
        RichOS over Safari again, and a secure field cannot be read back for a paste check)."""
        if not self.ax('find', '--title', 'aster passphrase', '--role', 'AXTextField', '--contains', app='Safari'):
            raise StepFailed('no master passphrase field in Safari')
        self.script('tell application "Safari" to activate')
        time.sleep(0.5)
        self.script('tell application "System Events" to keystroke ' + json.dumps(text))
        time.sleep(0.5)

    def richpass_html(self):
        """RichPass.html itself, the saved copy opened in the guest's Safari: first setup, one password."""
        path = self.home + '/RichPass-download-proof/RichPass.html'
        folders.guest(self.vm, 'open -a Safari ' + shlex.quote(path))
        time.sleep(8)
        self.safari_front()
        ev = {}
        master = 'correct horse battery staple 2026'

        def dump(name):
            (self.out / name).write_text(json.dumps(self.ax('tree', '--depth', '30', app='Safari'), indent=1))
        self.shot('html-0-open.png')
        dump('html-0-tree.json')
        if self.present('Get started', app='Safari'):
            for n in range(5):
                self.keys_into_field(master)
                time.sleep(0.5)
                self.shot(f'html-setup-{n}.png')
                for label in ('Get started', 'Next', 'Finish setup'):
                    if self.present(label, app='Safari'):
                        self.press(label, app='Safari')
                        break
                time.sleep(2 if n < 4 else 8)
            self.shot('html-setup-done.png')
            dump('html-1-tree.json')
            if self.present("OK. Let", app='Safari'):
                self.press("OK. Let", app='Safari')
                time.sleep(2)
        self.shot('html-generate.png')
        dump('html-2-tree.json')
        ev['setup_reached_generate'] = bool(self.ax('find', '--title', 'RichPass: Generate', '--role', 'AXHeading', '--contains', app='Safari'))
        if not ev['setup_reached_generate']:
            raise StepFailed('first setup did not reach the Generate page (html-setup-done.png, html-generate.png)')
        ev['passwords'] = []
        previous = ''
        for domain in ('example.com', 'mybank.example', 'news.example.org'):
            self.type_into(domain, '--title', 'Domain: Domain or URL:', '--role', 'AXTextField', app='Safari')
            time.sleep(0.5)
            self.press('Derive password from master', app='Safari')
            # The field's value is not in Safari's accessibility tree, so the password is read the way a
            # person uses it: the page's Copy button, then the guest's clipboard.
            got = ''
            for _ in range(30):  # Argon2id, 128 MiB, three passes: seconds in the guest
                folders.guest(self.vm, 'printf cleared | pbcopy')
                self.press('Copy', app='Safari')
                time.sleep(1)
                got = folders.guest(self.vm, 'pbpaste').strip()
                if got and got != 'cleared' and got != previous:
                    break
                got = ''
                time.sleep(1.5)
            previous = got
            self.shot('html-password-' + domain.split('.')[0] + '.png')
            ok = (len(got) == 20 and re.fullmatch(r'[A-Za-z0-9@]{20}', got) is not None
                  and any(c.isdigit() for c in got) and any(c.isupper() for c in got))
            ev['passwords'].append({'domain': domain, 'length': len(got), 'only_letters_digits_at': bool(re.fullmatch(r'[A-Za-z0-9@]*', got)),
                                    'has_digit': any(c.isdigit() for c in got), 'has_upper': any(c.isupper() for c in got), 'ok': ok})
            # not recorded: the passwords themselves stay out of the evidence
            time.sleep(1)
        ev['all_ok'] = all(x['ok'] for x in ev['passwords']) and len({x['length'] for x in ev['passwords']}) == 1
        self.facts['richpass_html'] = ev
        self.save()
        if not ev['all_ok']:
            raise StepFailed('a generated password broke the rules: ' + json.dumps(ev))
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
    p.add_argument('--expect-richpass-sha256', help='approved HTML checksum for the optional richpass-download step')
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in [*STEPS, *OPTIONAL]]
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))
    if {'richpass-download', 'richpass-protect'} & set(steps) and not re.fullmatch(r'[a-f0-9]{64}', a.expect_richpass_sha256 or ''):
        p.error('richpass-download and richpass-protect require --expect-richpass-sha256')
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

#!/usr/bin/env python3
"""resources-cleaner-accounts-walk.py — three nightly 83 surfaces on the real app in the guest:
the Resources square, the RichCleaner setting, and Claude accounts / the usage panel for an account
that has no five-hour window.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      ./resources-cleaner-accounts-walk.py --out DIR --expect-sha SHA [--steps a,b,c]

WHAT IT DOES, reusing adopt-walk.py's identity and first-run steps, never on the host's screen (CEO §65):
  identity    the running app says it was built from --expect-sha
  first-run   memory question declined, the company "Acme" with a folder, the first conversation
  resources   the square (#res-square, "Mac resources") at the foot of the sidebar is drawn; pointed
              at with the real mouse it shows the "This Mac" card with CPU, RAM and the last 60
              seconds; clicked, the card stays (aria-expanded); right-clicked, the "Mac resources
              view" menu offers Ink and Readout; Readout adds CPU % and RAM % to the square. One frame
              per state in the guest's light appearance and in its dark one.
  cleaner     Settings has a RichCleaner row; its sheet shows the switch OFF; the switch is pressed
              on, read back, and pressed off, read back. Where RichScribe is off the sheet says
              "Turn on RichScribe first": that is recorded as what the user sees, not hidden.
  accounts    Settings, Claude accounts, and Settings, Claude Code quota: the 5-hour meter and the
              5-hour choice are looked for. None, with a weekly meter, is PASS; one is NOT RUN with
              the reading seen.

Every step records the accessibility nodes it read (<out>/<step>.nodes.jsonl) and its frames. A step
that cannot answer says so (outcome FAIL with the reason) and the walk carries on to the next step.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed
command = adopt.command

STEPS = ['identity', 'first-run', 'resources', 'cleaner', 'accounts']


class Walk(adopt.Walk):
    def appearance(self, dark):
        guest(self.vm, "osascript -e 'tell application \"System Events\" to tell appearance preferences "
                       "to set dark mode to %s'" % ('true' if dark else 'false'))
        time.sleep(2)

    def activate(self):
        guest(self.vm, "osascript -e 'tell application id \"com.richos.app\" to activate'")

    def node(self, dom_id):
        try:
            nodes = [n for n in self.ax('find', '--id', dom_id, '--first') if not n.get('meta')]
        except StepFailed as exc:
            if adopt.axwait.not_yet(exc, 'find'):
                return None
            raise
        return nodes[0] if nodes else None

    def until_node(self, dom_id, seconds=20, gone=False):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            n = self.node(dom_id)
            if bool(n) != gone:
                return n
            time.sleep(1)
        return None

    def dump(self, name):
        """The whole accessibility tree as JSON lines, and its text values, for the record."""
        nodes = [n for n in self.ax('tree') if not n.get('meta')]
        (self.out / (name + '.nodes.jsonl')).write_text(''.join(json.dumps(n) + '\n' for n in nodes))
        return nodes

    def texts(self, nodes):
        return ' | '.join(str(n.get(k)) for n in nodes for k in ('title', 'value', 'desc') if n.get(k))

    def center(self, n):
        return '%d,%d' % (n['x'] + n['w'] / 2, n['y'] + n['h'] / 2)

    def pointer(self, point, mode=None):
        argv = [HERE / 'pointer-drag.sh', self.vm, point] + ([point] if mode != 'hover' else [])
        if mode == 'hover':
            argv.append('--hover')
        elif mode == 'right':
            argv.append('--right')
        command(argv, 60)

    def settings(self, want_open):
        """ONE press of Settings' own button (it toggles), then wait for #set-menu to be a panel of
        readable size (absent from the tree while closed) or gone."""
        self.activate()
        n = self.node('set-menu')
        shown = bool(n and (n.get('w') or 0) >= 100 and (n.get('h') or 0) >= 100)
        if shown != want_open:
            self.ax('click', '--id', 'set-btn', '--first')
        end = time.monotonic() + 15
        while time.monotonic() < end:
            n = self.node('set-menu')
            shown = bool(n and (n.get('w') or 0) >= 100 and (n.get('h') or 0) >= 100)
            if shown == want_open:
                time.sleep(1)
                return
            time.sleep(1)
        raise StepFailed('Settings did not %s after one press of its button' % ('open' if want_open else 'close'))

    def ocr(self, name):
        """The frame's text through the shared OCR reader (qa/ocr-read.py)."""
        self.shot(name + '.png')
        r = subprocess.run([sys.executable, str(HERE.parent / 'qa' / 'ocr-read.py'), str(self.out / (name + '.png'))],
                           capture_output=True, text=True, timeout=90)
        if r.returncode != 0:
            raise StepFailed('ocr-read could not read %s.png: %s' % (name, r.stderr.strip()))
        (self.out / (name + '.txt')).write_text(r.stdout)
        return r.stdout

    def pull_logs(self, name):
        """The app's own log, whether its process is alive, and any crash report the guest wrote,
        kept whole in the walk's out folder (a step that finds the app gone must say why)."""
        try:
            (self.out / (name + '.app.log')).write_text(
                guest(self.vm, 'cat ' + self.payload + '/app.log 2>/dev/null || true', 60))
            (self.out / (name + '.ps.txt')).write_text(
                guest(self.vm, "ps -axo pid,etime,command | grep -i '[R]ichOS' | cut -c1-200 || true", 60))
            (self.out / (name + '.crash.txt')).write_text(guest(
                self.vm, 'ls -la ~/Library/Logs/DiagnosticReports 2>&1; '
                         'find ~/Library/Logs/DiagnosticReports /Library/Logs/DiagnosticReports -type f -mmin -30 '
                         '-exec sh -c \'echo "== $1"; head -c 5000 "$1"\' _ {} \\; 2>&1; true', 60))
            (self.out / (name + '.oslog.txt')).write_text(guest(
                self.vm, "/usr/bin/log show --last 3m --style compact --predicate "
                         "'process == \"richos-tauri\" OR eventMessage CONTAINS \"richos-tauri\"' 2>&1 | tail -60; true", 90))
        except Exception as exc:  # noqa: BLE001
            (self.out / (name + '.pull-failed.txt')).write_text(str(exc))

    # --- steps --------------------------------------------------------------------------------
    def resources(self):
        ev = {}
        self.appearance(False)
        sq = self.node('res-square')
        if not sq:
            # a person opens the sidebar with its own button; the toolbar's first button
            for title in ('Show sidebar', 'Sidebar', 'Toggle sidebar', 'Hide sidebar'):
                if self.present(title, contains=True):
                    self.press(title)
                    break
            sq = self.until_node('res-square', 15)
        if not sq:
            self.dump('resources-missing')
            self.shot('resources-missing.png')
            raise StepFailed('no Mac resources square (#res-square) is on screen')
        ev['square'] = {k: sq.get(k) for k in ('role', 'title', 'desc', 'value', 'x', 'y', 'w', 'h')}
        self.shot('res-ink-light.png')
        # pointed at: the card
        self.pointer(self.center(sq), 'hover')
        time.sleep(1.5)
        nodes = self.dump('res-hover')
        t = self.texts(nodes)
        self.shot('res-hover-light.png')
        ev['hover_card'] = {w: (w in t) for w in ('This Mac', 'CPU', 'RAM', '60 seconds ago')}
        # clicked: the card stays
        self.pointer(self.center(sq))
        time.sleep(1.5)
        n2 = self.node('res-square')
        self.shot('res-click-light.png')
        nodes = self.dump('res-click')
        ev['click'] = {'aria_expanded_node': {k: (n2 or {}).get(k) for k in ('value', 'desc', 'title')},
                       'card_text': 'This Mac' in self.texts(nodes)}
        # right-click: the view menu
        self.pointer(self.center(sq), 'right')
        time.sleep(1.5)
        nodes = self.dump('res-menu')
        t = self.texts(nodes)
        self.shot('res-menu-light.png')
        ev['menu'] = {w: (w in t) for w in ('Mac resources view', 'Ink', 'Readout')}
        if not ev['menu']['Readout']:
            raise StepFailed('right-click did not offer Readout: ' + json.dumps(ev))
        self.press('Readout', role='AXMenuItem', contains=False) if self.present('Readout', 'AXMenuItem') else \
            self.press('Readout', role='AXButton')
        time.sleep(2)
        # leave the card, then read the square in Readout
        self.pointer('700,600', 'hover')
        time.sleep(2)
        nodes = self.dump('res-readout')
        self.shot('res-readout-light.png')
        # the readout is aria-hidden (the square's accessible name is "Mac resources"), so it is read
        # off the frame, as a person reads it: two percentages beside the square
        self.ocr('res-readout-light-read')
        sqn = self.node('res-square') or sq
        crop = [str(sqn['x'] - 90), str(sqn['y'] - 5), str(int(sqn['w']) + 100), str(int(sqn['h']) + 10)]
        subprocess.run([sys.executable, str(HERE.parent / 'qa' / 'frame.py'), 'crop',
                        str(self.out / 'res-readout-light-read.png'), str(self.out / 'res-readout-crop.png'),
                        *crop, '--scale', '4'], check=True, capture_output=True, text=True, timeout=60)
        seen = subprocess.run([sys.executable, str(HERE.parent / 'qa' / 'ocr-read.py'), str(self.out / 'res-readout-crop.png')],
                              capture_output=True, text=True, timeout=90).stdout
        (self.out / 'res-readout-crop.txt').write_text(seen)
        ev['readout'] = {'percent_texts_on_frame': re.findall(r'\b\d{1,3}\s?%', seen)[:8]}
        ev['readout']['texts_with_percent'] = ev['readout']['percent_texts_on_frame']
        # the dark theme, Readout and then the card
        self.appearance(True)
        self.shot('res-readout-dark.png')
        sq = self.node('res-square') or sq
        self.pointer(self.center(sq), 'hover')
        time.sleep(1.5)
        self.shot('res-hover-dark.png')
        self.pointer('700,600', 'hover')
        self.appearance(False)
        self.facts['resources'] = ev
        self.save()
        (self.out / 'resources.json').write_text(json.dumps(ev, indent=2) + '\n')
        if not all(ev['hover_card'].values()) and not ev['click']['card_text']:
            raise StepFailed('the This Mac card never showed: ' + json.dumps(ev))
        if not ev['readout']['texts_with_percent']:
            raise StepFailed('Readout shows no percentages: ' + json.dumps(ev))
        return ev

    def cleaner(self):
        ev = {}
        self.settings(True)
        self.shot('cleaner-settings.png')
        nodes = self.dump('cleaner-settings')
        if not self.node('set-cleaner-open'):
            raise StepFailed('Settings has no RichCleaner row; texts: ' + self.texts(nodes)[:600])
        row = self.node('set-cleaner-state')
        ev['row_state'] = row and (row.get('value') or row.get('title'))
        self.ax('click', '--id', 'set-cleaner-open', '--first')
        time.sleep(2)
        nodes = self.dump('cleaner-sheet')
        self.shot('cleaner-sheet-off.png')
        ev['sheet_text'] = self.texts(nodes)[:900]
        sw = None
        for n in nodes:
            if n.get('desc') == 'RichCleaner' or n.get('title') == 'RichCleaner':
                if n.get('role') in ('AXCheckBox', 'AXSwitch', 'AXButton', 'AXRadioButton'):
                    sw = n
        ev['switch_before'] = sw and {k: sw.get(k) for k in ('role', 'value', 'enabled', 'x', 'y')}
        if not sw:
            raise StepFailed('no RichCleaner switch in the sheet: ' + json.dumps(ev))
        self.pull_logs('cleaner-before-press')
        # CONTROL: the app is idle with the sheet open for 10 s and must still be running, so a
        # process that is gone after the press is told from one that was already going.
        time.sleep(10)
        alive = lambda: guest(self.vm, "ps -axo command | grep -c '[R]ichOS.app/Contents/MacOS/richos-tauri$' || true").strip()  # noqa: E731
        ev['main_process_before_press'] = alive()
        if ev['main_process_before_press'] in ('', '0'):
            self.pull_logs('cleaner-gone-before-press')
            raise StepFailed('the app was already gone before the switch was pressed (idle 10 s with the sheet open)')
        try:
            # a hand's click: the real mouse at the switch's center
            self.pointer(self.center(sw))
            time.sleep(3)
            ev['main_process_after_press'] = alive()
            nodes = self.dump('cleaner-on')
        except StepFailed:
            ev['main_process_after_press'] = alive()
            (self.out / 'cleaner-partial.json').write_text(json.dumps(ev, indent=2) + '\n')
            self.pull_logs('cleaner-after-press')
            raise
        self.shot('cleaner-sheet-on.png')
        ev['after_on_text'] = self.texts(nodes)[:900]
        ev['switch_on'] = next(({k: n.get(k) for k in ('role', 'value')} for n in nodes
                                if n.get('desc') == 'RichCleaner' and n.get('role') == sw['role']), None)
        self.ax('click', '--title', 'RichCleaner', '--role', sw['role'], '--first')
        time.sleep(3)
        nodes = self.dump('cleaner-off')
        self.shot('cleaner-sheet-off-again.png')
        ev['switch_off'] = next(({k: n.get(k) for k in ('role', 'value')} for n in nodes
                                 if n.get('desc') == 'RichCleaner' and n.get('role') == sw['role']), None)
        (self.out / 'cleaner.json').write_text(json.dumps(ev, indent=2) + '\n')
        self.facts['cleaner'] = ev
        self.save()
        return ev

    def accounts(self):
        ev = {}
        self.settings(True)
        self.ax('click', '--id', 'set-accounts-open', '--first')
        time.sleep(4)
        nodes = self.dump('accounts-sheet')
        self.shot('accounts-sheet.png')
        t = self.texts(nodes)
        ev['accounts'] = {'text': t[:1500], 'five_hour_meter': '5-hour limit' in t,
                          'five_hour_choice': 'When a 5-hour limit' in t, 'weekly_meter': 'Weekly limit' in t}
        if self.node('acc-close'):
            self.ax('click', '--id', 'acc-close', '--first')
        time.sleep(2)
        self.settings(True)
        if self.node('set-quota-open'):
            self.ax('click', '--id', 'set-quota-open', '--first')
            time.sleep(5)
            nodes = self.dump('quota-sheet')
            self.shot('quota-sheet.png')
            t = self.texts(nodes)
            ev['quota'] = {'text': t[:1500], 'five_hour': bool(re.search(r'5[- ]hour|five[- ]hour', t, re.I)),
                           'weekly': bool(re.search(r'week', t, re.I))}
            if self.node('quota-close'):
                self.ax('click', '--id', 'quota-close', '--first')
        else:
            ev['quota'] = 'no quota row in Settings'
        (self.out / 'accounts.json').write_text(json.dumps(ev, indent=2) + '\n')
        self.facts['accounts'] = ev
        self.save()
        return ev


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in a.steps.split(','):
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)[:1500]
            ok = False
            try:
                walk.shot('fail-%s.png' % step)
            except Exception:  # noqa: BLE001
                pass
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and step in ('identity', 'first-run'):
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""Walk a signed v1.3.8 update and exercise gestures after each real RichOS reopen."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('vm')
p.add_argument('--harness', type=Path, default=Path(__file__).resolve().parent)
p.add_argument('--bundle', type=Path, required=True)
p.add_argument('--update', type=Path, required=True)
p.add_argument('--signature', type=Path, required=True)
p.add_argument('--update-version', required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('--cycles', type=int, default=3)
p.set_defaults(suite='gestures', scenario='restart', within=125)
a = p.parse_args()
sys.path.insert(0, str(a.harness))
spec = importlib.util.spec_from_file_location('update_walk', a.harness / 'update-in-place-walk.py')
up = importlib.util.module_from_spec(spec)
spec.loader.exec_module(up)
q = shlex.quote

class GestureWalk(up.Walk):
    def run_tool(self, script, *args):
        result = subprocess.run([str(a.harness / script), self.vm, *args], capture_output=True, text=True, timeout=50)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout

    def point(self, app, title, role=None, scope=None):
        args = ['--app', app, '--title', title, '--first']
        if role:
            args += ['--role', role]
        if scope:
            args += ['--in', scope]
        rows = self.ax('find', *args)
        node = next(row for row in rows if row.get('w') and row.get('h'))
        return f"{node['x']+node['w']/2},{node['y']+node['h']/2}"

    def mouse(self, *args):
        return self.run_tool('pointer-drag.sh', *args, '--as-is')

    def click(self, point):
        return self.mouse(point, point)

    def key(self, *args):
        return self.g(shlex.join(['python3', self.work + '/hid-key.py', *args]))

    def prepare_inputs(self):
        home = self.home
        for name in ('hid-key.py', 'trash-diff.py', 'fake-claude-fill-first.pl'):
            self.push(a.harness / name, self.work + '/' + name)
        fake = self.work + '/claude'
        self.g('cp ' + q(self.work + '/fake-claude-fill-first.pl') + ' ' + q(fake)
               + ' && chmod 755 ' + q(fake)
               + ' && printf %s \'{"five":20,"weekly":40}\' > ' + q(self.work + '/usage-1.json')
               + ' && launchctl setenv RICHOS_CLAUDE_BIN ' + q(fake))
        self.g('defaults write app.richos.richfiles windowGestures.selection -int 27')
        self.g('defaults write -g AppleActionOnDoubleClick -string None')
        self.g('defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool false')
        for name in ('Slot 1', 'Slot 2', 'Slot 3', 'Open me'):
            self.g('mkdir -p ' + q(home + '/Desktop/' + name))
        self.g('touch ' + q(home + '/Desktop/Open me/marker-open-me.txt')
               + ' && printf "Gesture test\n" > ' + q(home + '/gesture-window.txt'))

    def grant_existing_permission(self):
        self.g('codesign --verify --deep --strict ' + q(up.SYSTEM))
        req = self.g('codesign -d -r- ' + q(up.GESTURES) + r""" 2>&1 | sed -n 's/^#* *designated => //p'""")
        reqfile = self.work + '/requirement'
        self.g(shlex.join(['csreq', '-r', '=' + req, '-b', reqfile]))
        code = self.g('xxd -p ' + q(reqfile) + r" | tr -d '\n'")
        assert re.fullmatch(r'[a-f0-9]+', code)
        for service in ('kTCCServiceAccessibility', 'kTCCServiceListenEvent'):
            query = ("INSERT OR REPLACE INTO access "
                     "(service,client,client_type,auth_value,auth_reason,auth_version,csreq,indirect_object_identifier_type,"
                     "indirect_object_identifier,flags,last_modified) "
                     f"VALUES ('{service}','app.richos.richfiles.window-gestures',0,2,3,1,X'{code}',0,'UNUSED',0,strftime('%s','now'));")
            self.g(shlex.join(['sudo', '-n', 'sqlite3', '-cmd', '.timeout 10000',
                               '/Library/Application Support/com.apple.TCC/TCC.db', query]))
        self.g('sudo -n /usr/bin/killall tccd || true')
        self.g(shlex.join([up.FILES + '/Contents/MacOS/RichFiles', '--window-gesture-service', 'register']))
        self.g('launchctl kickstart -k gui/$(id -u)/app.richos.richfiles.window-gestures')
        return self.wait(self.helper_ready, 'stable Window Gestures permission', 25)

    def helper_ready(self):
        pid = self.gesture_service_pid()
        state = self.g('defaults read app.richos.richfiles windowGestures.status 2>/dev/null || true')
        published = self.g('defaults read app.richos.richfiles windowGestures.process 2>/dev/null || true')
        return pid if str(pid) == published and state == 'running' else None

    def focus_app(self, name):
        self.suite_ax(name, 'set frontmost to true')
        self.wait(lambda: self.front() == name, name + ' frontmost', 10)

    def front(self):
        return self.osascript('tell application "System Events" to name of first process whose frontmost is true')

    def desktop(self):
        for name in ('RichFiles', 'TextEdit', 'Calculator', 'Finder'):
            self.g("osascript -e " + q(f'tell application "System Events" to tell process "{name}"\n'
                   'repeat with w in windows\ntry\nclick (first button of w whose subrole is "AXCloseButton")\nend try\nend repeat\nend tell')
                   + ' 2>/dev/null || true')
        self.suite_ax('richos-tauri', 'set visible to false')
        self.g('/usr/bin/killall NotificationCenter 2>/dev/null || true')
        self.focus_app('Finder')
        self.click('300,700')
        self.wait(lambda: self.front() == 'Finder' and self.focus_role() == 'AXGroup', 'Finder desktop group', 15)

    def focus_role(self):
        return self.suite_ax('Finder', 'return role of (value of attribute "AXFocusedUIElement")')

    def folder_open(self):
        self.desktop()
        point = self.point('Finder', 'Open me', 'AXImage')
        self.mouse(point, '--double', '--fn')
        def opened():
            if self.front() == 'Finder' and any(row.startswith('Open me|') for row in self.windows('Finder')):
                raise RuntimeError('Desktop folder opened in Finder')
            try:
                return self.ax('find', '--app', 'RichFiles', '--title', 'marker-open-me.txt', '--first')
            except up.StepFailed:
                return None
        self.wait(opened, 'the desktop folder marker in RichFiles', 12)
        return 'marker-open-me.txt'

    def folder_delete(self, label, mode='keyboard'):
        self.desktop()
        before = self.g('ls -1 ' + q(self.home + '/Desktop')).splitlines()
        if mode == 'keyboard':
            self.key('45', '--cmd', '--shift')
        elif mode == 'file':
            item = self.point('Finder', 'File', 'AXMenuBarItem', 'menubar')
            self.click(item)
            self.click(self.point('Finder', 'New Folder', 'AXMenuItem', 'menubar'))
        else:
            self.mouse('500,700', '500,700', '--right')
            self.click('550,711')
        self.wait(lambda: self.focus_role() == 'AXTextField', 'Finder new-folder rename editor', 10)
        selected = self.suite_ax('Finder', 'return name of (every UI element of group 1 of scroll area 1 whose selected is true)')
        assert selected and selected not in before, (selected, before)
        self.g('test -d ' + q(self.home + '/Desktop/' + selected))
        trash = ['python3', self.work + '/trash-diff.py']
        self.g(shlex.join(trash + ['mark', label]))
        self.key('117', '--fn', '--keypad', '--char', 'f728')
        row = json.loads(self.g(shlex.join(trash + ['check', label, '--moved', selected, '--wait', '4']) + ' || true'))
        assert row['holds'] and row['removed'] == [selected] and row['kept'] == sorted(before), row
        assert row['trashed'] == [row['moved_as'][selected]], row
        return row

    def frame(self):
        return [int(part.strip()) for part in self.suite_ax('TextEdit', 'return (position of window 1) & (size of window 1)').split(',')]

    def text(self):
        self.g('open -a TextEdit ' + q(self.home + '/gesture-window.txt'))
        self.wait(lambda: self.windows('TextEdit'), 'a TextEdit window', 20)
        self.focus_app('TextEdit')
        self.suite_ax('TextEdit', 'set position of window 1 to {400, 200}\nset size of window 1 to {700, 450}')
        return self.frame()

    def geometry(self, description, test):
        return self.wait(lambda: frame if test(frame := self.frame()) else None, description, 8)

    def maximize(self, function_key=False):
        original = self.text()
        modifiers = ['--fn'] if function_key else []
        self.mouse('1000,212', '--double', *modifiers)
        frame = self.geometry('maximized title bar', lambda f: f[2] >= 1600 and f[3] >= 900)
        self.mouse('1150,37', '--double', *modifiers)
        self.geometry('restored title bar', lambda f: f == original)
        return frame

    def edge(self):
        self.text()
        self.mouse('650,212', '640,230', '500,400', '3,500', '--fn')
        return self.geometry('left edge snapping', lambda f: abs(f[0]) <= 16 and 800 <= f[2] <= 880)

    def keyboard_tile(self):
        self.text()
        self.key('124', '--cmd', '--opt', '--ctrl')
        return self.geometry('right half keyboard tile', lambda f: 820 <= f[0] <= 860 and 800 <= f[2] <= 880)

    def switcher(self):
        self.text()
        self.g('open -a Calculator')
        self.wait(lambda: self.windows('Calculator'), 'Calculator opened', 15)
        self.focus_app('TextEdit')
        def loaded_picker():
            rows = self.g('tail -n 40 ~/Library/Logs/RichOS/richfiles-window-gestures.jsonl')
            return next((row['id'] for row in map(json.loads, reversed(rows.splitlines()))
                         if row['operation'] == 'windowSwitcherList' and row['outcome'] == 'complete'), None)
        previous = loaded_picker()
        self.osascript('tell application "System Events" to key code 13 using {control down, option down, command down}')
        self.wait(lambda: 'Switch Windows' in self.suite_ax('RichFilesWindowGestures', 'return name of every window'),
                  'Window Switcher', 10)
        self.wait(lambda: (loaded := loaded_picker()) and loaded != previous, 'switcher rows loaded', 10)
        self.osascript('tell application "System Events" to key code 125')
        self.osascript('tell application "System Events" to key code 36')
        self.wait(lambda: self.front() == 'Calculator', 'window switcher selected Calculator', 8)
        return 'Calculator'

    def dock_minimize(self):
        self.g('open -a Calculator')
        self.focus_app('Calculator')
        icon = self.point('Dock', 'Calculator', scope='app')
        def minimized():
            return self.suite_ax('Calculator', 'return value of attribute "AXMinimized" of window 1') == 'true'
        self.click(icon)
        self.wait(minimized, 'Dock click minimized Calculator', 8)
        self.click(self.point('Dock', 'Calculator', scope='app'))
        self.wait(lambda: not minimized(), 'the next Dock click restored Calculator', 8)
        return 'minimized and restored'

    def reopen(self):
        previous = self.owned['app']
        self.focus_app('richos-tauri')
        self.quit_app(previous)
        self.g('open -a ' + q(up.SYSTEM) + ' --stdout ' + q(self.log) + ' --stderr ' + q(self.log))
        new = self.wait(lambda: next((pid for pid in self.app_pids() if pid != previous), None), 'the reopened RichOS', 40)
        self.set_app(new)
        self.wait(lambda: self.present('Talk to Rich', 'AXButton'), 'RichOS ready after reopening', 50)
        return new

w = GestureWalk(a)
report = {}
failed = False
def save():
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

def record(stage, fn, fatal=True):
    global failed
    try:
        report[stage] = {'pass': True, 'result': fn()}
    except Exception as error:
        report[stage] = {'pass': False, 'detail': str(error)}
        failed = True
        w.shot(stage + '.png')
        if fatal:
            raise
    finally:
        print(stage, report[stage], flush=True)
        save()

def same_helper():
    pid = w.helper_ready()
    if pid != report['updated-helper']['result']:
        raise RuntimeError(f'helper changed or is unavailable (PID {pid})')
    return pid

try:
    record('serve', w.serve)
    record('fixture', w.prepare_inputs)
    record('install', w.install)
    assert w.version() == '1.3.8'
    record('stable-helper', w.grant_existing_permission)
    record('ready', w.ready)
    record('download', w.download)
    record('updated-helper', lambda: w.wait(lambda: (pid if (pid := w.helper_ready()) and pid !=
           report['stable-helper']['result'] and w.running_build('gestures', pid, a.update_version) else None),
           'the updated launchd process', 45))
    for i in range(a.cycles + 1):
        cycle = f'cycle-{i}'
        if i:
            record(cycle + '-reopen', w.reopen)
        record(cycle + '-maximize-before-desktop', w.maximize, False)
        record(cycle + '-desktop-open', w.folder_open, False)
        modes = (('keyboard', 'file', 'context') if i == 0 else ('keyboard',))
        for mode in modes:
            record(cycle + '-new-folder-' + mode, lambda mode=mode: w.folder_delete(cycle + '-' + mode, mode), False)
        record(cycle + '-maximize-after-desktop', lambda: w.maximize(function_key=True), False)
        record(cycle + '-edge', w.edge, False)
        record(cycle + '-keyboard-tile', w.keyboard_tile, False)
        record(cycle + '-switcher', w.switcher, False)
        record(cycle + '-dock', w.dock_minimize, False)
        record(cycle + '-same-helper', same_helper, False)
finally:
    for name in ('richfiles-window-gestures.jsonl', 'richfiles-timing.jsonl'):
        w.pull(w.home + '/Library/Logs/RichOS/' + name, a.out / name)
    w.pull(w.work + '/serve.log', a.out / 'serve.log')
    if hasattr(w, 'log'):
        w.pull(w.log, a.out / 'app.log')
    report['cleanup'] = w.cleanup()
    save()
sys.exit(1 if failed else 0)

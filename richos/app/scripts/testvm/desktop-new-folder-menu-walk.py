#!/usr/bin/env python3
"""Walk Finder New Folder and Forward Delete across menu bar modes and keyboard layouts.

Run with run-walk.py --no-app and pass a signed RichOS.app.tar.gz via --archive.
Use --mode Always to isolate the auto-hide regression.
Compile guest-input-source.swift with swiftc and pass --input-source-tool to
exercise the layout matrix, or add --keyboard-only to run only the keyboard checks.
"""
import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import time

from relaunch import guest

HERE = Path(__file__).resolve().parent
APP = '/Applications/RichOS.app'
FILES = APP + '/Contents/Applications/RichFiles.app'
HELPER = FILES + '/Contents/Library/LoginItems/RichFiles Window Gestures.app'
CLIENT = 'app.richos.richfiles.window-gestures'
MODES = ('Always', 'On Desktop Only', 'In Full Screen Only', 'Never')
# Physical key codes for N in each layout's Command map.
LAYOUTS = {'Russian': 45, 'Ukrainian': 45, 'Greek': 45, 'DVORAK-QWERTYCMD': 45, 'Dvorak': 37}


class Walk:
    def __init__(self, vm):
        self.vm = vm
        self.remote = '/Users/admin/testvm/' + vm + '/desktop-new-folder'
        self.desktop = '/Users/admin/Desktop'

    def g(self, *command):
        return guest(self.vm, shlex.join(command) if len(command) > 1 else command[0], 120)

    def tool(self, name, *args):
        result = subprocess.run([str(HERE / name), self.vm, *args], capture_output=True, text=True, timeout=60)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout

    def wait(self, description, probe, seconds=15):
        deadline = time.monotonic() + seconds
        last = None
        while time.monotonic() < deadline:
            try:
                if result := probe():
                    return result
            except (RuntimeError, StopIteration) as exc:
                last = str(exc)
            time.sleep(0.2)
        raise RuntimeError(f'{description} was not observed: {last}')

    def pointer(self, *args):
        return self.tool('pointer-drag.sh', *args, '--as-is')

    def ax(self, operation, app, *args):
        result = self.tool('ax.sh', operation, '--app', app, *args, '--json')
        return [json.loads(line) for line in result.splitlines() if line.startswith('{')]

    def finder(self, body):
        return self.g('osascript', '-e', 'tell application "System Events" to tell process "Finder"\n'
                      + body + '\nend tell')

    def install(self, archive):
        self.g('mkdir', '-p', self.remote)
        for source, name in ((archive, 'app.tar.gz'), (HERE / 'hid-key.py', 'hid-key.py'),
                             (HERE / 'trash-diff.py', 'trash-diff.py')):
            self.tool('guest.sh', '--push', str(source), self.remote + '/' + name)
        self.g('tar', '-xf', self.remote + '/app.tar.gz', '-C', '/Applications')
        self.g('codesign', '--verify', '--deep', '--strict', APP)
        requirement = self.g('codesign -d -r- ' + shlex.quote(HELPER)
                             + r""" 2>&1 | sed -n 's/^#* *designated => //p'""")
        path = self.remote + '/helper.csreq'
        self.g('csreq', '-r', '=' + requirement, '-b', path)
        blob = self.g('xxd -p ' + shlex.quote(path) + r" | tr -d '\n'")
        if not re.fullmatch(r'[0-9a-f]+', blob):
            raise RuntimeError('Missing signed helper designated requirement')
        for service in ('kTCCServiceAccessibility', 'kTCCServiceListenEvent'):
            sql = ('INSERT OR REPLACE INTO access '
                   '(service,client,client_type,auth_value,auth_reason,auth_version,csreq,'
                   'indirect_object_identifier_type,indirect_object_identifier,flags,last_modified) '
                   f"VALUES ('{service}','{CLIENT}',0,2,3,1,X'{blob}',0,'UNUSED',0,strftime('%s','now'));")
            self.g('sudo', '-n', 'sqlite3', '-cmd', '.timeout 10000',
                   '/Library/Application Support/com.apple.TCC/TCC.db', sql)
        self.g('sudo -n /usr/bin/killall tccd || true')
        if self.g(FILES + '/Contents/MacOS/RichFiles', '--window-gesture-service', 'register') != 'enabled':
            raise RuntimeError('The signed gesture helper could not register')
        self.g('launchctl kickstart -k gui/$(id -u)/' + CLIENT)
        self.wait('the launchd helper to be running', lambda: self.g(
            'defaults read app.richos.richfiles windowGestures.status 2>/dev/null || true') == 'running')
        self.g('defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool false')
        self.g('mkdir', self.desktop + '/Keep this folder')
        self.g('touch', self.desktop + '/Keep this file.txt')
        return self.g('/usr/libexec/PlistBuddy', '-c', 'Print :RichOSSourceRevision', HELPER + '/Contents/Info.plist')

    def set_mode(self, previous, wanted):
        self.g('open', 'x-apple.systempreferences:com.apple.ControlCenter-Settings.extension')
        def popup(value):
            return next(row for row in self.ax('find', 'System Settings', '--role', 'AXPopUpButton',
                                               '--value', value, '--max', '3000', '--first') if row.get('role'))
        self.wait('the Control Center menu bar preference', lambda: popup(previous))
        # Scroll the right pane to the setting so the popup menu is on screen.
        self.tool('pointer-drag.sh', '980,610', '--scroll', '-48', '--app', 'System Settings')
        if previous != wanted:
            self.ax('click', 'System Settings', '--role', 'AXPopUpButton', '--value', previous, '--max', '3000', '--first')
            self.wait(wanted + ' in the popup', lambda: self.ax(
                'find', 'System Settings', '--role', 'AXMenuItem', '--title', wanted, '--max', '3000', '--first'))
            self.ax('click', 'System Settings', '--role', 'AXMenuItem', '--title', wanted, '--max', '3000', '--first')
        evidence = self.wait('the selected menu bar mode', lambda: popup(wanted))
        self.g('osascript', '-e', 'tell application "System Events" to tell process "System Settings" '
               'to click (first button of window 1 whose subrole is "AXCloseButton")')
        return evidence

    def menu_point(self, title, role):
        row = next(row for row in self.ax('find', 'Finder', '--in', 'menubar', '--title', title,
                                          '--role', role, '--first') if row.get('w'))
        return f"{row['x'] + row['w'] / 2},{row['y'] + row['h'] / 2}"

    def focus_desktop(self):
        self.finder('repeat with w in windows\ntry\nclick (first button of w whose subrole is "AXCloseButton")'
                    '\nend try\nend repeat\nset frontmost to true')
        self.pointer('300,700', '300,700')
        self.wait('Finder Desktop focus', lambda: self.finder(
            'get role of (value of attribute "AXFocusedUIElement")') == 'AXGroup')
        return sorted(self.g('ls', '-1', self.desktop).splitlines())

    def folder(self, label):
        before = self.focus_desktop()
        self.pointer('126,0', '--hover')
        file = self.menu_point('File', 'AXMenuBarItem')
        self.pointer(file, file)
        new_folder = self.wait('Finder File > New Folder', lambda: self.menu_point('New Folder', 'AXMenuItem'))
        self.pointer(new_folder, new_folder)
        return dict(self.delete_new_folder(label, before), file_point=file, menu_point=new_folder)

    def selected(self):
        return self.finder('get name of (every UI element of group 1 of scroll area 1 whose selected is true)')

    def delete_new_folder(self, label, before):
        self.wait('the inline rename editor', lambda: self.finder(
            'get role of (value of attribute "AXFocusedUIElement")') == 'AXTextField')
        name = self.selected()
        if not name or name in before:
            raise RuntimeError(f'Finder did not select a newly created folder: {name!r}')
        self.g('test', '-d', self.desktop + '/' + name)
        return dict(self.delete_selection(label, name, before), folder=name, focused_editor_before_delete=True)

    def delete_selection(self, label, name, other_items):
        trash = ['python3', self.remote + '/trash-diff.py']
        self.g(*trash, 'mark', label)
        self.g('python3', self.remote + '/hid-key.py', '117', '--fn', '--keypad', '--char', 'f728')
        row = json.loads(self.g(shlex.join(trash + ['check', label, '--moved', name, '--wait', '4']) + ' || true'))
        passed = (row['holds'] and row['removed'] == [name] and row['kept'] == other_items
                  and row['trashed'] == [row['moved_as'][name]])
        return {'pass': passed, 'trash_diff': row}

    def keyboard(self, source_tool, layouts):
        tool = self.remote + '/guest-input-source'
        self.tool('guest.sh', '--push', str(source_tool), tool)
        self.g('chmod', '755', tool)
        hid = ['python3', self.remote + '/hid-key.py']
        results = {}
        for layout in layouts:
            source = 'com.apple.keylayout.' + layout
            self.g(tool, source)
            self.wait(f'{layout} to be selected', lambda: self.g(tool) == source)
            before = self.focus_desktop()
            if self.g(tool) != source:
                raise RuntimeError(f'{layout} was not retained when Finder took focus')
            # Let the selected input source translate the HID event.
            self.g(*hid, str(LAYOUTS[layout]), '--cmd', '--shift')
            folder = self.delete_new_folder(f'{layout}-n', before)
            row = {'input_source': source, 'new_folder': folder}
            if layout == 'Dvorak':
                row['physical_qwerty_n'] = self.dvorak_control(hid)
            results[layout] = row
            print(layout, row, flush=True)
        return results

    def dvorak_control(self, hid):
        before = self.focus_desktop()
        name = 'Keep this file.txt'
        row = next(row for row in self.ax('find', 'Finder', '--title', name,
                                          '--role', 'AXImage', '--first') if row.get('w'))
        point = f"{row['x'] + row['w'] / 2},{row['y'] + row['h'] / 2}"
        self.pointer(point, point)
        self.wait('the existing file selection', lambda: self.selected() == name)
        self.g(*hid, '45', '--cmd', '--shift')
        if self.selected() != name or sorted(self.g('ls', '-1', self.desktop).splitlines()) != before:
            raise RuntimeError('The Dvorak B shortcut changed the existing Desktop selection')
        return self.delete_selection('dvorak-b', name, [item for item in before if item != name])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('vm')
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=MODES, action='append')
    parser.add_argument('--input-source-tool', type=Path)
    parser.add_argument('--layout', choices=tuple(LAYOUTS), action='append')
    parser.add_argument('--keyboard-only', action='store_true')
    args = parser.parse_args()
    if (args.keyboard_only or args.layout) and not args.input_source_tool:
        parser.error('--keyboard-only and --layout need --input-source-tool')
    args.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(args.vm)
    report = {'helper_source': walk.install(args.archive), 'modes': {}}
    previous = 'In Full Screen Only'
    try:
        modes = () if args.keyboard_only else args.mode or MODES
        for i, mode in enumerate(modes):
            row = {'preference': walk.set_mode(previous, mode)}
            previous = mode
            row.update(walk.folder(f'file-menu-{i}'))
            report['modes'][mode] = row
            print(mode, row, flush=True)
            if not row['pass']:
                walk.tool('shot.sh', str(args.out / 'folder-remains.png'))
                return 1
        if args.input_source_tool:
            keyboard = walk.keyboard(args.input_source_tool, args.layout or LAYOUTS)
            report['keyboard'] = keyboard
            if any(not result['pass'] for row in keyboard.values() for key, result in row.items()
                   if key != 'input_source'):
                walk.tool('shot.sh', str(args.out / 'keyboard-failure.png'))
                return 1
    finally:
        (args.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        walk.tool('guest.sh', '--pull', '/Users/admin/Library/Logs/RichOS/richfiles-window-gestures.jsonl',
                  str(args.out / 'richfiles-window-gestures.jsonl'))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

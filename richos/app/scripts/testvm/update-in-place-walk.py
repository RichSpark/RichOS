#!/usr/bin/env python3
"""update-in-place-walk.py — an update reaches RichOS in /Applications through "Restart to update"
while RichFiles' Window Gestures login item runs from inside the app, and the pinned Dock item then
opens the new version from /Applications.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone. The guest's own account is used (its real home and Dock), because the updater only treats
/Applications as the install location for the account whose home holds its journal:

  run-walk.py --no-app --home EMPTY_DIR --engine none --report REPORT -- \\
      update-in-place-walk.py --out DIR --bundle ZIP --update ARCHIVE --update-version V \\
      --signature SIG

--bundle is the build under test: a zip holding RichOS.app, Developer ID signed by the same team as
the update (macOS lets an app replace another one in /Applications only then), built with
RICHOS_EXTRA_TAURI_CONFIG='{"plugins":{"updater":{"dangerousInsecureTransportProtocol":true,
"pubkey":"<the test key's public key>"}}}' so it fetches from 127.0.0.1 and trusts that key.
--update is a RichOS update archive (RichOS.app as a .tar.gz) of a newer version and --signature
its signature from `cargo tauri signer sign` with that test key. The guest serves both from
127.0.0.1, so what is downloaded, verified and staged is the app's own download path; nothing is
published anywhere.

The login item is started directly (`open -g` on its bundle inside the app), which is the same
executable at the same path that RichFiles registers with SMAppService.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65); the guest has only tart's
null speaker, so nothing is heard:
  serve      the update and its manifest served by a python http.server on 127.0.0.1
  install    the bundle at /Applications/RichOS.app, opened once; the Dock item kept
  gestures   the Window Gestures login item started from inside /Applications/RichOS.app
  ready      the app offers the update ("... is available")
  download   "Download update" pressed; the app restarts by itself into the new version, which
             runs from /Applications; nothing is created at ~/Applications/RichOS.app; the login
             item is still running from inside the app
  dock       the app quit from its menu; the kept Dock item clicked; it opens the new version
             from /Applications

Everything this walk starts is stopped by its recorded pid before it returns (CEO §54).
Exit 0 when every step passes.
"""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

SYSTEM = '/Applications/RichOS.app'
EXE = SYSTEM + '/Contents/MacOS/richos-tauri'
GESTURES = SYSTEM + '/Contents/Applications/RichFiles.app/Contents/Library/LoginItems/RichFiles Window Gestures.app'
PORT = 8765
STEPS = ['serve', 'install', 'gestures', 'ready', 'download', 'dock']
CUE_ROLE = 'AXPopUpButton'  # the update cue is a <button aria-haspopup="true"> (update-relaunch-walk.py)


class StepFailed(Exception):
    pass


def q(text):
    return shlex.quote(str(text))


class Walk:
    def __init__(self, a):
        self.a = a
        self.vm = a.vm
        self.out = a.out
        self.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.state = state
        self.payload = (state / 'payload').read_text().strip()
        if not self.payload.startswith('/Users/') or '/testvm/' not in self.payload:
            raise ValueError('not a guest payload: ' + self.payload)
        self.work = self.payload + '/update-in-place'
        self.home = guest(self.vm, 'cd ~ && pwd -P')
        self.owned = {}  # name -> pid, everything this walk started in the guest
        self.facts = {}

    # --- the guest ---------------------------------------------------------------------------
    def g(self, command, timeout=60):
        return guest(self.vm, command, timeout)

    def push(self, src, dest):
        subprocess.run([str(HERE / 'guest.sh'), self.vm, '--push', str(src), dest], check=True, timeout=300)

    def pull(self, src, dest):
        subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', src, str(dest)], check=False, timeout=300)

    def app_pids(self):
        """The RichOS app processes running /Applications/RichOS.app's executable: parent launchd
        and no arguments. The relaunch and at-quit helpers run the same executable with their
        own --richos-internal-... argument and are reparented to launchd once the app is gone."""
        pids = []
        for line in self.g('ps -axo pid=,ppid=,args=').splitlines():
            parts = line.split()
            if len(parts) >= 3 and parts[1] == '1' and parts[2] in (EXE, '/System/Volumes/Data' + EXE) \
                    and all(arg.startswith('-psn') for arg in parts[3:]):
                pids.append(int(parts[0]))
        return pids

    def set_app(self, pid):
        """The app this walk owns now; ax.sh reads the run's app.pid to find the window."""
        self.owned['app'] = pid
        (self.state / 'app.pid').write_text('%d\n' % pid)

    def helpers(self):
        """The update helpers running the app's executable, as ps lists them."""
        return [l.strip() for l in self.g('ps -axo pid=,ppid=,args=').splitlines() if '--richos-internal' in l]

    def gesture_pids(self):
        exe = GESTURES + '/Contents/MacOS/RichFilesWindowGestures'
        return [int(l.split(None, 1)[0]) for l in self.g('ps -axo pid=,comm=').splitlines()
                if l.split(None, 1)[1:] and l.split(None, 1)[1] in (exe, '/System/Volumes/Data' + exe)]

    def version(self, app=SYSTEM):
        return self.g('/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" '
                      + q(app + '/Contents/Info.plist') + ' 2>/dev/null || true')

    def wait(self, read, what, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            value = read()
            if value:
                return value
            time.sleep(0.5)
        raise StepFailed('timed out after %ds waiting for %s' % (seconds, what))

    def shot(self, name):
        subprocess.run([str(HERE / 'shot.sh'), self.vm, str(self.out / name)], check=False, timeout=60)

    def ax(self, mode, *args, timeout=150):
        r = subprocess.run([str(HERE / 'ax.sh'), self.vm, mode, *args, '--json'],
                           capture_output=True, text=True, timeout=timeout)
        if r.returncode:
            raise StepFailed((r.stderr or r.stdout).strip()[-600:])
        return [json.loads(s) for s in r.stdout.splitlines() if s.startswith('{')]

    def present(self, title, role, contains=True):
        try:
            return bool(self.ax('find', '--title', title, '--role', role, *(['--contains'] if contains else []),
                                '--first'))
        except StepFailed:
            return False

    def press(self, title, role='AXButton', contains=True):
        return self.ax('click', '--title', title, '--role', role, *(['--contains'] if contains else []), '--first')

    def osascript(self, source, timeout=30):
        return self.g('osascript -e ' + q(source), timeout)

    def dock(self, body):
        return self.osascript('tell application "System Events" to tell process "Dock"\n'
                              'set icon to first UI element of list 1 whose name is "RichOS"\n'
                              + body + '\nend tell')

    def click_dock(self, right=False):
        raw = self.dock('return (position of icon) & (size of icon)')
        x, y, w, h = (int(part.strip()) for part in raw.split(','))
        point = [x + w // 2, y + h // 2]
        params = {'asis': True, 'right': right, 'points': [point, point], 'rest': .1}
        script = 'var DRAG_PARAMS = ' + json.dumps(params) + ';\n' + (HERE / 'pointer-drag.js').read_text()
        r = subprocess.run([str(HERE / 'guest.sh'), self.vm, 'osascript -l JavaScript -'], input=script,
                           capture_output=True, text=True, timeout=60)
        return r.stdout.strip()

    def pinned(self):
        # The Dock's preferences hold bookmark data, which JSON cannot carry: read the XML plist.
        out = self.g('defaults export com.apple.dock - 2>/dev/null || true')
        try:
            prefs = plistlib.loads(out.encode())
        except (ValueError, plistlib.InvalidFileException):
            return None
        return next((t for t in prefs.get('persistent-apps', [])
                     if t.get('tile-data', {}).get('file-label') == 'RichOS'), None)

    def quit_app(self, pid):
        self.osascript('tell application "System Events" to tell (first process whose unix id is %d) to '
                       'click menu item "Quit RichOS" of menu 1 of menu bar item 2 of menu bar 1' % pid)
        self.wait(lambda: pid not in self.app_pids(), 'RichOS (pid %d) to quit' % pid, 60)

    # --- steps ------------------------------------------------------------------------------
    def serve(self):
        self.g('mkdir -p ' + q(self.work + '/serve'))
        archive = self.work + '/serve/update.tar.gz'
        self.push(self.a.update, archive)
        manifest = {
            'version': self.a.update_version,
            'notes': 'update-in-place walk',
            'pub_date': '2026-11-19T00:00:00Z',
            'platforms': {key: {'signature': self.a.signature.read_text().strip(),
                                'url': 'http://127.0.0.1:%d/update.tar.gz' % PORT}
                          for key in ('darwin-aarch64', 'darwin-aarch64-app')},
        }
        local = self.out / 'latest.json'
        local.write_text(json.dumps(manifest, indent=1) + '\n')
        self.push(local, self.work + '/serve/latest.json')
        pid = self.g('cd ' + q(self.work + '/serve') + ' && nohup python3 -m http.server %d --bind 127.0.0.1 '
                     '</dev/null >' % PORT + q(self.work + '/serve.log') + ' 2>&1 & echo $!')
        self.owned['server'] = int(pid)
        self.wait(lambda: '"version"' in self.g('curl -s http://127.0.0.1:%d/latest.json || true' % PORT),
                  'the update server', 20)
        return {'server_pid': int(pid), 'endpoint': 'http://127.0.0.1:%d/latest.json' % PORT}

    def install(self):
        if self.g('test -e %s -o -e %s && echo yes || true' % (q(SYSTEM), q(self.home + '/Applications/RichOS.app'))):
            raise StepFailed('fresh guest required: a RichOS copy already exists')
        self.push(self.a.bundle, self.work + '/bundle.zip')
        self.g('ditto -x -k ' + q(self.work + '/bundle.zip') + ' /Applications', 180)
        self.facts['version_before'] = self.version()
        # A Dock-launched app inherits launchd's environment: the endpoint, and no Claude updater.
        for key, value in (('RICHOS_UPDATE_ENDPOINT', 'http://127.0.0.1:%d/latest.json' % PORT),
                           ('DISABLE_AUTOUPDATER', '1'), ('RICHOS_ACTIVATION', 'regular')):
            self.g('launchctl setenv %s %s' % (key, q(value)))
        # The first start carries the endpoint itself and keeps its output; the relaunch helper
        # passes both on to the app it starts again.
        self.log = self.work + '/app.log'
        self.g('open -a ' + q(SYSTEM) + ' --env ' + q('RICHOS_UPDATE_ENDPOINT=http://127.0.0.1:%d/latest.json' % PORT)
               + ' --stdout ' + q(self.log) + ' --stderr ' + q(self.log))
        pid = self.wait(lambda: (self.app_pids() or [None])[-1], 'RichOS to start from /Applications', 90)
        self.set_app(pid)
        self.wait(lambda: self.dock('return name of icon') == 'RichOS', 'the Dock icon', 60)
        if not self.pinned():
            # The Dock menu's own "Keep in Dock", as a user pins it (Codex's dock fixture).
            self.click_dock(right=True)

            def menu():
                try:
                    return 'Options' in self.dock('return name of menu items of menu 1 of icon')
                except RuntimeError:
                    return False
            self.wait(menu, 'the Dock menu', 20)
            self.dock('click menu item "Keep in Dock" of menu 1 of menu item "Options" of menu 1 of icon')
        pin = self.wait(self.pinned, 'the kept Dock item', 30)
        url = pin['tile-data']['file-data']['_CFURLString']
        if url.rstrip('/') != 'file://' + SYSTEM:
            raise StepFailed('the Dock item points at ' + url)
        self.facts['pin_guid'] = pin.get('GUID')
        return {'pid': pid, 'version': self.facts['version_before'], 'pin': url, 'guid': pin.get('GUID')}

    def gestures(self):
        self.g('open -g ' + q(GESTURES))
        pid = self.wait(lambda: (self.gesture_pids() or [None])[-1], 'the Window Gestures login item', 30)
        self.owned['gestures'] = pid
        return {'pid': pid, 'running_from': GESTURES}

    def type_into(self, text, *selector):
        # ax.sh type pastes through the guest clipboard and refuses one that holds no text.
        subprocess.run([str(HERE / 'ax.sh'), self.vm, 'set the clipboard to "seed"'], check=True, timeout=60)
        return self.ax('type', text, *selector, '--contains', '--first', '--replace')

    def first_run(self):
        """adopt-walk.py's first run, in this account's real home: the company question answered
        with a fresh folder, the questions declined."""
        company = self.home + '/Acme'
        self.g('mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md && git init -q '
               '&& git add README.md && git -c user.name=QA -c user.email=qa@example.invalid commit -q -m init; fi'
               .format(c=q(company)))
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        self.facts['first_run'] = True

    def ready(self):
        def offered():
            if self.present('is available', CUE_ROLE):
                return True
            if self.present('Add this company', 'AXButton') and not self.facts.get('first_run'):
                self.first_run()
            elif self.present('Not now', 'AXButton'):
                self.press('Not now')
            elif self.present('setting up to do first', 'AXGroup'):
                # A build without an engine pin offers only Close on the setup sheet, and its
                # button has no accessible name; Escape presses whichever button the sheet shows.
                subprocess.run([str(HERE / 'ax.sh'), self.vm, '--key', '53'], capture_output=True, timeout=60)
            return False
        try:
            self.wait(offered, 'the update to be offered', self.a.within)
        except StepFailed:
            self.shot('1-not-offered.png')
            tree = subprocess.run([str(HERE / 'ax.sh'), self.vm, 'tree', '--depth', '12'],
                                  capture_output=True, text=True, timeout=150)
            (self.out / '1-not-offered-tree.txt').write_text(tree.stdout + tree.stderr)
            raise
        self.shot('1-available.png')
        return {'offered': True, 'first_run': bool(self.facts.get('first_run'))}

    def download(self):
        old = self.owned['app']
        self.press('is available', role=CUE_ROLE)
        self.wait(lambda: self.present('Download update', 'AXButton'), 'Download update', 30)
        pressed = time.monotonic()
        self.press('Download update')
        seen = set()

        def watch(read):
            # The helpers the quitting app leaves behind, as they come and go.
            seen.update(self.helpers())
            return read()
        self.wait(lambda: watch(lambda: old not in self.app_pids()), 'the app to quit for the restart', self.a.within)
        new = self.wait(lambda: watch(lambda: next((p for p in self.app_pids() if p != old), None)),
                        'the app to start again by itself', self.a.within)
        self.set_app(new)
        relaunched = round(time.monotonic() - pressed, 1)
        self.wait(lambda: self.version() == self.a.update_version, 'the new version in /Applications', 120)
        self.shot('2-after-restart.png')
        user_copy = self.g('ls -ld ' + q(self.home + '/Applications/RichOS.app') + ' 2>/dev/null || true')
        if user_copy:
            raise StepFailed('a copy appeared at ~/Applications/RichOS.app: ' + user_copy)
        if self.owned['gestures'] not in self.gesture_pids() and not self.g(
                'kill -0 %d 2>/dev/null && echo alive || true' % self.owned['gestures']):
            raise StepFailed('the login item stopped during the update')
        timing = self.g('cat ' + q(self.home + '/Library/Application Support/com.richos.app/update-timing.log')
                        + ' 2>/dev/null || true')
        (self.out / 'update-timing.log').write_text(timing + '\n')
        launch = self.g('cat ' + q(self.home + '/Library/Logs/RichOS/launch.log') + ' 2>/dev/null || true')
        return {'new_pid': new, 'restart_s': relaunched, 'version': self.version(),
                'helpers_seen': sorted(seen),
                'publication': [l for l in timing.splitlines() if 'phase=publication' in l][-3:],
                'relaunched_launch_line': next((l for l in reversed(launch.splitlines()) if 'pid %d ' % new in l), '')}

    def dock_step(self):
        self.quit_app(self.owned['app'])
        self.click_dock()
        pid = self.wait(lambda: (self.app_pids() or [None])[-1], 'the kept Dock item to open RichOS', 90)
        self.set_app(pid)
        exe = self.g('ps -p %d -o comm=' % pid)
        line = self.wait(lambda: next((l for l in reversed(self.g(
            'cat ' + q(self.home + '/Library/Logs/RichOS/launch.log') + ' 2>/dev/null || true').splitlines())
            if 'pid %d ' % pid in l), ''), 'the Dock launch record', 60)
        pin = self.pinned()
        self.shot('3-dock-open.png')
        if self.version() != self.a.update_version:
            raise StepFailed('/Applications holds ' + self.version())
        if pin is None or pin.get('GUID') != self.facts['pin_guid']:
            raise StepFailed('the kept Dock item changed: ' + repr(pin))
        if 'hand-over' in line:
            raise StepFailed('the Dock launch was handed over: ' + line)
        return {'pid': pid, 'executable': exe, 'launch_line': line, 'guid': pin.get('GUID')}

    # --- cleanup ----------------------------------------------------------------------------
    def cleanup(self):
        done = {}
        for log in ('Library/Logs/RichOS/launch.log',
                    'Library/Application Support/com.richos.app/update-timing.log'):
            self.pull(self.home + '/' + log, self.out / Path(log).name)
        if getattr(self, 'log', None):
            self.pull(self.log, self.out / 'app.log')
        app = self.owned.get('app')
        if app and app in self.app_pids():
            try:
                self.quit_app(app)
            except (StepFailed, RuntimeError, subprocess.SubprocessError):
                self.g('kill -TERM %d 2>/dev/null || true' % app)
        for name in ('gestures', 'server'):
            pid = self.owned.get(name)
            if pid:
                self.g('kill -TERM %d 2>/dev/null || true' % pid)
        for name, pid in self.owned.items():
            done[name] = {'pid': pid, 'gone': not self.g('kill -0 %d 2>/dev/null && echo alive || true' % pid)}
        return done


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--bundle', type=Path, required=True, help='zip holding the RichOS.app under test')
    p.add_argument('--update', type=Path, required=True, help='signed update archive (RichOS.app .tar.gz)')
    p.add_argument('--update-version', required=True)
    p.add_argument('--signature', type=Path, required=True, help="the update archive's updater signature file")
    p.add_argument('--within', type=float, default=300, help='seconds for each wait on the app')
    a = p.parse_args()
    walk = Walk(a)
    report = {'vm': a.vm, 'update_version': a.update_version, 'steps': []}
    ok = True
    try:
        for step in STEPS:
            began = time.monotonic()
            row = {'step': step}
            try:
                row['evidence'] = getattr(walk, 'dock_step' if step == 'dock' else step)()
                row['outcome'] = 'PASS'
            except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                row['outcome'] = 'FAIL'
                row['detail'] = str(exc)
                ok = False
            row['seconds'] = round(time.monotonic() - began, 1)
            report['steps'].append(row)
            (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''),
                  flush=True)
            if not ok:
                break
    finally:
        report['cleanup'] = walk.cleanup()
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print('cleanup:', json.dumps(report['cleanup']), flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

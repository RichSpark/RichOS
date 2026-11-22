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

--scenario picks the case (default restart, the steps above):

  reinstall  (review rv-20261119T144657Z finding 1) a copy the old updater left in
             ~/Applications, and the same version then installed into /Applications by hand.
    legacy     --stage-tool (the updater crate's `guest-stage` example, built for the guest)
               puts --bundle-archive (the build under test as an update archive, version
               --bundle-version) in place at ~/Applications with its publication receipt,
               through the updater's own staging and publication, as an older RichOS did
    install    the same build then installed at /Applications and opened, as above
    ready      as above
    download   as above, and the update is put in place at /Applications, not left pending:
               ~/Applications/RichOS.app is now a link to /Applications/RichOS.app and nothing
               is prepared any more
    dock       as above

  try-again  (finding 2) an update whose swap into /Applications is refused, permission then
             restored and Try again pressed in the Updates row.
    install, ready  as above (the update is offered, nothing downloaded yet)
    stage      the app quit; the app's own download path (RICHOS_UPDATE_SELFTEST=install) stages
               the update; the staged copy is made immutable, so the swap is refused with the
               same EPERM macOS returns when permission is denied
    denied     the app opened again: its launch-time swap is refused and the Updates row says
               "RichOS could not prepare this update." with the permission sentence as its reason
    retry      Try again pressed first with the flag still set: the refusal stays and nothing reads
               ready; then the flag cleared (permission restored) and Try again pressed again: the
               row no longer shows the permission sentence and the update reads as ready
    publish    the app quit from its menu: the update is put in place at /Applications as it
               closes and the saved denial is gone

--suite legacy checks that running pre-contract suite applications are left alone.
--suite restore checks that both RichFiles windows and the RichGlance viewer return
with their exact placements and that new processes report the new build.
--suite editor verifies the screenshot editor restores without taking keyboard focus.
--suite busy adds a cross-volume RichFiles copy, requires the old process to defer
and verifies the completed copy before accepting the restart.

Everything this walk starts is stopped before it returns (CEO §54).
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
FILES = SYSTEM + '/Contents/Applications/RichFiles.app'
GLANCE = FILES + '/Contents/Applications/RichGlance.app'
SUITE_LOG = 'Library/Logs/RichOS/richfiles-timing.jsonl'
VIEWER_IMAGE = HERE.parents[2] / 'tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.bmp'
EXE = SYSTEM + '/Contents/MacOS/richos-tauri'
GESTURES = SYSTEM + '/Contents/Applications/RichFiles.app/Contents/Library/LoginItems/RichFiles Window Gestures.app'
PORT = 8765
STEPS = {
    'restart': ['serve', 'install', 'gestures', 'ready', 'download', 'dock'],
    'reinstall': ['serve', 'legacy', 'install', 'ready', 'download', 'dock'],
    'try-again': ['serve', 'install', 'ready', 'stage', 'denied', 'retry', 'publish'],
}
CUE_ROLE = 'AXPopUpButton'  # the update cue is a <button aria-haspopup="true"> (update-relaunch-walk.py)
PERMISSION = 'needs permission to replace the existing app in Applications'
NOT_PREPARED = 'could not prepare this update'


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
        self.user_app = self.home + '/Applications/RichOS.app'
        self.journal = self.home + '/Applications/.richos-updater'
        self.owned = {}  # name -> pid, everything this walk started in the guest
        self.facts = {}
        self.immutable = None  # the staged copy this walk made immutable, cleared at cleanup
        self.endpoint_base = ('https' if a.suite else 'http') + '://127.0.0.1:%d' % PORT

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
                                'url': self.endpoint_base + '/update.tar.gz'}
                          for key in ('darwin-aarch64', 'darwin-aarch64-app')},
        }
        local = self.out / 'latest.json'
        local.write_text(json.dumps(manifest, indent=1) + '\n')
        self.push(local, self.work + '/serve/latest.json')
        folder = self.work + '/serve'
        command = 'python3 -m http.server %d --bind 127.0.0.1' % PORT
        if self.a.suite:
            # The published updater requires HTTPS. Trust this certificate only in this guest.
            cert = self.work + '/testvm-root.pem'
            key = self.work + '/testvm-key.pem'
            config = self.work + '/tls.cnf'
            self.g("umask 077; printf '%s' " + q(
                '[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n'
                '[dn]\nCN=RichOS disposable test VM\n[ext]\nsubjectAltName=IP:127.0.0.1\n'
                'basicConstraints=critical,CA:true\nkeyUsage=critical,digitalSignature,keyCertSign\n'
            ) + ' > ' + q(config)
                + ' && openssl req -x509 -newkey rsa:2048 -nodes -days 1 -config ' + q(config)
                + ' -keyout ' + q(key) + ' -out ' + q(cert) + ' 2>/dev/null'
                + ' && sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ' + q(cert))
            server = (
                'import http.server, ssl, sys\n'
                'server = http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), '
                'http.server.SimpleHTTPRequestHandler)\n'
                'context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)\n'
                'context.load_cert_chain(sys.argv[2], sys.argv[3])\n'
                'server.socket = context.wrap_socket(server.socket, server_side=True)\n'
                'server.serve_forever()\n'
            )
            command = 'python3 -c %s %d %s %s' % (q(server), PORT, q(cert), q(key))
        pid = self.g('cd ' + q(folder) + ' && { nohup ' + command + ' </dev/null >'
                     + q(self.work + '/serve.log') + ' 2>&1 & echo $!; }')
        self.owned['server'] = int(pid)
        self.wait(lambda: '"version"' in self.g('curl -fsS ' + q(self.endpoint_base + '/latest.json') + ' || true'),
                  'the update server', 20)
        return {'server_pid': int(pid), 'endpoint': self.endpoint_base + '/latest.json'}

    def receipt(self, name):
        """One of the updater's own journal documents, read only; None when absent."""
        raw = self.g('cat ' + q(self.journal + '/' + name) + ' 2>/dev/null || true')
        return json.loads(raw) if raw else None

    def legacy(self):
        if self.g('test -e %s -o -e %s && echo yes || true' % (q(SYSTEM), q(self.user_app))):
            raise StepFailed('fresh guest required: a RichOS copy already exists')
        self.g('mkdir -p ' + q(self.work))
        tool, archive = self.work + '/guest-stage', self.work + '/bundle.tar.gz'
        self.push(self.a.stage_tool, tool)
        self.push(self.a.bundle_archive, archive)
        out = self.g('chmod 755 %s && %s %s %s %s 2>&1' % (q(tool), q(tool), q(self.home), q(self.a.bundle_version),
                                                          q(archive)), 300)
        published = self.receipt('published.json') or {}
        if self.version(self.user_app) != self.a.bundle_version or published.get('version') != self.a.bundle_version:
            raise StepFailed('no published copy at ~/Applications: ' + out)
        return {'tool': out, 'user_copy': self.version(self.user_app), 'receipt': published.get('version')}

    def install(self):
        legacy = self.a.scenario == 'reinstall'
        if self.g('test -e %s && echo yes || true' % q(SYSTEM)) or (
                not legacy and self.g('test -e %s && echo yes || true' % q(self.user_app))):
            raise StepFailed('fresh guest required: a RichOS copy already exists')
        self.push(self.a.bundle, self.work + '/bundle.zip')
        self.g('ditto -x -k ' + q(self.work + '/bundle.zip') + ' /Applications', 180)
        self.facts['version_before'] = self.version()
        # A Dock-launched app inherits launchd's environment: the endpoint, and no Claude updater.
        for key, value in (('RICHOS_UPDATE_ENDPOINT', self.endpoint_base + '/latest.json'),
                           ('DISABLE_AUTOUPDATER', '1'), ('RICHOS_ACTIVATION', 'regular')):
            self.g('launchctl setenv %s %s' % (key, q(value)))
        # The first start carries the endpoint itself and keeps its output; the relaunch helper
        # passes both on to the app it starts again.
        self.log = self.work + '/app.log'
        self.g('open -a ' + q(SYSTEM) + ' --env ' + q('RICHOS_UPDATE_ENDPOINT=' + self.endpoint_base + '/latest.json')
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

    def gesture_service_pid(self):
        target = 'gui/$(id -u)/app.richos.richfiles.window-gestures'
        status = self.g('launchctl print ' + target + ' 2>/dev/null || true')
        pid = next((int(line.partition('=')[2].strip()) for line in status.splitlines()
                    if line.strip().startswith('pid =')), None)
        # launchd presents registered login items under their service label in ps.
        # Check the job's PID and use its own diagnostics to prove the running build.
        return pid if pid and self.g('kill -0 %d 2>/dev/null && echo alive || true' % pid) else None

    def gestures(self):
        if self.a.suite:
            pid = self.wait(self.gesture_service_pid, 'the registered Window Gestures login item', 90)
        else:
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

    def clear_sheets(self):
        """Answer whichever first-run sheet covers the window, one at a time."""
        if self.present('Add this company', 'AXButton') and not self.facts.get('first_run'):
            self.first_run()
        elif self.present('Not now', 'AXButton'):
            self.press('Not now')
        elif self.present('setting up to do first', 'AXGroup'):
            # A build without an engine pin offers only Close on the setup sheet, and its
            # button has no accessible name; Escape presses whichever button the sheet shows.
            subprocess.run([str(HERE / 'ax.sh'), self.vm, '--key', '53'], capture_output=True, timeout=60)

    def ready(self):
        def offered():
            if self.present('is available', CUE_ROLE):
                return True
            self.clear_sheets()
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

    def suite_pids(self, name):
        bundle = FILES if name == 'RichFiles' else GLANCE
        executable = bundle + '/Contents/MacOS/' + name
        return [int(pid) for pid, path in (line.strip().split(None, 1)
                for line in self.g('ps -axo pid=,comm=').splitlines() if line.strip())
                if path in (executable, '/System/Volumes/Data' + executable)]

    def suite_ax(self, name, body):
        return self.osascript('tell application "System Events" to tell process "%s"\n' % name
                              + body + '\nend tell')

    def windows(self, name):
        rows = self.suite_ax(name, '''
set resultRows to {}
repeat with w in windows
    set {leftEdge, topEdge} to position of w
    set {windowWidth, windowHeight} to size of w
    set end of resultRows to (name of w & "|" & leftEdge & "|" & topEdge & "|" & windowWidth & "|" & windowHeight)
end repeat
set AppleScript's text item delimiters to linefeed
return resultRows as text
''')
        return sorted(row for row in rows.splitlines() if row.count('|') == 4)

    def suite_open(self):
        self.source = self.work + '/Source'
        self.destination = self.work + '/Destination'
        self.g('mkdir -p ' + q(self.source) + ' ' + q(self.destination))
        self.push(VIEWER_IMAGE, self.source + '/Suite-Walk.bmp')
        if self.a.suite == 'busy':
            # Cross-volume copy exercises the real transfer. A clone on APFS can
            # otherwise finish before the update reaches the other application.
            disk = self.work + '/Copy.sparseimage'
            self.mount = self.work + '/copy-volume'
            self.g('mkdir -p ' + q(self.mount) + ' && hdiutil create -quiet -type SPARSE -fs APFS '
                   '-size 16g -volname SuiteCopy ' + q(disk)
                   + ' && hdiutil attach -quiet -nobrowse -mountpoint ' + q(self.mount) + ' ' + q(disk), 120)
            self.destination = self.mount + '/Destination'
            self.g('mkdir -p ' + q(self.destination) + ' && dd if=/dev/zero of='
                   + q(self.source + '/Suite Copy.bin') + ' bs=8m count=1024 2>/dev/null', 180)
        self.g('open -a ' + q(FILES) + ' ' + q(self.source))
        files_pid = self.wait(lambda: next(iter(self.suite_pids('RichFiles')), None), 'RichFiles', 40)
        self.owned['RichFiles-before'] = files_pid
        self.wait(lambda: any(row.startswith('Source') for row in self.windows('RichFiles')), 'Source window', 30)
        self.suite_ax('RichFiles', 'set frontmost to true\nkeystroke "n" using command down')
        self.wait(lambda: len(self.windows('RichFiles')) == 2, 'the second RichFiles window', 30)
        self.g('open -a ' + q(FILES) + ' ' + q(self.destination))
        self.wait(lambda: any(row.startswith('Destination') for row in self.windows('RichFiles')),
                  'Destination window', 30)
        self.suite_ax('RichFiles', 'set position of (first window whose name begins with "Source") to {55, 100}\n'
                                  'set position of (first window whose name begins with "Destination") to {520, 210}')
        image_argument = '' if self.a.suite == 'editor' else ' ' + q(self.source + '/Suite-Walk.bmp')
        self.g('open -a ' + q(GLANCE) + image_argument)
        glance_pid = self.wait(lambda: next(iter(self.suite_pids('RichGlance')), None), 'RichGlance', 30)
        self.owned['RichGlance-before'] = glance_pid
        self.wait(lambda: len(self.windows('RichGlance')) == 1, 'the RichGlance window', 30)
        self.facts['suite_windows'] = {name: self.windows(name) for name in ('RichFiles', 'RichGlance')}
        if self.a.suite != 'legacy':
            for name, pid in (('RichFiles', files_pid), ('RichGlance', glance_pid)):
                self.wait(lambda: self.running_build(name, pid, self.facts['version_before']),
                          name + ' to report the original running version', 30)
        self.shot('suite-before.png')
        if self.a.suite == 'editor':
            self.suite_ax('RichOS', 'set frontmost to true')
        if self.a.suite == 'busy':
            self.prepare_suite_copy()
        return {'pids': {name: self.owned[name + '-before'] for name in self.facts['suite_windows']},
                'windows': self.facts['suite_windows']}

    def suite_events(self):
        data = self.g('cat ' + q(self.home + '/' + SUITE_LOG) + ' 2>/dev/null || true')
        return [json.loads(line) for line in data.splitlines() if line.startswith('{')]

    def running_build(self, name, pid, version):
        bundle_ids = {'RichFiles': 'app.richos.richfiles', 'RichGlance': 'app.richos.richglance',
                      'gestures': 'app.richos.richfiles.window-gestures'}
        return next((row for row in self.suite_events()
                     if row.get('operation') == 'suiteRestartRunning' and row.get('pid') == pid
                     and row.get('application') == bundle_ids[name]
                     and row.get('build') == version), None)

    def prepare_suite_copy(self):
        self.suite_ax('RichFiles', 'set frontmost to true\n'
                      'perform action "AXRaise" of (first window whose name begins with "Source")')
        # ax.sh also emits JSON timing rows. Select the element's geometry.
        matches = self.ax('find', '--app', 'RichFiles', '--title', 'Suite Copy.bin')
        file = next((row for row in matches if row.get('w') and row.get('h')), None)
        if not file:
            raise StepFailed('the copy fixture has no visible accessibility row')
        point = '%d,%d' % (file['x'] + min(14, file['w'] / 2), file['y'] + file['h'] / 2)
        self.ax('click', '--app', 'RichFiles', '--at', point)
        self.suite_ax('RichFiles', 'click menu item "Copy Files" of menu 1 of menu bar item "Files" of menu bar 1')
        clipboard = self.osascript('return POSIX path of (the clipboard as «class furl»)')
        if clipboard != self.source + '/Suite Copy.bin':
            raise StepFailed('RichFiles did not put the copy fixture on the clipboard')
        self.suite_ax('RichFiles', 'perform action "AXRaise" of (first window whose name begins with "Destination")')
        if not self.suite_ax('RichFiles', 'return name of first window').startswith('Destination'):
            raise StepFailed('the copy destination window is not in front')

    def suite_copy(self):
        self.suite_ax('RichFiles', 'set frontmost to true\n'
                      'click menu item "Paste Files" of menu 1 of menu bar item "Files" of menu bar 1')
        self.facts['copy_requested'] = True

    def suite_after(self):
        report = {}
        for name, before in self.facts['suite_windows'].items():
            old = self.owned[name + '-before']
            if self.a.suite == 'legacy':
                if self.suite_pids(name) != [old] or self.windows(name) != before:
                    raise StepFailed(name + ' changed despite lacking the restart contract')
                report[name] = {'pid': old, 'windows': before, 'outcome': 'legacy-process-retained'}
                continue
            if self.a.suite == 'busy' and name == 'RichFiles':
                deferred = self.wait(lambda: next((row for row in self.suite_events()
                    if row.get('pid') == old and row.get('operation') == 'richFilesSuiteRestart'
                    and row.get('outcome') == 'interrupted'), None), 'RichFiles to defer the restart', 35)
                if old not in self.suite_pids(name):
                    raise StepFailed('RichFiles exited before the deferral could be observed')
                source = self.source + '/Suite Copy.bin'
                copied = self.destination + '/Suite Copy.bin'
                size = self.g('stat -f %z ' + q(source))
                self.wait(lambda: self.g('stat -f %z ' + q(copied) + ' 2>/dev/null || true') == size,
                          'the copy to finish', 180)
                report['copy'] = {'bytes': int(size), 'deferred': deferred}
            pid = self.wait(lambda: next((pid for pid in self.suite_pids(name)
                                          if pid != old and self.running_build(name, pid, self.a.update_version)), None),
                            name + ' to report the new running version', 150)
            self.owned[name + '-after'] = pid
            if old in self.suite_pids(name):
                raise StepFailed(name + ' still runs the original process')
            if self.a.suite == 'busy' and name == 'RichFiles':
                released = next((row for row in self.suite_events() if row.get('pid') == old
                                 and row.get('id') == deferred['id'] and row.get('outcome') == 'complete'), None)
                if released is None or released['workerMS'] - deferred['workerMS'] < 59_000:
                    raise StepFailed('RichFiles did not retry its deferred restart after a minute')
                report['copy']['released'] = released
                digests = self.g('shasum -a 256 ' + q(source) + ' ' + q(copied), 120).splitlines()
                if len(digests) != 2 or digests[0].split()[0] != digests[1].split()[0]:
                    raise StepFailed('The copied file differs from its source')
                report['copy']['sha256'] = digests[0].split()[0]
            after = self.wait(lambda: (rows if (rows := self.windows(name)) == before else None),
                              name + ' to restore the same windows and positions', 45)
            report[name] = {'old_pid': old, 'pid': pid, 'windows': after,
                            'running': self.running_build(name, pid, self.a.update_version)}
        if self.a.suite != 'legacy':
            old_gestures = self.owned['gestures']
            def updated_gestures():
                pid = self.gesture_service_pid()
                return pid if pid and self.running_build('gestures', pid, self.a.update_version) else None
            new_gestures = self.wait(updated_gestures, 'the registered Window Gestures service to report the new build', 90)
            self.owned['gestures-after'] = new_gestures
            report['gestures'] = {'old_pid': old_gestures,
                                  'running': self.running_build('gestures', new_gestures, self.a.update_version)}
        if self.a.suite == 'editor':
            foreground = self.osascript('tell application "System Events" to return bundle identifier '
                                       'of first application process whose frontmost is true')
            if foreground != 'com.richos.app':
                raise StepFailed('The editor restore changed foreground ownership to ' + foreground)
            report['foreground'] = foreground
        self.shot('suite-after.png')
        return report

    def download(self):
        old = self.owned['app']
        self.press('is available', role=CUE_ROLE)
        self.wait(lambda: self.present('Download update', 'AXButton'), 'Download update', 30)
        pressed = time.monotonic()
        self.press('Download update')
        if self.a.suite == 'busy':
            # Staging the signed archive can take longer than the copy. Use the
            # real updater's ready transition to start the copy before the swap.
            timing = self.home + '/Library/Application Support/com.richos.app/update-timing.log'
            self.wait(lambda: self.g('grep -F ' + q('pid=%d ' % old) + ' ' + q(timing)
                      + " | grep -F 'phase=ready outcome=restarting' || true"),
                      'the signed update to finish staging', self.a.within)
            self.suite_copy()
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
        launch_log = self.home + '/Library/Logs/RichOS/launch.log'

        def launch_record():
            log = self.g('cat ' + q(launch_log) + ' 2>/dev/null || true')
            prefix = 'RichOS %s pid %d [richos] launch timing:' % (self.a.update_version, new)
            return next((line for line in reversed(log.splitlines()) if prefix in line), '')

        if self.a.suite:
            launch_line = self.wait(lambda: launch_record() if new in self.app_pids() else '',
                                    'the new RichOS process to finish application startup', 60)
        else:
            launch_line = launch_record()
        self.shot('2-after-restart.png')
        user_copy = self.g('ls -ld ' + q(self.user_app) + ' 2>/dev/null || true')
        if self.a.scenario == 'reinstall':
            # The copy the old updater left is retired behind a link to the one in Applications.
            link = self.g('readlink ' + q(self.user_app) + ' 2>/dev/null || true')
            if link != SYSTEM:
                raise StepFailed('~/Applications/RichOS.app is not a link to %s: %s' % (SYSTEM, user_copy))
            if self.receipt('prepared.json') is not None:
                raise StepFailed('the update is still prepared, not put in place')
        elif user_copy:
            raise StepFailed('a copy appeared at ~/Applications/RichOS.app: ' + user_copy)
        if self.a.suite in (None, 'legacy') and 'gestures' in self.owned and self.owned['gestures'] not in self.gesture_pids() and not self.g(
                'kill -0 %d 2>/dev/null && echo alive || true' % self.owned['gestures']):
            raise StepFailed('the login item stopped during the update')
        timing = self.g('cat ' + q(self.home + '/Library/Application Support/com.richos.app/update-timing.log')
                        + ' 2>/dev/null || true')
        (self.out / 'update-timing.log').write_text(timing + '\n')
        return {'new_pid': new, 'restart_s': relaunched, 'version': self.version(),
                'helpers_seen': sorted(seen), 'user_copy': user_copy,
                'system_install': self.receipt('system-install.json'),
                'publication': [l for l in timing.splitlines() if 'phase=publication' in l][-3:],
                'relaunched_launch_line': launch_line}

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

    # --- try-again ----------------------------------------------------------------------------
    def says(self, text):
        """Whether any element on the window shows `text`, as its value or its name."""
        for key in ('--value', '--title'):
            try:
                if self.ax('find', key, text, '--contains', '--first'):
                    return True
            except StepFailed:
                pass
        return False

    def log_text(self):
        return self.g('cat ' + q(self.log) + ' 2>/dev/null || true')

    def open_app(self, log):
        """Open /Applications/RichOS.app as the Dock does, keeping its output in `log`."""
        self.log = log
        before = set(self.app_pids())
        self.g('open -a ' + q(SYSTEM) + ' --env ' + q('RICHOS_UPDATE_ENDPOINT=' + self.endpoint_base + '/latest.json')
               + ' --stdout ' + q(log) + ' --stderr ' + q(log))
        pid = self.wait(lambda: next((p for p in self.app_pids() if p not in before), None),
                        'RichOS to start from /Applications', 90)
        self.set_app(pid)
        return pid

    def stage(self):
        self.quit_app(self.owned['app'])
        log = self.work + '/selftest.log'
        before = set(self.app_pids())
        # The app's own check, download, signature check and staging, ending at "ready" without a
        # restart (updates.rs spawn_selftest), so nothing is put in place yet.
        self.g('open -n -a ' + q(SYSTEM) + ' --env RICHOS_UPDATE_SELFTEST=install --env '
               + q('RICHOS_UPDATE_ENDPOINT=' + self.endpoint_base + '/latest.json')
               + ' --stdout ' + q(log) + ' --stderr ' + q(log))
        pid = self.wait(lambda: next((p for p in self.app_pids() if p not in before), None),
                        'the staging run to start', 60)
        self.owned['selftest'] = pid
        out = self.wait(lambda: (lambda t: t if 'RICHOS-UPDATE-SELFTEST exit=' in t else '')(
            self.g('cat ' + q(log) + ' 2>/dev/null || true')), 'the staging run to finish', self.a.within)
        said = [l for l in out.splitlines() if l.startswith('RICHOS-UPDATE-SELFTEST')]
        if 'RICHOS-UPDATE-SELFTEST exit=0' not in out:
            raise StepFailed('the update did not stage: ' + ' | '.join(said))
        self.wait(lambda: pid not in self.app_pids(), 'the staging run to exit', 60)
        prepared = self.receipt('prepared.json')
        if not prepared or prepared.get('version') != self.a.update_version:
            raise StepFailed('nothing prepared: %r' % prepared)
        incoming = self.journal + '/' + prepared['archive_sha256'] + '/incoming.app'
        self.g('chflags uchg ' + q(incoming))
        self.immutable = incoming
        flags = self.g('ls -ldO ' + q(incoming))
        if 'uchg' not in flags:
            raise StepFailed('the staged copy is not immutable: ' + flags)
        return {'selftest': said, 'prepared': prepared.get('version'), 'immutable': flags}

    def open_updates_row(self, until, what):
        """Open Settings (the Updates row lives in its menu) and wait until `until()` holds."""
        def shown():
            if until():
                return True
            self.clear_sheets()
            if not (self.present('Try again', 'AXButton') or self.present('Check for updates', 'AXButton')):
                try:
                    self.press('Settings', role=CUE_ROLE, contains=False)
                except StepFailed:
                    pass
            return False
        return self.wait(shown, what, self.a.within)

    def denied(self):
        self.open_app(self.work + '/app-denied.log')
        line = self.wait(lambda: next((l for l in self.log_text().splitlines() if 'update remains deferred' in l), ''),
                         'the launch-time swap to be refused', 90)
        if PERMISSION not in line:
            raise StepFailed('the launch-time swap was not refused for permission: ' + line)
        self.open_updates_row(lambda: self.says(NOT_PREPARED), 'the Updates row to report the refusal')
        self.press('Show the technical reason')
        self.wait(lambda: self.says(PERMISSION), 'the permission sentence as the reason', 30)
        self.shot('4-denied.png')
        return {'pid': self.owned['app'], 'launch': line.strip(),
                'denial_record': self.receipt('system-update-denied.json') is not None,
                'version': self.version()}

    def retry(self):
        # Try again while the permission is still missing: the refusal stays, nothing reads ready.
        self.press('Try again')
        end = time.monotonic() + 10
        while time.monotonic() < end:
            if self.present('is ready', CUE_ROLE):
                self.shot('5-ready-while-denied.png')
                raise StepFailed('Try again reported the update ready while the swap is still refused')
            time.sleep(1)
        if not self.says(NOT_PREPARED) or self.receipt('system-update-denied.json') is None:
            raise StepFailed('the refusal left the Updates row while the swap is still refused')
        self.shot('5-still-denied-before-permission.png')
        self.g('chflags nouchg ' + q(self.immutable))
        flags = self.g('ls -ldO ' + q(self.immutable))
        self.press('Try again')
        self.wait(lambda: self.present('is ready', CUE_ROLE), 'the update to read as ready', 60)
        if self.says(PERMISSION) or self.says(NOT_PREPARED):
            self.shot('5-still-denied.png')
            raise StepFailed('the Updates row still reports the refusal after Try again')
        self.shot('5-after-try-again.png')
        if self.receipt('system-update-denied.json') is not None:
            raise StepFailed('the saved denial is still there')
        return {'flags': flags, 'ready': True, 'version': self.version()}

    def publish(self):
        self.quit_app(self.owned['app'])
        self.wait(lambda: self.version() == self.a.update_version, 'the update in /Applications after quit', 120)
        if self.receipt('prepared.json') is not None or self.receipt('system-update-denied.json') is not None:
            raise StepFailed('the journal still holds the prepared update or the denial')
        if self.g('ls -ld ' + q(self.user_app) + ' 2>/dev/null || true'):
            raise StepFailed('a copy appeared at ~/Applications/RichOS.app')
        timing = self.g('cat ' + q(self.home + '/Library/Application Support/com.richos.app/update-timing.log')
                        + ' 2>/dev/null || true')
        return {'version': self.version(), 'publication': [l for l in timing.splitlines() if 'phase=publication' in l][-3:]}

    # --- cleanup ----------------------------------------------------------------------------
    def cleanup(self):
        done = {}
        if self.immutable:
            self.g('chflags nouchg ' + q(self.immutable) + ' 2>/dev/null || true')
        for log in ('Library/Logs/RichOS/launch.log',
                    'Library/Application Support/com.richos.app/update-timing.log', SUITE_LOG):
            self.pull(self.home + '/' + log, self.out / Path(log).name)
        if getattr(self, 'log', None):
            self.pull(self.log, self.out / 'app.log')
        app = self.owned.get('app')
        if app and app in self.app_pids():
            try:
                self.quit_app(app)
            except (StepFailed, RuntimeError, subprocess.SubprocessError):
                self.g('kill -TERM %d 2>/dev/null || true' % app)
        for name, pid in self.owned.items():
            if name.startswith(('RichFiles-', 'RichGlance-')):
                self.g('kill -TERM %d 2>/dev/null || true' % pid)
        if getattr(self, 'mount', None):
            self.g('hdiutil detach -quiet ' + q(self.mount) + ' || true')
        for name in ('gestures', 'gestures-after', 'server'):
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
    p.add_argument('--scenario', choices=sorted(STEPS), default='restart')
    p.add_argument('--stage-tool', type=Path, help="reinstall: the updater crate's guest-stage example")
    p.add_argument('--bundle-archive', type=Path, help='reinstall: the build under test as an update archive')
    p.add_argument('--bundle-version', help="reinstall: that archive's version")
    p.add_argument('--suite', choices=('legacy', 'restore', 'busy', 'editor'),
                   help='verify suite windows and process builds across the restart')
    a = p.parse_args()
    if a.suite and a.scenario != 'restart':
        p.error('--suite requires the restart scenario')
    if a.scenario == 'reinstall' and not (a.stage_tool and a.bundle_archive and a.bundle_version):
        p.error('--scenario reinstall needs --stage-tool, --bundle-archive and --bundle-version')
    walk = Walk(a)
    report = {'vm': a.vm, 'scenario': a.scenario, 'suite': a.suite, 'update_version': a.update_version, 'steps': []}
    steps = (['serve', 'install', 'gestures', 'ready', 'suite_open', 'download', 'suite_after', 'dock']
             if a.suite else STEPS[a.scenario])
    ok = True
    try:
        for step in steps:
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

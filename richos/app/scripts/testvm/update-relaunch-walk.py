#!/usr/bin/env python3
"""update-relaunch-walk.py — a downloaded update relaunches by itself when nothing runs (CEO feedback 2026-10-06, item 1).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      update-relaunch-walk.py --out DIR --expect-sha SHA --endpoint URL --expect-version VERSION

run-walk.py passes the owned VM name as the first argument. The bundle is the build under test,
whose version must be OLDER than the release --endpoint names (a development build of a branch
is `<version>-dev.<sha>`, which sorts below every nightly of the same version). --endpoint is a
published release's own `latest.json`, so what is downloaded, verified and staged is a real
signed release, and nothing about it is written here.

HIS WORDS. "there was no work going on at the time. So, there was nothing that could be
interrupted. In this case, the app should automatically re-launch immediately after downloading
the new version."

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  install    the bundle under test copied to the per-user destination (~/Applications/RichOS.app,
             where the CEO runs it) and started from there with --endpoint, by its pid
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme"; the first conversation is the app's own
  ready      the update is offered once the app's own first turn has ended ("Download update")
  draft      an unsent sentence typed into the composer; the window's frame read
  download   the cue pressed, then "Download update"; within --within seconds: the notice
             "Restarting into RichOS <version>" is on screen (photographed), the old process
             ends, a new one starts by itself, its log says "update activated: <old> -> <new>",
             and the installed bundle is <new>
  after      the new process has a window with the same frame, the same conversation, and the
             unsent sentence still in the composer (photographed)

Every app instance is quit by run-walk.py's stop.sh (CEO §54): this walk records the relaunched
process's pid in the run state, so stop.sh quits the instance that is actually running.
Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed
command = adopt.command

STEPS = ['install', 'identity', 'first-run', 'ready', 'draft', 'download', 'after']
DRAFT = 'Half a sentence I have not sent yet'
# The update cue is a <button aria-haspopup="true"> (ui/updates.js), which the accessibility tree
# exposes as an AXPopUpButton, not an AXButton: the nightly 41 walk's run (walk-fe53d4385fc2) photographed
# the cue on screen while a 300 s wait for an AXButton named "is available" found nothing.
CUE_ROLE = 'AXPopUpButton'


class Walk(adopt.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.dest = self.home + '/Applications/RichOS.app'

    def ax(self, mode, *args, app=None, timeout=150):
        # An accessibility read (find, tree) with no answer on a busy host is asked again, at most
        # three times in all (axwait.ask_again); a click is sent once; any other failure, and a
        # read that never answers, is raised as before. First run of this walk: a single
        # deadline on "Start the questions" ended it at first-run with the app healthy.
        for attempt in range(3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 2:
                    raise
                time.sleep(2)

    # present() is adopt-walk's: inside a bounded wait a find with no answer is "not seen yet"
    # (axwait.py), so the wait's own bound decides, not one slow read on a loaded host.

    def wait_for(self, title, role='AXButton', app=None, seconds=30):
        # adopt-walk.py's own waits are 30 s; on a loaded host one accessibility read can take
        # most of that, so every wait here gets at least --within.
        return super().wait_for(title, role, app, max(seconds, self.a.within))

    def log_path(self):
        return self.facts.get('log') or (self.payload + '/app.log')

    def log(self):
        return guest(self.vm, 'cat ' + shlex.quote(self.log_path()) + ' 2>/dev/null || true', 60)

    def windows(self):
        # Only the window lines ("RichOS 1400x864 at (140,77)"): ax.sh also prints its own timing
        # lines, which differ on every read, so a before/after comparison of the whole output can
        # never be equal (walk-95e83a839f1b: the same window, compared unequal on ax_guest_seconds).
        out = command([HERE / 'ax.sh', self.vm, '--windows'], 150)
        return '\n'.join(l for l in out.splitlines() if l.strip() and not l.startswith('{')).strip()

    def composer(self):
        rows = self.ax('find', '--role', 'AXTextArea', '--title', 'Message to Rich', '--contains', '--first')
        return next((r.get('value', '') for r in rows if 'value' in r), '')

    # --- steps --------------------------------------------------------------------------------
    def install(self):
        if guest(self.vm, 'test -e ' + shlex.quote(self.dest) + ' && echo yes || true'):
            raise StepFailed('fresh fixture required: ' + self.dest + ' already exists')
        app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
        guest(self.vm, 'mkdir -p ' + shlex.quote(self.home + '/Applications') + ' && ditto '
              + shlex.quote(app) + ' ' + shlex.quote(self.dest), 120)
        started = relaunch(self.vm, self.dest, {'RICHOS_UPDATE_ENDPOINT': self.a.endpoint})
        self.facts.update({'log': started['log'], 'pid_before': started['pid']})
        self.save()
        return {'pid': started['pid'], 'log': started['log'], 'installed': self.dest}

    def identity(self):
        end = time.monotonic() + 60
        while time.monotonic() < end:
            m = re.search(r'this app: built from (\S+)', self.log())
            if m:
                break
            time.sleep(1)
        else:
            raise StepFailed('the app never said what it was built from')
        built = m.group(1)
        if not built.startswith(self.a.expect_sha):
            raise StepFailed(f'the running app was built from {built}, not {self.a.expect_sha}')
        version = guest(self.vm, '/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" '
                        + shlex.quote(self.dest + '/Contents/Info.plist'))
        self.facts.update({'built_from': built, 'version_before': version})
        self.save()
        return {'built_from': built, 'version': version}

    def ready(self):
        # The control appears only once the work gate is clear: the app's own first turn has
        # ended and the launch check has found the release.
        try:
            self.wait_for('is available', role=CUE_ROLE, seconds=self.a.within)
        except StepFailed as exc:
            # Keep what the screen and the app said: the nightly 41 walk's first run ended here with
            # no frame and no log, so nobody could tell a cue that never came from one not found.
            self.shot('1-not-offered.png')
            lines = [x for x in self.log().splitlines() if 'updat' in x.lower()]
            (self.out / '1-update-log.txt').write_text('\n'.join(lines[-60:]) + '\n')
            raise StepFailed(f'{exc}; frame 1-not-offered.png, the app log\'s update lines in 1-update-log.txt')
        self.shot('1-available.png')
        return {'offered': True}

    def draft(self):
        self.type_into(DRAFT, '--role', 'AXTextArea', '--title', 'Message to Rich')
        time.sleep(1)  # the composer's own 400 ms park debounce, so this is the ordinary path too
        frame = self.windows()
        value = self.composer()
        if DRAFT not in value:
            raise StepFailed('the composer does not hold the typed sentence: ' + repr(value))
        self.facts.update({'frame_before': frame, 'composer_before': value})
        self.save()
        return {'frame': frame, 'composer': value}

    def download(self):
        old = str(self.facts['pid_before'])
        self.press('is available', role=CUE_ROLE)
        self.wait_for('Download update', seconds=20)
        pressed = time.monotonic()
        self.press('Download update')
        # 1. THE NOTICE. It is on screen for three seconds once the download is verified and
        #    staged, then the app writes "relaunching into" and quits. A frame of the guest's
        #    screen is taken on every turn of this loop (a screen capture, which does not wait
        #    on the app's accessibility tree the way a find does on a loaded host), and the
        #    frames are read afterwards for the notice's own words.
        frames = self.out / 'download-frames'
        frames.mkdir(exist_ok=True)
        end = pressed + self.a.within
        n = 0
        after_line = None
        while time.monotonic() < end:
            n += 1
            command([HERE / 'shot.sh', self.vm, frames / ('%03d.png' % n)], 60)
            if after_line is None and 'relaunching into' in self.log():
                after_line = n
            if after_line is not None and n >= after_line + 1:
                break
        relaunch_line = next((x for x in self.log().splitlines() if 'relaunching into' in x), '')
        if not relaunch_line:
            raise StepFailed(f'no relaunch within {self.a.within} s of the press')
        # The notice's own words, as the app writes them: the pill's "Restarting into RichOS <v>…"
        # and the row's "Nothing is running, so RichOS will restart into it in a moment." The OCR
        # match is case-sensitive, and the old lowercase "restarting" matched neither (the nightly
        # 41 walk, walk-99fb46933c29: frames 5 and 6 show the row's sentence, 0 of 7 hits).
        hits = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), '[Rr]estart(ing)? into', str(frames),
                               '--quiet'], capture_output=True, text=True, timeout=300)
        notice = [Path(x.split()[1]).name for x in hits.stdout.splitlines() if x.startswith('HIT')]
        if notice:
            shutil.copy(frames / notice[0], self.out / '2-restarting-notice.png')
        # 2. THE OLD PROCESS ENDS AND A NEW ONE STARTS BY ITSELF from the installed bundle.
        new = None
        while time.monotonic() < end and not new:
            if guest(self.vm, 'kill -0 ' + old + ' 2>/dev/null && echo alive || true') != 'alive':
                for line in guest(self.vm, 'ps -axo pid=,ppid=,comm=').splitlines():
                    parts = line.split(None, 2)
                    if len(parts) == 3 and parts[0] != old and parts[1] == '1' \
                            and parts[2].startswith(self.dest) and parts[2].endswith('/richos-tauri'):
                        new = parts[0]
            if not new:
                time.sleep(0.5)
        if not new:
            raise StepFailed(f'no new RichOS process within {self.a.within} s of the press')
        relaunched = round(time.monotonic() - pressed, 1)
        # stop.sh quits the recorded pid: record the instance that is actually running now.
        (self.state / 'app.pid').write_text(new + '\n')
        end = time.monotonic() + 60
        while time.monotonic() < end and 'update activated:' not in self.log():
            time.sleep(1)
        log = self.log()
        line = next((x for x in log.splitlines() if 'update activated:' in x), '')
        want = f"{self.facts['version_before']} -> {self.a.expect_version}"
        if want not in line:
            raise StepFailed('the relaunch did not activate the update: ' + (line or 'no activation line'))
        version = guest(self.vm, '/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" '
                        + shlex.quote(self.dest + '/Contents/Info.plist'))
        if version != self.a.expect_version:
            raise StepFailed('the installed bundle is ' + version)
        self.facts.update({'pid_after': int(new), 'version_after': version})
        self.save()
        if not notice:
            raise StepFailed(f'relaunched and activated, but none of {n} frames shows the restart notice')
        return {'notice_frames': notice, 'frames': n, 'new_process_after_s': relaunched, 'pid_after': int(new),
                'relaunch_line': relaunch_line, 'activation_line': line, 'installed_version': version}

    def after(self):
        end = time.monotonic() + self.a.within
        frame = ''
        last_error = ''
        while time.monotonic() < end:
            try:
                frame = self.windows()
            except StepFailed as exc:
                frame, last_error = '', str(exc)[-600:]
            try:
                # The guest's first-run engine sheet ("There's one thing I need on this Mac") comes
                # back on every launch, the relaunch's too, and it is modal over the composer
                # (walk-ec1c6a5c0a65's frame: same window, same conversation, the draft in the
                # composer, under that sheet). first-run answers it with Not now; so does this.
                if frame and self.present('Set it up'):
                    self.press('Not now')
                    time.sleep(1)
                if frame and self.present('Message to Rich', role='AXTextArea'):
                    break
            except StepFailed as exc:
                last_error = str(exc)[-600:]
            time.sleep(1)
        else:
            # Keep what the screen showed and what the reads said: the nightly 41 walk's run
            # walk-c389e7d4353f ended here with neither, after a relaunch that had activated.
            self.shot('3-after-failed.png')
            ps = guest(self.vm, 'ps -axo pid=,ppid=,comm= | grep -i richos || true')
            raise StepFailed('the relaunched app has no window with a composer; windows read '
                             + repr(frame) + '; last read error ' + repr(last_error)
                             + '; processes ' + repr(ps[-600:]) + '; frame 3-after-failed.png')
        time.sleep(2)
        value = self.composer()
        title = self.facts.get('thread_title') or ''
        # The open conversation is the rail's selected row (an AXButton) and the header's crumb;
        # any node carrying its title counts, not only a static text (walk-95e83a839f1b's frame
        # shows "Acme / Running" and the selected "Running" row while an AXStaticText find missed).
        on_thread = None
        if title:
            on_thread = False
            for by in ('--title', '--value'):
                try:
                    if self.ax('find', by, title, '--first'):
                        on_thread = True
                        break
                except StepFailed as exc:
                    if not axwait.absent(exc):
                        raise
        self.shot('3-after-relaunch.png')
        problems = []
        if frame != self.facts['frame_before']:
            problems.append(f'window frame {frame!r} is not {self.facts["frame_before"]!r}')
        if DRAFT not in value:
            problems.append('the unsent sentence is gone: ' + repr(value))
        if title and not on_thread:
            problems.append('the conversation "' + title + '" is not on screen')
        if problems:
            raise StepFailed('; '.join(problems))
        return {'frame': frame, 'composer': value, 'conversation': title}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--endpoint', required=True, help="a published release's own latest.json")
    p.add_argument('--expect-version', required=True, help='the version that endpoint offers')
    p.add_argument('--within', type=float, default=300, help='seconds for each wait on the app')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    if '/releases/download/v' + a.expect_version + '/' not in a.endpoint:
        p.error('--endpoint must be the --expect-version release tag\'s own manifest')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'endpoint': a.endpoint, 'steps': []}
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
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

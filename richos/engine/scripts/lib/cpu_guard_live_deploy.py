#!/usr/bin/env python3
"""Put the committed live CPU guard in place, take it out again, or say which one runs.

    cpu_guard_live_deploy.py status
    cpu_guard_live_deploy.py check      exit 0: launchd runs this checkout's file; 3: it does not
    cpu_guard_live_deploy.py deploy
    cpu_guard_live_deploy.py rollback

WHAT IS RUNNING. launchd's `com.richos.cpu-guard` runs `<state>/runtime/cpu_guard.py`, and
the Claude Bash hook runs the same file. Since the 2026-09-27 rollback that file is
cpu_guard.py as of 9154bf51 (LEGACY_SHA256 below). `cpu_guard_live.py`, beside this script,
is those bytes plus the release-build window. main's `cpu_guard.py` is NOT deployable this
way: its watch loop publishes the verification controller's pressure record, which turns on
the containment that was rolled back.

DEPLOY refuses unless the plist runs exactly `<state>/runtime/cpu_guard.py`, the runtime's
cpu_policy.py is the one it was installed with, the candidate is committed, and the running
file is a known controller: the legacy one, or the last candidate this script deployed and
recorded in live-runtime.json (already the candidate: nothing to do). The candidate
must load and answer `status` from the runtime directory before anything changes. Then the
running file is copied to `<state>/runtime-backups/`, the candidate replaces it atomically,
the CPU watch service alone is restarted (`launchctl kickstart -k`), and the deploy waits for
a heartbeat from a NEW process naming the candidate's sha256. If none arrives it puts the
backup back, restarts again and exits 1. The device collector keeps running; its code path
is unchanged.

ROLLBACK puts the legacy bytes back from the backup whose sha256 is LEGACY_SHA256, restarts
and verifies the same way. STATUS changes nothing.
"""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import time
import machine_settings as M

LEGACY_SHA256 = '83a4d90c64f6011fd788b9069aee630031bf220e22514ca6ac8eb820a5ca41bd'
POLICY_SHA256 = 'ca04ecbd2b7640dd6443ccc5089759bdd47e51653a54cfbfc632c026ace88cce'
LABEL = 'com.richos.cpu-guard'
HEARTBEAT_WAIT = 20.0


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_json(path, default=None):
    try:
        return json.loads(Path(path).read_text())
    except FileNotFoundError:
        return default


class Live:
    def __init__(self, state=None, plist=None, candidate=None, restart=None, wait=HEARTBEAT_WAIT):
        self.state = M.validate(state) if state else M.state('cpu-guard', 'RICHOS_CPU_GUARD_STATE')
        self.plist = Path(plist or Path.home() / 'Library/LaunchAgents' / (LABEL + '.plist'))
        self.candidate = Path(candidate or Path(__file__).with_name('cpu_guard_live.py'))
        self.runtime = self.state / 'runtime'
        self.running = self.runtime / 'cpu_guard.py'
        self.backups = self.state / 'runtime-backups'
        self.record = self.state / 'live-runtime.json'
        self.restart = restart or self.kickstart
        self.wait = wait
        self.interpreter = None

    @staticmethod
    def kickstart():
        subprocess.run(['launchctl', 'kickstart', '-k', 'gui/%d/%s' % (os.getuid(), LABEL)],
                       check=True, capture_output=True, text=True, timeout=30)

    def describe(self, digest):
        if digest == LEGACY_SHA256:
            return 'legacy controller (9154bf51)'
        if self.candidate.exists() and digest == sha256(self.candidate):
            return 'this checkout\'s cpu_guard_live.py'
        last = self.previous()
        if last:
            return 'an earlier deploy by this script (%s, from %s)' % (
                last['source_revision'][:12], last.get('source_file', 'unknown'))
        return 'UNKNOWN controller'

    def previous(self):
        """The last deploy this script recorded, when the running file is still exactly it.

        Until 2026-10-02 a deploy refused any running file but the 9154bf51 legacy controller,
        so once one candidate was live no later candidate could replace it: cpu_guard_live.py
        changed on main (P5-22, 7bd2efeb6, 2026-09-30) while launchd kept running the
        8e9cad40 bytes. A running file that matches this script's own record is known."""
        record = read_json(self.record)
        if (isinstance(record, dict) and not record.get('rolled_back_at') and record.get('sha256')
                and record.get('source_revision') and self.running.exists()
                and sha256(self.running) == record['sha256']):
            return record
        return None

    def status(self):
        digest = sha256(self.running)
        beat = read_json(self.state / 'heartbeat.json', {})
        return {'running_file': str(self.running), 'running_sha256': digest,
                'running_is': self.describe(digest),
                'heartbeat_controller': beat.get('controller', 'not reported (legacy controller)'),
                'heartbeat_age_seconds': round(time.time() - beat.get('at', 0), 1),
                'last_deploy': read_json(self.record)}

    def preconditions(self):
        config = plistlib.loads(self.plist.read_bytes())
        program = config.get('ProgramArguments', [])
        if len(program) < 4 or Path(program[2]) != self.running or program[3] != 'watch':
            raise SystemExit('refused: %s does not run %s watch (it runs %s)' % (self.plist, self.running, program))
        # The interpreter launchd (and the Bash hook) run it with: /usr/bin/python3 is
        # 3.9 on this Mac, and a candidate that only loads under a newer Python must be
        # refused here, not discovered by a watchdog that no longer starts.
        self.interpreter = program[0]
        if sha256(self.runtime / 'cpu_policy.py') != POLICY_SHA256:
            raise SystemExit('refused: %s is not the policy module this controller was installed with'
                             % (self.runtime / 'cpu_policy.py'))

    def committed(self):
        root = subprocess.run(['git', '-C', str(self.candidate.parent), 'rev-parse', '--show-toplevel'],
                              capture_output=True, text=True)
        if root.returncode:
            raise SystemExit('refused: %s is not in a git checkout' % self.candidate)
        dirty = subprocess.run(['git', '-C', root.stdout.strip(), 'status', '--porcelain', '--',
                                str(self.candidate)], capture_output=True, text=True, check=True)
        if dirty.stdout.strip():
            raise SystemExit('refused: %s has uncommitted changes; deploy only committed bytes' % self.candidate)
        return subprocess.run(['git', '-C', root.stdout.strip(), 'rev-parse', 'HEAD'],
                              capture_output=True, text=True, check=True).stdout.strip()

    def loads(self, path):
        """The file must import and answer `status` beside the runtime's cpu_policy.py."""
        shutil.copyfile(Path(__file__).with_name('machine_settings.py'), self.runtime / 'machine_settings.py')
        result = subprocess.run([self.interpreter, '-B', str(path), 'status'], cwd=str(self.runtime),
                                env={**os.environ, 'RICHOS_CPU_GUARD_STATE': str(self.state)},
                                capture_output=True, text=True, timeout=30)
        try:
            answered = result.returncode in (0, 1) and 'healthy' in json.loads(result.stdout)
        except ValueError:
            answered = False
        if not answered:
            raise SystemExit('refused: %s does not load under %s: %s' % (
                path, self.interpreter, (result.stderr or result.stdout or 'no output')[-2000:]))

    def swap_in(self, source, expected):
        staged = self.runtime / 'cpu_guard.py.deploy-new'
        shutil.copyfile(source, staged)
        if sha256(staged) != expected:
            staged.unlink()
            raise SystemExit('refused: staged copy of %s does not match %s' % (source, expected))
        os.replace(staged, self.running)

    def restarted_as(self, expected, old_pid):
        """Restart the watch and wait for a heartbeat from a new process running `expected`.

        `expected` None means the legacy controller, whose heartbeat names no controller."""
        started = time.time()
        self.restart()
        deadline = time.monotonic() + self.wait
        while time.monotonic() < deadline:
            beat = read_json(self.state / 'heartbeat.json', {})
            if (beat.get('ok') is True and beat.get('at', 0) >= started and beat.get('pid') != old_pid
                    and beat.get('controller') == expected):
                return beat
            time.sleep(.25)
        return None

    def deploy(self):
        self.preconditions()
        revision = self.committed()
        candidate = sha256(self.candidate)
        running = sha256(self.running)
        if running == candidate:
            return 'already running %s (%s); nothing changed' % (candidate[:12], self.describe(running))
        if running != LEGACY_SHA256 and self.previous() is None:
            raise SystemExit('refused: the running controller %s is neither the legacy controller, this '
                             'candidate, nor the last deploy this script recorded; inspect it before '
                             'replacing it' % running)
        # What a failed deploy puts back must report as itself: the legacy controller names no
        # controller in its heartbeat; an earlier candidate names its own sha256.
        restored = None if running == LEGACY_SHA256 else running
        staged = self.runtime / 'cpu_guard.py.deploy-check'
        shutil.copyfile(self.candidate, staged)
        try:
            self.loads(staged)
        finally:
            staged.unlink(missing_ok=True)
        self.backups.mkdir(parents=True, exist_ok=True)
        backup = self.backups / ('cpu_guard-%s.py' % running[:12])
        if not backup.exists():
            shutil.copyfile(self.running, backup)
        if sha256(backup) != running:
            raise SystemExit('refused: backup %s does not match the running controller' % backup)
        old_pid = read_json(self.state / 'heartbeat.json', {}).get('pid')
        self.swap_in(self.candidate, candidate)
        beat = self.restarted_as(candidate, old_pid)
        if beat is None:
            self.swap_in(backup, running)
            back = self.restarted_as(restored, None)
            raise SystemExit('FAILED: no heartbeat from the new controller within %ss; the previous controller '
                             '(%s) was put back and %s' % (self.wait, running[:12], 'is running' if back else
                                                            'HAS NOT reported a heartbeat either: run status now'))
        record = {'deployed_at': time.time(), 'sha256': candidate, 'source_revision': revision,
                  'source_file': str(self.candidate), 'backup': str(backup), 'watch_pid': beat['pid']}
        self.record.write_text(json.dumps(record, indent=2) + '\n')
        return 'deployed %s from %s; watch pid %s reports it' % (candidate[:12], revision[:12], beat['pid'])

    def rollback(self):
        self.preconditions()
        running = sha256(self.running)
        if running == LEGACY_SHA256:
            return 'the legacy controller is already running; nothing changed'
        backup = self.backups / ('cpu_guard-%s.py' % LEGACY_SHA256[:12])
        if not backup.exists() or sha256(backup) != LEGACY_SHA256:
            raise SystemExit('refused: no verified legacy backup at %s' % backup)
        old_pid = read_json(self.state / 'heartbeat.json', {}).get('pid')
        self.swap_in(backup, LEGACY_SHA256)
        if self.restarted_as(None, old_pid) is None:
            raise SystemExit('FAILED: the legacy bytes are in place but no heartbeat arrived within %ss; '
                             'run status now' % self.wait)
        record = read_json(self.record, {}) or {}
        record.update(rolled_back_at=time.time())
        self.record.write_text(json.dumps(record, indent=2) + '\n')
        return 'rolled back to the legacy controller %s' % LEGACY_SHA256[:12]


def main(argv):
    if len(argv) != 1 or argv[0] not in ('status', 'check', 'deploy', 'rollback'):
        print(__doc__, file=sys.stderr)
        return 2
    live = Live()
    if argv[0] == 'status':
        print(json.dumps(live.status(), indent=2))
        return 0
    if argv[0] == 'check':
        # Exit 0 when launchd runs exactly this checkout's cpu_guard_live.py, 3 when it runs
        # something else (a land that changed the controller and was not deployed), so a land
        # or a session start can say so instead of anyone finding out from a stopped build.
        status = live.status()
        current = status['running_sha256'] == sha256(live.candidate)
        print(('CPU guard: launchd runs this checkout\'s controller (%s)' if current else
               'CPU GUARD STALE: launchd runs %s (%s), this checkout has %s; deploy it with '
               'cpu_guard_live_deploy.py deploy') % ((status['running_sha256'][:12],) if current else (
                   status['running_sha256'][:12], status['running_is'], sha256(live.candidate)[:12])))
        return 0 if current else 3
    M.validate(live.state)
    print(live.deploy() if argv[0] == 'deploy' else live.rollback())
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

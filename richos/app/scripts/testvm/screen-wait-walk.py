#!/usr/bin/env python3
"""screen-wait-walk.py — CEO §56 on the real app, fake Claude, in one guest run.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone (CEO §54):

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      screen-wait-walk.py --out DIR --expect-sha SHA

THE QUESTION. His ruling §56: the app waits for the Mac's screen to unlock "for automatic use when
Rich needs it", and nothing asks him to unlock. The front desk (fake-claude-fill-first.pl) writes
one job down marked needs_screen (its `register` and `needs-screen` files) and, once the GUEST's
screen is locked, hands it to Stu (`delegate`, released by `delegate-go`).

THE STEPS, in the guest, never on the host's screen (CEO §65: only the guest's screen is locked
and unlocked, by Control-Command-Q and its own password typed into its own lock screen):
  setup       the fake claude and the status read pushed, the app relaunched on the fake
  identity    the running app says it was built from --expect-sha (adopt-walk.py)
  first-run   the first-run sheets declined and a company added (adopt-walk.py)
  ask         the job typed and sent; the front desk writes it down marked needs_screen
  lock        the guest's screen locked, then the handoff released: the work host meets it locked
  locked      Rich's own status read (the app's --status-mcp server, run in the guest) reads the
              screen as locked and lists the job under waiting_for_the_screen with the plain line;
              the record says waiting-for-screen with no notice; the window shows "waiting for the
              screen"; no back-end turn ran, and none runs while the lock is held 20 s more
  unlock      the guest's screen unlocked; the status read says unlocked, and Stu's back end gets
              the job by itself (calls.log "work-turn answered ... Handoff:"), exactly once
  after       the record no longer says it is waiting
Evidence in --out: report.json, look-locked.json, look-unlocked.json, records-*.txt, calls.log,
window-locked.json, 1-locked.png (the guest's lock screen), 2-unlocked.png. Exit 0 only when every
step passes.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True  # the modules loaded below would otherwise leave __pycache__ in the checkout
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed

G = '/Users/admin/fill-first'  # the fake's folder in the guest (fake-claude-fill-first.pl)
JOB = 'Take a screenshot of the Northwind dashboard.'
LINE = 'The screen is locked, so this is waiting for it to unlock. It will carry on by itself.'
STEPS = ['setup', 'identity', 'first-run', 'ask', 'lock', 'locked', 'unlock', 'after']
# Rich's status read: the status server the front desk was given, asked the one question he asks.
LOOK = '''import json, subprocess
server = json.load(open("%s/status-server.json"))
frames = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18",
           "capabilities": {}, "clientInfo": {"name": "screen-wait-walk", "version": "1"}}},
          {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "background_work", "arguments": {}}}]
out = subprocess.run([server["command"], *server.get("args", [])], capture_output=True, text=True, timeout=60,
                     input="".join(json.dumps(f) + "\\n" for f in frames)).stdout
for line in out.splitlines():
    reply = json.loads(line)
    if reply.get("id") == 2:
        print(reply["result"]["content"][0]["text"])
''' % G


class ScreenWalk(adopt.Walk):
    def check(self, ok, what):
        self.facts.setdefault('checks', []).append(('ok   ' if ok else 'FAIL ') + what)
        self.save()
        if not ok:
            raise StepFailed(what)
        return what

    def calls(self):
        return guest(self.vm, 'cat %s/calls.log 2>/dev/null || true' % G)

    def wait_calls(self, pattern, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if re.search(pattern, self.calls()):
                return True
            time.sleep(3)
        return False

    def records(self, name):
        # One record per line: the app writes each as one compact JSON value with no newline.
        text = guest(self.vm, 'for f in %s/engine-state/assignments/*/*.json; do [ -f "$f" ] && cat "$f" && echo; done; true'
                     % shlex.quote(self.data))
        (self.out / name).write_text(text + '\n')
        return [json.loads(row) for row in text.splitlines() if row.startswith('{')]

    def job(self, name):
        rows = [r for r in self.records(name) if r.get('title') == JOB.rstrip('.')]
        return rows[0] if len(rows) == 1 else None

    def look(self, name):
        text = guest(self.vm, 'python3 %s/look.py' % G, 90)
        (self.out / name).write_text(text + '\n')
        return json.loads(text)

    def keys(self, *lines):
        guest(self.vm, shlex.join(['osascript', *sum((['-e', line] for line in lines), [])]), 60)

    # --- steps --------------------------------------------------------------------------------
    def setup(self):
        guest(self.vm, 'mkdir -p ' + G)
        adopt.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'fake-claude-fill-first.pl', G + '/claude'], 60)
        look = self.out / 'look.py'
        look.write_text(LOOK)
        adopt.command([HERE / 'guest.sh', self.vm, '--push', look, G + '/look.py'], 60)
        guest(self.vm, 'chmod 755 {g}/claude && printf \'{{"five":10,"weekly":20}}\' > {g}/usage-1.json && '
                       'touch {g}/log-work-turns {g}/needs-screen {g}/delegate && printf "%s\\n" {job} > {g}/register'
              .format(g=G, job=shlex.quote(JOB)))
        launched = relaunch(self.vm, environment={'RICHOS_CLAUDE_BIN': G + '/claude'})
        return {'relaunched': launched, 'home': self.enter_home()}

    def enter_home(self, seconds=90):
        """A relaunch opens on the home screen, whose live canvas outlasts accessibility reads: as
        stu-reminder-walk.py does, it is recognized by OCR on a frame and Enter, its own key for
        Talk to Rich, goes in."""
        began = time.monotonic()
        while time.monotonic() - began < seconds:
            self.shot('home.png')
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich', str(self.out / 'home.png'),
                                    '--first', '--quiet'], capture_output=True, text=True, timeout=60)
            if found.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the frame: ' + found.stderr)
            if found.returncode == 0:
                adopt.command([HERE / 'ax.sh', self.vm, '--key', '36'])
                time.sleep(3)
                return {'home_on_screen_after_s': round(time.monotonic() - began, 1)}
            time.sleep(2)
        raise StepFailed('the home screen never came up after the relaunch')

    def ask(self):
        self.type_into(JOB, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        registered = self.wait_calls(r'register ok', 120)
        (self.out / 'calls-asked.log').write_text(self.calls() + '\n')
        self.shot('asked.png')
        self.check(registered, 'the front desk wrote the job down')
        record = self.job('records-asked.txt')
        self.check(bool(record and record.get('needs_screen') is True), 'the job is recorded with needs_screen true')
        return {'state': record['state']}

    def lock(self):
        self.keys('tell application "System Events" to keystroke "q" using {control down, command down}')
        time.sleep(5)
        guest(self.vm, 'touch %s/delegate-go' % G)
        self.check(self.wait_calls(r'delegate ok', 120), "Rich's handoff to Stu was saved")
        end = time.monotonic() + 60
        while time.monotonic() < end:
            record = self.job('records-locked.txt')
            if record and record['state'] == 'waiting-for-screen':
                break
            time.sleep(2)
        self.facts['locked_at'] = time.time()
        return {'state': record and record['state']}

    def locked(self):
        look = self.look('look-locked.json')
        record = self.job('records-locked.txt')
        window = subprocess.run([str(HERE / 'ax.sh'), self.vm, 'find', '--title', 'waiting for the screen', '--contains', '--json'],
                                capture_output=True, text=True, timeout=60)
        (self.out / 'window-locked.json').write_text(window.stdout + window.stderr)
        subprocess.run([str(HERE / 'shot.sh'), self.vm, str(self.out / '1-locked.png')], capture_output=True, timeout=90)
        calls = self.calls()
        (self.out / 'calls-locked.log').write_text(calls + '\n')
        evidence = {'screen': look['screen'], 'waiting_for_the_screen': look['waiting_for_the_screen'],
                    'record': {k: record.get(k) for k in ('state', 'detail', 'needs_screen', 'notices')} if record else None}
        self.facts['locked'] = evidence
        self.check(look['screen']['state'] == 'locked', "the app reads the guest's screen as locked")
        self.check(bool(record) and record['state'] == 'waiting-for-screen', 'the record says waiting-for-screen')
        self.check(record['detail'] == LINE, 'the record carries the one plain line')
        self.check(record['notices'] == [], 'no notice was raised: nothing asks him to unlock')
        rows = look['waiting_for_the_screen']
        self.check(len(rows) == 1 and rows[0]['detail'] == LINE, 'the status read lists the job waiting for the screen, with the line')
        self.check('waiting for the screen' in window.stdout, 'the window shows the job waiting for the screen')
        self.check('work-turn answered' not in calls, 'Stu got nothing while the screen was locked')
        time.sleep(20)
        self.check('work-turn answered' not in self.calls(), 'still waiting, and Stu still has nothing, 20 s later')
        return evidence

    def unlock(self):
        self.keys('tell application "System Events" to key code 49')
        time.sleep(2)
        self.keys('tell application "System Events" to keystroke "admin"', 'delay 0.5',
                  'tell application "System Events" to key code 36')
        self.facts['unlocked_at'] = time.time()
        end = time.monotonic() + 30
        look = None
        while time.monotonic() < end:
            look = self.look('look-unlocked.json')
            if look['screen']['state'] == 'unlocked':
                break
            time.sleep(2)
        self.check(bool(look) and look['screen']['state'] == 'unlocked', "the app reads the guest's screen as unlocked")
        self.check(self.wait_calls(r'work-turn answered .*Handoff:', 120), 'Stu got the job by itself once the screen was back')
        return {'screen': look['screen']}

    def after(self):
        time.sleep(5)
        subprocess.run([str(HERE / 'shot.sh'), self.vm, str(self.out / '2-unlocked.png')], capture_output=True, timeout=90)
        calls = self.calls()
        (self.out / 'calls.log').write_text(calls + '\n')
        record = self.job('records-after.txt')
        turns = [line for line in calls.splitlines() if ' work-turn answered ' in line]
        started = float(turns[0].split()[0]) if turns else None
        evidence = {'record': {k: record.get(k) for k in ('state', 'detail')} if record else None, 'work_turns': len(turns),
                    'stu_turn_after_unlock_s': round(started - self.facts['unlocked_at'], 1) if started else None}
        self.facts['after'] = evidence
        self.check(len(turns) == 1, 'the job went to Stu exactly once')
        self.check(bool(record) and record['state'] != 'waiting-for-screen', 'the record no longer says it is waiting')
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    a = p.parse_args()
    walk = ScreenWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in STEPS:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, KeyError, TypeError, ValueError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

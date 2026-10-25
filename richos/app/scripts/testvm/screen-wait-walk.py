#!/usr/bin/env python3
"""screen-wait-walk.py — CEO §56 on the real app, with the guest's own Claude, in one guest run.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone (CEO §54):

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      screen-wait-walk.py --out DIR --expect-sha SHA

THE QUESTION. His ruling §56: the app waits for the Mac's screen to unlock "for automatic use when
Rich needs it", and nothing asks him to unlock. A job that needs the screen is asked for; the
GUEST's screen is locked straight after Send, while Rich is still writing it down and handing it
to Stu; the job must wait, say so, and go to Stu by itself at the unlock.

Real Rich, not a fake: fake-claude-fill-first.pl no longer matches the app's front desk (it takes
the desk for a back end because both carry richos_work now, and its register call is refused), so
this walk uses the guest's own signed-in claude, as command-walk.py does. Whether Rich marks the
job needs_screen is his judgment; the walk says so plainly when he does not.

THE STEPS, in the guest, never on the host's screen (CEO §65: only the guest's screen is locked
and unlocked, by Control-Command-Q and its own password typed into its own lock screen):
  identity    the running app says it was built from --expect-sha (adopt-walk.py)
  first-run   the first-run sheets declined and a company added (adopt-walk.py)
  ask         the job typed and sent, and the guest's screen locked at once; Rich writes it down
              marked needs_screen and hands it to Stu
  locked      the record says waiting-for-screen with the one plain line and no notice; the status
              read (the app's own --status-mcp server, scoped to this conversation) lists it under
              waiting_for_the_screen; Stu's inbox cursor shows the handoff NOT given to him; still
              so 20 s later
  unlock      the guest's screen unlocked; Stu's inbox cursor shows the handoff given to a session
              by itself, and the record no longer says it is waiting
Evidence in --out: report.json, facts.json, look-*.json, records-*.txt, delivery-*.json,
window-*.json, 1-locked.png (the guest's lock screen), 2-unlocked.png. Exit 0 only when every step
passes.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True  # the modules loaded below would otherwise leave __pycache__ in the checkout
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed

JOB = ('Walk the RichOS app on this Mac\'s screen for me: open its Settings, take a screenshot of '
       'each pane and put them in a folder called Settings walk on my Desktop.')
LINE = 'The screen is locked, so this is waiting for it to unlock. It will carry on by itself.'
STEPS = ['identity', 'first-run', 'ask', 'locked', 'unlock']

# The status read Rich makes: the app's own --status-mcp server on a scope for this conversation,
# started in the guest's GUI session when it can be (launchctl asuser), where the window server
# answers the screen reading; the session it ran in is part of the answer.
LOOK = r'''import json, os, subprocess, sys
app, scope, state, entity, thread = sys.argv[1:6]
json.dump({"version": 1, "state_root": state, "entity_id": entity, "thread_id": thread}, open(scope, "w"))
frames = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18",
           "capabilities": {}, "clientInfo": {"name": "screen-wait-walk", "version": "1"}}},
          {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "background_work", "arguments": {}}}]
feed = "".join(json.dumps(f) + "\n" for f in frames)
for session, argv in (("gui", ["sudo", "-n", "launchctl", "asuser", str(os.getuid()), app, "--status-mcp", scope]),
                      ("ssh", [app, "--status-mcp", scope])):
    out = subprocess.run(argv, input=feed, capture_output=True, text=True, timeout=60).stdout
    for line in out.splitlines():
        try:
            reply = json.loads(line)
        except ValueError:
            continue
        if reply.get("id") == 2 and "result" in reply:
            print(json.dumps({"session": session, "answer": json.loads(reply["result"]["content"][0]["text"])}))
            sys.exit(0)
sys.exit("the status server gave no answer in either session")
'''

# Was the job's handoff given to Stu? Read off Stu's own inbox and its cursor (session_tools.rs):
# the handoff's input is `peer:<end offset of its line>`; it was given to him when the cursor holds
# that input with a session, or has accepted it.
DELIVERY = r'''import json, sys
inbox, assignment = sys.argv[1:3]
offset, end = None, 0
for raw in open(inbox, "rb"):
    end += len(raw)
    row = json.loads(raw)
    if row.get("handoff") and (row.get("work") or {}).get("assignment_id") == assignment:
        offset = end
try:
    cursor = json.load(open(inbox[:-len(".jsonl")] + ".cursor"))
except (OSError, ValueError):
    cursor = {}
active = (cursor.get("active") or {}).get("peer:%s" % offset) if offset else None
given = bool(offset) and (bool(active and active.get("session_id")) or offset <= (cursor.get("through") or 0)
                          or str(offset) in (cursor.get("accepted") or {}))
print(json.dumps({"handoff_offset": offset, "given_to_stu": given, "cursor_entry": active,
                  "cursor_through": cursor.get("through")}))
'''


class ScreenWalk(adopt.Walk):
    def check(self, ok, what):
        self.facts.setdefault('checks', []).append(('ok   ' if ok else 'FAIL ') + what)
        self.save()
        if not ok:
            raise StepFailed(what)
        return what

    def push(self, name, text):
        local = self.out / name
        local.write_text(text)
        adopt.command([HERE / 'guest.sh', self.vm, '--push', local, self.payload + '/' + name], 60)
        return self.payload + '/' + name

    def records(self, name):
        # One record per line: the app writes each as one compact JSON value with no newline.
        text = guest(self.vm, 'for f in %s/engine-state/assignments/*/*.json; do [ -f "$f" ] && cat "$f" && echo; done; true'
                     % shlex.quote(self.data))
        (self.out / name).write_text(text + '\n')
        return [json.loads(row) for row in text.splitlines() if row.startswith('{')]

    def job(self, name):
        sent = self.facts.get('sent_at_ms', 0)
        rows = [r for r in self.records(name) if r.get('registered_at_ms', 0) >= sent]
        return max(rows, key=lambda r: r['registered_at_ms']) if rows else None

    def look(self, name, record):
        text = guest(self.vm, shlex.join(['python3', self.look_py, self.payload + '/RichOS.app/Contents/MacOS/richos-tauri',
                                          self.payload + '/walk-status-scope.json', self.data + '/engine-state',
                                          record['entity_id'], record['thread_id']]), 90)
        (self.out / name).write_text(text + '\n')
        return json.loads(text)

    def delivery(self, name, record):
        inbox = '%s/engine-state/messages/%s/%s/stu.jsonl' % (self.data, record['entity_id'], record['thread_id'])
        text = guest(self.vm, shlex.join(['python3', self.delivery_py, inbox, record['id']]), 60)
        (self.out / name).write_text(text + '\n')
        return json.loads(text)

    def window(self, name):
        found = subprocess.run([str(HERE / 'ax.sh'), self.vm, 'find', '--title', 'waiting for the screen', '--contains', '--json'],
                               capture_output=True, text=True, timeout=60)
        (self.out / name).write_text(found.stdout + found.stderr)
        return found.stdout

    def keys(self, *lines):
        guest(self.vm, shlex.join(['osascript', *sum((['-e', line] for line in lines), [])]), 60)

    def lock_guest(self):
        self.keys('tell application "System Events" to keystroke "q" using {control down, command down}')

    # --- steps --------------------------------------------------------------------------------
    def ask(self):
        self.look_py = self.push('look.py', LOOK)
        self.delivery_py = self.push('delivery.py', DELIVERY)
        self.facts['sent_at_ms'] = self.clock()
        self.type_into(JOB, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        self.lock_guest()
        self.facts['locked_at_ms'] = self.clock()
        self.save()
        end = time.monotonic() + 240
        record = None
        while time.monotonic() < end:
            record = self.job('records-asked.txt')
            if record and record['state'] != 'registered':
                break
            time.sleep(3)
        self.facts['asked'] = {k: record.get(k) for k in ('id', 'kind', 'state', 'needs_screen', 'detail')} if record else None
        self.save()
        self.check(record is not None, 'Rich wrote the job down')
        self.check(record['needs_screen'] is True, 'Rich marked the job needs_screen (his judgment, from the record tool)')
        self.check(record['state'] == 'waiting-for-screen', 'the job went to waiting-for-screen on the locked screen')
        return self.facts['asked']

    def locked(self):
        record = self.job('records-locked.txt')
        look = self.look('look-locked.json', record)
        given = self.delivery('delivery-locked.json', record)
        window = self.window('window-locked.json')
        subprocess.run([str(HERE / 'shot.sh'), self.vm, str(self.out / '1-locked.png')], capture_output=True, timeout=90)
        answer = look['answer']
        evidence = {'status_read_session': look['session'], 'screen': answer['screen'],
                    'waiting_for_the_screen': answer['waiting_for_the_screen'], 'delivery': given,
                    'record': {k: record.get(k) for k in ('state', 'detail', 'notices')}}
        self.facts['locked'] = evidence
        self.save()
        self.check(record['state'] == 'waiting-for-screen' and record['detail'] == LINE, 'the record carries the one plain line')
        self.check(record['notices'] == [], 'no notice was raised: nothing asks him to unlock')
        rows = answer['waiting_for_the_screen']
        self.check(len(rows) == 1 and rows[0]['detail'] == LINE, 'the status read lists the job waiting for the screen, with the line')
        self.check(given['handoff_offset'] is not None and not given['given_to_stu'], 'Stu was not given the job while the screen was locked')
        self.facts['window_shows_waiting'] = 'waiting for the screen' in window
        time.sleep(20)
        still = self.delivery('delivery-locked-20s.json', record)
        self.check(not still['given_to_stu'] and self.job('records-locked-20s.txt')['state'] == 'waiting-for-screen',
                   'still waiting, and Stu still has nothing, 20 s later')
        return evidence

    def unlock(self):
        self.keys('tell application "System Events" to key code 49')
        time.sleep(2)
        self.keys('tell application "System Events" to keystroke "admin"', 'delay 0.5',
                  'tell application "System Events" to key code 36')
        self.facts['unlocked_at_ms'] = self.clock()
        record = self.job('records-unlocked.txt')
        end = time.monotonic() + 120
        given = None
        while time.monotonic() < end:
            given = self.delivery('delivery-unlocked.json', record)
            if given['given_to_stu']:
                break
            time.sleep(2)
        self.facts['given_after_unlock_s'] = round((self.clock() - self.facts['unlocked_at_ms']) / 1000, 1)
        look = self.look('look-unlocked.json', record)
        window = self.window('window-unlocked.json')
        time.sleep(3)
        subprocess.run([str(HERE / 'shot.sh'), self.vm, str(self.out / '2-unlocked.png')], capture_output=True, timeout=90)
        after = self.job('records-after.txt')
        evidence = {'delivery': given, 'screen': look['answer']['screen'], 'status_read_session': look['session'],
                    'record': {k: after.get(k) for k in ('state', 'detail')}, 'window_still_waiting': 'waiting for the screen' in window}
        self.facts['unlocked'] = evidence
        self.save()
        self.check(bool(given and given['given_to_stu']), 'Stu was given the job by itself once the screen was back')
        self.check(after['state'] != 'waiting-for-screen', 'the record no longer says it is waiting')
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

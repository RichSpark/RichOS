#!/usr/bin/env python3
"""resend-walk.py — a start of the app never hands Stu a handoff his session already received
(CEO, 2026-11-01; richos-hq docs/audits/2026-11-01-app-start, finding A).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      resend-walk.py --out DIR --expect-sha SHA

THE QUESTION. At every start of his app Stu was handed the same six old handoffs again: a handoff
stays in Stu's inbox until its job closes, and what a session was given was remembered only until
the app quit. With the fix, a start reads the session's own Claude Code record and gives Stu only
what that record does not hold.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: memory setup declined, company "Acme" registered, questions declined
  seed       the app is stopped (relaunch.py's quit_app, the recorded pid only) and this
             conversation is given the fixture of his data: one open handoff in Stu's inbox
             (`engine-state/messages/<company>/<conversation>/stu.jsonl`), delivered and accepted
             by his cursor with the session it was given to, and that session's own record (its
             binding, `record.json`, and a Claude Code transcript holding the handoff as a user
             turn, as the app sends it). Nothing of the fixture is a model turn
  relaunch   command-walk.py's: the app opened again and its composer back
  quiet      for --quiet seconds after the relaunch: the session's record has no line it did not
             have (no new turn), no other session of Stu's appears, no `claude` process of Stu's
             runs, and his cursor still names the session, now with the `received` mark the fix
             leaves when it found the handoff in the record (quiet.json)

Evidence in --out: report.json, facts.json, quiet.json, the app logs and hook records. Exit 0 when
every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, quit_app  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed

STEPS = ['identity', 'first-run', 'seed', 'relaunch', 'quiet']
HANDOFF = 'Build nightly 78 from richos main (resend walk fixture {marker}).'

# Writes the fixture into the guest's app data. argv: data dir, home, entity, thread, session,
# handoff text, instruction sha256. Prints what it wrote.
SEED = r'''
import json, os, sys
data, home, entity, thread, session, text, sha = sys.argv[1:8]
state = os.path.join(data, "engine-state")
inbox_dir = os.path.join(state, "messages", entity, thread)
os.makedirs(inbox_dir, exist_ok=True)
inbox = os.path.join(inbox_dir, "stu.jsonl")
if os.path.exists(inbox):
    raise SystemExit("Stu already has an inbox in this conversation: " + inbox)
message = {"handoff": True, "from": "rich", "text": text,
           "instruction": {"ledger_ref": "ledger:%s:turn-resend-walk" % thread, "sha256": sha}}
line = json.dumps(message) + "\n"
with open(inbox, "w") as f:
    f.write(line)
through = len(line.encode())
cursor = {"through": through, "handoffs_retained": True, "accepted": {},
          "active": {"peer:%d" % through: {"input": "peer:%d" % through, "session_id": session}}}
with open(os.path.join(inbox_dir, "stu.cursor"), "w") as f:
    json.dump(cursor, f)
evidence = os.path.join(state, "evidence", session)
os.makedirs(evidence, exist_ok=True)
with open(os.path.join(evidence, "binding.json"), "w") as f:
    json.dump({"session_id": session, "role": "stu", "entity_id": entity, "thread_id": thread,
               "instruction": None}, f)
record = os.path.join(home, ".claude", "projects", "resend-walk", session + ".jsonl")
os.makedirs(os.path.dirname(record), exist_ok=True)
with open(os.path.join(evidence, "record.json"), "w") as f:
    json.dump({"transcript_path": record}, f)
delivered = dict(message, previous_delivery_attempt=None)
rows = [
    {"type": "user", "sessionId": session, "uuid": session[:8] + "-u1", "promptSource": "sdk",
     "message": {"role": "user", "content": [{"type": "text", "text": json.dumps(delivered)}]}},
    {"type": "assistant", "sessionId": session, "uuid": session[:8] + "-a1",
     "message": {"role": "assistant", "content": [{"type": "text", "text": "On it."}]}},
]
with open(record, "w") as f:
    f.write("".join(json.dumps(r) + "\n" for r in rows))
print(json.dumps({"inbox": inbox, "through": through, "record": record}))
'''

# Reads what the quiet step judges. argv: data dir, entity, thread, session, record path.
READ = r'''
import json, glob, os, subprocess, sys
data, entity, thread, session, record = sys.argv[1:6]
state = os.path.join(data, "engine-state")
try:
    lines = sum(1 for _ in open(record, "rb"))
except OSError as e:
    lines = "unreadable: %s" % e
cursor = json.load(open(os.path.join(state, "messages", entity, thread, "stu.cursor")))
others = []
for path in glob.glob(os.path.join(state, "evidence", "*", "binding.json")):
    try:
        b = json.load(open(path))
    except (OSError, ValueError):
        continue
    if b.get("role") == "stu" and b.get("session_id") != session:
        others.append(b.get("session_id"))
ps = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True).stdout
stu = [l.strip()[:300] for l in ps.splitlines() if "claude" in l and ":stu" in l]
print(json.dumps({"record_lines": lines, "cursor_active": cursor.get("active"), "other_stu_sessions": others,
                  "stu_processes": stu}))
'''


def quiet_verdict(before, samples, session):
    """before: the record's line count when it was written; samples: READ outputs over the quiet
    window. -> failures (empty when no turn of Stu's happened and the fix marked the handoff)."""
    failures = []
    if not samples:
        return ['nothing was read in the quiet window']
    grown = [s['record_lines'] for s in samples if s['record_lines'] != before]
    if grown:
        failures.append('the session record changed from %s lines to %s: a turn ran' % (before, grown[-1]))
    others = sorted({o for s in samples for o in s['other_stu_sessions']})
    if others:
        failures.append('another session of Stu\'s appeared: %s' % ', '.join(others))
    procs = [p for s in samples for p in s['stu_processes']]
    if procs:
        failures.append('a claude process of Stu\'s ran: %s' % procs[0])
    entries = list((samples[-1]['cursor_active'] or {}).values())
    if len(entries) != 1 or entries[0].get('session_id') != session:
        failures.append('his cursor no longer names the session the handoff was given to: %s' % json.dumps(entries))
    elif entries[0].get('received') is not True:
        failures.append('the app did not mark the handoff as found in the session\'s record: %s' % json.dumps(entries))
    return failures


class ResendWalk(command_walk.CommandWalk):
    def entity_of(self, thread):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        entity = None
        for row in rows.splitlines():
            try:
                event = json.loads(row)
            except ValueError:
                continue
            if event.get('thread_id') == thread and event.get('entity_id'):
                entity = event['entity_id']
        if not entity:
            raise StepFailed('the conversation %s has no company in the ledger' % thread)
        return entity

    def seed(self):
        thread = self.facts.get('thread')
        if not thread:
            raise StepFailed('the first-run step must have run (no conversation on record)')
        entity = self.entity_of(thread)
        marker = uuid.uuid4().hex[:8]
        session = str(uuid.uuid4())
        text = HANDOFF.format(marker=marker)
        sha = hashlib.sha256(text.encode()).hexdigest()
        quit_app(self.vm)
        wrote = json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(SEED) + ' ' + ' '.join(
            shlex.quote(v) for v in (self.data, self.home, entity, thread, session, text, sha)), 60))
        self.facts.update({'entity': entity, 'session': session, 'marker': marker, 'record': wrote['record'],
                           'record_lines': 2, 'inbox_through': wrote['through']})
        self.save()
        return dict(wrote, entity=entity, thread=thread, session=session)

    def read(self):
        f = self.facts
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(READ) + ' ' + ' '.join(
            shlex.quote(v) for v in (self.data, f['entity'], self.facts['thread'], f['session'], f['record'])), 60))

    def quiet(self):
        if not self.facts.get('session'):
            raise StepFailed('the seed step must have run (no session on record)')
        samples = []
        end = time.monotonic() + self.a.quiet
        while True:
            samples.append(self.read())
            if time.monotonic() >= end:
                break
            time.sleep(5)
        failures = quiet_verdict(self.facts['record_lines'], samples, self.facts['session'])
        evidence = {'seconds': self.a.quiet, 'samples': len(samples), 'first': samples[0], 'last': samples[-1],
                    'record_lines_written': self.facts['record_lines'], 'failures': failures}
        (self.out / 'quiet.json').write_text(json.dumps(evidence, indent=2) + '\n')
        self.shot('quiet.png')
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--relaunch-within', type=float, default=120, help='seconds for the composer to come back')
    p.add_argument('--quiet', type=float, default=90, help='seconds watched after the relaunch')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = ResendWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

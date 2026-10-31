#!/usr/bin/env python3
"""token-waste-guest.py — inside a test VM guest: the two reads token-waste-walk.py needs.

Run by token-waste-walk.py over ssh, with the guest's /usr/bin/python3 (3.9), never on a host:

  token-waste-guest.py receipts DATA
      every dispatch receipt the app's work tool wrote (<DATA>/engine-state/work-receipts/*/*.json)
      as one JSON list: name, role, status, request_id, whether the app admitted its Agent call
      (tool_use_id or agent_id), and its `settled` record (reason, withdrawn, retained).

  token-waste-guest.py quiet ENGINE SESSION SCRATCH
      stall-watch's own tick (`stall-watch.sh --tick`, the bundle's engine) over a FIXTURE registry
      in SCRATCH (never the app's own registry): one teammate that started and has been silent for
      an hour (the control: it MUST be reported SILENT, or the check measures nothing), one that
      was only prepared and one whose Agent call was registered and then refused (neither ever
      ran). Prints {"rc": N, "out": "<what stall-watch said>"}, the tick run with --jsonl: one
      {"text": block} line per announcement, whose stall headlines the walk reads by their shape.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path


def receipts(data):
    rows = []
    for path in sorted(Path(data).glob('engine-state/work-receipts/*/*.json')):
        try:
            value = json.loads(path.read_text())
        except (OSError, ValueError):
            rows.append({'file': path.name, 'unreadable': True})
            continue
        if not isinstance(value, dict):
            continue
        rows.append({'file': path.name, 'name': value.get('name'),
                     'role': (value.get('request') or {}).get('role'),
                     'obligation': (value.get('request') or {}).get('obligation_id'),
                     'status': value.get('status'), 'request_id': value.get('request_id'),
                     'admitted': bool(value.get('tool_use_id') or value.get('agent_id')),
                     'rebound': value.get('rebound'),
                     'settled': value.get('settled')})
    print(json.dumps(rows))


FIXTURE = r'''
import json, os, sys, time
sys.path.insert(0, os.path.join(sys.argv[1], "mega-lander"))
import workspaces
reg, sid, pid, kind, name = sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5], sys.argv[6]
state, start = workspaces.process_start(pid)
os.makedirs(os.path.join(reg, "sessions"), exist_ok=True)
os.makedirs(os.path.join(reg, "agents"), exist_ok=True)
with open(os.path.join(reg, "sessions", sid + ".json"), "w") as fh:
    json.dump({"session_id": sid, "pid": pid, "pid_start": start}, fh)
now = float(int(time.time()))
rec = workspaces.new_record("%s--%s" % (sid, name), name=name, session_id=sid, agent_id="a0000000000" + kind,
                            session_identity={"pid": pid, "pid_start": start},
                            started_at=workspaces.iso(now - 3600.0), workspaces=[])
if kind in ("prepared", "refused"):
    rec["agent_id"] = ""
    rec["registered_at"] = rec.pop("started_at")
    if kind == "refused":
        rec["spawned_at"] = rec["registered_at"]
        rec["tool_use_id"] = "toolu_refused_before_start"
with open(os.path.join(reg, "agents", rec["key"] + ".json"), "w") as fh:
    json.dump(rec, fh)
'''


def quiet(engine, session, scratch):
    box = Path(scratch)
    box.mkdir(parents=True, exist_ok=True)
    reg = str(box / 'workspaces')
    for sub in ('entity', 'platform-sessions', 'projects', 'waits', 'slots', 'testvm', 'agent-hold', 'test-devices'):
        (box / sub).mkdir(exist_ok=True)
    (box / 'entity' / 'orchestration.config').write_text('PROTECTED_PATHS=""\n')
    (box / 'slots' / 'limit').write_text('1\n')
    env = dict(os.environ, RICHOS_ENTITY_ROOT=str(box / 'entity'), RICHOS_SESSIONS_DIR=str(box / 'platform-sessions'),
               RICHOS_PROJECTS_DIR=str(box / 'projects'), RICHOS_SESSION_PID=str(os.getpid()),
               RICHOS_SESSION_ID=session, RICHOS_WAITS_DIR=str(box / 'waits'),
               RICHOS_PROOF_RUN_SLOTS_DIR=str(box / 'slots'), TESTVM_ROOT=str(box / 'testvm'),
               RICHOS_ESCALATION_LEDGER=str(box / 'escalations.jsonl'), RICHOS_AGENT_HOLD_DIR=str(box / 'agent-hold'),
               RICHOS_TEST_DEVICES_DIR=str(box / 'test-devices'), STALL_WATCH_CPU_BUSY='30',
               RICHOS_WORKSPACES_DIR=reg, STALL_WATCH_STATE_DIR=str(box / 'state'))
    for kind, name in (('started', 'control-silent'), ('prepared', 'lined-up-never-started'),
                       ('refused', 'refused-never-started')):
        made = subprocess.run([sys.executable, '-c', FIXTURE, engine, reg, session, str(os.getpid()), kind, name],
                              env=env, capture_output=True, text=True, timeout=60)
        if made.returncode:
            raise SystemExit('fixture %s could not be written: %s' % (name, made.stderr))
    now = int(time.time())
    env['STALL_WATCH_NOW'] = str(now)
    done = subprocess.run(['bash', engine + '/scripts/stall-watch.sh', '--tick', '--jsonl'], env=env,
                          capture_output=True, text=True, timeout=120)
    print(json.dumps({'rc': done.returncode, 'out': done.stdout + done.stderr}))


if __name__ == '__main__':
    {'receipts': receipts, 'quiet': quiet}[sys.argv[1]](*sys.argv[2:])

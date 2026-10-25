#!/usr/bin/env python3
"""due-notice-guest.py — inside a test VM guest: a paused teammate in the app's own registry.

Run by due-notice-walk.py over ssh, with the guest's python3, never on a host:

  due-notice-guest.py paused ENGINE WORKSPACES SESSION NAME
        register teammate NAME of the session SESSION, paused, in the app's own workspace registry
        (WORKSPACES, the conversation's partition under <app data>/engine-state/workspaces), as
        spawn.sh registers one (workspaces.new_record) with a pause beside it. Prints its path.
  due ENGINE WORKSPACES SESSION ENTITY THREAD
        the due work the engine computes for SESSION with the app's environment (RICHOS_APP_ENTITY
        and RICHOS_APP_THREAD set, as engine_profile.rs configure sets them): one JSON list of
        {name, source, command}.
  record ENGINE WORKSPACES SESSION NAME
        the registry record of NAME, as JSON.
"""
import json
import os
import subprocess
import sys
import time

PRELUDE = 'import json, sys; sys.path.insert(0, sys.argv[1]); import workspaces as w; w._ensure_dirs(); '


def run(engine, workspaces, code, *args, env=None):
    env = dict(os.environ, RICHOS_WORKSPACES_DIR=workspaces, **(env or {}))
    done = subprocess.run(['python3', '-c', PRELUDE + code, engine + '/mega-lander', *args],
                          env=env, capture_output=True, text=True, timeout=60)
    if done.returncode != 0:
        raise SystemExit('the registry could not be read or written: ' + done.stderr)
    print(done.stdout.strip())


def paused(engine, workspaces, session, name):
    run(engine, workspaces,
        'key = w.named_key(sys.argv[2], sys.argv[3]); '
        'w.write_json(w.agent_path(key), w.new_record(key, name=sys.argv[3], session_id=sys.argv[2], '
        'pause={"at": int(sys.argv[4]), "until": "Rich\'s resume"})); print(w.agent_path(key))',
        session, name, str(int(time.time())))


def due(engine, workspaces, session, entity, thread):
    run(engine, workspaces,
        'print(json.dumps([{"name": d["name"], "source": d["source"], "command": d["command"]} '
        'for d in w.due_set(sys.argv[2], w.pending(sys.argv[2], auto=False, dry=True))]))',
        session, env={'RICHOS_APP_ENTITY': entity, 'RICHOS_APP_THREAD': thread})


def record(engine, workspaces, session, name):
    run(engine, workspaces, 'print(json.dumps(w.load_agent(w.named_key(sys.argv[2], sys.argv[3]))))',
        session, name)


if __name__ == '__main__':
    {'paused': paused, 'due': due, 'record': record}[sys.argv[1]](*sys.argv[2:])

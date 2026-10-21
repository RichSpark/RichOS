#!/usr/bin/env python3
"""stu-reminder-guest.py — inside a test VM guest: one finished teammate waiting for its review.

Run by stu-reminder-walk.py over ssh, with the guest's /usr/bin/python3 (3.9), never on a host:

  stu-reminder-guest.py finished ENGINE WORKSPACES SESSION   register a finished teammate of the Stu
                                       session SESSION in the app's own workspace registry
                                       (WORKSPACES, the conversation's partition under
                                       <app data>/engine-state/workspaces), as spawn.sh registers
                                       one (workspaces.new_record): its run's end recorded and a
                                       wait beside it, so stall-watch's unfinished-work reminder has
                                       something to name once Stu is between turns. Prints the path.
"""
import os
import subprocess
import sys
import time


def finished(engine, workspaces, session):
    code = ('import json, sys; sys.path.insert(0, sys.argv[1]); import workspaces as w; w._ensure_dirs(); '
            'key = sys.argv[2] + "--finished-mate"; '
            'w.write_json(w.agent_path(key), w.new_record(key, name="finished-mate", session_id=sys.argv[2], '
            'end={"at": int(sys.argv[3]), "signal": "SubagentStop"}, '
            'waiting={"kind": "started", "on": "waiting for its review"})); print(w.agent_path(key))')
    env = dict(os.environ, RICHOS_WORKSPACES_DIR=workspaces)
    done = subprocess.run(['python3', '-c', code, engine + '/mega-lander', session, str(int(time.time()))],
                          env=env, capture_output=True, text=True, timeout=60)
    if done.returncode != 0:
        raise SystemExit('the teammate could not be registered: ' + done.stderr)
    print(done.stdout.strip())


if __name__ == '__main__':
    {'finished': finished}[sys.argv[1]](*sys.argv[2:])

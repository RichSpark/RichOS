#!/usr/bin/env python3
"""Relaunch the recorded test app in its guest, preserving its fixture environment."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE=Path(__file__).resolve().parent


def guest(vm, command, timeout=30):
    r=subprocess.run([str(HERE/'guest.sh'),vm,command],capture_output=True,text=True,timeout=timeout)
    if r.returncode: raise RuntimeError(r.stderr or r.stdout or 'guest command failed')
    return r.stdout.strip()


def quit_app(vm):
    """Stop the recorded test app and wait until it is gone; answers the guest's payload."""
    state=Path(os.environ.get('TESTVM_ROOT',str(Path.home()/'.richos-testvm')))/'run'/vm
    payload=(state/'payload').read_text().strip()
    if not payload.startswith('/Users/') or '/testvm/' not in payload: raise ValueError('not a guest payload')
    old=(state/'app.pid').read_text().strip()
    if not old.isdigit(): raise ValueError('recorded app PID is not numeric')
    # Only the captured process is stopped. A replacement PID is captured after launch.
    recorded=guest(vm,'ps -p '+old+' -o comm= 2>/dev/null || true')
    if recorded and (payload not in recorded or not recorded.endswith('/richos-tauri')):
        raise ValueError('recorded app PID no longer belongs to this payload')
    guest(vm,'kill -TERM '+old+' 2>/dev/null || true')
    for _ in range(30):
        if guest(vm,'kill -0 '+old+' 2>/dev/null && echo alive || true')!='alive': break
        time.sleep(.2)
    else:
        guest(vm,'kill -KILL '+old+' 2>/dev/null || true')
    return payload


def relaunch(vm, app=None, environment=None):
    state=Path(os.environ.get('TESTVM_ROOT',str(Path.home()/'.richos-testvm')))/'run'/vm
    payload=quit_app(vm)
    if app is None:
        app=guest(vm,'find '+shlex.quote(payload)+' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
    home=payload+'/home'
    env={'HOME':home,'CLAUDE_CONFIG_DIR':home+'/.claude','RICHOS_ACTIVATION':'regular',
         'RICHOS_CLAUDE_BIN':'/Users/admin/.local/bin/claude','DISABLE_AUTOUPDATER':'1',
         'RICHOS_ENGINE_DIR':payload+'/engine'}
    env.update(environment or {})
    logfile=payload+'/relaunch-'+str(time.time_ns())+'.log'
    args=['open','-n','-a',app]
    for key,value in env.items(): args.extend(['--env',key+'='+value])
    args.extend(['--stdout',logfile,'--stderr',logfile])
    existing={line.split(None,1)[0] for line in guest(vm,'ps -axo pid=,comm=').splitlines()}
    guest(vm,shlex.join(args))
    # Follow launcher redirects into the user-installed bundle as well.
    for _ in range(50):
        rows=guest(vm,'ps -axo pid=,ppid=,comm=')
        # The same executable also runs MCP helpers. LaunchServices owns the
        # GUI process; an existing helper must never become the recorded app.
        pids=[]
        for line in rows.splitlines():
            parts=line.split(None,2)
            if len(parts)!=3:continue
            pid,parent,exe=parts
            if pid not in existing and parent=='1' and payload in exe and exe.endswith('/richos-tauri'):
                pids.append(pid)
        if pids:
            pid=pids[-1];(state/'app.pid').write_text(pid+'\n')
            return {'pid':int(pid),'log':logfile,'app':app}
        if 'RICHOS-UPDATE-SELFTEST exit=' in guest(vm,'cat '+shlex.quote(logfile)+' 2>/dev/null || true'):
            return {'pid':None,'log':logfile,'app':app}
        time.sleep(.2)
    raise RuntimeError('no owned app process after relaunch: '+logfile)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('vm');p.add_argument('--app');p.add_argument('--env',action='append',default=[])
    a=p.parse_args()
    try: print(json.dumps(relaunch(a.vm,a.app,dict(x.split('=',1) for x in a.env))))
    except (ValueError,RuntimeError,OSError,subprocess.TimeoutExpired) as exc:
        print(str(exc),file=sys.stderr);sys.exit(2)

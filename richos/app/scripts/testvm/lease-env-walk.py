#!/usr/bin/env python3
"""lease-env-walk.py — the Claude Code worker the app launched carries the retry variables.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle APP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      lease-env-walk.py --out DIR [--within SECONDS]

run-walk.py passes the owned VM name as the first argument.

WHAT IT ANSWERS. Whether a Claude Code child process of THIS guest's app has
CLAUDE_CODE_MAX_RETRIES=15 and CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS=2000 in its environment
(native.rs `lease_retry_env`). The environment is read off the running process (kernel
KERN_PROCARGS2, in the guest only), not off the source. Polls until a child of the recorded app pid carries them or
--within seconds pass. Writes <out>/lease-env.json; exit 0 only when both were seen on one child.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO §54).
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

WANT = {'CLAUDE_CODE_MAX_RETRIES': '15', 'CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS': '2000'}


# Reads a process's environment from the kernel (KERN_PROCARGS2), in the guest: `ps -E` shows
# nothing for this child. Layout: argc, the executable path, padding, argv, then the environment.
PROCARGS = r"""import ctypes,struct,sys
libc=ctypes.CDLL('libc.dylib')
pid=int(sys.argv[1])
n=ctypes.c_size_t(1<<20);buf=ctypes.create_string_buffer(n.value)
mib=(ctypes.c_int*3)(1,49,pid)
if libc.sysctl(mib,3,buf,ctypes.byref(n),None,0):sys.exit(3)
raw=buf.raw[:n.value];argc=struct.unpack('i',raw[:4])[0]
parts=raw[4:].split(b'\0')
i=1
while i<len(parts) and parts[i]==b'':i+=1
for w in parts[i+argc:]:
    if b'=' in w:print(w.decode('utf8','replace'))
"""


def process_env(vm, pid):
    return guest(vm, 'python3 -c ' + shlex.quote(PROCARGS) + ' ' + str(pid) + ' 2>&1 || echo PROCARGS-FAILED', 30)


def children(vm, pid):
    """Every descendant of `pid` in the guest as (pid, environment-bearing command line)."""
    table = guest(vm, 'ps -axww -o pid=,ppid=', 30).splitlines()
    parent = {}
    for row in table:
        p = row.split()
        if len(p) == 2 and all(w.isdigit() for w in p):
            parent[int(p[0])] = int(p[1])
    mine = set()
    grew = True
    while grew:
        grew = False
        for child, par in parent.items():
            if child not in mine and (par == pid or par in mine):
                mine.add(child)
                grew = True
    found = []
    for line in guest(vm, 'ps -axeww -o pid=,command=', 30).splitlines():
        head = line.strip().split(None, 1)
        if len(head) == 2 and head[0].isdigit() and int(head[0]) in mine:
            found.append((int(head[0]), head[1]))
    return found


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--within', type=float, default=120)
    p.add_argument('--send-with-sha', metavar='SHA',
                   help='first send one message the way first-send-walk.py does (identity, first-run and the '
                        'Return-key send, built from SHA): a conversation lease exists only after a message')
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    if a.send_with_sha:
        sent = subprocess.run([str(HERE / 'first-send-walk.py'), a.vm, '--out', str(a.out / 'send'),
                               '--expect-sha', a.send_with_sha, '--steps', 'identity,first-run,return'],
                              timeout=900)
        if sent.returncode:
            raise SystemExit('the message did not reach Rich; there is no conversation lease to read')
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
    pid = (state / 'app.pid').read_text().strip()
    if not pid.isdigit():
        raise SystemExit('recorded app pid is not numeric')
    deadline = time.monotonic() + a.within
    record = {'vm': a.vm, 'app_pid': int(pid), 'want': WANT, 'workers': []}
    ok = False
    while True:
        record['workers'] = []
        for cpid, line in children(a.vm, int(pid)):
            if 'claude' not in line.lower():
                continue
            env = process_env(a.vm, cpid)
            seen = {k: (re.search(r'(?m)^' + k + r'=(\S*)$', env) or [None, None])[1] for k in WANT}
            record['workers'].append({'pid': cpid, 'command': line.split(' HOME=')[0][:1200], 'env': seen,
                                      'env_names': sorted(w.split('=', 1)[0] for w in env.splitlines() if '=' in w),
                                      'env_read': 'ok' if 'CLAUDE_CODE' in env or 'HOME=' in env else env[:200]})
            ok = ok or seen == WANT
        if ok or time.monotonic() >= deadline:
            break
        time.sleep(3)
    record['ok'] = ok
    (a.out / 'lease-env.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

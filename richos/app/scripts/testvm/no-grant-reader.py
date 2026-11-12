#!/usr/bin/env python3
"""no-grant-reader.py - what macOS and RichOS's LinkedIn reader do on a Mac that never allowed
RichOS Accessibility, measured on a vanilla macOS guest with System Integrity Protection on.

  no-grant-reader.py --bundle ZIP --out DIR --tart-home DIR [--image IMAGE] [--wait SECONDS]

WHY. The onboarding brief (richos-hq docs/briefs/2026-11-10-tuner-onboarding-round-2.md,
"Engineer assignment") asks what prompt appears, what continues and whether a user is prompted
again, on a clean Mac with no grant, and says a pregranted guest cannot answer it. The walk
guest (richos-base) has System Integrity Protection off and Accessibility rows for its own
tools; this guest is Cirrus Labs' vanilla image as downloaded, with nothing granted to anyone,
so what macOS shows here is what a new user's Mac shows. conversation-walk.py
--no-accessibility covers the same case through the whole app (specialist, Stu, Rich); this
covers what only a clean Mac can show: macOS's own prompt, and whether it comes back.

WHAT RUNS IN THE GUEST (as `admin`, never on this Mac): ZIP's RichOS.app is put in
/Applications and its reader is started the way Launch Services starts the app, as itself
(`open -n -a RichOS --args --linkedin-mcp`, its stdio through files), so macOS attributes the
request to com.richos.app. Three reads of a LinkedIn address, each given 90 s to answer:

  call 1  first read in a fresh reader process
  call 2  a second read in the SAME process (a retry, or another teammate of the session)
  call 3  a read in a NEW reader process (the next session's server)

macOS's TCC log is kept for the whole run and summarized (summary.txt): each Accessibility
request RichOS made (time, process) and each time macOS put its request window up or brought it
forward again (the window of `universalAccessAuthWarn`). The window list is not read: from the
guest's ssh session CGWindowList shows none of the GUI session's windows (both runs of
2026-11-10/12 listed nothing while the request was up). Nobody answers the request: this is the
user who ignores it. A Deny press is conversation-walk.py's guest
(it needs Accessibility for the harness, which this guest by design does not have).

DIR gets facts.txt, calls.txt (each call's time and the answer's first characters),
reader.*.out/.err (the JSON-RPC streams), tcc.log, summary.txt and drive.log. The guest is stopped and deleted
however the run ends, under a held test-VM slot (slots.py), for this run only.

--tart-home is where the vanilla image is cached (pulled on first use, a large download); it is
outside every repository. The image's login is Cirrus Labs' documented admin/admin.
"""
import argparse
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import slots  # noqa: E402

TART = str(Path.home() / '.richos-testvm/bin/tart.app/Contents/MacOS/tart')
IMAGE = 'ghcr.io/cirruslabs/macos-sequoia-vanilla:latest'
OPTS = ['-o', 'StrictHostKeyChecking=no', '-o', 'UserKnownHostsFile=/dev/null', '-o', 'PubkeyAuthentication=no',
        '-o', 'PreferredAuthentications=password,keyboard-interactive', '-o', 'ConnectTimeout=10', '-o', 'LogLevel=ERROR']

# Runs inside the guest. $1 is the LinkedIn address read.
GUEST = r'''#!/bin/bash
set -u
OUT=~/measure; rm -rf "$OUT"; mkdir -p "$OUT"; cd "$OUT"
APP=/Applications/RichOS.app
URL="$1"
{ echo "sip: $(csrutil status)"; echo "macos: $(sw_vers -productVersion) $(sw_vers -buildVersion)";
  codesign -dv "$APP" 2>&1 | grep -E "Identifier|TeamIdentifier|Signature"; } > facts.txt
echo admin | sudo -S -p '' true
sudo log stream --style compact --predicate 'subsystem == "com.apple.TCC" OR process == "universalAccessAuthWarn"' > tcc.log 2>&1 &
LOG=$!
sleep 2
start() {  # a reader process launched by Launch Services as RichOS; $1 names its files
  rm -f in.$1; mkfifo in.$1
  sleep 100000 > in.$1 &
  echo $! > hold.$1
  open -n -a "$APP" --stdin "$OUT/in.$1" --stdout "$OUT/reader.$1.out" --stderr "$OUT/reader.$1.err" --args --linkedin-mcp
  sleep 3
  printf '%s\n' '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}' > in.$1
  sleep 1
}
call() {  # $1 process, $2 id
  echo "== call $2 (process $1) at $(date -u +%FT%TZ)" >> calls.txt
  printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$2,\"method\":\"tools/call\",\"params\":{\"name\":\"read_profile\",\"arguments\":{\"url\":\"$URL\"}}}" > in.$1
  for i in $(seq 1 90); do
    grep -q "\"id\":$2[,}]" reader.$1.out 2>/dev/null && break
    sleep 1
  done
  echo "  answered at $(date -u +%FT%TZ): $(grep "\"id\":$2[,}]" reader.$1.out 2>/dev/null | cut -c1-600)" >> calls.txt
}
stop() { kill "$(cat hold.$1)" 2>/dev/null; sleep 3; }
start a; call a 1; call a 2; stop a
start b; call b 3; stop b
sleep 2
sudo kill "$LOG" 2>/dev/null
echo done > done.txt
'''


REQUEST = re.compile(r'^(\S+ [0-9:.]+) .*identifier=com\.richos\.app, pid=(\d+).*attempted to call TCCAccessRequest '
                     r'for kTCCServiceAccessibility')
WINDOW = re.compile(r'^(\S+ [0-9:.]+) .*universalAccessAuthWarn\[(\d+):[^]]*\] \[com\.apple\.AppKit:Window\] '
                    r'(Window <NSWindow[^>]*> windowNumber=(\w+) ordered front|order window: (\w+) op: 1 )')


def seconds(stamp):
    """`2026-11-12 05:34:25.724` as seconds since that day began."""
    h, m, s = stamp.split(' ')[1].split(':')
    return int(h) * 3600 + int(m) * 60 + float(s)


def summary(tcc_log):
    """The lines of summary.txt from the guest's TCC log: each of RichOS's Accessibility requests
    (tccd logs each one twice, folded into one), and each time macOS's request window was put up or
    brought forward, with its window number (the same number again is the same window, re-raised)."""
    out, last_ask, shown = [], {}, set()
    for line in tcc_log.splitlines():
        m = REQUEST.match(line)
        if m:
            # tccd logs one request as two lines about 13 ms apart; a request from the same process
            # more than a second later is a request of its own.
            at = seconds(m.group(1))
            previous = last_ask.get(m.group(2))
            last_ask[m.group(2)] = at
            if previous is None or at - previous > 1:
                out.append('%s RichOS (pid %s) asked macOS for Accessibility' % (m.group(1), m.group(2)))
        m = WINDOW.match(line)
        if m:
            number = m.group(4) or m.group(5)
            key = (m.group(2), number, line[:19])
            if key in shown:
                continue
            shown.add(key)
            out.append('%s macOS request window %s (universalAccessAuthWarn pid %s) %s' % (
                m.group(1), number, m.group(2), 'put up' if m.group(4) else 'brought forward again'))
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--bundle', type=Path, required=True, help="a RichOS.app zip (branch-bundle.sh's, or a release's)")
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--tart-home', type=Path, required=True)
    p.add_argument('--image', default=IMAGE)
    p.add_argument('--url', default='https://www.linkedin.com/in/williamhgates/',
                   help='the address read (nothing is opened without the permission)')
    p.add_argument('--wait', type=float, default=1800, help='seconds to wait for a guest slot (default 1800)')
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TART_HOME=str(a.tart_home))
    log = open(a.out / 'drive.log', 'a')

    def say(*words):
        print(*words, flush=True)
        print(*words, file=log, flush=True)

    def tart(*args, **k):
        return subprocess.run([TART, *args], env=env, capture_output=True, text=True, **k)

    with tempfile.TemporaryDirectory(prefix='no-grant-') as tmp:
        ask = Path(tmp) / 'askpass.sh'
        ask.write_text('#!/bin/sh\necho admin\n')
        ask.chmod(0o700)
        script = Path(tmp) / 'measure.sh'
        script.write_text(GUEST)
        ssh_env = dict(os.environ, SSH_ASKPASS=str(ask), SSH_ASKPASS_REQUIRE='force', DISPLAY=':0')

        def ssh(ip, cmd, timeout=120):
            return subprocess.run(['ssh', *OPTS, f'admin@{ip}', cmd], env=ssh_env, capture_output=True, text=True,
                                  timeout=timeout, stdin=subprocess.DEVNULL)

        def scp(src, dst, timeout=600):
            return subprocess.run(['scp', '-r', *OPTS, src, dst], env=ssh_env, capture_output=True, text=True,
                                  timeout=timeout, stdin=subprocess.DEVNULL).returncode

        vm = 'no-grant-' + uuid.uuid4().hex[:8]
        with slots.guest_slot(wait_seconds=a.wait, purpose='no-grant-reader.py: clean-Mac Accessibility measurement') as slot:
            say('slot', slot, 'vm', vm)
            r = tart('clone', a.image, vm)
            say('clone', r.returncode, r.stderr[-300:])
            if r.returncode:
                return 1
            tart('set', vm, '--cpu', '4', '--memory', '5120')
            run = subprocess.Popen([TART, 'run', '--no-graphics', vm], env=env, stdout=log, stderr=log)
            say('tart run pid', run.pid)
            try:
                ip = None
                for _ in range(120):
                    r = tart('ip', vm, '--wait', '5')
                    if r.returncode == 0 and r.stdout.strip():
                        ip = r.stdout.strip()
                        break
                    time.sleep(2)
                say('ip', ip)
                if not ip:
                    return 1
                for _ in range(60):
                    if ssh(ip, 'true', 20).returncode == 0:
                        break
                    time.sleep(5)
                # macOS shows its request only to a logged-in GUI session.
                for _ in range(60):
                    r = ssh(ip, 'pgrep -x Finder >/dev/null && echo finder')
                    if 'finder' in r.stdout:
                        break
                    time.sleep(5)
                say('gui session', r.stdout.strip() or 'none')
                say('copy bundle', scp(str(a.bundle), f'admin@{ip}:/tmp/RichOS.zip'))
                say('copy script', scp(str(script), f'admin@{ip}:/tmp/measure.sh'))
                r = ssh(ip, 'cd /tmp && ditto -x -k RichOS.zip /Applications && ls -d /Applications/RichOS.app', 300)
                say('install', r.returncode, r.stdout.strip(), r.stderr[-200:])
                r = ssh(ip, 'bash /tmp/measure.sh ' + a.url, 900)
                say('measure', r.returncode, r.stderr[-300:])
                say('pull', scp(f'admin@{ip}:measure/.', str(a.out)))
                if (a.out / 'tcc.log').exists():
                    lines = summary((a.out / 'tcc.log').read_text(errors='replace'))
                    (a.out / 'summary.txt').write_text('\n'.join(lines) + '\n')
                    say('summary:\n  ' + '\n  '.join(lines))
            finally:
                say('stop', tart('stop', vm).returncode)
                try:
                    run.wait(timeout=120)
                except subprocess.TimeoutExpired:
                    run.kill()
                say('delete', tart('delete', vm).returncode)
    return 0 if (a.out / 'done.txt').exists() else 1


if __name__ == '__main__':
    sys.exit(main())

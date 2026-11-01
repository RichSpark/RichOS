#!/usr/bin/env python3
"""count-walk.py — what the app does on its own after a plain start, measured in the guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE_TARBALL -- \\
      count-walk.py --out DIR [--after SECONDS]

run-walk.py passes the owned VM name as the first argument.

WHAT IT ANSWERS (the CEO's 2026-11-01 complaint: "6 agents running" after only starting the app):
  agents_working   `agentsWorking` in <app data>/engine-state/claude-quota.json, as the app wrote it
                   --after seconds (default 60) after launch. A start with nothing running is 0.
  watchers         processes in the guest whose command line names the App Management watcher
                   (must be none: the app no longer starts one).
  app_rss_mb       the app's resident memory at that moment, from `ps` on the recorded pid.
  claude_processes `claude` processes the start left running (a start asks Claude for nothing).

Nothing is planted: a lease session exists only while a Claude process runs, so the launch-then-resume
journal case is the unit test's (richos-core app_workers). Writes <out>/count.json. Exit 0 when
agents_working is 0 (or the file is not written yet and no lease exists) and no watcher runs.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
"""
import argparse
import json
import os
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402


def utc():
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--after', type=float, default=60, help='seconds after launch to measure (at most 600)')
    a = p.parse_args()
    if not 0 <= a.after <= 600:
        p.error('--after must be from 0 to 600 seconds')
    a.out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
    pid = (state / 'app.pid').read_text().strip()
    if not pid.isdigit():
        raise SystemExit('recorded app pid is not numeric')
    time.sleep(a.after)
    record = {'vm': a.vm, 'measured': utc(), 'after_seconds': a.after, 'app_pid': int(pid)}
    record['app_alive'] = guest(a.vm, f'ps -p {pid} -o comm= 2>/dev/null || true').endswith('/richos-tauri')
    rss = guest(a.vm, f'ps -p {pid} -o rss= 2>/dev/null || true').strip()
    record['app_rss_mb'] = round(int(rss) / 1024, 1) if rss.isdigit() else None
    where = guest(a.vm, 'find "$HOME" -name claude-quota.json -path "*engine-state*" 2>/dev/null | head -1 || true')
    record['quota_file'] = where or None
    quota = guest(a.vm, f"cat '{where}' 2>/dev/null || true") if where else ''
    record['engine_state_dirs'] = guest(a.vm, 'find "$HOME/Library/Application Support" -maxdepth 3 -name engine-state '
                                              '2>/dev/null || true').split()
    try:
        record['agents_working'] = json.loads(quota).get('agentsWorking') if quota else None
    except ValueError:
        record['agents_working'] = None
    record['quota_file_present'] = bool(quota)
    watchers = guest(a.vm, "ps -axo pid=,command= | grep -c '[a]pp_management_watch' || true")
    record['watchers'] = int(watchers) if watchers.isdigit() else None
    claudes = guest(a.vm, "ps -axo pid=,command= | grep -E '(^| )[^ ]*/claude( |$)' | grep -vc grep || true")
    record['claude_processes'] = int(claudes) if claudes.isdigit() else None
    (a.out / 'count.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))
    ok = record['app_alive'] and record['watchers'] == 0 and record['agents_working'] in (0, None)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

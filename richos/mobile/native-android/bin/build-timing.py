#!/usr/bin/env python3
"""Time this checkout's Android release build the way an agent's job meets it.

    build-timing.py [--edit FILE]

Runs `randroid build release` four times, in order, and prints one JSON object:

  prime       untimed in spirit (reported, not compared): brings the outputs up to date and,
              when native-work keeps a Gradle daemon, starts it
  no-op       nothing changed since prime
  incremental a one-line Kotlin edit (a new constant in FILE): Kotlin recompile, dex and R8 shrink
  revert      the edit taken out again, so the outputs match the source after it; Gradle's build
              cache usually already holds these outputs, so this row is NOT a second incremental

The edit is ALWAYS taken out (the file's original bytes are written back whatever happens), and
the script refuses to start on a FILE with uncommitted changes, so it can never leave an edit
behind or mix with somebody's work. Every row carries the total host CPU the watchdog's heartbeat
reported just before the build and native-work's own admission line (cores granted, busy at
admission), because a build time means nothing without the load it ran under.

It runs nothing itself but `randroid`, so admission, the compiler lane and the watchdog apply as
for any build. It does not touch a device.
"""
import argparse
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import machine_storage as M
import re
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
RANDROID = Path(os.environ.get('BUILD_TIMING_RANDROID') or HERE / 'randroid')   # override: its test only
DEFAULT_EDIT = HERE.parent / 'app/src/main/kotlin/dev/richos/android/app/PerformanceMarks.kt'
HEARTBEAT = M.state('cpu-guard', 'RICHOS_CPU_GUARD_STATE') / 'heartbeat.json'


def host_busy():
    try:
        beat = json.loads(HEARTBEAT.read_text())
    except (OSError, ValueError):
        return None
    return beat.get('host_busy') if time.time() - beat.get('at', 0) < 12 else None


def build(label):
    busy = host_busy()
    started = time.monotonic()
    result = subprocess.run([str(RANDROID), 'build', 'release'], capture_output=True, text=True)
    seconds = round(time.monotonic() - started, 1)
    admission = [line for line in result.stderr.splitlines() if line.startswith('native-work:')]
    # native-work says how long admission (machine worker, compiler lane, CPU headroom) took;
    # the build's own time is the rest.
    waited = [float(m.group(1)) for line in admission for m in [re.search(r'admitted after ([0-9.]+) s', line)] if m]
    row = {'build': label, 'seconds': seconds, 'exit': result.returncode,
           'admitted_after': waited[0] if waited else None,
           'build_seconds': round(seconds - waited[0], 1) if waited else None,
           'host_busy_before': busy, 'native_work': admission}
    if result.returncode:
        row['stderr_tail'] = result.stderr[-2000:]
    print(json.dumps(row), file=sys.stderr, flush=True)
    return row


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--edit', default=str(DEFAULT_EDIT))
    args = parser.parse_args(argv)
    target = Path(args.edit).resolve()
    dirty = subprocess.run(['git', '-C', str(target.parent), 'status', '--porcelain', '--', str(target)],
                           capture_output=True, text=True, check=True).stdout.strip()
    if dirty:
        print('build-timing: %s has uncommitted changes; refusing to edit it' % target, file=sys.stderr)
        return 2
    original = target.read_bytes()
    rows = [build('prime'), build('no-op')]
    try:
        probe = '\ninternal const val BUILD_TIMING_PROBE = "%s"\n' % uuid.uuid4().hex
        target.write_bytes(original + probe.encode())
        rows.append(build('incremental'))
    finally:
        target.write_bytes(original)
    rows.append(build('revert'))
    print(json.dumps({'checkout': str(HERE.parents[3]), 'edited': str(target), 'builds': rows}, indent=2))
    return 0 if all(r['exit'] == 0 for r in rows) else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""ghost-walk.py — the home screen of a user with a team and a big corpus, on the real app, over three launches.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      ghost-walk.py --out DIR [--add 20]

FIXTURE_HOME is a gui_boot_machine home with a team (seed-team.sh) and N records (seed-records.sh, about 1,000:
a compile of that many takes about 40 s, far past the 8 s the home screen waits).

WHAT IT SHOWS (CEO 2026-11-02, round 11.4 v2, "The Ghost"):
  launch 1   first launch, nothing kept yet: a frame at 10 s (the example and its legend; the screen did not
             wait) and a frame once the compile has finished and been kept (his records drawn, in gold).
  launch 2   the same corpus, relaunched: a frame 20 s after the launch, far less than the compile takes, so
             his records are drawn from the kept result.
  launch 3   --add more records written in the guest between the launches, relaunched and given the compile
             time again: a frame with the new records. Nothing already on the screen may have moved; the host
             compares the frames (compare-frames.py).
It quits nothing itself: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
Writes <out>/ghost.json and the frames <out>/launch{1-early,1-ready,2,3}.png.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402


def shot(vm, out):
    subprocess.run([str(HERE / 'shot.sh'), vm, str(out)], check=True, timeout=120)


def ax(vm, *args):
    r = subprocess.run([str(HERE / 'ax.sh'), vm, *args, '--json'], capture_output=True, text=True, timeout=60)
    return r.returncode == 0 and any(l.startswith('{') for l in r.stdout.splitlines())


def to_home(vm):
    """Decline the first-run sheets ("Not now", up to three), then press the RichOS mark: the home screen."""
    for _ in range(3):
        if ax(vm, 'click', '--title', 'Not now', '--role', 'AXButton', '--first'):
            time.sleep(3)
        else:
            break
    for _ in range(5):
        if ax(vm, 'click', '--title', 'go to the home screen', '--contains', '--first'):
            return True
        time.sleep(3)
    return False


def cache_state(vm):
    """(mtime, size) of the kept compile in the guest, or None."""
    payload = (Path.home() / '.richos-testvm' / 'run' / vm / 'payload').read_text().strip()
    row = guest(vm, 'f="$(find "' + payload + '" -name home-field-cache.json 2>/dev/null | head -1)"; '
                    '[ -n "$f" ] && stat -f "%m %z %N" "$f" || true', 60)
    return tuple(row.split(None, 2)) if row else None


def wait_for_cache(vm, changed_from, limit):
    began = time.monotonic()
    while time.monotonic() - began < limit:
        now = cache_state(vm)
        if now and (changed_from is None or now[0] != changed_from[0]):
            return now, round(time.monotonic() - began, 1)
        time.sleep(3)
    return None, round(time.monotonic() - began, 1)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--add', type=int, default=20, help='records written between launch 2 and launch 3')
    p.add_argument('--compile-limit', type=float, default=420)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    rec = {'vm': a.vm}

    time.sleep(10)
    rec['home_opened_1'] = to_home(a.vm)
    time.sleep(3)
    shot(a.vm, a.out / 'launch1-early.png')
    rec['kept_at_10s'] = cache_state(a.vm)

    kept, waited = wait_for_cache(a.vm, None, a.compile_limit)
    rec['launch1_compile_kept_after_s'] = waited
    rec['launch1_kept'] = kept
    time.sleep(12)  # the screen rebuilds with his records
    shot(a.vm, a.out / 'launch1-ready.png')

    relaunch(a.vm)
    time.sleep(3)
    rec['home_opened_2'] = to_home(a.vm)
    time.sleep(14)
    shot(a.vm, a.out / 'launch2.png')
    rec['launch2_kept'] = cache_state(a.vm)
    rec['launch2_cache_untouched'] = bool(kept and rec['launch2_kept'] and kept[0] == rec['launch2_kept'][0])

    if a.add:
        # records appended in the guest, with the guest's own writer (seed-records.sh adds only what is missing)
        payload = (Path.home() / '.richos-testvm' / 'run' / a.vm / 'payload').read_text().strip()
        subprocess.run([str(HERE / 'guest.sh'), a.vm, '--push', str(HERE / 'seed-records.sh'), payload + '/'], check=True, timeout=120)
        have = int(guest(a.vm, 'ls "' + payload + '/home/RichOS/corpus/companies/northwind/records" | wc -l', 60))
        rec['records_before_add'] = have
        out = guest(a.vm, 'cd "' + payload + '" && RICHOS_LORO_DIR="$(dirname "$(dirname "$(find "' + payload + '" -name loro-write.mjs -path "*/bin/*" 2>/dev/null | head -1)")")" '
                    'PATH="/opt/homebrew/bin:/usr/local/bin:$PATH" '
                    'sh "' + payload + '/seed-records.sh" "' + payload + '/home" ' + str(have + a.add) + ' northwind 2>&1 | tail -3', 900)
        rec['add_output'] = out
        relaunch(a.vm)
        time.sleep(3)
        rec['home_opened_3'] = to_home(a.vm)
        again, waited3 = wait_for_cache(a.vm, rec['launch2_kept'], a.compile_limit)
        rec['launch3_compile_kept_after_s'] = waited3
        rec['launch3_kept'] = again
        time.sleep(12)
        shot(a.vm, a.out / 'launch3.png')

    (a.out / 'ghost.json').write_text(json.dumps(rec, indent=2) + '\n')
    print(json.dumps(rec))
    return 0


if __name__ == '__main__':
    sys.exit(main())

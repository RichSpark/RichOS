#!/usr/bin/env python3
"""launch-speed-walk.py: from the Dock click to the splash screen showing, p95 against 380 ms.

THE CEO, 2026-11-14: "THE JOB AT HAND IS TO START SHOWING OUR ALREADY WELL-DESIGNED AND
FULLY-BUILT SPLASH SCREEN IN WELL UNDER 500 MS AFTER BUTTON CLICK." And his rule: "380 ms as the
maximum allowed p95 time for the RichOS app to start showing the splash screen. That's all there is
to it." (380 ms was Dictionary's slowest measured Dock launch on his Mac, Echo's audit: 287, 305,
318, 318, 328 and 380 ms; Dictionary is not launched beside the app.)
Every launch is timed from the launch request to the splash playing, plus ALLOWANCE_MS for the
Dock's own part of a click and the screen's next refresh: the splash's opening on the glass (the
app's launch line, "first frame" with "its opening used", Sage's plan 2026-11-14 "the splash the
rules picked is playing within 380 ms of the click", section 6), or, when the opening was not used,
the page's own splash painted and playing ("page took over").

Run by run-walk.py with --no-app, so the walk installs what it launches and the first launch after
the boot is its own; never on the host's screen (CEO ruling §65):

  run-walk.py --no-app --home FIXTURE --engine ENGINE [--previous-bundle OLDER.zip] --report R -- \\
      launch-speed-walk.py --candidate CANDIDATE.zip --out DIR [--runs 20] [--checks] [--report-only]

run-walk.py passes the owned VM name first and, with --previous-bundle, appends --previous-app.

THE ROUTES (Sage's plan 2026-11-14, section 5), each started as a Dock click starts it (`open -a`,
through LaunchServices), as a regular launch (an unattended one builds its window hidden):
  ordinary        a returning user's launch of the candidate installed in /Applications, --runs
                  times (its very first launch is reported, not judged);
  cold            the first launch after the guest starts, with the disk cache flushed by
                  `sudo purge`, because staging the bundle had just read every file;
  updated-from-applications
                  with --previous-app, the CEO's own situation: an older copy in /Applications (the
                  Dock's target) handing over to the candidate in ~/Applications with the receipt an
                  update leaves, --runs times (its first launch is reported, not judged);
  after-update    with --previous-app, the plan's route 3: the older copy installed in
                  ~/Applications as an update leaves it, the candidate prepared as its update (the
                  receipts the updater writes, derived as it derives them), the older copy launched
                  and quit so its helper puts the candidate in place at quit, then the candidate
                  launched: that launch is judged, --runs times;
  rule-switch     the plan's route 5, --runs fresh homes: the first start (no saved pick, reported),
                  then start 2, whose quit before it picked the other splash (#2) and drew its
                  opening, then start 3 (#1 again): both judged, so every splash today's rule can
                  pick is timed at least --runs times;
  fallbacks       reported once each, not judged (the plan's section 9): the first start after an
                  install, and the first start after an update that changed the splash's own files
                  (the saved opening's fingerprint made to differ, which is what such an update does);
  checks          with --checks: a start-up step made to fail (its conversation record unreadable),
                  which must end in the start-up alert and the exit; a quit from the app's own menu
                  during start-up; then a normal launch of the same home.

TIMES COME FROM LOG LINES, never from frames (Sage's review, piece 4, item 1): the app's launch
line, and for builds older than it, the guest's system log (WebKit's document loaded, the page's
splash). One untimed frame per route shows what was on screen.

THE RULE, the CEO's (2026-11-14): "380 ms as the maximum allowed p95 time for the RichOS app to
start showing the splash screen." The p95 of every judged launch, every route together, over
the limit refuses: LIMIT_MS times Dictionary's slowness in the same guest, compared like for like
(the CEO: "if the VM is slower, then the measurements need to be adjusted accordingly"): its median
on his Mac from the Dock click to its window, against its median in the guest from the same
starting point (the guest's launch-request median plus the Dock's own part measured on his Mac),
never a p95 a single cold outlier sets; the ratio and its inputs are recorded beside the limit.
Every route's own p95 and median, and every splash's, are stated. 20 launches a route by default,
so a p95 is a real 95th percentile and not the slowest of a handful.

Every instance it starts is quit by the pid captured at spawn and checked gone in the guest.
Exit 0 on a pass; 1 when a route is refused (unless --report-only) or a check failed; 2 on a harness
failure (nothing measured, or an instance left running).
"""
import argparse
import hashlib
import json
import os
import re
import shlex
import statistics
import struct
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
# The CEO's rule (2026-11-14): at most 380 ms p95 from the click to the splash showing (his
# benchmark was Dictionary's measured Dock launch on his Mac, at most 380 ms).
LIMIT_MS = 380
# THE GUEST IS SLOWER THAN HIS MAC, SO THE LIMIT IS SCALED, ONCE PER WALK (the CEO, 2026-11-14: "if
# the VM is slower, then the measurements need to be adjusted accordingly. This is NOT about some 100
# or 200 ms."), LIKE FOR LIKE AND BY MEDIANS: Dictionary's window was ordered front 287, 305, 318, 318,
# 328 and 380 ms after the Dock click on his Mac (Echo's audit, `richos-hq/docs/research/2026-11-14-
# app-launch-speed-audit.md`, "Dictionary, same Mac, same Dock"), median MAC_DICTIONARY_MEDIAN_MS; the
# walk launches Dictionary DICTIONARY_RUNS times in the same guest and times its window from the
# launch request, and adds the Dock's own part before the request (44, 41, 43 and 54 ms on his Mac,
# the audit's Dock-click table, median DOCK_PART_MS). The limit is LIMIT_MS x (that guest median /
# MAC_DICTIONARY_MEDIAN_MS), never below LIMIT_MS, recorded with its inputs.
MAC_DICTIONARY_MS = (287, 305, 318, 318, 328, 380)
MAC_DICTIONARY_MEDIAN_MS = statistics.median(MAC_DICTIONARY_MS)
DOCK_PART_INPUTS_MS = (44, 41, 43, 54)
DOCK_PART_MS = statistics.median(DOCK_PART_INPUTS_MS)
DICTIONARY_RUNS = 10
DICTIONARY = '/System/Applications/Dictionary.app'
# The splash today's rule picks for each start (richos-core `launch::pick_splash`): start 2 shows the
# second entry, every other start the first.
SPLASH_FOR_START = {2: 'round-11/v2'}
SPLASH_OTHERWISE = 'round-11/v1'
# A p95 is a 95th percentile only over enough launches: fewer judged launches than this refuse.
MIN_LAUNCHES = 20
# Added to every judged stamp: the Dock's own part of a click
# (41 to 54 ms on the CEO's Mac, the audit) and the screen's next refresh (one 60 Hz frame).
ALLOWANCE_MS = 54 + 17
INSTALLED = '/Applications/RichOS.app'
LAUNCH_LINE = 'launch timing:'
LOG_MARKS = (
    ('window', 'order window front conditionally'),
    ('page_committed', 'didCommitLoadForFrame'),
    ('document_loaded', 'didFinishDocumentLoadForFrame'),
)
SYSLOG = re.compile(r'^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3})\S*\s+\S+\s+(\S+?)\[(\d+):\w+\] (.*)')
# The app's launch line (src-tauri/src/launch_timing.rs), field by field.
APP_FIELDS = {
    'first_frame': re.compile(r'first frame (\d+) ms'),
    'page': re.compile(r'page took over (\d+) ms'),
    'bundle': re.compile(r'(bundle check .*?); engine check'),
    'engine': re.compile(r'engine check (.*?); process started at'),
    'started': re.compile(r'process started at (\d+)'),
    # Which splash, and whether its opening played (Sage's plan 2026-11-14, section 6).
    'splash_id': re.compile(r'splash shown, (\S+) \('),
    'opening': re.compile(r'its opening used \((\d+) later pictures\)'),
    'opening_not': re.compile(r'its opening not used: (.*?)(?:, the page joined it at \d+ ms)?; update put in place'),
    'joined': re.compile(r'the page joined it at (\d+) ms'),
}


def app_line(text):
    """The launch line's fields, or None when `text` is not a complete launch line."""
    found = {k: (m.group(1) if (m := r.search(text)) else None) for k, r in APP_FIELDS.items()}
    return found if found['first_frame'] and found['started'] else None


def syslog_events(text, utc_offset_seconds=0):
    """(epoch seconds, process, pid, message) for each line of `log show --style compact`, whose
    stamps are the guest's local time; `utc_offset_seconds` is the guest's offset (+0100 = 3600)."""
    out = []
    for line in text.splitlines():
        m = SYSLOG.match(line)
        if m:
            local = datetime.strptime(m.group(1), '%Y-%m-%d %H:%M:%S.%f').replace(tzinfo=timezone.utc)
            out.append((local.timestamp() - utc_offset_seconds, m.group(2), int(m.group(3)), m.group(4)))
    return out


def utc_offset(text):
    """`date +%z` (+0100, -0700) in seconds."""
    m = re.fullmatch(r'([+-])(\d\d)(\d\d)', text.strip())
    if not m:
        return 0
    seconds = int(m.group(2)) * 3600 + int(m.group(3)) * 60
    return seconds if m.group(1) == '+' else -seconds


def measure(run, events):
    """Milliseconds after `open` was started, for one launch. Pure over its inputs. The judged
    moment, `splash`, is the page's splash playing ("page took over" in the app's launch line), plus
    ALLOWANCE_MS. A build with no
    launch line (one from before it) falls back to WebKit's document loaded, its page's splash."""
    t0 = run['t_open']
    row = {'label': run['label'], 'pid': run['pid']}
    mine = [(t, msg) for t, _proc, pid, msg in events if pid == run['pid']]
    for key, needle in LOG_MARKS:
        hit = next((t for t, msg in mine if needle in msg), None)
        if hit is not None:
            row[key] = round((hit - t0) * 1000)
    app = None
    for entry in run.get('lines', []):
        if LAUNCH_LINE in entry['text']:
            app = app_line(entry['text']) or app
    if app:
        started = int(app['started']) / 1000
        row['process_start'] = round((started - t0) * 1000)
        row['first_frame'] = round((started + int(app['first_frame']) / 1000 - t0) * 1000)
        if app['page']:
            row['page_took_over'] = round((started + int(app['page']) / 1000 - t0) * 1000)
        row['bundle_check'] = app['bundle']
        row['engine_check'] = app['engine']
        row['splash_id'] = app['splash_id']
        row['opening'] = int(app['opening']) if app['opening'] else None
        row['opening_not_used'] = app['opening_not']
        row['joined_ms'] = int(app['joined']) if app['joined'] else None
    expect = run.get('expect_splash')
    if expect and row.get('splash_id') != expect:
        # The wrong splash is not the splash: no time is judged for it, so the route refuses.
        row['wrong_splash'] = 'expected %s, the launch line says %s' % (expect, row.get('splash_id'))
        return row
    # THE CEO, 2026-11-14: "THE JOB IS TO START SHOWING OUR EXISTING SPLASH SCREEN ... IN WELL UNDER
    # 500 MS AFTER BUTTON CLICK." So the judged moment is the splash playing: its opening's first
    # picture pushed to the glass, already playing (`first frame` with `its opening used`, Sage's plan
    # 2026-11-14 "the splash the rules picked is playing within 380 ms of the click"); without the
    # opening, the page's own splash painted and playing ("page took over"). The window's ground
    # color alone does not count.
    if row.get('opening') is not None and 'first_frame' in row:
        row['splash'], row['splash_from'] = row['first_frame'] + ALLOWANCE_MS, "the splash's opening on the glass"
    elif 'page_took_over' in row:
        row['splash'], row['splash_from'] = row['page_took_over'] + ALLOWANCE_MS, "the page's splash playing"
    elif run['label'].startswith('dictionary') and 'window' in row:
        row['splash'], row['splash_from'] = row['window'] + ALLOWANCE_MS, "AppKit's window ordered front"
    elif 'document_loaded' in row and 'process_start' not in row:
        # Only a build older than the launch line: a build that writes it and did not report its
        # splash is missing evidence, never timed by WebKit's document instead (review
        # rv-20261114T143754Z-7a96691c-3106, finding 3).
        row['splash'], row['splash_from'] = row['document_loaded'] + ALLOWANCE_MS, "WebKit's document loaded"
    return row


def p95(values):
    """The 95th percentile by nearest rank: the smallest value at least 95% of launches are at or
    under. With 20 launches it is the 19th fastest; with fewer than 20 it is the slowest."""
    ordered = sorted(values)
    return ordered[max(0, -(-len(ordered) * 95 // 100) - 1)]


def verdict(rows, planned=None, limit_ms=LIMIT_MS, ratio=None, inputs=None):
    """The CEO's rule, 2026-11-14: "380 ms as the maximum allowed p95 time for the RichOS app to
    start showing the splash screen." The p95 of each judged route, and of every judged launch
    together, against the limit: any one over it refuses.

    MISSING EVIDENCE NEVER PASSES (review rv-20261114T115101Z-10e7c6ac-7689, finding 1): a judged
    launch with no timing, a planned route with no launches (`planned`, {route: launches}), or
    fewer than MIN_LAUNCHES judged launches in all refuses, and says which."""
    judged, missing = {}, []
    for row in rows:
        route, _, kind = row['label'].partition(':')
        if kind != 'judged' or route == 'dictionary':
            continue
        if 'splash' in row:
            judged.setdefault(route, []).append(row['splash'])
        else:
            missing.append(row['label'] + (' (%s)' % row['wrong_splash'] if row.get('wrong_splash') else ''))
    for route, count in sorted((planned or {}).items()):
        got = len(judged.get(route, [])) + sum(1 for m in missing if m.startswith(route + ':'))
        if got < count:
            missing += ['%s:not launched' % route] * (count - got)
    every = [v for values in judged.values() for v in values]
    summary = {'limit_ms': limit_ms, 'guest_ratio': ratio, 'guest_inputs': inputs, 'launches': len(every),
               'p95_ms': p95(every) if every else None, 'routes': {}, 'reported': {}, 'missing': missing}
    summary['incomplete'] = bool(missing) or len(every) < MIN_LAUNCHES
    for route, values in sorted(judged.items()):
        summary['routes'][route] = {'p95_ms': p95(values), 'median_ms': statistics.median(values),
                                    'max_ms': max(values), 'launches': len(values)}
    # EVERY JUDGED ROUTE ON ITS OWN, and all of them together: one fast route never hides a slow
    # one (review rv-20261114T143754Z-7a96691c-3106, finding 2).
    summary['routes_over'] = sorted(r for r, s in summary['routes'].items() if s['p95_ms'] > limit_ms)
    summary['refused'] = summary['incomplete'] or summary['p95_ms'] > limit_ms or bool(summary['routes_over'])
    # Every splash the rules picked, on its own: the hundredth splash must cost what the first does.
    by_splash = {}
    for row in rows:
        route, _, kind = row['label'].partition(':')
        if kind == 'judged' and route != 'dictionary' and 'splash' in row and row.get('splash_id'):
            by_splash.setdefault(row['splash_id'], []).append(row['splash'])
    summary['splashes'] = {sid: {'p95_ms': p95(v), 'median_ms': statistics.median(v), 'launches': len(v)}
                           for sid, v in sorted(by_splash.items())}
    for row in rows:
        if row['label'].endswith(':first') and 'splash' in row:
            summary['reported'][row['label']] = row['splash']
    return summary


def sentence(summary):
    """What the nightly prints: the p95 against 380 ms, and every route's own numbers."""
    parts = ['%s p95 %d ms, median %d ms (%d launches)' % (route, r['p95_ms'], r['median_ms'], r['launches'])
             for route, r in sorted(summary['routes'].items())]
    parts += ['%s %d ms (reported, not judged)' % (label, ms) for label, ms in sorted(summary['reported'].items())]
    head = ('nothing measured' if summary['p95_ms'] is None else
            'p95 %d ms over %d launches, %s' % (summary['p95_ms'], summary['launches'],
                                                'REFUSED' if summary['refused'] else 'within it'))
    if summary.get('routes_over'):
        head += '; OVER THE LIMIT ON ITS OWN: ' + ', '.join(summary['routes_over'])
    if summary.get('incomplete'):
        head += ('; REFUSED, THE MEASUREMENT IS INCOMPLETE: %d judged launch(es) without a timing (%s), '
                 '%d launches where at least %d are required' % (
                     len(summary['missing']), ', '.join(sorted(set(summary['missing']))) or 'none',
                     summary['launches'], MIN_LAUNCHES))
    inputs = summary.get('guest_inputs') or {}
    scaled = ('' if not summary.get('guest_ratio') else
              ' (%d ms on his Mac x %.2f, Dictionary\'s slowness in this guest: its median %g ms from the Dock click here, '
              'the launch request\'s %g ms plus the Dock\'s %g, against %g ms on his Mac)' % (
                  LIMIT_MS, summary['guest_ratio'], inputs.get('guest_median_from_click_ms'),
                  inputs.get('guest_median_from_request_ms'), inputs.get('dock_part_ms'),
                  inputs.get('mac_median_from_click_ms')))
    parts += ['%s p95 %d ms (%d launches)' % (sid, s['p95_ms'], s['launches'])
              for sid, s in sorted(summary.get('splashes', {}).items())]
    return 'Launch speed, Dock click to the splash showing, against a p95 of %d ms%s: %s. %s.' % (
        summary['limit_ms'], scaled, head, '; '.join(parts))


def guest_limit(rows):
    """(limit in ms, ratio, inputs) from the Dictionary calibration launches, LIKE FOR LIKE: its
    median from the launch request to its window in the guest, plus the Dock's own part measured on
    his Mac, against its median from the Dock click to its window on his Mac. (LIMIT_MS, None, None)
    without them. A guest faster than his Mac never tightens the limit below LIMIT_MS."""
    windows = [r['window'] for r in rows if r['label'] == 'dictionary:calibration' and 'window' in r]
    if not windows:
        return LIMIT_MS, None, None
    guest = statistics.median(windows) + DOCK_PART_MS
    # Rounded ONCE, and the limit is made from the rounded ratio: publication (nightly-local.py,
    # `round(380 * ratio)` from the recorded ratio) re-derives exactly the same limit (review
    # rv-20261114T153837Z-b8423a44-96f8).
    ratio = round(max(1.0, guest / MAC_DICTIONARY_MEDIAN_MS), 3)
    inputs = {'guest_window_from_request_ms': sorted(windows), 'guest_median_from_request_ms': statistics.median(windows),
              'dock_part_ms': DOCK_PART_MS, 'dock_part_inputs_ms': list(DOCK_PART_INPUTS_MS),
              'guest_median_from_click_ms': guest, 'mac_median_from_click_ms': MAC_DICTIONARY_MEDIAN_MS,
              'mac_inputs_ms': list(MAC_DICTIONARY_MS), 'ratio': round(guest / MAC_DICTIONARY_MEDIAN_MS, 3)}
    return round(LIMIT_MS * ratio), ratio, inputs



# The guest's plans, in the order they run (each its own probe and folder).
PLANS = ('dictionary', 'fresh', 'switch', 'checks', 'updated', 'after')


def bmp_pixels(jpg):
    """A JPEG frame as (width, height, rows of BGR bytes), through the host's own `sips`."""
    bmp = str(jpg) + '.bmp'
    subprocess.run(['sips', '-s', 'format', 'bmp', str(jpg), '--out', bmp], capture_output=True, check=True, timeout=60)
    data = Path(bmp).read_bytes()
    os.unlink(bmp)
    offset, width, height = struct.unpack_from('<I', data, 10)[0], *struct.unpack_from('<ii', data, 18)
    bpp = struct.unpack_from('<H', data, 28)[0] // 8
    stride = (width * bpp + 3) & ~3
    rows = [data[offset + y * stride: offset + y * stride + width * bpp] for y in range(abs(height))]
    return width, abs(height), bpp, rows if height < 0 else rows[::-1]


def splash_playing(fresh_dir):
    """IT IS THE SPLASH, AND IT IS PLAYING (Sage's plan, section 6): the two pictures of the
    `ordinary:frames` launch, a quarter of a second apart. Neither may be empty or white, and the
    middle of the screen, where the splash rises, must have moved between them. Reported with the
    numbers; the pictures are kept beside it to be looked at."""
    frames = sorted(Path(fresh_dir).glob('run*/frame-0.45s.jpg'))
    if not frames:
        return {'pass': False, 'why': 'no frames were taken'}
    first, second = frames[-1], frames[-1].with_name('frame-0.70s.jpg')
    if not second.exists():
        return {'pass': False, 'why': 'the second frame is missing'}
    (w, h, bpp, a), (_, _, _, b) = bmp_pixels(first), bmp_pixels(second)
    x0, x1, y0, y1 = w // 4, 3 * w // 4, h // 4, 3 * h // 4
    moved = total = bright = 0
    for y in range(y0, y1, 2):
        for x in range(x0, x1, 2):
            pa, pb = a[y][x * bpp:x * bpp + 3], b[y][x * bpp:x * bpp + 3]
            total += 1
            moved += max(abs(pa[i] - pb[i]) for i in range(3)) > 12
            bright += min(pa) > 235
    out = {'frames': [str(first), str(second)], 'moved_fraction': round(moved / total, 4),
           'white_fraction': round(bright / total, 4)}
    out['pass'] = out['moved_fraction'] > 0.01 and out['white_fraction'] < 0.5
    return out


def receipt_tree_sha256(app):
    """richos-user-update's `tree_hash`, re-derived (crates/richos-user-update/src/lib.rs): for
    each entry in sorted order, its relative path's length and bytes and its mode, then `d` and
    its contents, `f`, its size and bytes, or `l`, its target's length and bytes."""
    h = hashlib.sha256()

    def visit(path):
        for name in sorted(os.listdir(path)):
            p = os.path.join(path, name)
            st = os.lstat(p)
            rel = os.path.relpath(p, app).encode()
            h.update(struct.pack('<Q', len(rel)) + rel + struct.pack('<I', st.st_mode & 0o7777))
            if os.path.islink(p):
                target = os.readlink(p).encode()
                h.update(b'l' + struct.pack('<Q', len(target)) + target)
            elif os.path.isdir(p):
                h.update(b'd')
                visit(p)
            else:
                h.update(b'f' + struct.pack('<Q', st.st_size))
                with open(p, 'rb') as f:
                    for chunk in iter(lambda: f.read(1 << 20), b''):
                        h.update(chunk)
    visit(app)
    return h.hexdigest()


def check_results(results):
    """The --checks launches: what each was required to show, and whether it did."""
    out = []
    for r in results:
        said = [e['what'] for e in r.get('events', [])]
        text = '\n'.join(e['text'] for e in r.get('lines', []))
        if r['label'] == 'checks:failing-step':
            ok = 'alert frame' in said and 'exited after the alert' in said and 'application startup:' in text
            out.append({'check': 'a start-up step made to fail ends in the alert and the exit', 'pass': ok,
                        'events': said, 'reported': [l for l in text.splitlines() if 'application startup' in l][:3]})
        elif r['label'] == 'checks:quit-during-startup':
            # Whether start-up had finished when the quit arrived is reported, not judged: on a
            # fast host it can finish first, and the quit must end the app either way.
            out.append({'check': 'a quit from the app\'s own menu during start-up ends the app',
                        'pass': 'gone after the quit' in said, 'events': said,
                        'start_up_had_finished_when_quit_arrived': 'boot complete' in text})
        elif r['label'] == 'checks:after':
            out.append({'check': 'the next launch of the same home starts', 'pass': 'boot complete' in text})
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--candidate', type=Path, required=True, help='the build under test, a RichOS zip')
    p.add_argument('--previous-app', help='(added by run-walk.py --previous-bundle) the older copy, in the guest')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--runs', type=int, default=20, help='judged launches per route')
    p.add_argument('--checks', action='store_true',
                   help='also: a start-up step made to fail, and a quit during start-up')
    p.add_argument('--only-checks', action='store_true',
                   help='only the start-up checks, no timed routes (a re-take of a failed check)')
    p.add_argument('--report-only', action='store_true',
                   help='measure and report, never exit 1 for speed (a before/after measurement of an old build)')
    a = p.parse_args()
    sys.path.insert(0, str(HERE))
    from relaunch import guest  # the committed helper

    a.out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
    payload = (state / 'payload').read_text().strip()
    if not payload.startswith('/Users/') or '/testvm/' not in payload:
        print('not a guest payload: ' + payload, file=sys.stderr)
        return 2

    def g(command, timeout=300):
        return guest(a.vm, command, timeout)

    def push(src, dest):
        subprocess.run([str(HERE / 'guest.sh'), a.vm, '--push', str(src), dest], check=True, timeout=900)

    work = payload + '/launch-speed'
    g('mkdir -p ' + shlex.quote(work + '/candidate'))
    for tool in ('launch-speed-guest.py', 'launch-speed-walk.py'):
        push(HERE / tool, work + '/' + tool)
    push(a.candidate, work + '/candidate/' + a.candidate.name)
    g('cd ' + shlex.quote(work + '/candidate') + ' && ditto -x -k ' + shlex.quote(a.candidate.name)
      + ' . && rm -f ' + shlex.quote(a.candidate.name), 600)
    staged = g('ls -d ' + shlex.quote(work + '/candidate') + '/*.app').splitlines()
    if len(staged) != 1:
        print('the candidate zip must hold exactly one app', file=sys.stderr)
        return 2
    version = g('/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" '
                + shlex.quote(staged[0] + '/Contents/Info.plist'))
    facts = {'vm': a.vm, 'candidate': a.candidate.name, 'version': version,
             'older': g('/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" '
                        + shlex.quote(a.previous_app + '/Contents/Info.plist')) if a.previous_app else None,
             'guest_os': g('sw_vers -productVersion'), 'guest_cpus': g('sysctl -n hw.ncpu'),
             'utc_offset': g('date +%z')}

    # One home per route, copied from the fixture run.sh staged: no route sees another's saved
    # state. The engine is the payload's, as every walk has it.
    fixture = payload + '/home'
    homes = {route: payload + '/home-' + route for route in ('fresh', 'updated', 'checks')}
    for home in homes.values():
        g('rm -rf ' + shlex.quote(home) + ' && cp -Rp ' + shlex.quote(fixture) + ' ' + shlex.quote(home))

    def install(app):
        g('sudo -n rm -rf ' + INSTALLED + ' && sudo -n ditto ' + shlex.quote(app) + ' ' + INSTALLED
          + ' && sudo -n chown -R "$(id -un)" ' + INSTALLED
          + ' && (xattr -dr com.apple.quarantine ' + INSTALLED + ' 2>/dev/null || true)', 600)

    def env(home):
        return {'HOME': home, 'CLAUDE_CONFIG_DIR': home + '/.claude', 'RICHOS_ACTIVATION': 'regular',
                'RICHOS_CLAUDE_BIN': '/Users/admin/.local/bin/claude', 'DISABLE_AUTOUPDATER': '1',
                'RICHOS_ENGINE_DIR': payload + '/engine'}

    def richos(label, home, owns, **extra):
        return {'label': label, 'app': INSTALLED, 'owns': owns, 'env': env(home), 'menu_quit': 'Quit RichOS',
                'done': LAUNCH_LINE, 'settle': 90, **extra}

    def run_plan(name, plan):
        """Run one plan in the guest and pull its results back; the probe quits what it starts."""
        plan_file = work + '/' + name + '.json'
        local = a.out / (name + '.json')
        local.write_text(json.dumps(plan, indent=1))
        push(local, plan_file)
        said = g('python3 ' + shlex.quote(work + '/launch-speed-guest.py') + ' --plan ' + shlex.quote(plan_file)
                 + ' --out ' + shlex.quote(work + '/' + name) + ' ; echo "probe exit $?"', 3600)
        (a.out / (name + '.probe.txt')).write_text(said + '\n')
        return said

    # Dictionary first, DICTIONARY_RUNS times: the guest's slowness against his Mac, measured once.
    said_dictionary = None if a.only_checks else run_plan('dictionary', [
        {'label': 'dictionary:calibration', 'app': DICTIONARY, 'owns': [DICTIONARY], 'env': {},
         'menu_quit': 'Quit Dictionary', 'done': '', 'settle': 4} for _ in range(DICTIONARY_RUNS)])
    install(staged[0])
    fresh = [INSTALLED]

    def expect(start):
        return SPLASH_FOR_START.get(start, SPLASH_OTHERWISE)

    # The fixture records earlier starts (launch-nothing-walk.py), so a home copied from it starts
    # at that count plus one; the rule-switch homes have the record removed (a fresh install).
    launches_json = 'Library/Application Support/com.richos.app/launches.json'
    prior = int(g('python3 -c ' + shlex.quote('import json,sys\ntry: print(len(json.load(open(sys.argv[1]))["starts"]))\n'
                                             'except Exception: print(0)') + ' ' + shlex.quote(fixture + '/' + launches_json))
                .strip() or 0)
    facts['fixture_starts'] = prior

    splash_record = homes['fresh'] + '/Library/Application Support/com.richos.app/splash-next.json'
    changed_files = ('python3 -c ' + shlex.quote(
        'import json,sys; p=sys.argv[1]; r=json.load(open(p)); r["opening"]["fingerprint"]="0"; '
        'json.dump(r, open(p, "w"))') + ' ' + shlex.quote(splash_record))
    # Its very first launch (the first start after an install: reported), then the first launch
    # after a cold start, --runs times, each after the disk cache is flushed, then --runs ordinary
    # launches; then one ordinary launch photographed twice, a quarter of a second apart (the splash,
    # and playing: reported), and the first start after an update that changed the splash's own
    # files (reported).
    said = [] if a.only_checks else [run_plan('fresh', [richos('fresh:first', homes['fresh'], fresh, frame_at=1.0)]
                     + [richos('cold:judged', homes['fresh'], fresh, purge=True, expect_splash=expect(prior + 2 + i))
                        for i in range(a.runs)]
                     + [richos('ordinary:judged', homes['fresh'], fresh, expect_splash=expect(prior + 2 + a.runs + i))
                        for i in range(a.runs)]
                     + [richos('ordinary:frames', homes['fresh'], fresh, frames_at=[0.45, 0.70]),
                        richos('changed-splash-files:first', homes['fresh'], fresh, before=changed_files)])]
    # THE PLAN'S ROUTE 5: a quit at which the rules pick a different splash than the one just shown,
    # then the launch. Start 1 to start 2 does this today (#1, then #2), and start 2 to start 3 again
    # (#2, then #1): each of --runs fresh homes is started three times, the last two judged.
    if not a.only_checks:
        switch_plan = []
        for i in range(a.runs):
            home = payload + '/home-switch-%02d' % i
            g('rm -rf ' + shlex.quote(home) + ' && cp -cRp ' + shlex.quote(fixture) + ' ' + shlex.quote(home)
              + ' && rm -f ' + shlex.quote(home + '/' + launches_json))
            switch_plan += [richos('rule-switch:prep', home, fresh),
                            richos('rule-switch:judged', home, fresh, expect_splash=expect(2)),
                            richos('rule-switch:judged', home, fresh, expect_splash=expect(3))]
        said.append(run_plan('switch', switch_plan))
    if a.checks or a.only_checks:
        ledger = homes['checks'] + '/Library/Application Support/com.richos.app/conversation-ledger.jsonl'
        said.append(run_plan('checks', [
            # The first launch writes the home's records; the next is made to fail at its first
            # step behind the window, by a conversation record it cannot read.
            richos('checks:prepare', homes['checks'], fresh),
            dict(richos('checks:failing-step', homes['checks'], fresh, alert=True),
                 before='chmod 000 ' + shlex.quote(ledger), after='chmod 600 ' + shlex.quote(ledger)),
            # A quit from the app's own menu as soon as its process is up, on a flushed disk cache
            # so start-up is still running when it arrives.
            richos('checks:quit-during-startup', homes['checks'], fresh, purge=True, quit_at=0.0),
            richos('checks:after', homes['checks'], fresh)]))
    if a.previous_app and not a.only_checks:
        user_app = homes['updated'] + '/Applications/RichOS.app'
        g('mkdir -p ' + shlex.quote(homes['updated'] + '/Applications') + ' && ditto ' + shlex.quote(staged[0])
          + ' ' + shlex.quote(user_app), 600)
        # The receipt an update leaves, derived as the updater derives it, inside the guest.
        tree = g('python3 -c ' + shlex.quote(
            'import importlib.util as u; s=u.spec_from_file_location("w", %r); m=u.module_from_spec(s); '
            's.loader.exec_module(m); print(m.receipt_tree_sha256(%r))' % (work + '/launch-speed-walk.py', user_app)),
            900)
        receipt = json.dumps({'version': version, 'archive_sha256': hashlib.sha256(a.candidate.name.encode()).hexdigest(),
                              'tree_sha256': tree, 'executable_name': 'richos-tauri'})
        root = homes['updated'] + '/Applications/.richos-updater'
        g('mkdir -p ' + shlex.quote(root) + ' && chmod 700 ' + shlex.quote(root) + ' && umask 077 && printf %s '
          + shlex.quote(receipt) + ' > ' + shlex.quote(root + '/published.json'))
        install(a.previous_app)
        owns = [INSTALLED, user_app]
        said.append(run_plan('updated', [richos('updated:first', homes['updated'], owns, frame_at=1.0)]
                             + [richos('updated-from-applications:judged', homes['updated'], owns,
                                       expect_splash=expect(prior + 2 + i)) for i in range(a.runs)]))
        # THE PLAN'S ROUTE 3: the first launch after an update put in place at quit. The older copy is
        # in this home's ~/Applications as an update leaves it, the candidate is prepared as its
        # update, the older copy is started and quit (its helper puts the candidate in place as it
        # closes), and the candidate's launch is judged. Each round puts the older copy and the
        # prepared candidate back (APFS clones, so no copy is paid for).
        after = payload + '/home-after-update'
        g('rm -rf ' + shlex.quote(after) + ' && cp -cRp ' + shlex.quote(fixture) + ' ' + shlex.quote(after)
          + ' && mkdir -p ' + shlex.quote(after + '/Applications'))
        tree_of = lambda app: g('python3 -c ' + shlex.quote(
            'import importlib.util as u; s=u.spec_from_file_location("w", %r); m=u.module_from_spec(s); '
            's.loader.exec_module(m); print(m.receipt_tree_sha256(%r))' % (work + '/launch-speed-walk.py', app)), 900)
        older_receipt = json.dumps({'version': facts['older'], 'archive_sha256': hashlib.sha256(b'older').hexdigest(),
                                    'tree_sha256': tree_of(a.previous_app), 'executable_name': 'richos-tauri'})
        new_sha = hashlib.sha256(a.candidate.name.encode() + b'-after-update').hexdigest()
        prepared = json.dumps({'version': version, 'archive_sha256': new_sha, 'tree_sha256': tree_of(staged[0]),
                               'executable_name': 'richos-tauri'})
        home_app = after + '/Applications/RichOS.app'
        root = after + '/Applications/.richos-updater'
        q = shlex.quote
        older_in_place = ('rm -rf %s %s && cp -cR %s %s && mkdir -m 700 %s && (umask 077 && printf %%s %s > %s/published.json)'
                          % (q(home_app), q(root), q(a.previous_app), q(home_app), q(root), q(older_receipt), q(root)))
        stage_update = ('mkdir -m 700 %s/%s && cp -cR %s %s/%s/incoming.app && (umask 077 && printf %%s %s > %s/prepared.json)'
                        % (q(root), new_sha, q(staged[0]), q(root), new_sha, q(prepared), q(root)))
        # The helper the quit left behind has put the update in place: no prepared receipt, and the
        # candidate's version in ~/Applications (30 s at most; the judged launch says what it found).
        helper_done = ('for i in $(seq 300); do [ ! -e %s/prepared.json ] && /usr/libexec/PlistBuddy -c '
                       '"Print :CFBundleShortVersionString" %s/Contents/Info.plist | grep -qx %s && break; sleep 0.1; done'
                       % (q(root), q(home_app), q(version)))

        def at_home(label, **extra):
            return dict(richos(label, after, [home_app], **extra), app=home_app)
        # Two starts of the older copy first, so the saved pick and its opening exist.
        plan3 = [at_home('after-update:warm', before=older_in_place), at_home('after-update:warm')]
        for i in range(a.runs):
            plan3 += [at_home('after-update:prep', before=older_in_place + ' && ' + stage_update),
                      at_home('after-update:judged', before=helper_done, expect_splash=expect(prior + 4 + 2 * i))]
        said.append(run_plan('after', plan3))

    # The guest's own system log for the walk, on the guest's clock.
    predicate = 'process == "richos-tauri" OR process == "Dictionary"'
    g('/usr/bin/log show --last 300m --style compact --predicate ' + shlex.quote(predicate) + ' > '
      + shlex.quote(work + '/guest-system.log') + ' 2>&1 || true', 900)
    facts['left_running'] = g('ps -axo pid=,comm= | grep -e "/richos-tauri$" -e "/Dictionary$" | grep -v grep || true')
    g('cd ' + shlex.quote(payload) + ' && tar czf launch-speed.tgz launch-speed/guest-system.log '
      + ' '.join('launch-speed/' + n for n in PLANS) + ' 2>/dev/null; true', 600)
    subprocess.run([str(HERE / 'guest.sh'), a.vm, '--pull', payload + '/launch-speed.tgz', str(a.out / 'launch-speed.tgz')],
                   check=True, timeout=900)
    with tarfile.open(a.out / 'launch-speed.tgz') as t:
        t.extractall(a.out, filter='data')
    events = syslog_events((a.out / 'launch-speed/guest-system.log').read_text(errors='replace'),
                           utc_offset(facts['utc_offset']))
    rows, results = [], []
    for name in PLANS:
        probe = a.out / 'launch-speed' / name / 'probe.json'
        if probe.exists():
            ran = json.loads(probe.read_text())['results']
            results += ran
            rows += [measure(r, events) for r in ran if not r['label'].startswith('checks')]
    planned = {"ordinary": a.runs, "cold": a.runs, "rule-switch": 2 * a.runs}
    if a.previous_app:
        planned["updated-from-applications"] = a.runs
        # An older copy from before the saved pick existed saves nothing at its quit, so the first
        # start after it hands over has no opening to play: that route is then reported, and says
        # why, rather than judged (the plan's section 9, "the first start after an update is picked
        # by the old version").
        if any(r['label'] == 'after-update:prep' for r in rows) and not any(
                r['label'] == 'after-update:prep' and r.get('splash_id') for r in rows):
            facts['after_update_not_judged'] = ('the older copy (%s) predates the splash picked at quit, so it saved '
                                                'no opening for the candidate' % facts['older'])
            for r in rows:
                if r['label'] == 'after-update:judged':
                    r['label'] = 'after-update:reported'
        else:
            planned["after-update"] = a.runs
    facts['playing'] = splash_playing(a.out / 'launch-speed' / 'fresh')
    limit_ms, ratio, inputs = guest_limit(rows)
    summary = verdict(rows, planned, limit_ms, ratio, inputs)
    checks = check_results(results) if a.checks or a.only_checks else []
    report = {'facts': facts, 'summary': summary, 'sentence': sentence(summary), 'launches': rows,
              'checks': checks, 'refusing': not a.report_only,
              'instances': [{'label': r['label'], 'pid': r['pid'], 'quit': r['quit'], 'all_gone': r['all_gone']}
                            for r in results]}
    (a.out / 'report.json').write_text(json.dumps(report, indent=1) + '\n')
    print(report['sentence'])
    for c in checks:
        print('%s: %s' % ('PASS' if c['pass'] else 'FAIL', c['check']))
    if facts['left_running'] or any(r['pid'] and not r['all_gone'] for r in results) or (not rows and not a.only_checks):
        print('harness failure: an instance was left running, or nothing was measured', file=sys.stderr)
        return 2
    if any(not c['pass'] for c in checks) or (summary['refused'] and not a.report_only and not a.only_checks):
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())

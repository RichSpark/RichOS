#!/usr/bin/env python3
"""launch-speed-walk.py's arithmetic, without a guest: the milliseconds read off log lines, the
p95 verdict against the CEO's 380 ms, the sentence the nightly prints, the start-up checks,
and the receipt it writes for the updated route, whose tree digest must be the updater's own."""
import importlib.util
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('launch_speed_walk', HERE.parent / 'launch-speed-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


# The system log's compact lines are the guest's local time; +0100 is an hour ahead of UTC.
log = ('2026-11-14 10:00:00.250 Df richos-tauri[501:1a] [com.apple.AppKit:Window] order window front conditionally\n'
       '2026-11-14 10:00:00.400 Df richos-tauri[501:1a] [com.apple.WebKit:Loading] didCommitLoadForFrame\n'
       '2026-11-14 10:00:00.600 Df richos-tauri[501:1a] [com.apple.WebKit:Loading] didFinishDocumentLoadForFrame\n'
       '2026-11-14 10:00:00.300 Df Dictionary[777:2b] [com.apple.AppKit:Window] order window front conditionally\n')
events = walk.syslog_events(log, walk.utc_offset('+0100'))
t_open = events[0][0] - 0.250  # the app's `open` started 250 ms before its window
check('the guest offset is read as seconds', walk.utc_offset('+0100') == 3600 and walk.utc_offset('-0700') == -25200)

started_ms = int((t_open + 0.080) * 1000)
line = ('[richos] launch timing: update step 15 ms, window 160 ms, first frame 165 ms, start-up finished 250 ms, '
        'page took over 330 ms after the process started; splash shown; update put in place: no; bundle check none '
        '(trusted as written at its update), 1 ms; engine check none (trusted as verified when it was written), 3 ms; '
        'process started at %d' % started_ms)
row = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'lines': [{'t': 0, 'text': line}]}, events)
check('AppKit and WebKit milestones in ms after open', (row.get('window'), row.get('page_committed'),
      row.get('document_loaded')) == (250, 400, 600), row)
A = walk.ALLOWANCE_MS
check("the judged moment is the page's splash playing, plus the allowance",
      row.get('splash') == 410 + A and row.get('splash_from') == "the page's splash playing"
      and row.get('first_frame') == 245, row)
check('the launch line says which checks ran', row.get('bundle_check', '').startswith('bundle check none (trusted')
      and row.get('engine_check', '').startswith('none (trusted'), row)
old = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'lines': []}, events)
check("an older build with no launch line falls back to WebKit's document loaded",
      old.get('splash') == 600 + A and old.get('splash_from') == "WebKit's document loaded", old)

check("the limit is the CEO's number", walk.LIMIT_MS == 380)
check('p95 by nearest rank: the 19th of 20, the slowest of fewer',
      walk.p95(list(range(1, 21))) == 19 and walk.p95([5, 1, 9]) == 9 and walk.p95([7]) == 7)
rows = [{'label': 'ordinary:judged', 'splash': ms} for ms in range(300, 320)] + \
       [{'label': 'cold:judged', 'splash': ms} for ms in (350, 360, 370)] + \
       [{'label': 'ordinary:first', 'splash': 1200}]
summary = walk.verdict(rows)
check('the p95 of every judged launch together, within 380 ms, passes',
      summary['p95_ms'] == 360 and summary['launches'] == 23 and summary['refused'] is False, summary)
check("each route's own p95 and median are stated",
      summary['routes']['cold'] == {'p95_ms': 370, 'median_ms': 360, 'max_ms': 370, 'launches': 3}, summary)
check('first launches are reported, not judged', summary['reported'] == {'ordinary:first': 1200}, summary)
slow = walk.verdict([{'label': 'ordinary:judged', 'splash': ms} for ms in [300] * 18 + [390, 400]])
check('a p95 over 380 ms refuses, however good the median', slow['p95_ms'] == 390 and slow['refused'] is True, slow)
check('nothing measured refuses', walk.verdict([])['refused'] is True)
said = walk.sentence(summary)
check('the sentence states the p95 against 380 ms and every route',
      'p95 of 380 ms' in said and 'p95 360 ms over 23 launches' in said and 'cold p95 370 ms' in said
      and 'Dictionary' not in said, said)

# The receipt's tree digest, on the tree whose digest the updater's own test pins
# (crates/richos-user-update/src/lib.rs, digests_are_spelled_as_sha2_0_10_spelled_them).
with tempfile.TemporaryDirectory() as d:
    os.mkdir(os.path.join(d, 'a'))
    os.chmod(os.path.join(d, 'a'), 0o755)
    for name, body, mode in (('a/x', b'hello', 0o644), ('b', b'world!', 0o755)):
        with open(os.path.join(d, name), 'wb') as f:
            f.write(body)
        os.chmod(os.path.join(d, name), mode)
    got = walk.receipt_tree_sha256(d)
check("the receipt's tree digest is the updater's", got == '6e96a12b30eb6d6763477bfec950ae61bc1d0a701a9421d529986c7a80ca58fa', got)

results = [
    {'label': 'checks:failing-step', 'events': [{'what': 'alert frame'}, {'what': 'exited after the alert'}],
     'lines': [{'text': '[richos] application startup: panicked at src/main.rs:1:1 — open ledger'}]},
    {'label': 'checks:quit-during-startup', 'events': [{'what': 'quit pressed during start-up'}, {'what': 'gone after the quit'}],
     'lines': []},
    {'label': 'checks:after', 'events': [], 'lines': [{'text': '[richos] boot complete — every line above'}]},
]
checks = walk.check_results(results)
check('the three start-up checks pass on what they must show', [c['pass'] for c in checks] == [True, True, True], checks)
stuck = walk.check_results([{'label': 'checks:failing-step', 'events': [{'what': 'alert frame'},
                             {'what': 'STILL RUNNING after the alert'}], 'lines': []}])
check('an app still running after its alert fails the check', stuck[0]['pass'] is False, stuck)

print('%d failure(s)' % len(failures))
sys.exit(1 if failures else 0)

#!/usr/bin/env python3
"""ledger-timeline.py - the app's conversation ledger read as one timeline.

  ledger-timeline.py LEDGER.jsonl [--thread ID] [--full] [--turns]

LEDGER is the app's own `conversation-ledger.jsonl`, as a walk keeps it (hire-walk.py's
keep_logs, a steps-walk `pull`). One line per event, oldest first:

  <seconds since the first row> <event> <source> <turn, last 6> <text>

The streamed reply pieces of a turn (AssistantDelta) are joined and printed once, at the moment
the FIRST piece arrived, as REPLY. Internal prompts (source `internal`) are printed like every
other row: they are the hidden turns a person never sees but waits through.

--turns prints instead one line per turn a person sent (source not internal or proactive): when
it was sent, when its first reply text came and when it completed, in seconds from the send. That
is the wait he sat through, measured off the app's own clock.

--full prints whole texts; otherwise each is cut at 300 characters.

Exit 0 with the timeline. Exit 2 with a sentence when the file is missing, empty or holds a line
that is not a JSON object with an `at` time: a ledger that cannot be read is never printed as an
empty one.
"""
import argparse
import json
import sys
from pathlib import Path


def read(path):
    """The ledger's rows, or a sentence why it cannot be read."""
    p = Path(path)
    if not p.is_file():
        return None, f'no ledger at {path}'
    rows = []
    for n, line in enumerate(p.read_text(errors='replace').splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except ValueError:
            return None, f'line {n} of {path} is not JSON'
        if not isinstance(row, dict) or not isinstance(row.get('at'), (int, float)):
            return None, f'line {n} of {path} has no `at` time'
        rows.append(row)
    if not rows:
        return None, f'{path} holds no rows'
    return rows, None


def cut(text, full):
    text = str(text or '').replace('\n', ' / ')
    return text if full or len(text) <= 300 else text[:300] + ' ...'


def timeline(rows, thread=None, full=False):
    if thread:
        rows = [r for r in rows if r.get('thread_id') in (None, thread)]
        if not rows:
            return []
    t0 = rows[0]['at']
    replies = {}
    for r in rows:
        if r.get('event') == 'AssistantDelta':
            at, text = replies.get(r.get('turn_id'), (r['at'], ''))
            replies[r.get('turn_id')] = (at, text + str(r.get('text', '')))
    out, said = [], set()
    for r in rows:
        event, turn = r.get('event'), str(r.get('turn_id') or '')
        if event == 'AssistantDelta':
            if turn in said:
                continue
            said.add(turn)
            at, text = replies[r.get('turn_id')]
            out.append(f"{(at - t0) / 1000:8.1f} REPLY - {turn[-6:]} {cut(text, full)}")
            continue
        what = r.get('text') or r.get('stop_reason') or r.get('kind') or ''
        out.append(f"{(r['at'] - t0) / 1000:8.1f} {event} {r.get('source') or '-'} {turn[-6:]} {cut(what, full)}")
    return out


def turns(rows, full=False):
    sent, first, done = {}, {}, {}
    for r in rows:
        event, turn = r.get('event'), r.get('turn_id')
        if event == 'PromptReceived' and r.get('source') not in ('internal', 'proactive'):
            sent[turn] = r
        elif event == 'AssistantDelta' and turn in sent and turn not in first:
            first[turn] = r['at']
        elif event in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed') and turn in sent:
            done[turn] = (r['at'], event)
    out = []
    for turn, r in sent.items():
        f = f"{(first[turn] - r['at']) / 1000:.1f}s" if turn in first else 'none'
        d = f"{(done[turn][0] - r['at']) / 1000:.1f}s {done[turn][1]}" if turn in done else 'not completed'
        out.append(f"sent {r['at']} first reply {f} end {d} | {cut(r.get('text'), full)}")
    return out


def main():
    if len(sys.argv) > 1 and sys.argv[1] in ('-h', '--help'):
        print(__doc__)
        return 0
    p = argparse.ArgumentParser(add_help=False)
    p.add_argument('ledger')
    p.add_argument('--thread')
    p.add_argument('--full', action='store_true')
    p.add_argument('--turns', action='store_true')
    try:
        a = p.parse_args()
    except SystemExit:
        print('ledger-timeline.py LEDGER.jsonl [--thread ID] [--full] [--turns]: see --help', file=sys.stderr)
        return 2
    rows, why = read(a.ledger)
    if why:
        print(f'ledger-timeline.py: cannot read the ledger: {why}', file=sys.stderr)
        return 2
    print('\n'.join(turns(rows, a.full) if a.turns else timeline(rows, a.thread, a.full)))
    return 0


if __name__ == '__main__':
    sys.exit(main())

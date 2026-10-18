#!/usr/bin/env python3
"""wait-turn.py - wait until a turn is open and streaming a long answer: the one wait before Stop.

  wait-turn.py LEDGER.jsonl (--after-ms AT | --prompt-contains TEXT) [--min-chars N]
               [--timeout SECONDS] [--interval SECONDS]

LEDGER is the app's own `conversation-ledger.jsonl`. It is re-read every --interval seconds
(default 1) until a turn qualifies:

  - the turn is OPEN: no TurnCompleted / TurnInterrupted / TurnStopped / TurnFailed row for it;
  - it has streamed at least --min-chars (default 200) characters of AssistantDelta text;
  - and, by the condition given:
      --after-ms AT          it started (its PromptReceived row, any source, internal turns
                             included) after AT, in the ledger's own `at` milliseconds;
      --prompt-contains TEXT its PromptReceived text contains TEXT.

A request's short acknowledgement turn completes and the long answer arrives as a DIFFERENT turn
(Stu's result, Rich's own), so completed turns never qualify and matching on the request's text
alone is the wrong turn. When several qualify the newest-started one wins.

Prints the turn id and exits 0. On timeout (default 120 s) prints why to stderr and exits 1; a
wait that gives up is a failure. Exit 2 for a usage error. A missing, empty or half-written ledger
is waited on (it is being written) and named in the timeout sentence.
"""
import argparse
import importlib.util
import sys
import time
from pathlib import Path

_spec = importlib.util.spec_from_file_location('ledger_timeline', Path(__file__).with_name('ledger-timeline.py'))
ledger_timeline = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ledger_timeline)

CLOSED = ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed')


def turn_states(rows):
    """Every turn: {id: {start, prompt, chars, open}} in ledger order."""
    turns = {}
    for r in rows:
        tid = r.get('turn_id')
        if not tid:
            continue
        t = turns.setdefault(tid, {'start': r['at'], 'prompt': '', 'chars': 0, 'open': True})
        event = r.get('event')
        if event == 'PromptReceived':
            if not t['prompt']:
                t['start'] = r['at']
            t['prompt'] += str(r.get('text') or '')
        elif event == 'AssistantDelta':
            t['chars'] += len(str(r.get('text') or ''))
        elif event in CLOSED:
            t['open'] = False
    return turns


def pick(rows, after_ms, prompt_contains, min_chars):
    """(turn id, or None; a sentence about the closest candidate)."""
    best, note = None, 'no turn matches the condition yet'
    for tid, t in turn_states(rows).items():
        if after_ms is not None and not t['start'] > after_ms:
            continue
        if prompt_contains is not None and prompt_contains not in t['prompt']:
            continue
        if not t['open']:
            note = f'the matching turn {tid[-6:]} already completed'
        elif t['chars'] < min_chars:
            note = f'the matching turn {tid[-6:]} is open but has streamed only {t["chars"]} of {min_chars} characters'
        elif best is None or t['start'] >= best[1]:
            best = (tid, t['start'])
    return (best[0] if best else None), note


def main():
    if len(sys.argv) > 1 and sys.argv[1] in ('-h', '--help'):
        print(__doc__)
        return 0
    p = argparse.ArgumentParser(add_help=False)
    p.add_argument('ledger')
    p.add_argument('--after-ms', type=float)
    p.add_argument('--prompt-contains')
    p.add_argument('--min-chars', type=int, default=200)
    p.add_argument('--timeout', type=float, default=120)
    p.add_argument('--interval', type=float, default=1)
    try:
        a = p.parse_args()
    except SystemExit:
        print('wait-turn.py LEDGER.jsonl (--after-ms AT | --prompt-contains TEXT) [--min-chars N] [--timeout S]: see --help', file=sys.stderr)
        return 2
    if (a.after_ms is None) == (a.prompt_contains is None):
        print('wait-turn.py: give exactly one of --after-ms or --prompt-contains', file=sys.stderr)
        return 2
    deadline = time.monotonic() + a.timeout
    note = 'the ledger was never read'
    while True:
        rows, why = ledger_timeline.read(a.ledger)
        if why:
            note = f'cannot read the ledger: {why}'
        else:
            tid, note = pick(rows, a.after_ms, a.prompt_contains, a.min_chars)
            if tid:
                print(tid)
                return 0
        if time.monotonic() >= deadline:
            print(f'wait-turn.py: no open turn with {a.min_chars}+ streamed characters within {a.timeout:g} s: {note}', file=sys.stderr)
            return 1
        time.sleep(max(0.05, min(a.interval, deadline - time.monotonic())))


if __name__ == '__main__':
    sys.exit(main())

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
empty one."""
import argparse
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from devtools_bridge import request

def read(path):
    return tuple(request("ledger-read", str(path)))

def cut(text, full):
    text = str(text or "").replace("\n", " / ")
    return text if full or len(text) <= 300 else text[:300] + " ..."

def timeline(rows, thread=None, full=False):
    return request("ledger-timeline", {"rows": rows, "thread": thread, "full": full})

def turns(rows, full=False):
    return request("ledger-turns", {"rows": rows, "full": full})

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

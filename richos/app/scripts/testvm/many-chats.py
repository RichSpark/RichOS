#!/usr/bin/env python3
"""many-chats.py LEDGER --count N - give a guest's RichOS a long chat list.

Run in the guest, with RichOS quit (the walk quits it and checks the process is gone first:
appending to the ledger while the app writes it is not what a user's Mac ever sees). It reads
the app's conversation-ledger.jsonl, takes the first chat bound to a company (--newest: the
last one, such as the chat the walk just wrote in; --thread ID: that one) with its turns, and appends N copies of it: each a new chat in the same company with new chat,
turn and session ids, titled "Older chat NNN", dated one minute apart going back from the
source chat, so they list below it. --turns K repeats the source chat's turns K times in each
copy (a long history: 500 chats of 80 turns is the 40,000 turns of the fix's own case). The
source chat is left exactly as it was.

Prints one JSON line: the source chat, how many chats were added and the ledger's row count
before and after. Exit 1 with a sentence when there is no ledger, no chat bound to a company,
no chat with that id, or a count or a turn count below 1.

Added with the nightly 123 walk: the chat list's speed with many chats (the sidebar did
20,000,000 turn checks for 500 chats before 871dabe24) is walked on the real app, not a test.
"""
import argparse
import json
import sys
import uuid
from pathlib import Path


def fail(sentence):
    print('many-chats: ' + sentence, file=sys.stderr)
    sys.exit(1)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('ledger', type=Path, help="the app's conversation-ledger.jsonl")
    p.add_argument('--count', type=int, required=True, help='how many chats to add')
    p.add_argument('--thread', help='the chat to copy (default: the first chat bound to a company)')
    p.add_argument('--newest', action='store_true', help='copy the newest chat bound to a company instead of the first')
    p.add_argument('--turns', type=int, default=1, help="how many times each copy repeats the source chat's turns")
    a = p.parse_args()
    if a.count < 1:
        fail(f'--count {a.count}: give 1 or more')
    if a.turns < 1:
        fail(f'--turns {a.turns}: give 1 or more')
    if not a.ledger.is_file():
        fail(f'no ledger at {a.ledger}')
    lines = [line for line in a.ledger.read_text().splitlines() if line.strip()]
    rows = []
    for line in lines:
        try:
            rows.append(json.loads(line))
        except ValueError:
            rows.append(None)
    created = [r for r in rows if r and r.get('event') == 'ThreadCreated' and r.get('entity_id')
               and (a.thread is None or r.get('thread_id') == a.thread)]
    if not created:
        fail(f'no chat {a.thread} in the ledger' if a.thread else 'no chat bound to a company in the ledger')
    source = created[-1] if a.newest else created[0]
    thread = source['thread_id']
    turns = {r['turn_id'] for r in rows if r and r.get('event') == 'PromptReceived' and r.get('thread_id') == thread}
    # A continuation (TurnReplayed, TurnSuperseded) names its new turn in by_turn_id and has no
    # PromptReceived of its own: it belongs to the chat too, and so do its rows.
    grew = True
    while grew:
        more = {r['by_turn_id'] for r in rows if r and r.get('turn_id') in turns and isinstance(r.get('by_turn_id'), str)}
        grew = not more <= turns
        turns |= more
    copied = [r for r in rows if r and r is not source
              and (r.get('thread_id') == thread or r.get('turn_id') in turns)]
    added = []
    for n in range(1, a.count + 1):
        shift = n * 60_000
        new_thread = str(uuid.uuid4())
        sessions = {}
        head = dict(source, thread_id=new_thread, title=f'Older chat {n:03d}', at=source['at'] - shift)
        added.append(head)
        for repeat in range(a.turns):
            new_turns = {t: str(uuid.uuid4()) for t in turns}
            for r in copied:
                r = dict(r)
                if r.get('thread_id') == thread:
                    r['thread_id'] = new_thread
                for key in ('turn_id', 'by_turn_id'):
                    if r.get(key) in new_turns:
                        r[key] = new_turns[r[key]]
                if isinstance(r.get('session_id'), str):
                    r['session_id'] = sessions.setdefault(r['session_id'], str(uuid.uuid4()))
                if isinstance(r.get('at'), int):
                    # Each repeat a little later than the last, all inside the chat's minute.
                    r['at'] = r['at'] - shift + repeat * 50
                added.append(r)
    with a.ledger.open('a') as out:
        for r in added:
            out.write(json.dumps(r, separators=(',', ':')) + '\n')
    print(json.dumps({'source_thread': thread, 'source_title': source.get('title'), 'turns_per_chat': len(turns) * a.turns,
                      'chats_added': a.count, 'rows_before': len(lines), 'rows_after': len(lines) + len(added)}))


if __name__ == '__main__':
    main()

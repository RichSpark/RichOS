#!/usr/bin/env python3
"""many-chats.py: N copies of a bound chat with new ids, the source untouched, and each refusal."""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'many-chats.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def run(*args):
    r = subprocess.run([sys.executable, str(TOOL), *map(str, args)], capture_output=True, text=True, timeout=30)
    return r.returncode, r.stdout, r.stderr


ROWS = [
    {'event': 'ThreadCreated', 'thread_id': 'unbound', 'title': 'Old', 'at': 1000},
    {'event': 'ThreadCreated', 'thread_id': 't1', 'title': 'Hello', 'at': 10_000_000, 'entity_id': 'acme',
     'person_id': 'p', 'binding_revision': 1},
    {'event': 'PromptReceived', 'turn_id': 'u1', 'thread_id': 't1', 'text': 'hi', 'source': 'text', 'at': 10_000_100,
     'entity_id': 'acme', 'person_id': 'p'},
    {'event': 'TurnStarted', 'turn_id': 'u1', 'session_id': 's1', 'at': 10_000_200},
    {'event': 'AssistantDelta', 'turn_id': 'u1', 'text': 'Hello there', 'at': 10_000_300},
    {'event': 'TurnCompleted', 'turn_id': 'u1', 'at': 10_000_400},
    {'event': 'PromptReceived', 'turn_id': 'other', 'thread_id': 'unbound', 'text': 'x', 'source': 'text', 'at': 1100},
]

with tempfile.TemporaryDirectory() as tmp:
    ledger = Path(tmp) / 'conversation-ledger.jsonl'
    original = ''.join(json.dumps(r) + '\n' for r in ROWS)
    ledger.write_text(original)
    code, out, err = run(ledger, '--count', 3)
    rows = [json.loads(x) for x in ledger.read_text().splitlines()]
    new = rows[len(ROWS):]
    heads = [r for r in new if r['event'] == 'ThreadCreated']
    check('three chats are added in the source chat\'s company, titled and dated below it',
          code == 0 and [h['title'] for h in heads] == ['Older chat 001', 'Older chat 002', 'Older chat 003']
          and all(h['entity_id'] == 'acme' and h['at'] < 10_000_000 for h in heads), (code, out, err, heads))
    check('each copy carries the four turn rows with fresh chat, turn and session ids',
          len(new) == 15 and len({h['thread_id'] for h in heads}) == 3
          and not any(r.get('thread_id') == 't1' or r.get('turn_id') == 'u1' or r.get('session_id') == 's1' for r in new),
          new)
    check('the source rows are untouched and the unbound chat is not copied',
          ledger.read_text().startswith(original) and not any(r.get('thread_id') == 'unbound' for r in new))
    summary = json.loads(out)
    check('the summary names the source and the row counts',
          summary == {'source_thread': 't1', 'source_title': 'Hello', 'turns_per_chat': 1, 'chats_added': 3,
                      'rows_before': 7, 'rows_after': 22}, summary)

    longer = Path(tmp) / 'longer.jsonl'
    longer.write_text(original)
    code, out, err = run(longer, '--count', 2, '--turns', 5)
    new = [json.loads(x) for x in longer.read_text().splitlines()][len(ROWS):]
    prompts = [r for r in new if r['event'] == 'PromptReceived']
    check('--turns 5 gives each copy five turns with distinct turn ids, in order',
          code == 0 and len(new) == 2 + 2 * 5 * 4 and len({r['turn_id'] for r in prompts}) == 10
          and json.loads(out)['turns_per_chat'] == 5
          and [r['at'] for r in prompts[:5]] == sorted(r['at'] for r in prompts[:5]), (code, out, err, len(new)))
    code, out, err = run(longer, '--count', 1, '--turns', 0)
    check('a turn count below 1 is refused: exit 1', code == 1 and '--turns 0' in err, (code, err))

    newest = Path(tmp) / 'newest.jsonl'
    newest.write_text(original + json.dumps(dict(ROWS[1], thread_id='t2', title='Latest')) + '\n')
    code, out, err = run(newest, '--count', 1, '--newest')
    check('--newest copies the last chat bound to a company', code == 0 and json.loads(out)['source_thread'] == 't2',
          (code, out, err))

    code, out, err = run(Path(tmp) / 'missing.jsonl', '--count', 2)
    check('a missing ledger is refused with a sentence: exit 1', code == 1 and 'no ledger' in err, (code, err))
    lonely = Path(tmp) / 'lonely.jsonl'
    lonely.write_text(json.dumps(ROWS[0]) + '\n')
    code, out, err = run(lonely, '--count', 2)
    check('a ledger with no chat bound to a company is refused: exit 1',
          code == 1 and 'no chat bound' in err and lonely.read_text() == json.dumps(ROWS[0]) + '\n', (code, err))
    code, out, err = run(ledger, '--count', 0)
    check('a count below 1 is refused: exit 1', code == 1 and '--count 0' in err, (code, err))
    code, out, err = run(ledger, '--count', 1, '--thread', 'nope')
    check('an unknown --thread is refused: exit 1', code == 1 and 'no chat nope' in err, (code, err))

print('many-chats.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)

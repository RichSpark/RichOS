#!/usr/bin/env python3
"""reap-guest.py — the guest-side half of reap-walk.py. Pushed into the guest; never run on the host.

The tool command Rich is asked to run is NOT here: it is reap-heartbeat.py, seeded into the
repository as heartbeat.py. It records ITSELF in DIR/NAME.id, as JSON on one line: {"name", "pid",
"pgid", "sid", "start"} (start = its start time from `ps -o lstart=`, an identity rather than a
clock), and nothing is inferred about it: the walk grades only commands that recorded themselves.
It is a separate file because the back end reads a script before it runs it, and this one's
watcher and recorder are not what it is told the script does (guest walk-1a978b4e081b,
2026-09-27: it declined to run it).

  reap-guest.py watch DIR OUT [--seconds N]
      The death watcher. Every 50 ms it reads DIR/*.id and, for each recorded process, notes on
      the guest's clock (ms) the first moment `kill(pid, 0)` says it is gone, or its pid answers
      with a different start. One JSON line per event in OUT: {"kind": "watch-start"},
      {"kind": "seen", "name", ...identity}, {"kind": "gone", "name", "t_ms"}.

  reap-guest.py group PGID
      Prints the pids still in process group PGID, one per line (none: nothing printed).

  reap-guest.py record OUT [--seconds N]
      Every 0.5 s for N seconds (default 120), appends the guest's time and its process table
      (pid, ppid, pgid, session, command) to OUT: what a lease actually started, when.

  reap-guest.py evidence DATA HOME SINCE_MS
      Prints, as one JSON object, what explains an ask that never started its commands, read
      from the app's own files (DATA is its data folder, HOME the guest user's home): every
      assignment, Rich's last turns (what he was asked, what he said, how each turn ended),
      every session's last words and tool calls from its own transcript with its role (rich or
      stu), and the last rows of Stu's inboxes. A diagnostic only: an unreadable file is
      named in "unreadable", never a failure.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def start_of(pid):
    r = subprocess.run(['/bin/ps', '-o', 'lstart=', '-p', str(pid)], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else ''


def alive(pid, start):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    now = start_of(pid)
    return bool(now) and now == start


def watch(directory, out, seconds):
    d = Path(directory)
    seen, gone = {}, set()
    end = time.monotonic() + seconds
    with open(out, 'a', encoding='utf-8') as fh:
        def emit(row):
            row['t_ms'] = int(time.time() * 1000)
            fh.write(json.dumps(row, sort_keys=True) + '\n')
            fh.flush()
        emit({'kind': 'watch-start'})
        while time.monotonic() < end:
            for path in sorted(d.glob('*.id')):
                try:
                    me = json.loads(path.read_text())
                except (OSError, ValueError):
                    continue
                key = (me['name'], me['pid'], me['start'])
                if key not in seen:
                    seen[key] = me
                    emit(dict(me, kind='seen'))
                if key not in gone and not alive(me['pid'], me['start']):
                    gone.add(key)
                    emit({'kind': 'gone', 'name': me['name'], 'pid': me['pid'], 'pgid': me['pgid']})
            time.sleep(0.05)


def group(pgid):
    r = subprocess.run(['/bin/ps', '-A', '-o', 'pid=,pgid=,stat='], capture_output=True, text=True, check=True)
    for line in r.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 3 and int(parts[1]) == pgid and not parts[2].startswith('Z'):
            print(parts[0])


def record(out, seconds):
    end = time.monotonic() + seconds
    with open(out, 'a', encoding='utf-8') as fh:
        while time.monotonic() < end:
            r = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid=,pgid=,sess=,command='], capture_output=True, text=True)
            fh.write('@ %d\n' % int(time.time() * 1000))
            fh.write(''.join(line[:220] + '\n' for line in r.stdout.splitlines()))
            fh.flush()
            time.sleep(0.5)


ASSIGNMENT_KEYS = ('id', 'kind', 'state', 'title', 'detail', 'obligation_id', 'work_session', 'registered_at_ms',
                   'updated_at_ms', 'user_stop_requested_at_ms')
TURN_ENDS = ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnSuperseded')


def _jsonl(path, unreadable):
    try:
        lines = Path(path).read_text(encoding='utf-8', errors='replace').splitlines()
    except OSError as exc:
        unreadable.append('%s: %s' % (path, exc))
        return []
    rows = []
    for line in lines:
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if isinstance(row, dict):
            rows.append(row)
    return rows


def _turns(ledger, since_ms, keep=4):
    """Rich's last turns from the conversation ledger: the prompt, his reply and its end. An
    AssistantDelta names its turn only, so a turn is read by turn_id (hire-walk.py said_since)."""
    turns, order = {}, []
    for row in ledger:
        turn = row.get('turn_id')
        if not turn:
            continue
        if turn not in turns:
            turns[turn] = {'turn_id': turn, 'reply': ''}
            order.append(turn)
        t, event = turns[turn], row.get('event')
        if event == 'PromptReceived':
            t.update(at=row.get('at'), source=row.get('source'), thread=row.get('thread_id'),
                     prompt=str(row.get('text', ''))[:400])
        elif event == 'TurnStarted':
            t['started_at'] = row.get('at')
        elif event == 'AssistantDelta':
            t['reply'] += str(row.get('text', ''))
        elif event in TURN_ENDS:
            t['ended'] = {k: row.get(k) for k in ('event', 'at', 'stop_reason', 'reason', 'requested_at', 'by_turn_id')
                          if row.get(k) is not None}
    picked = [turns[t] for t in order if (turns[t].get('at') or 0) >= since_ms] or [turns[t] for t in order]
    for t in picked:
        t['reply'] = t['reply'][-1500:]
    return picked[-keep:]


def _session(transcript, unreadable, keep=3):
    """A session's last words and tool calls, from its own Claude Code transcript."""
    said, tools, last = [], [], None
    for row in _jsonl(transcript, unreadable):
        message = row.get('message') if isinstance(row.get('message'), dict) else {}
        content = message.get('content')
        if row.get('timestamp'):
            last = row['timestamp']
        if row.get('type') != 'assistant' or not isinstance(content, list):
            continue
        for block in content:
            if not isinstance(block, dict):
                continue
            if block.get('type') == 'text' and str(block.get('text', '')).strip():
                said.append({'at': row.get('timestamp'), 'text': str(block['text'])[-1500:]})
            elif block.get('type') == 'tool_use':
                tools.append({'at': row.get('timestamp'), 'name': block.get('name'),
                              'input': json.dumps(block.get('input'))[:400]})
    return {'last_at': last, 'said': said[-keep:], 'tools': tools[-5:]}


def evidence(data, home, since_ms):
    data, home, unreadable = Path(data), Path(home), []
    state = data / 'engine-state'
    assignments = []
    for path in sorted(state.glob('assignments/*/*.json')):
        try:
            record = json.loads(path.read_text())
        except (OSError, ValueError) as exc:
            unreadable.append('%s: %s' % (path, exc))
            continue
        row = {k: record.get(k) for k in ASSIGNMENT_KEYS}
        row['notices'] = [n.get('text') for n in (record.get('notices') or [])[-2:] if isinstance(n, dict)]
        assignments.append(row)
    assignments.sort(key=lambda r: r.get('registered_at_ms') or 0)
    sessions = []
    for binding in state.glob('evidence/*/binding.json'):
        session = binding.parent.name
        try:
            role = json.loads(binding.read_text()).get('role')
        except (OSError, ValueError) as exc:
            role = None
            unreadable.append('%s: %s' % (binding, exc))
        # The transcript the session's own hooks name first: a session on another Claude
        # account writes under that account's folder, never HOME/.claude (review
        # rv-20261022T222750Z-aae6dfac-ab10); HOME/.claude is the fallback.
        named = [c.get('transcript_path') for c in (r.get('callback') for r in _jsonl(binding.parent / 'callbacks.jsonl', []))
                 if isinstance(c, dict) and c.get('transcript_path')]
        found = [Path(p) for p in named if Path(p).is_file()][:1] or sorted(home.glob('.claude/projects/*/%s.jsonl' % session))
        row = {'session': session, 'role': role, 'transcript': str(found[0]) if found else None}
        if found:
            row.update(_session(found[0], unreadable))
        sessions.append(row)
    sessions.sort(key=lambda r: r.get('last_at') or '')
    inboxes = []
    for path in sorted(state.glob('messages/*/*/stu.jsonl')):
        inboxes.append({'file': str(path), 'last': [{k: r.get(k) for k in ('from', 'at', 'text')}
                                                    for r in _jsonl(path, unreadable)[-5:]]})
    ledger = _jsonl(data / 'conversation-ledger.jsonl', unreadable)
    return {'since_ms': since_ms, 'now_ms': int(time.time() * 1000), 'assignments': assignments,
            'rich_turns': _turns(ledger, since_ms), 'sessions': sessions[-6:], 'stu_inboxes': inboxes,
            'unreadable': unreadable}


def main(argv):
    seconds = 0.0
    if '--seconds' in argv:
        i = argv.index('--seconds')
        seconds = float(argv[i + 1])
        argv = argv[:i] + argv[i + 2:]
    if argv[:1] == ['watch'] and len(argv) == 3:
        return watch(argv[1], argv[2], seconds or 3600)
    if argv[:1] == ['group'] and len(argv) == 2:
        return group(int(argv[1]))
    if argv[:1] == ['record'] and len(argv) == 2:
        return record(argv[1], seconds or 120)
    if argv[:1] == ['evidence'] and len(argv) == 4 and argv[3].isdigit():
        print(json.dumps(evidence(argv[1], argv[2], int(argv[3]))))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

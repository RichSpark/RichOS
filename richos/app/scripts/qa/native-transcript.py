#!/usr/bin/env python3
"""native-transcript.py - Claude Code's own session transcripts as one timeline.

  native-transcript.py ROOT [--since T] [--until T] [--tool NAME ...] [--grep TEXT]
                            [--results] [--text] [--sessions] [--width N]

ROOT is a Claude Code `projects` folder, one project folder inside it, or one session's .jsonl
(a walk keeps the guest's `~/.claude/projects` as `native-projects`). Every session file under
it is read, and so is every helper's file (`<session>/subagents/agent-*.jsonl`, with the
`agentType` and `description` of its `.meta.json`), so Rich, Stu and each worker are on one clock.

One line per event, oldest first:

  HH:MM:SS.mmm  WHO  KIND  NAME  DETAIL

WHO is the session id's first 8 characters, or `<session8>/<agent id>` for a helper. KIND is
  USE     a tool call: its name, then the input fields a reader needs (description,
          subagent_type, run_in_background, name, team_name, isolation, command, file_path,
          text, prompt), cut to --width
  RESULT  that call's result, `ok` or `ERR`, and its first characters (with --results)
  USER    a user-role string or text block: the delivered prompt, a task notification
  SAY     the model's own text (with --text)
  QUEUED  a message delivered while a turn was running (Claude Code's queued_command)
  QUEUE   its queue operation (enqueue, or remove with the reason, e.g. absorbed_mid_turn)
A streamed message repeats its blocks across rows; each tool call, result and text is printed once.

  --since/--until T   keep events at or after / before T: `HH:MM[:SS]` compares the time of
                      day, a full ISO time compares the whole stamp
  --tool NAME         keep calls of NAME (repeatable; `mcp__richos_status__*` ends in a star
                      to match a prefix) and, with --results, their results
  --grep TEXT         keep lines containing TEXT
  --sessions          instead of events: one line per file, its first and last time, its rows,
                      and its tool calls counted by name

Exit 0 with at least one line printed; 1 when the filters kept nothing (a filter that matched
nothing is an answer, never an empty success); 2 when ROOT is missing, holds no .jsonl, or no
row in any file is JSON. A live transcript's unfinished last line is skipped and counted on
stderr. Nothing is written. Added with the nightly 60 walk: the nightly 59 walk read these
transcripts with twenty-odd one-off commands.
"""
import argparse
import json
from pathlib import Path
import sys

FIELDS = ('description', 'subagent_type', 'run_in_background', 'name', 'team_name', 'isolation',
          'command', 'file_path', 'text', 'prompt')


def files_under(root):
    if root.is_file():
        return [root]
    return sorted(p for p in root.rglob('*.jsonl') if p.is_file())


def who(path):
    if path.parent.name == 'subagents':
        return path.parent.parent.name[:8] + '/' + path.stem.replace('agent-', '')
    return path.stem[:8]


def meta(path):
    m = path.with_suffix('.meta.json')
    if not m.exists():
        return ''
    try:
        d = json.loads(m.read_text())
    except (OSError, ValueError):
        return ''
    return ' '.join(f'{k}={d[k]}' for k in ('agentType', 'description') if d.get(k))


def flat(value):
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        return ' '.join(flat(v) for v in value)
    if isinstance(value, dict):
        return flat(value.get('text')) if 'text' in value else json.dumps(value)
    return '' if value is None else str(value)


def one_line(text, width):
    text = ' '.join(str(text).split())
    return text if len(text) <= width else text[:width - 3] + '...'


def events(path, a, bad):
    label = who(path)
    seen = set()
    rows = 0
    with path.open(errors='replace') as f:
        for line in f:
            try:
                row = json.loads(line)
            except ValueError:
                bad[0] += 1
                continue
            rows += 1
            ts = row.get('timestamp') or ''
            # A message delivered while a turn runs: queued, then absorbed mid-turn or at its end.
            att = row.get('attachment') or {}
            if row.get('type') == 'attachment' and att.get('type') == 'queued_command':
                yield ts, label, 'QUEUED', '', flat(att.get('prompt')), None
                continue
            if row.get('type') == 'queue-operation':
                yield ts, label, 'QUEUE', row.get('operation') or '', (row.get('reason') or '') + ' ' + flat(row.get('content')), None
                continue
            msg = row.get('message') or {}
            role = msg.get('role') or row.get('type')
            content = msg.get('content')
            if isinstance(content, str):
                if role == 'user':
                    yield ts, label, 'USER', '', content, None
                continue
            for i, b in enumerate(content or []):
                if not isinstance(b, dict):
                    continue
                t = b.get('type')
                if t == 'tool_use':
                    if b.get('id') in seen:
                        continue
                    seen.add(b.get('id'))
                    inp = b.get('input') or {}
                    detail = ' '.join(f'{k}={flat(inp[k])}' for k in FIELDS if k in inp) or json.dumps(inp)
                    yield ts, label, 'USE', b.get('name', ''), detail, b.get('id')
                elif t == 'tool_result' and a.results:
                    key = ('r', b.get('tool_use_id'))
                    if key in seen:
                        continue
                    seen.add(key)
                    yield ts, label, 'RESULT', '', ('ERR ' if b.get('is_error') else 'ok ') + flat(b.get('content')), b.get('tool_use_id')
                elif t == 'text' and (role == 'user' or a.text):
                    key = ('t', msg.get('id'), i, b.get('text'))
                    if key in seen:
                        continue
                    seen.add(key)
                    yield ts, label, 'USER' if role == 'user' else 'SAY', '', b.get('text', ''), None
    if rows == 0 and path.stat().st_size:
        bad[1] += 1
    bad[2] += rows


def in_window(ts, a):
    for bound, keep in ((a.since, lambda x, b: x >= b), (a.until, lambda x, b: x < b)):
        if not bound:
            continue
        x = ts if 'T' in bound else ts[11:11 + len(bound)]
        if not x or not keep(x, bound):
            return False
    return True


def named(name, wanted):
    return any(name.startswith(t[:-1]) if t.endswith('*') else name == t for t in wanted)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('root', type=Path)
    p.add_argument('--since')
    p.add_argument('--until')
    p.add_argument('--tool', action='append')
    p.add_argument('--grep')
    p.add_argument('--results', action='store_true')
    p.add_argument('--text', action='store_true')
    p.add_argument('--sessions', action='store_true')
    p.add_argument('--width', type=int, default=200)
    a = p.parse_args()
    if not a.root.exists():
        print(f'native-transcript: {a.root} does not exist', file=sys.stderr)
        return 2
    files = files_under(a.root)
    if not files:
        print(f'native-transcript: no .jsonl transcript under {a.root}', file=sys.stderr)
        return 2
    bad = [0, 0, 0]  # unparseable lines, files with no JSON row, JSON rows read
    printed = 0
    if a.sessions:
        for path in files:
            first = last = None
            counts = {}
            for ts, _, kind, name, _, _ in events(path, argparse.Namespace(results=False, text=False), bad):
                if ts:
                    first = first or ts
                    last = ts
                if kind == 'USE':
                    counts[name] = counts.get(name, 0) + 1
            calls = ' '.join(f'{k}={v}' for k, v in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))
            line = f'{who(path)}  first {first or "-"}  last {last or "-"}  {meta(path)}  calls: {calls or "none"}'
            if a.grep and a.grep not in line:
                continue
            print(one_line(line, max(a.width, 400)))
            printed += 1
    else:
        rows = []
        for path in files:
            rows.extend(events(path, a, bad))
        rows.sort(key=lambda r: r[0])
        kept = set()
        names = {r: n for _, _, k, n, _, r in rows if k == 'USE'}
        for ts, label, kind, name, detail, ref in rows:
            if not in_window(ts, a):
                continue
            if a.tool:
                if kind == 'USE' and named(name, a.tool):
                    kept.add(ref)
                elif not (kind == 'RESULT' and ref in kept):
                    continue
            if kind == 'RESULT':
                name = names.get(ref, '?')
            line = f'{ts[11:23] or "-":12}  {label}  {kind}  {name}  {one_line(detail, a.width)}'.rstrip()
            if a.grep and a.grep not in line:
                continue
            print(line)
            printed += 1
    if bad[0]:
        print(f'native-transcript: skipped {bad[0]} line(s) that are not JSON (a live file\'s unfinished last line is one)', file=sys.stderr)
    if bad[2] == 0:
        print(f'native-transcript: no row in {len(files)} file(s) under {a.root} is JSON', file=sys.stderr)
        return 2
    return 0 if printed else 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""routing-probe.py - which way does the app's routing step send a recorded request?

  routing-probe.py --engine DIR --case NAME [--runs N] [--claude PATH]

Before Rich answers a visible request, the app asks a tool-free Haiku process whether it is a new
handoff to Stu (`request_classifier.rs`, `routing.rs`). When it says `new_stu_handoff`, the app
saves the handoff to Stu before Rich's turn starts. That process reads only instruction files:
its standing prompt is `app/routing.md`, and every request carries Rich's and Stu's own
definitions (`app/agents/rich.md`, `app/agents/stu.md`) as their roles (CEO §124). So a wrong
route is fixed in that text and proved against the real model, not in app code.

This starts the installed `claude` exactly as `Provider::start` does (Haiku, low effort, the
standing prompt, no tools, no MCP servers, no setting sources, no session persistence), sends the
same warm-up the app sends, plays the case's earlier requests in the same process as the app does
for one conversation, then sends the judged request and reads the label.

DIR is the engine folder (`richos/engine` of a checkout). Nothing in it is changed.

Cases (each is a request Ray saw routed in a walk, and the label it must never get):

  write-it-yourself  Nightly 66 run P: "Please write a 1,500-word guide to running a coaching
                     discovery call. Write it yourself, right here in the chat." was routed to Stu
                     (`"classifier":"stu"`), so Stu wrote the guide while Rich wrote it in the
                     chat. The user asked Rich; it must not be a new handoff to Stu.
  write-it-yourself-first
                     Nightly 63 run G: "Please write a 4,000-word guide to running a coaching
                     business's first year. Write it yourself, right here." as the conversation's
                     first message, routed to Stu the same way.
  run-it-yourself    The reap walk's stop ask (walk walk-2bfb274d19d0, bundle 8cdad8794): "Please
                     run these two harmless test commands for me yourself with your shell tool, in
                     my Acme folder ..." was handed to Stu while Rich ran both commands himself.
                     The user asked Rich; it must not be a new handoff to Stu.
  run-it-as-a-job    The same ask without "yourself", as reap-walk.py words it now: a five-minute
                     job, which must reach Stu.
  delegated-work    The other direction, from the same run P: "please have Echo write a short
                     welcome email ... commit it to main and push it" is work for Stu, and
                     must reach Stu: anything but new_stu_handoff leaves it with Rich.

Prints one line per run (PASS/FAIL and the label), then a count. Exit 0 when every run passes,
1 when any run fails, 2 when the probe cannot run. The live model is not deterministic: a count is
the result, and the same --runs is used before and after a change.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# The app's own launch flags (`request_classifier.rs`, `Provider::start`).
SETTINGS = json.dumps({'disableAllHooks': True, 'autoMemoryEnabled': False,
                       'claudeMdExcludes': ['**/CLAUDE.md', '**/CLAUDE.local.md', '**/.claude/rules/**']},
                      separators=(',', ':'))
LABELS = ('rich_handles_message', 'new_stu_handoff', 'uncertain')

WELCOME = ("In the Northwind project folder, please have Echo write a short welcome email for new members "
           "into docs/welcome.md. Before writing, ask me two separate questions and don't guess either: "
           "whether it should be formal or casual, and whether it should mention pricing. Then commit it to "
           "main and push it to the origin.")

# The words the app sent, recovered from nightly 66 run P's saved handoff (its
# `original_user_request` and `recent_conversation`).
CASES = {
    'write-it-yourself': {
        'earlier': [
            ('What is 17 times 19?', ''),
            (WELCOME, 'user: What is 17 times 19?\nassistant: 17 × 19 = **323**.'),
        ],
        'request': ('Please write a 1,500-word guide to running a coaching discovery call. '
                    'Write it yourself, right here in the chat.'),
        'context': (
            'user: What is 17 times 19?\n'
            'assistant: 17 × 19 = **323**.\n'
            f'user: {WELCOME}\n'
            "assistant: I've passed this to Stu, who's running it with Echo. Before anything gets written, "
            'two separate question cards will show up here: one asks whether the email should be formal or '
            "casual, and the other asks whether it should mention pricing. Once you've answered both, Echo "
            'writes docs/welcome.md, and then it gets committed to main and pushed to origin. '
            "I'll let you know when that's done.\n"
            'assistant: Both questions are on your screen now: one asks whether the email should be formal '
            "or casual, and the other asks whether it should mention pricing. Echo will start writing as soon "
            "as you've answered both. After that, the file gets committed to main and pushed to origin."),
        'bad': 'new_stu_handoff',
    },
    'write-it-yourself-first': {
        'earlier': [],
        'request': ("Please write a 4,000-word guide to running a coaching business's first year. "
                    'Write it yourself, right here.'),
        'context': '',
        'bad': 'new_stu_handoff',
    },
    'run-it-yourself': {
        'earlier': [],
        # reap-walk.py ask_text('stop') as that walk sent it.
        'request': ('Please run these two harmless test commands for me yourself with your shell tool, in my '
                    'Acme folder, and tell me when they have finished and what they printed. First start this '
                    'one in the background (run_in_background: true): python3 heartbeat.py beat stop-bg '
                    '/tmp/reap-walk   Then run this one in the foreground and wait for it: python3 heartbeat.py '
                    'beat stop-fg /tmp/reap-walk --seconds 300   heartbeat.py is my own harmless test script at '
                    'the top of that folder: it records its own process id and writes the time into small files '
                    'under /tmp/reap-walk until it ends.'),
        'context': '',
        'bad': 'new_stu_handoff',
    },
    'run-it-as-a-job': {
        'earlier': [],
        # reap-walk.py ask_text('stop') since it stopped asking Rich to do it himself.
        'request': ('Please run these two harmless test commands for me with a shell tool, in my '
                    'Acme folder, and tell me when they have finished and what they printed. First start this '
                    'one in the background (run_in_background: true): python3 heartbeat.py beat stop-bg '
                    '/tmp/reap-walk   Then run this one in the foreground and wait for it: python3 heartbeat.py '
                    'beat stop-fg /tmp/reap-walk --seconds 300   heartbeat.py is my own harmless test script at '
                    'the top of that folder: it records its own process id and writes the time into small files '
                    'under /tmp/reap-walk until it ends.'),
        'context': '',
        'bad': 'rich_handles_message',
    },
    'delegated-work': {
        'earlier': [('What is 17 times 19?', '')],
        'request': WELCOME,
        'context': 'user: What is 17 times 19?\nassistant: 17 × 19 = **323**.',
        'bad': 'rich_handles_message',
    },
}


def message(content):
    return json.dumps({'type': 'user', 'message': {'role': 'user', 'content': content}}) + '\n'


def request(engine, original, context):
    """The body `RequestClassifier::classify` sends (serde_json orders the keys)."""
    roles = {name: (engine / 'app' / 'agents' / f'{name}.md').read_text() for name in ('rich', 'stu')}
    return message(json.dumps({'user_request': original, 'recent_conversation': context, 'roles': roles},
                              sort_keys=True, separators=(',', ':'), ensure_ascii=False))


def one_run(args, engine, case, cwd):
    argv = [args.claude, '-p', '--model', 'claude-haiku-5-5', '--effort', 'low',
            '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose',
            '--system-prompt', (engine / 'app' / 'routing.md').read_text(), '--tools', '',
            '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}', '--setting-sources', '',
            '--no-session-persistence', '--settings', SETTINGS, '--permission-mode', 'bypassPermissions']
    env = {k: v for k, v in os.environ.items() if k != 'CLAUDECODE'}
    child = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True)
    try:
        sends = [message('{"warmup":true}')] + [request(engine, text, context) for text, context in case['earlier']]
        sends.append(request(engine, case['request'], case['context']))
        label = None
        for body in sends:
            child.stdin.write(body)
            child.stdin.flush()
            # `Live::exchange`'s own checks: a result counts only after an answer from Haiku with
            # no tool call, and only when it is not an error; otherwise the app routes nothing.
            model_seen, invalid = False, False
            for line in child.stdout:
                try:
                    frame = json.loads(line)
                except ValueError:
                    continue
                if frame.get('type') == 'assistant':
                    reply = frame.get('message') or {}
                    invalid = invalid or reply.get('model') != 'claude-haiku-5-5' or any(
                        block.get('type') == 'tool_use' for block in reply.get('content') or [])
                    model_seen = True
                if frame.get('type') == 'result':
                    failed = invalid or not model_seen or frame.get('is_error') is not False
                    label = '' if failed else (frame.get('result') or '').strip()
                    break
            else:
                raise RuntimeError('the classifier ended without a result')
        return label
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--engine', required=True, help='engine folder holding app/routing.md (richos/engine)')
    parser.add_argument('--case', required=True, choices=sorted(CASES))
    parser.add_argument('--runs', type=int, default=10)
    parser.add_argument('--claude', default=shutil.which('claude'))
    args = parser.parse_args()
    engine = Path(args.engine)
    if not args.claude or not os.access(args.claude, os.X_OK):
        print('routing-probe: no claude executable (use --claude)', file=sys.stderr)
        return 2
    if not (engine / 'app' / 'routing.md').is_file():
        print(f'routing-probe: {engine} has no app/routing.md', file=sys.stderr)
        return 2
    case = CASES[args.case]
    failed = 0
    with tempfile.TemporaryDirectory(prefix='routing-probe-') as cwd:
        for n in range(1, args.runs + 1):
            try:
                label = one_run(args, engine, case, cwd)
            except (RuntimeError, OSError) as error:
                print(f'routing-probe: run {n} could not be judged: {error}', file=sys.stderr)
                return 2
            # Only new_stu_handoff hands the request to Stu (`routing.rs`). Anything else, a failed
            # classifier and an answer that is not exactly a label included ("Invalid classifier
            # decision", `request_classifier.rs` `choice`), leaves it with Rich.
            shown = (label if label in LABELS else 'the classifier failed, so Rich keeps it' if not label
                     else 'not a label, so Rich keeps it: ' + json.dumps(label[:120]))
            routed = label if label == 'new_stu_handoff' else 'rich_handles_message'
            failed += routed == case['bad']
            print(f"{'FAIL' if routed == case['bad'] else 'PASS'} run {n}: {shown}", flush=True)
    print(f'{args.case}: {args.runs - failed} of {args.runs} passed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())

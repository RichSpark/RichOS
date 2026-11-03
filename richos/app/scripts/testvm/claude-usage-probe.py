#!/usr/bin/env python3
"""claude-usage-probe.py — in the guest: does `claude` answer get_usage under one account's folder?

  python3 claude-usage-probe.py <CLAUDE_CONFIG_DIR> <HOME> [--claude PATH] [--seconds N]

The same control-only child the app's quota reader starts (no prompt, no tools, no settings, no
MCP, no session): `initialize`, then `get_usage`. Prints one JSON line: whether it answered, the
answer row (usage figures only), and the end of stderr. It reads and prints no credential.
For a walk where an account shows no reading, to tell a sign-in that does not work from an app
that never asked.
"""
import json
import os
import selectors
import signal
import math
import subprocess
import sys
import time


def read_wire(args, env, seconds, requests=(), gap=0, response_id=None, cwd='/var/empty'):
    """Bound input scheduling and BOTH output pipes by one monotonic deadline.

    Keep the direct child unreaped until its group is stopped. Its PID then cannot
    be reused as an unrelated group between leader exit and cleanup.
    """
    end = time.monotonic() + seconds
    p = subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    buf, err = b'', b''
    pending = list(requests)
    next_send = time.monotonic()
    try:
        with selectors.DefaultSelector() as sel:
            sel.register(p.stdout, selectors.EVENT_READ)
            sel.register(p.stderr, selectors.EVENT_READ)
            while sel.get_map():
                now = time.monotonic()
                if now >= end:
                    break
                if pending and now >= next_send:
                    try:
                        p.stdin.write((json.dumps(pending.pop(0)) + '\n').encode())
                        p.stdin.flush()
                    except BrokenPipeError:
                        pending.clear()
                    next_send = now + gap
                wait = end - now
                if pending:
                    wait = min(wait, max(0, next_send - now))
                for key, _ in sel.select(timeout=wait):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        sel.unregister(key.fileobj)
                    elif key.fileobj is p.stdout:
                        buf += chunk
                    else:
                        err = (err + chunk)[-1500:]
                if response_id:
                    for line in buf.splitlines(keepends=True):
                        if not line.endswith(b'\n'):
                            continue
                        try:
                            row = json.loads(line)
                            if row.get('type') == 'control_response' and row.get('response', {}).get('request_id') == response_id:
                                return buf, err
                        except (ValueError, AttributeError):
                            pass
    finally:
        # No poll()/wait() before this signal, even when stdout reached EOF.
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        p.wait(timeout=5)
        for pipe in (p.stdin, p.stdout, p.stderr):
            pipe.close()
    return buf, err


def main(argv):
    folder, home = argv[1], argv[2]
    claude = argv[argv.index('--claude') + 1] if '--claude' in argv else '/Users/admin/.local/bin/claude'
    seconds = float(argv[argv.index('--seconds') + 1]) if '--seconds' in argv else 40
    env = dict(os.environ, CLAUDE_CONFIG_DIR=folder, HOME=home)
    env.pop('CLAUDECODE', None)
    args = [claude, '--print', '--input-format=stream-json', '--output-format=stream-json', '--verbose',
            '--setting-sources', '', '--no-session-persistence', '--tools', '', '--strict-mcp-config',
            '--mcp-config', '{"mcpServers":{}}']
    # --debug-file PATH: Claude Code's own debug log of this one probe, and the lines about the
    # usage fetch are printed with the answer (why `rate_limits` came back null).
    debug = argv[argv.index('--debug-file') + 1] if '--debug-file' in argv else None
    if debug:
        args += ['--debug-file', debug]
    if not math.isfinite(seconds) or seconds <= 0:
        raise ValueError('--seconds must be finite and positive')
    began = time.monotonic()
    requests = [{'type': 'control_request', 'request_id': 'probe-%d' % n,
                 'request': {'subtype': sub, 'hooks': {}}}
                for n, sub in ((1, 'initialize'), (2, 'get_usage'))]
    buf, error = read_wire(args, env, seconds, requests, response_id='probe-2')
    err = error.decode(errors='replace')
    rows = []
    for line in buf.decode(errors='replace').splitlines():
        try:
            row = json.loads(line)
            if row.get('type') == 'control_response' and row.get('response', {}).get('request_id') == 'probe-2':
                rows.append(line)
        except (ValueError, AttributeError):
            pass
    usage_lines = []
    if debug:
        try:
            with open(debug, encoding='utf-8', errors='replace') as fh:
                usage_lines = [line.strip()[:400] for line in fh if 'usage' in line.lower() or '429' in line][-20:]
        except OSError as exc:
            usage_lines = ['no debug log: ' + str(exc)]
    print(json.dumps({'folder': folder, 'answered': bool(rows), 'seconds': round(time.monotonic() - began, 1),
                      'answer': rows[0][:2500] if rows else None, 'stderr': err, 'debug_usage_lines': usage_lines}))
    return 0 if rows else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))

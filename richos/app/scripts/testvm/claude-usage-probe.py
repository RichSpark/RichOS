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
import subprocess
import sys
import time


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
    began = time.time()
    p = subprocess.Popen(args, cwd='/var/empty', env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE)
    for n, sub in ((1, 'initialize'), (2, 'get_usage')):
        p.stdin.write((json.dumps({'type': 'control_request', 'request_id': 'probe-%d' % n,
                                   'request': {'subtype': sub, 'hooks': {}}}) + '\n').encode())
        p.stdin.flush()
    sel = selectors.DefaultSelector()
    sel.register(p.stdout, selectors.EVENT_READ)
    end, buf = time.time() + seconds, b''
    while time.time() < end and b'"probe-2"' not in buf:
        if sel.select(timeout=1):
            chunk = os.read(p.stdout.fileno(), 65536)
            if not chunk:
                break
            buf += chunk
    p.kill()
    err = p.stderr.read().decode(errors='replace')[-1500:]
    rows = [line for line in buf.decode(errors='replace').splitlines() if '"probe-2"' in line]
    usage_lines = []
    if debug:
        try:
            with open(debug, encoding='utf-8', errors='replace') as fh:
                usage_lines = [line.strip()[:400] for line in fh if 'usage' in line.lower() or '429' in line][-20:]
        except OSError as exc:
            usage_lines = ['no debug log: ' + str(exc)]
    print(json.dumps({'folder': folder, 'answered': bool(rows), 'seconds': round(time.time() - began, 1),
                      'answer': rows[0][:2500] if rows else None, 'stderr': err, 'debug_usage_lines': usage_lines}))
    return 0 if rows else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))

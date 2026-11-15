#!/usr/bin/env python3
"""browser-reader-guest.py — run INSIDE a test guest by browser-reader-walk.py, never on the host.

It drives the app's own `richos_browser` tool server (the executable with `--browser-mcp`) over
stdio exactly as claude drives it, ONE call at a time, the way a teammate uses it: open a page,
read the answer, choose an item by what it is called, press its number, read the page again.

  browser-reader-guest.py --exe RICHOS_TAURI < PLAN.json > RESULT.json

PLAN is {"steps": [STEP, ...]}. A STEP is one of:

  {"do": "tools"}                     the server's tool names
  {"do": "open", "url": U}            open_page
  {"do": "press", "name": REGEX}      press the first item of the page as last read whose name
                                      matches REGEX; "kind" narrows it to link, button, tab...
  {"do": "scroll"}                    scroll_down
  {"do": "next"}                      next_part
  {"do": "close"}                     close_page

Every step's row carries the tool's whole answer ("answer", the page text included; a long page's
following parts are asked for and joined as "whole"), whether the tool called it an error, and the
seconds it took. It judges nothing: the walk on the host does.
The server ends when this script closes its input, which is when it must close its own tab.
"""
import argparse
import json
import re
import select
import subprocess
import sys
import time

ITEM = r'\[(\d+) (%s) "((?:[^"\\]|\\.)*?)"'


class Server:
    def __init__(self, exe):
        self.proc = subprocess.Popen([exe, '--browser-mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.next = 0
        self.rpc('initialize', {})

    def rpc(self, method, params=None, timeout=180):
        self.next += 1
        frame = {'jsonrpc': '2.0', 'id': self.next, 'method': method}
        if params is not None:
            frame['params'] = params
        self.proc.stdin.write(json.dumps(frame) + '\n')
        self.proc.stdin.flush()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.proc.stdout], [], [], max(0.0, deadline - time.monotonic()))
            if not ready:
                break
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError('the tool server ended before it answered %s' % method)
            if line.strip().startswith('{'):
                reply = json.loads(line)
                if reply.get('id') == self.next:
                    return reply
        raise RuntimeError('the tool server did not answer %s within %d seconds' % (method, timeout))

    def call(self, name, arguments):
        result = self.rpc('tools/call', {'name': name, 'arguments': arguments}).get('result') or {}
        text = (result.get('content') or [{}])[0].get('text', '')
        try:
            answer = json.loads(text)
        except ValueError:
            answer = text
        return bool(result.get('isError')), answer

    def end(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(60)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            return 'killed after 60 seconds'
        return self.proc.returncode


def whole(server, answer):
    """A long page's following parts asked for and joined into the answer as `whole`."""
    if not isinstance(answer, dict) or 'page' not in answer:
        return answer
    pages, last = [answer['page']], answer
    while any('next_part gives' in note for note in last.get('notes', [])):
        error, last = server.call('next_part', {})
        if error or not isinstance(last, dict):
            answer['parts_failed'] = str(last)
            break
        pages.append(last['page'])
    if len(pages) > 1:
        answer['whole'] = '\n'.join(pages)
        answer['parts'] = len(pages)
    return answer


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--exe', required=True)
    a = p.parse_args()
    plan = json.load(sys.stdin)
    server = Server(a.exe)
    rows, page = [], ''
    for step in plan['steps']:
        began = time.monotonic()
        row = {'step': step}
        try:
            do = step['do']
            if do == 'tools':
                row['answer'] = [t['name'] for t in server.rpc('tools/list')['result']['tools']]
            elif do == 'press':
                kind = step.get('kind', 'link|button|tab|menu|menu item|disclosure')
                found = [m for m in re.finditer(ITEM % kind, page) if re.search(step['name'], m.group(3))]
                if not found:
                    raise RuntimeError('no item named /%s/ on the page as last read' % step['name'])
                row['item'] = '%s %s "%s"' % found[0].groups()
                row['error'], row['answer'] = server.call('press', {'item': int(found[0].group(1))})
            else:
                name = {'open': 'open_page', 'scroll': 'scroll_down', 'next': 'next_part', 'close': 'close_page'}[do]
                row['error'], row['answer'] = server.call(name, {'url': step['url']} if do == 'open' else {})
            if do in ('open', 'press', 'scroll') and not row['error']:
                row['answer'] = whole(server, row['answer'])
            if isinstance(row.get('answer'), dict) and 'page' in row['answer']:
                page = row['answer'].get('whole') or row['answer']['page']
        except (RuntimeError, KeyError, ValueError) as exc:
            row['failed'] = str(exc)
        row['seconds'] = round(time.monotonic() - began, 1)
        rows.append(row)
        if 'failed' in row and 'server ended' in row['failed']:
            break
    print(json.dumps({'rows': rows, 'server_exit': server.end()}))
    return 0


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""browser-reader-walk.py's judging and browser-reader-guest.py's driving, offline: the driver is
run for real against a stand-in tool server that speaks the app's `--browser-mcp` protocol (a long
page in parts, numbered items, a press that reveals text). The guest walk is its own proof."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

TESTVM = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('browser_reader_walk', TESTVM / 'browser-reader-walk.py')
walk = importlib.util.module_from_spec(spec)
sys.path.insert(0, str(TESTVM))
spec.loader.exec_module(walk)

# A stand-in for the app's tool server: one page in two parts, items 1 and 2; pressing item 2
# reveals a sentence and renumbers from 3; anything else is refused the way the app refuses.
SERVER = r'''
import json, sys
assert sys.argv[1] == '--browser-mcp'
opened = pressed = part = 0
def page():
    if pressed:
        return [{'page': '# Shop [3 button "Shipping" expanded] Ships from Rotterdam. [4 link "Help" -> /help]',
                 'address': 'https://shop.example/admin', 'items': '3 to 4'}]
    return [{'page': '# Shop [1 link "Orders (new)" -> /orders]', 'address': 'https://shop.example/admin', 'items': '1 to 2',
             'notes': ['The page is long: this is part 1 of 2. next_part gives the following part.']},
            {'page': '[2 button "Shipping" collapsed]', 'address': 'https://shop.example/admin',
             'notes': ['The page is long: this is part 2 of 2.']}]
for line in sys.stdin:
    r = json.loads(line)
    if 'id' not in r:
        continue
    name, args = r.get('params', {}).get('name'), r.get('params', {}).get('arguments', {})
    error, answer = False, None
    if r['method'] == 'initialize':
        result = {'serverInfo': {'name': 'richos_browser'}}
    elif r['method'] == 'tools/list':
        result = {'tools': [{'name': n} for n in ['open_page', 'press', 'scroll_down', 'next_part', 'close_page']]}
    else:
        if name == 'open_page':
            opened, pressed, part, answer = 1, 0, 0, page()[0]
        elif name == 'next_part' and opened and part + 1 < len(page()):
            part += 1
            answer = page()[part]
        elif name == 'press' and opened and args.get('item') == 2 and not pressed:
            pressed, part = 1, 0
            answer = dict(page()[0], pressed='2 button "Shipping"')
        elif name == 'close_page':
            answer = {'closed': 'The tab RichOS opened in Safari is closed.' if opened else 'No page was open.'}
            opened = 0
        else:
            error, answer = True, 'Nothing was pressed.'
        result = {'isError': error, 'content': [{'type': 'text', 'text': answer if error else json.dumps(answer)}]}
    print(json.dumps({'jsonrpc': '2.0', 'id': r['id'], 'result': result}), flush=True)
'''


class Driver(unittest.TestCase):
    def drive(self, steps):
        with tempfile.TemporaryDirectory() as d:
            exe = Path(d) / 'richos-tauri'
            exe.write_text('#!/bin/sh\nexec "%s" -c \'%s\' "$@"\n' % (sys.executable, SERVER.replace("'", "'\\''")))
            os.chmod(exe, 0o755)
            r = subprocess.run([sys.executable, str(TESTVM / 'browser-reader-guest.py'), '--exe', str(exe)],
                               input=json.dumps({'steps': steps}), capture_output=True, text=True, timeout=60)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout.splitlines()[-1])

    def test_a_page_in_parts_is_joined_and_an_item_is_pressed_by_what_it_is_called(self):
        result = self.drive([{'do': 'tools'}, {'do': 'open', 'url': 'https://shop.example/admin'},
                             {'do': 'press', 'name': '^Shipping$'}, {'do': 'press', 'name': '^Payroll$'}, {'do': 'close'}])
        tools, opened, pressed, missing, closed = result['rows']
        self.assertEqual(tools['answer'], walk.TOOLS)
        self.assertEqual(opened['answer']['parts'], 2)
        self.assertIn('[2 button "Shipping" collapsed]', opened['answer']['whole'], 'the second part was not joined')
        self.assertEqual(pressed['item'], '2 button "Shipping"', 'the item on the second part was not found by its name')
        self.assertIn('Ships from Rotterdam.', pressed['answer']['page'])
        self.assertIn('no item named /^Payroll$/', missing['failed'])
        self.assertIn('is closed', closed['answer']['closed'])
        self.assertEqual(result['server_exit'], 0, 'the server did not end when its input closed')
        # And the walk's judging of those rows.
        before = walk.whole(opened['answer'])
        self.assertEqual(walk.judge({'expect': ['Shop'], 'address': 'shop.example'}, opened, ''), [])
        self.assertEqual(walk.judge({'gains': ['Ships from Rotterdam']}, pressed, before), [])
        self.assertIn('did not appear', walk.judge({'gains': ['Shop']}, pressed, before)[0])
        self.assertIn('is not in the page', walk.judge({'expect': ['Payroll']}, opened, '')[0])
        self.assertIn('not at an address', walk.judge({'address': '/login'}, opened, '')[0])
        self.assertIn('do not say', walk.judge({'notes': ['rather than']}, pressed, before)[0])
        self.assertIn('notes say "is long"', walk.judge({'quiet': ['is long']}, opened, '')[0])
        self.assertEqual(walk.judge({'quiet': ['rather than']}, opened, ''), [])
        self.assertEqual(walk.judge({}, missing, before), [missing['failed']])

    def test_a_refusal_of_the_tool_is_a_failed_step_never_a_pass(self):
        result = self.drive([{'do': 'scroll'}])
        row = result['rows'][0]
        self.assertTrue(row['error'])
        self.assertIn('the tool refused', walk.judge({}, row, '')[0])


class LocalSite(unittest.TestCase):
    """browser-page-guest.py's pages for the reader's walks, served here and fetched as a browser would."""
    def test_the_store_shows_only_to_a_signed_in_browser_and_its_closed_panel_is_not_in_the_page(self):
        import http.server
        import threading
        import urllib.request
        spec = importlib.util.spec_from_file_location('browser_page_guest', TESTVM / 'browser-page-guest.py')
        site = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(site)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), site.Site)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            base = 'http://127.0.0.1:%d' % server.server_address[1]
            # load-bound: only against a hang, on a loopback fetch from a server in this process; the verdict is the page's text.
            get = lambda path, cookie=None: urllib.request.urlopen(urllib.request.Request(
                base + path, headers={'Cookie': cookie} if cookie else {}), timeout=60).read().decode()
            out = get('/store')
            self.assertIn(site.STORE_SIGNED_OUT, out)
            self.assertNotIn(site.STORE_ORDERS, out)
            signed = get('/store', site.COOKIE)
            self.assertIn(site.STORE_ORDERS, signed)
            self.assertIn('aria-expanded="false"', signed)
            # The panel's sentence is only in the script that a press runs, never in the page's text.
            self.assertNotIn('>' + site.STORE_HIDDEN, signed)
            self.assertIn('<div id="p" hidden></div>', signed)
            self.assertIn('target="_blank">Profile in a new tab', get('/links'))
        finally:
            server.shutdown()
            server.server_close()


class Pages(unittest.TestCase):
    def test_every_page_has_a_question_and_every_step_of_the_walk_is_a_method(self):
        for page in walk.PAGES:
            self.assertEqual(page['steps'][0]['do'], 'open', page['name'])
            for step in page['steps']:
                self.assertTrue(any(k in step for k in ('expect', 'gains', 'address', 'notes')), (page['name'], step))
        self.assertTrue(walk.STATE_PAGE['expect'])
        for name in walk.BROWSERS:
            self.assertIn('read-' + name, walk.STEPS)
            self.assertIn('states-' + name, walk.STEPS)
        for step in walk.STEPS:
            self.assertTrue(hasattr(walk.Walk, step.replace('-', '_')), step)


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""browser-page-guest.py — run INSIDE a test guest by browser-page-walk.py, never on the host.

Two jobs, both for the browser-page tool (the CEO, 2026-11-10: "we need a way for Rich to overcome
the LinkedIn barrier i.e. the problem of not being able to browse Linkedin without being
signed-in"; the user is signed in to every social profile in their default browser):

  serve --port P --dir D   a local site with three pages:
                           /article   a long article of numbered sentences (the known text the
                                      exposure measurement counts), most of it far below the
                                      first screen, plus text hidden with display:none that must
                                      NOT be read;
                           /sign-in   sets the cookie a "signed-in browser" holds;
                           /profile   a profile that shows a different sentence to a browser
                                      holding that cookie than to a plain fetch (the LinkedIn
                                      shape: signed out, you get a sign-in wall).
                           Its pid is written to D/server.pid; it serves until killed.

  measure --bundle B --url-contains U --expect F [--manual] [--enhanced]
                           how much of the page a browser exposes through macOS Accessibility:
                           every window of the app with bundle id B, every AXWebArea whose AXURL
                           contains U, the AXValue of every AXStaticText under it, and how many of
                           the sentences listed in F (one per line) appear in that text. --manual
                           sets AXManualAccessibility on the app first and --enhanced sets
                           AXEnhancedUserInterface (Chromium builds its web tree only once an
                           assistive client asks; which switch it honors is what is measured).
                           Prints one JSON object.

The reader here is Python over the same three calls the app's reader makes
(AXUIElementCreateApplication, AXUIElementCopyAttributeValue, AXUIElementSetAttributeValue); it
measured the exposure before the app's reader was written, as the brief ordered.
"""
import argparse
import ctypes
import http.server
import json
import os
import subprocess
import sys
import time

COOKIE = 'richos_walk_session=signed-in'
SIGNED_IN = 'Ada Walker, signed-in view: works on accessible browsers at Example Works.'
SIGNED_OUT = 'Sign in to see this profile.'
HIDDEN = 'This hidden sentence must never be read.'


def sentences(n=400):
    return ['Sentence %03d says the walk can read line %03d of this article.' % (i, i) for i in range(1, n + 1)]


def article_html():
    parts = ['<!doctype html><html><head><meta charset=utf-8><title>Walk article</title></head><body>',
             '<h1>Walk article</h1>']
    rows = sentences()
    for i in range(0, len(rows), 10):
        block = rows[i:i + 10]
        if i % 40 == 0:
            parts.append('<h2>Part %d</h2>' % (i // 40 + 1))
        if i % 30 == 10:
            parts.append('<ul>' + ''.join('<li>%s</li>' % s for s in block) + '</ul>')
        elif i % 30 == 20:
            parts.append('<table><tr>' + ''.join('<td>%s</td>' % s for s in block) + '</tr></table>')
        else:
            parts.append('<p>' + ' '.join('<span>%s</span>' % s for s in block) + '</p>')
    parts.append('<div style="display:none">%s</div></body></html>' % HIDDEN)
    return '\n'.join(parts)


class Site(http.server.BaseHTTPRequestHandler):
    def send(self, body, extra=()):
        data = body.encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Cache-Control', 'no-store')
        for k, v in extra:
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = self.path.split('?', 1)[0]
        if path == '/article':
            return self.send(article_html())
        if path == '/sign-in':
            return self.send('<!doctype html><title>Signed in</title><p>Signed in.</p>',
                             [('Set-Cookie', COOKIE + '; Path=/; Max-Age=86400')])
        if path == '/profile':
            signed = COOKIE in (self.headers.get('Cookie') or '')
            body = SIGNED_IN if signed else SIGNED_OUT
            return self.send('<!doctype html><title>Profile</title><h1>Profile</h1><p>%s</p>' % body)
        self.send_response(404)
        self.end_headers()

    def log_message(self, *a):
        pass


def serve(a):
    os.makedirs(a.dir, exist_ok=True)
    with open(os.path.join(a.dir, 'server.pid'), 'w') as f:
        f.write(str(os.getpid()))
    with open(os.path.join(a.dir, 'expected.txt'), 'w') as f:
        f.write('\n'.join(sentences()) + '\n')
    http.server.ThreadingHTTPServer(('127.0.0.1', a.port), Site).serve_forever()


class AX:
    def __init__(self):
        vp, c_long = ctypes.c_void_p, ctypes.c_long
        self.AS = AS = ctypes.cdll.LoadLibrary(
            '/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices')
        self.CF = CF = ctypes.cdll.LoadLibrary('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
        for fn, res, args in (
            ('CFStringCreateWithCString', vp, [vp, ctypes.c_char_p, ctypes.c_uint32]),
            ('CFGetTypeID', ctypes.c_ulong, [vp]), ('CFStringGetTypeID', ctypes.c_ulong, []),
            ('CFArrayGetTypeID', ctypes.c_ulong, []), ('CFURLGetTypeID', ctypes.c_ulong, []),
            ('CFURLGetString', vp, [vp]), ('CFArrayGetCount', c_long, [vp]),
            ('CFArrayGetValueAtIndex', vp, [vp, c_long]), ('CFStringGetLength', c_long, [vp]),
            ('CFStringGetMaximumSizeForEncoding', c_long, [c_long, ctypes.c_uint32]),
            ('CFStringGetCString', ctypes.c_bool, [vp, ctypes.c_char_p, c_long, ctypes.c_uint32]),
            ('CFRelease', None, [vp]),
        ):
            f = getattr(CF, fn)
            f.restype, f.argtypes = res, args
        for fn, res, args in (
            ('AXUIElementCreateApplication', vp, [ctypes.c_int]),
            ('AXUIElementCopyAttributeValue', ctypes.c_int, [vp, vp, ctypes.POINTER(vp)]),
            ('AXUIElementSetAttributeValue', ctypes.c_int, [vp, vp, vp]),
            ('AXUIElementCopyParameterizedAttributeValue', ctypes.c_int, [vp, vp, vp, ctypes.POINTER(vp)]),
            ('AXIsProcessTrusted', ctypes.c_bool, []),
        ):
            f = getattr(AS, fn)
            f.restype, f.argtypes = res, args
        self.UTF8 = 0x08000100
        self.names = {}
        self.true = ctypes.c_void_p.in_dll(CF, 'kCFBooleanTrue')

    def name(self, s):
        if s not in self.names:
            self.names[s] = self.CF.CFStringCreateWithCString(None, s.encode(), self.UTF8)
        return self.names[s]

    def string(self, ref):
        if not ref:
            return None
        t = self.CF.CFGetTypeID(ref)
        if t == self.CF.CFURLGetTypeID():
            ref = self.CF.CFURLGetString(ref)
            t = self.CF.CFGetTypeID(ref)
        if t != self.CF.CFStringGetTypeID():
            return None
        n = self.CF.CFStringGetMaximumSizeForEncoding(self.CF.CFStringGetLength(ref), self.UTF8) + 1
        buf = ctypes.create_string_buffer(n)
        return buf.value.decode('utf-8', 'replace') if self.CF.CFStringGetCString(ref, buf, n, self.UTF8) else ''

    def raw(self, el, attr):
        out = ctypes.c_void_p()
        err = self.AS.AXUIElementCopyAttributeValue(el, self.name(attr), ctypes.byref(out))
        return out.value if err == 0 else None

    def text(self, el, attr):
        return self.string(self.raw(el, attr))

    def marker_text(self, area):
        """The whole web area's text in two calls, the way VoiceOver reads a page: the text-marker
        range covering the element, then the string for that range."""
        rng = ctypes.c_void_p()
        if self.AS.AXUIElementCopyParameterizedAttributeValue(
                area, self.name('AXTextMarkerRangeForUIElement'), area, ctypes.byref(rng)) or not rng.value:
            return None
        out = ctypes.c_void_p()
        if self.AS.AXUIElementCopyParameterizedAttributeValue(
                area, self.name('AXStringForTextMarkerRange'), rng.value, ctypes.byref(out)):
            return None
        return self.string(out.value)

    def children(self, el):
        arr = self.raw(el, 'AXChildren')
        if not arr or self.CF.CFGetTypeID(arr) != self.CF.CFArrayGetTypeID():
            return []
        return [self.CF.CFArrayGetValueAtIndex(arr, i) for i in range(self.CF.CFArrayGetCount(arr))]


def pid_of(bundle):
    out = subprocess.run(['lsappinfo', 'info', '-only', 'pid', '-app', bundle], capture_output=True, text=True).stdout
    digits = ''.join(c for c in out.split('=')[-1] if c.isdigit())
    return int(digits) if digits else None


def measure(a):
    ax = AX()
    began = time.monotonic()
    pid = pid_of(a.bundle)
    result = {'bundle': a.bundle, 'pid': pid, 'trusted': bool(ax.AS.AXIsProcessTrusted()), 'manual': a.manual,
              'enhanced': a.enhanced}
    if not pid:
        result['error'] = 'not running'
        print(json.dumps(result))
        return 1
    app = ax.AS.AXUIElementCreateApplication(pid)
    if a.manual:
        result['manual_set'] = ax.AS.AXUIElementSetAttributeValue(app, ax.name('AXManualAccessibility'), ax.true)
        time.sleep(1.0)
    if a.enhanced:
        result['enhanced_set'] = ax.AS.AXUIElementSetAttributeValue(app, ax.name('AXEnhancedUserInterface'), ax.true)
        time.sleep(1.0)
    expected = [l.strip() for l in open(a.expect) if l.strip()] if a.expect else []
    deadline = time.monotonic() + a.timeout
    best = None
    while True:
        pass_began = time.monotonic()
        areas, urls = [], []
        windows = ax.children(app)
        stack = [(w, 0) for w in windows]
        deepest = 0
        while stack:
            el, depth = stack.pop()
            role = ax.text(el, 'AXRole')
            if role == 'AXWebArea':
                url = ax.text(el, 'AXURL') or ''
                urls.append(url)
                if a.url_contains in url:
                    areas.append(el)
                continue
            deepest = max(deepest, depth)
            if depth < 60:
                stack.extend((c, depth + 1) for c in ax.children(el))
        pieces, nodes = [], 0
        for area in areas:
            walk = [area]
            while walk:
                el = walk.pop()
                nodes += 1
                if ax.text(el, 'AXRole') == 'AXStaticText':
                    v = ax.text(el, 'AXValue')
                    if v:
                        pieces.append(v)
                walk.extend(reversed(ax.children(el)))
        walked = time.monotonic()
        text = ' '.join(pieces)
        found = sum(1 for s in expected if s in text)
        marker = ' '.join(filter(None, (ax.marker_text(area) for area in areas)))
        flat = ' '.join(marker.split())
        marker_found = sum(1 for s in expected if s in flat)
        best = {'app_children': len(windows), 'deepest_level_walked': deepest, 'web_areas_seen': urls, 'matching_areas': len(areas), 'nodes': nodes, 'text_chars': len(text),
                'expected': len(expected), 'found': found, 'hidden_read': HIDDEN in text,
                'signed_in_text': SIGNED_IN in text, 'signed_out_text': SIGNED_OUT in text,
                'walk_seconds': round(walked - pass_began, 2),
                'marker_chars': len(marker), 'marker_found': marker_found,
                'marker_missed': [x for x in expected if x not in flat][:8],
                'walk_missed': [x for x in expected if x not in text][:8], 'marker_hidden_read': HIDDEN in flat,
                'marker_seconds': round(time.monotonic() - walked, 2),
                'head': text[:300], 'tail': text[-300:], 'marker_head': marker[:300], 'marker_tail': marker[-300:]}
        done = found == len(expected) and marker_found == len(expected)
        if (expected and done) or (not expected and text) or time.monotonic() > deadline:
            break
        time.sleep(1)
    result.update(best)
    result['seconds'] = round(time.monotonic() - began, 2)
    print(json.dumps(result))
    return 0


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='mode', required=True)
    s = sub.add_parser('serve')
    s.add_argument('--port', type=int, required=True)
    s.add_argument('--dir', required=True)
    m = sub.add_parser('measure')
    m.add_argument('--bundle', required=True)
    m.add_argument('--url-contains', required=True)
    m.add_argument('--expect')
    m.add_argument('--manual', action='store_true')
    m.add_argument('--enhanced', action='store_true')
    m.add_argument('--timeout', type=float, default=20)
    a = p.parse_args()
    return serve(a) if a.mode == 'serve' else measure(a)


if __name__ == '__main__':
    sys.exit(main())

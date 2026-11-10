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

  serve-site --port P --dir D
                           the stand-in LinkedIn site linkedin-standin.py built in D (routed by
                           D/site.json), for linkedin-profile-walk.py: the saved profile, its
                           /details/experience/ page, company About pages (a numeric company
                           address redirected to its name, as LinkedIn does), all behind the
                           cookie /sign-in sets; without it, /authwall.

  default-browser --bundle B
                           make B the default browser for http and https and press "Use …" in
                           the confirmation macOS shows. Prints one JSON object.

  tab-bar --bundle B [--depth N] [--max N]
                           every window of B as Accessibility shows it outside its web pages
                           (role, subrole, title, description, value by depth): what the tab bar
                           looked like when linkedin-app-walk.py finds a tab left open.

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


STANDIN_SCRIPT = ('<script>document.addEventListener("click",function(e){var b=e.target.closest("[data-rx-more]");'
                  'if(!b)return;var t=document.getElementById(b.getAttribute("data-rx-more"));'
                  'if(t){t.style.display="inline";}b.remove();});</script>')


def standin_page(title, body):
    return ('<!doctype html><html><head><meta charset=utf-8><title>%s</title></head><body><main>%s</main>%s'
            '</body></html>') % (title, body, STANDIN_SCRIPT)


def standin_about(key, hidden):
    """A company About page for a company the stand-in has no saved page of: a name, an overview and
    the collapsed sentence the tool must bring back."""
    return standin_page('Company %s' % key, (
        '<h1>Company %s</h1><h2>Overview</h2><p>Stand-in About page for company %s.</p>'
        '<section><p>Visible part of a company overview. <span id="rx-company" style="display:none">'
        'Hidden company text %s came back after see more.</span></p>'
        '<button type="button" data-rx-more="rx-company">see more</button></section>') % (key, key, hidden))


def serve_site(a):
    """The stand-in LinkedIn site linkedin-standin.py built (its site.json routes it). Every /in/ and
    /company/ page needs the cookie /sign-in sets; without it the browser is sent to /authwall, as
    LinkedIn sends a signed-out visitor."""
    site = json.load(open(os.path.join(a.dir, 'site.json')))
    slug, about, expect = site['slug'], site['about'], site['expect']
    vanity = {row['vanity']: row['file'] for row in about.values()}

    class Handler(Site):
        def file(self, name):
            with open(os.path.join(a.dir, name), encoding='utf-8') as f:
                return self.send(f.read())

        def redirect(self, to):
            self.send_response(302)
            self.send_header('Location', to)
            self.send_header('Content-Length', '0')
            self.end_headers()

        def do_GET(self):
            path = self.path.split('?', 1)[0]
            parts = [p for p in path.split('/') if p]
            if path == '/sign-in':
                return self.send('<!doctype html><title>Signed in</title><p>Signed in.</p>',
                                 [('Set-Cookie', COOKIE + '; Path=/; Max-Age=86400')])
            if path == '/authwall':
                return self.send(standin_page('Sign in', '<h1>Join LinkedIn</h1><p>Sign in to see this profile.</p>'))
            if parts[:1] in (['in'], ['company']) and COOKIE not in (self.headers.get('Cookie') or ''):
                return self.redirect('/authwall?trk=standin')
            if parts == ['in', slug]:
                return self.file('profile.html')
            if parts == ['in', slug, 'details', 'experience']:
                return self.file('details-experience.html')
            if len(parts) == 4 and parts[:3] == ['in', slug, 'details']:
                return self.send(standin_page(parts[3], '<h2>%s</h2><p>Stand-in %s page.</p>' % (parts[3], parts[3])))
            if len(parts) == 3 and parts[0] == 'company' and parts[2] == 'about':
                key = parts[1]
                if key in about:
                    return self.redirect('/company/%s/about/' % about[key]['vanity'])
                if key in vanity:
                    return self.file(vanity[key])
                return self.send(standin_about(key, expect['companies'].get(key, 'RX-UNKNOWN')))
            self.send_response(404)
            self.send_header('Content-Length', '0')
            self.end_headers()

    with open(os.path.join(a.dir, 'server.pid'), 'w') as f:
        f.write(str(os.getpid()))
    http.server.ThreadingHTTPServer(('127.0.0.1', a.port), Handler).serve_forever()


def default_browser(a):
    """Make bundle id B the default for http and https, the way a browser's own "make default"
    does (LSSetDefaultHandlerForURLScheme), and press the button of the confirmation macOS shows
    (CoreServicesUIAgent) through Accessibility. Prints what the default is afterwards."""
    ax = AX()
    LS = ctypes.cdll.LoadLibrary('/System/Library/Frameworks/CoreServices.framework/CoreServices')
    LS.LSSetDefaultHandlerForURLScheme.restype = ctypes.c_int
    LS.LSSetDefaultHandlerForURLScheme.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    LS.LSCopyDefaultHandlerForURLScheme.restype = ctypes.c_void_p
    LS.LSCopyDefaultHandlerForURLScheme.argtypes = [ctypes.c_void_p]
    names = {}

    def cf(s):
        if s not in names:
            names[s] = ax.CF.CFStringCreateWithCString(None, s.encode(), ax.UTF8)
        return names[s]

    def current():
        return (ax.string(LS.LSCopyDefaultHandlerForURLScheme(cf('https'))) or '').lower()

    result = {'bundle': a.bundle, 'before': current(), 'set': {}, 'pressed': None}
    if result['before'] != a.bundle.lower():
        for scheme in ('http', 'https'):
            result['set'][scheme] = LS.LSSetDefaultHandlerForURLScheme(cf(scheme), cf(a.bundle))
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline and current() != a.bundle.lower():
            pid = pid_of('com.apple.coreservices.uiagent')
            if pid:
                app = ax.AS.AXUIElementCreateApplication(pid)
                stack = list(ax.children(app))
                while stack:
                    el = stack.pop()
                    if ax.text(el, 'AXRole') == 'AXButton' and (ax.text(el, 'AXTitle') or '').startswith('Use'):
                        result['pressed'] = ax.text(el, 'AXTitle')
                        ax.AS.AXUIElementPerformAction.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
                        ax.AS.AXUIElementPerformAction(el, ax.name('AXPress'))
                        break
                    stack.extend(ax.children(el))
            time.sleep(1)
    result['after'] = current()
    print(json.dumps(result))
    return 0 if result['after'] == a.bundle.lower() else 1


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
            ('CFNumberGetTypeID', ctypes.c_ulong, []), ('CFNumberGetValue', ctypes.c_bool, [vp, ctypes.c_int, vp]),
            ('CFBooleanGetTypeID', ctypes.c_ulong, []), ('CFBooleanGetValue', ctypes.c_bool, [vp]),
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


def tab_bar(a):
    """Each window of the app as Accessibility shows it outside its web pages (never walked):
    role, subrole, title, description and value of every element, indented by depth, so a walk
    that finds a tab left open can say what the browser's tab bar looked like."""
    ax = AX()
    pid = pid_of(a.bundle)
    if not pid:
        print(json.dumps({'bundle': a.bundle, 'error': 'not running'}))
        return 1
    lines = []
    stack = [(w, 0) for w in reversed(ax.children(ax.AS.AXUIElementCreateApplication(pid)))]
    while stack and len(lines) < a.max:
        el, depth = stack.pop()
        role = ax.text(el, 'AXRole') or ''
        value = ax.raw(el, 'AXValue')
        shown = ax.string(value) if value else None
        if value and shown is None and ax.CF.CFGetTypeID(value) == ax.CF.CFNumberGetTypeID():
            n = ctypes.c_long()
            # kCFNumberSInt64Type
            if ax.CF.CFNumberGetValue(value, 4, ctypes.byref(n)):
                shown = n.value
        elif value and shown is None and ax.CF.CFGetTypeID(value) == ax.CF.CFBooleanGetTypeID():
            shown = bool(ax.CF.CFBooleanGetValue(value))
        row = {'d': depth, 'role': role, 'subrole': ax.text(el, 'AXSubrole'), 'title': ax.text(el, 'AXTitle'),
               'desc': ax.text(el, 'AXDescription'), 'value': shown}
        lines.append({k: v for k, v in row.items() if v not in (None, '')})
        if role != 'AXWebArea' and depth < a.depth:
            stack.extend((c, depth + 1) for c in reversed(ax.children(el)))
    print(json.dumps({'bundle': a.bundle, 'trusted': bool(ax.AS.AXIsProcessTrusted()), 'elements': lines}))
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
    s = sub.add_parser('serve-site')
    s.add_argument('--port', type=int, required=True)
    s.add_argument('--dir', required=True)
    d = sub.add_parser('default-browser')
    d.add_argument('--bundle', required=True)
    t = sub.add_parser('tab-bar')
    t.add_argument('--bundle', required=True)
    t.add_argument('--depth', type=int, default=12)
    t.add_argument('--max', type=int, default=600)
    a = p.parse_args()
    return {'serve': serve, 'measure': measure, 'serve-site': serve_site, 'default-browser': default_browser,
            'tab-bar': tab_bar}[a.mode](a)


if __name__ == '__main__':
    sys.exit(main())

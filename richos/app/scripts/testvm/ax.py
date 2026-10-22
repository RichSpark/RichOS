"""ax.py — the guest's accessibility tree, and the press that drives it.

Run INSIDE the guest, by ax.sh, as:

    <one AX_PARAMS_JSON line> + this file  |  python3 -

ax.py never parses arguments. ax.sh prepends one line, `AX_PARAMS_JSON = '{...}'`, built
by Python's own repr() of the JSON, and the whole text travels on stdin, so nothing here
is ever interpolated into a remote command line.

It prints ONE JSON OBJECT PER LINE. The first is `{"meta":true,...}`; every line after it
is a node, a `{"clicked":...}`, an `{"action":...}` or an `{"error":...}`. The host
renders them; the guest decides nothing about formatting. The records are the ones ax.js
printed, field for field, so every walk and the host renderer read them unchanged.

===========================================================================
WHY THE ACCESSIBILITY API AND NOT SYSTEM EVENTS — esc-20261021T122950Z-b8b7362a
===========================================================================
ax.js read every attribute as an Apple Event to System Events, and System Events
resolves the element again from the process root for each one (`processes whose unix
id is N`, then every level down). Measured in guests on 2026-10-21 with the app idle:

    one attribute through System Events     7-26 ms shallow, 15-120 ms at depth 7
    one attribute through the AX API        0.29-0.41 ms (98 nodes, 392 reads, 115-161 ms)

So a 20-node tree took 4.4 s on a calm guest and 16.0 s in the two minutes after boot,
and one read in most walks crossed the 20 s deadline at a random step while the app's
own main thread sat idle in its run loop (sampled at the hang: 71 of 88 samples idle,
9 answering accessibility). The read was slow, not the app. Here each attribute is one
AXUIElementCopyAttributeValue to the process that owns the element, which is what
System Events was doing underneath, without the specifier machinery around every call.

Geometry stays four real numbers: AXPosition and AXSize are AXValues unpacked into a
CGPoint and a CGSize, never text (the AppleScript "1024700" lesson in ax.js's history
and in the host renderer, which still refuses a fused token).

What still goes through System Events: a keystroke (`type`'s paste) and a click at a
point (`click --at`), one Apple Event each, both refused unless the target is frontmost.
"""
import ctypes
import json
import os
import re
import shutil
import subprocess
import time

# ---------------------------------------------------------------------------
# THE MATCHER AND THE SEARCH — pure, and TESTED on the host with no guest
# ---------------------------------------------------------------------------
# TEXT MATCHES TITLE **OR** DESCRIPTION. An aria-label on a web view's control surfaces
# as AXDescription and not as AXTitle, which is why the THIRD rewrite of the same press
# script was the one that finally found the theme buttons. Matching the title alone is
# the defect, not a simplification.


def ax_text_hit(P, hay, want):
    if not hay:
        return False
    return (want in hay) if P.get('contains') else (hay == want)


def ax_matches(P, n):
    """A rendered node against the criteria (the host-side matcher tests drive this)."""
    if P.get('id') and n.get('id') != P['id']:
        return False
    if P.get('role') and n.get('role') != P['role']:
        return False
    if P.get('sub') and n.get('sub') != P['sub']:
        return False
    if P.get('text') is not None:
        if not ax_text_hit(P, n.get('title'), P['text']) and not ax_text_hit(P, n.get('desc'), P['text']):
            return False
    if P.get('value') is not None:
        if not ax_text_hit(P, n.get('value'), P['value']):
            return False
    return True


def ax_search(roots, P, api):
    """Breadth-first, so controls beside a long transcript are reached before every message.
    Matching reads only the attributes actually requested."""
    queue = [{'el': e, 'd': 0, 'parent': None} for e in roots]
    head = count = 0
    hits, near, shallow = [], [], []
    truncated = stopped_early = False
    limit = 1 if P.get('first') else (P['nth'] + 1 if P.get('nth') is not None else None)
    # "window" and "menubar" choose the ROOTS (run() below); nothing inside them needs finding.
    scoped = not P.get('scope') or P['scope'] in ('window', 'menubar')
    while head < len(queue):
        if count >= P['max']:
            truncated = True
            break
        q = queue[head]
        head += 1
        count += 1
        if q['d'] <= 5 and len(shallow) < 64:
            shallow.append(q)
        if not scoped:
            if api['scope'](q['el'], P['scope']):
                # A composer is its containing group, including adjacent controls.
                root = api['scopeRoot'](q['el'], q['parent'], P['scope']) if api.get('scopeRoot') else q['el']
                queue = [{'el': root, 'd': 0, 'parent': None}]
                head = 0
                scoped = True
                continue
        elif P.get('mode') == 'tree' or api['matches'](q['el']):
            hits.append(q)
            if limit is not None and len(hits) >= limit:
                stopped_early = True
                break
        elif api.get('near') and len(near) < 3 and api['near'](q['el']):
            # Same traversal and caps. A hint never becomes an actionable match.
            near.append(q)
        if q['d'] < P['depth']:
            for kid in api['children'](q['el']):
                queue.append({'el': kid, 'd': q['d'] + 1, 'parent': q['el']})
        elif api['children'](q['el']):
            truncated = True
    return {'hits': hits, 'near': near, 'shallow': shallow, 'count': count, 'truncated': truncated,
            'scoped': scoped, 'exhaustive': head >= len(queue) and not truncated and not stopped_early}


# Elements whose missing AXDescription System Events printed as '' (see description() in run()).
MENU_ROLES = ('AXMenuBar', 'AXMenuBarItem', 'AXMenu', 'AXMenuItem')


def js_string(v):
    """String(v) as ax.js wrote it, so `--expect enabled=true` and value=1 compare the same."""
    if v is None:
        return 'null'
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, float) and v.is_integer():
        return str(int(v))
    return str(v)


def scalar_or(v, fallback):
    """Only JSON scalars leave the guest; anything else is the fallback."""
    if isinstance(v, (str, bool)):
        return v
    if isinstance(v, (int, float)) and v == v and v not in (float('inf'), float('-inf')):
        return v
    return fallback


# ---------------------------------------------------------------------------
# run() — the same decisions ax.js made, against an `ax` backend and a `host` (the guest
# around the app: its processes, clipboard and keyboard). Both are injected so the tests
# drive every branch with fakes.
# ---------------------------------------------------------------------------

def run(P, ax, host):
    def error(code, detail, extra=None):
        record = {'error': code, 'detail': str(detail)}
        record.update(extra or {})
        return json.dumps(record)

    # An attribute the element does not implement is the fallback. An app that did not ANSWER
    # (TimeoutError, kAXErrorCannotComplete) is not: a fallback there would prune the tree
    # silently and report it complete, so it ends the read (respond() below).
    def get(el, attr, fallback=None):
        try:
            v = ax.get(el, attr)
        except TimeoutError:
            raise
        except Exception:
            return fallback
        return fallback if v is None else v

    def text(el, attr):
        return js_string(scalar_or(get(el, attr), ''))

    def description(el):
        # System Events' "description", which the walks match on, measured element by element
        # against the raw AX answers in one guest (walk-8feee21b526b, 2026-10-21): AXDescription
        # when it answers ('' included); when it does not, the role description ("standard
        # window", "group", "close button") for the 8 window-tree elements, and '' for all 77
        # menu elements, with the same kAXErrorNoValue on both sides. Role is what separates
        # them, so the fallback skips the menu roles.
        try:
            v = ax.get(el, 'AXDescription')
        except TimeoutError:
            raise
        except Exception:
            v = None
        if v is None and get(el, 'AXRole', '') not in MENU_ROLES:
            v = get(el, 'AXRoleDescription', '')
        return js_string(scalar_or(v, ''))

    def coordinate(el, attr, index):
        v = get(el, attr)
        try:
            n = v[index]
        except (TypeError, IndexError, KeyError):
            return None
        return n if isinstance(n, (int, float)) and not isinstance(n, bool) and n == n else None

    pid = host.target_pid(P.get('app'), P.get('pid'))
    if not pid:
        return error('noprocess', 'target process is absent')
    P['pid'] = pid
    name = host.process_name(pid) or str(pid)
    app = ax.application(pid)
    # Diagnose the common blocking system dialog before entering the app's tree.
    if name != 'SecurityAgent':
        try:
            if host.security_agent_has_window(ax):
                return error('blocked', 'SecurityAgent has a keychain dialog; target=%s pid=%s' % (name, pid))
        except Exception:
            pass
    try:
        wins = ax.get(app, 'AXWindows')
    except TimeoutError:
        raise  # the app did not answer: respond() reports 'unresponsive', which a wait asks again
    except Exception as e:
        return error('blocked', e)
    wins = wins if isinstance(wins, list) else []
    roots = wins
    if P.get('window'):
        roots = [wins[P['window'] - 1]] if len(wins) >= P['window'] else []
    if P.get('windowTitle'):
        roots = [w for w in roots if text(w, 'AXTitle') == P['windowTitle']]
    # The app's menus (its own Quit among them) hang off the menu bar, which is not a window.
    # An AXPress on a menu item works with its menu closed, like any other press.
    if P.get('scope') == 'menubar':
        bar = get(app, 'AXMenuBar')
        roots = [bar] if bar is not None else []
        if not roots:
            return error('nowindow', 'target=%s pid=%s has no menu bar' % (name, pid))
    if not roots:
        return error('nowindow', 'target=%s pid=%s windows=%d' % (name, pid, len(wins)))

    def front():
        try:
            return host.frontmost_pid(ax) == pid
        except Exception:
            return False

    if P['mode'] == 'clickat':
        if not front():
            return error('notfrontmost', 'coordinate click would land in another process')
        try:
            host.click_at(P['atx'], P['aty'])
        except Exception as e:
            return error('clickfailed', e)
        return json.dumps({'clicked': True, 'at': True, 'x': P['atx'], 'y': P['aty']})

    def match(el, relax_role=False):
        if P.get('id') and js_string(scalar_or(get(el, 'AXDOMIdentifier'), '')) != P['id']:
            return False
        if not relax_role and P.get('role') and get(el, 'AXRole', '') != P['role']:
            return False
        if P.get('sub') and get(el, 'AXSubrole', '') != P['sub']:
            return False
        if P.get('text') is not None and not ax_text_hit(P, text(el, 'AXTitle'), P['text']) \
                and not ax_text_hit(P, description(el), P['text']):
            return False
        if P.get('value') is not None and not ax_text_hit(P, text(el, 'AXValue'), P['value']):
            return False
        return True

    def scope(el, which):
        role = get(el, 'AXRole', '')
        if which == 'dialog':
            return role == 'AXSheet' or (role == 'AXGroup' and get(el, 'AXSubrole', '') == 'AXApplicationDialog')
        if which == 'composer':
            return role == 'AXTextArea' or (role == 'AXGroup' and get(el, 'AXDOMIdentifier', '') == 'composer')
        if which == 'sidebar':
            return role == 'AXGroup' and (description(el) == 'Entities and threads'
                                          or get(el, 'AXDOMIdentifier', '') == 'rail')
        return False

    def children(el):
        kids = get(el, 'AXChildren', [])
        return kids if isinstance(kids, list) else []

    def scope_root(el, parent, which):
        return parent if which == 'composer' and get(el, 'AXRole', '') == 'AXTextArea' and parent else el

    def full(q):
        el = q['el']
        return {'d': q['d'], 'role': text(el, 'AXRole'), 'sub': text(el, 'AXSubrole'), 'title': text(el, 'AXTitle'),
                'desc': description(el), 'value': text(el, 'AXValue')[:203],
                'enabled': scalar_or(get(el, 'AXEnabled'), None), 'current': scalar_or(get(el, 'AXARIACurrent'), None),
                'selected': scalar_or(get(el, 'AXSelected'), None),
                'x': coordinate(el, 'AXPosition', 0), 'y': coordinate(el, 'AXPosition', 1),
                'w': coordinate(el, 'AXSize', 0), 'h': coordinate(el, 'AXSize', 1)}

    result = None

    def missing(detail):
        # A modal web dialog hides the underlying composer from the AX tree. Inspect the
        # focused element's ancestors only on a miss, never on every transcript node.
        focused = get(app, 'AXFocusedUIElement')
        for _ in range(10):
            if focused is None:
                break
            if get(focused, 'AXSubrole', '') == 'AXApplicationDialog' or get(focused, 'AXRole', '') == 'AXSheet':
                modal = get(focused, 'AXTitle', get(focused, 'AXDescription', 'dialog'))
                return error('blocked', 'target=%s modal=%s; %s' % (name, js_string(modal), detail))
            focused = get(focused, 'AXParent')
        # A background webview may have no AXFocusedUIElement. Its dialog is a shallow
        # ancestor of the visible controls already visited by the search.
        for q in result['shallow']:
            if get(q['el'], 'AXSubrole', '') == 'AXApplicationDialog' or (q['d'] <= 1 and get(q['el'], 'AXRole', '') == 'AXSheet'):
                return error('blocked', 'target=%s modal=%s; %s' % (name, js_string(get(q['el'], 'AXTitle', 'dialog')), detail))
        return error('notfound', detail)

    api = {'matches': match, 'near': lambda el: bool(P.get('role') and (P.get('id') or P.get('text')) and match(el, True)),
           'scope': scope, 'scopeRoot': scope_root, 'children': children}
    result = ax_search(roots, P, api)
    hits = result['hits']
    meta = json.dumps({'meta': True, 'app': name, 'pid': pid, 'windows': len(wins), 'nodes': result['count'],
                       'truncated': result['truncated'], 'mode': P['mode'], 'matches': len(hits),
                       'exhaustive': result['exhaustive']})
    if not result['scoped']:
        return meta + '\n' + missing('scope is absent: ' + str(P.get('scope')))
    if result['truncated']:
        return meta + '\n' + error('incomplete', 'node/depth cap reached; absence or uniqueness is not established')
    if P['mode'] == 'tree':
        return meta + ''.join('\n' + json.dumps(full(q)) for q in hits)
    if not hits:
        failure = json.loads(missing('nothing matched'))
        if result['near']:
            failure['near_matches'] = [{'role': text(q['el'], 'AXRole'), 'sub': text(q['el'], 'AXSubrole'),
                                        'title': text(q['el'], 'AXTitle'), 'desc': description(q['el']),
                                        'id': js_string(scalar_or(get(q['el'], 'AXDOMIdentifier'), ''))}
                                       for q in result['near']]
        return meta + '\n' + json.dumps(failure)
    if P['mode'] == 'find':
        if P.get('nth') is not None:
            hits = hits[P['nth']:P['nth'] + 1]
        if not hits:
            return meta + '\n' + error('notfound', 'requested match index is absent')
        # A find also reads AXHelp, where WebKit puts an element's `title`: the reason a
        # disabled action gives as its tooltip. Only the matched nodes pay for the read.
        out = []
        for q in hits:
            o = full(q)
            o['help'] = js_string(scalar_or(get(q['el'], 'AXHelp'), ''))
            out.append(json.dumps(o))
        return meta + '\n' + '\n'.join(out)
    if not P.get('first') and P.get('nth') is None and len(hits) != 1:
        return meta + '\n' + error('ambiguous', 'multiple matches; specify --first or --nth (zero based)')
    index = P.get('nth') or 0
    if index >= len(hits):
        return meta + '\n' + error('notfound', 'requested match index is absent')
    target = hits[index]
    el = target['el']
    press_sent = False

    def expectation():
        observed, satisfied = {}, True
        attrs = {'value': 'AXValue', 'enabled': 'AXEnabled', 'selected': 'AXSelected', 'current': 'AXARIACurrent',
                 'title': 'AXTitle', 'description': 'AXDescription'}
        for k, want in (P.get('expect') or {}).items():
            observed[k] = description(el) if k == 'description' else scalar_or(get(el, attrs[k]), None)
            if observed[k] is None or js_string(observed[k]) != want:
                satisfied = False
        return {'satisfied': satisfied, 'observed': observed}

    try:
        # A retried toggle must not undo a state already established by the first press.
        before = expectation() if P['mode'] == 'click' and P.get('expect') else None
        if before and before['satisfied']:
            return meta + '\n' + json.dumps({'action': 'click', 'verified': True, 'already_satisfied': True,
                                             'observed': before['observed']})
        if not get(el, 'AXEnabled', False):
            return meta + '\n' + error('disabled', 'matched element is disabled')
        if P['mode'] == 'click':
            actions = ax.actions(el)
            if 'AXPress' not in actions:
                return meta + '\n' + error('noaction', 'element has no AXPress; actions=' + ','.join(actions))
            pressed_node = ({'role': text(el, 'AXRole'), 'title': text(el, 'AXTitle'), 'id': P.get('id')}
                            if P.get('expect') else full(target))
            # Guest-clock receipts, used by reap-walk: the press is timed where it happens.
            pressed_at = host.now_ms()
            press_sent = True
            ax.perform(el, 'AXPress')
            returned_at = host.now_ms()
            if P.get('expect'):
                check = None
                for _ in range(20):
                    check = expectation()
                    if check['satisfied']:
                        return meta + '\n' + json.dumps({'clicked': True, 'verified': True, 'node': pressed_node,
                                                         'matches': len(hits), 'observed': check['observed'],
                                                         'pressed_at_ms': pressed_at, 'returned_at_ms': returned_at})
                    host.delay(0.05)
                return meta + '\n' + error('effect_unknown', 'press sent once; postcondition not observed; inspect before retrying',
                                           {'expected': P['expect'], 'observed': check['observed'], 'press_sent': True,
                                            'pressed_at_ms': pressed_at, 'returned_at_ms': returned_at})
            return meta + '\n' + json.dumps({'clicked': True, 'node': pressed_node, 'matches': len(hits),
                                             'pressed_at_ms': pressed_at, 'returned_at_ms': returned_at})
        # Focusing can rebuild WebKit's AX tree and invalidate the element. Pin the discovered
        # DOM identity before sending focus; rebind by it only on a failed read, never send
        # focus twice and never type into another match.
        focus_id = js_string(scalar_or(get(el, 'AXDOMIdentifier'), ''))
        ax.set(app, 'AXFrontmost', True)
        ax.set(el, 'AXFocused', True)
        if front() and not get(el, 'AXFocused', False) and focus_id:
            refreshed = ax_search(roots, {'mode': 'find', 'scope': P.get('scope'), 'max': P['max'], 'depth': P['depth'],
                                          'first': False, 'nth': None},
                                  {'matches': lambda c: js_string(scalar_or(get(c, 'AXDOMIdentifier'), '')) == focus_id,
                                   'scope': scope, 'scopeRoot': scope_root, 'children': children})
            if refreshed['exhaustive'] and len(refreshed['hits']) == 1:
                target = refreshed['hits'][0]
                el = target['el']
        if not front() or not get(el, 'AXFocused', False):
            return meta + '\n' + error('focusfailed', 'target did not receive focus; no text sent')
        if P['mode'] == 'type':
            # Paste literal text so macOS smart quotes cannot rewrite instructions. This runs
            # only in the owned guest. The text clipboard is kept and put back; a clipboard
            # holding anything but text (an image, a file) is refused before it is touched.
            saved = host.clipboard_text()
            if saved is None:
                return meta + '\n' + error('typefailed', 'guest clipboard is not text; left unchanged')
            try:
                host.set_clipboard(P.get('input', ''))
                if P.get('replace'):
                    host.keystroke('a', command=True)
                host.keystroke('v', command=True)
                # WebKit updates AXValue asynchronously after the keystroke returns.
                verified = False
                for _ in range(20):
                    value = text(el, 'AXValue')
                    verified = value == P.get('input', '') if P.get('replace') else P.get('input', '') in value
                    if verified:
                        break
                    host.delay(0.05)
                if not verified:
                    return meta + '\n' + error('typefailed', 'field value does not contain the requested text')
            finally:
                host.set_clipboard(saved)
        return meta + '\n' + json.dumps({'action': P['mode'], 'verified': True, 'node': full(target)})
    except Exception as e:
        return meta + '\n' + error('effect_unknown' if press_sent else P['mode'] + 'failed', e, {'press_sent': press_sent})


# ---------------------------------------------------------------------------
# THE AX BACKEND — ctypes over ApplicationServices and CoreFoundation, in the guest only
# ---------------------------------------------------------------------------
class AX:
    """One accessibility call per attribute, to the process that owns the element."""

    def __init__(self):
        vp, c_int, c_long = ctypes.c_void_p, ctypes.c_int, ctypes.c_long
        self.AS = AS = ctypes.cdll.LoadLibrary(
            '/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices')
        self.CF = CF = ctypes.cdll.LoadLibrary('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
        for fn, res, args in (
            ('CFStringCreateWithCString', vp, [vp, ctypes.c_char_p, ctypes.c_uint32]),
            ('CFGetTypeID', ctypes.c_ulong, [vp]),
            ('CFStringGetTypeID', ctypes.c_ulong, []),
            ('CFArrayGetTypeID', ctypes.c_ulong, []),
            ('CFBooleanGetTypeID', ctypes.c_ulong, []),
            ('CFNumberGetTypeID', ctypes.c_ulong, []),
            ('CFArrayGetCount', c_long, [vp]),
            ('CFArrayGetValueAtIndex', vp, [vp, c_long]),
            ('CFStringGetLength', c_long, [vp]),
            ('CFStringGetMaximumSizeForEncoding', c_long, [c_long, ctypes.c_uint32]),
            ('CFStringGetCString', ctypes.c_bool, [vp, ctypes.c_char_p, c_long, ctypes.c_uint32]),
            ('CFBooleanGetValue', ctypes.c_bool, [vp]),
            ('CFNumberIsFloatType', ctypes.c_bool, [vp]),
            ('CFNumberGetValue', ctypes.c_bool, [vp, c_int, vp]),
            ('CFRetain', vp, [vp]),
        ):
            f = getattr(CF, fn)
            f.restype, f.argtypes = res, args
        for fn, res, args in (
            ('AXUIElementCreateApplication', vp, [c_int]),
            ('AXUIElementCopyAttributeValue', c_int, [vp, vp, ctypes.POINTER(vp)]),
            ('AXUIElementSetAttributeValue', c_int, [vp, vp, vp]),
            ('AXUIElementCopyActionNames', c_int, [vp, ctypes.POINTER(vp)]),
            ('AXUIElementPerformAction', c_int, [vp, vp]),
            ('AXUIElementGetTypeID', ctypes.c_ulong, []),
            ('AXValueGetTypeID', ctypes.c_ulong, []),
            ('AXValueGetType', c_int, [vp]),
            ('AXValueGetValue', ctypes.c_bool, [vp, c_int, vp]),
        ):
            f = getattr(AS, fn)
            f.restype, f.argtypes = res, args
        self.vp = vp
        self.UTF8 = 0x08000100
        self.types = {'str': CF.CFStringGetTypeID(), 'array': CF.CFArrayGetTypeID(),
                      'bool': CF.CFBooleanGetTypeID(), 'number': CF.CFNumberGetTypeID(),
                      'element': AS.AXUIElementGetTypeID(), 'axvalue': AS.AXValueGetTypeID()}
        self._names = {}
        self.true = ctypes.c_void_p.in_dll(CF, 'kCFBooleanTrue')
        self.false = ctypes.c_void_p.in_dll(CF, 'kCFBooleanFalse')

    def _name(self, s):
        if s not in self._names:
            self._names[s] = self.CF.CFStringCreateWithCString(None, s.encode(), self.UTF8)
        return self._names[s]

    def _string(self, ref):
        n = self.CF.CFStringGetMaximumSizeForEncoding(self.CF.CFStringGetLength(ref), self.UTF8) + 1
        buf = ctypes.create_string_buffer(n)
        return buf.value.decode('utf-8', 'replace') if self.CF.CFStringGetCString(ref, buf, n, self.UTF8) else ''

    def _convert(self, ref):
        if not ref:
            return None
        t = self.CF.CFGetTypeID(ref)
        if t == self.types['str']:
            return self._string(ref)
        if t == self.types['bool']:
            return bool(self.CF.CFBooleanGetValue(ref))
        if t == self.types['number']:
            if self.CF.CFNumberIsFloatType(ref):
                out = ctypes.c_double()
                self.CF.CFNumberGetValue(ref, 13, ctypes.byref(out))  # kCFNumberDoubleType
                v = out.value
                return int(v) if v.is_integer() else v
            out = ctypes.c_longlong()
            self.CF.CFNumberGetValue(ref, 11, ctypes.byref(out))  # kCFNumberLongLongType
            return out.value
        if t == self.types['array']:
            return [self._convert(self.CF.CFRetain(self.CF.CFArrayGetValueAtIndex(ref, i)))
                    for i in range(self.CF.CFArrayGetCount(ref))]
        if t == self.types['element']:
            return Element(ref)
        if t == self.types['axvalue']:
            kind = self.AS.AXValueGetType(ref)
            if kind in (1, 2):  # kAXValueCGPointType, kAXValueCGSizeType: two doubles
                pair = (ctypes.c_double * 2)()
                if self.AS.AXValueGetValue(ref, kind, ctypes.byref(pair)):
                    return [int(v) if float(v).is_integer() else v for v in pair]
            return None
        return None  # an attributed string or another type: not a JSON scalar, as in ax.js

    def application(self, pid):
        return Element(self.AS.AXUIElementCreateApplication(pid))

    def get(self, el, attr):
        out = self.vp()
        err = self.AS.AXUIElementCopyAttributeValue(el.ref, self._name(attr), ctypes.byref(out))
        if err == -25204:  # kAXErrorCannotComplete: the owning process did not answer in time
            raise TimeoutError('%s did not answer for %s (kAXErrorCannotComplete)' % (el, attr))
        return self._convert(out.value) if err == 0 else None

    def set(self, el, attr, value):
        ref = self.true if value is True else self.false if value is False else None
        if ref is None:
            raise ValueError('only booleans are set')
        err = self.AS.AXUIElementSetAttributeValue(el.ref, self._name(attr), ref)
        if err:
            raise RuntimeError('setting %s failed (AXError %d)' % (attr, err))

    def actions(self, el):
        out = self.vp()
        if self.AS.AXUIElementCopyActionNames(el.ref, ctypes.byref(out)) or not out.value:
            return []
        names = self._convert(out.value)
        return [n for n in names if isinstance(n, str)] if isinstance(names, list) else []

    def perform(self, el, action):
        err = self.AS.AXUIElementPerformAction(el.ref, self._name(action))
        if err:
            raise RuntimeError('%s failed (AXError %d)' % (action, err))

class Element:
    __slots__ = ('ref',)

    def __init__(self, ref):
        self.ref = ref

    def __repr__(self):
        return 'AXUIElement(0x%x)' % (self.ref or 0)


class GuestHost:
    """The guest around the app: processes, the frontmost app, the clipboard and the keyboard."""

    def target_pid(self, app_name, pid):
        if app_name:
            found = subprocess.run(['pgrep', '-o', '-x', app_name], capture_output=True, text=True).stdout.split()
            return int(found[0]) if found else None
        try:
            os.kill(int(pid), 0)
            return int(pid)
        except (OSError, TypeError, ValueError):
            return None

    def process_name(self, pid):
        out = subprocess.run(['ps', '-p', str(pid), '-o', 'comm='], capture_output=True, text=True).stdout.strip()
        return os.path.basename(out)

    def security_agent_has_window(self, ax):
        found = subprocess.run(['pgrep', '-x', 'SecurityAgent'], capture_output=True, text=True).stdout.split()
        return any(ax.get(ax.application(int(p)), 'AXWindows') for p in found)

    def frontmost_pid(self, ax):
        """The front application as WindowServer orders them (`lsappinfo front`): where a
        keystroke or a click at a point lands. Not the system-wide element's AXFocusedApplication,
        which, read from the SSH session, answered kAXErrorCannotComplete on 60 of 60 reads in
        5.3 s while lsappinfo and System Events both named the app (walk-efb859fa9378), so every
        focus and type refused focusfailed (walk-db710c72559c)."""
        asn = subprocess.run(['lsappinfo', 'front'], capture_output=True, text=True, timeout=5).stdout.strip()
        if not asn:
            return None
        info = subprocess.run(['lsappinfo', 'info', '-only', 'pid', asn], capture_output=True, text=True,
                              timeout=5).stdout
        found = re.search(r'"pid"\s*=\s*(\d+)', info)
        return int(found.group(1)) if found else None

    def _osascript(self, script):
        r = subprocess.run(['osascript', '-e', script], capture_output=True, text=True)
        if r.returncode:
            raise RuntimeError(r.stderr.strip() or 'osascript failed')
        return r.stdout

    def click_at(self, x, y):
        self._osascript('tell application "System Events" to click at {%d, %d}' % (int(x), int(y)))

    def keystroke(self, key, command=False):
        self._osascript('tell application "System Events" to keystroke "%s"%s'
                        % (key, ' using command down' if command else ''))

    _env = dict(os.environ, LANG='en_US.UTF-8', LC_ALL='en_US.UTF-8')

    def clipboard_text(self):
        """The clipboard's text, '' when it holds nothing (a fresh guest), None when it holds
        something that is not text and must not be overwritten."""
        info = self._osascript('clipboard info').strip()
        if not info:
            return ''
        if not any(t in info for t in ('utf8', 'ut16', 'string', 'TEXT', 'Unicode text')):
            return None
        return subprocess.run(['pbpaste'], capture_output=True, text=True, env=self._env).stdout

    def set_clipboard(self, value):
        subprocess.run(['pbcopy'], input=value, text=True, env=self._env, check=True)

    def delay(self, seconds):
        time.sleep(seconds)

    def now_ms(self):
        return int(time.time() * 1000)


def respond(P, ax, host):
    """run(), except that an app which stopped answering is a failure with its reason. A press
    already sent is effect_unknown inside run(); this covers the reads around it."""
    try:
        return run(P, ax, host)
    except TimeoutError as e:
        return json.dumps({'error': 'unresponsive', 'detail': '%s; the read is incomplete, so nothing is '
                           'reported as found or absent' % e})


if __name__ == '__main__' and 'AX_PARAMS_JSON' in globals():
    print(respond(json.loads(AX_PARAMS_JSON), AX(), GuestHost()))  # noqa: F821 - prepended by ax.sh

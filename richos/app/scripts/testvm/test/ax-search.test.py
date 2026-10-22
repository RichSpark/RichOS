#!/usr/bin/env python3
"""ax.py's decisions, driven on the host with no guest: ax_search, ax_matches and run().

run() takes an `ax` backend (the accessibility API) and a `host` (the guest's processes,
clipboard and keyboard); both are fakes here, so every branch a walk depends on is
exercised in milliseconds. The cases are the ones ax-search.test.js drove against ax.js's
System Events boundary, ported one for one, plus the AX backend's own CoreFoundation
conversions (real CF objects, no accessibility grant, no app touched).
"""
import ctypes
import importlib.util
import json
from pathlib import Path
import sys
import time
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('ax', HERE / 'ax.py')
ax = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ax)


# ---------------------------------------------------------------------------
# ax_search, pure: the traversal and its caps
# ---------------------------------------------------------------------------
def n(ident, kids=()):
    return {'id': ident, 'kids': list(kids)}


class Search(unittest.TestCase):
    def setUp(self):
        self.transcript = n('history', [n('old%d' % i) for i in range(4000)])
        self.composer = n('form', [n('Message'), n('Send')])
        self.root = n('window', [self.transcript, self.composer, n('Send')])
        self.examined = []

        def matches(e):
            self.examined.append(e['id'])
            return e['id'] == 'Send'
        self.api = {'children': lambda e: e['kids'], 'scope': lambda e, s: e['id'] == s, 'matches': matches}
        self.p = {'mode': 'find', 'max': 5000, 'depth': 16, 'first': True, 'nth': None}

    def search(self, roots=None, **extra):
        return ax.ax_search(roots or [self.root], dict(self.p, **extra), self.api)

    def test_first_match_precedes_thousands_of_history_children(self):
        self.assertEqual(self.search()['hits'][0]['el']['id'], 'Send')
        self.assertLess(len(self.examined), 6)

    def test_duplicates_nth_scope_and_caps(self):
        every = self.search(first=False)
        self.assertEqual(len(every['hits']), 2)
        self.assertTrue(every['exhaustive'])
        self.assertEqual(len(self.search(first=False, nth=1)['hits']), 2)
        scoped = self.search(scope='form')
        self.assertEqual(len(scoped['hits']), 1)
        self.assertIs(scoped['hits'][0]['parent'], self.composer)
        self.assertTrue(self.search(max=1)['truncated'])
        self.assertFalse(self.search(scope='missing')['scoped'])
        self.assertFalse(self.search([n('Send', [n('unvisited')])])['exhaustive'])
        self.assertFalse(self.search(first=False, depth=1)['exhaustive'])


# ---------------------------------------------------------------------------
# ax_matches: text matches AXTitle OR AXDescription (axpress3's lesson)
# ---------------------------------------------------------------------------
class Matcher(unittest.TestCase):
    def test_the_eight_matcher_cases(self):
        node = lambda **o: dict({'d': 0, 'role': 'AXButton', 'sub': '', 'title': '', 'desc': '', 'value': '',
                                 'enabled': True}, **o)
        P = lambda **o: dict({'role': None, 'sub': None, 'text': None, 'value': None, 'contains': False}, **o)
        self.assertTrue(ax.ax_matches(P(text='Dark'), node(title='Dark')), 'title')
        self.assertTrue(ax.ax_matches(P(text='Dark'), node(desc='Dark')), 'description')
        self.assertFalse(ax.ax_matches(P(text='Dark'), node(title='Light')), 'neither')
        self.assertFalse(ax.ax_matches(P(text='Dark'), node(desc='Dark theme')), 'exact by default')
        self.assertTrue(ax.ax_matches(P(text='Dark', contains=True), node(desc='Dark theme')), 'contains')
        self.assertFalse(ax.ax_matches(P(text='Dark', role='AXGroup'), node(desc='Dark')), 'role filters')
        self.assertTrue(ax.ax_matches(P(value='42'), node(value='42')), 'value')
        self.assertTrue(ax.ax_matches(P(), node()), 'no criteria')


# ---------------------------------------------------------------------------
# run(): a fake accessibility API and a fake guest
# ---------------------------------------------------------------------------
class Stale(Exception):
    """An element WebKit rebuilt: every read of it raises, as AX does for a dead element."""


class El:
    def __init__(self, role, title='', desc=None, kids=(), **attrs):
        # Every AX element has a role description; a missing AXDescription may fall back to it.
        self.attrs = dict({'AXRoleDescription': role[2:].lower()}, **attrs, AXRole=role, AXTitle=title)
        if desc is not None:
            self.attrs['AXDescription'] = desc
        self.kids = kids if callable(kids) else list(kids)
        self.actions = ['AXPress']
        self.stale = False


class FakeAX:
    def __init__(self, world):
        self.w = world

    def application(self, pid):
        return self.w.app

    def get(self, el, attr):
        if el.stale:
            raise Stale('invalid element')
        if attr == 'AXChildren':
            return el.kids() if callable(el.kids) else list(el.kids)
        v = el.attrs.get(attr)
        return v() if callable(v) else v

    def set(self, el, attr, value):
        if attr == 'AXFrontmost':
            self.w.front = value
        elif attr == 'AXFocused':
            self.w.focused = not self.w.x.get('rejectFocus') and value
        else:
            raise AssertionError('unexpected set ' + attr)

    def actions(self, el):
        return el.actions

    def perform(self, el, action):
        assert action == 'AXPress'
        self.w.press(el)


class FakeHost:
    def __init__(self, world):
        self.w = world

    def target_pid(self, app, pid):
        return 71

    def process_name(self, pid):
        return 'richos-tauri'

    def security_agent_has_window(self, ax_backend):
        return bool(self.w.x.get('blocked'))

    def frontmost_pid(self, ax_backend):
        return 71 if self.w.front else 99

    def click_at(self, x, y):
        self.w.clicked = 'click at {%d, %d}' % (x, y)

    def keystroke(self, key, command=False):
        w = self.w
        if command and key == 'a':
            w.value = ''
        elif command and key == 'v':
            if w.x.get('delayedValue'):
                w.pending = w.clipboard
            elif not w.x.get('dropInput'):
                w.value += w.clipboard
            w.typed += 1
        else:
            raise AssertionError('literal input must use paste')

    def clipboard_text(self):
        return self.w.x.get('clipboardHolds', self.w.clipboard)

    def set_clipboard(self, value):
        self.w.clipboard = value

    def delay(self, seconds):
        w = self.w
        if w.pending is not None:
            w.value = w.pending if w.x.get('delayedPress') else w.value + w.pending
            w.pending = None

    def now_ms(self):
        return int(time.time() * 1000)


class World:
    """The window ax-search.test.js built: one text area, optionally inside a modal or doubled."""

    def __init__(self, x):
        self.x = x
        self.value = x.get('initialValue', '')
        self.focused = False
        self.front = True
        self.pressed = self.typed = 0
        self.clicked = ''
        self.pending = None
        self.clipboard = 'original clipboard'
        self.area = self.text_area('route-choice')
        window_kids = [self.area]
        if x.get('dialog'):
            window_kids = [El('AXGroup', 'Modal', AXSubrole='AXApplicationDialog', kids=[self.area])]
        elif x.get('duplicate'):
            window_kids = [self.area, self.area]
        if x.get('namedAlias'):
            # Two distinct controls with the same label: each is its own element, never an alias.
            window_kids = [self.area, self.text_area('other-control')]
        if x.get('staleAfterFocus'):
            # Focusing rebuilds WebKit's tree: the old element dies, a fresh one takes its place.
            fresh = self.text_area('route-choice')
            fresh.attrs['AXFocused'] = lambda: self.focused
            self.area.attrs['AXFocused'] = lambda: (_ for _ in ()).throw(Stale('invalid index')) if self.focused else False
            window_kids = lambda: ([fresh, fresh] if x.get('duplicateAfterFocus') else [fresh]) if self.focused else [self.area]
        self.window = El('AXWindow', 'RichOS', kids=window_kids)
        self.app = El('AXApplication', 'RichOS', AXWindows=[self.window])

    def text_area(self, dom_id):
        x = self.x
        def value():
            return object() if x.get('toxicValue') else self.value
        return El(x.get('nodeRole', 'AXTextArea'), 'Message', desc='', AXValue=value, AXEnabled=True,
                  AXFocused=lambda: self.focused, AXPosition=[10, 20], AXSize=[100, 30],
                  AXDOMIdentifier=None if x.get('noID') else dom_id)

    def press(self, el):
        self.pressed += 1
        if 'pressValue' in self.x:
            if self.x.get('delayedPress'):
                self.pending = self.x['pressValue']
            else:
                self.value = self.x['pressValue']
        if self.x.get('throwAfterPress'):
            raise RuntimeError('reply lost after press')


def exercise(mode, **extra):
    world = World(extra)
    P = dict({'mode': mode, 'pid': 71, 'app': None, 'text': 'Message', 'value': None, 'first': True, 'nth': None,
              'max': 100, 'depth': 16, 'input': 'some text', 'replace': True, 'atx': 12, 'aty': 34,
              'role': None, 'sub': None, 'id': None, 'scope': None, 'window': None, 'windowTitle': None,
              'contains': False, 'expect': None},
             **{k: v for k, v in extra.items() if k in (
                 'text', 'value', 'first', 'nth', 'max', 'depth', 'input', 'replace', 'role', 'id', 'scope', 'expect')})
    records = [json.loads(line) for line in ax.run(P, FakeAX(world), FakeHost(world)).split('\n')]
    return records, world


class Run(unittest.TestCase):
    def test_focus_type_click_clickat_and_refusals(self):
        self.assertTrue(exercise('focus')[0][-1]['verified'])
        self.assertEqual(exercise('type')[1].value, 'some text')
        records, world = exercise('type', delayedValue=True)
        self.assertTrue(records[-1]['verified'])
        self.assertEqual(world.typed, 1)
        self.assertEqual(exercise('type', dropInput=True)[0][-1]['error'], 'typefailed')
        records, world = exercise('type', rejectFocus=True)
        self.assertEqual(records[-1]['error'], 'focusfailed')
        self.assertEqual(world.typed, 0)
        self.assertEqual(exercise('click')[1].pressed, 1)
        self.assertEqual(exercise('clickat')[1].clicked, 'click at {12, 34}')
        self.assertEqual(exercise('find', blocked=True)[0][-1]['error'], 'blocked')
        self.assertEqual(exercise('find', text='Absent', dialog=True)[0][-1]['error'], 'blocked')
        self.assertEqual(exercise('find', text='Absent')[0][-1]['error'], 'notfound')

    def test_an_unanswered_window_read_is_unresponsive_and_a_real_block_stays_blocked(self):
        def answer(failure):
            world = World({})
            class Fake(FakeAX):
                def get(self, el, attr):
                    if attr == 'AXWindows':
                        raise failure
                    return super().get(el, attr)
            P = {'mode': 'find', 'pid': 71, 'app': None, 'text': 'Open', 'value': None, 'first': True, 'nth': None,
                 'max': 100, 'depth': 16, 'role': None, 'sub': None, 'id': None, 'scope': None, 'window': None,
                 'windowTitle': None, 'contains': False, 'expect': None}
            return json.loads(ax.respond(P, Fake(world), FakeHost(world)).split('\n')[-1])['error']
        self.assertEqual(answer(TimeoutError('x did not answer for AXWindows (kAXErrorCannotComplete)')),
                         'unresponsive')
        self.assertEqual(answer(RuntimeError('a modal dialog blocks the read')), 'blocked')

    def test_the_press_is_timed_on_the_guest_clock(self):
        # A caller that reads the clock before ax.sh measures the SSH trip and the tree search
        # too (2.6-5.6 s, guest walk-0ddfdbf8ff00).
        before = int(time.time() * 1000)
        clicked = exercise('click')[0][-1]
        after = int(time.time() * 1000)
        self.assertTrue(isinstance(clicked['pressed_at_ms'], int) and before <= clicked['pressed_at_ms'] <= after, clicked)
        self.assertTrue(clicked['pressed_at_ms'] <= clicked['returned_at_ms'] <= after, clicked)

    def test_geometry_is_four_numbers(self):
        node = exercise('find')[0][-1]
        self.assertEqual((node['x'], node['y'], node['w'], node['h']), (10, 20, 100, 30))

    def test_literal_paste_preserves_text_and_restores_the_clipboard(self):
        records, world = exercise('type', input='Keep "quotes", `ticks`, $money and\nnewlines exactly.')
        self.assertTrue(records[-1]['verified'])
        self.assertEqual(world.clipboard, 'original clipboard')
        self.assertEqual(exercise('type', dropInput=True)[1].clipboard, 'original clipboard')
        # A clipboard holding something that is not text is refused before it is touched.
        records, world = exercise('type', clipboardHolds=None)
        self.assertEqual(records[-1]['error'], 'typefailed')
        self.assertEqual((world.typed, world.clipboard), (0, 'original clipboard'))
        # A fresh guest's clipboard is empty text: safe to use and to restore.
        records, world = exercise('type', clipboardHolds='')
        self.assertTrue(records[-1]['verified'])
        self.assertEqual(world.clipboard, '')

    def test_a_value_that_is_not_a_json_scalar_reads_as_empty(self):
        self.assertEqual(exercise('find', toxicValue=True)[0][-1]['value'], '')
        self.assertEqual(exercise('find', toxicValue=True, value='expected')[0][-1]['error'], 'notfound')

    def test_ids_role_hints_ambiguity_and_verified_clicks(self):
        self.assertEqual(exercise('find', id='route-choice')[0][-1]['role'], 'AXTextArea')
        self.assertEqual(exercise('find', id='wrong')[0][-1]['error'], 'notfound')
        records, world = exercise('click', role='AXCheckBox')
        self.assertEqual(world.pressed, 0)
        self.assertEqual(records[-1]['near_matches'][0]['role'], 'AXTextArea')
        self.assertEqual(exercise('click', id='route-choice', duplicate=True, first=False)[0][-1]['error'], 'ambiguous')
        records, world = exercise('click', id='route-choice', expect={'value': '1'}, initialValue='0',
                                  pressValue='1', delayedPress=True)
        self.assertTrue(records[-1]['verified'])
        self.assertEqual(world.pressed, 1)
        records, world = exercise('click', expect={'value': '1'}, initialValue='1')
        self.assertTrue(records[-1]['already_satisfied'])
        self.assertEqual(world.pressed, 0)
        records, world = exercise('click', expect={'value': '1'})
        self.assertEqual(records[-1]['error'], 'effect_unknown')
        self.assertEqual(world.pressed, 1)
        records, world = exercise('click', expect={'value': '1'}, pressValue='1', throwAfterPress=True)
        self.assertEqual(records[-1]['error'], 'effect_unknown')
        self.assertEqual(world.pressed, 1)
        self.assertEqual(exercise('click', expect={'value': '1'}, toxicValue=True)[0][-1]['error'], 'effect_unknown')
        self.assertEqual(exercise('click', id='route-choice', first=False, max=1)[0][-1]['error'], 'incomplete')

    def test_controls_with_the_same_label_stay_distinct(self):
        self.assertEqual(exercise('find', id='other-control', namedAlias=True)[0][-1]['role'], 'AXTextArea')
        records, world = exercise('click', id='other-control', namedAlias=True, first=False)
        self.assertEqual(world.pressed, 1, records)

    def test_focus_rebinds_a_stale_element_only_by_unique_dom_id(self):
        records, world = exercise('type', staleAfterFocus=True)
        self.assertTrue(records[-1]['verified'], records)
        self.assertEqual(world.typed, 1)
        records, world = exercise('type', staleAfterFocus=True, duplicateAfterFocus=True)
        self.assertEqual(records[-1]['error'], 'focusfailed')
        self.assertEqual(world.typed, 0)
        self.assertEqual(exercise('type', staleAfterFocus=True, noID=True)[1].typed, 0)
        self.assertEqual(exercise('type', staleAfterFocus=True, duplicateAfterFocus=True, max=2)[1].typed, 0)

    def test_an_app_that_stops_answering_is_a_failure_never_a_pruned_tree(self):
        # kAXErrorCannotComplete arrives after the AX messaging timeout, which can fall inside the
        # 20 s deadline: a fallback there would print a tree with a branch missing as complete.
        world = World({})
        P = {'mode': 'tree', 'pid': 71, 'text': None, 'value': None, 'first': False, 'nth': None,
             'max': 100, 'depth': 16, 'contains': False}
        stalled = FakeAX(world)
        real_get = stalled.get
        def get(el, attr):
            if el is world.window and attr == 'AXChildren':
                raise TimeoutError('AXUIElement did not answer for AXChildren (kAXErrorCannotComplete)')
            return real_get(el, attr)
        stalled.get = get
        records = [json.loads(l) for l in ax.respond(P, stalled, FakeHost(world)).split('\n')]
        self.assertEqual(len(records), 1, records)
        self.assertEqual(records[0]['error'], 'unresponsive')
        self.assertIn('kAXErrorCannotComplete', records[0]['detail'])
        # A stall after the press was sent is effect_unknown: the press may have landed.
        world = World({})
        P = dict(P, mode='click', text='Message', first=True, expect={'value': '1'})
        stalled = FakeAX(world)
        real_get2 = stalled.get
        def after_press(el, attr):
            if world.pressed and attr == 'AXValue':
                raise TimeoutError('did not answer')
            return real_get2(el, attr)
        stalled.get = after_press
        last = json.loads(ax.respond(P, stalled, FakeHost(world)).split('\n')[-1])
        self.assertEqual((last['error'], last['press_sent'], world.pressed), ('effect_unknown', True, 1))

    def test_description_falls_back_to_the_role_description(self):
        # System Events' `description` was AXRoleDescription when an element had no
        # AXDescription; the walks match on what it printed.
        world = World({})
        world.area.attrs.pop('AXDescription')
        world.area.attrs['AXRoleDescription'] = 'text entry area'
        P = {'mode': 'find', 'pid': 71, 'text': 'text entry area', 'value': None, 'first': True, 'nth': None,
             'max': 100, 'depth': 16, 'contains': False}
        node = json.loads(ax.run(P, FakeAX(world), FakeHost(world)).split('\n')[-1])
        self.assertEqual(node['desc'], 'text entry area')
        # ...but a menu element with no AXDescription printed '' (walk-8feee21b526b: all 77).
        records, world = MenuBar().exercise(scope='menubar', mode='tree', text=None, role=None, first=False)
        menu = [r for r in records if r.get('role', '').startswith('AXMenu')]
        self.assertEqual(len(menu), 5, records)
        self.assertEqual({r['desc'] for r in menu}, {''})


class MenuBar(unittest.TestCase):
    """The app's own Quit is a MENU BAR item (main.rs builds "Quit {name}" under MENU_QUIT), and
    the menu bar is not one of the app's windows. `--in menubar` searches AXMenuBar."""

    def exercise(self, **extra):
        world = World({})
        world.pressed = 0
        quit_item = El('AXMenuItem', 'Quit RichOS', AXEnabled=True, AXPosition=[0, 0], AXSize=[10, 10])
        bar = El('AXMenuBar', '', kids=[El('AXMenuBarItem', 'RichOS', kids=[
            El('AXMenu', '', kids=[El('AXMenuItem', 'About RichOS', AXEnabled=True), quit_item])])])
        world.window = El('AXWindow', 'RichOS', kids=[El('AXButton', 'Send', AXEnabled=True)])
        world.app = El('AXApplication', 'RichOS', AXWindows=[world.window], AXMenuBar=bar)
        P = dict({'mode': 'click', 'pid': 71, 'text': 'Quit RichOS', 'role': 'AXMenuItem', 'value': None,
                  'first': True, 'nth': None, 'max': 100, 'depth': 16}, **extra)
        records = [json.loads(l) for l in ax.run(P, FakeAX(world), FakeHost(world)).split('\n')]
        return records, world

    def test_a_menu_item_is_in_no_window_and_menubar_finds_it(self):
        records, world = self.exercise()
        self.assertEqual(world.pressed, 0)
        self.assertEqual(records[-1]['error'], 'notfound')
        records, world = self.exercise(scope='menubar')
        self.assertEqual(world.pressed, 1, records)
        self.assertEqual(records[-1]['node']['title'], 'Quit RichOS')
        self.assertIsInstance(records[-1]['pressed_at_ms'], int)
        self.assertGreaterEqual(records[-1]['returned_at_ms'], records[-1]['pressed_at_ms'])


# ---------------------------------------------------------------------------
# GuestHost.frontmost_pid: WindowServer's front app, never the system-wide AX element
# ---------------------------------------------------------------------------
class Frontmost(unittest.TestCase):
    """walk-efb859fa9378: from the SSH session the system-wide element's AXFocusedApplication
    answered kAXErrorCannotComplete on 60 of 60 reads while `lsappinfo` named the app, so every
    focus and type refused focusfailed (walk-db710c72559c). `lsappinfo` is a stub on PATH here,
    printing what the guest printed."""

    class SystemWideNeverAnswers:
        def get(self, el, attr):
            raise TimeoutError('did not answer for AXFocusedApplication (kAXErrorCannotComplete)')

        def system_wide(self):
            return object()

        def pid_of(self, el):
            raise AssertionError('no pid without an element')

    def frontmost(self, front_out, info_out):
        import os, tempfile
        with tempfile.TemporaryDirectory(prefix='ax-front-') as d:
            stub = Path(d) / 'lsappinfo'
            stub.write_text('#!/bin/sh\ncase "$1" in front) printf %%s %s ;; info) printf %%s %s ;; esac\n'
                            % (repr(front_out), repr(info_out)))
            stub.chmod(0o700)
            saved = os.environ['PATH']
            os.environ['PATH'] = d + os.pathsep + saved
            try:
                return ax.GuestHost().frontmost_pid(self.SystemWideNeverAnswers())
            finally:
                os.environ['PATH'] = saved

    def test_the_front_app_comes_from_lsappinfo(self):
        self.assertEqual(self.frontmost('ASN:0x0-0x2b02b:', '"pid"=1102'), 1102)

    def test_no_front_app_is_none_never_a_guess(self):
        self.assertIsNone(self.frontmost('', ''))
        self.assertIsNone(self.frontmost('ASN:0x0-0x2b02b:', 'no such application'))


# ---------------------------------------------------------------------------
# The AX backend's CoreFoundation conversions, on real CF objects (macOS only)
# ---------------------------------------------------------------------------
@unittest.skipUnless(sys.platform == 'darwin', 'CoreFoundation is macOS only')
class Backend(unittest.TestCase):
    def setUp(self):
        self.ax = ax.AX()
        CF, AS, vp = self.ax.CF, self.ax.AS, ctypes.c_void_p
        CF.CFNumberCreate.restype, CF.CFNumberCreate.argtypes = vp, [vp, ctypes.c_int, vp]
        CF.CFArrayCreate.restype, CF.CFArrayCreate.argtypes = vp, [vp, ctypes.POINTER(vp), ctypes.c_long, vp]
        AS.AXValueCreate.restype, AS.AXValueCreate.argtypes = vp, [ctypes.c_int, vp]

    def test_strings_numbers_booleans_and_arrays(self):
        CF = self.ax.CF
        self.assertEqual(self.ax._convert(self.ax._name('Dark théme "quoted"')), 'Dark théme "quoted"')
        i = ctypes.c_longlong(42)
        self.assertEqual(self.ax._convert(CF.CFNumberCreate(None, 11, ctypes.byref(i))), 42)
        f = ctypes.c_double(0.5)
        self.assertEqual(self.ax._convert(CF.CFNumberCreate(None, 13, ctypes.byref(f))), 0.5)
        self.assertIs(self.ax._convert(self.ax.true.value), True)
        self.assertIs(self.ax._convert(self.ax.false.value), False)
        items = (ctypes.c_void_p * 2)(self.ax._name('a'), self.ax._name('b'))
        # kCFTypeArrayCallBacks is a struct; CFArrayCreate takes its address.
        callbacks = ctypes.addressof(ctypes.c_void_p.in_dll(CF, 'kCFTypeArrayCallBacks'))
        array = CF.CFArrayCreate(None, items, 2, callbacks)
        self.assertEqual(self.ax._convert(array), ['a', 'b'])
        self.assertIsNone(self.ax._convert(None))

    def test_point_and_size_unpack_to_real_numbers_never_text(self):
        AS = self.ax.AS
        point = (ctypes.c_double * 2)(1024.0, 700.0)
        size = (ctypes.c_double * 2)(32.0, 32.5)
        self.assertEqual(self.ax._convert(AS.AXValueCreate(1, ctypes.byref(point))), [1024, 700])
        self.assertEqual(self.ax._convert(AS.AXValueCreate(2, ctypes.byref(size))), [32, 32.5])
        rect = (ctypes.c_double * 4)(0, 0, 1, 1)
        self.assertIsNone(self.ax._convert(AS.AXValueCreate(3, ctypes.byref(rect))), 'a CGRect is not read')


if __name__ == '__main__':
    unittest.main()

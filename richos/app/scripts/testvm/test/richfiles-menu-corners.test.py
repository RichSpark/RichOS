#!/usr/bin/env python3
"""richfiles-round107-walk.py --menu-corners: corner_verdict passes a menu File Explorer would
draw and fails one that left the display, moved away from the pointer with room to spare, or
opened away from the pointer in the middle of the display."""
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'round107', Path(__file__).resolve().parents[1] / 'richfiles-round107-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)
verdict = walk.corner_verdict
SCREEN = (1680, 1050)
WINDOW = {'x': 380, 'y': 60, 'w': 920, 'h': 420}
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def menu(x, y, w=240, h=400):
    return {'x': x, 'y': y, 'w': w, 'h': h}


ok, facts = verdict((1260, 440), menu(1262, 436), SCREEN, WINDOW)
check('a menu at the pointer past the window edge passes and says it crossed', ok and facts['past_window_edge'], facts)
ok, facts = verdict((1260, 440), menu(1020, 436), SCREEN, WINDOW)
check('a menu opened to the left of the pointer with room on the right fails in the window case', not ok, facts)
ok, facts = verdict((1640, 1000), menu(1400, 600), SCREEN)
check('at the bottom-right display corner a menu flipped up and left, wholly on the display, passes',
      ok and facts['opened_left'] and facts['opened_up'], facts)
ok, facts = verdict((1640, 1000), menu(1638, 996), SCREEN)
check('a menu running off the display at its corner fails', not ok and not facts['on_display'], facts)
ok, facts = verdict((40, 1000), menu(42, 600), SCREEN)
check('at the bottom-left display corner a menu flipped up at the pointer passes', ok, facts)
ok, facts = verdict((40, 1000), menu(400, 600), SCREEN)
check('a menu that fits but sits away from the pointer fails', not ok, facts)
ok, facts = verdict((40, 60), menu(300, 62), SCREEN)
check('at the top-left display corner a menu moved right although there was room fails', not ok, facts)
ok, facts = verdict((1640, 60), menu(1402, 58), SCREEN)
check('at the top-right display corner a menu moved left to fit, its top at the pointer, passes', ok, facts)

ok, facts = verdict((1640, 1000), menu(1360, 600), SCREEN)
check('a menu that fit but stopped short of both the pointer and the display edge fails', not ok, facts)
ok, facts = verdict((1640, 1000), menu(1436, 646), SCREEN)
check('a menu slid to the display corner with the pointer inside it passes', ok, facts)
target = walk.menu_target
check('the menu of a file, a folder and empty space is told apart, and an unknown one is None',
      target(['Open', 'Open With', 'Quick Look']) == 'file'
      and target(['Open', 'Open in New Tab', 'Open in New Window']) == 'folder'
      and target(['New Folder', 'New Text File']) == 'empty' and target(['Copy Path']) is None)

print('richfiles-menu-corners.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)

#!/usr/bin/env python3
"""The shared connect step, offline. The live proof is folders-walk.py in a guest.

Since round 27 the Settings dialog's "Connected folders (repositories)" row is a plain button
(settings-button.js buildDisclosureRow: "no `menuitem` role"). A walk that presses it as an
AXMenuItem waits out the guest's 20 s accessibility deadline and exits 124 (guest_deadline),
which reads like a busy host. Three walks did this on 2026-10-21. This test fails if any walk
names that row with the menu-item role again, or keeps its own copy of the connect step instead
of adopt-walk.py's connect_folder.
"""
from pathlib import Path
import re
import unittest

HERE = Path(__file__).resolve().parent.parent
WALKS = sorted(HERE.glob('*-walk.py'))
ROW_AS_MENU_ITEM = re.compile(r'Connected folders[^\n]*AXMenuItem|AXMenuItem[^\n]*Connected folders')


class ConnectFolder(unittest.TestCase):
    def test_the_walks_are_found(self):
        self.assertTrue({'adopt-walk.py', 'command-walk.py', 'folders-walk.py', 'reap-walk.py', 'review-walk.py'}
                        <= {p.name for p in WALKS})

    def test_no_walk_presses_the_row_as_a_menu_item(self):
        bad = [f'{p.name}:{n}: {line.strip()}' for p in WALKS
               for n, line in enumerate(p.read_text().splitlines(), 1) if ROW_AS_MENU_ITEM.search(line)]
        self.assertEqual(bad, [], 'the row is a button now; use Walk.connect_folder')

    def test_no_walk_keeps_its_own_connect_step(self):
        own = [p.name for p in WALKS if p.name != 'adopt-walk.py'
               and re.search(r"press\('Connected folders'", p.read_text())]
        self.assertEqual(own, [], 'a walk presses the row itself instead of calling connect_folder')

    def test_the_shared_step_presses_the_row_as_a_button(self):
        source = (HERE / 'adopt-walk.py').read_text()
        body = source[source.index('    def open_connected_folders'):source.index('    def decline_first_run_sheets')]
        self.assertIn("self.press('Connected folders')", body)
        self.assertNotIn("role='AXMenuItem'", body)

    def test_the_connecting_walks_call_it(self):
        for name in ('command-walk.py', 'reap-walk.py'):
            self.assertIn('self.connect_folder()', (HERE / name).read_text(), name)
        self.assertIn('self.open_connected_folders(', (HERE / 'folders-walk.py').read_text())


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""folder-map-walk.py — RichFinder's Folder Map in the packaged app: Settings > Search folders (three
modes, a ticked folder, coverage, Reindex, locked rows), "Rich in each company" limits that survive a
parent tick and All eligible, the Sources list beside RichFinder results, the RichFinder wording, and
a word-for-word match inside a reply Rich gave in several parts (round 57 B, CEO pick 2026-11-06).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      folder-map-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). Every driving method is
search-walk.py's (which is folders-walk.py's, which is adopt-walk.py's); the Folder Map's own controls
are addressed by their DOM ids and aria-labels (`ui/search-folder-settings.js`).
  identity        the running app says it was built from --expect-sha
  first-run       company "Acme" (adopt-walk.py's)
  second-company  company "Beta" (multico-walk.py's way: "+ New chat or company" > New company)
  fixtures        ~/Docs/Alpha, ~/Docs/Beta (each one synthetic note), ~/Docs/.stash (hidden) and
                  ~/Docs/node_modules (generated output), each holding a note
  modes           Settings > Search folders: Off, Chosen folders, All eligible; Alpha ticked; the
                  coverage count; the Settings row's state; Reindex; a hidden or generated-output folder
                  is a locked row with a disabled box; contrast of every text on the page, light and dark
  company-limit   Beta limited to Alpha; ticking the parent (Docs), then All eligible, keeps that limit
  finder-sources  RichFinder: the Sources list beside results; unchecking Alpha narrows one search; the
                  checkbox and status line say RichFinder, never Super Finder
  split-reply     a reply stored in two parts (two runs with a gap in their sequence) holds a word-for-word
                  match in its second part; the palette's hit opens the conversation at that part
  smoke           a message sent and answered; Settings and the update check open

Evidence is written to --out. Synthetic text only. Exit 0 when every step passes. Every app instance is
quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import re
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
QA = HERE.parent / 'qa'
STEPS = ['identity', 'first-run', 'second-company', 'fixtures', 'modes', 'company-limit',
         'finder-sources', 'split-reply', 'smoke']
QUERY = 'harborlight'
SPLIT_FIRST = 'Let me look at the club notes first. '
SPLIT_SECOND = 'The quartzmeadow refund window is thirty days for annual members.'
SPLIT_QUERY = 'quartzmeadow'


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build():
    search = load('search-walk')
    guest, relaunch, folders = search.host_modules()
    Base, StepFailed = search.build_walk(folders, guest, relaunch)
    command = folders.command

    class Walk(Base):
        def __init__(self, a):
            super().__init__(a)
            self.docs_root = self.home + '/Docs'
            self.beta = self.home + '/Beta'

        # --- small helpers -----------------------------------------------------------------
        def registry(self):
            text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/entities.json') + ' 2>/dev/null || true')
            try:
                return json.loads(text)
            except ValueError:
                return {}

        def settings_json(self):
            return guest(self.vm, 'cat ' + shlex.quote(self.data + '/settings.json') + ' 2>/dev/null || true')

        def click_id(self, dom_id, role=None, first=True):
            args = ['click', '--id', dom_id]
            if role:
                args += ['--role', role]
            if first:
                args.append('--first')
            self.ax(*args)
            time.sleep(1.5)

        def click_title(self, title, role='AXButton'):
            self.ax('click', '--title', title, '--role', role, '--contains', '--first')
            time.sleep(1.5)

        def present_id(self, dom_id):
            return bool(self.nodes('--id', dom_id))

        def wait_text(self, words, seconds=30):
            return self.says(words, seconds)

        def checkboxes(self):
            return [n for n in self.nodes('--role', 'AXCheckBox')]

        def radios(self):
            return [n for n in self.nodes('--role', 'AXRadioButton')]

        # --- steps --------------------------------------------------------------------------
        def second_company(self):
            guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Beta notes\\n" > README.md '
                           '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                           'commit -q -m init; fi'.format(c=shlex.quote(self.beta)))
            for attempt in range(2):
                try:
                    self.ax('click', '--id', 'rail-new-thread')
                    break
                except StepFailed:
                    if attempt:
                        raise
                    self.ax('click', '--id', 'rail-toggle')
                    time.sleep(2)
            time.sleep(1)
            for role in ('AXMenuItem', 'AXButton', 'AXStaticText'):
                if self.present('New company', role):
                    self.press('New company', role)
                    break
            else:
                raise StepFailed('"+ New chat or company" offers no "New company"')
            self.wait_for('Add this company', seconds=20)
            self.type_into('Beta', '--role', 'AXTextField', '--title', "What's the company called?")
            self.type_into(self.beta, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
            self.press('Add this company')
            end = time.monotonic() + 60
            while time.monotonic() < end:
                rows = {e['id']: e for e in self.registry().get('entities', [])}
                if 'beta' in rows:
                    time.sleep(3)
                    return {'entities': sorted(rows)}
                time.sleep(1)
            raise StepFailed('Beta was not registered within 60 s')

        def fixtures(self):
            d = shlex.quote(self.docs_root)
            guest(self.vm, f'mkdir -p {d}/Alpha {d}/Bravo {d}/.stash {d}/node_modules && '
                           f'printf "Alpha club notes. The harborlight walrus handbook for Alpha.\\n" > {d}/Alpha/alpha.md && '
                           f'printf "Beta club notes. The harborlight walrus handbook for Beta.\\n" > {d}/Bravo/beta.md && '
                           f'printf "Hidden stash harborlight secret.\\n" > {d}/.stash/stash.md && '
                           f'printf "Generated harborlight output.\\n" > {d}/node_modules/gen.md')
            listing = guest(self.vm, f'find {d} -type f | sort')
            return {'files': listing.splitlines()}

        # --- the Folder Map ------------------------------------------------------------------
        def open_map(self):
            self.key('key code 53')
            time.sleep(0.5)
            self.ax('click', '--id', 'set-btn', '--first')
            time.sleep(1.5)
            self.ax('click', '--id', 'set-search-folders-open', '--first')
            time.sleep(2.5)
            if not self.present_id('rf-folder-mode-off'):
                raise StepFailed('Settings > Search folders did not open the Folder Map')

        def close_map(self):
            self.ax('click', '--id', 'rf-folder-done', '--first')
            time.sleep(1)

        def mode_state(self):
            out = {}
            for n in self.nodes('--role', 'AXRadioButton'):
                out[n.get('id') or n.get('title') or n.get('description')] = n.get('value')
            return out

        def choose_mode(self, value):
            self.ax('click', '--id', 'rf-folder-mode-' + value, '--first')
            time.sleep(3)
            state = self.mode_state()
            return state

        def expand(self, name):
            if self.nodes('--title', 'Collapse ' + name, '--role', 'AXButton'):
                return
            self.ax('click', '--title', 'Expand ' + name, '--role', 'AXButton', '--first')
            time.sleep(2)

        def box(self, name):
            found = self.nodes('--title', 'Search ' + name, '--role', 'AXCheckBox')
            return found[0] if found else None

        def tick(self, name):
            before = self.box(name)
            self.ax('click', '--title', 'Search ' + name, '--role', 'AXCheckBox', '--first')
            time.sleep(3)
            return {'before': before and before.get('value'), 'after': (self.box(name) or {}).get('value')}

        SHEET = (355, 135, 1325, 910)  # the Folder Map's page on the guest's 1680x1050 frame

        def page_texts(self, words=None, titles=None, region=None):
            """Every static text and button label drawn on the Folder Map page (accessibility's boxes),
            kept only when the whole box lies inside the page: the app behind the scrim is not the page."""
            x0, y0, x1, y1 = region or self.SHEET
            rows, seen = [], set()
            # The page's own words (ui/search-folder-settings.js), each looked up by its text: the accessibility
            # tree also lists the app behind the scrim, whose words are not on this page.
            words = words or ['Search folders', 'Off', 'Chosen folders', 'All eligible', 'Tick what to search',
                     'Everything eligible is included', 'Tick a folder to start', 'Never searched ·',
                     'Document search', ' documents', 'searchable with RichFinder', 'skipped', 'indexing',
                     'System folders, app data', 'Rich in each company', 'All search folders', ' folder',
                     'A company limit only narrows', 'Nothing is searched until', 'Included by parent folder',
                     'Changes apply to the next search', 'Saved. Applies', 'Reindex started']
            for word in words:
                for n in self.nodes('--value', word, '--contains'):
                    key = (n['x'], n['y'], n['w'], n['h'])
                    if key in seen or n.get('w', 0) < 6 or n.get('h', 0) < 6:
                        continue
                    if n['x'] < x0 or n['y'] < y0 or n['x'] + n['w'] > x1 or n['y'] + n['h'] > y1:
                        continue
                    seen.add(key)
                    rows.append({'text': (n.get('value') or word).strip(), 'x': n['x'], 'y': n['y'], 'w': n['w'],
                                 'h': n['h'], 'role': 'AXStaticText'})
            for title in ('‹ Settings', 'Settings', 'Limit', 'Edit', 'Done', 'Reindex', 'Clear selection'):
                for n in self.nodes('--title', title, '--role', 'AXButton', '--contains'):
                    key = (n['x'], n['y'], n['w'], n['h'])
                    if key in seen or n['x'] < x0 or n['y'] < y0 or n['x'] + n['w'] > x1 or n['y'] + n['h'] > y1:
                        continue
                    seen.add(key)
                    rows.append({'text': title, 'x': n['x'], 'y': n['y'], 'w': n['w'], 'h': n['h'], 'role': 'AXButton'})
            return rows

        def contrast_of(self, frame, rows, theme):
            """contrast.py on each text's own box. A static text's box is as wide as its column, so the
            box is cut to the width its characters need (10.5 px each, the page's 15 px type, plus 12);
            a button is read whole. The lowest ratios and every one under 4.5 are kept."""
            results = []
            for r in rows:
                x, y, w, h = int(r['x']), int(r['y']), int(r['w']), int(r['h'])
                p = subprocess.run([sys.executable, str(QA / 'contrast.py'), str(self.out / frame), '--box',
                                    str(x), str(y), str(w), str(h), '--label', r['text'][:40], '--min-frac', '0.003'],
                                   capture_output=True, text=True, timeout=60)
                m = re.search(r'RATIO\s+([0-9.]+):1', p.stdout)
                if m:
                    results.append({'text': r['text'][:60], 'ratio': float(m.group(1)), 'box': [x, y, w, h]})
            low = sorted(results, key=lambda r: r['ratio'])[:8]
            return {'theme': theme, 'measured': len(results), 'lowest': low,
                    'under_4_5': [r for r in results if r['ratio'] < 4.5]}

        def modes(self):
            ev = {}
            self.open_map()
            ev['first_open_modes'] = self.mode_state()
            ev['settings_row_off'] = None
            self.shot('map-off-light.png')
            # Off
            ev['off'] = {'document_search_off_text': self.text_on_screen('Nothing is searched until you choose')}
            # Chosen
            ev['chosen_modes'] = self.choose_mode('chosen')
            self.expand('home')
            self.expand('Docs')
            ev['tree_rows'] = [n.get('title') or n.get('description') for n in self.checkboxes()]
            ev['alpha_tick'] = self.tick('Alpha')
            end = time.monotonic() + 240
            while time.monotonic() < end and (not self.text_on_screen('searchable with RichFinder')
                                              or self.text_on_screen('indexing')):
                time.sleep(2)
            ev['coverage_documents_text'] = self.text_on_screen(' documents')
            ev['coverage_says_richfinder'] = self.text_on_screen('searchable with RichFinder')
            ev['coverage_says_super_finder'] = self.text_on_screen('Super Finder')
            self.shot('map-chosen-light.png')
            # Read at once so the frame and its boxes are one layout. The first "1 indexing" line of the coverage card
            # can still be on the frame (the meaning model loads slowly in a fresh guest) and leave this frame's
            # pane boxes 32 px off; the settled frames (company_contrast, the finder's contrast) are the ones to trust.
            rows = self.page_texts()
            # locked rows
            locked = {}
            for name in ('.stash', 'node_modules'):
                b = self.box(name)
                locked[name] = {'listed': b is not None, 'enabled': (b or {}).get('enabled'),
                                'value': (b or {}).get('value')}
            locked['note_hidden'] = self.text_on_screen('Hidden folders')
            locked['note_generated'] = self.text_on_screen('Code dependencies or generated output')
            ev['locked_rows'] = locked
            # try to tick a locked row
            try:
                self.ax('click', '--title', 'Search .stash', '--role', 'AXCheckBox', '--first')
                time.sleep(2)
            except StepFailed as exc:
                ev['tick_hidden_error'] = str(exc)[:200]
            ev['hidden_after_click'] = (self.box('.stash') or {}).get('value')
            ev['settings_json_chosen'] = self.settings_json()[:600]
            # contrast, light then dark, on the open page
            ev['texts_on_page'] = len(rows)
            ev['contrast'] = [self.contrast_of('map-chosen-light.png', rows, 'light')]
            self.appearance(True)
            self.shot('map-chosen-dark.png')
            rows_dark = self.page_texts()
            ev['contrast'].append(self.contrast_of('map-chosen-dark.png', rows_dark, 'dark'))
            self.appearance(False)
            # Reindex
            self.ax('click', '--title', 'Reindex', '--role', 'AXButton', '--first')
            time.sleep(2)
            ev['reindex_feedback'] = self.text_on_screen('Reindex started')
            # All eligible
            ev['all_modes'] = self.choose_mode('all')
            ev['all_caption'] = self.text_on_screen('Everything eligible is included')
            ev['all_coverage'] = self.text_on_screen(' documents')
            self.shot('map-all-light.png')
            self.appearance(True)
            self.shot('map-all-dark.png')
            self.appearance(False)
            # back to Off
            ev['off_modes_again'] = self.choose_mode('off')
            ev['off_text_again'] = self.text_on_screen('Nothing is searched until you choose')
            self.shot('map-off-again-light.png')
            self.appearance(True)
            self.shot('map-off-dark.png')
            self.appearance(False)
            self.choose_mode('chosen')
            self.close_map()
            self.ax('click', '--id', 'set-btn', '--first')
            time.sleep(2)
            ev['settings_row'] = {'one_search_folder': self.text_on_screen('1 search folder'),
                                  'search_off': self.text_on_screen('Document search off'),
                                  'row_title': self.text_on_screen('Search folders')}
            self.shot('settings-row-light.png')
            self.facts['modes'] = ev
            self.save()
            return ev

        def company_limit(self):
            ev = {}
            self.open_map()
            self.expand('home')
            self.expand('Docs')
            ev['alpha'] = self.tick('Alpha') if (self.box('Alpha') or {}).get('value') in (0, '0', False) else 'already'
            ev['beta'] = self.tick('Bravo') if (self.box('Bravo') or {}).get('value') in (0, '0', False) else 'already'
            end = time.monotonic() + 90
            while time.monotonic() < end and not self.text_on_screen('Rich in each company'):
                time.sleep(2)
            ev['pane_shown'] = self.text_on_screen('Rich in each company')
            self.shot('limit-pane-light.png')
            self.ax('click', '--id', 'rf-company-toggle-beta', '--first')
            time.sleep(2)
            # the options sit in the right-hand pane under "Rich in each company"; their boxes carry no DOM id
            boxes = [n for n in self.checkboxes() if 925 < n['x'] < 1300 and 600 < n['y'] < 910]
            ev['beta_options'] = [{'value': n.get('value'), 'x': n.get('x'), 'y': n.get('y')} for n in boxes]
            self.shot('limit-options-light.png')
            names = [(n.get('value') or n.get('title') or '', n.get('y')) for n in self.nodes('--role', 'AXStaticText')]
            ev['static_names_near_options'] = [t for t, y in names if t in ('Alpha', 'Beta')]
            if len(boxes) < 2:
                raise StepFailed('the company limit did not offer two folders: ' + json.dumps(ev)[:600])
            # untick the one whose label is Beta: order is the tree's (Alpha first)
            beta_box = sorted(boxes, key=lambda n: n['y'])[1]
            self.click_at(beta_box['x'] + beta_box['w'] / 2, beta_box['y'] + beta_box['h'] / 2)
            time.sleep(3)
            ev['state_after_limit'] = self.text_on_screen('1 folder')
            ev['says_all_search_folders_beta'] = self.text_on_screen('All search folders')
            self.shot('limit-set-light.png')
            self.appearance(True)
            self.shot('limit-set-dark.png')
            self.appearance(False)
            saved = guest(self.vm, 'grep -r -l "sourceIds\\|source_ids" ' + shlex.quote(self.data) + ' 2>/dev/null | head -3 || true')
            ev['limit_files'] = saved
            if not ev['state_after_limit']:
                raise StepFailed('Beta did not show "1 folder" after the limit: ' + json.dumps(ev)[:700])
            # tick the parent folder (Docs): the limit must stay
            self.expand('home') if not self.box('Docs') else None
            ev['docs_tick'] = self.tick('Docs')
            time.sleep(2)
            ev['after_parent_tick_1_folder'] = self.text_on_screen('1 folder')
            ev['after_parent_all_search_folders_rows'] = len(self.nodes('--value', 'All search folders', '--contains'))
            self.shot('limit-after-parent-light.png')
            # All eligible
            self.choose_mode('all')
            time.sleep(3)
            ev['after_all_1_folder'] = self.text_on_screen('1 folder')
            ev['after_all_no_folders_text'] = self.text_on_screen('No folders')
            ev['after_all_remove_limit_button'] = bool(self.nodes('--title', 'Remove limit', '--role', 'AXButton'))
            ev['after_all_all_search_folders_rows'] = len(self.nodes('--value', 'All search folders', '--contains'))
            ev['after_all_outside_note'] = self.text_on_screen('Outside current search folders')
            self.shot('limit-after-all-light.png')
            company_words = ['No folders', 'Outside current search folders', 'Rich in each company', 'All search folders',
                             'A company limit only narrows', 'Alpha', 'Docs', 'myrichos', 'Saved. Applies']
            company_buttons = ['Remove limit', 'Done']
            ev['company_contrast'] = [self.contrast_of('limit-after-all-light.png',
                                                       self.page_texts(company_words, company_buttons, (925, 520, 1300, 910)), 'light')]
            self.appearance(True)
            self.shot('limit-after-all-dark.png')
            ev['company_contrast'].append(self.contrast_of('limit-after-all-dark.png',
                                                           self.page_texts(company_words, company_buttons, (925, 520, 1300, 910)), 'dark'))
            self.appearance(False)
            ev['all_search_folders_text_present'] = self.text_on_screen('All search folders')
            self.choose_mode('chosen')
            ev['after_back_chosen_1_folder'] = self.text_on_screen('1 folder')
            self.facts['company_limit'] = ev
            self.save()
            # Beta must never go back to "All search folders" (only Acme, unlimited, says that): after the
            # parent tick it still reads "1 folder"; after All eligible its limit is still there (Remove limit
            # offered, one row saying All search folders)
            if not (ev['after_parent_tick_1_folder'] and ev['after_parent_all_search_folders_rows'] == 1
                    and ev['after_all_remove_limit_button'] and ev['after_all_all_search_folders_rows'] == 1):
                raise StepFailed('the company limit did not survive: ' + json.dumps(ev)[:800])
            self.close_map()
            return ev

        def set_chosen(self, names):
            """Folder Map in Chosen folders mode with exactly `names` (children of ~/Docs) ticked."""
            self.open_map()
            self.choose_mode('chosen')
            self.ax('click', '--id', 'rf-folder-clear', '--first')
            time.sleep(3)
            self.choose_mode('chosen')
            self.expand('home')
            self.expand('Docs')
            ticks = {n: self.tick(n) for n in names}
            time.sleep(3)
            self.close_map()
            return ticks

        def source_boxes(self):
            """The palette's Sources checkboxes: the AXCheckBox nodes that are neither of the two mode
            boxes, each named by the static text on its own line (the box wraps its label)."""
            texts = self.nodes('--role', 'AXStaticText')
            out = []
            for n in self.nodes('--role', 'AXCheckBox'):
                if n.get('id') in ('srch-super', 'srch-strict'):
                    continue
                near = [t for t in texts if abs((t['y'] + t['h'] / 2) - (n['y'] + n['h'] / 2)) < 14
                        and t['x'] > n['x'] and t['x'] - n['x'] < 80]
                name = (near[0].get('value') if near else None) or n.get('title') or n.get('description')
                out.append({'name': name, 'value': n.get('value'), 'x': n['x'], 'y': n['y'], 'w': n['w'], 'h': n['h']})
            return out

        def palette_for(self, query):
            self.key('key code 53')
            time.sleep(0.7)
            self.key('keystroke "k" using command down')
            time.sleep(2)
            self.key('keystroke ' + json.dumps(query))
            time.sleep(6)

        def finder_sources(self):
            ev = {'chosen': self.set_chosen(['Alpha', 'Bravo'])}
            self.palette_for(QUERY)
            ev['checkbox_says_richfinder'] = self.text_on_screen('RichFinder')
            ev['says_super_finder_anywhere'] = self.text_on_screen('Super Finder')
            ev['sources_legend'] = self.text_on_screen('Sources')
            ev['status_line'] = self.text_on_screen('RichFinder ready') or self.text_on_screen('RichFinder indexing')
            boxes = self.source_boxes()
            ev['source_boxes'] = boxes
            ev['before_hits'] = {'alpha': self.text_on_screen('walrus handbook for Alpha'),
                                 'beta': self.text_on_screen('walrus handbook for Beta'),
                                 'stash': self.text_on_screen('Hidden stash'), 'generated': self.text_on_screen('Generated harborlight'),
                                 'count_line': [n.get('value') for n in self.nodes('--value', ' results', '--contains')] +
                                               [n.get('value') for n in self.nodes('--value', ' result ', '--contains')]}
            self.shot('finder-before-light.png')
            palette = (440, 180, 1295, 615)
            words = ['Sources', 'Uncheck a folder to narrow', 'RichFinder ready', 'Require quoted text', 'RichFinder',
                     'DOCUMENTS', 'meaning and words', 'harborlight', 'MEANING', 'Alpha/alpha.md', 'Bravo/beta.md',
                     'results', 'Alpha', 'Bravo']
            ev['contrast'] = [self.contrast_of('finder-before-light.png', self.page_texts(words, ['Change saved folders', 'Open original'], palette), 'light')]
            self.appearance(True)
            self.shot('finder-before-dark.png')
            ev['contrast'].append(self.contrast_of('finder-before-dark.png', self.page_texts(words, ['Change saved folders', 'Open original'], palette), 'dark'))
            self.appearance(False)
            alpha = next((b for b in boxes if b['name'] == 'Alpha'), None)
            if not alpha:
                raise StepFailed('no Alpha box in the Sources list: ' + json.dumps(ev)[:900])
            self.click_at(alpha['x'] + alpha['w'] / 2, alpha['y'] + alpha['h'] / 2)
            time.sleep(6)
            ev['after_uncheck_boxes'] = self.source_boxes()
            ev['after_hits'] = {'alpha': self.text_on_screen('walrus handbook for Alpha'),
                                'beta': self.text_on_screen('walrus handbook for Beta'),
                                'count_line': [n.get('value') for n in self.nodes('--value', ' result', '--contains')]}
            ev['note_after'] = self.text_on_screen('Uncheck a folder to narrow this search.')
            self.shot('finder-narrowed-light.png')
            self.appearance(True)
            self.shot('finder-narrowed-dark.png')
            self.appearance(False)
            self.facts['finder_sources'] = ev
            self.save()
            problems = []
            if not (ev['before_hits']['alpha'] and ev['before_hits']['beta']):
                problems.append('both folders should answer before unchecking')
            if ev['before_hits']['stash'] or ev['before_hits']['generated']:
                problems.append('a hidden or generated-output folder answered')
            if ev['after_hits']['alpha'] or not ev['after_hits']['beta']:
                problems.append('unchecking Alpha did not narrow the search to Beta')
            if ev['says_super_finder_anywhere'] or not ev['checkbox_says_richfinder']:
                problems.append('the wording is not RichFinder')
            if problems:
                raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:900])
            return ev

        SEED = """
import json, sys, time
path, first, second = sys.argv[1], sys.argv[2], sys.argv[3]
rows = [json.loads(l) for l in open(path) if l.strip()]
made = [r for r in rows if r.get('event') == 'ThreadCreated' and r.get('entity_id')]
thread = made[0]
rev = max(r.get('binding_revision', 0) for r in rows)
now = int(time.time() * 1000)
turn = 'turn_ffffffffffffffffffffffffffffff01'
wb = thread.get('written_by', 1)
out = [
 {'event': 'PromptReceived', 'turn_id': turn, 'thread_id': thread['thread_id'], 'text': 'Check my refund window please.',
  'source': 'text', 'at': now, 'entity_id': thread['entity_id'], 'binding_revision': rev, 'written_by': wb},
 {'event': 'TurnStarted', 'turn_id': turn, 'session_id': 'seed-session', 'at': now + 1, 'written_by': wb},
 {'event': 'AssistantDelta', 'turn_id': turn, 'text': first, 'at': now + 2, 'seq': 1, 'written_by': wb},
 {'event': 'AssistantDelta', 'turn_id': turn, 'text': second, 'at': now + 3, 'seq': 3, 'written_by': wb},
 {'event': 'TurnCompleted', 'turn_id': turn, 'stop_reason': 'end_turn', 'at': now + 4, 'written_by': wb},
]
with open(path, 'a') as f:
    for r in out:
        f.write(json.dumps(r) + chr(10))
print(json.dumps({'thread': thread['thread_id'], 'rev': rev, 'turn': turn}))
"""

        def split_reply(self):
            ev = {}
            sys.path.insert(0, str(HERE))
            from relaunch import quit_app
            quit_app(self.vm)
            time.sleep(2)
            ledger = self.data + '/conversation-ledger.jsonl'
            ev['seeded'] = guest(self.vm, 'python3 -c ' + shlex.quote(self.SEED) + ' ' + shlex.quote(ledger) + ' '
                                 + shlex.quote(SPLIT_FIRST) + ' ' + shlex.quote(SPLIT_SECOND))
            relaunch(self.vm, environment={})
            time.sleep(8)
            command([HERE / 'ax.sh', self.vm, '--key', '36'])
            time.sleep(3)
            ev['text_hidden_before'] = not self.text_on_screen('thirty days for annual')
            self.palette_for(SPLIT_QUERY)
            ev['conversations_section'] = self.says('CONVERSATIONS', 60)
            ev['open_message_nodes'] = [{k: n.get(k) for k in ('role', 'x', 'y', 'w', 'h')}
                                        for n in self.nodes('--title', 'Open message', '--contains')]
            self.shot('split-hit-light.png')
            if not ev['conversations_section']:
                raise StepFailed('the reply in two parts was not found by its exact word: ' + json.dumps(ev)[:900])
            if ev['open_message_nodes']:
                n = ev['open_message_nodes'][0]
                at = f"{round(n['x'] + n['w'] / 2)},{round(n['y'] + n['h'] / 2)}"
                command([HERE / 'pointer-drag.sh', self.vm, at, at], 60)
            else:
                self.key('key code 36')
            time.sleep(5)
            self.shot('split-opened.png')
            ev['palette_closed'] = not self.text_on_screen('Require quoted text')
            ev['second_part_on_screen'] = self.says('thirty days for annual', 20)
            ev['first_part_on_screen'] = self.text_on_screen('look at the club notes first')
            self.facts['split_reply'] = ev
            self.save()
            if not (ev['palette_closed'] and ev['second_part_on_screen']):
                raise StepFailed('the hit did not open the part: ' + json.dumps(ev)[:900])
            return ev

        def smoke(self):
            ev = {}
            self.key('key code 53')
            before = self.completed_turns()
            self.wait_for('Message to Rich', 'AXTextArea', seconds=60)
            self.type_into('Reply with the single word ready.', '--role', 'AXTextArea', '--title', 'Message to Rich')
            self.press('Send')
            end = time.monotonic() + 240
            while time.monotonic() < end and self.completed_turns() <= before:
                time.sleep(3)
            ev['reply_completed'] = self.completed_turns() > before
            ledger = self.data + '/conversation-ledger.jsonl'
            ev['assistant_rows'] = int(guest(self.vm, 'grep -c AssistantDelta ' + shlex.quote(ledger) + ' || true').strip() or 0)
            self.shot('smoke-reply.png')
            self.ax('click', '--id', 'set-btn', '--first')
            time.sleep(2)
            ev['settings_open'] = self.text_on_screen('Search folders') or bool(self.nodes('--id', 'set-search-folders-open'))
            ev['updates_row'] = bool(self.nodes('--id', 'set-updates'))
            ev['update_buttons'] = [n.get('title') or n.get('description') for n in self.nodes('--role', 'AXButton')
                                    if 'pdate' in (n.get('title') or n.get('description') or '')]
            self.shot('smoke-settings.png')
            self.ax('click', '--title', 'Check for updates', '--role', 'AXButton', '--contains', '--first')
            time.sleep(10)
            ev['update_check_words'] = [w for w in ('up to date', 'latest', 'Checking', 'checking', 'available', 'Could not',
                                                    "couldn't", 'could not', 'newer') if self.text_on_screen(w)]
            self.shot('smoke-update-check.png')
            self.facts['smoke'] = ev
            self.save()
            if not (ev['reply_completed'] and ev['settings_open']):
                raise StepFailed('smoke: ' + json.dumps(ev)[:700])
            return ev

    return Walk, StepFailed


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--bulk', type=int, default=0)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    Walk, StepFailed = build()
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and step in ('identity', 'first-run'):
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""richscribe-ui-walk.py — the RichScribe and SwearMeter screens a user touches, on the packaged app, in the guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home COMPANY_HOME --engine ENGINE --report REPORT -- \\
      richscribe-ui-walk.py --out DIR --expect-sha SHA --wav SPOKEN.wav --model SMALL.bin \\
          --post-key POST_KEY --key-probe KEY_PROBE [--steps a,b,c]

COMPANY_HOME is a `gui_boot_machine` home (one registered company, Northwind Traders), because the
vocabulary editor saves words only for a registered company.

WHY IT EXISTS. dictation-bar-walk.py, phase-a-walk.py, completion-native-walk.py and
writing-native-walk.py walk the dictation itself; none of them presses the recording bar's Finish,
reads the "Sent to ..." result, saves a word in the vocabulary editor, collects through RichDoctor's
own support adapter or turns SwearMeter on. Nightly 89's brief names each of those.

No sound is played anywhere (CEO §53): the sample is a file put through the capture path by
RICHOS_VOICE_INPUT_WAV. Never on the host's screen (CEO §65).

  identity, stage, relaunch, settle   (dictation-walk.py / dictation-bar-walk.py)
  bar-finish    F1, then the bar's own Finish button: the words land in TextEdit
  bar-cancel    F1, then the bar's Cancel button: nothing lands, the bar is gone, and the next
                dictation still works
  browser-sent  a dictation into a Chromium browser, whose text box cannot be read back: the
                result reads "Sent to Google Chrome" (frames, OCR)
  doctor        after a dictation, RichDoctor's own `--support-adapter` (the two read-only tools the
                signed-in Claude is given) is asked for the `richscribe` category: it answers with
                the dictation timing records and none of the spoken words
  swearmeter    off by default; on shows the count in settings and on the home screen at once; the
                retention (history removal) line stays fully inside the panel; off hides both
  vocabulary    Settings, RichScribe: choose the company, add a spelling, Save; it is listed and
                survives closing and reopening the sheet
  preview       Settings, RichScribe, More options: Live preview off by default, switched on, kept
Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--wav', type=Path, required=True)
    p.add_argument('--model', type=Path, required=True)
    p.add_argument('--post-key', type=Path, required=True)
    p.add_argument('--key-probe', type=Path, required=True)
    p.add_argument('--steps', default='identity,stage,relaunch,settle,bar-finish,bar-cancel,browser-sent,doctor,swearmeter,vocabulary,preview')
    a = p.parse_args()
    a.model_id, a.idle = 'small.en', 0
    a.long_wav = a.silent_wav = a.wav
    a.accurate_model = None
    here = Path(__file__).resolve().parent
    sys.path.insert(0, str(here))
    spec = importlib.util.spec_from_file_location('native_bar', here / 'dictation-bar-walk.py')
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    guest, StepFailed = base.guest, base.StepFailed
    command = base.command
    adopt = base.dictation_walk.adopt_walk
    import axwait

    class Walk(base.BarWalk):
        # ---- helpers -------------------------------------------------------------------------
        def stage(self):
            result = super().stage()
            command([here / 'guest.sh', self.vm, '--push', here / 'ax.py', self.payload + '/ui-ax.py'], 120)
            return result

        def tool_ax(self, mode, title=None, role=None):
            tools = self.tool_pids()
            if len(tools) != 1:
                raise StepFailed('Expected one owned tool, got ' + repr(tools))
            params = {'mode': mode, 'pid': tools[0]['pid'], 'app': '', 'depth': 30, 'max': 1500, 'first': True, 'contains': False}
            if title is not None:
                params['text'] = title
            if role is not None:
                params['role'] = role
            if title == 'Cancel' and role == 'AXButton':
                params['contains'] = True  # the control carries its visible esc hint
            code = 'import runpy; runpy.run_path(' + repr(self.payload + '/ui-ax.py') + ',run_name="__main__",init_globals={"AX_PARAMS_JSON":' + repr(json.dumps(params)) + '})'
            output = guest(self.vm, 'python3 -c ' + shlex.quote(code), 30)
            nodes = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
            errors = [n for n in nodes if n.get('error')]
            absent = mode == 'find' and any(n.get('exhaustive') is True and n.get('matches') == 0 for n in nodes) and all(
                n['error'] == 'notfound' or n['error'] == 'blocked' and 'nothing matched' in n.get('detail', '') for n in errors)
            # A bar that is gone leaves the tool with no window at all: that answer is "absent".
            gone = mode == 'find' and bool(errors) and all(n['error'] == 'nowindow' for n in errors)
            if errors and not absent and not gone:
                raise StepFailed('tool AX read/action failed: ' + repr(nodes))
            return nodes

        def wait_tool_button(self, title, seconds=30):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if any(n.get('role') == 'AXButton' for n in self.tool_ax('find', title, 'AXButton')):
                    return True
                time.sleep(.3)
            raise StepFailed('the bar showed no ' + title + ' button within ' + str(seconds) + ' s')

        def tool_button_gone(self, title, seconds=15):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if not any(n.get('role') == 'AXButton' for n in self.tool_ax('find', title, 'AXButton')):
                    return True
                time.sleep(.5)
            raise StepFailed('the bar still showed ' + title + ' after ' + str(seconds) + ' s')

        def by_id(self, dom_id):
            try:
                nodes = self.ax('find', '--id', dom_id, '--first')
            except StepFailed as exc:
                if axwait.not_yet(exc, 'find') or 'nowindow' in str(exc):
                    return None
                raise
            nodes = [n for n in nodes if not n.get('meta')]
            return nodes[0] if nodes else None

        def until(self, check, seconds, what):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                got = check()
                if got:
                    return got
                time.sleep(.7)
            raise StepFailed(what + ' within ' + str(seconds) + ' s')

        def press_title(self, title, role='AXButton'):
            return adopt.Walk.press(self, title, role, None)

        def decline_sheets(self, seconds=30):
            end = time.monotonic() + seconds
            n = 0
            while time.monotonic() < end and n < 3:
                if self.present('Not now'):
                    self.press_title('Not now')
                    n += 1
                    time.sleep(3)
                else:
                    time.sleep(1)
            return n

        def open_settings(self):
            for attempt in range(3):
                try:
                    self.ax('click', '--id', 'set-btn', '--first')
                    self.until(lambda: self.by_id('set-swearmeter') or self.by_id('set-dictation-open'), 20, 'the Settings panel did not open')
                    return
                except StepFailed:
                    if attempt == 2:
                        raise
                    self.decline_sheets(10)

        def close_settings(self):
            try:
                self.osa('tell application "System Events" to key code 53')
            except Exception:
                pass
            time.sleep(1)

        def dictate_into_textedit(self, label, finish):
            """F1, the sample, then `finish`: 'button' presses the bar's Finish, 'key' taps F1 again."""
            self.use_sample(self.spoken)
            self.open_textedit(self.home + '/ui-' + label + '.txt')
            since, before = len(self.dlog_lines()), self.samples_delivered()
            self.press('122')
            self.wait_dlog('listening (', since, 20)
            self.wait_delivered(before)
            if finish == 'button':
                self.wait_tool_button('Finish')
                self.shot(label + '-bar.png')
                self.tool_ax('click', 'Finish', 'AXButton')
            else:
                self.press('122')
            result = self.wait_dlog('dictation: model ', since, 120)
            return since, result

        # ---- steps ---------------------------------------------------------------------------
        def bar_finish(self):
            since, result = self.dictate_into_textedit('finish', 'button')
            if 'pasted' not in result or 'delivery=added' not in result:
                raise StepFailed('Finish did not add the words: ' + result)
            model = re.search(r'dictation: model (\S+),', result).group(1)
            self.expect_words(self.textedit_text(), self.expected(model), 'the bar\'s Finish button')
            self.shot('finish-added.png')
            return dict(result=result)

        def bar_cancel(self):
            self.use_sample(self.spoken)
            self.open_textedit(self.home + '/ui-cancel.txt')
            since, before = len(self.dlog_lines()), self.samples_delivered()
            self.press('122')
            self.wait_dlog('listening (', since, 20)
            self.wait_delivered(before)
            self.wait_tool_button('Cancel')
            self.shot('cancel-bar.png')
            self.tool_ax('click', 'Cancel', 'AXButton')
            self.tool_button_gone('Finish')
            time.sleep(3)
            if self.textedit_text().strip():
                raise StepFailed('Cancel left words in TextEdit: ' + repr(self.textedit_text()))
            late = [l for l in self.dlog_lines()[since:] if 'dictation: model ' in l]
            if late:
                raise StepFailed('Cancel still produced a dictation: ' + late[-1])
            self.shot('cancel-gone.png')
            again = self.dictate('122', base.TEXTEDIT, 'after-cancel')
            self.expect_words(self.textedit_text(), again['expected'], 'the dictation after Cancel')
            return dict(cancelled_nothing_added=True, next_dictation=again['log'])

        def browser_sent(self):
            """A destination whose text the app cannot read back (a Chromium browser with no
            accessibility switched on): the result should say "Sent to ...", the words having been
            pasted but not seen. Frames (OCR) are read until one carries "Sent to" or "Check"."""
            found = self.apps()
            app = self.facts['chromium_app']
            guest(self.vm, 'open -na ' + shlex.quote('/Applications/' + app) + ' --args ' + ' '.join(base.dictation_walk.CHROME_FLAGS) +
                  ' --user-data-dir=' + shlex.quote(self.payload + '/chrome-sent') + ' ' + shlex.quote('file://' + self.page))
            time.sleep(10)
            self.use_sample(self.spoken)
            bundle = guest(self.vm, 'defaults read /Applications/' + shlex.quote(app) + '/Contents/Info CFBundleIdentifier')
            if self.front_bundle() != bundle:
                raise StepFailed('the browser is not in front: ' + self.front_bundle())
            since, before = len(self.dlog_lines()), self.samples_delivered()
            self.press('122')
            self.wait_dlog('listening (', since, 20)
            self.wait_delivered(before)
            self.press('122')
            seen, n, end = None, 0, time.monotonic() + 150
            while time.monotonic() < end and seen is None:
                name = self.grab('browser-%d' % n)
                text = self.read(name)
                for words in ('Sent to', 'Check Google', 'Added'):
                    if base.read_contains(text, words):
                        seen = (words, name)
                        break
                n += 1
            line = next((l for l in self.dlog_lines()[since:] if 'dictation: model ' in l), '')
            if seen is None:
                raise StepFailed('no frame read a result; log line: ' + line)
            (self.out / 'browser-result-frame.txt').write_text(seen[0] + ' ' + seen[1] + '\n')
            if seen[0] != 'Sent to':
                raise StepFailed('the result frame read ' + seen[0] + ' (' + seen[1] + '.png), not "Sent to"; log line: ' + line)
            return dict(result_words=seen[0], frame=seen[1] + '.png', log=line, apps=found)

        def doctor(self):
            # a dictation first, so the log has this run's timing records
            self.use_sample(self.spoken)
            ev = self.dictate_into_textedit('doctor', 'key')[1]
            apps = guest(self.vm, 'find ' + shlex.quote(self.home) + ' -maxdepth 7 -name RichDoctor -path "*MacOS*" 2>/dev/null | head -3')
            binary = apps.splitlines()[0] if apps.strip() else ''
            if not binary:
                raise StepFailed('no RichDoctor executable under the guest home')
            data = self.data
            home = data.split('/Library/Application Support/')[0]
            draft = self.payload + '/rd-draft'
            now = int(time.time() * 1000)
            scope = json.dumps({'home': home, 'window': {'start': now - 86400000, 'end': now + 60000, 'known': False}, 'claude_bin': None, 'diagnostics': True})
            guest(self.vm, 'rm -rf ' + shlex.quote(draft) + ' && mkdir -m 700 ' + shlex.quote(draft) + ' && printf %s ' + shlex.quote(scope) + ' > ' + shlex.quote(draft + '/scope.json') + ' && chmod 600 ' + shlex.quote(draft + '/scope.json'))
            msgs = [{'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-11-25'}},
                    {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call', 'params': {'name': 'collect_diagnostics', 'arguments': {'category': 'richscribe'}}}]
            feed = '\n'.join(json.dumps(m) for m in msgs) + '\n'
            out = guest(self.vm, 'printf %s ' + shlex.quote(feed) + ' | ' + shlex.quote(binary) + ' --support-adapter ' + shlex.quote(draft), 60)
            replies = [json.loads(l) for l in out.splitlines() if l.startswith('{')]
            call = next((r for r in replies if r.get('id') == 2), None)
            if not call or call['result'].get('isError'):
                raise StepFailed('the adapter did not answer the richscribe collection: ' + out[:600])
            listing = json.loads(call['result']['content'][0]['text'])
            items = listing['items']
            if not items:
                raise StepFailed('the richscribe collection returned no item')
            ident = items[0].get('id') or items[0].get('evidence_id')
            read = guest(self.vm, 'printf %s ' + shlex.quote(json.dumps({'jsonrpc': '2.0', 'id': 3, 'method': 'tools/call', 'params': {'name': 'read_evidence', 'arguments': {'id': ident}}}) + '\n') + ' | ' + shlex.quote(binary) + ' --support-adapter ' + shlex.quote(draft), 60)
            got = next(json.loads(l) for l in read.splitlines() if l.startswith('{'))
            text = got['result']['content'][0]['text']
            (self.out / 'doctor-evidence.txt').write_text(text)
            if 'dictation timing' not in text:
                raise StepFailed('the collection holds no dictation timing record')
            spoken = [w for w in ('quarterly', 'report', 'Friday', 'afternoon', 'team') if w.lower() in text.lower()]
            if spoken:
                raise StepFailed('the collection holds spoken words: ' + repr(spoken))
            return dict(items=len(items), summary=items[0].get('summary'), timing_records=text.count('dictation timing'), spoken_words_found=spoken)

        def text_of(self, node):
            return (node or {}).get('value') or (node or {}).get('title') or (node or {}).get('description') or ''

        def rect(self, dom_id):
            n = self.by_id(dom_id)
            return None if n is None else {k: n.get(k) for k in ('x', 'y', 'w', 'h')}

        def scroll_into_view(self, dom_id, footer_top=770):
            """The Settings panel scrolls under a fixed footer (Your name, Splash screen): a row whose
            bottom edge is below `footer_top` is under it. Page Down in the panel, as a hand scrolls,
            until the row is clear of the footer (the Show SwearMeter row comes up half
            under the footer at the guest's 1680x1050)."""
            for _ in range(8):
                r = self.rect(dom_id)
                if r and r['y'] + r['h'] < footer_top:
                    return r
                pt = '%d,%d' % (520, 182)  # the panel's own title: a click there takes focus, presses nothing
                command([here / 'pointer-drag.sh', self.vm, pt, pt], 60)
                self.osa('tell application "System Events" to key code 121')
                time.sleep(1)
            return self.rect(dom_id)

        def mouse_click(self, node_rect):
            # The row spans the panel column; the switch is at its right end (the label's click area is
            # only its words), so the press lands 20 px in from the right edge, on the row's middle.
            x, y = node_rect['x'] + node_rect['w'] - 20, node_rect['y'] + node_rect['h'] / 2
            point = '%d,%d' % (x, y)
            command([here / 'pointer-drag.sh', self.vm, point, point], 60)

        def mouse_click_center(self, r):
            point = '%d,%d' % (r['x'] + r['w'] / 2, r['y'] + r['h'] / 2)
            command([here / 'pointer-drag.sh', self.vm, point, point], 60)

        def reads_in(self, frame, box, text, tag):
            """Whether the frame's pixels around `box` read `text` (OCR on a 3x crop): what a person sees there,
            not what the accessibility tree says is in the page."""
            qa = here.parent / 'qa'
            crop = self.out / (tag + '.png')
            x, y, w, h = [int(v) for v in (box['x'] - 8, box['y'] - 8, box['w'] + 16, box['h'] + 16)]
            subprocess.run([str(qa / 'frame.py'), 'crop', str(self.out / frame), str(crop), str(x), str(y), str(w), str(h), '--scale', '3'], check=True, capture_output=True, text=True, timeout=60)
            r = subprocess.run([str(qa / 'ocr-find.sh'), text, str(crop), '--quiet'], capture_output=True, text=True, timeout=120)
            if r.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the crop: ' + r.stderr)
            return r.returncode == 0

        def swearmeter(self):
            self.decline_sheets(15)
            # The count is drawn on the home screen: a home with no company opens on a conversation, so
            # go Home first by the rail's own Home button, as a person does.
            home = self.rect('nr-home')
            if home and not self.rect('home-swearmeter-host'):
                self.mouse_click_center(home)
                time.sleep(3)
            self.open_settings()
            box = self.until(lambda: self.rect('set-swearmeter'), 15, 'no Show SwearMeter control')
            self.shot('swear-0-before-scroll.png')
            box = self.scroll_into_view('set-swearmeter') or box
            node = self.by_id('set-swearmeter')
            self.shot('swear-0-default.png')
            if str(node.get('value')) not in ('0', 'False', 'false', ''):
                raise StepFailed('Show SwearMeter is not off by default: ' + repr(node))
            if self.by_id('home-swearmeter'):
                raise StepFailed('the home screen shows SwearMeter while it is off')
            began = time.monotonic()
            self.mouse_click(box)                       # the real mouse, as a hand clicks the switch
            self.until(lambda: self.rect('home-swearmeter'), 8, 'the home screen shows no SwearMeter after switching on (clicked the center of %r)' % (box,))
            on_home = time.monotonic() - began
            self.shot('swear-1-on.png')
            geometry = {name: self.rect(name) for name in ('set-swearmeter', 'set-swearmeter-result', 'retention-hint', 'home-swearmeter')}
            result = geometry['set-swearmeter-result']
            if not result or not result.get('w'):
                raise StepFailed('settings has no count line after switching on: ' + repr(geometry))
            seen_settings = self.reads_in('swear-1-on.png', result, 'SwearMeter', 'swear-1-result-crop')
            seen_home = self.reads_in('swear-1-on.png', geometry['home-swearmeter'], 'swear', 'swear-1-home-crop')  # round 73: "N swear words", "in the last 30 days"
            retention_seen = bool(geometry['retention-hint']) and self.reads_in('swear-1-on.png', geometry['retention-hint'], 'Using', 'swear-1-retention-crop')
            problems = []
            if not seen_home:
                problems.append('the home screen draws no SwearMeter count')
            if not seen_settings:
                problems.append('the count line in settings is not visible in the frame (covered or off the panel)')
            if not retention_seen:
                problems.append('the history retention line is not fully visible with SwearMeter on')
            self.mouse_click(self.scroll_into_view('set-swearmeter') or self.rect('set-swearmeter'))
            end = time.monotonic() + 10
            while self.rect('home-swearmeter') and time.monotonic() < end:
                time.sleep(.5)
            if self.rect('home-swearmeter'):
                problems.append('the home screen still shows SwearMeter after switching off')
            self.shot('swear-2-off.png')
            if problems:
                raise StepFailed('; '.join(problems) + ': ' + json.dumps(dict(geometry=geometry, seconds_to_home=round(on_home, 2), count_visible_in_settings=seen_settings, count_visible_on_home=seen_home, retention_line_visible=retention_seen)))
            return dict(seconds_to_home=round(on_home, 2), geometry=geometry, count_visible_in_settings=seen_settings, count_visible_on_home=seen_home, retention_line_visible=retention_seen)

        def open_sheet(self):
            for attempt in range(3):
                try:
                    self.open_settings()
                    self.ax('click', '--id', 'set-dictation-open', '--first')
                    self.until(lambda: self.by_id('dict-switch'), 30, 'the RichScribe sheet did not open')
                    return
                except StepFailed:
                    if attempt == 2:
                        raise
                    self.decline_sheets(10)

        def vocabulary(self):
            self.open_sheet()
            self.shot('vocab-0-sheet.png')
            self.until(lambda: self.by_id('voc-company'), 20, 'no company chooser (is a company registered?)')
            self.ax('click', '--id', 'voc-company', '--first')
            time.sleep(1)
            opt = [n for n in self.ax('find', '--title', 'Northwind Traders', '--contains') if not n.get('meta') and n.get('w')]
            (self.out / 'vocab-option-nodes.json').write_text(json.dumps(opt, indent=1))
            if not opt:
                raise StepFailed('the company list shows no Northwind Traders')
            self.mouse_click_center(opt[-1])
            time.sleep(2)
            self.press_title('Add spelling')
            spell = self.until(lambda: self.by_id('voc-spell'), 10, 'no spelling form')
            self.type_into('Zephyrion', '--id', 'voc-spell')
            self.type_into('zephyrian', '--id', 'voc-heard')
            self.shot('vocab-1-form.png')
            self.ax('click', '--id', 'voc-save', '--first')
            self.until(lambda: self.shows_text('Zephyrion'), 15, 'the saved spelling is not listed')
            self.shot('vocab-2-saved.png')
            saved = guest(self.vm, 'cat ' + shlex.quote(self.data + '/dictation-vocabulary.json') + ' 2>/dev/null || ls ' + shlex.quote(self.data))
            self.close_settings()
            self.open_sheet()
            self.until(lambda: self.shows_text('Zephyrion'), 15, 'the spelling is not there after reopening the sheet')
            self.shot('vocab-3-reopened.png')
            return dict(saved_file=saved[:400])

        def shows_text(self, text):
            for flag in ('--value', '--title'):
                try:
                    if self.ax('find', flag, text, '--contains', '--first'):
                        return True
                except StepFailed as exc:
                    if not axwait.not_yet(exc, 'find'):
                        raise
            return False

        def preview(self):
            self.open_sheet()
            if not self.present('More options'):
                raise StepFailed('no More options in the sheet')
            self.press_title('More options')
            time.sleep(1)
            self.shot('preview-0-off.png')
            if not self.shows_text('Live preview off'):
                raise StepFailed('Live preview is not off by default')
            nodes = [n for n in self.ax('find', '--title', 'Live preview', '--first') if not n.get('meta')]
            (self.out / 'preview-nodes.json').write_text(json.dumps(nodes, indent=1))
            sw = [n for n in nodes if n.get('w') and n.get('role') in ('AXCheckBox', 'AXSwitch', 'AXButton', 'AXRadioButton', 'AXToggle')] or [n for n in nodes if n.get('w')]
            # The switch sits below the sheet's visible part, where a mouse press would land on the desktop:
            # the accessibility press (which scrolls to it) is the way in here.
            self.ax('click', '--title', 'Live preview', '--role', 'AXCheckBox', '--first')
            self.until(lambda: self.shows_text('Live preview on'), 10, 'Live preview did not switch on')
            self.shot('preview-1-on.png')
            saved = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/dictation.json')))
            return dict(saved=saved)

    walk = Walk(a)
    steps = a.steps.split(',')
    report = dict(vm=a.vm, commit=a.expect_sha, steps=[], limitations=['Synthetic file injection, no audible playback or physical microphone'])
    a.out.mkdir(parents=True, exist_ok=True)
    ok = True
    try:
        for step in steps:
            began = time.monotonic()
            row = dict(step=step)
            try:
                row.update(outcome='PASS', evidence=getattr(walk, step.replace('-', '_'))())
            except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, StopIteration, subprocess.TimeoutExpired) as e:
                row.update(outcome='FAIL', detail=str(e))
                ok = False
            row['seconds'] = round(time.monotonic() - began, 3)
            report['steps'].append(row)
            (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            print(step + ': ' + row['outcome'] + (' ' + row.get('detail', '') if row['outcome'] == 'FAIL' else ''), flush=True)
            if row['outcome'] == 'FAIL':
                walk.shot(step + '-failed.png')
                # The setup steps decide whether any later step can run; the checks after them are
                # independent of each other, so one failing check does not hide the rest.
                if step in ('identity', 'stage', 'relaunch', 'settle'):
                    break
    finally:
        (a.out / 'dictation.log').write_text(guest(a.vm, 'cat ' + shlex.quote(walk.dlog) + ' 2>/dev/null || true', 30))
    return 0 if ok else 1


if __name__ == '__main__':
    raise SystemExit(main())

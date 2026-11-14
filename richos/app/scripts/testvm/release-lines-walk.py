#!/usr/bin/env python3
"""release-lines-walk.py — the user-facing lines of a release's notes (written for 1.3.2), in the packaged app, in the guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 1800 [--no-audio] -- \\
      release-lines-walk.py --out DIR --expect-sha SHA [--steps a,b,c] \\
          [--model LARGE.bin --meeting MEETING.wav --meeting-lines MEETING.json]

run-walk.py passes the owned VM name as the first argument. Two runs cover it, each in its own guest:
  UI       --steps identity,strip,first-run,welcome,richfiles,output-file,moments-local,moments-youtube,disk-low
  meetings --steps identity,companion,audio,first-run,prompt,ask-live,stop-notes,history   (with --no-audio)
  scribe   --steps identity,first-run,scribe-stage,scribe-relaunch,talk-now,scribe-long,cleaner
  onboard  --steps identity,first-run,onboarding,second-company   (with --site, a stand-in LinkedIn)

WHY IT EXISTS. The 1.3.2 release added RichFiles, the five tools on a first install, media moments, Source links
in meeting notes, asking Rich about a call while it records, questions across past meetings and the app's
own cleanup when the disk is low. No committed walk drove any of them in the packaged app. This one does,
as a user does each, reusing meeting-walk.py's guest helpers (BlackHole, the stand-in mic user, Take
Notes) and conversation-walk.py's ledger readers, never copies of them.

STEPS (in the guest, never on the host's screen, CEO section 65; nothing is played aloud, section 53):
  identity        the running app says it was built from --expect-sha
  strip           the first setup sheet: "Five tools come with me, free." and the five tools by name;
                  each icon pressed shows its own line under the strip; light and dark frames
  first-run       the sheets declined, the company "Acme" with a folder, the first conversation
  welcome         the onboarding card ("Let's get me up to speed.", Start onboarding); Not now pressed:
                  Rich's note that five tools came with him and the tools card with Turn on RichScribe;
                  Turn on RichScribe pressed: the card asks for the key, the microphone and Accessibility
  richfiles       the rail's RichFiles item opens the RichFiles page with the native browser in it (its
                  accessibility tree kept), light and dark frames; Compare Folders, Find Duplicate Files
                  and Disk Usage each opened and framed in both themes; Undock RichFiles opens RichFiles as its own app, which is then docked again
  output-file     Rich writes a file; in the Output panel, the file's menu has Show in RichFiles, and
                  pressing it opens the RichFiles page on that file
  moments-local   a recording in ~/Downloads (the meeting fixture) and a question about where in it a
                  topic comes up: a card lists moments with times and words; pressing one plays the
                  recording in the Output panel
  moments-youtube a YouTube video and the same kind of question: a card of moments, the video saved in
                  ~/Downloads/RichOS Videos, pressing a moment opens the video in the Output panel
  disk-low        the guest's disk filled to under 15% free while the app runs: within one 15-minute
                  check the app log says "disk low (... GB free): starting the cleanup" and the sweep runs
  companion, audio, prompt   meeting-walk.py's own steps
  ask-live        Take Notes pressed, --meeting played into the microphone; while it still plays, Rich is
                  asked what Priya just said about the trade show: his answer names what the last minute
                  said (hall B), and the recording is still running when he answers
  stop-notes      Stop: the notes come; Decisions is the first heading; Source links are in them; pressing
                  one opens the transcript in the Output panel at that passage. Live transcription ran
                  during the call (the app log) and the seconds from Stop to the notes are recorded
  history         a follow-up email for that meeting and who owes what: each answer names the meeting
                  and its date
  scribe-stage, scribe-relaunch   dictation-walk.py's stage, relaunch and settle with --dict-wav and More
                  accurate (the large model), the sample standing in for the microphone
  talk-now        onboarding started with RichScribe working: the four quick things given; when Rich comes
                  back by himself with his questions, "Or just say it: press F1 and talk." and Talk now
                  are above the message box; Talk now starts RichScribe and the words land in the box
  scribe-long     Live preview switched on in the RichScribe sheet; one dictation of --dict-wav (over
                  30 s, with a 12 s pause) into TextEdit, the screen photographed throughout (the bar's
                  live preview); the written text against the words that were said: no run of 3 or more
                  spoken words missing, no words after the last spoken one
  onboarding      Start onboarding; the four quick things (an unreachable website, the stand-in LinkedIn);
                  Rich back by himself with his read and the questions only the user can answer; the
                  answers; the onboarding ends with a file card I can open
  second-company  a second company added from "+ New chat or company"; its onboarding asks only for its
                  website, never again for the name, pronouns or profiles
  cleaner         RichCleaner switched on; two related Desktop files selected in Finder, the RichScribe key
                  and the spoken note (--note-wav): both go into ONE folder, the existing "Pricing" one
                  that fits; the RichCleaner window offers Undo for each; Rich finds the set again

Every step records what it read into DIR/facts.json and its frames into DIR; a step that cannot answer
fails with the reason and the walk carries on (identity failing ends it). Exit 0 when every step passes.
Every app instance is quit by run-walk.py's stop.sh (CEO section 54).
"""
import argparse
import difflib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
STEPS = ['identity', 'strip', 'first-run', 'welcome', 'richfiles', 'output-file', 'moments-local', 'moments-youtube',
         'disk-low', 'companion', 'audio', 'prompt', 'ask-live', 'stop-notes', 'history',
         'scribe-stage', 'scribe-relaunch', 'talk-now', 'scribe-long', 'cleaner', 'onboarding', 'second-company']
TOOLS = ['RichScribe', 'RichBrief', 'RichFinder', 'RichCleaner', 'RichFiles']
RETURN_KEY = '36'


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


meeting = load('meeting_walk', HERE / 'meeting-walk.py')
convo = load('conversation_walk', HERE / 'conversation-walk.py')
guest_fn, folders = meeting.host_modules()
MeetingWalk, StepFailed = meeting.build_walk(folders, guest_fn)
adopt = folders.adopt
command = folders.command
LABDIR = meeting.LABDIR
dictation = load('dictation_walk', HERE / 'dictation-walk.py')
WORD = re.compile(r"[a-z0-9']+")


def word_gaps(said, written):
    """(the longest run of spoken words missing from what was written, the words written after
    the last spoken word). Words are compared lower case, punctuation and number spelling aside:
    a digit is accepted for its word ("8" for "eight") by normalizing both to digits."""
    nums = {'zero': '0', 'one': '1', 'two': '2', 'three': '3', 'four': '4', 'five': '5', 'six': '6', 'seven': '7',
            'eight': '8', 'nine': '9', 'ten': '10', 'eleven': '11', 'twelve': '12', 'fifteen': '15', 'hundred': '00',
            'first': '1st'}

    def norm(text):
        return [nums.get(w, w) for w in WORD.findall(text.lower().replace('$', ' ').replace('%', ' percent'))]
    a, b = norm(said), norm(written)
    sm = difflib.SequenceMatcher(a=a, b=b, autojunk=False)
    longest = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag in ('delete', 'replace') and (i2 - i1) > len(longest) and (tag == 'delete' or (i2 - i1) > (j2 - j1) + 2):
            longest = a[i1:i2]
    blocks = [m for m in sm.get_matching_blocks() if m.size]
    extra = b[blocks[-1].b + blocks[-1].size:] if blocks else b
    return longest, extra


class Walk(MeetingWalk):
    # conversation-walk.py's stand-in LinkedIn set-up, borrowed whole (prepare_site calls these on self)
    grant_accessibility = convo.ConversationWalk.grant_accessibility
    accessibility_rows = convo.ConversationWalk.accessibility_rows
    allow_local_network = convo.ConversationWalk.allow_local_network

    # --- conversation ----------------------------------------------------------------------------
    def rows(self):
        text = self.g('cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for raw in text.splitlines():
            try:
                out.append(json.loads(raw))
            except ValueError:
                pass
        return out

    def ask(self, text, within=600):
        """Type TEXT into the message box, Return, and wait for Rich's turn on the ledger to close.
        Returns conversation-walk.py's turn record (state, reply, seconds)."""
        sent = self.clock()
        self.type_into(text, '--role', 'AXTextArea', '--title', 'Message to Rich')
        command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY], 60)
        end = time.monotonic() + within
        tid = None
        while time.monotonic() < end and not tid:
            tid = convo.find_turn(self.rows(), 'say', text, sent)
            if not tid:
                time.sleep(2)
        if not tid:
            raise StepFailed('no turn started for the message %r' % text[:80])
        while True:
            record = convo.turn_record(self.rows(), tid)
            if record['state'] != 'open' or time.monotonic() >= end:
                break
            time.sleep(3)
        record['seconds'] = round((record['closed_at_ms'] - sent) / 1000, 1) if record['closed_at_ms'] else None
        record['turn'] = tid
        return record

    def tree_to(self, name, *scope):
        nodes = [n for n in self.ax('tree', *scope) if not n.get('meta')]
        (self.out / name).write_text(''.join(json.dumps(n) + '\n' for n in nodes))
        return nodes

    def esc(self):
        command([HERE / 'ax.sh', self.vm, 'tell application "System Events" to key code 53'], 60)
        time.sleep(1)

    def titles(self, nodes):
        return [(n.get('role'), n.get('title') or n.get('desc') or n.get('value') or '') for n in nodes]

    def first_run(self):
        """adopt-walk.py's first run (the folder typed), leaving the onboarding card unpressed."""
        self.decline_first_run_sheets()
        self.g('mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
               '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
               'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        self.wait_for('Add this company', seconds=60)
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        end = time.monotonic() + 60
        while time.monotonic() < end:
            created = [r for r in self.rows() if r.get('event') == 'ThreadCreated']
            if created:
                self.facts['thread'] = created[0].get('thread_id')
                self.save()
                return {'thread': self.facts['thread']}
            time.sleep(1)
        raise StepFailed('no conversation was created after the company was added')

    # --- the five tools ----------------------------------------------------------------------------
    def strip(self):
        self.wait_for('Not now', seconds=90)
        time.sleep(2)
        nodes = self.tree_to('strip.tree.jsonl')
        text = json.dumps(self.titles(nodes))
        ev = {'head': 'Five tools come with me, free.' in text, 'tools': {t: t in text for t in TOOLS}}
        ev['frames'] = self.both_themes('strip')
        lines = {}
        for t in TOOLS:
            # Pointing at an icon says what it does in the line under the strip (suite-tools.js
            # mouseenter): the real pointer rests on the strip's button for that tool.
            icons = [n for n in nodes if n.get('role') in ('AXButton', 'AXCheckBox') and (n.get('title') or n.get('desc') or '') == t
                     and 540 < n.get('x', 0) < 1200]
            if not icons:
                lines[t] = 'no strip button'
                continue
            n = icons[0]
            x, y = int(n['x'] + n['w'] / 2), int(n['y'] + n['h'] / 2)
            command([HERE / 'pointer-drag.sh', self.vm, '%d,%d' % (x, y), '--hover'], 90)
            time.sleep(1.5)
            live = self.tree_to('strip-%s.tree.jsonl' % t.lower())
            said = [v for _, v in self.titles(live) if len(v) > 40 and v not in json.dumps(self.titles(nodes))]
            lines[t] = said[:2] or [v for _, v in self.titles(live) if v.startswith(t + '.')][:1]
            self.shot('strip-%s.png' % t.lower())
        ev['lines'] = lines
        self.facts['strip'] = ev
        self.save()
        missing = [t for t, seen in ev['tools'].items() if not seen]
        if not ev['head'] or missing:
            raise StepFailed('the setup sheet lacks the strip head or tools %s | %s' % (missing, json.dumps(ev)[:800]))
        unexplained = [t for t, said in lines.items() if not isinstance(said, list) or not said]
        if unexplained:
            raise StepFailed('no line of its own under the strip for %s | %s' % (unexplained, json.dumps(lines)[:1200]))
        return ev

    def welcome(self):
        ev = {'card_head': self.text_on_screen("Let's get me up to speed."), 'start': self.present('Start onboarding')}
        self.shot('welcome.png')
        self.tree_to('welcome.tree.jsonl')
        if not ev['start']:
            raise StepFailed('no Start onboarding on the welcome card | ' + json.dumps(ev))
        self.press('Not now')
        time.sleep(3)
        self.shot('after-not-now.png')
        # The note comes once this Mac can turn speech into words (main.js maybeOfferTools: "Only once
        # voice is ready"); a first install downloads the speech model in the background from launch.
        ev['voice_ready'] = self.wait_log('voice: ready on this machine', 1200)
        end = time.monotonic() + 60
        while time.monotonic() < end and not self.text_on_screen('five tools came with me'):
            time.sleep(3)
        ev['note'] = self.text_on_screen('five tools came with me')
        ev['turn_on'] = self.present('Turn on RichScribe')
        ev['frames'] = self.both_themes('tools-card')
        self.tree_to('tools-card.tree.jsonl')
        if ev['turn_on']:
            self.press('Turn on RichScribe')
            time.sleep(3)
            ev['asks_key'] = self.text_on_screen('Which key starts it?')
            ev['asks_mic'] = self.text_on_screen('Microphone')
            ev['asks_ax'] = self.text_on_screen('Accessibility')
            self.shot('turn-on-richscribe.png')
            self.tree_to('turn-on-richscribe.tree.jsonl')
        self.facts['welcome'] = ev
        self.save()
        if not (ev['note'] and ev['turn_on'] and ev.get('asks_key')):
            raise StepFailed('the tools card after Not now is not as the notes say | ' + json.dumps(ev))
        return ev

    # --- RichFiles ---------------------------------------------------------------------------------
    def richfiles_open(self):
        self.ax('click', '--id', 'nr-richfiles', '--first')
        time.sleep(4)

    def richfiles(self):
        self.g('mkdir -p ~/Acme/Docs ~/Acme/Docs-copy && printf "one\\n" > ~/Acme/Docs/a.txt && cp ~/Acme/Docs/a.txt '
               '~/Acme/Docs-copy/a.txt && printf "two\\n" > ~/Acme/Docs-copy/b-only-here.txt')
        rail = self.nodes('--id', 'nr-richfiles')
        ev = {'rail_item': [{k: n.get(k) for k in ('title', 'desc', 'x', 'y', 'w', 'h')} for n in rail[:1]]}
        if not rail:
            self.tree_to('richfiles-no-rail.tree.jsonl')
            raise StepFailed('no RichFiles item on the rail')
        self.richfiles_open()
        nodes = self.tree_to('richfiles.tree.jsonl')
        ev['frames'] = self.both_themes('richfiles')
        names = self.titles(nodes)
        ev['page_title'] = self.text_on_screen('RichFiles')
        ev['buttons'] = sorted({v for r, v in names if r in ('AXButton', 'AXMenuButton', 'AXPopUpButton') and v})[:80]
        ev['error'] = [n.get('value') for n in self.nodes('--id', 'richfiles-pane-error')]
        tools = {}
        tab = self.ax('click', '--title', 'New Tab', '--role', 'AXButton', '--first')
        time.sleep(2)
        tools['New Tab'] = {'pressed': bool(tab), 'tabs': [v for r, v in self.titles(self.tree_to('richfiles-new-tab.tree.jsonl'))
                                                           if r == 'AXButton' and v in ('admin', 'Home')]}
        self.shot('richfiles-new-tab.png')
        for label in ('Compare Folders', 'Find Duplicate Files', 'Disk Usage'):
            slug = label.lower().replace(' ', '-')
            try:
                self.ax('click', '--title', 'Tools', '--role', 'AXPopUpButton', '--first')
                time.sleep(1.5)
                self.tree_to('richfiles-tools-menu.tree.jsonl')
                self.ax('click', '--title', label, '--role', 'AXMenuItem', '--first', '--contains')
                time.sleep(4)
                # Each sheet in both themes: its hints (Disk Usage's footnote) are measured in both.
                frames = self.both_themes('richfiles-%s' % slug)
                after = self.tree_to('richfiles-%s.tree.jsonl' % slug)
                tools[label] = {'pressed': True, 'frames': frames,
                                'windows': [v for r, v in self.titles(after) if r in ('AXWindow', 'AXSheet')],
                                'texts': [v for r, v in self.titles(after) if r == 'AXStaticText' and v][:25]}
                self.esc()
                self.esc()
            except StepFailed as exc:
                tools[label] = {'pressed': False, 'why': str(exc)[-300:]}
                self.esc()
        ev['tools'] = tools
        # Undock, then back in the rail
        try:
            self.richfiles_open()
            self.ax('click', '--title', 'Undock', '--role', 'AXButton', '--first')
            time.sleep(6)
            procs = self.g("pgrep -fl 'RichFiles.app/Contents/MacOS' || true")
            self.shot('richfiles-undocked.png')
            windows = self.g('osascript -e \'tell application "System Events" to get name of every window of '
                             '(every process whose name contains "RichFiles")\' 2>&1 || true')
            ev['undock'] = {'process': procs.strip(), 'windows': windows.strip()}
            try:
                self.ax('click', '--title', 'Dock in RichOS', '--first', '--contains', app='RichFiles')
                time.sleep(5)
                ev['undock']['docked_again'] = True
            except StepFailed as exc:
                ev['undock']['docked_again'] = 'no Dock in RichOS: ' + str(exc)[-200:]
            self.shot('richfiles-docked-again.png')
        except StepFailed as exc:
            ev['undock'] = {'error': str(exc)[-400:]}
        self.facts['richfiles'] = ev
        self.save()
        self.ax('click', '--title', 'Chats', '--role', 'AXButton', '--first')
        time.sleep(2)
        problems = []
        if ev['error'] and any(ev['error']):
            problems.append('the page shows an error: %s' % ev['error'])
        unopened = [k for k, v in tools.items() if not v['pressed']]
        if unopened:
            problems.append('could not open: %s' % ', '.join(unopened))
        if not (ev['undock'].get('process')):
            problems.append('Undock RichFiles started no RichFiles app')
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:2500])
        return ev

    def output_file(self):
        rec = self.ask('Please write a short file called price-notes.md in the Acme folder with three bullet points '
                       'about wholesale tea pricing, and attach it here so I can open it.')
        time.sleep(4)
        self.shot('output-file-reply.png')
        ev = {'reply': rec['reply'][-600:], 'state': rec['state'], 'file': self.g('ls -l %s/price-notes.md 2>&1 || true' % shlex.quote(self.company))}
        # The Output panel at its list: closed first when an earlier step left it open on a file
        # (the button beside the message box toggles it).
        if self.nodes('--title', 'Close the output panel', '--role', 'AXButton'):
            self.ax('click', '--title', 'Close the output panel', '--role', 'AXButton', '--first')
            time.sleep(2)
        self.ax('click', '--id', 'out-bottom', '--first')
        time.sleep(3)
        self.shot('output-panel.png')
        nodes = self.tree_to('output-panel.tree.jsonl')
        menus = [n for n in nodes if n.get('role') == 'AXPopUpButton'
                 and (n.get('title') or n.get('desc') or '').startswith('More actions for price-notes')]
        ev['row_menu'] = self.titles(menus)
        if menus:
            self.ax('click', '--title', menus[0].get('title') or menus[0].get('desc'), '--role', 'AXPopUpButton', '--first')
            time.sleep(2)
        self.shot('output-menu.png')
        self.tree_to('output-menu.tree.jsonl')
        ev['menu_has_show_in_richfiles'] = self.text_on_screen('Show in RichFiles')
        if ev['menu_has_show_in_richfiles']:
            self.ax('click', '--title', 'Show in RichFiles', '--first', '--contains')
            time.sleep(5)
            page = self.tree_to('show-in-richfiles.tree.jsonl')
            self.shot('show-in-richfiles.png')
            ev['richfiles_page_shows_file'] = any('price-notes.md' in v for _, v in self.titles(page))
            ev['page_open'] = not any(n.get('hidden') for n in self.nodes('--id', 'richfiles-page'))
        self.facts['output_file'] = ev
        self.save()
        if not ev['menu_has_show_in_richfiles'] or not ev.get('richfiles_page_shows_file'):
            raise StepFailed('Show in RichFiles did not take me to the file | ' + json.dumps(ev)[:1500])
        return ev

    def pull_app_log(self):
        """The app's log (and a relaunch's) into --out, whatever happened (best effort)."""
        for remote in sorted({self.payload + '/app.log', self.applog}):
            subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', remote, str(self.out / Path(remote).name)],
                           capture_output=True, text=True, timeout=120)

    def pointer_right(self, n):
        x, y = int(n['x'] + n['w'] / 2), int(n['y'] + n['h'] / 2)
        command([HERE / 'pointer-drag.sh', self.vm, '%d,%d' % (x, y), '%d,%d' % (x, y), '--right'], 90)
        time.sleep(2)

    # --- media moments -----------------------------------------------------------------------------
    def moment_card(self, stem):
        nodes = self.tree_to(stem + '.tree.jsonl')
        moments = [n for n in nodes if n.get('role') == 'AXButton' and any(
            (n.get('title') or n.get('desc') or '').startswith(p) for p in ('0:', '1:', '2:', '00:'))]
        return nodes, moments

    def play_moment(self, stem, moments):
        m = moments[0]
        self.click_at(m['x'] + m['w'] / 2, m['y'] + m['h'] / 2)
        time.sleep(6)
        self.shot(stem + '-pressed.png')
        return self.tree_to(stem + '-pressed.tree.jsonl')

    def moments_local(self):
        self.g('mkdir -p ' + shlex.quote(self.home + '/Downloads'))
        command([HERE / 'guest.sh', self.vm, '--push', str(self.a.meeting), self.home + '/Downloads/pricing-meeting.wav'], 300)
        # What a returning user has: the speech models on disk when the app starts. A first install
        # downloads them in the background; Rich's session is started with RICHOS_SPEECH_MODEL only when
        # the model is there (video-watch-walk.py's speech-env), so the app is relaunched once they are.
        models = self.home + '/.config/richos/models'
        ev0 = {'models_before': self.g('ls -l %s 2>&1 || true' % shlex.quote(models)).strip()[-600:],
               'video_tools_line': self.g("grep -m1 'video tools are installed\\|video tools in the background' "
                                          + shlex.quote(self.applog) + ' || true').strip()}
        if self.a.model and self.g('test -e %s/%s && echo yes || echo no' % (shlex.quote(models), Path(self.a.model).name)).strip() != 'yes':
            self.g('mkdir -p ' + shlex.quote(models))
            command([HERE / 'guest.sh', self.vm, '--push', str(self.a.model), models + '/' + Path(self.a.model).name], 300)
        if self.a.relaunch_for_moments:
            from relaunch import relaunch as relaunch_app
            self.applog = relaunch_app(self.vm)['log']
            ev0['relaunched_log'] = self.applog
            time.sleep(20)
            ev0['back_in_conversation'] = self.to_conversation()
        ev0['lease_env'] = self.g("ps -E -ax -o command 2>/dev/null | grep -o 'RICHOS_SPEECH_MODEL=[^ ]*' | sort | uniq -c || true").strip()
        rec = self.ask('In ~/Downloads/pricing-meeting.wav, where do they talk about the Seattle Coffee Fest booth? '
                       'Show me the moments so I can play them.', within=1200)
        time.sleep(4)
        self.shot('moments-local.png')
        nodes, moments = self.moment_card('moments-local')
        ev = {'reply': rec['reply'][-800:], 'state': rec['state'], 'seconds': rec['seconds'],
              'moments': self.titles(moments), **ev0}
        if moments:
            after = self.play_moment('moments-local', moments)
            names = self.titles(after)
            ev['output_panel_media'] = [v for r, v in names if 'pricing-meeting' in v][:6]
            ev['playing'] = any(v in ('Ⅱ', 'Pause') or 'Pause' in v for _, v in names)
        self.facts['moments_local'] = ev
        self.save()
        if not moments:
            raise StepFailed('no moments card in the answer | ' + json.dumps(ev)[:1500])
        if not ev.get('output_panel_media'):
            raise StepFailed('pressing a moment showed no recording in the Output panel | ' + json.dumps(ev)[:1500])
        return ev

    def moments_youtube(self):
        rec = self.ask('Watch https://www.youtube.com/watch?v=jNQXAC9IVRw and show me the moments where he talks about '
                       "the elephants' trunks.", within=1500)
        time.sleep(4)
        self.shot('moments-youtube.png')
        nodes, moments = self.moment_card('moments-youtube')
        saved = self.g('ls -la %s 2>&1 || true' % shlex.quote(self.home + '/Downloads/RichOS Videos'))
        ev = {'reply': rec['reply'][-800:], 'state': rec['state'], 'seconds': rec['seconds'],
              'moments': self.titles(moments), 'saved': saved.strip()[-600:],
              'youtube_kind': any('YouTube' in v for _, v in self.titles(nodes))}
        if moments:
            after = self.play_moment('moments-youtube', moments)
            ev['after_press'] = [v for _, v in self.titles(after) if 'youtube' in v.lower() or 'zoo' in v.lower()][:8]
        self.facts['moments_youtube'] = ev
        self.save()
        problems = []
        if not moments:
            problems.append('no moments card in the answer')
        if 'No such file' in saved or not saved.strip():
            problems.append('nothing saved in ~/Downloads/RichOS Videos')
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:1500])
        return ev

    # --- the disk ----------------------------------------------------------------------------------
    def disk_low(self):
        before = self.log_has('starting the cleanup')
        df = self.g("df -k ~ | tail -1 | awk '{print $2, $4}'").split()
        total_kb, free_kb = int(df[0]), int(df[1])
        target_free_kb = int(total_kb * 0.10)
        fill_kb = max(0, free_kb - target_free_kb)
        began = time.monotonic()
        self.g('dd if=/dev/zero of=/Users/admin/qa-disk-fill bs=1m count=%d 2>&1 | tail -1' % (fill_kb // 1024), 1800)
        after_df = self.g("df -k ~ | tail -1 | awk '{print $2, $4}'").strip()
        ev = {'before_kb': [total_kb, free_kb], 'after_kb': after_df, 'fill_seconds': round(time.monotonic() - began, 1)}
        try:
            got = self.wait_log('starting the cleanup', 16 * 60 + 30, more_than=before)
            ev['seconds_to_line'] = round(time.monotonic() - began, 1)
            time.sleep(20)
            ev['log'] = self.g("grep -n 'disk low\\|starting the cleanup\\|cleanup\\|applied:\\|verdict:' "
                               + shlex.quote(self.applog) + " | tail -12 || true")
        finally:
            self.g('rm -f /Users/admin/qa-disk-fill')
        ev['line_seen'] = got
        self.facts['disk_low'] = ev
        self.save()
        if not got:
            raise StepFailed('no "starting the cleanup" line within one 15-minute check | ' + json.dumps(ev)[:1500])
        return ev

    # --- meetings ----------------------------------------------------------------------------------
    def ask_live(self):
        lines = json.loads(Path(self.a.meeting_lines).read_text())['lines']
        booth = next(l for l in lines if 'Seattle Coffee Fest' in l['text'])
        command([HERE / 'guest.sh', self.vm, '--push', str(self.a.meeting), LABDIR + '/meeting.wav'], 300)
        self.panel_press('Take Notes')
        if not self.wait_log('StartCapture', 40):
            self.shot('no-recording.png')
            raise StepFailed('Take Notes pressed, no StartCapture in the log')
        shown = self.panel_says('Recording meeting', 40)
        self.shot('recording.png')
        self.g(f'nohup {LABDIR}/mac-audio-lab play {LABDIR}/meeting.wav --device "BlackHole 2ch" > {LABDIR}/near.log 2>&1 &')
        played = time.monotonic()
        wait = booth['end'] + self.a.ask_after - (time.monotonic() - played)
        time.sleep(max(0, wait))
        asked_at = round(time.monotonic() - played, 1)
        rec = self.ask('What did Priya just say about the trade show booth? Which hall is it in?', within=300)
        answered_at = round(time.monotonic() - played, 1)
        self.shot('ask-live.png')
        still = self.capture_pids()
        ev = {'recording_shown': shown, 'booth_line': booth, 'asked_at_s': asked_at, 'answered_at_s': answered_at,
              'reply': rec['reply'][-800:], 'state': rec['state'], 'capture_running_after_answer': still,
              'reads': self.g("grep -c 'live_meeting_excerpt\\|live meeting' " + shlex.quote(self.applog) + ' || true').strip()}
        rest = json.loads(Path(self.a.meeting_lines).read_text())['seconds'] + 3 - (time.monotonic() - played)
        time.sleep(max(0, rest))
        self.facts['ask_live'] = ev
        self.save()
        reply = rec['reply'].lower()
        if rec['state'] != 'completed' or 'hall b' not in reply:
            raise StepFailed('Rich did not answer from the call in progress (no "hall B") | ' + json.dumps(ev)[:1500])
        if not still:
            raise StepFailed('the recording was no longer running when Rich answered | ' + json.dumps(ev)[:800])
        return ev

    def stop_notes(self):
        self.panel_press('Stop')
        stopped = time.monotonic()
        if not self.wait_log('StopCapture', 40):
            raise StepFailed('Stop pressed, no StopCapture in the log')
        self.stop_mic()
        if not self.wait_log('Rich said the notes', 900):
            self.shot('no-notes.png')
            raise StepFailed('Rich never posted the notes (no "Rich said the notes" in the log)')
        ev = {'seconds_stop_to_notes': round(time.monotonic() - stopped, 1)}
        time.sleep(5)
        self.shot('notes.png')
        nodes = self.tree_to('notes.tree.jsonl')
        texts = [v for _, v in self.titles(nodes)]
        heads = [v.rstrip(':') for v in texts if v.rstrip(':') in ('Decisions', 'Your actions', 'Other commitments', 'Open questions', 'Context')]
        ev['headings_in_order'] = heads
        sources = [n for n in nodes if (n.get('title') or n.get('value') or n.get('desc') or '').startswith('Source ')]
        ev['source_links'] = self.titles(sources)[:12]
        ev['live_log'] = self.g("grep -i -n 'live transcri\\|live checkpoint\\|live pass\\|normaliz' " + shlex.quote(self.applog)
                                + ' | head -12 || true')
        session = self.sessions()
        if session:
            folder = session[0].rsplit('/', 1)[0]
            ev['session_files'] = self.g('ls -l ' + shlex.quote(folder))
            brief = self.g('cat ' + shlex.quote(folder) + '/brief.md 2>/dev/null | head -40 || true')
            ev['brief_head'] = brief[:1500]
        self.facts['stop_notes'] = ev
        self.save()
        if sources:
            # An accessibility press: the notes are taller than the window, so the link may sit
            # above the visible part (a coordinate click there lands off screen). Two different
            # links, each looked at 4 s and 12 s later: is its passage inside the panel's view?
            presses = []
            for src in sources[:6]:
                label = src.get('title') or src.get('desc') or src.get('value')
                if any(p['label'] == label for p in presses):
                    continue
                stamp = label.split(' ', 1)[1]
                self.ax('click', '--title', label, '--role', src.get('role') or 'AXButton', '--first')
                row = {'label': label}
                for wait, tag in ((4, 'a'), (8, 'b')):
                    time.sleep(wait)
                    name = 'source-%s-%s' % (stamp.replace(':', ''), tag)
                    self.shot(name + '.png')
                    after = self.tree_to(name + '.tree.jsonl')
                    panel = [n for n in after if n.get('x', 0) > 1140 and n.get('role') == 'AXStaticText']
                    shown = [n for n in panel if str(n.get('value') or '').startswith('[%s]' % stamp)]
                    row[tag] = {'stamp_rows_y': [n.get('y') for n in shown],
                                'in_view': any(190 <= (n.get('y') or -1) <= 900 for n in shown),
                                'not_located': any('could not be located' in str(n.get('value') or '') for n in after)}
                presses.append(row)
                if len(presses) == 2:
                    break
            ev['presses'] = presses
            ev['pressed'] = presses[0]['label']
            ev['transcript_open'] = True
            ev['not_located'] = any(r[t]['not_located'] for r in presses for t in ('a', 'b'))
            ev['passage_in_view'] = all(r['b']['in_view'] for r in presses)
        self.facts['stop_notes'] = ev
        self.save()
        problems = []
        if not heads or heads[0] != 'Decisions':
            problems.append('the notes do not start with Decisions: %s' % heads)
        if sources and not ev.get('passage_in_view'):
            problems.append('a Source link opened the transcript away from its passage: %s' % ev.get('presses'))
        if not sources:
            problems.append('no Source link in the notes')
        elif not ev.get('transcript_open') or ev.get('not_located'):
            problems.append('pressing a Source link did not open the transcript at its passage')
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:2500])
        return ev

    # --- RichScribe and RichCleaner ---------------------------------------------------------------
    def scribe(self):
        """dictation-walk.py's own Walk on this guest, with the long sample and More accurate."""
        if not hasattr(self, '_scribe'):
            a = argparse.Namespace(**vars(self.a))
            a.wav, a.model, a.model_id = self.a.dict_wav, self.a.model, 'large-v3-turbo-q5_0'
            self._scribe = dictation.DictationWalk(a)
        return self._scribe

    def scribe_stage(self):
        return dictation.DictationWalk.stage(self.scribe())

    def scribe_relaunch(self):
        s = self.scribe()
        pre = {}
        if self.a.scribe_faster:
            # Faster (small.en), the model the app downloads beside More accurate: this guest decodes
            # the large model at about 10 s per utterance, past the dictation's own decoder bound.
            models = self.home + '/.config/richos/models'
            end = time.monotonic() + 1200
            while time.monotonic() < end and self.g('test -s %s/ggml-small.en.bin && echo yes || echo no'
                                                    % shlex.quote(models)).strip() != 'yes':
                time.sleep(10)
            settings = self.data + '/dictation.json'
            pre['faster'] = self.g('python3 -c %s %s' % (shlex.quote(
                'import json,sys,os; p=sys.argv[1]; d=json.load(open(p)) if os.path.exists(p) else {}; '
                'd["model"]="small.en"; json.dump(d,open(p,"w")); print(d.get("model"))'), shlex.quote(settings)))
        ev = dictation.DictationWalk.relaunch(s)
        ev.update(pre)
        self.applog = s.log
        # dictation-walk.py's settle also decodes the sample once per model on the guest as its
        # reference; this walk judges against the words that were said, so it only waits for the
        # relaunched app's voice readiness (a 47 s sample took over 300 s to decode that way here).
        ev['voice_ready'] = self.wait_log('voice: ready on this machine', 900)
        if not ev['voice_ready']:
            raise StepFailed('the relaunched app never said voice is ready | ' + json.dumps(ev)[:800])
        ev['back_in_conversation'] = self.to_conversation()
        return ev

    def to_conversation(self):
        """A relaunched app opens on Home (no rail): Talk to Rich goes back into the conversation,
        and a first-run sheet it raises again is declined as on the first run."""
        time.sleep(5)
        steps = []
        if self.present('Talk to Rich'):
            self.press('Talk to Rich')
            steps.append('Talk to Rich')
            time.sleep(4)
        for _ in range(3):
            if self.present('Set it up'):
                self.press('Not now')
                steps.append('Not now')
                time.sleep(3)
        self.shot('back-in-conversation.png')
        return steps

    def preview_switch(self):
        """The RichScribe sheet from the rail, More options, Live preview: its switch, turned on."""
        try:
            self.ax('click', '--id', 'nr-scribe', '--first')
        except StepFailed:
            self.press('RichScribe')
        time.sleep(3)
        self.shot('scribe-sheet.png')
        try:
            self.press('More options')
            time.sleep(2)
        except StepFailed:
            pass
        nodes = self.tree_to('scribe-more-options.tree.jsonl')
        sw = [n for n in nodes if (n.get('desc') == 'Live preview' or n.get('title') == 'Live preview')
              and n.get('role') in ('AXCheckBox', 'AXSwitch', 'AXButton')]
        before = sw[0].get('value') if sw else None
        if sw and str(before) not in ('1', 'true', 'True'):
            self.click_at(sw[0]['x'] + sw[0]['w'] / 2, sw[0]['y'] + sw[0]['h'] / 2)
            time.sleep(2)
        after = [n.get('value') for n in self.nodes('--title', 'Live preview', '--role', sw[0]['role'])] if sw else None
        self.shot('scribe-live-preview.png')
        self.esc()
        return {'switch_found': bool(sw), 'before': before, 'after': after}

    def press_and_reply(self, title, within=300):
        sent = self.clock()
        self.wait_for(title, seconds=60)
        self.press(title)
        end = time.monotonic() + within
        tid = None
        while time.monotonic() < end and not tid:
            tid = convo.find_turn(self.rows(), 'press', title, sent)
            time.sleep(2)
        while tid and time.monotonic() < end and convo.turn_record(self.rows(), tid)['state'] == 'open':
            time.sleep(3)
        return convo.turn_record(self.rows(), tid) if tid else {'state': 'none', 'reply': ''}

    def wait_rich_back(self, after_ms, seconds):
        """Rich's next turn of his own (a delivered prompt, no message of the user's) after AFTER_MS,
        waited for until it closes. None when none started within SECONDS."""
        end = time.monotonic() + seconds
        tid = None
        while time.monotonic() < end and not tid:
            tid = convo.find_turn(self.rows(), 'wait_for_rich', '', after_ms)
            if not tid:
                time.sleep(5)
        if not tid:
            return None
        while time.monotonic() < end + 600 and convo.turn_record(self.rows(), tid)['state'] == 'open':
            time.sleep(3)
        return dict(convo.turn_record(self.rows(), tid), turn=tid)

    def file_cards(self):
        """The file cards in the conversation on screen: buttons that open a file Rich attached."""
        return [v for r, v in self.titles(self.nodes('--role', 'AXButton')) if v.startswith('Open ')]

    def onboarding(self):
        """The onboarding as a new user does it: Start onboarding, the four quick things (the website
        cannot be reached; the profile is the stand-in LinkedIn when --site is given), Rich back by
        himself with his read and questions, the answers, and the finished piece of work in a file."""
        profile = ''
        if self.a.site:
            self.a.no_accessibility = False
            profile = convo.ConversationWalk.prepare_site(self)
        ev = {'start': self.press_and_reply('Start onboarding')}
        ev['start']['reply'] = ev['start']['reply'][-800:]
        self.shot('onb-1-asks.png')
        four = 'Mona Wells, she/her. Website: https://www.tidewater-tea-qa97.invalid' + (' LinkedIn: ' + profile if profile else '')
        r = self.ask(four, within=600)
        ev['four_things'] = {'state': r['state'], 'reply': r['reply'][-800:]}
        self.shot('onb-2-reading.png')
        back = self.wait_rich_back(r['closed_at_ms'] or self.clock(), 1800)
        if not back:
            raise StepFailed('Rich never came back by himself after the four things | ' + json.dumps(ev)[:1500])
        ev['came_back'] = {'state': back['state'], 'reply': back['reply'][-2500:]}
        self.shot('onb-3-back.png')
        answers = ('1. There is no website yet, skip it. 2. Two people report to me: Sam runs the warehouse and '
                   'shipping, Priya runs cafe accounts. 3. We use Shopify for orders, Gmail for email and a Google '
                   'Sheet for customers. 4. My priority this quarter is 30 more cafes on a monthly standing order; it '
                   'gets stuck because cafes forget to reorder. 5. Take the monthly reorder reminder emails to cafes '
                   'off my plate. 6. Never spend more than $500 or email a customer without asking me. 7. Short '
                   'updates every Monday morning.')
        r = self.ask(answers, within=900)
        ev['answers'] = {'state': r['state'], 'reply': r['reply'][-1500:]}
        self.shot('onb-4-answered.png')
        work = None
        last = r['closed_at_ms'] or self.clock()
        if '?' in r['reply'][-400:] and not self.file_cards():
            r2 = self.ask('Skip the rest of the questions, please go ahead with what you have.', within=900)
            ev['skip'] = {'state': r2['state'], 'reply': r2['reply'][-1000:]}
            last = r2['closed_at_ms'] or self.clock()
        if not self.file_cards():
            work = self.wait_rich_back(last, 2400)
            ev['work'] = work and {'state': work['state'], 'reply': work['reply'][-1500:]}
        time.sleep(5)
        self.shot('onb-5-work.png')
        self.tree_to('onb-5-work.tree.jsonl')
        ev['file_cards'] = self.file_cards()
        ev['person'] = self.g('cat %s 2>/dev/null | head -30 || true' % shlex.quote(self.home + '/myrichos/person.md'))[:1200]
        self.facts['onboarding'] = ev
        self.save()
        if not ev['file_cards']:
            raise StepFailed('the onboarding did not end with a file I can open | ' + json.dumps(ev)[:2500])
        return ev

    def second_company(self):
        """A second company: added from "+ New chat or company", New company; its onboarding asks only
        for what is new (its website), not the name, pronouns or profiles given for the first."""
        self.g('mkdir -p ' + shlex.quote(self.home + '/Harbor'))
        self.press('New chat or company', role='AXPopUpButton')
        time.sleep(2)
        self.tree_to('new-chat-menu.tree.jsonl')
        self.ax('click', '--title', 'New company', '--role', 'AXMenuItem', '--first')
        time.sleep(2)
        self.type_into('Harbor Lane', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.home + '/Harbor', '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        time.sleep(5)
        self.shot('second-company.png')
        ev = {'card': self.present('Start onboarding')}
        self.tree_to('second-company.tree.jsonl')
        if ev['card']:
            first = self.press_and_reply('Start onboarding')
        else:
            # No welcome card in the new company's chat (the first company's onboarding is still
            # under way): the user says it, as the skill's "otherwise said yes" allows.
            first = self.ask("Let's do the onboarding for Harbor Lane.", within=600)
        ev['asks'] = first['reply'][-1200:]
        self.shot('second-company-asks.png')
        self.facts['second_company'] = ev
        self.save()
        said = first['reply'].lower()
        again = [w for w in ('your gender', '1. your name', 'links to your public', 'how should i refer') if w in said]
        if 'website' not in said or again:
            raise StepFailed('the second company did not ask only for what is new (asked again: %s) | %s'
                             % (again, json.dumps(ev)[:1500]))
        return ev

    def talk_now(self):
        """Onboarding with RichScribe working: when Rich comes back by himself with his questions, the
        line above the message box says "Or just say it: press F1 and talk." with Talk now; pressing it
        starts RichScribe in the message box (the sample stands in for the voice)."""
        s = self.scribe()
        if not self.present('Start onboarding'):
            # Back from Home, Talk to Rich opens a new chat: the welcome card is in the first one.
            self.ax('click', '--title', 'Onboarding', '--first')
            time.sleep(3)
        ev = {'start': self.press_and_reply('Start onboarding')['state']}
        sent = self.clock()
        first = self.ask('Mona Wells, she/her. Website: https://www.tidewater-tea-qa97.invalid', within=600)
        ev['four_things'] = {'state': first['state'], 'reply': first['reply'][-600:]}
        end = time.monotonic() + 1800
        back = None
        while time.monotonic() < end and not back:
            back = convo.find_turn(self.rows(), 'wait_for_rich', '', first['closed_at_ms'] or sent)
            time.sleep(5)
        if not back:
            raise StepFailed('Rich did not come back by himself within 30 minutes | ' + json.dumps(ev)[:800])
        while time.monotonic() < end and convo.turn_record(self.rows(), back)['state'] == 'open':
            time.sleep(3)
        ev['came_back'] = convo.turn_record(self.rows(), back)['reply'][-1200:]
        time.sleep(5)
        self.shot('talk-now.png')
        self.tree_to('talk-now.tree.jsonl')
        ev['hint'] = self.text_on_screen('Or just say it')
        ev['talk_now'] = bool(self.nodes('--id', 'voice-hint-talk'))
        if ev['talk_now']:
            delivered = s.samples_delivered()
            self.ax('click', '--id', 'voice-hint-talk', '--first')
            t0 = time.monotonic()
            time.sleep(4)
            self.shot('talk-now-listening.png')
            while s.samples_delivered() <= delivered and time.monotonic() - t0 < 120:
                time.sleep(0.5)
            before_lines = len(s.dictated_lines())
            s.press('122')
            # Nothing is touched until the dictation has written (its own outcome line): a press or
            # a typed key in between would move the focus the words are delivered to.
            end = time.monotonic() + dictation.DICTATION_WITHIN
            while time.monotonic() < end and len(s.dictated_lines()) <= before_lines:
                time.sleep(2)
            ev['dictation_log'] = (s.dictated_lines()[before_lines:] or [None])[0]
            time.sleep(2)
            self.shot('talk-now-written.png')
            box = self.nodes('--role', 'AXTextArea', '--title', 'Message to Rich')
            ev['composer'] = (box[0].get('value') or '')[:400] if box else None
            self.type_into(' ', '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.facts['talk_now'] = ev
        self.save()
        if not (ev['hint'] and ev['talk_now']):
            raise StepFailed('no "Or just say it" line with Talk now when Rich came back | ' + json.dumps(ev)[:1500])
        if not ev.get('composer') or len(ev['composer'].split()) < 5:
            raise StepFailed('Talk now did not put the spoken words in the message box | ' + json.dumps(ev)[:1500])
        return ev

    def scribe_long(self):
        s = self.scribe()
        lines = json.loads(Path(self.a.dict_lines).read_text())
        ev = {'preview': self.preview_switch()}
        self.g(': > /tmp/n97-long.txt && open -a TextEdit /tmp/n97-long.txt')
        time.sleep(3)
        if s.front_bundle() != 'com.apple.TextEdit':
            raise StepFailed('TextEdit is not in front: ' + s.front_bundle())
        before_lines = len(s.dictated_lines())
        delivered_before = s.samples_delivered()
        s.press('122')
        tapped = time.monotonic()
        frames = []
        while s.samples_delivered() <= delivered_before:
            at = round(time.monotonic() - tapped, 1)
            if at > lines['seconds'] * 2 + 60:
                raise StepFailed('the long sample was not delivered in time')
            name = 'preview-%05.1f.png' % at
            try:
                self.shot(name)
                frames.append({'at_s': at, 'frame': name})
            except StepFailed:
                pass
        delivered = round(time.monotonic() - tapped, 1)
        s.press('122')
        end = time.monotonic() + dictation.DICTATION_WITHIN
        line = None
        while time.monotonic() < end and line is None:
            got = s.dictated_lines()
            if len(got) > before_lines:
                line = got[-1]
            else:
                time.sleep(1)
        time.sleep(2)
        # The whole text, read through System Events (its own grant): an ax.sh node carries a value
        # cut short, which would read as words the dictation dropped.
        text = self.script('tell application "System Events" to tell process "TextEdit" to get value of '
                           'text area 1 of scroll area 1 of window 1')
        self.shot('scribe-long-textedit.png')
        script = ' '.join(l['text'] for l in lines['lines'])
        drop, extra = word_gaps(script, text)
        ev.update({'frames': frames, 'sample_delivered_after_s': delivered, 'log': line, 'text': text,
                   'script': script, 'longest_dropped_run': drop, 'trailing_extra_words': extra,
                   'raw_whisper_reference': s.facts.get('expected', {}).get('large-v3-turbo-q5_0')})
        self.facts['scribe_long'] = ev
        self.save()
        problems = []
        if not line or 'pasted' not in line:
            problems.append('the dictation did not paste: %s' % line)
        if len(drop) >= 3:
            problems.append('%d spoken words in a row are missing: %s' % (len(drop), ' '.join(drop)))
        if extra:
            problems.append('words nobody said at the end: %s' % ' '.join(extra))
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:2000])
        return ev

    def cleaner(self):
        s = self.scribe()
        ev = {}
        # RichCleaner on, from Settings (resources-cleaner-accounts-walk.py's row and switch)
        self.press('Settings', role='AXPopUpButton')
        time.sleep(2)
        self.ax('click', '--id', 'set-cleaner-open', '--first')
        time.sleep(2)
        nodes = self.tree_to('cleaner-sheet.tree.jsonl')
        # The sheet's switch, never the rail's RichCleaner item (a button at the window's left edge).
        sw = [n for n in nodes if (n.get('desc') == 'RichCleaner' or n.get('title') == 'RichCleaner')
              and n.get('role') in ('AXCheckBox', 'AXSwitch', 'AXButton', 'AXRadioButton') and n.get('x', 0) > 200]
        if not sw:
            raise StepFailed('no RichCleaner switch in its sheet')
        if str(sw[0].get('value')) not in ('1', 'true', 'True'):
            # A hand's click: the real mouse at the switch's center (resources-cleaner-accounts-walk.py)
            x, y = int(sw[0]['x'] + sw[0]['w'] / 2), int(sw[0]['y'] + sw[0]['h'] / 2)
            command([HERE / 'pointer-drag.sh', self.vm, '%d,%d' % (x, y), '%d,%d' % (x, y)], 90)
            time.sleep(4)
        self.shot('cleaner-on.png')
        ev['sheet'] = self.titles(self.tree_to('cleaner-on.tree.jsonl'))[:60]
        self.esc()
        self.esc()
        # The filing root is the user's Documents/RichCleaner as macOS names it (cleaner_host.rs); the
        # guest's app has a fixture HOME, so the same folders are made in both places it can be.
        roots = [self.home + '/Documents/RichCleaner', '/Users/admin/Documents/RichCleaner']
        for root in roots:
            self.g('mkdir -p {r}/Pricing {r}/Travel {r}/Receipts && printf "old\\n" > {r}/Pricing/2025-price-list.txt && '
                   'printf "trip\\n" > {r}/Travel/portland.txt'.format(r=shlex.quote(root)))
        self.g('printf "a\\n" > ~/Desktop/cafe-price-list-draft-1.txt && printf "b\\n" > ~/Desktop/cafe-price-list-draft-2.txt')
        command([HERE / 'guest.sh', self.vm, '--push', str(self.a.note_wav), s.wav], 120)
        # Select both as a hand does, on the Desktop itself (RichCleaner reads the Desktop's own
        # selection, never a Finder window's: that is refused with "Select a file on your Desktop").
        # Asking Finder over Apple Events waits on an automation prompt this guest never answers.
        ev['desktop_before'] = self.g('ls -A ~/Desktop')
        # The real mouse draws a selection box on the Desktop around both icons (they sit at its top
        # right). A click on the wallpaper and Command-A selects nothing on this macOS (it shows the
        # desktop instead, walk-bd70813a929d).
        command([HERE / 'pointer-drag.sh', self.vm, '1545,32', '1600,150', '1675,268'], 90)
        time.sleep(2)
        ev['front'] = s.front_bundle()
        self.shot('cleaner-selected.png')
        logged = self.g('grep -c "RichCleaner filing" ' + shlex.quote(s.dlog) + ' 2>/dev/null || true').strip()
        delivered_before = s.samples_delivered()
        s.press('122')
        t0 = time.monotonic()
        while s.samples_delivered() <= delivered_before and time.monotonic() - t0 < 90:
            time.sleep(0.3)
        s.press('122')
        end = time.monotonic() + 300
        while time.monotonic() < end:
            desk = self.g('ls ~/Desktop')
            if 'cafe-price-list-draft' not in desk:
                break
            time.sleep(3)
        time.sleep(5)
        self.shot('cleaner-filed.png')
        ev['desktop_after'] = self.g('ls ~/Desktop')
        ev['root_after'] = '\n'.join(self.g('cd {r} 2>/dev/null && find . -type f | sort || true'.format(r=shlex.quote(r)))
                                     for r in roots)
        ev['settings_root'] = self.g('grep -rho \'"[^"]*RichCleaner[^"]*"\' %s/*.json 2>/dev/null | head -3 || true'
                                     % shlex.quote(self.data))
        ev['log'] = self.g('grep -i "RichCleaner" ' + shlex.quote(s.dlog) + ' 2>/dev/null | tail -12 || true')[-2500:]
        ev['filing_lines_before'] = logged
        # The RichCleaner window: each original with its Undo
        try:
            self.ax('click', '--id', 'nr-cleaner', '--first')
            time.sleep(3)
            win = self.tree_to('cleaner-window.tree.jsonl')
            self.shot('cleaner-window.png')
            ev['undo_buttons'] = len([n for n in win if (n.get('title') or n.get('desc') or '').startswith('Undo')])
            ev['window_names'] = [v for _, v in self.titles(win) if 'cafe-price-list' in v][:6]
            self.esc()
        except StepFailed as exc:
            ev['window'] = str(exc)[-300:]
        self.facts['cleaner'] = ev
        self.save()
        # The wallpaper click put the windows aside: RichOS back in front, then its conversation.
        self.script('tell application "System Events" to set frontmost of (first process whose name is "RichOS") to true')
        time.sleep(2)
        self.ax('click', '--title', 'Chats', '--role', 'AXButton', '--first')
        time.sleep(2)
        rec = self.ask('Where are the cafe price list drafts I filed with RichCleaner?', within=600)
        ev['rich'] = {'reply': rec['reply'][-1000:], 'state': rec['state']}
        self.facts['cleaner'] = ev
        self.save()
        filed = [l for l in ev['root_after'].splitlines() if 'cafe-price-list-draft' in l]
        folders_used = sorted({l.rsplit('/', 1)[0] for l in filed})
        ev['folders_used'] = folders_used
        problems = []
        if 'cafe-price-list-draft' in ev['desktop_after']:
            problems.append('the selected files are still on the Desktop')
        if len(filed) != 2 or len(folders_used) != 1:
            problems.append('the two files are not together in one folder: %s' % filed)
        elif not folders_used[0].startswith('./Pricing'):
            problems.append('a new folder was made (%s) though "Pricing" fits' % folders_used[0])
        if (ev.get('undo_buttons') or 0) < 2:
            problems.append('the RichCleaner window does not show an Undo for each file')
        if 'cafe' not in ev['rich']['reply'].lower() and 'pricing' not in ev['rich']['reply'].lower():
            problems.append('Rich did not find the set again')
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:2500])
        return ev

    def history(self):
        ev = {}
        rec = self.ask('Draft the follow-up email for that pricing meeting.', within=900)
        ev['email'] = {'reply': rec['reply'][-1500:], 'state': rec['state'], 'seconds': rec['seconds']}
        self.shot('history-email.png')
        rec = self.ask('Across my meetings, who owes what?', within=900)
        ev['owes'] = {'reply': rec['reply'][-1500:], 'state': rec['state'], 'seconds': rec['seconds']}
        if not any(m in rec['reply'] for m in ('Nov', time.strftime('%Y'))):
            # Handed to the team: the answer is Rich's next turn of his own.
            back = self.wait_rich_back(rec['closed_at_ms'] or self.clock(), 900)
            if back:
                ev['owes'] = {'reply': back['reply'][-2500:], 'state': back['state'], 'seconds': None,
                              'first_reply': rec['reply'][-600:]}
        self.shot('history-owes.png')
        self.facts['history'] = ev
        self.save()
        today = time.strftime('%Y')
        problems = []
        for k, r in ev.items():
            if r['state'] != 'completed':
                problems.append('%s: the turn ended %s' % (k, r['state']))
        if 'price list' not in ev['owes']['reply'].lower() and 'sam' not in ev['owes']['reply'].lower():
            problems.append('who owes what does not name Sam or the price list')
        if not any(m in ev['owes']['reply'] for m in ('Nov', today, 'today')):
            problems.append('who owes what names no meeting date')
        if problems:
            raise StepFailed('; '.join(problems) + ' | ' + json.dumps(ev)[:2500])
        return ev


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--model', type=Path, help='ggml-large-v3-turbo-q5_0.bin (audio, moments-local)')
    p.add_argument('--meeting', type=Path, help='the meeting fixture WAV (qa/speech-wav.py)')
    p.add_argument('--meeting-lines', type=Path, help="qa/speech-wav.py's JSON for --meeting")
    p.add_argument('--dict-wav', type=Path, help='a long spoken sample with a pause, 16 kHz (qa/speech-wav.py --rate 16000)')
    p.add_argument('--dict-lines', type=Path, help="qa/speech-wav.py's JSON for --dict-wav: the words that were said")
    p.add_argument('--note-wav', type=Path, help='the spoken RichCleaner note, 16 kHz')
    p.add_argument('--ask-after', type=float, default=20, help='ask-live: seconds after the booth line ends to ask (default 20)')
    p.add_argument('--site', type=Path, help="onboarding: linkedin-standin.py's stand-in site, served in the guest")
    p.add_argument('--relaunch-for-moments', action='store_true', help='moments-local: relaunch the app once the '
                   'speech models are on disk (a returning user), instead of asking in the session that started without them')
    p.add_argument('--scribe-faster', action='store_true', help='scribe-relaunch: dictate with Faster (small.en), '
                   'written to dictation.json before the relaunch, once the app has downloaded it')
    p.add_argument('--post-key', type=Path, help="dictation-walk.py's post_key example")
    p.add_argument('--key-probe', type=Path, help="dictation-walk.py's key_probe example")
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in STEPS]
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))
    if any(s in steps for s in ('audio', 'scribe-stage')) and not a.model:
        p.error('--model is needed for the audio and scribe-stage steps')
    if 'scribe-stage' in steps and not (a.dict_wav and a.dict_lines and a.post_key and a.key_probe):
        p.error('--dict-wav, --dict-lines, --post-key and --key-probe are needed for scribe-stage')
    if 'cleaner' in steps and not a.note_wav:
        p.error('--note-wav is needed for the cleaner step')
    if any(s in steps for s in ('ask-live', 'moments-local')) and not (a.meeting and a.meeting_lines):
        p.error('--meeting and --meeting-lines are needed for ask-live and moments-local')
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)[:3000]
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if row['outcome'] != 'PASS' and step in ('identity', 'first-run', 'audio', 'scribe-stage', 'scribe-relaunch'):
            break
    walk.pull_app_log()
    if 'audio' in steps:
        try:
            walk.stop_mic()
        except Exception as exc:  # noqa: BLE001 - the guest is removed by run-walk.py either way
            print('stop_mic at the end: ' + str(exc)[:200], flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

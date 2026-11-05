#!/usr/bin/env python3
"""search-walk.py — document search in the real app: the palette's Documents section, Super Finder,
Require quoted text, Open original and Rich's search tool, over a synthetic folder (search PRD v3.1;
the CEO's chosen design is richos-hq design/mockups/rounds/round-26/).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      search-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION, the CEO's words: "a search in RichOS that's not only super fast but also allows the
user (and Rich) to easily and quickly find the right things even when the exact term or name they
are looking for is unknown or forgotten." This walk proves the shipped app does what the PRD's
slice one says on a folder whose answers are known. It does not judge search quality on his
archive: that comparison is his, on his own folder.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). The folder is set the way
the PRD says, RICHOS_SEARCH_DOCUMENTS_ROOT, passed in the launch environment (which overrides the
settings file) by relaunch.py; the walk's own first launch has none.
  identity        the running app says it was built from --expect-sha
  first-run       memory setup declined, a company added (adopt-walk.py's own steps)
  no-folder       unset: the palette says "No search folder configured", companies, threads and
                  conversations still searched; photographed in light and dark
  missing-folder  set to a folder that does not exist: a named error, never a guessed default
  backfill-kill   the folder created (the ten synthetic stand-in notes the crate's tests use, plus
                  --bulk generated notes so indexing takes long enough to interrupt); the app
                  killed (SIGKILL) while the index's manifest shows sources still indexing;
                  relaunched; one short message sent while it indexes again and its reply waited
                  for (the PRD's chat check during indexing; voice is not checked, the guest has
                  no microphone path); every source complete, each with one point per passage
                  and no point shared between two sources
  modes           "the kayak rental in Norway", which shares no word with any note: with Super
                  Finder checked documents are listed (by meaning alone), unchecked "No documents
                  match." and the query stays, checked again they are back; `cancel "30 days"`
                  lists related wording, and with Require quoted text checked only the note
                  holding "30 days"; photographed, the first state in light and dark. Which note
                  Apple's model puts first is recorded, never required: a ranking on synthetic
                  notes says nothing about his archive (the harness comparison is for that)
  open            the exact-span note made active and opened with Enter: the palette closes and
                  the guest opens that file (Open original by source ID)
  tool            Rich's tool exactly as claude reaches it (the app executable's --search-mcp
                  over stdio, to the running app's socket): the four checkbox combinations, the
                  mode that ran, coverage, and a forged `folder` argument refused

Every frame and the evidence (report.json, facts.json) go to --out. Synthetic text only. Exit 0 when
every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54); the file viewer
`open` starts is quit by the open step.
"""
import argparse
import importlib.util
import json
import os
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
STANDIN = HERE.parents[1] / 'crates' / 'richos-search' / 'tests' / 'fixtures' / 'standin'
STEPS = ['identity', 'first-run', 'no-folder', 'missing-folder', 'backfill-kill', 'modes', 'open', 'tool']
# No note shares a word with it ("the" and "in" are stop words), so anything listed is listed by
# meaning alone, and nothing is listed with Super Finder off. The harness found Apple's model
# puts the Lisbon trip first on the stand-in (question s7); recorded here, never required.
MEANING_ONLY = 'the kayak rental in Norway'
MEANING_NEAREST = 'Planning the Lisbon conference trip'
STRICT = 'cancel "30 days"'
STRICT_ANSWER = 'Refund window for annual plans'
STRICT_RELATED = 'Early draft of the refund rule'


# --- the two halves that run IN THE GUEST (this file is pushed and run there) ------------------

# None of these is a word of the stand-in notes' subjects or of the walk's queries, so the bulk
# notes add indexing time and never an answer.
WORDS = ('ledger quarterly tenant quota roster cadence procurement spreadsheet vendor forecast '
         'backlog payroll warehouse retention template inventory mileage').split()


def make_notes(folder, count):
    """`count` synthetic notes, deterministic, three paragraphs each, none on any stand-in subject."""
    folder = Path(folder) / 'bulk'
    folder.mkdir(parents=True, exist_ok=True)
    for i in range(count):
        para = []
        for p in range(3):
            words = [WORDS[(i * 7 + p * 3 + k) % len(WORDS)] for k in range(40)]
            para.append(' '.join(words).capitalize() + '.')
        (folder / f'note-{i:04d}.md').write_text(f'# Synthetic note {i}\n\n' + '\n\n'.join(para) + '\n')
    print(json.dumps({'written': count, 'folder': str(folder)}))


def tool_probe(exe, endpoint):
    """Rich's tool over its own stdio server, the way claude drives it, one process per call."""
    calls = [
        {'query': MEANING_ONLY},
        {'query': MEANING_ONLY, 'super_finder': False},
        {'query': STRICT, 'strict': True},
        {'query': STRICT, 'super_finder': False, 'strict': True},
        {'query': 'refund', 'folder': '/'},
    ]
    for args in calls:
        lines = [
            {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-06-18'}},
            {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call', 'params': {'name': 'search_documents', 'arguments': args}},
        ]
        began = time.monotonic()
        r = subprocess.run([exe, '--search-mcp', endpoint], input=''.join(json.dumps(l) + '\n' for l in lines),
                           capture_output=True, text=True, timeout=60)
        wall = round((time.monotonic() - began) * 1000)
        replies = [json.loads(l) for l in r.stdout.splitlines() if l.strip()]
        result = next((x['result'] for x in replies if x.get('id') == 2 and 'result' in x), None)
        row = {'arguments': args, 'wall_ms_including_process_start': wall}
        if result is None:
            row['error'] = r.stderr.strip()[:300]
        elif result.get('isError'):
            row['refused'] = result['content'][0]['text']
        else:
            a = json.loads(result['content'][0]['text'])
            row.update({k: a[k] for k in ('requested', 'effective', 'meaning_search', 'coverage', 'total_matches')})
            row['results'] = [{'path': x['path'], 'why': x['why'], 'excerpt_chars': len(x['excerpt'])} for x in a['results']]
        print(json.dumps(row))


# --- the host half ------------------------------------------------------------------------------

def host_modules():
    sys.path.insert(0, str(HERE))
    from relaunch import guest, relaunch  # noqa: E402
    spec = importlib.util.spec_from_file_location('folders_walk', HERE / 'folders-walk.py')
    folders = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(folders)
    return guest, relaunch, folders


def build_walk(folders, guest, relaunch):
    StepFailed = folders.StepFailed
    command = folders.command

    class Walk(folders.Walk):
        def __init__(self, a):
            super().__init__(a)
            self.archive = self.home + '/Archive'
            self.env = {}
            self.remote = self.payload + '/search-walk.py'

        # --- the palette ----------------------------------------------------------------
        def key(self, text):
            self.script('tell application "System Events" to ' + text)

        def palette(self, query):
            """Open the palette fresh (Escape first, so a reopen resets both checkboxes), type."""
            self.key('key code 53')
            time.sleep(0.5)
            self.key('keystroke "k" using command down')
            time.sleep(1.5)
            self.key('keystroke ' + json.dumps(query))
            time.sleep(3)

        def says(self, words, seconds=30):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if self.text_on_screen(words):
                    return True
                time.sleep(1)
            return False

        BOXES = {'Super Finder': 'srch-super', 'Require quoted text': 'srch-strict'}

        def checkbox(self, title):
            # A checkbox wrapped in its <label> carries its name in neither AXTitle nor
            # AXDescription (measured: `click --title "Super Finder" --role AXCheckBox` matched
            # nothing), and order is no address either: WebKit also exposes the aria-pressed
            # toggles (both Output buttons, Talk to Rich) as AXCheckBox, and a click by order
            # opened the Output panel and pressed Talk to Rich (measured). So each box is
            # clicked by its DOM id, and its value before and after is recorded and must change.
            box = self.BOXES[title]
            before = self.box_value(box)
            self.ax('click', '--id', box, '--role', 'AXCheckBox')
            time.sleep(3)
            after = self.box_value(box)
            self.facts.setdefault('checkbox_clicks', []).append({'box': title, 'before': before, 'after': after})
            self.save()
            if before == after:
                raise StepFailed(f'clicking {title} did not change it ({before})')

        def box_value(self, box):
            found = self.nodes('--id', box, '--role', 'AXCheckBox')
            return found[0].get('value') if found else None

        def relaunch_with(self, root):
            self.env = {'RICHOS_SEARCH_DOCUMENTS_ROOT': root}
            started = relaunch(self.vm, environment=self.env)
            time.sleep(8)
            return started

        def manifest(self):
            text = guest(self.vm, 'cat ' + shlex.quote(self.home) + '/Library/Caches/RichOS/search/index-*/manifest.json 2>/dev/null || true')
            return json.loads(text) if text.strip() else None

        def completed_turns(self):
            rows = guest(self.vm, 'grep -c TurnCompleted ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' || true')
            return int(rows.strip() or 0)

        def indexing_counts(self, total):
            m = self.manifest() or {'sources': {}}
            done = sum(1 for s in m['sources'].values() if s['state'] == 'complete')
            return {'complete': done, 'of': total}

        def chat_while_indexing(self, total):
            """The PRD: "Check chat ... once during indexing in the packaged app." One short message
            while the relaunched app is still building the index; the reply is the ledger's next
            TurnCompleted. Voice is not checked: the guest has no microphone path."""
            # A launch after the first run opens on the home screen, whose live canvas starves
            # accessibility reads (asked-again-walk.py, measured on .32; measured again here at
            # 7 to 15 s a read, after indexing had finished too). Enter, the home screen's own
            # hint, goes to the app's pid; then the conversation view.
            command([HERE / 'ax.sh', self.vm, '--key', '36'])
            self.wait_for('Message to Rich', 'AXTextArea', seconds=90)
            self.shot('chat-while-indexing.png')
            # Does the window answer while the index is built? Each sample times one
            # accessibility read of the composer and records the app's CPU and memory and the
            # index's progress, during indexing and after it, so the two can be compared.
            samples = []

            def sample():
                began = time.monotonic()
                found = self.present('Message to Rich', role='AXTextArea')
                index = self.indexing_counts(total)
                samples.append({'indexing': index['complete'] < total, 'read_s': round(time.monotonic() - began, 2),
                                'found': found, 'index': index,
                                'app_cpu_rss': guest(self.vm, 'ps -o %cpu=,rss= -p ' + self.app_pid() + ' || true').strip()})

            sample()
            before = self.completed_turns()
            at_send = self.indexing_counts(total)
            sent = time.monotonic()
            self.type_into('Reply with the single word ready.', '--role', 'AXTextArea', '--title', 'Message to Rich')
            self.press('Send')
            reply_after = None
            end = time.monotonic() + 900
            while time.monotonic() < end:
                if reply_after is None and self.completed_turns() > before:
                    reply_after = round(time.monotonic() - sent, 1)
                sample()
                if not samples[-1]['indexing'] and sum(1 for s in samples if not s['indexing']) >= 3:
                    break
                time.sleep(2)
            self.facts['responsiveness_while_indexing'] = samples
            self.save()
            during = [s['read_s'] for s in samples if s['indexing']]
            after = [s['read_s'] for s in samples if not s['indexing']]
            return {'sent_while_indexing': at_send['complete'] < total, 'indexing_at_send': at_send,
                    'composer_read_s': {'during_indexing': during, 'after_indexing': after},
                    # The guest has no signed-in Claude account, so no reply is expected; a reply
                    # that does come is recorded with its time.
                    'reply_after_s': reply_after, 'samples': samples}

        def app_pid(self):
            state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
            return (state / 'app.pid').read_text().strip()

        # --- steps --------------------------------------------------------------------------
        def no_folder(self):
            self.palette('refund')
            if not self.says('No search folder configured'):
                raise StepFailed('the palette did not say "No search folder configured" with no folder set')
            return {'said': 'No search folder configured', 'frames': self.both_themes('no-folder')}

        def missing_folder(self):
            self.relaunch_with(self.archive)
            self.palette('refund')
            if not self.says('Search folder not found'):
                raise StepFailed('a configured folder that does not exist was not named as an error')
            self.shot('missing-folder.png')
            return {'said': 'Search folder not found: <home>/Archive', 'frame': 'missing-folder.png'}

        def backfill_kill(self):
            command([HERE / 'guest.sh', self.vm, '--push', STANDIN, self.archive], 120)
            command([HERE / 'guest.sh', self.vm, '--push', Path(__file__).resolve(), self.remote], 60)
            made = json.loads(guest(self.vm, 'python3 ' + shlex.quote(self.remote) + ' --make-notes '
                                    + shlex.quote(self.archive) + ' ' + str(self.a.bulk), timeout=120))
            total = 10 + made['written']
            self.relaunch_with(self.archive)
            seen, killed_at = None, None
            end = time.monotonic() + 600
            while time.monotonic() < end:
                m = self.manifest()
                if m:
                    states = [s['state'] for s in m['sources'].values()]
                    done = states.count('complete')
                    seen = {'complete': done, 'indexing': states.count('indexing'), 'listed': len(states)}
                    if 16 <= done < total:
                        pid = self.app_pid()
                        exe = guest(self.vm, 'ps -p ' + pid + ' -o comm= || true')
                        if self.payload not in exe or not exe.endswith('/richos-tauri'):
                            raise StepFailed('the recorded app pid is not this payload\'s app: ' + exe)
                        guest(self.vm, 'kill -KILL ' + pid)
                        killed_at = seen
                        break
                time.sleep(0.5)
            if not killed_at:
                raise StepFailed(f'indexing was never caught part-way (last manifest: {seen})')
            time.sleep(2)
            after_kill = self.manifest()
            self.relaunch_with(self.archive)
            chat = self.chat_while_indexing(total)
            end = time.monotonic() + 900
            final = None
            while time.monotonic() < end:
                m = self.manifest()
                if m and len(m['sources']) == total and all(s['state'] == 'complete' for s in m['sources'].values()):
                    final = m
                    break
                time.sleep(2)
            if not final:
                raise StepFailed('after the relaunch the index never completed')
            points = [p for s in final['sources'].values() for p in s['points']]
            per_source_ok = all(len(s['points']) == len(s['locators']) for s in final['sources'].values())
            if len(points) != len(set(points)) or not per_source_ok:
                raise StepFailed('a point is shared between sources, or a source has a point per passage missing')
            self.palette(MEANING_ONLY)
            if not self.says('Super Finder ready'):
                raise StepFailed('the status line did not reach "Super Finder ready" after the relaunch')
            # Recorded, never fatal: a guest whose Claude login did not arrive cannot answer, and
            # that must not cost the palette and tool evidence after this step. The report says it.
            self.facts['chat_while_indexing'] = chat
            self.save()
            return {'files': total, 'killed_while': killed_at, 'chat_while_indexing': chat,
                    'manifest_after_kill': {'complete': sum(1 for s in (after_kill or {'sources': {}})['sources'].values() if s['state'] == 'complete'),
                                            'indexing': sum(1 for s in (after_kill or {'sources': {}})['sources'].values() if s['state'] == 'indexing')},
                    'after_relaunch': {'sources': len(final['sources']), 'points': len(points), 'distinct_points': len(set(points))}}

        def listed(self, mode_label):
            """The Documents heading says `mode_label` and at least one document is listed."""
            return self.says(mode_label, 20) and not self.text_on_screen('No documents match')

        def modes(self):
            out = {}
            self.palette(MEANING_ONLY)
            out['super_finder_on_lists_documents_by_meaning'] = self.listed('meaning and words')
            out['recorded_nearest_is_lisbon'] = self.text_on_screen(MEANING_NEAREST)
            out['frames_on'] = self.both_themes('modes-on')
            self.checkbox('Super Finder')
            out['super_finder_off_says_no_match'] = self.says('words only', 20) and self.says('No documents match', 20)
            out['query_kept'] = bool(self.nodes('--value', MEANING_ONLY, '--contains'))
            self.shot('modes-off.png')
            self.checkbox('Super Finder')
            out['back_on_lists_documents'] = self.listed('meaning and words')
            self.palette(STRICT)
            out['loose_lists_related_wording'] = self.says(STRICT_RELATED, 20)
            self.checkbox('Require quoted text')
            out['strict_lists_the_exact_one'] = self.says(STRICT_ANSWER, 20)
            out['strict_drops_related_wording'] = not self.text_on_screen(STRICT_RELATED)
            self.shot('modes-strict.png')
            expected = {'super_finder_on_lists_documents_by_meaning': True, 'super_finder_off_says_no_match': True,
                        'query_kept': True, 'back_on_lists_documents': True, 'loose_lists_related_wording': True,
                        'strict_lists_the_exact_one': True, 'strict_drops_related_wording': True}
            wrong = {k: out[k] for k, v in expected.items() if out[k] != v}
            if wrong:
                raise StepFailed('the modes did not follow the table: ' + json.dumps(wrong))
            return out

        def open(self):
            before = guest(self.vm, 'ps -axo comm= | sort -u')
            # The exact-span note: listed first for this query whatever the model ranks, so the
            # step tests Open original, not a ranking.
            self.palette(STRICT)
            if not self.says(STRICT_ANSWER, 20):
                raise StepFailed('the exact-span note is not listed')
            # The first document is the active row when the list is drawn (its passage and Open
            # original show), and Return opens the active row: the keyboard path the palette's
            # footer names. No click: a row is role="option", which accessibility exposes as
            # static text, not a button (measured: `click --title ... --role AXButton` matched
            # nothing, its near match an AXStaticText).
            time.sleep(1.5)
            self.shot('open-active-row.png')
            self.key('key code 36')
            time.sleep(4)
            after = guest(self.vm, 'ps -axo comm= | sort -u')
            started = sorted(set(after.splitlines()) - set(before.splitlines()))
            viewers = [p for p in started if '.app/' in p]
            self.shot('open-original.png')
            closed = not self.text_on_screen('Require quoted text')
            windows = {}
            for p in viewers:
                name = p.split('.app/')[0].rsplit('/', 1)[-1]
                windows[name] = self.script('tell application "System Events" to get name of every window of process '
                                            + json.dumps(name))
                self.script('tell application ' + json.dumps(name) + ' to quit')
            if not viewers:
                raise StepFailed('no application opened the document')
            if not any('refund-window' in w for w in windows.values()):
                raise StepFailed('the viewer did not open the chosen file: ' + json.dumps(windows))
            gone = guest(self.vm, 'ps -axo comm= | sort -u')
            left = [p for p in viewers if p in gone.splitlines()]
            return {'opened_by': viewers, 'windows': windows, 'palette_closed': closed, 'viewer_still_running': left}

        def tool(self):
            command([HERE / 'guest.sh', self.vm, '--push', Path(__file__).resolve(), self.remote], 60)
            app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
            exe = app + '/Contents/MacOS/richos-tauri'
            endpoint = self.data + '/search-endpoint.json'
            rows = [json.loads(l) for l in guest(self.vm, 'python3 ' + shlex.quote(self.remote) + ' --tool-probe '
                                                 + shlex.quote(exe) + ' ' + shlex.quote(endpoint), timeout=300).splitlines() if l.startswith('{')]
            (self.out / 'tool.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
            on, off, strict_on, strict_off, forged = rows
            problems = []
            # A query no note shares a word with: on, every result is a meaning result; off,
            # there are none. Which note leads is recorded, never required.
            on_results = on.get('results', [])
            if not (on.get('effective', {}).get('super_finder') and on_results and all(r['why'] == 'meaning' for r in on_results)):
                problems.append('Super Finder on did not answer by meaning alone: ' + json.dumps(on_results))
            if off.get('effective', {}).get('super_finder') is not False or off.get('results'):
                problems.append('Super Finder off ran meaning search or listed something: ' + json.dumps(off.get('results')))
            longest = max((r['excerpt_chars'] for x in rows[:4] for r in x.get('results', [])), default=0)
            if longest > 602:
                problems.append(f'an excerpt ran to {longest} characters')
            for r in (strict_on, strict_off):
                if [x['path'] for x in r.get('results', [])] != ['2025/refund-window.md']:
                    problems.append('strict did not keep exactly the exact-span note: ' + json.dumps(r.get('results')))
            if 'refused' not in forged:
                problems.append('a forged folder argument was not refused')
            if problems:
                raise StepFailed('; '.join(problems))
            return {'calls': len(rows), 'wall_ms_including_process_start': [r['wall_ms_including_process_start'] for r in rows],
                    'on_first': on_results[0]['path'], 'longest_excerpt_chars': longest}

    return Walk, StepFailed


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--make-notes':
        return make_notes(sys.argv[2], int(sys.argv[3])) or 0
    if len(sys.argv) > 1 and sys.argv[1] == '--tool-probe':
        return tool_probe(sys.argv[2], sys.argv[3]) or 0
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--bulk', type=int, default=1500, help='generated notes added so indexing can be interrupted and outlasts reaching the composer')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    guest, relaunch, folders = host_modules()
    Walk, StepFailed = build_walk(folders, guest, relaunch)
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
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

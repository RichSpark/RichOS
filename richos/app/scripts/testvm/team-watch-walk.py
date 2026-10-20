#!/usr/bin/env python3
"""team-watch-walk.py — his team's quota-watch, stall-watch and codex-watch wake his lead in the app.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      team-watch-walk.py --out DIR --expect-sha SHA [--within SECONDS]

THE QUESTION. The CEO, 2026-10-20, about the RichOS app as his daily driver ("your setup as-is",
ruling 86): "how long do I have to wait for all that checking and doing to be completed?" His
team's lead in the app runs `claude --print`, where Claude Code starts no plugin monitor, so only
review-watch reached it. The host now runs quota-watch, stall-watch and codex-watch as its own
children and sends each notice to his team's lead (richos-core team_watch.rs). This walk shows
one notice from each reaching that lead, in the real app, on fixture data in a guest.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme", the first conversation
  fixture    team-watch-guest.py setup: his team's fixture (the operator probes' entity and engine,
             the fences on, QUOTA_PAUSE_PERCENT=1) and its declaration in the app's data folder
  relaunch   command-walk.py's: the app quit and opened again, so it reads the declaration
  watchers   the app's own log names the three children's pids, each record names a live pid
  task       typed into the composer: ask his team to reply with one word, so a lead starts
  lead       his team's lead started (the operator log), its session named to the watchers
  notices    once his lead's turn has ended (it is idle, so only a watcher can wake it): a
             teammate of his lead is registered working, Codex writes an entry to the lead's
             channel and a teammate's escalation is raised
  observe    within --within seconds (at least one quota-watch poll) his lead's own transcript
             holds, as messages it was sent (user rows, or queued user messages when one arrives
             during a turn): the QUOTA-THRESHOLD notice naming that teammate (or, when the
             threshold was read before any lead ran, kept and sent to it first at its start), the
             CODEX-WATCH entry and the ESCALATION-WATCH block. Evidence is pulled to --out.
  reminder   a finished teammate of his lead, waiting for its review, is registered; once his
             lead's turn has ended (the host's turn_ended in team-watch-lead.json: a --print
             transcript never records it), stall-watch's UNFINISHED-WORK reminder naming that wait
             is in his lead's own transcript. Its 12-hour interval is --reminder-seconds in this
             guest only (the declaration's STALL_WATCH_PENDING_WORK_SECONDS), set only when this
             step is walked. On its own after `lead`: --steps identity,first-run,fixture,relaunch,
             watchers,task,lead,reminder.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed

REPO = HERE.parents[3]
PROBES = HERE.parent / 'operator-probes'
STEPS = ['identity', 'first-run', 'fixture', 'relaunch', 'watchers', 'task', 'lead', 'notices', 'observe', 'reminder']
# Not in the default run: `version-notice` is walked on its own after `lead` (see its docstring).
OPTIONAL_STEPS = ['version-notice']
UNMEASURED_SENTENCE = "a version Rich hasn't checked this setup against yet"
# What stall-watch's unfinished-work reminder starts with, and the finished teammate's wait it names.
REMINDER = {'stall-watch reminder': 'UNFINISHED-WORK', 'its recorded wait': 'waiting for its review'}
TASK = ('Please ask my team to read the first line of CLAUDE.md in their repository and tell me what it says. '
        'Nothing else is needed.')
WATCHERS = ('quota-watch', 'stall-watch', 'codex-watch')
# What each watcher's notice starts with, as the lead is sent it.
MARKERS = {'quota-watch': 'QUOTA-THRESHOLD', 'codex-watch': 'CODEX-WATCH', 'stall-watch': 'ESCALATION-WATCH'}
CODEX_WORDS = 'READY TO LAND: codex/team-watch-walk, a fixture entry'
ESCALATION_ID = 'esc-team-watch-walk'
QUOTA_POLL_SECONDS = 300  # quota_watch.py's poll once a reading is known


class TeamWatchWalk(command_walk.CommandWalk):
    def __init__(self, a):
        super().__init__(a)
        self.operator = self.data + '/operator'

    def after_relaunch(self):
        """The relaunch opens on the home screen, whose live canvas keeps every accessibility read
        past ax.sh's 20 s deadline (run 5, 2026-10-20: CommandWalk.relaunch's first composer read
        ended 124 on it). As asked-again-walk.py does: the home screen is recognized by OCR on a
        frame, and Enter, its own key for Talk to Rich, goes in; the composer is read after that."""
        began = time.monotonic()
        while time.monotonic() - began < self.a.relaunch_within:
            self.shot('home-after-relaunch.png')
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich',
                                    str(self.out / 'home-after-relaunch.png'), '--first', '--quiet'],
                                   capture_output=True, text=True, timeout=60)
            if found.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the frame: ' + found.stderr)
            if found.returncode == 0:
                seen = round(time.monotonic() - began, 1)
                command_walk.command([HERE / 'ax.sh', self.vm, '--key', '36'])
                time.sleep(3)
                return {'home_on_screen_after_s': seen, 'entered_with': 'Enter'}
            time.sleep(2)
        return {'home_on_screen_after_s': None}

    def received(self):
        """His typed messages on the walk's conversation, as the conversation ledger records them
        (output-walk.py's PromptReceived rows): a Send press is confirmed by the turn it became."""
        rows = []
        for line in self.read(self.data + '/conversation-ledger.jsonl').splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                continue
            if (row.get('event') == 'PromptReceived' and row.get('thread_id') == self.facts.get('thread')
                    and row.get('source') != 'internal'):
                rows.append(row.get('turn_id'))
        return rows

    def task(self):
        """CommandWalk's task, confirmed: his words must become a turn on the conversation (its
        PromptReceived row) within 30 s of Send. On 2026-10-20 five runs in a row typed the task
        into the composer (its value read back), pressed the one Send button there is (clicked),
        and the app recorded nothing: no message on screen, no relay, no lead. What it typed into,
        what the press reached, the composer afterwards and a frame are kept for each attempt
        (task.json); a send the ledger did not record is typed and sent once more, never more."""
        before = self.clock()
        known = set(self.received())
        attempts = []
        for attempt in (1, 2):
            try:
                typed = self.type_into(self.a.task, '--role', 'AXTextArea', '--title', 'Message to Rich')
                pressed = self.press('Send')
            except (StepFailed, subprocess.TimeoutExpired) as exc:
                # An accessibility read past ax.sh's 20 s deadline on a busy host (run 8: exit 124
                # at 89% CPU) is not a lost send: the first attempt records it and the second runs.
                if attempt == 2:
                    raise
                attempts.append({'failed': str(exc)[:400]})
                continue
            end = time.monotonic() + 30
            new = []
            while time.monotonic() < end and not new:
                new = [t for t in self.received() if t not in known]
                if not new:
                    time.sleep(2)
            try:
                composer = self.ax('find', '--title', 'Message to Rich', '--role', 'AXTextArea', '--contains', '--first')
            except (StepFailed, subprocess.TimeoutExpired) as exc:
                composer = 'not read: %s' % str(exc)[:300]
            self.shot('task-attempt-%d.png' % attempt)
            attempts.append({'typed_into': typed, 'pressed': pressed, 'turns': new, 'composer_after': composer})
            if new:
                break
        self.facts['sent_ms'] = [before, self.clock()]
        self.save()
        evidence = {'sent_between_guest_ms': [round(v) for v in self.facts['sent_ms']], 'attempts': attempts}
        (self.out / 'task.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not attempts[-1]['turns']:
            raise StepFailed('his typed words never became a turn on the conversation (no PromptReceived row '
                             'in 30 s, twice); see task.json and task-attempt-*.png')
        return {'sent_between_guest_ms': evidence['sent_between_guest_ms'], 'attempts': len(attempts),
                'turn': attempts[-1]['turns'][-1]}

    def read(self, path, limit=200000):
        return guest(self.vm, 'tail -c %d %s 2>/dev/null || true' % (limit, shlex.quote(path)), 60)

    def fixture(self):
        made = self.out / 'probes'
        made.mkdir(parents=True, exist_ok=True)
        tar = made / 'engine.tar'
        command_walk.command(['git', '-C', REPO, 'archive', '--format=tar', '-o', tar, self.a.expect_sha,
                              '.claude-plugin', 'richos/engine'], 300)
        for name in ('guest_probes.py',):
            (made / name).write_bytes((PROBES / name).read_bytes())
        (made / 'team-watch-guest.py').write_bytes((HERE / 'team-watch-guest.py').read_bytes())
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', made, self.payload + '/'], 300)
        # The reminder's interval is shortened in this guest only, and only when it is walked.
        reminder = str(int(self.a.reminder_seconds)) if 'reminder' in self.a.steps.split(',') else ''
        said = guest(self.vm, '/usr/bin/python3 %s setup %s %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload), reminder), 900)
        record = json.loads(said.splitlines()[-1])
        (self.out / 'fixture.json').write_text(json.dumps(record, indent=2) + '\n')
        fences = record.get('fence_status') or [1, '']
        if fences[0] != 0:
            raise StepFailed('the fixture fences are not on: ' + json.dumps(record.get('fence_install')) + ' '
                             + json.dumps(fences))
        self.facts.update({'entity': record['entity'], 'engine': record['engine'], 'channels': record['channels']})
        self.save()
        return {'entity': record['entity'], 'fences': fences[1][-200:], 'declaration': record['data'] + '/operator.json'}

    def watchers(self):
        log = self.read(json.loads((self.out / 'relaunch-observed.json').read_text())['relaunched']['log'])
        (self.out / 'app-after-relaunch.log').write_text(log + '\n')
        pids = {}
        for name in WATCHERS:
            line = next((l for l in log.splitlines() if 'his team: %s runs as pid' % name in l), '')
            if not line:
                raise StepFailed('the app never said it started %s; see app-after-relaunch.log' % name)
            pid = line.rsplit(' ', 1)[-1]
            record = json.loads(self.read(self.data + '/engine-state/team-watch/%s.json' % name) or '{}')
            alive = guest(self.vm, 'kill -0 %s 2>/dev/null && echo alive || true' % pid) == 'alive'
            if str(record.get('pid')) != pid or not alive:
                raise StepFailed('%s: pid %s, record %s, alive %s' % (name, pid, record, alive))
            pids[name] = int(pid)
        self.facts['watchers'] = pids
        self.save()
        return pids

    def lead(self):
        began = time.monotonic()
        end = began + self.a.within
        shot = False
        while time.monotonic() < end:
            if not shot and time.monotonic() - began >= 20:
                self.shot('after-task.png')   # his words on screen, or not, before a lead could start
                shot = True
            log = self.read(self.operator + '/operator.log')
            pointer = json.loads(self.read(self.operator + '/team-watch-lead.json') or '{}')
            if 'lead started for' in log and pointer.get('session'):
                self.facts['session'] = pointer['session']
                self.save()
                return {'session': pointer['session'],
                        'started': [l for l in log.splitlines() if 'lead started for' in l][-1]}
            time.sleep(3)
        (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
        relaunched = json.loads((self.out / 'relaunch-observed.json').read_text())['relaunched']['log']
        (self.out / 'app-at-lead-failure.log').write_text(self.read(relaunched) + '\n')
        self.shot('lead-missing.png')
        raise StepFailed('no lead of his team started within %g s of the task; see operator.log, '
                         'app-at-lead-failure.log and after-task.png' % self.a.within)

    def version_notice(self):
        """Whether the app told the user his team's Claude Code version is unmeasured (operator_profile.rs
        init_check: "Your team is running on Claude Code <v>, a version Rich hasn't checked this setup against
        yet."). The words are looked for in everything the app keeps in its data folder, and on the screen. The
        guest's claude version is read from the binary itself. --expect-notice no (a measured version, 2.1.304)
        fails when the sentence is found; yes (an unmeasured one) fails when it is not, which is the positive
        control that proves this step can see the sentence at all. On its own after `lead`:
        --steps identity,first-run,fixture,relaunch,watchers,task,lead,version-notice."""
        version = guest(self.vm, 'perl -e "alarm shift; exec @ARGV" 60 "$HOME/.local/bin/claude" --version 2>&1 | head -1', 90).strip()
        found = guest(self.vm, 'grep -rIl %s %s 2>/dev/null || true' % (
            shlex.quote(UNMEASURED_SENTENCE), shlex.quote(self.data)), 120).split()
        self.shot('version-notice.png')
        evidence = {'guest_claude': version, 'sentence': UNMEASURED_SENTENCE, 'found_in': found,
                    'expect_notice': self.a.expect_notice}
        (self.out / 'version-notice.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if (self.a.expect_notice == 'yes') != bool(found):
            raise StepFailed('expected the unmeasured-version sentence to be %s, and it was %s (guest claude %s); see version-notice.json'
                             % ('shown' if self.a.expect_notice == 'yes' else 'absent', 'found' if found else 'not found', version))
        return evidence

    def turn_state(self, path):
        """'ended' when the last main-thread conversation row of the lead's transcript ends its
        turn, else 'busy'; and the transcript's size. His lead runs `claude --print`, whose
        transcript never carries the interactive `turn_duration` row (measured 2026-10-20 on this
        Mac: 0 of 296 print-mode transcripts, 76 of 97 interactive ones), so stall_watch's
        lead_is_idle rule never sees it end; its turn ends on an assistant row whose stop_reason
        is end_turn (or stop_sequence), after which only bookkeeping rows are written."""
        script = ('import json,os,sys\n'
                  'rows=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]\n'
                  'state="busy"\n'
                  'for r in reversed(rows):\n'
                  '    if r.get("isSidechain"): continue\n'
                  '    if r.get("type")=="system" and r.get("subtype")=="turn_duration": state="ended"; break\n'
                  '    if r.get("type")=="user": break\n'
                  '    if r.get("type")=="assistant":\n'
                  '        m=r.get("message") if isinstance(r.get("message"),dict) else {}\n'
                  '        state="ended" if m.get("stop_reason") in ("end_turn","stop_sequence") else "busy"; break\n'
                  'print(state, os.path.getsize(sys.argv[1]))\n')
        said = guest(self.vm, 'python3 -c %s %s 2>/dev/null || true' % (shlex.quote(script), shlex.quote(path)), 60).split()
        return (said[0], int(said[1])) if len(said) == 2 and said[1].isdigit() else ('busy', -1)

    def row_shapes(self, path):
        """The lead transcript's rows as type/subtype/stop_reason only, for a failure's evidence."""
        script = ('import json,sys\n'
                  'for l in open(sys.argv[1]):\n'
                  '    try: r=json.loads(l)\n'
                  '    except Exception: print("unparsed"); continue\n'
                  '    m=r.get("message") if isinstance(r.get("message"),dict) else {}\n'
                  '    a=r.get("attachment") if isinstance(r.get("attachment"),dict) else {}\n'
                  '    print(r.get("type"), r.get("subtype") or a.get("type") or r.get("operation") or "-",\n'
                  '          m.get("stop_reason") or (type(a["prompt"]).__name__ if "prompt" in a else "-"),\n'
                  '          "side" if r.get("isSidechain") else "main", r.get("timestamp") or "-",\n'
                  '          (r.get("content") or "").split("\\n")[0][:80] if r.get("operation")=="enqueue" and isinstance(r.get("content"),str) else "")\n')
        return guest(self.vm, 'python3 -c %s %s 2>&1 || true' % (shlex.quote(script), shlex.quote(path)), 60)

    def notices(self):
        """Once his lead's turn has ended and its transcript stayed unchanged for 15 s (a Stop hook
        or a queued message would have written to it), so only a watcher can wake it."""
        end = time.monotonic() + self.a.within
        path, last = '', None
        while time.monotonic() < end:
            path = path or self.transcript()
            now = self.turn_state(path) if path else ('busy', -1)
            if now[0] == 'ended' and last == now:
                break
            last = now
            time.sleep(15)
        else:
            if path:
                (self.out / 'lead-transcript-rows.txt').write_text(self.row_shapes(path) + '\n')
            (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
            raise StepFailed('his lead\'s turn never ended within %g s (transcript %s; its rows are in '
                             'lead-transcript-rows.txt)' % (self.a.within, path or 'none'))
        # A teammate of his lead, working: quota-watch, already past QUOTA_PAUSE_PERCENT=1, names
        # it at its next poll (QUOTA_POLL_SECONDS), as it would a teammate of his to pause.
        mate = guest(self.vm, '/usr/bin/python3 %s teammate %s %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload),
            shlex.quote(self.facts['session'])), 60)
        channel = self.facts['channels'][0]
        entry ='## %s — fixture\n%s\n' % (time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime()), CODEX_WORDS)
        guest(self.vm, 'printf %%s %s >> %s' % (shlex.quote(entry), shlex.quote(channel)))
        ledger = guest(self.vm, '/usr/bin/python3 %s ledger %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload)), 60)
        raised = guest(self.vm, "date -u +%Y-%m-%dT%H:%M:%SZ")
        row = {'event': 'Escalation', 'id': ESCALATION_ID, 'raised': raised, 'teammate': 'fixture-mate',
               'worktree': self.facts['entity'] + '/.claude/worktrees/fixture-mate', 'branch': 'cc/fixture-mate',
               'repo': 'fixture', 'head': '', 'state': 'proceeding', 'for': 'lead',
               'title': 'Team-watch walk: a fixture escalation',
               'question': 'A fixture question, raised by the walk to show the escalation watch reaches the lead.',
               'tried': '', 'meanwhile': '', 'record': '', 'session_id': '', 'actor': 'team-watch-walk'}
        guest(self.vm, 'mkdir -p %s && printf %%s\\\\n %s >> %s' % (
            shlex.quote(str(Path(ledger).parent)), shlex.quote(json.dumps(row)), shlex.quote(ledger)))
        self.facts.update({'ledger': ledger, 'raised': raised, 'teammate': mate})
        self.save()
        return {'teammate': mate, 'channel': channel, 'ledger': ledger, 'raised': raised}

    def transcript(self):
        found = guest(self.vm, 'ls %s/.claude/projects/*/%s.jsonl 2>/dev/null || true' % (
            shlex.quote(self.home), shlex.quote(self.facts['session'])))
        return found.splitlines()[0] if found else ''

    def sent_to_lead(self, path, markers=MARKERS):
        """Each watcher's notice, as a user message in his lead's own transcript: the first line of
        each, so the evidence carries no more of the fixture than it needs. A message that reaches
        the lead during a turn is written as a queued user message (an attachment of type
        queued_command, its text in `prompt`), one taken between turns as a user row (run 7: the
        notice sent at 14:11:25Z, during the turn the 14:11:10Z one started, was the former)."""
        script = ('import json,sys\n'
                  'out={}\n'
                  'for l in open(sys.argv[1]):\n'
                  '    try: r=json.loads(l)\n'
                  '    except Exception: continue\n'
                  '    a=r.get("attachment") if isinstance(r.get("attachment"),dict) else {}\n'
                  '    if r.get("type")=="user":\n'
                  '        c=(r.get("message") or {}).get("content"); how="user row"\n'
                  '        t=c if isinstance(c,str) else " ".join(x.get("text","") for x in c or [] if isinstance(x,dict))\n'
                  '    elif r.get("type")=="attachment" and a.get("type")=="queued_command":\n'
                  '        p=a.get("prompt"); how="queued user message"\n'
                  '        t=p if isinstance(p,str) else " ".join(x.get("text","") for x in p or [] if isinstance(x,dict))\n'
                  '    else: continue\n'
                  '    for k,m in json.loads(sys.argv[2]).items():\n'
                  '        if m in t and k not in out:\n'
                  '            line=next(s for s in t.splitlines() if m in s)\n'
                  '            out[k]={"timestamp":r.get("timestamp"),"as":how,"line":line[:300]}\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(path), shlex.quote(json.dumps(markers))), 60) or '{}')

    def observe(self):
        # One quota-watch poll past --within: the teammate registered in `notices` is seen at
        # its next poll, up to QUOTA_POLL_SECONDS later (its log: "polling every 300 s").
        waited = max(self.a.within, QUOTA_POLL_SECONDS + 60)
        end = time.monotonic() + waited
        got, path = {}, ''
        while time.monotonic() < end:
            path = path or self.transcript()
            got = self.sent_to_lead(path) if path else {}
            if len(got) == len(MARKERS):
                break
            time.sleep(5)
        (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
        if path:
            (self.out / 'lead-transcript-rows.txt').write_text(self.row_shapes(path) + '\n')
        for name in WATCHERS:
            (self.out / ('%s.log' % name)).write_text(self.read(self.data + '/engine-state/team-watch/%s.log' % name) + '\n')
        evidence = {'session': self.facts.get('session'), 'transcript': path, 'sent_to_lead': got}
        (self.out / 'observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        missing = [name for name in WATCHERS if name not in got]
        if missing:
            said = {name: [l for l in (self.out / ('%s.log' % name)).read_text().splitlines() if l.strip()][-1:]
                    for name in missing}
            raise StepFailed('not in his lead\'s transcript within %.0f s: %s; each one\'s last log line: %s'
                             % (waited, ', '.join(missing), json.dumps(said)))
        return got

    def reminder(self):
        """Stall-watch's unfinished-work reminder reaches his lead once its turn has ended. His lead
        runs `claude --print`, whose transcript never records a turn's end, so stall-watch takes the
        host's word for it (team-watch-lead.json's turn_ended). A finished teammate of his lead,
        waiting for its review, is registered; within the reminder's interval (shortened for this
        guest, --reminder-seconds), a stall-watch poll and his lead's turn, his lead's own transcript
        holds the UNFINISHED-WORK notice naming that wait, sent by the host.

        The teammate is registered only once the host says his lead is between turns: a record
        with no workspace counts as already landed, and the engine's turn-end gate (its Stop hook,
        pending() with auto-land) resolves it, so one registered during the turn is gone before
        the reminder is due (run 7, 2026-10-20: stall-watch idle, every 120 s, pending [])."""
        between = time.monotonic() + self.a.within
        while time.monotonic() < between:
            if '"turn_ended"' in self.read(self.operator + '/team-watch-lead.json', 4096):
                break
            time.sleep(3)
        else:
            raise StepFailed('the host never said his lead was between turns within %g s (no turn_ended in '
                             'team-watch-lead.json)' % self.a.within)
        mate = guest(self.vm, '/usr/bin/python3 %s finished %s %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload),
            shlex.quote(self.facts['session'])), 60)
        waited = self.a.within + self.a.reminder_seconds + 120
        end = time.monotonic() + waited
        got, path, pointers = {}, '', []
        while time.monotonic() < end:
            path = path or self.transcript()
            pointer = self.read(self.operator + '/team-watch-lead.json', 4096)
            if not pointers or pointers[-1] != pointer:
                pointers.append(pointer)
            got = self.sent_to_lead(path, REMINDER) if path else {}
            if len(got) == len(REMINDER):
                break
            time.sleep(5)
        (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
        (self.out / 'stall-watch.log').write_text(self.read(self.data + '/engine-state/team-watch/stall-watch.log') + '\n')
        if path:
            (self.out / 'lead-transcript-rows.txt').write_text(self.row_shapes(path) + '\n')
        # What stall-watch's reminder sees at the end, asked of the guest's engine under the watchers'
        # own environment: idle, the start, the interval, the pending list and its kept state.
        check = guest(self.vm, '/usr/bin/python3 %s remind-check %s %s %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload),
            shlex.quote(self.facts['session']), shlex.quote(self.operator + '/team-watch-lead.json')), 120)
        try:
            check = json.loads(check.splitlines()[-1])
        except (ValueError, IndexError):
            pass
        evidence = {'session': self.facts.get('session'), 'transcript': path, 'teammate': mate,
                    'reminder_seconds': self.a.reminder_seconds, 'sent_to_lead': got,
                    'lead_file_as_seen': pointers, 'stall_watch_sees': check}
        (self.out / 'reminder.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if len(got) != len(REMINDER):
            raise StepFailed('no UNFINISHED-WORK notice naming the wait in his lead\'s transcript within %.0f s '
                             '(lead file as seen: %s; stall-watch sees: %s); see reminder.json, stall-watch.log and '
                             'lead-transcript-rows.txt' % (waited, ' | '.join(pointers), json.dumps(check)[:600]))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=TASK)
    p.add_argument('--within', type=float, default=300, help='seconds for a lead to start, and for the notices')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--approvals', type=int, default=0)
    p.add_argument('--reminder-seconds', type=float, default=120,
                   help='stall-watch\'s unfinished-work reminder interval in this guest (12 hours otherwise)')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--expect-notice', choices=['yes', 'no'], default='no',
                   help='step version-notice: is the unmeasured-version sentence expected (yes, an unmeasured claude) or not (no)')
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS + OPTIONAL_STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = TeamWatchWalk(a)
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
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

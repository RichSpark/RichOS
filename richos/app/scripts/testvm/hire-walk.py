#!/usr/bin/env python3
"""hire-walk.py — the onboarding interview ends with Rich proposing a team, and "go" hires it.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      hire-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset, still
as one run under run-walk.py.

THE QUESTION. The CEO, 2026-10-13: "Why the fuck does Rich not already automatically auto-hire
everyone he deems to be needed???", and the ending in his words: Rich names the five teammates
he already has (Clark, Dean, Frank, Pierce, Reed), lists the new ones the interview calls for,
and ends "Say go and I'll get them hired now." On go, they are hired in this install, and only
the ones the app saved are named.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  first-run   setup sheets declined, company "Acme" added, and "Start the questions" pressed
  stock       a running lease's plugin registers the five every fresh install starts with
  interview   the answers in ANSWERS typed one per finished turn (stage 4 names a marketing lead
              and a bookkeeper), then "Let's wrap up the interview." until Rich's reply carries
              "Say go and I'll get them hired now." PASS needs that reply to open with the five
              and list, as bullets, a teammate for each role he asked for. Evidence: interview.json.
  go          "Go." typed; the assignment it registers is followed until it closes (--hire-within).
              PASS needs every proposed teammate saved as <app data>/team/<name>.md, every saved
              one named in what he was told, nobody named to him as a teammate who was not saved,
              and no Approve card put in front of him along the way, and nothing he was told says
              the write-up hit a snag or was sent again. Evidence: go-observed.json. A go that fails
              with somebody saved still lets the later steps walk the ones who were.
  relaunch    command-walk.py's relaunch: the app stopped by pid and started again; before its
              first read the walk waits for the guest's whisper-cli runs to end (--settle-within)
              and then reads with a longer deadline (--after-relaunch-deadline)
  roster      after the relaunch, the lease's roster holds the five and every hired teammate
  approvals   last: the Approve cards the hiring put in front of him after his go, counted during
              go and judged here so they do not hide the steps after it. Any fails.
  delegate    "Please ask <the hire whose definition speaks of marketing> for three subject lines ..."
              typed; a question card Rich asks is shot and answered with his recommended option, by
              click, as a person answers it. PASS needs a back-end
              dispatch receipt for THIS task's obligation naming that teammate, whose run handed back
              an answer (consult_answer, set from the run's own hand-back or stop), and the task
              closed settled with a result he was told. Evidence: delegate-observed.json, and
              whatever the verdict delegate-ledger.json (the conversation from the send on) and
              delegate-1-before/2-sent/3-end.png.
              With --ask-rich TEXT the job is TEXT, asked of Rich naming no teammate, as a user asks
              (a user never hands a job to a teammate and never needs to know who does it): PASS
              when Rich answers it in his own turn with no assignment (`answered_in_turn`), or when
              the assignment he registers closes settled with a result he was told; whoever Rich
              dispatched is recorded (`dispatched`), never required.
  hood        after delegate, "Under the hood" opened as a person opens it (the work summary chip
              under the conversation): both Output buttons, the stage header's (#out-top) and the
              composer's (#out-bottom), end at or before the panel's left edge and are shown
              (the CEO, 2026-10-14: the last card and the Output button belong under the panel, not
              under its edge). Evidence: hood-observed.json, under-the-hood.png.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed

STEPS = ['identity', 'first-run', 'stock', 'interview', 'go', 'relaunch', 'roster', 'delegate', 'hood', 'approvals']
# The five every install registers (engine_profile.rs ALWAYS_ACTIVE), as the CEO's sentence names them.
STOCK = {'clark': 'Senior Researcher', 'dean': 'HR Director', 'frank': 'Expert Advisor',
         'pierce': 'Brief Checker', 'reed': 'Source-Reading'}
GO_WORDS = "Say go and I'll get them hired now."
# One answer per finished turn. A fictional coaching business; stage 4 names two specialists.
ANSWERS = [
    "Acme runs leadership coaching for managers at mid-size companies. It promises managers who can "
    "lead their teams through change. The name is Acme.",
    "One product: live coaching cohorts, plus a simple members website. One audience, the managers.",
    "The website is on Squarespace, and I run everything myself with one part-time assistant.",
    "I'd like a marketing lead who fills the cohorts, and a bookkeeper. That covers it.",
    "Never share a client's coaching notes with anyone outside the company.",
    "Pricing and any spend over $1,000 is my call. Scheduling and drafts you can decide.",
]
WRAP_UP = "Let's wrap up the interview."
# The roles ANSWERS asks for in stage 4, each with the words a proposed job title may use for it.
REQUESTED_ROLES = {'marketing lead': ('market',), 'bookkeeper': ('book', 'account', 'financ')}
# A question card's recommended option carries this tag (ui/questions.js); a person answers the
# card by clicking that option.
RECOMMENDS = 'Rich recommends'
DELEGATE_TASK = ('Please ask {Name} to suggest three subject lines for our next newsletter to past '
                 'coaching clients, and tell me what they suggest.')


def turns_of(rows, thread):
    """The conversation's visible turns in order: {turn, prompt, reply, done}, from the ledger rows."""
    turns, by_id = [], {}
    for r in rows:
        event = r.get('event')
        if event == 'PromptReceived' and r.get('thread_id') == thread and r.get('source') not in ('internal', 'proactive'):
            t = {'turn': r['turn_id'], 'prompt': r.get('text', ''), 'reply': '', 'done': False, 'at': r.get('at')}
            turns.append(t)
            by_id[r['turn_id']] = t
        elif event == 'AssistantDelta' and r.get('turn_id') in by_id:
            by_id[r['turn_id']]['reply'] += r.get('text', '')
        elif event in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed') and r.get('turn_id') in by_id:
            by_id[r['turn_id']]['done'] = True
            by_id[r['turn_id']]['end'] = event
    return turns


def proposed_teammates(reply):
    """The new teammates in the end-of-interview message, one bullet each, "Name (Job title)":
    [(name, the rest of the bullet line)]."""
    tail = reply.split('following AI teammates', 1)[-1]
    out = []
    for line in tail.splitlines():
        m = re.match(r'\s*(?:[-*•]|\d+[.)])\s+\**([A-Z][A-Za-z]+)\**\s*([(—:-].*)', line)
        if m:
            out.append((m.group(1).lower(), m.group(2)))
    return out


def proposed_names(reply):
    return [name for name, _ in proposed_teammates(reply)]


def proposal_verdict(reply):
    """(proposed names, failures) for the reply that should end the interview. Every role he asked
    for in stage 4 must be one of the proposed teammates' job titles."""
    failures = []
    if GO_WORDS not in reply.replace('’', "'"):
        failures.append(f'the reply does not end with "{GO_WORDS}"')
    if 'At the beginning, I already had these teammates on my AI team' not in reply.replace('’', "'"):
        failures.append('the reply does not open the team message with the CEO\'s words')
    for name, role in STOCK.items():
        if f'{name.capitalize()} ({role}' not in reply:
            failures.append(f'{name.capitalize()} ({role} ...) is not in the opening five')
    proposed = proposed_teammates(reply)
    if not proposed:
        failures.append('no new teammate is listed as a bullet')
    titles = [title.lower() for _, title in proposed]
    for role, words in REQUESTED_ROLES.items():
        if not any(w in t for t in titles for w in words):
            failures.append(f'he asked for a {role} and no proposed teammate is one')
    return [name for name, _ in proposed], failures


# Words that open a sentence or name the company, never a teammate he is told about.
# Words that open a sentence, stand in for a person or name the company, never a teammate: walk 9
# of 2026-10-13 was told "Neither is on the team yet" and that read as a teammate called Neither.
NOT_A_TEAMMATE = {'acme', 'rich', 'i', 'and', 'the', 'your', 'my', 'our', 'both', 'they', 'everyone', 'nobody',
                  'neither', 'either', 'none', 'each', 'every', 'all', 'he', 'she', 'it', 'we', 'this', 'that',
                  'these', 'those', 'someone', 'one', 'nothing', 'everything'}
NAME = r'\*{0,2}([A-Z][a-z]{1,15})\*{0,2}'
# A job title in parentheses: capitalized words only, such as "(Bookkeeper)", "(Marketing
# Director)" or "(Source-Reading & Knowledge-Extraction Specialist)"; never "(a leadership firm)".
TITLE = r'\(\s*[A-Z][A-Za-z]*(?:[ &/-]+[A-Z][A-Za-z]*)*\s*[;,)]'


def claimed_names(told):
    """Every name he was told about as a teammate: a name before its job title in parentheses
    ("Bea (Bookkeeper)", "**Bea** (Bookkeeper)", "**Bea (Bookkeeper):**"), a bullet that opens with
    a name and a dash or title, and each name in a list that "joined", "is on" or "are on" the
    team ("Will and Bea joined", "Will, Nora and Bea are now on the team")."""
    text = told or ''
    names = set(re.findall(r'\b([A-Z][a-z]{1,15})\*{0,2}\s*' + TITLE, text))
    # A bullet's bare "Word:" is a label, not a name: nightly 56's walks read the rule lists he was
    # told ("- **Drafts:** she can draft invoices", "- **Confidentiality:** a client's coaching
    # notes never leave the company") as teammates. A name in a bullet needs its title or a dash...
    names |= set(re.findall(r'(?m)^\s*(?:[-*\u2022]|\d+[.)])\s+\*{0,2}([A-Z][a-z]{1,15})\*{0,2}\s*(?:\u2014|' + TITLE + ')', text))
    # ...unless the rest of its line says the name was hired or joined ("- Bea: hired as your
    # bookkeeper"): that is a claim about a teammate, and a rule label never says it.
    names |= set(re.findall(r'(?m)^\s*(?:[-*\u2022]|\d+[.)])\s+\*{0,2}([A-Z][a-z]{1,15})\*{0,2}\s*:\*{0,2}'
                            r'[^\n]*\b(?:hired|joined|on (?:your|the) team|teammate)\b', text))
    joined = re.compile(r'((?:' + NAME + r'(?:\s*,\s*(?:and\s+)?|\s+and\s+))*' + NAME + r')'
                        r'\s+(?:has\s+|have\s+)?(?:joined|(?:is|are)\s+(?:now\s+)?(?:on|part of))')
    for m in joined.finditer(text):
        names |= set(re.findall(r'[A-Z][a-z]{1,15}', m.group(1)))
    return sorted({n.lower() for n in names} - NOT_A_TEAMMATE)


def hired_verdict(proposed, saved, told):
    """Every proposed teammate saved; every saved teammate named in what he was told; and nobody
    he was told about as a teammate who is neither saved nor one of the five."""
    failures = []
    told_lower = (told or '').lower()
    for claimed in claimed_names(told):
        if claimed not in saved and claimed not in STOCK:
            failures.append(f'he was told about {claimed.capitalize()} and no such teammate was saved')
    for name in proposed:
        if name not in saved:
            failures.append(f'{name} was proposed and not saved in the team folder')
    for name in saved:
        if name not in told_lower:
            failures.append(f'{name} was saved and not named in what he was told')
    if not saved:
        failures.append('nobody was saved in the team folder')
    return failures


# What he was told when the hiring write-up did not go through on its first send (nightly 55's
# walk: "Starting the step that writes up Will and Beth hit a technical snag on my end. I'm sending
# it again in a shorter form."). The CEO, 2026-10-14: the write-up must not fail its first send.
RETRY_TOLD = re.compile(r'\bsnag\b|\bhiccup\b|\bglitch\b|technical (?:issue|problem|difficult)'
                        r'|sending it again|send(?:ing)? it again|in a shorter form', re.I)


def first_send_verdict(told):
    """The sentences that tell him the write-up was retried ([] = it went through the first time)."""
    return [m.group(0) for m in RETRY_TOLD.finditer(told or '')]


def hood_verdict(panel, boxes):
    """Why the open "Under the hood" panel covers something ([] = nothing): `panel` is the panel's
    node, `boxes` maps a name to its node (None when it is not on screen). Each must end at or
    before the panel's left edge; one pixel is allowed for rounding."""
    if not panel:
        return ['the Under the hood panel is not on screen']
    edge = panel['x']
    failures = []
    for name, node in boxes.items():
        if not node:
            failures.append(f'{name} is not on screen')
        elif node['x'] + node['w'] > edge + 1:
            failures.append(f"{name} ends at {node['x'] + node['w']}, under the panel's left edge at {edge}")
    return failures


def fitting_teammate(hired, definitions):
    """The hire DELEGATE_TASK (newsletter subject lines, a marketing job) fits: the first whose
    saved definition speaks of marketing most; the first hired when none does. Most, not first:
    nightly 57's Beth the bookkeeper mentioned marketing (spend) once, was saved first, and got the
    newsletter task, while a marketing hire's definition speaks of it throughout."""
    counts = {name: (definitions.get(name) or '').lower().count('market') for name in hired}
    best = max(hired, key=lambda name: counts[name])
    return best if counts[best] else hired[0]


def delegate_verdict(record, receipts, name):
    """Why the task sent after the relaunch does NOT show `name` doing it ([] = it does). With
    `name` None (--ask-rich: no teammate named) only the task's own outcome is judged.

    The evidence is the back end's own dispatch receipts (engine/mega-lander/app.py, `folder`:
    <app data>/engine-state/work-receipts/<hash>/*.json), taken only where the receipt's
    obligation is THIS assignment's obligation, so another job's or another session's run of the
    same teammate cannot count. A consult's answer is `consult_answer`, which the app sets only
    from that run's own hand-back or its SubagentStop (`observe`), never from the Agent call's
    immediate acknowledgment; it must say answered and carry a message."""
    failures = []
    notices = (record or {}).get('notices') or []
    if not record or record.get('state') != 'settled':
        failures.append(f"the task ended {(record or {}).get('state')}: {(record or {}).get('detail')}")
    if any('No work was started' in n.get('text', '') for n in notices):
        failures.append('he was told "No work was started"')
    if not notices or not notices[-1].get('text', '').strip():
        failures.append('he was told nothing about the result')
    obligation = (record or {}).get('obligation_id')
    mine = [r for r in receipts if obligation and (r.get('request') or {}).get('obligation_id') == obligation]
    if name is None:
        return failures
    runs = [r for r in mine if (r.get('request') or {}).get('teammate') == name]
    if not runs:
        failures.append(f"no run of {name} was dispatched for this task; its runs were "
                        f"{[(r.get('request') or {}).get('teammate') for r in mine]}")
    answered = [r for r in runs if (r.get('consult_answer') or {}).get('answered')
                and str((r.get('consult_answer') or {}).get('message', '')).strip()]
    if runs and not answered:
        failures.append(f'{name} was dispatched for this task and its run handed back no answer')
    return failures


def answered_in_turn(turns, asked):
    """Rich's reply to the message `asked`, once that turn COMPLETED with words in it, else None:
    a job asked of Rich (--ask-rich) may be answered in his own turn, with no assignment at all.
    An interrupted, stopped or failed turn is no answer, whatever partial text it carries."""
    mine = [t for t in turns if t['prompt'].strip() == asked.strip()]
    if mine and mine[-1].get('end') == 'TurnCompleted' and mine[-1]['reply'].strip():
        return mine[-1]['reply']
    return None


def answer_of(receipts, record, name):
    for r in receipts:
        request = r.get('request') or {}
        if request.get('obligation_id') == (record or {}).get('obligation_id') and request.get('teammate') == name:
            answer = (r.get('consult_answer') or {}).get('message')
            if answer:
                return answer
    return ''


def read_timeout(asked, deadline):
    """The seconds the host waits for one ax.sh call: what the caller asked, or the guest's read
    deadline (TESTVM_AX_TIMEOUT) plus 25 s for the transport, whichever is longer. adopt-walk's
    ax() waits 40 s, so a 60 s guest deadline was cut off by the host first (walk 4 of
    ray-opus-test55, 2026-10-13: `timed out after 40 seconds` in go)."""
    return max(asked, deadline + 25)


class HireWalk(command_walk.CommandWalk):
    def ax(self, mode, *args, app=None, timeout=40):
        deadline = int(os.environ.get('TESTVM_AX_TIMEOUT', '20'))
        return super().ax(mode, *args, app=app, timeout=read_timeout(timeout, deadline))

    def after_relaunch(self):
        """After the relaunch the guest is busy before the window can answer a read: the app runs
        its speech-speed check (2 to 5 whisper-cli at 50-140% CPU each, guest idle 0%) and the home
        screen animates, so every accessibility read took 16-35 s and the 'Onboarding' click hit
        the read deadline in 3 of 3 runs (ray-opus-test55, 2026-10-13, runs 1, 3 and 5). Wait,
        up to --settle-within seconds, until no whisper-cli runs, then give every later read
        --after-relaunch-deadline seconds. Recorded in relaunch-observed.json as `settled`."""
        began = time.monotonic()
        end = began + self.a.settle_within
        counts = []
        while time.monotonic() < end:
            running = guest(self.vm, 'pgrep -x whisper-cli | wc -l', 60).strip()
            counts.append(running)
            if running == '0':
                break
            time.sleep(5)
        os.environ['TESTVM_AX_TIMEOUT'] = str(int(self.a.after_relaunch_deadline))
        return {'whisper_cli_counts': counts, 'seconds': round(time.monotonic() - began, 1),
                'ax_deadline_s': int(self.a.after_relaunch_deadline)}

    def type_into(self, text, *selector, app=None):
        """One retry when the guest's accessibility read hits its own deadline (exit 124,
        `guest_deadline`): walk 1 of 2026-10-13 stopped there with the host at 100% CPU. Typing
        with --replace sets the field's whole value, so a second try cannot type it twice."""
        # Walk 8 hit the deadline on two tries in a row while the just-launched app was busy, so up
        # to four tries, 10 s apart.
        for attempt in range(4):
            try:
                return super().type_into(text, *selector, app=app)
            except StepFailed as exc:
                if not axwait.no_answer(exc) or attempt == 3:
                    raise
                time.sleep(10)

    # present() is adopt-walk's, whose find with no answer is "not seen yet" (axwait.py; walk 3 of
    # 2026-10-13: `find --title Start the questions` exited 124 after 16.3 s, host at 100% CPU).

    def approve_if_asked(self, title):
        """command-walk.py's one Approve press, searched by the start of the title, and a poll the
        accessibility read could not answer in time counts as "not asked this time" (walk 2 of
        2026-10-13: the search for a 150-character hiring title hit the read's 20 s deadline on a
        host at 100% CPU and ended the step). The next poll asks again."""
        try:
            return super().approve_if_asked(title[:40])
        except StepFailed as exc:
            if not axwait.no_answer(exc):
                raise
            return False

    def evidence_shot(self, name):
        """A screenshot kept as evidence only: a shot that fails is recorded, never the step's verdict."""
        try:
            self.shot(name)
        except (StepFailed, subprocess.TimeoutExpired) as exc:
            (self.out / (name + '.failed.txt')).write_text(str(exc) + '\n')

    def keep_logs(self):
        """command-walk.py's logs, plus every conversation as stored: what Rich said in all of
        them, the hiring's progress messages included, is the record a hunt reads."""
        kept = super().keep_logs()
        text = guest(self.vm, 'tail -c 3000000 ' + shlex.quote(self.data + '/conversation-ledger.jsonl')
                     + ' 2>/dev/null || true', 120)
        (self.out / 'conversation-ledger.jsonl').write_text(text + '\n')
        kept['conversation-ledger.jsonl'] = len(text)
        return kept

    def ledger(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        rows = []
        for line in text.splitlines():
            try:
                rows.append(json.loads(line))
            except ValueError:
                pass
        return rows

    def turns(self):
        return turns_of(self.ledger(), self.facts.get('thread'))

    def told_since(self, since_ms):
        """Every proactive message on the conversation since `since_ms`: how background results reach him."""
        thread = self.facts.get('thread')
        return [r.get('text', '') for r in self.ledger() if r.get('event') == 'ProactiveMessage'
                and r.get('thread_id') == thread and r.get('at', 0) >= since_ms]

    def said_since(self, since_ms):
        """Rich's own replies on the conversation since `since_ms` (AssistantDelta), joined."""
        # An AssistantDelta row carries its turn_id and no thread_id (nightly 56 run 4 read nothing),
        # so the conversation's turns are taken from the rows that name the thread.
        thread = self.facts.get('thread')
        rows = self.ledger()
        turns = {r.get('turn_id') for r in rows if r.get('thread_id') == thread and r.get('turn_id')}
        return ''.join(r.get('text', '') for r in rows if r.get('event') == 'AssistantDelta'
                       and r.get('turn_id') in turns and r.get('at', 0) >= since_ms)

    def wait_turns_done(self, count, seconds):
        """Wait until at least `count` visible turns exist and the last is finished; return them."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            turns = self.turns()
            if len(turns) >= count and turns[-1]['done']:
                return turns
            time.sleep(3)
        raise StepFailed(f'turn {count} did not finish within {seconds:.0f} s')

    def first_run(self):
        self.decline_first_run_sheets()
        guest(self.vm, 'mkdir -p ' + shlex.quote(self.company))
        self.wait_for('Add this company')
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        self.wait_for('Start the questions', seconds=60)
        self.press('Start the questions')
        end = time.monotonic() + 60
        while time.monotonic() < end:
            created = [r for r in self.ledger() if r.get('event') == 'ThreadCreated']
            if created:
                self.facts['thread'] = created[0]['thread_id']
                self.facts['thread_title'] = created[0].get('title')
                self.save()
                return {'thread': self.facts['thread'], 'title': self.facts['thread_title']}
            time.sleep(1)
        raise StepFailed('no conversation was created after the company was added')

    def stock(self):
        # The front desk's lease starts with the first conversation, and on a quick first run the
        # walk gets here before it has (walk 5 of 2026-10-13): wait for it, bounded.
        end, leases = time.monotonic() + 180, []
        while time.monotonic() < end and not leases:
            leases = self.leases()
            if not leases:
                time.sleep(3)
        if not leases:
            raise StepFailed('no running lease was found in the guest within 180 s')
        missing = [n for n in STOCK for l in leases if f'./agents/{n}.md' not in (l.get('agents') or [])]
        (self.out / 'stock-observed.json').write_text(json.dumps(leases, indent=2) + '\n')
        if missing:
            raise StepFailed('a running lease does not register: ' + ', '.join(sorted(set(missing))))
        return {'leases': len(leases), 'registered': sorted(STOCK)}

    def interview(self):
        record = []
        turns = self.wait_turns_done(1, self.a.turn_within)
        record.append(turns[-1])
        sends = list(ANSWERS) + [WRAP_UP] * 3
        for text in sends:
            if GO_WORDS in turns[-1]['reply'].replace('’', "'"):
                break
            self.send(text)
            turns = self.wait_turns_done(len(turns) + 1, self.a.turn_within)
            record.append(turns[-1])
            (self.out / 'interview.json').write_text(json.dumps(record, indent=2) + '\n')
        final = turns[-1]['reply']
        names, failures = proposal_verdict(final)
        self.facts['proposed'] = names
        self.save()
        if failures:
            raise StepFailed('; '.join(failures))
        return {'turns': len(record), 'proposed': names, 'message': final}

    def team_files(self):
        listing = guest(self.vm, 'ls ' + shlex.quote(self.data + '/team') + ' 2>/dev/null || true')
        return sorted(Path(n).stem for n in listing.split() if n.endswith('.md'))

    def go(self):
        proposed = self.facts.get('proposed') or []
        if not proposed:
            raise StepFailed('interview must have run (no proposed teammates on record)')
        sent = self.send('Go.')
        pressed = [0]
        # The skill splits a long list into more than one hiring task, so every assignment
        # registered after "Go." is followed until all of them have closed.
        thread = self.facts.get('thread')
        end, records = time.monotonic() + self.a.hire_within, []
        while time.monotonic() < end:
            records = [r for r in self.records() if r.get('thread_id') == thread and r.get('registered_at_ms', 0) >= sent[0]]
            if records and all(r.get('state') not in command_walk.OPEN for r in records):
                break
            for r in records:
                if r.get('state') in command_walk.OPEN and pressed[0] < self.a.approvals and self.approve_if_asked(r.get('title', '')):
                    pressed[0] += 1
            time.sleep(3)
        records = [self.settled_read(r) for r in records]
        saved = self.team_files()
        told = [n.get('text', '') for r in records for n in r.get('notices', [])] + self.told_since(sent[0])
        said = self.said_since(sent[0])
        evidence = {'proposed': proposed, 'assignments': records, 'saved': saved, 'told': told,
                    'said': said, 'approvals_pressed': pressed[0]}
        (self.out / 'go-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not records:
            raise StepFailed('"Go." registered no assignment')
        still = [r.get('state') for r in records if r.get('state') in command_walk.OPEN]
        if still:
            raise StepFailed(f"the hiring was still {still} after {self.a.hire_within:g} s")
        failures = hired_verdict(proposed, saved, '\n'.join(told))
        retried = first_send_verdict('\n'.join(told + [said]))
        if retried:
            failures.append(f'he was told the write-up was retried: {retried}')
        # Counted here and judged in `approvals`, the last step, so one card does not hide whether
        # relaunch, roster and delegation work.
        self.facts['approvals_go'] = pressed[0]
        self.save()
        # Whoever WAS saved is the team the later steps walk, whatever the verdict here, so one
        # missing hire does not hide whether the relaunch, the roster, a delegated job and Under
        # the hood work (nightly 56: Beth was not saved in 2 of 3 runs and nothing after go ran).
        self.facts['hired'] = saved
        self.save()
        if failures:
            raise StepFailed('; '.join(failures))
        return {'hired': saved, 'states': [r.get('state') for r in records], 'told': told[-1] if told else ''}

    def roster(self):
        hired = self.facts.get('hired') or []
        roster = sorted(Path(n).stem for n in guest(self.vm, 'ls ' + shlex.quote(
            self.data + '/coordination/.claude/agents')).split())
        missing = [n for n in list(STOCK) + hired if n not in roster]
        if missing:
            raise StepFailed('after the relaunch the roster lacks: ' + ', '.join(missing))
        return {'roster': roster}

    def approvals(self):
        """The CEO says go and the team is hired: an Approve card put in front of him by the
        hiring after his go fails. A card during the delegated task is reported, not judged here."""
        go, delegate = self.facts.get('approvals_go'), self.facts.get('approvals_delegate')
        if go is None:
            raise StepFailed('go must have run (no approval count on record)')
        if go:
            raise StepFailed(f'he was asked to approve {go} step(s) of the hiring after his go '
                             f'(and {delegate} during the delegated task); see callbacks.txt for the commands')
        return {'approvals_during_hiring': go, 'approvals_during_delegated_task': delegate}

    def receipts(self):
        """The back end's dispatch receipts, read-only: request (teammate, role, obligation_id),
        status and consult_answer, each bounded."""
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/work-receipts/*/*.json"):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    q=r.get("request") or {}\n'
                  '    a=r.get("consult_answer") or {}\n'
                  '    out.append({"file":p.rsplit("/",2)[-1],"status":r.get("status"),\n'
                  '                "request":{k:q.get(k) for k in ("teammate","role","obligation_id","title")},\n'
                  '                "consult_answer":{"answered":a.get("answered"),"source":a.get("source"),\n'
                  '                                  "message":str(a.get("message",""))[:4000]}})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def delegate(self):
        hired = self.facts.get('hired') or []
        if not hired:
            raise StepFailed('go must have run (nobody hired on record)')
        # The task is a marketing one, so it goes to the hire whose saved definition fits it, as a
        # person would ask. The walk of main 0a2ae649f gave it to hired[0], Beth the bookkeeper;
        # Rich rightly asked who should do it, and the walk never answered his card (911 s).
        definitions = {n: guest(self.vm, 'cat ' + shlex.quote(f'{self.data}/team/{n}.md') + ' 2>/dev/null || true')
                       for n in hired}
        name = None if self.a.ask_rich else fitting_teammate(hired, definitions)
        # The screen and the conversation around the message, kept whatever the verdict.
        self.evidence_shot('delegate-1-before.png')
        sent = self.send(self.a.ask_rich or DELEGATE_TASK.format(Name=name.capitalize()))
        time.sleep(5)
        self.evidence_shot('delegate-2-sent.png')
        pressed, answered = [0], []
        if self.a.ask_rich:
            # Asked of Rich, naming nobody: he may answer in his own turn and register nothing.
            # The turn is followed first; only when it ends registering an assignment is that
            # assignment followed below (ask-rich walk of nightly 57: he answered in 7.8 s, and
            # the walk then waited 942 s for an assignment that never existed).
            end, reply = time.monotonic() + self.a.turn_within, None
            while time.monotonic() < end and reply is None:
                reply = answered_in_turn(turns_of(self.ledger(), self.facts.get('thread')), self.a.ask_rich)
                if reply is None:
                    time.sleep(3)
            time.sleep(10)  # an assignment the turn registers is written as the turn ends
            # A question card he has not answered means the job is not done: it is answered
            # below (pick) as a person answers it, and the walk follows what comes after.
            if reply is not None and not self.ours(sent) and not self.present(RECOMMENDS):
                self.evidence_shot('delegate-3-end.png')
                since = [r for r in self.ledger() if r.get('at', 0) >= sent[0] - 1000]
                (self.out / 'delegate-ledger.json').write_text(json.dumps(since, indent=2) + '\n')
                evidence = {'teammate': None, 'asked': self.a.ask_rich, 'dispatched': [], 'assignment': None,
                            'answered_in_turn': reply}
                (self.out / 'delegate-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
                return {'teammate': None, 'dispatched': [], 'state': 'answered in the turn', 'answer': reply[:1000],
                        'notice': ''}

        def pick():
            if not answered and self.present(RECOMMENDS):
                # Rich asked a question card: it is on screen, and a person answers it with the
                # option Rich recommends, by click.
                self.evidence_shot('delegate-question.png')
                self.press(RECOMMENDS)
                answered.append(self.clock())
            return self.ours(sent)
        record = self.settled_read(self.until_closed(pick, self.a.within, pressed))
        self.evidence_shot('delegate-3-end.png')
        since = [r for r in self.ledger() if r.get('at', 0) >= sent[0] - 1000]
        (self.out / 'delegate-ledger.json').write_text(json.dumps(since, indent=2) + '\n')
        receipts = self.receipts()
        self.facts['approvals_delegate'] = pressed[0]
        self.save()
        obligation = (record or {}).get('obligation_id')
        dispatched = [(r.get('request') or {}).get('teammate') for r in receipts
                      if obligation and (r.get('request') or {}).get('obligation_id') == obligation]
        evidence = {'teammate': name, 'asked': self.a.ask_rich, 'dispatched': dispatched, 'assignment': record, 'receipts': receipts, 'approvals_pressed': pressed[0],
                    'question_card_answered_at_guest_ms': answered[0] if answered else None}
        (self.out / 'delegate-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = delegate_verdict(record, receipts, name)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'teammate': name, 'dispatched': dispatched, 'state': record.get('state'),
                'answer': answer_of(receipts, record, name)[:1000] if name else '',
                'notice': record['notices'][-1].get('text', '')}

    def hood(self):
        """Under the hood opened from the work summary chip; nothing of the conversation or the
        composer runs under its left edge (echo-opus-panel55's fix, found on nightly 55's walk)."""
        if not self.present('back to Rich'):
            self.ax('click', '--title', 'Open the work summary', '--contains', '--first')
        end = time.monotonic() + 30
        while time.monotonic() < end and not self.present('back to Rich'):
            time.sleep(2)

        def node(dom_id):
            try:
                found = [n for n in self.ax('find', '--id', dom_id, '--first') if not n.get('meta')]
            except StepFailed as exc:
                if axwait.not_yet(exc, 'find'):
                    return None
                raise
            return found[0] if found else None
        panel = node('slideover')
        # #messages has no node in the guest's accessibility tree (nightly 56 run 4: "not on
        # screen" with the conversation plainly there), so the boxes are the two Output buttons,
        # the stage header's and the composer's; the last card is in under-the-hood.png.
        boxes = {'the top Output button (#out-top)': node('out-top'), 'the Output button (#out-bottom)': node('out-bottom')}
        self.evidence_shot('under-the-hood.png')
        evidence = {'panel': panel, 'boxes': boxes}
        (self.out / 'hood-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = hood_verdict(panel, boxes)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'panel_left': panel['x'], **{k: v['x'] + v['w'] for k, v in boxes.items()}}

def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--turn-within', type=float, default=240, help='seconds for one interview turn to finish')
    p.add_argument('--hire-within', type=float, default=2400, help='seconds for the hiring to close')
    p.add_argument('--within', type=float, default=900, help='seconds for the delegated task to close')
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on one assignment')
    p.add_argument('--relaunch-within', type=float, default=120, help='seconds for the composer to come back')
    p.add_argument('--settle-within', type=float, default=300,
                   help='seconds to wait after the relaunch for the guest to stop running whisper-cli')
    p.add_argument('--after-relaunch-deadline', type=float, default=60,
                   help='the guest accessibility read deadline (TESTVM_AX_TIMEOUT) after the relaunch')
    p.add_argument('--ask-rich', default=None, metavar='TEXT',
                   help='delegate asks Rich for TEXT, naming no teammate, and records who did it')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = HireWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        # A go that saved somebody carries on (see go); any other failure ends the walk.
        if row['outcome'] == 'FAIL' and not (step == 'go' and walk.facts.get('hired')):
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

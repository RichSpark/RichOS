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
              one named in what he was told, and nobody named to him as a teammate who was not
              saved. Evidence: go-observed.json.
  relaunch    command-walk.py's relaunch: the app stopped by pid and started again
  roster      after the relaunch, the lease's roster holds the five and every hired teammate
  delegate    "Please ask <first hired> for three subject lines ..." typed; PASS needs, in the hook
              records written after that send only: an Agent call naming that teammate and its
              finished run handing back an answer; and the task closed settled with a result he was
              told. Evidence: delegate-observed.json.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
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

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed

STEPS = ['identity', 'first-run', 'stock', 'interview', 'go', 'relaunch', 'roster', 'delegate']
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


def hired_verdict(proposed, saved, told):
    """Every proposed teammate saved; every saved teammate named in what he was told; and nobody
    he was told about as a teammate ("Name (Job title)") who is neither saved nor one of the five."""
    failures = []
    told_lower = (told or '').lower()
    for claimed in sorted({m.lower() for m in re.findall(r'\b([A-Z][a-z]+)\s+\(', told or '')}):
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


def dispatched_teammates(callback_rows):
    """The teammates the back end's Agent calls named, from its hook records."""
    out = []
    for c in callback_rows:
        if c.get('hook_event_name') == 'PreToolUse' and c.get('tool_name') == 'Agent':
            kind = (c.get('tool_input') or {}).get('subagent_type', '')
            out.append(kind.split(':', 1)[-1])
    return out


def answered_by(callback_rows, name):
    """The answers `name` handed back: each PostToolUse of an Agent call that named it, non-empty."""
    out = []
    for c in callback_rows:
        kind = ((c.get('tool_input') or {}).get('subagent_type') or '').split(':', 1)[-1]
        if c.get('hook_event_name') == 'PostToolUse' and c.get('tool_name') == 'Agent' and kind == name:
            answer = c.get('tool_response')
            if answer:
                out.append(answer if isinstance(answer, str) else json.dumps(answer))
    return out


def delegate_verdict(record, new_rows, name):
    """Why the task sent after the relaunch does NOT show `name` doing it ([] = it does). `new_rows`
    are only the hook records written after that task was sent, so an earlier job's dispatch of
    the same teammate cannot count."""
    failures = []
    notices = (record or {}).get('notices') or []
    if not record or record.get('state') != 'settled':
        failures.append(f"the task ended {(record or {}).get('state')}: {(record or {}).get('detail')}")
    if any('No work was started' in n.get('text', '') for n in notices):
        failures.append('he was told "No work was started"')
    if not notices or not notices[-1].get('text', '').strip():
        failures.append('he was told nothing about the result')
    if name not in dispatched_teammates(new_rows):
        failures.append(f'the back end did not dispatch {name} for this task; it dispatched {dispatched_teammates(new_rows)}')
    if not answered_by(new_rows, name):
        failures.append(f'{name} handed back no answer for this task')
    return failures


class HireWalk(command_walk.CommandWalk):
    def type_into(self, text, *selector, app=None):
        """One retry when the guest's accessibility read hits its own deadline (exit 124,
        `guest_deadline`): walk 1 of 2026-10-13 stopped there with the host at 100% CPU. Typing
        with --replace sets the field's whole value, so a second try cannot type it twice."""
        try:
            return super().type_into(text, *selector, app=app)
        except StepFailed as exc:
            if 'guest_deadline' not in str(exc):
                raise
            time.sleep(5)
            return super().type_into(text, *selector, app=app)

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
        leases = self.leases()
        if not leases:
            raise StepFailed('no running lease was found in the guest')
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
        record = self.settled_read(self.until_closed(lambda: self.ours(sent), self.a.hire_within, pressed))
        saved = self.team_files()
        told = [n.get('text', '') for n in (record or {}).get('notices', [])] + self.told_since(sent[0])
        evidence = {'proposed': proposed, 'assignment': record, 'saved': saved, 'told': told,
                    'approvals_pressed': pressed[0]}
        (self.out / 'go-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not record:
            raise StepFailed('"Go." registered no assignment')
        if record.get('state') in command_walk.OPEN:
            raise StepFailed(f"the hiring was still {record.get('state')} after {self.a.hire_within:g} s")
        failures = hired_verdict(proposed, saved, '\n'.join(told))
        if failures:
            raise StepFailed('; '.join(failures))
        self.facts['hired'] = saved
        self.save()
        return {'hired': saved, 'state': record.get('state'), 'told': told[-1] if told else ''}

    def roster(self):
        hired = self.facts.get('hired') or []
        roster = sorted(Path(n).stem for n in guest(self.vm, 'ls ' + shlex.quote(
            self.data + '/coordination/.claude/agents')).split())
        missing = [n for n in list(STOCK) + hired if n not in roster]
        if missing:
            raise StepFailed('after the relaunch the roster lacks: ' + ', '.join(missing))
        return {'roster': roster}

    def callback_rows(self):
        """Every Agent hook record in the guest, each keyed by its file and line, so the rows written
        after a moment can be told from the ones before it."""
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    for i,l in enumerate(open(p)):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("tool_name")!="Agent": continue\n'
                  '        r=c.get("tool_response")\n'
                  '        out.append({"key":p+":"+str(i),"hook_event_name":c.get("hook_event_name"),"tool_name":"Agent",\n'
                  '                    "tool_input":{"subagent_type":(c.get("tool_input") or {}).get("subagent_type")},\n'
                  '                    "tool_response":(r if isinstance(r,str) else json.dumps(r) if r else "")[:4000]})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def delegate(self):
        hired = self.facts.get('hired') or []
        if not hired:
            raise StepFailed('go must have run (nobody hired on record)')
        name = hired[0]
        before = {r['key'] for r in self.callback_rows()}
        sent = self.send(DELEGATE_TASK.format(Name=name.capitalize()))
        pressed = [0]
        record = self.settled_read(self.until_closed(lambda: self.ours(sent), self.a.within, pressed))
        new_rows = [r for r in self.callback_rows() if r['key'] not in before]
        evidence = {'teammate': name, 'assignment': record, 'agent_rows_after_send': new_rows}
        (self.out / 'delegate-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = delegate_verdict(record, new_rows, name)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'teammate': name, 'state': record.get('state'), 'answer': answered_by(new_rows, name)[-1][:1000],
                'notice': record['notices'][-1].get('text', '')}


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
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

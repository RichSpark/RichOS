#!/usr/bin/env python3
"""lease-setup-walk.py — a regular install runs its leases under the user's own setup.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone.
The fixture home is made first, then the walk:

  lease-setup-walk.py --seed-home FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      lease-setup-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. Build plan richos-hq docs/plans/2026-10-21-everything-for-every-user.md, branch L
(items 0, 4, 8 and 10), section 6's walk, with the CEO's change of 2026-10-21 to item 0: the regular
app runs under what the USER set up, read the same way for every user, nothing of anyone hard-coded.
The fixture home is a fictional user's: a login profile, a ~/.gitconfig with a name, an email and a
credential helper, one teammate of their own; its orchestration.config is written mid-walk.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  first-run   adopt-walk.py's: company "Acme" with a folder that is a Git repository
  connect     command-walk.py's: that folder connected
  copies      the shipped rich.md and stu.md and an empty instructions.md are in ~/myrichos
  remote      a password-protected Git remote over HTTP (git-http-stand-in.py) is the Acme
              folder's origin
  job         JOB typed: one command in the Acme folder prints the login profile's mark and finds
              the profile's tool on PATH, commits, and pushes to origin. PASS needs: the command's
              own output (the back end's hook record) with the mark and the tool's path; the commit
              authored and committed by the fixture's ~/.gitconfig name and email; that commit in
              the remote; the remote's log showing the push authenticated as the fixture's user,
              and the fixture's credential helper's own log showing it was asked.
  model       per-job model (build plan item 7). The fixture's teammate walkhaiku, whose definition
              names haiku, asked for on sonnet for one job: PASS needs its receipt named
              walkhaiku-sonnet-*, and the worker's own start record (subagents/agent-<id>.meta.json)
              saying sonnet. Then on fable, with no reason given: PASS needs the receipt that asked
              for fable to carry the app's refusal ("more expensive model") and no worker to have
              started for it. No orchestration.config of the user's yet, so the ceiling is the
              engine's (opus).
  config      ~/myrichos/orchestration.config written to allow only sonnet and haiku (as an
              `export` line); the generated config carries it once and plain; a job for the
              fixture's own teammate, whose definition names opus, is refused by the app with the
              user's list ("one of sonnet, haiku"), in the back end's own records.
  rich-edit   a rule appended to the copied ~/myrichos/rich.md; in a new conversation (a session
              that starts after the edit) Rich's reply starts with the rule's word.
  instructions  a rule written into the user's own ~/myrichos/instructions.md (empty until now);
              in a new conversation Rich's reply ends with the rule's word.
  memory      Account 2 "Work" added at the folder run.sh signed in; Rich asked to save a code
              word to his auto-memory; the memory file is under <app data>/coordination/.claude/
              agent-memory (his definition's `memory: project`); the account in use
              moved to Work and the app relaunched; in a NEW conversation, on a lease whose
              CLAUDE_CONFIG_DIR is Work's folder, Rich answers with the code word.
Evidence in --out: report.json, facts.json, observed-*.json, screenshots, the app's logs.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO §54).
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import random
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN
command = command_walk.command

STEPS = ['identity', 'first-run', 'connect', 'copies', 'remote', 'job', 'model', 'config', 'rich-edit', 'instructions', 'memory']
MARK = 'lease-setup-walk-login-mark'
NAME, EMAIL = 'Walk Person', 'walk.person@example.invalid'
REMOTE_USER, REMOTE_SECRET = 'walk-user', 'walk-secret-not-a-credential'
PORT = 18765
TEAMMATE = 'walkmate'
MODEL_MATE = 'walkhaiku'
RULE_WORD = 'PINEAPPLE'
OWN_WORD = 'LIGHTHOUSE'
WORK_FOLDER = 'claude-accounts/2'
JOB = ('Please run this harmless test command for me yourself with your shell tool, in my Acme folder, '
       'and tell me when it has finished and what it printed: '
       'printf "%s\\n" "$FIXTURE_LOGIN_MARK"; command -v fixture-tool; echo walk >> walk.txt; '
       'git add walk.txt; git commit -q -m "Walk commit"; git push -q origin HEAD:main; git log -1 --format="%an <%ae>"')
USER_CONFIG = "# The fixture user's own values.\nexport ALLOWED_MODELS=\"sonnet haiku\"\n"
CONFIG_JOB = f'Please ask {TEAMMATE} to reply with the single word ready, and tell me what happened.'
NAMED_MODEL_JOB = (f'Please ask {MODEL_MATE} to reply with the single word ready, running it on the sonnet model '
                   'for this one job, and tell me what happened.')
ABOVE_CEILING_JOB = (f'Please ask {MODEL_MATE} to reply with the single word ready, running it on the fable model '
                     'for this one job. Give no reason for the model and add no acknowledgment line; if the app '
                     'refuses it, do not retry on another model, just tell me exactly what it said.')


def seed_home(home):
    """The fictional user's own setup, written into an empty fixture home."""
    home = Path(home)
    (home / 'fixture-bin').mkdir(parents=True, exist_ok=True)
    (home / '.zprofile').write_text(f'export FIXTURE_LOGIN_MARK={MARK}\nexport PATH="$HOME/fixture-bin:$PATH"\n')
    tool = home / 'fixture-bin/fixture-tool'
    tool.write_text('#!/bin/sh\necho fixture-tool\n')
    helper = home / 'fixture-bin/git-credential-fixture'
    helper.write_text('#!/bin/sh\n# The fixture user\'s credential helper: answers get, logs that it was asked.\n'
                      'echo "$(date +%s) $1" >> "$HOME/credential-helper.log"\n'
                      f'if [ "$1" = get ]; then echo username={REMOTE_USER}; echo password={REMOTE_SECRET}; fi\n')
    for path in (tool, helper):
        path.chmod(0o755)
    (home / '.gitconfig').write_text(f'[user]\n\tname = {NAME}\n\temail = {EMAIL}\n[credential]\n\thelper = fixture\n')
    team = home / 'Library/Application Support/com.richos.app/team'
    team.mkdir(parents=True, exist_ok=True)
    (team / f'{TEAMMATE}.md').write_text(f'---\nname: {TEAMMATE}\ndescription: Fictional walk teammate.\nmodel: opus\n'
                                         'tools: Read\n---\n\nA fictional teammate written by lease-setup-walk.py.\n')
    (team / f'{MODEL_MATE}.md').write_text(f'---\nname: {MODEL_MATE}\ndescription: Fictional walk teammate for one-word '
                                           'replies.\nmodel: haiku\ntools: Read\n---\n\nA fictional teammate written by '
                                           'lease-setup-walk.py. Reply with exactly the word you are asked for.\n')
    print(f'seeded {home}')
    return 0


class LeaseSetupWalk(command_walk.CommandWalk):
    def central(self):
        return self.home + '/myrichos'

    def turns(self, thread):
        rows = [json.loads(r) for r in guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl'), 60).splitlines() if r.strip()]
        turns, by_id = [], {}
        for r in rows:
            if r.get('event') == 'PromptReceived' and r.get('thread_id') == thread and r.get('source') not in ('internal', 'proactive'):
                t = {'turn': r['turn_id'], 'prompt': r.get('text', ''), 'reply': '', 'done': False}
                turns.append(t)
                by_id[r['turn_id']] = t
            elif r.get('event') == 'AssistantDelta' and r.get('turn_id') in by_id:
                by_id[r['turn_id']]['reply'] += r.get('text', '')
            elif r.get('event') in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed') and r.get('turn_id') in by_id:
                by_id[r['turn_id']]['done'] = True
                by_id[r['turn_id']]['end'] = r.get('event')
                by_id[r['turn_id']]['end_row'] = {k: v for k, v in r.items() if k not in ('text',)}
        return turns, rows

    def reply_to(self, text, seconds=240):
        """Send `text` in the conversation on record and answer Rich's finished reply to it."""
        thread = self.facts['thread']
        before = len(self.turns(thread)[0])
        self.send(text)
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            mine = [t for t in self.turns(thread)[0][before:] if t['prompt'].strip() == text.strip()]
            if mine and mine[-1]['done']:
                return mine[-1]['reply']
            time.sleep(3)
        raise StepFailed(f'Rich did not finish a reply to "{text[:60]}" within {seconds} s')

    def copies(self):
        found = {}
        end = time.monotonic() + 120
        while time.monotonic() < end:
            found = {name: guest(self.vm, 'test -f ' + shlex.quote(self.central() + '/' + name) + ' && wc -c < '
                                 + shlex.quote(self.central() + '/' + name) + ' || echo absent').strip()
                     for name in ('rich.md', 'stu.md', 'instructions.md')}
            if 'absent' not in found.values():
                break
            time.sleep(3)
        engine = self.payload + '/engine/app/agents/'
        same = {name: guest(self.vm, 'cmp -s ' + shlex.quote(engine + name) + ' ' + shlex.quote(self.central() + '/' + name)
                            + ' && echo same || echo different') for name in ('rich.md', 'stu.md')}
        self.facts['copies'] = {'bytes': found, 'as_shipped': same}
        self.save()
        if 'absent' in found.values() or found.get('instructions.md') != '0' or set(same.values()) != {'same'}:
            raise StepFailed('the first-run copies are not as shipped: ' + json.dumps(self.facts['copies']))
        return self.facts['copies']

    def remote(self):
        root = self.home + '/remote-root'
        backend = self.payload + '/engine/runtime/git/libexec/git-core/git-http-backend'
        script = self.payload + '/git-http-stand-in.py'
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'git-http-stand-in.py', script], 60)
        guest(self.vm, f'mkdir -p {shlex.quote(root)} && git init -q --bare {shlex.quote(root + "/acme.git")}')
        guest(self.vm, f'nohup python3 {shlex.quote(script)} {PORT} {shlex.quote(root)} {REMOTE_USER} {REMOTE_SECRET} '
                       f'{shlex.quote(self.payload + "/remote.log")} {shlex.quote(backend)} > /dev/null 2>&1 &')
        guest(self.vm, 'git -C ' + shlex.quote(self.company) + f' remote add origin http://127.0.0.1:{PORT}/acme.git')
        time.sleep(2)
        anonymous = guest(self.vm, 'curl -s -o /dev/null -w "%{http_code}" '
                          + shlex.quote(f'http://127.0.0.1:{PORT}/acme.git/info/refs?service=git-receive-pack'))
        if anonymous.strip() != '401':
            raise StepFailed('the stand-in remote did not refuse an anonymous push: ' + anonymous)
        return {'origin': f'http://127.0.0.1:{PORT}/acme.git', 'anonymous': anonymous.strip()}

    def job(self):
        sent = self.send(JOB)
        self.facts['job_sent_ms'] = sent
        self.save()
        end = time.monotonic() + self.a.within
        record = None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                break
            if record:
                self.approve_if_asked(record.get('title', ''))
            time.sleep(3)
        commands = self.evidence_commands()
        ran = next((c for c in commands if 'FIXTURE_LOGIN_MARK' in c['command']), None)
        local = guest(self.vm, 'git -C ' + shlex.quote(self.company) + ' log -1 --format="%H|%an <%ae>|%cn <%ce>|%s"')
        pushed = guest(self.vm, 'git --git-dir=' + shlex.quote(self.home + '/remote-root/acme.git') + ' log -1 --format=%H main 2>&1 || true')
        remote_log = [json.loads(r) for r in guest(self.vm, 'cat ' + shlex.quote(self.payload + '/remote.log') + ' 2>/dev/null || true').splitlines() if r.strip()]
        helper_log = guest(self.vm, 'cat ' + shlex.quote(self.home + '/credential-helper.log') + ' 2>/dev/null || true')
        evidence = {'assignment': record, 'command': ran, 'local_head': local, 'remote_head': pushed,
                    'remote_requests': remote_log, 'credential_helper_log': helper_log}
        (self.out / 'observed-job.json').write_text(json.dumps(evidence, indent=2) + '\n')
        head, author, committer, subject = (local.split('|') + ['', '', '', ''])[:4]
        failures = []
        if not ran:
            failures.append('no back-end command ran the job')
        else:
            if MARK not in ran['stdout']:
                failures.append('the login profile\'s variable was not in the lease: ' + ran['stdout'][:300])
            if 'fixture-bin/fixture-tool' not in ran['stdout']:
                failures.append('the login profile\'s PATH was not in the lease')
        if subject != 'Walk commit' or author != f'{NAME} <{EMAIL}>' or committer != f'{NAME} <{EMAIL}>':
            failures.append(f'the commit is not the user\'s: {local}')
        if pushed.strip() != head:
            failures.append(f'the remote does not hold the commit: {pushed!r} vs {head!r}')
        if not any(r.get('authenticated_as') == REMOTE_USER and r.get('method') == 'POST' for r in remote_log):
            failures.append('no push reached the remote authenticated as the user')
        if 'get' not in helper_log:
            failures.append('the user\'s credential helper was never asked')
        if failures:
            raise StepFailed('; '.join(failures))
        return {'command': ran['command'], 'stdout': ran['stdout'][:400], 'commit': local, 'pushed': pushed.strip()}

    def callbacks_text(self):
        script = ('import glob,sys\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    print(open(p, errors="replace").read())\n')
        return guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60)

    def model_rows(self):
        """The work receipts for MODEL_MATE and the SubagentStart callbacks of its type, from the guest."""
        script = ('import json,glob,sys\n'
                  'out={"receipts":[],"starts":[]}\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/work-receipts/*/*.json"):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    if r.get("request",{}).get("teammate")==sys.argv[2]:\n'
                  '        out["receipts"].append({k:r.get(k) for k in ("name","status","agent_id","problem","request")})\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("hook_event_name")=="SubagentStart" and str(c.get("agent_type","")).endswith(":"+sys.argv[2]):\n'
                  '            out["starts"].append({"agent_id":c.get("agent_id"),"agent_type":c.get("agent_type")})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data) + ' ' + MODEL_MATE, 60))

    def started_model(self, agent_id):
        """The model the worker's own start record names: Claude Code's subagents/agent-<id>.meta.json
        under the account folder it ran on (handoff-real-walk.py's agent_transcript)."""
        found = guest(self.vm, 'find ' + shlex.quote(self.home) + ' -name ' + shlex.quote('agent-' + agent_id + '.meta.json')
                      + ' -not -path "*/Library/Caches/*" 2>/dev/null | head -1', 120).strip()
        if not found:
            return None, None
        return found, json.loads(guest(self.vm, 'cat ' + shlex.quote(found))).get('model')

    def wait_for_job(self, sent, done):
        """Until `done(rows)` or the assignment closes, within --within seconds; returns the last rows."""
        end = time.monotonic() + self.a.within
        rows = self.model_rows()
        while time.monotonic() < end and not done(rows):
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                rows = self.model_rows()
                break
            time.sleep(5)
            rows = self.model_rows()
        return rows

    def model(self):
        sent = self.send(NAMED_MODEL_JOB)
        named = lambda rows: [r for r in rows['receipts'] if (r.get('name') or '').startswith(MODEL_MATE + '-sonnet-') and r.get('agent_id')]
        rows = self.wait_for_job(sent, named)
        started = None
        for receipt in named(rows):
            path, model = self.started_model(receipt['agent_id'])
            started = {'agent_id': receipt['agent_id'], 'meta': path, 'model': model}
            if model:
                break
        first_reply = self.ours(sent)
        sent_above = self.send(ABOVE_CEILING_JOB)
        asked = lambda rows: [r for r in rows['receipts'] if (r.get('request') or {}).get('model') == 'fable']
        rows = self.wait_for_job(sent_above, lambda rows: any(r.get('problem') for r in asked(rows)))
        evidence = {'named_job': first_reply, 'started': started, 'above_job': self.ours(sent_above), 'rows': rows}
        (self.out / 'observed-model.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = []
        if not named(rows):
            failures.append(f'no worker receipt named {MODEL_MATE}-sonnet-* was started')
        elif not started or not started['model'] or 'sonnet' not in started['model']:
            failures.append('the worker\'s own start record does not say sonnet: ' + json.dumps(started))
        refused = [r for r in asked(rows) if 'more expensive model' in (r.get('problem') or '') and not r.get('agent_id')]
        if not refused:
            failures.append('no receipt asking for fable carries the app\'s refusal: ' + json.dumps(asked(rows)))
        fable_ids = {r.get('agent_id') for r in asked(rows) if r.get('agent_id')}
        if fable_ids:
            failures.append('a worker started on fable: ' + ', '.join(sorted(fable_ids)))
        if failures:
            raise StepFailed('; '.join(failures))
        return {'started': started, 'refused': {k: refused[0].get(k) for k in ('name', 'status', 'problem')}}

    def config(self):
        # Written now, not seeded: until here every lease runs on the defaults, and the change
        # renews Stu at his next message (user_settings::digest), so the next lease reads it.
        guest(self.vm, 'printf ' + shlex.quote(USER_CONFIG) + ' > ' + shlex.quote(self.central() + '/orchestration.config'))
        sent = self.send(CONFIG_JOB)
        end = time.monotonic() + self.a.within
        found = False
        while time.monotonic() < end and not found:
            found = 'one of sonnet, haiku' in self.callbacks_text()
            record = self.ours(sent)
            if record and record.get('state') not in OPEN and not found:
                found = 'one of sonnet, haiku' in self.callbacks_text()
                break
            time.sleep(5)
        generated = guest(self.vm, 'cat ' + shlex.quote(self.data + '/coordination/orchestration.config'))
        (self.out / 'observed-config.json').write_text(json.dumps({'generated': generated, 'refused_with_users_list': found,
                                                                  'assignment': self.ours(sent)}, indent=2) + '\n')
        if 'ALLOWED_MODELS="sonnet haiku"' not in generated or generated.count('ALLOWED_MODELS=') != 1:
            raise StepFailed('the generated config does not carry the user\'s ALLOWED_MODELS once: ' + generated)
        if not found:
            raise StepFailed(f'no refusal naming the user\'s allowed models ("one of sonnet, haiku") was recorded for {TEAMMATE}')
        return {'generated': generated.splitlines(), 'refused_with_users_list': True}

    def rich_edit(self):
        path = self.central() + '/rich.md'
        rule = f'\\n\\n## A rule of this test\\n\\nStart every reply with the word {RULE_WORD}, in capitals.\\n'
        guest(self.vm, 'printf ' + shlex.quote(rule) + ' >> ' + shlex.quote(path))
        reply = self.new_conversation_reply('Say hello in one short sentence.')
        self.facts['rich_edit_reply'] = reply
        self.save()
        self.shot('rich-edit.png')
        if not reply.lstrip().lstrip('*').startswith(RULE_WORD):
            raise StepFailed('the next reply does not follow the edited rich.md: ' + reply[:200])
        return {'reply': reply[:200]}

    def by_id(self, dom_id):
        try:
            return self.ax('find', '--id', dom_id, '--first')
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return None
            raise

    def press_by_id(self, dom_id):
        try:
            return self.ax('click', '--id', dom_id)
        except StepFailed as exc:
            if not axwait.no_answer(exc):
                raise
            return None

    def thread_back(self, seconds=180):
        """After a relaunch: through the home screen's door into the thread, until the composer is back."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            try:
                if self.present('Message to Rich', role='AXTextArea'):
                    return True
                if self.present('Talk to Rich'):
                    self.press('Talk to Rich')
                    time.sleep(2)
                if self.present(self.facts.get('thread_title') or 'Running'):
                    self.press(self.facts.get('thread_title') or 'Running')
            except StepFailed:
                pass
            time.sleep(3)
        raise StepFailed(f'the composer was not back within {seconds} s of the relaunch')

    def instructions(self):
        path = self.central() + '/instructions.md'
        rule = f'End every reply with the word {OWN_WORD}, in capitals, as its last word.\n'
        guest(self.vm, 'printf ' + shlex.quote(rule) + ' >> ' + shlex.quote(path))
        reply = self.new_conversation_reply('Say good morning in one short sentence.')
        self.facts['instructions_reply'] = reply
        self.save()
        self.shot('instructions-edit.png')
        if not reply.rstrip().rstrip('.!*').rstrip().endswith(OWN_WORD):
            raise StepFailed('the next reply does not follow the user\'s instructions.md: ' + reply[-200:])
        return {'reply': reply[:200]}

    def thread_ids(self):
        return [json.loads(r)['thread_id'] for r in guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl')).splitlines()
                if '"ThreadCreated"' in r]

    def new_conversation_reply(self, text, seconds=300):
        """Open a new conversation, send `text` there and answer Rich's finished reply to it."""
        before = set(self.thread_ids())
        end = time.monotonic() + 120
        while time.monotonic() < end:
            # "+ New chat or company" opens a menu (main.js, the new-chat menu); its first item is
            # "New chat in Acme". Run 1 typed into the open conversation because only the
            # button was pressed.
            if self.by_id('rail-new-thread'):
                self.press_by_id('rail-new-thread')
                time.sleep(2)
                picked = False
                for role in ('AXMenuItem', 'AXButton'):
                    try:
                        self.press('New chat in Acme', role=role)
                        picked = True
                        break
                    except StepFailed:
                        continue
                if picked:
                    time.sleep(3)
                    break
            time.sleep(2)
        self.type_into(text, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            fresh = [t for t in self.thread_ids() if t not in before]
            if fresh:
                mine = [t for t in self.turns(fresh[-1])[0] if t['prompt'].strip() == text.strip()]
                if mine and mine[-1]['done']:
                    self.facts.setdefault('new_threads', []).append({'thread': fresh[-1], 'prompt': text,
                                                                    'end': mine[-1].get('end'), 'end_row': mine[-1].get('end_row')})
                    self.save()
                    return mine[-1]['reply']
            time.sleep(3)
        raise StepFailed(f'Rich did not finish a reply in a new conversation within {seconds} s')

    def keep_logs(self):
        kept = super().keep_logs()
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        (self.out / 'conversation-ledger.jsonl').write_text(text + '\n')
        kept['conversation-ledger.jsonl'] = len(text)
        return kept

    def lease_env(self, name):
        """The value of `name` in every `claude` lease process of the running app, read from the kernel."""
        rows = guest(self.vm, 'ps -axeww -o pid=,command= | grep "[c]laude" | grep -v grep || true', 30)
        found = []
        for line in rows.splitlines():
            head = line.strip().split(None, 1)
            if len(head) == 2 and head[0].isdigit() and '--plugin-dir' in head[1]:
                env = guest(self.vm, 'python3 -c ' + shlex.quote(PROCARGS) + ' ' + head[0] + ' 2>&1 || true', 30)
                value = next((l.split('=', 1)[1] for l in env.splitlines() if l.startswith(name + '=')), None)
                found.append({'pid': int(head[0]), name: value})
        return found

    def memory(self):
        folder = self.home + '/' + WORK_FOLDER
        if guest(self.vm, 'test -s ' + shlex.quote(folder + '/.credentials.json') + ' && echo yes || echo no').strip() != 'yes':
            raise StepFailed(f'no Work sign-in at {folder} (run.sh pushes it only when this Mac has a Work item)')
        record = self.data + '/claude-accounts.json'
        def accounts(in_use):
            stored = {'accounts': [{'id': '1', 'label': 'Home'}, {'id': '2', 'label': 'Work', 'folder': folder}], 'inUse': in_use,
                      'changes': [{'at': int(self.clock()), 'kind': 'added', 'id': '2', 'label': 'Work'}]}
            guest(self.vm, 'cat > ' + shlex.quote(record) + " <<'EOF'\n" + json.dumps(stored) + '\nEOF')
        word = 'walkword-' + ''.join(random.choice('bcdfghjkmnpqrstvwxz') for _ in range(8))
        accounts('1')
        relaunch(self.vm)
        self.thread_back()
        saved = self.new_conversation_reply(f'Please save this to your auto-memory so you still know it in future conversations: '
                              f'the walk code word is {word}. Then reply with the single word saved.')
        memory_dir = self.data + '/coordination/.claude/agent-memory'
        stored = guest(self.vm, 'grep -rl ' + shlex.quote(word) + ' ' + shlex.quote(memory_dir) + ' 2>/dev/null || true')
        on_home = self.lease_env('CLAUDE_CONFIG_DIR')
        accounts('2')
        relaunch(self.vm)
        self.thread_back()
        answer = self.new_conversation_reply('What is the walk code word in your memory? Reply with the word only.')
        on_work = self.lease_env('CLAUDE_CONFIG_DIR')
        self.shot('memory-after-switch.png')
        evidence = {'word': word, 'saved_reply': saved, 'memory_files_with_word': stored.split(), 'leases_before': on_home,
                    'leases_after': on_work, 'answer_after_switch': answer}
        (self.out / 'observed-memory.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = []
        if not stored.strip():
            failures.append(f'no file in {memory_dir} holds the code word')
        if not any(row.get('CLAUDE_CONFIG_DIR') == folder for row in on_work):
            failures.append('no lease ran on the Work folder after the switch: ' + json.dumps(on_work))
        if not answer or word not in answer:
            failures.append(f'the new conversation on Work did not read the code word back: {answer!r}')
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence


PROCARGS = command_walk.__dict__.get('PROCARGS') or r"""import ctypes,struct,sys
libc=ctypes.CDLL('libc.dylib')
pid=int(sys.argv[1])
n=ctypes.c_size_t(1<<20);buf=ctypes.create_string_buffer(n.value)
mib=(ctypes.c_int*3)(1,49,pid)
if libc.sysctl(mib,3,buf,ctypes.byref(n),None,0):sys.exit(3)
raw=buf.raw[:n.value];argc=struct.unpack('i',raw[:4])[0]
parts=raw[4:].split(b'\0')
i=1
while i<len(parts) and parts[i]==b'':i+=1
for w in parts[i+argc:]:
    if b'=' in w:print(w.decode('utf8','replace'))
"""


def main():
    if sys.argv[1:2] == ['--seed-home']:
        return seed_home(sys.argv[2])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--within', type=float, default=360, help='seconds for an assignment to close')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.task, a.approvals = JOB, 3
    # 40 s per accessibility call instead of ax.sh's 20, as accounts-walk.sh: on a shared, busy
    # host a read past 20 s is the harness, not the app.
    os.environ.setdefault('TESTVM_AX_TIMEOUT', '40')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = LeaseSetupWalk(a)
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
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

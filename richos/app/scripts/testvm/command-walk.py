#!/usr/bin/env python3
"""command-walk.py — a background assignment runs a real tool command and reports its result.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      command-walk.py --out DIR --expect-sha SHA [--within SECONDS]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset, still
as one run under run-walk.py.

THE QUESTION (esc-20260927T093052Z-85f3303f). The CEO's §50: work runs in the background and its
results come back when they land. Before this walk's fix, a task the back end carried out itself
— a command he asked to have run — was reported as "It stopped before it finished. No work was
started, so nothing was landed." seconds after the command had started, and the back end's own
report of what it printed was thrown away. This walk asks for one real command on a fresh
install and checks that its result reaches him.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: memory setup declined, company "Acme" registered with a folder that
             is a Git repository, the business questions declined
  connect    that folder connected through Settings > Connected folders (a company folder
             alone is not consent: `entity.rs` `connected_repositories`)
  watch      adopt-watch.py started in the guest (assignment states on the guest's clock)
  task       typed into the Mac's composer: run the harmless test command `git log --oneline`
             yourself in my Acme folder, and tell me when it has finished and what it printed
  observe    within --within seconds the assignment reaches a closed state. If the provider asks
             him to approve the command, Approve is pressed once per request, from the Under the
             hood panel, well inside the call's 300 s deadline. PASS needs all of: the assignment
             is a task and is `settled`; its last notice carries the repository's own HEAD short
             sha AND subject, read in the guest with git; no notice on it says "No work was
             started"; the back end's own hook evidence has a Bash PostToolUse running git.
             Evidence is written to --out.

FIVE MORE STEPS, run with --steps (esc-20260927T093052Z-85f3303f's leftovers):
  background     "start `sleep N && git log --oneline` in the background and tell me when it has
                 finished": PASS needs the back end's own hook record to show the command run with
                 run_in_background; the assignment seen `running` with "A command it started is
                 still running." while he already had its first words; then `settled` as
                 answered, with a LATER notice carrying the head's short sha and subject, raised
                 no earlier than N seconds after the send (the finish cannot be reported before
                 the command could end). Evidence: background-observed.json.
  deadline       checks the running app was launched with RICHOS_PERMISSION_DEADLINE_MS=--deadline-ms
                 (permissions.rs: it can only shorten the shipping 300 s), so a late approval takes
                 seconds to reach. Launch it so: TESTVM_APP_ENV=RICHOS_PERMISSION_DEADLINE_MS=20000
                 in run-walk.py's environment (run.sh, lib.sh app_env_args).
  late-approval  a command that needs his approval (`date +%s > /tmp/richos-late-approval.txt`);
                 NOTHING is pressed until the assignment is `blocked` -- the call has returned at
                 its deadline and the turn has ended -- then Approve is pressed once. PASS needs the
                 command's own output (its start, in whole guest seconds) within --late-bound
                 seconds of the press, and the assignment closed `settled`. The 2026-09-27
                 diagnosis measured (133.479, 137.542] s on the build before 76e977dc. Evidence:
                 late-approval-observed.json.
  next-job       a background command (sleep --yield-seconds, then echo a marker unique to the run),
                 and while it runs a second job (--task). PASS needs the second job closed without the
                 marker in what he was told; the first job still open, waiting on its command, when the
                 second closed; then the first `settled` as answered with a later notice carrying the
                 marker; its obligation closed on that answer; nothing about reviews, merges or closing.
                 Evidence: yield-observed.json. (background also checks the obligation and the words.)
  relaunch       relaunch.py after a first run (bgdone2 item 3), observed: one relaunch, then every
                 5 s up to --relaunch-within seconds the recorded pid, every process of the app's
                 executable with its parent, the recorded pid's windows, and whether the composer is
                 back. A launch after the first run lands on the HOME SCREEN by design (ui/home.js),
                 so its door ("Talk to Rich") is pressed once and the thread opened, and the moment
                 is recorded. PASS when the composer is back. Evidence: relaunch-observed.json, relaunch-app.log, and on a
                 miss relaunch-tree.txt and relaunch.png.

TWO STEPS FOR THE PROTO-TEAMMATE SHELF, slice 1 (richos-hq docs/plans/2026-10-06-proto-teammate-shelf.md
§2), run as --steps identity,first-run,connect,team,task,agents. The front desk's leases open at
launch, before any step, and a lease reads its team once (plan §1), so the user's own teammate is
seeded into the FIXTURE HOME before run-walk.py boots: `command-walk.py --seed-team FIXTURE_HOME [DEFINITION ...]`
writes FIXTURE_HOME/Library/Application Support/com.richos.app/team/walkmate.md and exits. `task`
registers the proven task, which opens the back end's lease.
  team           the seeded <app data>/team/walkmate.md is in the guest (refused otherwise).
  agents         waits up to --within seconds for the back end's lease (the one given richos_work),
                 then asks the back end to name every agent type its Agent tool lists. PASS needs all
                 of: the question registered and closed (the back end answered it); its last notice
                 names dean, clark, reed, frank, pierce and walkmate; every running lease's
                 --plugin-dir registers the always-active five, and the back end's walkmate too; all
                 six on coordination/.claude/agents; no shelf name (engine/team/shelf) on either.
                 Evidence: agents-observed.json.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import os
import re
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'first-run', 'connect', 'watch', 'task', 'observe']
MORE_STEPS = ['background', 'next-job', 'deadline', 'late-approval', 'relaunch', 'team', 'agents']
# The proto-teammate shelf, slice 1 (richos-hq docs/plans/2026-10-06-proto-teammate-shelf.md §2):
# every lease registers these four plus every file in <app data>/team/, and never the shelf.
ALWAYS_ACTIVE = ('dean', 'clark', 'reed', 'frank', 'pierce')
# A fictional teammate of the user's own, seeded by --seed-team before the leases open.
WALK_TEAMMATE = 'walkmate'
WALK_TEAMMATE_BODY = ('---\nname: walkmate\ndescription: Fictional walk teammate. Never use it for work.\n'
                      'model: sonnet\ntools: Read\n---\n\nA fictional teammate written by command-walk.py.\n')
# Asked of the back end once `task` has opened its lease. Worded for the front desk's third case,
# "the other one has to find out" (doctrine/front-desk.md): on 2026-10-06 a bare question about
# "your Agent tool" was answered by the front desk from its own lease, and a job worded with
# "yourself" was run by the front desk itself; neither registered anything.
AGENTS_TASK = ('Please ask the other one to check which agent types its Agent tool lists, '
               'and tell me every name, one per line.')
# The pane's words while an assignment waits on its command (work_host.rs COMMAND_STILL_RUNNING_DETAIL).
STILL_RUNNING = 'A command it started is still running.'
BACKGROUND_TASK = ('Please start this harmless test command in the background for me with your shell tool, in '
                   'my Acme folder, and tell me when it has finished and what it printed: sleep {seconds} && '
                   'git log --oneline')
# A command whose output is unique to this run, so another job's answer can be checked for it.
YIELD_TASK = ('Please start this harmless test command in the background for me with your shell tool, in '
              'my Acme folder, and tell me when it has finished and what it printed: sleep {seconds} && '
              'echo {marker}')
# Words about the app's own bookkeeping that a report on a job that changed nothing must not carry
# (vm-run-2 of 2026-09-27: "nothing to review or merge", "formally closing an assignment ... still open").
BOOKKEEPING = ('merge', 'reviewer', 'formally clos', 'still open there', 'closing an assignment')
LATE_FILE = '/tmp/richos-late-approval.txt'
LATE_TASK = ('Please run this harmless test command for me yourself with your shell tool, in my Acme folder, '
             'and tell me when it has finished: date +%s > ' + LATE_FILE)
# Worded as a job to be done, the way the escalation's walk and the 2026-09-27 diagnosis worded
# theirs (both registered as a `task`). "Tell me exactly what it prints" alone was registered
# as a `check` on the first proof run (the question path, which already reported answers) and
# did not exercise the task path this walk exists for.
TASK = ('Please run this harmless test command for me yourself with your shell tool, in my Acme '
        'folder, and tell me when it has finished and what it printed: git log --oneline')
OPEN = {'registered', 'preparing', 'running', 'blocked', 'waiting-for-screen', 'waiting-for-quota'}


class CommandWalk(adopt_walk.Walk):
    def connect(self):
        connected = self.connect_folder()
        self.facts['connected'] = connected
        self.save()
        return {'connected': connected}

    def task(self):
        before = self.clock()
        self.type_into(self.a.task, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        self.facts['sent_ms'] = [before, self.clock()]
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in self.facts['sent_ms']]}

    def records(self):
        script = ('import json,glob,sys\n'
                  'rows=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/assignments/*/*.json"):\n'
                  '    try: rows.append(json.load(open(p)))\n'
                  '    except Exception: pass\n'
                  'print(json.dumps(rows))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def approve_if_asked(self, title):
        """One press of this assignment's Approve, when the panel offers one. Returns whether it pressed."""
        label = 'Approve ' + title
        if not self.present(label):
            try:
                self.press('Open the work summary')
            except StepFailed:
                return False
            if not self.present(label):
                return False
        self.press(label)
        return True

    def evidence_commands(self):
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("hook_event_name")=="PostToolUse" and c.get("tool_name")=="Bash" and not c.get("agent_id"):\n'
                  '            r=c.get("tool_response") or {}\n'
                  '            out.append({"command":c.get("tool_input",{}).get("command",""),\n'
                  '                        "background":bool(c.get("tool_input",{}).get("run_in_background")),\n'
                  '                        "stdout":(r.get("stdout","") if isinstance(r,dict) else str(r))[:2000]})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def observe(self):
        thread, sent = self.facts.get('thread'), self.facts.get('sent_ms')
        if not thread or not sent:
            raise StepFailed('first-run and task must have run (no thread or no send time on record)')
        head = guest(self.vm, 'git -C ' + shlex.quote(self.company) + ' log -1 --format=%h%x09%s').split('\t')
        short, subject = head[0].strip(), head[1].strip()
        end = time.monotonic() + self.a.within
        pressed, record = 0, None
        while time.monotonic() < end:
            ours = [r for r in self.records() if r.get('thread_id') == thread and r.get('registered_at_ms', 0) >= sent[0]]
            record = max(ours, key=lambda r: r.get('registered_at_ms', 0)) if ours else None
            if record and record.get('state') not in OPEN:
                break
            if record and pressed < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed += 1
            time.sleep(3)
        commands = self.evidence_commands()
        evidence = {'repository_head': {'short': short, 'subject': subject}, 'approvals_pressed': pressed,
                    'assignment': record, 'back_end_bash': commands}
        (self.out / 'observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not record:
            raise StepFailed('no assignment was registered after the task was sent')
        if record.get('state') in OPEN:
            raise StepFailed(f"the assignment was still {record.get('state')} after {self.a.within:g} s")
        failures = verdict(record, short, subject, commands)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'state': record['state'], 'kind': record['kind'], 'notice': record['notices'][-1]['text'],
                'head': f'{short} {subject}', 'approvals_pressed': pressed,
                'command': next(c['command'] for c in commands if 'git' in c['command'])}


    def ours(self, sent):
        thread = self.facts.get('thread')
        rows = [r for r in self.records() if r.get('thread_id') == thread and r.get('registered_at_ms', 0) >= sent[0]]
        return max(rows, key=lambda r: r.get('registered_at_ms', 0)) if rows else None

    def send(self, text):
        before = self.clock()
        self.type_into(text, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        return [before, self.clock()]

    def head(self):
        head = guest(self.vm, 'git -C ' + shlex.quote(self.company) + ' log -1 --format=%h%x09%s').split('\t')
        return head[0].strip(), head[1].strip()

    def background(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        short, subject = self.head()
        sent = self.send(BACKGROUND_TASK.format(seconds=self.a.command_seconds))
        end = time.monotonic() + self.a.within + self.a.command_seconds
        pressed, record, waiting = 0, None, None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                break
            if (waiting is None and record and record.get('state') == 'running'
                    and record.get('detail') == STILL_RUNNING and record.get('notices')):
                waiting = {'guest_ms': round(self.clock()), 'notices': record['notices']}
            if record and pressed < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed += 1
            time.sleep(2)
        if record and record.get('state') not in OPEN:
            record = self.settled_read(record)
        commands = self.evidence_commands()
        evidence = {'sent_between_guest_ms': [round(v) for v in sent], 'command_seconds': self.a.command_seconds,
                    'repository_head': {'short': short, 'subject': subject}, 'approvals_pressed': pressed,
                    'seen_waiting_on_its_command': waiting, 'assignment': record, 'back_end_bash': commands}
        (self.out / 'background-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not record:
            raise StepFailed('no assignment was registered after the task was sent')
        obligation = self.obligation_closed(record['obligation_id'])
        evidence['obligation'] = obligation
        (self.out / 'background-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = background_verdict(record, short, subject, commands, sent, self.a.command_seconds, waiting,
                                      obligation)
        if failures:
            raise StepFailed('; '.join(failures))
        notices = record['notices']
        return {'state': record['state'], 'first_words': notices[0]['text'], 'report': notices[-1]['text'],
                'report_after_send_s': round((notices[-1]['raised_at_ms'] - sent[0]) / 1000, 3),
                'waiting_seen_at_guest_ms': waiting['guest_ms'], 'approvals_pressed': pressed,
                'obligation': obligation}

    def keep_logs(self):
        """The guest's app log and the back end's own hook records, copied out before the guest is
        deleted: without them a failure in the guest can only be guessed at (bgdone2 run 1)."""
        kept = {}
        # The first launch logs to app.log; each relaunch.py launch to its own relaunch-<ns>.log.
        names = ['app.log'] + guest(self.vm, 'cd ' + shlex.quote(self.payload)
                                    + ' && ls relaunch-*.log 2>/dev/null || true').split()
        for name in names:
            text = guest(self.vm, 'tail -c 400000 ' + shlex.quote(self.payload + '/' + name) + ' 2>/dev/null || true', 60)
            (self.out / name).write_text(text + '\n')
            kept[name] = len(text)
        script = ('import glob,os,sys\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl")):\n'
                  '    print("=== "+os.path.basename(os.path.dirname(p)))\n'
                  '    print(open(p).read()[-400000:])\n')
        text = guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60)
        (self.out / 'callbacks.txt').write_text(text + '\n')
        kept['callbacks.txt'] = len(text)
        # Why a tool call was refused, word for word, and every Pierce verdict with its wall time:
        # the callbacks show what Rich sent, never what came back (hiresnag55: the cause of a
        # "technical snag" was gone with the guest).
        script = ('import glob,json,os,sys\n'
                  'for p in sorted(glob.glob(sys.argv[2]+"/.claude/projects/*/*.jsonl")):\n'
                  '    for line in open(p, errors="replace"):\n'
                  '        if \'"is_error":true\' not in line.replace(" ", ""): continue\n'
                  '        row = json.loads(line)\n'
                  '        for part in (row.get("message") or {}).get("content") or []:\n'
                  '            if isinstance(part, dict) and part.get("is_error"):\n'
                  '                print("=== " + str(row.get("timestamp")) + " " + os.path.basename(p))\n'
                  '                print(json.dumps(part.get("content"), ensure_ascii=False)[:6000])\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/pierce/*.json")):\n'
                  '    v = json.load(open(p))\n'
                  '    if "verdict" in v:\n'
                  '        print("=== pierce " + os.path.basename(p) + " " + str(v.get("verdict")) + " at "\n'
                  '              + str(v.get("at")) + " " + str(v.get("review_mode")))\n'
                  '        print(str(v.get("report"))[:3000])\n')
        text = guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data) + ' '
                     + shlex.quote(self.home) + ' 2>&1 || true', 60)
        (self.out / 'refusals.txt').write_text(text + '\n')
        kept['refusals.txt'] = len(text)
        return kept

    def settled_read(self, record):
        """The same row read again a few seconds after it was first seen closed: the state and his
        notice are two writes, so a single read can land between them."""
        if not record:
            return record
        time.sleep(4)
        return self.by_id(record.get('id')) or record

    def obligation_closed(self, obligation_id, seconds=60):
        """The obligation's record once it has closed, or its last reading after `seconds`: the
        app closes it after his report is raised, which is engine subprocess calls later."""
        end = time.monotonic() + seconds
        reading = self.obligation(obligation_id)
        while (not reading or reading.get('status') != 'completed') and time.monotonic() < end:
            time.sleep(3)
            reading = self.obligation(obligation_id)
        return reading

    def obligation(self, obligation_id):
        """The engine's record of an assignment's obligation, read-only: its status and evidence."""
        script = ('import json,sqlite3,sys\n'
                  'c=sqlite3.connect("file:"+sys.argv[1]+"/ecs/ecs.sqlite3?mode=ro",uri=True)\n'
                  'r=c.execute("SELECT status,evidence_ref FROM ecs_continuity_items WHERE item_id=?",(sys.argv[2],)).fetchone()\n'
                  'print(json.dumps({"status":r[0],"evidence_ref":r[1]} if r else None))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data)
                                + ' ' + shlex.quote(obligation_id), 60))

    def by_id(self, assignment_id):
        return next((r for r in self.records() if assignment_id and r.get('id') == assignment_id), None)

    def until_closed(self, pick, seconds, pressed):
        """The assignment `pick()` reads, polled until it closes or `seconds` pass; Approve pressed as asked."""
        end, record = time.monotonic() + seconds, None
        while time.monotonic() < end:
            record = pick()
            if record and record.get('state') not in OPEN:
                break
            if record and pressed[0] < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed[0] += 1
            time.sleep(2)
        return record

    def team(self):
        """The user's own teammate, seeded in the fixture home, is in the guest's <app data>/team/."""
        path = self.data + '/team/' + WALK_TEAMMATE + '.md'
        if guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true') != WALK_TEAMMATE_BODY.strip():
            raise StepFailed(f'{path} is not the seeded teammate: run command-walk.py --seed-team FIXTURE_HOME '
                             'before run-walk.py boots (the leases open at launch)')
        return {'seeded': path}

    def leases(self):
        """Every running provider child in the guest: its --plugin-dir and what that plugin registers."""
        script = ('import json,re,subprocess,os\n'
                  'out=[]\n'
                  'ps=subprocess.run(["ps","-axww","-o","pid=,args="],capture_output=True,text=True).stdout\n'
                  'for line in ps.splitlines():\n'
                  '    m=re.search(r"--plugin-dir (.+?/engine-profiles/[0-9a-f-]{36})",line)\n'
                  '    if not m: continue\n'
                  '    try: agents=json.load(open(m.group(1)+"/.claude-plugin/plugin.json")).get("agents")\n'
                  '    except Exception as e: agents=str(e)\n'
                  '    out.append({"pid":int(line.split()[0]),"plugin":m.group(1),"agents":agents,\n'
                  '                "work_tools":"richos_work" in line})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script), 60))

    def agents(self):
        """The back end names the always-active five and the user's own teammate from its Agent listing."""
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        # `task` registered work, so the back end's lease opens: wait for it (the one given
        # richos_work), then read the registration first, so a failed send still leaves it.
        end, leases = time.monotonic() + self.a.within, []
        while time.monotonic() < end:
            leases = self.leases()
            if any(l.get('work_tools') for l in leases):
                break
            time.sleep(3)
        roster = guest(self.vm, 'ls ' + shlex.quote(self.data + '/coordination/.claude/agents')).split()
        shelf = [Path(n).stem for n in guest(self.vm, 'ls ' + shlex.quote(self.payload + '/engine/team/shelf')
                                             + ' 2>/dev/null || true').split()]
        evidence = {'roster': roster, 'leases': leases, 'shelf': shelf}
        (self.out / 'agents-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        sent = self.send(self.a.agents_task)
        pressed = [0]
        record = self.settled_read(self.until_closed(lambda: self.ours(sent), self.a.within, pressed))
        evidence.update({'assignment': record, 'approvals_pressed': pressed[0]})
        (self.out / 'agents-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = agents_verdict(record, roster, leases, shelf)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'notice': record['notices'][-1]['text'], 'roster': roster,
                'leases': [{'pid': l['pid'], 'agents': l['agents'], 'work_tools': l['work_tools']} for l in leases]}

    def next_job(self):
        """His next job arrives while a background command runs (bgdone2 item 2): the first job must
        stay open and report its own finish later, and the second job's answer must not carry it."""
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        marker = 'yield-marker-' + os.urandom(4).hex()
        pressed = [0]
        first_sent = self.send(YIELD_TASK.format(seconds=self.a.yield_seconds, marker=marker))
        waiting = {}

        def seen_waiting(record):
            if not waiting and record.get('state') == 'running' and record.get('detail') == STILL_RUNNING:
                waiting.update(guest_ms=round(self.clock()), notices=record.get('notices'))
        end = time.monotonic() + self.a.within
        while not waiting and time.monotonic() < end:
            record = self.ours(first_sent)
            if record:
                seen_waiting(record)
                if record.get('state') not in OPEN:
                    break
                if pressed[0] < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                    pressed[0] += 1
            time.sleep(2)
        # Followed by its id from here: once the second job exists, "the newest since the first send"
        # would be the second job.
        first_id = (self.ours(first_sent) or {}).get('id')
        second_sent = self.send(self.a.task)
        second = self.settled_read(self.until_closed(lambda: self.ours(second_sent), self.a.within, pressed))
        first_meanwhile = self.by_id(first_id)
        first = self.settled_read(self.until_closed(lambda: self.by_id(first_id), self.a.within + self.a.yield_seconds, pressed))
        evidence = {'marker': marker, 'command_seconds': self.a.yield_seconds, 'first_sent_ms': first_sent,
                    'second_sent_ms': second_sent, 'first_seen_waiting': waiting or None,
                    'first_when_second_closed': first_meanwhile, 'first': first, 'second': second,
                    'first_obligation': self.obligation_closed(first['obligation_id']) if first else None,
                    'approvals_pressed': pressed[0], 'back_end_bash': self.evidence_commands()}
        (self.out / 'yield-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = yield_verdict(first, second, first_meanwhile, marker, first_sent, self.a.yield_seconds,
                                 evidence['first_obligation'])
        if failures:
            raise StepFailed('; '.join(failures))
        return {'first_report': first['notices'][-1]['text'], 'second_answer': second['notices'][-1]['text'],
                'first_state_when_second_closed': first_meanwhile.get('state'),
                'first_obligation': evidence['first_obligation'], 'approvals_pressed': pressed[0]}

    def deadline(self):
        # Read off the running app's own environment rather than trusted from the command
        # line: the walk is only a late-approval proof if the app under test has the knob.
        # (A relaunch with it was tried first, on 2026-09-27; the composer never came back
        # within 90 s, undiagnosed, so the knob is set at the first launch instead.)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = (state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('the recorded app pid is not a number')
        environment = guest(self.vm, 'ps -E -ww -p ' + pid + ' -o command=')
        want = f'RICHOS_PERMISSION_DEADLINE_MS={self.a.deadline_ms}'
        if want not in environment.split():
            raise StepFailed(f'the app (pid {pid}) was not launched with {want}: run it with TESTVM_APP_ENV={want}')
        self.facts['deadline_ms'] = self.a.deadline_ms
        self.save()
        return {'app_pid': int(pid), 'deadline_ms': self.a.deadline_ms}

    def app_rows(self):
        """Every process in the guest running this payload's app executable: pid, parent, path."""
        rows = []
        for line in guest(self.vm, 'ps -axo pid=,ppid=,comm=').splitlines():
            parts = line.split(None, 2)
            if len(parts) == 3 and self.payload in parts[2] and parts[2].endswith('/richos-tauri'):
                rows.append({'pid': int(parts[0]), 'ppid': int(parts[1]), 'exe': parts[2][len(self.payload):]})
        return rows

    def windows_of(self, pid):
        script = f'tell application "System Events" to count windows of (first process whose unix id is {pid})'
        text = guest(self.vm, 'osascript -e ' + shlex.quote(script) + ' 2>&1 || true')
        return int(text) if text.strip().isdigit() else text.strip()[:200]

    def after_relaunch(self):
        """Called once right after relaunch.py has started the app again, before the first read of
        its window. Nothing here; a walk whose guest is busy after a relaunch waits in its own."""
        return None

    def relaunch(self):
        """relaunch.py after a first run, observed rather than assumed (bgdone2 item 3: in vm-run-2
        of 2026-09-27 the composer did not come back within 90 s, and that guest was deleted).

        One relaunch, then a sample every 5 s for up to --relaunch-within seconds of: the pid the
        harness recorded, every process of this payload's executable with its parent, the recorded
        pid's window count, whether the composer is on screen, and the relaunch log's tail. PASS
        when the composer is back. Either way the samples are relaunch-observed.json, and a miss
        adds relaunch-tree.txt and relaunch.png."""
        from relaunch import relaunch
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        observed = {'before': {'recorded_pid': (state / 'app.pid').read_text().strip(), 'rows': self.app_rows()}}
        began = time.monotonic()
        launched = relaunch(self.vm)
        observed['relaunched'] = launched
        observed['settled'] = self.after_relaunch()
        observed['samples'] = []
        back, door = None, None
        end = time.monotonic() + self.a.relaunch_within
        while time.monotonic() < end:
            pid = (state / 'app.pid').read_text().strip()
            sample = {'t': round(time.monotonic() - began, 1), 'recorded_pid': pid, 'rows': self.app_rows(),
                      'windows': self.windows_of(pid) if pid.isdigit() else None,
                      'composer': self.present('Message to Rich', role='AXTextArea')}
            # THE HOME SCREEN IS WHERE A LAUNCH AFTER THE FIRST RUN LANDS, by his ruling of
            # 2026-09-01 (`ui/home.js`: "must be shown in the app after the splash screen", with a
            # way into the app UI). The first launch goes straight to first-run setup, so a
            # walk that waited only for the composer never saw this door; reap-walk.py's
            # `enter` walks through it the same way. Through the door once, then the thread.
            if not sample['composer'] and door is None and self.present('Talk to Rich'):
                door = sample['t']
                sample['home_screen'] = True
                self.press('Talk to Rich')
                time.sleep(2)
                self.press(self.facts.get('thread_title') or 'Running')
            observed['samples'].append(sample)
            observed['home_screen_door_pressed_at_s'] = door
            (self.out / 'relaunch-observed.json').write_text(json.dumps(observed, indent=2) + '\n')
            if sample['composer']:
                back = sample['t']
                break
            time.sleep(5)
        log = guest(self.vm, 'tail -c 20000 ' + shlex.quote(launched['log']) + ' 2>/dev/null || true', 60)
        (self.out / 'relaunch-app.log').write_text(log + '\n')
        if back is None:
            try:
                (self.out / 'relaunch-tree.txt').write_text(
                    command([HERE / 'ax.sh', self.vm, 'tree', '--depth', '6'], 60))
            except StepFailed as exc:
                (self.out / 'relaunch-tree.txt').write_text('tree could not be read: ' + str(exc) + '\n')
            self.shot('relaunch.png')
            raise StepFailed(f'the composer was not back within {self.a.relaunch_within:.0f} s of the relaunch; '
                             'see relaunch-observed.json, relaunch-app.log, relaunch-tree.txt, relaunch.png')
        return {'composer_back_after_s': back, 'home_screen_door_pressed_at_s': door,
                'recorded_pid': observed['samples'][-1]['recorded_pid'],
                'windows': observed['samples'][-1]['windows'], 'log': launched['log']}

    def file_epoch(self):
        text = guest(self.vm, 'cat ' + shlex.quote(LATE_FILE) + ' 2>/dev/null || true').strip()
        return int(text) if text.isdigit() else None

    def late_approval(self):
        if not self.facts.get('deadline_ms'):
            raise StepFailed('the deadline step must have run: a late approval needs the call to end first')
        guest(self.vm, 'rm -f ' + shlex.quote(LATE_FILE))
        sent = self.send(LATE_TASK)
        record, blocked = None, None
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            record = self.ours(sent)
            if self.file_epoch() is not None:
                raise StepFailed('the command ran before anything was approved: the late path was not exercised')
            if record and record.get('state') == 'blocked':
                blocked = {'guest_ms': round(self.clock()), 'detail': record.get('detail')}
                break
            if record and record.get('state') not in OPEN:
                break
            time.sleep(2)
        if not blocked:
            (self.out / 'late-approval-observed.json').write_text(json.dumps({'assignment': record}, indent=2) + '\n')
            raise StepFailed('the assignment never reached `blocked` (its call returned and its turn ended): '
                             + json.dumps(record and {k: record.get(k) for k in ('state', 'detail')}))
        press_before = self.clock()
        if not self.approve_if_asked(record.get('title', '')):
            raise StepFailed('the assignment was blocked and no Approve was offered for it')
        press_after = self.clock()
        pressed, epoch = 1, None
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            epoch = epoch or self.file_epoch()
            record = self.ours(sent)
            if epoch is not None and record and record.get('state') not in OPEN:
                break
            if (record and record.get('state') == 'blocked' and pressed < self.a.approvals
                    and self.approve_if_asked(record.get('title', ''))):
                pressed += 1
            time.sleep(1)
        evidence = late_evidence(sent, blocked, press_before, press_after, epoch, pressed, record)
        (self.out / 'late-approval-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = late_verdict(evidence, self.a.late_bound)
        if failures:
            raise StepFailed('; '.join(failures))
        return {k: evidence[k] for k in ('blocked_after_send_s', 'started_after_press_s', 'approvals_pressed', 'final_state')}


def late_evidence(sent, blocked, press_before, press_after, epoch, pressed, record):
    """What the late-approval step saw, with the press-to-start bounds worked out.

    The press is known only between two guest-clock reads either side of the click, and the
    command's start only to the whole second `date +%s` wrote, so the delay lies in
    (epoch*1000 - press_after, (epoch+1)*1000 - press_before]."""
    started = None
    if epoch is not None:
        started = [round((epoch * 1000 - press_after) / 1000, 3), round(((epoch + 1) * 1000 - press_before) / 1000, 3)]
    return {'sent_between_guest_ms': [round(v) for v in sent], 'blocked': blocked,
            'blocked_after_send_s': round((blocked['guest_ms'] - sent[0]) / 1000, 3),
            'press_between_guest_ms': [round(press_before), round(press_after)], 'command_epoch_s': epoch,
            'started_after_press_s': started, 'approvals_pressed': pressed,
            'final_state': record and record.get('state'), 'final_detail': record and record.get('detail'),
            'assignment': record}


def late_verdict(evidence, bound):
    """Why this late approval did NOT reach its command promptly ([] = it did)."""
    failures = []
    started = evidence.get('started_after_press_s')
    if started is None:
        failures.append('the approved command never ran')
    elif started[1] > bound:
        failures.append(f'the approved command started up to {started[1]} s after the press (bound {bound} s)')
    if evidence.get('final_state') != 'settled':
        failures.append(f"the assignment ended {evidence.get('final_state')}: {evidence.get('final_detail')}")
    return failures


def background_verdict(record, short, subject, commands, sent, seconds, waiting, obligation):
    """Why this closed assignment does NOT show a background command's finish reported ([] = it does).

    Separate from the guest so the rules can be tested on records (test/command-walk.test.py)."""
    notices = record.get('notices') or []
    failures = []
    # `git -C "<folder>" log` is the same command (measured on the first proof run, 2026-09-27).
    if not any(c.get('background') and re.search(r'\bgit\b.*\blog\b', c.get('command', '')) for c in commands):
        failures.append('the back end did not run the command in the background, so this was not exercised: '
                        + json.dumps(commands))
    if record.get('kind') != 'task':
        failures.append(f"registered as a {record.get('kind')}, not a task")
    if record.get('state') != 'settled' or record.get('detail') != 'Answered.':
        failures.append(f"ended {record.get('state')}: {record.get('detail')}")
    if not waiting:
        failures.append('never seen waiting on its command with its first words already said')
    if len(notices) < 2:
        failures.append(f'{len(notices)} notice(s): the finish was not reported after the start')
    else:
        last = notices[-1]
        said = last.get('text', '')
        if not short or not subject or short not in said or subject not in said:
            failures.append(f"the last notice does not carry the command's result ({short} {subject}): {said!r}")
        if last.get('raised_at_ms', 0) < sent[0] + seconds * 1000:
            failures.append(f"the report was raised {(last.get('raised_at_ms', 0) - sent[0]) / 1000:.3f} s after the "
                            f'send, before a {seconds} s command could have ended')
    if any('No work was started' in n.get('text', '') for n in notices):
        failures.append('he was told "No work was started"')
    failures += self_handled_failures(notices, obligation)
    return failures


def self_handled_failures(notices, obligation):
    """A job the back end handled itself (bgdone2 item 1): its obligation is closed on the answer he
    was given, and nothing he was told is about the app's own bookkeeping."""
    failures = []
    evidence = str((obligation or {}).get('evidence_ref') or '')
    if not obligation or obligation.get('status') != 'completed' or not evidence.startswith('answer:'):
        failures.append(f'its obligation was not closed on his answer: {obligation}')
    for notice in notices:
        said = notice.get('text', '').lower()
        found = [w for w in BOOKKEEPING if w in said]
        if found:
            failures.append(f'he was told about the bookkeeping ({", ".join(found)}): {notice.get("text")!r}')
    return failures


def yield_verdict(first, second, first_meanwhile, marker, sent, seconds, obligation):
    """Why his next job did NOT leave the first job's finish to be reported on its own ([] = it did)."""
    if not first or not second:
        return [f'an assignment is missing (first {bool(first)}, second {bool(second)})']
    failures = []
    if second.get('state') in OPEN:
        failures.append(f"the second job never closed: {second.get('state')}")
    if any(marker in n.get('text', '') for n in second.get('notices') or []):
        failures.append("the first job's finish was folded into the second job's answer")
    meanwhile = first_meanwhile or {}
    if meanwhile.get('state') != 'running' or meanwhile.get('detail') != STILL_RUNNING:
        failures.append('when the second job closed, the first was not open and waiting on its command: '
                        + json.dumps({k: meanwhile.get(k) for k in ('state', 'detail')}))
    if first.get('state') != 'settled' or first.get('detail') != 'Answered.':
        failures.append(f"the first job ended {first.get('state')}: {first.get('detail')}")
    notices = first.get('notices') or []
    if len(notices) < 2 or marker not in notices[-1].get('text', ''):
        failures.append(f"the first job's last notice does not carry its command's output ({marker})")
    elif notices[-1].get('raised_at_ms', 0) < sent[0] + seconds * 1000:
        failures.append('the first job reported before its command could have ended')
    failures += self_handled_failures(notices, obligation)
    return failures


def agents_verdict(record, roster, leases, shelf):
    """Why the back end's Agent listing does NOT show slice 1's registration ([] = it does).

    The always-active five are in every running lease's plugin; the back end's lease (the one given richos_work)
    is open and registers the user's own teammate too; all five on the roster; no shelf name in
    either; and the back end's own answer, a closed assignment, names all five."""
    failures = []
    wanted = list(ALWAYS_ACTIVE) + [WALK_TEAMMATE]
    names = lambda agents: [Path(a).stem for a in agents] if isinstance(agents, list) else []
    if not record:
        failures.append('no assignment was registered for the question, so the back end did not answer it')
    elif record.get('state') in OPEN:
        failures.append(f"the assignment was still {record.get('state')}")
    else:
        said = ((record.get('notices') or [{}])[-1].get('text') or '').lower()
        missing = [n for n in wanted if not re.search(r'\b' + n + r'\b', said)]
        if missing:
            failures.append(f'the back end did not name {missing} from its Agent listing: {said!r}')
    if not leases:
        failures.append('no running provider child with an engine plugin was found in the guest')
    for lease in leases:
        absent = [n for n in ALWAYS_ACTIVE if n not in names(lease.get('agents'))]
        if absent:
            failures.append(f"lease pid {lease.get('pid')} does not register {absent}: {lease.get('agents')}")
    back_end = [l for l in leases if l.get('work_tools')]
    if leases and not back_end:
        failures.append('no running lease carries richos_work, so the back end\'s lease was not open')
    for lease in back_end:
        if WALK_TEAMMATE not in names(lease.get('agents')):
            failures.append(f"the back end's lease pid {lease.get('pid')} does not register the user's own "
                            f"{WALK_TEAMMATE}: {lease.get('agents')}")
    on_roster = [Path(n).stem for n in roster]
    absent = [n for n in wanted if n not in on_roster]
    if absent:
        failures.append(f'the roster lacks {absent}: {roster}')
    leaked = sorted({n for n in shelf if n in on_roster or any(n in names(l.get('agents')) for l in leases)})
    if leaked:
        failures.append(f'shelf teammates are registered: {leaked}')
    return failures


def verdict(record, short, subject, commands):
    """Why this closed assignment does NOT show a command run and its result reported ([] = it does).

    Separate from the guest so the rules can be tested on records (test/command-walk.test.py)."""
    notices = record.get('notices') or []
    said = notices[-1].get('text', '') if notices else ''
    failures = []
    if record.get('kind') != 'task':
        failures.append(f"registered as a {record.get('kind')}, not a task, so the task path was not exercised")
    if record.get('state') != 'settled':
        failures.append(f"ended {record.get('state')}: {record.get('detail')}")
    if any('No work was started' in n.get('text', '') for n in notices):
        failures.append('he was told "No work was started"')
    if not short or not subject or short not in said or subject not in said:
        failures.append(f"the last notice does not carry the command's result ({short} {subject}): {said!r}")
    if not any('git' in c.get('command', '') and short and short in c.get('stdout', '') for c in commands):
        failures.append('no Bash PostToolUse by the back end ran git and printed the head: ' + json.dumps(commands))
    return failures


def seed_team(home, definitions=()):
    """Write the walk's own teammate into a fixture home, where the app's data folder will be, and
    copy each given definition (a shelf file, e.g. engine/team/shelf/mark.md) beside it as an
    activated teammate of the user's own: output-walk.py's named step needs one who writes code."""
    folder = Path(home) / 'Library/Application Support/com.richos.app/team'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / (WALK_TEAMMATE + '.md')).write_text(WALK_TEAMMATE_BODY)
    print(folder / (WALK_TEAMMATE + '.md'))
    for definition in definitions:
        target = folder / Path(definition).name
        target.write_bytes(Path(definition).read_bytes())
        print(target)
    return 0


def main():
    if sys.argv[1:2] == ['--seed-team']:
        return seed_team(sys.argv[2], sys.argv[3:])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=TASK)
    p.add_argument('--within', type=float, default=240, help='seconds for the assignment to close')
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on this assignment')
    p.add_argument('--command-seconds', type=int, default=45, help='how long the background command sleeps')
    p.add_argument('--deadline-ms', type=int, default=20000, help='the shortened permission deadline (1000-300000)')
    p.add_argument('--late-bound', type=float, default=30, help='seconds from the late Approve to the command')
    p.add_argument('--relaunch-within', type=float, default=120, help='seconds for the composer to come back')
    p.add_argument('--yield-seconds', type=int, default=90, help="how long next-job's background command sleeps")
    p.add_argument('--agents-task', default=AGENTS_TASK, help="what `agents` asks the back end")
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS + MORE_STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = CommandWalk(a)
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

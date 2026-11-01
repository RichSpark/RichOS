#!/usr/bin/env python3
"""Real packaged-app dedicated Codex channel walk, on guest fixture data only.

Run through run-walk.py with --bundle, --home, --engine and --report. The walk
uses real Rich scoped tools for designation/Rich-first setup, the packaged helper
for independent Codex callers and the actual background Stu actors for replies.
Two companies, same chat titles and three connections exercise isolation before
and after rename, archive and restart. No live operator channels are touched.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stu = load('codex_walk_stu', 'stu-reminder-walk.py')
multi = load('codex_walk_multi', 'multico-walk.py')
StepFailed = stu.StepFailed
STEPS = ['identity', 'first-run', 'pair', 'parallel', 'rename-restart', 'reconcile']
DEFAULT_STEPS = STEPS


def judge_context(rows):
    """The `context` step's verdict over its rows (one per input, in transcript order): None when
    it passes, else why not. Each entry's input must be on record. The first input of a session is
    given the full blocks by design (the model holds no earlier copy); every LATER input of that
    session, and so the second entry's always, must add under 1,000 tokens (chars/4) and carry
    neither the full continuity block nor the company material."""
    for name in ('first', 'second'):
        if not any(r['entry'] == name for r in rows):
            return 'no input carrying the %s entry has host context on record' % name
    seen = set()
    for r in rows:
        later = r['session'] in seen
        seen.add(r['session'])
        if r['entry'] == 'second' and not later:
            return 'the second entry was the first input of its session, so it proves nothing about a repeat'
        if later and (r['tokens'] >= 1000 or r['full_continuity'] or r['full_company']):
            return 'a later input (%s entry) added %d tokens (chars/4), full continuity %s, full company material %s' % (
                r['entry'] or 'no', r['tokens'], r['full_continuity'], r['full_company'])
    return None


class CodexWatchWalk(stu.StuReminderWalk):
    HOME_TEXT = 'to Rich'

    def __init__(self, a):
        super().__init__(a)
        self.beta = self.home + '/Beta'
        self.coordination = self.home + '/.richos-coordination'
        self.python = self.payload + '/engine/runtime/bin/python3'
        self.helper = self.payload + '/engine/scripts/lib/codex_channels.py'

    def run_python(self, program, *args):
        return json.loads(guest(self.vm, shlex.join([self.python, '-B', '-c', program, *args]), 120))

    def helper_call(self, action, **request):
        command = shlex.join(['env', 'HOME=' + self.home, 'CODEX_COORDINATION_ROOT=' + self.coordination,
                              self.python, '-B', self.helper, action, '--request'])
        return json.loads(guest(self.vm, 'printf %s ' + shlex.quote(json.dumps(request)) + ' | ' + command, 120))

    def ledger(self):
        return self.run_python('import json,sys;print(json.dumps([json.loads(l) for l in open(sys.argv[1]) if l.strip()]))',
                               self.data + '/conversation-ledger.jsonl')

    def threads(self):
        return [r for r in self.ledger() if r.get('event') == 'ThreadCreated']

    def registry(self):
        return self.run_python('import json,sys;print(json.dumps(json.load(open(sys.argv[1]))))', self.data + '/entities.json')

    def navigation(self):
        return self.run_python('import json,sys;print(json.dumps(json.load(open(sys.argv[1]))))', self.data + '/navigation.json')

    def title(self, thread):
        nav = self.navigation()
        return nav.get('renamed_threads', {}).get(thread) or nav.get('auto_titles', {}).get(thread) or next(r['title'] for r in self.threads() if r['thread_id'] == thread)

    def wait_fact(self, description, read, predicate, seconds=None):
        end = time.monotonic() + (seconds or self.a.within)
        while True:
            value = read()
            if predicate(value):
                return value
            if time.monotonic() >= end:
                raise StepFailed(description + ': fact did not arrive; last observation ' + str(value)[:1000])
            time.sleep(3)

    def connections(self):
        return self.run_python('import glob,json,sys;print(json.dumps([dict(json.load(open(p)),locator=p.rsplit("/",1)[0]) for p in glob.glob(sys.argv[1]+"/connections/*/*/*/connection.json")]))', self.coordination)

    def coordinator(self):
        return self.run_python('import json,os,sys;p=sys.argv[1];print(json.dumps(json.load(open(p)) if os.path.exists(p) else {}))', self.coordination + '/coordinator.json')

    def rename(self, thread, title):
        self.wait_for('Actions for ' + self.title(thread), seconds=120)
        self.press('Actions for ' + self.title(thread))
        self.press('Rename…', 'AXMenuItem')
        self.type_into(title, '--role', 'AXTextField', '--title', 'Chat name')
        self.keys('tell application "System Events" to key code 36')
        return self.wait_fact('rename saved', self.navigation, lambda v: v.get('renamed_threads', {}).get(thread) == title)

    def pair(self):
        acme = self.facts['thread']
        self.send('Perform this harmless transport test yourself here. Explicitly designate THIS chat as the future Codex coordinator using codex_channel designate. Then use codex_channel connect with text "Harmless parallel roundtrip test: acknowledge marked transport observations on their own bridge locator only" and provenance "Guest user transport test". Return the handoff. Do not delegate or start repository work.')
        first = self.wait_fact('Rich-first connection', self.connections, lambda rows: len(rows) == 1)[0]
        if first['thread_id'] != acme:
            raise StepFailed('Rich-first connection has the wrong owner')
        rich_context = self.helper_call('read', locator=first['locator'])['records']
        if not any(r.get('kind') == 'task_context' and r.get('provenance') == 'Guest user transport test' for r in rich_context):
            raise StepFailed('Rich-first setup omitted the original context/source')
        self.rename(acme, 'Parallel transport test')
        second = self.helper_call('connect', text='Harmless parallel roundtrip test: acknowledge marked observations on their own connection', provenance='Guest Codex-first source')['locator']
        multi.Walk.second_company(self)
        self.send('Perform this harmless transport test yourself here. Explicitly designate THIS chat as the future Codex coordinator using codex_channel designate. This authorizes harmless transport acknowledgments through the scoped channel tool. Do not delegate or start repository work.')
        target = self.wait_fact('Beta default', self.coordinator, lambda v: v.get('entity_id') == 'beta')
        beta = target['thread_id']
        self.rename(beta, 'Parallel transport test')
        third = self.helper_call('connect', text='Harmless Beta roundtrip: acknowledge marked observations on their own connection', provenance='Guest Codex-first source')['locator']
        rows = self.connections()
        by_path = {r['locator']: r for r in rows}
        paths = [first['locator'], second, third]
        if len(set(paths)) != 3 or [by_path[p]['thread_id'] for p in paths] != [acme, acme, beta]:
            raise StepFailed('three fresh pairs were not bound to their intended immutable owners')
        if self.title(acme) != self.title(beta):
            raise StepFailed('two companies did not have the same chat title')
        self.facts.update(connections=paths, owners=[acme, acme, beta], beta_thread=beta)
        self.save()
        return dict(connections=rows, original_context=rich_context, default=target)

    def inboxes(self):
        return self.run_python('import glob,json,sys;out=[]\nfor p in glob.glob(sys.argv[1]+"/engine-state/messages/*/*/stu.jsonl"):\n for l in open(p):\n  r=json.loads(l)\n  if r.get("bridge"):out.append(dict(r,file=p,thread=p.split("/")[-2]))\nprint(json.dumps(out))', self.data)

    def exchange(self, label):
        paths, owners = self.facts['connections'], self.facts['owners']
        requests = []
        for i, path in enumerate(paths):
            marker = 'parallel-%s-%d-%s' % (label, i, uuid.uuid4().hex[:10])
            requests.append(dict(locator=path, message_id=marker, text='Harmless transport observation %s. Reply once on bridge.locator with text ACK-%s, message_id ack-%s and reply_to %s using codex_channel send. No other action is requested.' % (marker, marker, marker, marker)))
        sent = time.time()
        # Independent real CLI processes, sharing only their own helper module.
        program = 'import concurrent.futures,json,os,subprocess,sys;rows=json.loads(sys.argv[3]);env=dict(os.environ,HOME=sys.argv[4],CODEX_COORDINATION_ROOT=sys.argv[4]+"/.richos-coordination")\ndef send(r):\n p=subprocess.run([sys.argv[1],"-B",sys.argv[2],"send","--request"],input=json.dumps(r),env=env,text=True,capture_output=True);assert p.returncode==0,p.stderr;return json.loads(p.stdout)\nwith concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool: print(json.dumps(list(pool.map(send,rows))))'
        receipts = self.run_python(program, self.python, self.helper, json.dumps(requests), self.home)
        arrivals = {}
        def rows():
            observed = self.inboxes()
            for request in requests:
                if any(r.get('bridge', {}).get('message_id') == request['message_id'] for r in observed):
                    arrivals.setdefault(request['message_id'], time.time() - sent)
            return observed
        observed = self.wait_fact('three owning inbox receipts', rows, lambda rs: all(any(r.get('bridge', {}).get('message_id') == q['message_id'] for r in rs) for q in requests))
        evidence = []
        for request, owner in zip(requests, owners):
            match = [r for r in observed if r.get('bridge', {}).get('message_id') == request['message_id']]
            if len(match) != 1 or match[0]['thread'] != owner or match[0].get('instruction') is not None or match[0].get('handoff'):
                raise StepFailed('wrong, duplicate or instruction-bearing delivery: ' + str(match))
            reply = self.wait_fact('reply on original pair', lambda: self.helper_call('read', locator=request['locator']),
                lambda v: any(r.get('reply_to') == request['message_id'] and 'ACK-' + request['message_id'] in r['text'] for r in v['records']), self.a.answer_within)
            heard = self.wait_fact('owning background Stu transcript', lambda: self.stu_heard(request['message_id'], owner), bool)
            evidence.append(dict(request=request, inbox=match[0], reply=reply, stu=heard, source_save_to_inbox_seconds=arrivals[request['message_id']]))
        return dict(receipts=receipts, exchanges=evidence)

    def parallel(self):
        result = self.exchange('before')
        self.facts['first_exchange'] = result
        self.save()
        return result

    def rename_restart(self):
        # Rename both through the normal UI. Once the first has changed, the
        # remaining old title uniquely names the second Actions button.
        self.rename(self.facts['owners'][0], 'Transport renamed Acme')
        self.rename(self.facts['beta_thread'], 'Transport renamed Beta')
        acme_title = self.title(self.facts['owners'][0])
        self.press('Actions for ' + acme_title)
        self.press('Archive', 'AXMenuItem')
        launch = relaunch(self.vm)
        self.after_relaunch()
        result = self.exchange('after-restart-archived')
        return dict(relaunch=launch, exchange=result, endpoints=self.connections())

    def reconcile(self):
        first = self.facts['first_exchange']['exchanges'][0]['request']
        history = self.helper_call('read', locator=first['locator'], direction='to-rich')
        result = self.helper_call('send', **first)
        if not result.get('already_saved'):
            raise StepFailed('original recorded ID did not reconcile the uncertain retry')
        accepted = self.data + '/engine-state/codex-watch/accepted'
        # Only derived positions are removed; retained source and acknowledged
        # inbox history must prevent another actionable row after relaunch.
        guest(self.vm, 'rm -rf ' + shlex.quote(accepted), 60)
        launch = relaunch(self.vm)
        self.after_relaunch()
        def replayed():
            return self.run_python('import json,os,sys;paths=json.loads(sys.argv[2]);out=[]\nfor p in paths:\n f=sys.argv[1]+"/"+p.rsplit("/",1)[-1]+".json"\n v=json.load(open(f)) if os.path.exists(f) else {}\n out.append(dict(locator=p,through=v.get("through"),size=os.path.getsize(p+"/to-rich.jsonl")))\nprint(json.dumps(out))', accepted, json.dumps(self.facts['connections']))
        positions = self.wait_fact('all retained sources reconciled after position loss', replayed,
                                   lambda rows: all(r.get('through') == r['size'] for r in rows))
        incoming = self.helper_call('read', locator=first['locator'], direction='to-rich')
        observed = self.inboxes()
        own = [r for r in observed if r.get('bridge', {}).get('message_id') == first['message_id']]
        if len(own) != 1 or history['records'] != incoming['records']:
            raise StepFailed('replay changed source or duplicated the accepted observation')
        return dict(relaunch=launch, replay_receipt=result, positions=positions, original_rows=own, source_retained=True)

    def stu_heard(self, mark, thread=None):
        """The native transcripts of THIS conversation's Stu sessions that carry `mark`: each
        session bound as Stu (`<state>/evidence/<session>/binding.json`), its transcript the path its
        callbacks name (richos-core `session_tools::native_path`)."""
        script = ('import glob,json,os,sys\n'
                  'out=[]\n'
                  'for b in glob.glob(sys.argv[1]+"/engine-state/evidence/*/binding.json"):\n'
                  '    try: s=json.load(open(b))\n'
                  '    except Exception: continue\n'
                  '    if s.get("role")!="stu" or s.get("thread_id")!=sys.argv[2]: continue\n'
                  '    paths=set()\n'
                  '    try:\n'
                  '        for l in open(os.path.join(os.path.dirname(b),"callbacks.jsonl")):\n'
                  '            try: p=(json.loads(l).get("callback") or {}).get("transcript_path")\n'
                  '            except Exception: continue\n'
                  '            if p: paths.add(p)\n'
                  '    except OSError: pass\n'
                  '    for p in sorted(paths):\n'
                  '        try:\n'
                  '            if sys.argv[3] in open(p,errors="replace").read(): out.append({"session":s.get("session_id"),"transcript":p})\n'
                  '        except OSError: pass\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(thread or self.facts['thread']), shlex.quote(mark)), 120) or '[]')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for an entry to reach Stu and his turn on it")
    p.add_argument('--answer-within', type=float, default=300, help='seconds for a scoped reply to reach its dedicated journal')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(DEFAULT_STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = CodexWatchWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            name = step.replace('-', '_')
            row['evidence'] = getattr(walk, name)()
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

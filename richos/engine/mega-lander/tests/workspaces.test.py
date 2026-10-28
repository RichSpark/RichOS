#!/usr/bin/env python3
"""workspaces.test.py — one test per numbered point of the CEO's workspace spec
(docs/plans/worktree-spec-2026-09-11.md), each named after its point.

Every test runs in temporary git repositories with HOME and CLAUDE_CONFIG_DIR
redirected into a temporary directory: the operator's real ledger, sessions,
teams and worktrees are never read or written. A session is a real process
(`sleep`) whose pid the registry records, so "its process no longer exists"
is a fact the operating system reports, not a fixture flag.
"""
import importlib.util
import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import threading
from unittest.mock import patch
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(HERE, "..", "workspaces.py")

os.environ["RICHOS_WORKSPACES_RETRY_BASE"] = "0"
os.environ["RICHOS_WORKSPACES_STOP_GRACE"] = "1"
os.environ["RICHOS_WORKSPACES_SPAWN_WINDOW"] = "0"
spec = importlib.util.spec_from_file_location("workspaces", LIB)
ws = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ws)


def run(*args, **kw):
    cwd = kw.pop("cwd", None)
    r = subprocess.run(list(args), cwd=cwd, capture_output=True, text=True, env=os.environ.copy())
    if kw.get("check", True) and r.returncode != 0:
        raise AssertionError("%s failed: %s %s" % (args, r.stdout, r.stderr))
    return r


class Env(object):
    """A sandbox: HOME, CLAUDE_CONFIG_DIR, repositories, session processes."""

    def __init__(self):
        self.root = os.path.realpath(tempfile.mkdtemp(prefix="ws-spec-"))
        self.home = os.path.join(self.root, "home")
        os.makedirs(self.home)
        self.saved = {k: os.environ.get(k) for k in ("HOME", "CLAUDE_CONFIG_DIR", "RICHOS_WORKSPACES_DIR",
                                                     "RICHOS_SESSION_PID", "RICHOS_SESSIONS_DIR",
                                                     "RICHOS_SESSION_ID", "GIT_CONFIG_GLOBAL",
                                                     "RICHOS_BULK_ROOT", "RICHOS_CARGO_CACHE_ROOT",
                                                     "RICHOS_MACHINE_SETTINGS_RESOLVED", "RICHOS_APP_STATE")}
        os.environ["HOME"] = self.home
        # The app's state is found from here too (other_registries); a test run
        # inside the app must never read the operator's real app registries.
        os.environ.pop("RICHOS_APP_STATE", None)
        # A land removes the workspace's build caches (build_caches.py). Here they live in the
        # sandbox, so no test ever reads or moves the operator's real cache folders.
        self.caches = os.path.join(self.root, "caches")
        os.environ["RICHOS_BULK_ROOT"] = self.root
        os.environ["RICHOS_CARGO_CACHE_ROOT"] = os.path.join(self.caches, "cargo-target")
        os.environ["RICHOS_MACHINE_SETTINGS_RESOLVED"] = "1"
        os.environ["CLAUDE_CONFIG_DIR"] = os.path.join(self.home, ".claude")
        os.environ.pop("RICHOS_WORKSPACES_DIR", None)
        os.environ.pop("RICHOS_SESSION_ID", None)
        gc = os.path.join(self.home, ".gitconfig")
        with open(gc, "w") as f:
            f.write("[user]\n\tname = t\n\temail = t@example.invalid\n[init]\n\tdefaultBranch = main\n")
        os.environ["GIT_CONFIG_GLOBAL"] = gc
        self.procs = []

    def repo(self, name):
        p = os.path.join(self.root, name)
        os.makedirs(p)
        run("git", "init", "-q", "-b", "main", p)
        with open(os.path.join(p, ".gitignore"), "w") as f:
            f.write(".env\nbuild/\n")
        with open(os.path.join(p, "README"), "w") as f:
            f.write("x\n")
        run("git", "-C", p, "add", "-A")
        run("git", "-C", p, "commit", "-q", "-m", "init")
        return p

    def session(self, sid, cwd):
        """A running session: a real process, recorded at its start (point 12)."""
        pr = subprocess.Popen(["sleep", "600"])
        self.procs.append(pr)
        os.environ["RICHOS_SESSION_PID"] = str(pr.pid)
        time.sleep(0.05)
        ws.record_session_start(sid, cwd)
        return pr

    def use(self, pr):
        os.environ["RICHOS_SESSION_PID"] = str(pr.pid)

    def close(self):
        for pr in self.procs:
            try:
                pr.kill()
                pr.wait()
            except OSError:
                pass
        for k, v in self.saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        subprocess.run(["chmod", "-R", "u+w", self.root], capture_output=True)
        shutil.rmtree(self.root, ignore_errors=True)


def worktrees(repo):
    return [e["path"] for e in ws.worktree_list(repo)]


def branches(repo):
    return run("git", "-C", repo, "for-each-ref", "--format=%(refname:short)", "refs/heads").stdout.split()


def SOON():
    """A checkable end an hour away (orchestration reliability R2: every new wait names one)."""
    return "at:" + ws.iso(ws.now() + 3600)


class Base(unittest.TestCase):
    def setUp(self):
        self.env = Env()
        self.entity = self.env.repo("entity")
        self.other = self.env.repo("other")
        self.sid = "sess-aaaaaaaa-1111"
        self.sess = self.env.session(self.sid, self.entity)
        # Point 14: "The branch a body of work integrates on is RECORDED when
        # that work starts, before its first agent is spawned. Nothing infers it
        # and nothing guesses it." Nothing in the library derives this any more,
        # so the fixture records it exactly as Rich does — one command, before
        # the first spawn. A test that wants the no-record case removes it.
        ws.record_integration(self.entity, "main", "the fixture's body of work", self.sid)
        ws.record_integration(self.other, "main", "the fixture's body of work", self.sid)

    def tearDown(self):
        self.env.close()

    # --- the spawn, exactly as the hooks see it ---------------------------
    def make_cc(self, name, repo=None, base="HEAD"):
        repo = repo or self.other
        path = os.path.join(self.env.root, os.path.basename(repo) + "-wt", name)
        ws.register_cc(self.sid, name, repo, path, "cc/" + name)
        run("git", "-C", repo, "worktree", "add", "-q", path, "-b", "cc/" + name, base)
        ws.confirm_cc(self.sid, name, path, True)
        return path

    def spawn(self, name, cc=None, extra="", native=True, agent_id=None, sid=None,
              stype="zach"):
        sid = sid or self.sid
        prompt = "do it\n" + ("cross-repo-worktree: %s\n" % cc if cc else "") + extra
        payload = {"session_id": sid, "tool_use_id": "tu-" + name, "tool_name": "Agent",
                   "tool_input": {"name": name, "subagent_type": stype, "prompt": prompt,
                                  "isolation": "worktree" if native else ""}}
        ws.register_spawn(payload, self.entity)
        aid = agent_id or ("a" + name.replace("-", "")[:12] + "0000")
        npath = os.path.join(self.entity, ".claude", "worktrees", "agent-" + aid)
        if native:
            run("git", "-C", self.entity, "worktree", "add", "-q", npath, "-b", "worktree-agent-" + aid)
            ws.record_start(sid, aid, npath, "zach")
        ws.bind_agent(sid, "tu-" + name, aid, self.entity)
        return aid, npath

    def commit(self, path, fname="work.txt", text="work\n"):
        with open(os.path.join(path, fname), "w") as f:
            f.write(text)
        run("git", "-C", path, "add", fname)
        run("git", "-C", path, "commit", "-q", "-m", "work " + fname)

    def merge(self, repo, branch):
        run("git", "-C", repo, "merge", "-q", "--no-edit", branch)

    def spawn_readonly(self, agent_id, subagent_type="Explore", name=""):
        """A read-only spawn, exactly as the hooks make it: the guard registers
        it (no workspace, no name contract) and PostToolUse binds its id."""
        tuid = "tu-ro-" + agent_id
        payload = {"session_id": self.sid, "tool_use_id": tuid, "tool_name": "Agent",
                   "tool_input": {"subagent_type": subagent_type, "name": name,
                                  "prompt": "Find where the login button is defined."}}
        ws.register_readonly(payload, self.entity)
        ws.bind_agent(self.sid, tuid, agent_id, self.entity)
        return agent_id

    def pre(self, aid, call=""):
        """The FIRST half of one of the worker's own tool calls, exactly as the
        catch-all PreToolUse hook makes it: a payload carrying the platform's
        agent id and this call's own `tool_use_id`. It takes the snapshot
        creation is measured against (point 3)."""
        payload = {"session_id": self.sid, "agent_id": aid}
        if call:
            payload["tool_use_id"] = call
        return ws.barrier(payload)

    def post(self, aid, tool="Bash", call=""):
        """The SECOND half, exactly as the catch-all PostToolUse hook makes it.
        Together the two are one tool call, and a ref that appeared in between
        was created by this agent. The call id is the SAME string at both halves,
        which is what lets two of one agent's calls be open at once."""
        payload = {"session_id": self.sid, "agent_id": aid, "tool_name": tool,
                   "hook_event_name": "PostToolUse"}
        if call:
            payload["tool_use_id"] = call
        return ws.observe(payload)

    def tool_call(self, aid, call=""):
        """A whole tool call: both halves, with nothing in between."""
        verdict = self.pre(aid, call)
        self.post(aid, call=call)
        return verdict

    def created(self, name, sid=None):
        return sorted(b for _r, b in (tuple(x) for x in
                                      (self.rec(name, sid) or {}).get("created_branches") or []))

    def finish(self, aid):
        ws.record_end(self.sid, aid, "SubagentStop")

    def rec(self, name, sid=None):
        return ws.load_agent(ws.named_key(sid or self.sid, name))

    def names(self, sid=None):
        return sorted(i["name"] for i in ws.pending(sid or self.sid, self.entity, scan=True))

    def term_ignoring_holder(self):
        """A real process in a workspace that ignores SIGTERM, so only the
        SIGKILL escalation can stop it. It says ready only after its handler
        is installed, so the TERM is never sent before it can be ignored."""
        d = os.path.realpath(tempfile.mkdtemp(prefix="ws-kill-", dir=self.env.root))
        ready = os.path.join(d, "ready")
        pr = subprocess.Popen([sys.executable, "-c",
                               "import signal, sys, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                               "open(sys.argv[1], 'w').close(); time.sleep(3600)", ready], cwd=d)
        self.env.procs.append(pr)
        guard = time.monotonic() + 30
        while not os.path.exists(ready):
            self.assertLess(time.monotonic(), guard, "the holder never installed its handler")
            time.sleep(0.02)
        return d, pr


# ===========================================================================
class RecoveryOffParity(unittest.TestCase):
    def test_existing_native_spawn_continuation_review_and_landing_match_main_when_off(self):
        # Golden outcomes were witnessed on frozen main 97b7cc5ff and the candidate.
        # Real registrations and landing readers run in fresh script interpreters.
        # The reviewer provider and review processes are fixtures, never user agents.
        scenario = r"""import importlib.util,json,os,subprocess,sys,time
from pathlib import Path
engine=Path(sys.argv[1]).resolve()
spec=importlib.util.spec_from_file_location('fixture',engine/'mega-lander/tests/workspaces.test.py')
T=importlib.util.module_from_spec(spec);spec.loader.exec_module(T)
w=T.ws;case=T.Base();case.setUp()
try:
 root=Path(case.env.root);home=Path(case.env.home)
 os.environ['RICHOS_ENTITY_ROOT']=case.entity
 os.environ.pop('RICHOS_APP_STATE',None);os.environ.pop('RICHOS_APP_SCOPE',None)
 os.environ['SECOND_REVIEW_STATE_DIR']=str(root/'reviews')
 os.environ['REVIEW_WATCH_STATE_DIR']=str(root/'watch')
 binary=root/'bin';binary.mkdir()
 provider=binary/'claude'
 provider.write_text('#!'+sys.executable+'\nimport json\nprint(json.dumps({"structured_output":{"verdict":"PASS","report":"Authorized parity assignment","notes":[]}}))\n')
 provider.chmod(0o755)
 os.environ['PATH']=str(binary)+os.pathsep+os.environ['PATH']
 definition=Path(case.entity)/'.claude/agents/fixture.md';definition.parent.mkdir(parents=True)
 definition.write_text('Implement the explicitly requested fixture outcome.\n')
 transcript=root/'native.jsonl';transcript.write_text(json.dumps(dict(type='user',message=dict(content='Implement the fixture outcome and its corrections.')))+'\n')
 if len(sys.argv)>2 and sys.argv[2]=='off':
  declaration=Path(case.entity)/'.claude/recovery.json'
  declaration.write_text(json.dumps(dict(schema=1,mode='off',repositories=[case.other])))
 result={}
 def spawn(name,base='HEAD',extra='',host=None):
  cc=case.make_cc(name,base=base)
  payload=dict(session_id=case.sid,tool_use_id='tu-'+name,tool_name='Agent',hook_event_name='PreToolUse',cwd=case.entity,transcript_path=str(transcript),tool_input=dict(name=name,subagent_type='fixture',prompt='Implement the fixture outcome.\ncross-repo-worktree: '+cc+'\n'+extra,isolation=''))
  if host:payload['richos_recovery_assignment']=host
  answer=subprocess.run([sys.executable,'-I','-B',str(engine/'mega-lander/workspaces.py'),'--entity',case.entity,'register-spawn'],input=json.dumps(payload),text=True,capture_output=True,env=os.environ.copy())
  assert answer.returncode==0,answer.stdout+answer.stderr
  aid='id-'+name
  w.bind_agent(case.sid,'tu-'+name,aid,case.entity)
  case.commit(cc,fname=name+".txt")
  case.finish(aid)
  return cc,case.rec(name)
 cc,old=spawn('parity-old')
 old['registered_at']='2026-10-10T00:00:00Z';w.save_agent(old)
 result['native_spawn']='registered'
 cc2,new=spawn('parity-new',base='cc/parity-old',extra='continues: parity-old\n')
 sys.path.insert(0,str(engine/'scripts/lib'))
 import review_watch as RW,second_review as SR,operator_fences as F
 legacy=dict(new,key=new['key']+'-legacy',name='parity-legacy',continues=['withdrawn-orphan'])
 legacy.pop('recovery_assignment',None);w.save_agent(legacy)
 cfg=Path(case.entity)/'orchestration.config';cfg.write_text('SECOND_REVIEW_REPOS="other"\n')
 world=RW.World(str(engine),str(cfg));items,problems=world.registry_items(time.time(),{})
 item=next(i for i in items if i.name=='parity-new')
 assert not problems,problems
 result['legacy_review_included']=any(i.name=='parity-legacy' and i.work=='teammate:'+legacy['key'] for i in items)
 legacy['disposition']='discarded';w.save_agent(legacy)
 result['continuation_identity']='old-root' if item.work=='teammate:'+old['key'] else 'new-root'
 cc3,app=spawn('parity-app',base='cc/parity-new',extra='continues: parity-new\n',host=dict(kind='implementation',outcome='app:E:OB1',acceptance='original'))
 app_items,app_problems=world.registry_items(time.time(),{})
 assert not app_problems,app_problems
 app_item=next(i for i in app_items if i.name=='parity-app')
 result['app_review_identity']='old-root' if app_item.work=='teammate:'+old['key'] else 'new-root'
 original=Path(SR.state_root())/'reviews/prior';original.mkdir(parents=True)
 (original/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[dict(title='original blocker',blocks=True,priority=1)]))))
 ledger=Path(SR.ledger_path());ledger.parent.mkdir(parents=True,exist_ok=True)
 row=dict(id='prior',repo=case.other,repo_id=SR.repo_identity(case.other),work='teammate:'+old['key'],tip='a'*40,trigger='handover',verdict='changes-requested',record=str(original))
 ledger.write_text(json.dumps(row)+'\n')
 result['predecessor_findings']=len(SR.earlier_findings(item.work,item.tip,item.repo))
 result['app_predecessor_findings']=len(SR.earlier_findings(app_item.work,app_item.tip,app_item.repo))
 item=app_item
 ledger.write_text('')
 class Writer(RW.Watcher):
  calls=0
  def spawn(self,candidate,trigger,log_path):
   self.calls+=1
   record=Path(SR.state_root())/'reviews/automatic';record.mkdir(parents=True,exist_ok=True)
   (record/'verdict.json').write_text(json.dumps(dict(answer=dict(findings=[]))))
   row=dict(id='automatic',at=SR.iso(),repo=candidate.repo,repo_id=SR.repo_identity(candidate.repo),tip=candidate.tip,work=candidate.work,trigger=trigger,verdict='passed',record=str(record),findings=0,blocking=0,p1=0,reviewer='fixture',reviewer_model='fixture')
   with ledger.open('a') as out:out.write(json.dumps(row)+'\n')
   Path(log_path).write_text('fixture completed\n')
   return 999999,'fixture-finished'
 watcher=Writer(str(engine),str(cfg),world)
 now=time.time();watcher.look(now,{});watcher.look(now+1,{})
 result['automatic_handover_reviews']=watcher.calls
 # Execute landing's existing review reader in a fresh script-like interpreter.
 code=r'''
import importlib.util,json,os,sys
s=importlib.util.spec_from_file_location('w',sys.argv[1]);w=importlib.util.module_from_spec(s);s.loader.exec_module(w)
f=w._fence_program();repo,tip,ledger=sys.argv[2:]
w._registry_entity_map=lambda _:{}
w._declared_review_listing=lambda *a:(True,[],None)
w._review_ledgers=lambda *a:([ledger],None)
try:w._review_check([(repo,'cc/parity-app',repo,tip)])
except w.SpecError:print('refused')
else:print('accepted')
'''
 def landing():
  answer=subprocess.run([sys.executable,'-I','-B','-c',code,str(engine/'mega-lander/workspaces.py'),item.repo,item.tip,str(ledger)],capture_output=True,text=True,env=os.environ.copy())
  assert answer.returncode==0,answer.stdout+answer.stderr
  return answer.stdout.strip()
 result['passing_landing_review']=landing()
 row=json.loads(ledger.read_text().splitlines()[-1]);row.update(verdict='changes-requested',record=str(original),id='blocked')
 ledger.write_text(json.dumps(row)+'\n')
 result['blocking_landing_review']=landing()
 print(json.dumps(result,sort_keys=True))
finally:case.tearDown()
"""
        expected=dict(native_spawn='registered',continuation_identity='old-root',predecessor_findings=1,
            automatic_handover_reviews=1,passing_landing_review='accepted',blocking_landing_review='refused',legacy_review_included=True,
            app_review_identity='old-root',app_predecessor_findings=1)
        for mode in ('absent','off'):
            with self.subTest(mode=mode):
                engine=os.path.dirname(os.path.dirname(os.path.realpath(LIB)))
                result=subprocess.run([sys.executable,'-I','-B','-c',scenario,engine,mode],capture_output=True,text=True,env=os.environ.copy())
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                self.assertEqual(json.loads(result.stdout.strip().splitlines()[-1]),expected)


class RecoveryChain(Base):
    def test_refused_landing_routes_in_a_fresh_cli_interpreter(self):
        code = r"""
import importlib.util,json,os,sys
spec=importlib.util.spec_from_file_location('workspaces',sys.argv[1])
w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)
f=w._fence_program();repo=sys.argv[2];state=sys.argv[3];tip='a'*40
record=os.path.join(state,'rv');os.makedirs(record)
with open(os.path.join(record,'verdict.json'),'w') as out:
 json.dump(dict(answer=dict(findings=[dict(blocks=True,priority=1,title='defect',files=['a'],evidence='original')])),out)
ledger=os.path.join(state,'reviews.jsonl')
with open(ledger,'w') as out:
 out.write(json.dumps(dict(id='rv',repo=repo,repo_id=f.review_repo_identity(repo),work='teammate:fixture',tip=tip,trigger='handover',verdict='changes-requested',record=record))+'\n')
w._registry_entity_map=lambda _:{}
w._declared_review_listing=lambda *a:(True,[],None)
w._review_ledgers=lambda *a:([ledger],None)
try:w._review_check([(repo,'cc/work',repo,tip)])
except w.SpecError:
 assert (__import__('pathlib').Path(state)/'recovery/routing-requests.jsonl').is_file(), 'routing request absent'
else:raise AssertionError('blocked review was accepted')
"""
        state=os.path.join(self.env.root,'cli-review-state');os.makedirs(state)
        env=dict(os.environ,SECOND_REVIEW_STATE_DIR=state)
        result=subprocess.run([sys.executable,'-I','-B','-c',code,os.path.realpath(LIB),self.other,state],env=env,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_off_mode_continuation_preserves_predecessor_review_findings(self):
        import types
        cc=self.make_cc('old-review-work')
        aid,_=self.spawn('old-review-work',cc)
        self.commit(cc);self.finish(aid)
        old=self.rec('old-review-work')
        old['registered_at']='2026-10-10T00:00:00Z';ws.save_agent(old)
        cc2=self.make_cc('continued-review-work',base='cc/old-review-work')
        self.spawn('continued-review-work',cc2,extra='continues: old-review-work\n')
        new=self.rec('continued-review-work')
        baseline='teammate:'+old['key']
        self.assertEqual(ws.recovery_work_key(new),baseline)
        control,host=ws._recovery_adapter()
        import second_review as SR
        record=os.path.join(self.env.root,'prior-review');os.makedirs(record)
        with open(os.path.join(record,'verdict.json'),'w') as out:
            json.dump(dict(answer=dict(findings=[dict(title='data loss',blocks=True,priority=1)])),out)
        row=dict(id='prior',repo=self.other,work=baseline,tip='a'*40,trigger='handover',verdict='changes-requested',record=record)
        with patch.object(SR,'read_ledger',return_value=[row]):
            self.assertEqual([fid for fid,_ in SR.earlier_findings(ws.recovery_work_key(new),'b'*40,self.other)],['prior#1'])
        # Review identity does not rewrite semantic correction authority or cleanup.
        self.assertEqual(new['recovery_assignment']['correction_of'],[])
        self.assertEqual([r['key'] for r in ws._chain(new)],[new['key'],old['key']])

    def recovery_fixture(self, name='recovery-worker'):
        cc = self.make_cc(name)
        aid, _path = self.spawn(name, cc)
        rec = next(r for r in ws.all_agents() if r.get('agent_id') == aid)
        control, host = ws._recovery_adapter()
        state = os.path.join(self.env.root, 'private-review-state')
        self.env.saved.setdefault('SECOND_REVIEW_STATE_DIR', os.environ.get('SECOND_REVIEW_STATE_DIR'))
        os.environ['SECOND_REVIEW_STATE_DIR'] = state
        os.makedirs(state, exist_ok=True)
        work = ws.recovery_work_key(rec)
        ledger = os.path.join(state, 'reviews.jsonl')
        with open(ledger, 'w') as out:
            for n in range(3):
                out.write(json.dumps(dict(id='failed-'+str(n), repo=self.other, work=work, tip=str(n)*40,
                    trigger='handover', verdict='changes-requested', record=state))+'\n')
        record = control.R.reconcile(self.other, work, {work}, self.sid,
            rec['recovery_assignment']['acceptance'], mode='enforce')
        rec['recovery_jobs'] = [record['id']]
        ws.save_agent(rec)
        return cc, aid, rec, control, host, record

    def test_enforcement_tracks_tools_before_the_threshold_and_after_mode_change(self):
        cc,aid,rec,control,host,record=self.recovery_fixture('early-tracking')
        rec['recovery_jobs']=[]
        rec['recovery_assignment']['observation_only']=True
        ws.save_agent(rec)
        payload=dict(agent_id=aid,tool_use_id='early-write',tool_name='Write')
        with patch.object(control.R,'mode',return_value=dict(mode='enforce')):
            self.assertEqual(control.begin_tool(host,payload,rec),(True,''))
        self.assertIn('early-write',ws.load_agent(rec['key'])['recovery_tools'])
        control.finish_tool(host,payload)
        with patch.object(control.R,'mode',return_value=dict(mode='observe')),patch.object(ws,'Lock',side_effect=AssertionError('observation lock')):
            control,host=ws._recovery_adapter()
            self.assertEqual(control.begin_tool(host,payload,rec),(True,''))

    def test_idle_hooks_do_not_scan_all_workers_or_take_the_registry_lock(self):
        cc,aid,rec,control,host,record=self.recovery_fixture('idle-hooks')
        with patch.object(ws,'all_agents',side_effect=AssertionError('idle registry scan')), patch.object(ws,'Lock',side_effect=AssertionError('idle registry lock')):
            control,host=ws._recovery_adapter()
            self.assertEqual(control.cancellation_notice(host,self.sid),'')
            control.finish_tool(host,dict(agent_id=aid,tool_use_id='no-recovery-tool'))
        self.assertFalse(ws.load_agent(rec['key']).get('recovery_control'))

    def test_control_refuses_all_tools_without_manufacturing_an_end(self):
        cc, aid, rec, control, host, record = self.recovery_fixture()
        payload = dict(session_id=self.sid, agent_id=aid, tool_name='Write', tool_use_id='native-write')
        self.assertEqual(ws.barrier(payload)[0], 'REGISTERED')
        tip = control.source_identity(host, cc)
        decision = control.R.control(record['id'], dict(job=record['id'], acceptance=record['acceptance'],
            frontier=record['evidence_cursor'], action='stop', workers={rec['key']:tip},
            reason='the three attempts miss the same transition', evidence='independent findings'), 'axel')
        with patch.object(ws, 'end_owned_work', return_value={'ok':True,'survivors':[],'excluded':[]}):
            # host keeps function references, so get it after installing the spy.
            control, host = ws._recovery_adapter()
            result = control.stop(host, rec['key'], record, decision, tip)
        self.assertFalse(result['quiescent'])
        self.assertIn(aid,control.cancellation_notice(host,self.sid))
        self.assertFalse(ws.load_agent(rec['key']).get('end'))
        for tool in ('Read', 'Write', 'Edit', 'Bash', 'Agent', 'SubagentHandback'):
            self.assertEqual(ws.barrier(dict(payload, tool_name=tool))[0], 'CONTROLLED')
        ws.lifecycle(dict(payload, hook_event_name='PostToolUseFailure'), self.entity)
        self.assertEqual(ws.load_agent(rec['key'])['recovery_tools'], {})
        self.assertEqual(ws.barrier(dict(session_id=self.sid, tool_name='Read'))[0], 'LEAD')

    def test_a_stale_control_never_freezes_the_successor_source(self):
        cc, aid, rec, control, host, record = self.recovery_fixture('stale-worker')
        result = control.stop(host, rec['key'], record, dict(id='old', reason='old evidence'), 'f'*40)
        self.assertEqual(result['status'], 'stale')
        self.assertFalse(ws.load_agent(rec['key']).get('recovery_control'))

    def test_uncommitted_progress_invalidates_old_applicability(self):
        cc, aid, rec, control, host, record = self.recovery_fixture('changing-worker')
        source = control.source_identity(host, cc)
        with open(os.path.join(cc, 'new-source.txt'), 'w') as out:
            out.write('A fix that the earlier judgment has not seen')
        result = control.stop(host, rec['key'], record, dict(id='old', reason='old evidence'), source)
        self.assertEqual(result['status'], 'stale')
        self.assertFalse(ws.load_agent(rec['key']).get('recovery_control'))

    def test_correction_history_does_not_expand_lifecycle_ownership(self):
        old = ws.new_record('old-session--original', registered_at='2026-10-10T00:00:00Z',
            recovery_assignment=dict(kind='implementation', outcome='original', correction_of=[], acceptance='same'))
        ws.save_agent(old)
        successor = ws.new_record('new-session--replacement', registered_at='2026-10-11T00:00:00Z',
            lands_pending=[old['key']], recovery_assignment=dict(kind='implementation', outcome='original',
                correction_of=[old['key']], acceptance='same'))
        ws.save_agent(successor)
        self.assertEqual([r['key'] for r in ws.recovery_chain(successor)], [old['key'], successor['key']])
        self.assertEqual([r['key'] for r in ws._chain(successor)], [successor['key']])
        reviewer = dict(successor, key='reviewer', recovery_assignment=dict(kind='review', outcome='original', correction_of=[]))
        self.assertEqual(ws.recovery_work_key(reviewer), 'teammate:reviewer')
        feature = dict(successor, key='independent-feature', recovery_assignment=dict(kind='implementation', outcome='new-outcome', correction_of=[]))
        self.assertEqual(ws.recovery_work_key(feature), 'teammate:independent-feature')

    def test_off_mode_preserves_multi_continuation_and_ignores_unadopted_correction_metadata(self):
        first=ws.new_record(self.sid+'--first',recovery_assignment=dict(kind='implementation',outcome='a',correction_of=[],acceptance='a'))
        second=ws.new_record(self.sid+'--second',recovery_assignment=dict(kind='implementation',outcome='b',correction_of=[],acceptance='b'))
        ws.save_agent(first);ws.save_agent(second)
        current=ws.new_record(self.sid+'--off',entity=self.entity)
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='correction-of: missing\nContinue both finished agents'))
        assignment=ws._recovery_assignment(payload,current,[first['key'],second['key']],[])
        self.assertEqual(assignment['outcome'],'teammate:'+current['key'])
        self.assertEqual(assignment['correction_of'],[])

    def test_observed_identity_is_a_projection_and_never_rewrites_live_review_work(self):
        one=ws.new_record(self.sid+'--observed-one',registered_at='2026-10-10T00:00:00Z')
        two=ws.new_record(self.sid+'--observed-two',registered_at='2026-10-10T01:00:00Z')
        ws.save_agent(one);ws.save_agent(two)
        current=ws.new_record(self.sid+'--observed-current',entity=self.entity)
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='Continue the accepted outcome'))
        inspection=dict(id='observed',classification_index=dict(sha256='source'),assignment_classification=dict(kind='implementation',relation='corrective',correction_of=[one['key'],two['key']],evidence='indexed original briefs'))
        control,host=ws._recovery_adapter()
        with patch.object(control.R,'mode',return_value=dict(mode='observe',repositories=[self.other])):
            baseline=ws._recovery_assignment(payload,current,[],[])
            before=ws.recovery_work_key(dict(current,recovery_assignment=baseline))
            assignment=ws._recovery_assignment(payload,current,[],[],inspection)
            self.assertEqual(ws.recovery_work_key(dict(current,recovery_assignment=assignment)),before)
            self.assertEqual(ws.load_agent(one['key']),one)
            self.assertEqual(ws.load_agent(two['key']),two)
            projection=dict(current,recovery_assignment=current['recovery_observation_assignment'])
            work,aliases,acceptance=control.work(host,projection)
            self.assertEqual(work,'teammate:'+one['key'])
            self.assertEqual(aliases,{'teammate:'+r['key'] for r in [one,two,current]})
            self.assertEqual(acceptance,'legacy:'+one['key'])

    def test_native_classification_reconciles_legacy_corrections_but_not_dependencies(self):
        original=ws.new_record(self.sid+'--legacy-one',registered_at='2026-10-10T00:00:00Z')
        second=ws.new_record(self.sid+'--legacy-two',registered_at='2026-10-10T01:00:00Z')
        ws.save_agent(original)
        ws.save_agent(second)
        current=ws.new_record(self.sid+'--current',entity=self.entity)
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='Implement the same accepted outcome'))
        inspector=dict(id='inspection',classification_index=dict(sha256='source'),assignment_classification=dict(
            kind='implementation',relation='corrective',correction_of=[original['key'],second['key']],evidence='Both original indexed briefs correct the same outcome'))
        assignment=ws._classified_recovery_assignment(payload,current,[],[second['key']],inspector)
        self.assertEqual(assignment['acceptance'],'legacy:'+original['key'])
        current['recovery_assignment']=assignment
        self.assertEqual(len(ws.recovery_chain(current)),3)
        independent=dict(inspector,assignment_classification=dict(kind='implementation',relation='dependent',correction_of=[],evidence='A different accepted feature depends on the merge'))
        feature=ws._classified_recovery_assignment(payload,dict(current,key=self.sid+'--feature'),[],[second['key']],independent)
        self.assertNotEqual(feature['outcome'],assignment['outcome'])
        self.assertEqual(feature['correction_of'],[])
        self.assertEqual(len(ws._chain(current)),1)

    def test_app_consult_keeps_its_existing_research_tools_with_recovery_off(self):
        cc=self.make_cc('consult')
        aid,_path=self.spawn('consult',cc)
        rec=self.rec('consult')
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='Research the referenced video'))
        payload['richos_recovery_assignment']=dict(kind='helper',outcome='app:consult',acceptance='research')
        rec['recovery_assignment']=ws._recovery_assignment(payload,rec,[],[])
        ws.save_agent(rec)
        for tool in ('Read','Bash','WebSearch','WebFetch','Skill'):
            self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name=tool))[0],'REGISTERED',tool)
        self.assertEqual(rec['recovery_assignment']['kind'],'helper')

    def test_observation_does_not_restrict_a_native_helper_or_refuse_bad_identity(self):
        cc=self.make_cc('observed-helper')
        aid,_path=self.spawn('observed-helper',cc)
        rec=next(r for r in ws.all_agents() if r.get('agent_id')==aid)
        control,host=ws._recovery_adapter()
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='research the issue'))
        with patch.object(control.R,'mode',return_value=dict(mode='observe',repositories=[self.other])):
            rec['recovery_assignment']=ws._recovery_assignment(payload,rec,[],[],dict(id='observed',assignment_classification=dict(kind='helper',relation='new',correction_of=[],evidence='source')))
            ws.save_agent(rec)
            self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name='Bash'))[0],'REGISTERED')
            with patch.object(ws,'_classified_recovery_assignment',side_effect=ws.SpecError('identity gap')):
                assignment=ws._recovery_assignment(payload,rec,[],[])
                self.assertEqual(assignment['authority']['observation_gap'],'identity gap')
            with patch.object(control.R,'reconcile',side_effect=control.R.Unavailable('ledger unavailable')):
                self.assertEqual(control.admitted(host,payload,rec,dict(kind='implementation',acceptance='source'),[self.other],self.entity),[])
                self.assertEqual(rec['recovery_observation_gap'],'ledger unavailable')
        import recovery_runner as RR
        with patch.object(RR,'launch',side_effect=OSError('launch unavailable')):
            self.assertEqual(control.schedule(dict(mode='observe',axel_required=True,wes_candidates=[]),'engine')['observation_gap'],'launch unavailable')
            with self.assertRaises(OSError):control.schedule(dict(mode='enforce',controls=[],status='investigating',axel_required=True),'engine')
        with patch.object(control.R,'mode',return_value=dict(mode='enforce',repositories=[self.other])), patch.object(ws,'_classified_recovery_assignment',side_effect=ws.SpecError('identity gap')):
            with self.assertRaises(ws.SpecError):ws._recovery_assignment(payload,rec,[],[])

    def test_questions_only_dispatch_follows_observe_for_identity_admission_and_holds(self):
        # PRD 2026-10-20 section 8: a failed recovery classification cannot refuse
        # a questions-only dispatch, and no recovery hold reaches the worker.
        cc=self.make_cc('questions-only')
        aid,_path=self.spawn('questions-only',cc)
        rec=next(r for r in ws.all_agents() if r.get('agent_id')==aid)
        control,host=ws._recovery_adapter()
        payload=dict(session_id=self.sid,cwd=self.entity,tool_input=dict(prompt='implement the requested change'))
        with patch.object(control.R,'mode',return_value=dict(mode='questions-only',repositories=[self.other])):
            with patch.object(ws,'_classified_recovery_assignment',side_effect=ws.SpecError('identity gap')):
                assignment=ws._recovery_assignment(payload,rec,[],[])
            self.assertEqual(assignment['authority']['observation_gap'],'identity gap')
            self.assertTrue(assignment['observation_only'])
            self.assertEqual(assignment['authority']['mode'],'questions-only')
            rec['recovery_assignment']=assignment
            ws.save_agent(rec)
            self.assertEqual(ws.recovery_work_key(rec),'teammate:'+rec['key'])
            with patch.object(control.R,'reconcile',side_effect=control.R.Unavailable('ledger unavailable')):
                self.assertEqual(control.admitted(host,payload,rec,dict(kind='implementation',acceptance='source'),[self.other],self.entity),[])
            self.assertEqual(rec['recovery_observation_gap'],'ledger unavailable')
            self.assertEqual(control.begin_tool(host,dict(tool_use_id='t1',tool_name='Bash'),dict(rec,recovery_jobs=['job'])),(True,''))
            self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name='Bash'))[0],'REGISTERED')
        import recovery_runner as RR
        with patch.object(RR,'launch',side_effect=OSError('launch unavailable')):
            self.assertEqual(control.schedule(dict(mode='questions-only',axel_required=False,wes_candidates=[],repeat_signal=dict(label='w')),'engine')['observation_gap'],'launch unavailable')

    def test_read_only_classification_cannot_grant_a_writer_a_new_budget(self):
        cc = self.make_cc('helper')
        aid, _path = self.spawn('helper', cc, extra='recovery-role: helper')
        self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name='Bash'))[0], 'READONLY')
        self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name='Edit'))[0], 'READONLY')
        self.assertEqual(ws.barrier(dict(session_id=self.sid,agent_id=aid,tool_name='Read'))[0], 'REGISTERED')


class Point01_CcNaming(Base):
    """1. Every non-native Claude workspace is named cc/."""

    def test_point_01_every_non_native_workspace_is_named_cc(self):
        path = self.make_cc("zach-opus-p1")
        self.assertIn(path, worktrees(self.other))
        self.assertIn("cc/zach-opus-p1", branches(self.other))
        with self.assertRaises(ws.SpecError):
            ws.register_cc(self.sid, "zach-opus-p1b", self.other, os.path.join(self.env.root, "x"), "feature/x")

    def test_point_01_only_cc_non_native_workspaces_are_the_systems_concern(self):
        run("git", "-C", self.other, "worktree", "add", "-q", os.path.join(self.env.root, "mine"), "-b", "human/x")
        ws.pending(self.sid, self.entity, scan=True)
        self.assertEqual([a for a in ws.all_agents() if a.get("orphan")], [])
        self.assertIn(os.path.join(self.env.root, "mine"), worktrees(self.other))


class Point02_Codex(Base):
    """2. codex/ is never touched."""

    def test_point_02_codex_is_never_touched(self):
        cpath = os.path.join(self.env.root, "codex-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", cpath, "-b", "codex/fix")
        run("git", "-C", self.other, "branch", "codex/other")
        # never listed as unregistered work
        self.assertEqual(self.names(), [])
        # never spawned into
        payload = {"session_id": self.sid, "tool_use_id": "tu-c", "tool_name": "Agent",
                   "tool_input": {"name": "zach-opus-c", "prompt": "cross-repo-worktree: %s\n" % cpath,
                                  "isolation": "worktree"}}
        with self.assertRaises(ws.SpecError):
            ws.register_spawn(payload, self.entity)
        # never deleted, even named directly
        self.assertFalse(ws.delete_branch(self.other, "codex/other")[0])
        ok, why = ws.remove_workspace({"path": cpath, "repo": self.other})
        self.assertFalse(ok)
        self.assertIn("codex/", why)
        # and a land in the same repository leaves it alone
        cc = self.make_cc("zach-opus-c2")
        aid, _n = self.spawn("zach-opus-c2", cc=cc)
        self.finish(aid)
        ws.pending(self.sid, self.entity, scan=True)
        self.assertIn(cpath, worktrees(self.other))
        self.assertIn("codex/fix", branches(self.other))
        self.assertIn("codex/other", branches(self.other))


class Point03_TwoEvents(Base):
    """3. Two events, nothing else."""

    def test_point_03_failed_registration_means_no_spawn(self):
        # a cross-repo spawn naming a workspace nobody registered
        rogue = os.path.join(self.env.root, "rogue")
        run("git", "-C", self.other, "worktree", "add", "-q", rogue, "-b", "cc/rogue")
        payload = {"session_id": self.sid, "tool_use_id": "tu-r", "tool_name": "Agent",
                   "tool_input": {"name": "zach-opus-r", "prompt": "cross-repo-worktree: %s\n" % rogue,
                                  "isolation": "worktree"}}
        with self.assertRaises(ws.SpecError):
            ws.register_spawn(payload, self.entity)
        # and a registration that cannot read its session's process is refused
        os.environ["RICHOS_SESSION_PID"] = "999999"
        with self.assertRaises(ws.SpecError):
            ws.register_cc(self.sid, "zach-opus-r2", self.other, os.path.join(self.env.root, "r2"), "cc/r2")

    def test_point_03_unregistered_workspace_and_branch_are_finished_work(self):
        wt = os.path.join(self.env.root, "other-wt", "stray")
        run("git", "-C", self.other, "worktree", "add", "-q", wt, "-b", "cc/stray")
        self.commit(wt)
        run("git", "-C", self.entity, "branch", "worktree-agent-deadbeef00")
        run("git", "-C", self.entity, "checkout", "-q", "worktree-agent-deadbeef00")
        self.commit(self.entity, "native.txt")
        run("git", "-C", self.entity, "checkout", "-q", "main")
        ws._remember_repo(self.other)
        names = self.names()
        self.assertEqual(len(names), 2, names)
        self.assertTrue(all(n.startswith("orphan-") for n in names))

    # --- point 3, "any branch an agent created": WHOSE branch is it? ------
    # Attribution is read from the agent's OWN workspace and nowhere else. Two
    # agents in one repository is the normal case, and a repository-wide look
    # cannot tell "this agent made this ref" from "this ref appeared while this
    # agent happened to be running".

    def test_point_03_another_live_agents_branch_is_never_attributed(self):
        """B's branches are B's while A is running -- INCLUDING when every one of
        them appears DURING one of A's own tool calls, which is the hard case and
        the normal one: Rich spawns B while A works. Attributing them to A
        refused A's land (B's branch is not in main) and then DELETED them on a
        discard -- against point 8, "deletion therefore never loses anything that
        was meant to land", and point 5's guarantee for B.

        A has produced nothing of its own when its call closes, so it has no
        unlanded commit for anything to be on one line of history with, and
        nothing in the window is its own however it got there (filter 4)."""
        a_cc = self.make_cc("zach-opus-a1")
        aid_a, npath_a = self.spawn("zach-opus-a1", cc=a_cc)
        self.pre(aid_a)                                      # A's own call opens
        b_cc = self.make_cc("zach-opus-b1")                  # ...and inside it, all of B:
        aid_b, npath_b = self.spawn("zach-opus-b1", cc=b_cc)
        self.tool_call(aid_b)
        self.commit(npath_b, "b.txt")
        self.commit(b_cc, "bcc.txt")
        run("git", "-C", self.entity, "branch", "b-side", "worktree-agent-" + aid_b)
        self.post(aid_a)                                     # A's call closes: none of it is A's
        self.assertEqual(self.created("zach-opus-a1"), [])
        self.commit(npath_a, "a.txt")
        self.commit(a_cc, "acc.txt")
        self.finish(aid_a)                                   # nor at A's end-of-run signal
        self.assertEqual(self.created("zach-opus-a1"), [])
        self.merge(self.entity, "worktree-agent-" + aid_a)
        self.merge(self.other, "cc/zach-opus-a1")
        ws.land("zach-opus-a1", self.sid)                    # never held up by a branch of B's
        self.assertIn("cc/zach-opus-b1", branches(self.other))
        self.assertIn("worktree-agent-" + aid_b, branches(self.entity))
        self.assertIn("b-side", branches(self.entity))
        self.assertTrue(os.path.exists(b_cc))
        self.assertTrue(os.path.exists(npath_b))

    def test_point_03_a_continuing_agents_branch_is_never_its_predecessors(self):
        """THE ONE SHAPE WHERE ANOTHER AGENT'S REF IS ON THIS AGENT'S OWN LINE OF
        HISTORY, and the record is the only thing that tells them apart.

        Point 7: unfinished work is continued by a new agent, so the new agent's
        workspace is cut FROM the old agent's branch and its commits descend from
        the old agent's. Rich creates that workspace while the old agent is still
        in a tool call (create-teammate-worktree.sh runs before the spawn), so it
        is new, it is not landed, and it IS related to A's work -- every filter
        but one lets it through. The one that stops it is the record: a ref
        registered to another agent is never this agent's."""
        a_cc = self.make_cc("zach-opus-c1")
        aid_a, _npath_a = self.spawn("zach-opus-c1", cc=a_cc)
        self.tool_call(aid_a)
        self.commit(a_cc, "a.txt")                           # A's own unlanded work
        self.pre(aid_a)
        b_cc = self.make_cc("zach-opus-c2", base="cc/zach-opus-c1")   # cut from A's branch
        self.commit(b_cc, "b.txt")                           # and built on top of it
        self.post(aid_a)
        self.assertEqual(self.created("zach-opus-c1"), [])
        self.finish(aid_a)
        self.merge(self.other, "cc/zach-opus-c1")
        ws.land("zach-opus-c1", self.sid)                    # not held up by B's branch
        self.assertIn("cc/zach-opus-c2", branches(self.other))
        self.assertTrue(os.path.exists(b_cc))

    def test_point_03_a_branch_rich_cuts_in_the_main_checkout_is_never_the_agents(self):
        """Rich's own refs are not the system's concern (point 1) and are never
        deleted with an agent -- neither one he cuts in the main checkout nor one
        he checks out and commits on in a workspace of his own. Both are cut
        INSIDE one of the agent's own tool calls, while the agent has unlanded
        work of its own: co-occurrence in time is not authorship, so two
        different filters have to hold.

        `rich/notes` sits at the integration tip, so it carries nothing that is
        not landed and there is nothing at stake in it (filter 3). `rich/look`
        carries a commit of Rich's that is NOT landed -- and is on a line of
        history of his own rather than the agent's, so it is not the agent's
        either (filter 4)."""
        aid, npath = self.spawn("zach-opus-m1")
        self.tool_call(aid)
        self.commit(npath)                                   # the agent's own unlanded work
        self.pre(aid)                                        # its next call opens
        run("git", "-C", self.entity, "branch", "rich/notes")          # in the main checkout
        look = os.path.join(self.env.root, "rich-look")
        run("git", "-C", self.entity, "worktree", "add", "-q", look, "-b", "rich/look")
        self.commit(look, "rich.txt")                        # Rich's own commit, in no branch of the agent's
        self.post(aid)                                       # the call closes
        self.assertEqual(self.created("zach-opus-m1"), [])
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        ws.land("zach-opus-m1", self.sid)
        self.assertIn("rich/notes", branches(self.entity))
        self.assertIn("rich/look", branches(self.entity))
        self.assertTrue(os.path.exists(look))

    def test_point_03_two_agents_in_one_repository_both_land_their_own_refs(self):
        """The normal case, end to end: both land automatically (point 4) and
        neither takes the other's refs, so neither is left undecided (point 5)."""
        aid_a, pa = self.spawn("zach-opus-t1")
        self.tool_call(aid_a)
        aid_b, pb = self.spawn("zach-opus-t2")
        self.tool_call(aid_b)
        self.commit(pa, "a.txt")
        self.commit(pb, "b.txt")
        self.tool_call(aid_a)
        self.tool_call(aid_b)
        self.assertEqual(self.rec("zach-opus-t1")["created_branches"], [])
        self.assertEqual(self.rec("zach-opus-t2")["created_branches"], [])
        self.finish(aid_a)
        self.finish(aid_b)
        self.merge(self.entity, "worktree-agent-" + aid_a)
        self.merge(self.entity, "worktree-agent-" + aid_b)
        self.assertEqual(self.names(), [])
        for name, aid, path in (("zach-opus-t1", aid_a, pa), ("zach-opus-t2", aid_b, pb)):
            self.assertTrue(os.path.exists(ws.done_path(ws.named_key(self.sid, name))), name)
            self.assertFalse(os.path.exists(path), name)
            self.assertNotIn("worktree-agent-" + aid, branches(self.entity))

    def test_point_03_claude_worktree_sessions_are_not_allowed(self):
        wt = os.path.join(self.entity, ".claude", "worktrees", "my-session")
        run("git", "-C", self.entity, "worktree", "add", "-q", wt, "-b", "worktree-my-session")
        sid2 = "sess-worktree-2222"
        self.env.session(sid2, wt)
        kind, why = ws.barrier({"session_id": sid2, "tool_name": "Bash"})
        self.assertEqual(kind, "FORBIDDEN", why)
        self.assertEqual(ws.barrier({"session_id": self.sid, "tool_name": "Bash"})[0], "LEAD")


class Point03_TheLeadIsNeverLockedOutByASubagent(Base):
    """Point 3 forbids ONE thing: a session LAUNCHED inside a workspace. It never
    forbids the lead because one of its agents compacted.

    2026-09-22, 23:10:17Z. Subagent isaac-opus-n1 compacted its context. Claude
    Code 2.1.280 fires SessionStart(source=compact) for that, and builds the
    input WITHOUT the agent context: the lead's session_id, the subagent's
    worktree as cwd, and no agent_id (read from the binary: the SessionStart
    input is `{...jl(session, cwd), hook_event_name, source, agent_type, model}`
    with jl called without the agent argument). The engine took it for the lead
    starting in that worktree, stored FORBIDDEN on the lead's record, and every
    lead call after it was refused for the 33 minutes until the CEO exited, while
    ten agents kept the Mac at 100%.

    The lead's directory is the platform's own record of the process
    (~/.claude/sessions/<pid>.json, written once at launch), or the first
    record, and never a later SessionStart. Both payload shapes are covered: no
    agent_id (what 2.1.280 sends) and agent_id (what a later version may)."""

    def platform(self, sid, cwd, pid=None):
        """The platform's own session record, as Claude Code writes it."""
        d = ws._platform_sessions_dir()
        os.makedirs(d, exist_ok=True)
        pid = int(pid or os.environ["RICHOS_SESSION_PID"])
        with open(os.path.join(d, "%d.json" % pid), "w") as f:
            json.dump({"pid": pid, "sessionId": sid, "cwd": cwd, "kind": "interactive"}, f)
        return pid

    def subagent_workspace(self, aid="a275ae996204b25b3"):
        wt = os.path.join(self.entity, ".claude", "worktrees", "agent-" + aid)
        run("git", "-C", self.entity, "worktree", "add", "-q", wt, "-b", "worktree-agent-" + aid)
        return wt

    def session_workspace(self, name="my-session"):
        """What `claude --worktree <name>` makes: .claude/worktrees/<name>, not an
        agent-<id> workspace (which point 3 would count as unregistered work)."""
        wt = os.path.join(self.entity, ".claude", "worktrees", name)
        run("git", "-C", self.entity, "worktree", "add", "-q", wt, "-b", "worktree-" + name)
        return wt

    def compact(self, cwd, agent_id="", sid=None):
        p = {"hook_event_name": "SessionStart", "source": "compact",
             "session_id": sid or self.sid, "cwd": cwd}
        if agent_id:
            p["agent_id"] = agent_id
            p["agent_type"] = "isaac"
        return ws.lifecycle(p, self.entity)

    def lead(self, tool="Bash", sid=None, **ti):
        p = {"session_id": sid or self.sid, "tool_name": tool, "hook_event_name": "PreToolUse"}
        if ti:
            p["tool_input"] = ti
        return ws.barrier(p)

    def assert_lead_record_untouched(self):
        rec = ws.load_session(self.sid)
        self.assertEqual(rec["cwd"], os.path.realpath(self.entity))
        self.assertFalse(rec.get("forbidden"), rec.get("forbidden"))

    def test_point_03_a_subagent_compaction_never_locks_out_the_lead(self):
        self.platform(self.sid, self.entity)
        wt = self.subagent_workspace()
        self.compact(wt)                                   # 2.1.280's shape: no agent_id
        self.assertEqual(self.lead()[0], "LEAD")
        self.assert_lead_record_untouched()

    def test_point_03_a_subagent_compaction_carrying_its_agent_id_never_locks_out_the_lead(self):
        self.platform(self.sid, self.entity)
        wt = self.subagent_workspace()
        self.compact(wt, agent_id="a275ae996204b25b3")
        self.assertEqual(self.lead()[0], "LEAD")
        self.assert_lead_record_untouched()

    def test_point_03_without_the_platform_record_the_first_record_decides(self):
        wt = self.subagent_workspace()                     # no ~/.claude/sessions/<pid>.json
        self.compact(wt)
        self.assertEqual(self.lead()[0], "LEAD")
        self.assert_lead_record_untouched()

    def test_point_03_a_wrong_stored_flag_is_re_derived_at_the_verdict(self):
        """The record 9cfd9fc9 was left with: a subagent's directory and FORBIDDEN.
        The platform says the lead was launched in the main checkout, so the
        verdict is LEAD and the stored flag is healed, not trusted."""
        self.platform(self.sid, self.entity)
        wt = self.subagent_workspace()
        path = ws.session_path(self.sid)
        rec = json.load(open(path))
        rec["cwd"] = wt
        rec["forbidden"] = "the session's directory %s is a Claude Code workspace (claude --worktree)" % wt
        ws.write_json(path, rec)
        self.assertEqual(self.lead()[0], "LEAD")
        self.assertFalse(ws.load_session(self.sid).get("forbidden"))

    def test_point_03_a_session_launched_in_a_workspace_is_still_forbidden(self):
        wt = self.session_workspace()
        sid2 = "sess-worktree-5555"
        pr = self.env.session(sid2, wt)
        self.platform(sid2, wt, pr.pid)
        self.assertEqual(self.lead(sid=sid2)[0], "FORBIDDEN")
        # its own later compaction, from anywhere, does not clear it
        self.compact(self.entity, sid=sid2)
        self.assertEqual(self.lead(sid=sid2)[0], "FORBIDDEN")
        # and the platform's record alone is enough, whatever the payload said
        sid3 = "sess-worktree-6666"
        pr3 = self.env.session(sid3, self.entity)
        self.platform(sid3, wt, pr3.pid)
        ws.lifecycle({"hook_event_name": "SessionStart", "source": "startup", "session_id": sid3,
                      "cwd": self.entity}, self.entity)
        self.assertEqual(self.lead(sid=sid3)[0], "FORBIDDEN")

    def test_point_03_a_subagent_compaction_gets_none_of_the_leads_context(self):
        """The gate message is for the lead; a subagent cannot land anything."""
        self.platform(self.sid, self.entity)
        aid, npath = self.spawn("zach-opus-cx")
        self.commit(npath)
        self.finish(aid)
        wt = self.subagent_workspace()
        ctx, notices = self.compact(wt)
        self.assertEqual(ctx, "")
        self.assertEqual(notices, [])
        # CONTROL: the lead's own compaction still carries it
        ctx, _n = self.compact(self.entity)
        self.assertIn("zach-opus-cx", ctx)

    def forbidden_session(self):
        wt = self.session_workspace()
        sid2 = "sess-worktree-7777"
        pr = self.env.session(sid2, wt)
        self.platform(sid2, wt, pr.pid)
        self.assertEqual(self.lead(sid=sid2)[0], "FORBIDDEN")
        return sid2

    def test_point_03_a_forbidden_lead_can_always_stop_its_agents(self):
        sid2 = self.forbidden_session()
        engine = os.path.dirname(os.path.dirname(os.path.abspath(ws.__file__)))
        stop_sh = os.path.join(engine, "scripts", "stop.sh")
        ack_sh = os.path.join(engine, "scripts", "stop-work-ack.sh")
        self.assertEqual(self.lead("TaskStop", sid=sid2, task_id="a275ae996204b25b3")[0], "RECOVERY")
        for cmd in ("%s isaac-opus-n1 andy-opus-n1 --ceo-word 'Stop them now'" % stop_sh,
                    "bash %s isaac-opus-n1 --ceo-word \"Stop them\"" % stop_sh,
                    "%s --task isaac-opus-n1 --destroying 'nothing uncommitted' --why 'his word'" % ack_sh):
            self.assertEqual(self.lead("Bash", sid=sid2, command=cmd)[0], "RECOVERY", cmd)
        # Nothing else rides along with a stop.
        other = os.path.join(self.env.root, "scripts", "stop.sh")
        for cmd in ("%s x --ceo-word 'w'; rm -rf %s" % (stop_sh, self.entity),
                    "%s x --ceo-word \"$(touch %s/p)\"" % (stop_sh, self.env.root),
                    "%s x --ceo-word `id`" % stop_sh,
                    "%s x > %s/out" % (stop_sh, self.env.root),
                    "%s x --ceo-word 'w'" % other,
                    "git -C %s merge cc/x" % self.entity,
                    "ls"):
            self.assertEqual(self.lead("Bash", sid=sid2, command=cmd)[0], "FORBIDDEN", cmd)
        self.assertEqual(self.lead("SendMessage", sid=sid2, to="x", message="m")[0], "FORBIDDEN")

    def test_point_05_a_forbidden_session_is_told_once_and_never_held(self):
        """The spin: 42 Stop-hook refusals in 12 minutes, each demanding a
        `workspaces.sh wait` that the lock-out refused. A session that can do
        nothing but stop agents is TOLD, once, and its turn ends."""
        aid, npath = self.spawn("zach-opus-sp")
        self.commit(npath)
        self.finish(aid)
        self.assertIn("zach-opus-sp", ws.gate_stop({"session_id": self.sid}, self.entity)[2])   # CONTROL: told
        wt = self.session_workspace()
        self.platform(self.sid, wt)                         # this lead WAS launched in a workspace
        ws.lifecycle({"hook_event_name": "SessionStart", "source": "startup", "session_id": self.sid,
                      "cwd": wt}, self.entity)
        self.assertEqual(self.lead()[0], "FORBIDDEN")
        ok, msg, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertIn("TaskStop", msg)
        self.assertIn("stop.sh", msg)
        ok, msg2, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertNotIn("TaskStop", msg2)                 # said once, not every turn

    def test_point_03_all_stop_guards_release_a_forbidden_lead(self):
        sid = self.forbidden_session()
        engine = os.path.dirname(os.path.dirname(os.path.abspath(ws.__file__)))
        helper = os.path.join(engine, "scripts/lib/stop-session-recovery.py")
        payload = {"hook_event_name": "Stop", "session_id": sid, "cwd": self.entity,
                   "last_assistant_message": "I'm dispatching Zach now", "stop_hook_active": False}
        for data, expected in ((payload, 0), (dict(payload, agent_id="worker"), 1),
                               (dict(payload, session_id=self.sid), 1), ({}, 1)):
            result = subprocess.run([sys.executable, helper], input=json.dumps(data),
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, expected, result.stderr)
        for name in ("guard-ci-turn-gate", "guard-idle-land", "ass-kicker",
                     "guard-unresolved-claims", "guard-agent-state-claims", "guard-workspace-gate",
                     "guard-failure-type-answer", "guard-resource-waits"):
            result = subprocess.run(["bash", os.path.join(engine, "scripts/hooks", name + ".sh")],
                                    input=json.dumps(payload), capture_output=True, text=True,
                                    cwd=self.entity, timeout=15)
            self.assertEqual(result.returncode, 0, (name, result.stdout, result.stderr))
            if result.stdout.strip():
                self.assertNotEqual(json.loads(result.stdout).get("decision"), "block", name)


class Point54_TestDevicesAreGarbage(Base):
    """§54 for simulators: 2026-09-22 ended with three simulators booted by killed
    test runs still booted after the session. The collector is
    scripts/lib/testdevices.py (its own suite proves the rules); these prove it
    is WIRED: the land takes the devices keyed to the workspaces it deletes, and
    a session that starts takes the ones whose creator is gone."""

    FAKE = ("#!/usr/bin/env python3\nimport json, os, sys\n"
            "s = os.environ['FAKE_SIMCTL_STATE']; d = json.load(open(s)); a = sys.argv[1:]\n"
            "if a[:2] == ['list', 'devices']:\n"
            "    print(json.dumps({'devices': {'rt': d}})); sys.exit(0)\n"
            "if a[0] == 'delete':\n"
            "    d = [x for x in d if x['udid'] != a[1]]\n"
            "if a[0] == 'shutdown':\n"
            "    [x.update(state='Shutdown') for x in d if x['udid'] == a[1]]\n"
            "json.dump(d, open(s, 'w'))\n")

    def setUp(self):
        super().setUp()
        self.simctl = os.path.join(self.env.root, "simctl")
        with open(self.simctl, "w") as f:
            f.write(self.FAKE)
        os.chmod(self.simctl, 0o755)
        self.sims = os.path.join(self.env.root, "sims.json")
        with open(self.sims, "w") as f:
            json.dump([], f)
        os.environ.update({"RICHOS_SIMCTL": self.simctl, "FAKE_SIMCTL_STATE": self.sims,
                           "RICHOS_ANDROID_CACHES_ROOT": os.path.join(self.env.root, "no-android")})

    def tearDown(self):
        for k in ("RICHOS_SIMCTL", "FAKE_SIMCTL_STATE", "RICHOS_ANDROID_CACHES_ROOT"):
            os.environ.pop(k, None)
        super().tearDown()

    def sim(self, name):
        d = json.load(open(self.sims))
        udid = "SIM-%d" % (len(d) + 1)
        d.append({"udid": udid, "name": name, "state": "Booted"})
        json.dump(d, open(self.sims, "w"))
        return udid

    def names_left(self):
        return sorted(x["name"] for x in json.load(open(self.sims)))

    def test_point_54_the_land_takes_the_simulators_its_workspaces_made(self):
        import hashlib
        aid, npath = self.spawn("isaac-opus-sim")
        key = hashlib.sha256((os.path.realpath(npath) + "/richos/mobile/native-ios").encode()).hexdigest()[:10]
        self.sim("RichOS native-ios %s" % key)
        self.sim("iPhone 16 Pro")                                   # not ours: never touched
        self.commit(npath)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), [])                          # landed
        self.assertEqual(self.names_left(), ["iPhone 16 Pro"])

    def test_point_54_a_session_start_takes_a_killed_runs_simulator(self):
        pr = subprocess.Popen(["sleep", "600"])
        pr.kill()
        pr.wait()
        self.sim("rios-ui-%d-iPhone-SE" % pr.pid)
        ws.lifecycle({"hook_event_name": "SessionStart", "source": "compact", "session_id": self.sid,
                      "cwd": self.entity}, self.entity)
        self.assertEqual(len(self.names_left()), 1)                 # a compact is not a start
        ws.lifecycle({"hook_event_name": "SessionStart", "source": "startup", "session_id": self.sid,
                      "cwd": self.entity}, self.entity)
        self.assertEqual(self.names_left(), [])

    def test_point_54_agent_end_collects_without_waiting_for_land(self):
        ws.collect_test_devices(budget=1)  # imports the shared collector
        import testdevices
        aid, path = self.spawn("isaac-opus-end")
        udid = self.sim("RichOS native-ios platform-tests end")
        testdevices.register("ios-simulator", udid, checkout=path)
        ws.lifecycle({"hook_event_name": "SubagentStop", "session_id": self.sid, "agent_id": aid}, self.entity)
        self.assertEqual(self.names_left(), [])
        self.assertTrue(os.path.isdir(path))

    def test_point_54_session_end_collects_its_registered_device(self):
        ws.collect_test_devices(budget=1)
        import testdevices
        aid, path = self.spawn("isaac-opus-session")
        udid = self.sim("RichOS native-ios platform-tests session")
        testdevices.register("ios-simulator", udid, checkout=path)
        ws.lifecycle({"hook_event_name": "SessionEnd", "session_id": self.sid}, self.entity)
        self.assertEqual(self.names_left(), [])
        self.assertTrue(os.path.isdir(path))

    def test_point_54_unloadable_collector_still_creates_an_alert(self):
        with patch.dict(sys.modules, {"testdevices": None}):
            ws.collect_test_devices(budget=1)
        path = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "state/test-device-failures.json")
        self.assertIn("collector unavailable", json.load(open(path))["collector:incomplete"]["why"])


class Point04_LandedMeansDeleted(Base):
    """4. Landed means the workspace AND the branch are deleted — automatically."""

    def test_point_04_landed_means_workspace_and_branch_deleted_automatically(self):
        aid, npath = self.spawn("zach-opus-p4")
        self.commit(npath)
        self.finish(aid)
        self.assertEqual(self.names(), ["zach-opus-p4"])   # finished, not landed: pending
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), [])                 # landed: nothing undecided
        self.assertNotIn(npath, worktrees(self.entity))
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertEqual(self.rec("zach-opus-p4")["disposition"]["kind"], "landed")


class Point05_Guarantee(Base):
    """5. Rich lands 100% of everything after the agent is finished — guaranteed."""

    def _pending_one(self):
        aid, npath = self.spawn("zach-opus-p5")
        self.commit(npath)
        self.finish(aid)
        return aid, npath

    def test_point_05_unrelated_spawn_does_not_scan_pending_work(self):
        self._pending_one()
        with patch.object(ws, "pending", side_effect=AssertionError("spawn scanned pending work")):
            self.spawn("zach-opus-independent")
        self.assertIn("zach-opus-p5", self.names())
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
        self.assertIn("zach-opus-p5", ws.gate_stop({"session_id": self.sid}, self.entity)[2])   # told, not held

    def test_point_05_slow_failed_merge_does_not_block_unrelated_spawn(self):
        aid, npath = self._pending_one()
        ready = os.path.join(self.env.root, "merge-check-ready")
        release = os.path.join(self.env.root, "release-merge-check")
        hook = os.path.join(self.entity, ".git", "hooks", "pre-merge-commit")
        with open(hook, "w") as f:
            f.write("#!%s\nimport os, time\n" % sys.executable)
            f.write("open(%r, 'w').close()\n" % ready)
            f.write("deadline = time.monotonic() + 30\n")
            f.write("while not os.path.exists(%r) and time.monotonic() < deadline:\n    time.sleep(0.05)\n" % release)
            f.write("raise SystemExit(1)\n")
        os.chmod(hook, 0o755)
        head = run("git", "-C", self.entity, "rev-parse", "HEAD").stdout.strip()
        proc = subprocess.Popen(["git", "-C", self.entity, "merge", "--no-ff", "--no-edit",
                                 "worktree-agent-" + aid], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True)
        try:
            deadline = time.monotonic() + 15
            while not os.path.exists(ready) and proc.poll() is None and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(os.path.exists(ready), "merge never reached its pre-merge check")
            self.assertIsNone(proc.poll())
            self.spawn("zach-opus-during-merge")
            self.assertIsNone(proc.poll(), "merge check ended before the unrelated spawn")
            self.assertIn("zach-opus-p5", self.names())
            registry = ws.state_dir()
            ws.gate_stop({"session_id": self.sid}, self.entity)  # the fallback's own once-only state settles
            before = {os.path.relpath(os.path.join(root, name), registry): open(os.path.join(root, name), "rb").read()
                      for root, _dirs, files in os.walk(registry) for name in files}
            ok, message, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
            self.assertTrue(ok, message)
            self.assertNotIn("zach-opus-p5", lead)              # a live merge is not due
            after = {os.path.relpath(os.path.join(root, name), registry): open(os.path.join(root, name), "rb").read()
                     for root, _dirs, files in os.walk(registry) for name in files}
            self.assertEqual(before, after, "the active merge allowance wrote a registry wait")
            self.assertIsNone(self.rec("zach-opus-p5").get("waiting"))
            with patch.object(ws, "branch_tip_read", return_value=("", "unreadable ref")):
                self.assertFalse(ws._active_merge(self.rec("zach-opus-p5"), {}))
            with patch.object(ws, "integration_target", return_value=("dev/another-target", "", "")):
                self.assertFalse(ws._active_merge(self.rec("zach-opus-p5"), {}))
            self.assertFalse(ws._active_merge(self.rec("zach-opus-p5"),
                                             {"merge-processes": [(proc.pid, ["cc/unrelated-work"])]}))
        finally:
            with open(release, "w") as f:
                f.write("finish the check\n")
            try:
                proc.communicate(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.communicate()
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "HEAD").stdout.strip(), head)
        self.spawn("zach-opus-after-failure")
        self.assertIn("zach-opus-p5", self.names())
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
        self.assertTrue(os.path.isdir(npath))
        self.assertIn("worktree-agent-" + aid, branches(self.entity))
        ok, _msg, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertIn("zach-opus-p5", lead)                      # the merge failed: due, and told

    def test_point_05_new_work_starts_while_finished_work_is_pending(self):
        self._pending_one()
        self.spawn("zach-opus-new")
        self.assertIn("zach-opus-p5", self.names())
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
        self.spawn("zach-opus-fix", extra="lands-pending: zach-opus-p5\n")

    def test_r2_a_turn_ends_with_finished_work_pending_and_the_lead_is_told_once(self):
        """Orchestration reliability R2/R5: no blanket refusal to end a turn. While no
        watcher runs, the Stop fallback tells the lead the due work, once per change."""
        self._pending_one()
        ok, msg, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertEqual(msg, "")                                          # nothing for the person
        self.assertIn("STALL WATCH NOT ACTIVE", lead)
        self.assertIn("zach-opus-p5", lead)
        self.assertEqual(ws.gate_stop({"session_id": self.sid}, self.entity)[2], "")   # unchanged: said once
        with patch.object(ws, "_monitor_health", return_value=(True, "stall-watch: watching")):
            self.assertEqual(ws.gate_stop({"session_id": self.sid}, self.entity)[2], "")  # a watcher tells it
        self.assertIn("zach-opus-p5", ws.gate_stop({"session_id": self.sid}, self.entity)[2])  # watcher gone: again
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])                        # still owned

    def test_point_05_the_gate_answers_inside_its_budget(self):
        """"This is a guarantee, not a habit: it holds whether or not Rich
        remembers." The gate runs inside hook timeouts the platform enforces by
        CANCELING the hook and DISCARDING its output, while its work grows with
        the backlog. So it answers inside a budget: the item it could not check
        stays pending, which is the safe answer AND the cheap one, and it says
        which items those were rather than going quiet. No time is asserted
        here — only that an exhausted budget still yields a decision,
        announced, and lands nothing by guess."""
        aid, npath = self._pending_one()
        self.merge(self.entity, "worktree-agent-" + aid)     # it would auto-land
        os.environ["RICHOS_WORKSPACES_GATE_BUDGET"] = "0"
        try:
            ok, _msg, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
            self.assertTrue(ok)                              # a decision, not a dead hook
            self.assertIn("zach-opus-p5", lead)              # and the item stays due, named
            self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
            self.assertTrue(os.path.exists(npath))
            self.spawn("zach-opus-nb")                      # independent of the scan budget
            self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
        finally:
            os.environ.pop("RICHOS_WORKSPACES_GATE_BUDGET", None)
        # with its budget the same call decides the other way, on the same facts
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.assertFalse(os.path.exists(npath))

    def test_point_05_waiting_items_allow_turn_end_and_new_work(self):
        self._pending_one()
        ws.wait("zach-opus-p5", "outside", "GitHub is down", "CEO-TODOs 9.9", self.sid)
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.spawn("zach-opus-new2")
        self.assertIn("zach-opus-p5", self.names())
        with self.assertRaises(ws.SpecError):
            ws.wait("zach-opus-p5", "outside", "x", "", self.sid)   # must be on his TODO list

    def test_point_05_a_ceo_discard_question_blocks_nothing_else(self):
        """A discard waiting on the CEO stays pending without stopping other
        development or another item's automatic land."""
        aid2, npath2 = self.spawn("zach-opus-other")        # a second agent, running while nothing is pending
        aid, npath = self.spawn("zach-opus-ceo", extra="ceo-ordered: 'Implement spec.'\n")
        self.commit(npath)
        self.finish(aid)
        ws.wait("zach-opus-ceo", "ceo-discard", "May I discard the rejected spec branch?", "CEO-TODOs 1.1", self.sid)
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])   # the turn may end
        self.spawn("zach-opus-unrelated")
        self.assertIsNone(self.rec("zach-opus-ceo")["disposition"])
        # "blocks nothing else": the second agent finishes, is merged, and lands
        # on its own while the first still waits on him.
        self.commit(npath2, "other.txt")
        self.finish(aid2)
        self.merge(self.entity, "worktree-agent-" + aid2)
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.assertFalse(os.path.exists(npath2))
        self.assertEqual(ws.load_agent(ws.named_key(self.sid, "zach-opus-other"))["disposition"]["kind"], "landed")
        self.assertIsNone(self.rec("zach-opus-ceo")["disposition"])              # still waiting on his word

    def _merged_but_pending(self, name="zach-opus-p5", extra=""):
        """Finished, merged into the integration branch locally, and still
        pending: an uncommitted file keeps the automatic land from finishing
        (point 8), which is how a merged land sits pending while its checks run."""
        aid, npath = self.spawn(name, extra=extra)
        self.commit(npath)
        self.finish(aid)
        with open(os.path.join(npath, "residue.txt"), "w") as f:
            f.write("not committed\n")
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertIn(name, self.names())                 # the auto-land could not finish it
        return aid, npath

    def test_point_05_pending_merge_state_does_not_control_spawn(self):
        self._merged_but_pending()
        self.spawn("zach-opus-n1")
        ws.wait("zach-opus-p5", "started", "land checks running on the merged tree", "", self.sid, SOON())
        self.spawn("zach-opus-n2")
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])
        self.assertIn("zach-opus-p5", self.names())
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        run("git", "-C", self.entity, "reset", "-q", "--hard", "HEAD~1")
        self.spawn("zach-opus-n3")
        self.assertIn("zach-opus-p5", self.names())
        self.assertIsNone(self.rec("zach-opus-p5")["disposition"])

    def test_point_05_unmerged_started_work_does_not_block_new_work(self):
        self._pending_one()
        ws.wait("zach-opus-p5", "started", "about to merge it", "", self.sid, SOON())
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.spawn("zach-opus-n4")
        self.assertIn("zach-opus-p5", self.names())

    def test_point_05_outside_wait_does_not_block_new_work(self):
        self._merged_but_pending()
        ws.wait("zach-opus-p5", "outside", "GitHub is down", "CEO-TODOs 9.9", self.sid)
        self.spawn("zach-opus-n5")
        self.assertIn("zach-opus-p5", self.names())

    def test_point_05_ceo_word_wait_does_not_block_new_work(self):
        self._merged_but_pending("zach-opus-p5c", extra="ceo-ordered: 'Implement spec.'\n")
        ws.wait("zach-opus-p5c", "ceo-discard", "May I discard it?", "CEO-TODOs 1.1", self.sid)
        self.spawn("zach-opus-n6")
        self.assertIn("zach-opus-p5c", self.names())

    def test_point_05_an_agent_started_to_land_it_lets_the_turn_end(self):
        self._pending_one()
        self.spawn("zach-opus-helper", extra="lands-pending: zach-opus-p5\n")
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])

    def test_point_05_the_next_session_starts_with_it(self):
        self._pending_one()
        self.sess.kill()
        self.sess.wait()
        s2 = self.env.session("sess-bbbbbbbb-2222", self.entity)
        ctx, _n = ws.lifecycle({"hook_event_name": "SessionStart", "session_id": "sess-bbbbbbbb-2222",
                                "cwd": self.entity}, self.entity)
        self.assertIn("zach-opus-p5", ctx)
        self.assertIn("DUE WORK", ctx)


class Point06_Native(Base):
    """6. Native workspaces get exactly the same treatment."""

    def test_point_06_native_workspaces_registered_at_spawn_and_deleted_on_land(self):
        aid, npath = self.spawn("zach-opus-p6")
        r = self.rec("zach-opus-p6")
        self.assertEqual([w["path"] for w in r["workspaces"] if w["kind"] == "native"], [npath])
        self.finish(aid)
        self.assertEqual(self.names(), [])                  # produced nothing: landed, deleted
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))

    def test_point_06_a_landed_workspaces_build_caches_go_with_it(self):
        # CEO 2026-10-08: "Doesn't that cache clean-up belong to the Mega Lander then?" A
        # workspace's own Cargo, iOS and Android folders on the external drive are removed
        # at the moment it is deleted; a live neighbor's folders are not touched.
        import hashlib
        aid, npath = self.spawn("zach-opus-c6")
        neighbor = os.path.join(self.env.root, "another-checkout")

        def folders(checkout):
            ios = os.path.join(checkout, "richos", "mobile", "native-ios")
            android = os.path.join(checkout, "richos", "mobile", "native-android")
            return [
                os.path.join(self.env.caches, "cargo-target", "workspaces",
                             hashlib.sha256(checkout.encode()).hexdigest()[:24]),
                os.path.join(self.env.caches, "richos-native-ios",
                             hashlib.sha256(ios.encode()).hexdigest()[:10]),
                os.path.join(self.env.caches, "richos-native-android",
                             hashlib.sha1(android.encode()).hexdigest()[:10]),
            ]
        mine, theirs = folders(npath), folders(neighbor)
        for f in mine + theirs:
            os.makedirs(os.path.join(f, "debug"))
            with open(os.path.join(f, "debug", "artifact"), "w") as fh:
                fh.write("x" * 4096)
        self.finish(aid)
        self.assertEqual(self.names(), [])                  # produced nothing: landed, deleted
        self.assertFalse(os.path.exists(npath))
        for f in mine:
            self.assertFalse(os.path.exists(f), f)
        for f in theirs:
            self.assertTrue(os.path.exists(f), f)

    def test_point_06_subagentstart_before_the_agent_result(self):
        payload = {"session_id": self.sid, "tool_use_id": "tu-s", "tool_name": "Agent",
                   "tool_input": {"name": "zach-opus-s", "prompt": "x", "isolation": "worktree"}}
        ws.register_spawn(payload, self.entity)
        aid = "aorderfirst0001"
        npath = os.path.join(self.entity, ".claude", "worktrees", "agent-" + aid)
        run("git", "-C", self.entity, "worktree", "add", "-q", npath, "-b", "worktree-agent-" + aid)
        ws.record_start(self.sid, aid, npath, "zach")
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid})[0], "REGISTERED")
        ws.bind_agent(self.sid, "tu-s", aid, self.entity)
        r = self.rec("zach-opus-s")
        self.assertEqual(r["agent_id"], aid)
        self.assertEqual(len(r["workspaces"]), 1)


class Point07_LandedOrDiscarded(Base):
    """7. Finished work ends in exactly one of two ways — landed or discarded."""

    def test_point_07_discard_deletes_with_the_reason_recorded(self):
        aid, npath = self.spawn("zach-opus-d")
        self.commit(npath)
        self.finish(aid)
        with self.assertRaises(ws.SpecError):
            ws.discard("zach-opus-d", "reviewer rejected it", me=self.sid)   # must say whose order
        ws.discard("zach-opus-d", "reviewer rejected it", not_ceo_ordered="Rich's own cleanup task", me=self.sid)
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        d = self.rec("zach-opus-d")["disposition"]
        self.assertEqual((d["kind"], d["reason"]), ("discarded", "reviewer rejected it"))
        self.assertTrue(d["tips"])
        self.assertEqual(self.names(), [])

    def test_point_07_ceo_ordered_work_needs_his_word(self):
        aid, npath = self.spawn("zach-opus-o", extra="ceo-ordered: 'Implement spec.'\n")
        self.commit(npath)
        self.finish(aid)
        with self.assertRaises(ws.SpecError):
            ws.discard("zach-opus-o", "half done and superseded", not_ceo_ordered="it was not his order at all", me=self.sid)
        ws.discard("zach-opus-o", "half done and superseded", ceo_word="yes, drop it", me=self.sid)
        self.assertFalse(os.path.exists(npath))

    def test_point_07_an_agent_that_produced_nothing_counts_as_landed(self):
        cc = self.make_cc("zach-opus-n")
        aid, npath = self.spawn("zach-opus-n", cc=cc)
        self.finish(aid)
        self.assertEqual(self.names(), [])
        self.assertEqual(self.rec("zach-opus-n")["disposition"]["kind"], "landed")
        self.assertFalse(os.path.exists(cc) or os.path.exists(npath))

    def test_point_07_an_agent_with_no_workspaces_at_all_is_landed_not_pending(self):
        """The precondition registering read-only agents depends on: land()
        returns landed for an EMPTY workspace list rather than raising. If it
        raised, every Explore spawn would become a pending item nothing could
        end, and point 5 would block the CEO's turn ends forever."""
        aid = self.spawn_readonly("anoworkspaces01")
        rec = ws.load_agent(ws.key_for_id(aid))
        self.assertEqual(rec["workspaces"], [])
        ws.record_end(self.sid, aid, "SubagentStop")
        self.assertEqual(ws.land(rec["key"], self.sid), {"landed": True})
        self.assertEqual(self.names(), [])
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])

    def test_point_07_unfinished_work_is_continued_by_a_new_agent(self):
        cc = self.make_cc("zach-opus-old")
        aid, npath = self.spawn("zach-opus-old", cc=cc)
        self.commit(cc)
        self.finish(aid)
        cc2 = self.make_cc("zach-opus-new", base="cc/zach-opus-old")
        aid2, npath2 = self.spawn("zach-opus-new", cc=cc2, extra="continues: zach-opus-old\n")
        # the old workspaces are deleted when the new agent starts; its branch stays
        self.assertFalse(os.path.exists(cc) or os.path.exists(npath))
        self.assertIn("cc/zach-opus-old", branches(self.other))
        self.assertEqual(self.rec("zach-opus-old")["disposition"]["kind"], "continued")
        self.commit(cc2, "more.txt")
        self.finish(aid2)
        self.merge(self.other, "cc/zach-opus-new")
        self.assertEqual(self.names(), [])
        self.assertNotIn("cc/zach-opus-old", branches(self.other))
        self.assertNotIn("cc/zach-opus-new", branches(self.other))
        self.assertEqual(self.rec("zach-opus-old")["disposition"]["kind"], "landed")


class Point08_NothingUncommitted(Base):
    """8. Nothing uncommitted is ever landed."""

    def test_point_08_nothing_uncommitted_is_ever_landed(self):
        aid, npath = self.spawn("zach-opus-u")
        self.commit(npath)
        with open(os.path.join(npath, "left.txt"), "w") as f:
            f.write("uncommitted\n")
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), ["zach-opus-u"])      # not landed automatically
        with self.assertRaises(ws.SpecError) as cm:
            ws.land("zach-opus-u", self.sid)
        self.assertIn("uncommitted", str(cm.exception))
        self.assertTrue(os.path.exists(os.path.join(npath, "left.txt")))
        # Rich commits what it left to its branch, then it lands
        run("git", "-C", npath, "add", "left.txt")
        run("git", "-C", npath, "commit", "-q", "-m", "left")
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), [])

    def test_point_08_ignored_files_it_needs(self):
        aid, npath = self.spawn("zach-opus-i")
        os.makedirs(os.path.join(npath, "build"))
        with open(os.path.join(npath, "build", "report.html"), "w") as f:
            f.write("evidence\n")
        self.finish(aid)
        with self.assertRaises(ws.SpecError) as cm:
            ws.land("zach-opus-i", self.sid)
        self.assertIn("ignored", str(cm.exception))
        ws.land("zach-opus-i", self.sid, ignored_ok="build output, regenerated by the next run")
        self.assertFalse(os.path.exists(npath))


class Point08b_BorrowedBranch(Base):
    """8. "Deletion therefore never loses anything that was meant to land" --
    in the destructive direction."""

    def test_point_08_a_pre_existing_branch_the_agent_only_checked_out_survives(self):
        """A branch that was already there, carrying somebody else's unlanded
        commit, which the agent merely CHECKS OUT in its own workspace. Under
        attribution by possession it became the agent's: its land was refused
        because that branch is not in main, and its discard deleted the branch
        with `git branch -D`. A ref that already existed is never the agent's,
        however long the agent holds it -- it is not in the record as created by
        anyone, and creation is the only thing that attributes."""
        keep = os.path.join(self.env.root, "human-wt")
        run("git", "-C", self.entity, "worktree", "add", "-q", keep, "-b", "human/keep")
        self.commit(keep, "human.txt")                       # not in main, and not the agent's
        tip = run("git", "-C", self.entity, "rev-parse", "human/keep").stdout.strip()
        run("git", "-C", self.entity, "worktree", "remove", "--force", keep)
        aid, npath = self.spawn("zach-opus-bb")
        self.pre(aid)
        run("git", "-C", npath, "checkout", "-q", "human/keep")      # borrowed, not created
        self.post(aid)
        self.tool_call(aid)                                          # still on it at its next call
        self.assertEqual(self.created("zach-opus-bb"), [])
        self.finish(aid)                                             # ...and at its end of run
        self.assertEqual(self.created("zach-opus-bb"), [])
        ws.discard("zach-opus-bb", "the reviewer rejected the approach",
                   not_ceo_ordered="an internal experiment", me=self.sid)
        self.assertIn("human/keep", branches(self.entity))
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "human/keep").stdout.strip(), tip)
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))


class Point09_NeverWritesAgain(Base):
    """9. A finished agent never writes again."""

    def test_point_09_a_finished_agent_is_refused_every_tool(self):
        aid, npath = self.spawn("zach-opus-l")
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Edit"})[0],
                         "REGISTERED")
        self.commit(npath)
        self.finish(aid)
        for tool in ("Read", "Bash", "Edit"):
            self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": tool})[0],
                             "FINISHED")
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), [])
        # restarted after its workspace is gone: still refused
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Write"})[0],
                         "FINISHED")

    def test_point_09_a_restarted_read_only_agent_is_refused_every_tool(self):
        """A read-only type needs no workspace, so the spawn guard exempted it
        from isolation — and, by the same early return, from registration. The
        lock-out finds an agent through its registration, so it never fired for
        one. Explore's allowlist is every tool except Edit/Write/NotebookEdit:
        it carries Bash, so a restarted finished Explore could write."""
        aid = self.spawn_readonly("aexplorero0001")
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid,
                                     "tool_name": "Bash"})[0], "REGISTERED")
        ws.record_end(self.sid, aid, "SubagentStop")
        for tool in ("Read", "Bash", "Grep"):
            self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid,
                                         "tool_name": tool})[0], "FINISHED")
        # and it blocks nothing: it produced nothing, so it is already landed
        self.assertEqual(self.names(), [])
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid,
                                     "tool_name": "Bash"})[0], "FINISHED")

    def test_point_09_a_paused_agent_is_not_locked_out(self):
        aid, npath = self.spawn("zach-opus-pz")
        ws.pause("zach-opus-pz", "the quota reset", self.sid)
        self.finish(aid)
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid})[0], "REGISTERED")

    def test_point_09_every_process_it_started_is_stopped_before_deletion(self):
        aid, npath = self.spawn("zach-opus-pr")
        pr = subprocess.Popen(["sleep", "300"], cwd=npath)
        self.env.procs.append(pr)
        time.sleep(0.2)
        self.finish(aid)
        self.assertEqual(self.names(), [])
        pr.wait(timeout=10)
        self.assertIsNotNone(pr.returncode)
        self.assertFalse(os.path.exists(npath))
        ev = open(os.path.join(ws.state_dir(), "events.jsonl")).read()
        self.assertIn('"event": "processes-stopped"', ev)

    def test_point_09_a_killed_process_the_kernel_is_slow_to_remove_is_not_a_survivor(self):
        # THE RACE (2026-09-29, the load-sensitive checks audit, item 3). The
        # land used to SIGKILL, sleep 0.2 s and record whatever the kernel had
        # not yet torn down as a SURVIVOR, which fails the land. A busy Mac
        # takes longer than that to remove a killed process. The teardown's
        # lag is simulated here, deterministically: `_alive` keeps reporting
        # the killed holder for 0.6 s after its SIGKILL was sent, as a loaded
        # kernel does, and then reports what the operating system says.
        d, pr = self.term_ignoring_holder()
        real_alive, real_kill = ws._alive, os.kill
        killed_at = {}

        def kill(pid, sig):
            if sig == signal.SIGKILL:
                killed_at.setdefault(pid, time.monotonic())
            return real_kill(pid, sig)

        def alive(pid):
            if pid in killed_at and time.monotonic() - killed_at[pid] < 0.6:
                return True
            return real_alive(pid)

        with patch.object(ws.os, "kill", kill), patch.object(ws, "_alive", alive):
            res = ws.stop_processes([d])
        self.assertEqual(res["stopped"], [pr.pid])
        self.assertIn(pr.pid, killed_at, "the holder ignores TERM, so it was never sent a KILL")
        self.assertEqual(res["survivors"], [], "a killed process that took 0.6 s to be removed was recorded as surviving")
        self.assertEqual(pr.wait(timeout=30), -signal.SIGKILL)

    def test_point_09_a_process_the_kernel_never_removes_is_still_a_survivor(self):
        # THE OTHER HALF: waiting for the kernel is bounded, and a process that
        # outlives the bound is reported, not waited on for ever. `_alive`
        # says the holder is alive whatever happens to it.
        d, pr = self.term_ignoring_holder()
        with patch.object(ws, "_alive", lambda pid: True), patch.object(ws, "PROCESS_KILL_WAIT", 0.5):
            res = ws.stop_processes([d])
        self.assertEqual(res["survivors"], [pr.pid])
        self.assertEqual(pr.wait(timeout=30), -signal.SIGKILL)


class LandingShutdown(Base):
    def shutdown_writer(self, path, commit=False):
        code = """import signal, pathlib, subprocess, sys, time
commit = sys.argv[1] == 'commit'
def stopped(*args):
    pathlib.Path('shutdown-result.txt').write_text('result flushed on shutdown\\n')
    if commit:
        subprocess.run(['git', 'add', 'shutdown-result.txt'], check=True)
        subprocess.run(['git', 'commit', '-qm', 'shutdown result'], check=True)
    print('FLUSHED', flush=True)
    sys.exit(0)
signal.signal(signal.SIGTERM, stopped)
print('READY', flush=True)
while True: time.sleep(.1)
"""
        pr = subprocess.Popen([sys.executable, '-c', code, 'commit' if commit else 'write'],
                              cwd=path, stdout=subprocess.PIPE, text=True)
        self.env.procs.append(pr)
        self.assertEqual(pr.stdout.readline().strip(), 'READY')
        # Reap immediately so the OS liveness check does not observe a zombie.
        threading.Thread(target=pr.wait, daemon=True).start()
        return pr

    def check_shutdown(self, commit):
        name = 'zach-opus-flush'
        cc = self.make_cc(name)
        aid, native = self.spawn(name, cc)
        pr = self.shutdown_writer(cc, commit)
        self.finish(aid)
        with self.assertRaises(ws.SpecError):
            ws.land(name, self.sid)
        self.assertEqual(pr.stdout.readline().strip(), 'FLUSHED')
        self.assertTrue(os.path.exists(os.path.join(cc, 'shutdown-result.txt')))
        self.assertTrue(os.path.isdir(native))
        self.assertFalse(self.rec(name).get('disposition'))
        if not commit:
            self.commit(cc, 'shutdown-result.txt', 'result flushed on shutdown\n')
        self.merge(self.other, 'cc/' + name)
        ws.land(name, self.sid)
        self.assertFalse(os.path.exists(cc))
        self.assertFalse(os.path.exists(native))

    def test_land_preserves_a_file_flushed_during_shutdown(self):
        self.check_shutdown(False)

    def test_land_rechecks_commits_made_during_shutdown(self):
        self.check_shutdown(True)

    def check_retry(self, commit):
        name = 'zach-opus-retry'
        aid, path = self.spawn(name)
        self.finish(aid)
        with patch.object(ws, 'remove_workspace', return_value=(False, 'simulated disk refusal')):
            ws.land(name, self.sid)
        self.assertTrue(self.rec(name)['deletion'])
        if commit:
            self.commit(path, 'later.txt', 'new work after the first land check\n')
        else:
            with open(os.path.join(path, 'later.txt'), 'w') as f:
                f.write('new work after the first land check\n')
        ws.retry_due()
        self.assertTrue(os.path.isfile(os.path.join(path, 'later.txt')))
        self.assertIn('worktree-agent-' + aid, branches(self.entity))
        self.assertFalse(self.rec(name).get('disposition'))
        self.assertIn(name, self.names())

    def test_retry_preserves_new_uncommitted_work(self):
        self.check_retry(False)

    def test_retry_preserves_new_unmerged_commits(self):
        self.check_retry(True)


    def partial_cleanup(self, change=None, remove_pointer=False, dev=False):
        name = 'zach-opus-partial'
        if dev:
            run('git', '-C', self.entity, 'branch', 'dev/partial')
            ws.record_integration(self.entity, 'dev/partial', 'this body of work', self.sid)
        aid, path = self.spawn(name)
        self.commit(path, 'kept.txt', 'already merged\n')
        if dev:
            run('git', '-C', self.entity, 'branch', '-f', 'dev/partial', 'worktree-agent-' + aid)
            self.assertFalse(os.path.exists(os.path.join(self.entity, 'kept.txt')))
        else:
            self.merge(self.entity, 'worktree-agent-' + aid)
        self.finish(aid)
        def partial_remove(w, deadline=None):
            pointer = os.path.join(w['path'], '.git')
            with open(pointer) as f:
                admin = f.read().removeprefix('gitdir:').strip()
            shutil.rmtree(admin)
            if remove_pointer:
                os.unlink(pointer)
            return False, 'simulated Git removal that lost its admin directory'
        with patch.object(ws, 'remove_workspace', side_effect=partial_remove):
            ws.land(name, self.sid)
        self.assertTrue(self.rec(name)['deletion'])
        if change:
            with open(os.path.join(path, change), 'w') as f:
                f.write('new work after partial deletion\n')
        ws.retry_due()
        if change:
            self.assertTrue(os.path.isfile(os.path.join(path, change)))
            self.assertFalse(self.rec(name).get('disposition'))
            # Once the preserved work reaches main, landing can finish even though
            # the partial removal already destroyed the workspace's Git metadata.
            self.commit(self.entity, change, 'new work after partial deletion\n')
            ws.land(name, self.sid)
            self.assertFalse(os.path.exists(path))
        else:
            self.assertFalse(os.path.exists(path))
            self.assertFalse(self.rec(name).get('deletion'))

    def test_partial_cleanup_retries_without_git_metadata(self):
        self.partial_cleanup()

    def test_partial_cleanup_uses_the_recorded_dev_branch(self):
        self.partial_cleanup(dev=True)

    def test_partial_cleanup_preserves_a_changed_file(self):
        self.partial_cleanup('kept.txt')

    def test_partial_cleanup_preserves_new_work_without_a_git_pointer(self):
        self.partial_cleanup('new.txt', remove_pointer=True)


class Point10_AllTogether(Base):
    """10. All of an agent's workspaces go together."""

    def test_point_10_a_cross_repository_agent_loses_both_workspaces_as_one(self):
        cc = self.make_cc("zach-opus-x")
        aid, npath = self.spawn("zach-opus-x", cc=cc)
        self.commit(cc)
        self.finish(aid)
        self.merge(self.other, "cc/zach-opus-x")
        self.assertEqual(self.names(), [])
        self.assertFalse(os.path.exists(cc))
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("cc/zach-opus-x", branches(self.other))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertTrue(os.path.exists(ws.done_path(ws.named_key(self.sid, "zach-opus-x"))))

    def test_point_10_branches_created_in_a_workspace_go_with_it(self):
        """"Any branch an agent created" (point 3) -- and `git branch spare`
        CREATES one. It checks nothing out, so possession could not see it and it
        was left behind, against point 10's "none is left behind". Creation is
        recorded instead: the ref is absent when the agent's tool call starts,
        present when it ends, and carries the agent's own unlanded work."""
        aid, npath = self.spawn("zach-opus-b")
        self.tool_call(aid)
        self.pre(aid)
        run("git", "-C", npath, "checkout", "-q", "-b", "side-branch")
        self.commit(npath, "side.txt")
        run("git", "-C", npath, "branch", "plain-branch")    # checked out NOWHERE, still created
        self.post(aid)
        self.assertEqual(self.created("zach-opus-b"), ["plain-branch", "side-branch"])
        self.finish(aid)
        ws.discard("zach-opus-b", "not wanted any more", not_ceo_ordered="a probe of branch attribution", me=self.sid)
        self.assertNotIn("side-branch", branches(self.entity))
        self.assertNotIn("plain-branch", branches(self.entity))
        self.assertFalse(os.path.exists(npath))

    def test_point_03_a_ref_created_after_the_last_post_is_still_the_agents(self):
        """Point 3 ("any branch an agent created") with point 9 ("every process
        it started is stopped before its workspaces are deleted"): a
        backgrounded process outlives its tool call, so a ref it creates
        appears AFTER that call's PostToolUse consumed the window. The end-of-run
        signal compares once more against the LAST snapshot the agent took, so
        the ref is the agent's and goes with its work (round 8, item 8; the one
        red probe of round 7, esc-20260912T225456Z-d34bf4e6)."""
        aid, npath = self.spawn("zach-opus-late")
        self.commit(npath, "late.txt")
        self.tool_call(aid, "tu-late-1")                      # a whole call: its window is consumed
        self.assertEqual(self.created("zach-opus-late"), [])
        run("git", "-C", npath, "branch", "late/side")        # created after the last PostToolUse
        self.finish(aid)                                      # the end signal is the last observation
        self.assertEqual(self.created("zach-opus-late"), ["late/side"])
        with self.assertRaises(ws.SpecError) as e:            # it holds the land while unmerged
            ws.land("zach-opus-late", self.sid)
        self.assertIn("late/side", str(e.exception))
        self.merge(self.entity, "worktree-agent-" + aid)
        ws.land("zach-opus-late", self.sid)
        self.assertNotIn("late/side", branches(self.entity))
        self.assertFalse(os.path.exists(npath))

    def test_point_03_a_backgrounded_calls_ref_after_its_post_is_still_the_agents(self):
        """Point 3 with point 9. A Bash call the platform stamped
        `run_in_background` has, by the platform's own word, a process still
        running after its PostToolUse. Its window is kept open and judged —
        against its OWN before-set — at the agent's next observation, so the
        ref that process creates after the Post, which the next call's snapshot
        already holds, is still the agent's (round 8, item 8: the RED probe's
        two cases). The same ref cut by Rich between two ordinary CLOSED calls
        stays Rich's (test_point_08_a_branch_that_existed_before_the_call_is_
        never_created_in_it): git cannot tell the two apart, and the stamped
        field is what separates them."""
        aid, npath = self.spawn("zach-opus-bg")
        self.commit(npath, "bg.txt")
        self.pre(aid, "tu-bg")
        ws.observe({"session_id": self.sid, "agent_id": aid, "tool_name": "Bash",
                    "hook_event_name": "PostToolUse", "tool_use_id": "tu-bg",
                    "tool_input": {"command": "sleep 3 && git branch bg/side", "run_in_background": True}})
        run("git", "-C", npath, "branch", "bg/side")          # the process, after its call's Post
        self.tool_call(aid, "tu-next")                        # the next call: snapshot holds it; its Post judges the background window
        self.assertEqual(self.created("zach-opus-bg"), ["bg/side"])
        self.finish(aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-bg", self.sid)
        self.assertIn("bg/side", str(e.exception))
        self.merge(self.entity, "worktree-agent-" + aid)
        ws.land("zach-opus-bg", self.sid)
        self.assertNotIn("bg/side", branches(self.entity))

    def test_point_10_a_side_branch_switched_away_from_blocks_the_land(self):
        """Work committed on a branch the agent then switched away from. Nothing
        attributed it, so `land()` reported SUCCESS, `pending()` listed nothing,
        and the agent's commit was in no integration branch: point 5's "everything
        it produced is landed", and point 8's "deletion therefore never loses
        anything that was meant to land", both broken quietly.

        All of it in ONE tool call, which is the case a window between two calls
        cannot see: by the time anything looks, the agent is back on its own
        branch and that branch is empty. Its workspace's own HEAD reflog still
        carries the commit it made, which is what makes the side branch its own."""
        aid, npath = self.spawn("zach-opus-sb")
        self.pre(aid)
        run("git", "-C", npath, "checkout", "-q", "-b", "sidework")
        self.commit(npath, "side.txt")
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.post(aid)
        self.assertEqual(self.created("zach-opus-sb"), ["sidework"])
        self.commit(npath, "own.txt")
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)     # its own branch is in...
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-sb", self.sid)                # ...the side branch is not
        self.assertIn("sidework", str(e.exception))
        self.assertEqual(self.names(), ["zach-opus-sb"])     # so it stays pending (point 5)
        self.assertTrue(os.path.exists(npath))
        self.merge(self.entity, "sidework")                  # merged: nothing is lost
        self.assertEqual(self.names(), [])                   # and it lands on its own (point 4)
        self.assertNotIn("sidework", branches(self.entity))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertFalse(os.path.exists(npath))

    def test_point_10_a_branch_rich_cut_from_its_branch_is_not_the_agents(self):
        """"Deletion therefore never loses anything that was meant to land"
        (point 8). git writes `branch: Created from ...` whoever ran it, so a
        copy Rich cuts to rescue the work used to be deleted with the agent —
        or, carrying a commit of its own, used to hold its land hostage."""
        aid, npath = self.spawn("zach-opus-r")
        self.tool_call(aid)
        self.commit(npath)
        self.tool_call(aid)
        self.finish(aid)
        # the platform restarts finished agents (point 9), so the pair can open
        # AFTER the run has ended -- and both halves refuse a finished agent,
        # which is what keeps Rich's rescue his.
        self.assertEqual(self.pre(aid)[0], "FINISHED")
        # Rich rescues the work: a copy cut inside the agent's own workspace,
        # with a commit of his own on top, after the agent's run has ended.
        run("git", "-C", npath, "checkout", "-q", "-b", "rescue-rich")
        self.commit(npath, "rescued.txt")
        self.post(aid)
        self.assertEqual(self.created("zach-opus-r"), [])
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        ws.land("zach-opus-r", self.sid)                     # not held hostage by rescue-rich
        self.assertIn("rescue-rich", branches(self.entity))  # and not deleted with the agent
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))


class Point11_Finished(Base):
    """11. "Finished" means the agent's run has ended and Rich did not pause it."""

    def test_point_11_finished_means_run_ended_and_not_paused(self):
        aid, _n = self.spawn("zach-opus-f")
        r = self.rec("zach-opus-f")
        self.assertEqual(ws.finished_state(r)[:2], (False, False))
        self.finish(aid)
        self.assertEqual(ws.finished_state(self.rec("zach-opus-f"))[:2], (True, False))

    def test_point_11_a_recorded_pause_is_not_finished_and_resumes(self):
        aid, _n = self.spawn("zach-opus-q")
        ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "SendMessage", "session_id": self.sid,
                      "tool_input": {"to": "zach-opus-q", "message": "commit and hold\npause-until: the CEO's answer"}},
                     self.entity)
        self.finish(aid)
        self.assertEqual(ws.finished_state(self.rec("zach-opus-q"))[:2], (False, True))
        self.assertEqual(self.names(), [])
        ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "SendMessage", "session_id": self.sid,
                      "tool_input": {"to": "zach-opus-q", "message": "go on"}}, self.entity)
        self.assertEqual(ws.finished_state(self.rec("zach-opus-q"))[:2], (False, False))

    def test_point_11_a_pause_with_nothing_named_is_pending(self):
        aid, _n = self.spawn("zach-opus-e")
        ws.pause("zach-opus-e", "", self.sid)
        self.finish(aid)
        self.assertEqual(self.names(), ["zach-opus-e"])

    def test_point_11_handed_in_then_ended_is_finished_even_if_paused(self):
        aid, _n = self.spawn("zach-opus-h")
        ws.pause("zach-opus-h", "the quota reset", self.sid)
        ws.lifecycle({"hook_event_name": "TaskCompleted", "session_id": self.sid, "teammate_name": "zach-opus-h"},
                     self.entity)
        self.finish(aid)
        self.assertTrue(ws.finished_state(self.rec("zach-opus-h"))[0])

    def test_point_11_a_stopped_pause_is_finished(self):
        aid, _n = self.spawn("zach-opus-st")
        ws.pause("zach-opus-st", "the CEO's answer", self.sid)
        self.finish(aid)
        ws.stop("zach-opus-st", "no longer wanted", self.sid)
        self.assertTrue(ws.finished_state(self.rec("zach-opus-st"))[0])
        # and a TaskStop result is the platform's own signal
        aid2, _n2 = self.spawn("zach-opus-ts", extra="lands-pending: zach-opus-st\n")
        ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "TaskStop", "session_id": self.sid,
                      "tool_response": json.dumps({"task_id": aid2, "message": "stopped"})}, self.entity)
        self.assertTrue(ws.finished_state(self.rec("zach-opus-ts"))[0])


class Point03_MissedRegistration(Base):
    """3. "Two events, nothing else: the agent is spawned and its workspace
    registered ... If registration fails, the spawn does not happen." — read
    together with point 11's "automatically and never by Rich noticing".

    THE REGISTRATION IS WRITTEN BY A HOOK, AND THE PLATFORM MAY KILL A HOOK.
    Measured 2026-09-17: `guard-worktree-isolation.sh` was killed at 10.027s
    against its 10s budget (`hook_cancelled … timedOut: true` for tool_use_id
    toolu_01C3dCBDwFksv5tBHramMDch) and the platform LAUNCHED THE AGENT ANYWAY.
    The registry was left with a named record holding a cc/ workspace and no
    agent id, beside a provisional record holding the platform's own start and
    end for the same run, and `land` refused the finished work — "its run has
    not ended" — with no automatic way left to retire the workspace.

    These tests are the two halves of the repair and the two halves of its
    floor: the facts the platform recorded are ADOPTED, and nothing else is."""

    def missed_spawn(self, name, agent_id=None, agent_type="sage"):
        """A spawn whose PreToolUse registration never ran: no register_spawn,
        and the platform starts the agent regardless (SubagentStart)."""
        aid = agent_id or ("a" + name.replace("-", "")[:12] + "0000")
        npath = os.path.join(self.entity, ".claude", "worktrees", "agent-" + aid)
        run("git", "-C", self.entity, "worktree", "add", "-q", npath, "-b", "worktree-agent-" + aid)
        ws.record_start(self.sid, aid, npath, agent_type)
        return aid, npath

    def platform_record(self, agent_id, name, tool_use_id="", agent_type="sage"):
        """The per-agent record the platform writes beside the agent's
        transcript. It carries the NAME the spawn used and the TOOL CALL it came
        from; this suite writes it exactly as the platform does and never reads
        the registry to build it."""
        d = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "projects", "-entity", self.sid, "subagents")
        os.makedirs(d, exist_ok=True)
        rec = {"agentType": agent_type, "description": "d", "name": name, "spawnDepth": 1,
               "requestShape": "background"}
        if tool_use_id:
            rec["toolUseId"] = tool_use_id
        with open(os.path.join(d, "agent-%s.meta.json" % agent_id), "w") as f:
            json.dump(rec, f)

    def events_for(self, key, what=""):
        out = []
        path = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "state", "workspaces", "events.jsonl")
        if not os.path.exists(path):
            return out
        with open(path) as f:
            for line in f:
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                if e.get("key") == key and (not what or e.get("event") == what):
                    out.append(e)
        return out

    def test_point_03_a_spawn_whose_registration_was_killed_is_bound_at_its_post(self):
        """The Post is the first moment the gap exists, so it is where it is
        closed: the platform delivers it for a tool that actually ran, and it
        carries both the name the spawn used and the id it became."""
        cc = self.make_cc("sage-opus-killed1")
        aid, npath = self.missed_spawn("sage-opus-killed1")
        self.assertIsNone(self.rec("sage-opus-killed1").get("agent_id") or None)
        ws.lifecycle({"hook_event_name": "PostToolUse", "session_id": self.sid, "tool_name": "Agent",
                      "tool_use_id": "tu-killed1",
                      "tool_input": {"name": "sage-opus-killed1", "isolation": "worktree"},
                      "tool_response": {"agentId": aid, "status": "async_launched"}}, self.entity)
        r = self.rec("sage-opus-killed1")
        self.assertEqual(r["agent_id"], aid)
        self.assertEqual(r["tool_use_id"], "tu-killed1")
        self.assertEqual(r["registration_reconciled"]["source"], "PostToolUse[Agent]")
        # the provisional record's facts came with it, and it is gone
        self.assertTrue(r.get("started_at"))
        self.assertFalse(os.path.exists(ws.agent_path(ws._provisional_key(self.sid, aid))))
        self.assertEqual([e["event"] for e in self.events_for(r["key"], "bound")], ["bound"])
        self.assertEqual(self.events_for(r["key"], "bound")[0]["reconciled"], "PostToolUse[Agent]")
        # and from here it is an ordinary agent: it ends, its work lands, both
        # workspaces and both branches go (points 4, 10)
        self.commit(cc)
        self.finish(aid)
        self.merge(self.other, "cc/sage-opus-killed1")
        self.assertEqual(self.names(), [])
        self.assertFalse(os.path.exists(cc))
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("cc/sage-opus-killed1", branches(self.other))

    def test_point_03_a_spawn_the_registry_never_saw_is_reconciled_from_the_platforms_own_record(self):
        """Both hooks missed — the case that actually stranded a workspace. The
        registration has no id and no ending; the provisional record has the
        platform's own start AND end; the platform's per-agent record says they
        are one agent. `land` adopts that and succeeds, and the event log says
        how."""
        cc = self.make_cc("sage-opus-missed1")
        aid, npath = self.missed_spawn("sage-opus-missed1")
        ws.record_end(self.sid, aid, "SubagentStop")          # recorded on the PROVISIONAL
        self.platform_record(aid, "sage-opus-missed1", tool_use_id="tu-missed1")
        key = ws.named_key(self.sid, "sage-opus-missed1")
        self.assertFalse(ws.finished_state(ws.load_agent(key))[0])   # before: not finished
        self.commit(cc)
        self.merge(self.other, "cc/sage-opus-missed1")
        ws.land("sage-opus-missed1", self.sid)
        r = ws.load_agent(key)
        self.assertEqual(r["agent_id"], aid)
        self.assertEqual(r["registration_reconciled"]["source"], "platform-record")
        self.assertEqual(r["tool_use_id"], "tu-missed1")
        self.assertEqual(r["disposition"]["kind"], "landed")
        # the events say how it was reconciled, and from which record
        rec_events = self.events_for(key, "reconciled")
        self.assertEqual(len(rec_events), 1)
        self.assertEqual(rec_events[0]["source"], "platform-record")
        self.assertEqual(rec_events[0]["agent_id"], aid)
        self.assertEqual(rec_events[0]["adopted"], ws._provisional_key(self.sid, aid))
        # both workspaces and both branches are gone (points 4, 10)
        self.assertFalse(os.path.exists(cc))
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("cc/sage-opus-missed1", branches(self.other))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))

    def test_point_03_a_registration_with_no_platform_record_is_never_reconciled(self):
        """THE FLOOR. A registration whose spawn never happened has no id, no
        provisional twin and no per-agent record — and stays exactly that.
        Adopting anything here would be inventing an agent, and `withdraw-cc`
        (not a reconciliation) is what retires a spawn that never happened."""
        cc = self.make_cc("sage-opus-never1")
        key = ws.named_key(self.sid, "sage-opus-never1")
        with self.assertRaises(ws.SpecError) as e:
            ws.land("sage-opus-never1", self.sid)
        self.assertIn("its run has not ended", str(e.exception))
        r = ws.load_agent(key)
        self.assertEqual(r["agent_id"], "")
        self.assertIsNone(r.get("registration_reconciled"))
        self.assertEqual(self.events_for(key, "reconciled"), [])
        self.assertTrue(os.path.exists(cc))

    def test_point_03_a_reconciled_binding_never_invents_an_ending(self):
        """A per-agent record proves WHICH agent the call became; it does not
        prove the run ended. The id is adopted, the agent stays unfinished, and
        the automatic land leaves its workspace alone until an ending is
        recorded — then the same path takes it."""
        cc = self.make_cc("sage-opus-running1")
        aid, _n = self.missed_spawn("sage-opus-running1")
        self.platform_record(aid, "sage-opus-running1", tool_use_id="tu-running1")
        key = ws.named_key(self.sid, "sage-opus-running1")
        self.assertEqual(self.names(), [])                    # nothing pending: it is still working
        r = ws.load_agent(key)
        self.assertEqual(r["agent_id"], aid)                  # ...but bound now
        self.assertIsNone(r.get("end"))
        with self.assertRaises(ws.SpecError) as e:
            ws.land("sage-opus-running1", self.sid)
        self.assertIn("its run has not ended", str(e.exception))
        self.assertTrue(os.path.exists(cc))
        ws.record_end(self.sid, aid, "SubagentStop")          # now it ends, on the record itself
        self.assertTrue(ws.finished_state(ws.load_agent(key))[0])

    def test_point_03_an_ambiguous_platform_record_is_never_guessed_at(self):
        """Two of the platform's records answering to one name is not a
        reconciliation to choose between; nothing is adopted and the refusal
        stands."""
        self.make_cc("sage-opus-twin1")
        aid, _n = self.missed_spawn("sage-opus-twin1")
        self.platform_record(aid, "sage-opus-twin1")
        self.platform_record("a" + "f" * 16, "sage-opus-twin1")
        key = ws.named_key(self.sid, "sage-opus-twin1")
        ws.record_end(self.sid, aid, "SubagentStop")
        with self.assertRaises(ws.SpecError) as e:
            ws.land("sage-opus-twin1", self.sid)
        self.assertIn("its run has not ended", str(e.exception))
        self.assertEqual(ws.load_agent(key)["agent_id"], "")


class Point12_Sessions(Base):
    """12. An agent cannot outlive its session."""

    def test_point_12_session_end_recorded_finishes_its_agents(self):
        self.spawn("zach-opus-se")
        ws.lifecycle({"hook_event_name": "SessionEnd", "session_id": self.sid, "reason": "exit"}, self.entity)
        self.assertTrue(ws.finished_state(self.rec("zach-opus-se"))[0])

    def test_point_12_process_gone_finishes_its_agents(self):
        aid, npath = self.spawn("zach-opus-cr")
        self.commit(npath)
        self.sess.kill()
        self.sess.wait()
        fin, _p, why = ws.finished_state(self.rec("zach-opus-cr"))
        self.assertTrue(fin)
        self.assertIn("no longer exists", why)
        s2 = self.env.session("sess-next-3333", self.entity)
        self.assertEqual(sorted(i["name"] for i in ws.pending("sess-next-3333", self.entity)), ["zach-opus-cr"])

    def test_point_12_a_reused_process_number_is_not_the_session(self):
        self.spawn("zach-opus-ru")
        path = ws.session_path(self.sid)
        rec = json.load(open(path))
        rec["pid_start"] = "Thu Jan  1 00:00:00 1970"
        ws.write_json(path, rec)
        fin, _p, why = ws.finished_state(self.rec("zach-opus-ru"))
        self.assertTrue(fin)
        self.assertIn("no longer session", why)

    def test_point_12_two_sessions_each_handle_their_own(self):
        aid, npath = self.spawn("zach-opus-a1")
        self.commit(npath)
        self.finish(aid)
        s2 = self.env.session("sess-other-4444", self.entity)
        self.assertEqual(ws.pending("sess-other-4444", self.entity), [])
        self.env.use(self.sess)
        self.assertEqual(sorted(i["name"] for i in ws.pending(self.sid, self.entity)), ["zach-opus-a1"])


class Point13_Retry(Base):
    """13. A failed deletion is retried automatically until it succeeds."""

    def test_point_13_a_failed_deletion_is_retried_until_it_succeeds(self):
        aid, npath = self.spawn("zach-opus-rt")
        self.commit(npath)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        parent = os.path.dirname(npath)
        os.chmod(parent, 0o555)                              # a disk that refuses the delete
        try:
            self.assertEqual(self.names(), [])               # landed: it does not block Rich
            r = self.rec("zach-opus-rt")
            self.assertEqual(r["disposition"]["kind"], "landed")
            self.assertEqual(r["deletion"]["attempts"], 1)
            self.assertTrue(os.path.exists(npath))
            for _ in range(ws.RETRY_TELL_CEO_AFTER):
                ws.retry_due()
            self.assertTrue(ws.keeps_failing())               # only now does the CEO hear of it
            ok, msg, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
            self.assertIn("TELL THE CEO", msg)
        finally:
            os.chmod(parent, 0o755)
        ws.retry_due()
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertIsNone(ws.load_agent(ws.named_key(self.sid, "zach-opus-rt"))["deletion"])


class Point14_IntegrationBranch(Base):
    """14. Landed doesn't always mean landed on main."""

    def dev_branch(self, repo, name="dev/work", why="the workspace-spec round"):
        """A body of work whose dev branch is RECORDED before its first agent,
        exactly as the page requires: the record happens first, and nothing
        afterwards infers or re-derives it."""
        run("git", "-C", repo, "branch", name)
        return ws.record_integration(repo, name, why, self.sid)

    def ff(self, repo, onto, branch):
        """Merge `branch` onto `onto` without checking it out: a fast-forward,
        which is what a merge of an agent's branch onto its dev branch is when
        the dev branch has not moved. The main checkout stays on main throughout,
        which is the whole point -- main never learns of this work."""
        tip = run("git", "-C", repo, "rev-parse", branch).stdout.strip()
        run("git", "-C", repo, "branch", "-f", onto, tip)

    def test_point_14_work_merged_onto_its_dev_branch_counts_as_landed(self):
        """"Landing means merged into the branch this work integrates on ... When
        the work cannot reach main yet ... it is that work's dev branch. Rich
        merges each finished agent's work onto it and deletes that agent's
        workspaces and branches under point 4, exactly as in any other land."

        Before this, land() resolved the target with `git rev-parse HEAD` in the
        main checkout, so work merged onto a dev branch was reported NOT LANDED
        for as long as the dev branch stayed out of main -- and its workspaces
        and branches sat there, against point 4."""
        self.dev_branch(self.entity)
        self.dev_branch(self.other, "dev/other-work")
        cc = self.make_cc("zach-opus-i1")
        aid, npath = self.spawn("zach-opus-i1", cc=cc)
        self.commit(npath, "native.txt")
        self.commit(cc, "cc.txt")
        self.finish(aid)
        self.ff(self.entity, "dev/work", "worktree-agent-" + aid)
        self.ff(self.other, "dev/other-work", "cc/zach-opus-i1")
        # main has none of it, in either repository, and that is not a reason to
        # leave anything behind
        main_log = run("git", "-C", self.entity, "log", "--format=%s", "main").stdout
        self.assertNotIn("work native.txt", main_log)
        self.assertEqual(self.names(), [])                   # landed automatically (point 4)
        self.assertTrue(os.path.exists(ws.done_path(ws.named_key(self.sid, "zach-opus-i1"))))
        self.assertFalse(os.path.exists(npath))
        self.assertFalse(os.path.exists(cc))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertNotIn("cc/zach-opus-i1", branches(self.other))

    def test_point_14_work_merged_nowhere_is_not_landed(self):
        """The other half, and the reason the record is a branch rather than a
        waiver: work that reached NEITHER main nor its dev branch is not landed,
        it is pending (point 5), and its workspace and branch stay until Rich
        merges or discards it (point 7)."""
        self.dev_branch(self.entity)
        aid, npath = self.spawn("zach-opus-i2")
        self.commit(npath, "unmerged.txt")
        self.finish(aid)
        self.assertEqual(self.names(), ["zach-opus-i2"])     # not landed by itself
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-i2", self.sid)
        self.assertIn("dev/work", str(e.exception))          # it names the recorded target
        self.assertTrue(os.path.exists(npath))
        self.assertIn("worktree-agent-" + aid, branches(self.entity))
        # and merging it onto the recorded branch is what lands it
        self.ff(self.entity, "dev/work", "worktree-agent-" + aid)
        ws.land("zach-opus-i2", self.sid)
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))

    def test_point_14_two_of_one_agents_own_calls_open_at_once_keep_both_windows(self):
        """Points 3 and 10 through point 14's target: "any branch an agent
        created" ... "None is left behind." An agent's own calls OVERLAP. With
        one snapshot slot per agent the second window was lost, and a side branch
        created in it took real commits out of every integration branch while
        land() still reported LANDED. The window is keyed by the CALL, so
        Pre(A) Pre(B) Post(A) Post(B) attributes both."""
        aid, npath = self.spawn("zach-opus-i5")
        self.tool_call(aid, call="tu-warm")
        self.commit(npath, "own.txt")
        self.pre(aid, call="tu-A")                       # call A opens
        self.pre(aid, call="tu-B")                       # call B opens alongside it
        run("git", "-C", npath, "branch", "spare")       # created inside call A
        self.post(aid, call="tu-A")                      # A closes: its own window
        run("git", "-C", npath, "checkout", "-q", "-b", "tmpwork")
        self.commit(npath, "side.txt")                   # real work, inside call B
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.post(aid, call="tu-B")                      # B closes: its own window
        self.assertEqual(self.created("zach-opus-i5"), ["spare", "tmpwork"])
        # A call whose PostToolUse never arrives (the agent's run ends inside it)
        # still has its window: the end-of-run signal is the LAST observation and
        # it consumes every window still open, not one of them.
        self.pre(aid, call="tu-C")
        run("git", "-C", npath, "branch", "never-closed")
        self.finish(aid)
        self.assertEqual(self.created("zach-opus-i5"), ["never-closed", "spare", "tmpwork"])
        self.merge(self.entity, "worktree-agent-" + aid)
        # tmpwork carries a commit that reached no integration branch, so the
        # land is REFUSED and the work is pending rather than silently lost.
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-i5", self.sid)
        self.assertIn("tmpwork", str(e.exception))
        self.assertEqual(self.names(), ["zach-opus-i5"])
        self.assertIn("tmpwork", branches(self.entity))
        self.merge(self.entity, "tmpwork")
        ws.land("zach-opus-i5", self.sid)
        self.assertNotIn("tmpwork", branches(self.entity))   # point 10: none left behind
        self.assertNotIn("spare", branches(self.entity))
        self.assertNotIn("never-closed", branches(self.entity))

    def test_point_03_a_refused_call_never_widens_the_window_to_the_whole_run(self):
        """Points 3 and 8. A PreToolUse another guard REFUSES leaves a window no
        PostToolUse will ever consume. The end-of-run pass consumed every open
        window and INTERSECTED them, so one leak at minute one turned the last
        comparison of the run into "everything that appeared while this agent
        existed" — and a branch RICH cut between two of the agent's calls came
        back as the agent's and was destroyed by its discard, against point 8's
        "Deletion therefore never loses anything that was meant to land." """
        ws.record_integration(self.entity, "main", "this body of work", self.sid)
        aid, npath = self.spawn("zach-opus-w1")
        self.pre(aid, call="tu-refused")          # refused: no Post will ever come
        self.tool_call(aid, call="tu-1")
        self.commit(npath, "own.txt")
        self.tool_call(aid, call="tu-2")
        tip = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        # BETWEEN tool calls, at the agent's own tip: Rich's, not the agent's.
        run("git", "-C", self.entity, "branch", "rich/rescue", tip)
        self.tool_call(aid, call="tu-3")
        self.finish(aid)                          # consumes every window still open
        self.assertEqual(self.created("zach-opus-w1"), [])
        ws.discard("zach-opus-w1", "the reviewer rejected the approach",
                   not_ceo_ordered="a test of attribution", me=self.sid)
        self.assertIn("rich/rescue", branches(self.entity))

    def test_point_08_a_branch_that_existed_before_the_call_is_never_created_in_it(self):
        """Point 8, in the destructive direction: "Deletion therefore never
        loses anything that was meant to land."

        This is the case where the SNAPSHOT is the only thing standing between
        somebody else's branch and `git branch -D`. Rich cuts a bookmark at the
        agent's own unlanded tip BEFORE the call opens, so every other filter
        waves it through: it is not the main checkout's branch, not on any other
        agent's record, not the agent's own workspace branch, not already in the
        integration branch, and it sits exactly on the agent's line of work.
        Only "it was already there when this call started" keeps it out.

        It is written down because an empty before-set was measured killing NO
        shipped test, which made the first half of the pair an unproven claim."""
        aid, npath = self.spawn("zach-opus-w2")
        self.tool_call(aid, call="tu-warm")
        self.commit(npath, "own.txt")
        tip = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        run("git", "-C", self.entity, "branch", "rich/bookmark", tip)   # BEFORE the call
        self.pre(aid, call="tu-1")
        run("git", "-C", npath, "branch", "spare")                      # inside the call
        self.post(aid, call="tu-1")
        self.assertEqual(self.created("zach-opus-w2"), ["spare"])
        self.finish(aid)
        ws.discard("zach-opus-w2", "the reviewer rejected the approach",
                   not_ceo_ordered="a test of attribution", me=self.sid)
        self.assertIn("rich/bookmark", branches(self.entity))
        self.assertNotIn("spare", branches(self.entity))

    def test_point_03_a_post_that_carries_no_call_id_still_ends_a_call(self):
        """Point 3. The platform does not always put `tool_use_id` on both
        halves. A Pre keyed / Post unkeyed pair used to find nothing at either
        end, so the ref created inside that call was attributed to nobody until
        the end of the run and, once the run had ended, to nobody at all."""
        ws.record_integration(self.entity, "main", "this body of work", self.sid)
        aid, npath = self.spawn("zach-opus-w3")
        self.tool_call(aid, call="tu-warm")
        self.commit(npath, "own.txt")
        self.pre(aid, call="tu-A")                           # Pre KEYED
        run("git", "-C", npath, "branch", "keyed-pre-only")
        self.post(aid)                                       # Post UNKEYED
        self.pre(aid)                                        # Pre UNKEYED
        run("git", "-C", npath, "branch", "unkeyed-pre-only")
        self.post(aid, call="tu-B")                          # Post KEYED
        self.assertEqual(self.created("zach-opus-w3"),
                         ["keyed-pre-only", "unkeyed-pre-only"])

    def test_point_14_attribution_never_waits_for_the_record(self):
        """Points 5, 8 and 10 through point 14. Attribution happens once, at the
        end of a tool call. While no integration branch was recorded it did not
        happen AT ALL: the refs were written to an `attribution-skipped` event
        nothing read, recording the branch afterwards unblocked the land and
        never went back for them, and land() then reported success over a commit
        that had reached no integration branch."""
        # THE NO-TARGET CASE, since round 7: a spawn with no record is refused
        # (point 14), so the way an agent's target goes missing is the recorded
        # branch ceasing to exist after the spawn. Attribution must still happen.
        run("git", "-C", self.entity, "branch", "dev/next")
        ws.record_integration(self.entity, "dev/next", "this body of work", self.sid)
        aid, npath = self.spawn("zach-opus-w4")
        run("git", "-C", self.entity, "branch", "-D", "dev/next")   # the target is gone
        self.assertTrue(ws.integration_target([self.rec("zach-opus-w4")], self.entity)[2])
        self.pre(aid, call="tu-1")
        run("git", "-C", npath, "checkout", "-q", "-b", "sidework")
        self.commit(npath, "side.txt")                       # real work, no target
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        run("git", "-C", npath, "branch", "spare")
        self.post(aid, call="tu-1")
        self.assertEqual(self.created("zach-opus-w4"), ["sidework", "spare"])
        self.commit(npath, "own.txt")
        self.finish(aid)
        # Rich corrects the branch afterwards and merges the agent's own branch.
        ws.record_integration(self.entity, "main", "this body of work", self.sid, correct=True)
        self.merge(self.entity, "worktree-agent-" + aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-w4", self.sid)                # the stranded commit is named
        self.assertIn("sidework", str(e.exception))
        self.assertEqual(self.names(), ["zach-opus-w4"])
        self.merge(self.entity, "sidework")
        ws.land("zach-opus-w4", self.sid)
        self.assertNotIn("sidework", branches(self.entity))  # point 10: none left behind
        self.assertNotIn("spare", branches(self.entity))

    def test_point_14_a_tip_the_agent_only_borrowed_is_not_its_work(self):
        """Points 3 and 8. With no record there is no integration branch to
        subtract, so the floor is each workspace's OWN starting commit and the
        only commits that count are the ones it MADE. Without both, the base
        commit counts as the agent's work, every ref descends from it, and a ref
        Rich cuts at a commit the agent merely checked out becomes the agent's
        — and is deleted with it."""
        run("git", "-C", self.entity, "worktree", "add", "-q",
            os.path.join(self.env.root, "human-wt"), "-b", "human/keep")
        self.commit(os.path.join(self.env.root, "human-wt"), "human.txt")
        tip = run("git", "-C", self.entity, "rev-parse", "human/keep").stdout.strip()
        run("git", "-C", self.entity, "worktree", "remove", "--force",
            os.path.join(self.env.root, "human-wt"))
        # the no-target case (see test_point_14_attribution_never_waits_for_the_record)
        run("git", "-C", self.entity, "branch", "dev/next")
        ws.record_integration(self.entity, "dev/next", "this body of work", self.sid)
        aid, npath = self.spawn("zach-opus-w5")
        run("git", "-C", self.entity, "branch", "-D", "dev/next")
        self.pre(aid, call="tu-1")
        run("git", "-C", npath, "checkout", "-q", "human/keep")     # BORROWED, never made
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.post(aid, call="tu-1")
        self.pre(aid, call="tu-2")
        run("git", "-C", self.entity, "branch", "rich/backup", tip)  # Rich, at that tip
        self.post(aid, call="tu-2")
        self.assertEqual(self.created("zach-opus-w5"), [])

    def test_point_14_the_integration_branch_is_recorded_never_inferred(self):
        """"The branch a body of work integrates on is RECORDED when that work
        starts, before its first agent is spawned. NOTHING INFERS IT AND NOTHING
        GUESSES IT." So: a land with no record REFUSES and names the command;
        nothing is derived from what the main checkout happens to be on; nothing
        is frozen onto an agent; and the live record is the only answer, which is
        why recording it late REACHES an agent that was spawned before it."""
        self.assertEqual(ws.integration_record(self.entity)["branch"], "main")
        self.assertEqual(ws.integration_record(self.entity)["source"], "recorded")
        # NO RECORD AT ALL: THE SPAWN IS REFUSED, naming the command — "RECORDED
        # when that work starts, BEFORE its first agent is spawned." Round 6's
        # reading ("the land refuses later, and heals") was ruled a guess by
        # both reviewers: an agent bound to nothing was judged at land time
        # against whichever body of work was current, so a second recording in
        # between moved its verdict. Nothing is derived from what the main
        # checkout happens to be on, and a session start infers nothing either.
        os.unlink(os.path.join(ws.state_dir(), "integration.json"))
        ws.record_session_start(self.sid, self.entity)          # infers nothing (the old floor fired here)
        self.assertIsNone(ws.integration_record(self.entity))
        with self.assertRaises(ws.SpecError) as e0:
            self.spawn("zach-opus-i3")
        self.assertIn("workspaces.sh integration", str(e0.exception))
        self.assertIn("point 14", str(e0.exception))
        self.assertIsNone(self.rec("zach-opus-i3"))             # the spawn did not happen (point 3)
        self.assertIsNone(ws.integration_record(self.entity))   # and the refusal recorded nothing
        with self.assertRaises(ws.SpecError) as e1:             # the workspace half is refused too
            self.make_cc("zach-opus-i3")
        self.assertIn("workspaces.sh integration", str(e1.exception))
        self.assertIsNone(self.rec("zach-opus-i3"))
        # RICH RECORDS THE BRANCH, AND THE SAME SPAWN SUCCEEDS — bound, by id, to
        # the body of work that was current AT ITS SPAWN, and to nothing else.
        run("git", "-C", self.entity, "branch", "dev/next")
        first = ws.record_integration(self.entity, "dev/next", "this body of work", self.sid)
        aid, npath = self.spawn("zach-opus-i3")
        self.assertEqual(self.rec("zach-opus-i3")["integration_work"][self.entity], first["id"])
        self.assertNotIn("integration", self.rec("zach-opus-i3"))   # no branch frozen onto it
        self.commit(npath, "i3.txt")
        self.finish(aid)
        # A LATER, UNRELATED RECORDING DOES NOT MOVE IT (the reviewers' S1–S4):
        # a second body of work starts in the same repository, and this agent
        # still lands against dev/next.
        run("git", "-C", self.entity, "branch", "dev/other")
        ws.record_integration(self.entity, "dev/other", "an unrelated body of work", self.sid)
        self.merge(self.entity, "worktree-agent-" + aid)         # onto main: not its branch
        with self.assertRaises(ws.SpecError) as e2:
            ws.land("zach-opus-i3", self.sid)
        self.assertIn("dev/next", str(e2.exception))
        self.ff(self.entity, "dev/next", "worktree-agent-" + aid)
        ws.land("zach-opus-i3", self.sid)
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        # an agent's own workspace branch is never an integration branch
        with self.assertRaises(ws.SpecError):
            ws.record_integration(self.entity, "cc/zach-opus-i3", "wrong kind of branch", self.sid)
        # A CORRECTION REACHES AN AGENT IN FLIGHT. Nothing is frozen onto the
        # agent at its registration: it is bound to the BODY OF WORK by id, and
        # that work's branch is read live, so correcting the branch corrects
        # every agent bound to it. That is what a derived or frozen answer could
        # never do, and it is why the binding is an id and not a branch name.
        ws.record_integration(self.entity, "main", "back to main for a moment", self.sid)
        aid2, npath2 = self.spawn("zach-opus-i4")
        self.commit(npath2, "i4.txt")
        run("git", "-C", self.entity, "branch", "dev/later")
        ws.record_integration(self.entity, "dev/later", "it was the wrong branch",
                              self.sid, correct=True)
        self.finish(aid2)
        self.merge(self.entity, "worktree-agent-" + aid2)     # onto main: the OLD value
        with self.assertRaises(ws.SpecError) as e2:
            ws.land("zach-opus-i4", self.sid)
        self.assertIn("dev/later", str(e2.exception))
        self.assertTrue(os.path.exists(npath2))
        self.ff(self.entity, "dev/later", "worktree-agent-" + aid2)
        ws.land("zach-opus-i4", self.sid)
        self.assertFalse(os.path.exists(npath2))

    def test_point_14_never_refuses_a_path_that_is_not_a_repository(self):
        """A REFUSAL WHOSE REMEDY CANNOT BE RUN IS NOT A FLOOR. Point 14 records
        A BRANCH; a directory that is not a git repository has none, has no land
        to measure, and `record_integration` refuses it outright. So the spawn
        gate must not demand a record there: until it stopped, an entity root
        that was not a repository was refused every spawn forever, and the one
        command its refusal named refused in turn —

            spawn  exit 2  "no branch is recorded ... Record it, then spawn:
                            workspaces.sh integration --repo <dir> --branch ..."
            remedy exit 2  "the repository <dir> could not be resolved from git"

        — which is the shape this file deleted the first-registration floor for:
        "with NO record at all the refusal HEALS". This one could not. Nothing
        about a real repository is relaxed: the sibling assertion below is the
        control, and every other point-14 case still refuses."""
        plain = os.path.join(self.env.root, "not-a-repo")
        os.makedirs(plain)
        self.assertEqual(ws.main_checkout(plain), "")             # git resolves nothing here
        with self.assertRaises(ws.SpecError) as e:                # so the remedy cannot be run
            ws.record_integration(plain, "main", "impossible", self.sid)
        self.assertIn("could not be resolved from git", str(e.exception))
        payload = {"session_id": self.sid, "tool_use_id": "tu-plain", "tool_name": "Agent",
                   "tool_input": {"name": "zach-opus-plain1", "subagent_type": "zach",
                                  "prompt": "do it", "isolation": "worktree"}}
        ws.register_spawn(payload, plain)                         # allowed: nothing to record
        self.assertIsNotNone(self.rec("zach-opus-plain1"))
        self.assertEqual(self.rec("zach-opus-plain1").get("integration_work", {}), {})
        # THE CONTROL: a real repository with its record removed is still refused,
        # and still named, so this is an exemption for non-repositories only.
        os.unlink(os.path.join(ws.state_dir(), "integration.json"))
        with self.assertRaises(ws.SpecError) as e2:
            self.spawn("zach-opus-plain2")
        self.assertIn("workspaces.sh integration", str(e2.exception))
        self.assertIn("point 14", str(e2.exception))

    def test_point_14_a_record_bound_to_nothing_is_refused_never_guessed(self):
        """Point 14: "Nothing infers it and nothing guesses it." The one kind of
        record that is never spawned through the guard — a workspace the
        point-3 sweep found unregistered — can exist while nothing is recorded.
        Its land REFUSES and names the command; it never reads whichever body
        of work happens to be current at land time (round 6's "heals later",
        which both reviewers reproduced moving a land verdict). It binds at the
        first sweep after a body of work exists, by id, once, and then lands."""
        os.unlink(os.path.join(ws.state_dir(), "integration.json"))
        raw = os.path.join(self.env.root, "raw-cc")
        run("git", "-C", self.entity, "worktree", "add", "-q", raw, "-b", "cc/raw-hand")
        self.commit(raw, "raw.txt")
        self.assertEqual(self.names(), ["orphan-raw-cc"])            # found, unregistered
        orphan = [r for r in ws.all_agents() if r.get("name") == "orphan-raw-cc"][0]
        self.assertNotIn("integration_work", orphan)                 # bound to nothing
        with self.assertRaises(ws.SpecError) as e:
            ws.land("orphan-raw-cc", self.sid)
        self.assertIn("bound to no body of work", str(e.exception))
        self.assertIn("workspaces.sh integration", str(e.exception))
        # Rich records main, and merges the branch. WITHOUT A SWEEP the record is
        # still bound to nothing, and the land still refuses: it does not read
        # "current" now that a current one exists — that would be the guess.
        ws.record_integration(self.entity, "main", "this body of work", self.sid)
        self.merge(self.entity, "cc/raw-hand")
        with self.assertRaises(ws.SpecError) as e2:
            ws.land("orphan-raw-cc", self.sid)
        self.assertIn("bound to no body of work", str(e2.exception))
        self.assertTrue(os.path.exists(raw))
        # The next sweep binds it — recorded on it by id — and, being merged,
        # it lands on its own (point 4).
        self.assertEqual(self.names(), [])
        self.assertFalse(os.path.exists(raw))
        self.assertNotIn("cc/raw-hand", branches(self.entity))
        done = ws.load_agent(orphan["key"])
        self.assertEqual(done["disposition"]["kind"], "landed")
        self.assertEqual(done["integration_work"][self.entity], ws.integration_record(self.entity)["id"])

    def test_point_14_a_recorded_branch_moved_in_an_agents_call_is_reported_and_left_alone(self):
        """Round 8, item 2, as amended on 2026-09-14. The recorded branch moved
        by an UNNAMED verb — the checkout doorway, then a commit — during an
        agent's call is REPORTED at the call's PostToolUse and the ref is left
        exactly where it is: the engine never moves a protected ref back, since
        the shape of the result cannot tell an agent's doorway from Rich's own
        land, and the write it used to make moved refs/heads/main in richos
        three times in one night (docs/verification/
        ref-write-forensics-2026-09-14.md). A DELETION is still put back, with a
        create-only write that carries a reflog message. The lead's legitimate
        move (a descendant carrying none of the agent's work) is not even
        reported."""
        run("git", "-C", self.entity, "branch", "dev/rec")
        ws.record_integration(self.entity, "dev/rec", "body of work on a dev branch", self.sid)
        tip = run("git", "-C", self.entity, "rev-parse", "dev/rec").stdout.strip()
        aid, npath = self.spawn("zach-opus-mv")
        self.commit(npath, "mv.txt")
        self.pre(aid, "tu-1")
        run("git", "-C", npath, "checkout", "-q", "dev/rec")                  # the doorway
        run("git", "-C", npath, "commit", "-q", "--allow-empty", "-m", "moved unnamed")
        moved = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        self.post(aid, call="tu-1")
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "dev/rec").stdout.strip(), moved)
        hist = [h for h in self.rec("zach-opus-mv").get("history") or []
                if h.get("fact") == "protected ref moved (reported, not restored)"]
        self.assertEqual([(h["branch"], h["tip"], h["found"]) for h in hist], [("dev/rec", tip, moved)])
        # The deleted case: re-created at the tip the snapshot recorded, and the
        # write says who made it — an anonymous ref write is what cost a day.
        run("git", "-C", self.entity, "branch", "-f", "dev/rec", tip)
        self.pre(aid, "tu-2")
        run("git", "-C", self.entity, "update-ref", "-d", "refs/heads/dev/rec")   # deleted, from the main checkout
        self.post(aid, call="tu-2")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "dev/rec").stdout.strip(), tip)
        self.assertIn("richos engine: protected ref restored",
                      run("git", "-C", self.entity, "reflog", "show", "dev/rec").stdout)
        self.pre(aid, "tu-3")
        lead = run("git", "-C", self.entity, "commit-tree", tip + "^{tree}", "-p", tip, "-m",
                   "the lead lands a finished agent's work").stdout.strip()
        run("git", "-C", self.entity, "branch", "-f", "dev/rec", lead)              # the lead's move: a descendant, not this agent's work
        self.post(aid, call="tu-3")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "dev/rec").stdout.strip(), lead)
        # And his land is not even reported: exactly two protected-ref facts on
        # the record, the move and the deletion, in that order.
        self.assertEqual([h["fact"] for h in self.rec("zach-opus-mv").get("history") or []
                          if str(h.get("fact", "")).startswith("protected ref")],
                         ["protected ref moved (reported, not restored)", "protected ref restored"])

    def test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not(self):
        """THE SECOND HALF OF THE LAND LOCK (2026-09-17). The lock makes two
        conversations' lands take turns; this is how the thread that WAITED
        finds out. Its land moved the recorded branch under the other thread's
        running agents, and until now that was the silent case — a descendant
        carrying none of the agent's work, indistinguishable by shape from the
        lead's own land.

        It is not inferred from the shape here either. `integrate` appends a
        land record naming the conversation that landed, and the check reads
        it, so each case below is decided by the RECORD and never by what the
        move looks like:

          A  no land records for this repository  -> silent. The terminal path:
             Rich's hand-run `git merge` writes no record, and reporting every
             one of those would put an alarm on the most ordinary write the
             branch ever receives. THE CONTROL FOR EVERY CASE BELOW.
          B  a record naming ANOTHER thread       -> reported, with the thread,
             the time and both tips.
          C  a record naming THIS thread          -> silent. Its own agents
             already know, and this is the positive control that B is not
             simply "any record fires".
          D  records in use, this move in none    -> SILENT, like A. A repository
             that keeps land records is still merged into by a hand at a
             terminal, and that hand writes no record; reporting it once per
             running agent per merge was the defect (hunt part 4, finding 24).
          E  two lands in a row                   -> both are answered, which is
             the whole reason the record is APPENDED rather than kept as a
             field on the lock that the next lander overwrites.
        """
        run("git", "-C", self.entity, "branch", "dev/rec")
        ws.record_integration(self.entity, "dev/rec", "body of work on a dev branch", self.sid)
        aid, npath = self.spawn("zach-opus-land")
        self.commit(npath, "land.txt")

        def tip_now():
            return run("git", "-C", self.entity, "rev-parse", "dev/rec").stdout.strip()

        def a_land(message):
            """A descendant of dev/rec carrying none of the agent's work: the
            exact shape of a land, made without one."""
            parent = tip_now()
            child = run("git", "-C", self.entity, "commit-tree", parent + "^{tree}",
                        "-p", parent, "-m", message).stdout.strip()
            return parent, child

        def move(call, child):
            run("git", "-C", self.entity, "branch", "-f", "dev/rec", child)
            self.post(aid, call=call)

        def told():
            return [(h["tip"], h["found"], h["why"]) for h in self.rec("zach-opus-land").get("history") or []
                    if h.get("fact") == "another conversation landed on this branch"]

        def record(branch, before, commit, thread, at):
            ws.append_land_record(self.entity, {"schema": 1, "branch": branch, "before": before,
                                                "commit": commit, "at": at, "thread_id": thread,
                                                "entity_id": "depot", "session_id": "sess-" + thread})

        # --- A. NO LAND RECORDS: SILENT, AND THE FILE REALLY IS ABSENT ------
        self.assertFalse(os.path.exists(ws.land_record_path(self.entity)))
        self.pre(aid, "tu-a")
        before_a, child_a = a_land("the lead lands, by hand, at a terminal")
        move("tu-a", child_a)
        self.assertEqual(tip_now(), child_a)
        self.assertEqual(told(), [])

        # --- B. ANOTHER CONVERSATION'S LAND: REPORTED, AND IT NAMES IT ------
        self.pre(aid, "tu-b")
        before_b, child_b = a_land("thread-b lands its own reviewed work")
        record("dev/rec", before_b, child_b, "thread-b", "2026-09-17T19:00:00Z")
        with patch.dict(os.environ, {"RICHOS_APP_THREAD": "thread-a"}):
            move("tu-b", child_b)
        self.assertEqual([(t, f) for t, f, _w in told()], [(before_b, child_b)])
        why = told()[0][2]
        self.assertIn("thread-b", why)
        self.assertIn("2026-09-17T19:00:00Z", why)
        self.assertIn(before_b[:12], why)
        self.assertIn(child_b[:12], why)
        # AND NOTHING WAS PUT BACK. A report is not a write; the branch is where
        # the other conversation left it.
        self.assertEqual(tip_now(), child_b)

        # --- C. THIS CONVERSATION'S OWN LAND: SILENT ------------------------
        self.pre(aid, "tu-c")
        before_c, child_c = a_land("thread-a lands its own reviewed work")
        record("dev/rec", before_c, child_c, "thread-a", "2026-09-17T19:05:00Z")
        with patch.dict(os.environ, {"RICHOS_APP_THREAD": "thread-a"}):
            move("tu-c", child_c)
        self.assertEqual(len(told()), 1)                  # still only B's

        # --- D. RECORDS IN USE, AND THIS MOVE IN NONE: SILENT ----------------
        self.pre(aid, "tu-d")
        before_d, child_d = a_land("a hand at a terminal, in a repository that keeps records")
        with patch.dict(os.environ, {"RICHOS_APP_THREAD": "thread-a"}):
            move("tu-d", child_d)
        # A HAND MERGE IS SILENT WHETHER OR NOT OLDER RECORDS EXIST (hunt part 4,
        # finding 24). It used to be reported again and again as "moved forward
        # by a land this repository's land records do not name" the moment the
        # first land was recorded. Case A is the control that this is the same
        # terminal path, and B above is the control that a NAMED land still speaks.
        self.assertEqual(len(told()), 1)                  # still only B's
        self.assertIsNone(ws.land_by_another_conversation(self.entity, "dev/rec", before_d, child_d))
        self.assertEqual(tip_now(), child_d)              # and nothing was put back

        # --- E. TWO LANDS IN A ROW, BOTH STILL ANSWERABLE -------------------
        # The record is append-only, so the FIRST of the two is not erased by
        # the second. One window spans both: the agent's snapshot predates them
        # both and it is told about the tip it actually found.
        before_e1, child_e1 = a_land("thread-b lands again")
        record("dev/rec", before_e1, child_e1, "thread-b", "2026-09-17T19:10:00Z")
        run("git", "-C", self.entity, "branch", "-f", "dev/rec", child_e1)
        self.pre(aid, "tu-e")
        before_e2, child_e2 = a_land("thread-c lands straight after")
        record("dev/rec", before_e2, child_e2, "thread-c", "2026-09-17T19:11:00Z")
        with patch.dict(os.environ, {"RICHOS_APP_THREAD": "thread-a"}):
            move("tu-e", child_e2)
        self.assertEqual(len(told()), 2)
        self.assertIn("thread-c", told()[1][2])
        # BOTH LANDS ARE STILL ON THE RECORD — the earlier one was not
        # overwritten, and it still answers for the move it made.
        kept = [(r["thread_id"], r["commit"]) for r in ws.land_records(self.entity)]
        self.assertIn(("thread-b", child_e1), kept)
        self.assertIn(("thread-c", child_e2), kept)
        self.assertEqual(ws.land_by_another_conversation(self.entity, "dev/rec", before_e1, child_e1),
                         "landed by conversation thread-b at 2026-09-17T19:10:00Z, %s -> %s"
                         % (before_e1[:12], child_e1[:12]))

        # --- F. AFTER THE AGENT'S LAST CALL, WITH NO WINDOW OPEN ------------
        # The end-of-run comparison is made against the last snapshot, which can
        # be minutes old, and every descendant there is silent for the same
        # reason case A is: it is the shape of the lead's land. A RECORD is not
        # a shape, so this one is answered too — on the agent's durable record,
        # which outlives the run that could no longer be told anything.
        before_f, child_f = a_land("thread-b lands after the agent's last call")
        record("dev/rec", before_f, child_f, "thread-b", "2026-09-17T19:20:00Z")
        run("git", "-C", self.entity, "branch", "-f", "dev/rec", child_f)
        with patch.dict(os.environ, {"RICHOS_APP_THREAD": "thread-a"}):
            self.finish(aid)
        self.assertEqual(len(told()), 3)
        self.assertIn("thread-b", told()[2][2])
        self.assertIn("2026-09-17T19:20:00Z", told()[2][2])

    def test_point_14_the_engine_never_moves_the_recorded_branch_when_two_agents_run(self):
        """THE 2026-09-14 INCIDENT, as a test. Events 3, 4 and 5 of
        docs/verification/ref-write-forensics-2026-09-14.md: the engine moved
        refs/heads/main three times, twice within twelve seconds in opposite
        directions, while Rich was landing. Nothing was lost only because the
        second write happened to go forwards.

        The condition, which needs no attacker and no unusual verb — it is the
        NORMAL case of two agents running while the lead lands:

          A opens a call while main is at T0;
          Rich merges A's finished work, main -> T1;
          B opens a call while main is at T1;
          A's Post fires, then B's.

        A's window says T0, B's says T1, so under the old rule A "restored" main
        to T0 (dropping Rich's merge) and B "restored" it to T1 — each write
        manufacturing the condition the next one fired on. Now: two reports, no
        writes, main exactly where Rich left it, and B has nothing to report at
        all because nobody moved anything."""
        a_id, a_path = self.spawn("zach-opus-os1", agent_id="aosc000000000001")
        b_id, b_path = self.spawn("zach-opus-os2", agent_id="bosc000000000002")
        self.commit(a_path, "a.txt")
        self.commit(b_path, "b.txt")
        t0 = run("git", "-C", self.entity, "rev-parse", "main").stdout.strip()
        self.pre(a_id, "tu-a")                                   # A's window: main at T0
        self.merge(self.entity, "worktree-agent-" + a_id)        # Rich lands A's work
        t1 = run("git", "-C", self.entity, "rev-parse", "main").stdout.strip()
        self.assertNotEqual(t0, t1)
        self.pre(b_id, "tu-b")                                   # B's window: main at T1
        self.post(a_id, call="tu-a")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "main").stdout.strip(), t1)
        self.post(b_id, call="tu-b")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "main").stdout.strip(), t1)
        # Rich's merge is still reachable, and no ref write went in without a
        # reflog message — the signature of the three real events.
        self.assertEqual(run("git", "-C", self.entity, "merge-base", "--is-ancestor", t1, "main",
                             check=False).returncode, 0)
        reflog = run("git", "-C", self.entity, "reflog", "show", "main").stdout.splitlines()
        self.assertEqual([ln for ln in reflog if ln.rstrip().endswith(":")], [])
        # Reported, not silent: A saw it, B had nothing to see.
        self.assertEqual([h["branch"] for h in self.rec("zach-opus-os1").get("history") or []
                          if h.get("fact") == "protected ref moved (reported, not restored)"], ["main"])
        self.assertEqual([h for h in self.rec("zach-opus-os2").get("history") or []
                          if str(h.get("fact", "")).startswith("protected ref")], [])

    def test_point_14_a_recorded_branch_is_protected_in_its_own_repository_only(self):
        """§4 of the forensics: the protected set was keyed by branch NAME across
        every body of work, so `main` recorded for one repository protected —
        and made restorable — `refs/heads/main` in every other. Three bodies of
        work on the operator's machine, all three integrating on `main`.

        Keyed by (repository, branch), the tips an agent's window records for a
        repository are that repository's own protected refs."""
        # The same branch NAME exists in both repositories; it is the recorded
        # integration branch of one body of work, in one of them.
        run("git", "-C", self.entity, "branch", "dev/shared-name")
        run("git", "-C", self.other, "branch", "dev/shared-name")
        ws.record_integration(self.entity, "dev/shared-name", "the entity's dev branch", self.sid)
        other_refs = ws._local_refs(self.other)
        self.assertIn("dev/shared-name", other_refs)
        self.assertNotIn("dev/shared-name", ws._protected_tips(self.other, other_refs))
        self.assertIn("dev/shared-name", ws._protected_tips(self.entity, ws._local_refs(self.entity)))
        # Each repository keeps its own: both recorded `main`, and both are
        # protected where they were recorded.
        self.assertIn("main", ws._protected_tips(self.other, other_refs))

    def test_point_14_the_leads_land_after_the_agents_last_call_is_not_undone(self):
        """Round 8, item 2 — the bound measured on Sage's runner-round case R8:
        with NO call open, Rich fast-forwards the recorded branch to the agent's
        OWN tip (his land of its work, made after its last call and before its
        end signal). The end-of-run observation compares against the last
        snapshot, which is minutes old; "a descendant carrying the agent's own
        work" is the agent's doorway only INSIDE a window, so outside one it is
        left alone and the land proceeds."""
        aid, npath = self.spawn("zach-opus-ff")
        self.tool_call(aid, "tu-1")
        self.commit(npath, "ff.txt")
        self.tool_call(aid, "tu-2")                            # the last call closes
        self.merge(self.entity, "worktree-agent-" + aid)       # the lead's fast-forward of main to the agent's tip
        tip = run("git", "-C", self.entity, "rev-parse", "main").stdout.strip()
        self.finish(aid)                                       # the end signal: no window open
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "main").stdout.strip(), tip)
        # Nor is it REPORTED. Since 2026-09-14 nothing here writes a ref, so the
        # whole value of the windowed rule is that the check stays silent about
        # the lead's ordinary land: a report that fires on every land is alarm
        # fatigue, and this check only earns attention by being rare.
        self.assertEqual([h for h in self.rec("zach-opus-ff").get("history") or []
                          if str(h.get("fact", "")).startswith("protected ref")], [])
        ws.land("zach-opus-ff", self.sid)
        self.assertFalse(os.path.exists(npath))

    def test_point_02_a_codex_ref_deleted_in_an_agents_call_is_restored_and_a_move_is_reported(self):
        """Round 8, item 3, as amended on 2026-09-14: a codex/ ref DELETED during
        an agent's call is put back — point 2 says a codex/ branch is never
        deleted without the CEO's express word, the deletion is unambiguous, and
        re-creating it at the tip it held loses nothing. A MOVE is reported and
        left alone: the engine stopped moving protected refs after its own
        "restores" moved refs/heads/main three times in richos
        (docs/verification/ref-write-forensics-2026-09-14.md), and a move cannot
        be put back without deciding whose write it was.

        And a registered agent's Write/Edit aimed inside a codex/ workspace is
        refused by the lock-out (the only hook that sees it), while the lead's
        passes."""
        run("git", "-C", self.entity, "branch", "codex/keep")
        cx = os.path.join(self.env.root, "codex-wt")
        run("git", "-C", self.entity, "worktree", "add", "-q", cx, "-b", "codex/live")
        tip = run("git", "-C", self.entity, "rev-parse", "codex/keep").stdout.strip()
        aid, npath = self.spawn("zach-opus-cx")
        self.commit(npath, "cx.txt")
        mine = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        self.pre(aid, "tu-1")
        run("git", "-C", self.entity, "branch", "-f", "codex/keep", mine)          # a mover the guard did not see
        self.post(aid, call="tu-1")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "codex/keep").stdout.strip(), mine)
        self.assertEqual([h["branch"] for h in self.rec("zach-opus-cx").get("history") or []
                          if h.get("fact") == "protected ref moved (reported, not restored)"], ["codex/keep"])
        run("git", "-C", self.entity, "branch", "-f", "codex/keep", tip)
        self.pre(aid, "tu-2")
        run("git", "-C", self.entity, "update-ref", "-d", "refs/heads/codex/keep")
        self.post(aid, call="tu-2")
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "codex/keep").stdout.strip(), tip)
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Edit",
                                     "tool_input": {"file_path": os.path.join(cx, "README")}})[0], "CODEX")
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Write",
                                     "tool_input": {"file_path": os.path.join(cx, "sub", "new.txt")}})[0], "CODEX")
        self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Edit",
                                     "tool_input": {"file_path": os.path.join(npath, "README")}})[0], "REGISTERED")
        self.assertEqual(ws.barrier({"session_id": self.sid, "tool_name": "Edit",
                                     "tool_input": {"file_path": os.path.join(cx, "README")}})[0], "LEAD")

    def test_point_02_the_restore_of_a_deleted_ref_is_create_only_and_never_clobbers(self):
        """The one write this check still makes is CREATE-ONLY: the empty old
        value in `git update-ref -m <msg> --no-deref <ref> <tip> ""` means the
        ref must not exist. So a ref somebody re-created between the deletion and
        the check — at a different tip, and deliberately — is not clobbered, the
        attempt is recorded as a failure rather than forced, and the write can
        never oscillate with another agent's."""
        run("git", "-C", self.entity, "branch", "codex/race")
        tip = run("git", "-C", self.entity, "rev-parse", "codex/race").stdout.strip()
        aid, npath = self.spawn("zach-opus-cr")
        self.commit(npath, "cr.txt")
        other_tip = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        self.pre(aid, "tu-1")
        run("git", "-C", self.entity, "update-ref", "-d", "refs/heads/codex/race")
        run("git", "-C", self.entity, "branch", "codex/race", other_tip)       # re-created, elsewhere
        self.post(aid, call="tu-1")
        # Seen as a move, not a deletion — and a move is never written.
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "codex/race").stdout.strip(), other_tip)
        self.assertEqual([h["fact"] for h in self.rec("zach-opus-cr").get("history") or []
                          if str(h.get("fact", "")).startswith("protected ref")],
                         ["protected ref moved (reported, not restored)"])
        # And the engine's own write refuses while the ref exists, rather than
        # forcing it — the property, asked of the line that carries it.
        rc, _o, err = ws._recreate_deleted_ref(self.entity, "codex/race", tip, "zach-opus-cr")
        self.assertNotEqual(rc, 0)
        self.assertIn("already exists", err)
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "codex/race").stdout.strip(), other_tip)
        # With the ref gone it succeeds, at the tip it held, and says who wrote it.
        run("git", "-C", self.entity, "update-ref", "-d", "refs/heads/codex/race")
        rc, _o, err = ws._recreate_deleted_ref(self.entity, "codex/race", tip, "zach-opus-cr")
        self.assertEqual(rc, 0, err)
        self.assertEqual(run("git", "-C", self.entity, "rev-parse", "codex/race").stdout.strip(), tip)
        self.assertIn("richos engine: protected ref restored",
                      run("git", "-C", self.entity, "reflog", "show", "codex/race").stdout)

    def test_point_14_a_second_body_of_work_never_moves_the_first_ones_agents(self):
        """Point 14: "the branch THIS WORK integrates on." Point 5 permits a
        second body of work to start in a repository while the first one's
        agents are still running. The record used to have one slot per
        REPOSITORY, so recording the second body's branch moved the first
        body's running agents onto it retroactively: their work was merged onto
        the branch they were spawned for, their land was measured against a
        branch they had never heard of, it refused forever, and their
        workspaces were stranded — against point 14's own "'it cannot go to
        main yet' is never a reason for anything to be left behind"."""
        run("git", "-C", self.entity, "branch", "dev/first")
        first = ws.record_integration(self.entity, "dev/first", "body of work ONE", self.sid)
        aid, npath = self.spawn("zach-opus-b1")
        self.commit(npath, "one.txt")
        # A SECOND body of work starts in the same repository while that agent
        # is still running, and Rich records its branch as point 14 requires.
        run("git", "-C", self.entity, "branch", "dev/second")
        second = ws.record_integration(self.entity, "dev/second", "body of work TWO", self.sid)
        self.assertNotEqual(first["id"], second["id"])
        self.assertEqual(ws.integration_record(self.entity)["branch"], "dev/second")
        # The first body's agent is still bound to the FIRST body of work.
        self.finish(aid)
        self.ff(self.entity, "dev/first", "worktree-agent-" + aid)
        ws.land("zach-opus-b1", self.sid)
        self.assertFalse(os.path.exists(npath))
        self.assertNotIn("worktree-agent-" + aid, branches(self.entity))
        self.assertEqual(self.names(), [])
        # And an agent of the SECOND body lands against the second branch.
        aid2, npath2 = self.spawn("zach-opus-b2")
        self.commit(npath2, "two.txt")
        self.finish(aid2)
        self.ff(self.entity, "dev/second", "worktree-agent-" + aid2)
        ws.land("zach-opus-b2", self.sid)
        self.assertFalse(os.path.exists(npath2))
        # The superseded body of work is still readable — an agent spawned for
        # it lands against it, so it is not history.
        self.assertIn(first["id"], ws.all_bodies_of_work())


class QAToolkitAtTheLand(Base):
    """What a QA walk wrote from scratch is said at its land.

    The CEO, 2026-09-20: "what else must be done to ensure the QA toolkit
    actually gets used?" The spawn carries the toolkit's index in
    (scripts/lib/qa-toolkit.py); this is the other end. The type decides, the
    transcript is the evidence, and a count that could not be taken says so —
    an absent count and a clean walk are opposite facts that must never print
    the same way."""

    def _transcript(self, aid, rows):
        d = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "projects",
                         "-some-project", self.sid, "subagents")
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, "agent-%s.jsonl" % aid)
        with open(p, "w", encoding="utf-8") as fh:
            for r in rows:
                fh.write(json.dumps(r) + "\n")
        return p

    @staticmethod
    def _write(path):
        return {"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t", "name": "Write",
             "input": {"file_path": path, "content": "x"}}]}}

    def test_qa_type_land_names_what_the_walk_wrote(self):
        aid, _ = self.spawn("ray-opus-q1", agent_id="aray000000010000",
                              stype="ray", sid=self.sid)
        self._transcript(aid, [self._write("/scratch/ray/contrast.py"),
                               self._write("/scratch/ray/ocrgate.py"),
                               self._write("/scratch/ray/audit.md")])
        lines = ws.qa_throwaway_lines("ray-opus-q1", self.sid)
        joined = "\n".join(lines)
        self.assertIn("2 helper scripts", joined)
        self.assertIn("contrast.py", joined)
        self.assertIn("ocrgate.py", joined)
        self.assertNotIn("audit.md", joined)       # a document is not a helper

    def test_a_clean_walk_says_zero_rather_than_nothing(self):
        aid, _ = self.spawn("kai-opus-q2", agent_id="akai000000020000",
                              stype="kai", sid=self.sid)
        self._transcript(aid, [self._write("/scratch/kai/audit.md")])
        joined = "\n".join(ws.qa_throwaway_lines("kai-opus-q2", self.sid))
        self.assertIn("0 helper scripts", joined)

    def test_a_non_qa_type_prints_nothing_at_all(self):
        aid, _ = self.spawn("zach-opus-q3", agent_id="azach00000030000",
                              stype="zach", sid=self.sid)
        self._transcript(aid, [self._write("/scratch/zach/deploy.sh")])
        # An engineer writing a script is an engineer doing its job. The walk
        # toolkit says nothing about it, so neither does its land.
        self.assertEqual(ws.qa_throwaway_lines("zach-opus-q3", self.sid), [])

    def test_a_missing_transcript_is_said_never_silent(self):
        self.spawn("quint-opus-q4", agent_id="aquint0000040000",
                     stype="quint", sid=self.sid)
        joined = "\n".join(ws.qa_throwaway_lines("quint-opus-q4", self.sid))
        self.assertIn("no transcript found", joined)
        self.assertIn("NOT taken", joined)

    def test_it_never_breaks_the_land(self):
        """A counter that could break the deleter it hangs off would be a worse
        defect than the one it reports."""
        aid, _ = self.spawn("ray-opus-q5", agent_id="aray000000050000",
                              stype="ray", sid=self.sid)
        p = self._transcript(aid, [self._write("/scratch/ray/one.sh")])
        with open(p, "w", encoding="utf-8") as fh:
            fh.write("not json at all\n")          # the counter raises on this
        lines = ws.qa_throwaway_lines("ray-opus-q5", self.sid)
        self.assertTrue(lines and "could not be taken" in lines[0], lines)

    def test_an_unknown_agent_does_not_raise(self):
        lines = ws.qa_throwaway_lines("nobody-opus-q6", self.sid)
        self.assertTrue(all("Traceback" not in l for l in lines))

    def test_the_land_command_itself_prints_it(self):
        """The wiring, not just the function: `workspaces.sh land <agent>` is
        what Rich runs, and a count no command prints is a count nobody reads.

        sweep_scratch_after_land is patched out because it deletes through
        scripts/scratch-sweep.sh against the machine's declared scratch roots,
        and no unit test has any business reaching outside its sandbox to do
        that. (Since 2026-10-01 it takes the landed agent's scopes; the scoped
        sweep has its own suite, scripts/scratch-land-scope.test.sh.)"""
        import contextlib
        import io
        aid, npath = self.spawn("ray-opus-q7", agent_id="aray000000070000",
                                stype="ray", sid=self.sid)
        self._transcript(aid, [self._write("/scratch/ray/contrast.py")])
        self.commit(npath)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        buf = io.StringIO()
        with patch.object(ws, "sweep_scratch_after_land", lambda *a, **k: None):
            with contextlib.redirect_stdout(buf):
                rc = ws.main(["--session", self.sid, "land", "ray-opus-q7"])
        out = buf.getvalue()
        self.assertEqual(rc, 0, out)
        self.assertIn("landed: ray-opus-q7", out)
        self.assertIn("qa toolkit:", out)
        self.assertIn("contrast.py", out)


class UnknownIsNeverClean(Base):
    """Hunt part 4 (richos-hq docs/audits/2026-09-29-hunt/part-4-codex.md,
    findings 4 to 7): a read that failed, a directory that could not be
    listed, a process that may still be running and a repository that was
    never looked at are each UNKNOWN, and an unknown is never reported clean.
    Point 8: "Deletion therefore never loses anything that was meant to land.\""""

    def _failing_reads(self, npath, branch):
        """git, except that reading the worker's HEAD and reading its branch
        both fail the way a git that cannot read the repository fails."""
        real = ws.git
        npath = os.path.realpath(npath)

        def flaky(repo, *args, **kw):
            if args and args[0] == "rev-parse" and "HEAD" in args and os.path.realpath(repo) == npath:
                return 128, "", "fatal: simulated read failure"
            if args and args[0] in ("rev-parse", "for-each-ref", "show-ref") and \
                    any(branch in str(a) for a in args[1:]):
                return 128, "", "fatal: simulated read failure"
            return real(repo, *args, **kw)
        return flaky

    def test_point_14_a_failed_read_of_a_workers_commit_or_branch_never_proves_it_landed(self):
        """Finding 4. A worker commit that is in NO integration branch, and
        both reads that could show it (the workspace's HEAD and its branch's
        tip) fail. A failed read proves nothing about where the commit is, so
        the land is refused and names what could not be read."""
        aid, npath = self.spawn("zach-opus-rd")
        self.commit(npath, "unmerged.txt")
        self.finish(aid)
        branch = "worktree-agent-" + aid
        with patch.object(ws, "git", self._failing_reads(npath, branch)):
            with self.assertRaises(ws.SpecError) as e:
                ws.land("zach-opus-rd", self.sid)
        self.assertIn("could not be read", str(e.exception))
        self.assertTrue(os.path.isdir(npath))
        self.assertIn(branch, branches(self.entity))
        self.assertFalse((self.rec("zach-opus-rd") or {}).get("disposition"))

    def test_point_13_a_branch_whose_tip_cannot_be_read_is_never_recorded_deleted(self):
        """Finding 4, the deletion half. "Already gone" is (True, ""), which
        the record writes down as deleted; a tip that could not be READ is a
        failure to retry (point 13), never an absence."""
        run("git", "-C", self.entity, "branch", "worktree-agent-unreadable0")
        with patch.object(ws, "git", self._failing_reads(self.entity, "worktree-agent-unreadable0")):
            ok, why = ws.delete_branch(self.entity, "worktree-agent-unreadable0")
        self.assertIs(ok, False, why)
        self.assertIn("could not be read", why)
        self.assertIn("worktree-agent-unreadable0", branches(self.entity))

    def test_point_08_an_unreadable_ignored_directory_is_never_certified_unchanged(self):
        """Finding 5. An ignored directory whose name the main checkout also
        has is compared by content; one that cannot be LISTED has content
        nobody has compared, so it is named, never passed over. Two places
        can skip it: `git status` itself (the ignored directory is unreadable:
        git warns on stderr, exits 0 and lists nothing), and the content walk
        (a directory inside it is unreadable)."""
        aid, npath = self.spawn("zach-opus-ur")
        mine = os.path.join(npath, "build")
        other = os.path.join(self.entity, "build")
        os.makedirs(os.path.join(mine, "private"))
        with open(os.path.join(mine, "private", "only-copy.txt"), "w") as f:
            f.write("the only copy\n")
        os.makedirs(other)                                       # the main checkout has the name
        for d in (mine, other):
            with open(os.path.join(d, "same.txt"), "w") as f:
                f.write("identical in both\n")
        _dirty, readable_ignored = ws.uncommitted(npath)
        self.assertEqual(readable_ignored, ["build/private/only-copy.txt"])   # the control
        self.finish(aid)
        for locked in (os.path.join(mine, "private"), mine):   # a directory inside it, then the ignored one itself
            rel = os.path.relpath(locked, npath)
            os.chmod(locked, 0)
            try:
                if locked != mine:                               # the walk, on its own
                    walked = ws._ignored_dir_diff(mine, other, "build")
                    self.assertEqual(walked, ["build/private/ (unreadable: Permission denied)"])
                _dirty, ignored = ws.uncommitted(npath)
                self.assertEqual(ignored, ["%s/ (unreadable: Permission denied)" % rel])
                with self.assertRaises(ws.SpecError) as e:
                    ws.land("zach-opus-ur", self.sid)
                self.assertIn("ignored", str(e.exception))
            finally:
                os.chmod(locked, 0o755)
        self.assertTrue(os.path.isfile(os.path.join(mine, "private", "only-copy.txt")))

    def _bg_post(self, aid, call):
        ws.observe({"session_id": self.sid, "agent_id": aid, "tool_name": "Bash",
                    "hook_event_name": "PostToolUse", "tool_use_id": call,
                    "tool_input": {"command": "sleep 30 && git branch later", "run_in_background": True}})

    def test_point_03_a_background_window_outlives_the_next_observation(self):
        """Finding 6, the report's own witness. A backgrounded call's Post, a
        whole call after it while its process may still run, THEN the process
        makes a branch carrying a commit, another whole call, and the end of
        the run. Nothing says the process ended at the intervening call, so
        its window is still open when the branch appears."""
        aid, npath = self.spawn("zach-opus-bgl")
        self.commit(npath, "bgl.txt")
        self.pre(aid, "tu-bgl")
        self._bg_post(aid, "tu-bgl")
        self.tool_call(aid, "tu-bgl-mid")                   # nothing made yet
        run("git", "-C", npath, "checkout", "-q", "-b", "bg/later")
        self.commit(npath, "later.txt")                     # the process, after that call
        run("git", "-C", npath, "checkout", "-q", "worktree-agent-" + aid)
        self.tool_call(aid, "tu-bgl-after")
        self.finish(aid)
        self.assertEqual(self.created("zach-opus-bgl"), ["bg/later"])
        self.merge(self.entity, "worktree-agent-" + aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-bgl", self.sid)
        self.assertIn("bg/later", str(e.exception))
        self.assertTrue(os.path.isdir(npath))

    def test_point_08_a_background_window_closes_when_its_shell_is_confirmed_ended_and_not_before(self):
        """Finding 6, both directions, on agent_hold's own record of the call's
        shell (the record every subagent Bash call gets). While that shell
        runs, a branch it makes is the agent's; once the record shows it gone,
        the window is judged once more and closed, so a ref Rich cuts at the
        agent's tip afterwards stays his (the reason the window used to close
        at the next observation, kept where it holds)."""
        hold_env = patch.dict(os.environ, {"RICHOS_AGENT_HOLD_DIR": os.path.join(self.env.root, "agent-hold")})
        hold_env.start()                                    # the fixture's own, never the operator's
        self.addCleanup(hold_env.stop)
        aid, npath = self.spawn("zach-opus-bge")
        self.commit(npath, "bge.txt")
        ah = ws._agent_hold()
        call = "tu-bge"
        payload = {"session_id": self.sid, "agent_id": aid, "tool_use_id": call, "tool_name": "Bash",
                   "tool_input": {"command": "sleep 30", "run_in_background": True}}
        stem, _held = ah._record(payload, "bg", "sleep 30")
        self.pre(aid, call)
        shell = subprocess.Popen(["bash", "-c", "%s %s mark --state %s %s && exec sleep 30"
                                  % (sys.executable, ah.__file__, ah.state_dir(), stem)])
        self.env.procs.append(shell)
        for _ in range(100):
            if os.path.exists(stem + ".pid"):
                break
            time.sleep(0.05)
        self.assertTrue(os.path.exists(stem + ".pid"), "agent_hold's mark never recorded the shell")
        self._bg_post(aid, call)
        self.tool_call(aid, "tu-bge-1")                     # the shell still runs
        run("git", "-C", npath, "branch", "bg/while-running")
        self.tool_call(aid, "tu-bge-2")
        self.assertEqual(self.created("zach-opus-bge"), ["bg/while-running"])
        shell.kill()
        shell.wait()                                        # the record now shows the shell gone
        self.pre(aid, "tu-bge-3")                           # the next call starts: judged once more, closed
        key = self.rec("zach-opus-bge")["key"]
        self.assertEqual([p for p in ws._open_slots(key) if ws._is_background(p)], [])
        # Rich, in the main checkout, at the agent's tip, while that call is open
        run("git", "-C", self.entity, "branch", "rich/keep", run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip())
        self.tool_call(aid, "tu-bge-4")
        self.finish(aid)
        self.assertEqual(self.created("zach-opus-bge"), ["bg/while-running"])

    def test_point_09_a_subagentstop_while_its_own_command_runs_does_not_seal_it(self):
        """2026-10-04: echo-opus-panel16 ended its turn to wait on its own background
        timer; SubagentStop recorded it finished and the lock-out sealed it out of its
        workspace when the timer woke it. While a command the agent started is still
        running (agent_hold's own record of that call's shell), that SubagentStop is not
        its end; the one after the command has ended is."""
        hold_env = patch.dict(os.environ, {"RICHOS_AGENT_HOLD_DIR": os.path.join(self.env.root, "agent-hold")})
        hold_env.start()                                    # the fixture's own, never the operator's
        self.addCleanup(hold_env.stop)
        aid, _npath = self.spawn("echo-opus-timer")
        ah = ws._agent_hold()
        call = "tu-timer"
        payload = {"session_id": self.sid, "agent_id": aid, "tool_use_id": call, "tool_name": "Bash",
                   "tool_input": {"command": "sleep 30", "run_in_background": True}}
        stem, _held = ah._record(payload, "native", "sleep 30")
        shell = subprocess.Popen(["bash", "-c", "%s %s mark --state %s %s && exec sleep 30"
                                  % (sys.executable, ah.__file__, ah.state_dir(), stem)])
        self.env.procs.append(shell)
        for _ in range(100):
            if os.path.exists(stem + ".pid"):
                break
            time.sleep(0.05)
        self.assertTrue(os.path.exists(stem + ".pid"), "agent_hold's mark never recorded the shell")
        self.finish(aid)                                    # it ends its turn; the timer still runs
        fin, _paused, why = ws.finished_state(self.rec("echo-opus-timer"))
        self.assertFalse(fin, why)
        self.assertEqual(self.pre(aid, "tu-woken")[0], "REGISTERED", "woken by its timer, it may still work")
        shell.kill()
        shell.wait()                                        # the timer ends; the agent's next turn ends
        self.finish(aid)
        fin, _paused, why = ws.finished_state(self.rec("echo-opus-timer"))
        self.assertTrue(fin, why)
        self.assertEqual(self.pre(aid, "tu-after")[0], "FINISHED")

    def _endless_loop(self, aid, call):
        """2026-10-11, echo-opus-walk49fix1: its own wait loop on a file in a scratch folder it
        had already deleted, recorded by agent_hold exactly as a real call is. Never ends."""
        hold_env = patch.dict(os.environ, {"RICHOS_AGENT_HOLD_DIR": os.path.join(self.env.root, "agent-hold")})
        hold_env.start()                                    # the fixture's own, never the operator's
        self.addCleanup(hold_env.stop)
        ah = ws._agent_hold()
        loop = 'until grep -q "Finished" %s 2>/dev/null; do sleep 0.2; done' % os.path.join(
            self.env.root, "deleted-scratch", "build.log")
        payload = {"session_id": self.sid, "agent_id": aid, "tool_use_id": call, "tool_name": "Bash",
                   "tool_input": {"command": loop}}
        stem, _held = ah._record(payload, "native", loop)
        shell = subprocess.Popen(["bash", "-c", "%s %s mark --state %s %s\nexport %s=%s %s=%s\n%s"
                                  % (sys.executable, ah.__file__, ah.state_dir(), stem,
                                     ah.TAG, aid, ah.SESSION_TAG, self.sid, loop)],
                                 start_new_session=True)
        self.env.procs.append(shell)
        for _ in range(100):
            if os.path.exists(stem + ".pid"):
                break
            time.sleep(0.05)
        self.assertTrue(os.path.exists(stem + ".pid"), "agent_hold's mark never recorded the shell")
        self.assertIsNone(shell.poll(), "the loop never finishes by itself")
        return shell, ah

    def _gone_within(self, shell, seconds=120):
        try:
            shell.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            return False
        return True

    def test_point_09_when_its_run_ends_its_own_endless_loop_is_ended_and_logged(self):
        """The registry records the agent's end (here a stop): the endless loop it owns is
        ended, through agent_hold's ownership record, and the end says what it ended."""
        aid, _npath = self.spawn("echo-opus-loopstop")
        shell, ah = self._endless_loop(aid, "tu-loop-stop")
        ws.record_end(self.sid, aid, "stopped", "TaskStop")
        self.assertTrue(self._gone_within(shell), "its endless loop outlived the end of its run")
        with open(os.path.join(ah.state_dir(), "ended.log")) as f:
            log = f.read()
        self.assertIn("ended echo-opus-loopstop", log)
        self.assertIn('until grep -q "Finished"', log)
        events = [json.loads(line) for line in open(ws._p("events.jsonl"))]
        self.assertTrue(any(e.get("event") == "ended-owned-work" and e.get("agent_id") == aid for e in events))

    def test_point_11_the_apps_stop_of_a_paused_agent_ends_its_own_loop(self):
        """The app's Stop (operator_runtime.rs registry_stop -> `workspaces.sh stop`) of a paused
        agent whose turn already ended: the stop releases the hold, which continues the loop, so
        the stop must also end it (second review rv-20261011T042129Z-6b0f0810-1c93, P2)."""
        aid, _npath = self.spawn("echo-opus-loopapp")
        shell, ah = self._endless_loop(aid, "tu-loop-app")
        ws.pause("echo-opus-loopapp", "the quota reset", self.sid)
        ws.record_end(self.sid, aid, "SubagentStop")         # a paused run's turn ends: not a stop
        self.assertIsNone(shell.poll(), "a pause never ends anything")
        stopped = ws.stop("echo-opus-loopapp", "stop this agent", self.sid)
        self.assertEqual(stopped["end"]["signal"], "stopped")
        self.assertTrue(self._gone_within(shell), "the app's Stop left its own loop running")
        with open(os.path.join(ah.state_dir(), "ended.log")) as f:
            self.assertIn("ended echo-opus-loopapp", f.read())

    def test_point_09_a_deferred_end_keeps_its_loop_until_the_one_command_ends_it(self):
        """Option B (the lead, 2026-10-11): a SubagentStop while its own command runs is still not
        its end and ends nothing (echo-opus-panel16). The one command stall-watch names ends its
        own commands; its next turn then ends with nothing running, and that is its end."""
        aid, _npath = self.spawn("echo-opus-loopdefer")
        shell, ah = self._endless_loop(aid, "tu-loop-defer")
        self.finish(aid)                                    # its turn ends with its loop running
        fin, _paused, why = ws.finished_state(self.rec("echo-opus-loopdefer"))
        self.assertFalse(fin, why)
        self.assertIsNone(shell.poll(), "a deferred end ends nothing")
        cmd = ah.end_command(self.sid, aid, "echo-opus-loopdefer")
        out = subprocess.run(shlex.split(cmd), capture_output=True, text=True, timeout=60)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertTrue(self._gone_within(shell), "the one command did not end its loop")
        self.finish(aid)                                    # woken by that, its next turn ends
        fin, _paused, why = ws.finished_state(self.rec("echo-opus-loopdefer"))
        self.assertTrue(fin, why)

    def test_point_09_a_subagentstop_just_after_its_command_ended_but_before_the_wake_does_not_seal_it(self):
        """2026-10-08: echo-fable-dict5's background test run ended at 16:55:45, its turn ended
        at 16:55:46.18 with no shell of its own left running, the end was recorded, and the
        platform woke it 0.6 s later with that run's notification, into the lock-out. Replayed in
        that order: the command ends, SubagentStop, SubagentStart. Until the notification is in
        its transcript the stop is not its end; once it is, the next stop is, and a restart after
        that is refused (point 9)."""
        hold_env = patch.dict(os.environ, {"RICHOS_AGENT_HOLD_DIR": os.path.join(self.env.root, "agent-hold")})
        hold_env.start()                                    # the fixture's own, never the operator's
        self.addCleanup(hold_env.stop)
        aid, npath = self.spawn("echo-fable-race")
        ah = ws._agent_hold()
        call = "toolu_race"
        payload = {"session_id": self.sid, "agent_id": aid, "tool_use_id": call, "tool_name": "Bash",
                   "tool_input": {"command": "cargo test", "run_in_background": True}}
        stem, _held = ah._record(payload, "bg", "cargo test")
        transcript = os.path.join(ws._platform_projects_dir(), "-fixture", self.sid, "subagents",
                                  "agent-%s.jsonl" % aid)
        os.makedirs(os.path.dirname(transcript))

        def append(row):
            row.update({"agentId": aid, "sessionId": self.sid, "isSidechain": True})
            with open(transcript, "a") as f:
                f.write(json.dumps(row) + "\n")
        # The platform's receipt: the call went to the background.
        append({"type": "user", "toolUseResult": {"stdout": "", "stderr": "", "interrupted": False,
                                                  "backgroundTaskId": "bkrace"},
                "message": {"role": "user", "content": [{"tool_use_id": call, "type": "tool_result",
                            "content": "Command running in background with ID: bkrace. Output is being "
                                       "written to: /tmp/x/tasks/bkrace.output.", "is_error": False}]}})
        shell = subprocess.Popen(["bash", "-c", "%s %s mark --state %s %s && exit 101"
                                  % (sys.executable, ah.__file__, ah.state_dir(), stem)])
        self.env.procs.append(shell)
        shell.wait()                                        # the command ends (reaped: no shell is left)
        self.assertTrue(os.path.exists(stem + ".pid"), "agent_hold's mark never recorded the shell")
        self.finish(aid)                                    # its turn ends before the notification arrives
        fin, _paused, why = ws.finished_state(self.rec("echo-fable-race"))
        self.assertFalse(fin, why)
        ws.record_start(self.sid, aid, npath, "echo")       # woken by that notification, 0.6 s later
        append({"type": "user", "origin": {"kind": "task-notification"}, "message": {"role": "user", "content":
                "<task-notification>\n<task-id>bkrace</task-id>\n<tool-use-id>%s</tool-use-id>\n"
                "<status>failed</status>\n</task-notification>" % call}})
        self.assertEqual(self.pre(aid, "tu-woken")[0], "REGISTERED", "woken by its own command, it may still work")
        self.finish(aid)                                    # the turn the notification started ends
        fin, _paused, why = ws.finished_state(self.rec("echo-fable-race"))
        self.assertTrue(fin, why)
        ws.record_start(self.sid, aid, npath, "echo")       # the platform restarts a finished agent
        self.assertEqual(self.pre(aid, "tu-after")[0], "FINISHED")

    def test_point_03_a_known_stray_in_one_repository_never_hides_one_in_another(self):
        """Finding 7. A stray already known in the first repository must not
        narrow the scan of the repositories it was asked to look at."""
        a = os.path.join(self.env.root, "entity-wt", "stray-a")
        run("git", "-C", self.entity, "worktree", "add", "-q", a, "-b", "cc/stray-a")
        self.commit(a)
        first = ws.scan_unregistered([self.entity, self.other])
        self.assertEqual(len(first), 1, first)
        b = os.path.join(self.env.root, "other-wt", "stray-b")
        run("git", "-C", self.other, "worktree", "add", "-q", b, "-b", "cc/stray-b")
        self.commit(b)
        second = ws.scan_unregistered([self.entity, self.other])
        self.assertEqual(len(second), 1, second)
        self.assertEqual([w.get("branch") for w in ws.load_agent(second[0])["workspaces"]], ["cc/stray-b"])


class Finding11_ContainerCleanupIsRetried(Base):
    """Hunt part 4, finding 11 (richos-hq docs/audits/2026-09-29-hunt/
    part-4-codex.md): a workspace's containers that could not be stopped are a
    deletion failure like any other (point 13), whatever the disposition, and
    a reaper that could not even run is not a success."""

    FAILED = {"available": True, "removed": [], "kept": [],
              "failed": [{"name": "agent-ct-redis-1", "id": "c0ffee", "why": "simulated daemon refusal"}]}

    def _discarded_with(self, containers_result):
        aid, npath = self.spawn("zach-opus-ct11")
        self.finish(aid)
        with patch.object(ws, "stop_containers", return_value=containers_result):
            ws.discard("zach-opus-ct11", "the reviewer rejected the approach",
                       not_ceo_ordered="an internal experiment", me=self.sid)
        return npath

    def test_finding_11_a_discard_whose_containers_survived_keeps_a_retry(self):
        npath = self._discarded_with(self.FAILED)
        key = ws.named_key(self.sid, "zach-opus-ct11")
        self.assertTrue(os.path.exists(ws.agent_path(key)), "the record was filed as done")
        self.assertFalse(os.path.exists(ws.done_path(key)))
        d = self.rec("zach-opus-ct11")["deletion"]
        self.assertTrue(d, "no deletion retry was recorded")
        self.assertIn("agent-ct-redis-1", d["last_error"])
        # Held like a process that survived its stop: nothing of the workspace
        # is deleted while something may still be running against it.
        self.assertTrue(os.path.isdir(npath))
        # The retry asks about the SAME workspace again, and once its
        # containers are gone the deletion completes and the record is filed.
        seen = []
        with patch.object(ws, "stop_containers", side_effect=lambda paths: seen.append(list(paths)) or {}):
            ws.retry_due()
        self.assertIn([npath], seen)
        self.assertFalse(os.path.exists(npath))
        self.assertFalse(os.path.exists(ws.agent_path(key)))
        self.assertTrue(os.path.exists(ws.done_path(key)))
        self.assertIsNone(ws.load_agent(key)["deletion"])

    def test_finding_11_a_reaper_that_could_not_run_is_never_a_success(self):
        class Broken(object):
            @staticmethod
            def reap_for_workspaces(paths, ending=False):
                raise RuntimeError("simulated reaper crash")
        with patch.dict(sys.modules, {"containers": Broken}):
            res = ws.stop_containers(["/nonexistent/ws-finding-11"])
        self.assertTrue(res.get("error"), res)
        self.assertIn("simulated reaper crash", res["error"])
        # ...and it is carried into the deletion record like a failed container
        aid, npath = self.spawn("zach-opus-ct11b")
        self.finish(aid)
        with patch.dict(sys.modules, {"containers": Broken}):
            ws.discard("zach-opus-ct11b", "the reviewer rejected the approach",
                       not_ceo_ordered="an internal experiment", me=self.sid)
        d = self.rec("zach-opus-ct11b")["deletion"]
        self.assertTrue(d, "a crashed reaper left no retry")
        self.assertIn("simulated reaper crash", d["last_error"])


class Finding12_MentionIsNotOwnership(Base):
    """Hunt part 4, finding 12: a process whose command line merely MENTIONS a
    workspace path is not the workspace's process. Point 9 stops what the agent
    started; the second selection loop used to add every process naming the
    path — a reviewer, a log reader, an editor — to the same stop list."""

    def _sleeper(self, argv_path, cwd, **kw):
        """A python process that names `argv_path` in its arguments and does
        nothing else, started by this test (so owned by it, for cleanup)."""
        pr = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)", argv_path],
                              cwd=cwd, **kw)
        self.env.procs.append(pr)
        return pr

    def _detached(self, argv_path, cwd):
        """The residue point 9 exists for: a detached child left behind by a
        run that ended, reparented to launchd, in a session with no terminal,
        working from OUTSIDE the workspace but on it. Its pid is recorded by
        the shell that started it, so the test stops only that pid."""
        pidfile = os.path.join(self.env.root, "detached-%d.pid" % len(self.env.procs))
        sh = subprocess.Popen(["sh", "-c", '"$0" -c "import time; time.sleep(300)" "$1" & echo $! > "$2"',
                               sys.executable, argv_path, pidfile], cwd=cwd, start_new_session=True)
        sh.wait(timeout=30)
        pid = int(open(pidfile).read().strip())
        self.addCleanup(lambda: ws._alive(pid) and os.kill(pid, signal.SIGKILL))
        return pid

    def test_finding_12_a_process_that_only_names_the_workspace_keeps_running(self):
        aid, npath = self.spawn("zach-opus-ment")
        stranger = self._sleeper(npath, self.env.root)        # a reader elsewhere, naming the path
        detached = self._detached(npath, self.env.root)       # and one nobody's parent any more
        time.sleep(0.3)
        self.finish(aid)
        self.assertEqual(self.names(), [])                     # it produced nothing: landed
        self.assertFalse(os.path.exists(npath))                # and the workspace is still deleted
        self.assertIsNone(stranger.poll(), "a process that only named the workspace was stopped")
        self.assertTrue(ws._alive(detached), "a detached process that only named the workspace was stopped")
        # Not stopped, and not silent either: the land names what it spared.
        ev = [json.loads(l) for l in open(os.path.join(ws.state_dir(), "events.jsonl"))]
        named = [e for e in ev if e.get("event") == "processes-named-only"]
        self.assertTrue(named, "the spared processes were not recorded")
        self.assertIn(stranger.pid, named[-1]["pids"])
        self.assertIn(detached, named[-1]["pids"])

    def test_finding_12_a_child_of_a_process_in_the_workspace_is_still_stopped(self):
        """The reason for the second loop, kept where it holds: a command
        working on the workspace from elsewhere. A process the workspace's own
        process started is owned by that ancestry, wherever it works."""
        aid, npath = self.spawn("zach-opus-left")
        # a shell working IN the workspace, whose child works from `/`
        parent = subprocess.Popen(["sh", "-c", '(cd / && exec "$0" -c "import time; time.sleep(300)" "$1"); :',
                                   sys.executable, npath], cwd=npath, stdout=subprocess.PIPE)
        self.env.procs.append(parent)
        time.sleep(0.5)
        rows = [l.split() for l in run("ps", "-axo", "pid=,ppid=").stdout.splitlines()]
        child = [int(r[0]) for r in rows if len(r) == 2 and r[1] == str(parent.pid)]
        self.assertEqual(len(child), 1, "the shell's child was not found by its parent pid")
        self.finish(aid)
        self.assertEqual(self.names(), [])
        parent.wait(timeout=15)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and ws._alive(child[0]):
            time.sleep(0.1)
        self.assertFalse(ws._alive(child[0]), "the workspace process's own child, working elsewhere, survived")


class Finding13_NoSpeculativeShutdown(Base):
    """Hunt part 4, finding 13: an automatic land attempt stopped a finished
    worker's processes BEFORE finding its commits were not merged, so a status
    request, a spawn check or a turn end killed a test or server whose work
    could not be cleaned up anyway."""

    def _unmerged_with_process(self, name):
        aid, npath = self.spawn(name)
        self.commit(npath, "unmerged.txt")
        pr = subprocess.Popen(["sleep", "300"], cwd=npath)
        self.env.procs.append(pr)
        time.sleep(0.2)
        self.finish(aid)
        return aid, npath, pr

    def test_finding_13_a_pending_scan_leaves_unmerged_work_running(self):
        aid, npath, pr = self._unmerged_with_process("zach-opus-spec")
        self.assertEqual(self.names(), ["zach-opus-spec"])       # the automatic attempt
        self.assertTrue(ws.gate_stop({"session_id": self.sid}, self.entity)[0])
        self.assertIsNone(pr.poll(), "an automatic land attempt stopped a process of unmerged work")
        with self.assertRaises(ws.SpecError) as e:              # and a land by hand says why
            ws.land("zach-opus-spec", self.sid)
        self.assertIn("is not in", str(e.exception))
        self.assertIsNone(pr.poll())
        self.assertTrue(os.path.isdir(npath))

    def test_finding_13_once_merged_the_land_still_stops_and_rechecks(self):
        """The reason for the order, kept: once the work IS merged, processes
        are stopped first and landing is proved after (LandingShutdown)."""
        aid, npath, pr = self._unmerged_with_process("zach-opus-spec2")
        self.merge(self.entity, "worktree-agent-" + aid)
        self.assertEqual(self.names(), [])
        pr.wait(timeout=15)
        self.assertFalse(os.path.exists(npath))


class Finding29_AskingChangesNothing(Base):
    """Hunt part 4, finding 29: `workspaces.sh status` and the dry spawn check
    (`check-spawn`) ask what WOULD happen. They used to land and delete
    eligible work, stopping its processes, on the way to the answer."""

    def _landable_with_process(self, name):
        aid, npath = self.spawn(name)
        self.commit(npath, "landable.txt")
        pr = subprocess.Popen(["sleep", "300"], cwd=npath)
        self.env.procs.append(pr)
        time.sleep(0.2)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        return aid, npath, pr

    def _untouched(self, name, npath, pr):
        self.assertIsNone(pr.poll(), "asking stopped a process of %s" % name)
        self.assertTrue(os.path.isdir(npath), "asking deleted %s's workspace" % name)
        self.assertIsNone(self.rec(name)["disposition"], "asking landed %s" % name)

    def test_finding_29_status_reports_and_changes_nothing(self):
        aid, npath, pr = self._landable_with_process("zach-opus-st")
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with redirect_stdout(buf):
            ws._print_status(self.sid, self.entity)
        self._untouched("zach-opus-st", npath, pr)
        out = buf.getvalue()
        self.assertIn("zach-opus-st", out)
        self.assertIn("LANDABLE", out)                          # and it says what WOULD happen
        # The operation that is asked to act still acts.
        self.assertEqual(self.names(), [])
        pr.wait(timeout=15)
        self.assertFalse(os.path.exists(npath))

    def test_finding_29_a_dry_spawn_check_changes_nothing(self):
        pa, ppath = self.spawn("zach-opus-cont")
        self.commit(ppath, "unmerged.txt")
        self.finish(pa)                                         # the work the new spawn continues
        aid, npath, pr = self._landable_with_process("zach-opus-bystander")
        payload = {"session_id": self.sid, "tool_name": "Agent", "richos_spawn_check": {"planned": []},
                   "tool_input": {"name": "zach-opus-next", "subagent_type": "zach", "isolation": "worktree",
                                  "prompt": "do it\ncontinues: zach-opus-cont\n"}}
        self.assertTrue(ws.is_spawn_check(payload))
        ws.register_spawn(payload, self.entity, dry=True)
        self._untouched("zach-opus-bystander", npath, pr)
        self.assertIsNone(self.rec("zach-opus-next"), "a dry check wrote a registration")


class Finding14_WhatTheEngineMadeIsNotTheAgentsWork(Base):
    """Hunt part 4, finding 14: the creator's own setup leaves ignored content
    no main checkout has (richos: a per-workspace link to its own build cache),
    and the cleanliness check held every such workspace until somebody typed
    --ignored-not-needed. What the ENGINE made at creation is recorded when the
    workspace is confirmed, and only that, exactly as it was, needs no waiver.
    Everything else ignored is still compared and still refused (point 8):
    notes, nested repositories, and a setup product the agent then changed."""

    def _created_with_setup_link(self, name):
        with open(os.path.join(self.other, ".gitignore"), "a") as f:
            f.write("cache-link\n")
        run("git", "-C", self.other, "commit", "-qam", "ignore the setup's cache link")
        path = os.path.join(self.env.root, "other-wt", name)
        ws.register_cc(self.sid, name, self.other, path, "cc/" + name)
        run("git", "-C", self.other, "worktree", "add", "-q", path, "-b", "cc/" + name)
        cache = os.path.join(self.env.root, "cache-" + name)
        os.makedirs(cache)
        os.symlink(cache, os.path.join(path, "cache-link"))        # what .worktree-setup does
        ws.confirm_cc(self.sid, name, path, True)                   # the creator's step 5
        aid, _native = self.spawn(name, path, native=False)
        return aid, path

    def test_finding_14_the_setups_own_link_needs_no_waiver(self):
        aid, path = self._created_with_setup_link("zach-opus-gen")
        self.finish(aid)
        self.assertEqual(self.names(), [], "the engine's own setup product held the land")
        self.assertFalse(os.path.exists(path))

    def test_finding_14_anything_else_ignored_still_needs_the_waiver(self):
        """By hand it still needs the waiver. The AUTOMATIC land (2026-10-04)
        keeps it instead of waiting for one: the notes are moved to the kept
        directory, never deleted, and the setup's own link is not kept."""
        aid, path = self._created_with_setup_link("zach-opus-gen2")
        os.makedirs(os.path.join(path, "build"))
        with open(os.path.join(path, "build", "notes.txt"), "w") as f:
            f.write("the only copy of the agent's notes\n")
        self.finish(aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-gen2", self.sid)
        self.assertIn("build/", str(e.exception))
        self.assertNotIn("cache-link", str(e.exception))
        self.assertTrue(os.path.isfile(os.path.join(path, "build", "notes.txt")))
        self.assertEqual(self.names(), [])                              # the automatic land
        kept = self.rec("zach-opus-gen2")["disposition"]["kept_ignored"]
        found = [os.path.relpath(os.path.join(r, f), kept) for r, _d, fs in os.walk(kept) for f in fs]
        self.assertEqual(len(found), 1)
        self.assertTrue(found[0].endswith(os.path.join("build", "notes.txt")), found)
        self.assertFalse(os.path.exists(path))

    def test_finding_14_a_setup_product_the_agent_changed_is_its_own(self):
        aid, path = self._created_with_setup_link("zach-opus-gen3")
        link = os.path.join(path, "cache-link")
        os.unlink(link)
        os.symlink(os.path.join(self.env.root, "somewhere-else"), link)
        self.finish(aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-gen3", self.sid)
        self.assertIn("cache-link", str(e.exception))
        # The automatic land (2026-10-04) keeps the changed link, as a link.
        self.assertEqual(self.names(), [])
        kept = self.rec("zach-opus-gen3")["disposition"]["kept_ignored"]
        links = [os.path.join(r, n) for r, ds, fs in os.walk(kept) for n in fs + ds
                 if os.path.islink(os.path.join(r, n))]
        self.assertEqual([os.readlink(p) for p in links], [os.path.join(self.env.root, "somewhere-else")])


class Finding15_TheBudgetReachesTheWork(Base):
    """Hunt part 4, finding 15: the gate's budget exists so the platform never
    cancels the hook before it answers. The deletion's second landing proof
    ran with no deadline, a retry could overrun its budget inside one attempt,
    the Stop gate started its retries before making its deadline, and a file
    comparison read whole files without looking at the clock. The second
    proof itself stays: shutdown or partial deletion can change eligibility."""

    FILES = 20
    SLOW = 0.3

    def _landed_with_pending_deletion(self, name):
        aid, npath = self.spawn(name)
        self.commit(npath)
        for root in (npath, self.entity):                   # identical ignored files, compared one by one
            os.makedirs(os.path.join(root, "build"), exist_ok=True)
            for i in range(self.FILES):
                with open(os.path.join(root, "build", "f%02d.txt" % i), "w") as f:
                    f.write("same %d\n" % i)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        with patch.object(ws, "remove_workspace", return_value=(False, "simulated disk refusal")):
            self.assertEqual(self.names(), [])
        rec = self.rec(name)
        self.assertEqual(rec["disposition"]["kind"], "landed")
        self.assertTrue(rec["deletion"])
        return npath

    def _slow_same_file(self):
        real = ws._same_file

        def slow(a, b, *args, **kw):
            time.sleep(self.SLOW)
            return real(a, b, *args, **kw)
        return slow

    def test_finding_15_a_retry_stops_at_its_budget_inside_the_attempt(self):
        npath = self._landed_with_pending_deletion("zach-opus-bud")
        slow, compared = self._slow_same_file(), []

        def counted(*a, **kw):
            compared.append(a[0])
            return slow(*a, **kw)
        t0 = time.monotonic()
        # The stages before the proof are bounded by their own tests (V2-03). Here they
        # answer at once, so the 1 s budget reaches the proof on a loaded Mac too. When
        # they spent it (reproduced by slowing the process listing by 1 s), the attempt
        # was deferred before the proof ran and a proof with no deadline passed, as the
        # p05-retry-proof-has-no-deadline mutant did in nightly run 20261024T141722Z-b960a7c2.
        with patch.object(ws, "_same_file", counted), \
                patch.object(ws, "stop_processes", return_value={"stopped": [], "survivors": []}), \
                patch.object(ws, "stop_containers", return_value={}), \
                patch.object(ws, "stop_test_instances", return_value={}):
            ws.retry_due(budget=1.0)
        took = time.monotonic() - t0
        self.assertTrue(compared, "the budget ran out before the landing proof compared a file, "
                                  "so this attempt did not test the proof's deadline")
        self.assertLess(took, self.FILES * self.SLOW * 0.6,
                        "one retry attempt ran %.1fs past a 1s budget" % took)
        rec = self.rec("zach-opus-bud")
        # Out of time is not "eligibility changed": the land stands and the
        # deletion is simply retried later.
        self.assertEqual(rec["disposition"]["kind"], "landed")
        self.assertTrue(rec["deletion"])
        self.assertTrue(os.path.isdir(npath))
        ws.retry_due()                                       # with time, it finishes
        self.assertFalse(os.path.exists(npath))

    def test_finding_15_the_stop_gate_puts_its_retries_inside_its_budget(self):
        self._landed_with_pending_deletion("zach-opus-bud2")
        os.environ["RICHOS_WORKSPACES_GATE_BUDGET"] = "1"
        try:
            t0 = time.monotonic()
            with patch.object(ws, "_same_file", self._slow_same_file()):
                ok, _msg, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
            took = time.monotonic() - t0
        finally:
            os.environ.pop("RICHOS_WORKSPACES_GATE_BUDGET", None)
        self.assertTrue(ok)
        self.assertLess(took, self.FILES * self.SLOW * 0.6,
                        "the Stop gate took %.1fs on a 1s budget" % took)

    def test_finding_15_a_file_comparison_looks_at_the_clock(self):
        a = os.path.join(self.env.root, "big-a")
        b = os.path.join(self.env.root, "big-b")
        for p in (a, b):
            with open(p, "wb") as f:
                f.write(b"x" * (8 << 20))
        self.assertTrue(ws._same_file(a, b))
        with self.assertRaises(ws.Deadline):
            ws._same_file(a, b, deadline=ws.now() - 1)


class HuntV2_02_DeclaredBuildOutputNeedsNoWaiver(Base):
    """Hunt part 4 v2, V2-02: finding 14 exempted what the engine made at
    creation; build output written LATER (an ordinary test or build leaving
    build/) still held the land until somebody typed --ignored-not-needed.
    The repository now declares what is regenerable, in its own committed root
    .gitignore, and only that is waived."""

    def _declare(self, repo, line, commit=True):
        with open(os.path.join(repo, ".gitignore"), "a") as f:
            f.write(line + "\n")
        if commit:
            run("git", "-C", repo, "commit", "-qam", "declare regenerable output")

    def _built_after_setup(self, name):
        path = self.make_cc(name)
        aid, _n = self.spawn(name, path, native=False)
        os.makedirs(os.path.join(path, "build"))
        with open(os.path.join(path, "build", "output.bin"), "wb") as f:
            f.write(b"generated fixture output")
        self.finish(aid)
        return path

    def test_v2_02_declared_build_output_written_after_setup_lands_without_a_waiver(self):
        self._declare(self.other, "#regenerable: build/")
        path = self._built_after_setup("zach-opus-bldv2")
        res = ws.land("zach-opus-bldv2", self.sid)
        self.assertTrue(res["landed"])
        self.assertFalse(os.path.exists(path))

    def test_v2_02_an_undeclared_directory_still_needs_the_waiver(self):
        self._declare(self.other, "#regenerable: dist/")
        path = self._built_after_setup("zach-opus-bldv3")
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-bldv3", self.sid)
        self.assertIn("build/", str(e.exception))
        self.assertTrue(os.path.isfile(os.path.join(path, "build", "output.bin")))

    def test_v2_02_a_declaration_that_is_not_committed_waives_nothing(self):
        self._declare(self.other, "#regenerable: build/", commit=False)
        path = self._built_after_setup("zach-opus-bldv4")
        with self.assertRaises(ws.SpecError):
            ws.land("zach-opus-bldv4", self.sid)
        self.assertTrue(os.path.isdir(path))

    def test_v2_02_the_pattern_language(self):
        pats = ["build/", "**/__pycache__/", "dist/*.map", "/out"]
        yes = ["build/", "build/x.bin", "a/b/__pycache__/", "__pycache__/m.pyc", "dist/app.js.map",
               "out", "out/", "out/deep/file"]
        no = ["builds/", "src/build.py", "dist/app.js", "x/out", "build/ (unreadable: Permission denied)",
              "a/__pycache__"]
        for rel in yes:
            self.assertTrue(ws._is_regenerable(rel, pats), rel)
        for rel in no:
            self.assertFalse(ws._is_regenerable(rel, pats), rel)


class RichosBuildOutputDoesNotBlockAContinuation(Base):
    """2026-10-11: five continuation spawns in one day were refused because the
    finished workspace held the app's build and test output (ui-dist/,
    src-tauri/gen/, ui/tests/.shots/, ui/tests/.vouch/, the connect-helper
    link) that the main checkout lacked, and each time somebody moved it aside
    by hand. The fixture carries THIS repository's own ignore files, exactly as
    committed, so the proof is the declaration that actually ships."""

    REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
    IGNORES = (".gitignore", "richos/app/.gitignore", "richos/app/ui/tests/.gitignore")

    def _carry_the_real_ignore_files(self):
        if not all(os.path.isfile(os.path.join(self.REPO, rel)) for rel in self.IGNORES):
            self.skipTest("this engine copy is not inside the richos repository")
        for rel in self.IGNORES:
            dest = os.path.join(self.other, rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            shutil.copyfile(os.path.join(self.REPO, rel), dest)
            run("git", "-C", self.other, "add", "-f", rel)
        # src-tauri/ holds tracked source in the real checkout, so git names
        # gen/ and the helper link inside it rather than the whole directory.
        os.makedirs(os.path.join(self.other, "richos/app/src-tauri"), exist_ok=True)
        with open(os.path.join(self.other, "richos/app/src-tauri/Cargo.toml"), "w") as f:
            f.write("[package]\n")
        run("git", "-C", self.other, "add", "richos/app/src-tauri/Cargo.toml")
        run("git", "-C", self.other, "commit", "-qm", "the repository's own ignore files")

    def _build_and_test_output(self, path):
        outside = os.path.join(self.env.root, "helper-cache")
        os.makedirs(outside, exist_ok=True)
        for rel in ("richos/app/ui-dist/index.html", "richos/app/src-tauri/gen/schemas/acl.json",
                    "richos/app/ui/tests/.shots/home.png", "richos/app/ui/tests/.vouch/vouch.tgz"):
            os.makedirs(os.path.dirname(os.path.join(path, rel)), exist_ok=True)
            with open(os.path.join(path, rel), "wb") as f:
                f.write(b"regenerated output")
        os.symlink(outside, os.path.join(path, "richos/app/src-tauri/connect-helper"))

    def test_a_workspace_holding_only_the_apps_regenerated_output_is_continued(self):
        self._carry_the_real_ignore_files()
        cc = self.make_cc("zach-opus-ra")
        aid, _npath = self.spawn("zach-opus-ra", cc=cc)
        self.commit(cc)
        self._build_and_test_output(cc)
        self.assertEqual(ws.uncommitted(cc)[0], [])             # all of it is ignored, none of it work
        self.assertNotEqual(ws.uncommitted(cc)[1], [])          # and the main checkout has none of it
        self.finish(aid)
        cc2 = self.make_cc("zach-opus-rb", base="cc/zach-opus-ra")
        self.spawn("zach-opus-rb", cc=cc2, extra="continues: zach-opus-ra\n")
        self.assertEqual(self.rec("zach-opus-ra")["disposition"]["kind"], "continued")
        self.assertFalse(os.path.exists(cc))

    def test_an_undeclared_file_beside_them_still_refuses_the_continuation(self):
        self._carry_the_real_ignore_files()
        cc = self.make_cc("zach-opus-ka")
        aid, _npath = self.spawn("zach-opus-ka", cc=cc)
        self.commit(cc)
        self._build_and_test_output(cc)
        os.makedirs(os.path.join(cc, ".claude"), exist_ok=True)
        with open(os.path.join(cc, ".claude", "notes.md"), "w") as f:
            f.write("the only copy\n")
        self.finish(aid)
        cc2 = self.make_cc("zach-opus-kb", base="cc/zach-opus-ka")
        with self.assertRaises(ws.SpecError) as e:
            self.spawn("zach-opus-kb", cc=cc2, extra="continues: zach-opus-ka\n")
        self.assertIn("(.claude/", str(e.exception))
        self.assertNotIn("ui-dist", str(e.exception))
        self.assertTrue(os.path.isfile(os.path.join(cc, ".claude", "notes.md")))


class VerifierBuildOutputDoesNotBlockAContinuation(Base):
    """Verification reliability PRD A14, T20. On 2026-10-19 two continuations were refused
    because the finished workspace held the conformance verifier's Cargo output
    (echo-opus-phone67: 1487 ignored entries; echo-opus-phone67b: 960), and Rich deleted it by
    hand both times. The fixture carries this repository's own ignore files, the verifier's
    included, exactly as committed: first without the root .gitignore's declaration of that
    output (each refusal again), then as committed (each continued). Source work beside the
    output still refuses, and another task's output is never touched."""

    REPO = RichosBuildOutputDoesNotBlockAContinuation.REPO
    IGNORES = RichosBuildOutputDoesNotBlockAContinuation.IGNORES + ("richos/mobile/conformance/verifier/.gitignore",)
    DECLARATION = "#regenerable: richos/mobile/conformance/verifier/target/"
    TARGET = "richos/mobile/conformance/verifier/target"

    def _carry(self, declared):
        RichosBuildOutputDoesNotBlockAContinuation._carry_the_real_ignore_files(self)
        root = os.path.join(self.other, ".gitignore")
        with open(root) as f:
            text = f.read()
        self.assertIn(self.DECLARATION + "\n", text)          # the declaration that ships
        if not declared:
            with open(root, "w") as f:
                f.write(text.replace(self.DECLARATION + "\n", ""))
            run("git", "-C", self.other, "commit", "-qam", "the ignore files before the declaration")
        os.makedirs(os.path.join(self.other, "richos/mobile/conformance/verifier/src"), exist_ok=True)
        with open(os.path.join(self.other, "richos/mobile/conformance/verifier/src/lib.rs"), "w") as f:
            f.write("pub fn verify() {}\n")
        run("git", "-C", self.other, "add", "richos/mobile/conformance/verifier/src/lib.rs")
        run("git", "-C", self.other, "commit", "-qm", "the verifier's source")
        # The main checkout has built the verifier too, so each workspace file it lacks is named.
        built = os.path.join(self.other, self.TARGET, "shared-build-v1/debug/deps")
        os.makedirs(built)
        with open(os.path.join(built, "main-only.rlib"), "wb") as f:
            f.write(b"rlib")

    def _declare(self):
        with open(os.path.join(self.other, ".gitignore"), "a") as f:
            f.write(self.DECLARATION + "\n")
        run("git", "-C", self.other, "commit", "-qam", "declare the verifier's output regenerable")

    def _cargo_output(self, path, entries):
        """`entries` files of Cargo output, named as the refused continuations named them."""
        first = os.path.join(path, self.TARGET, "shared-build-v1/debug/.fingerprint/base64-2c3994cf87c86cb4")
        os.makedirs(first)
        with open(os.path.join(first, "dep-lib-base64"), "wb") as f:
            f.write(b"fingerprint")
        deps = os.path.join(path, self.TARGET, "shared-build-v1/debug/deps")
        os.makedirs(deps)
        for n in range(entries - 1):
            with open(os.path.join(deps, "unit-%04d.rlib" % n), "wb") as f:
                f.write(b"rlib")

    def _finished_with_output(self, name, entries, base="HEAD"):
        cc = self.make_cc(name, base=base)
        aid, _npath = self.spawn(name, cc=cc)
        self.commit(cc, fname=name + ".txt")
        self._cargo_output(cc, entries)
        self.assertEqual(ws.uncommitted(cc)[0], [])             # none of it is work
        self.finish(aid)
        return cc

    def test_both_refused_continuations_are_continued_once_the_output_is_declared(self):
        self._carry(declared=False)
        # echo-opus-phone67 and echo-opus-phone67b, by the fixture's short names (its agent ids are
        # a name's first twelve letters).
        hops = (("zach-opus-v67", "zach-opus-w67", "zach-opus-x67", 1487),
                ("zach-opus-v67b", "zach-opus-w67b", "zach-opus-x67b", 960))
        for name, refused, _then, entries in hops:
            with self.subTest(name):
                cc = self._finished_with_output(name, entries)
                self.assertEqual(len(ws.uncommitted(cc)[1]), entries)
                nxt = self.make_cc(refused, base="cc/" + name)
                with self.assertRaises(ws.SpecError) as e:
                    self.spawn(refused, cc=nxt, extra="continues: %s\n" % name)
                self.assertIn("%s has %d ignored entries the main checkout does not have (%s/shared-build-v1/"
                              % (cc, entries, self.TARGET), str(e.exception))
                self.assertTrue(os.path.isdir(os.path.join(cc, self.TARGET)))
                run("git", "-C", self.other, "worktree", "remove", "--force", nxt)
                run("git", "-C", self.other, "branch", "-D", "cc/" + refused)
        self._declare()
        for name, _refused, then, _entries in hops:
            with self.subTest(name + " declared"):
                cc = os.path.join(self.env.root, os.path.basename(self.other) + "-wt", name)
                nxt = self.make_cc(then, base="cc/" + name)
                self.spawn(then, cc=nxt, extra="continues: %s\n" % name)
                self.assertEqual(self.rec(name)["disposition"]["kind"], "continued")
                self.assertFalse(os.path.exists(cc))

    def test_source_work_beside_the_output_still_refuses_and_another_tasks_output_is_untouched(self):
        self._carry(declared=True)
        other = self._finished_with_output("zach-opus-vo", 3)
        done = self._finished_with_output("zach-opus-vd", 3)
        nxt = self.make_cc("zach-opus-vdn", base="cc/zach-opus-vd")
        self.spawn("zach-opus-vdn", cc=nxt, extra="continues: zach-opus-vd\n")
        self.assertFalse(os.path.exists(done))
        # Another task's workspace and its output are not that continuation's to touch.
        self.assertEqual(sorted(os.listdir(os.path.join(other, self.TARGET, "shared-build-v1/debug/deps"))),
                         ["unit-0000.rlib", "unit-0001.rlib"])
        # Source work beside the output: refused, named, and left where it is.
        cc = self._finished_with_output("zach-opus-vs", 3)
        with open(os.path.join(cc, "richos/mobile/conformance/verifier/src/lib.rs"), "a") as f:
            f.write("pub fn unfinished() {}\n")
        with open(os.path.join(cc, "richos/mobile/conformance/verifier/src/new.rs"), "w") as f:
            f.write("pub fn new() {}\n")
        nxt = self.make_cc("zach-opus-vsn", base="cc/zach-opus-vs")
        with self.assertRaises(ws.SpecError) as e:
            self.spawn("zach-opus-vsn", cc=nxt, extra="continues: zach-opus-vs\n")
        self.assertIn("uncommitted entr", str(e.exception))
        self.assertIn("verifier/src/lib.rs", str(e.exception))
        self.assertNotIn(self.TARGET, str(e.exception))
        self.assertTrue(os.path.isfile(os.path.join(cc, "richos/mobile/conformance/verifier/src/new.rs")))


class HuntV2_03_TheBudgetBoundsEveryCleanupStage(Base):
    """Hunt part 4 v2, V2-03: the shared deadline reached the retries and the
    proof, but process shutdown, container and test-instance cleanup, the
    300 s `git worktree remove` and branch deletion still ran on their own
    clocks, so a cleanup kept starting work after its caller's budget ended."""

    def _discarded(self, name):
        aid, path = self.spawn(name)
        self.finish(aid)
        rec = self.rec(name)
        rec["disposition"] = {"kind": "discarded", "at": ws.now(), "reason": "fixture only"}
        ws.save_agent(rec)
        return rec, path

    def test_v2_03_no_cleanup_stage_starts_after_the_deadline(self):
        rec, path = self._discarded("zach-opus-timev2")
        clock = [100.0]
        calls = []

        def slow_containers(paths):
            calls.append("containers")
            clock[0] += 10.0                                   # past the 105 deadline
            return {}

        def instances(paths, **kw):
            calls.append("test-instances")
            return {}
        with patch.object(ws, "now", side_effect=lambda: clock[0]), \
                patch.object(ws, "stop_containers", side_effect=slow_containers), \
                patch.object(ws, "stop_test_instances", side_effect=instances), \
                patch.object(ws, "remove_workspace", side_effect=AssertionError("removal started late")):
            ok = ws._delete(rec, ws.live_workspaces(rec), branches=True, why="v2-03",
                            processes={"stopped": [], "survivors": []}, deadline=105.0)
        self.assertFalse(ok)
        self.assertEqual(calls, ["containers"], "a stage started after the deadline")
        self.assertTrue(os.path.isdir(path))
        d = self.rec("zach-opus-timev2")["deletion"]
        self.assertIn("budget ran out", d["deferred"])
        self.assertEqual(d.get("attempts", 0), 0, "running out of time was counted as a failure")
        self.assertEqual([ok for _k, ok in ws.retry_due()], [True])   # with time, it finishes
        self.assertFalse(os.path.exists(path))

    def test_v2_03_no_branch_is_deleted_after_the_deadline(self):
        rec, path = self._discarded("zach-opus-timev3")
        clock = [100.0]
        real_remove = ws.remove_workspace

        def slow_remove(w, deadline=None):
            clock[0] += 10.0
            return real_remove(w)
        with patch.object(ws, "now", side_effect=lambda: clock[0]), \
                patch.object(ws, "stop_containers", return_value={}), \
                patch.object(ws, "stop_test_instances", return_value={}), \
                patch.object(ws, "remove_workspace", side_effect=slow_remove), \
                patch.object(ws, "delete_branch", side_effect=AssertionError("branch deleted late")):
            ok = ws._delete(rec, ws.live_workspaces(rec), branches=True, why="v2-03",
                            processes={"stopped": [], "survivors": []}, deadline=105.0)
        self.assertFalse(ok)
        self.assertFalse(os.path.exists(path))
        self.assertIn("worktree-agent-" + rec["agent_id"], branches(self.entity))
        self.assertEqual(self.rec("zach-opus-timev3")["deletion"].get("attempts", 0), 0)

    def test_v2_03_the_git_removal_gets_what_is_left_not_its_own_300_seconds(self):
        rec, path = self._discarded("zach-opus-timev4")
        seen = []
        real_git = ws.git

        def spy(repo, *args, **kw):
            if args[:2] == ("worktree", "remove"):
                seen.append(kw.get("timeout"))
            return real_git(repo, *args, **kw)
        with patch.object(ws, "git", side_effect=spy):
            self.assertTrue(ws.remove_workspace(ws.live_workspaces(rec)[0], deadline=ws.now() + 5)[0])
        self.assertEqual(len(seen), 1)
        self.assertLessEqual(seen[0], 5.0)

    def test_v2_03_a_land_past_its_deadline_stops_nothing(self):
        aid, npath = self.spawn("zach-opus-timev5")
        self.commit(npath)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        with patch.object(ws, "stop_processes", side_effect=AssertionError("stopped past the deadline")):
            with self.assertRaises(ws.Deadline):
                ws.land("zach-opus-timev5", self.sid, auto=True, deadline=ws.now() - 1)
        self.assertTrue(os.path.isdir(npath))

    def test_v2_03_a_process_listing_that_did_not_finish_is_unknown_not_none(self):
        aid, npath = self.spawn("zach-opus-timev6")
        with patch.object(ws, "_process_cwds", return_value=None):
            res = ws.stop_processes([npath], deadline=ws.now() + 5)
        self.assertTrue(res.get("unknown"))
        self.assertEqual(res["stopped"], [])


class HuntV2_04_ADamagedRecordIsNotAMissingOne(Base):
    """Hunt part 4 v2, V2-04: read_json answered None for a malformed record
    exactly as for an absent one, so the sweep registered a LIVE worker's
    workspace as an orphan ("finished work of an ended session"), stopped its
    process and deleted it."""

    def _live_worker_with_a_damaged_record(self, name):
        aid, path = self.spawn(name)
        rec = self.rec(name)
        self.assertFalse(ws.finished_state(rec)[0])
        sleeper = subprocess.Popen(["sleep", "300"], cwd=path)
        self.env.procs.append(sleeper)
        with open(ws.agent_path(rec["key"]), "w") as f:
            f.write("{")
        self.assertIsNone(ws.load_agent(rec["key"]))
        return aid, path, sleeper

    def test_v2_04_a_sweep_never_cleans_up_a_workspace_whose_record_is_damaged(self):
        _aid, path, sleeper = self._live_worker_with_a_damaged_record("zach-opus-jsonv2")
        items = ws.pending(self.sid, self.entity, scan=True)
        time.sleep(0.3)
        self.assertIsNone(sleeper.poll(), "the sweep stopped a live worker's process")
        self.assertTrue(os.path.isdir(path), "the sweep deleted a live worker's workspace")
        self.assertFalse([r for r in ws.all_agents() if r.get("orphan")],
                         "the damaged record's workspace was registered as an orphan")
        damaged = [i for i in items if i.get("damaged")]
        self.assertEqual(len(damaged), 1, items)
        self.assertIn(ws.agent_path(ws.named_key(self.sid, "zach-opus-jsonv2")), damaged[0]["why"])
        self.assertFalse(damaged[0]["blocks_turn_end"])

    def test_v2_04_status_names_the_damaged_record(self):
        import io
        from contextlib import redirect_stdout
        _aid, path, sleeper = self._live_worker_with_a_damaged_record("zach-opus-jsonv3")
        buf = io.StringIO()
        with redirect_stdout(buf):
            ws._print_status(self.sid, self.entity)
        self.assertIn("DAMAGED  damaged-record-", buf.getvalue())
        self.assertNotIn("UNREGISTERED", buf.getvalue())
        self.assertIsNone(sleeper.poll())

    def test_v2_04_once_repaired_an_unregistered_workspace_is_still_found(self):
        """The twin: the rule that cleans up truly unregistered work still runs
        when every record reads, so the guard is not "never sweep"."""
        _aid, path, sleeper = self._live_worker_with_a_damaged_record("zach-opus-jsonv4")
        sleeper.kill()
        sleeper.wait()
        os.unlink(ws.agent_path(ws.named_key(self.sid, "zach-opus-jsonv4")))   # gone, not damaged
        self.assertEqual(ws.damaged_records(), [])
        ws.pending(self.sid, self.entity, scan=True, auto=False)
        self.assertTrue([r for r in ws.all_agents() if r.get("orphan")])


class AnyRegistryOnThisMachineCounts(Base):
    """2026-10-28: a terminal session's Stop gate swept with only its own
    registry (~/.claude/state/workspaces), called the app's live worker
    worktrees unregistered — the app keeps them in its own per-conversation
    registry, <app data>/engine-state/workspaces/<hash> — and its automatic
    land stopped a worker's processes and deleted three worktrees. Here
    registry B is the terminal's (this sandbox's CLAUDE_CONFIG_DIR) and
    registry A is an app conversation's, under this sandbox's HOME exactly
    where the app keeps it."""

    APP_PARTITION = "f" * 64

    def app_registry(self):
        # Where the app keeps it (Tauri's app_data_dir for com.richos.app), under this sandbox's HOME.
        return os.path.join(self.env.home, "Library", "Application Support", "com.richos.app",
                            "engine-state", "workspaces", self.APP_PARTITION)

    def app_worker(self, name):
        """A cc/ worktree registered ONLY in registry A, aged past SPAWN_WINDOW,
        with a live child process in it."""
        path = os.path.join(self.env.root, "app-target-worktrees", name)
        branch = "cc/" + name
        app_sid = "app-sess-bbbbbbbb-2222"
        os.environ["RICHOS_WORKSPACES_DIR"] = self.app_registry()
        try:
            ws.record_integration(self.entity, "main", "the app conversation's body of work", app_sid)
            ws.register_cc(app_sid, name, self.entity, path, branch)
            run("git", "-C", self.entity, "worktree", "add", "-q", path, "-b", branch)
            ws.confirm_cc(app_sid, name, path, True)
            self.assertTrue([r for r in ws.all_agents() if r.get("name") == name], "not registered in A")
        finally:
            os.environ.pop("RICHOS_WORKSPACES_DIR", None)
        self.assertNotIn(path, [w["path"] for r in ws.all_agents(include_done=True)
                                for w in r.get("workspaces") or []], "registered in B too")
        # Aged past the real spawn window, read from git's own record of it.
        admin = ws._admin_dir_for(self.entity, path)
        old = time.time() - 3600
        os.utime(os.path.join(admin, "commondir"), (old, old))
        sleeper = subprocess.Popen(["sleep", "300"], cwd=path)
        self.env.procs.append(sleeper)
        return path, branch, sleeper

    def assert_survives(self, path, branch, sleeper):
        time.sleep(0.3)
        self.assertIsNone(sleeper.poll(), "the gate stopped a process in another registry's worktree")
        self.assertTrue(os.path.isdir(path), "the gate deleted another registry's worktree")
        self.assertIn(path, worktrees(self.entity))
        self.assertIn(branch, branches(self.entity), "the gate deleted another registry's branch")

    def test_a_worktree_only_the_app_registry_holds_survives_a_terminal_gate_stop(self):
        path, branch, sleeper = self.app_worker("ray-sonnet-app0001")
        with patch.object(ws, "SPAWN_WINDOW", 120.0):
            ok, _person, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assert_survives(path, branch, sleeper)
        self.assertEqual([r["key"] for r in ws.all_agents(include_done=True) if r.get("orphan")], [],
                         "an orphan record was made for a worktree registry A holds")

    def test_an_open_orphan_record_another_registry_holds_is_settled_with_nothing_deleted(self):
        path, branch, sleeper = self.app_worker("newt-opus-app0002")
        # The armed record exactly as the base sweep wrote it, claimed by this session.
        key = "orphan--armedarmedarmed1"
        rec = ws.new_record(key, name="orphan-newt-opus-app0002", orphan=True)
        ws._add_workspace(rec, "cc", self.entity, path, branch, "unregistered")
        rec["claimed_by"] = self.sid
        ws.save_agent(rec)
        # A question first: the dry status lists it nowhere and writes nothing.
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with patch.object(ws, "SPAWN_WINDOW", 120.0), redirect_stdout(buf):
            ws._print_status(self.sid, self.entity)
        self.assertNotIn("orphan-", buf.getvalue())
        self.assertIsNotNone(ws.read_json(ws.agent_path(key)), "the dry status wrote")
        # Then the gate settles it.
        with patch.object(ws, "SPAWN_WINDOW", 120.0):
            ok, _person, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assert_survives(path, branch, sleeper)
        self.assertIsNone(ws.load_agent(key), "the armed orphan record is still open (or was landed)")
        self.assertEqual([r["key"] for r in ws.all_agents(include_done=True) if r.get("orphan")], [])
        with open(ws._p("events.jsonl"), encoding="utf-8") as f:
            evs = [json.loads(line) for line in f if line.strip()]
        self.assertTrue([e for e in evs if e.get("event") == "unregistered-withdrawn" and e.get("key") == key],
                        "the withdrawal was not recorded")
        self.assertFalse([e for e in evs if e.get("event") == "landed" and e.get("key") == key])

    def test_a_truly_unregistered_worktree_is_still_found(self):
        """The twin: a worktree no registry holds is still the sweep's, so the
        rule is not "never sweep when an app registry exists"."""
        self.app_worker("echo-sonnet-app0003")       # A exists and holds something else
        stray = os.path.join(self.env.root, "stray-wt")
        run("git", "-C", self.entity, "worktree", "add", "-q", stray, "-b", "cc/stray")
        ws.pending(self.sid, self.entity, scan=True, auto=False)
        self.assertEqual([r["name"] for r in ws.all_agents() if r.get("orphan")], ["orphan-stray-wt"])


class HuntV2_05_StatusReadsTheRecordedStop(Base):
    """Hunt part 4 v2, V2-05: the dry status (finding 29) stopped adopting the
    platform's own record of a stop and then asked the stale registry record,
    so a worker the user had stopped printed WORKING beside "pending: none"."""

    def _stopped_by_user(self, name):
        aid, npath = self.spawn(name)
        self.commit(npath, "unfinished.txt")                  # unmerged: it cannot land
        meta = os.path.join(ws._platform_projects_dir(), "fixture", self.sid, "subagents",
                            "agent-%s.meta.json" % aid)
        os.makedirs(os.path.dirname(meta), exist_ok=True)
        with open(meta, "w") as f:
            json.dump({"stoppedByUser": True, "name": name, "toolUseId": "tu-" + name}, f)
        return aid, npath

    def test_v2_05_status_shows_a_stopped_worker_as_pending_and_writes_nothing(self):
        import io
        from contextlib import redirect_stdout
        self._stopped_by_user("zach-opus-stv2")
        before = json.dumps(self.rec("zach-opus-stv2"), sort_keys=True)
        buf = io.StringIO()
        with redirect_stdout(buf):
            ws._print_status(self.sid, self.entity)
        out = buf.getvalue()
        self.assertNotIn("pending: none", out)
        self.assertIn("PENDING  zach-opus-stv2", out)
        self.assertNotIn("WORKING  zach-opus-stv2", out)
        # A question: the stop is read, not adopted.
        self.assertEqual(json.dumps(self.rec("zach-opus-stv2"), sort_keys=True), before)
        self.assertIsNone(self.rec("zach-opus-stv2")["end"])
        # The live reconciliation still adopts the same fact and lists the same work.
        self.assertEqual([i["name"] for i in ws.pending(self.sid, self.entity, auto=False)],
                         ["zach-opus-stv2"])
        self.assertEqual(self.rec("zach-opus-stv2")["end"]["signal"], "stopped")

    def test_r2_a_stop_only_the_platform_recorded_offers_only_shelving_and_discard(self):
        """Review rv-20261024T033729Z-cfc3e0e1-4e49 #1: the watcher's dry read saw the platform's
        stop, but due_set reloaded the stored record and offered "land it ... or spawn its
        continuation" for work he had stopped. Nothing is adopted by the question."""
        self._stopped_by_user("zach-opus-stv3")
        due = [d for d in ws.due_set(self.sid, ws.pending(self.sid, self.entity, auto=False, dry=True))
               if d["name"] == "zach-opus-stv3"]
        self.assertEqual(len(due), 1, due)
        self.assertIn("shelved/zach-opus-stv3", due[0]["command"])
        self.assertNotIn("land it", due[0]["command"])
        self.assertNotIn("spawn its continuation", due[0]["command"])
        self.assertIsNone(self.rec("zach-opus-stv3")["end"])

    def test_v2_05_a_running_worker_is_still_working(self):
        import io
        from contextlib import redirect_stdout
        aid, npath = self.spawn("zach-opus-run2")
        self.commit(npath, "unfinished.txt")
        buf = io.StringIO()
        with redirect_stdout(buf):
            ws._print_status(self.sid, self.entity)
        self.assertIn("pending: none", buf.getvalue())
        self.assertIn("WORKING  zach-opus-run2", buf.getvalue())


class _Result(unittest.TextTestResult):
    """Prints `  PASS  <test>` / `  FAIL  <test>` so the mutation harness
    (workspaces.mutation.sh) can tell which point went red."""

    def addSuccess(self, test):
        super().addSuccess(test)
        self.stream.write("  PASS  %s\n" % test._testMethodName)

    def addFailure(self, test, err):
        super().addFailure(test, err)
        self.stream.write("  FAIL  %s\n" % test._testMethodName)

    def addError(self, test, err):
        super().addError(test, err)
        self.stream.write("  FAIL  %s (error)\n" % test._testMethodName)


def _resolve_names(names, loader):
    """A class name or `Class.test` passes through. A bare method name, or the start of
    one, selects every test method it begins -- the same set whose `  FAIL  <name>` line
    the mutation harness's `grep "FAIL  <want>"` would match, so a mutant focused on its
    want runs exactly the cases that could prove it (mutation_focus want-as-argument).
    A name that selects nothing is passed through for the loader to refuse loudly."""
    module = sys.modules[__name__]
    cases = [c for c in vars(module).values()
             if isinstance(c, type) and issubclass(c, unittest.TestCase)]
    out = []
    for n in names:
        if "." in n or hasattr(module, n):
            out.append(n)
            continue
        hits = ["%s.%s" % (c.__name__, m) for c in cases
                for m in loader.getTestCaseNames(c) if m.startswith(n)]
        out.extend(hits or [n])
    return out


class Hunt4_TurnEndPauseAndPathChecks(Base):
    """Part 4 of the 2026-09-29 hunt, findings 8, 9 and 10."""

    def test_hunt4_09_a_pause_addressed_by_agent_id_operates_the_pause_record(self):
        aid, _n = self.spawn("zach-opus-pid")
        self.assertEqual(ws.recipient_state(self.sid, aid)[0], "active")
        ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "SendMessage", "session_id": self.sid,
                      "tool_input": {"to": aid, "message": "commit and hold\npause-until: the CEO's answer"}},
                     self.entity)
        self.assertEqual(ws.finished_state(self.rec("zach-opus-pid"))[:2], (False, True))
        self.assertEqual(ws.recipient_state(self.sid, aid)[0], "paused")
        ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "SendMessage", "session_id": self.sid,
                      "tool_input": {"to": aid, "message": "go on"}}, self.entity)
        self.assertEqual(ws.finished_state(self.rec("zach-opus-pid"))[:2], (False, False))

    def test_hunt4_10_a_relative_edit_path_is_checked_from_the_payloads_working_folder(self):
        cx = os.path.join(self.env.root, "codex-wt10")
        run("git", "-C", self.entity, "worktree", "add", "-q", cx, "-b", "codex/live10")
        aid, npath = self.spawn("zach-opus-rel")
        here = os.getcwd()
        os.chdir(self.env.root)                  # the hook process runs somewhere else entirely
        try:
            self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Edit", "cwd": cx,
                                         "tool_input": {"file_path": "README"}})[0], "CODEX")
            self.assertEqual(ws.barrier({"session_id": self.sid, "agent_id": aid, "tool_name": "Edit", "cwd": npath,
                                         "tool_input": {"file_path": "README"}})[0], "REGISTERED")
        finally:
            os.chdir(here)


class Hunt4_MutationHarnessesDeclareTheirFocus(unittest.TestCase):
    """Part 4 (2026-09-29 hunt), finding 20: the app, creator and probe-runner
    harnesses never declared a mutation_focus, so every one of their mutants
    reran its whole suite to judge one named case."""

    def test_hunt4_20_each_mutation_harness_of_this_folder_declares_how_its_mutants_are_focused(self):
        import re
        want = {"app.mutation.sh": "want-as-argument", "create-teammate-worktree.mutation.sh": "stop-at-want",
                "workspace-probes.mutation.sh": "stop-at-want", "workspaces.mutation.sh": "want-as-argument",
                "workspace-spec-fourteen.mutation.sh": "stop-at-want"}
        for name, mode in sorted(want.items()):
            with open(os.path.join(HERE, name), encoding="utf-8") as f:
                src = f.read()
            declared = re.findall(r"(?m)^mutation_focus\s+(\S+)\s*$", src)
            self.assertEqual(declared, [mode], "%s declares %r, expected [%r]" % (name, declared, mode))
            self.assertLess(src.index("mutation_begin "), src.index("mutation_focus "),
                            "%s declares its focus before mutation_begin" % name)

    def test_hunt4_20_the_app_suite_takes_case_names_so_its_mutants_can_run_one_case(self):
        r = subprocess.run([sys.executable, "-B", "-W", "ignore", os.path.join(HERE, "app.test.py"),
                            "test_pause_message_is_fixed"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("PASS  test_pause_message_is_fixed_and_does_not_claim_delivery", r.stdout)
        self.assertIn("1 run, 0 failed", r.stdout)


class SeverityThreeGroupJ(Base):
    """Hunt V2-08 to V2-11. A Base, not a Point09_NeverWritesAgain: that
    subclassing (for term_ignoring_holder, now on Base) ran Point09's six
    tests a second time (hunt part 4 v3, V3-07)."""

    def test_v2_08_top_level_ignored_link_is_compared_by_target(self):
        with open(os.path.join(self.other, ".gitignore"), "a") as f:
            f.write("my-link\n")
        run("git", "-C", self.other, "commit", "-qam", "ignore my-link")
        for n in ("a.txt", "b.txt"):
            with open(os.path.join(self.env.root, n), "w") as f:
                f.write("same bytes\n")
        os.symlink(os.path.join(self.env.root, "a.txt"), os.path.join(self.other, "my-link"))
        path = self.make_cc("zach-opus-v208")
        os.symlink(os.path.join(self.env.root, "b.txt"), os.path.join(path, "my-link"))
        aid, _n = self.spawn("zach-opus-v208", path, native=False)
        self.finish(aid)
        with self.assertRaises(ws.SpecError) as e:
            ws.land("zach-opus-v208", self.sid)
        self.assertIn("my-link", str(e.exception))

    def test_v2_09_late_ending_does_not_recreate_a_live_record(self):
        aid, npath = self.spawn("zach-opus-v209")
        self.finish(aid)
        ws.land("zach-opus-v209", self.sid)
        key = ws.named_key(self.sid, "zach-opus-v209")
        self.assertFalse(os.path.exists(ws.agent_path(key)))
        ws.record_end(self.sid, aid, "SubagentStop")
        self.assertFalse(os.path.exists(ws.agent_path(key)), "a late ending wrote into agents/")
        self.assertTrue(os.path.exists(ws.done_path(key)))

    def test_v2_10_land_command_says_when_cleanup_is_unfinished(self):
        import contextlib
        import io
        aid, npath = self.spawn("zach-opus-v210")
        self.finish(aid)
        buf = io.StringIO()
        with patch.object(ws, "land", lambda *a, **k: {"landed": True, "cleanup_pending": True}), \
                patch.object(ws, "sweep_scratch_after_land", lambda *a, **k: None):
            with contextlib.redirect_stdout(buf):
                ws.main(["--session", self.sid, "land", "zach-opus-v210"])
        self.assertIn("NOT finished", buf.getvalue())
        self.assertNotIn("or retrying", buf.getvalue())

    def test_v2_11_process_stop_history_records_start_times(self):
        d, pr = self.term_ignoring_holder()
        res = ws.stop_processes([d])
        self.assertEqual(res["stopped"], [pr.pid])
        pr.wait(timeout=30)
        with open(ws._p("events.jsonl")) as f:
            evs = [json.loads(l) for l in f if '"processes-stopped"' in l]
        self.assertTrue(evs[-1].get("starts", {}).get(str(pr.pid)), evs[-1])


class AutoLand_MergedAndEndedLandsOnItsOwn(Base):
    """2026-10-04: four teammates' workspaces sat 1-2 days after their work
    was merged and their agents had ended. The automatic land refused them on
    ignored files the main checkout lacked (in-flight ack files, build output),
    pending() swallowed the refusal, and a stale "started" wait note kept them
    off every gate's list. A merged, ended workspace now lands on its own,
    whatever wait note it carries: the ignored files are KEPT, never lost."""

    def _finished_with_ignored_file(self, name):
        cc = self.make_cc(name)
        aid, npath = self.spawn(name, cc=cc)
        self.commit(cc)
        os.makedirs(os.path.join(cc, "build"))
        with open(os.path.join(cc, "build", "inflight.ack"), "w") as f:
            f.write("ack of main at abc123\n")
        self.finish(aid)
        return cc, npath

    def test_a_merged_ended_workspace_with_a_stale_wait_note_lands_on_its_own(self):
        cc, npath = self._finished_with_ignored_file("zach-opus-al")
        ws.wait("zach-opus-al", "started", "merging now (land chain 5, one merge)", "", self.sid, SOON())
        self.merge(self.other, "cc/zach-opus-al")
        ok, msg, _lead = ws.gate_stop({"session_id": self.sid}, self.entity)     # the lead's turn ends
        self.assertTrue(ok, msg)
        self.assertFalse(os.path.exists(cc), "the merged, ended cc/ workspace is still there")
        self.assertFalse(os.path.exists(npath), "its native workspace is still there")
        self.assertNotIn("cc/zach-opus-al", branches(self.other))
        d = self.rec("zach-opus-al")["disposition"]
        self.assertEqual((d["kind"], d["auto"]), ("landed", True))
        kept = [os.path.join(r, f) for r, _d, fs in os.walk(d["kept_ignored"]) for f in fs]
        self.assertEqual([open(p).read() for p in kept], ["ack of main at abc123\n"])

    def test_a_merge_of_a_finished_teammate_lands_it(self):
        """`workspaces.sh merge <name>` is the land command: after the merge of
        a finished teammate's branch its workspaces and branches are gone."""
        cc, npath = self._finished_with_ignored_file("zach-opus-mg")
        tip = run("git", "-C", self.other, "rev-parse", "cc/zach-opus-mg").stdout.strip()
        merged, res = ws.merge_and_land("zach-opus-mg", self.sid, "Merge cc/zach-opus-mg: the work")
        self.assertEqual(merged, [(self.other, "cc/zach-opus-mg")])
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        self.assertIn("Merge cc/zach-opus-mg: the work",
                      run("git", "-C", self.other, "log", "-1", "--format=%s").stdout)
        self.assertFalse(os.path.exists(cc) or os.path.exists(npath))
        self.assertNotIn("cc/zach-opus-mg", branches(self.other))
        self.assertEqual(self.names(), [])

    def test_a_running_teammate_is_not_merged(self):
        cc = self.make_cc("zach-opus-mr")
        self.spawn("zach-opus-mr", cc=cc)
        self.commit(cc)
        head = run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip()
        with self.assertRaises(ws.SpecError):
            ws.merge_and_land("zach-opus-mr", self.sid)
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip(), head)
        self.assertTrue(os.path.isdir(cc))

    def test_uncommitted_work_still_holds_the_land_and_says_why(self):
        cc, _npath = self._finished_with_ignored_file("zach-opus-ud")
        with open(os.path.join(cc, "left.txt"), "w") as f:
            f.write("uncommitted\n")
        self.merge(self.other, "cc/zach-opus-ud")
        item = [i for i in ws.pending(self.sid, self.entity) if i["name"] == "zach-opus-ud"]
        self.assertEqual(len(item), 1)
        self.assertIn("did not land on its own", item[0]["why"])
        self.assertIn("uncommitted", item[0]["why"])
        self.assertTrue(os.path.isfile(os.path.join(cc, "build", "inflight.ack")))  # nothing moved

    def test_an_ack_is_never_kept_and_the_reaper_deletes_a_kept_copy_after_7_days(self):
        """§54 (2026-10-04): what the automatic land keeps goes away by itself.
        An in-flight ack file (scripts/inflight-ack.sh writes it under
        .claude/inflight-acks/) is not kept at all; anything else kept is
        deleted by the scratch reaper once older than SCRATCH_KEPT_RETENTION_DAYS."""
        with open(os.path.join(self.other, ".gitignore"), "a") as f:
            f.write(".claude/\n")
        run("git", "-C", self.other, "commit", "-q", "-am", "ignore .claude/")
        os.makedirs(os.path.join(self.other, ".claude"))          # every real main checkout has one
        cc = self.make_cc("zach-opus-kr")
        aid, _npath = self.spawn("zach-opus-kr", cc=cc)
        self.commit(cc)
        os.makedirs(os.path.join(cc, ".claude", "inflight-acks"))
        with open(os.path.join(cc, ".claude", "inflight-acks", "abc123abc123.zach-opus-kr.ack"), "w") as f:
            f.write("ack of main at abc123\n")
        os.makedirs(os.path.join(cc, "build"))
        with open(os.path.join(cc, "build", "notes.txt"), "w") as f:
            f.write("the only copy of the agent's notes\n")
        self.finish(aid)
        _merged, res = ws.merge_and_land("zach-opus-kr", self.sid)
        self.assertTrue(res["landed"])
        kept = res["kept_ignored"]
        found = sorted(os.path.relpath(os.path.join(r, f), kept) for r, _d, fs in os.walk(kept) for f in fs)
        self.assertEqual(len(found), 1, found)                    # the ack is not kept
        self.assertTrue(found[0].endswith(os.path.join("build", "notes.txt")), found)
        self.assertFalse(os.path.exists(cc))

        spec = importlib.util.spec_from_file_location(
            "scratch_reaper", os.path.join(HERE, "..", "..", "scripts", "lib", "scratch-reaper.py"))
        reaper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reaper)
        kept_dir = os.path.realpath(ws._p("kept"))

        def sweep():
            r = object.__new__(reaper.Reaper)
            r.cfg = {"kept_dir": kept_dir, "kept_retention_days": 7}
            r.entries, r.roots, r.scope, r.unattributed, r.now = [], [], None, 0, time.time()
            r.scan_kept(reaper.Walls([kept_dir]))
            # Only what this test planned: no test instances, devices, Docker or failure file.
            r.collect_test_instances = lambda *_a: []
            r.collect_test_devices = lambda *_a: []
            r.prune_docker = lambda: []
            r._record_failures = lambda *_a: None
            r.apply(os.path.join(self.env.root, "reaper.log"))
            return [(e.path, e.action) for e in r.entries]

        real = os.path.realpath(kept)
        self.assertEqual(sweep(), [(real, reaper.KEEP)])          # 0 days old: kept
        self.assertTrue(os.path.isdir(kept))
        old = time.time() - 8 * 86400
        os.utime(kept, (old, old))
        self.assertEqual(sweep(), [(real, reaper.DELETE)])        # 8 days old: deleted
        self.assertFalse(os.path.exists(kept))


class CarriedWait_EndsWhenItsCarrierLands(Base):
    """2026-10-11: zach-opus-pierce3 had a workspace in two repositories and a
    wait "carried by zach-opus-pierce4, lands with it". pierce4 carried only
    the first repository's branch and landed; the wait outlived it, so the
    second repository's branch sat unlanded and off every gate for ~40 h.
    When an agent lands, a wait that names it ends unless all of the waiting
    agent's work landed with it, and the gate names the branch still out."""

    def test_a_wait_naming_the_lander_ends_and_the_branch_left_out_is_named(self):
        cc = self.make_cc("zach-opus-ca")                        # repository one: self.other
        aid_a, npath_a = self.spawn("zach-opus-ca", cc=cc)        # repository two: self.entity
        self.commit(cc, "first.txt")
        self.commit(npath_a, "second.txt")
        self.finish(aid_a)
        ws.wait("zach-opus-ca", "started", "carried by zach-opus-cb, lands with it", "", self.sid,
                "agent:zach-opus-cb")
        aid_b, npath_b = self.spawn("zach-opus-cb", extra="lands-pending: zach-opus-ca\n")
        self.commit(npath_b, "carrier.txt")
        self.finish(aid_b)
        self.merge(self.other, "cc/zach-opus-ca")                  # B carries only A's first repository
        self.merge(self.entity, "worktree-agent-" + aid_b)
        self.assertTrue(ws.land("zach-opus-cb", self.sid)["landed"])
        item = [i for i in ws.pending(self.sid, self.entity) if i["name"] == "zach-opus-ca"]
        self.assertEqual(len(item), 1)
        self.assertEqual(item[0]["waiting"], "")                   # the wait ended with B's land
        self.assertTrue(item[0]["blocks_turn_end"])
        self.assertIn("worktree-agent-" + aid_a, item[0]["why"])   # the branch still out is named
        ok, _msg, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertIn("zach-opus-ca", lead)
        self.assertIn("worktree-agent-" + aid_a, lead)

    def _gate_pass_that_lands_the_carrier(self, first):
        """Review rv-20261011T230453Z-7b7f2a33-fec0: pending() built A's item
        from its copy taken before it landed B automatically, so that gate
        still read A's ended wait and let the turn end; only the next gate
        blocked. `first` is the record all_agents() lists first."""
        cc = self.make_cc("zach-opus-ca")
        aid_a, npath_a = self.spawn("zach-opus-ca", cc=cc)
        self.commit(cc, "first.txt")
        self.commit(npath_a, "second.txt")
        self.finish(aid_a)
        ws.wait("zach-opus-ca", "started", "carried by zach-opus-cb, lands with it", "", self.sid,
                "agent:zach-opus-cb")
        aid_b, npath_b = self.spawn("zach-opus-cb", extra="lands-pending: zach-opus-ca\n")
        self.commit(npath_b, "carrier.txt")
        self.finish(aid_b)
        self.merge(self.other, "cc/zach-opus-ca")
        self.merge(self.entity, "worktree-agent-" + aid_b)
        real_all_agents = ws.all_agents

        def ordered(include_done=False):
            return sorted(real_all_agents(include_done), key=lambda r: r.get("name") != first)

        with patch.object(ws, "all_agents", ordered):
            ok, _msg, msg = ws.gate_stop({"session_id": self.sid}, self.entity)
        recs = {r.get("name"): r for r in real_all_agents(True)}
        self.assertEqual(recs["zach-opus-cb"]["disposition"]["kind"], "landed")   # B landed in this pass
        self.assertIsNone(recs["zach-opus-ca"].get("waiting"))
        self.assertTrue(ok, msg)                                                   # and this same gate tells it
        self.assertIn("zach-opus-ca", msg)
        self.assertIn("worktree-agent-" + aid_a, msg)                              # naming A's branch still out
        self.assertNotIn("carried by zach-opus-cb", msg)

    def test_the_gate_pass_that_lands_the_carrier_refuses_waiter_listed_first(self):
        self._gate_pass_that_lands_the_carrier("zach-opus-ca")

    def test_the_gate_pass_that_lands_the_carrier_refuses_carrier_listed_first(self):
        self._gate_pass_that_lands_the_carrier("zach-opus-cb")


class MergeGateRefusal_LeavesTheMainCheckoutAsItWas(Base):
    """2026-10-07: three of Rich's lands (merge-echo-cc292b, merge-retry1,
    merge-avskills1) ended in `=== MERGE INTO MAIN REFUSED ===` and git's "Not
    committing merge", and `workspaces.sh merge` exited leaving MERGE_HEAD set
    and the branch's files staged in the main checkout. His hand-typed
    `git merge --abort` then met the operator fence without the lease, which
    refused it after git had already rewritten the index. A refused land now
    aborts the merge it started, inside the same process tree that holds the
    lease, so the fence accepts the abort."""

    def test_a_gate_refusal_under_the_fence_leaves_no_merge_and_exits_refused(self):
        name = "zach-opus-ma"
        cc = self.make_cc(name)
        aid, _npath = self.spawn(name, cc=cc)
        self.commit(cc)
        self.finish(aid)
        engine = os.path.realpath(os.path.join(HERE, "..", ".."))
        # The fence, switched ON for this repository, as it is on the operator's Mac.
        decl = os.path.join(self.env.root, "fence-entity")
        os.makedirs(decl)
        with open(os.path.join(decl, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n')
        fences = os.path.join(engine, "scripts", "operator-fences.sh")
        run("bash", fences, "install", "--repo", self.other, "--entity", decl)
        run("bash", fences, "on", "--repo", self.other, "--entity", decl)
        launcher = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        with open(launcher) as f:
            self.assertIn('OPERATOR_FENCES_STATE="on"', f.read())
        # This test process is the Claude session that takes the land lease, as
        # Rich's session does before `workspaces.sh merge`.
        start = subprocess.run(["ps", "-o", "lstart=", "-p", str(os.getpid())], capture_output=True,
                               text=True, env=dict(os.environ, TZ="UTC0", LC_ALL="C")).stdout
        sessions = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "sessions")
        os.makedirs(sessions, exist_ok=True)
        with open(os.path.join(sessions, "%d.json" % os.getpid()), "w") as f:
            json.dump({"pid": os.getpid(), "sessionId": self.sid, "cwd": self.entity,
                       "procStart": " ".join(start.split()), "kind": "interactive"}, f)
        # The running session's own CLAUDE_PID would name another process.
        env = {k: v for k, v in os.environ.items() if k not in ("CLAUDE_PID", "CLAUDE_CODE_ENTRYPOINT")}
        r = subprocess.run(["bash", os.path.join(engine, "scripts", "land-lease.sh"), "acquire", "--repo",
                            self.other], capture_output=True, text=True, env=env)
        self.assertIn("ACQUIRED", r.stdout, r.stdout + r.stderr)
        # The merge gate: git's own pre-merge-commit check refuses, as proof-run did.
        hook = os.path.join(self.other, ".git", "hooks", "pre-merge-commit")
        with open(hook, "w") as f:
            f.write("#!/bin/sh\necho '=== MERGE INTO MAIN REFUSED: a check it owns failed ==='\nexit 1\n")
        os.chmod(hook, 0o755)
        head = run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip()

        r = subprocess.run([sys.executable, LIB, "--session", self.sid, "merge", name],
                           capture_output=True, text=True, env=env)

        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("REFUSED", r.stderr)
        self.assertIn("MERGE INTO MAIN REFUSED", r.stderr)
        merge_head = run("git", "-C", self.other, "rev-parse", "-q", "--verify", "MERGE_HEAD", check=False)
        self.assertNotEqual(merge_head.returncode, 0, "a merge is still in progress: MERGE_HEAD is set")
        self.assertEqual(run("git", "-C", self.other, "status", "--porcelain").stdout, "")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip(), head)
        # The fence saw both writes (the merge's and the abort's) and refused neither.
        home = os.path.dirname(ws.land_lock_path(self.other))
        self.assertFalse(os.path.exists(os.path.join(home, "fence-refusals.jsonl")))
        # Nothing was landed: the work and its branch are still there.
        self.assertTrue(os.path.isdir(cc))
        self.assertIn("cc/" + name, branches(self.other))


class MergeTakesTheLandLeaseItself(Base):
    """2026-10-14: `workspaces.sh merge echo-opus-hiresnag55b` was refused by
    the operator fence, "this process does not hold the land lease. The land
    lease for this repository is free.", and the lead ran `land-lease.sh
    acquire` by hand before every merge after it. The merge now takes a free
    lease itself and releases it when it is done; a lease its caller already
    holds stays the caller's; a lease another holder has is waited for, and
    the refusal names that holder."""

    def setUp(self):
        super().setUp()
        decl = os.path.join(self.env.root, "fence-entity")
        os.makedirs(decl)
        with open(os.path.join(decl, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n')
        fences = os.path.join(ENGINE, "scripts", "operator-fences.sh")
        run("bash", fences, "install", "--repo", self.other, "--entity", decl)
        run("bash", fences, "on", "--repo", self.other, "--entity", decl)
        self.sessions = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "sessions")
        os.makedirs(self.sessions, exist_ok=True)
        self.session_record(os.getpid())            # this test process is the lead's session
        self.envx = {k: v for k, v in os.environ.items() if k not in ("CLAUDE_PID", "CLAUDE_CODE_ENTRYPOINT")}
        self.lease_cmd = os.path.join(ENGINE, "scripts", "land-lease.sh")
        self.home = os.path.dirname(ws.land_lock_path(self.other))

    def session_record(self, pid):
        start = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True,
                               text=True, env=dict(os.environ, TZ="UTC0", LC_ALL="C")).stdout
        with open(os.path.join(self.sessions, "%d.json" % pid), "w") as f:
            json.dump({"pid": pid, "sessionId": self.sid, "cwd": self.entity,
                       "procStart": " ".join(start.split()), "kind": "interactive"}, f)

    def finished(self, name):
        cc = self.make_cc(name)
        aid, _npath = self.spawn(name, cc=cc)
        self.commit(cc)
        self.finish(aid)
        return run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip()

    def merge_cmd(self, name, **extra):
        return subprocess.run([sys.executable, LIB, "--session", self.sid, "merge", name],
                              capture_output=True, text=True, env=dict(self.envx, **extra))

    def leases(self):
        return [n for n in os.listdir(self.home) if n.endswith(".lease")]

    def landed(self, tip):
        return run("git", "-C", self.other, "merge-base", "--is-ancestor", tip, "main",
                   check=False).returncode == 0

    def test_a_merge_with_the_lease_free_takes_it_lands_and_releases_it(self):
        tip = self.finished("zach-opus-lf")
        self.assertEqual(self.leases(), [])
        r = self.merge_cmd("zach-opus-lf")
        said = r.stdout + r.stderr
        self.assertEqual(r.returncode, 0, said)
        self.assertNotIn("does not hold the land lease", said)
        self.assertIn("ACQUIRED", said)
        self.assertIn("RELEASED", said)
        self.assertTrue(self.landed(tip), said)
        self.assertEqual(self.leases(), [], "the lease this merge took was not released")
        self.assertFalse(os.path.exists(os.path.join(self.home, "fence-refusals.jsonl")))

    def test_a_lease_the_caller_already_holds_stays_its_own(self):
        tip = self.finished("zach-opus-lh")
        r = subprocess.run(["bash", self.lease_cmd, "acquire", "--repo", self.other],
                           capture_output=True, text=True, env=self.envx)
        self.assertIn("ACQUIRED", r.stdout, r.stdout + r.stderr)
        r = self.merge_cmd("zach-opus-lh")
        said = r.stdout + r.stderr
        self.assertEqual(r.returncode, 0, said)
        self.assertTrue(self.landed(tip), said)
        self.assertNotIn("RELEASED", said)
        self.assertEqual(len(self.leases()), 1, "the merge released a lease its caller had taken")

    def test_a_lease_another_holder_has_is_waited_for_and_named(self):
        tip = self.finished("zach-opus-lo")
        d = os.path.realpath(tempfile.mkdtemp(prefix="lease-other-", dir=self.env.root))
        took, go = os.path.join(d, "took"), os.path.join(d, "go")
        # Another Claude session, alive, holding the lease: a process with its
        # own session record that takes the lease as its child and stays. It
        # waits for its record to be written before it takes the lease.
        other = subprocess.Popen([sys.executable, "-c",
                                  "import os, subprocess, sys, time\n"
                                  "while not os.path.exists(sys.argv[4]): time.sleep(0.02)\n"
                                  "r = subprocess.run(['bash', sys.argv[1], 'acquire', '--repo', sys.argv[2]],"
                                  " capture_output=True, text=True)\n"
                                  "open(sys.argv[3], 'w').write(r.stdout + r.stderr)\n"
                                  "time.sleep(600)\n", self.lease_cmd, self.other, took, go],
                                 env=self.envx)
        self.env.procs.append(other)
        time.sleep(0.1)
        self.session_record(other.pid)
        open(go, "w").close()
        guard = time.monotonic() + 30
        while not os.path.exists(took):
            self.assertLess(time.monotonic(), guard, "the other holder never ran its acquire")
            time.sleep(0.05)
        self.assertIn("ACQUIRED", open(took).read())
        head = run("git", "-C", self.other, "rev-parse", "main").stdout.strip()
        r = self.merge_cmd("zach-opus-lo", LAND_LEASE_WAIT="2")
        said = r.stdout + r.stderr
        self.assertNotEqual(r.returncode, 0, said)
        self.assertIn("waiting for the land lease", said)
        self.assertIn("HELD", said)
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main").stdout.strip(), head)
        self.assertFalse(self.landed(tip))
        self.assertTrue(os.path.isdir(os.path.join(self.env.root, "other-wt", "zach-opus-lo")))


class DiscardTakesAWorkspaceWhoseTeammateNeverRan(Base):
    """2026-10-14: echo-opus-tap22's spawn was refused before its registration
    (its record has no tool_use_id, no agent id and no start), and `workspaces.sh
    discard echo-opus-tap22` answered "is not finished (its run has not
    ended)"; it took `workspaces.sh stop` first. A teammate that never ran has
    no run to end, so discard takes it directly. A spawn that WAS registered is
    still refused until its run ends: its agent may be running."""

    def test_a_workspace_whose_spawn_never_happened_is_discarded_directly(self):
        cc = self.make_cc("zach-opus-nr")
        r = ws.discard("zach-opus-nr", "never started: its spawn was refused",
                       not_ceo_ordered="a refused spawn, never an order of his", me=self.sid)
        self.assertTrue(r["discarded"])
        self.assertEqual(self.rec("zach-opus-nr")["disposition"]["kind"], "discarded")
        self.assertFalse(os.path.exists(cc))
        self.assertNotIn("cc/zach-opus-nr", branches(self.other))

    def test_a_registered_spawn_whose_run_has_not_ended_is_still_refused(self):
        cc = self.make_cc("zach-opus-ns")
        payload = {"session_id": self.sid, "tool_use_id": "tu-zach-opus-ns", "tool_name": "Agent",
                   "tool_input": {"name": "zach-opus-ns", "subagent_type": "zach", "isolation": "worktree",
                                  "prompt": "do it\ncross-repo-worktree: %s\n" % cc}}
        ws.register_spawn(payload, self.entity)
        with self.assertRaises(ws.SpecError) as e:
            ws.discard("zach-opus-ns", "no longer wanted at all", not_ceo_ordered="not his order, a test",
                       me=self.sid)
        self.assertIn("not finished", str(e.exception))
        self.assertTrue(os.path.isdir(cc))

    def lead_transcript(self, tuid, result=None):
        """The lead's transcript as the platform writes it: the Agent call, and
        (when given) the platform's answer to it."""
        path = os.path.join(self.env.root, "lead-%s.jsonl" % tuid)
        rows = [{"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": tuid, "name": "Agent", "input": {"name": "x"}}]}}]
        if result is not None:
            rows.append({"type": "user", "message": {"role": "user", "content": [
                dict({"type": "tool_result", "tool_use_id": tuid}, **result)]}})
        with open(path, "w") as f:
            f.write("".join(json.dumps(r) + "\n" for r in rows))
        return path

    def registered(self, name, transcript):
        """The registration guard passes and writes the spawn (tool_use_id,
        spawned_at). Pierce runs inside it on a real dispatch; it is a separate
        guard with its own tests and is not what is under test here."""
        cc = self.make_cc(name)
        payload = {"session_id": self.sid, "tool_use_id": "tu-" + name, "tool_name": "Agent",
                   "transcript_path": transcript,
                   "tool_input": {"name": name, "subagent_type": "zach", "isolation": "worktree",
                                  "prompt": "do it\ncross-repo-worktree: %s\n" % cc}}
        with patch.object(ws, "_pierce_before_registration", return_value=None):
            rec = ws.register_spawn(payload, self.entity)
        self.assertTrue(rec.get("tool_use_id") and rec.get("spawned_at"))
        return cc

    def test_a_registered_spawn_another_guard_refused_is_discarded_directly(self):
        # Review rv-20261014T105153Z-f22eff3f-da5a: the registration guard wrote
        # the spawn, verify-agent-prompt.sh then refused the same call (as it
        # did echo-opus-meetland1's first dispatch on 2026-10-14), and discard
        # still answered "is not finished". The lead's transcript holds the
        # platform's own answer to that call.
        tuid = "tu-zach-opus-nf"
        transcript = self.lead_transcript(tuid, {"is_error": True, "content": (
            "PreToolUse:Agent hook error: [bash /e/scripts/hooks/verify-agent-prompt.sh]: "
            "=== Agent prompt verification FAILED ===\n  - subagent-as-spawner")})
        cc = self.registered("zach-opus-nf", transcript)
        r = ws.discard("zach-opus-nf", "its dispatch was refused by a guard",
                       not_ceo_ordered="a refused spawn, never an order of his", me=self.sid)
        self.assertTrue(r["discarded"])
        disp = self.rec("zach-opus-nf")["disposition"]
        self.assertEqual(disp["kind"], "discarded")
        self.assertIn("verify-agent-prompt.sh", disp["dispatch_refused"])
        self.assertFalse(os.path.exists(cc))
        self.assertNotIn("cc/zach-opus-nf", branches(self.other))

    def test_discarding_an_empty_continuation_keeps_the_unlanded_work_it_continued(self):
        # 2026-10-24 ~07:50Z: `discard zach-opus-orch5` (continues orch3, which
        # continues orch1; orch5 had committed nothing) deleted orch1's and
        # orch3's reviewed, unlanded branches with its own. A discard takes only
        # the named workspace; a predecessor holding commits not in its
        # integration branch stays, back on the list of work to land.
        cc = self.make_cc("zach-opus-pa")
        aid, _npath = self.spawn("zach-opus-pa", cc=cc)
        self.commit(cc, "reviewed.txt")
        tip = run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip()
        self.finish(aid)
        cc2 = self.make_cc("zach-opus-pb", base="cc/zach-opus-pa")
        aid2, npath2 = self.spawn("zach-opus-pb", cc=cc2, extra="continues: zach-opus-pa\n")
        self.assertEqual(self.rec("zach-opus-pa")["disposition"]["kind"], "continued")
        self.finish(aid2)

        r = ws.discard("zach-opus-pb", "its continuation committed nothing",
                       not_ceo_ordered="Rich's own follow-up, not his order", me=self.sid)

        self.assertTrue(r["discarded"])
        self.assertEqual(self.rec("zach-opus-pb")["disposition"]["kind"], "discarded")
        self.assertFalse(os.path.exists(cc2) or os.path.exists(npath2))
        self.assertNotIn("cc/zach-opus-pb", branches(self.other))
        self.assertIn("cc/zach-opus-pa", branches(self.other), "the predecessor's unlanded work was deleted")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "cc/zach-opus-pa").stdout.strip(), tip)
        self.assertIsNone(self.rec("zach-opus-pa").get("disposition"))
        self.assertIn("zach-opus-pa", self.names())

    def test_unlanded_is_judged_against_the_predecessors_own_integration_branch(self):
        # Work bound to a release branch is not landed because main has it.
        run("git", "-C", self.other, "branch", "v1.3.1", "main")
        cc = os.path.join(self.env.root, "other-wt", "zach-opus-qa")
        ws.register_cc(self.sid, "zach-opus-qa", self.other, cc, "cc/zach-opus-qa", integration="v1.3.1")
        run("git", "-C", self.other, "worktree", "add", "-q", cc, "-b", "cc/zach-opus-qa", "v1.3.1")
        ws.confirm_cc(self.sid, "zach-opus-qa", cc, True)
        aid, _npath = self.spawn("zach-opus-qa", cc=cc)
        self.commit(cc, "release.txt")
        self.finish(aid)
        self.merge(self.other, "cc/zach-opus-qa")                # in main, not in v1.3.1
        cc2 = os.path.join(self.env.root, "other-wt", "zach-opus-qb")
        ws.register_cc(self.sid, "zach-opus-qb", self.other, cc2, "cc/zach-opus-qb", integration="v1.3.1")
        run("git", "-C", self.other, "worktree", "add", "-q", cc2, "-b", "cc/zach-opus-qb", "cc/zach-opus-qa")
        ws.confirm_cc(self.sid, "zach-opus-qb", cc2, True)
        aid2, _np2 = self.spawn("zach-opus-qb", cc=cc2, extra="continues: zach-opus-qa\n")
        self.finish(aid2)

        r = ws.discard("zach-opus-qb", "its continuation committed nothing",
                       not_ceo_ordered="Rich's own follow-up, not his order", me=self.sid)

        self.assertEqual(r["kept"], ["zach-opus-qa"])
        self.assertIn("cc/zach-opus-qa", branches(self.other))
        self.assertNotIn("cc/zach-opus-qb", branches(self.other))

    def test_a_registered_spawn_the_platform_has_not_answered_or_launched_is_still_refused(self):
        for name, result in (("zach-opus-nq", None),
                             ("zach-opus-nl", {"content": [{"type": "text", "text":
                                                            "Async agent launched successfully.\nagentId: a1"}]})):
            cc = self.registered(name, self.lead_transcript("tu-" + name, result))
            with self.assertRaises(ws.SpecError) as e:
                ws.discard(name, "no longer wanted at all", not_ceo_ordered="not his order, a test", me=self.sid)
            self.assertIn("not finished", str(e.exception))
            self.assertTrue(os.path.isdir(cc))


class TeammateBoundToABranchOfItsOwn(Base):
    """2026-10-14: the meeting and search work goes to the v1.3.0 release
    branch while fixes go to main. `spawn.sh --integration richos=v1.3.0` for
    echo-opus-tap22 CORRECTED the repository's body of work richos-001 from
    main to v1.3.0, moving every running main-bound teammate with it, and
    `workspaces.sh merge` could only merge into a branch the main checkout has
    checked out, so every v1.3.0 land was done by hand. Now the teammate gets a
    body of work of its own, nobody else moves, and its merge lands on v1.3.0
    in a temporary checkout while the main checkout stays on main."""

    def spawn_lib(self):
        spec_ = importlib.util.spec_from_file_location(
            "richos_spawn_lib", os.path.join(ENGINE, "scripts", "lib", "spawn.py"))
        mod = importlib.util.module_from_spec(spec_)
        spec_.loader.exec_module(mod)
        return mod

    def test_a_release_bound_teammate_lands_on_its_branch_and_moves_nobody(self):
        run("git", "-C", self.other, "branch", "v1.3.0", "main")
        # A main-bound teammate already running in the same repository.
        cc_main = self.make_cc("zach-opus-rm")
        self.spawn("zach-opus-rm", cc=cc_main)
        work_before = ws.integration_record(self.other)
        # spawn.sh's step for `--integration <other>=v1.3.0`.
        SP = self.spawn_lib()
        out = SP.ensure_integration(self.other, "v1.3.0", "the release", self.sid)
        self.assertEqual(ws.integration_record(self.other), work_before, "the repository's record moved: %s" % (out,))
        note, own = out
        self.assertEqual(own, "v1.3.0", note)
        self.assertIn("this teammate only", note)
        self.assertEqual(ws.integration_target([self.rec("zach-opus-rm")], self.other)[0], "main")
        # create-teammate-worktree.sh's registration, carrying that branch.
        path = os.path.join(self.env.root, "other-wt", "zach-opus-rv")
        ws.register_cc(self.sid, "zach-opus-rv", self.other, path, "cc/zach-opus-rv", integration="v1.3.0")
        run("git", "-C", self.other, "worktree", "add", "-q", path, "-b", "cc/zach-opus-rv", "v1.3.0")
        ws.confirm_cc(self.sid, "zach-opus-rv", path, True)
        aid, _npath = self.spawn("zach-opus-rv", cc=path)
        self.commit(path, "release.txt")
        tip = run("git", "-C", path, "rev-parse", "HEAD").stdout.strip()
        self.finish(aid)
        self.assertEqual(ws.integration_target([self.rec("zach-opus-rv")], self.other)[0], "v1.3.0")
        self.assertEqual(ws.integration_target([self.rec("zach-opus-rm")], self.other)[0], "main")
        self.assertEqual(ws.integration_record(self.other)["branch"], "main")
        main_before = run("git", "-C", self.other, "rev-parse", "main").stdout.strip()

        merged, res = ws.merge_and_land("zach-opus-rv", self.sid)

        self.assertEqual(merged, [(self.other, "cc/zach-opus-rv")])
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "v1.3.0^2").stdout.strip(), tip)
        self.assertIn("into v1.3.0", run("git", "-C", self.other, "log", "-1", "--format=%s", "v1.3.0").stdout)
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main").stdout.strip(), main_before)
        self.assertEqual(run("git", "-C", self.other, "symbolic-ref", "--short", "HEAD").stdout.strip(), "main")
        self.assertEqual(run("git", "-C", self.other, "status", "--porcelain").stdout, "")
        self.assertEqual(worktrees(self.other), [self.other, cc_main], "a temporary checkout was left behind")
        self.assertNotIn("cc/zach-opus-rv", branches(self.other))
        self.assertTrue(os.path.isdir(cc_main))
        # Its handover's last step names its own branch, never main.
        prepared = SP.PREPARE.prepare({"name": "zach-opus-rv", "subagent_type": "zach", "prompt": "do it"},
                                      [(self.other, "v1.3.0")])
        self.assertIn("run `git merge v1.3.0` inside your own worktree", prepared["prompt"])
        self.assertNotIn("git merge main", prepared["prompt"])


class ReleaseLandChecksRunOutsideTheOperatorsRecord(Base):
    """2026-10-24 (zach-opus-orch5): every land into a branch other than main
    failed. Its temporary checkout lived under ~/.claude/state/workspaces, and
    the test runner removes every variable pointing inside ~/.claude before a
    check (record-canary.sh _rc_roads_back), PATH included, whose first entry
    is the checkout's own scripts/bin: verification-pins.test.sh ended with
    INFRASTRUCTURE-ERROR rc=127. The checkout now lives in the engine's state
    location (machine_settings.state), outside the operator's record."""

    def test_a_merge_into_a_release_branch_runs_its_checks_outside_claude_config_dir(self):
        run("git", "-C", self.other, "branch", "v1.3.1", "main")
        seen = os.path.join(self.env.root, "hook-cwd")
        hook = os.path.join(self.other, ".git", "hooks", "pre-merge-commit")
        with open(hook, "w") as f:
            f.write("#!/bin/sh\npwd -P > %s\n" % shlex.quote(seen))
        os.chmod(hook, 0o755)
        path = os.path.join(self.env.root, "other-wt", "zach-opus-rc")
        ws.register_cc(self.sid, "zach-opus-rc", self.other, path, "cc/zach-opus-rc", integration="v1.3.1")
        run("git", "-C", self.other, "worktree", "add", "-q", path, "-b", "cc/zach-opus-rc", "v1.3.1")
        ws.confirm_cc(self.sid, "zach-opus-rc", path, True)
        aid, _npath = self.spawn("zach-opus-rc", cc=path)
        self.commit(path, "release.txt")
        self.finish(aid)

        merged, res = ws.merge_and_land("zach-opus-rc", self.sid)

        self.assertEqual(merged, [(self.other, "cc/zach-opus-rc")])
        self.assertTrue(res["landed"])
        with open(seen) as f:
            where = f.read().strip()
        record = os.path.realpath(os.environ["CLAUDE_CONFIG_DIR"])
        for root in (record, os.path.realpath(os.path.join(os.environ["HOME"], ".claude"))):
            self.assertFalse(where == root or where.startswith(root + os.sep),
                             "the land's checks ran inside the operator's record: %s" % where)
        self.assertTrue(where.startswith(os.path.realpath(self.env.root) + os.sep), where)
        self.assertFalse(os.path.exists(where), "the temporary checkout was left behind")


class LeaseHeldThroughTheLand_TheHoldersTurnEndDoesNotReleaseIt(Base):
    """2026-10-11, both in richos from the lead's terminal, each a background
    script that took the lease and then ran `workspaces.sh merge`:

    1. 14:51 (echo-opus-feature1): `land-lease: RENEWED`, the lead's turn ended
       a second later, and its turn-end release (operator_fences.py turn-end)
       found the repository at rest and deleted the lease; the merge's ORIG_HEAD
       was refused at 14:51:30, "The land lease for this repository is free."
    2. 17:18 (echo-sonnet-quotaflake3): ACQUIRED; the merge wrote ORIG_HEAD at
       17:18:10 under the lease; the lead's turn ended at once and its release
       ran before the merge wrote its index (AUTO_MERGE at 17:18:12), so the
       repository again looked at rest. The gate ran five minutes, the move of
       main was refused at 17:23:27, and the merge's own `git merge --abort` was
       refused by the same fence after rewriting the index, leaving MERGE_HEAD.

    The test process is the Claude session (as in the class above), the land is
    a background script it started, and the turn end is the release run as the
    session's own child while that script is between two steps."""

    def setUp(self):
        super().setUp()
        self.name = "zach-opus-lz"
        cc = self.make_cc(self.name)
        aid, _npath = self.spawn(self.name, cc=cc)
        self.commit(cc)
        self.finish(aid)
        self.tip = run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip()
        # main has an upstream and is pushed, so "at rest" can be true.
        origin = os.path.join(self.env.root, "other-origin.git")
        run("git", "init", "-q", "--bare", origin)
        run("git", "-C", self.other, "remote", "add", "origin", origin)
        run("git", "-C", self.other, "push", "-q", "-u", "origin", "main")
        decl = os.path.join(self.env.root, "fence-entity")
        os.makedirs(decl)
        with open(os.path.join(decl, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n')
        fences = os.path.join(ENGINE, "scripts", "operator-fences.sh")
        run("bash", fences, "install", "--repo", self.other, "--entity", decl)
        run("bash", fences, "on", "--repo", self.other, "--entity", decl)
        start = subprocess.run(["ps", "-o", "lstart=", "-p", str(os.getpid())], capture_output=True,
                               text=True, env=dict(os.environ, TZ="UTC0", LC_ALL="C")).stdout
        sessions = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "sessions")
        os.makedirs(sessions, exist_ok=True)
        with open(os.path.join(sessions, "%d.json" % os.getpid()), "w") as f:
            json.dump({"pid": os.getpid(), "sessionId": self.sid, "cwd": self.entity,
                       "procStart": " ".join(start.split()), "kind": "interactive"}, f)
        self.envx = {k: v for k, v in os.environ.items() if k not in ("CLAUDE_PID", "CLAUDE_CODE_ENTRYPOINT")}
        self.lease_cmd = os.path.join(ENGINE, "scripts", "land-lease.sh")
        self.d = os.path.realpath(tempfile.mkdtemp(prefix="lease-land-", dir=self.env.root))
        self.head = run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip()
        self.home = os.path.dirname(ws.land_lock_path(self.other))

    def flag(self, name):
        return os.path.join(self.d, name)

    def wait_for(self, name, pr):
        guard = time.monotonic() + 60
        while not os.path.exists(self.flag(name)):
            if pr.poll() is not None:
                self.fail("the land script ended before %s: %s" % (name, pr.communicate()))
            self.assertLess(time.monotonic(), guard, "the land script never reached " + name)
            time.sleep(0.02)

    def gate(self):
        """git's own merge gate, which passes (incident 2: the gate ran first)."""
        hook = os.path.join(self.other, ".git", "hooks", "pre-merge-commit")
        with open(hook, "w") as f:
            f.write("#!/bin/sh\necho 'the merge gate ran and passed'\nexit 0\n")
        os.chmod(hook, 0o755)

    def pause_after_orig_head(self):
        """Holds the merge right after Git committed its ORIG_HEAD, before it
        writes its index: the window the 17:18 turn end fell into."""
        chain = os.path.join(self.other, ".git", "hooks", "reference-transaction.d")
        os.makedirs(chain, exist_ok=True)
        hook = os.path.join(chain, "60-test-pause-after-orig-head")
        with open(hook, "w") as f:
            f.write("#!/bin/bash\n[ \"$1\" = committed ] || exit 0\ngrep -q ' ORIG_HEAD$' || exit 0\n"
                    "[ -f '%(d)s/arm' ] || exit 0\nrm -f '%(d)s/arm'\n: > '%(d)s/paused'\n"
                    "i=0; while [ ! -f '%(d)s/go' ] && [ $i -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n"
                    "exit 0\n" % {"d": self.d})
        os.chmod(hook, 0o755)
        open(self.flag("arm"), "w").close()

    def land_script(self, between=""):
        """The lead's script: acquire, then merge, in one background command."""
        path = os.path.join(self.d, "merge.sh")
        with open(path, "w") as f:
            f.write("#!/bin/bash\nbash '%s' acquire --repo '%s' || exit $?\n%s"
                    "'%s' '%s' --session '%s' merge '%s' -m 'Merge %s'\n"
                    % (self.lease_cmd, self.other, between, sys.executable, LIB, self.sid, self.name,
                       self.name))
        return subprocess.Popen(["bash", path], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                env=self.envx)

    def turn_end(self):
        """The holder's turn end, as the Stop hook runs it: a child of the session."""
        r = subprocess.run([sys.executable, os.path.join(ENGINE, "scripts", "lib", "operator_fences.py"),
                            "turn-end"], capture_output=True, text=True, env=self.envx)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        return r.stdout

    def landed(self):
        return run("git", "-C", self.other, "merge-base", "--is-ancestor", self.tip, "main",
                   check=False).returncode == 0

    def test_incident_1_renewed_then_turn_end_then_merge_at_once(self):
        # The previous land (tooling4) took the lease, so this script RENEWS it.
        r = subprocess.run(["bash", self.lease_cmd, "acquire", "--repo", self.other],
                           capture_output=True, text=True, env=self.envx)
        self.assertIn("ACQUIRED", r.stdout, r.stdout + r.stderr)
        pr = self.land_script(between=": > '%s'\nwhile [ ! -f '%s' ]; do sleep 0.02; done\n"
                              % (self.flag("acquired"), self.flag("go")))
        self.wait_for("acquired", pr)
        said = self.turn_end()
        open(self.flag("go"), "w").close()
        out, err = pr.communicate(timeout=180)
        self.assertIn("RENEWED", out + err)
        self.assertNotIn("does not hold the land lease", out + err)
        self.assertEqual(pr.returncode, 0, out + err)
        self.assertTrue(self.landed(), out + err)
        self.assertIn("still running", said)
        self.assertFalse(os.path.exists(os.path.join(self.home, "fence-refusals.jsonl")))

    def test_incident_2_turn_end_inside_the_merge_then_the_gate_then_main(self):
        self.gate()
        self.pause_after_orig_head()
        pr = self.land_script()
        self.wait_for("paused", pr)
        self.turn_end()
        open(self.flag("go"), "w").close()
        out, err = pr.communicate(timeout=180)
        self.assertIn("ACQUIRED", out + err)
        self.assertNotIn("does not hold the land lease", out + err)
        self.assertEqual(pr.returncode, 0, out + err)
        self.assertTrue(self.landed(), out + err)
        self.assertNotEqual(run("git", "-C", self.other, "rev-parse", "-q", "--verify", "MERGE_HEAD",
                                check=False).returncode, 0)
        self.assertFalse(os.path.exists(os.path.join(self.home, "fence-refusals.jsonl")))

    def test_the_turn_end_after_the_land_has_ended_still_releases_it(self):
        pr = self.land_script()
        out, err = pr.communicate(timeout=180)
        self.assertEqual(pr.returncode, 0, out + err)
        run("git", "-C", self.other, "push", "-q", "origin", "main")
        self.assertEqual(self.turn_end(), "")
        self.assertEqual([n for n in os.listdir(self.home) if n.endswith(".lease")], [])

    def test_a_merge_refused_for_a_lost_lease_leaves_no_merge_head(self):
        """Whatever frees the lease mid-merge (here the holder's own explicit
        release), the merge's own abort retakes the free lease and the refused
        merge leaves the main checkout as it was."""
        self.gate()
        self.pause_after_orig_head()
        pr = self.land_script()
        self.wait_for("paused", pr)
        r = subprocess.run(["bash", self.lease_cmd, "release", "--repo", self.other],
                           capture_output=True, text=True, env=self.envx)
        self.assertIn("RELEASED", r.stdout, r.stdout + r.stderr)
        open(self.flag("go"), "w").close()
        out, err = pr.communicate(timeout=180)
        self.assertNotEqual(pr.returncode, 0, out + err)
        self.assertIn("does not hold the land lease", err)
        self.assertIn("was aborted", err)
        self.assertNotEqual(run("git", "-C", self.other, "rev-parse", "-q", "--verify", "MERGE_HEAD",
                                check=False).returncode, 0, out + err)
        self.assertEqual(run("git", "-C", self.other, "status", "--porcelain",
                             "--untracked-files=no").stdout, "")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip(), self.head)
        self.assertFalse(self.landed())

    def test_a_land_that_renews_the_lease_while_the_turn_end_checks_keeps_it(self):
        """Review rv-20261011T205852Z-54ae8c6a-eec1, finding 1: the turn end
        read the lease, found no running land on it, and while it checked the
        repository a new land renewed the same holder's lease. The delete that
        followed checked only the holder, so the new land lost its lease."""
        r = subprocess.run(["bash", self.lease_cmd, "acquire", "--repo", self.other],
                           capture_output=True, text=True, env=self.envx)
        self.assertIn("ACQUIRED", r.stdout, r.stdout + r.stderr)
        driver = os.path.join(self.d, "turn-end.py")
        with open(driver, "w") as f:
            f.write("import os, sys, time\nsys.path.insert(0, %r)\nimport operator_fences as F\n"
                    "real = F.at_rest\n"
                    "def at_rest(*a):\n"
                    "    open(%r, 'w').close()\n"
                    "    guard = time.monotonic() + 60\n"
                    "    while not os.path.exists(%r) and time.monotonic() < guard:\n"
                    "        time.sleep(0.02)\n"
                    "    return real(*a)\n"
                    "F.at_rest = at_rest\nsys.exit(F.cmd_turn_end([]))\n"
                    % (os.path.join(ENGINE, "scripts", "lib"), self.flag("checking"), self.flag("renewed")))
        te = subprocess.Popen([sys.executable, driver], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              text=True, env=self.envx)
        # Both children wait on a flag this test writes; registered, so a failure
        # or timeout before it is written still ends them (Env.close kills each).
        self.env.procs.append(te)
        self.wait_for("checking", te)
        pr = self.land_script(between=": > '%s'\nwhile [ ! -f '%s' ]; do sleep 0.02; done\n"
                              % (self.flag("renewed"), self.flag("go")))
        self.env.procs.append(pr)
        self.wait_for("renewed", pr)
        said, err = te.communicate(timeout=60)
        self.assertEqual(te.returncode, 0, said + err)
        leases = [n for n in os.listdir(self.home) if n.endswith(".lease")]
        open(self.flag("go"), "w").close()
        out, err = pr.communicate(timeout=180)
        self.assertEqual(len(leases), 1, "the turn end deleted the renewed lease: " + said)
        self.assertIn("still running", said)
        self.assertIn("RENEWED", out + err)
        self.assertNotIn("does not hold the land lease", out + err)
        self.assertEqual(pr.returncode, 0, out + err)
        self.assertTrue(self.landed(), out + err)

    def test_a_declared_holders_refused_merge_retakes_the_lease_as_that_holder_and_aborts(self):
        """Review rv-20261011T205852Z-54ae8c6a-eec1, finding 2: the abort's
        retake ran `land-lease.sh acquire` without --holder, which a declared
        non-Claude holder (Codex) needs, so its refused merge was NOT aborted."""
        if not shutil.which("cc"):
            self.skipTest("no C compiler for the stand-in declared holder")
        # The stand-in for Codex, as operator-fences-fixture.sh builds it: it
        # stays alive as the ancestor of what it runs (a copy of /bin/bash is
        # killed by macOS outside the system volume).
        src = os.path.join(self.d, "fakecodex.c")
        with open(src, "w") as f:
            f.write("#include <sys/wait.h>\n#include <unistd.h>\nint main(int argc, char **argv) {\n"
                    "    pid_t pid = fork();\n"
                    "    if (pid == 0) { argv[0] = \"bash\"; execv(\"/bin/bash\", argv); _exit(127); }\n"
                    "    int status = 0;\n"
                    "    if (pid < 0 || waitpid(pid, &status, 0) < 0) return 126;\n"
                    "    return WIFEXITED(status) ? WEXITSTATUS(status) : 128;\n}\n")
        codex = os.path.join(self.d, "fakecodex")
        run("cc", "-O0", "-o", codex, src)
        decl = os.path.join(self.env.root, "fence-entity-codex")
        os.makedirs(decl)
        with open(os.path.join(decl, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS="codex=%s"\nOPERATOR_FENCES_REPOS=""\n' % codex)
        fences = os.path.join(ENGINE, "scripts", "operator-fences.sh")
        run("bash", fences, "install", "--repo", self.other, "--entity", decl)
        run("bash", fences, "on", "--repo", self.other, "--entity", decl)
        # No Claude session is behind this land: only the declared holder.
        os.unlink(os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "sessions", "%d.json" % os.getpid()))
        self.gate()
        self.pause_after_orig_head()
        script = os.path.join(self.d, "codex-merge.sh")
        with open(script, "w") as f:
            f.write("bash '%s' acquire --repo '%s' --holder codex || exit $?\n"
                    "'%s' '%s' --session '%s' merge '%s' -m 'Merge %s'\n"
                    % (self.lease_cmd, self.other, sys.executable, LIB, self.sid, self.name, self.name))
        pr = subprocess.Popen([codex, script], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                              env=self.envx)
        self.wait_for("paused", pr)
        # Whatever frees the lease mid-merge.
        for n in os.listdir(self.home):
            if n.endswith(".lease"):
                os.unlink(os.path.join(self.home, n))
        open(self.flag("go"), "w").close()
        out, err = pr.communicate(timeout=180)
        self.assertIn("ACQUIRED", out + err)
        self.assertNotEqual(pr.returncode, 0, out + err)
        self.assertIn("does not hold the land lease", err)
        self.assertIn("was aborted", err)
        self.assertNotEqual(run("git", "-C", self.other, "rev-parse", "-q", "--verify", "MERGE_HEAD",
                                check=False).returncode, 0, out + err)
        self.assertEqual(run("git", "-C", self.other, "status", "--porcelain",
                             "--untracked-files=no").stdout, "")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "HEAD").stdout.strip(), self.head)
        self.assertFalse(self.landed())


ENGINE = os.path.realpath(os.path.join(HERE, "..", ".."))


class SecondReview_NoWorkLandsUnreviewed(Base):

    def test_off_requires_a_reason_and_audits_native_authority(self):
        before=open(os.path.join(self.other,'.git/hooks/reference-transaction')).read()
        refused=run('bash',self.fences,'off','--repo',self.other,check=False)
        self.assertNotEqual(refused.returncode,0)
        self.assertIn('--reason',refused.stdout)
        self.assertEqual(open(os.path.join(self.other,'.git/hooks/reference-transaction')).read(),before)
        forged=run('bash',self.fences,'off','--repo',self.other,'--reason','Forged claim','--holder','unverified','--holder-pid','99999999',check=False)
        self.assertNotEqual(forged.returncode,0)
        self.assertEqual(open(os.path.join(self.other,'.git/hooks/reference-transaction')).read(),before)
        run('bash',self.fences,'off','--repo',self.other,'--reason','Investigate the lease mechanism')
        F=ws._fence_program()
        with open(os.path.join(F._default_home(),'operator-fence-settings.jsonl')) as audit:
            rows=[json.loads(line) for line in audit if line.strip()]
        self.assertEqual(rows[-1]['reason'],'Investigate the lease mechanism')
        self.assertEqual(rows[-1]['value'],'off')
        self.assertIn('actor',rows[-1])
        self.assertIn('authority',rows[-1])

    def test_off_retains_direct_git_review_acceptance(self):
        _cc,(tip,)=self.finished('zach-opus-off-review')
        run('bash',self.fences,'off','--repo',self.other,'--reason','Investigate the lease mechanism')
        before=self.head()
        result=run('git','-C',self.other,'merge','--ff-only','cc/zach-opus-off-review',check=False)
        self.assertNotEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('SECOND REVIEW',result.stderr)
        self.assertEqual(self.head(),before)
        self.verdict(tip,'passed')
        run('git','-C',self.other,'merge','--ff-only','cc/zach-opus-off-review')
        self.assertEqual(self.head(),tip)

    def test_conflicting_reviews_cannot_be_erased_by_a_later_pass(self):
        F = ws._fence_program()
        tip = run('git', '-C', self.other, 'rev-parse', 'HEAD').stdout.strip()
        rows = []
        def record(rid, verdict, findings=(), earlier=(), recheck=''):
            folder = os.path.join(self.env.root, rid)
            os.makedirs(folder)
            row = dict(id=rid, repo=self.other, tip=tip, trigger='handover', verdict=verdict,
                       record=folder, recheck_of=recheck)
            with open(os.path.join(folder, 'verdict.json'), 'w') as out:
                json.dump(dict(answer=dict(findings=list(findings), earlier_findings=list(earlier)),
                               earlier_findings_in=[dict(id=e['id']) for e in earlier]), out)
            rows.append(row)
        f = dict(blocks=True, priority=1, title='User data lost', files=['state.py'], evidence='fixture', fixture='')
        record('first', 'changes-requested', [f])
        record('late-pass', 'passed')
        self.assertEqual(F.review_of(rows, tip, self.other)[0]['verdict'], 'changes-requested')
        record('unwitnessed', 'passed', earlier=[dict(id='first#1',status='withdrawn',note='opinion')])
        self.assertEqual(F.review_of(rows, tip, self.other)[0]['verdict'], 'changes-requested')
        record('independent-resolution', 'passed', earlier=[dict(id='first#1',status='withdrawn',note='original authority inspected')], recheck='first')
        self.assertEqual(F.review_of(rows, tip, self.other)[0]['verdict'], 'passed')
        record('other-blocker', 'changes-requested', [f])
        self.assertEqual(F.review_of(rows, tip, self.other)[0]['verdict'], 'changes-requested')
        rows[-1]['repo'] = self.entity
        self.assertEqual(F.review_of(rows, tip, self.other)[0]['verdict'], 'passed')

    """CEO §113 (2026-10-08): "A regular RichOS user can never be expected
    anything even remotely close to that. So, this all must be completely
    automated." Slice 3 of richos-hq docs/plans/2026-10-09-automatic-second-
    review-and-t3-ideas.md (§2.5, §4 row 3) with Sage's check (§2 catches 1, 8):
    in a repository listed in SECOND_REVIEW_REPOS, work lands only with a
    passing second review of exactly its tip, whether it lands through
    `workspaces.sh merge`, a plain `git merge`, a fast-forward or a codex/
    branch. The fence is ON here, as on the operator's Mac, and this test
    process holds the land lease, so every refusal below is the review's."""

    def setUp(self):
        super().setUp()
        self.saved_env = {k: os.environ.pop(k, None) for k in
                          ("CLAUDE_PID", "CLAUDE_CODE_ENTRYPOINT", "SECOND_REVIEW_STATE_DIR", "RICHOS_AGENT_OWNER")}
        os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(self.env.root, "review-state")
        self.ledger = os.path.join(os.environ["SECOND_REVIEW_STATE_DIR"], "reviews.jsonl")
        self.decl = os.path.join(self.env.root, "fence-entity")
        os.makedirs(self.decl)
        self.declare("other")
        self.fences = os.path.join(ENGINE, "scripts", "operator-fences.sh")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        run("bash", self.fences, "on", "--repo", self.other, "--entity", self.decl)
        start = subprocess.run(["ps", "-o", "lstart=", "-p", str(os.getpid())], capture_output=True,
                               text=True, env=dict(os.environ, TZ="UTC0", LC_ALL="C")).stdout
        sessions = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "sessions")
        os.makedirs(sessions, exist_ok=True)
        with open(os.path.join(sessions, "%d.json" % os.getpid()), "w") as f:
            json.dump({"pid": os.getpid(), "sessionId": self.sid, "cwd": self.entity,
                       "procStart": " ".join(start.split()), "kind": "interactive"}, f)
        r = run("bash", os.path.join(ENGINE, "scripts", "land-lease.sh"), "acquire", "--repo", self.other)
        self.assertIn("ACQUIRED", r.stdout, r.stdout + r.stderr)

    def tearDown(self):
        for k, v in self.saved_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        super().tearDown()

    def declare(self, listed):
        with open(os.path.join(self.decl, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n'
                    'SECOND_REVIEW_REPOS="%s"\n' % listed)

    def repo_identity(self, repo):
        if not os.path.isdir(repo):
            return os.path.realpath(repo)
        common = run("git", "-C", repo, "rev-parse", "--git-common-dir").stdout.strip()
        real = os.path.realpath(os.path.join(repo, common))
        return real

    def verdict(self, tip, verdict="passed", trigger="handover", findings=(), repo=None, legacy=False, work=None):
        rid = "rv-test-%s-%d" % (tip[:8], len(open(self.ledger).readlines()) if os.path.exists(self.ledger) else 0)
        record = os.path.join(os.path.dirname(self.ledger), "reviews", rid)
        os.makedirs(record)
        found = [{"priority": p, "title": t, "files": ["work.txt:1"], "evidence": "seen", "fixture": ""}
                 for p, t in findings]
        with open(os.path.join(record, "verdict.json"), "w") as f:
            json.dump({"answer": {"reviewed_commit": tip, "verdict": verdict, "findings": found}}, f)
        row = {"id": rid, "repo": repo or self.other, "tip": tip, "verdict": verdict, "trigger": trigger,
               "reviewer": "codex", "reviewer_model": "gpt-6.1-sol", "findings": len(found),
               "p1": sum(1 for p, _t in findings if p == 1), "record": record,
               "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        if not legacy:
            row["repo_id"] = self.repo_identity(repo or self.other)
        if work:
            row["work"] = work
        with open(self.ledger, "a") as f:
            f.write(json.dumps(row) + "\n")
        return rid

    def finished(self, name, *texts):
        cc = self.make_cc(name)
        aid, _npath = self.spawn(name, cc=cc)
        tips = []
        for i, text in enumerate(texts or ("work\n",)):
            self.commit(cc, "work%d.txt" % i, text)
            tips.append(run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip())
        self.finish(aid)
        return cc, tips

    def head(self):
        return run("git", "-C", self.other, "rev-parse", "main").stdout.strip()

    def no_merge_left(self):
        if run("git", "-C", self.other, "rev-parse", "-q", "--verify", "MERGE_HEAD", check=False).returncode == 0:
            run("git", "-C", self.other, "merge", "--abort")        # this process holds the lease

    # -- workspaces.sh merge ------------------------------------------------------
    def test_second_review_merge_is_refused_with_no_verdict(self):
        cc, (tip,) = self.finished("zach-opus-rv0")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv0", self.sid)
        # The land command's own refusal, before git's merge gate ever runs (the
        # fence would refuse the same merge later, with "refused in").
        self.assertIn("SECOND REVIEW: cc/zach-opus-rv0 was not merged into", str(e.exception))
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertTrue(os.path.isdir(cc))
        self.assertIn("cc/zach-opus-rv0", branches(self.other))

    def test_second_review_merge_is_refused_with_a_verdict_on_an_older_commit(self):
        _cc, (older, tip) = self.finished("zach-opus-rv1", "one\n", "two\n")
        self.verdict(older, "passed")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv1", self.sid)
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_merge_is_refused_on_changes_requested_and_names_the_findings(self):
        _cc, (tip,) = self.finished("zach-opus-rv2")
        self.verdict(tip, "passed", trigger="long-job")                   # a mid-job pass never counts
        self.verdict(tip, "changes-requested", findings=[(1, "the microphone is never handed back")])
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv2", self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertIn("the microphone is never handed back", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_merge_goes_through_with_a_passing_verdict_on_the_tip(self):
        cc, (tip,) = self.finished("zach-opus-rv3")
        rid = self.verdict(tip, "passed", findings=[(3, "a comment could name the ruling")])
        merged, res = ws.merge_and_land("zach-opus-rv3", self.sid, "Merge cc/zach-opus-rv3: the work")
        self.assertEqual(merged, [(self.other, "cc/zach-opus-rv3")])
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        body = run("git", "-C", self.other, "log", "-1", "--format=%B").stdout
        self.assertIn("Merge cc/zach-opus-rv3: the work", body.splitlines()[0])
        self.assertIn("Second review: passed, %s" % rid, body)
        self.assertIn("P3 a comment could name the ruling", body)
        self.assertFalse(os.path.exists(cc))

    def test_second_review_another_branchs_open_finding_never_refuses_a_passed_tip(self):
        # 2026-10-15: cc/echo-opus-words58c passed its own review and was refused
        # with cc/echo-opus-hood57b's P2. Manual reviews of two branches cut from
        # the same main commit share one work key (range:<repo>:<base>).
        base = self.head()
        work = "range:%s:%s" % (self.other, base)
        _sib, (sibling,) = self.finished("zach-opus-rvsib", "sibling\n")
        _cc, (_older, tip) = self.finished("zach-opus-rvown", "one\n", "two\n")
        self.verdict(sibling, "changes-requested", trigger="manual", work=work,
                     findings=[(2, "the sibling branch's own defect")])
        rid = self.verdict(tip, "passed", trigger="manual", work=work)
        _merged, res = ws.merge_and_land("zach-opus-rvown", self.sid, "Merge cc/zach-opus-rvown: the work")
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        body = run("git", "-C", self.other, "log", "-1", "--format=%B").stdout
        self.assertIn("Second review: passed, %s" % rid, body)
        self.assertNotIn("sibling branch's own defect", body)
        # The branch's OWN earlier tip still counts: an open finding there refuses.
        _cc2, (own_old, own_tip) = self.finished("zach-opus-rvown2", "three\n", "four\n")
        work2 = "range:%s:%s" % (self.other, self.head())
        self.verdict(own_old, "changes-requested", trigger="manual", work=work2,
                     findings=[(2, "this branch's earlier defect")])
        self.verdict(own_tip, "passed", trigger="manual", work=work2)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rvown2", self.sid)
        self.assertIn("this branch's earlier defect", str(e.exception))
        self.assertEqual(self.head(), before)

    # -- the lead files a blocking finding as a follow-up (follow-up 12, D1) ------------
    # 2026-10-18 05:01Z, nightly 63 held on a finding the CEO had ruled a follow-up: "Override. ...
    # Follow-up for that review stuff." The only way through was switching the gate off.

    def follow_up(self, finding, *extra, check=True):
        return run("bash", os.path.join(ENGINE, "scripts", "second-review.sh"), "--follow-up", finding,
                   "--why", "Rich, per the CEO: not a normal-use hole in this work", *extra, check=check)

    def deferral_row(self, finding, repo, tip, action="follow-up"):
        with open(os.path.join(os.path.dirname(self.ledger), "review-deferrals.jsonl"), "a") as f:
            f.write(json.dumps({"action": action, "finding": finding, "repo": repo,
                                "repo_id": self.repo_identity(repo), "tip": tip, "why": "by hand"}) + "\n")

    def test_second_review_a_finding_filed_as_a_follow_up_lets_the_work_land(self):
        cc, (tip,) = self.finished("zach-opus-rvfu")
        rid = self.verdict(tip, "changes-requested", findings=[(2, "question-card answers are not in the cut")])
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rvfu", self.sid)
        self.assertIn("Blocking finding id(s): %s#1" % rid, str(e.exception))
        self.assertIn("--follow-up <finding id>", str(e.exception))
        self.assertEqual(self.head(), before)
        r = self.follow_up(rid + "#1")
        self.assertIn("filed as a follow-up", r.stdout)
        merged, res = ws.merge_and_land("zach-opus-rvfu", self.sid, "Merge cc/zach-opus-rvfu: the work")
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        body = run("git", "-C", self.other, "log", "-1", "--format=%B").stdout
        self.assertIn("Second review: passed, %s" % rid, body)
        self.assertIn("Filed as a follow-up: %s#1 question-card answers are not in the cut. "
                      "Why: Rich, per the CEO: not a normal-use hole in this work" % rid, body)

    def stale_fence(self):
        """The hook's copy of the fence made older than the engine's: its read_deferrals reads nothing.
        Returns (the installed copy's path, the engine's text)."""
        program = os.path.join(self.other, ".git", "hooks", "operator-fences", "operator_fences.py")
        engine_copy = os.path.join(ENGINE, "scripts", "lib", "operator_fences.py")
        with open(engine_copy) as f:
            text = f.read()
        older = text.replace("def read_deferrals(ledger):",
                             "def read_deferrals(ledger):\n    return {}\n\n\ndef _newer_read_deferrals(ledger):", 1)
        self.assertNotEqual(older, text)
        with open(program, "w") as f:
            f.write(older)
        return program, text

    def test_second_review_a_stale_installed_fence_never_refuses_a_land_that_passes(self):
        # 2026-10-19 ~05:30Z: the Escape fix passed its land check and was then refused by the
        # hook's copy of the fence, installed before the copy learned to read follow-up rulings.
        program, text = self.stale_fence()
        _cc, (tip,) = self.finished("zach-opus-rvstale")
        rid = self.verdict(tip, "changes-requested", findings=[(2, "a finding the lead ruled")])
        self.follow_up(rid + "#1")
        _merged, res = ws.merge_and_land("zach-opus-rvstale", self.sid, "Merge cc/zach-opus-rvstale: the work")
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        with open(program) as f:
            self.assertEqual(f.read(), text)

    def test_second_review_a_stale_installed_fence_never_refuses_a_plain_git_merge(self):
        # A release branch lands by a plain `git merge` into main, which never runs workspaces.sh merge.
        program, text = self.stale_fence()
        _cc, (tip,) = self.finished("zach-opus-rvstalegit")
        rid = self.verdict(tip, "changes-requested", findings=[(2, "a finding the lead ruled")])
        self.follow_up(rid + "#1")
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rvstalegit", check=False)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        with open(program) as f:
            self.assertEqual(f.read(), text)

    def test_a_fence_inside_the_repository_is_brought_current_from_main_never_the_checkout(self):
        # A plain `git merge` has already written the incoming work's files into the main checkout
        # when the fence decides; the incoming fence must not judge its own land.
        F = ws._fence_program()
        repo = os.path.join(self.env.root, "engine-inside")
        lib = os.path.join(repo, "engine", "scripts", "lib")
        os.makedirs(lib)
        run("git", "init", "-q", "-b", "main", repo)
        for name in F.PROGRAM_FILES:
            with open(os.path.join(lib, name), "w") as f:
                f.write("main's %s\n" % name)
        run("git", "-C", repo, "add", "-A")
        run("git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-qm", "base")
        with open(os.path.join(lib, "operator_fences.py"), "w") as f:
            f.write("the incoming work's fence\n")
        found = F.current_program({"ENGINE": os.path.join(repo, "engine"), "REPO": repo})
        self.assertEqual(found, {n: ("main's %s\n" % n).encode() for n in F.PROGRAM_FILES})

    def test_second_review_a_follow_up_ruling_reaches_a_plain_git_merge_too(self):
        _cc, (tip,) = self.finished("zach-opus-rvfug")
        rid = self.verdict(tip, "changes-requested", findings=[(2, "a finding the lead ruled")])
        self.follow_up(rid + "#1")
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rvfug", check=False)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_reopen_makes_the_finding_hold_again(self):
        _cc, (tip,) = self.finished("zach-opus-rvfur")
        rid = self.verdict(tip, "changes-requested", findings=[(1, "the microphone is never handed back")])
        self.follow_up(rid + "#1")
        r = run("bash", os.path.join(ENGINE, "scripts", "second-review.sh"), "--reopen", rid + "#1")
        self.assertIn("reopened", r.stdout)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rvfur", self.sid)
        self.assertIn("the microphone is never handed back", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_a_follow_up_of_another_repository_or_work_does_not_apply(self):
        base = self.head()
        work = "range:%s:%s" % (self.other, base)
        _sib, (sibling,) = self.finished("zach-opus-sibfu", "sibling\n")
        _cc, (tip,) = self.finished("zach-opus-ownfu", "own\n")
        sib = self.verdict(sibling, "changes-requested", trigger="manual", work=work,
                           findings=[(2, "the sibling branch's own defect")])
        own = self.verdict(tip, "changes-requested", trigger="manual", work=work, findings=[(2, "this work's defect")])
        self.follow_up(sib + "#1")                                     # another work's finding
        self.deferral_row(own + "#1", self.FOREIGN, tip)                # another repository's ruling
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-ownfu", self.sid)
        self.assertIn("this work's defect", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_a_follow_up_survives_a_later_review_that_says_still_open_and_blocking(self):
        _cc, (older, tip) = self.finished("zach-opus-rvfud", "one\n", "two\n")
        work = "teammate:zach-opus-rvfud"
        first = self.verdict(older, "changes-requested", work=work, findings=[(2, "a Stop path left open")])
        self.follow_up(first + "#1")
        later = self.verdict(tip, "changes-requested", work=work)
        with open(os.path.join(os.path.dirname(self.ledger), "reviews", later, "verdict.json"), "w") as f:
            json.dump({"answer": {"reviewed_commit": tip, "verdict": "changes-requested", "findings": [],
                                  "earlier_findings": [{"id": first + "#1", "status": "still-open",
                                                        "blocks": True, "note": "still open"}]},
                       "earlier_findings_in": [{"id": first + "#1"}]}, f)
        _merged, res = ws.merge_and_land("zach-opus-rvfud", self.sid, "Merge cc/zach-opus-rvfud: the work")
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_follow_up_never_upgrades_an_unresolved_legacy_pass(self):
        cc, (older, tip) = self.finished("zach-opus-rvful", "one\n", "two\n")
        work = "teammate:zach-opus-rvful"
        first = self.verdict(older, "changes-requested", work=work, findings=[(2, "a defect the lead ruled")])
        self.follow_up(first + "#1")
        self.verdict(tip, "passed", repo=cc, legacy=True, work=work)
        run("git", "-C", self.other, "worktree", "remove", "--force", cc)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rvful", self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertEqual(self.head(), before)

    FOREIGN = "/independent-clones/foreign"

    def test_second_review_a_passing_verdict_of_another_repository_authorizes_nothing(self):
        # Two independent clones can hold the same commit; the shared ledger's
        # verdict for the other one is no review of this one (review rv-20261009T063822Z-5db5607c-7e86).
        _cc, (tip,) = self.finished("zach-opus-rv6")
        self.verdict(tip, "passed", repo=self.FOREIGN)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv6", self.sid)
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_a_foreign_passing_verdict_does_not_override_a_local_refusal(self):
        _cc, (tip,) = self.finished("zach-opus-rv7")
        self.verdict(tip, "changes-requested", findings=[(1, "local defect")])
        self.verdict(tip, "passed", repo=self.FOREIGN)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv7", self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_the_fence_ignores_a_foreign_repositorys_verdict_too(self):
        _cc, (tip,) = self.finished("zach-opus-rv8")
        self.verdict(tip, "passed", repo=self.FOREIGN)
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rv8", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()

    def test_second_review_a_passing_verdict_recorded_from_a_linked_worktree_authorizes_the_land(self):
        # A manual review with --repo <linked worktree> records that worktree's path; Git says
        # it is the same repository as the main checkout (review rv-20261009T065022Z-ae3404f1-42cc).
        cc, (tip,) = self.finished("zach-opus-rv9")
        self.verdict(tip, "passed", repo=cc)
        merged, res = ws.merge_and_land("zach-opus-rv9", self.sid, "Merge cc/zach-opus-rv9: the work")
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_newer_refusal_from_a_linked_worktree_beats_an_older_pass_on_main(self):
        cc, (tip,) = self.finished("zach-opus-rv10")
        self.verdict(tip, "passed", repo=self.other)
        self.verdict(tip, "changes-requested", repo=cc, findings=[(1, "worktree defect")])
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv10", self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertEqual(self.head(), before)
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rv10", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()

    def refused_after(self, name, change, legacy=False, then_pass=True):
        cc, (tip,) = self.finished(name)
        if then_pass:
            self.verdict(tip, "passed", repo=self.other)
        self.verdict(tip, "changes-requested", repo=cc, findings=[(1, "worktree defect")], legacy=legacy)
        change(cc)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land(name, self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertEqual(self.head(), before)
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/" + name, check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()

    def test_second_review_a_newer_refusal_survives_its_worktree_being_moved(self):
        moved = os.path.join(self.env.root, "moved-rv11")
        self.refused_after("zach-opus-rv11", lambda cc: run("git", "-C", self.other, "worktree", "move", cc, moved))

    def test_second_review_a_newer_refusal_survives_its_worktree_being_removed(self):
        self.refused_after("zach-opus-rv12", lambda cc: run("git", "-C", self.other, "worktree", "remove", "--force", cc))

    def test_second_review_a_legacy_refusal_with_an_unresolvable_path_still_refuses(self):
        self.refused_after("zach-opus-rv13",
                           lambda cc: run("git", "-C", self.other, "worktree", "remove", "--force", cc), legacy=True)

    def test_second_review_a_legacy_pass_with_an_unresolvable_path_authorizes_nothing(self):
        cc, (tip,) = self.finished("zach-opus-rv14")
        self.verdict(tip, "passed", repo=cc, legacy=True)
        run("git", "-C", self.other, "worktree", "remove", "--force", cc)
        before = self.head()
        with self.assertRaises(ws.SpecError):
            ws.merge_and_land("zach-opus-rv14", self.sid)
        self.assertEqual(self.head(), before)

    def test_second_review_an_unresolvable_legacy_pass_blocks_even_over_an_older_pass(self):
        cc, (tip,) = self.finished("zach-opus-rv15")
        self.verdict(tip, "passed", repo=self.other)
        self.verdict(tip, "passed", repo=cc, legacy=True)
        run("git", "-C", self.other, "worktree", "remove", "--force", cc)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv15", self.sid)
        self.assertIn("changes-requested", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_a_bare_clone_and_a_separate_git_dir_clone_are_two_repositories(self):
        spec = importlib.util.spec_from_file_location(
            "srv_ident", os.path.join(ENGINE, "scripts", "lib", "second_review.py"))
        sr = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(sr)
        base = os.path.join(self.env.root, "ident")
        storage, local, foreign = (os.path.join(base, n) for n in ("storage", "local", "foreign"))
        run("git", "clone", "-q", "--bare", "--no-hardlinks", self.other, storage)
        run("git", "-C", storage, "worktree", "add", "-q", "--detach", local, "HEAD")
        run("git", "clone", "-q", "--no-hardlinks", "--separate-git-dir", os.path.join(storage, ".git"),
            self.other, foreign)
        a, b = sr.repo_identity(local), sr.repo_identity(foreign)
        self.assertEqual(a, os.path.realpath(storage))
        self.assertEqual(b, os.path.realpath(os.path.join(storage, ".git")))
        self.assertNotEqual(a, b)
        F = ws._fence_program()
        self.assertEqual(F.review_repo_identity(local), a)
        self.assertEqual(F.review_repo_identity(foreign), b)
        tip = run("git", "-C", local, "rev-parse", "HEAD").stdout.strip()
        rows = [dict(id="local-refusal", repo=local, repo_id=a, tip=tip, verdict="changes-requested", trigger="manual"),
                dict(id="foreign-pass", repo=foreign, repo_id=b, tip=tip, verdict="passed", trigger="manual")]
        chosen, _ = F.review_of(rows, tip, local)
        self.assertEqual(chosen["id"], "local-refusal")

    def test_second_review_repositories_whose_paths_differ_only_by_trailing_space_are_two_repositories(self):
        spec = importlib.util.spec_from_file_location(
            "srv_ident_ws", os.path.join(ENGINE, "scripts", "lib", "second_review.py"))
        sr = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(sr)
        base = os.path.join(self.env.root, "identws")
        plain, spaced = os.path.join(base, "storage"), os.path.join(base, "storage ")
        local, foreign = os.path.join(base, "local"), os.path.join(base, "foreign")
        run("git", "clone", "-q", "--bare", "--no-hardlinks", self.other, plain)
        run("git", "clone", "-q", "--bare", "--no-hardlinks", self.other, spaced)
        run("git", "-C", plain, "worktree", "add", "-q", "--detach", local, "HEAD")
        run("git", "-C", spaced, "worktree", "add", "-q", "--detach", foreign, "HEAD")
        a, b = sr.repo_identity(local), sr.repo_identity(foreign)
        self.assertEqual(a, os.path.realpath(plain))
        self.assertEqual(b, os.path.realpath(spaced))
        self.assertNotEqual(a, b)
        F = ws._fence_program()
        self.assertEqual(F.review_repo_identity(local), a)
        self.assertEqual(F.review_repo_identity(foreign), b)

    def test_second_review_identity_survives_awkward_folder_names_unchanged(self):
        spec = importlib.util.spec_from_file_location(
            "srv_ident_awk", os.path.join(ENGINE, "scripts", "lib", "second_review.py"))
        sr = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(sr)
        F = ws._fence_program()
        base = os.path.join(self.env.root, "identawk")
        for i, name in enumerate(["storage ", "storage\r", "stor\tage", "st\u00f6rage", "storage"]):
            bare = os.path.join(base, name)
            wt = os.path.join(base, "wt%d" % i)
            run("git", "clone", "-q", "--bare", "--no-hardlinks", self.other, bare)
            run("git", "-C", bare, "worktree", "add", "-q", "--detach", wt, "HEAD")
            want = os.path.realpath(bare)
            self.assertEqual(sr.repo_identity(wt), want, repr(name))
            self.assertEqual(F.review_repo_identity(wt), want, repr(name))

    def test_second_review_identity_ignores_the_git_environment_a_hook_inherits(self):
        spec = importlib.util.spec_from_file_location(
            "srv_ident_env", os.path.join(ENGINE, "scripts", "lib", "second_review.py"))
        sr = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(sr)
        F = ws._fence_program()
        local, foreign = self.other, os.path.join(self.env.root, "identenv")
        run("git", "clone", "-q", "--no-hardlinks", local, foreign)
        want = {"s": sr.repo_identity(foreign), "f": F.review_repo_identity(foreign)}
        self.assertNotEqual(want["f"], F.review_repo_identity(local))
        inherited = {"GIT_DIR": os.path.join(local, ".git"), "GIT_WORK_TREE": local,
                     "GIT_COMMON_DIR": os.path.join(local, ".git")}
        with patch.dict(os.environ, inherited):
            self.assertEqual(sr.repo_identity(foreign), want["s"])
            self.assertEqual(F.review_repo_identity(foreign), want["f"])

    def test_second_review_an_unlisted_repository_is_not_refused(self):
        self.declare("")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        _cc, (tip,) = self.finished("zach-opus-rv4")
        _merged, res = ws.merge_and_land("zach-opus-rv4", self.sid)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    # -- the fence: every other move of main ----------------------------------------
    def test_second_review_a_plain_merge_is_refused_by_the_fence_the_same_way(self):
        _cc, (tip,) = self.finished("zach-opus-rv5")
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rv5", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("SECOND REVIEW: refused", r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()
        self.verdict(tip, "passed")
        run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/zach-opus-rv5")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def notes_branch(self, name, *files):
        """A finished teammate whose one commit changes exactly `files`."""
        cc = self.make_cc(name)
        aid, _npath = self.spawn(name, cc=cc)
        for f in files:
            os.makedirs(os.path.join(cc, os.path.dirname(f)), exist_ok=True)
            with open(os.path.join(cc, f), "w") as out:
                out.write("notes of %s\n" % name)
        run("git", "-C", cc, "add", "--", *files)
        run("git", "-C", cc, "commit", "-q", "-m", "release notes")
        self.finish(aid)
        return run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip()

    def test_second_review_release_notes_only_land_with_no_review(self):
        # CEO 2026-10-23: a review of a release-notes job is useless.
        tip = self.notes_branch("newt-opus-rvnotes", "docs/CHANGELOG-NIGHTLY.md", "docs/CHANGELOG.md",
                                "docs/release-notes.json")
        merged, res = ws.merge_and_land("newt-opus-rvnotes", self.sid)
        self.assertEqual(merged, [(self.other, "cc/newt-opus-rvnotes")])
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)
        # A plain git merge goes through the fence the same way.
        tip2 = self.notes_branch("newt-opus-rvnotes2", "docs/release-notes.json")
        run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/newt-opus-rvnotes2")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip2)

    def test_second_review_release_notes_onto_a_release_branch_land_with_no_review(self):
        # Review rv-20261023T135732Z-c8bab74f-7a95: the notes are judged against
        # the branch they merge into, never the main checkout's HEAD, so a release
        # branch's own code is not taken for part of the notes change.
        run("git", "-C", self.other, "branch", "v1.3.0", "main")
        rel = os.path.join(self.env.root, "rel-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", rel, "v1.3.0")
        self.commit(rel, "release-only.txt")
        self.verdict(run("git", "-C", rel, "rev-parse", "HEAD").stdout.strip(), "passed")
        run("git", "-C", self.other, "worktree", "remove", rel)
        path = os.path.join(self.env.root, "other-wt", "newt-opus-rvrel")
        ws.register_cc(self.sid, "newt-opus-rvrel", self.other, path, "cc/newt-opus-rvrel", integration="v1.3.0")
        run("git", "-C", self.other, "worktree", "add", "-q", path, "-b", "cc/newt-opus-rvrel", "v1.3.0")
        ws.confirm_cc(self.sid, "newt-opus-rvrel", path, True)
        aid, _npath = self.spawn("newt-opus-rvrel", cc=path)
        os.makedirs(os.path.join(path, "docs"))
        self.commit(path, "docs/CHANGELOG.md", "notes of v1.3.0\n")
        tip = run("git", "-C", path, "rev-parse", "HEAD").stdout.strip()
        self.finish(aid)
        main_before = self.head()
        merged, res = ws.merge_and_land("newt-opus-rvrel", self.sid)
        self.assertEqual(merged, [(self.other, "cc/newt-opus-rvrel")])
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "v1.3.0^2").stdout.strip(), tip)
        self.assertEqual(self.head(), main_before)

    def test_second_review_release_notes_with_any_other_file_still_need_one(self):
        tip = self.notes_branch("newt-opus-rvmixed", "docs/CHANGELOG-NIGHTLY.md", "docs/other.md")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("newt-opus-rvmixed", self.sid)
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "cc/newt-opus-rvmixed", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()

    def test_second_review_a_fast_forward_and_a_direct_commit_are_refused_an_empty_commit_is_not(self):
        run("git", "-C", self.other, "branch", "ff-work")
        wt = os.path.join(self.env.root, "ff-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", wt, "ff-work")
        self.commit(wt, "ff.txt")
        tip = run("git", "-C", wt, "rev-parse", "HEAD").stdout.strip()
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--ff-only", "ff-work", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("SECOND REVIEW", r.stderr)
        self.assertEqual(self.head(), before)
        self.verdict(tip, "passed")
        run("git", "-C", self.other, "merge", "--ff-only", "ff-work")
        self.assertEqual(self.head(), tip)
        run("git", "-C", self.other, "commit", "-q", "--allow-empty", "-m", "move main, no file changes")
        with open(os.path.join(self.other, "direct.txt"), "w") as f:
            f.write("unreviewed\n")
        run("git", "-C", self.other, "add", "direct.txt")
        moved = self.head()
        r = run("git", "-C", self.other, "commit", "-q", "-m", "a direct change", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("SECOND REVIEW", r.stderr)
        self.assertEqual(self.head(), moved)
        run("git", "-C", self.other, "reset", "-q", "--hard", "HEAD")
        # A fast-forward to a merge made elsewhere: its tip is what lands, and a
        # verdict on the branch it merged does not cover its own merge.
        older = self.head()
        run("git", "-C", self.other, "commit", "-q", "--allow-empty", "-m", "move main again, no file changes")
        self.commit(wt, "ff2.txt")
        merged_tip = run("git", "-C", wt, "rev-parse", "HEAD").stdout.strip()
        self.verdict(merged_tip, "passed")
        side = os.path.join(self.env.root, "side-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", "-b", "side", side, older)
        run("git", "-C", side, "merge", "-q", "--no-ff", "--no-edit", "ff-work")
        run("git", "-C", side, "merge", "-q", "--no-ff", "--no-edit", "main")
        side_tip = run("git", "-C", side, "rev-parse", "HEAD").stdout.strip()
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--ff-only", "side", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % side_tip[:12], r.stderr)
        self.assertEqual(self.head(), before)

    def test_second_review_a_divergent_replacement_of_main_needs_a_review_a_rollback_does_not(self):
        """Review rv-20261009T023055Z-b51ebb97-bc65, finding 1: a `git reset
        --hard` of main to an unreviewed divergent commit went through, because
        every move that is not a fast-forward was taken for a rollback."""
        base = self.head()
        side = os.path.join(self.env.root, "replace-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", "-b", "replacement", side, base)
        self.commit(side, "unreviewed.txt", "unreviewed\n")
        tip = run("git", "-C", side, "rev-parse", "HEAD").stdout.strip()
        run("git", "-C", self.other, "commit", "-q", "--allow-empty", "-m", "advance main")
        moved = self.head()
        r = run("git", "-C", self.other, "reset", "-q", "--hard", tip, check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("SECOND REVIEW", r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), moved)
        run("git", "-C", self.other, "reset", "-q", "--hard", "HEAD")
        # A real rollback lands nothing: main goes back to a commit it already held.
        run("git", "-C", self.other, "reset", "-q", "--hard", base)
        self.assertEqual(self.head(), base)
        run("git", "-C", self.other, "reset", "-q", "--hard", moved)
        self.verdict(tip, "passed")
        run("git", "-C", self.other, "reset", "-q", "--hard", tip)
        self.assertEqual(self.head(), tip)

    def test_second_review_an_update_ref_with_no_old_value_is_judged_from_the_real_main(self):
        """Review rv-20261009T025426Z-15dae5ca-3cc1, finding 1: `git update-ref
        refs/heads/main <tip>` with no expected old value hands the fence an
        all-zero old value although main exists, and it was taken for a
        creation, so a divergent replacement landed unreviewed. Deleting main and
        creating it again did the same."""
        base = self.head()
        side = os.path.join(self.env.root, "update-ref-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", "-b", "update-side", side, base)
        self.commit(side, "never-reviewed.txt", "unreviewed replacement\n")
        tip = run("git", "-C", side, "rev-parse", "HEAD").stdout.strip()
        run("git", "-C", self.other, "commit", "-q", "--allow-empty", "-m", "advance main divergently")
        moved = self.head()
        r = run("git", "-C", self.other, "update-ref", "refs/heads/main", tip, check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), moved)
        # The same rule as any other move: back to a commit main held lands nothing.
        run("git", "-C", self.other, "update-ref", "refs/heads/main", base)
        self.assertEqual(self.head(), base)
        run("git", "-C", self.other, "update-ref", "refs/heads/main", moved)
        # Deleted and created again: nothing says main ever held the new commit.
        run("git", "-C", self.other, "update-ref", "-d", "refs/heads/main")
        r = run("git", "-C", self.other, "update-ref", "refs/heads/main", tip, check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertNotEqual(run("git", "-C", self.other, "rev-parse", "-q", "--verify", "refs/heads/main",
                                check=False).stdout.strip(), tip)
        self.verdict(tip, "passed")
        run("git", "-C", self.other, "update-ref", "refs/heads/main", tip)
        self.assertEqual(self.head(), tip)

    def test_second_review_merge_lands_the_commit_that_passed_not_a_newer_branch_tip(self):
        """Review rv-20261009T025426Z-15dae5ca-3cc1, finding 2: `workspaces.sh
        merge` checked the branch tip, then had Git merge the branch NAME, so a
        commit added to the branch after the check (even while the merge gate
        ran) landed unreviewed. Lease fencing is off here; review acceptance
        remains active in both the land command and direct Git backstop."""
        run("bash", self.fences, "off", "--repo", self.other, "--entity", self.decl,
            "--reason", "exercise exact reviewed-tip land with lease fencing off")
        cc, (reviewed,) = self.finished("zach-opus-rv9")
        self.verdict(reviewed, "passed")
        checked = ws._review_check
        late = []

        def check_then_advance(todo, *rest):
            out = checked(todo, *rest)
            self.commit(cc, "unreviewed-after-check.txt", "created after the tip passed\n")
            late.append(run("git", "-C", cc, "rev-parse", "HEAD").stdout.strip())
            return out

        with patch.object(ws, "_review_check", side_effect=check_then_advance):
            with self.assertRaises(ws.SpecError):
                ws.merge_and_land("zach-opus-rv9", self.sid)  # the newer commit is not landed, so no land
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), reviewed)
        self.assertFalse(os.path.exists(os.path.join(self.other, "unreviewed-after-check.txt")))
        self.assertNotEqual(run("git", "-C", self.other, "merge-base", "--is-ancestor", late[0], "main",
                                check=False).returncode, 0)
        self.assertTrue(os.path.isdir(cc))                            # the newer work is kept, not landed

    def test_second_review_a_merge_with_changes_of_its_own_needs_a_review_of_the_merge_itself(self):
        """Review rv-20261009T023055Z-b51ebb97-bc65, finding 2: a merge made
        elsewhere, whose first parent is main, carried an extra unreviewed file;
        a fast-forward to it was checked only against its second parent."""
        _cc, (reviewed,) = self.finished("zach-opus-rv6")
        self.verdict(reviewed, "passed")
        carrier = os.path.join(self.env.root, "carrier-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", "-b", "carrier", carrier, "main")
        run("git", "-C", carrier, "merge", "-q", "--no-ff", "--no-commit", "cc/zach-opus-rv6")
        self.commit(carrier, "unreviewed.txt", "added during the merge, absent from the reviewed tip\n")
        evil = run("git", "-C", carrier, "rev-parse", "HEAD").stdout.strip()
        self.assertEqual(run("git", "-C", carrier, "rev-parse", "HEAD^2").stdout.strip(), reviewed)
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--ff-only", "carrier", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % evil[:12], r.stderr)
        self.assertEqual(self.head(), before)
        run("git", "-C", self.other, "reset", "-q", "--hard", "HEAD")
        # The same merge made in the main checkout itself is refused the same way.
        run("git", "-C", self.other, "merge", "-q", "--no-ff", "--no-commit", "cc/zach-opus-rv6")
        with open(os.path.join(self.other, "unreviewed.txt"), "w") as f:
            f.write("added during the merge, absent from the reviewed tip\n")
        run("git", "-C", self.other, "add", "unreviewed.txt")
        r = run("git", "-C", self.other, "commit", "-q", "--no-edit", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("SECOND REVIEW", r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()
        run("git", "-C", self.other, "reset", "-q", "--hard", "HEAD")
        # Control: a clean merge made elsewhere brings in only the reviewed work.
        clean = os.path.join(self.env.root, "clean-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", "-b", "clean", clean, "main")
        run("git", "-C", clean, "merge", "-q", "--no-ff", "--no-edit", "cc/zach-opus-rv6")
        run("git", "-C", self.other, "merge", "--ff-only", "clean")
        self.assertEqual(self.head(), run("git", "-C", clean, "rev-parse", "HEAD").stdout.strip())
        # And the merge with its own changes lands once that merge itself passed.
        run("git", "-C", self.other, "reset", "-q", "--hard", before)
        self.verdict(evil, "passed")
        run("git", "-C", self.other, "merge", "--ff-only", "carrier")
        self.assertEqual(self.head(), evil)

    def test_second_review_a_codex_branch_lands_only_reviewed(self):
        """Sage's catch 1: codex/ work never enters the registry; Rich lands it
        with a plain merge in the main checkout, which the fence sees."""
        run("git", "-C", self.other, "branch", "codex/the-fix")
        wt = os.path.join(self.env.root, "codex-wt")
        run("git", "-C", self.other, "worktree", "add", "-q", wt, "codex/the-fix")
        self.commit(wt, "codex.txt")
        tip = run("git", "-C", wt, "rev-parse", "HEAD").stdout.strip()
        before = self.head()
        r = run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "codex/the-fix", check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("no second review of %s" % tip[:12], r.stderr)
        self.assertEqual(self.head(), before)
        self.no_merge_left()
        self.verdict(tip, "passed")
        run("git", "-C", self.other, "merge", "--no-ff", "--no-edit", "codex/the-fix")
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_listed_repository_without_its_launcher_is_still_refused(self):
        """Review rv-20261009T031207Z-ba444a8a-607a, finding 1: with the fence
        launcher absent, the land command read "no review ledger" as "not
        reviewed" and merged unreviewed work into a repository the declaration
        lists. Whether a repository is reviewed comes from the declaration (here
        the work's own entity's orchestration.config); the ledger is the one
        the declaration names."""
        with open(os.path.join(self.entity, "orchestration.config"), "w") as f:
            f.write('SECOND_REVIEW_REPOS="other"\n')
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv10")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv10", self.sid)
        self.assertIn("SECOND REVIEW: cc/zach-opus-rv10 was not merged into", str(e.exception))
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv10", self.sid)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_listed_repository_whose_launcher_cannot_be_read_is_refused(self):
        """The same finding's other half: a listed repository whose review setup
        cannot be established is refused, even with a passing verdict."""
        with open(os.path.join(self.entity, "orchestration.config"), "w") as f:
            f.write('SECOND_REVIEW_REPOS="other"\n')
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        _cc, (tip,) = self.finished("zach-opus-rv11")
        self.verdict(tip, "passed")
        before = self.head()
        os.chmod(hook, 0)
        try:
            with self.assertRaises(ws.SpecError) as e:
                ws.merge_and_land("zach-opus-rv11", self.sid)
        finally:
            os.chmod(hook, 0o755)
        self.assertIn("cannot be read", str(e.exception))
        self.assertEqual(self.head(), before)

    def test_second_review_a_governing_declaration_that_cannot_be_read_is_refused(self):
        """Review rv-20261009T033652Z-f3bfe22f-9372, finding 1 (its fixture
        review_policy_edges.py, UNREADABLE_CONFIG): with the launcher gone, an
        unreadable governing orchestration.config was skipped as if absent, and
        the repository it lists merged unreviewed work. A declaration that
        cannot be read leaves applicability unknown, so nothing is merged; an
        absent one still lists nothing."""
        config = os.path.join(self.entity, "orchestration.config")
        with open(config, "w") as f:
            f.write('SECOND_REVIEW_REPOS="other"\n')
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv12")
        self.verdict(tip, "passed")              # even a passing tip: applicability is unknown
        before = self.head()
        os.chmod(config, 0)
        try:
            with self.assertRaises(ws.SpecError) as e:
                ws.merge_and_land("zach-opus-rv12", self.sid)
        finally:
            os.chmod(config, 0o600)
        self.assertIn("SECOND REVIEW", str(e.exception))
        self.assertIn("%s cannot be read" % config, str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.other, "work0.txt")))
        _merged, res = ws.merge_and_land("zach-opus-rv12", self.sid)     # readable again: it passed
        self.assertTrue(res["landed"])

    def test_second_review_the_entity_the_fence_was_installed_from_governs_without_the_launcher(self):
        """Review rv-20261009T033652Z-f3bfe22f-9372, finding 2 (its fixture,
        FENCE_ENTITY): `operator-fences.sh install --entity` here names
        self.decl, whose SECOND_REVIEW_REPOS lists the repository, while the
        teammate was spawned from self.entity, which lists nothing. With the
        launcher removed, the land command never read self.decl and merged
        unreviewed work. The entity is recovered from the fence registry, which
        records it per repository, so a later install from another entity for
        another repository does not lose it.

        ONLY THAT DECLARATION MAY DECIDE HERE (nightly 20261010T014856Z-dcdee3e7:
        the mutant that stops reading the registry's entity survived). The
        install's own record ("reviewed", which comes first) refused the land
        by itself, so it is made to record nothing: the repository is installed
        while self.decl lists nothing and listed there afterwards, without a new
        install. Then self.decl, named only by the registry, is the one thing
        that says this repository is reviewed."""
        self.declare("")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        self.declare("other")
        entries = [e for e in self.registry()["repositories"].values()
                   if os.path.realpath(e.get("entity") or "") == os.path.realpath(self.decl)]
        self.assertEqual([e.get("reviewed") for e in entries], [False], entries)
        third = self.env.repo("third")
        elsewhere = os.path.join(self.env.root, "elsewhere-entity")
        os.makedirs(elsewhere)
        with open(os.path.join(elsewhere, "orchestration.config"), "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n'
                    'SECOND_REVIEW_REPOS=""\n')
        run("bash", self.fences, "install", "--repo", third, "--entity", elsewhere)
        self.assertFalse(os.path.exists(os.path.join(self.entity, "orchestration.config")))
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv13")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv13", self.sid)
        self.assertIn("SECOND REVIEW: cc/zach-opus-rv13 was not merged into", str(e.exception))
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.other, "work0.txt")))
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv13", self.sid)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def registry(self, change=None):
        """The fence registry `operator-fences.sh install` writes; `change`
        edits it in place (to seed the shape an older installer wrote)."""
        path = os.path.join(ws.land_locks_dir(), "operator-fences.json")
        with open(path) as f:
            reg = json.load(f)
        if change:
            change(reg)
            with open(path, "w") as f:
                json.dump(reg, f)
        return reg

    def install_unrelated(self, name="third"):
        """Another repository's fence, installed from another entity whose
        declaration lists only that repository. Returns its orchestration.config."""
        repo = self.env.repo(name)
        entity = os.path.join(self.env.root, "unrelated-entity")
        os.makedirs(entity)
        config = os.path.join(entity, "orchestration.config")
        with open(config, "w") as f:
            f.write('OPERATOR_FENCES="on"\nSECOND_REVIEW_REPOS="%s"\n' % name)
        run("bash", self.fences, "install", "--repo", repo, "--entity", entity)
        return config

    def legacy_registry(self, entity, stamps, keep=None):
        """Rewrite the fence registry into the shape the earlier installer left:
        no entry carries an "entity", each carries the `installed` time in
        `stamps` ({main checkout: ISO time}), the top-level entity is `entity`
        (that installer's last install), and only the entries in `keep` remain
        (its uninstall removed an entry and left the top-level entity as it was)."""
        def change(reg):
            reg["entity"] = os.path.realpath(entity)
            for main in list(reg["repositories"]):
                if keep is not None and main not in keep:
                    del reg["repositories"][main]
                    continue
                reg["repositories"][main].pop("entity", None)
                reg["repositories"][main].pop("reviewed", None)
                reg["repositories"][main]["installed"] = stamps[main]
        self.registry(change)

    def assert_unknown_until_installed(self, name):
        """With the launcher removed, a land into self.other, whose registry
        entry carries no entity, is refused naming the install command that
        records it, even with a passing verdict; once that command has run, the
        declaration it names governs: refused with no verdict on the tip, landed
        with one."""
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished(name)
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land(name, self.sid)
        self.assertIn("SECOND REVIEW: cc/%s was not merged into" % name, str(e.exception))
        self.assertIn("the entity its fence was installed from is not recorded", str(e.exception))
        self.assertIn("operator-fences.sh install --repo %s" % self.other, str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.other, "work0.txt")))
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        self.assertEqual(self.registry()["repositories"][self.other].get("entity"), os.path.realpath(self.decl))
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land(name, self.sid)
        self.assertIn("no second review of %s" % tip[:12], str(e.exception))
        self.assertEqual(self.head(), before)
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land(name, self.sid)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_registry_entry_from_before_entities_were_recorded_is_unknown(self):
        """Review rv-20261009T040812Z-9efed042-d03e, finding 1: an entry the
        earlier installer wrote carries only common, chain and installed, and
        nothing in it names the entity it was installed from. The entity is
        NEVER inferred from timestamps or the top-level entity: such an entry
        is unknown and its repository's land refused, naming the
        `operator-fences.sh install --repo ... --entity ...` that records it.
        Here the registry holds the one entry, from the one install, so even
        the history where inferring would have been right is refused; a fresh
        install always records the entity, so only an old install sees this."""
        self.legacy_registry(self.decl, {self.other: "2026-10-07T08:00:00Z"})
        self.assert_unknown_until_installed("zach-opus-rv14")

    def test_second_review_a_legacy_entry_after_the_latest_install_was_uninstalled_is_unknown(self):
        """The fixture's LATEST_UNINSTALLED history (fixtures/legacy_ownership.py
        of review rv-20261009T040812Z-9efed042-d03e): the earlier installer
        installed self.other from self.decl, then a third repository from an
        unrelated entity, then uninstalled the third. Its top-level entity is
        the unrelated one and the only entry left is self.other's. Inferring
        from the newest `installed` time credited the unrelated entity, an
        upgrade install of the third repository recorded that permanently, and
        with the launcher removed unreviewed work reached main."""
        config = self.install_unrelated()
        third = next(m for m in self.registry()["repositories"] if m != self.other)
        self.legacy_registry(os.path.dirname(config), {self.other: "2026-10-07T08:00:00Z",
                                                       third: "2026-10-07T08:01:00Z"}, keep=[self.other])
        run("bash", self.fences, "install", "--repo", third, "--entity", os.path.dirname(config))
        self.assertIsNone(self.registry()["repositories"][self.other].get("entity"))
        self.assert_unknown_until_installed("zach-opus-rv17")

    def test_second_review_a_legacy_entry_installed_in_the_same_second_as_another_is_unknown(self):
        """The fixture's SAME_SECOND history (fixtures/legacy_ownership.py of
        review rv-20261009T040812Z-9efed042-d03e): the earlier installer
        installed self.other from self.decl and a third repository from an
        unrelated entity within one second. Both `installed` times tie, so
        inferring credited the unrelated entity to both, an upgrade install
        recorded that permanently, and with the launcher removed unreviewed
        work reached main."""
        config = self.install_unrelated()
        third = next(m for m in self.registry()["repositories"] if m != self.other)
        self.legacy_registry(os.path.dirname(config), {self.other: "2026-10-07T08:00:00Z",
                                                       third: "2026-10-07T08:00:00Z"})
        run("bash", self.fences, "install", "--repo", third, "--entity", os.path.dirname(config))
        self.assertIsNone(self.registry()["repositories"][self.other].get("entity"))
        self.assert_unknown_until_installed("zach-opus-rv18")

    def test_second_review_a_repository_whose_governing_entity_is_unknown_is_refused(self):
        """Review rv-20261009T035450Z-882c7e75-0eab, finding 1: a registry
        entry whose entity is not recorded leaves the repository's
        governing declaration unknown, and nothing is merged, even with a
        passing verdict, until `operator-fences.sh install` records it."""
        self.install_unrelated()
        self.registry(lambda reg: reg["repositories"][self.other].pop("entity"))
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv15")
        self.verdict(tip, "passed")
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land("zach-opus-rv15", self.sid)
        self.assertIn("SECOND REVIEW: cc/zach-opus-rv15 was not merged into", str(e.exception))
        self.assertIn("the entity its fence was installed from is not recorded", str(e.exception))
        self.assertIn("operator-fences.sh install --repo %s" % self.other, str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.other, "work0.txt")))
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        _merged, res = ws.merge_and_land("zach-opus-rv15", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_only_the_declarations_governing_the_landed_repository_are_read(self):
        """Review rv-20261009T035450Z-882c7e75-0eab, finding 2 (its fixture,
        UNRELATED_CONFIG): the lookup read every entity in the machine-wide
        registry, so an unreadable declaration governing only another
        repository blocked this land. Only the entities governing the
        repositories being landed are read: the one governing this repository
        through the registry still refuses when it cannot be read; the other
        repository's does not."""
        unrelated = self.install_unrelated()
        own = os.path.join(self.decl, "orchestration.config")
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv16")
        self.verdict(tip, "passed")
        before = self.head()
        os.chmod(own, 0)
        try:
            with self.assertRaises(ws.SpecError) as e:
                ws.merge_and_land("zach-opus-rv16", self.sid)
        finally:
            os.chmod(own, 0o600)
        self.assertIn("%s cannot be read" % own, str(e.exception))
        self.assertEqual(self.head(), before)
        os.chmod(unrelated, 0)
        try:
            _merged, res = ws.merge_and_land("zach-opus-rv16", self.sid)
        finally:
            os.chmod(unrelated, 0o600)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    # -- "not reviewed" needs positive evidence for every governing entity ----------
    # Review rv-20261009T042243Z-42ecc969-5f85 was the fourth in a row to find a
    # way the answer fell back to "not reviewed" with the launcher gone. A
    # repository is now unreviewed only when every governing declaration was
    # read and does not list it, or the entity is a spawning one whose
    # directory exists with no orchestration.config in it. Each case below was
    # a way that answer came out "not reviewed" without that evidence.

    def refused_unread(self, name, *expected):
        """merge_and_land(name) refuses, merging nothing, naming each of `expected`."""
        before = self.head()
        with self.assertRaises(ws.SpecError) as e:
            ws.merge_and_land(name, self.sid)
        self.assertIn("SECOND REVIEW: cc/%s was not merged into" % name, str(e.exception))
        for text in expected:
            self.assertIn(text, str(e.exception))
        self.assertEqual(self.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.other, "work0.txt")))
        return str(e.exception)

    def unfenced(self):
        """No fence for self.other: launcher and registry entry removed by
        `operator-fences.sh uninstall`, so only the spawning entity governs."""
        run("bash", self.fences, "uninstall", "--repo", self.other, "--entity", self.decl)
        self.assertNotIn(self.other, self.registry()["repositories"])
        self.assertFalse(os.path.exists(os.path.join(self.other, ".git", "hooks", "reference-transaction")))

    def set_entity(self, name, entity):
        rec = ws._resolve(name, self.sid)
        rec["entity"] = entity
        ws.save_agent(rec)

    def test_second_review_a_recorded_entity_whose_declaration_is_gone_is_refused(self):
        """Review rv-20261009T042243Z-42ecc969-5f85, finding 1 (its fixture
        missing_governing_config.py): once install had recorded the entity,
        deleting its orchestration.config, or retiring the entity's directory,
        was read as "lists nothing", and with the launcher gone an unreviewed
        tip landed. The registry says a declaration governed this repository,
        so its absence is not evidence of anything: refused, naming how to
        restore it or record the entity that governs it now."""
        config = os.path.join(self.decl, "orchestration.config")
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv19")
        os.rename(config, config + ".away")
        self.refused_unread("zach-opus-rv19", "the fence registry records that %s's fence was installed from %s"
                            % (self.other, os.path.realpath(self.decl)), "%s is missing" % config,
                            "operator-fences.sh install --repo %s" % self.other)
        retired = self.decl + ".retired"
        os.rename(self.decl, retired)                               # the entity's worktree retired
        self.refused_unread("zach-opus-rv19", "%s is gone with its directory" % config)
        with open(self.decl, "w") as f:                             # ENOTDIR: a file where it was
            f.write("not an entity\n")
        self.refused_unread("zach-opus-rv19", "%s is gone with its directory" % config)
        os.remove(self.decl)
        os.rename(retired, self.decl)
        os.rename(config + ".away", config)
        self.refused_unread("zach-opus-rv19", "no second review of %s" % tip[:12])
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv19", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_a_spawning_entity_that_no_longer_exists_is_refused(self):
        """A spawning entity lists nothing only when its directory exists with no
        orchestration.config in it. One whose directory is gone (a retired
        worktree) or is not a directory gives no evidence either way, so it was
        a way to "not reviewed" with no fence installed. Recreating the
        directory without a declaration says it never had one."""
        self.unfenced()
        _cc, _tips = self.finished("zach-opus-rv20")
        gone = os.path.join(self.env.root, "retired-entity")
        self.set_entity("zach-opus-rv20", gone)
        self.refused_unread("zach-opus-rv20", "the entity %s that governs this work is gone" % gone)
        with open(gone, "w") as f:
            f.write("not an entity\n")
        self.refused_unread("zach-opus-rv20", "the entity %s that governs this work is gone" % gone)
        os.remove(gone)
        os.makedirs(gone)
        _merged, res = ws.merge_and_land("zach-opus-rv20", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_a_declaration_that_is_a_link_to_nothing_is_refused(self):
        """An orchestration.config that is a symbolic link to nothing fails to
        open exactly as an absent one does, but something was there: it is not
        an entity that never had a declaration."""
        self.unfenced()
        config = os.path.join(self.entity, "orchestration.config")
        os.symlink(os.path.join(self.env.root, "moved-away.config"), config)
        _cc, _tips = self.finished("zach-opus-rv21")
        self.refused_unread("zach-opus-rv21", "%s is a link to nothing" % config)
        os.remove(config)
        _merged, res = ws.merge_and_land("zach-opus-rv21", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_work_that_no_declaration_governs_is_refused(self):
        """No record names an entity, this run resolved none and the fence
        registry records none: nothing was read at all, and that was "not
        reviewed". Naming the entity for this run lets the land read it."""
        self.unfenced()
        _cc, _tips = self.finished("zach-opus-rv22")
        self.set_entity("zach-opus-rv22", "")
        saved = os.environ.pop("RICHOS_ENTITY_ROOT_RESOLVED", None)
        try:
            self.refused_unread("zach-opus-rv22", "no declaration is known to govern")
            os.environ["RICHOS_ENTITY_ROOT_RESOLVED"] = self.entity
            _merged, res = ws.merge_and_land("zach-opus-rv22", self.sid)
            self.assertTrue(res["landed"])
        finally:
            if saved is None:
                os.environ.pop("RICHOS_ENTITY_ROOT_RESOLVED", None)
            else:
                os.environ["RICHOS_ENTITY_ROOT_RESOLVED"] = saved

    def test_second_review_a_declaration_whose_assignment_cannot_be_read_is_refused(self):
        """A declaration bash cannot source cleanly (an unterminated quote, a
        command that is not found, a last command that fails), or whose value
        is not names (a shell expansion to a path), was read as listing
        nothing, or as a value that matches nothing. Neither review-watch nor
        the land can say what it lists, so nothing is merged until it reads."""
        config = os.path.join(self.decl, "orchestration.config")
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv23")
        for line in ('SECOND_REVIEW_REPOS="other\n', 'SECOND_REVIEW_REPOS="other"\nfalse\n',
                     'SECOND_REVIEW_REPOS="$HOME/other"\n', 'SECOND_REVIEW_REPOS=richos other\n'):
            with open(config, "w") as f:
                f.write('OPERATOR_FENCES="on"\n' + line)
            self.refused_unread("zach-opus-rv23", "SECOND_REVIEW_REPOS in %s cannot be read" % config)
        self.declare("other")
        self.refused_unread("zach-opus-rv23", "no second review of %s" % tip[:12])
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv23", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_a_fence_registry_without_its_repositories_is_unknown(self):
        """A fence registry that exists but holds no `repositories` table is not
        the shape install writes. It was read as naming no entity, so the
        entity it should name was never read."""
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv24")
        saved = self.registry()
        self.registry(lambda reg: reg.pop("repositories"))
        self.refused_unread("zach-opus-rv24", "not the registry's shape")
        self.registry(lambda reg: reg.update(saved))
        self.refused_unread("zach-opus-rv24", "no second review of %s" % tip[:12])

    def test_second_review_an_unreadable_launcher_is_refused_even_where_no_declaration_lists_it(self):
        """The land honors the review ledger a fence launcher carries even where
        no declaration lists the repository (the fence would ask it anyway).
        An existing launcher that cannot be read was taken as carrying none;
        whether that ledger applies is now unknown, so nothing is merged."""
        self.declare("")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        _cc, _tips = self.finished("zach-opus-rv25")
        os.chmod(hook, 0)
        try:
            self.refused_unread("zach-opus-rv25", "fence launcher %s cannot be read" % hook)
        finally:
            os.chmod(hook, 0o755)
        _merged, res = ws.merge_and_land("zach-opus-rv25", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_a_repository_that_cannot_be_read_is_refused_even_where_no_declaration_lists_it(self):
        """With the repository unreadable, neither the registry entry for it nor
        its launcher can be found, and an unlisted repository was taken as
        unreviewed. Shown at the land check itself, with only Git's answers
        for this repository failing: first everywhere (the registry entry
        cannot be found), then only where the launcher is looked for."""
        self.declare("")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        F = ws._fence_program()
        real = ws.git

        def unreadable(cwd, *args, **kw):
            if "--git-common-dir" in args:
                return 128, "", "fatal: not a git repository"
            return real(cwd, *args, **kw)

        todo = [(self.other, "cc/unreadable", self.other, "b" * 40)]
        with patch.object(ws, "_fence_program", return_value=F), \
                patch.object(ws, "git", side_effect=unreadable):
            with patch.object(F, "repo_paths", return_value=None):
                with self.assertRaises(ws.SpecError) as e:
                    ws._review_check(todo, [self.entity])
            self.assertIn("SECOND REVIEW: cc/unreadable was not merged into", str(e.exception))
            self.assertIn("the repository %s cannot be read, so the fence registry entry that governs it cannot "
                          "be found" % self.other, str(e.exception))
            with self.assertRaises(ws.SpecError) as e:
                ws._review_check(todo, [self.entity])
            self.assertIn("the repository %s cannot be read, so whether its fence launcher carries a review "
                          "ledger cannot be established" % self.other, str(e.exception))
        self.assertEqual(ws._review_check(todo, [self.entity]), {})    # readable, and listed nowhere

    # -- the install is the one durable record ----------------------------------------
    # Review rv-20261009T044823Z-fc8c7569-5919 was the eighth to find a way a
    # declaration read at land time could disappear. `operator-fences.sh install`
    # now records, per repository, whether its declaration listed it then; the
    # land requires a review where that record says so OR a current declaration
    # lists it, so removing or breaking a declaration afterwards never switches
    # the review off until the next install.

    def test_second_review_an_installed_repository_stays_reviewed_until_the_next_install(self):
        """The review's finding 1 (its fixture review_fallbacks.py,
        SPAWNING_CONFIG_AFTER_REMOVAL), for an installed repository: the
        spawning entity's declaration listed the repository and was deleted,
        and the declaration the fence was installed from no longer lists it, so
        with the launcher gone every declaration read said "not reviewed" and an
        unreviewed tip landed. The install recorded that the repository is
        reviewed, so it still is; the next install, from a declaration that does
        not list it, is what switches it off."""
        self.assertIs(self.registry()["repositories"][self.other].get("reviewed"), True)
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv26")
        self.declare("")                                            # the install's declaration edited
        self.refused_unread("zach-opus-rv26", "no second review of %s" % tip[:12])
        spawning = os.path.join(self.entity, "orchestration.config")
        with open(spawning, "w") as f:
            f.write('SECOND_REVIEW_REPOS="other"\n')
        self.refused_unread("zach-opus-rv26", "no second review of %s" % tip[:12])
        os.remove(spawning)                                         # the spawning declaration deleted
        self.refused_unread("zach-opus-rv26", "no second review of %s" % tip[:12])
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        self.assertIs(self.registry()["repositories"][self.other].get("reviewed"), False)
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _merged, res = ws.merge_and_land("zach-opus-rv26", self.sid)
        self.assertTrue(res["landed"])
        self.assertEqual(run("git", "-C", self.other, "rev-parse", "main^2").stdout.strip(), tip)

    def test_second_review_a_never_installed_repositorys_deleted_spawning_declaration_is_not_kept(self):
        """The same finding with no install: nothing records that the spawning
        entity ever had a declaration, and the repository has no fence for a
        plain `git merge` either, so the land asks no more of it (by decision,
        not oversight): listed, refused; the declaration deleted, it lands."""
        self.unfenced()
        spawning = os.path.join(self.entity, "orchestration.config")
        with open(spawning, "w") as f:
            f.write('SECOND_REVIEW_REPOS="other"\n')
        _cc, (tip,) = self.finished("zach-opus-rv27")
        self.refused_unread("zach-opus-rv27", "no second review of %s" % tip[:12])
        os.remove(spawning)
        _merged, res = ws.merge_and_land("zach-opus-rv27", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_a_registry_or_launcher_that_is_a_link_to_nothing_is_refused(self):
        """The review's finding 2 (REGISTRY_LINK_TARGET_MISSING,
        LAUNCHER_LINK_TARGET_MISSING): both were read as absent, so a registry
        whose target was gone hid the entity whose declaration lists the
        repository, and a launcher whose target was gone hid the ledger it
        carries. Something was installed there: refused, as a declaration that
        is a link to nothing already was."""
        registry = os.path.join(ws.land_locks_dir(), "operator-fences.json")
        os.remove(os.path.join(self.other, ".git", "hooks", "reference-transaction"))
        _cc, (tip,) = self.finished("zach-opus-rv28")
        os.rename(registry, registry + ".away")
        os.symlink(registry + ".gone", registry)
        self.refused_unread("zach-opus-rv28", "the fence registry %s is a link to nothing" % registry)
        os.remove(registry)
        os.rename(registry + ".away", registry)
        self.refused_unread("zach-opus-rv28", "no second review of %s" % tip[:12])
        self.declare("")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        os.remove(hook)
        os.symlink(hook + ".gone", hook)
        self.refused_unread("zach-opus-rv28", "fence launcher %s is a link to nothing" % hook)
        os.remove(hook)
        _merged, res = ws.merge_and_land("zach-opus-rv28", self.sid)
        self.assertTrue(res["landed"])

    def install_reads_listed(self, name, tip, body):
        """`operator-fences.sh install` from a declaration whose text after the
        usual first lines is `body`: it succeeds, records that self.other is
        reviewed, bakes the review ledger into the launcher, and the land of
        `name` (whose tip is `tip`) is refused for want of a verdict."""
        config = os.path.join(self.decl, "orchestration.config")
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        with open(config, "w") as f:
            f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n' + body)
        r = run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl, check=False)
        self.assertEqual(r.returncode, 0, "%r: %s%s" % (body, r.stdout, r.stderr))
        self.assertTrue(self.registry()["repositories"][self.other].get("reviewed"), body)
        with open(hook) as h:
            self.assertIn('OPERATOR_FENCES_REVIEWS="%s"' % self.ledger, h.read(), body)
        self.refused_unread(name, "no second review of %s" % tip[:12])

    SHELL_FORMS = ('readonly SECOND_REVIEW_REPOS="other"\n', 'declare -x SECOND_REVIEW_REPOS="other"\n',
                   'OPERATOR_FENCES="on"; SECOND_REVIEW_REPOS="other"\n', 'SECOND_REVIEW_REPOS=other;\n',
                   'SECOND_REVIEW_REPOS=other;:\n', 'SECOND_REVIEW_REPOS=other&&:\n',
                   'export SECOND_REVIEW_REPOS="other"\n', "SECOND_REVIEW_REPOS='other'\n",
                   ' SECOND_REVIEW_REPOS=other  # a note\n',
                   'SECOND_REVIEW_REPOS="other"\nNOTE="see SECOND_REVIEW_REPOS"\n',
                   '# SECOND_REVIEW_REPOS  (used by: review-watch)\nSECOND_REVIEW_REPOS_NOTE="x"\n'
                   'SECOND_REVIEW_REPOS="other"\n')

    def test_second_review_an_assignment_in_any_shell_form_is_read_as_bash_reads_it(self):
        """The review's finding 3 (READONLY_ASSIGNMENT, DECLARE_ASSIGNMENT,
        ASSIGNMENT_AFTER_SEMICOLON) and rv-20261009T050654Z-e21238ff-7685's
        finding 2 (`other;`, `other&&:`): a reader of lines skipped these, or
        took `other;` for the value, and recorded "not reviewed". Bash sets
        `other` for every one of them, and so does review_repos now: install
        records the review and the land refuses until a passing verdict."""
        _cc, (tip,) = self.finished("zach-opus-rv29")
        for body in self.SHELL_FORMS:
            with self.subTest(body):
                self.install_reads_listed("zach-opus-rv29", tip, body)
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv29", self.sid)
        self.assertTrue(res["landed"])

    # -- one reader, and it asks the shell -----------------------------------------------
    # Review rv-20261009T050654Z-e21238ff-7685: install skipped forms the land
    # refused (finding 1), and the readers took `other;` for the value where a shell
    # sets `other` (finding 2). Review rv-20261009T052655Z-daf09e19-0bc6: a reader of
    # lines also read decoys in a skipped `if` or a heredoc and missed a key split by
    # a backslash-newline. One function, operator_fences.review_repos, reads
    # SECOND_REVIEW_REPOS for install, the land and review-watch, and it sources the
    # declaration in a clean bash and checks the value bash sets.

    UNREADABLE_FORMS = ('SECOND_REVIEW_REPOS="other', 'SECOND_REVIEW_REPOS="other"\nfalse',
                        'SECOND_REVIEW_REPOS="other"\nexit 0', 'SECOND_REVIEW_REPOS="other"\ntrap "echo x" EXIT',
                        'SECOND_REVIEW_REPOS="$HOME"', 'SECOND_REVIEW_REPOS="../other"',
                        'SECOND_REVIEW_REPOS="other"\r', 'SECOND_REVIEW_REPOS=richos other')

    def reader(self, text):
        """review_repos of an orchestration.config holding `text`."""
        folder = os.path.join(self.env.root, "reader-entity")
        os.makedirs(folder, exist_ok=True)
        config = os.path.join(folder, "orchestration.config")
        with open(config, "w") as f:
            f.write(text)
        return ws._fence_program().review_repos(config)

    def test_second_review_the_reader_returns_the_value_bash_sets(self):
        """The value bash leaves after sourcing the declaration: unset is
        None, set is its names, whatever shell form set it; a value that is
        not names, or a declaration bash cannot source cleanly, in time and
        printing nothing else, is an error."""
        for text, names in (("", None), ('# SECOND_REVIEW_REPOS="other"\n', None),
                            ('  # SECOND_REVIEW_REPOS=other;\nSECOND_REVIEW_REPOS_NOTE="x"\n', None),
                            ('NOTE="see SECOND_REVIEW_REPOS"\n', None),
                            ('SECOND_REVIEW_REPOS="other"\nunset SECOND_REVIEW_REPOS\n', None),
                            ('set -u\n', None),
                            ('set -u\nSECOND_REVIEW_REPOS="richos"\n', ["richos"]),
                            ('SECOND_REVIEW_REPOS="richos"\n', ["richos"]),
                            ('SECOND_REVIEW_REPOS="a.b  c_d e-f 9"\n', ["a.b", "c_d", "e-f", "9"]),
                            ('SECOND_REVIEW_REPOS=""\n', []),
                            ('SECOND_REVIEW_REPOS="one"\nSECOND_REVIEW_REPOS="two"\n', ["two"]),
                            ('if true; then\nSECOND_REVIEW_REPOS="other"\nfi\n', ["other"]),
                            ('echo noise\necho more >&2\nSECOND_REVIEW_REPOS="other"\n', ["other"])):
            self.assertEqual(self.reader(text), (names, ""), text)
        for body in self.SHELL_FORMS + tuple(b for _label, b in self.SHELL_CONTEXT):
            self.assertEqual(self.reader('OPERATOR_FENCES="on"\n' + body), (["other"], ""), body)
        for line in self.UNREADABLE_FORMS:
            names, why = self.reader('OPERATOR_FENCES="on"\n%s\n' % line)
            self.assertIsNone(names, line)
            self.assertTrue(why, line)
        F = ws._fence_program()
        with patch.object(F, "REVIEW_READ_SECONDS", 1):
            folder = os.path.join(self.env.root, "slow-entity")
            os.makedirs(folder)
            with open(os.path.join(folder, "orchestration.config"), "w") as f:
                f.write('SECOND_REVIEW_REPOS="other"\nsleep 30\n')
            self.assertEqual(F.review_repos(os.path.join(folder, "orchestration.config")),
                             (None, "bash did not finish sourcing it within 1 seconds"))
        names, why = F.review_repos(os.path.join(self.env.root, "no-such-entity", "orchestration.config"))
        self.assertIsNone(names)
        self.assertTrue(why)
        self.assertEqual(F.review_repos(os.path.join(ENGINE, "orchestration.config")), (["richos"], ""))

    def test_second_review_install_and_the_land_refuse_what_cannot_be_read(self):
        """A declaration bash does not source cleanly (an unterminated quote, a
        last command that fails, an `exit`, an EXIT trap that prints, a command
        not found) or whose value is not names (an expansion to a path, `..`,
        a carriage return): install refuses each before it changes the
        registry or the launcher, and the land refuses each too."""
        config = os.path.join(self.decl, "orchestration.config")
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        registry = os.path.join(ws.land_locks_dir(), "operator-fences.json")

        def installed():
            with open(hook) as h, open(registry) as r:
                return h.read(), r.read()

        _cc, (tip,) = self.finished("zach-opus-rv30")
        for line in self.UNREADABLE_FORMS:
            with open(config, "w") as f:
                f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n%s\n' % line)
            before = installed()
            r = run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl, check=False)
            self.assertNotEqual(r.returncode, 0, "%s: %s" % (line, r.stdout))
            self.assertIn("nothing installed. SECOND_REVIEW_REPOS in %s cannot be read" % config,
                          r.stdout + r.stderr, line)
            self.assertEqual(installed(), before, line)
            self.refused_unread("zach-opus-rv30", "SECOND_REVIEW_REPOS in %s cannot be read" % config)
        self.declare("other")
        run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl)
        self.refused_unread("zach-opus-rv30", "no second review of %s" % tip[:12])
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv30", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_install_and_the_land_check_call_the_same_reader(self):
        """install (operator_fences_admin), the land check (workspaces) and
        review-watch read SECOND_REVIEW_REPOS through one function of the one
        operator_fences.py: what that function says decides all three."""
        lib = os.path.join(ENGINE, "scripts", "lib")
        spec = importlib.util.spec_from_file_location("srv_admin", os.path.join(lib, "operator_fences_admin.py"))
        A = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(A)
        spec = importlib.util.spec_from_file_location("srv_review_watch", os.path.join(lib, "review_watch.py"))
        RW = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(RW)
        F = ws._fence_program()
        self.assertIs(RW.F, A.F)
        self.assertEqual(os.path.realpath(A.F.__file__), os.path.realpath(F.__file__))
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        registry = os.path.join(ws.land_locks_dir(), "operator-fences.json")

        def installed():
            with open(hook) as h, open(registry) as r:
                return h.read(), r.read()

        why = "line 7 is the reader's own sentinel"
        before = installed()
        with patch.object(A.F, "review_repos", return_value=(None, why)) as at_install:
            with self.assertRaises(SystemExit) as e:
                A.cmd_install({"--entity": self.decl}, [self.other])
            self.assertEqual(RW.configured_repos(os.path.join(self.decl, "orchestration.config")), (None, why))
        self.assertTrue(at_install.called)
        self.assertIn(why, str(e.exception))
        self.assertEqual(installed(), before)
        todo = [(self.other, "cc/same-reader", self.other, "b" * 40)]
        with patch.object(ws, "_fence_program", return_value=F), \
                patch.object(F, "review_repos", return_value=(None, why)) as at_land:
            with self.assertRaises(ws.SpecError) as e:
                ws._review_check(todo, [self.entity])
        self.assertTrue(at_land.called)
        self.assertIn(why, str(e.exception))

    # -- ask the shell ------------------------------------------------------------------
    # Review rv-20261009T052655Z-daf09e19-0bc6 (its fixture declaration_context.py):
    # a reader of physical lines read a value Bash does not. A key split by a
    # backslash-newline was read as no assignment, and an assignment inside a
    # skipped `if` or a heredoc was read as the value, while Bash keeps the
    # earlier "other". Install recorded "not reviewed" and the land merged with no
    # verdict. review_repos now sources the declaration in bash and reads the
    # value bash sets.

    SHELL_CONTEXT = (
        ("CONDITIONAL_DECOY", 'SECOND_REVIEW_REPOS="other"\nif false; then\nSECOND_REVIEW_REPOS="nothing"\nfi\n'),
        ("HEREDOC_DECOY", 'SECOND_REVIEW_REPOS="other"\n: <<\'DOC\'\nSECOND_REVIEW_REPOS="nothing"\nDOC\n'),
        ("CONTINUED_KEY", 'SECOND_REVIEW_\\\nREPOS="other"\n'),
    )

    def test_second_review_install_and_the_land_read_the_value_bash_sets(self):
        """The review's three cases: each one lists `other` for Bash, so install
        records that it is reviewed and bakes the review ledger into the
        launcher, and the land refuses the tip until a passing verdict."""
        config = os.path.join(self.decl, "orchestration.config")
        hook = os.path.join(self.other, ".git", "hooks", "reference-transaction")
        _cc, (tip,) = self.finished("zach-opus-rv31")
        for label, body in self.SHELL_CONTEXT:
            with self.subTest(label):
                with open(config, "w") as f:
                    f.write('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n' + body)
                r = run("bash", self.fences, "install", "--repo", self.other, "--entity", self.decl, check=False)
                self.assertEqual(r.returncode, 0, "%s: %s%s" % (label, r.stdout, r.stderr))
                self.assertTrue(self.registry()["repositories"][self.other].get("reviewed"), label)
                with open(hook) as h:
                    self.assertIn('OPERATOR_FENCES_REVIEWS="%s"' % self.ledger, h.read(), label)
                self.refused_unread("zach-opus-rv31", "no second review of %s" % tip[:12])
        self.verdict(tip, "passed")
        _merged, res = ws.merge_and_land("zach-opus-rv31", self.sid)
        self.assertTrue(res["landed"])

    def test_second_review_status_reports_a_launcher_that_disagrees_with_the_declaration(self):
        r = run("bash", self.fences, "status", "--repo", self.other, "--entity", self.decl, check=False)
        self.assertNotIn("SECOND_REVIEW_REPOS", r.stdout)
        self.declare("")
        r = run("bash", self.fences, "status", "--repo", self.other, "--entity", self.decl, check=False)
        self.assertIn("SECOND_REVIEW_REPOS does not list it", r.stdout)
        self.assertNotEqual(r.returncode, 0)


class SecondReview_AMidJobVerdictReachesTheRunningTeammate(Base):
    """Plan §2.5, "Changes requested, mid-job": "Delivered to the running
    teammate once, at its next tool call, by a hook (no mailbox)." The hook is
    scripts/hooks/deliver-review-verdict.sh (its logic: scripts/lib/review_delivery.py),
    run here as the host runs it."""

    HOOK = os.path.join(ENGINE, "scripts", "hooks", "deliver-review-verdict.sh")

    def setUp(self):
        super().setUp()
        self.state = os.path.join(self.env.root, "review-state")
        os.makedirs(self.state)
        self.ledger = os.path.join(self.state, "reviews.jsonl")

    def verdict(self, work, verdict="changes-requested", trigger="long-job", finished=None, title="a defect"):
        rid = "rv-mid-%d" % (len(open(self.ledger).readlines()) if os.path.exists(self.ledger) else 0)
        record = os.path.join(self.state, "reviews", rid)
        os.makedirs(record)
        with open(os.path.join(record, "verdict.json"), "w") as f:
            json.dump({"answer": {"verdict": verdict, "findings": [
                {"priority": 1, "title": title, "files": ["a.rs:9"], "evidence": "a fixture shows it",
                 "fixture": ""}], "not_yet_claimed": []}}, f)
        row = {"id": rid, "repo": self.other, "tip": "f" * 40, "work": work, "verdict": verdict,
               "trigger": trigger, "reviewer": "codex", "reviewer_model": "gpt-6.1-sol", "findings": 1, "p1": 1,
               "record": record,
               "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(finished or time.time()))}
        with open(self.ledger, "a") as f:
            f.write(json.dumps(row) + "\n")
        return rid

    def call(self, aid=None):
        payload = {"session_id": self.sid, "hook_event_name": "PreToolUse", "tool_name": "Bash",
                   "tool_input": {"command": "true"}, "cwd": self.entity}
        if aid:
            payload["agent_id"] = aid
        env = dict(os.environ, SECOND_REVIEW_STATE_DIR=self.state,
                   REVIEW_WATCH_STATE_DIR=os.path.join(self.env.root, "review-watch"))
        r = subprocess.run(["bash", self.HOOK], input=json.dumps(payload), capture_output=True, text=True, env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        if not r.stdout.strip():
            return ""
        return json.loads(r.stdout)["hookSpecificOutput"]["additionalContext"]

    def test_second_review_a_running_teammates_next_tool_call_carries_a_new_verdict_once(self):
        aid, _npath = self.spawn("zach-opus-mid1")
        other_aid, _p = self.spawn("zach-opus-mid2")
        key = self.rec("zach-opus-mid1")["key"]
        self.verdict("teammate:" + key, finished=time.time() - 7200, title="older than the teammate")
        self.assertEqual(self.call(aid), "")                     # nothing new: no delivery
        self.verdict("teammate:" + key, title="the microphone is never handed back")
        got = self.call(aid)
        self.assertIn("CHANGES-REQUESTED", got)
        self.assertIn("the microphone is never handed back", got)
        self.assertNotIn("older than the teammate", got)
        self.assertEqual(self.call(aid), "")                     # once
        self.assertEqual(self.call(other_aid), "")               # not another teammate's work
        self.assertEqual(self.call(None), "")                    # the lead is told by review-watch

    def test_second_review_a_verdict_waits_for_the_teammates_next_call_however_late(self):
        """Review rv-20261009T023055Z-b51ebb97-bc65, finding 3: a verdict older
        than 24 hours was dropped before its delivery was checked, so a teammate
        whose next call came later never got it."""
        aid, _npath = self.spawn("zach-opus-mid3")
        rec = self.rec("zach-opus-mid3")
        # Registered 26 hours ago; the verdict finished 25 hours ago, and this is
        # the teammate's first tool call since.
        rec["registered_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 26 * 3600))
        ws.save_agent(rec)
        self.verdict("teammate:" + rec["key"], finished=time.time() - 25 * 3600,
                     title="the microphone is never handed back")
        got = self.call(aid)
        self.assertIn("CHANGES-REQUESTED", got)
        self.assertIn("the microphone is never handed back", got)
        self.assertEqual(self.call(aid), "")                     # still once

    def test_second_review_a_continuation_gets_a_verdict_on_the_work_it_continues(self):
        """Review rv-20261009T031207Z-ba444a8a-607a, finding 2: a continuation
        judged every inherited verdict against its OWN registration, so a
        verdict on the predecessor's work that finished between the two
        registrations never reached it. Each verdict is judged against the
        registration of the work it is on; delivery stays once per teammate."""
        old_aid, _p = self.spawn("zach-opus-mid4")
        aid, _q = self.spawn("zach-opus-mid5")
        old, cur = self.rec("zach-opus-mid4"), self.rec("zach-opus-mid5")
        t = time.time()
        old["registered_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t - 7200))
        cur["registered_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t - 600))
        cur["continues"] = [old["key"]]
        ws.save_agent(old)
        ws.save_agent(cur)
        self.verdict("teammate:" + old["key"], finished=t - 9000, title="older than the predecessor")
        self.verdict("teammate:" + old["key"], finished=t - 1200, title="the microphone is never handed back")
        self.assertIn("the microphone is never handed back", self.call(old_aid))
        got = self.call(aid)
        self.assertIn("CHANGES-REQUESTED", got)
        self.assertIn("the microphone is never handed back", got)
        self.assertNotIn("older than the predecessor", got)
        self.assertEqual(self.call(aid), "")                     # once for this teammate too


class HuntV3_01_ARecordWithoutItsWorkspacesIsDamaged(Base):
    """Hunt part 4 v3, V3-01: a keyed record that had lost its `workspaces`
    list passed the V2-04 check, the sweep read it as owning nothing, made its
    live workspace an orphan, stopped the worker's process and deleted it."""

    def test_v3_01_a_sweep_never_cleans_up_a_live_worker_whose_record_lost_its_workspaces(self):
        name = "zach-opus-v3record"
        _aid, path = self.spawn(name)
        rec = self.rec(name)
        self.assertFalse(ws.finished_state(rec)[0])
        sleeper = subprocess.Popen(["sleep", "300"], cwd=path)
        self.env.procs.append(sleeper)
        self.assertIn(sleeper.pid, ws.processes_in([path]))
        rec.pop("workspaces")
        with open(ws.agent_path(rec["key"]), "w") as f:
            json.dump(rec, f)
        with patch.object(ws, "stop_containers", return_value={}), \
                patch.object(ws, "stop_test_instances", return_value={}):
            items = ws.pending(self.sid, self.entity, scan=True)
        time.sleep(0.3)
        self.assertIsNone(sleeper.poll(), "the sweep stopped a live worker's process")
        self.assertTrue(os.path.isdir(path), "the sweep deleted a live worker's workspace")
        self.assertFalse([r for r in ws.all_agents() if r.get("orphan")])
        self.assertEqual(len([i for i in items if i.get("damaged")]), 1, items)
        self.assertEqual([p for p, _w in ws.damaged_records()], [ws.agent_path(rec["key"])])


class HuntV3_02_TheBudgetBoundsTheWholeCleanup(Base):
    """Hunt part 4 v3, V3-02: V2-03 stopped a stage from STARTING past the
    deadline, but a slow helper inside a stage (the container reaper) still
    kept the caller waiting, and an already-landed retry dropped the deadline."""

    def _ended(self, name, kind):
        aid, path = self.spawn(name)
        self.finish(aid)
        rec = self.rec(name)
        rec["disposition"] = {"kind": kind, "at": ws.now(), "reason": "fixture only"}
        ws.save_agent(rec)
        return rec, path

    def test_v3_02_a_slow_container_helper_never_keeps_the_caller_past_its_budget(self):
        rec, path = self._ended("zach-opus-v3deadline", "discarded")
        release = threading.Event()

        def slow_containers(paths):
            release.wait(1.25)
            return {}
        t0 = time.monotonic()
        try:
            with patch.object(ws, "stop_containers", side_effect=slow_containers), \
                    patch.object(ws, "stop_test_instances", return_value={}):
                ok = ws._delete(rec, ws.live_workspaces(rec), True, "v3-02",
                                processes={"stopped": [], "survivors": []}, deadline=ws.now() + 0.15)
            took = time.monotonic() - t0
        finally:
            release.set()
            time.sleep(0.1)                     # the helper's own thread returns
        self.assertLess(took, 0.6, "a 0.15 s cleanup budget kept its caller %.2f s" % took)
        self.assertFalse(ok)
        self.assertTrue(os.path.isdir(path))
        d = self.rec("zach-opus-v3deadline")["deletion"]
        self.assertIn("containers", d["deferred"])
        self.assertEqual(d.get("attempts", 0), 0, "running out of time was counted as a failure")

    def test_v3_02_an_already_landed_retry_keeps_the_callers_deadline(self):
        self._ended("zach-opus-v3retry", "landed")
        deadline = ws.now() - 1
        with patch.object(ws, "_delete_chain", return_value=False) as spy:
            res = ws.land("zach-opus-v3retry", self.sid, deadline=deadline)
        self.assertEqual(spy.call_args.kwargs.get("deadline"), deadline)
        self.assertTrue(res["cleanup_pending"])


class HuntV3_05_AFailedProcessListingIsNotAnEmptyOne(Base):
    """Hunt part 4 v3, V3-05: `lsof` and `ps` that exited non-zero with no
    output were read as "no process works there", so the land deleted a
    workspace while its process still ran."""

    def test_v3_05_failed_process_listings_never_certify_a_workspace_empty(self):
        name = "zach-opus-v3listing"
        aid, path = self.spawn(name)
        self.finish(aid)
        sleeper = subprocess.Popen(["sleep", "300"], cwd=path)
        self.env.procs.append(sleeper)
        self.assertIn(sleeper.pid, ws.processes_in([path]))
        real = subprocess.run

        def failing(command, *args, **kw):
            if command[0] == "lsof" or list(command[:2]) == ["ps", "-axww"]:
                return subprocess.CompletedProcess(command, 2, stdout="", stderr="controlled listing failure")
            return real(command, *args, **kw)
        with patch.object(ws.subprocess, "run", side_effect=failing), \
                patch.object(ws, "stop_containers", return_value={}), \
                patch.object(ws, "stop_test_instances", return_value={}):
            stopped = ws.stop_processes([path], deadline=ws.now() + 10)
            with self.assertRaises(ws.SpecError):
                ws.land(name, self.sid, deadline=ws.now() + 10)
            with self.assertRaises(ws.SpecError):
                ws.land(name, self.sid)                       # no deadline: the same answer
        self.assertTrue(stopped.get("unknown"), stopped)
        self.assertIsNone(sleeper.poll(), "the process was stopped on a listing that failed")
        self.assertTrue(os.path.isdir(path), "the workspace was deleted while its process ran")
        self.assertFalse(self.rec(name).get("disposition"))


class HuntV3_04_AReopenedLandingIsNotReportedLanded(Base):
    """Hunt part 4 v3, V3-04: a land retry whose reproof found new work
    withdrew the landing and its retry, yet the CLI printed "landed ... it is
    retried", exited 0 and swept the scratch."""

    def test_v3_04_a_retry_that_reopens_the_landing_is_refused_not_reported_landed(self):
        import contextlib
        import io
        name = "zach-opus-v3reopened"
        aid, path = self.spawn(name)
        self.commit(path)
        self.finish(aid)
        self.merge(self.entity, "worktree-agent-" + aid)
        swept = []
        out, err = io.StringIO(), io.StringIO()
        with patch.object(ws, "stop_containers", return_value={}), \
                patch.object(ws, "stop_test_instances", return_value={}):
            with patch.object(ws, "remove_workspace", return_value=(False, "fixture removal failure")):
                first = ws.land(name, self.sid)
            self.assertTrue(first["landed"] and first["cleanup_pending"], first)
            self.commit(path, "late.txt")                     # new, unmerged work in the kept workspace
            with patch.object(ws, "sweep_scratch_after_land", side_effect=lambda *a, **k: swept.append(a)), \
                    contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                rc = ws.main(["--session", self.sid, "land", name])
        rec = self.rec(name)
        self.assertIsNone(rec["disposition"])
        self.assertIsNone(rec["deletion"])
        self.assertTrue(os.path.isfile(os.path.join(path, "late.txt")))
        self.assertNotIn("landed:", out.getvalue())
        self.assertNotEqual(rc, 0, out.getvalue())
        self.assertIn("REFUSED", err.getvalue())
        self.assertEqual(swept, [], "the scratch was swept after a landing that was withdrawn")


class HuntV3_06_AStatusQueryWritesNoCleanupHistory(Base):
    """Hunt part 4 v3, V3-06: the dry status proved eligibility through the
    V2-02 regenerable-output helper, which appended a
    `regenerable-not-preserved` event at every query although nothing was
    cleaned up."""

    def test_v3_06_status_appends_nothing_to_the_event_history(self):
        import contextlib
        import io
        with open(os.path.join(self.other, ".gitignore"), "a") as f:
            f.write("#regenerable: build/\n")
        run("git", "-C", self.other, "commit", "-qam", "declare regenerable output")
        name = "zach-opus-v3status"
        path = self.make_cc(name)
        aid, _n = self.spawn(name, path, native=False)
        os.makedirs(os.path.join(path, "build"))
        with open(os.path.join(path, "build", "output.bin"), "wb") as f:
            f.write(b"generated fixture output")
        self.finish(aid)
        events = ws._p("events.jsonl")
        before = open(events).read() if os.path.exists(events) else ""
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            ws._print_status(self.sid, self.entity)
        after = open(events).read() if os.path.exists(events) else ""
        self.assertIn("LANDABLE " + name, out.getvalue())
        self.assertEqual(after[len(before):], "", "a status query wrote history")
        self.assertTrue(os.path.isfile(os.path.join(path, "build", "output.bin")))
        # The real land still records what it did not preserve.
        ws.land(name, self.sid)
        self.assertIn('"regenerable-not-preserved"', open(events).read())


class ReleaseBranches_TheMergeGateIsToldWhichBranchItLandsOn(unittest.TestCase):
    """Review rv-20261013T095956Z-f4ae06df-2813, finding 1, and 2026-10-18: a land onto
    a branch the main checkout is not on merges in a temporary checkout of that branch,
    where the merge gate saw no main and ran the commit check instead of the land check;
    two app suites broken on v1.2.1 were found only when v1.2.1 was merged into main.
    The lander names the branch it lands onto in RICHOS_AUTOCHECK_LAND_TARGET, which
    richos's merge gate (autocheck.py) reads; its own test proves the gate then runs the
    land check. This hook refuses a merge that does not say which branch it lands on, as
    that gate would have no way to tell."""

    def setUp(self):
        self.root = os.path.realpath(tempfile.mkdtemp(prefix="ws-land-target-"))
        self.saved = {k: os.environ.get(k) for k in ("RICHOS_WORKSPACES_DIR", "GIT_CONFIG_GLOBAL",
                                                     "RICHOS_AUTOCHECK_LAND_TARGET")}
        os.environ["RICHOS_WORKSPACES_DIR"] = os.path.join(self.root, "state")
        os.environ.pop("RICHOS_AUTOCHECK_LAND_TARGET", None)
        gc = os.path.join(self.root, "gitconfig")
        with open(gc, "w") as f:
            f.write("[user]\n\tname = t\n\temail = t@example.invalid\n[init]\n\tdefaultBranch = main\n")
        os.environ["GIT_CONFIG_GLOBAL"] = gc

    def tearDown(self):
        for k, v in self.saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.root, ignore_errors=True)

    def test_every_land_off_the_main_checkout_names_its_branch_to_the_merge_gate(self):
        repo = os.path.join(self.root, "repo")
        run("git", "init", "-q", "-b", "main", repo)
        run("git", "-C", repo, "commit", "-q", "--allow-empty", "-m", "base")
        for b in ("v1.2.1", "v1.3.0"):
            run("git", "-C", repo, "branch", b)
        run("git", "-C", repo, "switch", "-q", "-c", "cc/worker")
        with open(os.path.join(repo, "fix.txt"), "w") as f:
            f.write("fix\n")
        run("git", "-C", repo, "add", "fix.txt")
        run("git", "-C", repo, "commit", "-q", "-m", "fix")
        worker = run("git", "-C", repo, "rev-parse", "HEAD").stdout.strip()
        # The main checkout stays on main while work lands on the release branches.
        run("git", "-C", repo, "switch", "-q", "main")
        seen = os.path.join(self.root, "gate-saw")
        hook = os.path.join(repo, ".git", "hooks", "pre-merge-commit")
        with open(hook, "w") as f:
            f.write('#!/bin/sh\nprintf "%%s\\n" "${RICHOS_AUTOCHECK_LAND_TARGET:-}" >> %s\n'
                    '[ -n "${RICHOS_AUTOCHECK_LAND_TARGET:-}" ]\n' % shlex.quote(seen))
        os.chmod(hook, 0o755)
        for target in ("v1.2.1", "v1.3.0"):
            self.assertFalse(ws._merges_in_main_checkout(repo, target, "cc/worker"), target)
            args = ["merge", "--no-ff", "--no-edit", "-m", "land worker", worker]
            ws._merge_in_temporary_checkout(repo, target, args, "cc/worker")
            self.assertEqual(run("git", "-C", repo, "rev-parse", target + "^2").stdout.strip(), worker)
        with open(seen) as f:
            self.assertEqual(f.read().split(), ["v1.2.1", "v1.3.0"])
        self.assertEqual(run("git", "-C", repo, "symbolic-ref", "--short", "HEAD").stdout.strip(), "main")


class ProofReport_TheHandoverPointsAtItsSavedRun(Base):
    """Verification reliability PRD 2026-10-20, R3/A8: a handover (TaskCompleted) and a normal
    end (SubagentStop) attach a pointer to the workspace's newest saved proof run of its HEAD,
    with the runner's own counts and every check that did not pass. It is a report, never
    proof; a missing, stale, unreadable or slow one never delays or prevents the fact."""

    def setUp(self):
        super().setUp()
        self.history_root = os.path.join(self.env.root, "proof-runs")
        os.environ["RICHOS_PROOF_RUN_DIR"] = self.history_root
        self.addCleanup(os.environ.pop, "RICHOS_PROOF_RUN_DIR", None)
        self.cc = self.make_cc("zach-opus-pr")
        self.aid, _n = self.spawn("zach-opus-pr", cc=self.cc)
        self.commit(self.cc)
        self.path = os.path.realpath(self.cc)

    def head(self):
        return run("git", "-C", self.cc, "rev-parse", "HEAD").stdout.strip()

    def saved_run(self, name, summary, head=None):
        run_dir = os.path.join(ws._proof_history(self.path), name)
        os.makedirs(run_dir)
        head = head or self.head()
        with open(os.path.join(run_dir, "plan.json"), "w") as f:
            json.dump({"schema": 1, "root": self.path, "source": {"commit": head},
                       "items": [{"check": c} for c in ("a.js", "b.js", "c.js", "d")],
                       "selection": {"arguments": ["base..cand"], "mode": "normal",
                                     "base": "b" * 40, "candidate": head}}, f)
        with open(os.path.join(run_dir, "summary.json"), "w") as f:
            f.write(summary if isinstance(summary, str) else json.dumps(summary))
        return run_dir

    def report(self):
        return [w for w in self.rec("zach-opus-pr")["workspaces"] if w.get("path") == self.path][0].get("proof_report")

    def handed_in(self):
        ws.lifecycle({"hook_event_name": "TaskCompleted", "session_id": self.sid,
                      "teammate_name": "zach-opus-pr"}, self.entity)

    def test_an_incomplete_run_is_reported_by_the_runners_own_outcomes(self):
        run_dir = self.saved_run("20261020T000000Z-x", {"checks": [
            {"check": "a.js", "result": "passed"}, {"check": "b.js", "result": "passed", "reused_from": "/r"},
            {"check": "c.js", "result": "failed"},
            {"check": "d", "result": "not-run", "not_run": {"why": "no-screen"}}]})
        self.handed_in()
        report = self.report()
        self.assertTrue(self.rec("zach-opus-pr").get("handed_in"))
        self.assertEqual(report["run"], run_dir)
        self.assertEqual(report["candidate"], self.head())
        self.assertEqual(report["outcomes"], {"passed": 2, "failed": 1, "not-run": 1})
        self.assertEqual(report["line"], "4 planned, 2 passed (1 of them reused); NOT complete: "
                                         "c.js (failed), d (no-screen)")
        # Never counted as passes: total minus failed would say 3.
        self.assertNotIn("3 passed", report["line"])

    def test_a_known_red_unit_is_never_reported_as_a_pass(self):
        self.saved_run("20261020T000000Z-x", {"checks": [
            {"check": "a.js", "result": "passed"},
            {"check": "engine u", "result": "passed", "known_red": True}]})
        self.handed_in()
        self.assertEqual(self.report()["line"], "4 planned, 1 passed (0 of them reused); NOT complete: "
                                                "engine u (known-red, a declared exception)")

    def test_a_deferred_end_attaches_no_report_and_the_real_end_does(self):
        """PRD A8: the SubagentStop deferral as a controlled witness. While a command the agent
        started still runs, the end is not recorded and no report is attached; the later real end
        records both."""
        self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        with patch.object(ws, "_running_calls", return_value=[("toolu_fixture", "sleep 600")]):
            self.finish(self.aid)
        record = self.rec("zach-opus-pr")
        self.assertFalse(record.get("end"))
        self.assertEqual(record["history"][-1]["fact"], ws.END_DEFERRED_FACT)
        self.assertIsNone(self.report())
        with patch.object(ws, "_running_calls", return_value=[]):
            self.finish(self.aid)
        self.assertTrue(self.rec("zach-opus-pr").get("end"))
        self.assertEqual(self.report()["on"], "end")

    def test_a_run_of_an_older_head_is_not_this_handovers_report(self):
        self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        self.commit(self.cc, "later.txt")
        self.finish(self.aid)
        self.assertTrue(self.rec("zach-opus-pr").get("end"))
        self.assertIn("no saved proof run of HEAD", self.report()["unavailable"])
        self.assertEqual(self.report()["on"], "end")

    def test_missing_or_unreadable_reports_never_prevent_the_fact(self):
        self.handed_in()
        self.assertTrue(self.rec("zach-opus-pr").get("handed_in"))
        self.assertIn("no saved proof run", self.report()["unavailable"])
        run_dir = self.saved_run("20261020T000001Z-y", "{ not json")
        self.finish(self.aid)
        self.assertTrue(self.rec("zach-opus-pr").get("end"))
        self.assertEqual(self.report()["run"], run_dir)
        self.assertIn("no readable summary", self.report()["unavailable"])

    def test_a_slow_or_failing_report_stays_inside_the_hooks_budget(self):
        self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        # The budget is the report's own deadline: spent, it stops reading and says so.
        with patch.object(ws, "PROOF_REPORT_SECONDS", 0):
            self.handed_in()
        self.assertTrue(self.rec("zach-opus-pr").get("handed_in"))
        self.assertIn("time budget", self.report()["unavailable"])
        before = self.rec("zach-opus-pr")["end"] if self.rec("zach-opus-pr").get("end") else None
        with patch.object(ws, "proof_report", side_effect=RuntimeError("broken reader")):
            self.finish(self.aid)
        self.assertTrue(self.rec("zach-opus-pr").get("end"))
        self.assertIsNone(before)

    def test_a_read_that_spends_the_shared_budget_never_becomes_a_report(self):
        """Review rv-20261020T044017Z-ebbb640b-fc3b #2: the deadline was asked only between saved
        runs, so a plan read that took four seconds of a three-second budget still returned a
        complete report. Every read is now followed by the question."""
        self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        clock = [100.0]
        real_open = open

        def slow(path, *args, **kwargs):
            if str(path).endswith("plan.json"):
                clock[0] = 104.0
            return real_open(path, *args, **kwargs)
        with patch.object(ws.time, "monotonic", side_effect=lambda: clock[0]), \
                patch("builtins.open", side_effect=slow):
            report = ws.proof_report(self.path, deadline=103.0)
        self.assertIn("time budget", report.get("unavailable", ""), report)
        self.assertNotIn("line", report)
        # The HEAD read itself gets only what is left of the budget.
        with patch.object(ws.time, "monotonic", side_effect=lambda: 200.0):
            self.assertIn("time budget", ws.proof_report(self.path, deadline=199.0)["unavailable"])

    def test_the_land_runs_its_gate_in_the_callers_locale(self):
        """PRD step 3 (A10/R8): every UI suite binds LC_ALL, and the managed merge forced LC_ALL=C on
        its pre-merge-commit gate while the author's run, a plain-Git land and the nightly kept the
        caller's locale, so no author pass could match a managed land's identity."""
        with patch.dict(os.environ, {"LC_ALL": "en_US.UTF-8"}):
            self.assertEqual(ws._land_env("main")["LC_ALL"], "en_US.UTF-8")
        without = {k: v for k, v in os.environ.items() if k != "LC_ALL"}
        with patch.dict(os.environ, without, clear=True):
            self.assertNotIn("LC_ALL", ws._land_env("v1.3.0", ["/a/run"]))
            # This tool still reads git's own output in the C locale.
            self.assertEqual(ws._git_env()["LC_ALL"], "C")

    def test_the_report_says_whether_its_run_is_held_and_names_other_held_runs(self):
        """PRD A11: an explicit abandonment or supersession names the run for Rich to release."""
        older = self.saved_run("20261020T000000Z-old", {"checks": [{"check": "a.js", "result": "passed"}]},
                               head="c" * 40)
        with open(os.path.join(older, "retain"), "w") as f:
            json.dump({"holder": "handover", "candidate": "c" * 40}, f)
        newer = self.saved_run("20261020T000001Z-new", {"checks": [{"check": "a.js", "result": "passed"}]})
        with open(os.path.join(newer, "retain"), "w") as f:
            json.dump({"holder": "handover", "candidate": self.head()}, f)
        report = ws.proof_report(self.path)
        self.assertEqual(report["run"], newer)
        self.assertTrue(report["held"])
        self.assertEqual(report["also_held"], [older])
        self.assertIn("1 earlier handover run(s) of this checkout still held", report["line"])

    def test_the_held_run_scan_stays_inside_the_shared_budget(self):
        """Review rv-20261020T073921Z-1e0461cc-0956 #3: the retain markers were read after the last
        deadline check; a slow read past the budget still returned a complete report."""
        self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        clock = [100.0]
        real_exists = os.path.exists

        def slow_exists(path):
            if str(path).endswith("retain"):
                clock[0] = 104.0
            return real_exists(path)
        with patch.object(ws.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(ws.os.path, "exists", side_effect=slow_exists):
            report = ws.proof_report(self.path, deadline=103.0)
        self.assertIn("time budget", report.get("unavailable", ""), report)
        self.assertNotIn("line", report)

    def test_the_merge_offers_only_the_run_reported_for_the_tip_it_merges(self):
        run_dir = self.saved_run("20261020T000000Z-x", {"checks": [{"check": "a.js", "result": "passed"}]})
        self.handed_in()
        chain = ws._chain(self.rec("zach-opus-pr"))
        self.assertEqual(ws._author_runs(chain, self.head()), [run_dir])
        self.assertEqual(ws._land_env("main", ws._author_runs(chain, self.head()))[ws.AUTHOR_RUNS_ENV], run_dir)
        # Another tip (the branch moved after the report) is offered nothing.
        self.assertEqual(ws._author_runs(chain, "c" * 40), [])
        with patch.dict(os.environ, {ws.AUTHOR_RUNS_ENV: "/an/inherited/run"}):
            self.assertNotIn(ws.AUTHOR_RUNS_ENV, ws._land_env("main", []))


class HuntV3_07_NoTestIsCollectedTwice(unittest.TestCase):
    """Hunt part 4 v3, V3-07: SeverityThreeGroupJ subclassed the concrete
    Point09_NeverWritesAgain for one helper and so ran its six tests twice.
    Every collected test must be defined by the class that runs it, or by a
    class the loader does not collect on its own."""

    def test_v3_07_no_collected_test_is_inherited_from_another_collected_class(self):
        module = sys.modules[type(self).__module__]
        loader = unittest.defaultTestLoader
        cases = [c for c in vars(module).values()
                 if isinstance(c, type) and issubclass(c, unittest.TestCase)]
        collected = set(c for c in cases if loader.getTestCaseNames(c))
        twice = ["%s.%s (defined by %s)" % (c.__name__, m, owner.__name__)
                 for c in cases for m in loader.getTestCaseNames(c)
                 for owner in [next(k for k in c.__mro__ if m in vars(k))]
                 if owner is not c and owner in collected]
        self.assertEqual(twice, [])



class NoSilentHolds(Base):
    """richos-hq docs/plans/2026-10-22-no-silent-holds.md, T5. The CEO: "WHEN WILL
    [THIS] STOP ONCE AND FOR ALL???" A hold ends only when the work runs, when it
    lands or is discarded, or by his words; Rich saying so ends nothing."""

    def _pending_one(self):
        aid, npath = self.spawn("zach-opus-p5")
        self.commit(npath)
        self.finish(aid)
        return aid, npath

    def test_holds_t5_release_without_his_word_or_running_work_is_refused(self):
        ws.hold("stream-b", "Stream B of his spelling order", "ceo:CEO-TODOs 9.1", "on hold, deliberately", self.sid)
        self.assertEqual([h["name"] for h in ws.holds(self.sid)], ["stream-b"])
        with self.assertRaises(ws.SpecError) as cm:
            ws.release("stream-b", session_id=self.sid)
        self.assertIn("--ceo-word", str(cm.exception))
        self.assertEqual([h["name"] for h in ws.holds(self.sid)], ["stream-b"])      # still held
        ws.release("stream-b", ceo_word="park Stream B until Monday", session_id=self.sid)
        self.assertEqual(ws.holds(self.sid), [])

    def test_holds_t5_the_work_running_ends_it_and_a_false_catch_is_shown_once(self):
        aid, _n = self.spawn("zach-opus-y")
        ws.hold("stream-c", "Stream C", "agent:zach-opus-y", "", self.sid)
        self.assertEqual(ws.holds(self.sid)[0]["state"], "pending")                 # Y does it: quiet
        ws.release("stream-c", session_id=self.sid)                                   # the work runs
        ws.hold("held-1a2b3c4d", "it goes in with the next land", "ceo:CEO-TODOs 9.2", "a reply held it", self.sid)
        with self.assertRaises(ws.SpecError):
            ws.release("held-1a2b3c4d", not_a_hold="short", session_id=self.sid)
        ws.release("held-1a2b3c4d", not_a_hold="a description of a bundle that exists", session_id=self.sid)
        self.assertEqual(ws.holds(self.sid), [])
        ws.hold("stream-d", "Stream D", "ceo:CEO-TODOs 9.3", "", self.sid)
        self.spawn("zach-opus-d", extra="continues: stream-d\n")                      # a continuation runs it
        self.assertEqual(ws.holds(self.sid), [])

    def test_holds_r1_a_free_text_message_creates_no_obligation_and_a_legacy_hold_ends_at_the_go(self):
        """Orchestration reliability R1 (richos-hq docs/prds/2026-10-23-orchestration-
        reliability.md): "Yes, stop it now", "Hand over now" and "wait for my go" were each
        kept as an obligation a later commit or his word had to end. A message's words record
        nothing now; a legacy `held` wait still ends at the go, never at a question."""
        aid, _n = self.spawn("zach-opus-w7")
        send = lambda text: ws.lifecycle({"hook_event_name": "PostToolUse", "tool_name": "SendMessage",
                                          "session_id": self.sid, "tool_input": {"to": "zach-opus-w7",
                                                                                 "message": text}}, self.entity)
        for text in ["Commit what you have. Then wait for my go before the walk.", "Yes, stop it now",
                     "Hand over now", "Fix review finding #1: quota.rs:702."]:
            _ctx, notes = send(text)
            self.assertEqual(ws.holds(self.sid), [], text)
            self.assertFalse(any(n.startswith("HOLD RECORDED") for n in notes), notes)
        rec = self.rec("zach-opus-w7")
        self.assertNotIn("correction", rec)
        rec["waiting"] = {"kind": "held", "on": "told to wait: wait for my go", "todo": "", "at": ws.now(),
                          "end": ""}                                   # a legacy record
        ws.save_agent(rec)
        for text in ["What is your current status?", "What do you need before you can proceed?",
                     "Do not resume yet; the review is still running."]:
            send(text)
            self.assertEqual([(h["name"], h["source"]) for h in ws.holds(self.sid)], [("zach-opus-w7", "wait")],
                             text)
        send("Go: run the walk now.")
        self.assertEqual(ws.holds(self.sid), [])

    def test_r2_stopped_work_is_offered_only_shelving_and_discard(self):
        """Frank D4: a stopped agent's due line said "land it ... or spawn its continuation"."""
        aid, npath = self.spawn("zach-opus-sp")
        self.commit(npath)
        ws.stop("zach-opus-sp", "he said stop", self.sid)
        due = [d for d in ws.due_set(self.sid, ws.pending(self.sid, self.entity, auto=False, dry=True))
               if d["name"] == "zach-opus-sp"]
        self.assertEqual(len(due), 1)
        self.assertIn("--ceo-word", due[0]["command"])
        self.assertIn("shelved/zach-opus-sp", due[0]["command"])
        self.assertNotIn("land it", due[0]["command"])
        self.assertNotIn("spawn its continuation", due[0]["command"])
        # Review rv-20261024T053817Z-c68788ad-bcc2 #1: a mid-job review asked for changes, then
        # he stopped the worker. Its review obligation is due too, and is offered only shelving.
        # Only the registry's Git readers are stubbed (its branch, its tip and the review of it).
        tip = "1234567890abcdef1234567890abcdef12345678"
        review = {"id": "rv-fixture", "verdict": "changes-requested", "findings": 1,
                  "finished_at": "2026-10-24T01:00:00Z"}
        items = ws.pending(self.sid, self.entity, auto=False, dry=True)
        with patch.object(ws, "_reviews_by_tip", return_value={tip: review}), \
                patch.object(ws, "_branch_targets", return_value={(self.entity, "fixture-branch")}), \
                patch.object(ws, "branch_tip", return_value=tip):
            due = [d for d in ws.due_set(self.sid, items) if d["name"] == "zach-opus-sp"]
            # The same one choice in the release error's advice (Codex 05:51Z).
            with self.assertRaises(ws.SpecError) as refused:
                ws.release("zach-opus-sp", session_id=self.sid)
        self.assertEqual([d["source"] for d in due], ["review"], due)
        self.assertIn("shelved/zach-opus-sp", due[0]["command"])
        self.assertNotIn("spawn the teammate", due[0]["command"])
        self.assertNotIn("continues:", due[0]["command"])
        self.assertIn("shelved/zach-opus-sp", str(refused.exception))
        self.assertNotIn("continues:", str(refused.exception))

    def test_r6_a_due_set_that_merely_emptied_is_not_told(self):
        """Sage D2 / Frank D3: in journey-due-work-acted-on the lead resumed the due item and the
        next Stop said "Nothing is due now.", which cost a model turn that did nothing."""
        self._pending_one()
        lead = lambda: ws.gate_stop({"session_id": self.sid}, self.entity)[2]
        self.assertIn("zach-opus-p5", lead())                            # due: told
        ws.discard("zach-opus-p5", "handled by the lead in this fixture", not_ceo_ordered="a test fixture, not his order", me=self.sid)
        self.assertEqual(lead(), "")                                     # emptied, same watcher: quiet

    def test_r2_an_unreadable_record_never_hides_the_readable_due_work(self):
        """Codex 03:24Z item A (Sage D6): one damaged registry record made the watcher drop
        every readable due item. Readable due work is told, with one UNREADABLE RECORD line,
        in the fallback, at session start and in `holds`; nothing refuses anything."""
        import io
        from contextlib import redirect_stdout
        self._pending_one()
        with open(ws.agent_path("damaged-fixture"), "w") as f:
            f.write("{")
        ok, person, lead = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertTrue(ok)
        self.assertIn("zach-opus-p5", lead)
        self.assertIn("UNREADABLE RECORD: 1 workspace registry record", lead)
        self.assertIn("damaged-fixture", lead)
        buf = io.StringIO()
        with redirect_stdout(buf):
            ws.main(["--session", self.sid, "--entity", self.entity, "holds"])
        self.assertIn("zach-opus-p5", buf.getvalue())
        self.assertIn("UNREADABLE RECORD", buf.getvalue())
        os.unlink(ws.agent_path("damaged-fixture"))
        self.assertEqual(ws.unreadable_line(), "")

    def test_r5_an_unreadable_due_set_is_never_told_as_nothing_due(self):
        """Codex 2026-10-24 (R5/R6/A7): an exception reading the due set became an empty set,
        "Nothing is due now." was appended, and the once-only key ignored the failure, so a
        failed read after an empty one said nothing at all. Unavailable stays distinct from
        verified-empty in the message and in the once-only state."""
        lead = lambda: ws.gate_stop({"session_id": self.sid}, self.entity)[2]
        first = lead()                                                   # no watcher, nothing due
        self.assertEqual(first, "")                                      # nothing to act on: quiet (F1)
        with patch.object(ws, "due_set", side_effect=OSError("registry unreadable")):
            failed = lead()
            self.assertIn("could NOT be read", failed)                 # a change: told
            self.assertIn("registry unreadable", failed)
            self.assertNotIn("Nothing is due now.", failed)            # never "nothing due"
            self.assertEqual(lead(), "")                               # the same failure: once
        self.assertIn("Nothing is due now.", lead())                    # read again: told again

    def test_r6_a_hold_reaches_the_lead_once_never_a_wall_at_every_turn_end(self):
        """Orchestration reliability R6: the 2026-10-23 screenshot turn carried 7,849
        characters of hold and pending lines, repeated at every turn end, to the person.
        A due hold is told to the lead, once per change, and only while no watcher runs."""
        ws.hold("stream-b", "Stream B of his spelling order", "ceo:CEO-TODOs 9.1", "on hold, deliberately", self.sid)
        first = ws.gate_stop({"session_id": self.sid}, self.entity)
        self.assertEqual(first[1], "")
        self.assertIn("stream-b", first[2])
        self.assertNotIn("HELD BY RICH", first[2])
        self.assertEqual(ws.gate_stop({"session_id": self.sid}, self.entity)[2], "")

    def test_holds_a_pause_outlives_its_session(self):
        """Review rv-20261022T013056Z-12c77c90-1291 finding 5: a manual pause, then an ordinary
        session end, must not drop the paused work from both reporting paths."""
        aid, _n = self.spawn("zach-opus-ps")
        ws.pause("zach-opus-ps", "Rich's resume", self.sid)
        self.assertEqual([h["source"] for h in ws.holds(self.sid) if h["name"] == "zach-opus-ps"], ["pause"])
        ws.record_session_end(self.sid, "SessionEnd")
        held = [h for h in ws.holds(self.sid) if h["name"] == "zach-opus-ps"]
        self.assertEqual([h["source"] for h in held], ["pause-ended"])
        self.assertIn("continues: zach-opus-ps", ws.hold_command(held[0]))
        self.assertTrue(any(l.startswith("zach-opus-ps:") for l in ws.due_lines(ws.due_set(self.sid))))

    def test_holds_a_repeated_wait_keeps_the_age_of_the_hold(self):
        """Review rv-20261022T002958Z-2ac46e09-fa4e finding 2: repeating an unresolved wait every
        20 minutes must not make a 2-hour hold look new."""
        self._pending_one()
        ws.wait("zach-opus-p5", "started", "real-app checks left", "", self.sid, SOON())
        rec = self.rec("zach-opus-p5")
        rec["waiting"]["at"] -= 139 * 60
        ws.save_agent(rec)
        ws.wait("zach-opus-p5", "started", "real-app checks left (still)", "", self.sid, SOON())
        held = [h for h in ws.holds(self.sid) if h["name"] == "zach-opus-p5"]
        self.assertGreaterEqual(ws.now() - held[0]["since"], 139 * 60)

    def test_holds_a_todo_reference_is_his_only_when_it_is_on_his_page(self):
        """Review rv-20261022T002958Z-2ac46e09-fa4e finding 3: a GitHub outage with a TODO
        reference is not his decision; only an item his CEO-TODOs page shows waiting on him is."""
        self._pending_one()
        ws.wait("zach-opus-p5", "outside", "GitHub is down, the merge cannot be pushed", "CEO-TODOs 7.1", self.sid)
        held = [h for h in ws.holds(self.sid) if h["name"] == "zach-opus-p5"]
        self.assertEqual(held[0]["state"], "none")                 # reported: not on his page
        page = os.path.join(self.env.root, "CEO-TODOs.md")
        with open(page, "w") as f:
            f.write("# Your TODOs\n\n## Waiting on the CEO - his hands\n\n### 7.1 \u2014 GitHub billing\n")
        with patch.dict(os.environ, {"RICHOS_CEO_TODOS_VIEW": page}):
            held = [h for h in ws.holds(self.sid) if h["name"] == "zach-opus-p5"]
            self.assertEqual(held[0]["state"], "ceo")              # on his page: his decision, quiet

    def test_holds_r1_a_legacy_correction_field_is_never_an_obligation(self):
        """2026-10-22/23: "Yes, stop it now" (honorfile1) and "Hand over now" (filecard3) were
        reported the next day as corrections their runs never acted on, though both runs had
        stopped and handed over. The generic field is no longer read."""
        aid, npath = self.spawn("zach-opus-c6")
        self.commit(npath)
        self.finish(aid)
        rec = self.rec("zach-opus-c6")
        rec["correction"] = {"at": ws.now(), "text": "Hand over now"}  # written before 2026-10-24
        ws.save_agent(rec)
        self.assertEqual([h for h in ws.holds(self.sid) if h["source"] == "correction"], [])

    def test_holds_t6_a_changes_requested_review_handed_over_without_the_fix(self):
        aid, npath = self.spawn("zach-opus-r6")
        self.commit(npath)
        tip = run("git", "-C", npath, "rev-parse", "HEAD").stdout.strip()
        state = os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "state")
        os.makedirs(state, exist_ok=True)
        with open(os.path.join(state, "reviews.jsonl"), "a") as f:
            f.write(json.dumps({"id": "rv-fixture-cr", "tip": tip, "verdict": "changes-requested", "findings": 1,
                                "trigger": "long-job", "finished_at": ws.iso()}) + "\n")
        self.finish(aid)                                       # hands over on the reviewed tip
        held = [h for h in ws.holds(self.sid) if h["source"] == "review"]
        self.assertEqual([(h["name"], h["review"]) for h in held], [("zach-opus-r6", "rv-fixture-cr")])
        ws.release("zach-opus-r6", ceo_word="ship it as it is", session_id=self.sid)
        self.assertEqual([h for h in ws.holds(self.sid) if h["source"] == "review"], [])

class OrchestrationR2Waits(Base):
    """Orchestration reliability R2 (richos-hq docs/prds/2026-10-23-orchestration-
    reliability.md): a wait names an end the engine can check; a self-wait or an agent:
    cycle is refused with the cycle; ownership is decided inside the registry lock."""

    def _finished(self, name):
        aid, npath = self.spawn(name)
        self.commit(npath)
        self.finish(aid)
        return aid, npath

    def test_r2_a_new_wait_needs_a_checkable_end(self):
        self._finished("zach-opus-w1")
        with self.assertRaises(ws.SpecError) as cm:
            ws.wait("zach-opus-w1", "started", "honorfile3 is running now", "", self.sid)
        self.assertIn("--until", str(cm.exception))
        with self.assertRaises(ws.SpecError):
            ws.hold("stream-e", "Stream E", "", "on hold", self.sid)
        ws.wait("zach-opus-w1", "started", "the carrier lands it", "", self.sid, "agent:zach-opus-w2")
        self.assertEqual(self.rec("zach-opus-w1")["waiting"]["end"], "agent:zach-opus-w2")

    def _hold_cli(self, name, until):
        import contextlib, io
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = ws.main(["--session", self.sid, "--entity", self.entity, "hold", name,
                          "--what", "the release notes", "--until", until])
        return rc, buf.getvalue()

    def test_r2_a_hold_on_a_finished_agent_prints_the_holds_end(self):
        """2026-10-24: `workspaces.sh hold newt-... --until agent:ray-...` recorded the hold,
        then crashed printing it (TypeError: str + dict): the agent's own run end, a dict,
        was printed in place of the hold's end."""
        self._finished("zach-opus-h1")
        self.assertIsInstance(self.rec("zach-opus-h1")["end"], dict)    # its run has a recorded end
        rc, out = self._hold_cli("zach-opus-h1", "agent:zach-opus-h2")
        self.assertEqual(rc, 0, out)
        self.assertEqual(out.strip(), "held: zach-opus-h1 until agent:zach-opus-h2")
        self.assertEqual(self.rec("zach-opus-h1")["waiting"]["end"], "agent:zach-opus-h2")

    def test_r2_a_standalone_hold_still_prints_its_end(self):
        rc, out = self._hold_cli("stream-h", "agent:zach-opus-h2")
        self.assertEqual(rc, 0, out)
        self.assertEqual(out.strip(), "held: stream-h until agent:zach-opus-h2")

    def test_r2_the_detail_command_lists_every_due_item_past_the_shown_eight(self):
        """Review rv-20261024T013623Z-f883a475-d24b: the watcher showed eight due items and
        pointed to `workspaces.sh holds`, which read holds() alone and printed nothing for
        finished work without a wait, so the ninth worker was never named (the lead had 12
        such items on 2026-10-23). The detail command reads the same due calculation."""
        names = ["zach-opus-n%d" % i for i in range(1, 10)]
        for n in names:
            self._finished(n)
        due = ws.due_set(self.sid, ws.pending(self.sid, self.entity, auto=False, dry=True))
        self.assertEqual(sorted(d["name"] for d in due), names)
        shown = ws.due_lines(due)
        self.assertEqual(len(shown), 9)                                  # eight, then the pointer
        self.assertIn("workspaces.sh holds", shown[-1])
        import contextlib, io
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = ws.main(["--session", self.sid, "--entity", self.entity, "holds"])
        out = buf.getvalue()
        self.assertEqual(rc, 0, out)
        for n in names:
            self.assertRegex(out, r"(?m)^DUE .*%s" % n)                  # all nine, the ninth too

    def test_r2_a_pause_cycle_is_refused_like_a_wait_cycle(self):
        """Review rv-20261024T013623Z-f883a475-d24b: pause(B, until A) after pause(A, until B)
        was accepted and both holds stayed quiet, the two-agent deadlock R2 names."""
        self.spawn("zach-opus-pa")
        self.spawn("zach-opus-pb")
        ws.pause("zach-opus-pa", "agent:zach-opus-pb", self.sid)
        with self.assertRaises(ws.SpecError) as cm:
            ws.pause("zach-opus-pb", "agent:zach-opus-pa", self.sid)
        self.assertIn("zach-opus-pb -> zach-opus-pa -> zach-opus-pb", str(cm.exception))
        self.assertFalse(self.rec("zach-opus-pb").get("pause"))
        ws.pause("zach-opus-pb", "at:2099-01-01T00:00:00Z", self.sid)     # control: an end that is not a cycle
        self.assertTrue(self.rec("zach-opus-pb").get("pause"))

    def test_r2_a_self_wait_and_an_agent_cycle_are_refused_with_the_cycle(self):
        self._finished("zach-opus-c1")
        self._finished("zach-opus-c2")
        with self.assertRaises(ws.SpecError) as cm:
            ws.wait("zach-opus-c1", "started", "itself", "", self.sid, "agent:zach-opus-c1")
        self.assertIn("zach-opus-c1 -> zach-opus-c1", str(cm.exception))
        ws.wait("zach-opus-c1", "started", "c2 lands it", "", self.sid, "agent:zach-opus-c2")
        with self.assertRaises(ws.SpecError) as cm:
            ws.wait("zach-opus-c2", "started", "c1 lands it", "", self.sid, "agent:zach-opus-c1")
        self.assertIn("zach-opus-c2 -> zach-opus-c1 -> zach-opus-c2", str(cm.exception))
        self.assertIsNone(self.rec("zach-opus-c2").get("waiting"))       # only the mutation was refused

    def test_r2_a_legacy_free_text_wait_reads_as_due(self):
        self._finished("zach-opus-l1")
        rec = self.rec("zach-opus-l1")
        rec["waiting"] = {"kind": "started", "on": "filecard3 is running now", "todo": "", "at": ws.now(), "end": ""}
        ws.save_agent(rec)                                               # written before 2026-10-24
        held = [h for h in ws.holds(self.sid) if h["name"] == "zach-opus-l1"]
        self.assertEqual([h["state"] for h in held], ["none"])

    def test_r2_the_claim_is_decided_again_inside_the_lock(self):
        """Two leads: the read outside the lock said claimable; inside it the record is
        another live lead's. It is neither claimed nor handled here."""
        self._finished("zach-opus-k1")
        rec = self.rec("zach-opus-k1")
        rec["session_id"] = "sess-ended-0000"
        ws.save_agent(rec)
        with patch.object(ws, "_claimable", side_effect=[True, False]):
            items = ws.pending(self.sid, self.entity, auto=False)
        self.assertEqual([i["name"] for i in items], [])
        self.assertNotEqual(ws.load_agent(rec["key"]).get("claimed_by"), self.sid)

    def test_r2_work_another_live_lead_claimed_is_not_due_here(self):
        """Review rv-20261024T062159Z-abcb35c8-f97e: an ended session's due work, claimed by
        one live lead, was still due for a second live lead through holds(), whose fallback
        and watcher told it to spawn a continuation that register_spawn refuses. Due work
        follows the ownership pending() uses; once the claimant's session ends, it is due
        here again."""
        self._finished("zach-opus-k2")
        rec = self.rec("zach-opus-k2")
        rec["session_id"] = "sess-ended-0002"
        rec["waiting"] = {"kind": "started", "on": "the prerequisite has ended", "at": ws.now(),
                          "end": "at:2000-01-01T00:00:00Z"}
        ws.save_agent(rec)
        ws.record_session_end("sess-ended-0002")
        due_here = lambda: [d["name"] for d in ws.due_set(self.sid, ws.pending(self.sid, self.entity, auto=False, dry=True))]
        self.assertEqual(due_here(), ["zach-opus-k2"])                     # control: unclaimed, due here
        other = "sess-bbbbbbbb-2222"
        self.env.session(other, self.entity)
        ws.pending(other, self.entity, auto=False)                        # the other lead claims it
        self.env.use(self.sess)
        self.assertEqual(ws.load_agent(rec["key"]).get("claimed_by"), other)
        self.assertEqual(ws.pending(self.sid, self.entity, auto=False, dry=True), [])
        self.assertEqual(due_here(), [])                                   # the claimant's, not ours
        self.assertNotIn("zach-opus-k2", [h["name"] for h in ws.holds(self.sid)])
        self.assertNotIn("continues: zach-opus-k2", ws.fallback_notice(self.sid) or "")
        ws.record_session_end(other)
        self.assertEqual(due_here(), ["zach-opus-k2"])                     # its claimant ended: ours again


class AppDueNotice(Base):
    """CEO §138: everything he uses in the terminal is a feature of the regular app. In the app,
    Rich's turn opens with the due work (app-engine-hook.py held_work_context, fallback_notice),
    and the notice gave him workspaces.sh commands the app's lease PATH does not carry
    (engine_profile.rs lease_path). There it names the richos_work tools (mega-lander/app.py TOOLS);
    in the terminal it is unchanged."""

    APP = {"RICHOS_APP_ENTITY": "entity-fixture", "RICHOS_APP_THREAD": "thread-fixture"}

    def _every_kind_due(self):
        """One due item of each kind, more than the eight a notice shows, and a damaged record."""
        self.spawn("zach-opus-ap")
        ws.pause("zach-opus-ap", "Rich's resume", self.sid)                      # a pause
        for n in ["zach-opus-af"] + ["zach-opus-a%d" % i for i in range(1, 6)]:
            aid, npath = self.spawn(n)
            self.commit(npath)
            self.finish(aid)                                                      # finished, not landed
        aid, npath = self.spawn("zach-opus-as")
        self.commit(npath)
        ws.stop("zach-opus-as", "he said stop", self.sid)                         # stopped
        aid, npath = self.spawn("zach-opus-aw")
        self.commit(npath)
        self.finish(aid)
        rec = self.rec("zach-opus-aw")
        rec["waiting"] = {"kind": "started", "on": "the review", "at": ws.now(), "end": "at:2000-01-01T00:00:00Z"}
        ws.save_agent(rec)                                                        # a wait whose end came
        ws.hold("stream-a", "Stream A", "ceo:CEO-TODOs 9.1", "on hold", self.sid)  # held, no teammate
        with open(ws.agent_path("damaged-fixture"), "w") as f:
            f.write("{")
        due = ws.due_set(self.sid, ws.pending(self.sid, self.entity, auto=False, dry=True))
        self.assertEqual({d["source"] for d in due}, {"pause", "finished", "wait", "hold"}, due)
        self.assertGreater(len(due), 8)
        return {d["name"]: d["command"] for d in due}

    def _notices(self):
        start, _n = ws.lifecycle({"hook_event_name": "SessionStart", "source": "compact",
                                  "session_id": self.sid, "cwd": self.entity}, self.entity)
        return [ws.fallback_notice(self.sid), start, ws.unreadable_line()]

    def test_app_due_notice_names_only_the_apps_own_tools(self):
        import re
        spec = importlib.util.spec_from_file_location("richos_app_for_notice", os.path.join(HERE, "..", "app.py"))
        app = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(app)
        tools = {t["name"] for t in app.TOOLS}
        with patch.dict(os.environ, self.APP):
            commands = self._every_kind_due()
            notices = self._notices()
            with patch.object(ws, "due_set", side_effect=OSError("registry unreadable")):
                notices.append(ws.fallback_notice(self.sid))                     # an unread due set
        self.assertTrue(all(notices), notices)
        self.assertIn("could NOT be read", notices[-1])
        for text in list(commands.values()) + notices:
            self.assertNotRegex(text, r"[\w.-]+\.sh\b", text)                    # no terminal script
            self.assertNotIn("--ceo-word", text)
            for named in re.findall(r"mcp__richos_work__(\w+)", text):
                self.assertIn(named, tools, text)                                 # a tool the app has
        self.assertIn("mcp__richos_work__resume_message to zach-opus-ap", commands["zach-opus-ap"])
        self.assertIn("mcp__richos_work__integrate", commands["zach-opus-af"])
        self.assertIn("continue_of", commands["zach-opus-af"])
        self.assertIn("continue_of", commands["zach-opus-aw"])
        self.assertIn("continues: stream-a", commands["stream-a"])
        self.assertIn("shelved/zach-opus-as", commands["zach-opus-as"])
        self.assertNotIn("integrate", commands["zach-opus-as"])                 # stopped: never landed
        self.assertNotIn("continu", commands["zach-opus-as"].replace("never land or continue it", ""))
        # An action the app has no tool for is said, never dropped or shown as done: no discard,
        # so saving a stopped worker's commits does not close it (Codex, 2026-10-24).
        self.assertIn("The app has no tool that discards it", commands["zach-opus-af"])
        self.assertIn("The app has no tool that discards it, so saving them does not close it",
                      commands["zach-opus-as"])
        self.assertIn("The app has no tool that repairs or removes it.", notices[2])

    def test_terminal_due_notice_is_unchanged(self):
        commands = self._every_kind_due()
        self.assertEqual(commands["zach-opus-ap"], "workspaces.sh resume zach-opus-ap")
        self.assertEqual(commands["zach-opus-af"],
                         "land it (workspaces.sh land zach-opus-af), discard it, or spawn its continuation")
        self.assertEqual(commands["zach-opus-aw"], "spawn the teammate that does it with the line  continues: zach-opus-aw")
        self.assertEqual(commands["stream-a"], "spawn the teammate that does it with the line  continues: stream-a")
        self.assertEqual(commands["zach-opus-as"],
                         "save its commits on shelved/zach-opus-as and discard it with his words "
                         "(workspaces.sh discard zach-opus-as --ceo-word '<his words>'); never land or continue it")
        fallback, start, bad = self._notices()
        self.assertIn("STALL WATCH NOT ACTIVE (stall-watch: NOT WATCHED: nothing looks for stalls in this session. "
                      "Start as a background command (Bash with run_in_background: true): stall-watch.sh --watch)",
                      fallback)
        self.assertIn("more due; the complete list: workspaces.sh holds", fallback)
        self.assertTrue(start.startswith("DUE WORK (%d): handle it as you resume; the complete list: "
                                         "workspaces.sh holds\n" % len(commands)), start)
        self.assertTrue(bad.endswith("the due work told is from the readable records only. workspaces.sh status "
                                     "names it; repair or remove it when convenient."), bad)


if __name__ == "__main__":
    # No arguments: every point. Arguments: the named classes or tests only,
    # e.g. `workspaces.test.py Point05_Guarantee` -- one point's proof -- or a
    # test method's name (or its start), e.g. `test_point_05_`.
    runner = unittest.TextTestRunner(stream=sys.stdout, verbosity=0, resultclass=_Result)
    loader = unittest.defaultTestLoader
    names = _resolve_names([a for a in sys.argv[1:] if a], loader)
    suite = (loader.loadTestsFromNames(names, sys.modules[__name__]) if names
             else loader.loadTestsFromModule(sys.modules[__name__]))
    result = runner.run(suite)
    print("=== workspaces spec tests: %d run, %d failed ===" % (
        result.testsRun, len(result.failures) + len(result.errors)))
    sys.exit(0 if result.wasSuccessful() else 1)

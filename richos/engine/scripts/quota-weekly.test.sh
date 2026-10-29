#!/usr/bin/env bash
# Covers quota_weekly.py, quota_watch.py, pause_protocol.py and quota-reset.sh delegation.
# Shared weekly policy integration. Fake redemption only; never real credentials.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# The app's state is never read from a test unless a case sets it (a test must not read the
# LIVE claude-quota.json; richos-hq follow-ups.md 7fed0bef, R3).
unset RICHOS_APP_STATE
PYTHONPATH="$HERE/lib${PYTHONPATH:+:$PYTHONPATH}" python3 - <<'PY'
import contextlib, io, json, types, unittest
from unittest.mock import patch
import quota_weekly as weekly
import quota_watch as watcher
import pause_protocol

NOW = 1800000000

def reading(used=99, five=10, until=NOW + 3600):
    return {'state':'ok','used':five,'resets_at':until,'ended':until <= NOW,'age':0,
            'why':'','windows':[{'id':'seven_day','label':'Weekly','used':used,'resets_at':NOW+86400}]}

def workers(working=None, held=None, paused=None):
    return {'known':True,'working':working or [],'quota_paused':paused or [],
            'weekly_paused':held or [],'other_paused':[], 'session':'fixture'}

class Recorder(watcher.Out):
    """The real Out, also keeping (kind, the poll's clock) of every wake."""
    def __init__(self,*a,**k):
        super().__init__(*a,**k); self.woken=[]
    def wake(self,kind,text,now):
        self.woken.append((kind,now))
        return super().wake(kind,text,now)

def run_polls(readings,worker_rows,polls,args=None,out_cls=None):
    """Run the real poll loop for `polls` polls. The clock advances one poll (300 s) per sleep; a
    reading that is an Exception is raised from read_source. Returns the Out it wrote to."""
    out_cls=out_cls or watcher.Out
    args=args or types.SimpleNamespace(threshold=93,threshold_problem='',config='',until_reset=False,
        engine_root='fixture',poll=300,stale=300,command='watch')
    out=out_cls(lines=io.StringIO(),events=io.StringIO())
    naps=[0]
    class Done(BaseException): pass
    def nap(_seconds):
        naps[0]+=1
        if naps[0]>=polls: raise Done
    def read(_a,_now):
        r=readings[min(naps[0],len(readings)-1)]
        if isinstance(r,Exception): raise r
        return dict(r)
    def who(_engine):
        return worker_rows[min(naps[0],len(worker_rows)-1)]
    with patch.object(watcher,'read_source',side_effect=read),patch.object(watcher,'workers',side_effect=who), \
        patch.object(watcher.time,'time',side_effect=lambda:NOW+300*naps[0]),patch.object(watcher.time,'sleep',side_effect=nap), \
        patch.object(weekly,'reset_tick',return_value={'error':'no offer'}):
        try:
            watcher.poll_loop(args,out)
        except Done:
            return out
    raise AssertionError('the poll loop returned before its polls were done')

class Weekly(unittest.TestCase):
    def test_all_reported_windows_and_provider_names(self):
        windows = weekly.windows({'five_hour':{'utilization':14,'resets_at':'2099-01-01T00:00:00Z'},
            'seven_day':{'utilization':92,'resets_at':'2099-01-02T00:00:00Z'},
            'model_scoped':[{'display_name':'Fable','utilization':8,'resets_at':'2099-01-02T00:00:00Z'}],
            'seven_day_new_model':{'utilization':30,'resets_at':'2099-01-02T00:00:00Z'}},watcher._parse_reset)
        self.assertEqual([w['used'] for w in windows],[14,92,8,30])
        self.assertIn('model:Fable',[w['id'] for w in windows])
    def test_watcher_helper_can_only_tick_never_approve(self):
        fake=types.SimpleNamespace(returncode=0,stdout='{"resets":{"approval":null}}')
        with patch.object(weekly.subprocess,'run',return_value=fake) as run:
            self.assertIsNone(weekly.reset_tick('/engine')['resets']['approval'])
            self.assertEqual(run.call_args.args[0],['/engine/scripts/quota-reset.sh','tick'])
    def test_missing_reset_time_holds_without_guessing_a_date(self):
        r=reading();r['windows'][0]['resets_at']=None
        self.assertTrue(weekly.held(r,NOW))
        r["windows"][0]["resets_at"]=NOW-1
        self.assertTrue(weekly.held(r,NOW), "elapsed reset time alone cannot release a weekly hold")
        pause_protocol.validate_text(pause_protocol.render('weekly-quota',None))
    def test_trigger_is_overall_weekly_not_model_or_five_hour(self):
        for value in (0,92,98.99,99,100):
            r=reading(value,100)
            r['windows'].append({'id':'model:Fable','used':100,'label':'Fable','resets_at':NOW+86400})
            self.assertEqual(weekly.held(r,NOW),value>=99)
    def test_weekly_has_no_twenty_minute_exception(self):
        r=reading();r['windows'][0]['resets_at']=NOW+1
        self.assertTrue(weekly.held(r,NOW))
    def test_unknown_or_stale_cannot_release_weekly_hold(self):
        args=types.SimpleNamespace(stale=300,command='watch')
        for r in ({},reading(20)):
            r['age']=301
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(weekly.handle(args,r,NOW,workers(held=['worker']),{},lambda:'below'),(True,False))
    def test_reset_does_not_release_while_five_hour_still_holds(self):
        args=types.SimpleNamespace(stale=300,command='watch')
        self.assertEqual(weekly.handle(args,reading(0),NOW,workers(held=['worker']),{},lambda:'at-or-above'),(True,False))
        out=io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(weekly.handle(args,reading(0),NOW,workers(held=['worker']),{},lambda:'below'),(False,True))
        self.assertIn('RESUME:',out.getvalue())
    def test_the_weekly_release_prints_the_generated_resume_a_hold_accepts(self):
        args=types.SimpleNamespace(stale=300,command='watch')
        out=io.StringIO()
        with contextlib.redirect_stdout(out):
            weekly.handle(args,reading(0),NOW,workers(held=['worker']),{},lambda:'below')
        lines=out.getvalue().splitlines()
        start=next(i for i,l in enumerate(lines) if l.startswith('RESUME:'))
        end=next(i for i,l in enumerate(lines) if l.startswith(pause_protocol.RESUME_MARKER))
        message='\n'.join(lines[start:end+1])
        self.assertTrue(pause_protocol.is_generated_resume(message),message)
        self.assertEqual(pause_protocol.validate_resume_text(message)['reason'],'weekly-quota')
    def test_standard_weekly_message_is_validated_unchanged(self):
        message=pause_protocol.render('weekly-quota','2026-10-01T09:00:00Z')
        self.assertEqual(pause_protocol.validate_text(message)['reason'],'weekly-quota')
        with self.assertRaises(ValueError):pause_protocol.validate_text(message+'\nStop your processes.')
    def test_watcher_checks_weekly_before_five_hour_release_even_until_reset(self):
        for five_until in (NOW-1,NOW+100):
            for until_reset in (False,True):
                args=types.SimpleNamespace(threshold=93,threshold_problem='',config='',until_reset=until_reset,
                    engine_root='fixture',poll=300,stale=300,command='watch')
                r=reading(99,95,five_until)
                out=io.StringIO()
                # The poll loop never returns on an event (2026-09-28): its
                # first sleep ends this test, after the first poll's output.
                class FirstPollDone(BaseException): pass
                def end(_seconds): raise FirstPollDone
                with patch.object(watcher,'read_source',return_value=r),patch.object(watcher,'workers',return_value=workers(['worker'])), \
                    patch.object(watcher.time,'time',return_value=NOW),patch.object(watcher.time,'sleep',side_effect=end), \
                    patch.object(weekly,'reset_tick',return_value={'error':'no offer'}):
                    with self.assertRaises(FirstPollDone):
                        watcher.poll_loop(args,watcher.Out(lines=out,events=out))
                text=out.getvalue()
                self.assertIn('WEEKLY-QUOTA-THRESHOLD',text)
                self.assertNotIn('RESUME:',text)
                message=text.split('WAIT until the orchestrator',1)[1].split('\n  Summary:',1)[0]
                pause_protocol.validate_text('WAIT until the orchestrator'+message)
    def drive(self,readings,worker_rows,polls,args=None,out_cls=None):
        """Everything run_polls woke the lead with."""
        return run_polls(readings,worker_rows,polls,args,out_cls).events.getvalue()
    def second_window(self,used):
        r=reading(used);r['windows'][0]['resets_at']=NOW+86400+604800
        return r
    def test_a_second_weekly_window_gets_its_own_threshold_event(self):
        # Hunt part 5, P5-16: 99%, reset to 0%, release, then 99% in the NEXT weekly window with the same
        # worker. The dedupe key named the headline and the workers, so the second crossing never woke the lead.
        events=self.drive([reading(99),reading(0),self.second_window(99)],
                          [workers(['worker']),workers(held=['worker']),workers(['worker'])],3)
        self.assertEqual(events.count('WEEKLY-QUOTA-THRESHOLD'),2,events)
        self.assertEqual(events.count('WEEKLY-QUOTA-RELEASE'),1,events)
    def test_the_same_weekly_window_still_wakes_once(self):
        events=self.drive([reading(99),reading(99),reading(99)],[workers(['worker'])],3)
        self.assertEqual(events.count('WEEKLY-QUOTA-THRESHOLD'),1,events)
    def test_repeated_poll_exceptions_tell_the_lead_the_watcher_is_blind(self):
        # Hunt part 5, P5-15: every poll raises; each exception restarted the loop and its blind timers, so the
        # lead was never told. After one poll's worth of consecutive failures it is, once.
        events=self.drive([RuntimeError('boom')],[workers(['worker'])],5)
        self.assertEqual(events.count('QUOTA-UNKNOWN'),1,events)
        self.assertIn('RuntimeError',events)
    def test_a_threshold_notice_that_fails_at_every_poll_still_reaches_the_blind_alarm(self):
        # Hunt part 5, P5-15 (v2 re-check): the quota is READ fine but delivering the threshold notice
        # raises at every poll. The failure count was reset right after the read, so these polls never
        # completed yet never added up to the QUOTA-UNKNOWN alarm. A poll ends an episode only when done.
        class FailingThreshold(watcher.Out):
            def wake(self,kind,*rest,**kw):
                if kind=='THRESHOLD': raise OSError('fixture: the threshold notice could not be written')
                return super().wake(kind,*rest,**kw)
        events=self.drive([reading(5,95)],[workers(['worker'])],5,out_cls=FailingThreshold)
        self.assertEqual(events.count('QUOTA-UNKNOWN'),1,events)
        self.assertIn('OSError',events)
    def test_one_failed_poll_between_good_ones_does_not_wake_the_lead(self):
        events=self.drive([reading(10),RuntimeError('boom'),reading(10),reading(10)],[workers(['worker'])],4)
        self.assertNotIn('QUOTA-UNKNOWN',events)
    def test_unusable_reset_still_pauses(self):
        args=types.SimpleNamespace(stale=300,command='watch')
        for status in ({'error':'unavailable'},{'actionError':'uncertain'},{'resets':{'approval':None}}):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(weekly.handle(args,reading(),NOW,workers(['worker']),status,lambda:'below'),(True,True))

class AppReading(unittest.TestCase):
    """richos-hq follow-ups.md 7fed0bef: for the app, the reading is the app's own of the account in
    use, never a probe and never the status-line file, which hold another account's figures."""
    def setUp(self):
        import tempfile
        self.dir=tempfile.TemporaryDirectory(); self.state=self.dir.name
    def tearDown(self): self.dir.cleanup()
    def publish(self, checked, five=28, week=3, in_use={'id':'2','label':'Work'}):
        view={'windows':[{'id':'five_hour','label':'Five-hour','usedPercent':five,'resetsAt':(NOW+3600)*1000,'durationMs':18000000},
                         {'id':'seven_day','label':'Weekly','usedPercent':week,'resetsAt':(NOW+86400)*1000,'durationMs':604800000}],
              'checkedAt':checked*1000,'state':'fresh','resets':{'weeklyUsed':None},
              'accounts':[{'id':'1','label':'Home','inUse':False,'windows':[{'id':'seven_day','usedPercent':99}]}]}
        if in_use: view['inUse']=in_use
        with open(self.state+'/claude-quota.json','w') as f: json.dump(view,f)
    def read(self):
        a=types.SimpleNamespace(payload='/nonexistent/statusline-payload.json',engine_root='fixture')
        with patch.dict(watcher.os.environ,{'RICHOS_APP_STATE':self.state}), \
             patch.object(watcher,'read_get_usage',side_effect=AssertionError('probed in the app')), \
             patch.object(watcher,'read_reading',side_effect=AssertionError('read the status line in the app')):
            return watcher.read_source(a,NOW)
    def test_the_in_use_account_is_read_and_named(self):
        self.publish(NOW-30)
        r=self.read()
        self.assertEqual((r['state'],r['used'],r['source']),('ok',28,'the app (account Work)'))
        self.assertEqual(weekly.weekly(r)['used'],3); self.assertEqual(r['age'],30)
        self.assertFalse(weekly.held(r,NOW))
        self.assertEqual(watcher.classify(r,93,300)[0],'below')
    def test_an_old_app_file_names_its_accounts_row_never_account_1_by_default(self):
        # The live file of 2026-10-28 17:14Z had no inUse; defaulting to Account 1 named Home.
        self.publish(NOW-30,in_use=None)
        self.assertEqual(self.read()['source'],'the app (account not named)')
        with open(self.state+'/claude-quota.json') as f: view=json.load(f)
        view['accounts'].append({'id':'2','label':'Work','inUse':True,'windows':[]})
        with open(self.state+'/claude-quota.json','w') as f: json.dump(view,f)
        self.assertEqual((self.read()['source'],self.read()['account']['id']),('the app (account Work)','2'))
        view['accounts']=[]; view['firstLabel']='Home'
        with open(self.state+'/claude-quota.json','w') as f: json.dump(view,f)
        self.assertEqual((self.read()['source'],self.read()['account']['id']),('the app (account Home)','1'))
    def test_stale_missing_or_unreadable_is_unknown_never_a_probe(self):
        self.publish(NOW-3600)
        self.assertEqual(watcher.classify(self.read(),93,300)[0],'unknown')
        import os; os.remove(self.state+'/claude-quota.json')
        self.assertEqual(self.read()['state'],'missing')
        with open(self.state+'/claude-quota.json','w') as f: f.write('{not json')
        self.assertEqual(self.read()['state'],'malformed')
        self.publish(0)
        self.assertEqual(self.read()['state'],'missing')
    def test_the_app_never_ticks_the_shared_reset_record(self):
        with patch.dict(weekly.os.environ,{'RICHOS_APP_STATE':self.state}), \
             patch.object(weekly.subprocess,'run',side_effect=AssertionError('ticked from the app')):
            self.assertEqual(weekly.reset_tick('/engine'),{'status':weekly.APP_RUNS_RESET})

class GateAdmission(unittest.TestCase):
    """Second review of d37b1f0de (rv-20261028T184535Z-d37b1f0d-e0d9), blocking finding: an app review
    takes the desktop gate's own decision from the published file and the pause setting beside it
    (quota/gate.rs `admission`), with no terminal entity config. Only a hold holds. The decision is
    the gate's recompute from the published figures, never the published `admission`, which every
    case below sets to the opposite of the gate's answer (second review of 2097df51c)."""
    def setUp(self):
        import tempfile, os
        self.dir=tempfile.TemporaryDirectory(); self.app=self.dir.name
        self.state=os.path.join(self.app,'engine-state'); os.makedirs(self.state)
    def tearDown(self): self.dir.cleanup()
    def publish(self, five=99, five_reset=NOW+4740, week=20, week_reset=NOW+300000, speeds=None, admission=None,
                held_until=None, policy=None, setting=None, checked=NOW-30):
        win=lambda i,used,reset,d: {'id':i,'label':i,'usedPercent':used,'resetsAt':reset*1000,'durationMs':d}
        view={'state':'fresh','refreshIntervalMs':300000,
              'windows':[win('five_hour',five,five_reset,18000000),win('seven_day',week,week_reset,604800000)],
              'checkedAt':None if checked is None else checked*1000,'speeds':speeds or {},
              'policy':policy or {'enabled':True,'pausePercent':93},'admission':admission or {'state':'ready'},
              'heldUntil':held_until,'inUse':{'id':'2','label':'Work'},
              'accounts':[{'id':'1','label':'Home','inUse':False,'windows':[]},{'id':'2','label':'Work','inUse':True,'windows':[]}]}
        with open(self.state+'/claude-quota.json','w') as f: json.dump(view,f)
        if setting is not None:
            with open(self.app+'/claude-quota-policy.json','w') as f: json.dump(setting,f)
    def gate(self): return watcher.gate_admission(self.state,NOW)
    def test_the_account_in_use_at_its_line_holds_until_its_reset(self):
        self.publish()
        g=self.gate()
        self.assertEqual((g['held'],g['until'],g['account']['id']),(True,NOW+4740,'2'),g)
        self.assertIn('HOLDS work on Work (account 2)',g['line'])
    def test_a_hold_whose_window_has_reset_admits(self):
        self.publish(five_reset=NOW-1,admission={'state':'held','resetsAt':(NOW+4740)*1000})
        self.assertFalse(self.gate()['held'])
    def test_every_account_exhausted_holds_even_with_the_pause_off(self):
        self.publish(five=10,held_until=(NOW+600)*1000,policy={'enabled':False,'pausePercent':93})
        g=self.gate(); self.assertEqual((g['held'],g['until']),(True,NOW+600),g)
    def test_the_published_admission_is_never_the_answer(self):
        # The review's case: weekly 95% at one point a minute. The service published `ready` with a
        # measured 3-minute handoff (its point 96%); the gate, with the 5-minute default, has 94%.
        self.publish(five=10,week=95,speeds={'seven_day':1/60000},admission={'state':'ready'})
        g=self.gate(); self.assertEqual((g['held'],g['until']),(True,NOW+300000),g)
        self.assertIn('handoff point 94%',g['line'])
        # Inside the last 20 minutes the gate's exemption admits a published hold.
        self.publish(five=95,five_reset=NOW+19*60,admission={'state':'held','resetsAt':(NOW+19*60)*1000})
        self.assertFalse(self.gate()['held'])
        # A weekly hold that has reset leaves a five-hour hold still in force, until ITS reset.
        self.publish(five=96,five_reset=NOW+3600,week=99,week_reset=NOW-1,admission={'state':'held','resetsAt':(NOW-1)*1000})
        g=self.gate(); self.assertEqual((g['held'],g['until']),(True,NOW+3600),g)
    def test_no_reading_or_an_old_one_below_the_line_admits(self):
        self.publish(checked=None,admission={'state':'held','resetsAt':(NOW+4740)*1000})
        self.assertFalse(self.gate()['held'])
        self.publish(five=50,checked=NOW-3600,admission={'state':'held','resetsAt':(NOW+4740)*1000})
        self.assertFalse(self.gate()['held'])
    def test_the_setting_beside_the_file_wins_as_in_the_gate(self):
        self.publish(setting={'enabled':False,'pausePercent':93})
        self.assertFalse(self.gate()['held'])
        self.publish(five=85,setting={'enabled':True,'pausePercent':80})
        self.assertTrue(self.gate()['held'])
        self.publish(five=85,setting={'enabled':'yes'})   # not a Policy: the published one (93) decides
        self.assertFalse(self.gate()['held'])
    def test_no_file_or_a_broken_one_is_the_gates_unknown_and_admits(self):
        g=self.gate(); self.assertFalse(g['held']); self.assertIn('no usage reading',g['line'])
        with open(self.state+'/claude-quota.json','w') as f: f.write('{not json')
        self.assertFalse(self.gate()['held'])

class AccountSwitch(unittest.TestCase):
    """Second review of d37b1f0de (rv-20261028T184535Z-d37b1f0d-e0d9), finding 2: quota_watch's poll
    loop kept window_end and the notice state of the account it read first. Read Home, switch to
    Work, pause Work at 99%: the release came on Home's reset timer while Work's reset was still
    4,740 s away. Each account's hold follows its own reset."""
    HOME={'id':'1','label':'Home'}; WORK={'id':'2','label':'Work'}
    def on(self,account,five,until):
        r=reading(10,five,until); r['account']=dict(account); r['source']='the app (account %s)'%account['label']
        return r
    def test_after_a_switch_the_release_follows_the_new_accounts_reset(self):
        home_reset=NOW+1800                 # Home's last 20 minutes begin at NOW+600: the poll at NOW+900 is in them
        work_reset=NOW+900+4740             # 4,740 s past that poll, as in the review's fixture
        out=run_polls([self.on(self.HOME,40,home_reset)]+[self.on(self.WORK,99,work_reset)]*16,
                      [workers(['worker']),workers(['worker'])]+[workers(paused=['worker'])]*15,17,out_cls=Recorder)
        kinds=[k for k,_ in out.woken]
        self.assertIn('THRESHOLD',kinds,out.woken)
        releases=[t for k,t in out.woken if k=='RELEASE']
        self.assertEqual(len(releases),1,'one release, on Work\'s timer: %s'%out.woken)
        self.assertTrue(0<work_reset-releases[0]<1200,'released %d s before Work\'s reset: %s'%(work_reset-releases[0],out.woken))
        self.assertNotIn('RESET',kinds,'Home\'s reset is not the end of Work\'s window: %s'%out.woken)
    def test_a_hold_left_on_the_account_switched_from_releases_once_the_new_one_has_room(self):
        out=run_polls([self.on(self.HOME,95,NOW+7200)]*2+[self.on(self.WORK,10,NOW+9000)]*3,
                      [workers(['worker'])]+[workers(paused=['worker'])]*4,5,out_cls=Recorder)
        self.assertEqual([(k,t) for k,t in out.woken if k in('THRESHOLD','RELEASE')],
                         [('THRESHOLD',NOW),('RELEASE',NOW+600)],out.woken)
        self.assertIn('changed from Home to Work',out.events.getvalue())
    def test_a_switch_to_an_account_at_the_line_keeps_the_hold_and_tells_its_threshold(self):
        out=run_polls([self.on(self.HOME,95,NOW+7200)]+[self.on(self.WORK,96,NOW+9000)]*3,
                      [workers(['worker'])]*4,4,out_cls=Recorder)
        self.assertEqual([k for k,_ in out.woken],['THRESHOLD','THRESHOLD'],out.woken)

unittest.main()
PY

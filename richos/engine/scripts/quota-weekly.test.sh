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

def workers(working=None, held=None):
    return {'known':True,'working':working or [],'quota_paused':[],
            'weekly_paused':held or [],'other_paused':[], 'session':'fixture'}

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
        """Run the real poll loop for `polls` polls. The clock advances one poll per sleep; a reading
        that is an Exception is raised from read_source. Returns everything woken to the lead."""
        out_cls=out_cls or watcher.Out
        args=args or types.SimpleNamespace(threshold=93,threshold_problem='',config='',until_reset=False,
            engine_root='fixture',poll=300,stale=300,command='watch')
        lines,events=io.StringIO(),io.StringIO()
        naps=[0]
        class Done(BaseException): pass
        def nap(_seconds):
            naps[0]+=1
            if naps[0]>=polls: raise Done
        def read(_a,_now):
            r=readings[min(naps[0],len(readings)-1)]
            if isinstance(r,Exception): raise r
            return r
        def who(_engine):
            return worker_rows[min(naps[0],len(worker_rows)-1)]
        with patch.object(watcher,'read_source',side_effect=read),patch.object(watcher,'workers',side_effect=who), \
            patch.object(watcher.time,'time',side_effect=lambda:NOW+300*naps[0]),patch.object(watcher.time,'sleep',side_effect=nap), \
            patch.object(weekly,'reset_tick',return_value={'error':'no offer'}):
            with self.assertRaises(Done):
                watcher.poll_loop(args,out_cls(lines=lines,events=events))
        return events.getvalue()
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

unittest.main()
PY

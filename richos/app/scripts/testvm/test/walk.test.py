#!/usr/bin/env python3
"""Offline boundary tests. Live GUI behavior is deliberately a separate proof."""
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
HERE=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(HERE))
import fixture
import rollback
from scenario import Failure


def module(name):
    spec=importlib.util.spec_from_file_location(name,HERE/(name+'.py'))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value


class WalkTests(unittest.TestCase):
    def test_fixture_keeps_source_and_removes_only_history_in_delta(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'source';data=source/'Library/Application Support/com.richos.app';data.mkdir(parents=True)
            rows=[{'event':'ThreadCreated','thread_id':str(i),'entity_id':'e','person_id':'p','title':'old'} for i in range(2)]
            rows.append({'event':'MessageAdded','text':'old'})
            ledger=data/'conversation-ledger.jsonl';ledger.write_text(''.join(json.dumps(x)+'\n' for x in rows));original=ledger.read_bytes()
            (source/'.claude').symlink_to('/nonexistent/credentials')
            retired=source/'Library/Application Support/RichOS/engine.previous.fixture/runtime/bin'
            retired.mkdir(parents=True);(retired/'node').symlink_to('../node/bin/node')
            delta=Path(tmp)/'delta';long=Path(tmp)/'long'
            fixture.prepare(source,delta,'delta');fixture.prepare(source,long,'long-history')
            self.assertEqual(ledger.read_bytes(),original)
            self.assertEqual((long/'Library/Application Support/com.richos.app/conversation-ledger.jsonl').read_bytes(),original)
            cooked=[json.loads(x) for x in (delta/'Library/Application Support/com.richos.app/conversation-ledger.jsonl').read_text().splitlines()]
            self.assertEqual([r['title'] for r in cooked],['Scenario A','Scenario B'])
            self.assertFalse((delta/'.claude').exists())
            self.assertFalse((delta/'Library/Application Support/RichOS/engine.previous.fixture').exists())
    def test_fixture_refuses_links_before_editing_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'source';source.mkdir();outside=Path(tmp)/'original';outside.write_text('untouched')
            (source/'linked').symlink_to(outside)
            with self.assertRaises(ValueError):fixture.prepare(source,Path(tmp)/'copy','delta')
            self.assertEqual(outside.read_text(),'untouched');self.assertFalse((Path(tmp)/'copy').exists())
    def test_rollback_missing_history_never_browses_or_relaunches(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,TESTVM_ROOT=tmp):
            state=Path(tmp)/'run/demo';state.mkdir(parents=True);(state/'payload').write_text('/Users/admin/testvm/demo')
            with patch.object(rollback,'receipts',side_effect=RuntimeError('missing')),patch.object(rollback,'relaunch') as launch:
                with self.assertRaises(Failure) as raised:rollback.exercise('demo','1.0.1','1.0.2','https://example.invalid/releases/download/v1.0.2/update.json',check_only=True)
                self.assertEqual(raised.exception.outcome,'prerequisite unavailable');launch.assert_not_called()
    def test_rollback_keeps_partial_evidence_and_clears_development_engine_override(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,TESTVM_ROOT=tmp):
            state=Path(tmp)/'run/demo';state.mkdir(parents=True);(state/'payload').write_text('/Users/admin/testvm/demo')
            evidence=[];launches=[]
            def guest(vm,command,*args):
                if 'PlistBuddy' in command:return '1.0.1'
                if command.startswith('cat '):return 'boot without activation marker'
                return ''
            def launch(vm,app,env):launches.append(env);return {'log':'/guest/boot.log'}
            with patch.object(rollback,'guest',side_effect=guest),patch.object(rollback,'relaunch',side_effect=launch),patch.object(rollback,'wait_log',return_value='RICHOS-UPDATE-SELFTEST exit=0'),patch.object(rollback,'receipts',return_value={'current':'1.0.2','previous':'1.0.1'}),patch.object(rollback.time,'sleep'):
                with self.assertRaisesRegex(Failure,'activation direction'):
                    rollback.exercise('demo','1.0.1','1.0.2','https://example.invalid/releases/download/v1.0.2/latest.json','/guest/Old.app',progress=lambda r:evidence.append(json.loads(json.dumps(r))))
            self.assertEqual([s['action'] for s in evidence[-1]['steps']],['update','activate'])
            self.assertEqual(evidence[-1]['steps'][-1]['log'],'boot without activation marker')
            self.assertTrue(all(e['RICHOS_ENGINE_DIR']=='' and e['RICHOS_ENGINE_ROOT']=='' for e in launches))
    def test_rollback_rejects_moving_channel_before_accessing_guest(self):
        with patch.object(rollback,'guest') as guest:
            with self.assertRaises(ValueError):rollback.exercise('demo','1.0.1','1.0.2','https://example.invalid/latest.json')
            guest.assert_not_called()
    def test_manifest_orders_destructive_check_last_and_budget_six(self):
        manifest=json.loads((HERE/'scenarios/delta.json').read_text());steps=manifest['steps'];ids=[x['id'] for x in steps]
        self.assertLess(ids.index('rejected-phone'),ids.index('rollback'))
        self.assertLess(ids.index('samples'),ids.index('work-chip-switch'))
        # The phone web page is retired: no step reads a phone screen any more.
        for gone in ('typed-to-phone','reply-to-phone','phone-to-mac'):self.assertNotIn(gone,ids)
        self.assertEqual(manifest['model_turn_limit'],6)
        self.assertEqual(next(x['count'] for x in steps if x['id']=='samples'),5)
    def test_real_key_reads_use_default_search_list_and_never_emit_secrets(self):
        verifier=module('keychain-verify');calls=[]
        def guest(vm,command,timeout):calls.append(command);return json.dumps({a:'fingerprint' for a in verifier.ACCOUNTS})
        with patch.object(verifier,'guest',side_effect=guest):
            result=verifier.keys('demo','/Users/admin/testvm/demo/home')
        self.assertEqual(len(result),3);self.assertIn('find-generic-password',calls[0])
        self.assertNotIn('add-generic-password',calls[0]);self.assertNotIn('login.keychain',calls[0])
        self.assertNotIn(' -A ',calls[0]);self.assertIn('hashlib.sha256',calls[0])
    def test_driver_clock_is_milliseconds_and_negative_latency_is_explicit(self):
        walk=module('delta-walk');w=object.__new__(walk.Walk)
        self.assertEqual(w.bound([-100,500],1000,signed=True)['milliseconds'],[-100,500])
        with self.assertRaises(Failure):w.bound([-100],1000)
        with self.assertRaises(Failure):w.bound([1001],1000)
    def test_ocr_marker_separates_the_prompt_and_tolerates_only_spacing(self):
        walk=module('delta-walk');token='probeabcdef'
        pattern=re.compile(walk.marker_pattern(token),re.I)
        self.assertIsNone(pattern.search('Reverse this sequence: '+token[::-1]))
        self.assertIsNotNone(pattern.search(' '.join(token)))
        self.assertIsNotNone(pattern.search(token))
        self.assertIsNone(pattern.search(token.replace('b','6')))
        self.assertIsNone(pattern.search('x'+token+'x'))
    def test_every_wait_band_matcher_takes_the_opening_headline(self):
        # Review rv-20261018T195835Z-253cc28a-26b4#1: the band opens on "Rich is thinking"
        # (ui/main.js), and the three matchers took only the later two headlines.
        walk=module('delta-walk');pattern=re.compile(walk.WORKING_HEADLINE,re.I)
        for line in ('Rich is thinking','Rich is working...','Writing the reply'):
            self.assertIsNotNone(pattern.search('Conversation\n'+line+'\n'),line)
        self.assertIsNone(pattern.search('Rich is thinking about the plan'))
        self.assertNotIn('(?:Rich is working|Writing the reply)',(HERE/'delta-walk.py').read_text())
    def test_timing_interval_refuses_uncertainty_and_subtracts_conservatively(self):
        walk=module('delta-walk');w=object.__new__(walk.Walk)
        self.assertEqual(w.interval_bound([[0,100]],100)['intervals_ms'],[[0,100]])
        with self.assertRaises(Failure) as raised:w.interval_bound([[99,101]],100)
        self.assertEqual(raised.exception.outcome,'harness failure')
        with self.assertRaises(Failure) as raised:w.interval_bound([[101,102]],100)
        self.assertEqual(raised.exception.outcome,'product failure')
    def test_deadline_preserves_stdout_stderr_and_status(self):
        r=subprocess.run([sys.executable,str(HERE/'ax-deadline.py'),'3',sys.executable,'-c','import sys;print("partial");print("cause",file=sys.stderr);sys.exit(17)'],input=b'',capture_output=True)
        self.assertEqual(r.returncode,17);self.assertIn(b'partial',r.stdout);self.assertIn(b'cause',r.stderr)
    def test_relaunch_records_new_gui_process_instead_of_mcp_helpers(self):
        launch=module('relaunch')
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,TESTVM_ROOT=tmp):
            state=Path(tmp)/'run/demo';state.mkdir(parents=True)
            payload='/Users/admin/testvm/demo';app=payload+'/RichOS.app'
            exe=app+'/Contents/MacOS/richos-tauri'
            (state/'payload').write_text(payload);(state/'app.pid').write_text('123')
            calls=[]
            def guest(vm,command,*args):
                calls.append(command)
                if command.startswith('ps -p '):return exe
                if command=='ps -axo pid=,comm=':return '9 '+exe
                if command=='ps -axo pid=,ppid=,comm=':
                    return '9 1 '+exe+'\n456 1 '+exe+'\n567 456 '+exe
                return ''
            with patch.object(launch,'guest',side_effect=guest):result=launch.relaunch('demo',app)
            self.assertEqual(result['pid'],456)
            self.assertEqual((state/'app.pid').read_text().strip(),'456')
            self.assertEqual([c for c in calls if c.startswith('kill -TERM')],['kill -TERM 123 2>/dev/null || true'])
    def test_switch_uses_visible_header_and_requires_work_before_actual_switch(self):
        driver=module('delta-walk');w=object.__new__(driver.Walk)
        with tempfile.TemporaryDirectory() as tmp:
            w.out=Path(tmp);w.switched={'directory':tmp};w.switch={'before':200,'after':500}
            w.a=SimpleNamespace(thread_b='Scenario B',header_box=[300,28,650,55],chip_box=[320,500,670,145])
            w.ax=lambda *args:[{'current':'true'}]
            frames=[(n,n*100) for n in range(1,8)]
            img=SimpleNamespace(crop=lambda *args:None)
            def read(path,active=True):
                name=Path(path).name
                if name.startswith('switch-header-'):return 'Scenario B' if int(name[-8:-4])>=4 else 'Scenario A'
                if name=='0003.png':return 'Rich is working' if active else 'Done'
                return ''
            with patch.object(driver.timeline,'_read_meta',return_value=({},frames)),patch.object(driver.qaimg,'load',return_value=img),patch.object(driver.qaimg,'save'),patch.object(driver.qaocr,'text',side_effect=read):
                self.assertEqual(w.judge_switch()["visible_switch_ms"],400)
            with patch.object(driver.timeline,'_read_meta',return_value=({},frames)),patch.object(driver.qaimg,'load',return_value=img),patch.object(driver.qaimg,'save'),patch.object(driver.qaocr,'text',side_effect=lambda p:read(p,False)):
                with self.assertRaises(Failure) as raised:w.judge_switch()
                self.assertEqual(raised.exception.outcome,'prerequisite unavailable')


    def phone_walk(self,driver,calls,phone):
        """A delta Walk on a scripted guest whose phone is the walk phone (walkphone.py)."""
        url='https://richos-test-1.example.ts.net:8443/#pair=ABCD2345'
        w=object.__new__(driver.Walk)
        w.vm='walk-fixture';w.out=Path('/nonexistent-out');w.home='/Users/admin/testvm/demo/home'
        w.phone_state=w.out/'phone.json'
        w.a=SimpleNamespace(tailnet_route_choice=False,thread_a='Scenario A')
        def ax(mode,*args,app=None):
            calls.append(' '.join(map(str,[mode,*args,'app=%s'%app])))
            return [{'title':url,'desc':'Your phone said the six words did not match'}]
        w.ax=ax
        def pair(state,link,reject=False):
            phone.append((state.name,link,reject));return {'words':'six words'}
        return w,url,pair
    def test_delta_pairing_and_rejection_use_the_walk_phone_never_the_old_page(self):
        driver=module('delta-walk');calls=[];phone=[]
        w,url,pair=self.phone_walk(driver,calls,phone)
        def guest(vm,command,*args):
            calls.append('guest '+command)
            return 'abc  file' if command.startswith('shasum') else ''
        with patch.object(driver,'guest',side_effect=guest),patch.object(driver.walkphone,'pair',side_effect=pair),\
             patch.object(driver,'relaunch',return_value={'pid':42,'log':'/guest/boot.log'}),patch.object(driver.time,'sleep'):
            self.assertEqual(w.pair({},None)['words'],'six words')
            self.assertTrue(w.rejected_phone({},None)['record_survived'])
        self.assertEqual(phone,[('phone.json',url,False),('phone-rejected.json',url,True)])
        self.assertTrue(any(c.startswith('click --title They match') for c in calls),calls)
        self.assertFalse([c for c in calls if 'Safari' in c or c.startswith('guest open')],calls)
    def test_delta_phone_that_cannot_pair_is_a_named_product_failure(self):
        driver=module('delta-walk');w,url,_=self.phone_walk(driver,[],[])
        with patch.object(driver.walkphone,'pair',side_effect=driver.walkphone.PhoneFailed('not trusted')):
            with self.assertRaises(Failure) as raised:w.phone_pair(w.phone_state,url)
        self.assertEqual(raised.exception.outcome,'product failure');self.assertIn('not trusted',str(raised.exception))


if __name__=='__main__':unittest.main()

#!/usr/bin/env python3
"""Normal leader exit cannot strand Stop-probe or walk descendants. No guest or provider."""
import contextlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
from types import SimpleNamespace

ROOT=Path(os.environ.get('G8_ROOT',Path(__file__).resolve().parents[5]))
HERE=ROOT/'richos/app/scripts/testvm'
sys.path.insert(0,str(HERE))
spec=importlib.util.spec_from_file_location('run_walk',HERE/'run-walk.py')
walk=importlib.util.module_from_spec(spec);spec.loader.exec_module(walk)

class Cleanup(unittest.TestCase):
    def fixture(self, root):
        pidfile=root/'child.pid'; command=root/'scenario'
        command.write_text(f'''#!{sys.executable}
import os,signal,sys,time
if '--version' in sys.argv: print('fixture'); sys.exit(0)
if '--print' in sys.argv:
    for _ in range(2): sys.stdin.readline()
pid=os.fork()
if pid==0:
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    while True: time.sleep(1)
open({str(pidfile)!r},'w').write(str(pid))
sys.exit(0)
''');command.chmod(0o755)
        return command,pidfile

    def assert_gone(self,pidfile):
        self.assertTrue(pidfile.exists(),'owned scenario never executed')
        pid=int(pidfile.read_text())
        row=subprocess.run(['ps','-p',str(pid),'-o','stat='],capture_output=True,text=True).stdout.strip()
        self.assertTrue(not row or row.startswith('Z'),f'owned descendant {pid} still running: {row}')

    def cleanup(self,pidfile):
        if pidfile.exists():
            try: os.kill(int(pidfile.read_text()),signal.SIGKILL)
            except ProcessLookupError: pass

    def test_walk_cleans_both_boot_and_scenario_leader_exit(self):
        for phase in ('boot','scenario'):
            with self.subTest(phase=phase),tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp);command,pidfile=self.fixture(root)
                @contextlib.contextmanager
                def slot(**kwargs): yield root/'slot'
                handlers={s:signal.getsignal(s) for s in (signal.SIGINT,signal.SIGTERM,signal.SIGHUP)}
                try:
                    with patch.dict(os.environ,TESTVM_ROOT=str(root),CLAUDE_CONFIG_DIR=str(root/'private-config')),patch.object(walk,'guest_slot',slot),patch.object(walk,'boot_argv',return_value=[str(command)] if phase=='boot' else ['/usr/bin/true']),patch.object(sys,'argv',['run-walk.py','--bundle','fixture','--home',tmp,'--engine','fixture','--report',str(root/'report.json'),'--',str(command) if phase=='scenario' else '/usr/bin/true']):
                        self.assertEqual(walk.main(),0)
                    self.assertTrue(json.loads((root/'report.json').read_text())['cleanup_complete'])
                    self.assert_gone(pidfile)
                finally:
                    for s,h in handlers.items(): signal.signal(s,h)
                    self.cleanup(pidfile)

    def test_walk_stops_guest_before_tree_and_sweeps_even_if_stop_fails(self):
        for fail_stop in (False, True):
            with self.subTest(fail_stop=fail_stop), tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp); events=[]
                @contextlib.contextmanager
                def slot(**kwargs): yield root/'slot'
                class Tracker:
                    def __init__(self,*args): pass
                    def refresh(self,**kwargs): pass
                def finish(*args): events.append('tree'); return []
                tree=SimpleNamespace(SCOPE_ENV='FIXTURE_SCOPE',TrackedTree=Tracker,finish_scope=finish)
                class Child:
                    pid=12345
                    def __init__(self,args,**kwargs):
                        if args[0]=='boot': (root/'run'/args[1]).mkdir(parents=True)
                    def wait(self,**kwargs): return 0
                    def poll(self): return 0
                def stop(args,**kwargs):
                    events.append('guest stop')
                    if fail_stop: raise subprocess.CalledProcessError(1,args)
                    shutil.rmtree(root/'run'/args[1])
                handlers={sig:signal.getsignal(sig) for sig in (signal.SIGINT,signal.SIGTERM,signal.SIGHUP)}
                try:
                    with patch.dict(os.environ,TESTVM_ROOT=tmp),patch.object(walk,'guest_slot',slot),patch.object(walk,'boot_argv',side_effect=lambda vm,*a:['boot',vm]),patch.object(walk,'proc_tree',tree,create=True),patch.object(walk.subprocess,'Popen',Child),patch.object(walk.subprocess,'run',stop),patch.object(sys,'argv',['run-walk.py','--bundle','fixture','--home',tmp,'--engine','fixture','--report',str(root/'report.json'),'--','/usr/bin/true']):
                        if fail_stop:
                            with self.assertRaises(subprocess.CalledProcessError): walk.main()
                        else: self.assertEqual(walk.main(),0)
                    self.assertEqual(events,['guest stop','tree'])
                    self.assertEqual(json.loads((root/'report.json').read_text())['cleanup_complete'],not fail_stop)
                finally:
                    for sig,handler in handlers.items(): signal.signal(sig,handler)

    def test_stop_probe_cleans_after_provider_exits(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);command,pidfile=self.fixture(root)
            try:
                # load-bound: hang guard only; actual descendant absence establishes cleanup.
                result=subprocess.run([sys.executable,str(ROOT/'richos/app/scripts/qa/stop-probe.py'),'--mode','none','--out',str(root/'out'),'--claude',str(command),'--start-within','0.1'],env={**os.environ,'CLAUDE_CONFIG_DIR':str(root/'private-config')},capture_output=True,text=True,timeout=45)
                self.assertEqual(result.returncode,2,result.stdout+result.stderr)
                self.assert_gone(pidfile)
            finally: self.cleanup(pidfile)

if __name__=='__main__':unittest.main()

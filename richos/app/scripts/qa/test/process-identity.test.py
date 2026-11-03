#!/usr/bin/env python3
"""Reused identities never authorize a continue or adoption. All signals are recorders."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

ROOT = Path(os.environ.get('G8_ROOT', Path(__file__).resolve().parents[5]))
spec = importlib.util.spec_from_file_location('lab_pause', ROOT / 'richos/app/scripts/qa/lab-pause.py')
pause = importlib.util.module_from_spec(spec); spec.loader.exec_module(pause)

class Identity(unittest.TestCase):
    def test_resume_requires_the_stopped_generation_and_lab(self):
        for change in ('birth', 'lab', 'unreadable', 'none'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as tmp:
                data = Path(tmp) / 'data'; data.mkdir(); (data / 'lab-owner').write_text(pause.OWNER)
                (Path(tmp) / 'mac.json').write_text(json.dumps({'data':str(data), 'pid':12345}))
                state = {'changed':False}; sent = []
                def ps(*args, **kw):
                    if state['changed'] and change == 'unreadable': return SimpleNamespace(returncode=1, stdout='')
                    birth = 'Tue Nov 3 10:00:01 2026' if state['changed'] and change == 'birth' else 'Tue Nov 3 10:00:00 2026'
                    folder = '/foreign' if state['changed'] and change == 'lab' else str(data)
                    return SimpleNamespace(returncode=0, stdout=f'{birth} {pause.LISTENER} RICHOS_MOBILE_MAC_DIR={folder}')
                calls = 0
                def wait(*args):
                    nonlocal calls
                    calls += 1
                    if calls == 1: return 'STOP', 5
                    state['changed'] = True; return 'CONT', 10
                handlers = {s:signal.getsignal(s) for s in (signal.SIGINT,signal.SIGTERM,signal.SIGHUP)}
                try:
                    with patch.object(pause.subprocess,'run',ps), patch.object(pause,'wait_line',wait), patch.object(pause.os,'kill',lambda p,s:sent.append(s)), contextlib.redirect_stdout(io.StringIO()):
                        rc = pause.main(['--lab',tmp,'--log',tmp+'/log','--stop-at','STOP','--cont-at','CONT'])
                finally:
                    for s,h in handlers.items(): signal.signal(s,h)
                self.assertEqual(sent, [signal.SIGSTOP,signal.SIGCONT] if change=='none' else [signal.SIGSTOP])
                self.assertEqual(rc, 0 if change=='none' else 2)

    def test_listener_birth_is_independent_of_en_gb_locale(self):
        def ps(*args, **kwargs):
            env=kwargs.get('env',os.environ)
            stamp='Tue Nov 3 10:00:00 2026' if env.get('LC_ALL')=='C' else 'Tue  3 Nov 10:00:00 2026'
            return SimpleNamespace(returncode=0,stdout=f'{stamp} {pause.LISTENER} RICHOS_MOBILE_MAC_DIR=/fixture/lab')
        with patch.dict(os.environ,LANG='en_GB.UTF-8',LC_ALL='en_GB.UTF-8',TZ='Europe/London'),patch.object(pause.subprocess,'run',side_effect=ps) as called:
            self.assertEqual(pause.check_listener(12345,Path('/fixture/lab')),'Tue Nov 3 10:00:00 2026')
            self.assertEqual(called.call_args.kwargs['env']['LC_ALL'],'C')
            self.assertEqual(called.call_args.kwargs['env']['TZ'],'UTC0')

    def test_browser_parent_reuse_does_not_adopt_foreign_children(self):
        source = ROOT / 'richos/app/ui/tests/lib/browser-reaper.js'
        code = r'''
const fs=require('fs'), vm=require('vm');
let ticks=[], signals=[], exited=false;
const rows=[
 '101 1 original\n201 101 owned\n',
 '101 1 replacement\n202 101 foreign\n201 1 owned\n',
 '201 1 owned\n202 1 foreign\n'
];
const sandbox={require:()=>({spawnSync:()=>({status:0,stdout:rows.shift()||''})}),
 process:{pid:999,kill:(p,s)=>signals.push(p),exit:()=>{exited=true;}},
 setInterval:f=>ticks.push(f), module:{exports:{}}, exports:{}};
vm.runInNewContext(fs.readFileSync(process.argv[1],'utf8')+'\nwatch(101, "original");', sandbox);
for(let n=0;n<2 && !exited;n++) ticks[0]();
if(JSON.stringify(signals)!=='[201]') throw Error(JSON.stringify(signals));
'''
        # load-bound: outer hang guard only; recorded signals decide the verdict.
        result = subprocess.run(['node','-e',code,str(source)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)

if __name__ == '__main__': unittest.main()

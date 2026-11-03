#!/usr/bin/env python3
"""Walk control flow refuses failed steps and failed theme presses without mislabeled captures."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(os.environ.get('G8_ROOT',Path(__file__).resolve().parents[5]))
HERE=ROOT/'richos/app/scripts/testvm'

def function(text,name):
    start=re.search(r'^'+name+r'\(\) \{',text,re.M).start()
    first=text[start:].splitlines()[0]
    if first.rstrip().endswith('}'): return first+'\n'
    return text[start:text.index('\n}',start)+2]+'\n'

class WalkFailures(unittest.TestCase):
    def test_all_six_themes_require_a_successful_press(self):
        for name in ('accounts','round16-panel','spend','use-first','bug-report','feature-request'):
            for works in (False,True):
                with self.subTest(walk=name,works=works),tempfile.TemporaryDirectory() as tmp:
                    root=Path(tmp); ax=root/'ax.sh';ax.write_text('#!/bin/sh\nexit '+('0' if works else '1')+'\n');ax.chmod(0o755)
                    body=function((HERE/f'{name}-walk.sh').read_text(),'theme')
                    script='T='+str(root)+'\nVM=fixture\nmenu() { :; }\nnote() { echo "$*"; }\nsleep() { :; }\nax() { "$T/ax.sh" "$VM" "$@"; }\n'+body+'theme Dark\necho CAPTURE\n'
                    # load-bound: hang guard only; refused presses must prevent the capture marker.
                    result=subprocess.run(['bash'],input=script,capture_output=True,text=True,timeout=30)
                    self.assertEqual(result.returncode==0,works,result.stdout+result.stderr)
                    self.assertEqual('CAPTURE' in result.stdout,works,result.stdout)

    def test_round16_rejects_failed_guest_operations(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);shutil.copy2(HERE/'round16-panel-walk.sh',root/'walk.sh')
            for name in ('ax.sh','shot.sh','guest.sh'):
                path=root/name;path.write_text('#!/bin/sh\nexit 1\n');path.chmod(0o755)
            (root/'relaunch.py').write_text('def relaunch(*a,**kw): raise RuntimeError("fixture refusal")\n')
            bin_dir=root/'bin';bin_dir.mkdir();sleep=bin_dir/'sleep';sleep.write_text('#!/bin/sh\nexit 0\n');sleep.chmod(0o755)
            # load-bound: only catches a hung fixture; status and lack of PNGs establish refusal.
            result=subprocess.run(['bash',str(root/'walk.sh'),'fixture',str(root/'out')],env={**os.environ,'PATH':str(bin_dir)+':'+os.environ['PATH']},capture_output=True,text=True,timeout=30)
            self.assertNotEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertNotIn('[walk] done',result.stdout)

    def test_round16_capture_failure_prevents_completion(self):
        text=(HERE/'round16-panel-walk.sh').read_text()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);shot=root/'shot.sh';shot.write_text('#!/bin/sh\nexit 1\n');shot.chmod(0o755)
            settings=re.search(r'^set -[^\n]+',text,re.M)[0]
            script=settings+'\nT='+str(root)+'\nVM=fixture\nS='+str(root)+'\ntheme() { :; }\n'+function(text,'pair')+'pair example\necho COMPLETE\n'
            # load-bound: outer hang guard; the failed capture must prevent the completion marker.
            result=subprocess.run(['bash'],input=script,capture_output=True,text=True,timeout=30)
            self.assertNotEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertNotIn('COMPLETE',result.stdout)

if __name__=='__main__': unittest.main()

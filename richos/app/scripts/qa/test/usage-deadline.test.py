#!/usr/bin/env python3
"""Quota probes end without waiting for pipe-holding descendants. No provider or guest."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT=Path(os.environ.get('G8_ROOT',Path(__file__).resolve().parents[5]))

class QuotaDeadlines(unittest.TestCase):
    def run_probe(self, guest=False, version_hangs=False, answer=False):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); binary=root/'claude'; child_pid=root/'child.pid'
            binary.write_text(f'''#!{sys.executable}
import json,os,signal,sys,time
if '--version' in sys.argv:
    {'time.sleep(300)' if version_hangs else "print('fixture'); sys.exit(0)"}
for _ in range(2): request=json.loads(sys.stdin.readline())
pid=os.fork()
if pid==0:
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    while True: time.sleep(1)
open({str(child_pid)!r},'w').write(str(pid))
{'print(json.dumps({"type":"control_response","response":{"request_id":request["request_id"],"subtype":"success","response":{"rate_limits_available":True,"rate_limits":{"five_hour":{"utilization":0.1}}}}}),flush=True)' if answer else ''}
# Exit immediately, leaving a child with both output pipes.
sys.exit(0)
'''); binary.chmod(0o755)
            if guest:
                argv=[sys.executable,str(ROOT/'richos/app/scripts/testvm/claude-usage-probe.py'),tmp,tmp,'--claude',str(binary),'--seconds','10' if answer else '0.2']
            else:
                argv=['sh',str(ROOT/'richos/app/scripts/qa/usage-shape.sh'),'--claude',str(binary),'--wait','2']
            p=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
            try:
                # load-bound: outer hang guard; an unanswered probe must terminate, regardless of load.
                stdout,stderr=p.communicate(timeout=15)
                self.assertEqual(p.returncode, 0 if answer else 1 if guest else 2, (stdout,stderr))
                if child_pid.exists():
                    pid=int(child_pid.read_text())
                    end=time.monotonic()+30  # load-bound: wait for death, never assert a speed.
                    while True:
                        row=subprocess.run(['ps','-p',str(pid),'-o','stat='],capture_output=True,text=True).stdout.strip()
                        if not row or row.startswith('Z') or time.monotonic()>=end: break
                        time.sleep(0.05)
                    self.assertTrue(not row or row.startswith('Z'),f'probe left child {pid} running: {row}')
            finally:
                if p.poll() is None: os.killpg(p.pid,signal.SIGKILL); p.wait()
                if child_pid.exists():
                    try: os.kill(int(child_pid.read_text()),signal.SIGKILL)
                    except ProcessLookupError: pass

    def test_usage_shape_bounds_version(self): self.run_probe(version_hangs=True)
    def test_usage_shape_bounds_provider_and_children(self): self.run_probe()
    def test_guest_bounds_pipe_read_and_children(self): self.run_probe(guest=True)
    def test_usage_shape_reads_valid_control_response(self): self.run_probe(answer=True)
    def test_guest_reads_valid_control_response(self): self.run_probe(guest=True,answer=True)

if __name__=='__main__': unittest.main()

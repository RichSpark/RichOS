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

class GuestSound(unittest.TestCase):
    def test_sound_reads_distinguish_absence_from_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = root / 'run' / 'sound-fixture'
            state.mkdir(parents=True)
            (state / 'ip').write_text('192.0.2.1\n')
            (state / 'audio').write_text('off\n')
            ssh = root / 'ssh'
            ssh.write_text('''#!/bin/bash
system_profiler() {
  case "$SOUND_FIXTURE" in
    devices-fail) echo 'fixture device read failed' >&2; return 2 ;;
    absent) return 0 ;;
    passthrough) printf 'Input Channels: 2\\n' ;;
  esac
  printf 'Output Channels: 2\\n'
}
osascript() {
  case "$SOUND_FIXTURE" in
    mute-fail) echo 'fixture mute read failed' >&2; return 2 ;;
    mute-empty) return 0 ;;
  esac
  echo false
}
pgrep() {
  case "$SOUND_FIXTURE" in
    app-fail) echo 'fixture process read failed' >&2; return 2 ;;
    absent) return 1 ;;
  esac
  printf '123\\n456\\n'
}
if [ "$SOUND_FIXTURE" = ssh-fail ]; then
  echo 'fixture SSH unavailable' >&2
  exit 255
fi
eval "${!#}"
''')
            ssh.chmod(0o700)
            env = {**os.environ, 'TESTVM_ROOT': tmp, 'PATH': f"{tmp}:{os.environ['PATH']}"}

            def read(case):
                # load-bound: hang guard for the local SSH fixture, not a sound assertion.
                return subprocess.run(['bash', str(HERE / 'guest-sound.sh'), 'sound-fixture'],
                                      capture_output=True, text=True, timeout=30,
                                      env={**env, 'SOUND_FIXTURE': case})

            for case, diagnostic in (
                ('ssh-fail', 'sound devices'), ('devices-fail', 'sound devices'),
                ('mute-fail', 'output mute'), ('mute-empty', 'output mute'),
                ('app-fail', 'app process'),
            ):
                with self.subTest(case=case):
                    result = read(case)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn(diagnostic, result.stderr)
                    self.assertNotIn('passthrough to this Mac:', result.stdout)
                    self.assertNotIn('app running:', result.stdout)

            for case, status, app in (
                ('absent', 'no (no sound device listed)', 'none'),
                ('output-only', 'no (output only:', '123 456'),
                ('passthrough', 'yes (input and output channels)', '123 456'),
            ):
                with self.subTest(case=case):
                    result = read(case)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(f'passthrough to this Mac: {status}', result.stdout)
                    self.assertIn('output muted: false', result.stdout)
                    self.assertIn(f'app running: {app}', result.stdout)


if __name__=='__main__': unittest.main()

#!/usr/bin/env python3
"""Walk entry points refuse missing steps or failed reads, presses and captures."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

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


class WalkSelection(unittest.TestCase):
    def test_entry_points_refuse_empty_steps_and_dispatch_valid_selections_in_order(self):
        walks = (
            ('video-anywhere', 'seed button record', 'still button'),
            ('home-video', 'seed button voice film space long', 'film button'),
            ('home-standard', 'identity history companies picture swear-on space card enter swear-off', 'fresh swear-on'),
            ('home-fps', 'example companies during change relaunch', 'relaunch change'),
            ('openings', 'first history gallery play pick daily sound save spend sheet', 'missing_pack gallery'),
            ('loro-stages', 'fresh hire grow replay', 'replay grow'),
            ('keep-alive', 'setup talk meter keep idle keepalive unseen after', 'unseen after'),
        )
        for name, default, subset in walks:
            spec = importlib.util.spec_from_file_location(name, HERE / f'{name}-walk.py')
            module = importlib.util.module_from_spec(spec)
            with patch.object(sys, 'path', [str(HERE), *sys.path]):
                spec.loader.exec_module(module)

            cases = [
                (['--steps='], []), (['--steps=,,,'], []),
                (['--steps=,' + ',,'.join(subset.split()) + ','], subset.split()),
                ([], default.split()),
            ]
            for cli, expected in cases:
                with self.subTest(walk=name, cli=cli), tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    out = root / 'out'
                    state = root / 'run' / 'fixture'
                    state.mkdir(parents=True)
                    (state / 'audio').write_text('off\n')
                    events = []

                    class StubWalk:
                        def __init__(self, *args):
                            out.mkdir(exist_ok=True)
                            events.append('start')

                    def step_stub(step):
                        return lambda *_: events.append(step)

                    if name == 'keep-alive':
                        # The walk defines its class in main. Stub only its scenario methods and pull.
                        class StubConversation(StubWalk):
                            def __getattribute__(self, attr):
                                if attr.startswith('step_') or attr == 'pull':
                                    return step_stub(attr.removeprefix('step_'))
                                return super().__getattribute__(attr)

                        conversation = SimpleNamespace(ConversationWalk=StubConversation, adopt_walk=SimpleNamespace(
                            StepFailed=RuntimeError, guest=lambda *_: self.fail('unexpected guest call')))
                        scenario = patch.object(module, 'load', side_effect=[conversation, None])
                    else:
                        StubWalk.sound_mode = lambda _: 'off'
                        for step in set(default.split() + subset.split()):
                            method = 'button_step' if step == 'button' else step.replace('-', '_')
                            setattr(StubWalk, method, step_stub(step))
                        scenario = patch.object(module, 'Walk', StubWalk)

                    args = [str(spec.origin), 'fixture', '--out', str(out), *cli]
                    with scenario, patch.object(sys, 'argv', args), patch.dict(os.environ, TESTVM_ROOT=tmp), \
                            contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as err:
                        if not expected:
                            with self.assertRaises(SystemExit) as stopped:
                                module.main()
                            self.assertEqual(stopped.exception.code, 2)
                            self.assertIn('--steps must select at least one step', err.getvalue())
                            self.assertEqual(events, [])
                            self.assertFalse(out.exists())
                        else:
                            self.assertEqual(module.main(), 0)
                            report = json.loads((out / 'report.json').read_text())
                            steps = report if name == 'keep-alive' else report['steps']
                            self.assertEqual(list(steps), expected)
                            self.assertEqual(events, ['start', *expected, *(['pull'] if name == 'keep-alive' else [])])


if __name__=='__main__': unittest.main()

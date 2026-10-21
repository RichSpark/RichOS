#!/usr/bin/env python3
"""AX shell/deadline contracts against owned stub processes, never a VM."""
import ast
import json
import importlib.util
from unittest.mock import patch
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))


def ax_params(program):
    """The parameter line ax.sh puts in front of ax.py, decoded as the guest decodes it."""
    name, _, literal = program.read_text().splitlines()[0].partition(' = ')
    assert name == 'AX_PARAMS_JSON', name
    return json.loads(ast.literal_eval(literal))


# Run the actual CLI and scratch lifecycle with a controlled subprocess boundary.
# Expiry is an injected event, not a bet that a child starts within one second.
DEADLINE_FIXTURE = r"""
import json, os, runpy, selectors, signal, subprocess, sys, time
from pathlib import Path
from unittest.mock import Mock, patch
fixture=json.loads(os.environ['AX_FIXTURE'])
events=[];clock=[1000.0];owned=[];real_popen=subprocess.Popen;real_killpg=os.killpg

def launch(command, **kwargs):
    events.append({'event':'launch','new_session':kwargs.get('start_new_session')})
    if fixture.get('real_child'):
        child=real_popen(command, **kwargs);owned.append(child)
        receive=child.communicate
    else:
        child=Mock(pid=999999999, returncode=fixture.get('exit',0))
        child.wait.side_effect=lambda: child.returncode
        child.poll.side_effect=lambda: child.returncode
    first=[True];prefix=[b'']
    def communicate(source=None, timeout=None):
        events.append({'event':'communicate','source':source.decode() if source is not None else None,'timeout':timeout})
        if first[0]:
            first[0]=False
            if fixture.get('real_child'):
                # Hang guard only. Timeout classification is injected after READY.
                with selectors.DefaultSelector() as selector:
                    selector.register(child.stdout, selectors.EVENT_READ)
                    if not selector.select(120):
                        raise RuntimeError('owned fixture never emitted READY')
                prefix[0]=child.stdout.readline()
                if not prefix[0].startswith(b'owned_pid='):
                    raise RuntimeError('owned fixture emitted no identity')
                events.append({'event':'ready','pid':child.pid})
            phase=fixture.get('phase')
            if phase:
                path=Path(os.environ['TESTVM_AX_PHASE_FILE'])
                with path.open('a') as stream:
                    stream.write(json.dumps({'phase':'transport','at':clock[0]})+'\n')
                if phase=='guest':
                    Path(os.environ['TESTVM_AX_LOG_FILE']).write_text(json.dumps({'ax_phase':'guest'}))
            clock[0]+=fixture.get('elapsed',1.0)
            if fixture.get('expire'):
                child.returncode=None
                raise subprocess.TimeoutExpired(command,timeout)
        if fixture.get('real_child'):
            out,err=receive(source,timeout=timeout)
            return prefix[0]+out,err
        return fixture.get('stdout','').encode(),b''
    if fixture.get('real_child'):
        child.communicate=communicate
    else:
        child.communicate.side_effect=communicate
    return child

def kill(pid, sig):
    events.append({'event':'kill','pid':pid,'signal':int(sig)})
    if owned:
        assert pid==owned[0].pid, 'refuse to signal an unrelated process'
        real_killpg(pid,sig)
    else:
        assert pid==999999999
        virtual.returncode=-int(sig)

# Retain the virtual child's identity for the simulated kernel signal.
virtual=None
def popen(command, **kwargs):
    global virtual
    virtual=launch(command, **kwargs)
    return virtual

try:
    with patch.object(time,'monotonic',side_effect=lambda:clock[0]), \
         patch.object(subprocess,'Popen',side_effect=popen), \
         patch.object(subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')), \
         patch.object(os,'killpg',side_effect=kill):
        sys.argv=sys.argv[1:]
        runpy.run_path(sys.argv[0],run_name='__main__')
finally:
    for child in owned:
        if child.poll() is None:
            events.append({'event':'fixture_cleanup'})
            real_killpg(child.pid,signal.SIGKILL);child.wait()
    Path(os.environ['AX_TRACE']).write_text(json.dumps(events))
"""


class Contracts(unittest.TestCase):
    def deadline(self, *, host=True, expire=True, phase=None, stdout='', exit=0,
                 elapsed=1.0, real_child=False):
        with tempfile.TemporaryDirectory(prefix='ax-contract-') as directory:
            trace=Path(directory)/'trace.json'
            fixture=dict(expire=expire,phase=phase,stdout=stdout,exit=exit,
                         elapsed=elapsed,real_child=real_child)
            env=dict(os.environ,TMPDIR=directory,HOME=directory,CLAUDE_CONFIG_DIR=directory,
                     AX_FIXTURE=json.dumps(fixture),AX_TRACE=str(trace))
            argv=[sys.executable,'-B','-c',DEADLINE_FIXTURE,str(HERE/'ax-deadline.py')]
            if host:argv+=['--host']
            code='import os,signal;print("owned_pid="+str(os.getpid()),flush=True);signal.pause()'
            # Outer hang guard; all semantic deadlines run on the fixture clock.
            result=subprocess.run(argv+['2',sys.executable,'-c',code],input='input payload',
                                  text=True,capture_output=True,env=env,timeout=300)
            self.assertTrue(trace.exists(),result.stdout+result.stderr)
            events=json.loads(trace.read_text())
            self.assertEqual(sorted(p.name for p in Path(directory).iterdir()),['trace.json'],
                             'supervisor scratch must be removed')
            self.assertTrue(events[0]['new_session'],'supervisor must own its child group')
            self.assertEqual(events[1]['source'],'input payload')
            self.assertEqual(events[1]['timeout'],1.0,'retain the one-second cleanup reserve')
            self.assertNotIn('fixture_cleanup',[e['event'] for e in events],
                             'supervisor, not fixture cleanup, must reap its child')
            return result,events

    def test_preflight_timeout_classification_precedes_partial_output(self):
        r,events=self.deadline(stdout='partial\n')
        self.assertEqual(r.returncode,124)
        self.assertEqual(json.loads(r.stdout.splitlines()[0])['error'],'preflight_deadline')
        self.assertIn('partial',r.stdout)
        self.assertEqual([e['signal'] for e in events if e['event']=='kill'],[9])

    def test_transport_and_guest_are_distinct(self):
        for phase in ('transport','guest'):
            # Large virtual elapsed cost cannot prevent the recorded phase event.
            r,_=self.deadline(phase=phase,elapsed=120.0)
            self.assertEqual(r.returncode,124)
            self.assertEqual(json.loads(r.stdout.splitlines()[0])['error'],phase+'_deadline')

    def test_guest_timeout_and_owned_child_cleanup(self):
        r,events=self.deadline(host=False,real_child=True)
        self.assertEqual(r.returncode,124)
        self.assertEqual(json.loads(r.stdout.splitlines()[0])['error'],'guest_deadline')
        pid=int(next(line.split('=')[1] for line in r.stdout.splitlines() if line.startswith('owned_pid=')))
        names=[e['event'] for e in events]
        self.assertLess(names.index('ready'),names.index('kill'))
        self.assertEqual([e['pid'] for e in events if e['event']=='kill'],[pid])
        with self.assertRaises(ProcessLookupError):os.kill(pid,0)

    def test_nonzero_exit_preserved_and_timing_reported(self):
        r,events=self.deadline(expire=False,stdout='evidence\n',exit=7,elapsed=120.0)
        self.assertEqual(r.returncode,7)
        self.assertEqual(r.stdout.strip(),'evidence')
        self.assertNotIn('kill',[e['event'] for e in events])
        timing=json.loads(r.stderr.splitlines()[-1])
        self.assertTrue(timing['ax_timing'])
        self.assertEqual(timing['total_seconds'],120.0)

    def test_shell_delegates_to_host_supervisor(self):
        with tempfile.TemporaryDirectory(prefix='ax-contract-') as directory:
            root=Path(directory);binary=root/'bin';binary.mkdir();trace=root/'trace'
            stub=binary/'python3'
            stub.write_text('#!/bin/sh\nprintf "%s\\n" "$TESTVM_AX_SUPERVISED" "$@" > "$AX_TRACE"\nexit 23\n')
            stub.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+os.pathsep+os.environ['PATH'],
                     AX_TRACE=str(trace),TESTVM_AX_TIMEOUT='20')
            env.pop('TESTVM_AX_SUPERVISED',None)
            r=subprocess.run(['bash',str(HERE/'ax.sh'),'fixture','find','--json'],
                             env=env,text=True,capture_output=True,timeout=300)
            self.assertEqual(r.returncode,23,r.stdout+r.stderr)
            self.assertEqual(trace.read_text().splitlines(),
                             ['1',str(HERE/'ax-deadline.py'),'--host','20','bash',str(HERE/'ax.sh'),
                              'fixture','find','--json'])

    def test_shell_preserves_structured_error_and_selector_parameters(self):
        with tempfile.TemporaryDirectory(prefix='ax-contract-') as directory:
            root = Path(directory)
            binary = root/'bin';binary.mkdir()
            clock = root/'python-clock.py'
            # Render with the same deadline context as the supervisor, without
            # letting host scheduling determine whether preflight consumed it.
            clock.write_text('''import sys,time
time.monotonic=lambda:1000.0
mode=sys.argv[1]
if mode=='-c':
    code=sys.argv[2];sys.argv=['-c',*sys.argv[3:]]
elif mode=='-':
    code=sys.stdin.read();sys.argv=sys.argv[1:]
else:
    raise SystemExit('unexpected fixture Python invocation')
exec(compile(code,'<ax-shell-fixture>','exec'),{'__name__':'__main__'})
''')
            python = binary/'python3'
            python.write_text('#!/bin/sh\nexec "$AX_REAL_PYTHON" -B "$AX_PYTHON_CLOCK" "$@"\n')
            python.chmod(0o700)
            stub = root/'guest.sh'
            stub.write_text('#!/bin/sh\ncat > "$PROGRAM"\ncat "$FIXTURE"\n')
            stub.chmod(0o700)
            fixture = root/'fixture'
            fixture.write_text('{"meta":true,"nodes":1}\n{"error":"notfound","near_matches":[{"role":"AXCheckBox"}]}\n')
            env = dict(os.environ, TMPDIR=directory, HOME=directory, CLAUDE_CONFIG_DIR=directory,
                       TESTVM_ROOT=str(root/'state'), TESTVM_GUEST_EXEC=str(stub),
                       PROGRAM=str(root/'program'), FIXTURE=str(fixture),
                       TESTVM_AX_SUPERVISED='1',TESTVM_AX_DEADLINE='1020',TESTVM_AX_TIMEOUT='20',
                       PATH=str(binary)+os.pathsep+os.environ['PATH'],
                       AX_REAL_PYTHON=sys.executable,AX_PYTHON_CLOCK=str(clock))
            args = ['bash', str(HERE/'ax.sh'), 'fixture', 'click', '--app', 'Fixture',
                    '--id', 'route-choice', '--expect', 'value=1', '--json']
            # Test rendering independently; CLI supervision is covered above.
            r = subprocess.run(args, capture_output=True, text=True, env=env, timeout=300)
            self.assertEqual(r.returncode, 1, r.stdout+r.stderr)
            self.assertEqual(json.loads(r.stdout.splitlines()[0])['error'], 'notfound')
            params = ax_params(root/'program')
            self.assertEqual(params['id'], 'route-choice')
            self.assertEqual(params['expect'], {'value':'1'})
            self.assertFalse(params['first'])
            self.assertFalse(list(root.glob('testvm-ax-*')))
            legacy = subprocess.run(args[:-3]+['--json'], capture_output=True, text=True, env=env, timeout=300)
            self.assertEqual(legacy.returncode, 1, legacy.stdout+legacy.stderr)
            legacy_params = ax_params(root/'program')
            self.assertIsNone(legacy_params['expect'])
            (root/'program').unlink()
            bad = subprocess.run(args[:-3]+['--expect','unknown=1','--json'],
                                 capture_output=True, text=True, env=env, timeout=300)
            self.assertNotEqual(bad.returncode, 0)
            self.assertFalse((root/'program').exists(), 'invalid expectations must not reach guest')


    def test_consumer_uses_exact_names_and_preserves_json_failure(self):
        spec = importlib.util.spec_from_file_location('delta_walk', HERE/'delta-walk.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        walk = module.Walk.__new__(module.Walk)
        walk.vm = 'fixture'
        success = subprocess.CompletedProcess([], 0, '{"clicked":true}\n', '')
        with patch.object(module.subprocess, 'run', return_value=success) as invoke:
            walk.press('Scenario A')
            argv = invoke.call_args.args[0]
            self.assertNotIn('--first', argv)
            self.assertNotIn('--contains', argv)
            self.assertNotIn('--role', argv)
        failure = subprocess.CompletedProcess([], 1, '{"error":"notfound"}\n', 'unrelated prose')
        with patch.object(module.subprocess, 'run', return_value=failure):
            walk.optional_press('Missing')
        failure.stdout = '{"error":"blocked"}\n'
        with patch.object(module.subprocess, 'run', return_value=failure):
            with self.assertRaises(module.Failure) as caught:
                walk.optional_press('Missing')
            self.assertEqual(caught.exception.status, 'blocked')


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env bash
# run-tests: no-host-screen: walk dependencies are replaced with in-process stand-ins
# run-tests: inputs richos/app/scripts/my-richos-walk.test.sh richos/app/scripts/testvm/my-richos-walk.py
# run-tests: covers richos/app/scripts/testvm/my-richos-walk.py
set -euo pipefail
APP="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 -B - "$APP/scripts/testvm/my-richos-walk.py" <<'PY'
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('my_richos_walk', sys.argv[1])
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

calls = []


class FakeConversationWalk:
    def __init__(self, args):
        calls.append('init')
        self.a, self.vm, self.payload = args, args.vm, '/fixture'
        args.out.mkdir()

    def setup(self):
        calls.append('setup')
        return {'thread': 'fixture-thread'}

    def pull(self):
        calls.append('pull')


def guest(_vm, command, _timeout):
    assert command.startswith('grep -m1'), command
    calls.append('identity')
    return 'this app: built from abcdef123456'


def load(name, _path):
    calls.append(name)
    if name == 'conversation_walk':
        return SimpleNamespace(ConversationWalk=FakeConversationWalk,
                               adopt_walk=SimpleNamespace(StepFailed=RuntimeError, guest=guest))
    if name == 'relaunch':
        return SimpleNamespace()
    raise AssertionError(name)


with tempfile.TemporaryDirectory(prefix='my-richos-walk-test-') as tmp:
    def walk(steps, output):
        calls.clear()
        argv = [str(tool.HERE / 'my-richos-walk.py'), 'offline-fixture', '--expect-sha', 'abcdef123456',
                '--out', str(output), '--steps', steps]
        stderr = io.StringIO()
        with patch.object(tool, 'load', load), patch.object(sys, 'argv', argv), contextlib.redirect_stderr(stderr):
            try:
                code = tool.main()
            except SystemExit as exc:
                code = exc.code
            except AttributeError as exc:
                return None, str(exc)
        return code, stderr.getvalue()

    failures = []
    for i, (steps, diagnostic) in enumerate([
        ('', 'must select at least one step'),
        (',,,', 'must select at least one step'),
        ('identity,unknown', 'unknown step(s): unknown'),
    ]):
        out = Path(tmp) / f'invalid-{i}'
        code, error = walk(steps, out)
        if code != 2 or diagnostic not in error or calls or out.exists():
            failures.append((steps, code, error, calls.copy()))

    out = Path(tmp) / 'valid'
    assert walk('setup,identity', out) == (0, '')
    assert calls == ['conversation_walk', 'relaunch', 'init', 'setup', 'identity', 'pull'], calls
    report = json.loads((out / 'report.json').read_text())
    assert list(report) == ['setup', 'identity'] and all(row['pass'] for row in report.values()), report

assert not failures, failures
print('My RichOS rejects empty and unknown selectors before setup and runs valid steps in order')
PY

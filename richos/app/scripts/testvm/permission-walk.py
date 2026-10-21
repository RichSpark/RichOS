#!/usr/bin/env python3
"""Walk S7 through the installed native app with a deterministic provider child.

Run only as the scenario of run-walk.py. The provider fixture replaces Claude
for this guest launch. Permission policy, Tauri IPC, rendering and native input
remain real. This is not proof that a current Claude model chooses to ask.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
from types import SimpleNamespace
import sys
import time

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('permission_adopt', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adopt)
from relaunch import guest, relaunch
import axwait  # noqa: E402


def visible(rows, role):
    # Hidden WebKit controls can appear as a ten-point accessibility strip.
    return [row for row in rows if row.get('role') == role and row.get('enabled')
            and row.get('w', 0) > 15 and row.get('h', 0) > 15]


def run(a):
    w = adopt.Walk(SimpleNamespace(vm=a.vm, out=a.out, expect_sha=a.expect_sha))
    report = {'provider': 'deterministic stream-json child; no tool executed',
              'source': a.expect_sha, 'checks': [], 'result': 'failed'}
    try:
        report['identity'] = w.identity()
        # This scenario always starts with an empty home. The helper's initial
        # optional-dialog check is instantaneous; wait for the asynchronous
        # first-run surface before letting it decide which control to press.
        w.wait_for('Set it up')
        w.first_run()
        fixture = w.payload + '/permission-provider.py'
        adopt.command([HERE / 'guest.sh', a.vm, '--push', HERE / 'permission-provider.py', fixture], 60)
        guest(a.vm, 'chmod 700 ' + shlex.quote(fixture))
        report['launch'] = relaunch(a.vm, environment={'RICHOS_CLAUDE_BIN': fixture})
        # Subsequent launches show the home screen, which keeps the desk inert.
        # Enter through its native control, as command-walk's relaunch does.
        w.wait_for('Talk to Rich')
        w.press('Talk to Rich', contains=False)
        report['home_door_pressed'] = True
        w.wait_for('Message to Rich', role='AXTextArea')

        def send(text):
            w.type_into(text, '--role', 'AXTextArea', '--title', 'Message to Rich')
            w.press('Send', contains=False)

        def button(title):
            w.wait_for(title)
            rows = w.ax('find', '--title', title, '--role', 'AXButton', '--contains')
            found = visible(rows, 'AXButton')
            if not found:
                raise RuntimeError('permission control is not visibly usable: ' + title)
            return found[0]

        def wait_for_reply(text):
            # WebKit exposes static prose as AXValue, not AXTitle/Description.
            end = time.monotonic() + 30
            while time.monotonic() < end:
                try:
                    rows = w.ax('find', '--value', text, '--role', 'AXStaticText', '--contains', '--first')
                    if any(row.get('role') == 'AXStaticText' and row.get('w', 0) > 0
                           and row.get('h', 0) > 0 for row in rows):
                        return
                except adopt.StepFailed as error:
                    if not axwait.not_yet(error, 'find'):
                        raise
                time.sleep(1)
            raise adopt.StepFailed('reply never appeared within 30 s: ' + text)

        for decision, control in [('allow', 'Allow action'), ('deny', 'Decline')]:
            marker = 'S7_NATIVE_PERMISSION_' + decision.upper()
            # A separate thread gives every check an unambiguous native rail label.
            w.press('New thread in Acme')
            send(marker)
            observed = button(control)
            w.shot('pending-' + decision + '.png')
            draft = 'S7 unsent draft while permission waits'
            w.type_into(draft, '--role', 'AXTextArea', '--title', 'Message to Rich')
            fields = w.ax('find', '--title', 'Message to Rich', '--role', 'AXTextArea')
            if not any(row.get('enabled') and row.get('value') == draft for row in fields):
                raise RuntimeError('composer did not retain input while permission waited')
            report['checks'].append({'name': decision + ': composer usable while pending',
                                     'result': 'passed', 'control': observed})
            # PRD section 5 explicitly preserves the permission turn's wait.
            # Prove the card leaves other native controls usable, without
            # demanding another model turn while that conversation lock is held.
            w.press('Settings', role='AXPopUpButton', contains=False)
            w.wait_for('Use Rich from your phone', role='AXMenuItem')
            w.shot('settings-while-pending-' + decision + '.png')
            adopt.command([HERE / 'ax.sh', a.vm, '--key', '53'])
            button(control)
            fields = w.ax('find', '--title', 'Message to Rich', '--role', 'AXTextArea')
            if not any(row.get('enabled') and row.get('value') == draft for row in fields):
                raise RuntimeError('opening settings lost the pending composer draft')
            report['checks'].append({'name': decision + ': settings usable and draft preserved while pending',
                                     'result': 'passed'})
            report.setdefault('decision_presses', []).append(w.press(control, contains=False))
            w.shot('decision-pressed-' + decision + '.png')
            receipt = 'S7 provider received ' + decision + ' for s7-'
            wait_for_reply(receipt)
            w.shot('answered-' + decision + '.png')
            report['checks'].append({'name': decision + ': exact decision receipt',
                                     'result': 'passed', 'receipt_prefix': receipt})
            # Once its permission is answered, another conversation can reply.
            w.press('New thread in Acme')
            send('S7_INDEPENDENT_' + decision.upper())
            wait_for_reply('S7 independent conversation replied.')
            w.shot('independent-after-decision-' + decision + '.png')
            report['checks'].append({'name': decision + ': new conversation replies after decision',
                                     'result': 'passed'})
        report['result'] = 'passed'
    finally:
        report['finished_at_ms'] = time.time() * 1000
        a.out.mkdir(parents=True, exist_ok=True)
        (a.out / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    args = p.parse_args()
    try:
        sys.exit(run(args))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""account-remove-walk.py — a chat that ran on a second Claude account keeps its history and
answers after that account is removed (the CEO, 2026-11-01: removing one connected Claude account
broke every chat that had run on it).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      account-remove-walk.py --out DIR --expect-sha SHA

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" and its first conversation
  accounts   a second account, Work, in the app's own list (claude-accounts.json) and in use, its
             folder signed in with claude-login.sh (--account, from --host-item: this Mac's own
             Claude sign-in unless another is named, access token only), then a relaunch; Work's
             `projects` is then a link to Account 1's records folder
  chat       a message with a codeword, answered on Work: the turn's record names Work's folder
             (the provider's transcript_path), and the record is in Account 1's records folder
  switch     Account 1's "Use this one now" (offered once Account 1 has a reading; waited for up to
             --reading-wait seconds), then a turn in the same chat: answered in the same
             session, on Account 1, from the record's permanent path in Account 1's records folder
             (never through Work's folder, which goes with Work)
  remove     the Work card's Remove, then its confirmation, in the Claude accounts sheet, as a
             person removes it, while the chat runs on Account 1: Work is gone from the list and
             its folder is gone
  continue   a message asking for the codeword: answered, in the SAME session as the codeword (its
             prompt is in the same session's callbacks, so the chat resumed its record and did not
             start over), and the answer names the codeword

Evidence in --out: report.json, accounts.json, chat.json, switch.json, remove.json, continue.json, screenshots
and the app logs. Exit 0 when every step passes. Every app instance is quit by run-walk.py's
stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('toolchain_ask_walk', HERE / 'toolchain-ask-walk.py')
toolchain_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(toolchain_walk)
StepFailed = toolchain_walk.StepFailed

STEPS = ['identity', 'first-run', 'accounts', 'chat', 'switch', 'remove', 'continue']
REMEMBER = 'Remember this codeword for later in this chat: {code}. Reply with one word: noted.'
RECALL = 'What was the codeword I asked you to remember earlier in this chat? Reply with the codeword only.'
RETURN_KEY = '36'


class AccountRemoveWalk(toolchain_walk.ToolchainAskWalk):
    def __init__(self, a):
        super().__init__(a)
        self.facts.setdefault('code', 'kiwi-' + uuid.uuid4().hex[:6])
        self.work = self.data + '/claude-accounts/2'
        self.records = self.home + '/.claude/projects'
        self.save()

    def key(self, code):
        subprocess.run([str(HERE / 'ax.sh'), self.vm, '--key', code], check=True, capture_output=True, timeout=60)

    def through_home(self, seconds=120):
        """After a relaunch the home screen is up (recognized by OCR, as first-send-walk.py does:
        its live canvas keeps accessibility reads past their deadline); Return leaves it."""
        began = time.monotonic()
        while time.monotonic() - began < seconds:
            self.shot('home.png')
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich', str(self.out / 'home.png'),
                                    '--first', '--quiet'], capture_output=True, text=True, timeout=60)
            if found.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the frame: ' + found.stderr)
            if found.returncode == 0:
                self.key(RETURN_KEY)
                self.wait_for('Send', seconds=60)
                return round(time.monotonic() - began, 1)
            time.sleep(1)
        raise StepFailed('the home screen was not on screen within %g s of the relaunch' % seconds)

    def write(self, name, evidence):
        (self.out / name).write_text(json.dumps(evidence, indent=2) + '\n')

    def accounts_file(self):
        return json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/claude-accounts.json')
                                + ' 2>/dev/null || echo "{}"') or '{}')

    def accounts(self):
        stored = self.accounts_file()
        stored.setdefault('accounts', [{'id': '1', 'label': 'Account 1'}])
        stored['accounts'] = [a for a in stored['accounts'] if a.get('id') != '2']
        stored['accounts'].append({'id': '2', 'label': 'Work', 'folder': self.work})
        stored['inUse'] = '2'
        guest(self.vm, 'mkdir -p ' + shlex.quote(self.work) + ' && printf %s ' + shlex.quote(json.dumps(stored))
              + ' > ' + shlex.quote(self.data + '/claude-accounts.json'))
        # Work's sign-in: this Mac's own, for the folder the app gives Work. CLAUDE_CONFIG_DIR is
        # left out so a renewal (if one is due) renews the item read and no other.
        env = {k: v for k, v in os.environ.items() if k not in ('CLAUDE_CONFIG_DIR', 'CLAUDE_SECURESTORAGE_CONFIG_DIR')}
        signed = subprocess.run([str(HERE / 'claude-login.sh'), 'push', '--account', self.a.host_item,
                                 '--guest-home', self.home, self.vm, self.work],
                                env=env, capture_output=True, text=True, timeout=120)
        line = (signed.stdout.strip().splitlines() or [''])[-1]
        if signed.returncode != 0:
            raise StepFailed('Work could not be signed in: ' + (signed.stderr.strip()[-400:] or line))
        relaunch(self.vm)
        self.through_home()
        link = guest(self.vm, 'readlink ' + shlex.quote(self.work + '/projects') + ' || true').strip()
        evidence = {'accounts_file': self.accounts_file(), 'work_signed_in': line, 'work_projects_link': link}
        self.write('accounts.json', evidence)
        self.shot('accounts.png')
        if link != self.records:
            raise StepFailed("Work's records folder is not a link to Account 1's: readlink says %r" % link)
        return evidence

    def prompt_sessions(self, words):
        """The sessions (evidence folders) whose UserPromptSubmit callback carries WORDS."""
        script = ('import glob,json,os,sys\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl")):\n'
                  '    for l in open(p, errors="replace"):\n'
                  '        if sys.argv[2] not in l: continue\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except ValueError: continue\n'
                  '        if c.get("hook_event_name")=="UserPromptSubmit":\n'
                  '            out.append({"session":os.path.basename(os.path.dirname(p)),"transcript_path":c.get("transcript_path")})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data)
                                + ' ' + shlex.quote(words), 60))

    def answered(self, words, judge):
        sent = self.send(words)
        end = time.monotonic() + self.a.within
        turns = []
        while time.monotonic() < end:
            turns = toolchain_walk.turns_after(self.ledger_rows(), sent[0])
            if any(judge(t['text']) for t in turns):
                break
            time.sleep(5)
        return sent, turns

    def chat(self):
        code = self.facts['code']
        words = REMEMBER.format(code=code)
        sent, turns = self.answered(words, lambda text: 'noted' in text.lower())
        prompts = self.prompt_sessions(code)
        record = guest(self.vm, 'grep -l ' + shlex.quote(code) + ' ' + shlex.quote(self.records)
                       + '/*/*.jsonl 2>/dev/null || true').split()
        evidence = {'code': code, 'sent_between_guest_ms': [round(v) for v in sent], 'prompts': prompts,
                    'records_with_code': record, 'rich_turns_since_send': [t['text'][:400] for t in turns]}
        self.write('chat.json', evidence)
        self.shot('chat.png')
        failures = []
        if not any('noted' in t['text'].lower() for t in turns):
            failures.append('Rich did not answer the codeword message')
        if not prompts:
            failures.append('no session received the codeword message')
        elif not all((p.get('transcript_path') or '').startswith(self.work + '/') for p in prompts):
            failures.append('the codeword turn did not run on Work: %s' % prompts)
        if not record:
            failures.append("the chat's record is not in Account 1's records folder")
        if failures:
            raise StepFailed('; '.join(failures))
        self.facts['session'] = prompts[0]['session']
        self.save()
        return evidence

    def open_sheet(self):
        for _ in range(3):
            self.press('Settings', role='AXPopUpButton')
            time.sleep(3)
            try:
                self.press('Claude accounts', role='AXButton')
            except StepFailed:
                continue
            time.sleep(3)
            if self.present('Close Claude accounts'):
                return
        raise StepFailed('the Claude accounts sheet did not open')

    def switch(self):
        """Account 1 chosen with its own "Use this one now", then a turn in the same chat: it runs on
        Account 1, resumed from the record's permanent path in Account 1's records folder, never
        through Work's folder (second review rv-20261101T183042Z, P1: removing Work then broke the
        live chat's next write)."""
        # The button is offered only once Account 1 has a reading with room (accounts.js "use"),
        # which comes with the quota service's next check (every 5 minutes). Not a relaunch: a
        # relaunch opens the chat on a fresh lease and would not resume this record at all.
        offered = False
        end = time.monotonic() + self.a.reading_wait
        while time.monotonic() < end and not offered:
            self.open_sheet()
            offered = self.present('Use Account 1 now')
            if not offered:
                self.press('Close Claude accounts')
                time.sleep(20)
        self.shot('switch-sheet.png')
        if not offered:
            raise StepFailed('the sheet never offered "Use this one now" on Account 1 within %d s (no reading of it)'
                             % self.a.reading_wait)
        self.press('Use Account 1 now', contains=False)
        end = time.monotonic() + 30
        while time.monotonic() < end and self.accounts_file().get('inUse') != '1':
            time.sleep(2)
        self.press('Close Claude accounts')
        switched_by = 'Use this one now'
        if self.accounts_file().get('inUse') != '1':
            raise StepFailed('Account 1 was not put in use')
        words = 'Here is one more thing to note, the word {w}. Reply with one word: noted.'.format(w='harbor')
        sent, turns = self.answered(words, lambda text: 'noted' in text.lower())
        prompts = self.prompt_sessions('one more thing to note, the word harbor')
        evidence = {'switched_by': switched_by, 'sent_between_guest_ms': [round(v) for v in sent], 'prompts': prompts,
                    'rich_turns_since_send': [t['text'][:400] for t in turns]}
        self.write('switch.json', evidence)
        self.shot('switch.png')
        failures = []
        if not any('noted' in t['text'].lower() for t in turns):
            failures.append('Rich did not answer on Account 1')
        if not any(p['session'] == self.facts.get('session') for p in prompts):
            failures.append('the turn on Account 1 ran in another session %s, not %s'
                            % ([p['session'] for p in prompts], self.facts.get('session')))
        elif not all((p.get('transcript_path') or '').startswith(self.records + '/') for p in prompts):
            failures.append("the chat resumed on Account 1 writes through another folder than Account 1's records: %s" % prompts)
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence

    def remove(self):
        self.open_sheet()
        self.shot('remove-sheet.png')
        self.press('Remove Work', contains=False)
        time.sleep(2)
        self.shot('remove-confirm.png')
        # The confirmation takes the focus (accounts.js "remove"); Return presses it, as a person would.
        self.key(RETURN_KEY)
        end = time.monotonic() + 30
        stored = {}
        while time.monotonic() < end:
            stored = self.accounts_file()
            if not any(a.get('id') == '2' for a in stored.get('accounts', [])):
                break
            time.sleep(2)
        self.shot('removed.png')
        folder = guest(self.vm, 'ls -la ' + shlex.quote(self.work) + ' 2>&1 || true').strip()
        evidence = {'accounts_file': stored, 'work_folder': folder}
        self.write('remove.json', evidence)
        if self.present('Close Claude accounts'):
            self.press('Close Claude accounts')
        failures = []
        if any(a.get('id') == '2' for a in stored.get('accounts', [])):
            failures.append('Work is still in the account list')
        if 'No such file' not in folder:
            failures.append("Work's folder is still there: " + folder[:300])
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence

    def continue_(self):
        code = self.facts['code']
        sent, turns = self.answered(RECALL, lambda text: code in text)
        prompts = self.prompt_sessions('What was the codeword I asked you to remember')
        record = guest(self.vm, 'grep -c ' + shlex.quote(code) + ' ' + shlex.quote(self.records)
                       + '/*/' + shlex.quote(self.facts.get('session', 'none')) + '.jsonl 2>/dev/null || true').strip()
        evidence = {'sent_between_guest_ms': [round(v) for v in sent], 'prompts': prompts,
                    'codeword_session': self.facts.get('session'), 'codeword_lines_in_its_record': record,
                    'rich_turns_since_send': [t['text'][:400] for t in turns]}
        self.write('continue.json', evidence)
        self.shot('continue.png')
        failures = []
        if not any(code in t['text'] for t in turns):
            failures.append('the answer does not name the codeword %s' % code)
        if not prompts:
            failures.append('no session received the follow-up')
        elif not any(p['session'] == self.facts.get('session') for p in prompts):
            failures.append('the follow-up ran in another session %s, not the codeword\'s %s (the chat started over)'
                            % ([p['session'] for p in prompts], self.facts.get('session')))
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=240, help='seconds for each of Rich\'s answers')
    p.add_argument('--reading-wait', type=int, default=420,
                   help='seconds to wait for Account 1\'s reading, which offers its "Use this one now"')
    p.add_argument('--host-item', default='Claude Code-credentials',
                   help="this Mac's keychain item Work is signed in from (default: this Mac's own sign-in)")
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = AccountRemoveWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            name = {'continue': 'continue_'}.get(step, step.replace('-', '_'))
            row['evidence'] = getattr(walk, name)()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

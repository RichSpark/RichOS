#!/usr/bin/env python3
"""handoff-real-walk.py — the weekly-switch handoff on the real app, real `claude`, his two real
accounts, in one guest run (richos-hq docs/plans/2026-10-07-weekly-switch-handoff.md §4, round 2).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone. His two sign-ins reach the guest through run.sh (Home, and Work from the app's Account 2
folder; access tokens only). The fixture home must carry the two Opus teammates first:

  handoff-real-walk.py --seed-team FIXTURE_HOME
  TESTVM_APP_MODEL=opus run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      handoff-real-walk.py --out DIR --expect-sha SHA --corpus CORPUS.tar [--within SECONDS]

run-walk.py passes the owned VM name as the first argument.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  team       the seeded Opus teammates (scribe, checker) are in the guest's <app data>/team/
  corpus     the Acme repository: --corpus (a tar of lib/*.py) unpacked and committed
  first-run  adopt-walk.py's: company "Acme" with that folder
  connect    command-walk.py's: the folder connected as a repository
  accounts   Account 2 "Work" written into the app's claude-accounts.json at the folder run.sh
             signed in (<home>/claude-accounts/2), the app relaunched, the quota panel opened and
             both accounts' weekly readings read from the app's own view (claude-quota.json)
  watch      handoff-watch.py started in the guest (every record the handoff writes, timed)
  cutoff     relaunched with RICHOS_TEST_WEEKLY_CUTOFF=1:<Home's weekly now + 1>; the view's
             weekly point for Home must say so
  job        JOB, typed into the composer at once: one teammate reads every file and summarizes
             it into MODULES.md, committing after every five files (Home moved 2 points in 24 min
             on 2026-10-07 09:21-09:45Z, so the order can come early); another reviews; then it
             is landed. Refused when Home already reached the cut-off (run 9).
  work-reading  the relaunched app reads Work (the quota panel's Refresh every 45 s, only until
             it has; counted): the app switches only to an account it has read
  observe    nothing is pressed (an Approve the app asks for is pressed and COUNTED: the pass
             line "with no step by anyone" then fails). Watched until the job closes or --within.
             Then the verdict below, from what was saved.

THE PASS LIST (plan §4, verbatim), each read from the saved records:
  1 The order arrives at the cut-off.
  2 The worker's branch holds its work commits, then a `RichOS handoff:` commit.
  3 A successor on Work starts at that commit, with the handoff text in its brief.
  4 The successor's diff does not rewrite summaries already committed (nothing redone).
  5 The job is reviewed, landed and closed with no step by anyone.
  6 Record how long the handoff took (sets `H`).
  + his 3-5 minute expectation: each ordered teammate's time from the order to its end; over 5
    minutes is a finding.

In --out: report.json (steps), verdict.json (the pass list with evidence), handoff-watch.jsonl,
acme-log.txt, MODULES.md, transcripts/{home,work}/ (both accounts' Claude transcripts, for the
token count), screenshots, the app's logs. PRIVATE: it holds his accounts' transcripts; keep it
outside every repository. Exit 0 only when every step and every pass line passed.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN
command = command_walk.command

STEPS = ['identity', 'team', 'corpus', 'first-run', 'connect', 'accounts', 'watch', 'cutoff', 'job', 'work-reading', 'observe']
HANDOFF_PREFIX = 'RichOS handoff:'
LEFTOVERS = 'RichOS: work in progress saved at the account switch'
HANDOFF_BRIEF = 'Handoff from the previous teammate:'
# His 3-5 minutes (2026-10-07): over this is a finding.
HANDOFF_LIMIT_MS = 5 * 60_000
WORK_FOLDER = 'claude-accounts/2'
# Two teammates of the user's own, on Opus (his word 2026-10-07: "run all teammates on Opus").
TEAM = {
    'scribe': ('---\nname: scribe\ndescription: Careful reader who reads source files in full and writes a plain '
               'one-paragraph summary of each. Use for read-every-file and summarize jobs.\nmodel: opus\n'
               'tools: Read, Glob, Grep, Bash, Write, Edit\n---\n\nYou are Scribe. You read each file you are '
               'given in full before you summarize it, in the order you are given, and you commit exactly as '
               'the brief asks.\n'),
    'checker': ('---\nname: checker\ndescription: Reviewer who checks a summary document against the files it '
                'summarizes.\nmodel: opus\ntools: Read, Glob, Grep, Bash\n---\n\nYou are Checker. You review '
                'what you are asked to review against the files themselves and report plainly.\n'),
}
# The plan's example writes SUMMARY.md; Claude Code's Write refuses a SUBAGENT any file named
# /^(REPORT|SUMMARY|FINDINGS|ANALYSIS).*\.md$/i ("Subagents should return findings as text, not write
# report files", errorCode 5, read out of 2.1.292), and every RichOS teammate is a subagent (run 8:
# scribe stopped at file 5 and the job waited on his answer). So the file is MODULES.md.
JOB = ('Please have scribe do this job in the Acme repository, and have checker review it before it is landed. '
       'Read every .py file in the lib folder, in alphabetical order, one file at a time and each one in full, '
       'and write a one-paragraph summary of each into MODULES.md at the top of the repository: one section per '
       'file, headed "## lib/<file name>", followed by the paragraph. Commit after every five files. When every '
       'file is summarized, land it.')


class HandoffWalk(command_walk.CommandWalk):
    # An accessibility read that misses its deadline (ax.sh exit 124) on a busy host is "not
    # read", not "not there" and not a failed walk (runs 1 and 7: a find for the composer and for
    # "Start the questions" ended the walk). A read with no answer is asked again up to three
    # times (axwait.ask_again), and present() then reads it as "not seen yet"; a press, whose
    # effect is unknown after a deadline, once, only after a find says it is still there.
    def ax(self, mode, *args, app=None, timeout=40):
        for attempt in range(3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 2:
                    raise
                time.sleep(2)

    def press(self, title, role='AXButton', app=None, contains=True):
        try:
            return super().press(title, role, app, contains)
        except StepFailed as exc:
            if not axwait.no_answer(exc):
                raise
            time.sleep(3)
            if not self.present(title, role, app):
                return []
            return super().press(title, role, app, contains)

    def team(self):
        found = {}
        for name, body in TEAM.items():
            path = self.data + '/team/' + name + '.md'
            if guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true') != body.strip():
                raise StepFailed(f'{path} is not the seeded teammate: run handoff-real-walk.py --seed-team '
                                 'FIXTURE_HOME before run-walk.py boots (the leases open at launch)')
            found[name] = path
        return found

    def corpus(self):
        remote = self.payload + '/corpus.tar'
        command([HERE / 'guest.sh', self.vm, '--push', self.a.corpus, remote], 120)
        c = shlex.quote(self.company)
        guest(self.vm, f'mkdir -p {c} && tar -C {c} -xf {shlex.quote(remote)} && cd {c} && '
                       'printf "Acme notes\\n" > README.md && git init -q && git add -A && '
                       'git -c user.name=QA -c user.email=qa@example.invalid commit -q -m "Acme: the lib folder" && '
                       'git rev-parse HEAD', 120)
        base = guest(self.vm, 'git -C ' + c + ' rev-parse HEAD').strip()
        files = int(guest(self.vm, 'ls ' + c + '/lib/*.py | wc -l').strip())
        self.facts.update(base=base, files=files)
        self.save()
        return {'base': base, 'files': files}

    def quota(self):
        value = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/engine-state/claude-quota.json')
                                 + ' 2>/dev/null || echo null'))
        return value or {}

    def weekly(self, quota, account):
        for a in quota.get('accounts') or []:
            if a.get('id') == account:
                return next((w.get('usedPercent') for w in a.get('windows') or [] if w.get('id') == 'seven_day'), None)
        return None

    def thread_back(self, seconds=180):
        """After a relaunch: through the home screen's door into the thread, until the composer is back."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            # A freshly relaunched app can hold an accessibility read past its deadline (exit 124,
            # walk-9f611a4b22a7): that is "not drawn yet", not a failure, so the read is retried.
            # The door and the thread are pressed again on every pass until the composer is there
            # (run 5: a press that missed once was never tried again).
            try:
                if self.present('Message to Rich', role='AXTextArea'):
                    return True
                if self.present('Talk to Rich'):
                    self.press('Talk to Rich')
                    time.sleep(2)
                if self.present(self.facts.get('thread_title') or 'Running'):
                    self.press(self.facts.get('thread_title') or 'Running')
            except StepFailed:
                pass
            time.sleep(3)
        try:
            self.shot('relaunch-no-composer-%d.png' % int(time.time()))
            (self.out / 'relaunch-no-composer-tree.txt').write_text(command([HERE / 'ax.sh', self.vm, 'tree', '--depth', '6'], 60))
        except StepFailed:
            pass
        raise StepFailed(f'the composer was not back within {seconds} s of the relaunch')

    def panel(self):
        """The quota panel (Settings > Claude Code quota, ui id set-quota-open), opened and refreshed.
        Best effort: the readings are taken from the app's own claude-quota.json, which the app
        keeps current with or without the panel; the panel is for the screenshot."""
        # The row is an AXMenuItem titled "Claude Code quota ..." (fakewalk1 run7's log: "pressed
        # AXMenuItem title='Claude Code quota Home 50% weekly · next Work'"); by DOM id it was
        # refused in runs 2-6 with the menu open, so the title is tried first. Every refusal is
        # kept in facts.json, so a panel that does not open says why.
        def attempt(*ways):
            for way in ways:
                try:
                    way()
                    return True
                except StepFailed as exc:
                    self.facts.setdefault('panel_refusals', []).append(str(exc)[:400])
            return False
        for _ in range(3):
            if not self.present('Claude Code quota', role='AXMenuItem'):
                self.close_panel()
                attempt(lambda: self.press('Settings', role='AXPopUpButton'))
                time.sleep(2)
            if attempt(lambda: self.press('Claude Code quota', role='AXMenuItem'),
                       lambda: self.ax('click', '--id', 'set-quota-open')):
                time.sleep(3)
                if attempt(lambda: self.ax('click', '--id', 'quota-refresh'),
                           lambda: self.press('Refresh'), lambda: self.press('Ask Claude Code')):
                    self.save()
                    return True
        self.save()
        return False

    def refresh_panel(self):
        """One press of the open panel's Refresh (its words: Refresh, or Ask Claude Code)."""
        for way in (lambda: self.ax('click', '--id', 'quota-refresh'), lambda: self.press('Refresh'),
                    lambda: self.press('Ask Claude Code')):
            try:
                way()
                return True
            except StepFailed:
                continue
        return False

    def close_panel(self):
        try:
            command([HERE / 'ax.sh', self.vm, '--key', '53'])
        except StepFailed:
            pass
        time.sleep(2)

    def relaunch_app(self, environment):
        env = {'ANTHROPIC_MODEL': 'opus', **environment}
        at = self.clock()
        launched = relaunch(self.vm, environment=env)
        self.facts.setdefault('relaunches', []).append({'env': env, 'at': at, 'log': launched.get('log'),
                                                        'pid': launched.get('pid')})
        self.save()
        time.sleep(5)
        self.thread_back()
        return launched

    def accounts(self):
        folder = self.home + '/' + WORK_FOLDER
        if guest(self.vm, 'test -s ' + shlex.quote(folder + '/.credentials.json') + ' && echo yes || echo no').strip() != 'yes':
            raise StepFailed(f'no Work sign-in at {folder} (run.sh pushes it only when this Mac has a Work item)')
        stored = {'accounts': [{'id': '1', 'label': 'Home'}, {'id': '2', 'label': 'Work', 'folder': folder}],
                  'inUse': '1', 'changes': [{'at': int(self.clock()), 'kind': 'added', 'id': '2', 'label': 'Work'}]}
        path = self.data + '/claude-accounts.json'
        guest(self.vm, 'cat > ' + shlex.quote(path) + ' <<\'EOF\'\n' + json.dumps(stored) + '\nEOF')
        self.relaunch_app({})
        written = json.loads(guest(self.vm, 'cat ' + shlex.quote(path)))
        if not any(a.get('folder') == folder for a in written.get('accounts', [])):
            raise StepFailed('the app does not keep Work at ' + folder + ': ' + json.dumps(written))
        # THE USAGE ENDPOINT ANSWERS ABOUT ONCE IN FOUR MINUTES PER SIGN-IN, AND THIS MAC ASKS TOO.
        # Measured (probe2, run-walk.py --no-app, no app at all): Home answered at 1 s and 245 s
        # and 429 at 11, 31, 61 and 121 s; Claude Code then answers get_usage with
        # `rate_limits: null` ("429 remembered for this bearer"). The guest holds this Mac's own
        # access tokens, and this Mac's RichOS app and tooling ask with them on their own
        # schedules, so the guest app's five-minute check of Work can land in the 429 window
        # every time (runs 2-4: Work never read in 7 minutes) and the app cannot switch to an
        # account it never read (quota.rs readings). The quota panel's Refresh asks at once, so
        # it is pressed every 45 s, only until both accounts have a reading. The panel is the
        # technical view's (settings-button.js), so that is turned on first.
        self.technical_view()
        opened = self.panel()
        self.facts['refresh_presses'] = 0
        # Only Home's reading is needed here (the cut-off). Work is read in the instance that runs
        # the job (`work-reading`); a reading here is kept as evidence that the sign-in answers.
        quota, home, work = self.read_both(90, opened)
        self.shot('1-quota-panel.png')
        (self.out / 'quota-at-start.json').write_text(json.dumps(quota, indent=2) + '\n')
        self.close_panel()
        if home is None:
            self.diagnose(folder, 'failed')
            raise StepFailed(f'the app has no weekly reading for Home: '
                             + json.dumps([{k: a.get(k) for k in ('id', 'label', 'message')} for a in quota.get('accounts') or []]))
        self.facts.update(home_weekly_at_start=home, work_weekly_at_start=work)
        self.save()
        return {'home_weekly': home, 'work_weekly': work, 'work_folder': folder}

    def probe(self):
        """Not in the default steps: run alone under `run-walk.py --no-app` to ask, with no app at
        all, whether each pushed sign-in answers get_usage with figures (Claude Code's own debug
        lines about the usage fetch included)."""
        remote = self.payload + '/claude-usage-probe.py'
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'claude-usage-probe.py', remote], 60)
        found = {}
        began = time.monotonic()
        # --probe-gaps: when, in seconds from the first, each pair of asks is made (the usage
        # endpoint answers 429 to a second ask on the same sign-in a second later: probe1).
        for n, at in enumerate(float(g) for g in self.a.probe_gaps.split(',')):
            time.sleep(max(0, began + at - time.monotonic()))
            for label, folder in (('home', self.home + '/.claude'), ('work', self.home + '/' + WORK_FOLDER)):
                text = guest(self.vm, 'python3 ' + shlex.quote(remote) + ' ' + shlex.quote(folder) + ' ' + shlex.quote(self.home)
                             + ' --debug-file /tmp/probe-' + label + '-' + str(n) + '.txt || true', 120)
                row = json.loads(text.strip().splitlines()[-1])
                row['at_s'] = round(time.monotonic() - began, 1)
                found[f'{label}-{n}'] = row
        (self.out / 'probe.json').write_text(json.dumps(found, indent=2) + '\n')
        return {k: {'answered': v['answered'], 'rate_limits_null': '"rate_limits":null' in (v.get('answer') or '')}
                for k, v in found.items()}

    def read_both(self, seconds, opened):
        """Both accounts' weekly readings from the app's own view, waited for at most `seconds`."""
        end, quota, home, work, pressed = time.monotonic() + seconds, {}, None, None, 0.0
        while time.monotonic() < end:
            quota = self.quota()
            home, work = self.weekly(quota, '1'), self.weekly(quota, '2')
            if home is not None and work is not None:
                break
            if time.monotonic() - pressed >= 45:
                pressed = time.monotonic()
                if (opened and self.refresh_panel()) or self.panel():
                    opened = True
                    self.facts['refresh_presses'] = self.facts.get('refresh_presses', 0) + 1
                else:
                    opened = False
                self.save()
            time.sleep(5)
        return quota, home, work

    def technical_view(self):
        """Settings > Technical view > Turn it on (fakewalk1's handoff-walk.sh: the toggle's place)."""
        try:
            self.press('Settings', role='AXPopUpButton')
            time.sleep(2)
            command([HERE / 'ax.sh', self.vm, 'click', '--at', '1492,284'])
            time.sleep(2)
            self.press('Turn it on')
            time.sleep(2)
        except StepFailed as exc:
            self.facts['technical_view'] = 'not confirmed: ' + str(exc)[:300]
            self.save()

    def diagnose(self, folder, name='failed'):
        """Why an account has no reading, asked in the guest before it is deleted: the app's claude
        children and any keychain dialog, then the same control-only get_usage the app asks, under
        each account's folder (usage figures only; no credential is read or printed)."""
        probe = self.payload + '/claude-usage-probe.py'
        text = []
        try:
            command([HERE / 'guest.sh', self.vm, '--push', HERE / 'claude-usage-probe.py', probe], 60)
        except StepFailed as exc:
            text.append('== the probe could not be pushed\n' + str(exc)[:1000])
        for title, cmd in (
                ('claude processes', 'ps -axww -o pid,etime,args | grep -i "[c]laude" | cut -c1-400'),
                ('keychain dialog', 'pgrep -fl SecurityAgent || echo none'),
                ('work folder', 'ls -la ' + shlex.quote(folder)),
                ('get_usage under Work', 'python3 ' + shlex.quote(probe) + ' ' + shlex.quote(folder) + ' ' + shlex.quote(self.home) + ' || true'),
                ('get_usage under Home', 'python3 ' + shlex.quote(probe) + ' ' + shlex.quote(self.home + '/.claude') + ' '
                 + shlex.quote(self.home) + ' || true')):
            try:
                text.append('== ' + title + '\n' + guest(self.vm, cmd, 90))
            except (RuntimeError, subprocess.TimeoutExpired) as exc:
                text.append('== ' + title + '\nnot answered: ' + str(exc)[:1000])
        (self.out / ('work-diagnosis-' + name + '.txt')).write_text('\n'.join(text) + '\n')
        try:
            self.shot('1b-diagnosis-' + name + '.png')
        except StepFailed:
            pass

    def cutoff(self):
        # Home's reading as the app shows it NOW, right before the relaunch that sets the cut-off
        # (Home moved a point in about 12 minutes on 2026-10-07; the one from `accounts` is minutes old).
        home = self.weekly(self.quota(), '1')
        if home is None:
            home = self.facts.get('home_weekly_at_start')
        if home is None:
            raise StepFailed('accounts must have run (no Home reading on record)')
        cut = int(home) + 1
        self.facts.update(cutoff=cut, home_weekly_at_cutoff=home)
        self.save()
        self.relaunch_app({'RICHOS_TEST_WEEKLY_CUTOFF': '1:%d' % cut})
        end, quota = time.monotonic() + 240, {}
        while time.monotonic() < end:
            quota = self.quota()
            point = (quota.get('actAt') or {}).get('seven_day')
            if point is not None and quota.get('checkedAt', 0) > self.facts['relaunches'][-1].get('at', 0) and point <= cut:
                break
            time.sleep(10)
        point = (quota.get('actAt') or {}).get('seven_day')
        (self.out / 'quota-after-cutoff.json').write_text(json.dumps(quota, indent=2) + '\n')
        if point is None or point > cut:
            raise StepFailed(f'the app\'s weekly point for Home is {point}, not the cut-off {cut}')
        return {'cutoff': '1:%d' % cut, 'weekly_point_in_view': point, 'leaving': quota.get('leaving')}

    def work_reading(self):
        """THE RELAUNCH FORGETS WORK'S READING (run 8: Work null in the relaunched app), and the app
        switches only to an account it has read (quota.rs readings). Reading Work BEFORE the job
        cost the job its start (run 9: four presses, ~4 min, and Home reached the cut-off as the
        job was sent, so it ran on Work from the first turn). So the job goes out first and this
        instance reads Work while the job's first files are read, with the quota panel's Refresh,
        every 45 s, only until Work has a reading; the presses are counted, and none is made
        after that."""
        before = self.facts.get('refresh_presses', 0)
        opened = self.panel()
        quota, home_now, work = self.read_both(900, opened)
        self.close_panel()
        (self.out / 'quota-work-read.json').write_text(json.dumps(quota, indent=2) + '\n')
        self.facts['refresh_presses_during_job'] = self.facts.get('refresh_presses', 0) - before
        self.facts['work_read_at_guest_ms'] = round(self.clock()) if work is not None else None
        self.save()
        if work is None:
            self.diagnose(self.home + '/' + WORK_FOLDER, 'work-reading')
            raise StepFailed('the relaunched app never read Work in 15 minutes, so it cannot switch to it')
        return {'home_now': home_now, 'work': work, 'presses': self.facts['refresh_presses_during_job'],
                'leaving': quota.get('leaving')}

    def watch(self):
        remote = self.payload + '/handoff-watch.py'
        self.facts['watch_out'] = self.payload + '/handoff-watch.jsonl'
        self.save()
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'handoff-watch.py', remote], 60)
        guest(self.vm, 'nohup python3 {s} {d} {o} --seconds {n} >/dev/null 2>&1 &'.format(
            s=shlex.quote(remote), d=shlex.quote(self.data), o=shlex.quote(self.facts['watch_out']),
            n=int(self.a.within + 3600)))
        time.sleep(3)
        if 'watch-start' not in guest(self.vm, 'head -1 ' + shlex.quote(self.facts['watch_out'])):
            raise StepFailed('the watcher did not start in the guest')
        return {'watching': self.data}

    def job(self):
        quota = self.quota()
        if '1' in (quota.get('leaving') or []):
            raise StepFailed('Home reached the cut-off (%s of %s) before the job was sent; run again'
                             % (self.weekly(quota, '1'), self.facts.get('cutoff')))
        sent = self.send(JOB)
        self.facts['sent_ms'] = sent
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in sent]}

    def markers(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data) + '/engine-state/handoffs/*.json 2>/dev/null || true')
        out = []
        for chunk in text.replace('}{', '}\n{').splitlines():
            try:
                out.append(json.loads(chunk))
            except ValueError:
                pass
        return out

    def observe(self):
        sent = self.facts.get('sent_ms')
        if not sent:
            raise StepFailed('job must have run (no send time on record)')
        end = time.monotonic() + self.a.within
        presses, record, shots, last_note, misses, left_at = [], None, set(), 0, 0, None
        while time.monotonic() < end:
            # One read or press that misses its deadline in a run of hours is not the round's
            # verdict: it is recorded and the next pass tries again; ten in a row end the watch.
            try:
                record = self.ours(sent) or record
                if record and record.get('state') not in OPEN:
                    break
                if record and len(presses) < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                    presses.append({'guest_ms': round(self.clock()), 'state': record.get('state')})
                    self.facts['approvals'] = presses
                    self.save()
                markers = self.markers()
                if markers and 'order' not in shots:
                    shots.add('order')
                    self.shot('3-at-the-order.png')
                if markers and any(m.get('continued_at') for m in markers) and 'continued' not in shots:
                    shots.add('continued')
                    time.sleep(20)
                    self.shot('4-successor.png')
                if not markers and '1' in (self.quota().get('leaving') or []):
                    # Home is at its point and no helper on it was ordered: either the order is
                    # late, or nothing ran on Home when it got there (run 9: the job started on
                    # Work). Ten minutes of that ends the watch; the verdict says which.
                    left_at = left_at or time.monotonic()
                    if time.monotonic() - left_at > 600:
                        self.facts['ended_early'] = 'Home was leaving for 10 minutes and no helper was ordered'
                        self.save()
                        break
                if time.monotonic() - last_note > 300:
                    last_note = time.monotonic()
                    quota = self.quota()
                    print(json.dumps({'guest_ms': round(self.clock()), 'state': (record or {}).get('state'),
                                      'home': self.weekly(quota, '1'), 'work': self.weekly(quota, '2'),
                                      'leaving': quota.get('leaving'), 'markers': len(markers)}), flush=True)
                misses = 0
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError) as exc:
                misses += 1
                self.facts.setdefault('observe_misses', []).append(str(exc)[:500])
                self.save()
                if misses >= 10:
                    break
            time.sleep(20)
        if record:
            record = self.settled_read(record)
        try:
            self.shot('5-end.png')
        except StepFailed:
            pass
        self.facts['approvals'] = presses
        self.save()
        evidence = self.collect(record)
        verdict = judge(evidence)
        (self.out / 'verdict.json').write_text(json.dumps(verdict, indent=2) + '\n')
        failed = [line for line in verdict['lines'] if line['outcome'] != 'PASS']
        if failed:
            raise StepFailed('; '.join(f"{line['line']}: {line['detail']}" for line in failed))
        return {'lines': [{'line': l['line'], 'outcome': l['outcome']} for l in verdict['lines']],
                'handoff_ms': verdict['handoff_ms']}

    def collect_now(self):
        """Not in the default steps: `--steps collect-now` against a run's own live guest (its VM
        name and its --out) saves the evidence and the verdict without waiting for the job to close,
        for a job that waits on something the walk will not do (run 10: a seventh Approve after the
        walk's six). Never presses anything."""
        record = self.ours(self.facts['sent_ms'])
        try:
            self.shot('5-collected.png')
        except StepFailed:
            pass
        verdict = judge(self.collect(record))
        (self.out / 'verdict.json').write_text(json.dumps(verdict, indent=2) + '\n')
        return {'lines': [{'line': l['line'], 'outcome': l['outcome'], 'detail': l['detail']} for l in verdict['lines']]}

    def collect(self, record):
        """Everything the verdict reads, saved into --out first, so it can be judged again offline."""
        watch = guest(self.vm, 'cat ' + shlex.quote(self.facts['watch_out']), 180)
        (self.out / 'handoff-watch.jsonl').write_text(watch + '\n')
        rows = [json.loads(line) for line in watch.splitlines() if line.startswith('{')]
        c = shlex.quote(self.company)
        acme = guest(self.vm, 'git -C ' + c + ' log --all --format=%H%x09%P%x09%ct%x09%s', 60)
        (self.out / 'acme-log.txt').write_text(acme + '\n')
        summary = guest(self.vm, 'git -C ' + c + ' show HEAD:MODULES.md 2>/dev/null || true', 60)
        (self.out / 'MODULES.md').write_text(summary + '\n')
        head = guest(self.vm, 'git -C ' + c + ' rev-parse HEAD', 60).strip()
        handoffs = [x.split('\t') for x in acme.splitlines() if x.count('\t') >= 3 and x.split('\t')[3].startswith(HANDOFF_PREFIX)]
        diffs = {}
        for sha, *_ in handoffs:
            # Against the newest branch that holds the handoff commit: the landed main when the job
            # landed, the successor's own branch when it has not (run 10 stopped before its land).
            tip = guest(self.vm, 'git -C ' + c + ' for-each-ref --contains ' + shlex.quote(sha)
                        + ' --sort=-committerdate --format=%\\(objectname\\) refs/heads | head -1', 60).strip() or head
            diffs[sha + ':tip'] = tip
            diffs[sha] = guest(self.vm, 'git -C ' + c + ' diff --numstat ' + shlex.quote(sha) + ' ' + shlex.quote(tip)
                               + ' -- MODULES.md 2>&1 || true', 60).strip()
            diffs[sha + ':tip-summary'] = guest(self.vm, 'git -C ' + c + ' show ' + shlex.quote(tip)
                                                + ':MODULES.md 2>/dev/null || true', 60)
            diffs[sha + ':summary'] = guest(self.vm, 'git -C ' + c + ' show ' + shlex.quote(sha)
                                            + ':MODULES.md 2>/dev/null || true', 60)
        (self.out / 'handoff-diffs.json').write_text(json.dumps(diffs, indent=2) + '\n')
        transcripts = {}
        for label, folder in (('home', self.home + '/.claude'), ('work', self.home + '/' + WORK_FOLDER)):
            target = self.out / 'transcripts' / label
            target.mkdir(parents=True, exist_ok=True)
            archive = self.payload + '/transcripts-' + label + '.tgz'
            guest(self.vm, 'tar -C ' + shlex.quote(folder) + ' -czf ' + shlex.quote(archive) + ' projects 2>/dev/null || true', 300)
            local = self.out / ('transcripts-' + label + '.tgz')
            command([HERE / 'guest.sh', self.vm, '--pull', archive, local], 600)
            try:
                with tarfile.open(local) as tar:
                    tar.extractall(target, filter='data')
                local.unlink()
            except (tarfile.TarError, OSError) as exc:
                transcripts[label] = 'not copied: ' + str(exc)
                continue
            transcripts[label] = sorted(str(p.relative_to(target)) for p in target.rglob('*.json*'))
        # The app's own per-turn records (machinery: each lease turn's usage from Claude Code's
        # stream) and the hook evidence, for the token count: the leases keep no transcripts.
        for name in ('machinery', 'engine-state/evidence', 'engine-state/provider-leases'):
            archive = self.payload + '/' + name.replace('/', '-') + '.tgz'
            guest(self.vm, 'tar -C ' + shlex.quote(self.data) + ' -czf ' + shlex.quote(archive) + ' '
                  + shlex.quote(name) + ' 2>/dev/null || true', 300)
            local = self.out / (name.replace('/', '-') + '.tgz')
            try:
                command([HERE / 'guest.sh', self.vm, '--pull', archive, local], 600)
            except StepFailed as exc:
                transcripts[name] = 'not copied: ' + str(exc)[:300]
        (self.out / 'records-final.json').write_text(json.dumps(self.records(), indent=2) + '\n')
        obligation = self.obligation((record or {}).get('obligation_id')) if record and record.get('obligation_id') else None
        (self.out / 'obligation.json').write_text(json.dumps(obligation, indent=2) + '\n')
        return {'rows': rows, 'acme_log': acme, 'acme_head': head, 'summary': summary, 'diffs': diffs,
                'transcripts': transcripts, 'record': record, 'facts': self.facts, 'obligation': obligation,
                'out': str(self.out)}


def saved_evidence(out):
    """The verdict's evidence read back from a finished run's --out, so the pass list can be judged
    again offline (`--judge OUT`) after a rule is corrected, without another guest run."""
    out = Path(out)
    facts = json.loads((out / 'facts.json').read_text())
    rows = [json.loads(line) for line in (out / 'handoff-watch.jsonl').read_text().splitlines() if line.startswith('{')]
    sent = facts.get('sent_ms') or [0]
    records = json.loads((out / 'records-final.json').read_text()) if (out / 'records-final.json').exists() else []
    mine = [r for r in records if r.get('registered_at_ms', 0) >= sent[0]]
    record = max(mine, key=lambda r: r.get('registered_at_ms', 0)) if mine else None
    obligation = None
    if (out / 'obligation.json').exists():
        obligation = json.loads((out / 'obligation.json').read_text())
    elif (out / 'verdict.json').exists():
        for l in json.loads((out / 'verdict.json').read_text()).get('lines', []):
            if l['line'].startswith('5 ') and isinstance(l.get('evidence'), dict):
                obligation = l['evidence'].get('obligation')
    return {'rows': rows, 'acme_log': (out / 'acme-log.txt').read_text(), 'summary': (out / 'MODULES.md').read_text(),
            'diffs': json.loads((out / 'handoff-diffs.json').read_text()), 'record': record, 'facts': facts,
            'obligation': obligation, 'out': str(out)}


def sections(text):
    return [line[3:].strip() for line in text.splitlines() if line.startswith('## ')]


def agent_transcript(out, agent_id):
    """Which account folder the agent's own record is under ('home' or 'work'), and the last timestamp
    of its transcript when there is one. The app's leases run with --no-session-persistence
    (native.rs), so a helper leaves only `subagents/agent-<id>.meta.json` under the folder of the
    account it ran on (run 8: under Home's .claude, "model":"opus"); that file is enough to say where."""
    if not agent_id:
        return None, None
    for label in ('home', 'work'):
        folder = Path(out) / 'transcripts' / label
        for path in folder.rglob('agent-' + agent_id + '.jsonl'):
            last = None
            for line in path.read_text(errors='replace').splitlines():
                try:
                    last = json.loads(line).get('timestamp') or last
                except ValueError:
                    pass
            return label, last
        for _ in folder.rglob('agent-' + agent_id + '.meta.json'):
            return label, None
    return None, None


def iso_ms(text):
    import datetime
    return int(datetime.datetime.fromisoformat(text.replace('Z', '+00:00')).timestamp() * 1000) if text else None


def judge(e):
    """The pass list, from the saved evidence alone (testable offline: test/handoff-real-walk.test.py)."""
    rows, facts = e['rows'], e['facts']
    cut = facts.get('cutoff')
    markers = {}
    for r in rows:
        if r['kind'] == 'marker':
            markers[r['value']['agent']] = r['value']
    receipts = {}
    for r in rows:
        if r['kind'] == 'receipt':
            receipts[r['value']['id']] = r['value'] | {'_t_ms': r['t_ms']}
    briefs = {Path(r['path']).stem: r['text'] for r in rows if r['kind'] == 'brief'}
    gits = {}
    for r in rows:
        if r['kind'] == 'git':
            gits.setdefault(Path(r['path']).name, []).append(r['value'] | {'t_ms': r['t_ms']})
    stops = {}
    for r in rows:
        if r['kind'] == 'hook' and r['event'] == 'SubagentStop':
            stops.setdefault(r['agent_id'], r['t_ms'])
    starts = {}
    for r in rows:
        if r['kind'] == 'hook' and r['event'] == 'SubagentStart':
            starts.setdefault(r['agent_id'], r['t_ms'])
    quotas = [r for r in rows if r['kind'] == 'quota']
    lines, handoff_ms = [], {}

    def line(name, ok, detail, evidence=None):
        lines.append({'line': name, 'outcome': 'PASS' if ok else 'FAIL', 'detail': detail, 'evidence': evidence})

    # 1 The order arrives at the cut-off.
    first = min(markers.values(), key=lambda m: m['at']) if markers else None
    if not first:
        line('1 The order arrives at the cut-off.', False, 'no helper was ordered (no handoff marker was written)',
             {'quota': [q['value'] for q in quotas][-3:]})
    else:
        before = [q for q in quotas if q['t_ms'] <= first['at']]
        at = before[-1]['value'] if before else None
        home = next((a['weekly'] for a in (at or {}).get('accounts', []) if a['id'] == '1'), None)
        point = ((at or {}).get('actAt') or {}).get('seven_day')
        early = [q for q in quotas if q['t_ms'] <= first['at'] and '1' in (q['value'].get('leaving') or [])
                 and next((a['weekly'] for a in q['value']['accounts'] if a['id'] == '1'), 0) < (cut or 0)]
        # The view's `actAt` is the point of the account IN USE: at the order the switch has
        # usually happened already, so Home's own point is read from the last view in which Home
        # was in use (run 10: 99 at the order, Work's; 83 just before, Home's cut-off).
        home_rows = [q for q in before if any(a['id'] == '1' and a.get('inUse') for a in q['value']['accounts'])]
        point = ((home_rows[-1]['value'].get('actAt') or {}).get('seven_day')) if home_rows else point
        ok = (at is not None and '1' in (at.get('leaving') or []) and home is not None and cut is not None
              and home >= min(cut, point if point is not None else cut))
        line('1 The order arrives at the cut-off.', ok,
             f'first order {first["agent"]} at {first["at"]}: Home weekly {home}, Home\'s point {point}, cut-off {cut}, '
             f'leaving {at.get("leaving") if at else None}' + ('; Home was leaving below the cut-off' if early else ''),
             {'marker': first, 'view_at_order': at})
    # The ordered workers and their successors.
    ordered = [r for r in receipts.values() if r.get('agent_id') in markers and r['request'].get('role') == 'worker']
    found2, found3, found4 = [], [], []
    for w in ordered:
        snaps = gits.get(w['name'], [])
        last = snaps[-1] if snaps else None
        log = (last or {}).get('log') or []
        base = facts.get('base')
        above = []
        for sha, _ct, subject in log:
            if sha == base:
                break
            above.append((sha, subject))
        tip_ok = bool(above) and above[0][1].startswith(HANDOFF_PREFIX)
        work = [a for a in above[1:] if not a[1].startswith(HANDOFF_PREFIX) and a[1] != LEFTOVERS]
        found2.append({'worker': w['name'], 'agent_id': w['agent_id'], 'commits_above_base': above,
                       'handoff_is_last': tip_ok, 'work_commits': len(work),
                       'leftovers_saved': any(a[1] == LEFTOVERS for a in above)})
        handoff_sha = above[0][0] if tip_ok else None
        m = markers[w['agent_id']]
        end_hook = stops.get(w['agent_id'])
        where, last_ts = agent_transcript(e['out'], w['agent_id'])
        end_t = iso_ms(last_ts)
        took = (end_hook - m['at']) if end_hook else None
        handoff_ms[w['name']] = {'order_ms': m['at'], 'subagent_stop_seen_ms': end_hook, 'transcript_last_ms': end_t,
                                 'order_to_end_ms': took,
                                 'order_to_last_transcript_line_ms': (end_t - m['at']) if end_t else None,
                                 'account_of_transcript': where}
        succ = [r for r in receipts.values() if r['request'].get('continue_of') == w['id']]
        s = succ[0] if succ else None
        brief = briefs.get((s or {}).get('id'), '')
        s_where, _ = agent_transcript(e['out'], (s or {}).get('agent_id'))
        found3.append({'worker': w['name'], 'handoff_commit': handoff_sha, 'successor': (s or {}).get('name'),
                       'successor_commit': ((s or {}).get('continuation') or {}).get('commit'),
                       'brief_has_handoff': HANDOFF_BRIEF in brief and HANDOFF_PREFIX in brief,
                       'successor_transcript_account': s_where,
                       'successor_started_ms': starts.get((s or {}).get('agent_id'))})
        numstat = e['diffs'].get(handoff_sha or '', '')
        deleted = sum(int(x.split('\t')[1]) for x in numstat.splitlines() if x.count('\t') == 2 and x.split('\t')[1].isdigit())
        at_handoff = sections(e['diffs'].get((handoff_sha or '') + ':summary', ''))
        final = sections(e['diffs'].get((handoff_sha or '') + ':tip-summary') or e['summary'])
        dup = sorted({n for n in final if final.count(n) > 1})
        found4.append({'worker': w['name'], 'summary_at_handoff_sections': len(at_handoff), 'final_sections': len(final),
                       'lines_deleted_after_handoff': deleted, 'numstat': numstat, 'duplicated_sections': dup,
                       'kept': all(n in final for n in at_handoff)})
    line('2 The worker\'s branch holds its work commits, then a `RichOS handoff:` commit.',
         bool(found2) and all(f['handoff_is_last'] and f['work_commits'] > 0 for f in found2),
         '; '.join(f"{f['worker']}: {f['work_commits']} work commit(s), handoff last {f['handoff_is_last']}"
                   + (', leftovers saved by the engine' if f['leftovers_saved'] else '') for f in found2) or 'no ordered worker',
         found2)
    line('3 A successor on Work starts at that commit, with the handoff text in its brief.',
         bool(found3) and all(f['handoff_commit'] and f['successor_commit'] == f['handoff_commit'] and f['brief_has_handoff']
                              and f['successor_transcript_account'] == 'work' for f in found3),
         '; '.join(f"{f['worker']} -> {f['successor']}: starts at {f['successor_commit']} (handoff {f['handoff_commit']}), "
                   f"handoff in brief {f['brief_has_handoff']}, transcript under {f['successor_transcript_account']}"
                   for f in found3) or 'no ordered worker',
         found3)
    line('4 The successor\'s diff does not rewrite summaries already committed (nothing redone).',
         bool(found4) and all(f['lines_deleted_after_handoff'] == 0 and not f['duplicated_sections'] and f['kept']
                              and f['summary_at_handoff_sections'] > 0 for f in found4),
         '; '.join(f"{f['worker']}: {f['summary_at_handoff_sections']} sections at the handoff, {f['final_sections']} landed, "
                   f"{f['lines_deleted_after_handoff']} line(s) deleted after it, duplicates {f['duplicated_sections']}"
                   for f in found4) or 'no ordered worker',
         found4)
    # 5 reviewed, landed, closed, with no step by anyone.
    record = e.get('record') or {}
    reviewers = [r for r in receipts.values() if r['request'].get('role') == 'reviewer']
    integrated = [r for r in receipts.values() if r['request'].get('role') == 'worker' and r.get('status') == 'integrated']
    files = facts.get('files')
    final = sections(e['summary'])
    presses = facts.get('approvals') or []
    obligation = e.get('obligation') or {}
    ok5 = (record.get('state') == 'settled' and reviewers and integrated and files and len(set(final)) == files
           and not presses and obligation.get('status') == 'completed')
    line('5 The job is reviewed, landed and closed with no step by anyone.', bool(ok5),
         f"job {record.get('state')} ({record.get('detail')}); reviewers {[r['name'] + ':' + str(r.get('status')) for r in reviewers]}; "
         f"integrated {[r['name'] for r in integrated]}; landed MODULES.md sections {len(set(final))} of {files}; "
         f"Approve presses {len(presses)}; obligation {obligation.get('status')}",
         {'record': record, 'reviewers': reviewers, 'integrated': integrated, 'approvals': presses, 'obligation': obligation})
    # 6 how long the handoff took; and his 3-5 minutes.
    took = [v['order_to_end_ms'] for v in handoff_ms.values()]
    def minutes(v):
        return 'no end seen' if v['order_to_end_ms'] is None else '%.2f min' % (v['order_to_end_ms'] / 60000)
    line('6 Record how long the handoff took (sets `H`).', bool(took) and all(t is not None for t in took),
         '; '.join(f'{k}: {minutes(v)}' for k, v in handoff_ms.items()) or 'nothing was ordered', handoff_ms)
    line('+ Each teammate ends within 5 minutes of the order (his 3-5 minutes).',
         bool(took) and all(t is not None and t <= HANDOFF_LIMIT_MS for t in took),
         '; '.join(f"{k}: {round(v['order_to_end_ms'] / 1000) if v['order_to_end_ms'] is not None else None} s"
                   for k, v in handoff_ms.items()) or 'nothing was ordered', handoff_ms)
    return {'cutoff': cut, 'lines': lines, 'handoff_ms': handoff_ms}


def seed_team(home):
    folder = Path(home) / 'Library/Application Support/com.richos.app/team'
    folder.mkdir(parents=True, exist_ok=True)
    for name, body in TEAM.items():
        (folder / (name + '.md')).write_text(body)
        print(folder / (name + '.md'))
    return 0


def main():
    if sys.argv[1:2] == ['--seed-team']:
        return seed_team(sys.argv[2])
    if sys.argv[1:2] == ['--judge']:
        verdict = judge(saved_evidence(sys.argv[2]))
        (Path(sys.argv[2]) / 'verdict-rejudged.json').write_text(json.dumps(verdict, indent=2) + '\n')
        for l in verdict['lines']:
            print(f"{l['outcome']}  {l['line']}  {l['detail']}")
        return 0 if all(l['outcome'] == 'PASS' for l in verdict['lines']) else 1
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--corpus', type=Path, required=True, help='a tar holding lib/*.py, the files the job summarizes')
    p.add_argument('--within', type=float, default=5 * 3600, help='seconds for the job to close after it is sent')
    p.add_argument('--approvals', type=int, default=6, help='most Approve presses (each one fails pass line 5)')
    p.add_argument('--probe-gaps', default='0,2', help="the probe step's ask times, seconds from the first")
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS + ['probe', 'collect-now']]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = HandoffWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
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

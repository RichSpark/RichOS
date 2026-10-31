#!/usr/bin/env python3
"""toolchain-ask-walk.py — on a Mac with no developer tools, Rich asks before installing a tool
the work needs, installs it once on the user's yes, and the work continues (CEO 2026-10-29).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      toolchain-ask-walk.py --out DIR --expect-sha SHA

THE QUESTION. The CEO, 2026-10-29: "If a tool is needed for actual work and the tool isn't
there, then the user needs to be asked permission for installing etc. Unless the user has already
given a general permission for installing what's needed." and "no user would ever want getting
the same fucking tool installed a million times." Most regular users are not developers, so the
case is a Mac with no Apple developer tools, where `make` is only Apple's install stub.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: memory setup declined, company "Acme" registered, questions declined
  connect    command-walk.py's: the Acme folder connected, so work can be handed over
  no-tools   the guest becomes a Mac without developer tools: the Command Line Tools folder moved
             aside and the xcode-select link removed (the base image has them for Homebrew; the
             clone is deleted after the run). The harness's own `python3` is pointed at the
             engine's runtime Python, so no harness call reaches a stub. The engine's resolver
             says no developer directory and `make` is missing
  task       typed into the composer: run `make --version` and say what it printed first
  ask        within --within seconds Rich has asked, in a turn of his: his text names installing
             and the tool. The ask ledger says `make` was asked about, the installer never ran
             and no Command Line Tools exist yet
  yes        typed into the composer: the user's yes
  installer  Apple's "Install Command Line Developer Tools" window appears (exactly one), and the
             walk presses Install and Agree as the user does
  installed  within --install-within seconds the Command Line Tools' make exists
  continued  within --within seconds a Bash run of the app printed `make --version`'s "GNU Make"
             line, and Rich said it in the conversation
  once       the same task again: it runs straight away with no ask, no installer window and no
             second install; the guest's install log holds one Command Line Tools install

Evidence in --out: report.json, facts.json, screenshots, Rich's words, the ask ledger, the app
log, the hook callbacks and every refused tool call word for word (refusals.txt). Exit 0 when
every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed

STEPS = ['identity', 'first-run', 'connect', 'no-tools', 'task', 'ask', 'yes', 'installer', 'installed',
         'continued', 'once']
# command-walk.py's TASK wording, the form proven to reach a turn that runs the command.
TASK = ('Please run this harmless test command for me yourself with your shell tool, in my Acme '
        'folder, and tell me when it has finished and the first line it printed: make --version')
AGAIN = ('Thanks. Please run the same command once more yourself, in my Acme folder, and tell me the '
         'first line it printed: make --version')
YES = 'Yes, please install it.'
INSTALLER = 'Install Command Line Developer Tools'
CLT = '/Library/Developer/CommandLineTools'
HIDDEN = CLT + '.walk-hidden'


def turns_after(rows, after_ms):
    """Rich's completed turns that began after AFTER_MS: [{'turn_id', 'text'}], oldest first."""
    began, text, done = {}, {}, []
    for r in rows:
        turn = r.get('turn_id')
        if not turn:
            continue
        began.setdefault(turn, r.get('at') or 0)  # epoch milliseconds (ledger.rs)
        if r.get('event') == 'AssistantDelta':
            text[turn] = text.get(turn, '') + (r.get('text') or '')
        elif r.get('event') == 'TurnCompleted':
            done.append(turn)
    return [{'turn_id': t, 'text': text.get(t, '')} for t in done if not began.get(t) or began[t] >= after_ms]


def asks_to_install(text, tool='make'):
    """Whether Rich's words ask the user about installing TOOL (or the developer tools it comes in)."""
    low = text.lower()
    return 'install' in low and (tool in low or 'developer tools' in low or 'command line tools' in low)


def make_runs(rows):
    """The Bash PostToolUse rows that ran `make --version` itself (not the install command)."""
    return [r for r in rows if 'make --version' in r.get('command', '') and 'toolchain.py' not in r.get('command', '')]


class ToolchainAskWalk(command_walk.CommandWalk):
    def runtime_python(self):
        return self.payload + '/engine/runtime/bin/python3'

    def ledger_rows(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        rows = []
        for line in text.splitlines():
            try:
                rows.append(json.loads(line))
            except ValueError:
                pass
        return rows

    def asks(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/engine-state/toolchain/asks.json') + ' 2>/dev/null || true')
        try:
            return json.loads(text) if text else {}
        except ValueError:
            return {'unreadable': text[:500]}

    def installers(self):
        """Apple's installer processes running now (pgrep is not a developer-tools stub)."""
        return [l for l in guest(self.vm, 'pgrep -fl ' + shlex.quote(INSTALLER) + ' || true').splitlines() if l.strip()]

    def installs_logged(self):
        return [l for l in guest(self.vm, "grep -a 'Installed \"Command Line Tools' /var/log/install.log || true", 60)
                .splitlines() if l.strip()]

    def bash_runs(self):
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("hook_event_name")=="PostToolUse" and c.get("tool_name")=="Bash":\n'
                  '            r=c.get("tool_response") or {}\n'
                  '            out.append({"agent_id":c.get("agent_id"),\n'
                  '                        "command":c.get("tool_input",{}).get("command",""),\n'
                  '                        "stdout":(r.get("stdout","") if isinstance(r,dict) else str(r))[:2000]})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def wait_turn(self, after_ms, judge, seconds, what):
        end = time.monotonic() + seconds
        turns = []
        while time.monotonic() < end:
            turns = turns_after(self.ledger_rows(), after_ms)
            hit = next((t for t in turns if judge(t['text'])), None)
            if hit:
                return hit, turns
            time.sleep(5)
        (self.out / ('turns-%s.json' % what)).write_text(json.dumps(turns, indent=2) + '\n')
        raise StepFailed('no turn of Rich\'s %s within %g s; his turns since: %s'
                         % (what, seconds, json.dumps([t['text'][:300] for t in turns])[:1500]))

    # --- steps --------------------------------------------------------------------------------
    def no_tools(self):
        py = self.runtime_python()
        setup = ('set -e\n'
                 'if [ -d /Applications/Xcode.app ]; then echo "XCODE"; exit 3; fi\n'
                 'if [ -d %(clt)s ]; then sudo -n mv %(clt)s %(hidden)s; fi\n'
                 'sudo -n rm -f /var/db/xcode_select_link\n'
                 'mkdir -p "$HOME/walk-bin"\n'
                 'ln -sf %(py)s "$HOME/walk-bin/python3"\n'
                 'grep -q walk-bin "$HOME/.zshenv" 2>/dev/null || '
                 'echo \'export PATH="$HOME/walk-bin:$PATH"\' >> "$HOME/.zshenv"\n'
                 % {'clt': CLT, 'hidden': HIDDEN, 'py': shlex.quote(py)})
        guest(self.vm, 'sh -c ' + shlex.quote(setup), 60)
        resolved = guest(self.vm, shlex.quote(py) + ' ' + shlex.quote(self.payload + '/engine/scripts/lib/toolchain.py')
                         + ' resolve make || true')
        found = json.loads(resolved)
        harness_python = guest(self.vm, 'command -v python3')
        if found.get('developer_dir') or found['tools'].get('make'):
            raise StepFailed('the guest still has developer tools: ' + resolved)
        if '/walk-bin/' not in harness_python:
            raise StepFailed('the harness python3 is still ' + harness_python + ' (Apple\'s stub without tools)')
        self.facts['walk_start_install_log'] = len(self.installs_logged())
        self.save()
        return {'resolve': found, 'harness_python3': harness_python, 'installs_logged_before': self.facts['walk_start_install_log']}

    def ask(self):
        sent = self.facts.get('sent_ms')
        if not sent:
            raise StepFailed('the task step must have run (no send time on record)')
        hit, turns = self.wait_turn(sent[0], asks_to_install, self.a.within, 'asking about installing make')
        asks = self.asks()
        row = (asks.get('tools') or {}).get('make') or {}
        evidence = {'rich_said': hit['text'], 'asks_make': row, 'installers': self.installers(),
                    'clt_present': guest(self.vm, 'test -e %s/usr/bin/make && echo yes || echo no' % CLT)}
        (self.out / 'ask.json').write_text(json.dumps(evidence, indent=2) + '\n')
        self.shot('ask.png')
        if row.get('state') != 'asked':
            raise StepFailed('the ask ledger does not say make was asked about: ' + json.dumps(asks)[:800])
        if evidence['installers'] or evidence['clt_present'] == 'yes':
            raise StepFailed('something was installed before the user answered: ' + json.dumps(evidence)[:800])
        self.facts['ask_turn'] = hit['turn_id']
        self.save()
        return evidence

    def yes(self):
        self.facts['yes_ms'] = self.send(YES)
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in self.facts['yes_ms']]}

    def installer_button(self, name, press=False):
        """Whether Apple's installer shows button NAME (pressing it when PRESS). Through System
        Events, by the process's full name: ax.py finds a named app with `pgrep -x`, which matches
        only the kernel's 16-character process name and so never this one."""
        script = ('tell application "System Events"\n'
                  '  if not (exists process "%(app)s") then return "no"\n'
                  '  tell process "%(app)s"\n'
                  '    repeat with w in windows\n'
                  '      repeat with e in (entire contents of w)\n'
                  '        try\n'
                  '          if role of e is "AXButton" and name of e is "%(name)s" then\n'
                  '            %(act)s\n'
                  '            return "yes"\n'
                  '          end if\n'
                  '        end try\n'
                  '      end repeat\n'
                  '    end repeat\n'
                  '  end tell\n'
                  'end tell\n'
                  'return "no"' % {'app': INSTALLER, 'name': name, 'act': 'click e' if press else 'set x to 1'})
        return command_walk.adopt_walk.command([HERE / 'ax.sh', self.vm, script], 90).strip().endswith('yes')

    def wait_installer_button(self, name, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if self.installer_button(name):
                return
            time.sleep(3)
        self.shot('no-%s.png' % name.lower())
        raise StepFailed('Apple\'s installer never showed "%s" within %g s' % (name, seconds))

    def installer(self):
        self.wait_installer_button('Install', self.a.within)
        running = self.installers()
        self.shot('installer.png')
        if not self.installer_button('Install', press=True):
            raise StepFailed('the installer\'s Install button could not be pressed')
        self.wait_installer_button('Agree', 120)
        self.shot('license.png')
        if not self.installer_button('Agree', press=True):
            raise StepFailed('the license\'s Agree button could not be pressed')
        self.facts['installer_running'] = running
        self.save()
        if len(running) != 1:
            raise StepFailed('expected one installer process, saw: ' + json.dumps(running))
        return {'installer_processes': running}

    def installed(self):
        began = time.monotonic()
        end = began + self.a.install_within
        while time.monotonic() < end:
            if guest(self.vm, 'test -x %s/usr/bin/make && echo yes || echo no' % CLT) == 'yes':
                seconds = round(time.monotonic() - began)
                self.shot('installed.png')
                try:
                    self.installer_button('Done', press=True)
                except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                    pass
                return {'make': CLT + '/usr/bin/make', 'seconds_after_agree': seconds}
            time.sleep(10)
        self.shot('not-installed.png')
        raise StepFailed('the Command Line Tools were not installed within %g s' % self.a.install_within)

    def continued(self):
        yes = self.facts.get('yes_ms')
        if not yes:
            raise StepFailed('the yes step must have run')
        end = time.monotonic() + self.a.within
        runs = []
        while time.monotonic() < end:
            runs = [r for r in make_runs(self.bash_runs()) if 'GNU Make' in r['stdout']]
            if runs:
                break
            time.sleep(5)
        if not runs:
            raise StepFailed('no Bash run printed GNU Make after the install')
        hit, _ = self.wait_turn(yes[0], lambda text: 'GNU Make' in text, self.a.within, 'saying what make printed')
        installs = [r for r in self.bash_runs() if 'toolchain.py' in r['command']]
        evidence = {'make_run': runs[0], 'rich_said': hit['text'], 'install_commands': installs}
        (self.out / 'continued.json').write_text(json.dumps(evidence, indent=2) + '\n')
        self.shot('continued.png')
        self.facts['first_make_runs'] = len(make_runs(self.bash_runs()))
        self.save()
        return evidence

    def once(self):
        before = len(make_runs(self.bash_runs()))
        sent = self.send(AGAIN)
        hit, _ = self.wait_turn(sent[0], lambda text: 'GNU Make' in text, self.a.within, 'answering the second request')
        asks = self.asks()
        logged = self.installs_logged()
        evidence = {'rich_said': hit['text'], 'make_runs_before': before, 'make_runs_after': len(make_runs(self.bash_runs())),
                    'installers': self.installers(), 'asks': asks, 'installs_logged': logged,
                    'installs_logged_before_walk': self.facts.get('walk_start_install_log')}
        (self.out / 'once.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = []
        if asks_to_install(hit['text']) and '?' in hit['text']:
            failures.append('Rich asked about installing again')
        if evidence['installers']:
            failures.append('an installer is running again')
        if ((asks.get('tools') or {}).get('make') or {}).get('state') != 'found':
            failures.append('the ask ledger does not record make as found')
        new_installs = len(logged) - (self.facts.get('walk_start_install_log') or 0)
        if new_installs != 1:
            failures.append('the install log holds %d Command Line Tools installs from this walk, not 1' % new_installs)
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=TASK)
    p.add_argument('--within', type=float, default=600, help='seconds for each of Rich\'s turns')
    p.add_argument('--install-within', type=float, default=2400, help='seconds for Apple\'s install to finish')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = ToolchainAskWalk(a)
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
        (a.out / 'asks.json').write_text(json.dumps(walk.asks(), indent=2) + '\n')
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

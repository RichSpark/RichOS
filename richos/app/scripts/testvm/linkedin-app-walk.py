#!/usr/bin/env python3
"""linkedin-app-walk.py — Rich reads a LinkedIn profile through the app, with Accessibility on RichOS only.

The job, as the CEO confirmed it on 2026-11-10: during onboarding, Rich uses the user's default
browser exactly as it is (already signed in everywhere, no step for the user) to quickly open their
LinkedIn profile and the company pages connected to it and grab the info, browsing LinkedIn only as
much as that takes.

linkedin-profile-walk.py drives the tool over stdio from the guest's ssh session, whose own
Accessibility grant the guest image carries. This walk proves the two things that one cannot:

  1. the tool is started the way the app starts it, by Rich's `claude` session through the app's
     MCP config (`native.rs` `mcp_config`, `--linkedin-mcp`), and works with the Accessibility
     permission granted to RichOS (com.richos.app) and to nothing else on the guest, the row
     System Settings writes when a user allows RichOS (what RichScribe's sheet asks for);
  2. every tab the tool opens in the user's browser is closed again, and none of the user's.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest with the app and
removes it:

  linkedin-standin.py --profile SAVED.html --slug SLUG --out SITE
  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TAR --report REPORT --wait 600 -- \\
      linkedin-app-walk.py --out DIR --site SITE [--expect-sha SHA]

ZIP is branch-bundle.sh's (this checkout's executable); ENGINE_TAR the engine the app installs
(a nightly's richos-engine-*.tar.gz). SITE is the stand-in built from saved pages in scratch,
never committed: they are real people's information, and so are DIR's transcripts.

  site        the stand-in served in the guest (linkedin-profile-walk.py's step)
  chrome      Google Chrome installed (browser-page-walk.py's step)
  tcc         Accessibility for com.richos.app in the system TCC database (client type bundle id,
              allowed), and Apple Events from the ssh session to Safari and Chrome so the walk can
              list their tabs without Accessibility
  setup       the app's first run up to its first conversation (conversation-walk.py's setup)
  safari      Safari the default browser and signed in to the stand-in, with two tabs of the
              user's own (the sign-in page and the profile itself). One message to Rich asks him
              to read the profile. The moment it is sent, every Accessibility row except
              com.richos.app's is deleted (the base image's: the ssh session, System Events,
              osascript, Python, the tart agent; each kept aside in a file), tccd is restarted
              and the rows are listed. While the turn runs, the tabs are listed every two seconds
              and the `--linkedin-mcp` process is found with its ancestors. Then the rows are put
              back for the next typing (and, when a tab was left, the browser's tab bar is written
              to DIR/<browser>-tab-bar.json). PASS when: Accessibility was on com.richos.app
              alone; the tool's parent is a `claude` the app started (through the engine's
              provider-supervisor.py, so the app is among its ancestors); the turn completed;
              more tabs were open during the turn than before; and the tabs after the turn are
              the user's own, exactly as before
  chrome-read the same with Chrome the default browser
  control     every Accessibility row deleted, RichOS's too, and tccd restarted: first a plain
              Python from ssh asks AXIsProcessTrusted. When it is trusted all the same (the
              cirruslabs image, SIP disabled, measured in walk-4a262a1b1e63 with no MDM override),
              the guest cannot show that the permission is needed, and the step says NOT PROVABLE
              HERE. Otherwise the tool started directly must be refused for want of the permission.
              Then every row is put back
  pull        Claude Code's transcripts, the ledger and the app log into DIR. PASS when Rich made
              one read_profile call per browser, after the other rows were deleted, and each
              returned the profile's, its Show all page's and every company's hidden text

Exit 0 when every step passes, 3 when none failed and one could not be shown on this guest, 1
otherwise. DIR/report.json is the report.
"""
import argparse
import calendar
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

TOOL = 'mcp__richos_linkedin__read_profile'
SAFARI, CHROME = 'com.apple.Safari', 'com.google.Chrome'
STEPS = ['site', 'chrome', 'tcc', 'setup', 'safari', 'chrome-read', 'control', 'pull']
SYS_DB = '"/Library/Application Support/com.apple.TCC/TCC.db"'
USER_DB = '"$HOME/Library/Application Support/com.apple.TCC/TCC.db"'
ROW = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
       "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) "
       "VALUES ('{svc}', '{client}', {ctype}, 2, 2, 1, 0, '{indirect}', 0, strftime('%s','now'));")
# Every Accessibility row but RichOS's: the base image's (the ssh session, System Events,
# osascript, Python, the tart agent), deleted while Rich's turn runs and put back after it.
OTHER_AX = "service='kTCCServiceAccessibility' AND client != 'com.richos.app'"
TABS_JS = {SAFARI: 'Safari', CHROME: 'Google Chrome'}


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


conversation_walk = load('conversation_walk', HERE / 'conversation-walk.py')
profile_walk = load('linkedin_profile_walk', HERE / 'linkedin-profile-walk.py')
StepFailed = conversation_walk.StepFailed


class NotProvable(Exception):
    """What a step measures cannot be shown on this guest: neither a pass nor a failure of the app."""


def sql(db, statement, sudo):
    # tccd holds the database while it answers; wait for it rather than fail ("database is
    # locked", seen right after a guest boots).
    return ('sudo -n ' if sudo else '') + f"sqlite3 -cmd '.timeout 10000' {db} " + shlex.quote(statement)


class Walk(conversation_walk.ConversationWalk):
    def __init__(self, a):
        a.out.mkdir(parents=True, exist_ok=True)
        plan = a.out / 'conversation.json'
        plan.write_text('["unused"]\n')
        a.conversation, a.company, a.within, a.press_grace = plan, 'Acme', 600, 20
        super().__init__(a)
        self.li = profile_walk.Walk(argparse.Namespace(vm=a.vm, out=a.out, site=a.site, exe=Path('/dev/null')))
        state = Path(__import__('os').environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.app_pid = (state / 'app.pid').read_text().strip()
        app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
        self.exe = app + '/Contents/MacOS/richos-tauri'
        self.rounds = {}

    def g(self, cmd, timeout=60):
        return guest(self.vm, cmd, timeout)

    # --- the permission -------------------------------------------------------------------------
    def ax_clients(self):
        q = "SELECT client FROM access WHERE service='kTCCServiceAccessibility' ORDER BY client;"
        return {'system': self.g(sql(SYS_DB, q, True)).splitlines(), 'user': self.g(sql(USER_DB, q, False)).splitlines()}

    def set_aside(self, where, name):
        """The Accessibility rows `where` selects deleted, each kept first in a file of its own
        beside the payload (never a table in TCC.db) so they go back exactly as they were."""
        for db, sudo, tag in ((SYS_DB, True, 'system'), (USER_DB, False, 'user')):
            backup = f'{self.payload}/ax-{name}-{tag}.db'
            self.g(sql(db, f"ATTACH '{backup}' AS b; DROP TABLE IF EXISTS b.rows; "
                           f"CREATE TABLE b.rows AS SELECT * FROM access WHERE {where}; "
                           f"DELETE FROM access WHERE {where};", sudo))
        self.flush_tcc()
        return self.ax_clients()

    def put_back(self, name):
        for db, sudo, tag in ((SYS_DB, True, 'system'), (USER_DB, False, 'user')):
            backup = f'{self.payload}/ax-{name}-{tag}.db'
            self.g(f'if [ -f {shlex.quote(backup)} ]; then ' + sql(db, f"ATTACH '{backup}' AS b; "
                                                                      "INSERT OR REPLACE INTO access SELECT * FROM b.rows;", sudo) + '; fi')
        self.flush_tcc()

    def revoke_all_but_richos(self):
        return self.set_aside(OTHER_AX, 'others')

    def restore_harness(self):
        self.put_back('others')

    def flush_tcc(self):
        """Both tccd processes ended (launchd starts them again on the next question), so a
        changed row holds at once, whatever tccd remembered of the old one."""
        self.g('sudo -n killall tccd 2>/dev/null; sleep 1; true')

    def direct_call(self):
        """The tool started directly from the ssh session, as the control."""
        frames = [{'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {}},
                  {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call',
                   'params': {'name': 'read_profile', 'arguments': {'url': self.li.profile}}}]
        text = ''.join(json.dumps(f) + '\n' for f in frames)
        out = self.g(f'printf %s {shlex.quote(text)} | {shlex.quote(self.exe)} --linkedin-mcp', 120)
        for line in out.splitlines():
            if line.startswith('{') and json.loads(line).get('id') == 2:
                return json.loads(line)['result']
        raise StepFailed(f'the direct tool call answered nothing: {out[-400:]}')

    # --- the steps ------------------------------------------------------------------------------
    def site(self):
        return self.li.site()

    def chrome(self):
        return self.li.chrome()

    def tcc(self):
        self.g(sql(SYS_DB, ROW.format(svc='kTCCServiceAccessibility', client='com.richos.app', ctype=0, indirect='UNUSED'), True))
        for target in (SAFARI, CHROME):
            for client, ctype in (('com.apple.sshd-session', 0), ('/usr/libexec/sshd-keygen-wrapper', 1)):
                self.g(sql(USER_DB, ROW.format(svc='kTCCServiceAppleEvents', client=client, ctype=ctype, indirect=target), False))
        self.flush_tcc()
        return {'accessibility': self.ax_clients()}

    def tabs(self, bundle):
        js = ('var a=Application(%s);JSON.stringify(a.windows().map(function(w){try{return w.tabs().map('
              'function(t){return t.url()})}catch(e){return []}}))') % json.dumps(TABS_JS[bundle])
        out = self.g('osascript -l JavaScript -e ' + shlex.quote(js), 60)
        return json.loads(out.splitlines()[-1])

    def mcp_chain(self):
        rows = {}
        for line in self.g('ps -axo pid=,ppid=,command=').splitlines():
            parts = line.split(None, 2)
            if len(parts) == 3:
                rows[parts[0]] = (parts[1], parts[2])
        """Each `--linkedin-mcp` process with its ancestors up to launchd: [{pid, command}, ...],
        the tool first. Measured in walk-59ad5d5270c7: the app starts `claude` through the
        engine's provider-supervisor.py, so the app is the tool's great-grandparent."""
        found = []
        for pid, (_, cmd) in rows.items():
            if cmd.endswith('/richos-tauri --linkedin-mcp'):
                chain, at = [], pid
                while at in rows and at not in ('0', '1') and len(chain) < 8:
                    chain.append({'pid': at, 'command': rows[at][1][:200]})
                    at = rows[at][0]
                found.append(chain)
        # The control's own direct call from ssh can be running too: the app's is the one under the app.
        return next((c for c in found if self.under_app(c)), found[0] if found else None)

    def under_app(self, chain):
        """The tool's parent is `claude` and the app is among its ancestors."""
        return bool(chain) and len(chain) > 1 and chain[1]['command'].split()[0].endswith('/claude') \
            and any(link['pid'] == self.app_pid for link in chain[2:])

    def round(self, bundle):
        default = self.li.default(bundle)
        self.g(f'open -g -b {bundle} {self.li.base}/sign-in')
        time.sleep(4)
        # The user's own second tab: the profile itself, so a tab on the very page the tool reads
        # must survive too.
        self.g(f'open -g -b {bundle} {self.li.profile}')
        time.sleep(6)
        before = self.tabs(bundle)
        message = (f'This is a test of your LinkedIn tool. Read my LinkedIn profile at {self.li.profile} with '
                   'read_profile (it is served on this Mac for the test), then tell me in one line which companies it found.')
        step = {'kind': 'say', 'text': message, 'expect_reply': True}
        observed = {'max_tabs': sum(map(len, before)), 'chain': None, 'polls': 0}
        stop = threading.Event()
        marks = {}

        def watch():
            while not stop.is_set():
                try:
                    observed['max_tabs'] = max(observed['max_tabs'], sum(map(len, self.tabs(bundle))))
                    if not self.under_app(observed['chain']):
                        observed['chain'] = self.mcp_chain() or observed['chain']
                    observed['polls'] += 1
                except (RuntimeError, ValueError, subprocess.TimeoutExpired):
                    pass
                stop.wait(2)

        original_act = self.act

        def act(step):
            original_act(step)
            marks['accessibility_during'] = self.revoke_all_but_richos()
            marks['revoked_at'] = self.clock() / 1000
            threading.Thread(target=watch, daemon=True).start()

        self.act = act
        try:
            record = self.run_step(len(self.rounds) + 1, step)
        finally:
            self.act = original_act
            stop.set()
            # The harness's own rows back: the next round types into the app through them.
            self.restore_harness()
        time.sleep(3)
        after = self.tabs(bundle)
        name = 'safari' if bundle == SAFARI else 'chrome'
        if after != before:
            bar = self.g(f'python3 {shlex.quote(self.li.helper)} tab-bar --bundle {bundle} || true', 120)
            (self.out / f'{name}-tab-bar.json').write_text(bar + '\n')
        row = {'default': default, 'tabs_before': before, 'tabs_after': after, 'max_tabs_during_turn': observed['max_tabs'],
               'watch_polls': observed['polls'], 'process_chain': observed['chain'], 'turn': record.get('turn'),
               'turn_state': record.get('state'), 'turn_seconds': record.get('seconds'), 'reply': record.get('reply', '')[:400],
               'revoked_at': marks.get('revoked_at'), 'accessibility_during_turn': marks.get('accessibility_during')}
        self.rounds[name] = row
        (self.out / f'{name}-round.json').write_text(json.dumps(row, indent=2) + '\n')
        problems = []
        during = marks.get('accessibility_during') or {}
        if sorted(during.get('system', []) + during.get('user', [])) != ['com.richos.app']:
            problems.append(f'Accessibility was not on com.richos.app alone during the turn: {during}')
        chain = observed['chain']
        if not self.under_app(chain):
            problems.append(f'the tool process is not a child of claude under the app (pid {self.app_pid}): {chain}')
        if record.get('state') != 'completed':
            problems.append(f'the turn ended {record.get("state")}')
        if observed['max_tabs'] <= sum(map(len, before)):
            problems.append('no tab more was ever seen during the turn: the tool opened nothing')
        if after != before:
            problems.append(f'the tabs after the turn are not the user\'s own as before: {before} -> {after}')
        if problems:
            raise StepFailed(f'{name}: ' + '; '.join(problems))
        return row

    def safari(self):
        return self.round(SAFARI)

    def chrome_read(self):
        return self.round(CHROME)

    def control(self):
        rows = self.set_aside("service='kTCCServiceAccessibility'", 'all')
        probe = ("python3 -c 'import ctypes; print(ctypes.cdll.LoadLibrary(\"/System/Library/Frameworks/"
                 "ApplicationServices.framework/ApplicationServices\").AXIsProcessTrusted())'")
        try:
            # What else could grant it: a plain Python from the same ssh session, the MDM
            # overrides TCC reads beside its database, and SIP.
            seen = {'python_from_ssh_trusted': self.g(probe + ' 2>&1 || true'),
                    'mdm_overrides': self.g('sudo -n test -f "/Library/Application Support/com.apple.TCC/'
                                            'MDMOverrides.plist" && echo present || echo absent'),
                    'csrutil': self.g('csrutil status 2>&1 || true')}
            if seen['python_from_ssh_trusted'].strip() == '1' and not (rows['system'] or rows['user']):
                # Measured in walk-4a262a1b1e63 (2026-11-10): this image (SIP disabled) answers
                # AXIsProcessTrusted true to a plain Python with no Accessibility row at all and
                # no MDM override, so it cannot show the permission is what lets the tool read.
                raise NotProvable('this guest grants Accessibility with no row at all (a plain Python from ssh is '
                                  f'trusted; {seen}), so whether RichOS\'s row is needed cannot be shown here. The rounds '
                                  'show the tool started by the app works with RichOS\'s row the only one in TCC.')
            refused = self.direct_call()
        finally:
            self.put_back('all')
        problems = []
        if rows['system'] or rows['user']:
            problems.append(f'Accessibility rows were left: {rows}')
        if not refused.get('isError') or 'Accessibility' not in json.dumps(refused):
            problems.append(f'with no Accessibility row at all the tool was not refused: {json.dumps(refused)[:300]}; '
                            f'seen: {json.dumps(seen)}')
        if problems:
            raise StepFailed('; '.join(problems))
        return {'accessibility_rows': rows, 'answer': refused['content'][0]['text'], 'seen': seen, 'after': self.ax_clients()}

    def pull_step(self):
        self.pull()
        return self.judge_transcripts()

    def judge_transcripts(self):
        """Rich's read_profile calls in Claude Code's own transcripts: when each was made, and
        whether it returned the stand-in's hidden text."""
        calls, results = [], {}
        for f in sorted((self.out / 'native-projects').rglob('*.jsonl')):
            for raw in f.read_text(errors='replace').splitlines():
                try:
                    row = json.loads(raw)
                except ValueError:
                    continue
                content = (row.get('message') or {}).get('content')
                if not isinstance(content, list):
                    continue
                for block in content:
                    if block.get('type') == 'tool_use' and block.get('name') == TOOL:
                        # A call is written more than once (walk-59ad5d5270c7: once without a time).
                        if row.get('timestamp') and not any(c['id'] == block.get('id') for c in calls):
                            calls.append({'id': block.get('id'), 'at': row.get('timestamp'), 'input': block.get('input')})
                    if block.get('type') == 'tool_result':
                        body = block.get('content')
                        text = body if isinstance(body, str) else ''.join(
                            b.get('text', '') for b in body or [] if isinstance(b, dict))
                        results[block.get('tool_use_id')] = {'is_error': bool(block.get('is_error')), 'text': text}
        expect = self.li.manifest['expect']
        markers = [expect['profile_hidden'], expect['details_hidden']] + [
            expect['companies'][self.li.manifest['about'].get(k, {}).get('vanity', k)] for k in self.li.manifest['experience_keys']]
        judged = []
        for call in calls:
            result = results.get(call['id'], {'is_error': True, 'text': ''})
            try:
                browser = json.loads(result['text']).get('browser')
            except ValueError:
                browser = None
            epoch = calendar.timegm(time.strptime(call['at'][:19], '%Y-%m-%dT%H:%M:%S'))
            judged.append({'at': call['at'], 'epoch': epoch, 'browser': browser, 'is_error': result['is_error'],
                           'markers_back': sum(m in result['text'] for m in markers), 'markers': len(markers),
                           'chars': len(result['text'])})
        problems = []
        for name, label in (('safari', 'Safari'), ('chrome', 'Google Chrome')):
            mine = [j for j in judged if j['browser'] == label]
            revoked = (self.rounds.get(name) or {}).get('revoked_at')
            if not mine:
                problems.append(f'no read_profile call answered from {label}')
            elif mine[0]['is_error'] or mine[0]['markers_back'] != mine[0]['markers']:
                problems.append(f'{label}: the call did not return every hidden text: {mine[0]}')
            elif revoked and mine[0]['epoch'] and mine[0]['epoch'] + 1 < revoked:
                problems.append(f'{label}: the call was made before the other grants were deleted')
        report = {'calls': judged}
        if problems:
            raise StepFailed('; '.join(problems) + ' ' + json.dumps(report))
        return report


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--site', type=Path, required=True, help='the stand-in site linkedin-standin.py built')
    p.add_argument('--expect-sha', default='')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok, provable = True, True
    for step in steps:
        if not ok and step != 'pull':
            continue
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = (walk.pull_step if step == 'pull' else getattr(walk, step.replace('-', '_')))()
            row['outcome'] = 'PASS'
        except NotProvable as exc:
            row['outcome'] = 'NOT PROVABLE HERE'
            row['detail'] = str(exc)[:4000]
            provable = False
        except (StepFailed, profile_walk.StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)[:4000]
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" ({row['seconds']} s)" if row['outcome'] == 'PASS' else f" — {row.get('detail')}"),
              flush=True)
    # 3: nothing failed, and a step could not be shown on this guest (said in its row).
    return 1 if not ok else 0 if provable else 3


if __name__ == '__main__':
    sys.exit(main())

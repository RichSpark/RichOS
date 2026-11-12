#!/usr/bin/env python3
"""conversation-walk.py - a fresh app's first setup, then a given conversation with Rich, pulled back.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest, runs this and
removes the clone (run-walk.py passes the owned VM name as the first argument). Do NOT pass
--new-user to run-walk.py: that flag skips copying the engine payload in (run.sh), and the engine
is how a branch's instruction files (rich.md, the bootstrap-interview skill) reach the app.

  # 1. The engine of the branch under test. Engine 1.2.0 needs a verified runtime; the nightly
  #    keeps them read-only at ~/.richos-nightly/runtime-<recipe sha256[:12]>. The one the
  #    nightly uses for THIS checkout's recipe:
  #      RUNTIME=$(python3 richos/app/scripts/lib/runtime_cache.py path ~/.richos-nightly \\
  #                        richos/app/scripts/runtime-sources.json)
  richos/app/scripts/make-engine-asset.sh --out ENGINE_DIR --runtime-dir "$RUNTIME"
  #    -> ENGINE_DIR/richos-engine-1.2.0.tar.gz (check: it lists engine/app/agents/rich.md)

  # 2. The conversation (see below), and one run. The app is any built bundle, e.g. the latest
  #    nightly's ~/.richos-nightly/releases/<tag>/RichOS-<version>-macos-aarch64.zip
  richos/app/scripts/testvm/run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TAR \\
      --report REPORT.json --wait 600 -- \\
      richos/app/scripts/testvm/conversation-walk.py --conversation conversation.json \\
      --out OUT_DIR [--within 300]
  #    (the walk is given as a path: run-walk.py refuses a bare name that is not on PATH)

  #    With --site SITE (a stand-in LinkedIn built by linkedin-standin.py from saved pages in
  #    scratch, never committed; see linkedin-app-walk.py) the guest's default browser (Safari)
  #    is also signed in to the stand-in before the conversation, as linkedin-profile-walk.py's
  #    steps do it, and RichOS is given Accessibility as linkedin-app-walk.py's `tcc` step does
  #    (without that row the guest refuses the tool), so Rich's read_profile reads it. The profile is
  #    http://127.0.0.1:18766/in/SLUG/ (SLUG is linkedin-standin.py's --slug; the walk prints it).
  #    Add `--site SITE` after conversation-walk.py. Without it, nothing LinkedIn is set up.

  #    With --action NAME=TOOL[~TEXT] and --expect NAME=ROLE (native-transcript.py --actors's own
  #    arguments, repeatable), the walk also says WHO performed each declared action from the
  #    pulled transcripts, into OUT_DIR/actors.json, and fails when an expectation fails. The
  #    onboarding reads: --action linkedin=mcp__richos_linkedin__read_profile
  #    --action website=WebFetch~<company host> --expect linkedin=specialist --expect website=specialist

  # 3. Read what Rich did, with the committed readers, unchanged:
  richos/app/scripts/qa/native-transcript.py OUT_DIR/native-projects --text
  richos/app/scripts/qa/ledger-timeline.py OUT_DIR/ledger.jsonl --turns

THE CONVERSATION FILE is a JSON list, one item per step, run in order. An item is a message to
type, `{"say": "Alex"}` (a bare string is the same), or a button to press by its label,
`{"press": "Start the questions"}`. Each step starts only after the previous reply has finished:
no turn is open on the app's ledger. A message must get a reply (its turn ends with text from
Rich) or the walk fails. A press waits --press-grace seconds for a turn to start and, if one does,
for its reply; a press that starts no turn is reported as such and passes, unless the item says
`"expect_reply": true`, which makes a reply required. A step `{"wait_for_rich": 900}` sends
nothing and waits up to that many seconds for the next turn Rich starts by himself (Stu's research
coming back to him); its reply is recorded like any other step's, and no turn within the time fails
the walk. With --site, a message may say {linkedin_profile} and the walk puts the stand-in profile's
address there. A list is checked without a guest, and
run-walk.py checks it before a guest is booted: `conversation-walk.py --check FILE...`.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  setup      first setup as adopt-walk.py does it: the first-run sheets declined ("Not now"), the
             company question answered (--company, a folder that is a repository), and the app's
             first conversation created. The first-run card is left on screen, unpressed: pressing
             it, or not, is the conversation's business
  turns      the conversation file, one step at a time. A message is typed with the accessibility
             paste that focuses and verifies the composer and sent with the Return key; a reply is
             read off the app's own ledger (PromptReceived, AssistantDelta, TurnCompleted)
  pull       whatever happened, even a failed step, is pulled into --out:
               native-projects/   Claude Code's own transcripts (the guest's ~/.claude/projects):
                                  Rich's, Stu's and every worker's
               ledger.jsonl       the app's conversation-ledger.jsonl
               companies/         the guest's ~/myrichos/companies (the company notes)
               app.log, final.png, turns.json (one record per step, with Rich's reply text)
               actors.json        with --action: native-transcript.py --actors --json over
                                  native-projects (who performed each action, and the verdicts)

OUTPUT: one line per step on stdout: `<n> say|press|wait_for_rich "<what>": reply yes|NO|none (<state>), <s> s,
<chars> chars`, then the exit code: 0 only when every step passed.
"""
import argparse
import datetime
import importlib.util
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

RETURN_KEY = '36'
CLOSED = ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed')
ITEM_KEYS = {'say': {'say'}, 'press': {'press', 'expect_reply'}, 'wait_for_rich': {'wait_for_rich'}}
PROFILE_PLACEHOLDER = '{linkedin_profile}'


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def plan_problems(path):
    """Every reason the conversation file cannot be walked, as sentences; [] when it can."""
    try:
        plan = json.loads(Path(path).read_text())
    except (OSError, ValueError) as exc:
        return ['%s: cannot be read as JSON (%s)' % (path, exc)]
    if not isinstance(plan, list) or not plan:
        return ['%s: must be a non-empty JSON list of steps' % path]
    bad = []
    for n, item in enumerate(plan, 1):
        if isinstance(item, str):
            item = {'say': item}
        if not isinstance(item, dict):
            bad.append('step %d: must be a string or an object with "say" or "press"' % n)
            continue
        kinds = [k for k in ITEM_KEYS if k in item]
        if len(kinds) != 1:
            bad.append('step %d: needs exactly one of "say" or "press"' % n)
            continue
        extra = sorted(set(item) - ITEM_KEYS[kinds[0]])
        if extra:
            bad.append('step %d: unknown key(s) %s for "%s"' % (n, ', '.join(extra), kinds[0]))
        if kinds[0] == 'wait_for_rich':
            if isinstance(item[kinds[0]], bool) or not isinstance(item[kinds[0]], (int, float)) or item[kinds[0]] <= 0:
                bad.append('step %d: "wait_for_rich" must be a number of seconds above 0' % n)
        elif not isinstance(item[kinds[0]], str) or not item[kinds[0]].strip():
            bad.append('step %d: "%s" must be a non-empty string' % (n, kinds[0]))
        if 'expect_reply' in item and not isinstance(item['expect_reply'], bool):
            bad.append('step %d: "expect_reply" must be true or false' % n)
    return bad


def steps_of(path):
    """The normalized steps: [{'kind': 'say'|'press', 'text': str, 'expect_reply': bool}]."""
    out = []
    for item in json.loads(Path(path).read_text()):
        if isinstance(item, str):
            item = {'say': item}
        kind = next(k for k in ITEM_KEYS if k in item)
        out.append({'kind': kind, 'text': item[kind], 'expect_reply': kind != 'press' or bool(item.get('expect_reply'))})
    return out


def find_turn(rows, kind, text, after_ms):
    """The id of the first turn that started after AFTER_MS and, for a message, carried TEXT; None
    until there is one. Replays (an overload retry gets the original's prompt) are followed by
    qa/wait-turn.py's own turn_states."""
    for tid, t in turn_states(rows).items():
        if not t['start'] > after_ms:
            continue
        if kind == 'say' and t['prompt'].strip() != text.strip():
            continue
        # Rich's own turn is started by a delivered prompt (Stu's report). An app notice the
        # ledger files under its own turn id (a ProactiveMessage, such as the weekly quota
        # digest) has no prompt and never closes: waiting on it failed the 2026-11-10 walk.
        if kind == 'wait_for_rich' and not t['prompt'].strip():
            continue
        return tid
    return None


def notice_turns(rows):
    """Turn ids that are only an app notice: a ProactiveMessage and no delivered prompt."""
    notices, prompted = set(), set()
    for r in rows:
        if r.get('event') == 'ProactiveMessage':
            notices.add(r.get('turn_id'))
        elif r.get('event') == 'PromptReceived':
            prompted.add(r.get('turn_id'))
    return notices - prompted


def turn_record(rows, tid):
    """What the ledger says about turn TID: state (completed|interrupted|stopped|failed|open),
    Rich's reply text, and the time its closing row was written (guest ms)."""
    reply, state, closed_at = [], 'open', None
    for r in rows:
        if r.get('turn_id') != tid:
            continue
        event = r.get('event')
        if event == 'AssistantDelta':
            reply.append(str(r.get('text') or ''))
        elif event in CLOSED and state == 'open':
            state, closed_at = event[len('Turn'):].lower(), r.get('at')
        elif event == 'TurnReplayed':
            state, closed_at = 'open', None
    return {'state': state, 'reply': ''.join(reply), 'closed_at_ms': closed_at}


def verdict(step, record):
    """None when the step did what its item asked, else the sentence saying what did not."""
    if record.get('turn') is None:
        if step['expect_reply']:
            return 'no turn started within %g s of the %s' % (record['waited_s'], step['kind'])
        return None
    if record['state'] != 'completed':
        return 'the turn ended %s, not completed' % record['state']
    if not record['reply'].strip():
        return 'the turn completed with no text from Rich'
    return None


def line(n, step, record):
    got = 'none (no turn started)' if record.get('turn') is None else \
        '%s (%s)' % ('yes' if record['reply'].strip() else 'NO', record['state'])
    secs = '-' if record.get('seconds') is None else '%.1f' % record['seconds']
    return '%d %s %s: reply %s, %s s, %d chars' % (n, step['kind'], json.dumps(step['text']), got, secs, len(record.get('reply', '')))


adopt_walk = load('adopt_walk', HERE / 'adopt-walk.py')
turn_states = load('wait_turn', HERE.parent / 'qa' / 'wait-turn.py').turn_states
StepFailed = adopt_walk.StepFailed


class ConversationWalk(adopt_walk.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.company = self.home + '/' + a.company
        self.steps = steps_of(a.conversation)
        self.records = []
        self.began_ms = None

    def ledger_rows(self):
        text = adopt_walk.guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        rows = []
        for raw in text.splitlines():
            try:
                rows.append(json.loads(raw))
            except ValueError:
                pass
        return rows

    def setup(self):
        """adopt-walk.py's first run, up to the app's first conversation, and no further: its
        Not now on the business-questions card is left out, because that card is the thing a
        conversation may press."""
        self.decline_first_run_sheets()
        adopt_walk.guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "notes\\n" > README.md '
                                  '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                                  'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        self.wait_for('Add this company')
        self.type_into(self.a.company, '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        end = time.monotonic() + 60
        while time.monotonic() < end:
            created = [r for r in self.ledger_rows() if r.get('event') == 'ThreadCreated']
            if created:
                return {'thread': created[0].get('thread_id'), 'title': created[0].get('title')}
            time.sleep(1)
        raise StepFailed('no conversation was created after the company was added')

    def prepare_site(self):
        """--site: the guest's default browser signed in to the stand-in LinkedIn, by
        linkedin-profile-walk.py's own steps (loaded, never copied): its `site` step serves SITE in
        the guest; its `default` makes Safari the default browser; and the browser's sign-in is the
        stand-in's /sign-in, opened as that walk's `judged` does. Returns the profile's address."""
        profile_walk = load('linkedin_profile_walk', HERE / 'linkedin-profile-walk.py')
        li = profile_walk.Walk(argparse.Namespace(vm=self.vm, out=self.out, site=self.a.site, exe=Path('/dev/null')))
        try:
            li.site()
            li.default(profile_walk.SAFARI)
        except profile_walk.StepFailed as exc:
            raise StepFailed('the stand-in site: %s' % str(exc)[:500])
        if getattr(self.a, "no_accessibility", False):
            print("Accessibility: not granted to RichOS (--no-accessibility); its rows: %s"
                  % (self.accessibility_rows() or 'none'), flush=True)
        else:
            self.grant_accessibility()
        adopt_walk.guest(self.vm, 'open -g -b %s %s/sign-in' % (profile_walk.SAFARI, li.base), 60)
        time.sleep(4)
        self.allow_local_network()
        return li.profile

    def grant_accessibility(self):
        """Accessibility for com.richos.app in the guest's TCC database, by linkedin-app-walk.py's
        own `tcc` step (loaded, never copied; its methods run on a stand-in carrying the three it
        uses). Needed on this guest image: without the row the app's read_profile answered that the
        Mac had not given RichOS Accessibility (walk-63b0ad52b8e4)."""
        import types
        app_walk = load('linkedin_app_walk', HERE / 'linkedin-app-walk.py')
        shim = types.SimpleNamespace(g=lambda cmd, timeout=60: adopt_walk.guest(self.vm, cmd, timeout))
        shim.flush_tcc = lambda: app_walk.Walk.flush_tcc(shim)
        shim.ax_clients = lambda: app_walk.Walk.ax_clients(shim)
        app_walk.Walk.tcc(shim)

    def accessibility_rows(self):
        """com.richos.app's Accessibility rows in the guest's system TCC database, by
        linkedin-app-walk.py's own query helper; '' when there are none."""
        app_walk = load('linkedin_app_walk', HERE / 'linkedin-app-walk.py')
        query = app_walk.sql(app_walk.SYS_DB, "select client, auth_value from access where "
                             "service = 'kTCCServiceAccessibility' and client = 'com.richos.app'", True)
        return adopt_walk.guest(self.vm, query + ' 2>&1 || true', 60).strip()

    def allow_local_network(self):
        """macOS asks 'Allow "Python" to find devices on local networks?' once the stand-in's server
        is used, a sheet that takes the keyboard from the composer (walk-62ef6dacabd8: the next
        message was refused focusfailed). Allowed when it is there, in the guest only, so the
        conversation can type. Best effort: no sheet is not a failure."""
        script = ('tell application "System Events" to tell process "UserNotificationCenter" to '
                  'click button "Allow" of window 1')
        for _ in range(5):
            r = subprocess.run([str(HERE / 'ax.sh'), self.vm, script], capture_output=True, text=True, timeout=60)
            if not r.returncode:
                print('local-network sheet allowed', flush=True)
                return
            time.sleep(2)
        print('no local-network sheet to allow: %s' % (r.stderr or r.stdout).strip()[-200:], file=sys.stderr)

    def wait_idle(self, seconds):
        """Until no turn on the ledger is open: the previous reply has finished. An app notice
        (a ProactiveMessage under its own turn id, never closed) is not a turn anyone is waiting
        on: counting it blocked every later step (review rv-20261110T222955Z-e9dfb1b6-c63e #2)."""
        end = time.monotonic() + seconds
        while True:
            rows = self.ledger_rows()
            notices = notice_turns(rows)
            open_turns = [tid for tid, t in turn_states(rows).items() if t['open'] and tid not in notices]
            if not open_turns:
                return
            if time.monotonic() >= end:
                raise StepFailed('a turn was still open %g s later: %s' % (seconds, ', '.join(t[-6:] for t in open_turns)))
            time.sleep(2)

    def act(self, step):
        if step['kind'] == 'wait_for_rich':
            return  # sends nothing: the next turn is Rich's own
        if step['kind'] == 'say':
            # ax.sh `type` focuses the composer and verifies it before anything is sent.
            self.type_into(step['text'], '--role', 'AXTextArea', '--title', 'Message to Rich')
            adopt_walk.command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY], 60)
        else:
            self.wait_for(step['text'], seconds=30)
            self.press(step['text'])

    def run_step(self, n, step):
        self.wait_idle(self.a.within)
        sent = self.clock()
        self.act(step)
        grace = float(step['text']) if step['kind'] == 'wait_for_rich' else \
            self.a.within if step['expect_reply'] else self.a.press_grace
        begun = time.monotonic()
        tid, record = None, {'waited_s': grace}
        while time.monotonic() - begun < grace:
            tid = find_turn(self.ledger_rows(), step['kind'], step['text'], sent)
            if tid:
                break
            time.sleep(2)
        if tid:
            # The reply budget starts when the turn does: a wait_for_rich turn that begins late in
            # its wait still gets the whole --within to finish.
            end = (time.monotonic() if step['kind'] == 'wait_for_rich' else begun) + self.a.within
            while True:
                rows = self.ledger_rows()
                record = turn_record(rows, tid)
                if record['state'] != 'open' or time.monotonic() >= end:
                    break
                time.sleep(2)
            record.update(turn=tid, waited_s=grace,
                          seconds=((record['closed_at_ms'] - sent) / 1000) if record['closed_at_ms'] else None)
        else:
            record.update(turn=None, state='none', reply='', seconds=None)
        record.update(n=n, kind=step['kind'], text=step['text'], sent_at_ms=sent)
        return record

    def pull(self):
        """Everything the run left, pulled even when a step failed. Each is best effort and says
        so on stderr, so a missing piece is never mistaken for an empty one."""
        native = self.out / 'native-projects'
        shutil.rmtree(native, ignore_errors=True)  # scp -r nests into a directory that exists
        for remote, local in ((self.home + '/.claude/projects', native),
                              (self.data + '/conversation-ledger.jsonl', self.out / 'ledger.jsonl'),
                              (self.payload + '/app.log', self.out / 'app.log'),
                              (self.home + '/myrichos/companies', self.out / 'companies')):
            if remote.endswith('/myrichos/companies') and \
                    adopt_walk.guest(self.vm, 'test -d %s && echo yes || echo no' % shlex.quote(remote), 60).strip() != 'yes':
                continue  # Rich saved no company notes: nothing to pull, nothing to say
            r = subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', remote, str(local)],
                               capture_output=True, text=True, timeout=300)
            if r.returncode:
                print('pull of %s failed: %s' % (remote, (r.stderr or r.stdout).strip()[-200:]), file=sys.stderr)
        try:
            self.shot('final.png')
        except (StepFailed, subprocess.TimeoutExpired) as exc:
            print('final screenshot failed: %s' % str(exc)[-200:], file=sys.stderr)


def iso_utc(ms):
    """Guest milliseconds as the transcript's own stamp: 2026-11-10T10:00:00.123Z."""
    return datetime.datetime.fromtimestamp(ms / 1000, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z'


def actors(a, since_ms=None):
    """Who performed each declared action, from the pulled transcripts, by the committed reader.
    Saved as actors.json; its PASS/FAIL lines are printed. True when every expectation passed.
    SINCE_MS (the guest clock just before the walk's first step) keeps only calls this walk's conversation made:
    an earlier read in the pulled history never answers for it."""
    args = [sys.executable, str(HERE.parent / 'qa' / 'native-transcript.py'), str(a.out / 'native-projects'), '--actors', '--json']
    if since_ms is not None:
        args += ['--since', iso_utc(since_ms)]
    for spec in a.action:
        args += ['--action', spec]
    for spec in a.expect:
        args += ['--expect', spec]
    r = subprocess.run(args, capture_output=True, text=True, timeout=300)
    if r.returncode not in (0, 1) or not r.stdout.strip():
        print('actors: the transcript reader could not judge (%d): %s' % (r.returncode, (r.stderr or r.stdout).strip()[-300:]), flush=True)
        return False
    (a.out / 'actors.json').write_text(r.stdout)
    for name, v in json.loads(r.stdout)['verdicts'].items():
        print('actor %s: %s (%s)' % (name, v['verdict'], v['why']), flush=True)
    return r.returncode == 0


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--conversation', type=Path)
    p.add_argument('--out', type=Path)
    p.add_argument('--company', default='Acme', help='the company the first run adds (default Acme)')
    p.add_argument('--within', type=float, default=300, help='seconds for a reply, and for the previous reply to finish (default 300)')
    p.add_argument('--press-grace', type=float, default=20, help='seconds a press waits for a turn to start (default 20)')
    p.add_argument('--site', type=Path, help='the stand-in LinkedIn site linkedin-standin.py built: served in the guest, the '
                   "default browser (Safari) signed in to it before the conversation, so Rich's LinkedIn tool can read it")
    p.add_argument('--no-accessibility', action='store_true', help='with --site: leave RichOS without the macOS '
                   'Accessibility permission, as on a Mac where it was never allowed (the first-run case); its '
                   'rows in the TCC database are printed, none expected')
    p.add_argument('--check', nargs='+', type=Path, metavar='FILE', help='check conversation files without a guest')
    p.add_argument('--action', action='append', default=[], help='NAME=TOOL[~TEXT]: an action whose actor is reported '
                   '(native-transcript.py --actors)')
    p.add_argument('--expect', action='append', default=[], help='NAME=ROLE[|ROLE]: who may perform it; a failed '
                   'expectation fails the walk')
    a = p.parse_args()
    if a.check:
        found = [sentence for f in a.check for sentence in plan_problems(f)]
        print('\n'.join(found) if found else 'ok')
        return 1 if found else 0
    if not (a.vm and a.conversation and a.out):
        p.error('the VM name (run-walk.py gives it), --conversation and --out are required')
    bad = plan_problems(a.conversation)
    if bad:
        p.error('; '.join(bad))
    walk = ConversationWalk(a)
    if not a.site and any(PROFILE_PLACEHOLDER in st['text'] for st in walk.steps if st['kind'] == 'say'):
        p.error('%s is in a message but there is no --site to name it' % PROFILE_PLACEHOLDER)
    ok = True
    try:
        walk.setup()
        if a.site:
            profile = walk.prepare_site()
            print('stand-in profile: %s' % profile, flush=True)
            for step in walk.steps:
                if step['kind'] == 'say':
                    step['text'] = step['text'].replace(PROFILE_PLACEHOLDER, profile)
        walk.began_ms = walk.clock()  # the actor verdicts cover this conversation from here on
        for n, step in enumerate(walk.steps, 1):
            try:
                record = walk.run_step(n, step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
                record = {'n': n, 'kind': step['kind'], 'text': step['text'], 'turn': None, 'state': 'none',
                          'reply': '', 'seconds': None, 'failure': str(exc)}
                walk.records.append(record)
                print('%d %s %s: FAILED: %s' % (n, step['kind'], json.dumps(step['text']), str(exc)[:300]), flush=True)
                ok = False
                break
            record['failure'] = verdict(step, record)
            walk.records.append(record)
            print(line(n, step, record) + ('' if not record['failure'] else ' - FAILED: ' + record['failure']), flush=True)
            if record['failure']:
                ok = False
                break
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
        print('setup FAILED: %s' % str(exc)[:600], flush=True)
        ok = False
    finally:
        (a.out / 'turns.json').write_text(json.dumps(walk.records, indent=2) + '\n')
        walk.pull()
    if a.action and not actors(a, walk.began_ms):
        ok = False
    print('exit %d' % (0 if ok else 1), flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

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

  # 3. Read what Rich did, with the committed readers, unchanged:
  richos/app/scripts/qa/native-transcript.py OUT_DIR/native-projects --text
  richos/app/scripts/qa/ledger-timeline.py OUT_DIR/ledger.jsonl --turns

THE CONVERSATION FILE is a JSON list, one item per step, run in order. An item is a message to
type, `{"say": "Alex"}` (a bare string is the same), or a button to press by its label,
`{"press": "Start the questions"}`. Each step starts only after the previous reply has finished:
no turn is open on the app's ledger. A message must get a reply (its turn ends with text from
Rich) or the walk fails. A press waits --press-grace seconds for a turn to start and, if one does,
for its reply; a press that starts no turn is reported as such and passes, unless the item says
`"expect_reply": true`, which makes a reply required. A list is checked without a guest, and
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
               app.log, final.png, turns.json (one record per step, with Rich's reply text)

OUTPUT: one line per step on stdout: `<n> say|press "<what>": reply yes|NO|none (<state>), <s> s,
<chars> chars`, then the exit code: 0 only when every step passed.
"""
import argparse
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
ITEM_KEYS = {'say': {'say'}, 'press': {'press', 'expect_reply'}}


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
        if not isinstance(item[kinds[0]], str) or not item[kinds[0]].strip():
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
        kind = 'say' if 'say' in item else 'press'
        out.append({'kind': kind, 'text': item[kind], 'expect_reply': True if kind == 'say' else bool(item.get('expect_reply'))})
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
        return tid
    return None


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

    def wait_idle(self, seconds):
        """Until no turn on the ledger is open: the previous reply has finished."""
        end = time.monotonic() + seconds
        while True:
            open_turns = [tid for tid, t in turn_states(self.ledger_rows()).items() if t['open']]
            if not open_turns:
                return
            if time.monotonic() >= end:
                raise StepFailed('a turn was still open %g s later: %s' % (seconds, ', '.join(t[-6:] for t in open_turns)))
            time.sleep(2)

    def act(self, step):
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
        grace = self.a.within if step['expect_reply'] else self.a.press_grace
        begun = time.monotonic()
        tid, record = None, {'waited_s': grace}
        while time.monotonic() - begun < grace:
            tid = find_turn(self.ledger_rows(), step['kind'], step['text'], sent)
            if tid:
                break
            time.sleep(2)
        if tid:
            end = begun + self.a.within
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
                              (self.payload + '/app.log', self.out / 'app.log')):
            r = subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', remote, str(local)],
                               capture_output=True, text=True, timeout=300)
            if r.returncode:
                print('pull of %s failed: %s' % (remote, (r.stderr or r.stdout).strip()[-200:]), file=sys.stderr)
        try:
            self.shot('final.png')
        except (StepFailed, subprocess.TimeoutExpired) as exc:
            print('final screenshot failed: %s' % str(exc)[-200:], file=sys.stderr)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--conversation', type=Path)
    p.add_argument('--out', type=Path)
    p.add_argument('--company', default='Acme', help='the company the first run adds (default Acme)')
    p.add_argument('--within', type=float, default=300, help='seconds for a reply, and for the previous reply to finish (default 300)')
    p.add_argument('--press-grace', type=float, default=20, help='seconds a press waits for a turn to start (default 20)')
    p.add_argument('--check', nargs='+', type=Path, metavar='FILE', help='check conversation files without a guest')
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
    ok = True
    try:
        walk.setup()
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
    print('exit %d' % (0 if ok else 1), flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

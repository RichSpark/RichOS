#!/usr/bin/env python3
"""keep-alive-walk.py - on the real app: the context meter reads a chat's real context size, and
a kept chat's session gets one keep-alive message that nobody sees.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest, runs this and
removes the clone (run-walk.py passes the owned VM name as the first argument):

  richos/app/scripts/testvm/run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TAR \\
      --report REPORT.json --wait 600 -- \\
      richos/app/scripts/testvm/keep-alive-walk.py --out OUT_DIR [--real-idle]

The bundle and the engine are this branch's (branch-bundle.sh, make-engine-asset.sh; see
conversation-walk.py's header): the engine's hook is part of what is proven.

THE QUESTIONS (CEO, 2026-11-16, round 83; richos-hq
docs/research/2026-11-16-cache-heartbeat-what-can-be-done.md, "What was not tested"):
  1. Does the context meter show each conversation's real context size?
  2. A keep-alive on a session that carries Rich's own instructions, under the real engine's
     UserPromptSubmit and Stop handlers: is it answered with one word and nothing else, is it
     kept off the screen and out of the conversation, and does the chat carry on after it?

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65), and nothing plays:
  setup      conversation-walk.py's first run: the company Acme and its first conversation
  talk       one message to Rich, settled (TurnCompleted)
  meter      Settings > Context meter is switched on. PASS needs the meter's own words in the
             window ("Context window. Rich, talking with you: N%, Xk of Yk ...") to be the figures
             chat-readings.json holds for this conversation, and those to be the context size of
             the LAST request in Rich's own transcript (input + cache read + cache creation of its
             last assistant message), on a session that wrote under the one-hour cache
  keep       the conversation's Keep chat alive box is checked: navigation.json names it
  idle       the session goes 55 minutes without a request. By default that is SEEDED: the app is
             quit by its recorded pid, chat-readings.json's time of Rich's last request is moved
             56 minutes back, and the app is started again (the clock is the wall clock, read from
             that file at launch, so this is the app's own decision on a real reading; the
             provider's cache itself is still warm). With --real-idle nothing is touched and the
             walk waits out the 55 minutes with the app running
  keepalive  within --within seconds of the look that follows: the app's log says Rich's session
             of this conversation answered its keep-alive, and Rich's transcript holds the message
             (it opens with <richos-keep-alive>) answered by ONE assistant message with no tool
             call and nothing after it (a judged turn end would have fed the turn back). How its
             request was paid for (read from the cache, or written again) is reported; with
             --real-idle at least half of it must have been read: the cache was still there
             (a lapsed cache reads nothing; a session's second request writes about a sixth
             again whenever it is sent)
  unseen     the conversation's ledger has no event the keep-alive added. The walk goes in by the
             home screen's own door (a relaunch opens there) and opens the conversation from the
             rail: it is on screen with its box still checked, and the window says nothing of the
             keep-alive: no "keep-alive", no "keep this chat ready"
  after      one more message is answered on the same session, the meter's figure for Rich is the
             size of that reply's request, and at least 90% of that request was read from the
             cache. With --real-idle the message is sent 62 minutes after the request the idle
             time was counted from: past the hour the cache would have lapsed in without the
             keep-alive, which is the thing Keep chat alive is for
  pull       whatever happened: Claude Code's transcripts, the ledger, the app's log(s),
             chat-readings.json and navigation.json, a last screenshot, and report.json

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

TAG = '<richos-keep-alive>'
SIDES = ('input_tokens', 'cache_read_input_tokens', 'cache_creation_input_tokens')
SIDE = r'(?:(\d+)%, (\d+)k of (\d+)k|not read yet)'
METER = re.compile(r'Context window\. Rich, talking with you: ' + SIDE + r'\. Rich, directing your team: ' + SIDE + r'\.')
SEEDED_IDLE_MINUTES = 56
STEPS = ['setup', 'talk', 'meter', 'keep', 'idle', 'keepalive', 'unseen', 'after']


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def meter_words(text):
    """The meter's figures as the window says them: {'rich': (percent, k, of_k), 'team': ...},
    a side being None when the window says it is not read yet; None when the window does not
    say them at all."""
    found = METER.search(text or '')
    if not found:
        return None
    n = found.groups()

    def side(three):
        return None if three[0] is None else tuple(int(g) for g in three)
    return {'rich': side(n[:3]), 'team': side(n[3:])}


def words_for(used, size):
    """What the meter says for a reading: (percent, thousands used, thousands in the window),
    rounded as the window rounds them (a half goes up)."""
    def rounded(x):
        return int(x + 0.5)
    return (rounded(used / size * 100), rounded(used / 1000), rounded(size / 1000))


def transcript_rows(text):
    rows = []
    for raw in text.splitlines():
        try:
            rows.append(json.loads(raw))
        except ValueError:
            pass
    return rows


def last_request_usage(rows):
    """How the lead's last model request was paid for: {'read': cache read, 'written': cache
    creation, 'plain': input} tokens. None when the transcript holds no request."""
    for row in reversed(rows):
        usage = (row.get('message') or {}).get('usage')
        if row.get('type') == 'assistant' and not row.get('isSidechain') and isinstance(usage, dict):
            return {'read': int(usage.get('cache_read_input_tokens') or 0),
                    'written': int(usage.get('cache_creation_input_tokens') or 0),
                    'plain': int(usage.get('input_tokens') or 0)}
    return None


def read_share(usage):
    """The share of a request's context that came from the cache, 0 to 1."""
    total = usage['read'] + usage['written'] + usage['plain']
    return usage['read'] / total if total else 0.0


def text_of(message):
    content = (message or {}).get('content')
    if isinstance(content, str):
        return content
    return ''.join(part.get('text', '') for part in content or [] if isinstance(part, dict) and part.get('type') == 'text')


def is_user_text(row):
    """A message somebody gave the session (not a tool's result, not a helper's)."""
    if row.get('type') != 'user' or row.get('isSidechain'):
        return False
    content = (row.get('message') or {}).get('content')
    if isinstance(content, list) and any(isinstance(p, dict) and p.get('type') == 'tool_result' for p in content):
        return False
    return bool(text_of(row.get('message')))


def last_request_size(rows):
    """The context size of the lead's last model request in a transcript: input + cache read +
    cache creation of its last assistant message. None when it holds none."""
    for row in reversed(rows):
        usage = (row.get('message') or {}).get('usage')
        if row.get('type') == 'assistant' and not row.get('isSidechain') and isinstance(usage, dict):
            return sum(int(usage.get(k) or 0) for k in SIDES)
    return None


def keep_alives(rows):
    """Every keep-alive message in a transcript with what followed it, up to the next message
    somebody gave the session: [{'replies': [text...], 'tool_calls': n, 'other': n}]."""
    found, current = [], None
    for row in rows:
        if is_user_text(row):
            current = None
            if text_of(row.get('message')).startswith(TAG):
                current = {'replies': [], 'tool_calls': 0, 'other': 0}
                found.append(current)
            continue
        if current is None or row.get('isSidechain'):
            continue
        if row.get('type') == 'assistant':
            content = (row.get('message') or {}).get('content') or []
            current['tool_calls'] += sum(1 for p in content if isinstance(p, dict) and p.get('type') == 'tool_use')
            said = text_of(row.get('message')).strip()
            if said:
                current['replies'].append(said)
        elif row.get('type') == 'user':
            current['other'] += 1  # a tool's result, or a turn end fed back to the model
    return found


def keep_alive_problems(exchange):
    """Every way one keep-alive was more than one word and nothing else; [] when it was that."""
    bad = []
    if exchange['tool_calls']:
        bad.append('%d tool call(s) were made in its turn' % exchange['tool_calls'])
    if exchange['other']:
        bad.append('%d more message(s) were fed into its turn (a tool result or a judged turn end)' % exchange['other'])
    if len(exchange['replies']) != 1:
        bad.append('it was answered %d time(s), not once' % len(exchange['replies']))
    elif re.sub(r'[^a-z]', '', exchange['replies'][0].lower()) != 'ok':
        bad.append('its answer was not the single word ok: %r' % exchange['replies'][0][:120])
    return bad


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--out', type=Path)
    p.add_argument('--company', default='Acme')
    p.add_argument('--within', type=float, default=300, help='seconds for a reply (default 300)')
    p.add_argument('--real-idle', action='store_true', help='wait out the 55 idle minutes instead of seeding them')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    if not (a.vm and a.out):
        p.error('the VM name (run-walk.py gives it) and --out are required')
    conversation_walk = load('conversation_walk', HERE / 'conversation-walk.py')
    adopt_walk = conversation_walk.adopt_walk
    relaunch = load('relaunch', HERE / 'relaunch.py')
    StepFailed = adopt_walk.StepFailed
    guest = adopt_walk.guest

    class Walk(conversation_walk.ConversationWalk):
        def __init__(self, a):
            a.conversation, a.memory = None, False
            a.press_grace = 20
            conversation_walk.steps_of = lambda _path: []  # this walk gives its own messages
            super().__init__(a)
            self.thread = None
            self.report = {}

        # --- reading the guest ---------------------------------------------------------------
        def json_file(self, name):
            text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/' + name) + ' 2>/dev/null || true', 60)
            try:
                return json.loads(text)
            except ValueError:
                return None

        def reading(self):
            return ((self.json_file('chat-readings.json') or {}).get(self.thread) or {})

        def window_text(self):
            return adopt_walk.command([HERE / 'ax.sh', self.vm, 'tree'], 90)

        def meter_said(self):
            """The meter's own words (its accessible name), or None while it is not in the chat."""
            try:
                nodes = self.ax('find', '--id', 'ctx-meter', '--first')
            except StepFailed:
                return None
            return meter_words(' '.join(str(n.get(k) or '') for n in nodes for k in ('title', 'desc', 'value', 'help')))

        def rich_session(self):
            """The provider session Rich's turns of this conversation ran on (the ledger's TurnStarted)."""
            sessions = [r.get('session_id') for r in self.ledger_rows()
                        if r.get('event') == 'TurnStarted' and r.get('session_id')]
            if not sessions:
                raise StepFailed('no turn of this conversation has started on a session yet')
            return sessions[-1]

        def rich_transcript(self):
            """Rich's own transcript of this conversation, pulled whole: Claude Code's record of
            the session his turns ran on."""
            session = self.rich_session()
            found = guest(self.vm, 'ls ' + shlex.quote(self.home + '/.claude/projects') + '/*/' + shlex.quote(session + '.jsonl') + ' 2>/dev/null || true', 60)
            if not found.strip():
                raise StepFailed("Rich's transcript %s.jsonl was not found under %s/.claude/projects" % (session, self.home))
            local = self.out / 'rich-transcript.jsonl'
            r = subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', found.splitlines()[0], str(local)], capture_output=True, text=True, timeout=300)
            if r.returncode:
                raise StepFailed("Rich's transcript could not be pulled: " + (r.stderr or r.stdout).strip()[-200:])
            return transcript_rows(local.read_text())

        def app_log(self):
            # `find`, never a shell pattern: with no relaunch there is no relaunch-*.log, and the
            # guest's zsh refuses a whole `cat` over a pattern that matches nothing.
            return guest(self.vm, "find " + shlex.quote(self.payload) + " -maxdepth 1 \\( -name app.log -o -name 'relaunch-*.log' \\) -exec cat {} + 2>/dev/null || true", 60)

        def say(self, n, text):
            record = self.run_step(n, {'kind': 'say', 'text': text, 'expect_reply': True})
            failure = conversation_walk.verdict({'kind': 'say', 'text': text, 'expect_reply': True}, record)
            if failure:
                raise StepFailed('%r got no reply: %s' % (text, failure))
            return record

        def until(self, what, test, seconds, every=3):
            end = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() >= end:
                    raise StepFailed('%s did not happen within %g s' % (what, seconds))
                time.sleep(every)

        # --- steps ---------------------------------------------------------------------------
        def step_setup(self):
            made = self.setup()
            self.thread = made['thread']
            return made

        def step_talk(self):
            record = self.say(1, 'Reply with the single word pong.')
            return {'reply': record.get('reply', '')[:80], 'seconds': record.get('seconds')}

        def step_meter(self):
            self.ax('click', '--id', 'set-btn', '--first')
            self.until('the Context meter switch', lambda: self.present_id('set-ctxmeter'), 30)
            self.ax('click', '--id', 'set-ctxmeter', '--first')
            adopt_walk.command([HERE / 'ax.sh', self.vm, '--key', '53'], 60)  # Escape closes Settings
            said = self.until('the meter in the chat', self.meter_said, 30)
            reading = self.reading().get('rich') or {}
            size = last_request_size(self.rich_transcript())
            fact = {'window': said, 'reading': reading, 'transcript_last_request': size}
            if not reading.get('used') or not reading.get('size'):
                raise StepFailed('chat-readings.json holds no figure for Rich: %s' % json.dumps(fact))
            if said['rich'] != words_for(reading['used'], reading['size']):
                raise StepFailed('the window does not say the reading: %s' % json.dumps(fact))
            if size != reading['used']:
                raise StepFailed("the reading is not the size of Rich's last request: %s" % json.dumps(fact))
            if reading.get('one_hour') is not True:
                raise StepFailed('the session did not write under the one-hour cache: %s' % json.dumps(fact))
            self.shot('meter.png')
            return fact

        def present_id(self, dom_id):
            try:
                return bool(self.ax('find', '--id', dom_id, '--first'))
            except StepFailed:
                return False

        def step_keep(self):
            self.ax('click', '--id', 'kca-box', '--first')
            kept = self.until('the saved choice', lambda: self.thread in ((self.json_file('navigation.json') or {}).get('keep_alive_threads') or []), 20)
            self.shot('kept.png')
            return {'kept': bool(kept)}

        def step_idle(self):
            self.report['events_before'] = len(self.ledger_rows())
            self.report['keep_alives_before'] = len(keep_alives(self.rich_transcript()))
            self.report['idle_from_ms'] = self.reading()['rich']['last_request_ms']
            if self.a.real_idle:
                at = self.report['idle_from_ms']
                wait = max(0, at / 1000 + 55 * 60 - self.clock() / 1000)
                time.sleep(wait)
                return {'waited_s': round(wait)}
            self.wait_idle(self.a.within)
            relaunch.quit_app(self.vm)
            path = shlex.quote(self.data + '/chat-readings.json')
            edit = ('import json,sys,time; p=sys.argv[1]; d=json.load(open(p)); r=d[sys.argv[2]]["rich"]; '
                    'r["last_request_ms"]=int(time.time()*1000)-%d*60*1000; json.dump(d,open(p,"w")); print(r["last_request_ms"])' % SEEDED_IDLE_MINUTES)
            seeded = guest(self.vm, 'python3 -c %s %s %s' % (shlex.quote(edit), path, shlex.quote(self.thread)), 60)
            started = relaunch.relaunch(self.vm)
            return {'seeded_last_request_ms': int(seeded), 'relaunched_pid': started.get('pid')}

        def step_keepalive(self):
            line = "keep chat alive: Rich's session of %s answered" % self.thread
            log = self.until('the keep-alive', lambda: next((l for l in self.app_log().splitlines() if line in l), None), self.a.within + 60)
            found = keep_alives(self.rich_transcript())
            new = found[self.report['keep_alives_before']:]
            if len(new) != 1:
                raise StepFailed("Rich's transcript holds %d new keep-alive message(s), not one" % len(new))
            bad = keep_alive_problems(new[0])
            if bad:
                raise StepFailed('the keep-alive was not one word and nothing else: ' + '; '.join(bad))
            after = self.reading().get('rich') or {}
            paid = last_request_usage(self.rich_transcript())
            fact = {'log': log.strip()[-200:], 'exchange': new[0], 'reading_after': after, 'paid': paid,
                    'read_from_cache': round(read_share(paid), 3)}
            # With --real-idle the session was live and 55 minutes idle: its cache must still be
            # there, so the keep-alive is a read. Half, not nine tenths: a session's SECOND
            # request writes about a sixth of the chat again whenever it is sent (measured
            # 2026-11-16 on Claude Code 2.1.325: 52,027 read and 10,364 written 57 seconds after
            # the first request, 52,027 and 10,390 after 55 idle minutes), and a cache that has
            # lapsed reads nothing at all. (Seeded, the session was resumed in a new process,
            # which is its own question; the figure is reported, not judged.)
            if self.a.real_idle and read_share(paid) < 0.5:
                raise StepFailed('the keep-alive after 55 idle minutes did not read the chat from the cache: %s' % json.dumps(fact))
            return fact

        def step_unseen(self):
            events = len(self.ledger_rows())
            if events != self.report['events_before']:
                added = self.ledger_rows()[self.report['events_before']:]
                raise StepFailed('the ledger gained %d event(s) across the keep-alive: %s' % (len(added), json.dumps(added)[:400]))
            # A relaunch opens on the home screen: go in as he does, by its own door (home.js
            # `home-enter`), which lands on a new chat; then open the kept conversation from the
            # rail, by the title the rail shows it under.
            if self.present_id('home-enter'):
                self.ax('click', '--id', 'home-enter', '--first')
            if not self.present_id('kca-box'):
                title = self.title_of(self.thread)
                self.wait_for(title, seconds=60)
                self.press(title)
            box = self.until('the conversation with its Keep chat alive box', lambda: self.ax_or_none('kca-box'), 60)
            checked = str(box[0].get('value')) in ('1', 'True', 'true')
            if not checked:
                raise StepFailed('the box is not checked after the relaunch: %s' % json.dumps(box[0])[:300])
            window = self.window_text().lower()
            if 'pong' not in window:
                raise StepFailed('the conversation is not what the window shows (its reply "pong" is not there)')
            shown = [w for w in ('keep-alive', 'keep this chat ready', 'richos-keep') if w in window]
            if shown:
                raise StepFailed('the window shows the keep-alive: %s' % shown)
            self.shot('after-keep-alive.png')
            return {'ledger_events': events, 'box_checked_after_relaunch': checked,
                    'window_checked_for': ['keep-alive', 'keep this chat ready', 'richos-keep']}

        def title_of(self, thread):
            """The conversation's title as the rail shows it (nav.rs `display_title`): his rename,
            then Haiku's title, then the title it was created with (idle-session-walk.py's rule)."""
            nav = self.json_file('navigation.json') or {}
            for key in ('renamed_threads', 'auto_titles'):
                shown = (nav.get(key) or {}).get(thread)
                if shown:
                    return shown
            titles = [r.get('title') for r in self.ledger_rows() if r.get('thread_id') == thread and r.get('title')]
            if not titles:
                raise StepFailed('the conversation has no title to find it by in the rail')
            return titles[-1]

        def ax_or_none(self, dom_id):
            try:
                return [n for n in self.ax('find', '--id', dom_id, '--first') if not n.get('meta')] or None
            except StepFailed:
                return None

        def step_after(self):
            waited = 0
            if self.a.real_idle:
                # Past the hour the cache would have lapsed in without the keep-alive: 62 minutes
                # after the request the idle time was counted from.
                waited = max(0, self.report['idle_from_ms'] / 1000 + 62 * 60 - self.clock() / 1000)
                time.sleep(waited)
            record = self.say(2, 'Reply with the single word ping.')
            said = self.until('the meter after the reply', self.meter_said, 30)
            reading = self.reading().get('rich') or {}
            rows = self.rich_transcript()
            size, paid = last_request_size(rows), last_request_usage(rows)
            fact = {'reply': record.get('reply', '')[:80], 'window': said, 'reading': reading, 'transcript_last_request': size,
                    'paid': paid, 'read_from_cache': round(read_share(paid), 3), 'waited_s': round(waited)}
            if size != reading.get('used') or said['rich'] != words_for(reading['used'], reading['size']):
                raise StepFailed('the meter did not follow the reply: %s' % json.dumps(fact))
            # The reply after a keep-alive reads the chat from the cache: with --real-idle that is
            # the whole point, 62 minutes after the last real request.
            if read_share(paid) < 0.9:
                raise StepFailed('the reply after the keep-alive did not read the chat from the cache: %s' % json.dumps(fact))
            return fact

        def pull(self):
            super().pull()
            for name in ('chat-readings.json', 'navigation.json'):
                subprocess.run([str(HERE / 'guest.sh'), self.vm, '--pull', self.data + '/' + name, str(self.out / name)],
                               capture_output=True, text=True, timeout=120)
            logs = self.app_log()
            (self.out / 'app-all.log').write_text(logs)

    walk = Walk(a)
    ok = True
    try:
        for name in [s for s in a.steps.split(',') if s]:
            began = time.monotonic()
            try:
                fact = getattr(walk, 'step_' + name)()
                walk.report[name] = {'pass': True, 'seconds': round(time.monotonic() - began, 1), 'fact': fact}
                print('%s PASS %s' % (name, json.dumps(fact)[:600]), flush=True)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired, KeyError, ValueError) as exc:
                walk.report[name] = {'pass': False, 'seconds': round(time.monotonic() - began, 1), 'failure': str(exc)[:2000]}
                print('%s FAIL %s' % (name, str(exc)[:1200]), flush=True)
                ok = False
                break
    finally:
        try:
            walk.pull()
        except Exception as exc:  # noqa: BLE001 -- the evidence is best effort, the verdict is not
            print('pull failed: %s' % str(exc)[:300], file=sys.stderr)
        (a.out / 'report.json').write_text(json.dumps(walk.report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())

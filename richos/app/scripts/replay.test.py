#!/usr/bin/env python3
"""The replay toolkit as programs: the replay child's command line, the scrubber, the recorder.

Run by replay.test.sh. No live account, no network, no window: the recorder talks to a fake
`claude` written here, and the replay child plays the committed fixtures.
"""
import json
import os
import pathlib
import queue
import re
import subprocess
import sys
import tempfile
import threading
import unittest

APP = pathlib.Path(__file__).resolve().parents[1]
REPLAY = APP / 'scripts/replay'
FIXTURES = APP / 'crates/richos-core/tests/fixtures/replay'
CHILD = os.environ.get('RICHOS_REPLAY_BIN', '')

# A fake `claude` on the stream-json wire: it answers `initialize` WITH an account (which must
# never reach a capture), runs each message as one turn, and reports a background command's end
# the way 2.1.283 does (cap-plain / cap-fold): outside a turn with a platform turn after it, or
# inside a turn whose foreground command keeps it open. Seconds are the command's / 50.
FAKE_CLAUDE = r'''#!/usr/bin/env python3
import json, os, re, sys, threading, time
lock = threading.Lock()
def out(o):
    with lock:
        sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
n = [0]
def task():
    n[0] += 1; return "btask%d" % n[0]
def ended(tid):
    out({"type": "system", "subtype": "task_notification", "task_id": tid, "status": "completed",
         "summary": "Background command completed (exit code 0)"})
def platform_turn():
    out({"type": "assistant", "message": {"content": [{"type": "text", "text": "It finished."}]}})
    out({"type": "result", "subtype": "success", "result": "It finished.", "queued_turn_count": 0})
for line in sys.stdin:
    m = json.loads(line)
    if m.get("type") == "control_request":
        out({"type": "control_response", "response": {"subtype": "success", "request_id": m["request_id"],
             "response": {"account": {"email": "someone@example.com", "organization": "Example"}}}})
        continue
    if m.get("type") != "user":
        continue
    u = m["uuid"]; text = m["message"]["content"][0]["text"]
    out({"type": "command_lifecycle", "command_uuid": u, "state": "started"})
    out({"type": "system", "subtype": "init", "cwd": os.getcwd(), "tools": ["Bash"], "model": "fake",
         "memory_paths": {"auto": "/Users/someone/.claude/memory/"}})
    bg = re.search(r"run exactly this command: sleep (\d+); echo ([\w-]+)", text)
    if bg and "run_in_background" in text:
        tid = task()
        out({"type": "system", "subtype": "task_started", "task_id": tid, "task_type": "local_bash",
             "is_backgrounded": True, "description": bg.group(0)})
        out({"type": "assistant", "message": {"id": "msg_01AAAAAAAAAAAAAAAAAAAAAAAA", "content": [
             {"type": "thinking", "thinking": "", "signature": "c2VjcmV0"}]}})
        seconds = int(bg.group(1)) / 50.0
        if "foreground: sleep 12" in text or os.environ.get("FAKE_FOLD_ALL"):
            time.sleep(seconds); ended(tid)
        else:
            threading.Timer(seconds, lambda tid=tid: (ended(tid), platform_turn())).start()
    out({"type": "assistant", "message": {"content": [{"type": "text", "text": "Done."}]}})
    out({"type": "result", "subtype": "success", "result": "Done.", "queued_turn_count": 0})
    out({"type": "command_lifecycle", "command_uuid": u, "state": "completed"})
'''


def lines_of(path):
    return [json.loads(l) for l in pathlib.Path(path).read_text().splitlines() if l.strip()]


class Scrubber(unittest.TestCase):
    def scrub(self, rows, *extra):
        with tempfile.TemporaryDirectory() as d:
            raw, out = pathlib.Path(d) / 'raw.jsonl', pathlib.Path(d) / 'out.jsonl'
            raw.write_text(''.join(json.dumps(r) + '\n' for r in rows))
            r = subprocess.run([sys.executable, str(REPLAY / 'scrub-capture.py'), str(raw), str(out), *extra],
                               capture_output=True, text=True)
            check = subprocess.run([sys.executable, str(REPLAY / 'scrub-capture.py'), '--check', str(raw)],
                                   capture_output=True, text=True)
            return r, (out.read_text() if out.exists() else ''), check

    RAW = [
        {'t': 0.0, 'sent': {'type': 'control_request', 'request_id': 'init_1', 'request': {'subtype': 'initialize'}}},
        {'t': 0.0, 'sent': {'type': 'user', 'uuid': 'u1', 'message': {'content': 'go'}}},
        {'t': 0.4, 'frame': {'type': 'control_response', 'response': {'subtype': 'success', 'request_id': 'init_1',
                             'response': {'account': {'email': 'someone@example.com'}}}}},
        {'t': 0.5, 'frame': {'type': 'system', 'subtype': 'init', 'cwd': '/Volumes/X/rec-cwd', 'tools': ['Bash'],
                             'model': 'm', 'mcp_servers': [{'name': 'c', 'source': 'claudeai'}],
                             'skills': ['private-skill'], 'memory_paths': {'auto': '/Users/someone/.claude/p/'},
                             'plugins': [{'name': 'b', 'path': 'builtin'}, {'name': 'mine', 'path': '/Users/someone/p'}]}},
        {'t': 1.0, 'frame': {'type': 'rate_limit_event', 'rate_limit_info': {
            'status': 'allowed', 'rateLimitType': 'five_hour', 'resetsAt': 5, 'overageStatus': 'rejected',
            'overageDisabledReason': 'org_level_disabled', 'unifiedWindows': {'five_hour': {'utilization': 0.37, 'resetsAt': 5}}}}},
        {'t': 2.0, 'frame': {'type': 'assistant', 'message': {'id': 'msg_011CfTpFDf62bxwBch5gfknB', 'content': [
            {'type': 'thinking', 'thinking': '', 'signature': 'EoEFCrIBCBIYAipA'},
            {'type': 'tool_use', 'id': 'toolu_01AWD6b5mcQo5xEmjxCYCjS4', 'name': 'Bash', 'input': {}}]},
            'request_id': 'req_011CfTpFDE3UMWoVk8exPEgi'}},
        {'t': 2.1, 'frame': {'type': 'user', 'message': {'content': [{'type': 'tool_result',
            'tool_use_id': 'toolu_01AWD6b5mcQo5xEmjxCYCjS4',
            'content': 'Output: /private/tmp/claude-501/-Volumes-X-rec-cwd/s/tasks/b.output'}]}}},
        {'t': 9.0, 'sent': {'type': 'user', 'uuid': 'u2', 'message': {'content': 'it has ended'}}},
    ]

    def test_the_account_goes_and_the_recording_stays(self):
        r, text, check = self.scrub(self.RAW, '--expect', '2:has ended')
        self.assertEqual(r.returncode, 0, r.stderr)
        for gone in ('someone', 'claudeai', 'private-skill', 'overage', 'org_level', 'EoEFCrIB', 'msg_011',
                     'req_011', 'toolu_01', '/Volumes/X/rec-cwd', '-Volumes-X-rec-cwd', '0.37'):
            self.assertNotIn(gone, text, gone)
        rows = [json.loads(l) for l in text.splitlines()]
        self.assertEqual(rows[2]['frame'], {'type': 'control_response',
                                            'redacted': 'initialize reply not recorded (account details)'})
        init = rows[3]['frame']
        self.assertEqual((init['cwd'], init['tools'], init['plugins'], init['mcp_servers']),
                         ('/replay/cwd', ['Bash'], [{'name': 'b', 'path': 'builtin'}], []))
        # One id, one placeholder, everywhere it appears: the frames still agree.
        self.assertEqual(text.count('toolu_replay_1'), 2)
        self.assertIn('/private/tmp/claude-501/-replay-cwd/s/tasks/b.output', text)
        self.assertEqual(rows[-1]['expect'], {'contains': 'has ended'})
        self.assertEqual(check.returncode, 1, 'the raw capture must be called out')
        self.assertIn('LEFT: an email address', check.stdout)

    def test_a_path_cut_across_streamed_pieces_is_scrubbed_whole(self):
        # The 2026-10-09 recording cut `-Users-admin-testvm-walk-...` into pieces like these.
        def piece(text, index=1):
            return {'t': 3.0, 'frame': {'type': 'stream_event', 'event': {
                'type': 'content_block_delta', 'index': index,
                'delta': {'type': 'input_json_delta', 'partial_json': text}}}}
        rows = self.RAW[:4] + [
            {'t': 2.9, 'frame': {'type': 'stream_event', 'event': {'type': 'message_start', 'message': {}}}},
            piece('{"file_path": "/private/tmp/claude-501/-Users-some'), piece('one-other-'),
            piece('dir/x"}'), piece('untouched', index=2)]
        r, text, _ = self.scrub(rows)
        self.assertEqual(r.returncode, 0, r.stderr)
        deltas = [json.loads(l)['frame']['event']['delta']['partial_json'] for l in text.splitlines()
                  if '"content_block_delta"' in l]
        self.assertEqual(deltas, ['{"file_path": "/private/tmp/claude-501/-Users-user-other-dir/x"}', '', '',
                                  'untouched'])

    def test_a_capture_that_cannot_be_cleaned_is_refused_and_nothing_is_written(self):
        rows = self.RAW + [{'t': 9.5, 'frame': {'type': 'assistant', 'message': {'content': [
            {'type': 'text', 'text': 'Mail me at someone@example.com'}]}}}]
        r, text, _ = self.scrub(rows)
        self.assertEqual((r.returncode, text), (1, ''))
        self.assertIn('an email address', r.stderr)

    def test_every_committed_capture_is_clean(self):
        captures = sorted(FIXTURES.glob('*.jsonl'))
        self.assertGreaterEqual(len(captures), 3)
        for path in captures:
            r = subprocess.run([sys.executable, str(REPLAY / 'scrub-capture.py'), '--check', str(path)],
                               capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, '%s: %s' % (path.name, r.stdout))


class Recorder(unittest.TestCase):
    def record(self, **env):
        with tempfile.TemporaryDirectory() as d:
            fake = pathlib.Path(d) / 'claude'
            fake.write_text(FAKE_CLAUDE)
            fake.chmod(0o755)
            out = pathlib.Path(d) / 'cap.jsonl'
            r = subprocess.run([sys.executable, str(REPLAY / 'record-long-session.py'), str(out), '--claude', str(fake),
                                '--seconds', '60'], capture_output=True, text=True, env=dict(os.environ, **env))
            return r, (out.read_text() if out.exists() else '')

    def test_the_session_is_recorded_in_the_hosts_order_without_the_account(self):
        r, text = self.record()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        summary = json.loads(r.stdout)
        self.assertEqual(summary['finishes'], {'A': 'outside', 'C': 'folded', 'D': 'outside'})
        self.assertNotIn('someone@example.com', text)
        rows = [json.loads(l) for l in text.splitlines()]
        sent = [row for row in rows if 'sent' in row]
        self.assertEqual(len(sent), 7, 'initialize and six messages')
        reports = [row for row in sent if 'expect' in row]
        self.assertEqual([row['expect'] for row in reports], [{'contains': 'that is this app telling you'}] * 2)
        self.assertEqual([sent.index(row) for row in reports], [3, 6])
        for row in reports:
            self.assertIn('that is this app telling you', row['sent']['message']['content'][0]['text'])
        # Each report was sent after its command's notification, which came outside any turn.
        notified = [row['t'] for row in rows if row.get('frame', {}).get('subtype') == 'task_notification']
        self.assertEqual(len(notified), 3)
        self.assertLessEqual(notified[0], reports[0]['t'])
        self.assertEqual(rows[-1].get('exit'), 0)

    def test_a_session_that_did_not_take_the_shape_is_refused(self):
        r, _ = self.record(FAKE_FOLD_ALL='1')
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertFalse(json.loads(r.stdout)['shaped'])


@unittest.skipUnless(CHILD, 'RICHOS_REPLAY_BIN names the built richos-replay')
class Child(unittest.TestCase):
    """The replay child as `NativeClient` meets it: over stdin and stdout."""

    def start(self, capture, *extra):
        d = tempfile.mkdtemp()
        self.addCleanup(lambda: subprocess.run(['rm', '-rf', d]))
        transcript = pathlib.Path(d) / 't.jsonl'
        p = subprocess.Popen([CHILD, '--capture', str(capture), '--speed', '50', '--transcript', str(transcript),
                              '--', '--print', '--session-id', 'our-session', *extra],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        def end():
            if p.poll() is None:
                p.kill()
            p.wait()
            for pipe in (p.stdin, p.stdout, p.stderr):
                try:
                    pipe.close()
                except (OSError, ValueError):
                    pass
        self.addCleanup(end)
        lines = queue.Queue()
        threading.Thread(target=lambda: [lines.put(l) for l in p.stdout] and None, daemon=True).start()
        return p, lines, transcript

    @staticmethod
    def write(p, obj):
        p.stdin.write(json.dumps(obj) + '\n')
        p.stdin.flush()

    @staticmethod
    def until(lines, predicate, seconds=20):
        got = []
        while True:
            frame = json.loads(lines.get(timeout=seconds))
            got.append(frame)
            if predicate(frame):
                return got

    def test_it_plays_the_capture_under_the_hosts_ids_and_refuses_a_message_it_never_recorded(self):
        p, lines, transcript = self.start(FIXTURES / '2026-09-27-cap-continue.jsonl')
        self.write(p, {'type': 'control_request', 'request_id': 'req_init',
                       'request': {'subtype': 'initialize', 'hooks': {}}})
        reply = self.until(lines, lambda f: f.get('type') == 'control_response')[-1]
        self.assertEqual(reply, {'type': 'control_response',
                                 'response': {'subtype': 'success', 'request_id': 'req_init', 'response': {}}})
        self.write(p, {'type': 'user', 'uuid': 'our-first', 'message': {'role': 'user', 'content': [
            {'type': 'text', 'text': 'start it'}]}})
        first = self.until(lines, lambda f: f.get('subtype') == 'task_notification')
        started = [f for f in first if f.get('type') == 'command_lifecycle']
        self.assertTrue(started and all(f['command_uuid'] == 'our-first' for f in started), started)
        self.assertTrue(all(f.get('session_id') in (None, 'our-session') for f in first))
        # Where the host is meant to ask for the report, anything else is a divergence.
        self.write(p, {'type': 'user', 'uuid': 'our-second', 'message': {'content': [
            {'type': 'text', 'text': 'Prepare the Northwind summary.'}]}})
        self.assertEqual(p.wait(), 3)
        events = lines_of(transcript)
        self.assertEqual(events[-1]['event'], 'divergence')
        self.assertIn('Background commands ended (provider observations):', events[-1]['why'])

    def test_closed_input_ends_it_cleanly_and_an_unreadable_capture_is_refused(self):
        p, _, transcript = self.start(FIXTURES / '2026-09-27-cap-plain.jsonl')
        p.stdin.close()
        self.assertEqual(p.wait(), 0)
        self.assertEqual(lines_of(transcript)[-1]['event'], 'end-of-input')
        p, _, _ = self.start(FIXTURES / 'no-such-capture.jsonl')
        self.assertEqual(p.wait(), 2)


if __name__ == '__main__':
    unittest.main(verbosity=2)

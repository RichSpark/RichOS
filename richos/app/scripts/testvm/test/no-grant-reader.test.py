#!/usr/bin/env python3
"""Test the TCC-log summary and the reader's evidence handling with local guest and transfer stubs."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('no_grant_reader', Path(__file__).resolve().parents[1] / 'no-grant-reader.py')
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)

ASK = ('2026-11-12 05:{t} E  tccd[154:108a] [com.apple.TCC:access] TCCDProcess: identifier=com.richos.app, pid={pid}, '
       'auid=501, euid=501, binary_path=/Applications/RichOS.app/Contents/MacOS/richos-tauri attempted to call '
       'TCCAccessRequest for kTCCServiceAccessibility')
UP = ('2026-11-12 05:{t} Df universalAccessAuthWarn[707:16b8] [com.apple.AppKit:Window] Window <NSWindow: 0x14b7267b0> '
      'windowNumber=1b ordered front from a non-active application and may order beneath the active application')
AGAIN = '2026-11-12 05:{t} Df universalAccessAuthWarn[707:16b8] [com.apple.AppKit:Window] order window: 1b op: 1 relative: 0 related: 0'
ANSWERS = ''.join('  answered at fixture: {"jsonrpc":"2.0","id":%d,"result":{}}\n' % n for n in (1, 2, 3))


class Summary(unittest.TestCase):
    def test_one_request_per_process_and_the_same_window_brought_forward_for_a_new_process(self):
        log = '\n'.join([ASK.format(t='34:25.724', pid=649), ASK.format(t='34:25.737', pid=649),
                         UP.format(t='34:26.867'), UP.format(t='34:26.878'),
                         ASK.format(t='36:38.223', pid=1050), AGAIN.format(t='36:38.286'),
                         '2026-11-12 05:36:41.391 Df tccd[403:21b0] [com.apple.TCC:access] REQUEST: unrelated'])
        self.assertEqual(reader.summary(log), [
            '2026-11-12 05:34:25.724 RichOS (pid 649) asked macOS for Accessibility',
            '2026-11-12 05:34:26.867 macOS request window 1b (universalAccessAuthWarn pid 707) put up',
            '2026-11-12 05:36:38.223 RichOS (pid 1050) asked macOS for Accessibility',
            '2026-11-12 05:36:38.286 macOS request window 1b (universalAccessAuthWarn pid 707) brought forward again'])

    def test_two_requests_from_one_process_minutes_apart_are_two(self):
        log = '\n'.join([ASK.format(t='34:25.724', pid=649), ASK.format(t='34:25.737', pid=649),
                         ASK.format(t='36:38.223', pid=649), ASK.format(t='36:38.250', pid=649)])
        self.assertEqual(len(reader.summary(log)), 2)

    def verdict_of(self, calls, done=True):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            (out / 'calls.txt').write_text(calls)
            if done:
                (out / 'done.txt').write_text('done\n')
            return reader.verdict(out, say=lambda *a: None)

    def test_the_run_passes_only_when_every_read_was_answered(self):
        answer = '  answered at 2026-11-12T05:35:28Z: {"jsonrpc":"2.0","id":%d,"result":{"isError":true}}\n'
        self.assertEqual(self.verdict_of(''.join(answer % n for n in (1, 2, 3))), 0)
        self.assertEqual(self.verdict_of(answer % 1 + answer % 2 + '  NO ANSWER within 90 s\n'), 1)
        self.assertEqual(self.verdict_of(''.join(answer % n for n in (1, 2, 3)), done=False), 1)
        self.assertEqual(self.verdict_of(''), 1)

    def test_a_log_without_a_request_says_nothing(self):
        self.assertEqual(reader.summary('2026-11-12 05:00:00.000 Df tccd[1:2] nothing here'), [])


class Measurement(unittest.TestCase):
    def run_reader(self, out, failed=None, deliver=True):
        events = []
        destinations = []

        def run(args, **kwargs):
            stdout = ''
            if args[0] == reader.TART:
                stage = args[1]
                if stage == 'ip':
                    stdout = '192.0.2.1\n'
            elif args[0] == 'scp':
                src, dest = args[-2:]
                if dest.endswith('/RichOS.zip'):
                    stage = 'copy bundle'
                elif dest.endswith('/measure.sh'):
                    stage = 'copy script'
                else:
                    self.assertTrue(src.endswith(':measure/.'))
                    stage = 'pull'
                    dest = Path(dest)
                    destinations.append(dest)
                    if deliver:
                        (dest / 'calls.txt').write_text(ANSWERS)
                        (dest / 'done.txt').write_text('fresh done\n')
                        (dest / 'tcc.log').write_text(ASK.format(t='34:25.724', pid=649))
            else:
                self.assertEqual(args[0], 'ssh')
                command = args[-1]
                if command == 'true':
                    stage = 'ssh ready'
                elif command.startswith('pgrep -x Finder '):
                    stage = 'gui session'
                    stdout = 'finder\n'
                elif 'ditto' in command:
                    stage = 'install'
                else:
                    self.assertTrue(command.startswith('bash /tmp/measure.sh '))
                    stage = 'measure'
            events.append(stage)
            rc = 1 if stage == failed else 0
            return subprocess.CompletedProcess(args, rc, stdout, 'fixture failure' if rc else '')

        args = ['no-grant-reader.py', '--out', str(out), '--tart-home', str(out.parent / 'tart'),
                '--bundle', str(out.parent / 'RichOS.zip')]
        boot = Mock(pid=123)
        with patch.object(sys, 'argv', args), patch.object(reader.slots, 'guest_slot', return_value=contextlib.nullcontext(0)), \
                patch.object(reader.subprocess, 'run', side_effect=run), patch.object(reader.subprocess, 'Popen', return_value=boot), \
                patch.object(reader.time, 'sleep'), contextlib.redirect_stdout(io.StringIO()) as printed:
            rc = reader.main()
        return rc, events, destinations, printed.getvalue(), boot

    def test_failed_prerequisites_cannot_pass_with_a_previous_result(self):
        for stage, next_stage in (('set', 'ip'), ('ssh ready', 'gui session'), ('gui session', 'copy bundle'),
                                  ('copy bundle', 'copy script'), ('copy script', 'install'), ('install', 'measure')):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / 'evidence'
                old = self.write_old_result(out)
                rc, events, _, printed, _ = self.run_reader(out, failed=stage)
                self.assertEqual(rc, 1, printed)
                self.assertIn(stage, events)
                self.assertNotIn(next_stage, events)
                self.assertEqual(events[-1], 'delete')
                self.assert_old_result(out, old)

    def test_measure_or_pull_failure_keeps_partial_output_but_refuses_complete_looking_files(self):
        for stage in ('measure', 'pull'):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / 'evidence'
                old = self.write_old_result(out)
                rc, events, destinations, printed, boot = self.run_reader(out, failed=stage)
                self.assertEqual(rc, 1, printed)
                self.assertIn(f'{stage} 1', printed)
                self.assertEqual(events[-2:], ['stop', 'delete'])
                boot.wait.assert_called_once_with(timeout=120)
                fresh, = destinations
                self.assertEqual(fresh.parent, out)
                self.assertTrue((fresh / 'done.txt').exists())
                self.assertFalse((fresh / 'summary.txt').exists())
                self.assert_old_result(out, old)

    def test_successful_guest_steps_still_need_fresh_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / 'evidence'
            old = self.write_old_result(out)
            rc, _, destinations, printed, _ = self.run_reader(out, deliver=False)
            self.assertEqual(rc, 1, printed)
            self.assertIn('0 of 3 reads answered', printed)
            fresh, = destinations
            self.assertFalse((fresh / 'done.txt').exists())
            self.assert_old_result(out, old)

    def test_success_keeps_the_original_and_returns_new_evidence_on_each_repeat(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / 'evidence'
            rc, _, destinations, printed, _ = self.run_reader(out)
            self.assertEqual(rc, 0, printed)
            self.assertEqual(destinations, [out])
            old = {path.name: path.read_bytes() for path in out.iterdir()}
            self.assertIn(b'asked macOS for Accessibility', old['summary.txt'])
            first_repeat = None
            for _ in range(2):
                rc, _, destinations, printed, _ = self.run_reader(out)
                self.assertEqual(rc, 0, printed)
                fresh, = destinations
                self.assertEqual(fresh.parent, out)
                self.assertNotEqual(fresh, first_repeat)
                self.assertIn(str(fresh), printed)
                self.assertEqual((fresh / 'summary.txt').read_bytes(), old['summary.txt'])
                first_repeat = fresh
            self.assert_old_result(out, old)

    def write_old_result(self, out):
        out.mkdir()
        old = {'calls.txt': ANSWERS.encode(), 'done.txt': b'old done\n',
               'tcc.log': b'old tcc\n', 'summary.txt': b'old summary\n', 'drive.log': b'old drive\n'}
        for name, data in old.items():
            (out / name).write_bytes(data)
        return old

    def assert_old_result(self, out, old):
        self.assertEqual({name: (out / name).read_bytes() for name in old}, old)


if __name__ == '__main__':
    unittest.main()

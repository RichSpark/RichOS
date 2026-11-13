#!/usr/bin/env python3
"""no-grant-reader.py's TCC-log summary, offline, on lines in the shape macOS 15.7.7's tccd and
universalAccessAuthWarn logged them in the 2026-11-12 clean-guest run. The guest run is its own proof."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('no_grant_reader', Path(__file__).resolve().parents[1] / 'no-grant-reader.py')
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)

ASK = ('2026-11-12 05:{t} E  tccd[154:108a] [com.apple.TCC:access] TCCDProcess: identifier=com.richos.app, pid={pid}, '
       'auid=501, euid=501, binary_path=/Applications/RichOS.app/Contents/MacOS/richos-tauri attempted to call '
       'TCCAccessRequest for kTCCServiceAccessibility')
UP = ('2026-11-12 05:{t} Df universalAccessAuthWarn[707:16b8] [com.apple.AppKit:Window] Window <NSWindow: 0x14b7267b0> '
      'windowNumber=1b ordered front from a non-active application and may order beneath the active application')
AGAIN = '2026-11-12 05:{t} Df universalAccessAuthWarn[707:16b8] [com.apple.AppKit:Window] order window: 1b op: 1 relative: 0 related: 0'


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
        import tempfile
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


if __name__ == '__main__':
    unittest.main()

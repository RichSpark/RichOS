#!/usr/bin/env python3
"""splash-trials-tools.test.py — see splash-trials-tools.test.sh."""
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / 'docs/verification/2026-11-14-splash-trials/tools'


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), TOOLS / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# A synthetic recording: frames 100x60 gray (half the points, as the recorder writes them), the
# window's content at (10, 20) points, 100x80 points, the page filling it, and the opening's region
# at the page's top-left, so the stamp's six 24-point cells are centered at x = 15 + 12k, y = 20 px.
W, H = 100, 60
T_OPEN = 1000.0  # seconds
CONTENT = (10, 20, 100, 80)


def frame(stamp=None, window=0):
    pixels = bytearray(W * H)
    if window:
        for y in range(10, 50):
            for x in range(5, 55):
                pixels[y * W + x] = window
    if stamp is not None:
        bits = [1] + [(stamp >> (4 - j)) & 1 for j in range(5)]
        for c, bit in enumerate(bits):
            cx = 15 + 12 * c
            for y in range(15, 26):
                for x in range(cx - 5, cx + 6):
                    pixels[y * W + x] = 255 if bit else 0
    return bytes(pixels)


def write_launch(out, name, frames, lines):
    d = out / name
    (d / 'frames').mkdir(parents=True)
    with open(d / 'frames' / 'frames.gray', 'wb') as f:
        for _, pixels in frames:
            f.write(pixels)
    text = '%d %d\nindex display_mach display_epoch_ms\n' % (W, H)
    text += ''.join('%d 0 %.3f\n' % (i, T_OPEN * 1000 + ms) for i, (ms, _) in enumerate(frames))
    (d / 'frames' / 'frames.txt').write_text(text)
    launch = {'name': name, 'load_before': [1.0, 1.0, 1.0], 't_open': T_OPEN, 'all_gone': True,
              'lines': [{'t': T_OPEN + ms / 1000, 'text': '[trial] %.1f %s' % (T_OPEN * 1000 + ms, t)} for ms, t in lines],
              'rss_kb': [[T_OPEN, 102400]]}
    (d / 'launch.json').write_text(json.dumps(launch))
    return launch


def trial_lines(busy_end_ms):
    attached = 'attached decode 9.00 ms attach 0.30 ms picture 100x80 32000 bytes content %d %d %d %d scale 1' % CONTENT
    return [(100, 'asked mode 1 picture full-000.png'), (110, attached),
            (120, 'pushed media 1.0000 nopush false process_started_ms %d' % (T_OPEN * 1000 + 5)),
            (busy_end_ms, 'busy end'), (busy_end_ms + 1, 'removed')]


class Tools(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='splash-trials-tools-'))

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def test_every_tool_parses(self):
        for script in sorted(TOOLS.glob('*.py')):
            compile(script.read_text(), str(script), 'exec')
        subprocess.run(['bash', '-n', str(TOOLS / 'release-bundle.sh')], check=True)
        for script in ('splash-trials-guest.py', 'splash-trials-walk.py', 'analyze.py'):
            if script == 'analyze.py':
                continue
            r = subprocess.run([sys.executable, str(TOOLS / script), '--help'], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)

    @unittest.skipUnless(shutil.which('swiftc'), 'swiftc is not installed')
    def test_the_recorder_type_checks(self):
        env = dict(os.environ, TMPDIR=str(self.tmp))
        r = subprocess.run(['swiftc', '-typecheck', '-target', 'arm64-apple-macos14.0', str(TOOLS / 'screen-frames.swift')],
                           capture_output=True, text=True, env=env)
        self.assertEqual(r.returncode, 0, r.stderr)

    def recording(self):
        out = self.tmp / 'out'
        opening = out / 't2-files' / 'opening'
        opening.mkdir(parents=True)
        (opening / 'opening.json').write_text(json.dumps({'frames': 30, 'fps': 30.0, 'region': [0, 0, 100, 80], 'page': [100, 80]}))
        t1 = write_launch(out, 't1-01', [(50, frame()), (200, frame(0, window=40)), (2300, frame())], trial_lines(2250))
        slot = 1000 / 30
        t3 = write_launch(out, 't3png-01', [(50, frame())] + [(300 + k * slot, frame(k, window=40)) for k in range(3)]
                          + [(2300, frame())], trial_lines(2250))
        (out / 'results.json').write_text(json.dumps([t1, t3]))
        return out

    def test_the_analysis_reads_the_recording(self):
        out = self.recording()
        env = dict(os.environ, TMPDIR=str(self.tmp))
        r = subprocess.run([sys.executable, str(TOOLS / 'analyze.py'), str(out)], capture_output=True, text=True, env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        summary = json.loads((out / 'summary.json').read_text())
        t1 = summary['launches']['t1'][0]
        self.assertEqual(t1['request_to_picture_on_glass_ms'], 200.0)
        self.assertEqual(t1['request_to_window_on_glass_ms'], 200.0)
        self.assertEqual(t1['picture_off_glass_after_request_ms'], 2300.0)
        self.assertEqual(t1['picture_on_glass_before_busy_end_ms'], 2050.0)
        self.assertEqual(t1['request_to_pushed_stamp_ms'], 120.0)
        self.assertEqual(t1['process_start_to_pushed_ms'], 115.0)
        t3 = summary['launches']['t3png'][0]
        self.assertEqual(t3['pictures_seen'], 3)
        self.assertEqual(t3['pictures_missing'], list(range(3, 30)))
        self.assertEqual(t3['worst_lateness_ms'], 0.0)
        self.assertEqual(summary['t1_stats']['request_to_picture_on_glass_ms']['p95_nearest_rank'], 200.0)

    def test_a_recorded_instant_becomes_a_picture(self):
        out = self.recording()
        png = self.tmp / 'f.png'
        r = subprocess.run([sys.executable, str(TOOLS / 'frame-png.py'), str(out / 't1-01'), '250', str(png)],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn('frame 1, displayed 200.0 ms', r.stdout)
        self.assertTrue(png.stat().st_size > 0)

    def test_the_pixel_comparison(self):
        analyze = load('analyze.py')
        pictures = []
        for name, changed in (('a', False), ('b', False), ('c', True)):
            pixels = bytearray([0x10, 0x20, 0x30] * (16 * 8))
            if changed:
                pixels[0:3] = bytes([0x50, 0x20, 0x30])
            ppm = self.tmp / (name + '.ppm')
            ppm.write_bytes(b'P6 16 8 255\n' + bytes(pixels))
            png = self.tmp / (name + '.png')
            subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(ppm), str(png)], check=True)
            pictures.append(str(png))
        same = analyze.compare(pictures[0], pictures[1], str(self.tmp))
        self.assertEqual(same['psnr_db'], 'identical')
        one = analyze.compare(pictures[0], pictures[2], str(self.tmp), str(self.tmp / 'd.png'), (0, 0, 16, 8), (0, 0, 16, 8))
        self.assertEqual(one['pixels_over_16_pct'], round(100 / 128, 4))
        self.assertGreater(one['max_channel_diff'], 32)
        self.assertTrue((self.tmp / 'd.png').stat().st_size > 0)


if __name__ == '__main__':
    unittest.main(verbosity=2)

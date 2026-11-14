#!/usr/bin/env python3
"""analyze.py OUT_DIR — reads a splash-trials walk's pulled output (out.tgz extracted) and prints
the three trials' results against the plan's pass lines, plus the Dictionary benchmark. Writes
OUT_DIR/summary.json and, for trial 2, amplified difference pictures.

No third-party Python: pictures are turned into raw RGB by ffmpeg after macOS's own `sips` has
matched both to sRGB (a screen capture carries the display's profile, a WebKit snapshot its own),
and screen frames are the recorder's own 8-bit gray.
"""
import json
import math
import os
import re
import subprocess
import sys
import tempfile

SRGB = '/System/Library/ColorSync/Profiles/sRGB Profile.icc'


def p95(values):
    """Nearest-rank 95th percentile."""
    v = sorted(values)
    return v[max(0, math.ceil(0.95 * len(v)) - 1)] if v else None


def ranges(numbers):
    """[1, 3, 4, 5, 15] -> '1, 3-5, 15'; [] -> 'none'."""
    out, run = [], []
    for n in sorted(numbers):
        if run and n == run[-1] + 1:
            run.append(n)
            continue
        if run:
            out.append(str(run[0]) if len(run) == 1 else '%d-%d' % (run[0], run[-1]))
        run = [n]
    if run:
        out.append(str(run[0]) if len(run) == 1 else '%d-%d' % (run[0], run[-1]))
    return ', '.join(out) if out else 'none'


def median(values):
    v = sorted(values)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def rgb(png, tmp, crop=None):
    """(w, h, bytes) of `png` in sRGB, 8 bits per channel; `crop` = (x, y, w, h) in pixels."""
    tag = '' if crop is None else '.%d-%d-%d-%d' % crop
    matched = os.path.join(tmp, os.path.basename(png) + tag + '.srgb.png')
    subprocess.run(['sips', '--matchTo', SRGB, png, '--out', matched + '.full.png'], capture_output=True, check=True)
    if crop is None:
        os.rename(matched + '.full.png', matched)
    else:
        x, y, w, h = crop
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', matched + '.full.png', '-vf', 'crop=%d:%d:%d:%d' % (w, h, x, y),
                        matched], capture_output=True, check=True)
    probe = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height',
                            '-of', 'csv=p=0', matched], capture_output=True, text=True, check=True).stdout.strip()
    w, h = (int(x) for x in probe.split(','))
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', matched, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'],
                         capture_output=True, check=True).stdout
    return w, h, raw, matched


def compare(a_png, b_png, tmp, diff_png=None, crop_a=None, crop_b=None):
    wa, ha, a, ma = rgb(a_png, tmp, crop_a)
    wb, hb, b, mb = rgb(b_png, tmp, crop_b)
    if (wa, ha) != (wb, hb):
        return {'sizes': [[wa, ha], [wb, hb]], 'comparable': False}
    n = len(a)
    hist = [0] * 256
    for x, y in zip(a, b):
        hist[abs(x - y)] += 1
    se = sum(d * d * c for d, c in enumerate(hist))
    worst_px = 0
    over = {2: 0, 8: 0, 16: 0}
    for i in range(0, n, 3):
        d = max(abs(a[i] - b[i]), abs(a[i + 1] - b[i + 1]), abs(a[i + 2] - b[i + 2]))
        if d > worst_px:
            worst_px = d
        for t in over:
            if d > t:
                over[t] += 1
    pixels = n // 3
    mse = se / n
    out = {
        'comparable': True, 'size': [wa, ha],
        'mean_abs_diff': round(sum(d * c for d, c in enumerate(hist)) / n, 4),
        'max_channel_diff': worst_px,
        'psnr_db': round(10 * math.log10(255 * 255 / mse), 2) if mse else 'identical',
        'pixels_over_2_pct': round(100 * over[2] / pixels, 4),
        'pixels_over_8_pct': round(100 * over[8] / pixels, 4),
        'pixels_over_16_pct': round(100 * over[16] / pixels, 4),
    }
    if diff_png:
        # The difference, multiplied by 16 (a one-level difference is visible), written from the
        # very bytes compared above, as a PPM that ffmpeg turns into PNG.
        amp = bytes(min(255, abs(x - y) * 16) for x, y in zip(a, b))
        ppm = diff_png + '.ppm'
        with open(ppm, 'wb') as f:
            f.write(b'P6 %d %d 255\n' % (wa, ha))
            f.write(amp)
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', ppm, diff_png], capture_output=True, check=True)
        os.remove(ppm)
        out['difference_x16'] = os.path.basename(diff_png)
    return out


def trial_lines(launch):
    """{'attached': (epoch, text), ...} from the app's own [trial] lines."""
    found = {}
    for line in launch['lines']:
        m = re.match(r'\[trial\] ([0-9.]+) (.*)', line['text'])
        if not m:
            continue
        t, text = float(m.group(1)), m.group(2)
        for key in ('asked', 'attached', 'pushed', 'busy end', 'removed', 'trial 3 read', 'trial 3 handed'):
            if text.startswith(key) and key not in found:
                found[key] = (t, text)
    return found


class Frames:
    def __init__(self, d):
        txt = open(os.path.join(d, 'frames.txt')).read().splitlines()
        self.w, self.h = (int(x) for x in txt[0].split())
        self.t = [float(line.split()[2]) for line in txt[2:] if line.strip()]
        self.raw = open(os.path.join(d, 'frames.gray'), 'rb').read()
        self.n = min(len(self.t), len(self.raw) // max(1, self.w * self.h))

    def avg(self, i, cx, cy, r=3):
        base = i * self.w * self.h
        total = cnt = 0
        for y in range(max(0, cy - r), min(self.h, cy + r + 1)):
            row = base + y * self.w
            for x in range(max(0, cx - r), min(self.w, cx + r + 1)):
                total += self.raw[row + x]
                cnt += 1
        return total / cnt if cnt else 0

    def changed(self, i, j, top, bottom, step=4, level=24):
        """Share of sampled pixels that differ by more than `level` between frames i and j, rows
        from `top` to `bottom` (frame pixels)."""
        bi, bj = i * self.w * self.h, j * self.w * self.h
        diff = total = 0
        for y in range(top, bottom, step):
            for x in range(0, self.w, step):
                total += 1
                if abs(self.raw[bi + y * self.w + x] - self.raw[bj + y * self.w + x]) > level:
                    diff += 1
        return diff / total if total else 0


def stamp_reader(frames, content, stamp_at):
    """Returns read(i) -> index shown by the stamp in frame i, or None when no stamp is there.
    The stamp is six 24-point cells; the recorder's frames are half the display's points."""
    s = 0.5
    cells = [(int((content[0] + stamp_at[0] + 12 + 24 * c) * s), int((content[1] + stamp_at[1] + 12) * s)) for c in range(6)]

    def read(i):
        lum = [frames.avg(i, x, y, 2) for x, y in cells]
        if lum[0] < 170:
            return None
        bits = []
        for v in lum[1:]:
            if v > 170:
                bits.append(1)
            elif v < 60:
                bits.append(0)
            else:
                return None
        return int(''.join(str(b) for b in bits), 2)
    return read


def main():
    out = sys.argv[1]
    results = json.load(open(os.path.join(out, 'results.json')))
    summary = {}
    t2dir = os.path.join(out, 't2-files')
    tmp = tempfile.mkdtemp(prefix='splash-analyze-', dir=os.environ.get('TMPDIR'))
    # ---------------- TRIAL 2 ----------------
    if os.path.exists(os.path.join(t2dir, 'trial2.json')):
        t2 = json.load(open(os.path.join(t2dir, 'trial2.json')))
        rows = []
        for k in range(3):
            run = t2['running'][k]
            row = {'k': k, 'moment_ms': round(run['running']['delta'], 2)}
            # The page: its viewport, which sits under the window's title bar on the screen and at
            # the top of WebKit's snapshot (which leaves the title bar's strip empty at the bottom).
            scale = run['request']['scale']
            pw, ph = round(run['running']['w'] * scale), round(run['running']['h'] * scale)
            inset = round(run['request']['content'][3] * scale) - ph
            on_screen, in_snapshot = (0, inset, pw, ph), (0, 0, pw, ph)
            row['page_px'], row['title_bar_px'] = [pw, ph], inset
            screen = os.path.join(t2dir, 'screen-%d.png' % k)
            hidden = os.path.join(t2dir, 'hidden-%d.png' % k)
            visible = os.path.join(t2dir, 'visible-snapshot-%d.png' % k)
            if os.path.exists(screen) and os.path.exists(hidden):
                row['hidden_vs_screen'] = compare(hidden, screen, tmp, os.path.join(t2dir, 'diff-hidden-vs-screen-%d.png' % k), in_snapshot, on_screen)
            if os.path.exists(visible) and os.path.exists(screen):
                row['visible_snapshot_vs_screen'] = compare(visible, screen, tmp, None, in_snapshot, on_screen)
            if os.path.exists(visible) and os.path.exists(hidden):
                row['hidden_vs_visible_snapshot'] = compare(hidden, visible, tmp, None, in_snapshot, in_snapshot)
            rows.append(row)
            print('trial 2 moment %d (%.1f ms): %s' % (k, row['moment_ms'], json.dumps({x: y for x, y in row.items() if x not in ('k', 'moment_ms')})))
        summary['trial2'] = {'moments': rows, 'hidden_ready_ms': t2.get('hidden_ready_ms'),
                             'hidden_frames': t2.get('hidden_frames'), 'opening_ms': t2.get('opening_ms'),
                             'region': t2.get('region'), 'points': t2.get('points')}
        print('trial 2 hidden page ready %.0f ms; opening of 30 moments drawn in %.0f ms; frames %s' % (
            t2.get('hidden_ready_ms', -1), t2.get('opening_ms', -1), t2.get('hidden_frames')))
    opening_meta = os.path.join(t2dir, 'opening', 'opening.json')
    meta = json.load(open(opening_meta)) if os.path.exists(opening_meta) else {}
    region = meta.get('region')
    page = meta.get('page')
    # ---------------- TRIALS 1 AND 3, AND DICTIONARY ----------------
    by_kind = {}
    for launch in results:
        name = launch['name']
        kind = name.rsplit('-', 1)[0]
        fdir = os.path.join(out, name, 'frames')
        if not os.path.exists(os.path.join(fdir, 'frames.txt')):
            continue
        frames = Frames(fdir)
        t_open = launch['t_open'] * 1000
        row = {'name': name, 'frames': frames.n}
        first_after = next((i for i in range(frames.n) if frames.t[i] > t_open), None)
        start = first_after if first_after is not None else frames.n
        before = max(0, start - 1)
        if kind == 'dictionary':
            # The first frame after the request in which the screen changed, menu bar and the
            # Dock's band left out (the Dock bounces and the menu bar changes its name on launch).
            top, bottom = int(30 * 0.5), frames.h - int(110 * 0.5)
            hit = next((i for i in range(start, frames.n) if frames.changed(before, i, top, bottom) > 0.01), None)
            if hit is not None:
                row['request_to_window_on_glass_ms'] = round(frames.t[hit] - t_open, 1)
            by_kind.setdefault(kind, []).append(row)
            continue
        lines = trial_lines(launch)
        att = lines.get('attached')
        if not att:
            row['error'] = 'no attached line'
            by_kind.setdefault(kind, []).append(row)
            continue
        m = re.search(r'content ([-0-9.]+) ([-0-9.]+) ([-0-9.]+) ([-0-9.]+) scale', att[1])
        content = [float(x) for x in m.groups()]
        m2 = re.search(r'decode ([0-9.]+) ms attach ([0-9.]+) ms', att[1])
        row['decode_ms'], row['attach_ms'] = float(m2.group(1)), float(m2.group(2))
        pushed = lines.get('pushed')
        ps = re.search(r'process_started_ms (\d+)', pushed[1]) if pushed else None
        if ps:
            row['process_start_to_pushed_ms'] = round(pushed[0] - int(ps.group(1)), 1)
            row['request_to_process_start_ms'] = round(int(ps.group(1)) - t_open, 1)
        if pushed:
            row['request_to_pushed_stamp_ms'] = round(pushed[0] - t_open, 1)
        busy_end = lines['busy end'][0] if 'busy end' in lines else None
        row['request_to_busy_end_ms'] = round(busy_end - t_open, 1) if busy_end else None
        # The picture sits where the page draws, under the title bar (from walk 2 on; walk 1 put it
        # at the top of the content view).
        inset = content[3] - page[1] if page else 0
        read = stamp_reader(frames, [content[0], content[1] + inset], [region[0] + 8, region[1] + 8])
        row['load_before'] = launch.get('load_before')
        shown = [(i, read(i)) for i in range(start, frames.n)]
        on = [(i, v) for i, v in shown if v is not None]
        # The window itself (any change inside its content), whatever it shows.
        s = 0.5
        top, bottom = int(content[1] * s), int((content[1] + content[3]) * s)
        win = next((i for i in range(start, frames.n) if frames.changed(before, i, top, bottom) > 0.05), None)
        if win is not None:
            row['request_to_window_on_glass_ms'] = round(frames.t[win] - t_open, 1)
        if on:
            first = on[0][0]
            row['request_to_picture_on_glass_ms'] = round(frames.t[first] - t_open, 1)
            gone = next((i for i, v in shown if i > first and v is None), None)
            if gone is not None:
                row['picture_off_glass_after_request_ms'] = round(frames.t[gone] - t_open, 1)
            if busy_end:
                row['picture_on_glass_before_busy_end_ms'] = round(busy_end - frames.t[first], 1)
        if kind.startswith('t3') and on:
            t0 = frames.t[on[0][0]]
            first_seen = {}
            for i, v in on:
                first_seen.setdefault(v, frames.t[i])
            late = {k: round(first_seen[k] - (t0 + k * 1000 / 30), 1) for k in sorted(first_seen)}
            row['pictures_seen'] = len(first_seen)
            row['pictures_missing'] = ranges([k for k in range(30) if k not in first_seen])
            row['lateness_ms_by_picture'] = late
            row['worst_lateness_ms'] = max(late.values())
            seq = [v for _, v in on]
            row['out_of_order'] = sum(1 for a, b in zip(seq, seq[1:]) if b < a)
            for key in ('trial 3 read', 'trial 3 handed'):
                if key in lines:
                    row[key.replace(' ', '_')] = lines[key][1]
        rss = [kb for _, kb in launch.get('rss_kb', []) if kb]
        if rss:
            row['max_rss_mb'] = round(max(rss) / 1024, 1)
        by_kind.setdefault(kind, []).append(row)
    for kind, rows in by_kind.items():
        print('\n== %s ==' % kind)
        for row in rows:
            print(json.dumps(row))
        for metric in ('request_to_picture_on_glass_ms', 'request_to_window_on_glass_ms', 'request_to_pushed_stamp_ms',
                       'process_start_to_pushed_ms', 'request_to_process_start_ms'):
            vals = [r[metric] for r in rows if isinstance(r.get(metric), (int, float))]
            if vals:
                stat = {'n': len(vals), 'min': min(vals), 'median': median(vals), 'p95_nearest_rank': p95(vals), 'max': max(vals)}
                print('%s %s: %s' % (kind, metric, stat))
                summary.setdefault(kind + '_stats', {})[metric] = stat
    summary['launches'] = by_kind
    with open(os.path.join(out, 'summary.json'), 'w') as f:
        json.dump(summary, f, indent=1)


if __name__ == '__main__':
    main()

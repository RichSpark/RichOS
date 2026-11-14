#!/usr/bin/env python3
"""frame-png.py LAUNCH_DIR MS OUT.png — the screen as the recorder saw it MS milliseconds after the
launch request (the last new frame displayed at or before that instant), as a PNG. The recorder's
frames are 8-bit gray at half the display's points (screen-frames.swift). Prints which frame and
when it was displayed."""
import json
import os
import subprocess
import sys


def main():
    launch_dir, ms, out = sys.argv[1], float(sys.argv[2]), sys.argv[3]
    launch = json.load(open(os.path.join(launch_dir, 'launch.json')))
    lines = open(os.path.join(launch_dir, 'frames', 'frames.txt')).read().splitlines()
    w, h = (int(x) for x in lines[0].split())
    times = [float(line.split()[2]) for line in lines[2:] if line.strip()]
    target = launch['t_open'] * 1000 + ms
    index = max((i for i, t in enumerate(times) if t <= target), default=0)
    with open(os.path.join(launch_dir, 'frames', 'frames.gray'), 'rb') as f:
        f.seek(index * w * h)
        pixels = f.read(w * h)
    pgm = out + '.pgm'
    with open(pgm, 'wb') as f:
        f.write(b'P5 %d %d 255\n' % (w, h))
        f.write(pixels)
    subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', pgm, out], check=True)
    os.remove(pgm)
    print('%s: frame %d, displayed %.1f ms after the request' % (out, index, times[index] - launch['t_open'] * 1000))


if __name__ == '__main__':
    main()

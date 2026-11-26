#!/usr/bin/env python3
"""side-by-side.py - two pictures next to each other in one, each scaled to the same height, with a
caption over each: the real app beside the design round it was built from.

  side-by-side.py LEFT.png RIGHT.png OUT.png [--left-caption TEXT] [--right-caption TEXT] [--height PX]

Exits non-zero with a sentence when a picture cannot be read. Needs Pillow.
"""
import argparse
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit('side-by-side.py needs Pillow (python3 -m pip install Pillow)')


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    p.add_argument('left')
    p.add_argument('right')
    p.add_argument('out')
    p.add_argument('--left-caption', default='')
    p.add_argument('--right-caption', default='')
    p.add_argument('--height', type=int, default=900)
    a = p.parse_args()
    try:
        pictures = [Image.open(a.left).convert('RGB'), Image.open(a.right).convert('RGB')]
    except OSError as exc:
        sys.exit('side-by-side.py: cannot read a picture: %s' % exc)
    scaled = [im.resize((round(im.width * a.height / im.height), a.height)) for im in pictures]
    band, gap = 44, 24
    out = Image.new('RGB', (sum(im.width for im in scaled) + gap, a.height + band), (255, 255, 255))
    draw = ImageDraw.Draw(out)
    try:
        font = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 26)
    except OSError:
        font = ImageFont.load_default()
    x = 0
    for im, caption in zip(scaled, (a.left_caption, a.right_caption)):
        out.paste(im, (x, band))
        draw.text((x + 8, 8), caption, fill=(0, 0, 0), font=font)
        x += im.width + gap
    out.save(a.out)
    print(a.out)


if __name__ == '__main__':
    main()

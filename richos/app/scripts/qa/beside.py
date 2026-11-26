#!/usr/bin/env python3
"""Put two screenshots side by side in one picture, each scaled to the same height and labeled.

  beside.py LEFT.png RIGHT.png OUT.png [--labels "Mockup" "App"] [--height 900]

For comparing a design round's mockup with the real app on the same screen. Each picture keeps
its aspect ratio; a 40 px strip above each names it (black on white, 16 px). Exits 2 with a
sentence when an input cannot be read.
"""
import argparse
import sys

from PIL import Image, ImageDraw, ImageFont


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("left")
    parser.add_argument("right")
    parser.add_argument("out")
    parser.add_argument("--labels", nargs=2, default=["Mockup", "App"])
    parser.add_argument("--height", type=int, default=900)
    args = parser.parse_args()
    pictures = []
    for path in (args.left, args.right):
        try:
            image = Image.open(path).convert("RGB")
        except OSError as error:
            print(f"cannot read {path}: {error}", file=sys.stderr)
            return 2
        width = round(image.width * args.height / image.height)
        pictures.append(image.resize((width, args.height), Image.LANCZOS))
    strip, gap = 40, 24
    canvas = Image.new("RGB", (sum(p.width for p in pictures) + gap, args.height + strip), "white")
    draw = ImageDraw.Draw(canvas)
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 16)
    except OSError:
        font = ImageFont.load_default()
    x = 0
    for picture, label in zip(pictures, args.labels):
        canvas.paste(picture, (x, strip))
        draw.text((x + 8, 11), label, fill="black", font=font)
        x += picture.width + gap
    canvas.save(args.out, optimize=True)
    print(f"{args.out}: {canvas.width} x {canvas.height}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

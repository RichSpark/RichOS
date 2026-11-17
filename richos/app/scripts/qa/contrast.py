#!/usr/bin/env python3
"""contrast.py — the WCAG contrast ratio of two colors, or of a region of a frame.

  contrast.py <color> <color> [--large|--nontext]
  contrast.py <frame.png> <label>:<x0>,<y0>,<x1>,<y1> [more...] [--large|--nontext]
  contrast.py <frame.png> --box <x> <y> <w> <h> [--label L] [--large|--nontext]
  contrast.py <over-light.png> --edge <over-dark.png> <label>:<x0>,<y0>,<x1>,<y1> [more...]
  contrast.py --help

A color is '#8f7030', '8f7030', '143,112,48' or 'rgb(143,112,48)'.
A region is half-open: x0,y0 inclusive, x1,y1 exclusive.

--edge is for light text drawn straight on a picture with no panel, read by its shadow (RichGlance's
info overlay). It takes two frames of the same text at the same place, one over a light picture and
one over a dark one. The dark frame says which pixels the text made: the glyph core is relative
luminance 0.8 or more, and a pixel the text did not touch is no lighter than the dark region's most
frequent color. Each core pixel is compared, in the light frame, with the LIGHTEST untouched pixel
within 1 px of it, so the worst ground beside a stroke, shadow included. One frame cannot say this:
a stroke's own antialiased edge looks like ground. Lowest and 1st-percentile ratios are printed.

===========================================================================
WHY THIS FILE EXISTS
===========================================================================
Ten separate implementations of this calculation were written across eight
QA walks between 2026-09-18 and 2026-09-20 — `contrast.py`, `contrast2.py`,
`tc.py`, a `contrast` subcommand of `px.py`, and inline heredocs. They did
NOT agree. Each chose the text color from the region by a different rule:
the darkest color occurring at least 3 times; the color furthest from the
background occupying at least 8 pixels; the furthest occupying at least
0.4% of the region; the 2nd percentile by distance; and, in two of them,
the single furthest pixel in the crop.

On antialiased text those rules differ by a full ratio point or more, and
the last one is simply wrong: one stray antialiasing pixel sets the figure.
So the project has been producing contrast verdicts that are not comparable
between walks, against a threshold — 4.5:1 — whose whole value is that it
is the same number every time.

===========================================================================
THE ONE RULE, STATED SO A REVIEWER CAN DISAGREE WITH IT
===========================================================================
BACKGROUND is the most frequent color in the region. Text occupies a
minority of a text region; its ground does not.

TEXT is the color FURTHEST from that background in relative luminance,
among colors covering at least `--min-frac` of the region (default 0.5%,
floor 4 pixels). Antialiasing spreads a stroke over intermediate shades,
and the extreme shades are the glyph CORE — the ink a reader actually
sees. The coverage floor is what keeps a single edge pixel, or a cursor
artifact, from setting the answer.

Both the chosen pair and the floor that produced it are PRINTED, so the
number can be argued with instead of believed. `--dump` lists the
candidates that were considered and rejected.

This computes the ratio; it does not decide whether a given string is
exempt from the floor. Exemptions are declared by the person claiming them,
where a reviewer sees them (CLAUDE.md, 'Contrast — WCAG AA, ALWAYS').
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

from collections import Counter                          # noqa: E402

import qaimg                                             # noqa: E402

NORMAL = 4.5
LARGE = 3.0
NONTEXT = 3.0


def measure(img, x0, y0, x1, y1, min_frac):
    px = img.region(x0, y0, x1, y1)
    total = len(px)
    counts = Counter(px)
    bg, bgn = counts.most_common(1)[0]
    lbg = qaimg.luminance(bg)
    floor = max(4, int(total * min_frac))

    ranked = sorted(
        ((abs(qaimg.luminance(c) - lbg), c, n) for c, n in counts.items()),
        key=lambda t: (-t[0], t[1]))
    eligible = [t for t in ranked if t[2] >= floor]
    if not eligible:
        return None, bg, bgn, total, floor, ranked
    _, fg, fgn = eligible[0]
    return (fg, fgn), bg, bgn, total, floor, ranked


def report(label, img, x0, y0, x1, y1, min_frac, threshold, tname, dump):
    picked, bg, bgn, total, floor, ranked = measure(img, x0, y0, x1, y1, min_frac)
    print("%s  region %d,%d..%d,%d (%d px)" % (label, x0, y0, x1, y1, total))
    print("    background   %-9s %6d px (%4.1f%%)"
          % (qaimg.hexs(bg), bgn, 100.0 * bgn / total))
    if picked is None:
        print("    NO CANDIDATE color covers the %d px floor (%.2f%% of the region)."
              % (floor, min_frac * 100))
        print("    Either the region is a flat fill, or it is too small/too far off the")
        print("    text to hold a glyph core. Nothing is reported rather than guessed.")
        if dump:
            _dump(ranked, bg)
        return None
    fg, fgn = picked
    r = qaimg.ratio(fg, bg)
    print("    text (core)  %-9s %6d px (%4.1f%%)   floor %d px"
          % (qaimg.hexs(fg), fgn, 100.0 * fgn / total, floor))
    print("    RATIO        %.2f:1     AA %s (%.1f:1) -> %s"
          % (r, tname, threshold, "PASS" if r >= threshold else "FAIL"))
    if dump:
        _dump(ranked, bg)
    return r


def edge(label, img, dark, x0, y0, x1, y1, threshold, tname):
    if (dark.w, dark.h) != (img.w, img.h):
        raise qaimg.ImageError("the two frames differ in size: %dx%d and %dx%d" % (img.w, img.h, dark.w, dark.h))
    ground = qaimg.luminance(Counter(dark.region(x0, y0, x1, y1)).most_common(1)[0][0])
    x0, y0, x1, y1 = max(0, x0), max(0, y0), min(img.w, x1), min(img.h, y1)
    light = [[qaimg.luminance(img.at(i, j)) for i in range(x0, x1)] for j in range(y0, y1)]
    made = [[qaimg.luminance(dark.at(i, j)) for i in range(x0, x1)] for j in range(y0, y1)]
    found = []
    for j in range(1, y1 - y0 - 1):
        for i in range(1, x1 - x0 - 1):
            if made[j][i] < 0.8:
                continue
            beside = [light[j + b][i + a] for a in (-1, 0, 1) for b in (-1, 0, 1)
                      if made[j + b][i + a] <= ground + 0.001]
            if beside:
                text = light[j][i]
                found.append(((max(text, max(beside)) + 0.05) / (min(text, max(beside)) + 0.05), x0 + i, y0 + j))
    print("%s  edge region %d,%d..%d,%d (dark frame's ground %.4f)" % (label, x0, y0, x1, y1, ground))
    if not found:
        print("    NO GLYPH EDGE found: no core pixel (0.8 or more in the dark frame) has an untouched")
        print("    pixel within 1 px. Check the two frames show the same text at the same place.")
        return None
    found.sort()
    low, x, y = found[0]
    print("    stroke pixels beside a non-glyph pixel: %d" % len(found))
    print("    lowest       %.2f:1 at %d,%d (%s)" % (low, x, y, qaimg.hexs(img.at(x, y))))
    print("    1st pct      %.2f:1" % found[len(found) // 100][0])
    print("    RATIO        %.2f:1     AA %s (%.1f:1) -> %s"
          % (low, tname, threshold, "PASS" if low >= threshold else "FAIL"))
    return low


def _dump(ranked, bg):
    print("    considered (furthest from the background first):")
    for d, c, n in ranked[:10]:
        print("      %-9s n=%-7d ratio=%5.2f:1" % (qaimg.hexs(c), n, qaimg.ratio(c, bg)))


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__.strip())
        return 0

    min_frac = 0.005
    if "--min-frac" in argv:
        i = argv.index("--min-frac")
        try:
            min_frac = float(argv[i + 1])
        except (IndexError, ValueError):
            qaimg.die("--min-frac needs a fraction, for example 0.005")
        del argv[i:i + 2]
    dump = "--dump" in argv
    argv = [a for a in argv if a != "--dump"]
    dark = None
    if "--edge" in argv:
        i = argv.index("--edge")
        try:
            dark = qaimg.load(argv[i + 1])
        except IndexError:
            qaimg.die("--edge needs the frame over a dark picture")
        except (OSError, qaimg.ImageError) as exc:
            qaimg.die("--edge: %s" % exc)
        del argv[i:i + 2]

    threshold, tname = NORMAL, "normal text"
    if "--large" in argv:
        threshold, tname = LARGE, "large text (18.66px bold / 24px+)"
        argv = [a for a in argv if a != "--large"]
    if "--nontext" in argv:
        threshold, tname = NONTEXT, "non-text indicator"
        argv = [a for a in argv if a != "--nontext"]

    label = "region"
    if "--label" in argv:
        i = argv.index("--label")
        try:
            label = argv[i + 1]
        except IndexError:
            qaimg.die("--label needs a name")
        del argv[i:i + 2]

    if not argv:
        qaimg.die("nothing to measure. `contrast.py --help`")

    # --- two colors -------------------------------------------------------
    if len(argv) == 2 and not argv[0].lower().endswith(".png"):
        try:
            a = qaimg.parse_color(argv[0])
            b = qaimg.parse_color(argv[1])
        except qaimg.ImageError as exc:
            qaimg.die(str(exc))
        r = qaimg.ratio(a, b)
        print("%s vs %s   RATIO %.2f:1   AA %s (%.1f:1) -> %s"
              % (qaimg.hexs(a), qaimg.hexs(b), r, tname, threshold,
                 "PASS" if r >= threshold else "FAIL"))
        return 0 if r >= threshold else 1

    # --- a frame ----------------------------------------------------------
    png = argv[0]
    if not os.path.isfile(png):
        qaimg.die("no such frame: %s" % png)
    try:
        img = qaimg.load(png)
    except qaimg.ImageError as exc:
        qaimg.die(str(exc))

    specs = []
    rest = argv[1:]
    if rest and rest[0] == "--box":
        if len(rest) < 5:
            qaimg.die("--box needs <x> <y> <w> <h>")
        try:
            x, y, w, h = (int(v) for v in rest[1:5])
        except ValueError:
            qaimg.die("--box takes four integers")
        specs.append((label, x, y, x + w, y + h))
        rest = rest[5:]
    for spec in rest:
        if ":" not in spec:
            qaimg.die("%r is not <label>:<x0>,<y0>,<x1>,<y1>" % spec)
        lab, coords = spec.split(":", 1)
        parts = coords.split(",")
        if len(parts) != 4:
            qaimg.die("%r needs exactly four coordinates" % spec)
        try:
            x0, y0, x1, y1 = (int(v) for v in parts)
        except ValueError:
            qaimg.die("%r has a non-integer coordinate" % spec)
        specs.append((lab, x0, y0, x1, y1))

    if not specs:
        specs.append(("whole frame", 0, 0, img.w, img.h))

    print("%s  (%dx%d)" % (png, img.w, img.h))
    worst = None
    for lab, x0, y0, x1, y1 in specs:
        try:
            if dark is not None:
                r = edge(lab, img, dark, x0, y0, x1, y1, threshold, tname)
            else:
                r = report(lab, img, x0, y0, x1, y1, min_frac, threshold, tname, dump)
        except qaimg.ImageError as exc:
            qaimg.die(str(exc))
        if r is None:
            return 1
        worst = r if worst is None else min(worst, r)
    return 0 if worst is not None and worst >= threshold else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

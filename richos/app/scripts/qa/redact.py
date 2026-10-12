#!/usr/bin/env python3
"""redact.py — black out what must not ship, and PROVE it is gone.

  redact.py <in.png> <out.png> [--addresses] [--phones] [--homes] [--box x,y,w,h ...] [--pad N]
  redact.py --help

  --addresses   find address-shaped text by OCR and cover it (the default when
                none of --box, --phones and --homes is given)
  --phones      find phone-shaped digit runs by OCR and cover them: the same
                pattern ocr-gate.sh calls a shape hit, matched across the words
                of one OCR line, so the gate and the redactor agree
  --homes       find `/Users/<name>`-shaped home paths by OCR and cover them: ocr-gate.sh's
                second SHAPE, verbatim, matched the same way as --phones. Added with the
                candidate 38 walk, whose frames carry the guest's own home path
                (/Users/admin/testvm/...) in attachment lines and work receipts [machine-path-exempt: isolated guest fixture example; no customer default]
  --box         cover an exact rectangle, as many times as you like
  --pad N       grow every OCR-derived box by N pixels (default 3)
  --no-verify   skip the re-scan (and say so in the output)

Exit 0 the output is clean, 1 something survived, 2 the tool could not run.

===========================================================================
UNDER-REDACTION IS THE ONLY FAILURE THAT MATTERS, AND OVER-REDACTION IS THE
ONLY WAY TO BE SURE — BUT IT DESTROYS THE EVIDENCE
===========================================================================
Seven versions of this were written across four walks, and they oscillated
between the two errors. The version that keyed on a bare '@' blacked out
'QA TEST CO' and 'Talk to Rich' — evidence destroyed, nothing proved. The
version that keyed on a strict address pattern missed addresses that
tesseract had split into two tokens across a space.

Both are fixed the same way, and it is the rule here: a word is covered when
IT, or IT JOINED TO ITS NEIGHBOR ON THE SAME OCR LINE, matches a real
address pattern. Line identity comes from tesseract's own block/paragraph/
line numbers, so 'Talk to Rich' cannot be joined into an address and
'jane @ example.com' cannot escape as three tokens.

===========================================================================
A REDACTOR THAT DOES NOT RE-READ ITS OUTPUT IS A CLAIM, NOT A REDACTION
===========================================================================
The last step is always to OCR the OUTPUT and require zero address-shaped
hits. If the box was drawn in the wrong place, or the pad was too small and
a tail of the domain survived, this is what notices. It exits 1 and names
the file rather than printing a count of boxes drawn.
"""

import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import qaimg                                             # noqa: E402
import qaocr                                             # noqa: E402

ADDR = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
LOOSE = re.compile(r"@[A-Za-z0-9.-]+\.[A-Za-z]{2,}|[A-Za-z0-9._%+-]+@")
# ocr-gate.sh's third SHAPE, verbatim: a phone-shaped run of digits.
PHONE = re.compile(r"\+?[0-9][0-9 ()-]{8,}[0-9]")
# ocr-gate.sh's second SHAPE, verbatim: a home path.
HOME = re.compile(r"/Users/[A-Za-z][A-Za-z0-9._-]+")


def phone_boxes(png, pad, pattern=PHONE):
    """Boxes covering every run `pattern` matches (a phone-shaped run unless told otherwise),
    matched over each OCR line's joined words."""
    lines = {}
    for w in qaocr.words(png):
        lines.setdefault(w["line"], []).append(w)
    boxes = []
    for words in lines.values():
        text, spans = "", []
        for w in words:
            if text:
                text += " "
            spans.append((len(text), len(text) + len(w["text"]), w))
            text += w["text"]
        for m in pattern.finditer(text):
            group = [w for a, b, w in spans if a < m.end() and b > m.start()]
            box = (min(g["l"] for g in group) - pad, min(g["t"] for g in group) - pad,
                   max(g["l"] + g["w"] for g in group) + pad, max(g["t"] + g["h"] for g in group) + pad)
            if box not in boxes:
                boxes.append(box)
    return boxes


def address_boxes(png, pad):
    """Boxes covering every genuine address, joining split tokens on one line."""
    rows = qaocr.words(png)
    boxes = []
    for i, w in enumerate(rows):
        group = None
        if ADDR.search(w["text"]):
            group = [w]
        else:
            # Join with the next token on the SAME line, and with the one after
            # it: tesseract splits 'name @ host.tld' into three.
            for span in (2, 3):
                if i + span > len(rows):
                    continue
                grp = rows[i:i + span]
                if any(g["line"] != w["line"] for g in grp):
                    continue
                joined = "".join(g["text"] for g in grp)
                if ADDR.fullmatch(joined) or (LOOSE.search(w["text"]) and ADDR.search(joined)):
                    group = grp
                    break
        if not group:
            continue
        box = (min(g["l"] for g in group) - pad,
               min(g["t"] for g in group) - pad,
               max(g["l"] + g["w"] for g in group) + pad,
               max(g["t"] + g["h"] for g in group) + pad)
        if box not in boxes:
            boxes.append(box)
    return boxes


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__.strip())
        return 0

    pad = 3
    if "--pad" in argv:
        i = argv.index("--pad")
        try:
            pad = int(argv[i + 1])
        except (IndexError, ValueError):
            qaimg.die("--pad needs an integer")
        del argv[i:i + 2]

    verify = True
    if "--no-verify" in argv:
        verify = False
        argv = [a for a in argv if a != "--no-verify"]

    manual = []
    while "--box" in argv:
        i = argv.index("--box")
        try:
            spec = argv[i + 1]
        except IndexError:
            qaimg.die("--box needs x,y,w,h")
        parts = spec.split(",")
        if len(parts) != 4:
            qaimg.die("--box takes exactly x,y,w,h — got %r" % spec)
        try:
            x, y, w, h = (int(p) for p in parts)
        except ValueError:
            qaimg.die("--box has a non-integer value: %r" % spec)
        manual.append((x, y, x + w, y + h))
        del argv[i:i + 2]

    want_phones = "--phones" in argv
    want_homes = "--homes" in argv
    want_addresses = "--addresses" in argv or not (manual or want_phones or want_homes)
    argv = [a for a in argv if a not in ("--addresses", "--phones", "--homes")]

    if len(argv) != 2:
        qaimg.die("usage: redact.py <in.png> <out.png> [--addresses] [--box x,y,w,h]")
    src, dst = argv
    if not os.path.isfile(src):
        qaimg.die("no such frame: %s" % src)

    try:
        img = qaimg.load(src)
    except qaimg.ImageError as exc:
        qaimg.die(str(exc))

    boxes = list(manual)
    if want_addresses:
        try:
            boxes += address_boxes(src, pad)
        except qaocr.OcrUnavailable as exc:
            qaimg.die("%s\nWithout a reader this tool cannot FIND an address, and a "
                      "redactor that covers nothing must not report success." % exc)
    if want_phones:
        try:
            boxes += phone_boxes(src, pad)
        except qaocr.OcrUnavailable as exc:
            qaimg.die("%s\nWithout a reader this tool cannot FIND a phone number, and a "
                      "redactor that covers nothing must not report success." % exc)
    if want_homes:
        try:
            boxes += phone_boxes(src, pad, HOME)
        except qaocr.OcrUnavailable as exc:
            qaimg.die("%s\nWithout a reader this tool cannot FIND a home path, and a "
                      "redactor that covers nothing must not report success." % exc)

    covered = 0
    for x0, y0, x1, y1 in boxes:
        covered += img.fill_box(x0, y0, x1, y1, (0, 0, 0))

    outdir = os.path.dirname(os.path.abspath(dst))
    if outdir and not os.path.isdir(outdir):
        os.makedirs(outdir)
    qaimg.save(img, dst)

    print("%s -> %s" % (src, dst))
    print("  %d box(es), %d pixels covered" % (len(boxes), covered))
    for b in boxes:
        print("    %d,%d..%d,%d" % b)

    if not verify:
        print("  RE-SCAN SKIPPED (--no-verify): nothing here proves the output is clean.")
        return 0

    try:
        seen = qaocr.text(dst)
        left = (ADDR.findall(seen) + (PHONE.findall(seen) if want_phones else [])
                + (HOME.findall(seen) if want_homes else []))
    except qaocr.OcrUnavailable as exc:
        print("  RE-SCAN IMPOSSIBLE: %s" % exc, file=sys.stderr)
        return 2
    if left:
        print("  STILL LEGIBLE: %d address-, phone- or home-path-shaped string(s) survive in %s"
              % (len(left), dst), file=sys.stderr)
        print("  Widen --pad, or add an explicit --box over the region.", file=sys.stderr)
        return 1
    print("  re-scan: no address-shaped%s%s string survives in the output"
          % (" or phone-shaped" if want_phones else "", " or home-path-shaped" if want_homes else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

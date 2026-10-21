#!/usr/bin/env python3
"""The two files of round 36 screen 3, made up, for the phone lab to attach (`lab mac` with
RICHOS_MOBILE_LAB_ATTACH): a two-page PDF and a spreadsheet of comparable deals.

    python3 richos/mobile/dev/lab-files.py OUT_DIR

Writes OUT_DIR/Acme-counteroffer.pdf and OUT_DIR/Acme-comparables.xlsx and prints their paths
joined by `:`, ready for RICHOS_MOBILE_LAB_ATTACH. Standard library only. The PDF is laid out the
way every PDF writer lays one out (objects, a cross-reference table with each byte offset, a
trailer), with real text on each page, so a phone's own PDF reader draws it. The workbook is the
XML Excel writes (shared strings, a named first sheet behind a relationship, a currency and a
percent format), a real spreadsheet for the phone to hand to the app that opens it (Android) or
to Quick Look (iPhone). Nobody's real data.
"""
import sys
import zipfile
from pathlib import Path

DEALS = [
    ("Customer", "Year", "List", "Discount"),
    ("Acme Corp. (ask)", 2026, 240000, 0.08),
    ("Bellweather", 2026, 198000, 0.04),
    ("Corvid Health", 2025, 310000, 0.055),
    ("Dunmore", 2025, 176000, 0.03),
    ("Eastline Freight", 2025, 265000, 0.06),
    ("Fairhaven", 2025, 221000, 0.025),
    ("Granite & Co.", 2024, 289000, 0.05),
    ("Harlow Group", 2024, 204000, 0.035),
]


def pdf(pages):
    """A PDF of Letter pages, each with its lines of text in Helvetica."""
    objects = ["<< /Type /Catalog /Pages 2 0 R >>", None, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    kids = []
    for lines in pages:
        text = "BT /F1 14 Tf 72 720 Td 18 TL " + " ".join(
            "(%s) '" % line.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)") for line in lines) + " ET"
        objects.append("<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text))
        content = len(objects)
        objects.append("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
                       "/Resources << /Font << /F1 3 0 R >> >> >>" % content)
        kids.append("%d 0 R" % len(objects))
    objects[1] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join(kids), len(kids))
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n%s\nendobj\n" % (number, body.encode("latin-1"))
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for offset in offsets:
        out += b"%010d 00000 n \n" % offset
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objects) + 1, xref)
    return bytes(out)


def xlsx(path):
    main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    strings = []

    def shared(text):
        if text not in strings:
            strings.append(text)
        return strings.index(text)

    def esc(text):
        return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

    rows = []
    for r, row in enumerate(DEALS, 1):
        cells = []
        for c, value in enumerate(row):
            ref = "ABCD"[c] + str(r)
            if isinstance(value, str):
                cells.append('<c r="%s" t="s"><v>%d</v></c>' % (ref, shared(value)))
            else:
                style = {2: ' s="1"', 3: ' s="2"'}.get(c, "")
                cells.append('<c r="%s"%s><v>%s</v></c>' % (ref, style, value))
        rows.append('<row r="%d">%s</row>' % (r, "".join(cells)))
    parts = {
        "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>',
        "_rels/.rels": '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="%s/officeDocument" Target="xl/workbook.xml"/></Relationships>' % rel,
        "xl/workbook.xml": '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="%s" xmlns:r="%s"><sheets>'
        '<sheet name="Comparable deals" sheetId="1" r:id="rId1"/></sheets></workbook>' % (main, rel),
        "xl/_rels/workbook.xml.rels": '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="%s/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="%s/styles" Target="styles.xml"/>'
        '<Relationship Id="rId3" Type="%s/sharedStrings" Target="sharedStrings.xml"/></Relationships>' % (rel, rel, rel),
        "xl/styles.xml": '<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="%s">'
        '<numFmts count="1"><numFmt numFmtId="164" formatCode="&quot;$&quot;#,##0"/></numFmts>'
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        '<fills count="1"><fill><patternFill patternType="none"/></fill></fills>'
        '<borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0"/></cellStyleXfs>'
        '<cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164" applyNumberFormat="1"/><xf numFmtId="10" applyNumberFormat="1"/></cellXfs>'
        '</styleSheet>' % main,
        "xl/worksheets/sheet1.xml": '<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="%s"><sheetData>%s</sheetData></worksheet>'
        % (main, "".join(rows)),
    }
    parts["xl/sharedStrings.xml"] = '<?xml version="1.0" encoding="UTF-8"?><sst xmlns="%s" count="%d" uniqueCount="%d">%s</sst>' % (
        main, len(strings), len(strings), "".join("<si><t>%s</t></si>" % esc(s) for s in strings))
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels",
                     "xl/styles.xml", "xl/sharedStrings.xml", "xl/worksheets/sheet1.xml"]:
            z.writestr(name, parts[name])


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = Path(sys.argv[1]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    counter = out / "Acme-counteroffer.pdf"
    counter.write_bytes(pdf([
        ["NORTHWIND TRADERS", "Counteroffer to Acme Corp.", "", "Annual license, list: $240,000",
         "Discount: 3%", "Onboarding fee: waived", "Annual total: $232,800"],
        ["This offer stands until October 31.", "", "For Northwind Traders", "For Acme Corp."],
    ]))
    comps = out / "Acme-comparables.xlsx"
    xlsx(comps)
    print("%s:%s" % (counter, comps))


if __name__ == "__main__":
    main()

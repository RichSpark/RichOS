// glance-scroll-check.swift — is the saved PNG of an automatic downward capture of
// glance-scroll-fixture.swift's sticky page right, pixel by pixel? Built and run on the host by
// glance-scroll-walk.sh.
//
//   glance-scroll-check <png> [scale] [inset] [partial]
//
// The walk's 576 × 362 selection starts 12 px below the top of the page view and 20 px from its left,
// and ends 4 px above its bottom, so a whole capture of the 1990 px page is 576 × 1974 px: 44 rows of
// the translucent header (#1b3a6b at 80% over white), in ONE band at the top; then the page itself,
// where EVERY pixel must be what the page has there (white, a dark block, or the red end block; the
// blocks are worked out again here from the fixture's numbers); then 44 rows of the white bottom bar,
// with its green square once, in its place, and its labels. A pixel of the page that is missing,
// repeated, moved or covered by a piece of a bar fails the check. With `inset` the page is the one whose
// bar stops 70 px short of each side: beside the bar, the last rows must be the page's too. With
// `partial` the capture may have stopped before the end of the page (the app said so and offered what
// it had): the image may be shorter, and every pixel it does hold is checked the same way. EXIT
// STATUS: 0 only when all of these hold.
import CoreGraphics
import Foundation
import ImageIO

let args = CommandLine.arguments
guard args.count > 1, let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    print("FAIL cannot read PNG"); exit(1)
}
let scale = args.dropFirst(2).compactMap { Int($0) }.first ?? 1
let inset = args.dropFirst(2).contains("inset")
let partial = args.dropFirst(2).contains("partial")
let (w, h) = (image.width, image.height)
var rgba = [UInt8](repeating: 0, count: w * h * 4)
rgba.withUnsafeMutableBytes { bytes in
    let context = CGContext(data: bytes.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.setBlendMode(.copy)
    context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
}

typealias Color = (Int, Int, Int)
let white: Color = (255, 255, 255)
let ink: Color = (0x1a, 0x1a, 0x1a)
let red: Color = (0xb0, 0x30, 0x2a)
let green: Color = (0x2e, 0x7d, 0x4f)

/// Whether an image pixel is opaque and within 14 levels of a color.
func has(_ x: Int, _ y: Int, _ target: Color) -> Bool {
    let i = (y * w + x) * 4
    return rgba[i + 3] == 255 && abs(Int(rgba[i]) - target.0) <= 14 && abs(Int(rgba[i + 1]) - target.1) <= 14
        && abs(Int(rgba[i + 2]) - target.2) <= 14
}

func runs(_ rows: [Int]) -> String {
    var ranges: [(Int, Int)] = []
    for row in rows {
        if let last = ranges.last, last.1 + 1 == row { ranges[ranges.count - 1].1 = row } else { ranges.append((row, row)) }
    }
    return ranges.prefix(12).map { "\($0.0)-\($0.1)" }.joined(separator: ",") + (ranges.count > 12 ? ",…" : "")
}

// The page, as the fixture's script lays it out: 56 px of header, 1820 px of blocks, 50 px of red,
// 64 px of white. The same numbers, in the same order, as there.
let (pageWidth, headerHeight, textHeight, endHeight, spacerHeight) = (620, 56, 1820, 50, 64)
let pageHeight = headerHeight + textHeight + endHeight + spacerHeight
var block = [Bool](repeating: false, count: pageWidth * textHeight)
var seed = 7
func random(_ n: Int) -> Int { seed = seed * 48271 % 2147483647; return seed % n }
var top = 10
var line = 0
while top + 12 <= 1810 {
    let last = line % 6 == 5
    let end = last ? 150 + random(300) : 572
    var x = 22
    while true {
        let width = 18 + random(70)
        if x + width > end { break }
        for row in top..<(top + 12) { for column in x..<(x + width) { block[row * pageWidth + column] = true } }
        x += width + 8
    }
    top += last ? 34 : 22
    line += 1
}
func page(_ x: Int, _ y: Int) -> Color {
    if y < headerHeight + textHeight { return block[(y - headerHeight) * pageWidth + x] ? ink : white }
    return y < headerHeight + textHeight + endHeight ? red : white
}

// The selection inside the page view.
let (insetX, insetTop, insetBottom, selectionWidth, barHeight) = (20, 12, 4, 576, 48)
let headerRows = (headerHeight - insetTop) * scale
let barRows = (barHeight - insetBottom) * scale
var ok = true
print("PNG \(w) x \(h) px")
if w != selectionWidth * scale { print("FAIL width \(w), expected \(selectionWidth * scale)"); ok = false }
let wholeHeight = (pageHeight - insetTop - insetBottom) * scale
if partial ? h > wholeHeight || h < 362 * scale : h != wholeHeight {
    print("FAIL height \(h), expected \(partial ? "at most " : "")\(wholeHeight): part of the page is missing or repeated"); ok = false
}
guard ok else { exit(1) }

// The header: 80% of #1b3a6b over white, with its labels, in the first rows and nowhere else.
let headerColor: Color = (73, 97, 137)
let tinted = (0..<h).filter { y in (0..<w).filter { has($0, y, headerColor) }.count * 2 > w }
print("header rows \(runs(tinted))")
if tinted.first != 0 || tinted.last != headerRows - 1 || tinted.count < headerRows - 14 * scale {
    print("FAIL the header must appear once, in the first \(headerRows) rows"); ok = false
}

// The page between the header and the bar: every pixel.
var wrong = 0
var wrongRows: [Int] = []
for y in headerRows..<(h - barRows) {
    var bad = 0
    for x in 0..<w where !has(x, y, page(x / scale + insetX, y / scale + insetTop)) { bad += 1 }
    if bad > 0 { wrong += bad; wrongRows.append(y) }
}
print("page rows \(headerRows)..<\(h - barRows): \(wrong) wrong or missing pixels\(wrong > 0 ? " in rows \(runs(wrongRows))" : "")")
if wrong > 0 { print("FAIL the page between the header and the bar must be whole and nothing else"); ok = false }

// The bar: white, its green square at its place and nowhere else in the image, its dark labels. Beside
// an inset bar, the page.
let barColumns = inset ? (70 - insetX) * scale..<(pageWidth - 70 - insetX) * scale : 0..<w
let square = CGRect(x: barColumns.lowerBound + 30 * scale - (inset ? 0 : insetX * scale), y: h - barRows + 18 * scale,
                    width: 12 * scale, height: 12 * scale)
var beside = 0
// The four corner pixels of every capture so far, before the stitcher checked anything too, are a
// shade off what is under them: the frames themselves carry it at the corners of the selection. In a
// whole-width bar they pass as gray; beside an inset bar they are counted here and named, not failed.
var corners = 0
var greenInside = 0
var greenOutside = 0
var labels = 0
var strange = 0
for y in 0..<h {
    for x in 0..<w {
        let inSquare = square.contains(CGPoint(x: x, y: y))
        if has(x, y, green) { if inSquare { greenInside += 1 } else { greenOutside += 1 } }
        guard y >= h - barRows, !inSquare else { continue }
        guard barColumns.contains(x) else {
            if !has(x, y, page(x / scale + insetX, y / scale + insetTop)) {
                if (x == 0 || x == w - 1) && y == h - 1 { corners += 1 } else { beside += 1 }
            }
            continue
        }
        let i = (y * w + x) * 4
        let (r, g, b) = (Int(rgba[i]), Int(rgba[i + 1]), Int(rgba[i + 2]))
        if max(r, g, b) < 110 { labels += 1 }
        // Black on white text is gray all the way; anything colored is not the bar.
        if rgba[i + 3] != 255 || max(r, g, b) - min(r, g, b) > 40 { strange += 1 }
    }
}
print("bar rows \(h - barRows)..<\(h): green square \(greenInside) of \(Int(square.width * square.height)) px, green elsewhere \(greenOutside) px, label pixels \(labels), other colors \(strange) px, wrong pixels beside it \(beside)\(corners > 0 ? ", corner pixels of the selection off the page's color \(corners)" : "")")
if greenInside != Int(square.width * square.height) || greenOutside != 0 || labels < 60 * scale * scale || strange != 0 || beside != 0 {
    print("FAIL the bottom bar must appear once, whole, in the last \(barRows) rows"); ok = false
}
if !ok { exit(1) }
if h < wholeHeight {
    print("PASS the header appears once at the top, the bar once at the bottom, and every pixel of the page between them is the page's; the capture stopped at row \(h) of \(wholeHeight)")
} else {
    print("PASS the header appears once at the top, the bar once at the bottom, and every pixel of the page between them is the page's; the capture ran to the end of the page")
}

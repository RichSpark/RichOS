// glance-scroll-check.swift — is the saved PNG of an automatic downward capture of
// glance-scroll-fixture.swift's sticky page whole? Built and run on the host by glance-scroll-walk.sh.
//
//   glance-scroll-check <png> [scale]
//
// The translucent header (#1b3a6b at 80% over white) forms ONE band, at the top; the bottom bar
// (#2e7d4f) forms ONE band, in the last rows; the red end marker (#b0302a) sits just above the bar,
// so the capture ran to the end of the page; and the seven body markers each appear once, in order,
// 250 px apart, so no part of the page between was skipped or repeated. EXIT STATUS: 0 only when all
// of these hold.
import CoreGraphics
import Foundation
import ImageIO

let args = CommandLine.arguments
guard args.count > 1, let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    print("FAIL cannot read PNG"); exit(1)
}
let scale = args.count > 2 ? Int(args[2])! : 1
let (w, h) = (image.width, image.height)
var rgba = [UInt8](repeating: 0, count: w * h * 4)
rgba.withUnsafeMutableBytes { bytes in
    let context = CGContext(data: bytes.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
}

/// The share of a row's pixels (every eighth one, between columns `from` and `to`) within 14 levels
/// of a color.
func share(_ y: Int, _ target: (Int, Int, Int), from: Int = 0, to: Int = w) -> Double {
    let xs = Array(stride(from: from, to: min(to, w), by: 8))
    let hits = xs.filter { x in
        let i = (y * w + x) * 4
        return abs(Int(rgba[i]) - target.0) <= 14 && abs(Int(rgba[i + 1]) - target.1) <= 14
            && abs(Int(rgba[i + 2]) - target.2) <= 14
    }.count
    return Double(hits) / Double(xs.count)
}

/// Runs of rows mostly in the color. A row or two of text showing through the translucent header
/// does not split it.
func bands(_ target: (Int, Int, Int), from: Int = 0, to: Int = w) -> [[Int]] {
    var result: [[Int]] = []
    for y in 0..<h where share(y, target, from: from, to: to) > 0.5 {
        if let last = result.last, y <= last[1] + 1 + 4 * scale { result[result.count - 1][1] = y } else { result.append([y, y]) }
    }
    return result
}

// 80% of #1b3a6b over white.
let header = bands((73, 97, 137))
let bar = bands((0x2e, 0x7d, 0x4f))
let end = bands((0xb0, 0x30, 0x2a))
print("PNG \(w) x \(h) px; header bands \(header); bottom bar bands \(bar); end marker bands \(end)")
var ok = true
if w != 576 * scale { print("FAIL width \(w)"); ok = false }
if header.count != 1 || header[0][0] != 0 { print("FAIL the header must appear once, at the top: \(header)"); ok = false }
if bar.count != 1 || bar[0][1] < h - 1 - 2 * scale { print("FAIL the bottom bar must appear once, at the bottom: \(bar)"); ok = false }
if let marker = end.last, let barTop = bar.first?[0] {
    if marker[1] >= barTop || barTop - marker[1] > 80 * scale { print("FAIL the end marker must sit just above the bar"); ok = false }
} else {
    print("FAIL the end marker must be in the image"); ok = false
}
// The body markers: page x 520...580, which is 500...560 in the 576-pixel crop from page x 20.
let marks = (0..<7).map { bands((230, 30 + 25 * $0, 140), from: 504 * scale, to: 556 * scale) }
print("body markers \(marks)")
for (index, found) in marks.enumerated() where found.count != 1 {
    print("FAIL body marker \(index + 1) must appear once: \(found)"); ok = false
}
if marks.allSatisfy({ $0.count == 1 }) {
    for index in 1..<marks.count where abs(marks[index][0][0] - marks[index - 1][0][0] - 250 * scale) > 2 * scale {
        print("FAIL body markers \(index) and \(index + 1) must be 250 px apart: part of the page is missing or repeated"); ok = false
    }
}
if !ok { exit(1) }
print("PASS the header appears once at the top, the bar once at the bottom, the page between whole; the capture ran to the end of the page")

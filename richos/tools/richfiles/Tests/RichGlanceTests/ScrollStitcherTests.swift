import CoreGraphics
import CoreText
import XCTest
@testable import RichGlance

final class ScrollStitcherTests: XCTestCase {
    func testVerticalCaptureDoesNotRepeatOverlap() throws {
        let document = fixture(width: 120, height: 280)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 120, h: 140)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 57, w: 120, h: 140)), .moved(dx: 0, dy: 57))
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 140, w: 120, h: 140)), .moved(dx: 0, dy: 83))
        try check(sut, against: document)
    }

    func testHorizontalCaptureCanScrollBackPastTheStart() throws {
        let document = fixture(width: 257, height: 120)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 70, y: 0, w: 130, h: 120)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 127, y: 0, w: 130, h: 120)), .moved(dx: 57, dy: 0))
        XCTAssertEqual(sut.append(crop(document, x: 70, y: 0, w: 130, h: 120)), .moved(dx: -57, dy: 0))
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 130, h: 120)), .moved(dx: -70, dy: 0))
        try check(sut, against: document)
    }

    func testBothDirectionsCoverTheFullGridAfterAlternatingSweeps() throws {
        let document = fixture(width: 185, height: 176)
        var sut = ScrollStitcher()
        for (x, y) in [(0, 0), (65, 0), (65, 56), (0, 56)] {
            _ = sut.append(crop(document, x: x, y: y, w: 120, h: 120))
        }
        try check(sut, against: document)
    }

    func testDiagonalPanLeavesUnvisitedCornersTransparent() throws {
        let document = fixture(width: 174, height: 166)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 120, h: 120)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 54, y: 46, w: 120, h: 120)), .moved(dx: 54, dy: 46))
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, 174)
        XCTAssertEqual(actual.height, 166)
        XCTAssertEqual(pixels(crop(actual, x: 0, y: 0, w: 120, h: 120)),
                       pixels(crop(document, x: 0, y: 0, w: 120, h: 120)))
        XCTAssertEqual(pixels(crop(actual, x: 54, y: 46, w: 120, h: 120)),
                       pixels(crop(document, x: 54, y: 46, w: 120, h: 120)))
        XCTAssertTrue(pixels(crop(actual, x: 120, y: 0, w: 54, h: 46)).allSatisfy { $0 == 0 })
    }

    func testShortEdgeStepOnWideTableIsStillRegistered() throws {
        let width = 1091
        let height = 362
        var values: [UInt32] = []
        for y in 0..<height {
            let row = y / 31
            for x in 0..<width {
                let column = x / 137
                if y % 31 == 30 || x % 137 == 136 {
                    values.append(0xffff_ffff)
                } else {
                    let r = UInt32((row * 17 + column * 31 + 157) % 125 + 120)
                    let g = UInt32((row * 53 + column * 11 + 83) % 125 + 120)
                    let b = UInt32((row * 71 + column * 91 + 117) % 125 + 120)
                    values.append(0xff00_0000 | r << 16 | g << 8 | b)
                }
            }
        }
        let document = image(width: width, height: height, pixels: values)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 576, h: height)), .initial)
        for x in [162, 324, 486] {
            XCTAssertEqual(sut.append(crop(document, x: x, y: 0, w: 576, h: height)), .moved(dx: 162, dy: 0))
        }
        XCTAssertEqual(sut.append(crop(document, x: 515, y: 0, w: 576, h: height)), .moved(dx: 29, dy: 0))
        try check(sut, against: document)
    }

    func testDuplicateAndUnrelatedFramesCannotCreateFalseSeams() throws {
        let document = fixture(width: 156, height: 220)
        let first = crop(document, x: 0, y: 0, w: 156, h: 120)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(first), .initial)
        XCTAssertEqual(sut.append(first), .unchanged)
        XCTAssertEqual(sut.append(fixture(width: 156, height: 120, salt: 111)), .unmatched)
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 61, w: 156, h: 120)), .moved(dx: 0, dy: 61))
        try check(sut, against: crop(document, x: 0, y: 0, w: 156, h: 181))
    }

    func testFixedHeaderDoesNotStopAutomaticScrollAndAppearsOnce() throws {
        // A sticky site header stays put while the text under it scrolls. Steps are 28% of
        // the viewport, as automatic scrolling sends them.
        let page = textPage(width: 800, height: 2000)
        let viewport = 500
        let step = 140
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: 0, w: 800, h: viewport), rows: 64)), .initial)
        var y = 0
        while y + viewport < page.height {
            let next = min(page.height - viewport, y + step)
            XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: next, w: 800, h: viewport), rows: 64)),
                           .moved(dx: 0, dy: next - y), "step to y=\(next)")
            y = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, 800)
        XCTAssertEqual(actual.height, page.height)
        // The first view, header included, then the page below it without another header.
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: viewport))
                      == pixels(withHeader(crop(page, x: 0, y: 0, w: 800, h: viewport), rows: 64)))
        XCTAssertTrue(pixels(crop(actual, x: 0, y: viewport, w: 800, h: page.height - viewport))
                      == pixels(crop(page, x: 0, y: viewport, w: 800, h: page.height - viewport)))
    }

    private func check(_ sut: ScrollStitcher, against expected: CGImage,
                       file: StaticString = #filePath, line: UInt = #line) throws {
        let actual = try XCTUnwrap(sut.image(), file: file, line: line)
        XCTAssertEqual(actual.width, expected.width, file: file, line: line)
        XCTAssertEqual(actual.height, expected.height, file: file, line: line)
        XCTAssertTrue(pixels(actual) == pixels(expected), "Stitched pixels differ from the fixture.", file: file, line: line)
    }

    private let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private let bitmap = CGBitmapInfo(rawValue: CGBitmapInfo.byteOrder32Little.rawValue
                                     | CGImageAlphaInfo.premultipliedFirst.rawValue)

    private func fixture(width: Int, height: Int, salt: UInt32 = 31337) -> CGImage {
        var state = salt
        var values: [UInt32] = []
        for _ in 0..<(width * height) {
            state ^= state << 13
            state ^= state >> 17
            state ^= state << 5
            values.append(0xff00_0000 | (state & 0x00ff_ffff))
        }
        return image(width: width, height: height, pixels: values)
    }

    private func image(width: Int, height: Int, pixels: [UInt32]) -> CGImage {
        let data = pixels.withUnsafeBytes { Data($0) }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                       space: colorSpace, bitmapInfo: bitmap, provider: CGDataProvider(data: data as CFData)!,
                       decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
    }

    /// Dark rendered text on a white page, as on a document or web page.
    private func textPage(width: Int, height: Int, scale: CGFloat = 1) -> CGImage {
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        let words = ["the", "quarterly", "report", "shows", "revenue", "growth", "across", "regions", "and", "teams",
                     "with", "margin", "notes", "for", "each", "customer", "segment", "plus", "forecast"]
        var seed: UInt32 = 7
        var text = ""
        for index in 0..<(height * width / 250) {
            seed = seed &* 1_103_515_245 &+ 12_345
            text += words[Int(seed >> 16) % words.count] + (index % 13 == 12 ? ".\n" : " ")
        }
        let attributes: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, 13 * scale, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 0.1, green: 0.1, blue: 0.1, alpha: 1)
        ]
        let setter = CTFramesetterCreateWithAttributedString(NSAttributedString(string: text, attributes: attributes))
        let path = CGPath(rect: CGRect(x: 20 * scale, y: 0, width: CGFloat(width) - 40 * scale, height: CGFloat(height)),
                          transform: nil)
        CTFrameDraw(CTFramesetterCreateFrame(setter, CFRange(location: 0, length: 0), path, nil), context)
        return context.makeImage()!
    }

    /// A light gray header bar with bold labels and a bottom border over the top rows of a view.
    private func withHeader(_ view: CGImage, rows: Int) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(red: 0.96, green: 0.96, blue: 0.96, alpha: 1))
        context.fill(CGRect(x: 0, y: view.height - rows, width: view.width, height: rows))
        context.setFillColor(CGColor(red: 0.8, green: 0.8, blue: 0.8, alpha: 1))
        context.fill(CGRect(x: 0, y: view.height - rows, width: view.width, height: 1))
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "Home      Products      Pricing      Docs", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica-Bold" as CFString, 12, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 0.2, green: 0.2, blue: 0.2, alpha: 1)
        ]))
        context.textPosition = CGPoint(x: 20, y: CGFloat(view.height - rows) + CGFloat(rows) * 0.35)
        CTLineDraw(line, context)
        return context.makeImage()!
    }

    private func crop(_ image: CGImage, x: Int, y: Int, w: Int, h: Int) -> CGImage {
        image.cropping(to: CGRect(x: x, y: y, width: w, height: h))!
    }

    private func pixels(_ image: CGImage) -> [UInt32] {
        var bytes = [UInt32](repeating: 0, count: image.width * image.height)
        bytes.withUnsafeMutableBytes { buffer in
            let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height,
                                    bitsPerComponent: 8, bytesPerRow: image.width * 4, space: colorSpace,
                                    bitmapInfo: bitmap.rawValue)!
            context.setBlendMode(.copy)
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        return bytes
    }
}

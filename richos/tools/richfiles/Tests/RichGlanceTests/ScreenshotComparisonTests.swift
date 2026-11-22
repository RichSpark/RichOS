import AppKit
import XCTest
@testable import RichGlance

final class ScreenshotComparisonTests: XCTestCase {
    func testOneChangedPixelHasExactCountAndTopLeftCoordinates() throws {
        let before = try fixture()
        let after = try fixture(change: CGPoint(x: 27, y: 5))
        let comparison = try XCTUnwrap(ScreenshotComparison.pixels(before: before, after: after))
        XCTAssertEqual(comparison.changedPixels, 1)
        XCTAssertEqual(comparison.areas, [CGRect(x: 27, y: 5, width: 1, height: 1)])
    }

    func testComparisonRetainsRedactionAndIgnoresMarkup() throws {
        let before = try fixture()
        let after = try fixture(change: CGPoint(x: 27, y: 5))
        var left = ScreenshotDocument(original: before, scale: 1)
        let cover = ScreenshotMark(tool: .redact, start: CGPoint(x: 25, y: 3), end: CGPoint(x: 30, y: 9))
        left.edits.marks = [cover]
        var right = ScreenshotDocument(original: after, scale: 1)
        right.edits.marks = [cover, ScreenshotMark(tool: .box, start: .zero, end: CGPoint(x: 19, y: 11))]
        let result = try XCTUnwrap(ScreenshotComparison.pixels(
            before: XCTUnwrap(ScreenshotRenderer.forComparison(left)),
            after: XCTUnwrap(ScreenshotRenderer.forComparison(right))))
        XCTAssertEqual(result.changedPixels, 0)
    }

    func testRepeatDoesNotClipAndRecomputesPhysicalPixelsOnlyOnScaleChange() {
        let box = CGRect(x: 13, y: 33, width: 101, height: 57)
        let region = ScreenshotRepeatTarget(id: UUID(), display: "fixture", pixels: box, scale: 2)
        XCTAssertEqual(region.crop(at: 2, within: CGSize(width: 400, height: 300)), box)
        XCTAssertEqual(region.crop(at: 1, within: CGSize(width: 200, height: 150)),
                       CGRect(x: 7, y: 17, width: 51, height: 29))
        XCTAssertNil(region.crop(at: 2, within: CGSize(width: 100, height: 300)))
    }

    private func fixture(change: CGPoint? = nil) throws -> CGImage {
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: 36, height: 24, bitsPerComponent: 8, bytesPerRow: 0,
                                            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(NSColor.white.cgColor)
        context.fill(CGRect(x: 0, y: 0, width: 36, height: 24))
        if let change {
            context.setFillColor(NSColor.red.cgColor)
            context.fill(CGRect(x: change.x, y: 23 - change.y, width: 1, height: 1))
        }
        return try XCTUnwrap(context.makeImage())
    }
}

import AppKit
import XCTest
@testable import RichFilesCore

final class GlanceEyeTests: XCTestCase {
    /// The menu bar item is the app icon's eye: lids and eyeball inked, the opening and the
    /// corners clear (a viewfinder's corner brackets would ink them), centered, as a template.
    func testMenuBarImageIsTheAppIconEyeAsATemplate() throws {
        let image = GlanceEye.templateImage()
        XCTAssertTrue(image.isTemplate, "macOS tints a template for a light or dark menu bar")
        XCTAssertEqual(image.size, NSSize(width: 18, height: 18))

        // 18 points at 2x, as the menu bar draws it on a Retina display: 36 / 290 px per unit.
        let size = 36
        let rep = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
                                                 bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                                 colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        rep.size = image.size
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        image.draw(in: NSRect(origin: .zero, size: image.size))
        NSGraphicsContext.restoreGraphicsState()
        let data = try XCTUnwrap(rep.bitmapData)
        let alpha = { (x: Int, y: Int) in Int(data[y * rep.bytesPerRow + x * 4 + 3]) }

        XCTAssertEqual(alpha(18, 18), 255, "the eyeball")
        XCTAssertEqual(alpha(26, 18), 0, "the opening beside the eyeball")
        XCTAssertGreaterThan(alpha(34, 17), 200, "the corner where the lids meet")
        XCTAssertEqual(alpha(18, 9), 255, "one lid")
        XCTAssertEqual(alpha(18, 26), 255, "the other lid")
        XCTAssertEqual(alpha(18, 4), 0, "clear beyond the lids")
        for (x, y) in [(1, 1), (34, 1), (1, 34), (34, 34)] {
            XCTAssertEqual(alpha(x, y), 0, "the corner at (\(x), \(y)) is clear")
        }
        for column in [true, false] {
            let ink = (0..<size * size).filter { alpha($0 % size, $0 / size) > 0 }.map { column ? $0 % size : $0 / size }
            let near = try XCTUnwrap(ink.min())
            let far = size - 1 - (try XCTUnwrap(ink.max()))
            XCTAssertLessThanOrEqual(abs(near - far), 1, "ink margins \(near) and \(far)")
        }
    }
}

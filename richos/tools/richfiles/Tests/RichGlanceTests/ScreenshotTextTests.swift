import AppKit
import XCTest
@testable import RichGlance

final class ScreenshotTextTests: XCTestCase {
    func testRedactionKeepsWordsOnEitherSideAndInTheOtherColumn() throws {
        let size = CGSize(width: 1120, height: 110)
        let image = NSImage(size: size, flipped: false) { rect in
            NSColor.white.setFill()
            rect.fill()
            let attributes: [NSAttributedString.Key: Any] = [
                .foregroundColor: NSColor.black, .font: NSFont.monospacedSystemFont(ofSize: 32, weight: .semibold),
            ]
            ("From: PRIVATE approved" as NSString).draw(at: CGPoint(x: 12, y: 36), withAttributes: attributes)
            ("Invoice total" as NSString).draw(at: CGPoint(x: 780, y: 36), withAttributes: attributes)
            return true
        }
        let bitmap = try XCTUnwrap(image.cgImage(forProposedRect: nil, context: nil, hints: nil))
        let redaction = CGRect(x: 150, y: 18, width: 95, height: 63)
        let results = try ScreenshotText.recognize(.init(image: bitmap, redactions: [redaction]))
        XCTAssertTrue(results.contains { $0.matches("From") }, "\(results)")
        XCTAssertTrue(results.contains { $0.matches("approved") }, "\(results)")
        XCTAssertTrue(results.contains { $0.matches("Invoice total") }, "\(results)")
        XCTAssertFalse(results.contains { $0.matches("PRIVATE") }, "\(results)")
        XCTAssertFalse(results.contains { redaction.intersects($0.bounds) }, "\(results)")
    }

    func testVisionReadsScannedPixels() throws {
        let image = NSImage(size: CGSize(width: 480, height: 100), flipped: false) { rect in
            NSColor.white.setFill()
            rect.fill()
            ("ERR-4471" as NSString).draw(at: NSPoint(x: 12, y: 18),
                                         withAttributes: [.foregroundColor: NSColor.black,
                                                          .font: NSFont.monospacedSystemFont(ofSize: 48, weight: .semibold)])
            return true
        }
        let pixels = try XCTUnwrap(image.cgImage(forProposedRect: nil, context: nil, hints: nil))
        let lines = try ScreenshotText.recognize(.init(image: pixels))
        XCTAssertTrue(lines.contains { $0.matches("ERR-4471") }, "\(lines)")
    }
}

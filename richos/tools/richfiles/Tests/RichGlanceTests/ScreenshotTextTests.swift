import AppKit
import XCTest
@testable import RichGlance

final class ScreenshotTextTests: XCTestCase {
    func testRedactionDropsLineFragmentsAndKeepsUnaffectedLine() {
        let wholeLine = [
            ScreenshotText.Line(text: "Ticket:", bounds: CGRect(x: 1, y: 15, width: 35, height: 10)),
            ScreenshotText.Line(text: "failed", bounds: CGRect(x: 90, y: 15, width: 40, height: 10)),
            ScreenshotText.Line(text: "Try again later", bounds: CGRect(x: 1, y: 28, width: 120, height: 10))
        ]
        let visible = ScreenshotText.visibleLines(wholeLine, redactions: [CGRect(x: 40, y: 13, width: 46, height: 13)])
        XCTAssertEqual(visible.map(\.text), ["Try again later"])
        XCTAssertTrue(visible[0].matches("AGAIN"))
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

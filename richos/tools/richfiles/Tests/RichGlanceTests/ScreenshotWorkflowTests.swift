import AppKit
import ImageIO
import XCTest
@testable import RichGlance

@MainActor
final class ScreenshotWorkflowTests: XCTestCase {
    func testFlattenedRedactionReplacesPixelsAfterCropAndOverlappingMarks() async throws {
        let original = try fixture()
        var document = ScreenshotDocument(original: original, scale: 2)
        document.edits.crop = CGRect(x: 2, y: 1, width: 9, height: 7)
        document.edits.marks = [
            ScreenshotMark(tool: .redact, start: CGPoint(x: 4, y: 2), end: CGPoint(x: 9, y: 7)),
            ScreenshotMark(tool: .highlight, start: CGPoint(x: 5, y: 3), end: CGPoint(x: 8, y: 6), ink: .yellow),
        ]
        let flat = try XCTUnwrap(ScreenshotRenderer.flattened(document))
        let png = try XCTUnwrap(ScreenshotRenderer.png(flat))
        let source = try XCTUnwrap(CGImageSourceCreateWithData(png as CFData, nil))
        XCTAssertEqual(CGImageSourceGetCount(source), 1)
        let decoded = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(decoded.width, 9)
        XCTAssertEqual(decoded.height, 7)
        for y in 1..<6 {
            for x in 2..<7 {
                let pixel = try sample(decoded, x: x, y: y)
                XCTAssertEqual(pixel, [12, 19, 34, 255], "pixel (\(x), \(y))")
            }
        }
        XCTAssertEqual(try sample(decoded, x: 0, y: 0), try sample(original, x: 2, y: 1))
        XCTAssertEqual(try sample(decoded, x: 8, y: 6), try sample(original, x: 10, y: 7))
        XCTAssertNotEqual(try sample(original, x: 5, y: 4), [12, 19, 34, 255])
    }

    func testStepOrderIsEditableAndContiguousAfterDeletion() async throws {
        let start = CGPoint(x: 3, y: 5)
        let a = ScreenshotMark(tool: .steps, start: start, end: start)
        let arrow = ScreenshotMark(tool: .arrow, start: .zero, end: start)
        let b = ScreenshotMark(tool: .steps, start: start, end: start)
        let c = ScreenshotMark(tool: .steps, start: start, end: start)
        var edits = ScreenshotEdits(crop: .zero, marks: [a, arrow, b, c])
        edits.reorderStep(c.id, by: -1)
        XCTAssertEqual(edits.marks.map(\.id), [a.id, arrow.id, c.id, b.id])
        edits.marks.removeAll { $0.id == a.id }
        XCTAssertEqual(edits.number(for: c), 1)
        XCTAssertEqual(edits.number(for: b), 2)
        XCTAssertNil(edits.number(for: arrow))
    }

    func testHistoryIsOptInExpiresAndCannotRecreateDeletedEdits() async throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("glance-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let store = ScreenshotStore(root: home)
        let now = Date()
        var document = ScreenshotDocument(original: try fixture(), scale: 2, created: now)
        XCTAssertFalse(store.keepsHistory)
        XCTAssertFalse(try store.retain(document))
        XCTAssertFalse(FileManager.default.fileExists(atPath: home.path))
        try store.setHistory(true)
        XCTAssertTrue(try store.retain(document))
        let mark = ScreenshotMark(tool: .redact, start: CGPoint(x: 1, y: 2), end: CGPoint(x: 7, y: 5))
        document.edits.marks = [mark]
        XCTAssertTrue(try store.retain(document, existingOnly: true))
        let entry = try XCTUnwrap(store.entries().first)
        let reopened = try XCTUnwrap(store.load(entry))
        XCTAssertEqual(reopened.edits.marks, [mark])
        XCTAssertEqual(try sample(reopened.original, x: 3, y: 3), try sample(document.original, x: 3, y: 3))
        try store.remove(document.id)
        XCTAssertFalse(try store.retain(document, existingOnly: true))
        XCTAssertTrue(try store.entries().isEmpty)
        XCTAssertTrue(try store.retain(document))
        XCTAssertTrue(try store.entries(now: now.addingTimeInterval(7 * 86_400)).isEmpty)
        XCTAssertFalse(try store.retain(document, existingOnly: true))
        XCTAssertTrue(try store.retain(document))
        try store.setHistory(false)
        XCTAssertFalse(store.keepsHistory)
        let directory = home.appendingPathComponent("Recent Captures")
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.path))
    }

    private func fixture() throws -> CGImage {
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let bitmap = try XCTUnwrap(CGContext(data: nil, width: 14, height: 12, bitsPerComponent: 8,
                                             bytesPerRow: 0, space: space,
                                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        for row in 0..<12 {
            bitmap.setFillColor(red: 0.8, green: CGFloat(row + 1) / 15, blue: 0.2, alpha: 1)
            bitmap.fill(CGRect(x: 0, y: row, width: 14, height: 1))
        }
        return try XCTUnwrap(bitmap.makeImage())
    }

    private func sample(_ image: CGImage, x: Int, y: Int) throws -> [UInt8] {
        let pixel = try XCTUnwrap(image.cropping(to: CGRect(x: x, y: y, width: 1, height: 1)))
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        var bytes = [UInt8](repeating: 0, count: 4)
        try bytes.withUnsafeMutableBytes { storage in
            let context = try XCTUnwrap(CGContext(data: storage.baseAddress, width: 1, height: 1, bitsPerComponent: 8,
                                                   bytesPerRow: 4, space: space,
                                                   bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(pixel, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        }
        return bytes
    }
}

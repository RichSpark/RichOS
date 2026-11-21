import AppKit
import ImageIO
import XCTest
@testable import RichGlance

@MainActor
final class ScreenshotWorkflowTests: XCTestCase {
    func testEditMenuLetsTheFocusedResponderHandleTextShortcuts() async throws {
        let menu = Application.makeEditMenu()
        let commands: [(String, String, String, NSEvent.ModifierFlags)] = [
            ("Undo", "undo:", "z", .command),
            ("Redo", "redo:", "z", [.command, .shift]),
            ("Cut", "cut:", "x", .command),
            ("Copy", "copy:", "c", .command),
            ("Paste", "paste:", "v", .command),
            ("Select All", "selectAll:", "a", .command),
        ]
        for (title, action, key, modifiers) in commands {
            let item = try XCTUnwrap(menu.item(withTitle: title))
            XCTAssertNil(item.target, title)
            XCTAssertEqual(item.action, NSSelectorFromString(action), title)
            XCTAssertEqual(item.keyEquivalent, key, title)
            XCTAssertEqual(item.keyEquivalentModifierMask, modifiers, title)
        }
    }

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
        let clipboard = NSPasteboard.withUniqueName()
        defer { clipboard.releaseGlobally() }
        XCTAssertTrue(ScreenshotRenderer.copy(png, to: clipboard))
        // macOS synthesizes TIFF and legacy aliases for the same flattened bitmap.
        let rasterTypes: Set<String> = ["public.png", "Apple PNG pasteboard type", "public.tiff", "NeXT TIFF v4.0 pasteboard type"]
        let types = try XCTUnwrap(clipboard.types)
        XCTAssertTrue(types.allSatisfy { rasterTypes.contains($0.rawValue) }, "\(types)")
        XCTAssertEqual(clipboard.pasteboardItems?.count, 1)
        XCTAssertEqual(clipboard.data(forType: .png), png)
        if let tiff = clipboard.data(forType: .tiff) {
            let tiffSource = try XCTUnwrap(CGImageSourceCreateWithData(tiff as CFData, nil))
            let tiffImage = try XCTUnwrap(CGImageSourceCreateImageAtIndex(tiffSource, 0, nil))
            XCTAssertEqual(tiffImage.width, 9)
            XCTAssertEqual(tiffImage.height, 7)
            for y in 1..<6 {
                for x in 2..<7 {
                    XCTAssertEqual(try sample(tiffImage, x: x, y: y), [12, 19, 34, 255])
                }
            }
        }
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

    func testHistoryDefaultsOnExpiresAndCannotRecreateDeletedEdits() async throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("glance-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let store = ScreenshotStore(root: home)
        let now = Date()
        var document = ScreenshotDocument(original: try fixture(), scale: 2, kind: .scrolling, created: now)
        XCTAssertTrue(store.keepsHistory)
        XCTAssertFalse(FileManager.default.fileExists(atPath: home.path))
        XCTAssertTrue(try store.retain(document))
        let mark = ScreenshotMark(tool: .redact, start: CGPoint(x: 1, y: 2), end: CGPoint(x: 7, y: 5))
        document.edits.marks = [mark]
        XCTAssertTrue(try store.retain(document, existingOnly: true))
        let entry = try XCTUnwrap(store.entries().first)
        let reopened = try XCTUnwrap(store.load(entry))
        XCTAssertEqual(reopened.kind, .scrolling)
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

    func testShelfVisibilityAndRetentionAreIndependentAcrossReopen() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("glance-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let store = ScreenshotStore(root: home)
        let now = Date()
        let document = ScreenshotDocument(original: try fixture(), scale: 1, created: now, name: "A capture")
        XCTAssertTrue(store.showsShelf)
        XCTAssertEqual(store.retention, .week)
        try store.setShelfVisible(false)
        try store.setRetention(.forever)
        XCTAssertTrue(try store.retain(document))
        let reopened = ScreenshotStore(root: home)
        XCTAssertFalse(reopened.showsShelf)
        XCTAssertTrue(reopened.keepsHistory)
        XCTAssertEqual(reopened.retention, .forever)
        let future = now.addingTimeInterval(10 * 86_400)
        XCTAssertEqual(try reopened.entries(now: future).first?.name, "A capture")
        try reopened.setShelfVisible(true)
        try reopened.setRetention(.week, now: future)
        XCTAssertTrue(reopened.showsShelf)
        XCTAssertTrue(try reopened.entries(now: future).isEmpty)
        try reopened.setHistory(false)
        XCTAssertFalse(ScreenshotStore(root: home).keepsHistory)
        XCTAssertTrue(ScreenshotStore(root: home).showsShelf)
        XCTAssertFalse(try reopened.retain(document))
    }

    func testRound100ToolDefaultsAndIndependentStyles() async throws {
        let canvas = ScreenshotCanvas(document: ScreenshotDocument(original: try fixture(), scale: 1))
        canvas.tool = .arrow
        XCTAssertTrue(canvas.large)
        XCTAssertEqual(canvas.ink, .red)
        canvas.setStyle(ink: .blue, large: false)
        canvas.tool = .highlight
        XCTAssertEqual(canvas.ink, .yellow)
        canvas.setStyle(ink: .black)
        canvas.tool = .box
        XCTAssertFalse(canvas.large)
        XCTAssertEqual(canvas.ink, .blue)
        canvas.setStyle(large: true)
        canvas.tool = .arrow
        XCTAssertFalse(canvas.large)
        canvas.tool = .highlight
        XCTAssertEqual(canvas.ink, .black)
        canvas.tool = .steps
        XCTAssertFalse(canvas.large)
    }

    func testSameEditorKeepsWorkingMarksAndUndoAsCapturesChange() async throws {
        _ = NSApplication.shared
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("glance-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let store = ScreenshotStore(root: home)
        var first = ScreenshotDocument(original: try fixture(), scale: 1)
        let second = ScreenshotDocument(original: try fixture(), scale: 1, kind: .scrolling)
        let mark = ScreenshotMark(tool: .steps, start: CGPoint(x: 4, y: 4), end: CGPoint(x: 4, y: 4))
        first.edits.marks = [mark]
        XCTAssertTrue(try store.retain(first))
        XCTAssertTrue(try store.retain(second))
        let editor = ScreenshotEditor(store: store)
        let window = editor.window
        editor.changeDocument(first)
        editor.canvas?.selected = mark.id
        editor.canvas?.nudge(dx: 1, dy: 0)
        editor.changeDocument(second)
        let saved = try XCTUnwrap(store.entries().first { $0.id == first.id })
        editor.changeDocument(try XCTUnwrap(store.load(saved)))
        XCTAssertTrue(editor.window === window)
        XCTAssertEqual(editor.canvas?.document.edits.marks.first?.start.x, 5)
        XCTAssertEqual(editor.canvas?.canUndo, true)
        editor.canvas?.undo()
        XCTAssertEqual(editor.canvas?.document.edits.marks.first?.start.x, 4)
        XCTAssertTrue(editor.allowApplicationQuit())
        let persisted = try XCTUnwrap(store.entries().first { $0.id == first.id })
        XCTAssertEqual(persisted.edits.marks.first?.start.x, 4)
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

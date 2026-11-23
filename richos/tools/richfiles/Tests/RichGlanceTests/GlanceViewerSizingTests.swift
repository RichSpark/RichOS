import AppKit
import ImageIO
import UniformTypeIdentifiers
import XCTest
import RichFilesCore
@testable import RichGlance

@MainActor
final class GlanceViewerSizingTests: XCTestCase {
    func test72DPIUsesOnePointPerImagePixelAtBothBackingScales() throws {
        let image = try png(dpi: 72)
        try assertViewerSize(image, window: CGSize(width: 330, height: 240), points: CGSize(width: 240, height: 160))
        try assertViewerSize(image, window: CGSize(width: 180, height: 160), points: CGSize(width: 180, height: 120))
    }

    func test144DPIUsesHalfAPointPerImagePixelAtBothBackingScales() throws {
        let image = try png(dpi: 144)
        try assertViewerSize(image, window: CGSize(width: 150, height: 120), points: CGSize(width: 120, height: 80))
        try assertViewerSize(image, window: CGSize(width: 90, height: 110), points: CGSize(width: 90, height: 60))
    }

    func testOpenedRetinaImageRendersLargeArrowAtCaptureScale() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".png")
        defer { try? FileManager.default.removeItem(at: url) }
        try writePNG(dpi: 144, to: url)
        let viewer = Viewer(preferences: GlanceInfoPreferences())
        defer { viewer.stop() }
        viewer.open(url)
        let decoded = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            MainActor.assumeIsolated { viewer.canEditImage }
        }, object: nil)
        // Wait for the production decoder to deliver the image to the viewer.
        wait(for: [decoded], timeout: 5)
        var opened: ScreenshotDocument?
        viewer.openEditor = { opened = $0 }
        viewer.editImage()
        var file = try XCTUnwrap(opened)
        var capture = ScreenshotDocument(original: file.original, scale: 2)
        let arrow = ScreenshotMark(tool: .arrow, start: CGPoint(x: 20, y: 80),
                                   end: CGPoint(x: 218, y: 80), ink: .red, large: true)
        file.edits.marks = [arrow]
        capture.edits.marks = [arrow]
        XCTAssertEqual(file.scale, capture.scale)
        let filePNG = try XCTUnwrap(ScreenshotRenderer.flattened(file).flatMap(ScreenshotRenderer.png))
        let capturePNG = try XCTUnwrap(ScreenshotRenderer.flattened(capture).flatMap(ScreenshotRenderer.png))
        XCTAssertEqual(filePNG, capturePNG, "The opened file and a fresh 2× capture should export identical arrows")
    }

    private func png(dpi: Int) throws -> GlanceImage {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".png")
        defer { try? FileManager.default.removeItem(at: url) }
        try writePNG(dpi: dpi, to: url)
        return try GlanceImage.decode(url)
    }

    private func writePNG(dpi: Int, to url: URL) throws {
        let context = try XCTUnwrap(CGContext(
            data: nil, width: 240, height: 160, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
        ))
        context.setFillColor(red: 1, green: 0, blue: 1, alpha: 1)
        context.fill(CGRect(x: 0, y: 0, width: context.width, height: context.height))
        let bitmap = try XCTUnwrap(context.makeImage())
        let output = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(output, bitmap, [kCGImagePropertyDPIWidth: dpi, kCGImagePropertyDPIHeight: dpi] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(output))
    }

    private func assertViewerSize(_ image: GlanceImage, window: CGSize, points: CGSize) throws {
        let suite = "glance-sizing-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let canvas = Canvas(preferences: GlanceInfoPreferences(defaults: defaults))
        canvas.frame = CGRect(origin: .zero, size: window)
        canvas.show(image, filename: "Sizing fixture")
        canvas.layoutSubtreeIfNeeded()

        for scale in [1, 2] {
            let bitmap = try XCTUnwrap(NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: Int(window.width) * scale, pixelsHigh: Int(window.height) * scale,
                bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
            ))
            bitmap.size = window
            canvas.cacheDisplay(in: canvas.bounds, to: bitmap)
            func isImage(_ x: Int, _ y: Int) -> Bool {
                guard let color = bitmap.colorAt(x: x, y: y) else { return false }
                return color.redComponent > 0.75 && color.greenComponent < 0.25 && color.blueComponent > 0.75
            }
            // Sample within the picture, to the right of the viewer buttons even at small sizes.
            let sampleX = bitmap.pixelsWide / 2 + Int(points.width * CGFloat(scale) / 3)
            let width = (0..<bitmap.pixelsWide).filter { isImage($0, bitmap.pixelsHigh / 2) }.count
            let height = (0..<bitmap.pixelsHigh).filter { isImage(sampleX, $0) }.count
            XCTAssertEqual(width, Int(points.width) * scale, "image width at \(scale)x")
            XCTAssertEqual(height, Int(points.height) * scale, "image height at \(scale)x")
        }
    }
}

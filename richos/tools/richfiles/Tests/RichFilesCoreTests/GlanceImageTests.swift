import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import RichFilesCore

final class GlanceImageTests: XCTestCase {
    func testDecodesNewImageTypesAndChoosesLargestIcon() throws {
        let cases: [(String, CGSize)] = [
            ("sample.avif", CGSize(width: 40, height: 24)),
            ("sample.bmp", CGSize(width: 40, height: 24)),
            ("sample.heic", CGSize(width: 40, height: 24)),
            ("sample.icns", CGSize(width: 128, height: 128)),
            ("sample.ico", CGSize(width: 64, height: 64)),
            ("sample.jpe", CGSize(width: 40, height: 24)),
            ("sample.tif", CGSize(width: 40, height: 24)),
            ("still.webp", CGSize(width: 40, height: 24))
        ]
        for (name, size) in cases {
            let image = try GlanceImage.decode(fixture(name))
            XCTAssertEqual(image.size, size, name)
            XCTAssertEqual(CGSize(width: image.bitmap.width, height: image.bitmap.height), size, name)
        }
        let heif = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".heif")
        try FileManager.default.copyItem(at: fixture("sample.heic"), to: heif)
        defer { try? FileManager.default.removeItem(at: heif) }
        XCTAssertEqual(try GlanceImage.decode(heif).size, CGSize(width: 40, height: 24))
        XCTAssertNil(try GlanceImage.decode(fixture("sample.tif")).animation)
        XCTAssertNil(try GlanceImage.decode(fixture("still.webp")).animation)
    }

    func testDecodesFinderAliasesAndPackedDIB() throws {
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: temporary) }
        let hif = temporary.appendingPathComponent("camera.HIF")
        try FileManager.default.copyItem(at: fixture("sample.heic"), to: hif)
        XCTAssertEqual(try GlanceImage.decode(hif).size, CGSize(width: 40, height: 24))
        let dib = temporary.appendingPathComponent("packed.dib")
        let bmp = try Data(contentsOf: fixture("sample.bmp"))
        try bmp.dropFirst(14).write(to: dib)
        XCTAssertEqual(try GlanceImage.decode(dib).size, CGSize(width: 40, height: 24))
        try bmp.dropFirst(14).prefix(20).write(to: dib)
        XCTAssertThrowsError(try GlanceImage.decode(dib))
    }

    func testGIFAndWebPDecodeCompositedFramesAndTiming() throws {
        for name in ["delta.gif", "delta.webp"] {
            let first = try GlanceImage.decode(fixture(name))
            let motion = try XCTUnwrap(first.animation, name)
            XCTAssertEqual(motion.frameCount, 3, name)
            XCTAssertEqual(motion.loopCount, 2, name)
            XCTAssertEqual(motion.initialDuration, 0.08, accuracy: 0.001, name)
            XCTAssertEqual(try dominantChannel(in: first.bitmap, x: 20, y: 12), 0, name)
            for (index, expectedDuration) in [(1, 0.16), (2, 0.24)] {
                let frame = try motion.frame(at: index)
                XCTAssertEqual(frame.duration, expectedDuration, accuracy: 0.001, name)
                XCTAssertEqual(CGSize(width: frame.bitmap.width, height: frame.bitmap.height), first.size, name)
                // The changed patch advances while the background from frame one survives.
                XCTAssertEqual(try dominantChannel(in: frame.bitmap, x: 20, y: 12), index, name)
                XCTAssertEqual(try dominantChannel(in: frame.bitmap, x: 2, y: 2), 0, name)
            }
            XCTAssertThrowsError(try motion.frame(at: 3), name)
        }
    }

    private func dominantChannel(in image: CGImage, x: Int, y: Int) throws -> Int {
        let sample = try XCTUnwrap(image.cropping(to: CGRect(x: x, y: y, width: 1, height: 1)))
        var rgba = [UInt8](repeating: 0, count: 4)
        return try rgba.withUnsafeMutableBytes { bytes in
            let context = try XCTUnwrap(CGContext(data: bytes.baseAddress, width: 1, height: 1, bitsPerComponent: 8,
                                                bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(),
                                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
            XCTAssertEqual(bytes[3], 255)
            return try XCTUnwrap((0..<3).max(by: { bytes[$0] < bytes[$1] }))
        }
    }

    func testDocumentClaimsMatchTheOpenPanelAndExcludeSVG() throws {
        let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let plist = try PropertyListSerialization.propertyList(
            from: Data(contentsOf: package.appendingPathComponent("RichGlance-Info.plist")), format: nil
        ) as? [String: Any]
        let claims = try XCTUnwrap((plist?["CFBundleDocumentTypes"] as? [[String: Any]])?.first)
        XCTAssertEqual(claims["CFBundleTypeRole"] as? String, "Viewer")
        XCTAssertEqual(claims["LSHandlerRank"] as? String, "Alternate")
        XCTAssertEqual(Set(try XCTUnwrap(claims["LSItemContentTypes"] as? [String])),
                       Set(GlanceFiles.contentTypes.map(\.identifier)))
        XCTAssertFalse(GlanceFiles.supports(URL(fileURLWithPath: "/example.svg")))
    }

    private func fixture(_ name: String) throws -> URL {
        try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: nil, subdirectory: "Glance"))
    }

    func testFolderListsOnlyImmediateVisibleImagesInFilenameOrder() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        for name in ["Bravo.jpeg", "Zulu.JPG", "alpha.png", "c.HEIC", "d.WebP", "e.avif", "f.gif",
                     "g.tiff", "h.bmp", "i.ico", "j.icns", "notes.txt", "vector.svg", ".hidden.png"] {
            try Data().write(to: root.appendingPathComponent(name))
        }
        try FileManager.default.createDirectory(at: root.appendingPathComponent("Folder.png"), withIntermediateDirectories: false)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("link.jpg"),
                                                   withDestinationURL: root.appendingPathComponent("Zulu.JPG"))
        let urls = try GlanceFiles.folder(containing: root.appendingPathComponent("Bravo.jpeg"))
        XCTAssertEqual(urls.map(\.lastPathComponent), ["alpha.png", "Bravo.jpeg", "c.HEIC", "d.WebP", "e.avif",
                                                        "f.gif", "g.tiff", "h.bmp", "i.ico", "j.icns", "Zulu.JPG"])
    }

    func testDecodesRotatedJPEGAndRefusesMislabeledImage() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let image = try XCTUnwrap(CGContext(data: nil, width: 48, height: 24, bitsPerComponent: 8,
                                           bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                           bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)?.makeImage())
        let jpeg = directory.appendingPathComponent("landscape.jpeg")
        let output = try XCTUnwrap(CGImageDestinationCreateWithURL(jpeg as CFURL, UTType.jpeg.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(output, image, [kCGImagePropertyOrientation: 6] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(output))
        let decoded = try GlanceImage.decode(jpeg)
        XCTAssertEqual(decoded.size, CGSize(width: 24, height: 48))
        XCTAssertEqual(decoded.bitmap.width, 24)
        XCTAssertEqual(decoded.bitmap.height, 48)

        let mislabeled = directory.appendingPathComponent("mislabeled.png")
        let gif = try XCTUnwrap(CGImageDestinationCreateWithURL(mislabeled as CFURL, UTType.gif.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(gif, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(gif))
        XCTAssertThrowsError(try GlanceImage.decode(mislabeled))
    }

    func testZoomKeepsImagePointUnderPointerAndPanStopsAtEdge() {
        var viewport = GlanceViewport()
        viewport.reset(imageSize: CGSize(width: 1_200, height: 900), in: CGSize(width: 600, height: 450))
        let pointer = CGPoint(x: 240, y: 180)
        let anchor = viewport.imagePoint(at: pointer)
        viewport.zoom(by: 2, at: pointer, holding: anchor)
        XCTAssertTrue(viewport.canPan)
        XCTAssertEqual(viewport.imagePoint(at: pointer), anchor)
        viewport.pan(by: CGPoint(x: -10_000, y: 10_000))
        XCTAssertEqual(viewport.imageRect.maxX, 600)
        XCTAssertEqual(viewport.imageRect.minY, 0)

        viewport.reset(imageSize: CGSize(width: 300, height: 900), in: CGSize(width: 600, height: 450))
        XCTAssertFalse(viewport.canPan)
        XCTAssertEqual(viewport.imageRect, CGRect(x: 225, y: 0, width: 150, height: 450))
        viewport.resize(to: CGSize(width: 600, height: 900))
        XCTAssertEqual(viewport.imageRect, CGRect(x: 150, y: 0, width: 300, height: 900))
    }

    func testViewerStartsAtActualSizeOrFitsLargerImagesAndResetsOnReopen() {
        var viewport = GlanceViewport()
        let window = CGSize(width: 600, height: 400)
        let small = CGSize(width: 300, height: 200)
        let wide = CGSize(width: 1_200, height: 300)
        viewport.reset(imageSize: small, in: window)
        XCTAssertEqual(viewport.imageRect, CGRect(x: 150, y: 100, width: 300, height: 200))
        XCTAssertFalse(viewport.isFit)

        viewport.zoom(by: 100, at: .zero, holding: .zero)
        XCTAssertEqual(viewport.scale, 20)
        viewport.zoom(by: 0.001, at: .zero, holding: .zero)
        XCTAssertEqual(viewport.scale, 0.05)
        viewport.resize(to: CGSize(width: 400, height: 300))
        XCTAssertEqual(viewport.scale, 0.05)
        viewport.reset(imageSize: small, in: window)
        XCTAssertEqual(viewport.scale, 1)

        viewport.reset(imageSize: wide, in: window)
        XCTAssertEqual(viewport.scale, 0.5)
        XCTAssertTrue(viewport.isFit)
        viewport.zoom(by: 0.5, at: .zero, holding: .zero)
        XCTAssertEqual(viewport.scale, 0.25)
        XCTAssertFalse(viewport.isFit)
        viewport.reset(imageSize: wide, in: window)
        XCTAssertEqual(viewport.scale, 0.5)
        XCTAssertTrue(viewport.isFit)
    }
}

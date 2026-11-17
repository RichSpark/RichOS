import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import RichFilesCore

final class GlanceImageTests: XCTestCase {
    func testFolderListsOnlyImmediateVisibleImagesInFilenameOrder() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        for name in ["Bravo.jpeg", "Zulu.JPG", "alpha.png", "notes.txt", ".hidden.png"] {
            try Data().write(to: root.appendingPathComponent(name))
        }
        try FileManager.default.createDirectory(at: root.appendingPathComponent("Folder.png"), withIntermediateDirectories: false)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("link.jpg"),
                                                   withDestinationURL: root.appendingPathComponent("Zulu.JPG"))
        let urls = try GlanceFiles.folder(containing: root.appendingPathComponent("Bravo.jpeg"))
        XCTAssertEqual(urls.map(\.lastPathComponent), ["alpha.png", "Bravo.jpeg", "Zulu.JPG"])
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
}

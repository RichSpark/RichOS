import AppKit
import ImageIO
import RichFilesCore
import UniformTypeIdentifiers
import XCTest

final class WorkspaceServicesTests: XCTestCase {
    func testImageActionsPublishSeparateRotatedAndConvertedFiles() throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let folder = URL(fileURLWithPath: temporary, isDirectory: true)
            .appendingPathComponent("RichFilesImages-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }

        let sourceURL = folder.appendingPathComponent("Screenshot.png")
        try writePNG(to: sourceURL)
        let original = try Data(contentsOf: sourceURL)
        let source = try FileReference(url: sourceURL)
        let rotation = try ImageQuickAction.rotateLeft.apply(to: source, in: folder)
        let rotated = try XCTUnwrap(CGImageSourceCreateWithURL(rotation.copied.url as CFURL, nil))
        let pixels = try XCTUnwrap(CGImageSourceCreateImageAtIndex(rotated, 0, nil))
        XCTAssertEqual(pixels.width, 24)
        XCTAssertEqual(pixels.height, 32)

        let conversion = try ImageQuickAction.jpeg.apply(to: source, in: folder)
        let jpeg = try XCTUnwrap(CGImageSourceCreateWithURL(conversion.copied.url as CFURL, nil))
        XCTAssertEqual(CGImageSourceGetType(jpeg) as String?, UTType.jpeg.identifier)
        let repeatConversion = try ImageQuickAction.jpeg.apply(to: source, in: folder)
        XCTAssertNotEqual(repeatConversion.copied.url, conversion.copied.url)
        XCTAssertEqual(try Data(contentsOf: sourceURL), original)
    }

    func testReadsFileAssociationAndRendersAnActualPNGThumbnail() async throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let folder = URL(fileURLWithPath: temporary, isDirectory: true)
            .appendingPathComponent("RichFilesWorkspace-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }

        let text = folder.appendingPathComponent("fixture.txt")
        try Data("synthetic fixture".utf8).write(to: text)
        let apps = try WorkspaceServices.applications(for: text)
        let preferred = try XCTUnwrap(apps.defaultApplication)
        let alternatives = apps.alternatives.map { $0.url.resolvingSymlinksInPath() }
        XCTAssertEqual(Set(alternatives).count, alternatives.count)
        XCTAssertFalse(alternatives.contains(preferred.url.resolvingSymlinksInPath()))

        let png = folder.appendingPathComponent("fixture.png")
        try writePNG(to: png)
        let thumbnail = try await WorkspaceServices.thumbnail(for: png, size: CGSize(width: 32, height: 32))
        XCTAssertGreaterThan(thumbnail.width, 0)
        XCTAssertGreaterThan(thumbnail.height, 0)
    }

    private func writePNG(to png: URL) throws {
        let context = try XCTUnwrap(CGContext(
            data: nil, width: 32, height: 24, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.setFillColor(red: 0.2, green: 0.6, blue: 0.4, alpha: 1)
        context.fill(CGRect(x: 0, y: 0, width: 32, height: 24))
        let image = try XCTUnwrap(context.makeImage())
        let output = try XCTUnwrap(CGImageDestinationCreateWithURL(png as CFURL, UTType.png.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(output, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(output))
    }
}

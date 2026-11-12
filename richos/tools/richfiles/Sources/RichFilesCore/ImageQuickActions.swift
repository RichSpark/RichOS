import CoreImage
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

public enum ImageQuickAction: String, CaseIterable, Identifiable, Sendable {
    case rotateLeft = "Rotate Left and Save Copy"
    case png = "Convert Copy to PNG"
    case jpeg = "Convert Copy to JPEG"

    public var id: Self { self }

    public func apply(to file: FileReference, in folder: URL) throws -> CopyReceipt {
        let (data, version) = try readImage(file)
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              CGImageSourceGetCount(source) == 1,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0, width <= 40_000_000 / height,
              let pixels = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw ImageActionError.unsupportedImage
        }
        let sourceOrientation = (properties[kCGImagePropertyOrientation] as? UInt32)
            .flatMap(CGImagePropertyOrientation.init(rawValue:)) ?? .up
        var image = CIImage(cgImage: pixels).oriented(sourceOrientation)
        if self == .rotateLeft { image = image.oriented(.left) }
        let outputType: UTType
        switch self {
        case .png: outputType = .png
        case .jpeg: outputType = .jpeg
        case .rotateLeft: outputType = CGImageSourceGetType(source) as String? == UTType.jpeg.identifier ? .jpeg : .png
        }
        let context = CIContext()
        guard let rendered = context.createCGImage(image, from: image.extent) else {
            throw ImageActionError.unsupportedImage
        }

        let manager = FileManager.default
        let stage = try manager.url(for: .itemReplacementDirectory, in: .userDomainMask,
                                    appropriateFor: folder, create: true)
        defer { try? manager.removeItem(at: stage) }
        let basename = file.url.deletingPathExtension().lastPathComponent
        let suffix = self == .rotateLeft ? " Rotated" : " Converted"
        let name = basename + suffix + (outputType == .jpeg ? ".jpg" : ".png")
        let output = stage.appendingPathComponent(name)
        guard let writer = CGImageDestinationCreateWithURL(output as CFURL, outputType.identifier as CFString, 1, nil) else {
            throw ImageActionError.couldNotSave
        }
        var options: [CFString: Any] = [kCGImageDestinationOrientation: CGImagePropertyOrientation.up.rawValue]
        if outputType == .jpeg {
            options[kCGImageDestinationLossyCompressionQuality] = 0.95
            options[kCGImageDestinationBackgroundColor] = CGColor(gray: 1, alpha: 1)
        }
        let metadata = CGImageSourceCopyMetadataAtIndex(source, 0, nil)
        CGImageDestinationAddImageAndMetadata(writer, rendered, metadata, options as CFDictionary)
        guard CGImageDestinationFinalize(writer) else { throw ImageActionError.couldNotSave }
        let parent = try OpenFolder(file.url.deletingLastPathComponent())
        guard try FileVersion(parent.verifiedStatus(of: file)) == version else {
            throw FileOperationError.sourceChanged(file.url)
        }
        return try FileOperations.copy(FileReference(url: output), into: folder, conflictPolicy: .keepBoth)
    }

    private func readImage(_ file: FileReference) throws -> (Data, FileVersion) {
        let maximumBytes = 128 * 1024 * 1024
        let parent = try OpenFolder(file.url.deletingLastPathComponent())
        let initial = try parent.verifiedStatus(of: file)
        guard initial.st_mode & S_IFMT == S_IFREG, initial.st_size > 0, initial.st_size <= maximumBytes,
              !TextContentReader.isDataless(initial) else { throw ImageActionError.unsupportedImage }
        let descriptor = openat(parent.descriptor, file.url.lastPathComponent, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw ImageActionError.couldNotRead }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var opened = stat()
        let expected = FileVersion(initial)
        guard fstat(descriptor, &opened) == 0, FileVersion(opened) == expected,
              let data = try handle.read(upToCount: maximumBytes + 1), data.count == initial.st_size,
              fstat(descriptor, &opened) == 0, FileVersion(opened) == expected else {
            throw FileOperationError.sourceChanged(file.url)
        }
        return (data, expected)
    }

    private enum ImageActionError: LocalizedError {
        case unsupportedImage, couldNotRead, couldNotSave

        var errorDescription: String? {
            switch self {
            case .unsupportedImage: return "Choose a downloaded, single-frame image up to 128 MB and 40 megapixels."
            case .couldNotRead: return "RichFiles could not read this image."
            case .couldNotSave: return "RichFiles could not save the image copy."
            }
        }
    }
}

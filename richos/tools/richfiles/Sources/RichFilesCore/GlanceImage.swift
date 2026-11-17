import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

public enum GlanceFiles {
    public static func supports(_ url: URL) -> Bool {
        url.isFileURL && ["png", "jpg", "jpeg"].contains(url.pathExtension.lowercased())
    }

    public static func folder(containing file: URL) throws -> [URL] {
        let file = file.standardizedFileURL
        let keys: Set<URLResourceKey> = [.isRegularFileKey, .isSymbolicLinkKey]
        let entries = try FileManager.default.contentsOfDirectory(
            at: file.deletingLastPathComponent(), includingPropertiesForKeys: Array(keys),
            options: .skipsHiddenFiles
        )
        var images = entries.filter { candidate in
            guard supports(candidate), let values = try? candidate.resourceValues(forKeys: keys) else { return false }
            return values.isRegularFile == true && values.isSymbolicLink != true
        }
        if !images.contains(where: { $0.standardizedFileURL.path == file.path }) { images.append(file) }
        return images.sorted {
            let left = $0.lastPathComponent
            let right = $1.lastPathComponent
            let order = left.localizedStandardCompare(right)
            return order == .orderedSame ? left.utf8.lexicographicallyPrecedes(right.utf8) : order == .orderedAscending
        }
    }
}

public struct GlanceImage {
    public enum Failure: Error { case unavailable, unreadable }

    public let bitmap: CGImage
    public let size: CGSize

    public static func decode(_ url: URL) throws -> Self {
        // Opening a directory, device or named pipe as an image must never block the decoder.
        guard (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else {
            throw Failure.unavailable
        }
        guard GlanceFiles.supports(url),
              let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let kind = CGImageSourceGetType(source) as String?, [UTType.png.identifier, UTType.jpeg.identifier].contains(kind),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = props[kCGImagePropertyPixelWidth] as? Double,
              let height = props[kCGImagePropertyPixelHeight] as? Double,
              width.isFinite, height.isFinite, width >= 1, height >= 1 else {
            throw Failure.unreadable
        }
        let orientation = props[kCGImagePropertyOrientation] as? Int ?? 1
        let size = (5...8).contains(orientation) ? CGSize(width: height, height: width) : CGSize(width: width, height: height)
        // Preserve normal screenshots at full resolution. Bound the decoded surface for very large files.
        let longest = max(width, height)
        let reduction = min(1, 8192 / longest, sqrt(32_000_000 / width / height))
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: max(1, Int(longest * reduction)),
            kCGImageSourceShouldCacheImmediately: true
        ]
        guard let bitmap = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary),
              CGImageSourceGetStatusAtIndex(source, 0) == .statusComplete else { throw Failure.unreadable }
        return Self(bitmap: bitmap, size: size)
    }
}

public struct GlanceViewport {
    public private(set) var imageSize: CGSize = .zero
    public private(set) var size: CGSize = .zero
    public private(set) var scale: CGFloat = 1
    public private(set) var origin: CGPoint = .zero
    public private(set) var isFit = true

    public var imageRect: CGRect {
        CGRect(origin: origin, size: CGSize(width: imageSize.width * scale, height: imageSize.height * scale))
    }
    public var canPan: Bool { imageRect.width > size.width + 0.5 || imageRect.height > size.height + 0.5 }

    private var fit: CGFloat {
        guard imageSize.width > 0, imageSize.height > 0, size.width > 0, size.height > 0 else { return 1 }
        return min(size.width / imageSize.width, size.height / imageSize.height)
    }

    public init() {}

    public mutating func reset(imageSize: CGSize, in size: CGSize) {
        self.imageSize = imageSize
        self.size = size
        isFit = true
        clamp()
    }

    public mutating func resize(to size: CGSize) {
        origin.x += (size.width - self.size.width) / 2
        origin.y += (size.height - self.size.height) / 2
        self.size = size
        clamp()
    }

    public func imagePoint(at point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - origin.x) / scale, y: (point.y - origin.y) / scale)
    }

    public mutating func zoom(by factor: CGFloat, at pointer: CGPoint, holding anchor: CGPoint) {
        scale = min(max(8, fit), max(fit, scale * factor))
        isFit = scale <= fit * 1.0005
        origin = CGPoint(x: pointer.x - anchor.x * scale, y: pointer.y - anchor.y * scale)
        clamp()
    }

    public mutating func pan(by delta: CGPoint) {
        guard canPan else { return }
        origin.x += delta.x
        origin.y += delta.y
        clamp()
    }

    private mutating func clamp() {
        if isFit || scale <= fit { scale = fit; isFit = true }
        let image = imageRect.size
        func offset(_ old: CGFloat, _ extent: CGFloat, _ window: CGFloat) -> CGFloat {
            extent <= window ? (window - extent) / 2 : min(0, max(window - extent, old))
        }
        origin = CGPoint(x: offset(origin.x, image.width, size.width),
                         y: offset(origin.y, image.height, size.height))
    }
}

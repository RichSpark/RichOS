import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

public enum GlanceFormat: String, CaseIterable, Identifiable {
    case png = "public.png"
    case jpeg = "public.jpeg"
    case heic = "public.heic"
    case heif = "public.heif"
    case webp = "org.webmproject.webp"
    case avif = "public.avif"
    case gif = "com.compuserve.gif"
    case tiff = "public.tiff"
    case bmp = "com.microsoft.bmp"
    case ico = "com.microsoft.ico"
    case icns = "com.apple.icns"

    public var id: String { rawValue }
    public var contentType: UTType? { UTType(rawValue) }

    public var title: String {
        switch self {
        case .png: "PNG"
        case .jpeg: "JPEG"
        case .heic: "HEIC"
        case .heif: "HEIF"
        case .webp: "WebP"
        case .avif: "AVIF"
        case .gif: "GIF"
        case .tiff: "TIFF"
        case .bmp: "BMP"
        case .ico: "ICO"
        case .icns: "ICNS"
        }
    }

    public var extensions: [String] {
        switch self {
        case .png: ["png"]
        case .jpeg: ["jpg", "jpeg", "jpe"]
        case .heic: ["heic"]
        case .heif: ["heif", "hif"]
        case .webp: ["webp"]
        case .avif: ["avif"]
        case .gif: ["gif"]
        case .tiff: ["tiff", "tif"]
        case .bmp: ["bmp", "dib"]
        case .ico: ["ico"]
        case .icns: ["icns"]
        }
    }

    private static let decoderTypes = Set(CGImageSourceCopyTypeIdentifiers() as? [String] ?? [])

    /// Launch Services changes the type as a whole, including its extension aliases.
    public var associationProblem: String? {
        guard let contentType, Self.decoderTypes.contains(rawValue) else {
            return "This version of macOS has no decoder for \(title)."
        }
        let aliases = contentType.tags[.filenameExtension] ?? []
        let unsupported = aliases.map { $0.lowercased() }.filter { !extensions.contains($0) }
        guard unsupported.isEmpty else {
            let names = unsupported.map { ".\($0)" }.joined(separator: ", ")
            return "RichGlance cannot open \(names), which macOS includes in this type."
        }
        return nil
    }

    private static let byExtension = Dictionary(uniqueKeysWithValues:
        allCases.flatMap { format in format.extensions.map { ($0, format) } }
    )

    static func forFile(_ url: URL) -> Self? {
        guard url.isFileURL else { return nil }
        return byExtension[url.pathExtension.lowercased()]
    }

    func accepts(_ decoded: Self) -> Bool {
        self == decoded || (self == .heic || self == .heif) && (decoded == .heic || decoded == .heif)
    }
}

public enum GlanceFiles {
    public static let contentTypes: [UTType] = GlanceFormat.allCases.compactMap { UTType($0.rawValue) }

    public static func supports(_ url: URL) -> Bool {
        GlanceFormat.forFile(url) != nil
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
    /// Oriented source dimensions in pixels, even when the decoded bitmap is smaller.
    public let size: CGSize
    public let animation: GlanceAnimation?

    public static func decode(_ url: URL) throws -> Self {
        // Opening a directory, device or named pipe as an image must never block the decoder.
        guard (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else {
            throw Failure.unavailable
        }
        guard let expected = GlanceFormat.forFile(url), let source = imageSource(url),
              let kind = CGImageSourceGetType(source) as String?, let format = GlanceFormat(rawValue: kind),
              expected.accepts(format), CGImageSourceGetCount(source) > 0 else { throw Failure.unreadable }

        let index: Int
        switch format {
        case .heic, .heif: index = CGImageSourceGetPrimaryImageIndex(source)
        case .ico, .icns: index = largestRepresentation(in: source)
        default: index = 0
        }
        let animation = GlanceAnimation(source: source, format: format)
        let first = try decodeFrame(in: source, at: index, pixelLimit: animation == nil ? 32_000_000 : GlanceAnimation.pixelLimit)
        return Self(bitmap: first.bitmap, size: first.size, animation: animation)
    }

    private static func imageSource(_ url: URL) -> CGImageSource? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        if let source = CGImageSourceCreateWithURL(url as CFURL, options), CGImageSourceGetType(source) != nil {
            return source
        }
        guard url.pathExtension.lowercased() == "dib", let bitmap = packedBitmap(url) else { return nil }
        return CGImageSourceCreateWithData(bitmap as CFData, options)
    }

    /// A packed Windows DIB has the pixel and palette data of a BMP without its 14-byte file header.
    private static func packedBitmap(_ url: URL) -> Data? {
        guard let values = try? url.resourceValues(forKeys: [.fileSizeKey]),
              let fileSize = values.fileSize, (12...128 * 1024 * 1024).contains(fileSize),
              let dib = try? Data(contentsOf: url, options: [.mappedIfSafe]), dib.count == fileSize else { return nil }
        func uint16(_ offset: Int) -> Int {
            Int(dib[offset]) | (Int(dib[offset + 1]) << 8)
        }
        func uint32(_ offset: Int) -> Int {
            uint16(offset) | (uint16(offset + 2) << 16)
        }
        let headerSize = uint32(0)
        let bitmapOffset: Int
        switch headerSize {
        case 12:
            let depth = uint16(10)
            guard [1, 4, 8, 24].contains(depth) else { return nil }
            bitmapOffset = headerSize + (depth <= 8 ? (1 << depth) * 3 : 0)
        case 40, 52, 56, 108, 124:
            guard dib.count >= headerSize else { return nil }
            let depth = uint16(14)
            let compression = uint32(16)
            let colors = uint32(32)
            let masks = headerSize == 40 && (compression == 3 || compression == 6) ? (compression == 3 ? 12 : 16) : 0
            // Do not delegate linked color profiles, which reference another file, to Image I/O.
            if headerSize == 124 && uint32(56) == 0x4C494E4B { return nil }
            let palette = colors == 0 && (1...8).contains(depth) ? 1 << depth : colors
            bitmapOffset = headerSize + masks + palette * 4
        default:
            return nil
        }
        guard bitmapOffset < dib.count else { return nil }
        var bmp = Data([0x42, 0x4D])
        for value in [UInt32(14 + dib.count), 0, UInt32(14 + bitmapOffset)] {
            var littleEndian = value.littleEndian
            withUnsafeBytes(of: &littleEndian) { bmp.append(contentsOf: $0) }
        }
        bmp.append(dib)
        return bmp
    }

    private static func largestRepresentation(in source: CGImageSource) -> Int {
        var largest = 0
        var area = 0.0
        for index in 0..<CGImageSourceGetCount(source) {
            guard let props = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
                  let width = props[kCGImagePropertyPixelWidth] as? Double,
                  let height = props[kCGImagePropertyPixelHeight] as? Double,
                  (width * height).isFinite, width > 0, height > 0, width * height > area else { continue }
            area = width * height
            largest = index
        }
        return largest
    }

    fileprivate static func decodeFrame(in source: CGImageSource, at index: Int,
                                        pixelLimit: Double) throws -> (bitmap: CGImage, size: CGSize) {
        guard let props = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let width = props[kCGImagePropertyPixelWidth] as? Double,
              let height = props[kCGImagePropertyPixelHeight] as? Double,
              width.isFinite, height.isFinite, width >= 1, height >= 1 else {
            throw Failure.unreadable
        }
        let orientation = props[kCGImagePropertyOrientation] as? Int ?? 1
        let size = (5...8).contains(orientation) ? CGSize(width: height, height: width) : CGSize(width: width, height: height)
        // Preserve normal screenshots at full resolution. Bound the decoded surface for very large files.
        let longest = max(width, height)
        let reduction = min(1, 8192 / longest, sqrt(pixelLimit / width / height))
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: max(1, Int(longest * reduction)),
            kCGImageSourceShouldCacheImmediately: true
        ]
        guard let bitmap = CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary),
              CGImageSourceGetStatusAtIndex(source, index) == .statusComplete else { throw Failure.unreadable }
        return (bitmap, size)
    }
}

/// Uses Image I/O's composited frames, including GIF disposal and WebP blends.
/// The source is private and its reads after initialization are serialized by a lock.
public struct GlanceAnimation: @unchecked Sendable {
    private struct Keys {
        let dictionary: CFString
        let loop: CFString
        let delay: CFString
        let unclamped: CFString
    }

    fileprivate static let pixelLimit = 4_000_000.0
    private let source: CGImageSource
    private let sourceLock = NSLock()
    private let keys: Keys
    public let frameCount: Int
    /// Image I/O reports total plays. Zero means loop indefinitely.
    public let loopCount: Int
    public let initialDuration: TimeInterval

    fileprivate init?(source: CGImageSource, format: GlanceFormat) {
        let count = CGImageSourceGetCount(source)
        guard count > 1 else { return nil }
        let keys: Keys
        switch format {
        case .gif:
            keys = Keys(dictionary: kCGImagePropertyGIFDictionary, loop: kCGImagePropertyGIFLoopCount,
                        delay: kCGImagePropertyGIFDelayTime, unclamped: kCGImagePropertyGIFUnclampedDelayTime)
        case .webp:
            keys = Keys(dictionary: kCGImagePropertyWebPDictionary, loop: kCGImagePropertyWebPLoopCount,
                        delay: kCGImagePropertyWebPDelayTime, unclamped: kCGImagePropertyWebPUnclampedDelayTime)
        default:
            return nil
        }
        let container = CGImageSourceCopyProperties(source, nil) as? [CFString: Any]
        let timing = container?[keys.dictionary] as? [CFString: Any]
        self.source = source
        self.keys = keys
        frameCount = count
        loopCount = max(0, timing?[keys.loop] as? Int ?? 1)
        initialDuration = Self.duration(in: source, at: 0, keys: keys)
    }

    public func frame(at index: Int) throws -> (bitmap: CGImage, duration: TimeInterval) {
        guard (0..<frameCount).contains(index) else { throw GlanceImage.Failure.unreadable }
        return try sourceLock.withLock {
            let frame = try GlanceImage.decodeFrame(in: source, at: index, pixelLimit: Self.pixelLimit)
            return (frame.bitmap, Self.duration(in: source, at: index, keys: keys))
        }
    }

    private static func duration(in source: CGImageSource, at index: Int, keys: Keys) -> TimeInterval {
        let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any]
        let timing = properties?[keys.dictionary] as? [CFString: Any]
        for key in [keys.unclamped, keys.delay] {
            if let seconds = timing?[key] as? Double, seconds.isFinite, seconds >= 0.02 {
                return seconds
            }
        }
        return 0.1
    }
}

public struct GlanceViewport {
    public private(set) var imageSize: CGSize = .zero
    public private(set) var size: CGSize = .zero
    public private(set) var scale: CGFloat = 1
    public private(set) var origin: CGPoint = .zero
    private var automaticScale = true
    private var backingScale: CGFloat = 1

    // At 100%, one source pixel occupies one pixel of the window's backing store.
    private var drawingScale: CGFloat { scale / backingScale }

    public var isFit: Bool { automaticScale && fit < 1 }

    public var imageRect: CGRect {
        CGRect(origin: origin, size: CGSize(width: imageSize.width * drawingScale, height: imageSize.height * drawingScale))
    }
    public var canPan: Bool { imageRect.width > size.width + 0.5 || imageRect.height > size.height + 0.5 }

    private var fit: CGFloat {
        guard imageSize.width > 0, imageSize.height > 0, size.width > 0, size.height > 0 else { return 1 }
        return min(1, size.width * backingScale / imageSize.width, size.height * backingScale / imageSize.height)
    }

    public init() {}

    public mutating func reset(imageSize: CGSize, in size: CGSize, backingScale: CGFloat = 1) {
        self.imageSize = imageSize
        self.size = size
        self.backingScale = backingScale.isFinite && backingScale > 0 ? backingScale : 1
        automaticScale = true
        clamp()
    }

    public mutating func changeBackingScale(to backingScale: CGFloat) {
        guard backingScale.isFinite, backingScale > 0, backingScale != self.backingScale else { return }
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        let ratio = self.backingScale / backingScale
        origin = CGPoint(x: center.x + (origin.x - center.x) * ratio,
                         y: center.y + (origin.y - center.y) * ratio)
        self.backingScale = backingScale
        clamp()
    }

    public mutating func resize(to size: CGSize) {
        origin.x += (size.width - self.size.width) / 2
        origin.y += (size.height - self.size.height) / 2
        self.size = size
        clamp()
    }

    public func imagePoint(at point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - origin.x) / drawingScale, y: (point.y - origin.y) / drawingScale)
    }

    public mutating func zoom(by factor: CGFloat, at pointer: CGPoint, holding anchor: CGPoint) {
        guard factor.isFinite, factor > 0, factor != 1 else { return }
        // Very large images can start fitted below 5%. A request to zoom out
        // from there should leave that image fitted rather than making it larger.
        guard scale >= 0.05 || factor > 1 else { return }
        scale = min(20, max(0.05, scale * factor))
        automaticScale = false
        origin = CGPoint(x: pointer.x - anchor.x * drawingScale, y: pointer.y - anchor.y * drawingScale)
        clamp()
    }

    public mutating func pan(by delta: CGPoint) {
        guard canPan else { return }
        origin.x += delta.x
        origin.y += delta.y
        clamp()
    }

    private mutating func clamp() {
        if automaticScale { scale = fit }
        let image = imageRect.size
        func offset(_ old: CGFloat, _ extent: CGFloat, _ window: CGFloat) -> CGFloat {
            extent <= window ? (window - extent) / 2 : min(0, max(window - extent, old))
        }
        origin = CGPoint(x: offset(origin.x, image.width, size.width),
                         y: offset(origin.y, image.height, size.height))
    }
}

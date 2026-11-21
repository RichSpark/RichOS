import AppKit
import CoreImage
import CoreMedia
import CoreVideo
import ScreenCaptureKit

@MainActor
enum CaptureFrame {
    enum Failure: Error { case displayUnavailable, windowUnavailable }

    struct WindowTarget {
        let id: CGWindowID
        let frame: CGRect
        let pid: pid_t
        let name: String?
    }

    static func targets() -> [WindowTarget] {
        let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        return rows.compactMap { row in
            guard let number = row[kCGWindowNumber as String] as? NSNumber,
                  let pid = row[kCGWindowOwnerPID as String] as? NSNumber, pid.int32Value != getpid(),
                  let layer = row[kCGWindowLayer as String] as? NSNumber, layer.intValue == 0,
                  let bounds = row[kCGWindowBounds as String] as? NSDictionary,
                  let rect = CGRect(dictionaryRepresentation: bounds as CFDictionary),
                  rect.width >= 24, rect.height >= 24,
                  (row[kCGWindowAlpha as String] as? NSNumber)?.doubleValue != 0 else { return nil }
            let name = (row[kCGWindowName as String] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
            let owner = row[kCGWindowOwnerName as String] as? String
            return WindowTarget(id: number.uint32Value, frame: rect, pid: pid.int32Value,
                                name: name?.isEmpty == false ? name : owner)
        }
    }

    static func read(displayID: CGDirectDisplayID, size: CGSize, pointer: Bool = false,
                     excludingOwnWindows: Bool = false, excluding windows: Set<CGWindowID> = []) async throws -> CGImage {
        let (filter, config) = try await configuration(displayID: displayID, size: size, excluding: windows,
                                                       excludingOwnWindows: excludingOwnWindows, pointer: pointer)
        return try await read(filter: filter, configuration: config)
    }

    fileprivate static func configuration(displayID: CGDirectDisplayID, size: CGSize, excluding windows: Set<CGWindowID>,
                                          excludingOwnWindows: Bool = false, pointer: Bool = false) async throws
        -> (SCContentFilter, SCStreamConfiguration) {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw Failure.displayUnavailable
        }
        let excluded = content.windows.filter {
            if windows.contains($0.windowID) { return true }
            guard excludingOwnWindows, let owner = $0.owningApplication else { return false }
            // A first-time Screen Recording prompt can float without owning foreground focus.
            return owner.processID == getpid() || owner.bundleIdentifier == "com.apple.UserNotificationCenter"
        }
        return (SCContentFilter(display: display, excludingWindows: excluded), configuration(size: size, pointer: pointer))
    }

    static func read(windowID: CGWindowID, scale: CGFloat, pointer: Bool) async throws -> (CGImage, CGFloat) {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: false)
        guard let window = content.windows.first(where: { $0.windowID == windowID && $0.isOnScreen }) else {
            throw Failure.windowUnavailable
        }
        let filter = SCContentFilter(desktopIndependentWindow: window)
        var size = window.frame.size
        var actualScale = scale
        if #available(macOS 14.0, *) {
            size = filter.contentRect.size
            actualScale = CGFloat(filter.pointPixelScale)
        }
        guard size.width > 0, size.height > 0, actualScale > 0 else { throw Failure.windowUnavailable }
        let pixels = CGSize(width: (size.width * actualScale).rounded(), height: (size.height * actualScale).rounded())
        let config = configuration(size: pixels, pointer: pointer)
        if #available(macOS 14.0, *) {
            config.ignoreShadowsSingleWindow = true
            config.ignoreGlobalClipSingleWindow = true
        }
        if #available(macOS 14.2, *) { config.includeChildWindows = false }
        return (try await read(filter: filter, configuration: config), actualScale)
    }

    private static func configuration(size: CGSize, pointer: Bool) -> SCStreamConfiguration {
        let config = SCStreamConfiguration()
        config.width = Int(size.width)
        config.height = Int(size.height)
        config.showsCursor = pointer
        config.capturesAudio = false
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
        return config
    }

    private static func read(filter: SCContentFilter, configuration config: SCStreamConfiguration) async throws -> CGImage {
        if #available(macOS 14.0, *) {
            return try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        }
        return try await firstCompleteFrame(filter: filter, config: config)
    }

    private static func firstCompleteFrame(filter: SCContentFilter, config: SCStreamConfiguration) async throws -> CGImage {
        let frames = LegacyFrameOutput()
        let stream = SCStream(filter: filter, configuration: config, delegate: frames)
        try stream.addStreamOutput(frames, type: .screen, sampleHandlerQueue: frames.queue)
        try await stream.startCapture()
        DispatchQueue.global().asyncAfter(deadline: .now() + 3) { [continuation = frames.continuation] in
            continuation.finish()
        }
        var iterator = frames.images.makeAsyncIterator()
        let image = await iterator.next()
        try? await stream.stopCapture()
        guard let image else { throw Failure.displayUnavailable }
        return image
    }
}

private final class LegacyFrameOutput: NSObject, SCStreamOutput, SCStreamDelegate {
    let queue = DispatchQueue(label: "app.richos.richglance.capture-frame")
    let images: AsyncStream<CGImage>
    let continuation: AsyncStream<CGImage>.Continuation
    private var delivered = false

    override init() {
        (images, continuation) = AsyncStream.makeStream(bufferingPolicy: .bufferingNewest(1))
        super.init()
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard !delivered, type == .screen, let pixels = buffer.imageBuffer,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let status = attachments.first?[.status] as? Int, status == SCFrameStatus.complete.rawValue else { return }
        let frame = CIImage(cvPixelBuffer: pixels)
        guard let image = CIContext().createCGImage(frame, from: frame.extent) else { return }
        delivered = true
        continuation.yield(image)
        continuation.finish()
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        continuation.finish()
    }
}

@MainActor
final class ScrollFrameStream {
    private let stream: SCStream
    private let output: ScrollFrameOutput
    var images: AsyncThrowingStream<CGImage, Error> { output.images }

    private init(stream: SCStream, output: ScrollFrameOutput) {
        self.stream = stream
        self.output = output
    }

    static func open(displayID: CGDirectDisplayID, size: CGSize, crop: CGRect,
                     excluding windows: Set<CGWindowID>) async throws -> ScrollFrameStream {
        let (filter, config) = try await CaptureFrame.configuration(displayID: displayID, size: size, excluding: windows)
        try Task.checkCancellation()
        config.minimumFrameInterval = CMTime(value: 1, timescale: 10)
        let output = ScrollFrameOutput(crop: crop, displaySize: size)
        let stream = SCStream(filter: filter, configuration: config, delegate: output)
        try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: output.queue)
        try await stream.startCapture()
        return ScrollFrameStream(stream: stream, output: output)
    }

    /// A fresh viewport through the same pixel pipeline as the frames used for stitching.
    static func snapshot(displayID: CGDirectDisplayID, size: CGSize, crop: CGRect,
                         excluding windows: Set<CGWindowID>) async throws -> CGImage {
        let captured = try await open(displayID: displayID, size: size, crop: crop, excluding: windows)
        let timeout = Task {
            try? await Task.sleep(for: .seconds(3))
            if !Task.isCancelled { captured.output.continuation.finish() }
        }
        var iterator = captured.images.makeAsyncIterator()
        let image = try? await iterator.next()
        timeout.cancel()
        await captured.stop()
        guard let image else { throw CaptureFrame.Failure.displayUnavailable }
        return image
    }

    func stop() async {
        output.continuation.finish()
        try? await stream.stopCapture()
    }
}

private final class ScrollFrameOutput: NSObject, SCStreamOutput, SCStreamDelegate {
    let queue = DispatchQueue(label: "app.richos.richglance.scroll-frames", qos: .userInitiated)
    let images: AsyncThrowingStream<CGImage, Error>
    let continuation: AsyncThrowingStream<CGImage, Error>.Continuation
    private let crop: CGRect
    private let displaySize: CGSize

    init(crop: CGRect, displaySize: CGSize) {
        (images, continuation) = AsyncThrowingStream.makeStream(bufferingPolicy: .bufferingNewest(1))
        self.crop = crop
        self.displaySize = displaySize
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, let pixels = buffer.imageBuffer,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              attachments.first?[.status] as? Int == SCFrameStatus.complete.rawValue else { return }
        guard CGSize(width: CVPixelBufferGetWidth(pixels), height: CVPixelBufferGetHeight(pixels)) == displaySize,
              CVPixelBufferGetPixelFormatType(pixels) == kCVPixelFormatType_32BGRA else {
            continuation.finish(throwing: CaptureFrame.Failure.displayUnavailable)
            return
        }
        if let image = croppedImage(from: pixels) { continuation.yield(image) }
    }

    private func croppedImage(from pixels: CVPixelBuffer) -> CGImage? {
        let (x, y, width, height) = (Int(crop.minX), Int(crop.minY), Int(crop.width), Int(crop.height))
        guard x >= 0, y >= 0, width > 0, height > 0,
              x + width <= CVPixelBufferGetWidth(pixels), y + height <= CVPixelBufferGetHeight(pixels),
              CVPixelBufferLockBaseAddress(pixels, .readOnly) == kCVReturnSuccess else { return nil }
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        guard let source = CVPixelBufferGetBaseAddress(pixels) else { return nil }
        let inputStride = CVPixelBufferGetBytesPerRow(pixels)
        let outputStride = width * 4
        // ScreenCaptureKit already delivers sRGB BGRA. Copy its pixels verbatim so a new
        // stream cannot round them differently in a separate Core Image context.
        var data = Data(count: outputStride * height)
        data.withUnsafeMutableBytes { destination in
            guard let base = destination.baseAddress else { return }
            for row in 0..<height {
                base.advanced(by: row * outputStride).copyMemory(
                    from: source.advanced(by: (y + row) * inputStride + x * 4), byteCount: outputStride)
            }
        }
        guard let provider = CGDataProvider(data: data as CFData),
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: outputStride, space: colorSpace,
                       bitmapInfo: [.byteOrder32Little, CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue)],
                       provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        continuation.finish(throwing: error)
    }
}

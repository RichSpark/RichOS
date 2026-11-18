import AppKit
import CoreImage
import CoreMedia
import ScreenCaptureKit

@MainActor
enum CaptureFrame {
    enum Failure: Error { case displayUnavailable }

    static func read(displayID: CGDirectDisplayID, size: CGSize,
                     excluding windows: Set<CGWindowID> = []) async throws -> CGImage {
        let (filter, config) = try await configuration(displayID: displayID, size: size, excluding: windows)
        if #available(macOS 14.0, *) {
            return try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        }
        return try await firstCompleteFrame(filter: filter, config: config)
    }

    fileprivate static func configuration(displayID: CGDirectDisplayID, size: CGSize,
                                          excluding windows: Set<CGWindowID>) async throws
        -> (SCContentFilter, SCStreamConfiguration) {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw Failure.displayUnavailable
        }
        let filter = SCContentFilter(display: display, excludingWindows: content.windows.filter { windows.contains($0.windowID) })
        let config = SCStreamConfiguration()
        config.width = Int(size.width)
        config.height = Int(size.height)
        config.showsCursor = false
        config.capturesAudio = false
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
        return (filter, config)
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
    private let context = CIContext(options: [.cacheIntermediates: false])

    init(crop: CGRect, displaySize: CGSize) {
        (images, continuation) = AsyncThrowingStream.makeStream(bufferingPolicy: .bufferingNewest(1))
        self.crop = crop
        self.displaySize = displaySize
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, let pixels = buffer.imageBuffer,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              attachments.first?[.status] as? Int == SCFrameStatus.complete.rawValue else { return }
        let frame = CIImage(cvPixelBuffer: pixels)
        guard frame.extent.size == displaySize else {
            continuation.finish(throwing: CaptureFrame.Failure.displayUnavailable)
            return
        }
        // Core Image y coordinates run upward. The region selector uses top-left pixels.
        let rect = CGRect(x: crop.minX, y: frame.extent.height - crop.maxY, width: crop.width, height: crop.height)
        if let image = context.createCGImage(frame, from: rect) { continuation.yield(image) }
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        continuation.finish(throwing: error)
    }
}

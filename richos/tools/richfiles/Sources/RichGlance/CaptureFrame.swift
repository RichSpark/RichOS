import AppKit
import CoreImage
import CoreMedia
import ScreenCaptureKit

@MainActor
enum CaptureFrame {
    enum Failure: Error { case displayUnavailable }

    static func read(displayID: CGDirectDisplayID, size: CGSize) async throws -> CGImage {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw Failure.displayUnavailable
        }
        let filter = SCContentFilter(display: display, excludingWindows: [])
        let config = SCStreamConfiguration()
        config.width = Int(size.width)
        config.height = Int(size.height)
        config.showsCursor = false
        config.capturesAudio = false
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
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

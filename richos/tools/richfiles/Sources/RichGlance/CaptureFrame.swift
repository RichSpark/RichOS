import AppKit
import CoreImage
import CoreMedia
import ScreenCaptureKit

@MainActor
enum CaptureFrame {
    enum Failure: Error { case displayUnavailable, windowUnavailable }

    struct WindowTarget {
        let id: CGWindowID
        let frame: CGRect
        let title: String
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
            let owner = row[kCGWindowOwnerName as String] as? String ?? "Window"
            let name = row[kCGWindowName as String] as? String
            return WindowTarget(id: number.uint32Value, frame: rect, title: name.flatMap { $0.isEmpty ? nil : $0 } ?? owner)
        }
    }

    static func read(displayID: CGDirectDisplayID, size: CGSize, pointer: Bool = false,
                     excludingOwnWindows: Bool = false) async throws -> CGImage {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw Failure.displayUnavailable
        }
        let excluded = excludingOwnWindows ? content.windows.filter { $0.owningApplication?.processID == getpid() } : []
        return try await read(filter: SCContentFilter(display: display, excludingWindows: excluded), size: size, pointer: pointer)
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
        return (try await read(filter: filter, size: pixels, pointer: pointer, window: true), actualScale)
    }

    private static func read(filter: SCContentFilter, size: CGSize, pointer: Bool, window: Bool = false) async throws -> CGImage {
        let config = SCStreamConfiguration()
        config.width = Int(size.width)
        config.height = Int(size.height)
        config.showsCursor = pointer
        if #available(macOS 14.0, *), window {
            config.ignoreShadowsSingleWindow = true
            config.ignoreGlobalClipSingleWindow = true
        }
        if #available(macOS 14.2, *), window { config.includeChildWindows = false }
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

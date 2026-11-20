// Guest companion to timeline.py for short responses. Use a visible patch of the
// target window that changes only when the action succeeds.
import AppKit
import CoreMedia
import ScreenCaptureKit

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(("timeline-window: " + message + "\n").utf8))
    exit(2)
}

let arguments = CommandLine.arguments
guard NSUserName() == "admin", ProcessInfo.processInfo.environment["RICHOS_TESTVM_CAPTURE"] == "1" else {
    fail("capture is only for the test guest")
}
guard arguments.count == 6, let pid = Int32(arguments[1]) else {
    fail("usage: timeline-window PID X,Y,W,H click|double|right|key X,Y|KEYCODE MODIFIER_FLAGS")
}
let patch = arguments[2].split(separator: ",").compactMap { Double($0) }
guard patch.count == 4, patch.allSatisfy(\.isFinite), patch[2] >= 2, patch[3] >= 2,
      let flags = UInt64(arguments[5]) else { fail("invalid patch or modifiers") }

NSApplication.shared.setActivationPolicy(.prohibited)
guard CGPreflightPostEventAccess() else { fail("the guest has no event-posting permission") }

let events: [CGEvent]
if arguments[3] == "key" {
    guard let code = UInt16(arguments[4]), let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
          let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else { fail("invalid key") }
    events = [down, up]
} else {
    let point = arguments[4].split(separator: ",").compactMap { Double($0) }
    guard point.count == 2, point.allSatisfy(\.isFinite), ["click", "double", "right"].contains(arguments[3]) else {
        fail("invalid click")
    }
    let right = arguments[3] == "right"
    let count = arguments[3] == "double" ? 2 : 1
    let location = CGPoint(x: point[0], y: point[1])
    events = (1...count).flatMap { clicks in
        let types: [CGEventType] = right ? [.rightMouseDown, .rightMouseUp] : [.leftMouseDown, .leftMouseUp]
        return types.map { type in
            guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: location,
                                      mouseButton: right ? .right : .left) else { fail("could not make click") }
            event.setIntegerValueField(.mouseEventClickState, value: Int64(clicks))
            return event
        }
    }
}

final class Patch: NSObject, SCStreamOutput {
    let rectangle: CGRect
    let lock = NSLock()
    var baseline: [UInt8] = []
    var first: UInt64?
    var displayed: UInt64?
    var recorded: [UInt8] = []
    var action: UInt64?
    var frames = 0
    var stable = 0

    init(_ rectangle: CGRect) { self.rectangle = rectangle }

    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let details = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let raw = details.first?[.status] as? Int, let status = SCFrameStatus(rawValue: raw),
              status == .complete || status == .started,
              let image = CMSampleBufferGetImageBuffer(sample) else { return }
        CVPixelBufferLockBaseAddress(image, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(image, .readOnly) }
        guard let buffer = CVPixelBufferGetBaseAddress(image),
              Int(rectangle.maxX) <= CVPixelBufferGetWidth(image),
              Int(rectangle.maxY) <= CVPixelBufferGetHeight(image) else { return }
        let stride = CVPixelBufferGetBytesPerRow(image)
        var pixels: [UInt8] = []
        for y in Int(rectangle.minY)..<Int(rectangle.maxY) {
            let row = buffer.advanced(by: y * stride).assumingMemoryBound(to: UInt8.self)
            for x in Int(rectangle.minX)..<Int(rectangle.maxX) {
                for channel in 0..<3 { pixels.append(row[x * 4 + channel]) }
            }
        }
        lock.lock()
        defer { lock.unlock() }
        let displayTime = details.first?[.displayTime] as? UInt64
        guard let action else { baseline = pixels; return }
        if let displayTime, displayTime < action { return }
        frames += 1
        let changed = zip(baseline, pixels).reduce(0) { total, pair in
            total + (abs(Int(pair.0) - Int(pair.1)) >= 10 ? 1 : 0)
        }
        if changed > pixels.count / 8 {
            if first == nil {
                first = mach_absolute_time()
                displayed = displayTime
                recorded = pixels
            }
            stable += 1
        }
    }

    func post(events: [CGEvent], pid: pid_t, flags: UInt64) -> (UInt64, UInt64) {
        lock.lock()
        defer { lock.unlock() }
        guard !baseline.isEmpty else { fail("no baseline frame with pixels in the target patch") }
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else { fail("target app is not in front") }
        let before = mach_absolute_time()
        action = before
        for (index, event) in events.enumerated() {
            if index == 2 { Thread.sleep(forTimeInterval: 0.045) }
            event.flags = CGEventFlags(rawValue: flags)
            event.post(tap: .cghidEventTap)
        }
        return (before, mach_absolute_time())
    }

    func result(posted: (UInt64, UInt64)) -> [String: Any] {
        lock.lock()
        defer { lock.unlock() }
        var timebase = mach_timebase_info_data_t()
        mach_timebase_info(&timebase)
        let scale = Double(timebase.numer) / Double(timebase.denom) / 1_000_000
        let delta = first.map { Double($0 - posted.0) * scale }
        let display = displayed.flatMap { stamp in
            stamp >= posted.0 && stamp <= (first ?? 0) ? Double(stamp - posted.0) * scale : nil
        }
        return ["visible_upper_ms": delta.map { $0 as Any } ?? NSNull(),
                "display_ms": display.map { $0 as Any } ?? NSNull(), "post_ms": Double(posted.1 - posted.0) * scale,
                "changed_frames": stable, "received_frames": frames,
                "baseline_bgr": Array(baseline.prefix(3)), "result_bgr": Array(recorded.prefix(3))]
    }
}

Task {
    do {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        let windows = content.windows.filter { $0.owningApplication?.processID == pid && $0.windowLayer == 0 && $0.isOnScreen }
        guard let window = windows.max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) else {
            fail("target app has no visible window")
        }
        let config = SCStreamConfiguration()
        config.width = Int(window.frame.width)
        config.height = Int(window.frame.height)
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.minimumFrameInterval = CMTime(value: 1, timescale: 120)
        config.queueDepth = 3
        config.showsCursor = false
        if #available(macOS 14, *) { config.ignoreShadowsSingleWindow = true }
        let rectangle = CGRect(x: patch[0] - window.frame.minX, y: patch[1] - window.frame.minY,
                               width: patch[2], height: patch[3])
        guard CGRect(origin: .zero, size: window.frame.size).contains(rectangle) else { fail("patch is outside the window") }
        let output = Patch(rectangle)
        let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: nil)
        try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: DispatchQueue(label: "timeline-window"))
        try await stream.startCapture()
        try await Task.sleep(nanoseconds: 350_000_000)
        let posted = output.post(events: events, pid: pid, flags: flags)
        try await Task.sleep(nanoseconds: 950_000_000)
        try await stream.stopCapture()
        let data = try JSONSerialization.data(withJSONObject: output.result(posted: posted), options: [.sortedKeys])
        print(String(decoding: data, as: UTF8.self))
        exit(0)
    } catch {
        fail(String(describing: error))
    }
}
dispatchMain()

// frame-rate.swift — RUNS INSIDE THE GUEST (home-fps-walk.py compiles it on the host and copies it in).
//
// How many frames a second one app's window actually PRESENTS, measured from outside the app.
// ScreenCaptureKit streams the window and marks each delivered frame `.complete` when the window's
// content changed since the last one and `.idle` when it did not, so counting the complete frames
// over a span is the window's own frame rate, capped by the display's refresh (60 in the guest,
// provision-guest.sh pins it). Nothing is injected into the app and nothing about it is changed:
// the app measured is the app as it ships.
//
// Usage: frame-rate <pid> <seconds per sample> <samples> [<pause seconds between samples>]
// Prints one JSON object: the window measured and, per sample, the complete frames, the span they
// fell in and the rate. The capture is scaled to a quarter of the window, which is all a count needs
// and keeps the measurement's own cost small. Exit 2 when the window cannot be found or captured:
// a measurement that cannot be made says so instead of printing a rate.
import AppKit
import CoreMedia
import Foundation
import ScreenCaptureKit

setvbuf(stdout, nil, _IOLBF, 0)
// A process started over ssh has no window-server connection until something asks for one, and
// ScreenCaptureKit then aborts in CGInitialization.c ("CGS_REQUIRE_INIT"). The shared application,
// never shown and never in the Dock, opens that connection.
NSApplication.shared.setActivationPolicy(.prohibited)

func fail(_ why: String) -> Never {
    FileHandle.standardError.write(("frame-rate: " + why + "\n").data(using: .utf8)!)
    exit(2)
}

let argv = CommandLine.arguments
guard argv.count >= 4, let pid = Int32(argv[1]), let span = Double(argv[2]), let samples = Int(argv[3]),
      span > 0, span <= 60, samples > 0, samples <= 20 else {
    fail("usage: frame-rate <pid> <seconds per sample, at most 60> <samples, at most 20> [<pause seconds>]")
}
let pause = argv.count > 4 ? (Double(argv[4]) ?? 0) : 0

final class Counter: NSObject, SCStreamOutput {
    let lock = NSLock()
    var complete = 0
    var idle = 0
    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let raw = attachments.first?[.status] as? Int, let status = SCFrameStatus(rawValue: raw) else { return }
        lock.lock()
        if status == .complete { complete += 1 } else if status == .idle { idle += 1 }
        lock.unlock()
    }
    func take() -> (Int, Int) {
        lock.lock(); defer { lock.unlock() }
        let out = (complete, idle)
        complete = 0; idle = 0
        return out
    }
}

Task {
    let content: SCShareableContent
    do { content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true) } catch {
        fail("no screen content (is the screen-recording grant in place?): \(error)")
    }
    // the app's largest on-screen window at the normal level: its main window, never a helper's
    let mine = content.windows.filter { $0.owningApplication?.processID == pid && $0.windowLayer == 0 && $0.isOnScreen }
    guard let window = mine.max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }),
          window.frame.width >= 200, window.frame.height >= 200 else {
        fail("process \(pid) has no on-screen window of at least 200 x 200")
    }
    let config = SCStreamConfiguration()
    config.width = max(64, Int(window.frame.width / 4))
    config.height = max(64, Int(window.frame.height / 4))
    config.minimumFrameInterval = CMTime(value: 1, timescale: 120) // above the display's 60: never the cap
    config.queueDepth = 6
    config.showsCursor = false
    let counter = Counter()
    let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: nil)
    do {
        try stream.addStreamOutput(counter, type: .screen, sampleHandlerQueue: DispatchQueue(label: "frames"))
        try await stream.startCapture()
    } catch {
        fail("could not capture the window: \(error)")
    }
    try? await Task.sleep(nanoseconds: 500_000_000) // the stream's first frames are its start, not the app
    var rows: [[String: Any]] = []
    for n in 0..<samples {
        if n > 0 && pause > 0 { try? await Task.sleep(nanoseconds: UInt64(pause * 1e9)) }
        _ = counter.take()
        let began = Date()
        try? await Task.sleep(nanoseconds: UInt64(span * 1e9))
        let (complete, idle) = counter.take()
        let seconds = Date().timeIntervalSince(began)
        rows.append(["complete": complete, "idle": idle, "seconds": (seconds * 1000).rounded() / 1000,
                     "fps": (Double(complete) / seconds * 10).rounded() / 10])
    }
    try? await stream.stopCapture()
    let report: [String: Any] = [
        "pid": Int(pid), "window": ["title": window.title ?? "", "w": Int(window.frame.width), "h": Int(window.frame.height)],
        "capture": ["w": config.width, "h": config.height], "samples": rows,
    ]
    let data = try! JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
    exit(0)
}
dispatchMain()

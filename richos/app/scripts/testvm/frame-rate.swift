// frame-rate.swift — RUNS INSIDE THE GUEST (home-fps-walk.py compiles it on the host and copies it in).
//
// How many frames a second the loro PICTURE draws in one app's window, measured from outside the
// app. ScreenCaptureKit streams the window and marks a delivered frame `.complete` when anything in
// the window changed. That alone is not the picture's rate: the home screen also runs animations of
// its own that the compositor drives without the picture (the bokeh's drift, `home.css` `home-drift`;
// the Talk to Rich dot's breathing, `home-sill-breathe`), and they produce complete frames while the
// picture's own frame function is slow or stopped. So each complete frame is also compared with the
// one before it inside a region of the window that only the picture paints (the middle of the
// picture, clear of the left column, the dot and the right column): a frame whose region has at least
// MIN_CHANGED_SHARE of its pixels changed by DELTA or more is a picture frame. The picture changes
// every frame it draws (the nodes twinkle, the packets travel, the idle tilt drifts), while the
// bokeh's drift (10 points in 38 s, a thousandth of a capture pixel a frame) and the dot change none
// of those pixels. Both rates are printed: the window's and the picture's, with, per sample, how many
// complete frames changed the region at all (`any_change`) and a histogram of how many region pixels
// each complete frame moved by DELTA or more (0, 1-7, 8-63, 64-511, 512-4095, 4096+), so the line
// between a picture frame and an unrelated one can be read off the data. The display's refresh
// (60 in the guest, provision-guest.sh pins it) caps both. Nothing is injected into the app.
//
// Usage: frame-rate <pid> <seconds per sample> <samples> [<pause seconds between samples>
//                   [<region as x,y,w,h fractions of the window>]]
// Prints one JSON object: the window measured, the region and, per sample, the window's complete
// frames, the picture's frames, the span they fell in and both rates. The capture is scaled to a
// quarter of the window, which keeps the measurement's own cost small. Exit 2 when the window cannot
// be found or captured: a measurement that cannot be made says so instead of printing a rate.
import AppKit
import CoreMedia
import CoreVideo
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

let DELTA = 6                       // a channel that moved by this much (of 255) is a changed pixel
let MIN_CHANGED_SHARE = 0.002       // a picture frame changes at least this share of the region

let argv = CommandLine.arguments
guard argv.count >= 4, let pid = Int32(argv[1]), let span = Double(argv[2]), let samples = Int(argv[3]),
      span > 0, span <= 60, samples > 0, samples <= 20 else {
    fail("usage: frame-rate <pid> <seconds per sample, at most 60> <samples, at most 20> [<pause seconds> [<x,y,w,h>]]")
}
let pause = argv.count > 4 ? (Double(argv[4]) ?? 0) : 0
let region: [Double] = argv.count > 5 ? argv[5].split(separator: ",").compactMap { Double($0) } : [0.32, 0.16, 0.44, 0.52]
guard region.count == 4, region.allSatisfy({ $0 >= 0 && $0 <= 1 }), region[0] + region[2] <= 1, region[1] + region[3] <= 1,
      region[2] > 0.05, region[3] > 0.05 else {
    fail("the region is x,y,w,h as fractions of the window, each from 0 to 1, inside it")
}

final class Counter: NSObject, SCStreamOutput {
    let lock = NSLock()
    var complete = 0
    var picture = 0
    var faint = 0
    var idle = 0
    var moved: [Int] = [0, 0, 0, 0, 0, 0]   // complete frames by pixels moved: 0, 1-7, 8-63, 64-511, 512-4095, 4096+
    var previous: [UInt8] = []
    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let raw = attachments.first?[.status] as? Int, let status = SCFrameStatus(rawValue: raw) else { return }
        if status == .idle { lock.lock(); idle += 1; lock.unlock(); return }
        guard status == .complete, let image = CMSampleBufferGetImageBuffer(buffer) else { return }
        let (strong, weak) = regionChanged(image)
        lock.lock()
        complete += 1
        if Double(strong) >= MIN_CHANGED_SHARE * Double(regionPixels) { picture += 1 }
        if weak > 0 { faint += 1 }
        let bucket = strong == 0 ? 0 : strong < 8 ? 1 : strong < 64 ? 2 : strong < 512 ? 3 : strong < 4096 ? 4 : 5
        moved[bucket] += 1
        lock.unlock()
    }
    var regionPixels = 1
    /// How many of the region's pixels moved since the last complete frame: by DELTA or more, and by
    /// at least one level (32-bit BGRA pixels).
    func regionChanged(_ image: CVPixelBuffer) -> (Int, Int) {
        CVPixelBufferLockBaseAddress(image, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(image, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(image) else { return (0, 0) }
        let w = CVPixelBufferGetWidth(image), h = CVPixelBufferGetHeight(image), stride = CVPixelBufferGetBytesPerRow(image)
        let x0 = Int(Double(w) * region[0]), y0 = Int(Double(h) * region[1])
        let x1 = min(w, x0 + Int(Double(w) * region[2])), y1 = min(h, y0 + Int(Double(h) * region[3]))
        let bytes = base.assumingMemoryBound(to: UInt8.self)
        var now = [UInt8](repeating: 0, count: (x1 - x0) * (y1 - y0) * 3)
        var i = 0
        for y in y0..<y1 {
            let row = bytes + y * stride
            for x in x0..<x1 {
                let p = row + x * 4
                now[i] = p[0]; now[i + 1] = p[1]; now[i + 2] = p[2]
                i += 3
            }
        }
        defer { previous = now }
        regionPixels = max(1, now.count / 3)
        guard previous.count == now.count else { return (0, 0) }
        var strong = 0, weak = 0
        var j = 0
        while j < now.count {
            let d = max(abs(Int(now[j]) - Int(previous[j])), abs(Int(now[j + 1]) - Int(previous[j + 1])), abs(Int(now[j + 2]) - Int(previous[j + 2])))
            if d >= DELTA { strong += 1 }
            if d >= 1 { weak += 1 }
            j += 3
        }
        return (strong, weak)
    }
    func take() -> (Int, Int, Int, Int, [Int]) {
        lock.lock(); defer { lock.unlock() }
        let out = (complete, picture, faint, idle, moved)
        complete = 0; picture = 0; faint = 0; idle = 0; moved = [0, 0, 0, 0, 0, 0]
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
    config.pixelFormat = kCVPixelFormatType_32BGRA
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
        let (complete, picture, faint, idle, moved) = counter.take()
        let seconds = Date().timeIntervalSince(began)
        rows.append(["complete": complete, "picture": picture, "any_change": faint, "moved_histogram": moved, "idle": idle,
                     "seconds": (seconds * 1000).rounded() / 1000,
                     "window_fps": (Double(complete) / seconds * 10).rounded() / 10,
                     "fps": (Double(picture) / seconds * 10).rounded() / 10])
    }
    try? await stream.stopCapture()
    let report: [String: Any] = [
        "pid": Int(pid), "window": ["title": window.title ?? "", "w": Int(window.frame.width), "h": Int(window.frame.height)],
        "capture": ["w": config.width, "h": config.height], "region": region,
        "picture_frame": ["delta": DELTA, "min_changed_share": MIN_CHANGED_SHARE], "samples": rows,
    ]
    let data = try! JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
    exit(0)
}
dispatchMain()

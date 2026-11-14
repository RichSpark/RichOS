// screen-frames OUTDIR SECONDS — runs INSIDE the test guest only.
//
// Records the main display with ScreenCaptureKit for SECONDS. Each NEW frame (status complete;
// macOS sends none while nothing on the screen changes) is written as 8-bit gray at half the
// display's point size to OUTDIR/frames.gray, and its line in OUTDIR/frames.txt carries the
// instant macOS displayed it (SCStreamFrameInfo.displayTime, mach time) converted to the wall
// clock in ms, the same clock the app's "[trial]" lines and the launch request use. So "when was
// the picture on the glass" is read off the frames, not inferred.
//
// Build on the host: swiftc -O -target arm64-apple-macos14.0 screen-frames.swift -o screen-frames
import CoreMedia
import CoreVideo
import Foundation
import ScreenCaptureKit

setvbuf(stdout, nil, _IOLBF, 0)
let args = CommandLine.arguments
guard args.count >= 3, let seconds = Double(args[2]) else {
    print("usage: screen-frames OUTDIR SECONDS")
    exit(2)
}
let outDir = URL(fileURLWithPath: args[1])
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
let grayPath = outDir.appendingPathComponent("frames.gray").path
FileManager.default.createFile(atPath: grayPath, contents: nil)
let grayFile = FileHandle(forWritingAtPath: grayPath)!

var timebase = mach_timebase_info_data_t()
mach_timebase_info(&timebase)
func epochMs(ofMach m: UInt64) -> Double {
    var tv = timeval()
    let nowMach = mach_absolute_time()
    gettimeofday(&tv, nil)
    let nowEpoch = Double(tv.tv_sec) * 1000 + Double(tv.tv_usec) / 1000
    let agoNs = (Double(nowMach) - Double(m)) * Double(timebase.numer) / Double(timebase.denom)
    return nowEpoch - agoNs / 1e6
}

final class Sink: NSObject, SCStreamOutput {
    var count = 0
    var width = 0
    var height = 0
    var lines = ["index display_mach display_epoch_ms"]
    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, buffer.isValid,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let info = attachments.first,
              let raw = info[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
              let pixels = CMSampleBufferGetImageBuffer(buffer) else { return }
        let shown = (info[.displayTime] as? UInt64) ?? mach_absolute_time()
        CVPixelBufferLockBaseAddress(pixels, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        let w = CVPixelBufferGetWidth(pixels), h = CVPixelBufferGetHeight(pixels)
        let stride = CVPixelBufferGetBytesPerRow(pixels)
        let base = CVPixelBufferGetBaseAddress(pixels)!.assumingMemoryBound(to: UInt8.self)
        var gray = [UInt8](repeating: 0, count: w * h)
        for y in 0..<h {
            let row = base + y * stride
            for x in 0..<w {
                let p = row + x * 4  // BGRA
                gray[y * w + x] = UInt8((Int(p[2]) * 77 + Int(p[1]) * 150 + Int(p[0]) * 29) >> 8)
            }
        }
        grayFile.write(Data(gray))
        width = w
        height = h
        lines.append("\(count) \(shown) \(String(format: "%.3f", epochMs(ofMach: shown)))")
        count += 1
    }
}

let sink = Sink()
var stream: SCStream?
let started = DispatchSemaphore(value: 0)
SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { content, error in
    guard let content, let display = content.displays.first else {
        print("ERROR no display: \(String(describing: error))")
        exit(3)
    }
    let filter = SCContentFilter(display: display, excludingWindows: [])
    let config = SCStreamConfiguration()
    config.width = display.width / 2
    config.height = display.height / 2
    config.minimumFrameInterval = CMTime(value: 1, timescale: 60)
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.showsCursor = false
    config.queueDepth = 8
    let s = SCStream(filter: filter, configuration: config, delegate: nil)
    do {
        try s.addStreamOutput(sink, type: .screen, sampleHandlerQueue: DispatchQueue(label: "frames"))
    } catch {
        print("ERROR output: \(error)")
        exit(4)
    }
    s.startCapture { error in
        if let error {
            print("ERROR start: \(error)")
            exit(5)
        }
        print("STARTED \(display.width)x\(display.height) points")
        started.signal()
    }
    stream = s
}
started.wait()
Thread.sleep(forTimeInterval: seconds)
let stopped = DispatchSemaphore(value: 0)
stream?.stopCapture { _ in stopped.signal() }
_ = stopped.wait(timeout: .now() + 5)
try? grayFile.close()
let meta = "\(sink.width) \(sink.height)\n" + sink.lines.joined(separator: "\n") + "\n"
try! meta.write(to: outDir.appendingPathComponent("frames.txt"), atomically: true, encoding: .utf8)
print("DONE \(sink.count) frames \(sink.width)x\(sink.height)")

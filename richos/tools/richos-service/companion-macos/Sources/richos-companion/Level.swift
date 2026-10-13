import Foundation
import RichOSCompanionCore

/// `richos-companion level [--exclude-pid N]...`: the call's sound level, for the meeting prompt's
/// voice bar, as JSON lines on stdout, about 20 a second:
///
///   {"type":"level","at":ms,"rms":0.0123,"peak":0.08}
///
/// The CEO's answer of 2026-10-13 to esc-20261013T090538Z-8defdf42: "move with the call's sound as
/// designed" — after Not this one, the bar's level follows the call, read live through the
/// system-audio tap once he has granted that access, never recorded or stored. So this reads the
/// SYSTEM TAP ONLY (no microphone), keeps nothing (each block is measured and dropped), writes no
/// file, and exits when stdin closes (the app holds the pipe) or on SIGINT/SIGTERM.
///
/// It never asks for the permission: the app starts it only after a capture has shown the grant
/// exists (`capture --json` reports `system-audio`), because starting a tap on a Mac that has not
/// decided would put the permission dialog up from a prompt the CEO declined.
@available(macOS 14.4, *)
func runLevelMeter(excludePids: [Int32]) {
    let tap = CoreAudioCapture(excludePids: excludePids)
    let lock = NSLock()
    var sumSq = 0.0, n = 0, peak = 0.0
    do {
        try tap.prepare()
        try tap.runLevels { block in
            var s = 0.0, p = 0.0
            for x in block { let a = Double(abs(x)); if a > p { p = a }; s += Double(x) * Double(x) }
            lock.lock(); sumSq += s; n += block.count; if p > peak { peak = p }; lock.unlock()
        }
    } catch {
        let line = JSON.object([("type", .string("error")), ("at", .int(Int(nowMs()))),
                                ("message", .string(error.localizedDescription))])
        FileHandle.standardOutput.write(Data((line.compact() + "\n").utf8))
        exit(1)
    }
    let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "com.richos.companion.level"))
    timer.schedule(deadline: .now() + 0.05, repeating: 0.05)
    var exclusionTicks = 0
    timer.setEventHandler {
        lock.lock()
        let rms = n > 0 ? (sumSq / Double(n)).squareRoot() : 0
        let p = peak
        sumSq = 0; n = 0; peak = 0
        lock.unlock()
        let line = JSON.object([("type", .string("level")), ("at", .int(Int(nowMs()))),
                                ("rms", .double((rms * 10_000).rounded() / 10_000)),
                                ("peak", .double((p * 10_000).rounded() / 10_000))])
        FileHandle.standardOutput.write(Data((line.compact() + "\n").utf8))
        exclusionTicks += 1
        if exclusionTicks % 20 == 0 { _ = tap.refreshExclusions() }
    }
    timer.resume()
    DispatchQueue.global(qos: .utility).async {
        while true {
            if FileHandle.standardInput.availableData.isEmpty { tap.stop(); exit(0) }
        }
    }
    var sources: [DispatchSourceSignal] = []
    for sig in [SIGINT, SIGTERM] {
        signal(sig, SIG_IGN)
        let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
        src.setEventHandler { tap.stop(); exit(0) }
        src.resume()
        sources.append(src)
    }
    withExtendedLifetime((timer, sources)) { RunLoop.main.run() }
}

func cmdLevel(_ args: [String]) {
    var excludePids: [Int32] = [getpid()]
    for (i, a) in args.enumerated() where a == "--exclude-pid" {
        guard i + 1 < args.count, let pid = Int32(args[i + 1]) else {
            FileErr.write("--exclude-pid needs a number\n"); exit(2)
        }
        excludePids.append(pid)
    }
    if #available(macOS 14.4, *) {
        runLevelMeter(excludePids: excludePids)
    } else {
        FileErr.write("level requires macOS 14.4+ (Core Audio process taps).\n")
        exit(1)
    }
}

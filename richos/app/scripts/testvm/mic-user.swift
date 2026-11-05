// mic-user.swift — RUNS INSIDE THE GUEST (meeting-walk.py compiles it with swiftc there).
//
// A stand-in for "another app starts using the microphone": it opens the default input with
// AVAudioEngine and keeps it running, doing nothing with the samples, until it is sent SIGINT or
// SIGTERM. Core Audio then reports this process as running input, which is all the capture
// companion's `watch` looks at (kAudioProcessPropertyIsRunningInput). It plays and records
// nothing audible and writes no file. Usage: mic-user [SECONDS]  (default: until a signal).
import AVFoundation
import Foundation

setvbuf(stdout, nil, _IOLBF, 0)
let engine = AVAudioEngine()
let input = engine.inputNode
let format = input.outputFormat(forBus: 0)
var frames: Int64 = 0
input.installTap(onBus: 0, bufferSize: 4096, format: format) { buffer, _ in frames += Int64(buffer.frameLength) }
do { try engine.start() } catch { print("mic-user: could not start: \(error)"); exit(2) }
print("mic-user: input running, \(format.sampleRate) Hz, \(format.channelCount) ch")
for sig in [SIGINT, SIGTERM] {
    signal(sig, SIG_IGN)
    let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
    source.setEventHandler { engine.stop(); print("mic-user: stopped after \(frames) frames"); exit(0) }
    source.resume()
}
if CommandLine.arguments.count > 1, let s = Double(CommandLine.arguments[1]) {
    DispatchQueue.main.asyncAfter(deadline: .now() + s) { engine.stop(); print("mic-user: done, \(frames) frames"); exit(0) }
}
dispatchMain()

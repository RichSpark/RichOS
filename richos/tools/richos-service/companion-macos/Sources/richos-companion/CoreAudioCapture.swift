import Foundation
import RichOSCompanionCore
#if canImport(CoreAudio)
import CoreAudio
import AVFoundation
import AudioToolbox

/// The irreducibly-native macOS capture engine (architecture §5.2): a **Core Audio process tap** on all
/// system output (Granola parity, §10-Q2 "all-output first"), plus the **default mic**, mixed into
/// the frozen 2-channel contract (LEFT=mic, RIGHT=system).
///
/// - System audio: `CATapDescription(stereoGlobalTapButExcludeProcesses:)` -> `AudioHardwareCreateProcessTap`
///   added to a private **aggregate device**, read via `AudioDeviceCreateIOProcIDWithBlock`. Audio
///   keeps playing to the user (the tap reads a COPY of the render stream, pre-output-device — so it
///   works on headphones exactly as on speakers). Uses the narrow **"System Audio Recording"** TCC
///   permission, not Screen Recording. RichOS's own speech is excluded (`excludePids`, Granola PRD
///   V4: "Exclude RichOS's own spoken output from the system tap so it is not treated as a remote
///   participant").
/// - Microphone: `AVAudioEngine.inputNode`, resampled to the tap's sample rate so the two async
///   sources stay frame-aligned in `SessionWriter`.
///
/// TWO STEPS, ON PURPOSE: `prepare()` makes the tap and reads its REAL format; only then does the
/// caller make the session (whose WAV header carries the sample rate) and `run(writer:)` start the
/// audio. Until 2026-10-13 the session was always written as 48 kHz while the microphone was
/// resampled to the tap's rate, so on an output device at another rate (a Bluetooth headset in call
/// mode runs at 16 or 24 kHz) the recording would have played at the wrong speed.
///
/// PERMISSION GATE (honest): the first `AudioDeviceStart` on an aggregate containing a tap triggers
/// the macOS "System Audio Recording" prompt — a HUMAN GUI grant with no non-interactive path and no
/// API to query current status (architecture §5.2). Until granted, the tap delivers no frames; the watchdog
/// treats sudden all-silence on the system side as a possible revocation and alarms (§6.4).
@available(macOS 14.4, *)
final class CoreAudioCapture {
    private var writer: SessionWriter?
    /// Processes whose output the system tap must NOT record. A process has a Core Audio object
    /// only once it has used audio, so `refreshExclusions()` re-checks once a second.
    private let excludePids: [Int32]
    private var excludedObjects: [AudioObjectID] = []
    private var tapDescription: CATapDescription?
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var ioProcID: AudioDeviceIOProcID?
    private let ioQueue = DispatchQueue(label: "com.richos.companion.tap-io")
    private let engine = AVAudioEngine()
    private var converter: AVAudioConverter?
    private var tapSampleRate: Double = 48_000
    private(set) var tapRunning = false
    private(set) var micRunning = false

    // Cheap level metering accumulated in the audio callbacks; snapshot+reset once per health tick.
    private let levelLock = NSLock()
    private var micPeak = 0.0, micSumSq = 0.0, micN = 0
    private var sysPeak = 0.0, sysSumSq = 0.0, sysN = 0

    init(excludePids: [Int32] = []) { self.excludePids = excludePids }

    /// (micPeak, micRms, sysPeak, sysRms) since the last snapshot; resets accumulators.
    func snapshotLevels() -> (Double, Double, Double, Double) {
        levelLock.lock(); defer { levelLock.unlock() }
        let mRms = micN > 0 ? (micSumSq / Double(micN)).squareRoot() : 0
        let sRms = sysN > 0 ? (sysSumSq / Double(sysN)).squareRoot() : 0
        let out = (micPeak, mRms, sysPeak, sRms)
        micPeak = 0; micSumSq = 0; micN = 0; sysPeak = 0; sysSumSq = 0; sysN = 0
        return out
    }

    private func meter(_ block: [Float], mic: Bool) {
        var peak = 0.0, sumSq = 0.0
        for x in block { let a = Double(abs(x)); if a > peak { peak = a }; sumSq += Double(x) * Double(x) }
        levelLock.lock()
        if mic { if peak > micPeak { micPeak = peak }; micSumSq += sumSq; micN += block.count }
        else { if peak > sysPeak { sysPeak = peak }; sysSumSq += sumSq; sysN += block.count }
        levelLock.unlock()
    }

    /// Make the tap and its aggregate device and read the tap's real format. Nothing is recorded yet.
    func prepare() throws {
        try createSystemTap()
    }

    /// Start audio into `writer`: the tap's IO and the microphone.
    func run(writer: SessionWriter) throws {
        self.writer = writer
        try startTapIO()
        try startMic()
    }

    /// The tap's sample rate, known after `prepare()`.
    var sampleRate: Int { Int(tapSampleRate) }

    // MARK: - Exclusions (RichOS's own speech)

    private func excludedProcessObjects() -> [AudioObjectID] {
        excludePids.compactMap { pid -> AudioObjectID? in
            var addr = AudioObjectPropertyAddress(
                mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
                mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
            var qualifier = pid
            var object = AudioObjectID(kAudioObjectUnknown)
            var size = UInt32(MemoryLayout<AudioObjectID>.size)
            let status = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr,
                                                    UInt32(MemoryLayout<Int32>.size), &qualifier, &size, &object)
            return status == noErr && object != kAudioObjectUnknown ? object : nil
        }.sorted()
    }

    /// Re-check the excluded processes: one that started using audio after the tap was made is added
    /// to the tap's exclusion list. Returns a sentence when something changed (or could not).
    func refreshExclusions() -> String? {
        guard let desc = tapDescription, tapID != kAudioObjectUnknown else { return nil }
        let now = excludedProcessObjects()
        guard now != excludedObjects else { return nil }
        desc.processes = now
        var addr = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyDescription,
                                              mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var ref: CATapDescription = desc
        let status = withUnsafeMutablePointer(to: &ref) {
            AudioObjectSetPropertyData(tapID, &addr, 0, nil, UInt32(MemoryLayout<CATapDescription>.size), $0)
        }
        if status == noErr {
            excludedObjects = now
            return "system tap now excludes \(now.count) RichOS audio process(es)"
        }
        return "could not update the system tap's exclusions (OSStatus \(status))"
    }

    // MARK: - System tap

    private func createSystemTap() throws {
        excludedObjects = excludedProcessObjects()
        let desc = CATapDescription(stereoGlobalTapButExcludeProcesses: excludedObjects)
        desc.name = "RichOS System Capture"
        desc.isPrivate = true
        desc.muteBehavior = .unmuted // keep audio audible to the user while we tap a copy
        tapDescription = desc

        var status = AudioHardwareCreateProcessTap(desc, &tapID)
        guard status == noErr, tapID != kAudioObjectUnknown else {
            throw err("AudioHardwareCreateProcessTap failed", status)
        }

        // The tap's output stream format (so we downmix/interleave correctly, at its own rate).
        if let asbd = tapFormat() { tapSampleRate = asbd.mSampleRate > 0 ? asbd.mSampleRate : 48_000 }

        // A private aggregate device holding just the tap. Audio still flows to the real output.
        let aggUID = "com.richos.companion.aggregate.\(UUID().uuidString)"
        let description: [String: Any] = [
            kAudioAggregateDeviceNameKey as String: "RichOS Capture",
            kAudioAggregateDeviceUIDKey as String: aggUID,
            kAudioAggregateDeviceIsPrivateKey as String: true,
            kAudioAggregateDeviceIsStackedKey as String: false,
            kAudioAggregateDeviceTapAutoStartKey as String: true,
            kAudioAggregateDeviceTapListKey as String: [
                [kAudioSubTapUIDKey as String: desc.uuid.uuidString]
            ],
        ]
        status = AudioHardwareCreateAggregateDevice(description as CFDictionary, &aggregateID)
        guard status == noErr, aggregateID != kAudioObjectUnknown else {
            throw err("AudioHardwareCreateAggregateDevice failed", status)
        }
    }

    private func startTapIO() throws {
        let channels = Int(tapFormat()?.mChannelsPerFrame ?? 2)
        var status = AudioDeviceCreateIOProcIDWithBlock(&ioProcID, aggregateID, ioQueue) {
            [weak self] (_, inInputData, _, _, _) in
            self?.handleTapInput(inInputData, channels: channels)
        }
        guard status == noErr, ioProcID != nil else {
            throw err("AudioDeviceCreateIOProcIDWithBlock failed", status)
        }

        // This is the call that triggers the "System Audio Recording" TCC prompt on first run.
        status = AudioDeviceStart(aggregateID, ioProcID)
        guard status == noErr else { throw err("AudioDeviceStart failed", status) }
        tapRunning = true
    }

    private func handleTapInput(_ list: UnsafePointer<AudioBufferList>, channels: Int) {
        let abl = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: list))
        guard let first = abl.first, let raw = first.mData else { return }
        let ch = max(1, Int(first.mNumberChannels))
        let frameCount = Int(first.mDataByteSize) / (MemoryLayout<Float>.size * ch)
        guard frameCount > 0 else { return }
        let ptr = raw.assumingMemoryBound(to: Float.self)
        let interleaved = Array(UnsafeBufferPointer(start: ptr, count: frameCount * ch))
        let mono = ChannelMixer.downmixToMono(interleaved: interleaved, channelCount: ch)
        meter(mono, mic: false)
        writer?.pushSystem(mono)
    }

    private func tapFormat() -> AudioStreamBasicDescription? {
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioTapPropertyFormat,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var asbd = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        let status = AudioObjectGetPropertyData(tapID, &addr, 0, nil, &size, &asbd)
        return status == noErr ? asbd : nil
    }

    // MARK: - Microphone (resampled to the tap rate so both sources stay frame-aligned)

    private func startMic() throws {
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard let outFormat = AVAudioFormat(
            commonFormat: .pcmFormatFloat32, sampleRate: tapSampleRate, channels: 1, interleaved: false
        ) else { throw err("could not build mic output format", -1) }
        converter = AVAudioConverter(from: inFormat, to: outFormat)

        input.installTap(onBus: 0, bufferSize: 4096, format: inFormat) { [weak self] buffer, _ in
            guard let self = self, let converter = self.converter else { return }
            let ratio = outFormat.sampleRate / inFormat.sampleRate
            let cap = AVAudioFrameCount(Double(buffer.frameLength) * ratio + 16)
            guard let out = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: cap) else { return }
            var fed = false
            var e: NSError?
            converter.convert(to: out, error: &e) { _, status in
                if fed { status.pointee = .noDataNow; return nil }
                fed = true; status.pointee = .haveData; return buffer
            }
            if let ch = out.floatChannelData, out.frameLength > 0 {
                let mono = Array(UnsafeBufferPointer(start: ch[0], count: Int(out.frameLength)))
                self.meter(mono, mic: true)
                self.writer?.pushMic(mono)
            }
        }
        engine.prepare()
        try engine.start()
        micRunning = true
    }

    func stop() {
        if let ioProcID = ioProcID, aggregateID != kAudioObjectUnknown {
            AudioDeviceStop(aggregateID, ioProcID)
            AudioDeviceDestroyIOProcID(aggregateID, ioProcID)
        }
        if aggregateID != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(aggregateID) }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        engine.inputNode.removeTap(onBus: 0)
        if engine.isRunning { engine.stop() }
        tapRunning = false
        micRunning = false
    }

    private func err(_ msg: String, _ status: OSStatus) -> NSError {
        NSError(domain: "CoreAudioCapture", code: Int(status),
                userInfo: [NSLocalizedDescriptionKey: "\(msg) (OSStatus \(status))"])
    }
}
#endif

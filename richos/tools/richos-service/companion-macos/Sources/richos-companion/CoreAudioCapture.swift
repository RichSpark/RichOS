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
///   sources stay frame-aligned in `SessionWriter`. It follows the default input through the call:
///   on a device change (a headset plugged in) a new engine starts on the new input and the time
///   missed becomes silence on the microphone side, so the two sides stay aligned (`MicTimeline`).
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
    private var refusedObjects: [AudioObjectID]?
    private var tapDescription: CATapDescription?
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var ioProcID: AudioDeviceIOProcID?
    private let ioQueue = DispatchQueue(label: "com.richos.companion.tap-io")
    private var tapSampleRate: Double = 48_000
    private(set) var tapRunning = false

    // The microphone: one engine per input device. Everything below is touched on `micQueue`
    // (start, device changes, stop), except the timeline, which the engine's tap block writes.
    private let micQueue = DispatchQueue(label: "com.richos.companion.mic")
    private var engine: AVAudioEngine?
    private var configObserver: NSObjectProtocol?
    private var defaultInputListener: AudioObjectPropertyListenerBlock?
    private var micStopped = false
    private var restartScheduled = false
    private var restartAttempt = 0
    private let timelineLock = NSLock()
    private var timeline = MicTimeline()
    /// Sentences about the microphone's device changes (`Notifier.info` in the live capture).
    var onInfo: ((String) -> Void)?

    private let runningLock = NSLock()
    private var micRunningValue = false
    /// Read by the 1 Hz health tick; written on `micQueue`. Stays true across a restart in
    /// progress: only a restart that failed every try makes the microphone "not running".
    private(set) var micRunning: Bool {
        get { runningLock.lock(); defer { runningLock.unlock() }; return micRunningValue }
        set { runningLock.lock(); micRunningValue = newValue; runningLock.unlock() }
    }

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

    /// Start audio into `writer`: the tap's IO and the microphone, which follows the default input
    /// through the call (a headset plugged in mid-call is recorded from then on).
    func run(writer: SessionWriter) throws {
        self.writer = writer
        try startTapIO()
        try micQueue.sync {
            try startMic()
            watchDefaultInput()
        }
    }

    /// LEVELS ONLY (`richos-companion level`): the system tap's IO and nothing else. No microphone,
    /// no writer, nothing kept: each block goes to `onBlock` and is gone. The meeting prompt's voice
    /// bar moves with the call's sound this way (CEO, 2026-10-13: "move with the call's sound as
    /// designed"); the app starts it only once a capture has shown the system-audio grant exists.
    func runLevels(onBlock: @escaping ([Float]) -> Void) throws {
        levelSink = onBlock
        try startTapIO()
    }
    private var levelSink: (([Float]) -> Void)?

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
        // A set the tap already refused is not asked again every second: on a running tap macOS
        // can answer '!hog' (OSStatus 560492391; end-detection run 2, 2026-10-13, 111 lines of it).
        // The exclusions made at prepare() still hold; it is said once per new set.
        guard now != excludedObjects, now != refusedObjects else { return nil }
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
        refusedObjects = now
        return "could not update the system tap's exclusions (OSStatus \(status)); the ones set at the start still hold"
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
        levelSink?(mono)
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

    /// Start the microphone on the CURRENT default input with a new engine. `afterRestart`: the
    /// input is replacing one that stopped, so its first buffer fills the time missed (`MicTimeline`).
    private func startMic(afterRestart: Bool = false) throws {
        let engine = AVAudioEngine()
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0, inFormat.channelCount > 0 else {
            throw err("the input device offers no audio format", -1)
        }
        guard let outFormat = AVAudioFormat(
            commonFormat: .pcmFormatFloat32, sampleRate: tapSampleRate, channels: 1, interleaved: false
        ), let converter = AVAudioConverter(from: inFormat, to: outFormat) else {
            throw err("could not convert the microphone's format (\(inFormat))", -1)
        }
        // The microphone is the input's FIRST channel, on every device. Without this map a device
        // with a discrete multichannel layout (BlackHole 16ch, a multi-input interface) converts to
        // mono as exact silence (measured offline 2026-10-13: 16 discrete channels at 48, 44.1 and
        // 24 kHz all gave 0.0); one and two channels already took the first channel.
        converter.channelMap = [0]

        var first = afterRestart
        input.installTap(onBus: 0, bufferSize: 4096, format: inFormat) { [weak self] buffer, when in
            guard let self = self else { return }
            let ratio = outFormat.sampleRate / inFormat.sampleRate
            let cap = AVAudioFrameCount(Double(buffer.frameLength) * ratio + 16)
            guard let out = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: cap) else { return }
            var fed = false
            var e: NSError?
            converter.convert(to: out, error: &e) { _, status in
                if fed { status.pointee = .noDataNow; return nil }
                fed = true; status.pointee = .haveData; return buffer
            }
            // A buffer that converts to nothing is not on the timeline: its time counts as missed.
            guard let ch = out.floatChannelData, out.frameLength > 0 else { return }
            // Where this buffer sits on the host clock: the time a restart missed is measured.
            let seconds = Double(buffer.frameLength) / inFormat.sampleRate
            let start = when.isHostTimeValid
                ? AVAudioTime.seconds(forHostTime: when.hostTime)
                : AVAudioTime.seconds(forHostTime: mach_absolute_time()) - seconds
            self.timelineLock.lock()
            let missed = self.timeline.buffer(startSeconds: start, seconds: seconds, afterRestart: first)
            self.timelineLock.unlock()
            let mono = Array(UnsafeBufferPointer(start: ch[0], count: Int(out.frameLength)))
            self.meter(mono, mic: true)
            if first {
                first = false
                let pad = self.writer?.pushMic(mono, afterGapOf: missed) ?? 0
                let rate = self.tapSampleRate
                self.micQueue.async {
                    self.say(String(format: "the microphone's first audio on the new input came %.0f ms after the last; "
                        + "%d frames (%.0f ms) of silence keep the two sides aligned", missed * 1000, pad,
                        Double(pad) / rate * 1000))
                }
            } else {
                self.writer?.pushMic(mono)
            }
        }
        engine.prepare()
        do { try engine.start() } catch {
            input.removeTap(onBus: 0)
            throw error
        }
        // The engine stops itself when its input's format changes (another device, another rate).
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.micQueue.async { self?.inputChanged("the microphone's engine stopped on a configuration change") }
        }
        self.engine = engine
        micRunning = true
    }

    // MARK: - Microphone device changes (a headset plugged in mid-call)

    /// Watch the system's default input: plugging in a headset or AirPods, or choosing another
    /// microphone, changes it. Measured before this existed (richos-hq
    /// docs/verification/2026-10-13-granola-call-end-rule/run1): the engine stopped at the first
    /// switch and the CEO's side stayed silent for the rest of the call, even after switching back.
    private func watchDefaultInput() {
        var addr = Self.defaultInputAddress
        let block: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.inputChanged("the default input changed")
        }
        let status = AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &addr, micQueue, block)
        if status == noErr { defaultInputListener = block }
        else { say("could not watch the default input (OSStatus \(status)); the AVAudioEngine notice is the only signal") }
    }

    private static var defaultInputAddress = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)

    /// On `micQueue`. Both signals usually arrive together for one change: they are coalesced into
    /// one restart a moment later, by which time the new device is the default.
    private func inputChanged(_ reason: String) {
        guard !micStopped else { return }
        restartAttempt = 0
        scheduleRestart(after: Self.coalesceSeconds, reason: reason)
    }

    private func scheduleRestart(after delay: Double, reason: String) {
        guard !restartScheduled else { return }
        restartScheduled = true
        micQueue.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self = self else { return }
            self.restartScheduled = false
            self.restartMic(reason: reason)
        }
    }

    /// On `micQueue`: the old engine goes, a new one starts on the current default input. A device
    /// that is not ready yet (a Bluetooth headset still connecting) is tried again; only when every
    /// try failed is the microphone reported as stopped, which the app records as a gap.
    private func restartMic(reason: String) {
        guard !micStopped else { return }
        let began = DispatchTime.now().uptimeNanoseconds
        stopMicEngine()
        do {
            try startMic(afterRestart: true)
            let ms = Double(DispatchTime.now().uptimeNanoseconds - began) / 1e6
            let format = engine?.inputNode.outputFormat(forBus: 0)
            say(String(format: "microphone restarted on %@ (%.0f Hz, %d ch) after %@; the restart took %.0f ms",
                       Self.defaultInputName(), format?.sampleRate ?? 0, Int(format?.channelCount ?? 0), reason, ms))
        } catch {
            if restartAttempt < Self.retrySeconds.count {
                let delay = Self.retrySeconds[restartAttempt]
                restartAttempt += 1
                say("the microphone could not start on \(Self.defaultInputName()) (\(error.localizedDescription)); trying again in \(delay) s")
                scheduleRestart(after: delay, reason: reason)
            } else {
                micRunning = false
                say("the microphone could not start on \(Self.defaultInputName()) after \(Self.retrySeconds.count + 1) tries; "
                    + "it is tried again at the next input change")
            }
        }
    }

    /// Coalescing delay, then the retry spacing for a device that is not ready: 0.1 + 0.25 + 0.5 +
    /// 1 + 2 + 4 = 7.85 s at most before the microphone is reported stopped, inside the writer's
    /// 15 s starve threshold, so a restart that succeeds on any try is never reported as a gap.
    private static let coalesceSeconds = 0.1
    private static let retrySeconds = [0.25, 0.5, 1.0, 2.0, 4.0]

    private func stopMicEngine() {
        if let observer = configObserver { NotificationCenter.default.removeObserver(observer) }
        configObserver = nil
        guard let engine = engine else { return }
        engine.inputNode.removeTap(onBus: 0)
        if engine.isRunning { engine.stop() }
        self.engine = nil
    }

    private static func defaultInputName() -> String {
        var addr = defaultInputAddress
        var device = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &device) == noErr,
              device != kAudioObjectUnknown else { return "no input device" }
        var nameAddr = AudioObjectPropertyAddress(mSelector: kAudioObjectPropertyName,
                                                  mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var name: Unmanaged<CFString>?
        var nameSize = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(device, &nameAddr, 0, nil, &nameSize, &name) == noErr,
              let value = name?.takeRetainedValue() else { return "input device \(device)" }
        return value as String
    }

    private func say(_ message: String) { onInfo?(message) }

    func stop() {
        if let ioProcID = ioProcID, aggregateID != kAudioObjectUnknown {
            AudioDeviceStop(aggregateID, ioProcID)
            AudioDeviceDestroyIOProcID(aggregateID, ioProcID)
        }
        if aggregateID != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(aggregateID) }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        micQueue.sync {
            micStopped = true
            if let block = defaultInputListener {
                var addr = Self.defaultInputAddress
                AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &addr, micQueue, block)
                defaultInputListener = nil
            }
            stopMicEngine()
        }
        tapRunning = false
        micRunning = false
    }

    private func err(_ msg: String, _ status: OSStatus) -> NSError {
        NSError(domain: "CoreAudioCapture", code: Int(status),
                userInfo: [NSLocalizedDescriptionKey: "\(msg) (OSStatus \(status))"])
    }
}
#endif

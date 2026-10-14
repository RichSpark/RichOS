import Foundation

/// Orchestrates one capture session's on-disk contract directory (architecture §6.1 never-silent).
///
/// Lifecycle:
///   1. `start()`  — create the dir, write `session.json` (status "open", empty audio) IMMEDIATELY,
///                   open the WAV. A call that captures nothing therefore leaves a loud open/empty
///                   session on disk — an anomaly the pipeline's reconcile guard flags — never an
///                   absence nobody notices.
///   2. `pushMic` / `pushSystem` — real-time callbacks feed mono Float frames.
///   3. `pump()`   — mix aligned frames (with failover), append to WAV, accrue byte growth.
///   4. `writeHealthTick()` — one `health.ndjson` row per second.
///   5. `close()`  — flush + fsync WAV, rewrite `session.json` (status "closed", populated audio +
///                   health). The closed status is what triggers P1's pipeline.
public final class SessionWriter {
    public let dir: String
    public let params: SessionContract.Params
    private let clock: () -> Int64

    private var wav: WavWriter?
    private let micRing = RingBuffer()
    private let sysRing = RingBuffer()
    /// Microphone-side silence the failover wrote since the microphone last delivered samples.
    private let micGapLock = NSLock()
    private var micSilenceFilled = 0
    /// Frames delivered by each side, and the microphone silence added for restarts (health rows).
    private var micFrames = 0, sysFrames = 0, micFillFrames = 0

    private var firstAudioMs: Int64?
    private var lastAudioMs: Int64?
    private var lastBytesTotal = 0
    private var health = SessionContract.HealthSummary()
    private var healthHandle: FileHandle?

    /// Failover threshold: a source silent this long while the other is live trips silence-fill+alarm.
    public var starveThresholdMs: Int64 = 15_000

    public private(set) var closed = false
    public private(set) var lastDecision: MixDecision?

    public init(rootZone: String, params: SessionContract.Params, clock: @escaping () -> Int64) {
        self.dir = (rootZone as NSString).appendingPathComponent(params.sessionId)
        self.params = params
        self.clock = clock
    }

    /// Write `session.json` (open) + open the WAV part. Idempotent-safe to call once.
    public func start() throws {
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        try writeSessionJSON(status: "open", endedAt: nil, parts: [])
        let wavPath = (dir as NSString).appendingPathComponent(SessionContract.audioPartFile(0))
        wav = try WavWriter(path: wavPath, sampleRate: params.sampleRate, channels: 2, bitsPerSample: 16)
        let healthPath = (dir as NSString).appendingPathComponent("health.ndjson")
        FileManager.default.createFile(atPath: healthPath, contents: nil)
        healthHandle = FileHandle(forWritingAtPath: healthPath)
    }

    public func pushMic(_ mono: [Float]) {
        micGapLock.lock(); micSilenceFilled = 0; micFrames += mono.count; micGapLock.unlock()
        micRing.append(mono, nowMs: clock())
    }
    public func pushSystem(_ mono: [Float]) {
        micGapLock.lock(); sysFrames += mono.count; micGapLock.unlock()
        sysRing.append(mono, nowMs: clock())
    }

    /// The first microphone samples after the microphone was restarted on a new input device, which
    /// missed `seconds` of audio (`MicTimeline`). That time goes in as silence ahead of `mono`, so
    /// the two sides stay paired frame for frame. Silence the failover already wrote on the
    /// microphone side during the gap (a restart that took longer than `starveThresholdMs`) counts
    /// toward it, so the gap is never filled twice. Returns the frames of silence added.
    @discardableResult
    public func pushMic(_ mono: [Float], afterGapOf seconds: Double) -> Int {
        let missed = Int((max(0, seconds) * Double(params.sampleRate)).rounded())
        micGapLock.lock()
        let pad = max(0, missed - micSilenceFilled)
        micSilenceFilled = 0
        micFrames += mono.count
        micFillFrames += pad
        micGapLock.unlock()
        if pad > 0 { micRing.append([Float](repeating: 0, count: pad), nowMs: clock()) }
        micRing.append(mono, nowMs: clock())
        return pad
    }

    /// Mix one block of aligned frames to the WAV. Returns bytes written this pump.
    @discardableResult
    public func pump() throws -> Int {
        guard let wav = wav, !closed else { return 0 }
        let now = clock()
        let micSilent = micRing.lastAppend() == 0 ? 0 : now - micRing.lastAppend()
        let sysSilent = sysRing.lastAppend() == 0 ? 0 : now - sysRing.lastAppend()
        let decision = MixDecision.decide(
            micAvail: micRing.count,
            systemAvail: sysRing.count,
            micSilentMs: micSilent,
            systemSilentMs: sysSilent,
            starveThresholdMs: starveThresholdMs
        )
        lastDecision = decision
        guard decision.frames > 0 else { return 0 }

        let mic = decision.useMic ? micRing.take(decision.frames)
                                  : [Float](repeating: 0, count: decision.frames)
        if !decision.useMic { micGapLock.lock(); micSilenceFilled += decision.frames; micGapLock.unlock() }
        let sys = decision.useSystem ? sysRing.take(decision.frames)
                                     : [Float](repeating: 0, count: decision.frames)
        // Guard alignment even under failover silence-fill.
        let n = min(mic.count, sys.count)
        let interleaved = ChannelMixer.interleaveToInt16(mic: Array(mic[0..<n]), system: Array(sys[0..<n]))
        let bytes = ChannelMixer.int16LEBytes(interleaved)
        try wav.append(bytes)
        if firstAudioMs == nil { firstAudioMs = now }
        lastAudioMs = now
        return bytes.count
    }

    /// Flush the WAV to disk (fsync) — the ≤ one-flush-window crash guarantee.
    public func flush() throws { try wav?.flush() }

    /// Total bytes on disk for the audio part so far — the watchdog's ground-truth liveness signal.
    public var pumpedBytesTotal: Int { wav?.totalBytesOnDisk ?? 0 }

    /// Emit one health row + accrue the rolled-up summary from precomputed per-channel levels
    /// (peak + RMS over the last second). Kept level-based (not block-based) so the live capture can
    /// meter cheaply in its audio callbacks without re-buffering a second of samples.
    public func writeHealthTick(
        micPeak: Double, micRms: Double, sysPeak: Double, sysRms: Double,
        tapRunning: Bool, micRunning: Bool
    ) throws {
        let now = clock()
        let bytesTotal = wav?.totalBytesOnDisk ?? 0
        let delta = bytesTotal - lastBytesTotal
        lastBytesTotal = bytesTotal

        var problems: [String] = []
        var level = "green"
        // Never-silent signals (§6.2): stalled byte growth, and failover-tripped starvation.
        if health.heartbeats > 20 && delta == 0 { problems.append("no audio bytes written in the last second"); level = "amber" }
        if let d = lastDecision, d.micStarved { problems.append("microphone source stalled — silence-filling LEFT (mic-only failover the other way)"); level = "red" }
        if let d = lastDecision, d.systemStarved { problems.append("system-audio tap stalled — silence-filling RIGHT (possible permission revocation)"); level = "red" }
        if !tapRunning { problems.append("system tap IOProc not running"); level = "red" }
        if !micRunning { problems.append("microphone input not running"); level = "red" }

        micGapLock.lock()
        let (micIn, sysIn, fill) = (micFrames, sysFrames, micFillFrames)
        micGapLock.unlock()
        let sample = SessionContract.HealthSample(
            sessionId: params.sessionId, t: now,
            micRms: micPeak, sysRms: sysPeak,
            micRmsMean: micRms, sysRmsMean: sysRms,
            bytesTotal: bytesTotal, bytesDelta: delta, part: 0,
            tapRunning: tapRunning, micRunning: micRunning, level: level, problems: problems,
            micFrames: micIn, sysFrames: sysIn, micFillFrames: fill
        )
        if let h = healthHandle {
            try h.write(contentsOf: Data((sample.json().compact() + "\n").utf8))
        }
        health.heartbeats += 1
        switch level {
        case "green": health.greenSeconds += 1
        case "amber": health.amberSeconds += 1
        default: health.redSeconds += 1
        }
        let rank = ["green": 0, "amber": 1, "red": 2]
        if (rank[level] ?? 0) > (rank[health.worstLevel] ?? 0) { health.worstLevel = level }
        health.recordsWritten += 1
    }

    /// Drain remaining buffered audio, flush, and rewrite `session.json` as closed+populated.
    public func close() throws {
        guard !closed else { return }
        // Drain whatever is left in both rings before finalizing.
        for _ in 0..<64 { if try pump() == 0 { break } }
        try wav?.close()
        let bytes = wav?.totalBytesOnDisk ?? 0
        let part = SessionContract.AudioPart(part: 0, bytes: bytes, firstChunkAt: firstAudioMs, lastChunkAt: lastAudioMs)
        try writeSessionJSON(status: "closed", endedAt: clock(), parts: bytes > 0 ? [part] : [])
        try healthHandle?.close()
        closed = true
    }

    private func writeSessionJSON(status: String, endedAt: Int64?, parts: [SessionContract.AudioPart]) throws {
        let json = SessionContract.sessionJSON(params, status: status, endedAt: endedAt, audioParts: parts, health: health)
        let path = (dir as NSString).appendingPathComponent("session.json")
        try json.data().write(to: URL(fileURLWithPath: path))
    }
}

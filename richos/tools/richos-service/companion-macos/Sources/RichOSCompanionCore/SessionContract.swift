import Foundation

/// Builds the frozen capture->pipeline contract (schemaVersion 2) that P1's pipeline consumes.
///
/// The single most load-bearing interface in the system (architecture §3 / P1 README "The frozen
/// capture->pipeline contract"). The macOS companion is "just another producer" of the SAME
/// session directory the extension writes — so the pipeline (§4) and reliability model (§6) are
/// surface-independent. Every field name/shape below is matched to what `lib/contract.js`,
/// `lib/pipeline.js`, `lib/reconcile.js` and `merge.js#renderMarkdown` read.
public enum SessionContract {

    /// The frozen `capture.source` enum value for this surface (P1 `CAPTURE_SOURCE.macos`).
    /// NB: the frozen enum is `desktop-companion-macos` — the task brief's shorthand "companion-macos"
    /// resolves to this exact literal, which is what `contract.js` and the pipeline expect.
    public static let captureSource = "desktop-companion-macos"
    public static let captureMethod = "coreaudio-tap+mic"

    /// Portable, `:`-free, tz-unambiguous timestamp — byte-identical to the extension's
    /// `session.js#stampFor`: `new Date(ms).toISOString().replace(/\.\d+Z$/, 'Z').replace(/:/g, '-')`.
    public static func stampFor(_ epochMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: Double(epochMs) / 1000.0)
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let c = cal.dateComponents([.year, .month, .day, .hour, .minute, .second], from: date)
        return String(
            format: "%04d-%02d-%02dT%02d-%02d-%02dZ",
            c.year!, c.month!, c.day!, c.hour!, c.minute!, c.second!
        )
    }

    /// `${stamp}--${platformId}--${slug}` — matches `session.js#sessionDirName`.
    public static func sessionDirName(startedAt: Int64, platformId: String, slug: String) -> String {
        "\(stampFor(startedAt))--\(platformId)--\(slug)"
    }

    public static func audioPartFile(_ part: Int) -> String {
        String(format: "audio-part-%02d.wav", part)
    }

    /// A finished audio part's accounting, as it appears in `session.json.audio.parts[]`.
    public struct AudioPart {
        public let part: Int
        public let bytes: Int
        public let firstChunkAt: Int64?
        public let lastChunkAt: Int64?
        public init(part: Int, bytes: Int, firstChunkAt: Int64?, lastChunkAt: Int64?) {
            self.part = part
            self.bytes = bytes
            self.firstChunkAt = firstChunkAt
            self.lastChunkAt = lastChunkAt
        }
    }

    /// Rolled-up health accounting for `session.json.health` (mirrors `session.js#accrueHealth`).
    public struct HealthSummary {
        public var heartbeats = 0
        public var greenSeconds = 0
        public var amberSeconds = 0
        public var redSeconds = 0
        public var worstLevel = "green"
        public var recordsWritten = 0
        public init() {}
    }

    public struct Params {
        public var sessionId: String
        public var startedAt: Int64
        public var sampleRate: Int
        public var chunkMs: Int
        public var micEnabled: Bool
        public var captureTarget: String
        public var platformId: String
        public var platformLabel: String
        public var platformSlug: String
        public var companionVersion: String
        public var processHint: String?
        public init(
            sessionId: String, startedAt: Int64, sampleRate: Int, chunkMs: Int, micEnabled: Bool,
            captureTarget: String, platformId: String, platformLabel: String, platformSlug: String,
            companionVersion: String, processHint: String?
        ) {
            self.sessionId = sessionId
            self.startedAt = startedAt
            self.sampleRate = sampleRate
            self.chunkMs = chunkMs
            self.micEnabled = micEnabled
            self.captureTarget = captureTarget
            self.platformId = platformId
            self.platformLabel = platformLabel
            self.platformSlug = platformSlug
            self.companionVersion = companionVersion
            self.processHint = processHint
        }
    }

    /// The full v2 `session.json` object. `status`/`endedAt`/`audio`/`health` change between the
    /// call-START write (`open`, empty audio) and the call-CLOSE write (`closed`, populated) — the
    /// never-silent inversion (§6.1): the record is on disk from second zero, so a call that
    /// captured nothing is a loud `open`/empty anomaly, never an absence.
    public static func sessionJSON(
        _ p: Params,
        status: String,
        endedAt: Int64?,
        audioParts: [AudioPart],
        health: HealthSummary
    ) -> JSON {
        let bytesTotal = audioParts.reduce(0) { $0 + $1.bytes }

        let partsJSON: [JSON] = audioParts.map { part in
            .object([
                ("part", .int(part.part)),
                ("file", .string(audioPartFile(part.part))),
                ("bytes", .int(part.bytes)),
                ("firstChunkAt", part.firstChunkAt.map { JSON.int(Int($0)) } ?? .null),
                ("lastChunkAt", part.lastChunkAt.map { JSON.int(Int($0)) } ?? .null),
                ("written", .bool(true)),
            ])
        }

        return .object([
            ("schemaVersion", .int(2)),
            ("sessionId", .string(p.sessionId)),
            ("dir", .string(p.sessionId)),
            ("status", .string(status)),
            ("producer", .object([
                ("product", .string("RichOS macOS companion")),
                ("module", .string("call-capture")),
                ("companionVersion", .string(p.companionVersion)),
            ])),
            ("platform", .object([
                ("id", .string(p.platformId)),
                ("label", .string(p.platformLabel)),
                ("slug", .string(p.platformSlug)),
            ])),
            ("startedAt", .int(Int(p.startedAt))),
            ("endedAt", endedAt.map { JSON.int(Int($0)) } ?? .null),
            ("capture", .object([
                ("source", .string(captureSource)),
                ("method", .string(captureMethod)),
                ("channels", .object([
                    ("left", .string("microphone (me)")),
                    ("right", .string("system/tap (everyone else)")),
                ])),
                ("captureTarget", .string(p.captureTarget)),
                ("sampleRate", .int(p.sampleRate)),
                ("container", .string("audio/wav;codecs=pcm_s16le")),
                ("micEnabled", .bool(p.micEnabled)),
                ("chunkMs", .int(p.chunkMs)),
            ])),
            ("audio", .object([
                ("parts", .array(partsJSON)),
                ("bytesTotal", .int(bytesTotal)),
                ("chunkCount", .int(audioParts.count)),
            ])),
            ("health", .object([
                ("heartbeats", .int(health.heartbeats)),
                ("greenSeconds", .int(health.greenSeconds)),
                ("amberSeconds", .int(health.amberSeconds)),
                ("redSeconds", .int(health.redSeconds)),
                ("worstLevel", .string(health.worstLevel)),
                ("recordsWritten", .int(health.recordsWritten)),
            ])),
            ("alerts", .array([])),
            ("recovery", .array([])),
            // §3.4: a desktop-app companion has NO platform captions. Present-from-birth, empty.
            ("captions", .object([
                ("available", .bool(false)),
                ("adapter", .null),
                ("adapterVersion", .null),
                ("count", .int(0)),
                ("speakers", .array([])),
                ("degraded", .bool(false)),
            ])),
            ("mode", .string("full")),
            ("notes", .array([])),
            // §5.4 dedup handshake block — a lone companion session owns itself.
            ("ownership", .object([
                ("ownerSurface", .string(captureSource)),
                ("supersedes", .null),
                ("processHint", p.processHint.map { JSON.string($0) } ?? .null),
            ])),
            // Written by the pipeline; born `pending` so a never-run pipeline is visible on disk.
            ("pipeline", .object([
                ("status", .string("pending")),
                ("model", .null),
                ("modelRuns", .array([])),
                ("ffmpegVersion", .null),
                ("whisperVersion", .null),
                ("loroCorrection", .object([
                    ("applied", .bool(false)),
                    ("entitiesVersion", .null),
                    ("corrections", .int(0)),
                ])),
            ])),
        ])
    }

    /// One `health.ndjson` row (§3.3 absolute-`t` keyed; §6.2 second-by-second signals). Contract:
    /// "per-channel levels, byte growth, state." Named for the companion's two sources (mic / system
    /// tap) rather than the extension's (mic / tab).
    public struct HealthSample {
        public var sessionId: String
        public var t: Int64
        public var micRms: Double
        public var sysRms: Double
        public var micRmsMean: Double
        public var sysRmsMean: Double
        public var bytesTotal: Int
        public var bytesDelta: Int
        public var part: Int
        public var tapRunning: Bool
        public var micRunning: Bool
        public var level: String
        public var problems: [String]
        /// Frames each side has delivered since the start, and the silence added on the microphone
        /// side to fill restarts (`MicTimeline`). Against the clock they show which side lost time
        /// and when: the two sides are paired frame by frame, so a side that delivers fewer frames
        /// than the clock slides the other side's words later in the recording.
        public var micFrames: Int
        public var sysFrames: Int
        public var micFillFrames: Int
        public init(
            sessionId: String, t: Int64, micRms: Double, sysRms: Double, micRmsMean: Double,
            sysRmsMean: Double, bytesTotal: Int, bytesDelta: Int, part: Int, tapRunning: Bool,
            micRunning: Bool, level: String, problems: [String],
            micFrames: Int = 0, sysFrames: Int = 0, micFillFrames: Int = 0
        ) {
            self.micFrames = micFrames
            self.sysFrames = sysFrames
            self.micFillFrames = micFillFrames
            self.sessionId = sessionId
            self.t = t
            self.micRms = micRms
            self.sysRms = sysRms
            self.micRmsMean = micRmsMean
            self.sysRmsMean = sysRmsMean
            self.bytesTotal = bytesTotal
            self.bytesDelta = bytesDelta
            self.part = part
            self.tapRunning = tapRunning
            self.micRunning = micRunning
            self.level = level
            self.problems = problems
        }

        public func json() -> JSON {
            .object([
                ("sessionId", .string(sessionId)),
                ("t", .int(Int(t))),
                ("micRms", .double(round6(micRms))),
                ("sysRms", .double(round6(sysRms))),
                ("micRmsMean", .double(round6(micRmsMean))),
                ("sysRmsMean", .double(round6(sysRmsMean))),
                ("bytesTotal", .int(bytesTotal)),
                ("bytesDelta", .int(bytesDelta)),
                ("part", .int(part)),
                ("tapRunning", .bool(tapRunning)),
                ("micRunning", .bool(micRunning)),
                ("level", .string(level)),
                ("problems", .array(problems.map { JSON.string($0) })),
                ("micFrames", .int(micFrames)),
                ("sysFrames", .int(sysFrames)),
                ("micFillFrames", .int(micFillFrames)),
            ])
        }

        private func round6(_ x: Double) -> Double {
            (x * 1_000_000).rounded() / 1_000_000
        }
    }
}

import Foundation

/// Which OTHER processes are using the microphone, as JSON lines for the app (`richos-companion
/// watch`; Granola PRD V4, "Meeting prompts and automatic completion").
///
/// The app needs two facts from this: a call app started using the microphone (an unscheduled call
/// to prompt for) and the call app stopped using it (the end of the call, after the app's own grace
/// interval). A device-wide "mic in use" flag cannot give the second while RichOS itself holds the
/// microphone for a capture, so the unit here is the PROCESS: Core Audio's per-process
/// `kAudioProcessPropertyIsRunningInput`, with RichOS's own processes excluded by the caller.
///
/// Everything in this file is pure: the executable reads Core Audio into `[AudioProcess]` and hands
/// each reading to `MicActivityTracker.update`, which says what changed.
public struct AudioProcess: Equatable {
    public let pid: Int32
    public let bundleId: String?
    public let runningInput: Bool
    public let runningOutput: Bool

    public init(pid: Int32, bundleId: String?, runningInput: Bool, runningOutput: Bool) {
        self.pid = pid
        self.bundleId = bundleId
        self.runningInput = runningInput
        self.runningOutput = runningOutput
    }

    /// The app this process belongs to, by bundle id. Chrome, Edge, Brave and Electron apps run
    /// audio in helper processes whose bundle id is the app's plus `.helper` (and sometimes a
    /// further suffix: `com.google.Chrome.helper.renderer`); a call is the app's, not the helper's.
    public var family: String? { AudioProcess.family(of: bundleId) }

    public static func family(of bundleId: String?) -> String? {
        guard let id = bundleId, !id.isEmpty else { return nil }
        let parts = id.split(separator: ".", omittingEmptySubsequences: false)
        if let i = parts.firstIndex(where: { $0.lowercased() == "helper" }), i > 0 {
            return parts[..<i].joined(separator: ".")
        }
        return id
    }
}

/// Processes the watcher must never report: RichOS's own voice and dictation (the app's pid), the
/// capture companion recording a meeting, and the watcher itself.
public struct MicActivityFilter: Equatable {
    public var pids: Set<Int32>
    /// Matched against a process's bundle id AND its family, so excluding `com.richos.app`
    /// also excludes any `com.richos.app.helper`.
    public var bundleIds: Set<String>

    public init(pids: Set<Int32> = [], bundleIds: Set<String> = []) {
        self.pids = pids
        self.bundleIds = bundleIds
    }

    public func excludes(_ p: AudioProcess) -> Bool {
        if pids.contains(p.pid) { return true }
        if let id = p.bundleId, bundleIds.contains(id) { return true }
        if let fam = p.family, bundleIds.contains(fam) { return true }
        return false
    }
}

public enum MicActivityChange: Equatable {
    /// The process began running input (or was already running it at the first reading).
    case started(AudioProcess)
    /// The process released its call audio. `exited` is true when it left Core Audio's process list
    /// altogether (quit or crashed).
    case stopped(AudioProcess, exited: Bool)
}

/// Turns successive full readings of Core Audio's process list into started/stopped changes.
/// A reading is the whole list each time, so a missed callback can never leave the state wrong for
/// longer than the next reading: there is no inference from the ABSENCE of a callback.
public struct MicActivityTracker {
    public let filter: MicActivityFilter
    /// Processes that opened input and whose call audio has not ended, by PID.
    public private(set) var active: [Int32: AudioProcess] = [:]
    private var chromeOutput: [Int32: (began: Int64, linked: Bool)] = [:]

    /// Chrome can open playback a few seconds before the microphone as a call joins.
    /// Playback already running before the join is no evidence that this call is still going.
    private static let chromeJoinWindowMs: Int64 = 8_000

    public init(filter: MicActivityFilter) { self.filter = filter }

    public mutating func update(_ reading: [AudioProcess], at ms: Int64) -> [MicActivityChange] {
        var now: [Int32: AudioProcess] = [:]
        var present = Set<Int32>()
        var output: [Int32: (began: Int64, linked: Bool)] = [:]
        for p in reading where !filter.excludes(p) {
            present.insert(p.pid)
            if p.family == "com.google.Chrome", p.runningOutput {
                let began = chromeOutput[p.pid]?.began ?? ms
                let startingInput = p.runningInput && active[p.pid] == nil
                let linked = chromeOutput[p.pid]?.linked == true ||
                    (startingInput && ms >= began && ms - began <= Self.chromeJoinWindowMs)
                output[p.pid] = (began, linked)
            }
            // Zoom in Chrome can close and reopen its input during join while the same process
            // still plays the call. Only input can start detection. Playback linked at that
            // first input keeps the call alive until the process releases both streams.
            if p.runningInput || (active[p.pid] != nil && output[p.pid]?.linked == true) {
                now[p.pid] = p
            }
        }
        chromeOutput = output
        var changes: [MicActivityChange] = []
        // Stopped first, in pid order, so the output is deterministic.
        for pid in active.keys.sorted() where now[pid] == nil {
            changes.append(.stopped(active[pid]!, exited: !present.contains(pid)))
        }
        for pid in now.keys.sorted() where active[pid] == nil {
            changes.append(.started(now[pid]!))
        }
        active = now
        return changes
    }

    /// One JSON line for a change. `app` is the display name of the process's app when the caller
    /// could find one (e.g. "zoom.us", "Google Chrome"), else null.
    public static func line(_ change: MicActivityChange, at ms: Int64, app: String?) -> JSON {
        let p: AudioProcess
        var fields: [(String, JSON)] = [("type", .string("input"))]
        switch change {
        case .started(let s):
            p = s
            fields.append(("state", .string("started")))
        case .stopped(let s, let exited):
            p = s
            fields.append(("state", .string("stopped")))
            fields.append(("exited", .bool(exited)))
        }
        fields += [
            ("at", .int(Int(ms))),
            ("pid", .int(Int(p.pid))),
            ("bundleId", p.bundleId.map(JSON.string) ?? .null),
            ("family", p.family.map(JSON.string) ?? .null),
            ("app", app.map(JSON.string) ?? .null),
        ]
        return .object(fields)
    }
}

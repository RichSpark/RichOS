import Foundation
import RichOSCompanionCore
#if canImport(CoreAudio)
import AppKit
import CoreAudio

/// `richos-companion watch`: report OTHER processes' microphone use to the app as JSON lines on
/// stdout (Granola PRD V4: "a small `watch` command ... per-process Core Audio activity, excluding
/// RichOS voice/dictation and capture processes").
///
/// Lines, one JSON object each:
///   {"type":"hello","version":..,"at":ms,"pid":<watcher pid>}
///   {"type":"input","state":"started"|"stopped",["exited":bool,]"at":ms,"pid":..,"bundleId":..,"family":..,"app":..}
///   {"type":"error","at":ms,"message":..}
///   {"type":"trace","at":ms,"processes":[{"pid":..,"bundleId":..,"in":bool,"out":bool}]}  (--trace only:
///     every process Core Audio lists, for measurement; the app never passes it)
///
/// Reading the process list needs no permission: no TCC prompt, no microphone opened.
///
/// LIFETIME: the watcher exits when its stdin reaches end of file, so it dies with the app that
/// started it (the app holds the pipe; an app that crashes closes it). SIGINT/SIGTERM also end it.
///
/// Each Core Audio notification (the process list changed, or a process started or stopped input)
/// triggers a FULL re-read of the list, coalesced on one serial queue; a full re-read also runs
/// every `--poll` seconds (default 2) as a backstop for a notification Core Audio did not send.
@available(macOS 14.2, *)
final class MicWatcher {
    private let queue = DispatchQueue(label: "com.richos.companion.watch")
    private var tracker: MicActivityTracker
    private var listened = Set<AudioObjectID>()
    private var rescanPending = false

    private let trace: Bool
    private var lastTrace: [AudioProcess] = []

    init(filter: MicActivityFilter, trace: Bool) {
        tracker = MicActivityTracker(filter: filter)
        self.trace = trace
    }

    func start(pollSeconds: Double) {
        emit(.object([
            ("type", .string("hello")), ("version", .string(VERSION)),
            ("at", .int(Int(nowMs()))), ("pid", .int(Int(getpid()))),
        ]))
        var addr = MicWatcher.address(kAudioHardwarePropertyProcessObjectList)
        let status = AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &addr, queue) {
            [weak self] _, _ in self?.scheduleRescan()
        }
        if status != noErr { emitError("could not listen for the process list (OSStatus \(status))") }
        queue.async { self.rescan() }
        let poll = DispatchSource.makeTimerSource(queue: queue)
        poll.schedule(deadline: .now() + pollSeconds, repeating: pollSeconds)
        poll.setEventHandler { [weak self] in self?.rescan() }
        poll.resume()
        self.poll = poll
    }
    private var poll: DispatchSourceTimer?

    private func scheduleRescan() {
        // Already on `queue`. Coalesce a burst of notifications into one read.
        if rescanPending { return }
        rescanPending = true
        queue.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.rescanPending = false
            self?.rescan()
        }
    }

    private func rescan() {
        guard let ids = MicWatcher.processObjects() else {
            emitError("could not read the process list")
            return
        }
        var reading: [AudioProcess] = []
        for id in ids {
            guard let pid: Int32 = MicWatcher.read(id, kAudioProcessPropertyPID) else { continue }
            let input: UInt32 = MicWatcher.read(id, kAudioProcessPropertyIsRunningInput) ?? 0
            let output: UInt32 = MicWatcher.read(id, kAudioProcessPropertyIsRunningOutput) ?? 0
            reading.append(AudioProcess(pid: pid, bundleId: MicWatcher.bundleId(id),
                                        runningInput: input != 0, runningOutput: output != 0))
            listen(id)
        }
        let at = nowMs()
        if trace {
            // Measurement only: every process Core Audio lists, with both flags, whenever any of
            // that changed. Never read by the app.
            let sorted = reading.sorted { $0.pid < $1.pid }
            if sorted != lastTrace {
                lastTrace = sorted
                emit(.object([
                    ("type", .string("trace")), ("at", .int(Int(at))),
                    ("processes", .array(sorted.map { p in .object([
                        ("pid", .int(Int(p.pid))), ("bundleId", p.bundleId.map(JSON.string) ?? .null),
                        ("in", .bool(p.runningInput)), ("out", .bool(p.runningOutput)),
                    ]) })),
                ]))
            }
        }
        for change in tracker.update(reading) {
            let process: AudioProcess
            switch change {
            case .started(let p): process = p
            case .stopped(let p, _): process = p
            }
            emit(MicActivityTracker.line(change, at: at, app: MicWatcher.appName(process)))
        }
        // Forget listeners for objects that are gone (their ids are not reused while we run, but
        // the set should not grow without bound over days of uptime).
        listened.formIntersection(ids)
    }

    private func listen(_ id: AudioObjectID) {
        if listened.contains(id) { return }
        var addr = MicWatcher.address(kAudioProcessPropertyIsRunningInput)
        let status = AudioObjectAddPropertyListenerBlock(id, &addr, queue) { [weak self] _, _ in
            self?.scheduleRescan()
        }
        if status == noErr { listened.insert(id) }
    }

    private func emit(_ json: JSON) {
        FileHandle.standardOutput.write(Data((json.compact() + "\n").utf8))
    }

    private func emitError(_ message: String) {
        emit(.object([("type", .string("error")), ("at", .int(Int(nowMs()))), ("message", .string(message))]))
    }

    // MARK: Core Audio reads

    static func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                   mElement: kAudioObjectPropertyElementMain)
    }

    static func processObjects() -> [AudioObjectID]? {
        var addr = address(kAudioHardwarePropertyProcessObjectList)
        let system = AudioObjectID(kAudioObjectSystemObject)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr else { return nil }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &addr, 0, nil, &size, &ids) == noErr else { return nil }
        return Array(ids.prefix(Int(size) / MemoryLayout<AudioObjectID>.size))
    }

    static func read<T>(_ id: AudioObjectID, _ selector: AudioObjectPropertySelector) -> T? {
        var addr = address(selector)
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { value.deallocate() }
        guard AudioObjectGetPropertyData(id, &addr, 0, nil, &size, value) == noErr else { return nil }
        return value.pointee
    }

    static func bundleId(_ id: AudioObjectID) -> String? {
        var addr = address(kAudioProcessPropertyBundleID)
        var size = UInt32(MemoryLayout<CFString?>.size)
        var value: Unmanaged<CFString>?
        guard AudioObjectGetPropertyData(id, &addr, 0, nil, &size, &value) == noErr,
              let s = value?.takeRetainedValue() as String?, !s.isEmpty else { return nil }
        return s
    }

    /// The name a person knows the app by: the running app with the process's family bundle id
    /// (Chrome for a Chrome helper), else the process itself.
    static func appName(_ p: AudioProcess) -> String? {
        if let fam = p.family,
           let app = NSRunningApplication.runningApplications(withBundleIdentifier: fam).first,
           let name = app.localizedName {
            return name
        }
        return NSRunningApplication(processIdentifier: p.pid)?.localizedName
    }
}

func cmdWatch(_ args: [String]) {
    guard #available(macOS 14.2, *) else {
        FileErr.write("watch requires macOS 14.2+ (Core Audio process objects).\n")
        exit(1)
    }
    var pids: Set<Int32> = [getpid()]
    var bundles: Set<String> = []
    var poll = 2.0
    var trace = false
    var i = 0
    while i < args.count {
        let a = args[i]
        let v = i + 1 < args.count ? args[i + 1] : nil
        switch a {
        case "--exclude-pid":
            guard let v = v, let n = Int32(v) else { FileErr.write("--exclude-pid needs a number\n"); exit(2) }
            pids.insert(n); i += 2
        case "--exclude-bundle":
            guard let v = v, !v.isEmpty else { FileErr.write("--exclude-bundle needs a bundle id\n"); exit(2) }
            bundles.insert(v); i += 2
        case "--trace":
            trace = true; i += 1
        case "--poll":
            guard let v = v, let s = Double(v), s >= 0.2 else { FileErr.write("--poll needs seconds >= 0.2\n"); exit(2) }
            poll = s; i += 2
        default:
            FileErr.write("watch: unknown argument \(a)\n"); exit(2)
        }
    }
    let watcher = MicWatcher(filter: MicActivityFilter(pids: pids, bundleIds: bundles), trace: trace)
    watcher.start(pollSeconds: poll)

    // Exit when the parent closes our stdin (it quit or crashed), or on SIGINT/SIGTERM.
    DispatchQueue.global(qos: .utility).async {
        while true {
            let data = FileHandle.standardInput.availableData
            if data.isEmpty { exit(0) }
        }
    }
    for sig in [SIGINT, SIGTERM] {
        signal(sig, SIG_IGN)
        let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
        src.setEventHandler { exit(0) }
        src.resume()
        signalSources.append(src)
    }
    withExtendedLifetime(watcher) { RunLoop.main.run() }
}
private var signalSources: [DispatchSourceSignal] = []
#endif

import Darwin
import Foundation

public enum RichFilesDiagnostics {
    public enum Operation: String, Codable, Sendable {
        case suiteRestartRunning, richFilesSuiteRestart, richGlanceSuiteRestart, captureHelperSuiteRestart, gesturesSuiteRestart
        case listing, listingVisible, copy, move, recovery, gitHistory, gitComparison, windowPlacement, transferStall
        case cleanupScan, cleanupDuplicates, cleanupTrash, cleanupDelete, openWithDiscovery
        case windowGestureAvailability, windowGestureMaximize, windowGestureRestore
        case windowGestureDockMinimize, windowGestureDockRestore, desktopDeleteShortcut
        case windowGestureTile, windowGestureTileRestore
        case windowSwitcherList, windowSwitcherActivate
        case glanceCaptureFrame, glanceCaptureOutput, glanceCaptureSession, glanceScrollCapture, glanceCompare, glanceOCR, glanceSearch, glancePin, glanceRedactedCopy, glanceShare
        case glanceCaptureHelperStart, glanceCaptureHelperLogin, glanceCaptureShortcutLaunch, glanceCaptureMenuLaunch
        case glanceDisplay, glanceDecode, glanceFolder, glancePreview, glanceDefaults, glanceMetadata

        fileprivate var isSuiteRestart: Bool {
            switch self {
            case .suiteRestartRunning, .richFilesSuiteRestart, .richGlanceSuiteRestart,
                 .captureHelperSuiteRestart, .gesturesSuiteRestart:
                return true
            default:
                return false
            }
        }

        fileprivate var logFilename: String {
            switch self {
            case .windowGestureAvailability, .windowGestureMaximize, .windowGestureRestore,
                 .windowGestureDockMinimize, .windowGestureDockRestore, .desktopDeleteShortcut,
                 .windowGestureTile, .windowGestureTileRestore, .windowSwitcherList, .windowSwitcherActivate:
                return RichFilesDiagnostics.gestureFilename
            default:
                return RichFilesDiagnostics.filename
            }
        }
    }
    public enum Outcome: String, Codable, Sendable {
        case complete, issues, unavailable, interrupted
        case cancelled = "canceled"
    }

    public enum ScrollCaptureFailure: String, Codable, Sendable {
        case conflictingPixels, lostOverlap, incompleteCoverage, noMotion, sourceChanged
        case streamInterrupted, noFrame, finalFrameUnavailable, sizeLimit, timeLimit, accessibility
    }

    struct Sample: Encodable, Sendable {
        let atMS: Int64
        let id: UUID
        let build: String
        let operation: Operation
        let outcome: Outcome
        let workerMS: Int
        let firstRowsMS: Int?
        let total: Int
        let confirmed: Int
        var scrollCaptureFailure: ScrollCaptureFailure? = nil
        var application: String? = nil
        var pid: Int32? = nil
    }

    static let filename = "richfiles-timing.jsonl"
    static let gestureFilename = "richfiles-window-gestures.jsonl"
    private static let maximumBytes: off_t = 128 * 1024
    private static let writer = DispatchQueue(label: "app.richos.richfiles.diagnostics", qos: .utility)

    public static func record(
        _ operation: Operation, id: UUID, outcome: Outcome, started: TimeInterval,
        total: Int, confirmed: Int, firstRowsMS: Int? = nil, scrollCaptureFailure: ScrollCaptureFailure? = nil
    ) {
        let bundle = Bundle.main
        guard ["app.richos.richfiles", "com.richos.app", "app.richos.richglance", WindowGesturePreferences.helperID, GlanceHelper.bundleID]
            .contains(bundle.bundleIdentifier ?? ""),
              let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first else { return }
        let suiteVersion = operation.isSuiteRestart ? bundle.object(forInfoDictionaryKey: "RichOSSuiteVersion") as? String : nil
        let version = suiteVersion ?? bundle.object(forInfoDictionaryKey: "RichOSSourceRevision") as? String
            ?? bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "unknown"
        let build = String(version.filter { $0.isASCII && ($0.isLetter || $0.isNumber || ".-+".contains($0)) }.prefix(40))
        let sample = Sample(atMS: Int64(Date().timeIntervalSince1970 * 1_000), id: id, build: build,
                            operation: operation, outcome: outcome,
                            workerMS: milliseconds(since: started), firstRowsMS: firstRowsMS,
                            total: total, confirmed: confirmed, scrollCaptureFailure: scrollCaptureFailure,
                            application: operation.isSuiteRestart ? bundle.bundleIdentifier : nil,
                            pid: operation.isSuiteRestart ? getpid() : nil)
        let folder = library.appendingPathComponent("Logs/RichOS", isDirectory: true)
        writer.async { try? append(sample, in: folder) }
    }

    /// Finish enqueued records before a short-lived app exits.
    public static func flush() { writer.sync {} }

    public static func milliseconds(since started: TimeInterval) -> Int {
        max(0, Int((ProcessInfo.processInfo.systemUptime - started) * 1_000))
    }

    static func append(_ sample: Sample, in folder: URL) throws {
        var bytes = try JSONEncoder().encode(sample)
        bytes.append(10)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let descriptor = open(folder.appendingPathComponent(sample.operation.logFilename).path,
                              O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteNoPermission) }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else { throw CocoaError(.fileWriteUnknown) }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0, info.st_nlink == 1 else {
            throw CocoaError(.fileWriteNoPermission)
        }
        if info.st_size > maximumBytes - off_t(bytes.count), ftruncate(descriptor, 0) != 0 {
            throw CocoaError(.fileWriteUnknown)
        }
        try file.write(contentsOf: bytes)
    }
}

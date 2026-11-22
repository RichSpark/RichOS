import AppKit
import CoreServices
import Darwin
import os

/// The installed RichOS sends this only after starting from a verified forward update.
@MainActor
public final class SuiteRestart: NSObject {
    private static let notice = Notification.Name("com.richos.app.suite-update.v1")
    private static let versionKey = "RichOSSuiteVersion"
    private static let maximumSnapshotBytes = 16 * 1024 * 1024
    private static let retryInterval: TimeInterval = 60
    private let bundle = Bundle.main.bundleURL.standardizedFileURL
    private let runningVersion = Bundle.main.object(forInfoDictionaryKey: SuiteRestart.versionKey) as? String
    private let operation: RichFilesDiagnostics.Operation
    private let prepare: () -> Data?
    private let relaunchArguments: (pid_t) -> [String]
    private var retry: Timer?
    private var releasing = false

    public init(operation: RichFilesDiagnostics.Operation, prepare: @escaping () -> Data?,
                relaunchArguments: @escaping (pid_t) -> [String] = { _ in [] }) {
        self.operation = operation
        self.prepare = prepare
        self.relaunchArguments = relaunchArguments
        super.init()
        guard runningVersion != nil else { return }
        record(.suiteRestartRunning, .complete, id: UUID(), since: ProcessInfo.processInfo.systemUptime)
        DistributedNotificationCenter.default().addObserver(
            self, selector: #selector(updateArrived(_:)), name: Self.notice, object: nil,
            suspensionBehavior: .deliverImmediately
        )
    }

    public static var hasBlockingWindow: Bool {
        NSApp.modalWindow != nil || NSApp.windows.contains {
            $0.isDocumentEdited || $0.attachedSheet != nil || $0.isSheet
        } || NSDocumentController.shared.documents.contains(where: \.isDocumentEdited)
    }

    /// Relative paths account for nested apps and login items, and exclude standalone builds.
    private static func richOSBundle(containing bundle: URL) -> URL? {
        var candidate = bundle
        while candidate.path != "/" {
            candidate.deleteLastPathComponent()
            if candidate.pathExtension == "app",
               info(at: candidate)?["CFBundleIdentifier"] as? String == "com.richos.app" {
                return candidate
            }
        }
        return nil
    }

    private static func info(at bundle: URL) -> [String: Any]? {
        // Bundle caches its Info.plist. This read must see the bundle currently on disk.
        guard let data = try? Data(contentsOf: bundle.appendingPathComponent("Contents/Info.plist")),
              let plist = try? PropertyListSerialization.propertyList(from: data, format: nil)
        else { return nil }
        return plist as? [String: Any]
    }

    @objc private func updateArrived(_ notification: Notification) {
        guard !releasing, let runningVersion, let path = notification.object as? String,
              let version = notification.userInfo?["version"] as? String, version != runningVersion,
              let owner = notification.userInfo?["pid"] as? NSNumber,
              let richOS = Self.richOSBundle(containing: bundle), richOS.path == path,
              let sender = NSRunningApplication(processIdentifier: owner.int32Value),
              sender.bundleIdentifier == "com.richos.app",
              sender.bundleURL?.standardizedFileURL == richOS
        else { return }
        retry?.invalidate()
        attempt(version: version, owner: owner.int32Value, id: UUID(), since: ProcessInfo.processInfo.systemUptime)
    }

    private func attempt(version: String, owner: pid_t, id: UUID, since: TimeInterval) {
        guard Self.info(at: bundle)?[Self.versionKey] as? String == version else { return }
        guard !Self.hasBlockingWindow, let snapshot = prepare() else {
            record(operation, .interrupted, id: id, since: since)
            schedule(version: version, owner: owner, id: id, since: since)
            return
        }
        do {
            let token = UUID()
            try Self.write(snapshot, token: token)
            do {
                let next = Process()
                guard let executable = Self.info(at: bundle)?["CFBundleExecutable"] as? String else {
                    throw CocoaError(.fileNoSuchFile)
                }
                next.executableURL = bundle.appendingPathComponent("Contents/MacOS/\(executable)")
                next.currentDirectoryURL = URL(fileURLWithPath: "/")
                next.arguments = ["--suite-reopen", String(getpid()), version, token.uuidString,
                                  NSApp.isActive ? "active" : (NSApp.isHidden ? "hidden" : "background")]
                    + relaunchArguments(owner)
                try next.run()
            } catch {
                try? FileManager.default.removeItem(at: Self.snapshotURL(token))
                throw error
            }
            releasing = true
            record(operation, .complete, id: id, since: since)
            RichFilesDiagnostics.flush()
            NSApp.terminate(nil)
            // AppKit returns if termination was declined. The reopen helper times out.
            releasing = false
        } catch {
            record(operation, .issues, id: id, since: since)
        }
        schedule(version: version, owner: owner, id: id, since: since)
    }

    private func schedule(version: String, owner: pid_t, id: UUID, since: TimeInterval) {
        let timer = Timer(timeInterval: Self.retryInterval, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.attempt(version: version, owner: owner, id: id, since: since) }
        }
        RunLoop.main.add(timer, forMode: .common)
        retry = timer
    }

    private func record(_ operation: RichFilesDiagnostics.Operation, _ outcome: RichFilesDiagnostics.Outcome,
                        id: UUID, since: TimeInterval) {
        RichFilesDiagnostics.record(operation, id: id, outcome: outcome, started: since,
                                    total: 1, confirmed: outcome == .complete ? 1 : 0)
    }

    public static func runCommand(_ arguments: [String]) -> Bool {
        switch arguments.first {
        case "--suite-update-arrived":
            guard arguments.count == 3, let pid = Int32(arguments[1]),
                  let richOS = richOSBundle(containing: Bundle.main.bundleURL.standardizedFileURL),
                  info(at: richOS)?["CFBundleShortVersionString"] as? String == arguments[2]
            else { return true }
            DistributedNotificationCenter.default().postNotificationName(
                notice, object: richOS.path, userInfo: ["version": arguments[2], "pid": NSNumber(value: pid)],
                deliverImmediately: true
            )
        case "--suite-reopen":
            guard arguments.count >= 5, let pid = Int32(arguments[1]), pid > 1,
                  getppid() == pid || getppid() == 1,
                  let token = UUID(uuidString: arguments[3]) else { return true }
            // launchd may already have adopted the helper if the old app quit
            // before the helper's entry point ran. PID reuse cannot affect getppid.
            let deadline = ProcessInfo.processInfo.systemUptime + 15
            while getppid() == pid, ProcessInfo.processInfo.systemUptime < deadline {
                Thread.sleep(forTimeInterval: 0.05)
            }
            guard getppid() != pid, info(at: Bundle.main.bundleURL)?[versionKey] as? String == arguments[2]
            else {
                try? FileManager.default.removeItem(at: snapshotURL(token))
                return true
            }
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = arguments[4] == "active"
            configuration.hides = arguments[4] == "hidden"
            configuration.addsToRecentItems = false
            configuration.arguments = ["--suite-restore", token.uuidString, "-ApplePersistenceIgnoreState", "YES"]
                + Array(arguments.dropFirst(5))
            reopen(Bundle.main.bundleURL, version: arguments[2], oldPID: pid, configuration: configuration)
        default:
            return false
        }
        return true
    }

    private static func reopen(_ bundle: URL, version: String, oldPID: pid_t,
                               configuration: NSWorkspace.OpenConfiguration) {
        // An atomic bundle exchange can leave LaunchServices resolving the old
        // registration briefly after the app exits. Refresh the exact new URL and
        // retry a bounded number of times without changing foreground ownership.
        for delay in [0.0, 0.25, 0.75, 1.5, 3.0] {
            if delay > 0 { Thread.sleep(forTimeInterval: delay) }
            guard info(at: bundle)?[versionKey] as? String == version else { return }
            let registered = LSRegisterURL(bundle as CFURL, true)
            let completed = DispatchSemaphore(value: 0)
            let launched = OSAllocatedUnfairLock(initialState: false)
            NSWorkspace.shared.openApplication(at: bundle, configuration: configuration) { app, error in
                if let app, !app.isTerminated, app.processIdentifier != oldPID {
                    launched.withLock { $0 = true }
                } else if let error = error as NSError? {
                    NSLog("Suite restart: LaunchServices %@ (%ld), registration %d",
                          error.domain, error.code, registered)
                }
                completed.signal()
            }
            if completed.wait(timeout: .now() + 3) == .success, launched.withLock({ $0 }) { return }
        }
        NSLog("Suite restart: LaunchServices did not reopen the application")
    }

    private static func snapshotURL(_ token: UUID) -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("richos-suite-\(token.uuidString)")
    }

    private static func write(_ data: Data, token: UUID) throws {
        guard data.count <= maximumSnapshotBytes else { throw CocoaError(.fileWriteOutOfSpace) }
        let fd = open(snapshotURL(token).path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
        guard fd >= 0 else { throw CocoaError(.fileWriteNoPermission) }
        let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        do { try file.write(contentsOf: data); try file.close() }
        catch {
            try? FileManager.default.removeItem(at: snapshotURL(token))
            throw error
        }
    }

    public static func restore<State: Decodable>(_ type: State.Type) -> State? {
        guard let flag = CommandLine.arguments.firstIndex(of: "--suite-restore"),
              CommandLine.arguments.indices.contains(flag + 1),
              let token = UUID(uuidString: CommandLine.arguments[flag + 1]) else { return nil }
        let url = snapshotURL(token)
        let fd = open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard fd >= 0 else { return nil }
        let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        var status = stat()
        guard fstat(fd, &status) == 0, status.st_uid == getuid(), status.st_mode & S_IFMT == S_IFREG,
              status.st_mode & 0o077 == 0, status.st_size <= maximumSnapshotBytes
        else { return nil }
        defer { try? FileManager.default.removeItem(at: url) }
        guard let data = try? file.read(upToCount: maximumSnapshotBytes + 1) else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }
}

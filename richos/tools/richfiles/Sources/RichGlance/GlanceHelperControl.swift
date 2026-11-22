import AppKit
import OSLog
import RichFilesCore
import ServiceManagement

enum GlanceHelperControl {
    typealias Shortcut = GlanceHelper.Shortcut

    struct Status: Encodable, Sendable {
        let available: Bool
        let enabled: Bool
        let registered: Bool
        let running: Bool
        let shortcut: Shortcut
        let againShortcut: Shortcut
        let textShortcut: Shortcut
        let background: Bool
        let menuBar: Bool
        let service: String
    }

    private struct ShortcutChange: Decodable {
        let enabled: Bool
        let shortcut: Shortcut
    }

    private static let store = GlanceHelper.Store()
    private static let logger = Logger(subsystem: GlanceHelper.bundleID, category: "settings")
    private static var service: SMAppService { .loginItem(identifier: GlanceHelper.bundleID) }
    private static var bundle: URL { Bundle.main.bundleURL.appendingPathComponent(GlanceHelper.bundlePath, isDirectory: true) }
    private static var installed: Bool { Bundle(url: bundle)?.executableURL != nil }
    private static var richOS: NSRunningApplication? {
        NSRunningApplication.runningApplications(withBundleIdentifier: GlanceHelper.richOSID).first { !$0.isTerminated }
    }

    static func runCommand(_ arguments: [String]) -> Bool {
        guard arguments.first == GlanceHelper.command else { return false }
        do {
            let result: Status
            switch Array(arguments.dropFirst()) {
            case ["status"]:
                result = try status()
            case ["restore"]:
                if installed {
                    let lock = try store.lock("settings")
                    defer { try? lock?.close() }
                    let preferences = try store.read().preferences
                    let loginEnabled = service.status == .enabled
                    if shouldRunSession(preferences, loginEnabled: loginEnabled) {
                        _ = try startSessionIfNeeded()
                    } else if preferences.background && loginEnabled {
                        if try !waitForHelper(), shouldRunSession(preferences, loginEnabled: service.status == .enabled) {
                            _ = try startSessionIfNeeded()
                        }
                    }
                }
                result = try status()
            case let values where values.count == 2 && values[0] == "configure":
                let change = try JSONDecoder().decode(ShortcutChange.self, from: Data(values[1].utf8))
                result = try setShortcut(change.shortcut, enabled: change.enabled)
            default:
                throw GlanceHelper.Problem("Unknown RichGlance Capture command.")
            }
            let data = try JSONEncoder().encode(result)
            FileHandle.standardOutput.write(data)
        } catch {
            FileHandle.standardError.write(Data(error.localizedDescription.utf8))
            exit(EXIT_FAILURE)
        }
        return true
    }

    static func shouldRunSession(_ preferences: GlanceHelper.Preferences, loginEnabled: Bool) -> Bool {
        preferences.enabled && (!preferences.background || !loginEnabled)
    }

    static func status() throws -> Status {
        let preference = try store.read().preferences
        let endpoint = installed ? (try? store.send(.init(.status))) : nil
        let name: String
        switch service.status {
        case .enabled: name = preference.background ? "on" : "off"
        case .requiresApproval: name = preference.background ? "approval" : "off"
        case .notRegistered, .notFound: name = preference.background ? "unavailable" : "off"
        @unknown default: name = "unavailable"
        }
        return Status(available: installed, enabled: preference.enabled, registered: endpoint?.registered == true,
                      running: endpoint != nil, shortcut: preference.shortcut,
                      againShortcut: preference.againShortcut, textShortcut: preference.textShortcut, background: preference.background, menuBar: preference.menuBar,
                      service: name)
    }

    static func setShortcut(_ shortcut: Shortcut, enabled: Bool, for action: CaptureRequest.Action = .capture) throws -> Status {
        _ = try shortcut.carbon()
        return try update {
            $0[action] = shortcut
            $0.enabled = enabled
        }
    }

    static func setMenuBar(_ enabled: Bool) throws -> Status {
        try update { $0.menuBar = enabled }
    }

    private static func update(_ change: (inout GlanceHelper.Preferences) -> Void) throws -> Status {
        guard installed else { throw GlanceHelper.Problem("The RichGlance capture helper could not be found in this installation.") }
        let lock = try store.lock("settings")
        defer { try? lock?.close() }
        let original = try store.read().preferences
        var next = original
        change(&next)
        let actions: [CaptureRequest.Action] = [.capture, .captureAgain, .copyText]
        for (index, action) in actions.enumerated() {
            guard !actions[..<index].contains(where: { next[$0] == next[action] }) else {
                throw GlanceHelper.Problem("That shortcut already belongs to another RichGlance action.")
            }
        }
        if shouldRunSession(next, loginEnabled: service.status == .enabled) { _ = try startSessionIfNeeded() }
        let command = GlanceHelper.Request(.configure, preferences: next)
        if try store.send(command) == nil { try store.write(next) }
        return try status()
    }

    static func setBackground(_ enabled: Bool) throws -> Status {
        guard installed else { throw GlanceHelper.Problem("The RichGlance capture helper could not be found in this installation.") }
        let lock = try store.lock("settings")
        defer { try? lock?.close() }
        let record = try store.read()
        let original = record.preferences
        let item = service
        let started = ProcessInfo.processInfo.systemUptime
        var outcome = RichFilesDiagnostics.Outcome.issues
        defer {
            RichFilesDiagnostics.record(.glanceCaptureHelperLogin, id: UUID(), outcome: outcome,
                                        started: started, total: 1, confirmed: outcome == .complete ? 1 : 0)
        }
        if enabled, item.status == .enabled || item.status == .requiresApproval {
            if !original.background { try store.write(original.settingBackground(true, firstUse: record.firstUse)) }
            outcome = item.status == .enabled ? .complete : .unavailable
            return try status()
        }

        if enabled {
            try stopSession()
            let next = original.settingBackground(true, firstUse: record.firstUse)
            try store.write(next)
            do {
                try item.register()
            } catch {
                if item.status != .requiresApproval {
                    try store.restore(record)
                    if shouldRunSession(original, loginEnabled: item.status == .enabled) { _ = try? startSessionIfNeeded() }
                    logger.error("login-item action=enable outcome=failed")
                    throw GlanceHelper.Problem("macOS could not enable RichGlance Capture. Check Login Items in System Settings.")
                }
            }
            if item.status == .enabled { _ = try waitForHelper() }
            else if shouldRunSession(next, loginEnabled: false) { _ = try startSessionIfNeeded() }
        } else {
            try stopSession()
            try store.write(original.settingBackground(false, firstUse: false))
            if item.status == .enabled || item.status == .requiresApproval {
                do { try item.unregister() }
                catch {
                    try store.restore(record)
                    logger.error("login-item action=disable outcome=failed")
                    throw GlanceHelper.Problem("macOS could not turn off RichGlance Capture. Check Login Items in System Settings.")
                }
                _ = try waitForStop()
            }
            if original.enabled { _ = try startSessionIfNeeded() }
        }
        outcome = enabled && item.status != .enabled ? .unavailable : .complete
        logger.info("login-item enabled=\(enabled) status=\(item.status.rawValue)")
        return try status()
    }

    private static func stopSession() throws {
        guard try store.send(.init(.status))?.session == true else { return }
        _ = try store.send(.init(.stopSession))
        guard try waitForStop() else { throw GlanceHelper.Problem("RichGlance Capture is still closing. Try again.") }
    }

    private static func waitForStop() throws -> Bool {
        let end = ProcessInfo.processInfo.systemUptime + 2
        repeat {
            if let owner = try store.lock("owner", nonblocking: true) {
                try owner.close()
                return true
            }
            Thread.sleep(forTimeInterval: 0.05)
        } while ProcessInfo.processInfo.systemUptime < end
        return false
    }

    private static func startSessionIfNeeded() throws -> Bool {
        guard let richOS, !richOS.isTerminated else { return false }
        if try store.send(.init(.status)) != nil { return true }
        let command = Process()
        command.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        command.arguments = ["-gj", "-a", bundle.path, "--args", "--richos-session", String(richOS.processIdentifier)]
        command.standardOutput = FileHandle.nullDevice
        command.standardError = FileHandle.nullDevice
        try command.run()
        command.waitUntilExit()
        guard command.terminationStatus == 0, try waitForHelper() else {
            throw GlanceHelper.Problem("RichGlance Capture could not start.")
        }
        return true
    }

    private static func waitForHelper() throws -> Bool {
        let end = ProcessInfo.processInfo.systemUptime + 3
        repeat {
            if try store.send(.init(.status)) != nil { return true }
            Thread.sleep(forTimeInterval: 0.05)
        } while ProcessInfo.processInfo.systemUptime < end
        return false
    }
}

import AppKit
import Carbon
import OSLog
import RichFilesCore

@MainActor
final class CaptureHelper: NSObject, NSApplicationDelegate {
    private static let keyboardClass: OSType = 0x6B657962  // keyb
    private static let hotkeyType: OSType = 0x686B6964     // hkid
    private static let signature: OSType = 0x52474370      // RGCp
    private let logger = Logger(subsystem: GlanceHelper.bundleID, category: "capture")
    private let store = GlanceHelper.Store()
    private let session: NSRunningApplication?
    private let glance: URL
    private var preferences: GlanceHelper.Preferences
    private var lease: FileHandle?
    private var port: CFMessagePort?
    private var suiteRestart: SuiteRestart?
    private struct Registration {
        let ref: EventHotKeyRef
        let id: UInt32
    }
    private static let captureActions: [CaptureRequest.Action] = [.capture, .captureAgain, .copyText]
    private var shortcuts: [CaptureRequest.Action: Registration] = [:]
    private var hotkey: EventHotKeyRef? { shortcuts[.capture]?.ref }
    private var handler: EventHandlerRef?
    private var nextID: UInt32 = 0
    private var lastPress: [CaptureRequest.Action: TimeInterval] = [:]
    private var item: NSStatusItem?
    private var previousApplication: NSRunningApplication?

    init?(session: NSRunningApplication?, login: Bool) {
        guard Bundle.main.bundleIdentifier == GlanceHelper.bundleID,
              let record = try? store.read(),
              login ? record.preferences.background : session?.isTerminated == false else { return nil }
        self.session = session
        preferences = record.preferences
        glance = (0..<4).reduce(Bundle.main.bundleURL) { url, _ in url.deletingLastPathComponent() }
        guard Bundle(url: glance)?.bundleIdentifier == "app.richos.richglance" else { return nil }
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let started = ProcessInfo.processInfo.systemUptime
        var outcome = RichFilesDiagnostics.Outcome.issues
        defer {
            RichFilesDiagnostics.record(.glanceCaptureHelperStart, id: UUID(), outcome: outcome, started: started,
                                        total: preferences.enabled ? 1 : 0, confirmed: hotkey != nil ? 1 : 0)
            RichFilesDiagnostics.flush()
        }
        do {
            lease = try claimOwner()
            guard lease != nil else {
                logger.info("start outcome=another-owner")
                NSApp.terminate(nil)
                return
            }
            guard session?.isTerminated != true else { NSApp.terminate(nil); return }
            observeWorkspace()
            try listen()
            do { try replaceShortcut(with: preferences, saving: false) }
            catch { logger.notice("start shortcut=unavailable") }
            updateMenu()
            let isSession = session != nil
            suiteRestart = SuiteRestart(operation: .captureHelperSuiteRestart, prepare: {
                try? JSONEncoder().encode(true)
            }, relaunchArguments: { owner in
                isSession ? ["--richos-session", String(owner)] : []
            })
            outcome = preferences.enabled && hotkey == nil ? .unavailable : .complete
            logger.info("start outcome=ready session=\(self.session != nil) shortcut=\(self.hotkey != nil) menu=\(self.item != nil)")
        } catch {
            logger.error("start outcome=unavailable")
            NSApp.terminate(nil)
        }
    }

    private func claimOwner() throws -> FileHandle? {
        if let owner = try store.lock("owner", nonblocking: true) { return owner }
        // Re-enabling the login item in System Settings can start it while the
        // temporary RichOS session still owns the shortcut.
        guard session == nil, try store.send(.init(.status))?.session == true else { return nil }
        _ = try store.send(.init(.stopSession))
        let end = ProcessInfo.processInfo.systemUptime + 2
        repeat {
            if let owner = try store.lock("owner", nonblocking: true) { return owner }
            Thread.sleep(forTimeInterval: 0.05)
        } while ProcessInfo.processInfo.systemUptime < end
        return nil
    }

    private func observeWorkspace() {
        let workspace = NSWorkspace.shared
        previousApplication = workspace.frontmostApplication
        DistributedNotificationCenter.default().addObserver(self, selector: #selector(pinsChanged),
                                                            name: GlancePins.changed, object: nil)
        workspace.notificationCenter.addObserver(self, selector: #selector(activated(_:)),
                                                  name: NSWorkspace.didActivateApplicationNotification, object: nil)
        workspace.notificationCenter.addObserver(self, selector: #selector(terminated(_:)),
                                                  name: NSWorkspace.didTerminateApplicationNotification, object: nil)
    }

    @objc private func pinsChanged() { updateMenu() }

    @objc private func activated(_ notification: Notification) {
        guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app.activationPolicy == .regular else { return }
        previousApplication = app
    }

    @objc private func terminated(_ notification: Notification) {
        guard let session,
              let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app.processIdentifier == session.processIdentifier else { return }
        NSApp.terminate(nil)
    }

    func applicationWillTerminate(_ notification: Notification) {
        for registration in shortcuts.values { UnregisterEventHotKey(registration.ref) }
        if let handler { RemoveEventHandler(handler) }
        if let port { CFMessagePortInvalidate(port) }
        if lease != nil { try? FileManager.default.removeItem(at: store.endpoint) }
        try? lease?.close()
    }

    private func listen() throws {
        let name = UUID().uuidString
        var context = CFMessagePortContext(version: 0, info: Unmanaged.passUnretained(self).toOpaque(),
                                           retain: nil, release: nil, copyDescription: nil)
        guard let port = CFMessagePortCreateLocal(nil, name as CFString, { _, _, data, context in
            guard let data, let context else { return nil }
            let helper = Unmanaged<CaptureHelper>.fromOpaque(context).takeUnretainedValue()
            return MainActor.assumeIsolated {
                helper.reply(to: data as Data).map { Unmanaged.passRetained($0 as CFData) }
            }
        }, &context, nil),
        let source = CFMessagePortCreateRunLoopSource(nil, port, 0) else {
            throw GlanceHelper.Problem("RichGlance Capture could not start listening.")
        }
        self.port = port
        CFRunLoopAddSource(CFRunLoopGetMain(), source, CFRunLoopMode.commonModes)
        try name.write(to: store.endpoint, atomically: true, encoding: .utf8)
    }

    private func reply(to data: Data) -> Data? {
        guard data.count < 2048, let request = try? JSONDecoder().decode(GlanceHelper.Request.self, from: data) else { return nil }
        var response = GlanceHelper.Response(registered: hotkey != nil, session: session != nil)
        do {
            switch request.action {
            case .status:
                break
            case .configure:
                guard let next = request.preferences, next.background == preferences.background else {
                    throw GlanceHelper.Problem("RichGlance Capture is restarting. Try again.")
                }
                try replaceShortcut(with: next, saving: true, requiring: request.requiredShortcut)
                updateMenu()
                response.registered = hotkey != nil
            case .stopSession:
                guard session != nil else { throw GlanceHelper.Problem("RichGlance Capture is running in the background.") }
                DispatchQueue.main.async { NSApp.terminate(nil) }
            }
        } catch {
            response.error = error.localizedDescription
        }
        response.againRegistered = shortcuts[.captureAgain] != nil
        response.textRegistered = shortcuts[.copyText] != nil
        return try? JSONEncoder().encode(response)
    }

    private func replaceShortcut(with next: GlanceHelper.Preferences, saving: Bool,
                                 requiring required: CaptureRequest.Action? = nil) throws {
        var replacement: [CaptureRequest.Action: Registration] = [:]
        var added: [Registration] = []
        do {
            if next.enabled {
                for action in Self.captureActions {
                    if let existing = shortcuts[action], next[action] == preferences[action] {
                        replacement[action] = existing
                    } else {
                        do {
                            let registered = try register(next[action])
                            added.append(registered)
                            replacement[action] = registered
                        } catch {
                            if action == required {
                                let labels: [CaptureRequest.Action: String] = [.capture: "Capture Region", .captureAgain: "Capture Again", .copyText: "Copy Text"]
                                throw GlanceHelper.Problem("\(labels[action] ?? "Capture") shortcut unavailable. Choose another combination.")
                            }
                        }
                    }
                }
            }
            if saving { try store.write(next) }
        } catch {
            for registration in added { UnregisterEventHotKey(registration.ref) }
            throw error
        }
        for (action, registration) in shortcuts where replacement[action]?.id != registration.id {
            UnregisterEventHotKey(registration.ref)
        }
        shortcuts = replacement
        lastPress.removeAll()
        preferences = next
    }

    private func register(_ shortcut: GlanceHelper.Shortcut) throws -> Registration {
        let chord = try shortcut.carbon()
        let target = GetApplicationEventTarget()
        if handler == nil {
            var kind = EventTypeSpec(eventClass: Self.keyboardClass, eventKind: UInt32(kEventHotKeyPressed))
            let result = InstallEventHandler(target, { _, event, context in
                guard let event, let context else { return OSStatus(eventNotHandledErr) }
                let helper = Unmanaged<CaptureHelper>.fromOpaque(context).takeUnretainedValue()
                return MainActor.assumeIsolated { helper.pressed(event) }
            }, 1, &kind, Unmanaged.passUnretained(self).toOpaque(), &handler)
            guard result == noErr else { throw GlanceHelper.Problem("macOS could not start listening for the shortcut.") }
        }
        nextID &+= 1
        let id = EventHotKeyID(signature: Self.signature, id: nextID)
        var replacement: EventHotKeyRef?
        guard RegisterEventHotKey(chord.key, chord.modifiers, id, target, UInt32(kEventHotKeyExclusive), &replacement) == noErr,
              let replacement else {
            logger.notice("shortcut outcome=unavailable")
            throw GlanceHelper.Problem("That shortcut is unavailable. Choose another combination.")
        }
        return Registration(ref: replacement, id: nextID)
    }

    private func pressed(_ event: EventRef) -> OSStatus {
        var id = EventHotKeyID()
        guard GetEventParameter(event, UInt32(kEventParamDirectObject), Self.hotkeyType, nil,
                                MemoryLayout<EventHotKeyID>.size, nil, &id) == noErr,
              id.signature == Self.signature, let action = shortcuts.first(where: { $0.value.id == id.id })?.key else { return OSStatus(eventNotHandledErr) }
        let now = ProcessInfo.processInfo.systemUptime
        if now - (lastPress[action] ?? -.infinity) >= 0.25 {
            lastPress[action] = now
            launch(action, source: "shortcut")
        }
        return noErr
    }

    private func updateMenu() {
        guard session == nil, preferences.menuBar else {
            if let item { NSStatusBar.system.removeStatusItem(item) }
            item = nil
            return
        }
        if item == nil { item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength) }
        item?.button?.image = NSImage(systemSymbolName: "viewfinder", accessibilityDescription: "RichGlance Capture")
        item?.button?.setAccessibilityLabel("RichGlance Capture")
        let pins = GlancePins.active()
        item?.length = pins.isEmpty ? NSStatusItem.squareLength : NSStatusItem.variableLength
        item?.button?.title = pins.isEmpty ? "" : " \(pins.count)"
        item?.button?.toolTip = pins.isEmpty ? "RichGlance Capture" : "RichGlance Capture, \(pins.count) pinned"
        let menu = NSMenu()
        let capture = menu.addItem(withTitle: "Capture…", action: #selector(captureFromMenu), keyEquivalent: "")
        capture.target = self
        if hotkey != nil { capture.title = "Capture…    \(preferences.shortcut.label)" }
        let again = menu.addItem(withTitle: "Capture Again", action: #selector(againFromMenu), keyEquivalent: "")
        again.target = self
        if shortcuts[.captureAgain] != nil { again.title += "    \(preferences.againShortcut.label)" }
        let text = menu.addItem(withTitle: "Copy Text", action: #selector(copyTextFromMenu), keyEquivalent: "")
        text.target = self
        if shortcuts[.copyText] != nil { text.title += "    \(preferences.textShortcut.label)" }
        menu.addItem(withTitle: "Open Editor", action: #selector(openEditor), keyEquivalent: "").target = self
        if !pins.isEmpty {
            menu.addItem(.separator())
            for (index, id) in pins.enumerated() {
                let close = menu.addItem(withTitle: "Close Pin \(index + 1)", action: #selector(closePin(_:)), keyEquivalent: "")
                close.representedObject = id
                close.target = self
            }
        }
        menu.addItem(.separator())
        menu.addItem(withTitle: "Shortcuts…", action: #selector(openSettings), keyEquivalent: "").target = self
        item?.menu = menu
    }

    @objc private func captureFromMenu() { launch(.capture, source: "menu") }
    @objc private func againFromMenu() { launch(.captureAgain, source: "menu") }
    @objc private func copyTextFromMenu() { launch(.copyText, source: "menu") }
    @objc private func openEditor() { launch(.editor, source: "menu") }
    @objc private func openSettings() { launch(.settings, source: "menu") }

    @objc private func closePin(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? UUID else { return }
        launch(.closePin, source: "menu", pinID: id)
    }

    private func launch(_ action: CaptureRequest.Action, source: String, pinID: UUID? = nil) {
        let start = ProcessInfo.processInfo.systemUptime
        let id = UUID()
        let operation: RichFilesDiagnostics.Operation = source == "shortcut" ? .glanceCaptureShortcutLaunch : .glanceCaptureMenuLaunch
        do {
            let front = NSWorkspace.shared.frontmostApplication
            let origin = front?.activationPolicy == .regular ? front : previousApplication
            let request = try CaptureRequest.issue(action, originPID: origin?.processIdentifier, pinID: pinID)
            let configuration = NSWorkspace.OpenConfiguration()
            // Capture keeps the source app active until the overlay is ready.
            configuration.activates = !action.startsCapture && action != .closePin
            configuration.arguments = ["--capture-request"]
            let logger = self.logger
            NSWorkspace.shared.open([request.url], withApplicationAt: glance, configuration: configuration) { app, error in
                let accepted = error == nil && app != nil
                if !accepted { try? FileManager.default.removeItem(at: request.ticket) }
                if action.startsCapture {
                    RichFilesDiagnostics.record(operation, id: id, outcome: accepted ? .complete : .unavailable,
                                                started: start, total: 1, confirmed: accepted ? 1 : 0)
                }
                let duration = Int((ProcessInfo.processInfo.systemUptime - start) * 1_000)
                logger.info("launch source=\(source, privacy: .public) action=\(action.rawValue, privacy: .public) accepted=\(error == nil && app != nil) milliseconds=\(duration)")
            }
        } catch {
            if action.startsCapture {
                RichFilesDiagnostics.record(operation, id: id, outcome: .issues, started: start, total: 1, confirmed: 0)
            }
            logger.error("launch source=\(source, privacy: .public) outcome=request-failed")
        }
    }
}

MainActor.assumeIsolated {
    let arguments = Array(CommandLine.arguments.dropFirst())
    if SuiteRestart.runCommand(arguments) { return }
    let restored = SuiteRestart.restore(Bool.self) == true
    let mode = restored ? Array(arguments.dropFirst(4)) : arguments
    let login = mode.isEmpty
    let session = mode.count == 2 && mode[0] == "--richos-session"
        ? pid_t(mode[1]).flatMap(NSRunningApplication.init(processIdentifier:)) : nil
    if !login && session?.bundleIdentifier != GlanceHelper.richOSID { exit(EXIT_SUCCESS) }
    if let helper = CaptureHelper(session: session, login: login) {
        let app = NSApplication.shared
        app.delegate = helper
        app.setActivationPolicy(.accessory)
        withExtendedLifetime(helper) { app.run() }
    }
}

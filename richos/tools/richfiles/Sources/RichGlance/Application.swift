import AppKit
import Darwin
import OSLog
import RichFilesCore
import UniformTypeIdentifiers

@main
@MainActor
final class Application: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private static let permissionLog = Logger(subsystem: "app.richos.richglance", category: "capture-permission")
    private let workspace = NSWorkspace.shared
    private var previousApplication: NSRunningApplication?
    private var returnTo: NSRunningApplication?
    private var viewer: Viewer?
    private let infoPreferences = GlanceInfoPreferences()
    private var infoMenuItem: NSMenuItem?
    private var keyMonitor: Any?
    private var openingPanel: NSOpenPanel?
    private var captureSettings: BackgroundCaptureSettings?
    private var capture: CaptureController?
    private var permissionProbe: Process?
    private var captureNotice: NSPanel?
    private var noticeCapture: UUID?
    private let screenshotStore = ScreenshotStore()
    private var lastRegionDocument: ScreenshotDocument?
    private var editor: ScreenshotEditor?
    private var captureLaunch = CommandLine.arguments.contains("--capture-request")
    private var launchOrigin = Application.launchOrigin()

    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())
        if GlanceHelperControl.runCommand(arguments) { return }
        if arguments == ["--check-screen-recording-access"] {
            exit(CGPreflightScreenCaptureAccess() ? EXIT_SUCCESS : EXIT_FAILURE)
        }
        if arguments.first == "--reopen-after-screen-recording" {
            reopenAfterExit(arguments)
            return
        }
        let delegate = Application()
        let app = NSApplication.shared
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        delegate.installMenu()
        withExtendedLifetime(delegate) { app.run() }
    }

    override init() {
        super.init()
        previousApplication = externalFrontmost
        workspace.notificationCenter.addObserver(
            self, selector: #selector(applicationActivated(_:)),
            name: NSWorkspace.didActivateApplicationNotification, object: nil
        )
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .mouseMoved]) { [weak self] event in
            let handled = MainActor.assumeIsolated {
                guard let viewer = self?.viewer, NSApp.keyWindow === viewer.window,
                      event.window == nil || event.window === viewer.window else { return false }
                if event.type == .mouseMoved {
                    viewer.info.trackPointer(at: viewer.window.mouseLocationOutsideOfEventStream)
                    return false
                }
                return viewer.handleKey(event)
            }
            return handled ? nil : event
        }
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls {
            if let request = CaptureRequest.consume(url) {
                switch request.action {
                case .capture: beginCapture(originPID: request.originPID)
                case .captureAgain: beginCapture(originPID: request.originPID, repeatsRegion: true)
                case .copyText: beginCapture(originPID: request.originPID, copiesText: true)
                case .editor:
                    captureLaunch = false
                    openWithoutFile()
                case .settings:
                    captureLaunch = false
                    openCaptureSettings()
                }
                return
            }
        }
        if let file = urls.first(where: GlanceFiles.supports) ?? urls.first(where: \.isFileURL) {
            captureLaunch = false
            open(file)
        } else if captureLaunch, viewer == nil, capture == nil {
            NSApp.terminate(nil)
        }
    }

    func applicationOpenUntitledFile(_ sender: NSApplication) -> Bool {
        if viewer == nil, editor == nil, capture == nil, captureSettings?.window?.isVisible != true, !captureLaunch { openWithoutFile() }
        return true
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        if capture == nil { editor?.refreshShelf() }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        if screenshotStore.keepsHistory {
            let store = screenshotStore
            DispatchQueue.global(qos: .utility).async { _ = try? store.entries() }
        }
        let arguments = CommandLine.arguments
        guard captureLaunch, arguments.count >= 4, arguments[2] == "--retry-screen-recording",
              let originPID = pid_t(arguments[3]), capture == nil else { return }
        if arguments.count == 5 {
            captureLaunch = false
            open(URL(fileURLWithPath: arguments[4]), activate: originPID == 0)
        }
        beginCapture(originPID: originPID > 0 ? originPID : nil)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if let capture {
            capture.revisitPermission()
            return false
        }
        guard !captureLaunch else { return false }
        if let window = captureSettings?.window, window.isVisible {
            window.makeKeyAndOrderFront(nil)
            return true
        }
        if viewer == nil, editor == nil { openWithoutFile() }
        else if !hasVisibleWindows { if let editor { editor.show() } else { viewer?.show() } }
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if let editor, !editor.allowApplicationQuit() { return .terminateCancel }
        return .terminateNow
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        capture == nil && editor == nil
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        // Hand back foreground ownership before the last window disappears.
        if NSApp.isActive, let origin = returnTo, !origin.isTerminated {
            if #available(macOS 14, *) {
                _ = origin.activate(from: .current, options: [])
            } else {
                _ = origin.activate(options: [])
            }
        }
        return true
    }

    func windowWillClose(_ notification: Notification) {
        viewer?.stop()
        viewer = nil
        returnTo = nil
    }

    private var externalFrontmost: NSRunningApplication? {
        guard let front = workspace.frontmostApplication, front != .current,
              front.activationPolicy == .regular else { return nil }
        return front
    }

    @objc private func applicationActivated(_ note: Notification) {
        guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app != .current, app.activationPolicy == .regular else { return }
        previousApplication = app
    }

    private func open(_ file: URL, activate: Bool = true) {
        let finder = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.finder").first
        if let origin = [launchOrigin, externalFrontmost, previousApplication, finder]
            .compactMap({ $0 }).first(where: { !$0.isTerminated }) {
            returnTo = origin
        }
        launchOrigin = nil
        let viewer = self.viewer ?? Viewer(preferences: infoPreferences)
        self.viewer = viewer
        viewer.info.preferencesChanged = { [weak self] in self?.updateInfoShortcut() }
        openingPanel?.cancel(nil)
        viewer.window.delegate = self
        viewer.open(file.standardizedFileURL)
        viewer.show()
        if activate {
            if #available(macOS 14, *) { NSApp.activate() }
            else { NSApp.activate(ignoringOtherApps: true) }
        }
    }

    private static func launchOrigin() -> NSRunningApplication? {
        let arguments = CommandLine.arguments
        guard let flag = arguments.firstIndex(of: "--return-to"), arguments.indices.contains(flag + 1),
              let pid = pid_t(arguments[flag + 1]), let origin = NSRunningApplication(processIdentifier: pid),
              ["app.richos.richfiles", "com.richos.app"].contains(origin.bundleIdentifier ?? "") else { return nil }
        return origin
    }

    private func openWithoutFile() {
        if (try? screenshotStore.entries().isEmpty) == false { editLastCapture() }
        else { showEditor(nil, origin: externalFrontmost ?? previousApplication, previousWindow: nil) }
    }

    @objc private func chooseImage() {
        guard openingPanel == nil else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = GlanceFiles.contentTypes
        panel.allowsMultipleSelection = false
        openingPanel = panel
        panel.begin { [weak self, weak panel] result in
            guard let self else { return }
            openingPanel = nil
            if result == .OK, let file = panel?.url { open(file) }
            else if viewer == nil, editor == nil, capture == nil { NSApp.terminate(nil) }
        }
    }

    @objc private func startCapture() { beginCapture(originPID: nil) }
    @objc private func captureAgain() { beginCapture(originPID: nil, repeatsRegion: true) }
    @objc private func copyRegionText() { beginCapture(originPID: nil, copiesText: true) }

    @objc private func openCaptureSettings() {
        if captureSettings == nil { captureSettings = BackgroundCaptureSettings() }
        captureSettings?.showWindow(nil)
        if #available(macOS 14, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
    }

    private func beginCapture(originPID: pid_t?, fromEditor: Bool = false, repeatsRegion: Bool = false, copiesText: Bool = false) {
        if let capture {
            capture.revisitPermission()
            return
        }
        captureNotice?.orderOut(nil)
        captureNotice = nil
        noticeCapture = nil
        let shortcutOrigin = originPID.flatMap(NSRunningApplication.init(processIdentifier:))
        let reliableOrigin = shortcutOrigin.flatMap { app in
            app != .current && !app.isTerminated && app.activationPolicy == .regular ? app : nil
        }
        let origin = reliableOrigin ?? externalFrontmost
        let keyWindow = origin == nil && NSApp.isActive ? NSApp.keyWindow : nil
        let baseline = repeatsRegion ? lastRegionBaseline() : nil
        let session = CaptureController(previousWindow: keyWindow,
                                        previousApp: origin ?? previousApplication, store: screenshotStore,
                                        preferredAction: fromEditor ? .editor : nil, repeatsRegion: repeatsRegion, copiesText: copiesText, openEditor: { [weak self] document in
            if document.repeatTarget != nil { self?.lastRegionDocument = document }
            self?.showEditor(document, origin: origin ?? self?.previousApplication, previousWindow: keyWindow,
                             newCapture: true, baseline: baseline)
        },
                                        refreshPermission: { [weak self] session, originPID in
            self?.refreshCapturePermission(for: session, originPID: originPID)
        }) { [weak self] message, display, retained in
            guard let self else { return }
            capture = nil
            editor?.refreshShelf()
            if let message, let display { showCaptureNotice(message, display: display, retained: retained) }
            else { endCaptureLaunch() }
        }
        capture = session
        openingPanel?.cancel(nil)
        session.start()
    }

    private func lastRegionBaseline() -> ScreenshotDocument? {
        guard let region = screenshotStore.lastRegion() else { return nil }
        if let current = editor?.savedDocument(region.id) { return current }
        if let lastRegionDocument, lastRegionDocument.id == region.id, !screenshotStore.keepsHistory { return lastRegionDocument }
        guard let entry = try? screenshotStore.entries().first(where: { $0.id == region.id }) else { return nil }
        return try? screenshotStore.load(entry)
    }

    private func refreshCapturePermission(for session: CaptureController, originPID: pid_t?) {
        guard permissionProbe == nil, let executable = Bundle.main.executableURL else { return }
        let probe = Process()
        probe.executableURL = executable
        probe.arguments = ["--check-screen-recording-access"]
        probe.terminationHandler = { [weak self, weak session] process in
            let granted = process.terminationReason == .exit && process.terminationStatus == EXIT_SUCCESS
            Task { @MainActor in
                guard let self else { return }
                self.permissionProbe = nil
                guard granted, let session, self.capture === session, session.canRevisitPermission else { return }
                self.reopenForCapture(session, originPID: originPID)
            }
        }
        do {
            try probe.run()
            permissionProbe = probe
        } catch {
            // The existing permission panel stays usable if the check cannot launch.
        }
    }

    private func reopenForCapture(_ session: CaptureController, originPID: pid_t?) {
        guard let executable = Bundle.main.executableURL else { return }
        let helper = Process()
        helper.executableURL = executable
        var arguments = ["--reopen-after-screen-recording", String(getpid()), String(originPID ?? 0)]
        if let file = viewer?.file { arguments.append(file.path) }
        helper.arguments = arguments
        do {
            try helper.run()
            session.cancel()
            NSApp.terminate(nil)
        } catch {
            // Leave the open image and permission panel available if the helper cannot launch.
        }
    }

    private static func reopenAfterExit(_ arguments: [String]) {
        guard arguments.count >= 3, let oldPID = pid_t(arguments[1]), oldPID > 0 else { return }
        // Launch Services allows only one RichGlance instance. Wait until this helper
        // has been reparented and the previous app is gone from Launch Services.
        permissionLog.info("Waiting for RichGlance to exit after the Screen Recording grant")
        Task { @MainActor in
            let deadline = ContinuousClock.now + .seconds(5)
            while getppid() == oldPID || NSRunningApplication(processIdentifier: oldPID)?.isTerminated == false {
                guard ContinuousClock.now < deadline else {
                    permissionLog.error("The previous RichGlance instance did not exit")
                    exit(EXIT_FAILURE)
                }
                try? await Task.sleep(for: .milliseconds(25))
            }
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = arguments[2] == "0" && arguments.count == 4
            configuration.addsToRecentItems = false
            configuration.arguments = ["--capture-request", "--retry-screen-recording"] + arguments.dropFirst(2)
            do {
                let app = try await NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration)
                guard app.processIdentifier != oldPID else {
                    permissionLog.error("Launch Services returned the previous RichGlance instance")
                    exit(EXIT_FAILURE)
                }
                permissionLog.info("Reopened RichGlance after the Screen Recording grant")
                exit(EXIT_SUCCESS)
            } catch {
                permissionLog.error("Could not reopen RichGlance after the Screen Recording grant")
                exit(EXIT_FAILURE)
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 10) { exit(EXIT_FAILURE) }
        dispatchMain()
    }

    private func showCaptureNotice(_ message: String, display: CGRect, retained: UUID?) {
        noticeCapture = retained
        let width: CGFloat = retained == nil ? 285 : 348
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let panel = NSPanel(contentRect: CGRect(x: display.midX - width / 2, y: display.maxY - 84, width: width, height: 38),
                            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.level = .statusBar
        panel.hasShadow = true
        panel.ignoresMouseEvents = retained == nil
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        let background = NSView(frame: CGRect(x: 0, y: 0, width: width, height: 38))
        background.wantsLayer = true
        background.layer?.backgroundColor = (dark ? NSColor(red: 0.094, green: 0.141, blue: 0.251, alpha: 1)
                                                   : NSColor(red: 0.992, green: 0.988, blue: 0.973, alpha: 1)).cgColor
        background.layer?.cornerRadius = 19
        let label = NSTextField(labelWithString: "✓  " + message)
        label.textColor = dark ? .white : .black
        label.font = .systemFont(ofSize: 16, weight: .semibold)
        label.alignment = .center
        label.frame = CGRect(x: 8, y: 9, width: width - (retained == nil ? 16 : 91), height: 20)
        background.addSubview(label)
        if retained != nil {
            let edit = NSButton(title: "Edit…", target: self, action: #selector(editFromNotice))
            edit.font = .systemFont(ofSize: 16, weight: .semibold)
            edit.bezelStyle = .rounded
            edit.frame = CGRect(x: width - 81, y: 4, width: 72, height: 30)
            background.addSubview(edit)
        }
        panel.contentView = background
        captureNotice = panel
        panel.orderFrontRegardless()
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak self, weak panel] in
            guard let self, let panel, captureNotice === panel else { return }
            panel.orderOut(nil)
            captureNotice = nil
            noticeCapture = nil
            endCaptureLaunch()
        }
    }

    private func endCaptureLaunch() {
        if captureLaunch, viewer == nil, editor == nil {
            RichFilesDiagnostics.flush()
            NSApp.terminate(nil)
        }
        captureLaunch = false
    }

    @objc private func editFromNotice() {
        guard let id = noticeCapture else { return }
        captureNotice?.orderOut(nil)
        captureNotice = nil
        noticeCapture = nil
        editRecent(id: id)
    }

    @objc private func editLastCapture() { editRecent(id: nil) }

    private func editRecent(id: UUID?) {
        do {
            let entries = try screenshotStore.entries()
            guard let entry = id == nil ? entries.first : entries.first(where: { $0.id == id }) else {
                showHistoryError("There is no recent capture to edit.")
                return
            }
            guard let document = try screenshotStore.load(entry) else {
                showHistoryError("This capture could not be opened.")
                editor?.refreshShelf()
                return
            }
            showEditor(document, origin: externalFrontmost ?? previousApplication, previousWindow: NSApp.keyWindow)
        } catch {
            showHistoryError("This capture could not be opened.")
        }
    }

    private func showHistoryError(_ message: String) {
        let alert = NSAlert()
        alert.messageText = message
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    private func showEditor(_ document: ScreenshotDocument?, origin: NSRunningApplication?, previousWindow: NSWindow?,
                            newCapture: Bool = false, baseline: ScreenshotDocument? = nil) {
        captureLaunch = false
        if let editor {
            if let document { editor.changeDocument(document, newCapture: newCapture, baseline: baseline) }
            editor.show()
            return
        }
        let editor = ScreenshotEditor(store: screenshotStore)
        self.editor = editor
        editor.capture = { [weak self] in self?.beginCapture(originPID: nil, fromEditor: true) }
        editor.captureAgain = { [weak self] in self?.captureAgain() }
        editor.closed = { [weak self, weak previousWindow] in
            guard let self else { return }
            self.editor = nil
            if let previousWindow, previousWindow.isVisible, !previousWindow.isMiniaturized {
                previousWindow.makeKeyAndOrderFront(nil)
            } else if let origin, !origin.isTerminated, NSApp.isActive {
                if #available(macOS 14, *) { _ = origin.activate(from: .current, options: []) }
                else { _ = origin.activate(options: []) }
            }
        }
        if let document { editor.changeDocument(document, newCapture: newCapture, baseline: baseline) }
        editor.show()
    }

    @objc private func showRecents() {
        if editor == nil, (try? screenshotStore.entries().isEmpty) == false { editLastCapture() }
        else { showEditor(nil, origin: externalFrontmost ?? previousApplication, previousWindow: NSApp.keyWindow) }
        editor?.showShelf()
    }

    private var keyEditor: ScreenshotEditor? {
        guard let editor, NSApp.keyWindow === editor.window else { return nil }
        return editor
    }
    @objc private func copyScreenshot() { keyEditor?.copyImage() }
    @objc private func saveScreenshot() { keyEditor?.saveImage() }

    @objc private func openInPreview() { viewer?.openInPreview() }
    @objc private func showInfo() { viewer?.info.toggleInfo() }
    @objc private func showInfoByDefault() { viewer?.info.toggleDefault() }
    @objc private func changeInfoShortcut() { viewer?.info.beginRecording() }

    private func updateInfoShortcut() {
        infoMenuItem?.keyEquivalent = infoPreferences.shortcut.key
        infoMenuItem?.keyEquivalentModifierMask = infoPreferences.shortcut.flags
    }

    static func makeEditMenu() -> NSMenu {
        let menu = NSMenu(title: "Edit")
        menu.addItem(withTitle: "Undo", action: NSSelectorFromString("undo:"), keyEquivalent: "z")
        let redo = menu.addItem(withTitle: "Redo", action: NSSelectorFromString("redo:"), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        menu.addItem(.separator())
        menu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        menu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        menu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        menu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        return menu
    }

    private func installMenu() {
        let menuBar = NSMenu()
        let app = NSMenuItem()
        app.submenu = NSMenu()
        app.submenu?.addItem(withTitle: "About RichGlance", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        app.submenu?.addItem(.separator())
        app.submenu?.addItem(withTitle: "Capture Settings…", action: #selector(openCaptureSettings), keyEquivalent: "").target = self
        app.submenu?.addItem(.separator())
        app.submenu?.addItem(withTitle: "Hide RichGlance", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.submenu?.addItem(withTitle: "Quit RichGlance", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        menuBar.addItem(app)

        let file = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        fileMenu.delegate = self
        file.submenu = fileMenu
        fileMenu.addItem(withTitle: "Open…", action: #selector(chooseImage), keyEquivalent: "o").target = self
        fileMenu.addItem(withTitle: "Open in Preview", action: #selector(openInPreview), keyEquivalent: "").target = self
        fileMenu.addItem(withTitle: "Capture…", action: #selector(startCapture), keyEquivalent: "").target = self
        fileMenu.addItem(withTitle: "Capture Again", action: #selector(captureAgain), keyEquivalent: "").target = self
        fileMenu.addItem(withTitle: "Copy Text from Screen…", action: #selector(copyRegionText), keyEquivalent: "").target = self
        let editLast = fileMenu.addItem(withTitle: "Edit Last Capture", action: #selector(editLastCapture), keyEquivalent: "e")
        editLast.keyEquivalentModifierMask = [.command, .shift]
        editLast.target = self
        fileMenu.addItem(withTitle: "Recent Captures", action: #selector(showRecents), keyEquivalent: "").target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Copy Image", action: #selector(copyScreenshot), keyEquivalent: "c").target = self
        fileMenu.addItem(withTitle: "Save PNG…", action: #selector(saveScreenshot), keyEquivalent: "s").target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        menuBar.addItem(file)

        let edit = NSMenuItem()
        edit.submenu = Self.makeEditMenu()
        menuBar.addItem(edit)

        let view = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        infoMenuItem = viewMenu.addItem(withTitle: "Show Info", action: #selector(showInfo), keyEquivalent: "")
        infoMenuItem?.target = self
        updateInfoShortcut()
        viewMenu.addItem(withTitle: "Show Info by Default", action: #selector(showInfoByDefault), keyEquivalent: "").target = self
        viewMenu.addItem(withTitle: "Change Info Shortcut…", action: #selector(changeInfoShortcut), keyEquivalent: "").target = self
        view.submenu = viewMenu
        menuBar.addItem(view)

        let window = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        window.submenu = windowMenu
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        menuBar.addItem(window)
        NSApp.windowsMenu = windowMenu
        NSApp.mainMenu = menuBar
    }
}

extension Application: NSMenuItemValidation {
    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if menuItem.action == #selector(startCapture) { return capture?.canRevisitPermission ?? true }
        if menuItem.action == #selector(editLastCapture) { return (try? screenshotStore.entries().isEmpty) == false }
        if menuItem.action == #selector(showRecents) { return capture == nil }
        if menuItem.action == #selector(copyScreenshot) || menuItem.action == #selector(saveScreenshot) {
            menuItem.isHidden = keyEditor?.hasDocument != true
            return keyEditor?.hasDocument == true && keyEditor?.canvas?.isTyping == false
        }
        if menuItem.action == #selector(openInPreview) { return viewer?.file != nil }
        if menuItem.action == #selector(showInfo) {
            menuItem.title = viewer?.info.explicitlyShown == true ? "Hide Info" : "Show Info"
            return viewer?.info.isHidden == false && openingPanel == nil
        }
        if menuItem.action == #selector(showInfoByDefault) {
            menuItem.state = infoPreferences.showByDefault ? .on : .off
            return viewer?.info.isHidden == false && openingPanel == nil
        }
        if menuItem.action == #selector(changeInfoShortcut) {
            return viewer?.info.isHidden == false && openingPanel == nil
        }
        return true
    }
}

extension Application: NSMenuDelegate {
    func menuNeedsUpdate(_ menu: NSMenu) {
        for item in menu.items where item.action == #selector(copyScreenshot) || item.action == #selector(saveScreenshot) {
            item.isHidden = keyEditor == nil
        }
    }
}

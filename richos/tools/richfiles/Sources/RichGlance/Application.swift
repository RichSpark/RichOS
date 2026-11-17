import AppKit
import RichFilesCore
import UniformTypeIdentifiers

@main
@MainActor
final class Application: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private let workspace = NSWorkspace.shared
    private var previousApplication: NSRunningApplication?
    private var returnTo: NSRunningApplication?
    private var viewer: Viewer?
    private let infoPreferences = GlanceInfoPreferences()
    private var infoMenuItem: NSMenuItem?
    private var keyMonitor: Any?
    private var openingPanel: NSOpenPanel?
    private var capture: CaptureController?
    private var captureNotice: NSPanel?
    private var captureLaunch = CommandLine.arguments.contains("--capture-request")
    private var launchOrigin = Application.launchOrigin()

    static func main() {
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
        if urls.first(where: CaptureRequest.consume) != nil {
            startCapture()
        } else if let file = urls.first(where: GlanceFiles.supports) ?? urls.first(where: \.isFileURL) {
            captureLaunch = false
            open(file)
        } else if captureLaunch, viewer == nil, capture == nil {
            NSApp.terminate(nil)
        }
    }

    func applicationOpenUntitledFile(_ sender: NSApplication) -> Bool {
        if viewer == nil, capture == nil, !captureLaunch { chooseImage() }
        return true
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        guard capture == nil, !captureLaunch else { return false }
        if viewer == nil { chooseImage() }
        else if !hasVisibleWindows { viewer?.show() }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { capture == nil }

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

    private func open(_ file: URL) {
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
        if #available(macOS 14, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
    }

    private static func launchOrigin() -> NSRunningApplication? {
        let arguments = CommandLine.arguments
        guard let flag = arguments.firstIndex(of: "--return-to"), arguments.indices.contains(flag + 1),
              let pid = pid_t(arguments[flag + 1]), let origin = NSRunningApplication(processIdentifier: pid),
              ["app.richos.richfiles", "com.richos.app"].contains(origin.bundleIdentifier ?? "") else { return nil }
        return origin
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
            else if viewer == nil, capture == nil { NSApp.terminate(nil) }
        }
    }

    @objc private func startCapture() {
        if let capture {
            capture.revisitPermission()
            return
        }
        captureNotice?.orderOut(nil)
        captureNotice = nil
        let origin = externalFrontmost
        let keyWindow = origin == nil && NSApp.isActive ? NSApp.keyWindow : nil
        let session = CaptureController(previousWindow: keyWindow === viewer?.window ? keyWindow : nil,
                                        previousApp: origin ?? previousApplication) { [weak self] message, display in
            guard let self else { return }
            capture = nil
            if let message, let display { showCaptureNotice(message, display: display) }
            else { endCaptureLaunch() }
        }
        capture = session
        openingPanel?.cancel(nil)
        session.start()
    }

    private func showCaptureNotice(_ message: String, display: CGRect) {
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let panel = NSPanel(contentRect: CGRect(x: display.midX - 123, y: display.maxY - 84, width: 246, height: 38),
                            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.level = .statusBar
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        let background = NSView(frame: CGRect(x: 0, y: 0, width: 246, height: 38))
        background.wantsLayer = true
        background.layer?.backgroundColor = (dark ? NSColor(red: 0.094, green: 0.141, blue: 0.251, alpha: 1)
                                                   : NSColor(red: 0.992, green: 0.988, blue: 0.973, alpha: 1)).cgColor
        background.layer?.cornerRadius = 19
        let label = NSTextField(labelWithString: "✓  " + message)
        label.textColor = dark ? .white : .black
        label.font = .systemFont(ofSize: 16, weight: .semibold)
        label.alignment = .center
        label.frame = CGRect(x: 0, y: 9, width: 246, height: 20)
        background.addSubview(label)
        panel.contentView = background
        captureNotice = panel
        panel.orderFrontRegardless()
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.8) { [weak self, weak panel] in
            guard let self, let panel, captureNotice === panel else { return }
            panel.orderOut(nil)
            captureNotice = nil
            endCaptureLaunch()
        }
    }

    private func endCaptureLaunch() {
        if captureLaunch, viewer == nil {
            RichFilesDiagnostics.flush()
            NSApp.terminate(nil)
        }
        captureLaunch = false
    }

    @objc private func openInPreview() { viewer?.openInPreview() }
    @objc private func showInfo() { viewer?.info.toggleInfo() }
    @objc private func showInfoByDefault() { viewer?.info.toggleDefault() }
    @objc private func changeInfoShortcut() { viewer?.info.beginRecording() }

    private func updateInfoShortcut() {
        infoMenuItem?.keyEquivalent = infoPreferences.shortcut.key
        infoMenuItem?.keyEquivalentModifierMask = infoPreferences.shortcut.flags
    }

    private func installMenu() {
        let menuBar = NSMenu()
        let app = NSMenuItem()
        app.submenu = NSMenu()
        app.submenu?.addItem(withTitle: "About RichGlance", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        app.submenu?.addItem(.separator())
        app.submenu?.addItem(withTitle: "Hide RichGlance", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.submenu?.addItem(withTitle: "Quit RichGlance", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        menuBar.addItem(app)

        let file = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        file.submenu = fileMenu
        fileMenu.addItem(withTitle: "Open…", action: #selector(chooseImage), keyEquivalent: "o").target = self
        fileMenu.addItem(withTitle: "Open in Preview", action: #selector(openInPreview), keyEquivalent: "").target = self
        fileMenu.addItem(withTitle: "Capture Region…", action: #selector(startCapture), keyEquivalent: "").target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        menuBar.addItem(file)

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
        if menuItem.action == #selector(startCapture) { return capture == nil }
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

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
    private var openingPanel: NSOpenPanel?
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
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        if let file = urls.first(where: GlanceFiles.supports) { open(file) }
    }

    func applicationOpenUntitledFile(_ sender: NSApplication) -> Bool {
        if viewer == nil { chooseImage() }
        return true
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if viewer == nil { chooseImage() }
        else if !hasVisibleWindows { viewer?.show() }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

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
        let viewer = self.viewer ?? Viewer()
        self.viewer = viewer
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
        panel.allowedContentTypes = [.png, .jpeg]
        panel.allowsMultipleSelection = false
        openingPanel = panel
        panel.begin { [weak self, weak panel] result in
            guard let self else { return }
            openingPanel = nil
            if result == .OK, let file = panel?.url { open(file) }
            else if viewer == nil { NSApp.terminate(nil) }
        }
    }

    @objc private func openInPreview() { viewer?.openInPreview() }

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
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        menuBar.addItem(file)

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
        if menuItem.action == #selector(openInPreview) { return viewer?.file != nil }
        return true
    }
}

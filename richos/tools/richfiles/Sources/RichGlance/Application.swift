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

    static func main() {
        let app = NSApplication.shared
        let delegate = Application()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        delegate.installMenu()
        app.run()
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

    func windowWillClose(_ notification: Notification) {
        viewer?.stop()
        viewer = nil
        defer { returnTo = nil }
        // Let the window server choose the next app when there was no reliable origin.
        // Closing a window in the background must not pull another app to the front.
        guard NSApp.isActive, let origin = returnTo, !origin.isTerminated else { return }
        if #available(macOS 14, *) { NSApp.yieldActivation(to: origin) }
        origin.activate(options: [])
    }

    private var externalFrontmost: NSRunningApplication? {
        guard let front = workspace.frontmostApplication, front != .current else { return nil }
        return front
    }

    @objc private func applicationActivated(_ note: Notification) {
        guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app != .current else { return }
        previousApplication = app
    }

    private func open(_ file: URL) {
        if let origin = externalFrontmost ?? previousApplication, !origin.isTerminated {
            returnTo = origin
        }
        let viewer = self.viewer ?? Viewer()
        self.viewer = viewer
        openingPanel?.cancel(nil)
        viewer.window.delegate = self
        viewer.open(file.standardizedFileURL)
        viewer.show()
        if #available(macOS 14, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
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

import AppKit
import ImageIO
import RichFilesCore
import UniformTypeIdentifiers

@MainActor
final class CaptureController: NSObject, NSWindowDelegate {
    private struct Display {
        let id: CGDirectDisplayID
        let frame: CGRect
        let scale: CGFloat

        init?(_ screen: NSScreen) {
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return nil }
            id = number.uint32Value
            frame = screen.frame
            scale = screen.backingScaleFactor
        }

        var size: CGSize { CGSize(width: (frame.width * scale).rounded(), height: (frame.height * scale).rounded()) }
        func matches(_ screen: NSScreen) -> Bool {
            guard let other = Display(screen) else { return false }
            return id == other.id && frame == other.frame && scale == other.scale
        }
    }

    private let id = UUID()
    private let began = ProcessInfo.processInfo.systemUptime
    private let finish: (_ copied: String?, _ display: CGRect?) -> Void
    private weak var previousWindow: NSWindow?
    private let previousApp: NSRunningApplication?
    private var display: Display?
    private var acquisition: Task<Void, Never>?
    private var overlay: CaptureWindow?
    private var canvas: CaptureCanvas?
    private var savePanel: NSSavePanel?
    private var isFinished = false
    private var permissionRequested = false
    private var waitingForPermission = false

    init(previousWindow: NSWindow?, previousApp: NSRunningApplication?,
         finish: @escaping (_ copied: String?, _ display: CGRect?) -> Void) {
        self.previousWindow = previousWindow
        self.previousApp = previousApp
        self.finish = finish
    }

    func start() {
        let point = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? NSScreen.main,
              let display = Display(screen) else {
            stop(.unavailable, operation: .glanceCaptureFrame)
            return
        }
        self.display = display
        if !CGPreflightScreenCaptureAccess() {
            let allowed = !permissionRequested && CGRequestScreenCaptureAccess()
            permissionRequested = true
            if !allowed {
                record(.glanceCaptureFrame, .unavailable, since: began)
                showPermissionHelp()
                return
            }
        }
        NotificationCenter.default.addObserver(self, selector: #selector(displaysChanged),
                                               name: NSApplication.didChangeScreenParametersNotification, object: nil)
        acquisition = Task { [weak self] in
            do {
                let image = try await CaptureFrame.read(displayID: display.id, size: display.size)
                guard !Task.isCancelled, let self, !isFinished else { return }
                show(image, on: display, point: point)
            } catch {
                guard !Task.isCancelled, let self else { return }
                record(.glanceCaptureFrame, .issues, since: began)
                showCaptureError()
            }
        }
    }

    func cancel() { stop(.cancelled) }

    /// A shortcut pressed from System Settings returns the same permission session to the user.
    func revisitPermission() {
        guard waitingForPermission else { return }
        waitingForPermission = false
        NotificationCenter.default.removeObserver(self, name: NSApplication.didBecomeActiveNotification, object: NSApp)
        showPermissionHelp()
    }

    @objc private func resumeAfterSettings() {
        Task { @MainActor [weak self] in self?.revisitPermission() }
    }

    private func show(_ image: CGImage, on display: Display, point: CGPoint) {
        let window = CaptureWindow(contentRect: display.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.setFrame(display.frame, display: false)
        window.level = .screenSaver
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        window.isReleasedWhenClosed = false
        window.hasShadow = false
        window.acceptsMouseMovedEvents = true
        window.delegate = self
        window.title = "RichGlance region capture"
        let geometry = GlanceCaptureGeometry(frame: CGSize(width: image.width, height: image.height), view: display.frame.size)
        let pointer = geometry.sourcePoint(CGPoint(x: point.x - display.frame.minX, y: display.frame.maxY - point.y))
        let canvas = CaptureCanvas(image: image, frame: CGRect(origin: .zero, size: display.frame.size), pointer: pointer)
        canvas.copy = { [weak self] in self?.copyImage() }
        canvas.save = { [weak self] in self?.saveImage() }
        canvas.cancel = { [weak self] in self?.cancel() }
        let frame = CGRect(origin: .zero, size: display.frame.size)
        let background = NSImageView(frame: frame)
        background.image = NSImage(cgImage: image, size: frame.size)
        background.imageScaling = .scaleAxesIndependently
        background.setAccessibilityElement(false)
        background.wantsLayer = true
        let content = NSView(frame: frame)
        content.addSubview(background)
        content.addSubview(canvas)
        window.contentView = content
        self.canvas = canvas
        overlay = window
        activate()
        if NSApp.isActive {
            focusCapture()
        } else {
            // A background Launch Services wakeup must leave the source app undisturbed
            // until the frame exists. Ask Launch Services to activate the existing app now.
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            configuration.addsToRecentItems = false
            NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration) { [weak self] _, _ in
                Task { @MainActor in self?.focusCapture() }
            }
        }
        record(.glanceCaptureFrame, .complete, since: began)
    }

    private func focusCapture() {
        guard let overlay, let canvas else { return }
        overlay.makeKeyAndOrderFront(nil)
        overlay.makeFirstResponder(canvas)
    }

    private func copyImage() {
        let started = ProcessInfo.processInfo.systemUptime
        guard let canvas else { return }
        guard let crop = canvas.selection.crop, !crop.isEmpty else {
            canvas.showError("Nothing selected yet. Move a corner with the arrows.")
            return
        }
        guard let data = pngData() else {
            outputFailed("The selected image could not be copied.", since: started)
            return
        }
        let clipboard = NSPasteboard.general
        clipboard.clearContents()
        guard clipboard.setData(data, forType: .png) else {
            outputFailed("The selected image could not be copied.", since: started)
            return
        }
        record(.glanceCaptureOutput, .complete, since: started)
        stop(.complete, copied: "Copied \(Int(crop.width)) × \(Int(crop.height)) px")
    }

    private func saveImage() {
        guard let overlay, let canvas, canvas.selection.canCapture, savePanel == nil else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        let stamp = DateFormatter()
        stamp.dateFormat = "yyyy-MM-dd 'at' h.mm.ss a"
        panel.nameFieldStringValue = "RichGlance Capture \(stamp.string(from: Date())).png"
        panel.canCreateDirectories = true
        savePanel = panel
        // The panel must float over the full-screen capture window.
        panel.level = overlay.level
        panel.beginSheetModal(for: overlay) { [weak self, weak panel] outcome in
            guard let self, let panel, savePanel === panel, !isFinished else { return }
            savePanel = nil
            guard outcome == .OK, let url = panel.url else {
                overlay.makeKeyAndOrderFront(nil)
                overlay.makeFirstResponder(canvas)
                return
            }
            let started = ProcessInfo.processInfo.systemUptime
            do {
                guard let data = pngData() else { throw CocoaError(.fileWriteUnknown) }
                try data.write(to: url, options: .atomic)
                record(.glanceCaptureOutput, .complete, since: started)
                stop(.complete)
            } catch {
                outputFailed("The PNG could not be saved. Choose a different location and try again.", since: started)
            }
        }
    }

    private func pngData() -> Data? {
        guard let canvas, let crop = canvas.selection.crop, !crop.isEmpty,
              let image = canvas.image.cropping(to: crop) else { return nil }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }

    private func outputFailed(_ message: String, since started: TimeInterval) {
        record(.glanceCaptureOutput, .issues, since: started)
        canvas?.showError(message)
        overlay?.makeFirstResponder(canvas)
    }

    private func showPermissionHelp() {
        activate()
        let alert = NSAlert()
        alert.messageText = "RichGlance needs your permission to capture the screen"
        alert.informativeText = "RichGlance takes one still picture when you choose Capture Region, and it stays on this Mac. Turn on the app named in the macOS prompt under Screen & System Audio Recording, then try again. If macOS asks you to quit and reopen that app, do that before capturing again."
        alert.addButton(withTitle: "Open System Settings")
        alert.addButton(withTitle: "Try Again")
        alert.addButton(withTitle: "Not Now").keyEquivalent = "\u{1b}"
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            guard let settings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"),
                  NSWorkspace.shared.open(settings) else {
                stop(.unavailable)
                return
            }
            waitingForPermission = true
            NotificationCenter.default.addObserver(self, selector: #selector(resumeAfterSettings),
                                                   name: NSApplication.didBecomeActiveNotification, object: NSApp)
        case .alertSecondButtonReturn:
            Task { @MainActor [weak self] in self?.start() }
        default:
            stop(.unavailable)
        }
    }

    private func showCaptureError() {
        if !CGPreflightScreenCaptureAccess() {
            showPermissionHelp()
            return
        }
        activate()
        let alert = NSAlert()
        alert.messageText = "Could not capture this display"
        alert.informativeText = "Try the capture again with your pointer on the display you want to use."
        alert.addButton(withTitle: "OK")
        alert.runModal()
        stop(.issues)
    }

    @objc private func displaysChanged() {
        guard let display, !NSScreen.screens.contains(where: display.matches) else { return }
        stop(.interrupted)
    }

    func windowDidResignKey(_ notification: Notification) {
        guard savePanel == nil else { return }
        // App switching should put the user back in control of the new app.
        stop(.cancelled, restoreFocus: false)
    }

    private func activate() {
        if #available(macOS 14.0, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
    }

    private func stop(_ outcome: RichFilesDiagnostics.Outcome,
                      operation: RichFilesDiagnostics.Operation = .glanceCaptureSession,
                      restoreFocus: Bool = true, copied: String? = nil) {
        guard !isFinished else { return }
        isFinished = true
        acquisition?.cancel()
        acquisition = nil
        NotificationCenter.default.removeObserver(self)
        savePanel?.cancel(nil)
        savePanel = nil
        overlay?.delegate = nil
        if restoreFocus, NSApp.isActive {
            if let previousWindow, previousWindow.isVisible, !previousWindow.isMiniaturized {
                previousWindow.makeKeyAndOrderFront(nil)
            } else if let previousApp, !previousApp.isTerminated {
                if #available(macOS 14.0, *) { _ = previousApp.activate(from: .current, options: []) }
                else { _ = previousApp.activate(options: []) }
            }
        }
        overlay?.orderOut(nil)
        overlay = nil
        canvas = nil
        record(operation, outcome, since: began)
        finish(copied, display?.frame)
    }

    private func record(_ operation: RichFilesDiagnostics.Operation, _ outcome: RichFilesDiagnostics.Outcome, since: TimeInterval) {
        RichFilesDiagnostics.record(operation, id: id, outcome: outcome, started: since,
                                    total: 1, confirmed: outcome == .complete ? 1 : 0)
    }
}

private final class CaptureWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

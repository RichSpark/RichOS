import AppKit
import RichFiles
import SwiftUI

@_cdecl("richfiles_rail_place")
public func placeRichFilesRail(_ webViewPointer: UnsafeMutableRawPointer?, _ left: Double, _ top: Double,
                               _ width: Double, _ height: Double, _ visible: Int32) -> Int32 {
    guard Thread.isMainThread, let webViewPointer else { return 0 }
    return MainActor.assumeIsolated {
        let view = Unmanaged<NSView>.fromOpaque(webViewPointer).takeUnretainedValue()
        let frame = CGRect(x: left, y: top, width: width, height: height)
        return RailHost.shared.place(beside: view, normalizedFrame: frame, visible: visible != 0) ? 1 : 0
    }
}

@MainActor
private final class RailHost {
    static let shared = RailHost()
    private weak var webView: NSView?
    private var hosting: PaneHostingView?
    private var monitor: Any?

    func place(beside webView: NSView, normalizedFrame: CGRect, visible: Bool) -> Bool {
        guard visible else {
            hide()
            return true
        }
        guard let parent = webView.superview, webView.window != nil, !normalizedFrame.isEmpty,
              CGRect(x: 0, y: 0, width: 1.000001, height: 1.000001).contains(normalizedFrame) else {
            return false
        }
        if self.webView !== webView || hosting?.superview !== parent {
            hide()
            hosting?.removeFromSuperview()
            self.webView = webView
            let home = FileManager.default.homeDirectoryForCurrentUser
            let pane = RichFilesPane(folder: home, recoveryStore: RichFilesInstallation.dockedRecoveryStore,
                                     openWindow: { [weak self] folder in self?.openStandalone(folder: folder) },
                                     acceptsKeyboard: { [weak self] in self?.hasPaneFocus() == true },
                                     returnKeyboardFocus: { [weak self] in self?.focusPane() })
            let view = PaneHostingView(rootView: pane)
            view.sizingOptions = []
            view.autoresizingMask = []
            view.isHidden = true
            parent.addSubview(view, positioned: .above, relativeTo: webView)
            hosting = view
            installFocusMonitor()
        }
        guard let hosting else { return false }
        let bounds = webView.bounds
        let top = webView.isFlipped ? normalizedFrame.minY : 1 - normalizedFrame.maxY
        let local = CGRect(x: bounds.minX + normalizedFrame.minX * bounds.width,
                           y: bounds.minY + top * bounds.height,
                           width: normalizedFrame.width * bounds.width,
                           height: normalizedFrame.height * bounds.height)
        hosting.frame = webView.convert(local, to: parent)
        if hosting.isHidden {
            hosting.isHidden = false
            focusPane()
        }
        return true
    }

    private func hide() {
        if hasPaneFocus(), let webView { webView.window?.makeFirstResponder(webView) }
        hosting?.isHidden = true
    }

    private func focusPane() {
        guard let hosting, !hosting.isHidden else { return }
        hosting.window?.makeFirstResponder(hosting)
    }

    private func hasPaneFocus() -> Bool {
        guard let hosting, !hosting.isHidden, let responder = hosting.window?.firstResponder else { return false }
        if let view = responder as? NSView, view === hosting || view.isDescendant(of: hosting) { return true }
        if let editor = responder as? NSTextView, let control = editor.delegate as? NSView {
            return control.isDescendant(of: hosting)
        }
        return false
    }

    private func installFocusMonitor() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self, let pane = self.hosting, let window = pane.window,
                  !pane.isHidden, event.window === window, window.attachedSheet == nil else { return event }
            let local = pane.convert(event.locationInWindow, from: nil)
            if pane.bounds.contains(local), !self.hasPaneFocus() { window.makeFirstResponder(pane) }
            return event
        }
    }

    private func openStandalone(folder: URL) {
        let application = RichFilesInstallation.embeddedApplication
        let configuration = NSWorkspace.OpenConfiguration()
        NSWorkspace.shared.open([folder], withApplicationAt: application, configuration: configuration) { _, error in
            if let error {
                Task { @MainActor in
                    let alert = NSAlert(error: error)
                    if let window = self.hosting?.window { alert.beginSheetModal(for: window) }
                }
            }
        }
    }
}

private final class PaneHostingView: NSHostingView<RichFilesPane> {
    override var acceptsFirstResponder: Bool { true }
}

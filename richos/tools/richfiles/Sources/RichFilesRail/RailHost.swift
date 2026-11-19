import AppKit
import RichFiles
import RichFilesCore
import SwiftUI
import WebKit

@_cdecl("richfiles_register_glance")
public func registerRichGlance() {
    RichFilesInstallation.registerGlance()
}

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

@_cdecl("richfiles_rail_tooltip")
public func setRichFilesRailTooltip(_ webViewPointer: UnsafeMutableRawPointer?, _ left: Double, _ top: Double,
                                    _ width: Double, _ height: Double, _ radius: Double, _ arrow: Double) {
    guard Thread.isMainThread, let webViewPointer else { return }
    MainActor.assumeIsolated {
        let webView = Unmanaged<NSView>.fromOpaque(webViewPointer).takeUnretainedValue()
        RailHost.shared.setTooltip(beside: webView, frame: CGRect(x: left, y: top, width: width, height: height),
                                   radius: radius, arrow: arrow)
    }
}

@_cdecl("richfiles_rail_recording_frame")
public func recordRichFilesRailFrame(
    _ webViewPointer: UnsafeMutableRawPointer?,
    _ reply: @convention(c) (UnsafeRawPointer?, Int, Float, Float, UnsafeMutableRawPointer?) -> Void,
    _ context: UnsafeMutableRawPointer?
) {
    guard Thread.isMainThread, let webViewPointer else { return }
    MainActor.assumeIsolated {
        let webView = Unmanaged<NSView>.fromOpaque(webViewPointer).takeUnretainedValue()
        guard let (image, cursor) = RailHost.shared.recordingFrame(beside: webView) else { return }
        image.withUnsafeBytes { bytes in
            reply(bytes.baseAddress, bytes.count, Float(cursor.x), Float(cursor.y), context)
        }
    }
}

@_cdecl("richfiles_rail_receive_workspace")
public func receiveRichFilesWorkspace(_ identifier: UnsafePointer<CChar>?) -> Int32 {
    guard Thread.isMainThread, let identifier, let value = String(validatingUTF8: identifier),
          let id = UUID(uuidString: value) else { return 0 }
    return MainActor.assumeIsolated { RailHost.shared.receive(id) ? 1 : 0 }
}

@_cdecl("richfiles_rail_reveal")
public func revealRichFilesItem(_ path: UnsafePointer<CChar>?) -> Int32 {
    guard Thread.isMainThread, let path, let value = String(validatingUTF8: path), value.hasPrefix("/") else { return 0 }
    return MainActor.assumeIsolated {
        RailHost.shared.reveal(URL(fileURLWithPath: value)) ? 1 : 0
    }
}

@_cdecl("richfiles_git_place")
public func placeOutputGit(_ webViewPointer: UnsafeMutableRawPointer?, _ left: Double, _ top: Double,
                           _ width: Double, _ height: Double, _ visible: Int32, _ dark: Int32,
                           _ context: UnsafePointer<CChar>?, _ compare: Int32) -> Int32 {
    guard Thread.isMainThread, let webViewPointer else { return 0 }
    return MainActor.assumeIsolated {
        guard let web = Unmanaged<NSView>.fromOpaque(webViewPointer).takeUnretainedValue() as? WKWebView else { return 0 }
        let payload: GitPanelContext?
        if let context {
            guard let value = String(validatingUTF8: context),
                  let decoded = try? JSONDecoder().decode(GitPanelContext.self, from: Data(value.utf8)) else { return 0 }
            payload = decoded
        } else { payload = nil }
        return OutputGitHost.shared.place(web, frame: CGRect(x: left, y: top, width: width, height: height),
                                          visible: visible != 0, dark: dark != 0, context: payload,
                                          mode: compare) ? 1 : 0
    }
}

@_cdecl("richfiles_git_chat")
public func changeRichFilesGitChat(_ value: UnsafePointer<CChar>?) -> Int32 {
    guard Thread.isMainThread else { return 0 }
    return MainActor.assumeIsolated {
        let thread = value.flatMap(String.init(validatingUTF8:))
        RailHost.shared.setChat(thread)
        OutputGitHost.shared.chatChanged(thread)
        return 1
    }
}

@MainActor
private final class RailHost {
    static let shared = RailHost()
    private weak var webView: NSView?
    private var container: NSView?
    private var hosting: PaneHostingView?
    private var dockTarget: RichFilesDockTarget?
    private var monitor: Any?
    private var currentChat: String?
    private var gitHandoff: GitPanelSnapshot?
    private var recordingVisit = UUID()
    private var reportedFrame = false
    private var reportedFrameFailure = false

    func place(beside webView: NSView, normalizedFrame: CGRect, visible: Bool) -> Bool {
        guard visible else {
            hide()
            return true
        }
        guard let parent = webView.superview, webView.window != nil, !normalizedFrame.isEmpty,
              CGRect(x: 0, y: 0, width: 1.000001, height: 1.000001).contains(normalizedFrame) else {
            return false
        }
        if self.webView !== webView || container?.superview !== parent {
            hide()
            container?.removeFromSuperview()
            self.webView = webView
            let home = FileManager.default.homeDirectoryForCurrentUser
            let destination = RichFilesDockTarget()
            destination.chatChanged(currentChat)
            dockTarget = destination
            let pane = RichFilesPane(folder: home, recoveryStore: RichFilesInstallation.dockedRecoveryStore,
                                     openWindow: { [weak self] folder in self?.openStandalone(folder: folder) },
                                     acceptsKeyboard: { [weak self] in self?.hasPaneFocus() == true },
                                     returnKeyboardFocus: { [weak self] in self?.focusPane() },
                                     dockTarget: destination, didUndock: { [weak self] in self?.showChats() })
            let container = RailPaneContainer()
            container.wantsLayer = true
            parent.addSubview(container, positioned: .above, relativeTo: webView)
            self.container = container
            let view = PaneHostingView(rootView: pane)
            view.sizingOptions = []
            view.autoresizingMask = []
            view.isHidden = true
            container.addSubview(view)
            hosting = view
            installFocusMonitor()
        }
        guard let hosting, let container else { return false }
        container.frame = webView.convert(webRect(normalizedFrame, in: webView), to: parent)
        hosting.frame = container.bounds
        if hosting.isHidden {
            hosting.isHidden = false
            focusPane()
        }
        if let snapshot = gitHandoff {
            gitHandoff = nil
            _ = performInPane { try await $0.showGit(snapshot) }
        }
        return true
    }

    private func webRect(_ frame: CGRect, in webView: NSView) -> CGRect {
        let bounds = webView.bounds
        let top = webView.isFlipped ? frame.minY : 1 - frame.maxY
        return CGRect(x: bounds.minX + frame.minX * bounds.width, y: bounds.minY + top * bounds.height,
                      width: frame.width * bounds.width, height: frame.height * bounds.height)
    }

    func setTooltip(beside webView: NSView, frame: CGRect, radius: CGFloat, arrow: CGFloat) {
        guard self.webView === webView, let container, let layer = container.layer else { return }
        let tip = webView.convert(webRect(frame, in: webView), to: container)
        let arrow = arrow * webView.bounds.width
        guard !tip.isEmpty, container.bounds.intersects(tip.insetBy(dx: -arrow, dy: 0)) else {
            layer.mask = nil
            return
        }
        // The label belongs to WebKit. Cut its shape out of the native sibling so its
        // existing text, size and shortcut stay visible, with the pane still interactive.
        let radius = min(radius * webView.bounds.width, tip.width / 2, tip.height / 2)
        let path = CGMutablePath()
        path.addRect(container.bounds)
        path.addRoundedRect(in: tip, cornerWidth: radius, cornerHeight: radius)
        path.move(to: CGPoint(x: tip.minX, y: tip.midY - arrow))
        path.addLine(to: CGPoint(x: tip.minX - arrow, y: tip.midY))
        path.addLine(to: CGPoint(x: tip.minX, y: tip.midY + arrow))
        path.closeSubpath()
        let mask = CAShapeLayer()
        mask.frame = layer.bounds
        mask.fillRule = .evenOdd
        mask.path = path
        layer.mask = mask
    }

    func recordingFrame(beside webView: NSView) -> (Data, CGPoint)? {
        guard self.webView === webView, let hosting, !hosting.isHiddenOrHasHiddenAncestor,
              let window = hosting.window, !hosting.bounds.isEmpty else { return nil }
        let started = ProcessInfo.processInfo.systemUptime
        let bounds = hosting.bounds
        let scale = min(1, 1440 / bounds.width, 1080 / bounds.height)
        let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil,
            pixelsWide: max(1, Int(bounds.width * scale)), pixelsHigh: max(1, Int(bounds.height * scale)),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0)
        bitmap?.size = bounds.size
        // Snapshot the hosting view alone. The tooltip cutout belongs to its parent
        // and the canvas draws the rail tooltip over the completed frame itself.
        let picture = bitmap.flatMap { bitmap in
            hosting.cacheDisplay(in: bounds, to: bitmap)
            return bitmap.representation(using: .jpeg, properties: [.compressionFactor: 0.82])
        }
        if picture == nil && !reportedFrameFailure {
            reportedFrameFailure = true
            RichFilesDiagnostics.record(.railVideoFrame, id: recordingVisit, outcome: .unavailable,
                                        started: started, total: 1, confirmed: 0)
        }
        guard let picture else { return nil }
        if !reportedFrame {
            reportedFrame = true
            RichFilesDiagnostics.record(.railVideoFrame, id: recordingVisit, outcome: .complete,
                                        started: started, total: 1, confirmed: 1)
        }
        // WebKit does not receive pointer moves over its native sibling.
        let mouse = hosting.convert(window.mouseLocationOutsideOfEventStream, from: nil)
        let cursor: CGPoint
        if bounds.contains(mouse) {
            let y = (mouse.y - bounds.minY) / bounds.height
            cursor = CGPoint(x: (mouse.x - bounds.minX) / bounds.width, y: hosting.isFlipped ? y : 1 - y)
        } else {
            cursor = CGPoint(x: CGFloat.nan, y: CGFloat.nan)
        }
        return (picture, cursor)
    }

    func setChat(_ thread: String?) {
        currentChat = thread
        if let gitHandoff, gitHandoff.threadId != thread { self.gitHandoff = nil }
        dockTarget?.chatChanged(thread)
    }

    func showGit(_ snapshot: GitPanelSnapshot, from web: WKWebView) {
        guard snapshot.threadId == currentChat else { return }
        if !performInPane({ try await $0.showGit(snapshot) }) {
            gitHandoff = snapshot
            web.evaluateJavaScript("window.RichOutput?.close({keepFocus:true}); window.RichNavRail?.showPage('richfiles')")
        }
    }

    func receive(_ id: UUID) -> Bool {
        performInPane { try await $0.accept(id) }
    }

    func reveal(_ location: URL) -> Bool {
        performInPane { try await $0.reveal(location) }
    }

    private func performInPane(_ action: @escaping @MainActor (RichFilesDockTarget) async throws -> Void) -> Bool {
        guard let dockTarget, hosting?.isHidden == false else { return false }
        Task {
            do { try await action(dockTarget); focusPane() }
            catch {
                let alert = NSAlert(error: error)
                if let window = hosting?.window { alert.beginSheetModal(for: window, completionHandler: nil) }
            }
        }
        return true
    }

    private func showChats() {
        guard let web = webView as? WKWebView else { return }
        web.evaluateJavaScript("window.RichNavRail?.showPage('chats')")
    }

    private func hide() {
        if hasPaneFocus(), let webView { webView.window?.makeFirstResponder(webView) }
        if reportedFrame || reportedFrameFailure { recordingVisit = UUID() }
        reportedFrame = false
        reportedFrameFailure = false
        hosting?.isHidden = true
        container?.layer?.mask = nil
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
        Task {
            do {
                try await RichFilesInstallation.openStandaloneWindow(at: folder)
            } catch {
                if let window = hosting?.window { NSAlert(error: error).beginSheetModal(for: window, completionHandler: nil) }
            }
        }
    }
}

/// Holds the pane so the rail tooltip can be cut out of it. The container itself never
/// takes a click or scroll: while the pane is hidden, they reach the web view beneath.
final class RailPaneContainer: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }
}

private final class PaneHostingView: NSHostingView<RichFilesPane> {
    override var acceptsFirstResponder: Bool { true }
}

@MainActor
private final class OutputGitHost {
    static let shared = OutputGitHost()
    private let model = GitPanelModel()
    private weak var web: WKWebView?
    private var hosting: OutputGitHostingView?
    private var thread: String?
    private var dark: Bool?

    func chatChanged(_ next: String?) {
        guard thread != next else { return }
        thread = next
        hide()
    }

    func place(_ web: WKWebView, frame: CGRect, visible: Bool, dark: Bool, context: GitPanelContext?, mode: Int32) -> Bool {
        if !visible { hide(clear: mode == 3); return true }
        if let context {
            thread = context.threadId
            RailHost.shared.setChat(context.threadId)
            model.beginOutput(context, compare: mode == 1, refresh: mode == 2)
        }
        guard model.open, let parent = web.superview, let window = web.window, !frame.isEmpty,
              CGRect(x: 0, y: 0, width: 1.000001, height: 1.000001).contains(frame) else { return false }
        let needsFocus = hosting == nil || hosting?.isHidden == true
        if self.web !== web || hosting?.superview !== parent || self.dark != dark {
            hosting?.removeFromSuperview()
            self.web = web
            self.dark = dark
            let panel = RichFilesGitPanel(model: model, dismiss: { [weak self] in self?.closeFromPanel() },
                                          handoff: { [weak self] in self?.handoff($0) },
                                          refresh: { [weak self] in self?.web?.evaluateJavaScript("window.RichOutput?.nativeGitRefresh()") })
                .onReceive(model.$directComparisonUnavailable) { [weak self] unavailable in
                    if unavailable { self?.web?.evaluateJavaScript("window.RichOutput?.nativeGitUnavailable()") }
                }
                .environment(\.colorScheme, dark ? .dark : .light)
                .defaultAppStorage(RichFilesInstallation.gitPreferences)
            let view = OutputGitHostingView(rootView: AnyView(panel))
            view.sizingOptions = []
            parent.addSubview(view, positioned: .above, relativeTo: web)
            hosting = view
        }
        // WKWebView can extend beneath the title bar. CSS viewport coordinates
        // start at the visible content, which is also the overlay's origin.
        let bounds = web.bounds.intersection(web.convert(window.contentLayoutRect, from: nil))
        guard !bounds.isEmpty else { return false }
        let top = web.isFlipped ? frame.minY : 1 - frame.maxY
        let local = CGRect(x: bounds.minX + frame.minX * bounds.width, y: bounds.minY + top * bounds.height,
                           width: frame.width * bounds.width, height: frame.height * bounds.height)
        hosting?.frame = web.convert(local, to: parent)
        hosting?.isHidden = false
        if needsFocus || mode == 1, let hosting { window.makeFirstResponder(hosting) }
        return true
    }

    private func hide(clear: Bool = true) {
        if clear { model.close() }
        guard let hosting, !hosting.isHidden else { return }
        if let responder = hosting.window?.firstResponder as? NSView,
           responder === hosting || responder.isDescendant(of: hosting), let web {
            web.window?.makeFirstResponder(web)
        }
        hosting.isHidden = true
    }

    private func closeFromPanel() {
        hide()
        web?.evaluateJavaScript("window.RichOutput?.nativeGitClosed()")
    }

    private func handoff(_ snapshot: GitPanelSnapshot) {
        guard let web else { return }
        hide()
        RailHost.shared.showGit(snapshot, from: web)
    }
}

private final class OutputGitHostingView: NSHostingView<AnyView> {
    override var acceptsFirstResponder: Bool { true }
}

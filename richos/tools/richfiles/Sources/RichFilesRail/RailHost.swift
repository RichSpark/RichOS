import AppKit
import RichFiles
import SwiftUI
import WebKit

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

/// Builds the pane, hidden, before the first click on its rail icon, so that click only shows it.
@_cdecl("richfiles_rail_prepare")
public func prepareRichFilesRail(_ webViewPointer: UnsafeMutableRawPointer?) -> Int32 {
    guard Thread.isMainThread, let webViewPointer else { return 0 }
    return MainActor.assumeIsolated {
        let view = Unmanaged<NSView>.fromOpaque(webViewPointer).takeUnretainedValue()
        return RailHost.shared.prepare(beside: view) ? 1 : 0
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
    private var hosting: PaneHostingView?
    private var dockTarget: RichFilesDockTarget?
    private var monitor: Any?
    private var currentChat: String?
    private var gitHandoff: GitPanelSnapshot?

    func place(beside webView: NSView, normalizedFrame: CGRect, visible: Bool) -> Bool {
        guard visible else {
            hide()
            return true
        }
        guard let parent = webView.superview, webView.window != nil, !normalizedFrame.isEmpty,
              CGRect(x: 0, y: 0, width: 1.000001, height: 1.000001).contains(normalizedFrame) else {
            return false
        }
        build(beside: webView, in: parent)
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
        if let snapshot = gitHandoff {
            gitHandoff = nil
            _ = performInPane { try await $0.showGit(snapshot) }
        }
        return true
    }

    /// The pane built and laid out while still hidden, in the web view's own area: `place` gives
    /// it its frame and shows it. A pane that is already on screen is left as it is.
    func prepare(beside webView: NSView) -> Bool {
        guard let parent = webView.superview, webView.window != nil else { return false }
        build(beside: webView, in: parent)
        guard let hosting else { return false }
        if hosting.isHidden {
            hosting.frame = webView.frame
            hosting.layoutSubtreeIfNeeded()
        }
        return true
    }

    private func build(beside webView: NSView, in parent: NSView) {
        guard self.webView !== webView || hosting?.superview !== parent else { return }
        hide()
        hosting?.removeFromSuperview()
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
        let view = PaneHostingView(rootView: pane)
        view.sizingOptions = []
        view.autoresizingMask = []
        view.isHidden = true
        parent.addSubview(view, positioned: .above, relativeTo: webView)
        hosting = view
        installFocusMonitor()
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
        Task {
            do {
                try await RichFilesInstallation.openStandaloneWindow(at: folder)
            } catch {
                if let window = hosting?.window { NSAlert(error: error).beginSheetModal(for: window, completionHandler: nil) }
            }
        }
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

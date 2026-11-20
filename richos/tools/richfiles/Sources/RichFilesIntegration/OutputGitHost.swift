import AppKit
import RichFiles
import RichFilesCore
import SwiftUI
import WebKit

@_cdecl("richfiles_register_glance")
public func registerRichGlance() {
    RichFilesInstallation.registerGlance()
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
        OutputGitHost.shared.chatChanged(thread)
        return 1
    }
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
        Task {
            do { try await RichFilesInstallation.showGitInRichFiles(snapshot) }
            catch {
                if let window = web?.window {
                    NSAlert(error: error).beginSheetModal(for: window, completionHandler: nil)
                }
            }
        }
    }
}

private final class OutputGitHostingView: NSHostingView<AnyView> {
    override var acceptsFirstResponder: Bool { true }
}

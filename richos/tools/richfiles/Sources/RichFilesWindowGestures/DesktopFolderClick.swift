import AppKit
import RichFilesCore

/// Finder supplies a file reference for each real desktop icon. The first click
/// stays native and gives us time to check the reference without blocking input.
final class DesktopFolderClick {
    private final class Candidate {
        let icon: AXUIElement
        let reference: URL
        let point: CGPoint
        var folder: URL?

        init(icon: AXUIElement, reference: URL, point: CGPoint) {
            self.icon = icon
            self.reference = reference
            self.point = point
        }
    }

    private var first: Candidate?
    private var second: (folder: URL, point: CGPoint)?
    private var opening = 0
    var busy: Bool { second != nil || opening > 0 }

    private lazy var richFiles: URL? = {
        var parent = Bundle.main.bundleURL
        for _ in WindowGesturePreferences.helperPath.split(separator: "/") {
            parent.deleteLastPathComponent()
        }
        return Bundle(url: parent)?.bundleIdentifier == "app.richos.richfiles" ? parent : nil
    }()

    func reset() {
        first = nil
        second = nil
    }

    /// Ask WindowServer which window would receive this click, including when
    /// another app is active or a mouse-transparent overlay covers the Desktop.
    static func isDesktopWindow(at screenPoint: CGPoint) -> Bool {
        let number = NSWindow.windowNumber(at: screenPoint, belowWindowWithWindowNumber: 0)
        guard let id = CGWindowID(exactly: number), id != kCGNullWindowID,
              let windows = CGWindowListCopyWindowInfo(.optionIncludingWindow, id) as? [[String: Any]],
              let window = windows.first,
              let layer = window[kCGWindowLayer as String] as? Int32,
              layer == CGWindowLevelForKey(.desktopIconWindow),
              let pid = window[kCGWindowOwnerPID as String] as? pid_t else { return false }
        return NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.finder"
    }

    func down(_ event: CGEvent, element: AXUIElement?, clicks: Int) {
        let previous = first
        reset()
        guard let element, richFiles != nil else { return }
        if clicks == 2 {
            guard let previous, let folder = previous.folder, element == previous.icon,
                  let pid = element.processID, NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
                  let selection: [AXUIElement] = element.element(kAXParentAttribute)?.value(kAXSelectedChildrenAttribute),
                  selection == [element], let current: URL = element.value(kAXURLAttribute),
                  current == previous.reference else { return }
            // Finder still receives a normal press, so dragging this icon works.
            event.setIntegerValueField(.mouseEventClickState, value: 1)
            second = (folder, event.location)
            return
        }

        guard clicks == 1, let pid = element.processID,
              NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.finder",
              element.role == kAXImageRole, let group = element.element(kAXParentAttribute),
              group.role == kAXGroupRole, let desktop = group.element(kAXParentAttribute),
              desktop.role == kAXScrollAreaRole, desktop.element(kAXParentAttribute)?.role == kAXApplicationRole,
              let reference: URL = element.value(kAXURLAttribute), reference.isFileURL else { return }
        let candidate = Candidate(icon: element, reference: reference, point: event.location)
        first = candidate
        DispatchQueue.global(qos: .userInitiated).async {
            // Validate the destination but match the second click against the original icon.
            let folder = reference.resolvingSymlinksInPath()
            guard let values = try? folder.resourceValues(forKeys: [.isDirectoryKey, .isPackageKey, .isVolumeKey, .isSymbolicLinkKey]),
                  values.isDirectory == true, values.isPackage == false, values.isVolume == false,
                  values.isSymbolicLink == false else { return }
            DispatchQueue.main.async { [weak self, weak candidate] in
                if let candidate, self?.first === candidate { candidate.folder = folder }
            }
        }
    }

    func dragged(_ event: CGEvent) {
        if let first, moved(from: first.point, to: event.location) { self.first = nil }
        guard let second else { return }
        event.setIntegerValueField(.mouseEventClickState, value: 1)
        if moved(from: second.point, to: event.location) { self.second = nil }
    }

    func up(_ event: CGEvent) {
        guard let second else { return }
        self.second = nil
        event.setIntegerValueField(.mouseEventClickState, value: 1)
        guard !moved(from: second.point, to: event.location), let richFiles else { return }
        let started = ProcessInfo.processInfo.systemUptime
        opening += 1
        NSWorkspace.shared.open([second.folder], withApplicationAt: richFiles, configuration: .init()) { _, error in
            DispatchQueue.main.async {
                self.opening -= 1
                RichFilesDiagnostics.record(.desktopFolderOpen, id: UUID(), outcome: error == nil ? .complete : .issues,
                                            started: started, total: 1, confirmed: error == nil ? 1 : 0)
                if error != nil, let finder = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.finder") {
                    NSWorkspace.shared.open([second.folder], withApplicationAt: finder, configuration: .init(), completionHandler: nil)
                }
            }
        }
    }

    private func moved(from start: CGPoint, to end: CGPoint) -> Bool {
        hypot(end.x - start.x, end.y - start.y) >= 5
    }
}

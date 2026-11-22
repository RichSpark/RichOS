import AppKit
import RichFilesCore

@MainActor
final class BrowserWindowPlacement {
    enum Family: String {
        case browser, folder, handoff
    }

    struct Slot: Hashable {
        let family: Family
        let index: Int

        fileprivate var key: String { "richfiles.windowFrame.v2.\(family.rawValue).\(index)" }
    }

    static let legacyKey = "richfiles.primaryWindowFrame"

    private let defaults: UserDefaults
    private var occupied: Set<Slot> = []

    init(defaults: UserDefaults) { self.defaults = defaults }

    func claim(_ family: Family, preferred: Int) -> Slot {
        if preferred >= 0 {
            let slot = Slot(family: family, index: preferred)
            if occupied.insert(slot).inserted { return slot }
        }
        var index = 0
        while true {
            let slot = Slot(family: family, index: index)
            if occupied.insert(slot).inserted { return slot }
            index += 1
        }
    }

    func release(_ slot: Slot) { occupied.remove(slot) }

    func savedFrame(for slot: Slot) -> NSRect? {
        guard let value = defaults.string(forKey: slot.key) else { return nil }
        let frame = NSRectFromString(value)
        return Self.valid(frame) ? frame : nil
    }

    func save(_ frame: NSRect, in slot: Slot) {
        guard occupied.contains(slot), Self.valid(frame), frame != savedFrame(for: slot) else { return }
        defaults.set(NSStringFromRect(frame), forKey: slot.key)
    }

    func restore(_ slot: Slot, to window: NSWindow) {
        let started = ProcessInfo.processInfo.systemUptime
        let frame: NSRect
        if let saved = savedFrame(for: slot) {
            frame = saved
        } else if slot.family == .browser, slot.index == 0,
                  let legacy = defaults.string(forKey: Self.legacyKey), !legacy.isEmpty {
            window.setFrame(from: legacy)
            frame = window.frame
        } else {
            return
        }
        let fitted = Self.fit(frame, to: NSScreen.screens.map(\.visibleFrame),
                              fallback: NSScreen.main?.visibleFrame)
        window.setFrame(fitted, display: false)
        let restored = window.frame == fitted
        RichFilesDiagnostics.record(.windowPlacement, id: UUID(), outcome: restored ? .complete : .issues,
                                    started: started, total: 1, confirmed: restored ? 1 : 0)
    }

    private static func valid(_ frame: NSRect) -> Bool {
        [frame.minX, frame.minY, frame.width, frame.height].allSatisfy(\.isFinite)
            && frame.width > 0 && frame.height > 0
    }

    static func fit(_ frame: NSRect, to screens: [NSRect], fallback: NSRect?) -> NSRect {
        // Keep intentional placement across screens or edges when a user can still grab the title bar.
        let grabHeight = min(22, frame.height)
        let titleBar = NSRect(x: frame.minX, y: frame.maxY - grabHeight, width: frame.width, height: grabHeight)
        if screens.contains(where: { screen in
            let reachable = screen.intersection(titleBar)
            return reachable.width >= min(80, titleBar.width) && reachable.height >= grabHeight
        }) { return frame }

        func overlap(_ screen: NSRect) -> CGFloat {
            let intersection = screen.intersection(frame)
            return intersection.isEmpty ? 0 : intersection.width * intersection.height
        }
        let best = screens.max { overlap($0) < overlap($1) }
        let screen = best.flatMap { overlap($0) > 0 ? $0 : nil } ?? fallback ?? screens.first
        guard let screen else { return frame }
        let width = min(frame.width, screen.width)
        let height = min(frame.height, screen.height)
        let origin = overlap(screen) > 0 ? frame.origin : NSPoint(x: screen.midX - width / 2,
                                                                   y: screen.midY - height / 2)
        return NSRect(x: min(max(origin.x, screen.minX), screen.maxX - width),
                      y: min(max(origin.y, screen.minY), screen.maxY - height),
                      width: width, height: height)
    }
}

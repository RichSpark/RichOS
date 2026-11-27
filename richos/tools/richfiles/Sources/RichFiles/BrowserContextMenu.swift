import AppKit
import SwiftUI

@MainActor
enum BrowserContextMenu {
    /// AppKit normally flips a context menu to the left of the pointer at the
    /// display edge. We keep the pointer over the menu by fitting its right
    /// edge to the display. Ask for the menu at the original click first so
    /// fitting it cannot change which item or pane supplies the actions.
    static func fitAtRightEdge(_ event: NSEvent, in window: NSWindow) -> Bool {
        let point = window.convertPoint(toScreen: event.locationInWindow)
        guard let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? window.screen else {
            return false
        }
        var hit = window.contentView?.hitTest(event.locationInWindow)
        while let view = hit {
            if let menu = view.menu(for: event) {
                let visible = screen.visibleFrame
                let x = max(visible.minX, visible.maxX - menu.size.width)
                guard point.x > x else { return false }
                let anchor = view.convert(window.convertPoint(fromScreen: NSPoint(x: x, y: point.y)), from: nil)
                view.willOpenMenu(menu, with: event)
                menu.popUp(positioning: nil, at: anchor, in: view)
                view.didCloseMenu(menu, with: nil)
                return true
            }
            hit = view.superview
        }
        return false
    }

    /// The sidebar already supplies its own SwiftUI menus and has no drag surface.
    struct Sidebar: NSViewRepresentable {
        func makeNSView(context: Context) -> Monitor { Monitor() }
        func updateNSView(_ view: Monitor, context: Context) {}

        final class Monitor: NSView {
            private var monitor: Any?

            override func hitTest(_ point: NSPoint) -> NSView? { nil }

            override func viewDidMoveToWindow() {
                super.viewDidMoveToWindow()
                if let monitor { NSEvent.removeMonitor(monitor) }
                monitor = nil
                guard window != nil else { return }
                monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { [weak self] event in
                    let modifiers = event.modifierFlags.intersection([.control, .command, .option, .shift])
                    guard event.type == .rightMouseDown || modifiers == .control,
                          let self, !isHiddenOrHasHiddenAncestor,
                          let window, event.window === window, window.attachedSheet == nil,
                          bounds.contains(convert(event.locationInWindow, from: nil)) else { return event }
                    return BrowserContextMenu.fitAtRightEdge(event, in: window) ? nil : event
                }
            }

            deinit {
                if let monitor { NSEvent.removeMonitor(monitor) }
            }
        }
    }
}

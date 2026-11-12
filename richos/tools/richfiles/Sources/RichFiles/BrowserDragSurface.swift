import AppKit
import SwiftUI

@MainActor
struct BrowserDragSurface: NSViewRepresentable {
    struct Payload {
        let token: UUID
        let urls: [URL]
    }

    let prepare: (CGPoint) -> Payload?
    let controlClick: (CGPoint?, NSPoint, NSWindow) -> Bool

    static var pasteboardToken: UUID? {
        NSPasteboard(name: .drag).string(forType: Source.tokenType).flatMap(BrowserDragSelection.localToken(from:))
    }

    func makeNSView(context: Context) -> Source { Source() }

    func updateNSView(_ view: Source, context: Context) {
        view.prepare = prepare
        view.controlClick = controlClick
    }

    final class Source: NSView, NSDraggingSource {
        static let tokenType = NSPasteboard.PasteboardType(BrowserDragSelection.type)
        var prepare: ((CGPoint) -> Payload?)?
        var controlClick: ((CGPoint?, NSPoint, NSWindow) -> Bool)?
        private var monitor: Any?
        private var down: NSEvent?
        private var consumedClick = false

        override var isFlipped: Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            down = nil
            consumedClick = false
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp]) { [weak self] event in
                guard let self else { return event }
                return self.track(event) ? nil : event
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }

        private func track(_ event: NSEvent) -> Bool {
            guard !isHiddenOrHasHiddenAncestor, let window, event.window === window, window.attachedSheet == nil else {
                down = nil
                consumedClick = false
                return false
            }
            if event.type == .leftMouseDown {
                let point = convert(event.locationInWindow, from: nil)
                let inside = bounds.contains(point)
                let modifiers = event.modifierFlags.intersection([.control, .command, .option, .shift])
                consumedClick = modifiers == .control &&
                    controlClick?(inside ? point : nil, event.locationInWindow, window) == true
                down = inside && !consumedClick ? event : nil
                return consumedClick
            }
            if event.type == .leftMouseUp {
                down = nil
                let handled = consumedClick
                consumedClick = false
                return handled
            }
            guard let down,
                  hypot(event.locationInWindow.x - down.locationInWindow.x,
                        event.locationInWindow.y - down.locationInWindow.y) >= 5 else { return false }
            self.down = nil
            let point = convert(down.locationInWindow, from: nil)
            guard let payload = prepare?(point) else { return false }

            let icon = NSImage(systemSymbolName: "doc", accessibilityDescription: nil)
            let items = payload.urls.enumerated().map { index, url in
                let item = NSPasteboardItem()
                item.setString(url.absoluteString, forType: .fileURL)
                if index == 0 {
                    item.setString(BrowserDragSelection.pasteboardValue(for: payload.token), forType: Self.tokenType)
                }
                let dragging = NSDraggingItem(pasteboardWriter: item)
                let offset = CGFloat(min(index, 5)) * 2
                dragging.setDraggingFrame(CGRect(x: point.x - 14 + offset, y: point.y - 14 + offset,
                                                 width: 28, height: 28), contents: icon)
                return dragging
            }
            let session = beginDraggingSession(with: items, event: down, source: self)
            session.draggingFormation = .pile
            return true
        }

        func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
            context == .withinApplication ? [.copy, .move] : .copy
        }

        func ignoreModifierKeys(for session: NSDraggingSession) -> Bool { true }
    }
}

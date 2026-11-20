import AppKit
import SwiftUI

@MainActor
struct BrowserDragSurface: NSViewRepresentable {
    struct Payload {
        let token: UUID
        let urls: [URL]
    }

    let prepare: (CGPoint) -> (() -> Payload?)?
    let press: (CGPoint, NSEvent) -> Bool
    let click: (CGPoint, CGPoint, NSEvent) -> Bool
    let controlClick: (CGPoint?, NSPoint, NSWindow) -> Bool

    static var pasteboardToken: UUID? {
        NSPasteboard(name: .drag).string(forType: Source.tokenType).flatMap(BrowserDragSelection.localToken(from:))
    }

    func makeNSView(context: Context) -> Source { Source() }

    func updateNSView(_ view: Source, context: Context) {
        view.prepare = prepare
        view.press = press
        view.click = click
        view.controlClick = controlClick
    }

    final class Source: NSView, NSDraggingSource {
        static let tokenType = NSPasteboard.PasteboardType(BrowserDragSelection.type)
        var prepare: ((CGPoint) -> (() -> Payload?)?)?
        var press: ((CGPoint, NSEvent) -> Bool)?
        var click: ((CGPoint, CGPoint, NSEvent) -> Bool)?
        var controlClick: ((CGPoint?, NSPoint, NSWindow) -> Bool)?
        private var monitor: Any?
        private var down: (event: NSEvent, prepare: () -> Payload?, selected: Bool)?
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
                down = nil
                if inside && !consumedClick, let start = prepare?(point) {
                    let selected = window.isKeyWindow && event.clickCount == 1 && press?(point, event) == true
                    down = (event, start, selected)
                    if selected { return true }
                }
                return consumedClick
            }
            if event.type == .leftMouseUp {
                let pressed = down
                down = nil
                let handled = consumedClick
                consumedClick = false
                if handled { return true }
                guard let pressed else { return false }
                if pressed.selected { return true }
                let from = convert(pressed.event.locationInWindow, from: nil)
                let to = convert(event.locationInWindow, from: nil)
                guard bounds.contains(to), hypot(to.x - from.x, to.y - from.y) < 5 else { return false }
                return click?(from, to, pressed.event) == true
            }
            guard let down,
                  hypot(event.locationInWindow.x - down.event.locationInWindow.x,
                        event.locationInWindow.y - down.event.locationInWindow.y) >= 5 else { return false }
            self.down = nil
            let point = convert(down.event.locationInWindow, from: nil)
            guard let payload = down.prepare() else { return false }

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
            let session = beginDraggingSession(with: items, event: down.event, source: self)
            session.draggingFormation = .pile
            return true
        }

        func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
            context == .withinApplication ? [.copy, .move] : .copy
        }

        func ignoreModifierKeys(for session: NSDraggingSession) -> Bool { true }
    }
}

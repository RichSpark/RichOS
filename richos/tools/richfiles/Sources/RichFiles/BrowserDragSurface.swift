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
    /// Start, end, the press and whether the window was already key when it was pressed.
    let click: (CGPoint, CGPoint, NSEvent, Bool) -> Bool
    let controlClick: (CGPoint?, NSPoint, NSWindow) -> Bool
    /// A click on empty space in the pane, never on a scroll bar or a text box.
    var blankClick: (CGPoint) -> Void = { _ in }
    /// A right-click (or a Control-click the Windows keys did not take), before its menu opens.
    var secondaryPress: (CGPoint) -> Void = { _ in }
    /// Every press anywhere in the window, with whether it landed in a text box.
    var anyPress: (_ inText: Bool) -> Void = { _ in }

    static var pasteboardToken: UUID? {
        NSPasteboard(name: .drag).string(forType: Source.tokenType).flatMap(BrowserDragSelection.localToken(from:))
    }

    func makeNSView(context: Context) -> Source { Source() }

    func updateNSView(_ view: Source, context: Context) {
        view.prepare = prepare
        view.press = press
        view.click = click
        view.controlClick = controlClick
        view.blankClick = blankClick
        view.secondaryPress = secondaryPress
        view.anyPress = anyPress
    }

    final class Source: NSView, NSDraggingSource {
        static let tokenType = NSPasteboard.PasteboardType(BrowserDragSelection.type)
        var prepare: ((CGPoint) -> (() -> Payload?)?)?
        var press: ((CGPoint, NSEvent) -> Bool)?
        var click: ((CGPoint, CGPoint, NSEvent, Bool) -> Bool)?
        var controlClick: ((CGPoint?, NSPoint, NSWindow) -> Bool)?
        var blankClick: (CGPoint) -> Void = { _ in }
        var secondaryPress: (CGPoint) -> Void = { _ in }
        var anyPress: (Bool) -> Void = { _ in }
        private var monitor: Any?
        private var down: (event: NSEvent, prepare: () -> Payload?, selected: Bool, wasKey: Bool)?
        private var blankDown: NSEvent?
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
            monitor = NSEvent.addLocalMonitorForEvents(
                matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp, .rightMouseDown]
            ) { [weak self] event in
                guard let self else { return event }
                return self.track(event) ? nil : event
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }

        /// The deepest view under a window point is a scroll bar or a text box, which take
        /// their own clicks: neither is empty space.
        private func landsOnControl(_ event: NSEvent, in window: NSWindow) -> (control: Bool, text: Bool) {
            var view = window.contentView?.hitTest(event.locationInWindow)
            while let current = view {
                if current is NSText || current is NSTextField { return (true, true) }
                if current is NSScroller { return (true, false) }
                view = current.superview
            }
            return (false, false)
        }

        private func track(_ event: NSEvent) -> Bool {
            guard !isHiddenOrHasHiddenAncestor, let window, event.window === window, window.attachedSheet == nil else {
                down = nil
                blankDown = nil
                consumedClick = false
                return false
            }
            if event.type == .rightMouseDown {
                anyPress(landsOnControl(event, in: window).text)
                let point = convert(event.locationInWindow, from: nil)
                if bounds.contains(point) { secondaryPress(point) }
                return false
            }
            if event.type == .leftMouseDown {
                let point = convert(event.locationInWindow, from: nil)
                let inside = bounds.contains(point)
                let target = landsOnControl(event, in: window)
                anyPress(target.text)
                let modifiers = event.modifierFlags.intersection([.control, .command, .option, .shift])
                consumedClick = modifiers == .control &&
                    controlClick?(inside ? point : nil, event.locationInWindow, window) == true
                if modifiers == .control && !consumedClick && inside { secondaryPress(point) }
                down = nil
                blankDown = nil
                if inside && !consumedClick, let start = prepare?(point) {
                    let wasKey = window.isKeyWindow
                    let selected = wasKey && event.clickCount == 1 && press?(point, event) == true
                    down = (event, start, selected, wasKey)
                    if selected { return true }
                } else if inside && !consumedClick && modifiers.isEmpty && !target.control {
                    blankDown = event
                }
                return consumedClick
            }
            if event.type == .leftMouseUp {
                let pressed = down
                down = nil
                let blank = blankDown
                blankDown = nil
                let handled = consumedClick
                consumedClick = false
                if handled { return true }
                if let blank {
                    let from = convert(blank.locationInWindow, from: nil)
                    let to = convert(event.locationInWindow, from: nil)
                    if bounds.contains(to), hypot(to.x - from.x, to.y - from.y) < 5 { blankClick(from) }
                    return false
                }
                guard let pressed else { return false }
                if pressed.selected { return true }
                let from = convert(pressed.event.locationInWindow, from: nil)
                let to = convert(event.locationInWindow, from: nil)
                guard bounds.contains(to), hypot(to.x - from.x, to.y - from.y) < 5 else { return false }
                return click?(from, to, pressed.event, pressed.wasKey) == true
            }
            if let blank = blankDown,
               hypot(event.locationInWindow.x - blank.locationInWindow.x,
                     event.locationInWindow.y - blank.locationInWindow.y) >= 5 {
                blankDown = nil
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

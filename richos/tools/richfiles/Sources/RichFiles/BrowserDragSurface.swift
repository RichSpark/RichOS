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
        /// The inline name box and the character a press in it started at, while the press lasts.
        private var textAnchor: (editor: NSTextView, index: Int)?

        private func textIndex(_ event: NSEvent, in editor: NSTextView) -> Int {
            let index = editor.characterIndexForInsertion(at: editor.convert(event.locationInWindow, from: nil))
            return max(0, min(index, (editor.string as NSString).length))
        }
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
                self?.track(event) == true ? nil : event
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }

        /// The deepest view under a window point is a scroll bar or a text box, which take
        /// their own clicks: neither is empty space.
        /// The field editor of the text box being edited, when the press is inside that box.
        private func editingField(under event: NSEvent, in window: NSWindow) -> NSTextView? {
            guard let editor = window.firstResponder as? NSTextView, editor.isFieldEditor,
                  let field = editor.delegate as? NSView,
                  field.bounds.contains(field.convert(event.locationInWindow, from: nil)) else { return nil }
            return editor
        }

        private func landsOnControl(_ event: NSEvent, in window: NSWindow) -> (control: Bool, text: Bool) {
            if editingField(under: event, in: window) != nil { return (true, true) }
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
                textAnchor = nil
                consumedClick = false
                return false
            }
            if event.type == .rightMouseDown {
                let target = landsOnControl(event, in: window)
                anyPress(target.text)
                let point = convert(event.locationInWindow, from: nil)
                guard bounds.contains(point) else { return false }
                secondaryPress(point)
                return !target.control && BrowserContextMenu.fitAtRightEdge(event, in: window)
            }
            // A press in the inline name box places the caret there, a drag selects the part of the
            // name it crosses and a double-click selects a word, as in any text box. SwiftUI's
            // hosting view takes presses over the row, so the box never sees them; the selection is
            // set on its field editor here instead (catalog entry 2).
            if event.type == .leftMouseDown, let editor = editingField(under: event, in: window) {
                anyPress(true)
                down = nil
                blankDown = nil
                consumedClick = false
                let index = textIndex(event, in: editor)
                if event.clickCount >= 2 {
                    let word = editor.selectionRange(forProposedRange: NSRange(location: index, length: 0),
                                                     granularity: event.clickCount == 2 ? .selectByWord : .selectByParagraph)
                    editor.setSelectedRange(word)
                    textAnchor = nil
                } else {
                    editor.setSelectedRange(NSRange(location: index, length: 0))
                    textAnchor = (editor, index)
                }
                return true
            }
            if let anchor = textAnchor {
                if event.type == .leftMouseDragged {
                    let index = textIndex(event, in: anchor.editor)
                    anchor.editor.setSelectedRange(NSRange(location: min(index, anchor.index),
                                                           length: abs(index - anchor.index)))
                    return true
                }
                if event.type == .leftMouseUp {
                    textAnchor = nil
                    return true
                }
            }
            if event.type == .leftMouseDown {
                let point = convert(event.locationInWindow, from: nil)
                let inside = bounds.contains(point)
                let target = landsOnControl(event, in: window)
                anyPress(target.text)
                let modifiers = event.modifierFlags.intersection([.control, .command, .option, .shift])
                consumedClick = modifiers == .control &&
                    controlClick?(inside ? point : nil, event.locationInWindow, window) == true
                down = nil
                blankDown = nil
                if modifiers == .control && !consumedClick && inside {
                    secondaryPress(point)
                    if !target.control && BrowserContextMenu.fitAtRightEdge(event, in: window) { return true }
                }
                // A press in the inline name box is the box's own: a drag there selects text.
                if inside && !consumedClick && !target.text, let start = prepare?(point) {
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

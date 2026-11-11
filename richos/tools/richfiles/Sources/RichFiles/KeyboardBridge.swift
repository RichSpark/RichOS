import AppKit
import SwiftUI

struct KeyboardBridge: NSViewRepresentable {
    var windowsKeys: Bool
    var acceptsInput: () -> Bool
    var perform: (BrowserCommand) -> Bool
    var prefix: (String) -> Bool

    func makeNSView(context: Context) -> KeyView { KeyView() }

    func updateNSView(_ view: KeyView, context: Context) {
        view.windowsKeys = windowsKeys
        view.acceptsInput = acceptsInput
        view.perform = perform
        view.prefix = prefix
    }

    final class KeyView: NSView {
        var windowsKeys = true
        var acceptsInput: () -> Bool = { false }
        var perform: ((BrowserCommand) -> Bool)?
        var prefix: ((String) -> Bool)?
        private var monitor: Any?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .otherMouseDown, .swipe]) { [weak self] event in
                guard let self, let window = self.window, event.window === window,
                      window.attachedSheet == nil, self.acceptsInput() else { return event }
                if event.type != .keyDown { return self.navigate(event) ? nil : event }
                let editing = window.firstResponder is NSTextView || window.firstResponder is NSTextField
                return self.handle(event, editing: editing) ? nil : event
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }

        private func navigate(_ event: NSEvent) -> Bool {
            switch event.type {
            case .otherMouseDown:
                if event.buttonNumber == 3 { return perform?(.back) == true }
                if event.buttonNumber == 4 { return perform?(.forward) == true }
            case .swipe:
                if abs(event.deltaX) > abs(event.deltaY) {
                    return perform?(event.deltaX < 0 ? .back : .forward) == true
                }
            default: break
            }
            return false
        }

        private func handle(_ event: NSEvent, editing: Bool) -> Bool {
            let modifiers = event.modifierFlags.intersection([.command, .control, .option, .shift])
            let shortcut: NSEvent.ModifierFlags = windowsKeys ? .control : .command
            let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
            if editing {
                if modifiers == shortcut && key == "l" { return perform?(.address) == true }
                if modifiers == shortcut && key == "f" { return perform?(.search) == true }
                return false
            }
            if modifiers.isEmpty || modifiers == .shift {
                let extend = modifiers == .shift
                switch event.keyCode {
                case 123: return perform?(.grid(-1, 0, extend)) == true
                case 124: return perform?(.grid(1, 0, extend)) == true
                case 125: return perform?(.grid(0, 1, extend)) == true
                case 126: return perform?(.grid(0, -1, extend)) == true
                default: break
                }
            }
            if modifiers.isEmpty {
                switch event.keyCode {
                case 36, 76: return perform?(.open) == true
                case 120: return perform?(.rename) == true
                case 51, 117: return perform?(.trash) == true
                case 49: return perform?(.preview) == true
                case 96: return perform?(.refresh) == true
                default:
                    guard !key.isEmpty, key.allSatisfy({ $0.isLetter || $0.isNumber }) else { return false }
                    return prefix?(key) == true
                }
            }
            if modifiers == .option {
                switch event.keyCode {
                case 36, 76: return perform?(.properties) == true
                case 123: return perform?(.back) == true
                case 124: return perform?(.forward) == true
                case 126: return perform?(.up) == true
                default:
                    if key == "d" { return perform?(.address) == true }
                }
            }
            if modifiers == [shortcut, .shift] {
                if key == "n" { return perform?(.newFolder) == true }
                if key == "z" { return perform?(.redo) == true }
                if key == "p" { return perform?(.actionPalette) == true }
            }
            if modifiers == shortcut {
                switch key {
                case "a": return perform?(.selectAll) == true
                case "c": return perform?(.copy) == true
                case "x": return perform?(.cut) == true
                case "v": return perform?(.paste) == true
                case "z": return perform?(.undo) == true
                case "y": return perform?(.redo) == true
                case "l": return perform?(.address) == true
                case "f": return perform?(.search) == true
                case "t": return perform?(.newTab) == true
                case "w" where windowsKeys: return perform?(.closeTab) == true
                default: break
                }
            }
            if modifiers == .command && (event.keyCode == 51 || event.keyCode == 117) {
                return perform?(.trash) == true
            }
            return false
        }
    }
}

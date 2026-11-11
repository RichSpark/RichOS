import AppKit
import SwiftUI

struct BrowserShortcut: Hashable, Codable {
    private let key: String
    private let mask: UInt

    private static let modifiers: [(String, NSEvent.ModifierFlags)] = [
        ("Control", .control), ("Option", .option), ("Shift", .shift), ("Command", .command)
    ]
    private static let keyCodes: [UInt16: String] = [
        36: "enter", 76: "enter", 49: "space", 51: "delete", 117: "delete",
        122: "f1", 120: "f2", 99: "f3", 118: "f4", 96: "f5", 97: "f6",
        98: "f7", 100: "f8", 101: "f9", 109: "f10", 103: "f11", 111: "f12",
        123: "left", 124: "right", 125: "down", 126: "up"
    ]

    private init(_ key: String, _ flags: NSEvent.ModifierFlags = []) {
        self.key = key
        mask = flags.rawValue
    }

    init(event: NSEvent, modifiers: NSEvent.ModifierFlags) {
        self.init(Self.keyCodes[event.keyCode] ?? event.charactersIgnoringModifiers?.lowercased() ?? "", modifiers)
    }

    init?(_ input: String) {
        var parts = input.split(separator: "+", omittingEmptySubsequences: false).map {
            $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        }
        guard let key = parts.popLast() else { return nil }
        var flags: NSEvent.ModifierFlags = []
        for part in parts {
            let flag: NSEvent.ModifierFlags
            switch part {
            case "control", "ctrl": flag = .control
            case "option", "alt": flag = .option
            case "shift": flag = .shift
            case "command", "cmd": flag = .command
            default: return nil
            }
            guard !flags.contains(flag) else { return nil }
            flags.insert(flag)
        }
        let function = key.hasPrefix("f") && Self.keyCodes.values.contains(key)
        let modified = !flags.intersection([.command, .control, .option]).isEmpty
        let ordinary = key.count == 1 && key.allSatisfy { $0.isLetter || $0.isNumber }
        guard function || (modified && (ordinary || Self.keyCodes.values.contains(key))) else { return nil }
        self.init(key, flags)
    }

    var label: String {
        let flags = NSEvent.ModifierFlags(rawValue: mask)
        let names = Self.modifiers.compactMap { flags.contains($0.1) ? $0.0 : nil }
        let shownKey = key.count == 1 || key.hasPrefix("f") ? key.uppercased() : key.capitalized
        return (names + [shownKey]).joined(separator: "+")
    }

    static func preset(_ windows: Bool) -> [Self: BrowserCommand] { windows ? windowsPreset : macPreset }

    private static let windowsPreset = makePreset(.control, windows: true)
    private static let macPreset = makePreset(.command, windows: false)

    private static func makePreset(_ primary: NSEvent.ModifierFlags, windows: Bool) -> [Self: BrowserCommand] {
        var shortcuts: [Self: BrowserCommand] = [
            Self("enter"): .open, Self("f2"): .rename, Self("delete"): .trash,
            Self("space"): .preview, Self("f5"): .refresh, Self("delete", .command): .trash,
            Self("enter", .option): .properties, Self("left", .option): .back,
            Self("right", .option): .forward, Self("up", .option): .up, Self("d", .option): .address,
            Self("n", [primary, .shift]): .newFolder, Self("z", [primary, .shift]): .redo,
            Self("p", [primary, .shift]): .actionPalette,
            Self("a", primary): .selectAll, Self("c", primary): .copy, Self("x", primary): .cut,
            Self("v", primary): .paste, Self("z", primary): .undo, Self("y", primary): .redo,
            Self("l", primary): .address, Self("f", primary): .search, Self("t", primary): .newTab
        ]
        if windows { shortcuts[Self("w", primary)] = .closeTab }
        return shortcuts
    }

    static func assign(
        _ draft: String, to command: BrowserCommand, in overrides: [BrowserCommand: Self]
    ) throws -> [BrowserCommand: Self] {
        guard let shortcut = Self(draft) else { throw ShortcutError.invalid }
        if ["q", "h", "m", "w"].contains(where: { shortcut == Self($0, .command) }) {
            throw ShortcutError.systemShortcut
        }
        let conflict = overrides.first { $0.key != command && $0.value == shortcut }?.key
            ?? [windowsPreset[shortcut], macPreset[shortcut]].compactMap { $0 }.first {
                $0 != command
            }
        if let conflict {
            let title = BrowserAction.all.first { $0.command == conflict }?.title ?? "File Actions"
            throw ShortcutError.inUse(title)
        }
        var updated = overrides
        updated[command] = shortcut
        return updated
    }

    static func load(_ data: Data) -> [BrowserCommand: Self] {
        (try? JSONDecoder().decode([BrowserCommand: Self].self, from: data)) ?? [:]
    }

    private enum ShortcutError: LocalizedError {
        case invalid, systemShortcut, inUse(String)

        var errorDescription: String? {
            switch self {
            case .invalid: return "Enter a key combination such as Control+Shift+J."
            case .systemShortcut: return "That shortcut is reserved for the Mac app window."
            case .inUse(let title): return "That shortcut already runs \(title)."
            }
        }
    }
}

struct KeyboardBridge: NSViewRepresentable {
    var windowsKeys: Bool
    var acceptsInput: () -> Bool
    var shortcuts: [BrowserCommand: BrowserShortcut]
    var perform: (BrowserCommand) -> Bool
    var prefix: (String) -> Bool

    func makeNSView(context: Context) -> KeyView { KeyView() }

    func updateNSView(_ view: KeyView, context: Context) {
        view.windowsKeys = windowsKeys
        view.acceptsInput = acceptsInput
        view.shortcuts = shortcuts
        view.perform = perform
        view.prefix = prefix
    }

    final class KeyView: NSView {
        var windowsKeys = true
        var acceptsInput: () -> Bool = { false }
        var shortcuts: [BrowserCommand: BrowserShortcut] = [:]
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
            let chord = BrowserShortcut(event: event, modifiers: modifiers)
            let preset = BrowserShortcut.preset(windowsKeys)[chord]
            if editing {
                let primary: NSEvent.ModifierFlags = windowsKeys ? .control : .command
                if modifiers == primary, let preset, [.address, .search].contains(preset), shortcuts[preset] == nil {
                    return perform?(preset) == true
                }
                return false
            }
            if let action = shortcuts.first(where: { $0.value == chord })?.key {
                return perform?(action) == true
            }
            if let preset, shortcuts[preset] == nil { return perform?(preset) == true }
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
            guard modifiers.isEmpty, let key = event.charactersIgnoringModifiers?.lowercased(),
                  !key.isEmpty, key.allSatisfy({ $0.isLetter || $0.isNumber }) else { return false }
            return prefix?(key) == true
        }
    }
}

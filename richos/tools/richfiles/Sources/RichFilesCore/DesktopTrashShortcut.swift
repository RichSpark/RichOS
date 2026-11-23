import AppKit
import Carbon.HIToolbox

public enum DesktopTrashShortcut {
    /// Return Finder's Move to Trash key only for its focused native Desktop icon view.
    public static func replacement(for event: CGEvent, desktopHasFocus: () -> Bool) -> CGEvent? {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        guard event.type == .keyDown, event.getIntegerValueField(.keyboardEventKeycode) == kVK_ForwardDelete,
              event.flags.intersection(modifiers).isEmpty,
              event.getIntegerValueField(.keyboardEventAutorepeat) == 0, desktopHasFocus() else { return nil }

        // An explicit Unicode payload turns the event into text input. Even
        // replacing U+F728 with U+007F leaves Finder ignoring the menu shortcut.
        // Build an ordinary physical-key event and let the keyboard layout supply its character.
        guard let replacement = CGEvent(keyboardEventSource: CGEventSource(event: event),
                                        virtualKey: CGKeyCode(kVK_Delete), keyDown: true) else { return nil }
        replacement.timestamp = event.timestamp
        replacement.flags = event.flags.subtracting([.maskSecondaryFn, .maskNumericPad]).union(.maskCommand)
        return replacement
    }
}

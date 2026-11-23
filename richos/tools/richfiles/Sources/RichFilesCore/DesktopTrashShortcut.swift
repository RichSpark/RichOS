import AppKit
import Carbon.HIToolbox

public enum DesktopTrashShortcut {
    /// Turn Forward Delete into Finder's Move to Trash shortcut when its native
    /// Desktop selection has keyboard focus. Text fields keep their editing keys.
    public static func rewrite(_ event: CGEvent, hasDesktopSelection: () -> Bool) -> Bool {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        guard event.type == .keyDown, event.getIntegerValueField(.keyboardEventKeycode) == kVK_ForwardDelete,
              event.flags.intersection(modifiers).isEmpty,
              event.getIntegerValueField(.keyboardEventAutorepeat) == 0, hasDesktopSelection() else { return false }

        // Some keyboards include the Forward Delete character (U+F728) in the
        // event. Changing its key code alone leaves Finder seeing that character.
        var character = UniChar(NSDeleteCharacter)
        event.setIntegerValueField(.keyboardEventKeycode, value: Int64(kVK_Delete))
        event.keyboardSetUnicodeString(stringLength: 1, unicodeString: &character)
        event.flags.subtract([.maskSecondaryFn, .maskNumericPad])
        event.flags.insert(.maskCommand)
        return true
    }
}

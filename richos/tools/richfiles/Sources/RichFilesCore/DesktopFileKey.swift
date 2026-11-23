import AppKit
import Carbon.HIToolbox

public enum DesktopFileKey {
    case passThrough
    case ignoreBackspace
    case moveToTrash(CGEvent)

    /// Finder owns the native Desktop icon view. Once it receives its Move to
    /// Trash shortcut, it applies the command to its selection in event order.
    public static func action(for event: CGEvent, desktopHasFocus: () -> Bool) -> Self {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        guard event.type == .keyDown, event.flags.intersection(modifiers).isEmpty else { return .passThrough }
        switch Int(event.getIntegerValueField(.keyboardEventKeycode)) {
        case kVK_Delete:
            return !event.flags.contains(.maskSecondaryFn) && desktopHasFocus() ? .ignoreBackspace : .passThrough
        case kVK_ForwardDelete:
            guard event.getIntegerValueField(.keyboardEventAutorepeat) == 0, desktopHasFocus() else { return .passThrough }
            // An explicit Unicode payload (including U+F728 or U+007F) can
            // reach Finder as text input. Send a layout-translated physical key.
            guard let replacement = CGEvent(keyboardEventSource: CGEventSource(event: event),
                                            virtualKey: CGKeyCode(kVK_Delete), keyDown: true) else { return .passThrough }
            replacement.timestamp = event.timestamp
            replacement.flags = event.flags.subtracting([.maskSecondaryFn, .maskNumericPad]).union(.maskCommand)
            return .moveToTrash(replacement)
        default:
            return .passThrough
        }
    }
}

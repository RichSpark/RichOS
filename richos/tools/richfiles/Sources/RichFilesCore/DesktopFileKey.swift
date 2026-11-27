import AppKit
import Carbon.HIToolbox

public enum DesktopFileKey {
    case passThrough
    case backspace
    case forwardDelete

    public static func classify(_ event: CGEvent) -> Self {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        guard event.type == .keyDown, event.flags.intersection(modifiers).isEmpty else { return .passThrough }
        switch Int(event.getIntegerValueField(.keyboardEventKeycode)) {
        case kVK_Delete:
            return event.flags.contains(.maskSecondaryFn) ? .passThrough : .backspace
        case kVK_ForwardDelete:
            return event.getIntegerValueField(.keyboardEventAutorepeat) == 0 ? .forwardDelete : .passThrough
        default:
            return .passThrough
        }
    }
}
